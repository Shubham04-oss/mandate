use crate::ids::{AgentId, MemberId, SpaceId, TenantId};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD as B64};
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sha2::{Digest, Sha256};
use uuid::Uuid;

pub const TTL_SECS: i64 = 300;
/// How long past expiry the registry and renewal still accept a token: a turn can wait an hour
/// on an approval, and a queued turn can start long after its forward token was minted.
pub const RENEW_GRACE: i64 = 2 * 60 * 60;
const MAX_TOKEN_LEN: usize = 4096;

/// Who a turn runs as on the wire; absent means `member` (every token minted before ORY-89).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrincipalKind {
    Member,
    Agent,
}

/// What a token is for; each purpose has exactly one acceptor (docs/channels.md, section 5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Purpose {
    Turn,
    Forward,
    Answer,
}

/// SHA-256 of a forwarded body, base64url on the wire; eve recomputes it over what it received.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BodyHash([u8; 32]);

impl BodyHash {
    pub fn of(body: &[u8]) -> BodyHash {
        BodyHash(Sha256::digest(body).into())
    }
}

impl Serialize for BodyHash {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&B64.encode(self.0))
    }
}

impl<'de> Deserialize<'de> for BodyHash {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        let bytes = B64.decode(s).map_err(serde::de::Error::custom)?;
        let hash = <[u8; 32]>::try_from(bytes.as_slice())
            .map_err(|_| serde::de::Error::custom("body hash is not 32 bytes"))?;
        Ok(BodyHash(hash))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Assertion {
    pub tenant: TenantId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub principal: Option<PrincipalKind>,
    pub member: MemberId,
    pub agent: AgentId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub space: Option<SpaceId>,
    /// Set on turn and answer tokens once the core knows the conversation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conversation: Option<Uuid>,
    /// The `channel_threads` row; forward tokens only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread: Option<Uuid>,
    /// Forward tokens only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<BodyHash>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub purpose: Option<Purpose>,
    pub expires_at: i64,
}

impl Assertion {
    pub fn purpose(&self) -> Purpose {
        self.purpose.unwrap_or(Purpose::Turn)
    }

    pub fn principal(&self) -> PrincipalKind {
        self.principal.unwrap_or(PrincipalKind::Member)
    }

