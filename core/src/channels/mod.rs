//! Channels (docs/channels.md): the core in front of every channel (gateway, in) and behind it
//! (proxy, out). These are the shared types and traits every stream builds on; provider ids in
//! them never reach a log, an event or a span.
pub mod credentials;
pub mod decision;
pub mod gateway;
pub mod identity;
pub mod notify;
pub mod proxy;
pub mod registry;
pub mod slack;

pub use credentials::{CredentialError, CredentialKey, CredentialStore, Secret};

use crate::events::Decision;
use crate::ids::{AgentId, MemberId, SpaceId, TenantId};
use crate::store::StoreError;
use async_trait::async_trait;
use axum::response::Response;
use http::HeaderMap;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fmt;
use time::OffsetDateTime;
use url::Url;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ChannelKind {
    Web,
    Slack,
    Whatsapp,
    Sms,
}

impl ChannelKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ChannelKind::Web => "web",
            ChannelKind::Slack => "slack",
            ChannelKind::Whatsapp => "whatsapp",
            ChannelKind::Sms => "sms",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpaceKind {
    Direct,
    Shared,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("invalid provider value")]
pub struct ProviderValueError;

// Provider values reach storage and eve, never logs: a narrow charset and a length cap.
fn provider_value(s: &str, max: usize) -> Result<String, ProviderValueError> {
    let ok = !s.is_empty()
        && s.len() <= max
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b':' | b'+' | b'.' | b'_' | b'-'));
    ok.then(|| s.to_owned()).ok_or(ProviderValueError)
}

