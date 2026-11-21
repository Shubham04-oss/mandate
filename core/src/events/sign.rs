//! Ed25519 over canonical JSON. One key per source; consumers verify before any other use.
use super::{Envelope, Source};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD as B64};
use ed25519_dalek::pkcs8::{DecodePrivateKey, DecodePublicKey};
use ed25519_dalek::{Signature, Signer as _, SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use time::{Duration, OffsetDateTime};

/// Older events are refused, so a captured event cannot be replayed after the inbox forgets it.
pub const MAX_AGE: Duration = Duration::hours(24);
/// Clock skew allowed for an event stamped in the future.
const MAX_SKEW: Duration = Duration::minutes(5);
/// SQS caps a message at 256 KiB; an envelope of ids is far smaller.
pub const MAX_LEN: usize = 64 * 1024;

/// The envelope as it travels: every field is covered by `signature`, including `kid`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SignedEnvelope {
    #[serde(flatten)]
    pub envelope: Envelope,
    pub kid: String,
    pub signature: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum VerifyError {
    #[error("not an envelope")]
    Undecodable,
    #[error("unsigned")]
    Unsigned,
    #[error("unknown key")]
    UnknownKey,
    #[error("bad signature")]
    BadSignature,
    #[error("key does not belong to the event's source")]
    WrongSource,
    #[error("too old or from the future")]
    Stale,
}

#[derive(Debug, thiserror::Error)]
pub enum KeyError {
    #[error("{0}: {1}")]
    Read(String, std::io::Error),
    #[error("{0}: not an Ed25519 PKCS#8 private key")]
    Private(String),
    #[error("{0}: not an Ed25519 SPKI public key")]
    Public(String),
    #[error("MANDATE_EVENT_PUBLIC_KEYS: bad entry {0:?} (want <source>=<pem path>)")]
    Entry(String),
}

/// Keys sorted at every level, no whitespace. serde_json's own map order depends on a crate
/// feature some dependency could enable, so the order is made explicit here.
pub fn canonical(v: &Value) -> String {
    let mut out = String::new();
    write_canonical(v, &mut out);
    out
}

fn write_canonical(v: &Value, out: &mut String) {
    match v {
        Value::Object(m) => {
            let mut keys: Vec<&String> = m.keys().collect();
            keys.sort();
            out.push('{');
            for (i, k) in keys.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str(&Value::String((*k).clone()).to_string());
                out.push(':');
                write_canonical(&m[k.as_str()], out);
            }
            out.push('}');
        }
        Value::Array(a) => {
            out.push('[');
            for (i, x) in a.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_canonical(x, out);
            }
            out.push(']');
        }
        scalar => out.push_str(&scalar.to_string()),
    }
}

/// Derived from the public key, so a key id can never point at a different key.
pub fn kid(key: &VerifyingKey) -> String {
    B64.encode(&Sha256::digest(key.as_bytes())[..12])
}

/// A source's signing key; only the publisher holds one.
pub struct Signer {
    source: Source,
    key: SigningKey,
    kid: String,
}

impl Signer {
    pub fn new(source: Source, key: SigningKey) -> Signer {
        let kid = kid(&key.verifying_key());
        Signer { source, key, kid }
    }

    /// `MANDATE_EVENT_SIGNING_KEY`: PKCS#8 PEM path of the `mandate.core` key.
    pub fn from_pem_file(path: &str) -> Result<Signer, KeyError> {
        let pem = std::fs::read_to_string(path).map_err(|e| KeyError::Read(path.into(), e))?;
        let key = SigningKey::from_pkcs8_pem(&pem).map_err(|_| KeyError::Private(path.into()))?;
        Ok(Signer::new(Source::core(), key))
    }

    pub fn source(&self) -> &Source {
        &self.source
    }

    pub fn verifying_key(&self) -> VerifyingKey {
        self.key.verifying_key()
    }

    /// `None` if the envelope is not from this signer's source: a key signs only its own.
    pub fn sign(&self, envelope: Envelope) -> Option<SignedEnvelope> {
        if envelope.source != self.source {
            return None;
        }
        let mut v = serde_json::to_value(&envelope).ok()?;
        v.as_object_mut()?
            .insert("kid".into(), Value::String(self.kid.clone()));
        let sig = self.key.sign(canonical(&v).as_bytes());
        Some(SignedEnvelope {
            envelope,
            kid: self.kid.clone(),
            signature: B64.encode(sig.to_bytes()),
        })
    }
}

/// The public keys a consumer trusts, each bound to the one source it may sign for.
#[derive(Default, Clone)]
pub struct Keyring {
    keys: HashMap<String, (Source, VerifyingKey)>,
}