    /// The field rules of section 5 that serde alone cannot express.
    fn well_formed(&self) -> bool {
        let forward = self.purpose() == Purpose::Forward;
        // ponytail: `member` is still required, so an agent principal is refused here (fail
        // closed) until the agent-principal work makes it optional and the gate refuses it.
        self.principal() == PrincipalKind::Member
            && (forward || (self.thread.is_none() && self.body.is_none()))
            && (!forward || self.thread.is_some())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum AssertionError {
    #[error("malformed assertion")]
    Malformed,
    #[error("bad assertion signature")]
    BadSignature,
    #[error("assertion expired")]
    Expired,
    #[error("assertion presented for the wrong purpose")]
    WrongPurpose,
}

pub fn sign(a: &Assertion, key: &SigningKey) -> String {
    // Serializing ids, uuids, unit enums and an integer cannot fail.
    let json = serde_json::to_vec(a).unwrap_or_default();
    format!(
        "{}.{}",
        B64.encode(&json),
        B64.encode(key.sign(&json).to_bytes())
    )
}

/// A live `turn` token, the only purpose the core's tool routes accept: a `forward` or `answer`
/// token is refused here, so no token works beyond its one acceptor.
pub fn verify(token: &str, key: &VerifyingKey, now: i64) -> Result<Assertion, AssertionError> {
    let a = verify_with_grace(token, key, now, 0)?;
    if a.purpose() != Purpose::Turn {
        return Err(AssertionError::WrongPurpose);
    }
    Ok(a)
}

/// A token of any purpose, accepted until `grace` seconds past its expiry. For the registry and
/// renewal (with `RENEW_GRACE`); the caller checks the purpose it accepts.
pub fn verify_with_grace(
    token: &str,
    key: &VerifyingKey,
    now: i64,
    grace: i64,
) -> Result<Assertion, AssertionError> {
    use AssertionError::*;
    if token.len() > MAX_TOKEN_LEN {
        return Err(Malformed);
    }
    let (payload, sig) = token.split_once('.').ok_or(Malformed)?;
    let json = B64.decode(payload).map_err(|_| Malformed)?;
    let sig = B64.decode(sig).map_err(|_| Malformed)?;
    let sig = Signature::from_slice(&sig).map_err(|_| Malformed)?;
    key.verify_strict(&json, &sig).map_err(|_| BadSignature)?;
    let a: Assertion = serde_json::from_slice(&json).map_err(|_| Malformed)?;
    if !a.well_formed() {
        return Err(Malformed);
    }
    if now >= a.expires_at.saturating_add(grace.max(0)) {
        return Err(Expired);
    }
    Ok(a)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(n: u8) -> SigningKey {
        SigningKey::from_bytes(&[n; 32])
    }

    fn sample(space: Option<&str>) -> Assertion {
        Assertion {
            tenant: TenantId::parse("acme").unwrap(),
            principal: None,
            member: MemberId::parse("m1").unwrap(),
            agent: AgentId::parse("a1").unwrap(),
            space: space.map(|s| SpaceId::parse(s).unwrap()),
            conversation: None,
            thread: None,
            body: None,
            purpose: None,
            expires_at: 1000,
        }
    }

    fn forward() -> Assertion {
        Assertion {
            principal: Some(PrincipalKind::Member),
            conversation: Some(Uuid::from_u128(1)),
            thread: Some(Uuid::from_u128(2)),
            body: Some(BodyHash::of(b"{}")),
            purpose: Some(Purpose::Forward),
            ..sample(Some("sp-1"))
        }
    }

    /// A token over exactly these JSON bytes, as another minter (or an old core) would sign it.
    fn raw(json: &[u8], k: &SigningKey) -> String {
        format!(
            "{}.{}",
            B64.encode(json),
            B64.encode(k.sign(json).to_bytes())
        )
    }

    #[test]
    fn round_trip() {
        let turn = Assertion {
            purpose: Some(Purpose::Turn),
            conversation: Some(Uuid::from_u128(1)),
            ..sample(Some("s1"))
        };
        for a in [sample(None), sample(Some("s1")), turn] {
            let t = sign(&a, &key(1));
            assert_eq!(verify(&t, &key(1).verifying_key(), 999), Ok(a));
        }
        let f = forward();
        let t = sign(&f, &key(1));
        assert_eq!(
            verify_with_grace(&t, &key(1).verifying_key(), 999, 0),
            Ok(f)
        );
    }

    #[test]
    fn a_token_minted_before_the_new_fields_still_parses_as_a_members_turn() {
        let json =
            br#"{"tenant":"acme","member":"m1","agent":"a1","space":"s1","expires_at":1000}"#;
        let a = verify(&raw(json, &key(1)), &key(1).verifying_key(), 0).unwrap();
        assert_eq!(a.purpose(), Purpose::Turn);
        assert_eq!(a.principal(), PrincipalKind::Member);
        // And a token without new fields serializes without them, as eve's verifier expects.
        let wire = serde_json::to_value(sample(None)).unwrap();
        let keys: Vec<_> = wire.as_object().unwrap().keys().cloned().collect();
        assert_eq!(keys.len(), 4, "{keys:?}");
    }

    #[test]
    fn verify_accepts_only_turn_tokens() {
        let vk = key(1).verifying_key();
        let answer = Assertion {
            purpose: Some(Purpose::Answer),
            ..sample(None)
        };
        for a in [forward(), answer] {
            let t = sign(&a, &key(1));
            assert_eq!(verify(&t, &vk, 0), Err(AssertionError::WrongPurpose));
            assert_eq!(verify_with_grace(&t, &vk, 0, 0), Ok(a));
        }
    }

    #[test]
    fn grace_extends_expiry_by_exactly_its_length() {
        let vk = key(1).verifying_key();
        let t = sign(&sample(None), &key(1));
        assert_eq!(verify(&t, &vk, 1000), Err(AssertionError::Expired));
        assert!(verify_with_grace(&t, &vk, 1000 + RENEW_GRACE - 1, RENEW_GRACE).is_ok());
        assert_eq!(
            verify_with_grace(&t, &vk, 1000 + RENEW_GRACE, RENEW_GRACE),
            Err(AssertionError::Expired)
        );
        // A negative grace never shortens a token's life below its expiry.
        assert!(verify_with_grace(&t, &vk, 999, -100).is_ok());
    }

    #[test]
    fn field_rules_of_section_5_are_enforced() {
        let vk = key(1).verifying_key();
        let agent = Assertion {
            principal: Some(PrincipalKind::Agent),
            ..sample(None)
        };
        let thread_on_turn = Assertion {
            thread: Some(Uuid::nil()),
            ..sample(None)
        };
        let body_on_answer = Assertion {
            body: Some(BodyHash::of(b"x")),
            purpose: Some(Purpose::Answer),
            ..sample(None)
        };
        let forward_without_thread = Assertion {
            thread: None,
            ..forward()
        };
        for bad in [
            agent,
            thread_on_turn,
            body_on_answer,
            forward_without_thread,
        ] {
            let t = sign(&bad, &key(1));
            assert_eq!(
                verify_with_grace(&t, &vk, 0, 0),
                Err(AssertionError::Malformed),
                "{bad:?}"
            );
        }
        // An agent principal without a member (the contract's wire form) is refused too.
        let json = br#"{"tenant":"acme","principal":"agent","agent":"a1","expires_at":1000}"#;
        assert_eq!(
            verify_with_grace(&raw(json, &key(1)), &vk, 0, 0),
            Err(AssertionError::Malformed)
        );
    }

    #[test]
    fn unknown_purposes_and_bad_body_hashes_are_malformed() {
        let vk = key(1).verifying_key();
        let base = r#""tenant":"acme","member":"m1","agent":"a1","expires_at":1000"#;
        for extra in [
            r#""purpose":"admin""#,
            r#""principal":"root""#,
            r#""purpose":"forward","thread":"00000000-0000-0000-0000-000000000000","body":"short""#,
            r#""purpose":"forward","thread":"not-a-uuid""#,
            r#""conversation":"nope""#,
        ] {
            let json = format!("{{{base},{extra}}}");
            assert_eq!(
                verify_with_grace(&raw(json.as_bytes(), &key(1)), &vk, 0, 0),
                Err(AssertionError::Malformed),
                "{extra}"
            );
        }
    }

    #[test]
    fn body_hash_is_base64url_sha256() {
        let h = BodyHash::of(b"hello");
        let wire = serde_json::to_value(h).unwrap();
        assert_eq!(wire, "LPJNul-wow4m6DsqxbninhsWHlwfp0JecwQzYpOLmCQ");
        assert_eq!(serde_json::from_value::<BodyHash>(wire).unwrap(), h);
    }

    #[test]
    fn expired_is_rejected() {
        let t = sign(&sample(None), &key(1));
        assert_eq!(
            verify(&t, &key(1).verifying_key(), 1000),
            Err(AssertionError::Expired)
        );
    }

    #[test]
    fn tampered_payload_or_signature_is_rejected() {
        let t = sign(&sample(None), &key(1));
        let vk = key(1).verifying_key();
        let (p, s) = t.split_once('.').unwrap();
        let flip = |x: &str| {
            let mut b = x.as_bytes().to_vec();
            b[3] = if b[3] == b'A' { b'B' } else { b'A' };
            String::from_utf8(b).unwrap()
        };
        for bad in [format!("{}.{s}", flip(p)), format!("{p}.{}", flip(s))] {
            assert!(matches!(
                verify(&bad, &vk, 0),
                Err(AssertionError::BadSignature | AssertionError::Malformed)
            ));
        }
    }

    #[test]
    fn wrong_key_is_rejected() {
        let t = sign(&sample(None), &key(1));
        assert_eq!(
            verify(&t, &key(2).verifying_key(), 0),
            Err(AssertionError::BadSignature)
        );
    }

    #[test]
    fn invalid_ids_inside_a_valid_signature_are_rejected() {
        let json = br#"{"tenant":"BAD ID","member":"m1","agent":"a1","expires_at":1000}"#;
        let k = key(1);
        assert_eq!(
            verify(&raw(json, &k), &k.verifying_key(), 0),
            Err(AssertionError::Malformed)
        );
    }

    #[test]
    fn oversized_token_is_rejected_before_decoding() {
        let t = "a".repeat(8192);
        assert_eq!(
            verify(&t, &key(1).verifying_key(), 0),
            Err(AssertionError::Malformed)
        );
    }
}