macro_rules! provider_type {
    ($(#[$doc:meta])* $name:ident, $max:expr) => {
        $(#[$doc])*
        #[derive(Clone, PartialEq, Eq, Hash)]
        pub struct $name(String);

        impl $name {
            pub fn parse(s: &str) -> Result<Self, ProviderValueError> {
                provider_value(s, $max).map(Self)
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        // Redacted so a provider id cannot reach a log through `{:?}`.
        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(concat!(stringify!($name), "([redacted])"))
            }
        }
    };
}

provider_type!(
    /// A provider's account id, validated: 1..=128 of [A-Za-z0-9:+._-]. Never logged or sent in events.
    ExternalId,
    128
);
provider_type!(
    /// eve's continuation token for a channel; parsed, never built from unchecked input.
    ThreadKey,
    256
);
provider_type!(
    /// Where a card the core posted landed (Slack `<channel>:<ts>`, a Twilio message sid).
    MessageRef,
    256
);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalUser {
    pub kind: ChannelKind,
    pub id: ExternalId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Installation {
    pub tenant: TenantId,
    pub id: Uuid,
    pub kind: ChannelKind,
    pub external_id: Option<ExternalId>,
    pub agent: Option<AgentId>,
}

/// What an adapter reads from a request it has verified. No `Debug`: it holds decision tokens.
#[derive(Clone, PartialEq, Eq)]
pub enum Inbound {
    Challenge(String),
    Lifecycle {
        install: ExternalId,
        removed: bool,
    },
    CoreAction {
        install: InstallKey,
        sender: ExternalUser,
        token: String,
    },
    Message {
        install: InstallKey,
        sender: Sender,
        space: (ExternalId, SpaceKind),
        thread: ThreadKey,
        event_id: Option<String>,
    },
    Ignore,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstallKey {
    Team(ExternalId),
    Shared,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Sender {
    User(ExternalUser),
    Bot,
}

/// Messages never carry provider text or input: they reach logs and error replies.
#[derive(Debug, thiserror::Error)]
pub enum ChannelError {
    #[error("provider signature did not verify")]
    BadSignature,
    #[error("provider request is outside the accepted time window")]
    Stale,
    #[error("provider request could not be parsed")]
    Malformed,
    #[error("refused: not on the channel's allowlist")]
    Refused,
    #[error("no live installation")]
    NotInstalled,
    #[error("provider answered {0}")]
    Provider(u16),
    #[error("provider unreachable")]
    Unreachable,
    #[error(transparent)]
    Credential(#[from] CredentialError),
    #[error(transparent)]
    Store(#[from] StoreError),
}

#[async_trait]
pub trait Adapter: Send + Sync {
    fn kind(&self) -> ChannelKind;
    /// The provider's signature over the raw request: the only trust step before parsing.
    fn verify(
        &self,
        headers: &HeaderMap,
        url: &str,
        body: &[u8],
        now: OffsetDateTime,
    ) -> Result<(), ChannelError>;
    fn parse(&self, headers: &HeaderMap, body: &[u8]) -> Result<Inbound, ChannelError>;
    /// The body eve receives: provider file URLs pointed at the proxy.
    fn rewrite_inbound(&self, body: &[u8]) -> Result<Vec<u8>, ChannelError>;
    /// eve's route path, and the extra headers its channel needs (Twilio's re-signature).
    fn eve_route(&self) -> &'static str;
    fn forward_headers(&self, body: &[u8]) -> Result<HeaderMap, ChannelError>;
    /// Tell an unlinked sender how to link; returns the provider response for the webhook.
    async fn prompt_link(
        &self,
        inst: &Installation,
        to: &ExternalUser,
        link: &Url,
    ) -> Result<Response, ChannelError>;
    /// The proxy's rule for one outbound call, or `None` to refuse it.
    fn outbound(&self, method: &str) -> Option<Outbound>;
    /// Every person-visible string in an outbound body, or `Err` for anything unknown.
    fn visible_text(&self, method: &str, body: &Value) -> Result<String, ChannelError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Outbound {
    pub name: &'static str,
    pub write: bool,
    pub guarded: bool,
    pub stream: bool,
}

/// A card the core renders; adapters turn it into Block Kit or a WhatsApp template.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Card {
    pub approval: Uuid,
    pub state: CardState,
    /// Display name; adapters escape it.
    pub requester: String,
    pub agent: String,
    /// `None` in shared spaces, or when the guard blocked the summary.
    pub detail: Option<CardDetail>,
    pub expires_at: OffsetDateTime,
    /// `{web}/approvals/<id>`.
    pub open_url: Url,
    /// Empty once decided.
    pub buttons: Vec<Button>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CardState {
    Pending,
    /// `by` is the decider's display name.
    Approved {
        by: String,
        via: ChannelKind,
    },
    Denied {
        by: String,
        via: ChannelKind,
    },
    Expired,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CardDetail {
    pub tool: String,
    pub args_summary: String,
}

#[derive(Clone, PartialEq, Eq)]
pub struct Button {
    pub decision: Decision,
    pub token: String,
}

// A decision token never reaches a log.
impl fmt::Debug for Button {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Button")
            .field("decision", &self.decision)
            .field("token", &"[redacted]")
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Destination {
    Thread { thread: ThreadKey, space: SpaceId },
    Direct { to: ExternalUser },
}

#[async_trait]
pub trait Deliver: Send + Sync {
    fn kind(&self) -> ChannelKind;
    async fn post(
        &self,
        inst: &Installation,
        to: &Destination,
        card: &Card,
    ) -> Result<MessageRef, ChannelError>;
    async fn update(
        &self,
        inst: &Installation,
        msg: &MessageRef,
        card: &Card,
    ) -> Result<(), ChannelError>;
}

/// Approval clicks and other actions the core handles itself (stream B implements it).
#[async_trait]
pub trait CoreActions: Send + Sync {
    async fn handle(
        &self,
        tenant: &TenantId,
        by: &MemberId,
        token: &str,
        via: ChannelKind,
    ) -> ActionReply;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionReply {
    Decided(Decision),
    AlreadyDecided,
    NotAllowed,
    Expired,
    Invalid,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_values_are_checked() {
        for ok in [
            "T0123:U0456",
            "+15551234567",
            "C01.1700000000.000200",
            "a_b-c",
        ] {
            assert!(ExternalId::parse(ok).is_ok(), "{ok}");
        }
        let long = "x".repeat(129);
        for bad in ["", "a b", "a/b", "<@U1>", "é", "a\nb", long.as_str()] {
            assert!(ExternalId::parse(bad).is_err(), "{bad:?}");
        }
        assert!(ThreadKey::parse(&"x".repeat(256)).is_ok());
        assert!(ThreadKey::parse(&"x".repeat(257)).is_err());
        assert!(MessageRef::parse("C01:1700000000.000200").is_ok());
    }

    #[test]
    fn provider_values_never_print() {
        let id = ExternalId::parse("T0123:U0456").unwrap();
        let shown = format!(
            "{:?}",
            ExternalUser {
                kind: ChannelKind::Slack,
                id
            }
        );
        assert!(!shown.contains("T0123"), "{shown}");
        let key = ThreadKey::parse("C01:1700000000.000200").unwrap();
        assert!(!format!("{key:?}").contains("C01"));
        let b = Button {
            decision: Decision::Approved,
            token: "d1.secret".into(),
        };
        assert!(!format!("{b:?}").contains("secret"));
    }

    #[test]
    fn kinds_match_the_database_and_wire_names() {
        for k in [
            ChannelKind::Web,
            ChannelKind::Slack,
            ChannelKind::Whatsapp,
            ChannelKind::Sms,
        ] {
            assert_eq!(serde_json::to_value(k).unwrap(), k.as_str());
        }
    }
}
