//! Core events: a versioned envelope, written to the outbox in the caller's transaction and
//! signed when it is sent. `data` carries ids only, never text, arguments, emails or tokens.


use crate::ids::{AgentId, MemberId, TenantId};
use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};
use std::fmt;
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NameError {
    #[error("invalid event source {0:?}")]
    Source(String),
    #[error("invalid event type {0:?}")]
    Type(String),
}

fn word(s: &str) -> bool {
    !s.is_empty() && s.len() <= 32 && s.chars().all(|c| c.is_ascii_lowercase() || c == '_')
}

/// `mandate.core`, or `mandate.app.<provider>` for app events.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[serde(transparent)]
pub struct Source(String);

impl Source {
    pub const CORE: &'static str = "mandate.core";

    pub fn core() -> Source {
        Source(Self::CORE.to_owned())
    }

    pub fn parse(s: &str) -> Result<Source, NameError> {
        let ok = s == Self::CORE || s.strip_prefix("mandate.app.").is_some_and(word);
        ok.then(|| Source(s.to_owned()))
            .ok_or_else(|| NameError::Source(s.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// `<entity>.<verb>.v<N>`; a breaking change is a new version, never an edit.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[serde(transparent)]
pub struct EventType(String);

impl EventType {
    pub fn parse(s: &str) -> Result<EventType, NameError> {
        let bad = || NameError::Type(s.to_owned());
        let mut parts = s.split('.');
        let (Some(entity), Some(verb), Some(v), None) =
            (parts.next(), parts.next(), parts.next(), parts.next())
        else {
            return Err(bad());
        };
        let n = v.strip_prefix('v').ok_or_else(bad)?;
        let version_ok = !n.is_empty()
            && n.len() <= 4
            && !n.starts_with('0')
            && n.chars().all(|c| c.is_ascii_digit());
        if word(entity) && word(verb) && version_ok {
            Ok(EventType(s.to_owned()))
        } else {
            Err(bad())
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

macro_rules! parse_on_deserialize {
    ($t:ty) => {
        impl<'de> Deserialize<'de> for $t {
            fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                <$t>::parse(&String::deserialize(d)?).map_err(serde::de::Error::custom)
            }
        }
        impl fmt::Display for $t {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }
    };
}
parse_on_deserialize!(Source);
parse_on_deserialize!(EventType);

/// What a consumer sees, only after its signature, key and age were checked.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Envelope {
    pub id: Uuid,
    pub source: Source,
    pub r#type: EventType,
    pub tenant: TenantId,
    #[serde(with = "time::serde::rfc3339")]
    pub occurred_at: OffsetDateTime,
    pub data: serde_json::Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Decision {
    Approved,
    Denied,
}

/// Who caused a lifecycle change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", content = "id", rename_all = "lowercase")]
pub enum Actor {
    Member(#[schemars(with = "String")] MemberId),
    Directory,
    System,
}

/// The core's events. The serde tag is the event type, so `data` decodes by type alone.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", content = "data")]
pub enum Event {
    #[serde(rename = "approval.requested.v1")]
    ApprovalRequested {
        approval: Uuid,
        #[schemars(with = "String")]
        agent: AgentId,
    },
    #[serde(rename = "approval.decided.v1")]
    ApprovalDecided {
        approval: Uuid,
        decision: Decision,
        #[schemars(with = "String")]
        by: MemberId,
    },
    #[serde(rename = "conversation.started.v1")]
    ConversationStarted { conversation: Uuid },
    #[serde(rename = "conversation.finished.v1")]
    ConversationFinished { conversation: Uuid },
    #[serde(rename = "grant.created.v1")]
    GrantCreated { grant: Uuid },
    #[serde(rename = "grant.revoked.v1")]
    GrantRevoked { grant: Uuid },
    #[serde(rename = "member.created.v1")]
    MemberCreated {
        #[schemars(with = "String")]
        member: MemberId,
        actor: Actor,
    },
    #[serde(rename = "member.updated.v1")]
    MemberUpdated {
        #[schemars(with = "String")]
        member: MemberId,
        actor: Actor,
    },
    #[serde(rename = "member.removed.v1")]
    MemberRemoved {
        #[schemars(with = "String")]
        member: MemberId,
        actor: Actor,
    },
    #[serde(rename = "organization.created.v1")]
    OrganizationCreated { actor: Actor },
}

#[derive(Debug, thiserror::Error)]
#[error("event data does not match its type: {0}")]
pub struct DecodeError(#[from] serde_json::Error);

impl Event {
    pub fn event_type(&self) -> &'static str {
        match self {
            Event::ApprovalRequested { .. } => "approval.requested.v1",
            Event::ApprovalDecided { .. } => "approval.decided.v1",
            Event::ConversationStarted { .. } => "conversation.started.v1",
            Event::ConversationFinished { .. } => "conversation.finished.v1",
            Event::GrantCreated { .. } => "grant.created.v1",
            Event::GrantRevoked { .. } => "grant.revoked.v1",
            Event::MemberCreated { .. } => "member.created.v1",
            Event::MemberUpdated { .. } => "member.updated.v1",
            Event::MemberRemoved { .. } => "member.removed.v1",
            Event::OrganizationCreated { .. } => "organization.created.v1",
        }
    }

    /// The `data` object of the envelope.
    pub fn data(&self) -> serde_json::Value {
        // Ids, uuids and unit enums always serialize.
        match serde_json::to_value(self) {
            Ok(serde_json::Value::Object(mut m)) => m.remove("data").unwrap_or_default(),
            _ => serde_json::Value::Null,
        }
    }

    /// The typed event inside a verified envelope.
    pub fn from_envelope(env: &Envelope) -> Result<Event, DecodeError> {
        Ok(serde_json::from_value(serde_json::json!({
            "type": env.r#type.as_str(),
            "data": env.data,
        }))?)
    }
}

/// The wire form, for the checked-in JSON Schema only.
#[allow(dead_code)]
#[derive(JsonSchema)]
#[schemars(title = "mandate event v1")]
struct Wire {
    id: Uuid,
    /// `mandate.core`, or `mandate.app.<provider>`.
    source: String,
    tenant: String,
    /// RFC 3339.
    occurred_at: String,
    /// Names the Ed25519 key that signed the envelope; it belongs to one source.
    kid: String,
    /// Ed25519 over the canonical JSON (sorted keys, no whitespace) of every other field,
    /// base64url without padding.
    signature: String,
    #[serde(flatten)]
    event: Event,
}

pub fn schema() -> serde_json::Value {
    serde_json::to_value(schemars::schema_for!(Wire)).unwrap_or_default()
}

#[cfg(test)]
pub(crate) fn samples() -> Vec<Event> {
    let m = MemberId::parse("m1").unwrap();
    let u = Uuid::nil();
    vec![
        Event::ApprovalRequested {
            approval: u,
            agent: AgentId::parse("a1").unwrap(),
        },
        Event::ApprovalDecided {
            approval: u,
            decision: Decision::Approved,
            by: m.clone(),
        },
        Event::ConversationStarted { conversation: u },
        Event::ConversationFinished { conversation: u },
        Event::GrantCreated { grant: u },
        Event::GrantRevoked { grant: u },
        Event::MemberCreated {
            member: m.clone(),
            actor: Actor::Member(m.clone()),
        },
        Event::MemberUpdated {
            member: m.clone(),
            actor: Actor::Directory,
        },
        Event::MemberRemoved {
            member: m.clone(),
            actor: Actor::System,
        },
        Event::OrganizationCreated {
            actor: Actor::Member(m),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    #[test]
    fn type_names_are_versioned() {
        let all = samples();
        assert_eq!(all.len(), 10, "add every new variant to samples()");
        for e in &all {
            let t = e.event_type();
            assert!(EventType::parse(t).is_ok(), "{t}");
            assert!(t.ends_with(".v1"), "{t}");
            // The name in code and the serde tag on the wire agree.
            assert_eq!(serde_json::to_value(e).unwrap()["type"], t);
        }
        for bad in [
            "approval.decided",
            "approval.decided.v0",
            "approval.decided.v01",
            "Approval.decided.v1",
            "a.b.c.v1",
            "approval..v1",
            "approval.decided.vx",
        ] {
            assert!(EventType::parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn sources_are_core_or_app() {
        assert!(Source::parse("mandate.core").is_ok());
        assert!(Source::parse("mandate.app.gmail").is_ok());
        for bad in [
            "mandate",
            "mandate.app.",
            "mandate.app.Gmail",
            "other.core",
            "mandate.app.a.b",
        ] {
            assert!(Source::parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn data_carries_ids_only() {
        const FIELDS: &[&str] = &[
            "approval",
            "agent",
            "decision",
            "by",
            "conversation",
            "grant",
            "member",
            "actor",
            "kind",
            "id",
        ];
        fn walk(v: &Value) {
            match v {
                Value::Object(m) => m.iter().for_each(|(k, v)| {
                    assert!(FIELDS.contains(&k.as_str()), "unexpected field {k}");
                    walk(v)
                }),
                // Ids and enum names: short, no spaces, nothing that could be text.
                Value::String(s) => assert!(s.len() <= 64 && !s.contains(' '), "{s:?}"),
                other => panic!("not an id: {other}"),
            }
        }
        for e in samples() {
            let d = e.data();
            assert!(d.is_object(), "{}", e.event_type());
            walk(&d);
        }
    }

    #[test]
    fn data_decodes_by_type() {
        for e in samples() {
            let env = Envelope {
                id: Uuid::nil(),
                source: Source::core(),
                r#type: EventType::parse(e.event_type()).unwrap(),
                tenant: TenantId::parse("t1").unwrap(),
                occurred_at: OffsetDateTime::UNIX_EPOCH,
                data: e.data(),
            };
            assert_eq!(Event::from_envelope(&env).unwrap(), e);
        }
    }

    #[test]
    fn schema_matches_checked_in_file() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../schema/events/v1.json");
        let generated = format!("{}\n", serde_json::to_string_pretty(&schema()).unwrap());
        if std::env::var("MANDATE_WRITE_SCHEMA").as_deref() == Ok("1") {
            std::fs::write(path, &generated).unwrap();
        }
        let checked_in = std::fs::read_to_string(path).unwrap_or_default();
        assert!(
            checked_in == generated,
            "schema/events/v1.json is stale: run with MANDATE_WRITE_SCHEMA=1"
        );
    }
}
