// stream A: SecretsManagerStore and the channel-cleanup consumer land here.
//! Provider tokens, behind a narrow interface the token-vault work replaces.
use crate::ids::TenantId;
use async_trait::async_trait;
use std::fmt;
use uuid::Uuid;

/// Provider tokens. A narrow interface on purpose: the vault work replaces the implementation.
#[async_trait]
pub trait CredentialStore: Send + Sync {
    async fn put(&self, key: &CredentialKey, secret: &Secret) -> Result<(), CredentialError>;
    async fn get(&self, key: &CredentialKey) -> Result<Secret, CredentialError>;
    async fn delete(&self, key: &CredentialKey) -> Result<(), CredentialError>;
}

/// Built from ids only (`<prefix><tenant>/<installation>`), never from input.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CredentialKey {
    pub tenant: TenantId,
    pub installation: Uuid,
}

/// Debug and Display print `[redacted]`.
#[derive(Clone, PartialEq, Eq)]
pub struct Secret(String);

impl Secret {
    pub fn new(s: String) -> Self {
        Secret(s)
    }

    /// The token itself, only for the provider call that needs it.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[redacted]")
    }
}

impl fmt::Display for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[redacted]")
    }
}

// ponytail: Secrets Manager behind CredentialStore until the token-vault design lands.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CredentialError {
    #[error("no credential stored for this installation")]
    NotFound,
    #[error("credential store unavailable")]
    Unavailable,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_secret_never_prints() {
        let s = Secret::new("xoxb-real-token".into());
        assert_eq!(format!("{s:?} {s}"), "[redacted] [redacted]");
        assert_eq!(s.expose(), "xoxb-real-token");
    }
}
