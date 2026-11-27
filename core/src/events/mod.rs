//! Core events: a versioned envelope, written to the outbox in the caller's transaction and
//! signed when it is sent. `data` carries ids only, never text, arguments, emails or tokens.
pub mod consume;
pub mod email;
pub mod posthog;
pub mod publisher;
pub mod run;
pub mod sign;

pub use consume::{Consumer, Handler, HandlerError, consumer};
pub use run::start;

use crate::ids::{AgentId, MemberId, TenantId};
use crate::store::{StoreError, Tx};
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

/// How a person proved who they are; a later step (an emailed code, two-step) doesn't change it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SignInMethod {
    Password,
    MagicCode,
    Sso,
}

impl SignInMethod {
    pub fn as_str(self) -> &'static str {
        match self {
            SignInMethod::Password => "password",
            SignInMethod::MagicCode => "magic_code",
            SignInMethod::Sso => "sso",
        }
    }
}

/// Which setting changed; never its value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Setting {
    /// The member turned two-step (TOTP) on.
    TwoStep,
}

/// Which email went out; never its content or address.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EmailTemplate {
    ApprovalRequested,
}

impl EmailTemplate {
    pub fn as_str(self) -> &'static str {
        match self {
            EmailTemplate::ApprovalRequested => "approval_requested",
        }
    }

    pub fn parse(s: &str) -> Option<EmailTemplate> {
        match s {
            "approval_requested" => Some(EmailTemplate::ApprovalRequested),
            _ => None,
        }
    }
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
    /// A person joined by their own sign-up or first sign-in, not through the directory.
    #[serde(rename = "member.signed_up.v1")]
    MemberSignedUp {
        #[schemars(with = "String")]
        member: MemberId,
        method: SignInMethod,
    },
    /// A session was created.
    #[serde(rename = "member.signed_in.v1")]
    MemberSignedIn {
        #[schemars(with = "String")]
        member: MemberId,
        method: SignInMethod,
    },
    /// The member picked this organization (the envelope's tenant) from several.
    #[serde(rename = "organization.chosen.v1")]
    OrganizationChosen {
        #[schemars(with = "String")]
        member: MemberId,
    },
    #[serde(rename = "setting.changed.v1")]
    SettingChanged {
        #[schemars(with = "String")]
        by: MemberId,
        setting: Setting,
    },
    /// SES accepted an email to the member; `cause` is the event that asked for it.
    #[serde(rename = "email.sent.v1")]
    EmailSent {
        #[schemars(with = "String")]
        member: MemberId,
        template: EmailTemplate,
        cause: Uuid,
    },
    /// The member's address bounced for good; it is now suppressed.
    #[serde(rename = "email.bounced.v1")]
    EmailBounced {
        #[schemars(with = "String")]
        member: MemberId,
        template: EmailTemplate,
    },
    /// The member marked an email as spam; their address is now suppressed.
    #[serde(rename = "email.complained.v1")]
    EmailComplained {
        #[schemars(with = "String")]
        member: MemberId,
        template: EmailTemplate,
    },
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
            Event::MemberSignedUp { .. } => "member.signed_up.v1",
            Event::MemberSignedIn { .. } => "member.signed_in.v1",
            Event::OrganizationChosen { .. } => "organization.chosen.v1",
            Event::SettingChanged { .. } => "setting.changed.v1",
            Event::EmailSent { .. } => "email.sent.v1",
            Event::EmailBounced { .. } => "email.bounced.v1",
            Event::EmailComplained { .. } => "email.complained.v1",
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

impl Event {
    /// The member and actor of a people-lifecycle event, which `publish` also records in
    /// `lifecycle_log`; `None` for every other event.
    fn lifecycle(&self) -> Option<(Option<&MemberId>, &Actor)> {
        match self {
            Event::MemberCreated { member, actor }
            | Event::MemberUpdated { member, actor }
            | Event::MemberRemoved { member, actor } => Some((Some(member), actor)),
            Event::OrganizationCreated { actor } => Some((None, actor)),
            _ => None,
        }
    }
}

/// Writes the event to the outbox (and a lifecycle event to `lifecycle_log`) in the caller's
/// transaction, so it is published exactly when the change commits. The tenant is the
/// transaction's; a transaction without one fails.
pub async fn publish(tx: &mut Tx, event: Event) -> Result<(), StoreError> {
    let id = Uuid::new_v4();
    sqlx::query(
        "insert into outbox (tenant_id, id, type, data) \
         values (nullif(current_setting('mandate.tenant', true), ''), $1, $2, $3)",
    )
    .bind(id)
    .bind(event.event_type())
    .bind(sqlx::types::Json(event.data()))
    .execute(&mut **tx)
    .await?;
    if let Some((member, actor)) = event.lifecycle() {
        let (kind, actor_member) = match actor {
            Actor::Member(m) => ("member", Some(m.as_str())),
            Actor::Directory => ("directory", None),
            Actor::System => ("system", None),
        };
        sqlx::query(
            "insert into lifecycle_log (tenant_id, event_id, type, member, actor_kind, actor_member) \
             values (nullif(current_setting('mandate.tenant', true), ''), $1, $2, $3, $4, $5)",
        )
        .bind(id)
        .bind(event.event_type())
        .bind(member.map(MemberId::as_str))
        .bind(kind)
        .bind(actor_member)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
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
            actor: Actor::Member(m.clone()),
        },
        Event::MemberSignedUp {
            member: m.clone(),
            method: SignInMethod::Password,
        },
        Event::MemberSignedIn {
            member: m.clone(),
            method: SignInMethod::MagicCode,
        },
        Event::OrganizationChosen { member: m.clone() },
        Event::SettingChanged {
            by: m.clone(),
            setting: Setting::TwoStep,
        },
        Event::EmailSent {
            member: m.clone(),
            template: EmailTemplate::ApprovalRequested,
            cause: u,
        },
        Event::EmailBounced {
            member: m.clone(),
            template: EmailTemplate::ApprovalRequested,
        },
        Event::EmailComplained {
            member: m,
            template: EmailTemplate::ApprovalRequested,
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
        assert_eq!(all.len(), 17, "add every new variant to samples()");
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
            "method",
            "setting",
            // email.*: which template (an enum) and the event that caused the send (an id).
            "template",
            "cause",
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
