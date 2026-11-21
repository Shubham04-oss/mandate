use serde::{Deserialize, Deserializer, Serialize};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum IdError {
    #[error("id must be 1 to 64 characters, got {0}")]
    Length(usize),
    #[error("id contains invalid character {0:?}")]
    Char(char),
}

// Lowercase ascii, digits, `-` and `_` (plus `.` for tool ids); ids reach policy and storage keys.
fn validate(s: &str, allow_dot: bool) -> Result<(), IdError> {
    if s.is_empty() || s.len() > 64 {
        return Err(IdError::Length(s.len()));
    }
    match s.chars().find(|c| {
        !(c.is_ascii_lowercase()
            || c.is_ascii_digit()
            || matches!(c, '-' | '_')
            || (allow_dot && *c == '.'))
    }) {
        Some(c) => Err(IdError::Char(c)),
        None => Ok(()),
    }
}

macro_rules! id_type {
    ($name:ident, $dot:expr) => {
        #[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            pub fn parse(s: &str) -> Result<Self, IdError> {
                validate(s, $dot)?;
                Ok(Self(s.to_owned()))
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                let s = String::deserialize(d)?;
                Self::parse(&s).map_err(serde::de::Error::custom)
            }
        }
    };
}

id_type!(TenantId, false);
id_type!(MemberId, false);
id_type!(TeamId, false);
id_type!(AgentId, false);
id_type!(SpaceId, false);
id_type!(ToolId, true);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_valid_and_rejects_invalid() {
        assert!(TenantId::parse("acme-co_1").is_ok());
        for bad in ["", "Acme", "a b", "a/b", &"x".repeat(65), "é"] {
            assert!(TenantId::parse(bad).is_err(), "{bad:?}");
        }
        assert!(ToolId::parse("web.read_page").is_ok());
        assert!(AgentId::parse("web.read_page").is_err());
    }

    #[test]
    fn deserialize_validates() {
        assert!(serde_json::from_str::<MemberId>("\"ok\"").is_ok());
        assert!(serde_json::from_str::<MemberId>("\"NOT OK\"").is_err());
    }
}
