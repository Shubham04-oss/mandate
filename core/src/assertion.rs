use crate::ids::{AgentId, MemberId, SpaceId, TenantId};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD as B64};
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};

pub const TTL_SECS: i64 = 300;
const MAX_TOKEN_LEN: usize = 4096;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Assertion {
    pub tenant: TenantId,
    pub member: MemberId,
    pub agent: AgentId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub space: Option<SpaceId>,
    pub expires_at: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum AssertionError {
    #[error("malformed assertion")]
    Malformed,
    #[error("bad assertion signature")]
    BadSignature,
    #[error("assertion expired")]
    Expired,
}

pub fn sign(a: &Assertion, key: &SigningKey) -> String {
    // Serializing plain strings and an integer cannot fail.
    let json = serde_json::to_vec(a).unwrap_or_default();
    format!(
        "{}.{}",
        B64.encode(&json),
        B64.encode(key.sign(&json).to_bytes())
    )
}

pub fn verify(token: &str, key: &VerifyingKey, now: i64) -> Result<Assertion, AssertionError> {
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
    if now >= a.expires_at {
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
            member: MemberId::parse("m1").unwrap(),
            agent: AgentId::parse("a1").unwrap(),
            space: space.map(|s| SpaceId::parse(s).unwrap()),
            expires_at: 1000,
        }
    }

    #[test]
    fn round_trip() {
        for a in [sample(None), sample(Some("s1"))] {
            let t = sign(&a, &key(1));
            assert_eq!(verify(&t, &key(1).verifying_key(), 999), Ok(a));
        }
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
        let t = format!(
            "{}.{}",
            B64.encode(json),
            B64.encode(k.sign(json).to_bytes())
        );
        assert_eq!(
            verify(&t, &k.verifying_key(), 0),
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