impl Keyring {
    pub fn add(&mut self, source: Source, key: VerifyingKey) {
        self.keys.insert(kid(&key), (source, key));
    }

    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }

    /// `MANDATE_EVENT_PUBLIC_KEYS`: comma-separated `<source>=<SPKI PEM path>`.
    pub fn from_spec(spec: &str) -> Result<Keyring, KeyError> {
        let mut ring = Keyring::default();
        for entry in spec.split(',').map(str::trim).filter(|e| !e.is_empty()) {
            let (source, path) = entry
                .split_once('=')
                .ok_or_else(|| KeyError::Entry(entry.into()))?;
            let source = Source::parse(source.trim()).map_err(|_| KeyError::Entry(entry.into()))?;
            let path = path.trim();
            let pem = std::fs::read_to_string(path).map_err(|e| KeyError::Read(path.into(), e))?;
            let key = VerifyingKey::from_public_key_pem(&pem)
                .map_err(|_| KeyError::Public(path.into()))?;
            ring.add(source, key);
        }
        Ok(ring)
    }

    /// Signature first, then the typed decode, the key's source and the age; any failure is
    /// a refusal. `raw` is the envelope's JSON as received.
    pub fn verify(&self, raw: &[u8], now: OffsetDateTime) -> Result<Envelope, VerifyError> {
        use VerifyError::*;
        if raw.len() > MAX_LEN {
            return Err(Undecodable);
        }
        let mut v: Value = serde_json::from_slice(raw).map_err(|_| Undecodable)?;
        let obj = v.as_object_mut().ok_or(Undecodable)?;
        let Some(Value::String(sig)) = obj.remove("signature") else {
            return Err(Unsigned);
        };
        let Some(Value::String(kid)) = obj.get("kid").cloned() else {
            return Err(Unsigned);
        };
        let (source, key) = self.keys.get(&kid).ok_or(UnknownKey)?;
        let sig = B64.decode(sig).map_err(|_| BadSignature)?;
        let sig = Signature::from_slice(&sig).map_err(|_| BadSignature)?;
        key.verify_strict(canonical(&v).as_bytes(), &sig)
            .map_err(|_| BadSignature)?;
        let env: Envelope = serde_json::from_value(v).map_err(|_| Undecodable)?;
        if &env.source != source {
            return Err(WrongSource);
        }
        if env.occurred_at < now - MAX_AGE || env.occurred_at > now + MAX_SKEW {
            return Err(Stale);
        }
        Ok(env)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{Event, EventType, samples};
    use crate::ids::TenantId;
    use uuid::Uuid;

    fn key(n: u8) -> SigningKey {
        SigningKey::from_bytes(&[n; 32])
    }

    fn now() -> OffsetDateTime {
        OffsetDateTime::from_unix_timestamp(1_800_000_000).unwrap()
    }

    fn envelope(source: Source, e: &Event, at: OffsetDateTime) -> Envelope {
        Envelope {
            id: Uuid::new_v4(),
            source,
            r#type: EventType::parse(e.event_type()).unwrap(),
            tenant: TenantId::parse("t1").unwrap(),
            occurred_at: at,
            data: e.data(),
        }
    }

    fn ring(source: Source, k: &SigningKey) -> Keyring {
        let mut r = Keyring::default();
        r.add(source, k.verifying_key());
        r
    }

    fn wire(s: &SignedEnvelope) -> Vec<u8> {
        serde_json::to_vec(s).unwrap()
    }

    #[test]
    fn canonical_sorts_keys_and_drops_whitespace() {
        let v: Value =
            serde_json::from_str(r#"{ "b": [1, {"z": 1, "a": "x"}], "a": null }"#).unwrap();
        assert_eq!(canonical(&v), r#"{"a":null,"b":[1,{"a":"x","z":1}]}"#);
    }

    #[test]
    fn envelope_signature_round_trips() {
        let signer = Signer::new(Source::core(), key(1));
        let ring = ring(Source::core(), &key(1));
        for e in samples() {
            let env = envelope(Source::core(), &e, now());
            let signed = signer.sign(env.clone()).unwrap();
            assert_eq!(ring.verify(&wire(&signed), now()), Ok(env.clone()));
            // Key order on the wire does not matter: the signature is over the canonical form.
            let v: Value = serde_json::from_slice(&wire(&signed)).unwrap();
            let pretty = serde_json::to_vec_pretty(&v).unwrap();
            assert_eq!(ring.verify(&pretty, now()), Ok(env));
        }
    }

    #[test]
    fn tampered_envelope_fails() {
        let signer = Signer::new(Source::core(), key(1));
        let ring = ring(Source::core(), &key(1));
        let signed = signer
            .sign(envelope(Source::core(), &samples()[1], now()))
            .unwrap();
        let good: Value = serde_json::to_value(&signed).unwrap();
        let edits: [(&str, Value); 5] = [
            ("tenant", "t2".into()),
            ("type", "approval.requested.v1".into()),
            ("id", Uuid::new_v4().to_string().into()),
            (
                "data",
                serde_json::json!({"approval": Uuid::nil(), "decision": "denied", "by": "m1"}),
            ),
            ("extra", "x".into()),
        ];
        for (field, value) in edits {
            let mut bad = good.clone();
            bad[field] = value;
            assert_eq!(
                ring.verify(&serde_json::to_vec(&bad).unwrap(), now()),
                Err(VerifyError::BadSignature),
                "{field}"
            );
        }
        let mut bad = good.clone();
        bad["signature"] = B64.encode([0u8; 64]).into();
        assert_eq!(
            ring.verify(&serde_json::to_vec(&bad).unwrap(), now()),
            Err(VerifyError::BadSignature)
        );
    }

    #[test]
    fn wrong_source_key_fails() {
        let gmail = Source::parse("mandate.app.gmail").unwrap();
        // The gmail key is trusted, but only for gmail: it cannot speak for the core.
        let mut r = ring(Source::core(), &key(1));
        r.add(gmail.clone(), key(2).verifying_key());
        let forged = Signer::new(gmail.clone(), key(2));
        let mut env = envelope(gmail, &samples()[1], now());
        let signed = forged.sign(env.clone()).unwrap();
        let mut v = serde_json::to_value(&signed).unwrap();
        // Re-sign the envelope claiming `mandate.core` with the gmail key.
        env.source = Source::core();
        v["source"] = "mandate.core".into();
        v.as_object_mut().unwrap().remove("signature");
        let sig = key(2).sign(canonical(&v).as_bytes());
        v["signature"] = B64.encode(sig.to_bytes()).into();
        assert_eq!(
            r.verify(&serde_json::to_vec(&v).unwrap(), now()),
            Err(VerifyError::WrongSource)
        );
        // An unknown key and a signer asked to sign for another source both refuse.
        let stranger = Signer::new(Source::core(), key(9));
        let s = stranger
            .sign(envelope(Source::core(), &samples()[0], now()))
            .unwrap();
        assert_eq!(r.verify(&wire(&s), now()), Err(VerifyError::UnknownKey));
        assert!(
            stranger
                .sign(envelope(
                    Source::parse("mandate.app.x").unwrap(),
                    &samples()[0],
                    now()
                ))
                .is_none()
        );
    }

    #[test]
    fn unsigned_or_older_than_24h_is_rejected() {
        let signer = Signer::new(Source::core(), key(1));
        let ring = ring(Source::core(), &key(1));
        let env = envelope(Source::core(), &samples()[0], now());
        let mut v = serde_json::to_value(signer.sign(env.clone()).unwrap()).unwrap();
        v.as_object_mut().unwrap().remove("signature");
        assert_eq!(
            ring.verify(&serde_json::to_vec(&v).unwrap(), now()),
            Err(VerifyError::Unsigned)
        );
        let unsigned = serde_json::to_vec(&env).unwrap();
        assert_eq!(ring.verify(&unsigned, now()), Err(VerifyError::Unsigned));

        for (at, ok) in [
            (now() - MAX_AGE, true),
            (now() - MAX_AGE - Duration::seconds(1), false),
            (now() + Duration::minutes(5), true),
            (now() + Duration::minutes(6), false),
        ] {
            let s = signer
                .sign(envelope(Source::core(), &samples()[0], at))
                .unwrap();
            let r = ring.verify(&wire(&s), now());
            assert_eq!(r.is_ok(), ok, "{at}: {r:?}");
            if !ok {
                assert_eq!(r, Err(VerifyError::Stale));
            }
        }
    }

    #[test]
    fn garbage_is_undecodable() {
        let ring = ring(Source::core(), &key(1));
        for raw in [&b"not json"[..], b"[]", b"\"x\""] {
            assert_eq!(ring.verify(raw, now()), Err(VerifyError::Undecodable));
        }
        assert_eq!(
            ring.verify(&vec![b' '; MAX_LEN + 1], now()),
            Err(VerifyError::Undecodable)
        );
        // A valid signature over ids that fail validation still does not decode.
        let mut v = serde_json::to_value(envelope(Source::core(), &samples()[0], now())).unwrap();
        v["tenant"] = "BAD TENANT".into();
        v["kid"] = kid(&key(1).verifying_key()).into();
        let sig = key(1).sign(canonical(&v).as_bytes());
        v["signature"] = B64.encode(sig.to_bytes()).into();
        assert_eq!(
            ring.verify(&serde_json::to_vec(&v).unwrap(), now()),
            Err(VerifyError::Undecodable)
        );
    }
}
