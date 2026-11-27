//! Sends the core's product events to PostHog (EU), so usage is counted where it happens rather
//! than in the browser. Ids and enums only: every property below is built field by field, never
//! by copying `data`, so a new field can't reach PostHog without being added here.
use super::{Actor, Decision, Envelope, Event, Handler, HandlerError, Setting, Source};
use crate::store::Tx;
use async_trait::async_trait;
use serde_json::{Map, Value, json};
use std::time::Duration;
use time::format_description::well_known::Rfc3339;
use url::Url;

const TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, thiserror::Error)]
pub enum PosthogError {
    #[error("POSTHOG_HOST: {0}")]
    Host(String),
    #[error("PostHog client: {0}")]
    Client(#[source] reqwest::Error),
}

pub struct Posthog {
    http: reqwest::Client,
    batch: Url,
    key: String,
}

impl Posthog {
    /// `host` is PostHog's ingestion origin, e.g. `https://eu.i.posthog.com`; only https.
    pub fn new(host: &str, key: String) -> Result<Self, PosthogError> {
        let base = Url::parse(host).map_err(|e| PosthogError::Host(e.to_string()))?;
        if base.scheme() != "https" {
            return Err(PosthogError::Host("must be https".into()));
        }
        let batch = base
            .join("/batch/")
            .map_err(|e| PosthogError::Host(e.to_string()))?;
        Self::at(batch, key)
    }

    fn at(batch: Url, key: String) -> Result<Self, PosthogError> {
        let http = reqwest::Client::builder()
            .timeout(TIMEOUT)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(PosthogError::Client)?;
        Ok(Self { http, batch, key })
    }
}

/// The PostHog event for a core event: its name, who did it and its properties.
fn capture(env: &Envelope, event: &Event) -> Value {
    let id = |u: &uuid::Uuid| Value::String(u.to_string());
    let mut props = Map::new();
    // `None` is an event no one person did; it counts for the organization only.
    let (name, person) = match event {
        Event::ApprovalRequested { approval, agent } => {
            props.insert("approval_id".into(), id(approval));
            props.insert("agent_id".into(), agent.as_str().into());
            ("approval_requested", None)
        }
        Event::ApprovalDecided {
            approval,
            decision,
            by,
        } => {
            props.insert("approval_id".into(), id(approval));
            let outcome = match decision {
                Decision::Approved => "approved",
                Decision::Denied => "denied",
            };
            props.insert("outcome".into(), outcome.into());
            ("approval_decided", Some(by.as_str()))
        }
        Event::ConversationStarted { conversation } => {
            props.insert("conversation_id".into(), id(conversation));
            ("conversation_started", None)
        }
        Event::ConversationFinished { conversation } => {
            props.insert("conversation_id".into(), id(conversation));
            ("conversation_finished", None)
        }
        Event::GrantCreated { grant } => {
            props.insert("grant_id".into(), id(grant));
            ("grant_created", None)
        }
        Event::GrantRevoked { grant } => {
            props.insert("grant_id".into(), id(grant));
            ("grant_revoked", None)
        }
        Event::MemberCreated { member, actor }
        | Event::MemberUpdated { member, actor }
        | Event::MemberRemoved { member, actor } => {
            props.insert("member_id".into(), member.as_str().into());
            let name = match event {
                Event::MemberCreated { .. } => "member_created",
                Event::MemberUpdated { .. } => "member_updated",
                _ => "member_removed",
            };
            (name, actor_member(actor, &mut props))
        }
        Event::OrganizationCreated { actor } => {
            ("organization_created", actor_member(actor, &mut props))
        }
        Event::MemberSignedUp { member, method } | Event::MemberSignedIn { member, method } => {
            props.insert("method".into(), method.as_str().into());
            let name = match event {
                Event::MemberSignedUp { .. } => "signed_up",
                _ => "signed_in",
            };
            (name, Some(member.as_str()))
        }
        Event::OrganizationChosen { member } => ("org_chosen", Some(member.as_str())),
        Event::SettingChanged { by, setting } => {
            let setting = match setting {
                Setting::TwoStep => "two_step",
            };
            props.insert("setting".into(), setting.into());
            ("settings_changed", Some(by.as_str()))
        }
    };
    props.insert(
        "$groups".into(),
        json!({ "organization": env.tenant.as_str() }),
    );
    props.insert("source".into(), "core".into());
    // The same member id the web app identifies with, so both sides land on one person.
    let distinct_id = match person {
        Some(m) => m.to_owned(),
        None => {
            props.insert("$process_person_profile".into(), false.into());
            format!("organization:{}", env.tenant.as_str())
        }
    };
    json!({
        "event": name,
        "distinct_id": distinct_id,
        // PostHog drops a repeated uuid, so a redelivered event counts once.
        "uuid": env.id.to_string(),
        "timestamp": env.occurred_at.format(&Rfc3339).unwrap_or_default(),
        "properties": props,
    })
}

fn actor_member<'a>(actor: &'a Actor, props: &mut Map<String, Value>) -> Option<&'a str> {
    let (kind, member) = match actor {
        Actor::Member(m) => ("member", Some(m.as_str())),
        Actor::Directory => ("directory", None),
        Actor::System => ("system", None),
    };
    props.insert("actor_kind".into(), kind.into());
    member
}

#[async_trait]
impl Handler for Posthog {
    async fn handle(&self, _: &mut Tx, env: &Envelope) -> Result<(), HandlerError> {
        self.send(env).await
    }
}

impl Posthog {
    async fn send(&self, env: &Envelope) -> Result<(), HandlerError> {
        // App events and data that doesn't decode are not product usage; retrying won't change that.
        if env.source != Source::core() {
            return Ok(());
        }
        let Ok(event) = Event::from_envelope(env) else {
            return Ok(());
        };
        let body = json!({ "api_key": self.key, "batch": [capture(env, &event)] });
        let res = self
            .http
            .post(self.batch.clone())
            .json(&body)
            .send()
            .await
            .map_err(|e| HandlerError::Retry(format!("posthog: {}", e.without_url())))?;
        if !res.status().is_success() {
            return Err(HandlerError::Retry(format!(
                "posthog answered {}",
                res.status()
            )));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{EventType, samples};
    use crate::ids::TenantId;
    use time::OffsetDateTime;
    use uuid::Uuid;

    fn env(event: &Event) -> Envelope {
        Envelope {
            id: Uuid::from_u128(7),
            source: Source::core(),
            r#type: EventType::parse(event.event_type()).unwrap(),
            tenant: TenantId::parse("acme").unwrap(),
            occurred_at: OffsetDateTime::UNIX_EPOCH,
            data: event.data(),
        }
    }

    #[test]
    fn every_event_is_ids_and_enums_grouped_by_organization() {
        for event in samples() {
            let c = capture(&env(&event), &event);
            assert_eq!(c["uuid"], Uuid::from_u128(7).to_string());
            assert_eq!(c["properties"]["$groups"]["organization"], "acme");
            assert_eq!(c["properties"]["source"], "core");
            for (k, v) in c["properties"].as_object().unwrap() {
                // Ids from the samples, uuids, enums and flags: nothing else may appear.
                let ok = match v {
                    Value::String(s) => {
                        s.len() <= 64
                            && s.bytes()
                                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
                    }
                    Value::Bool(_) | Value::Object(_) => true,
                    _ => false,
                };
                assert!(ok, "{} {k}: {v}", event.event_type());
            }
        }
    }

    #[test]
    fn a_decision_is_its_deciders_and_a_system_event_no_ones() {
        let m = crate::ids::MemberId::parse("m1").unwrap();
        let decided = Event::ApprovalDecided {
            approval: Uuid::nil(),
            decision: Decision::Denied,
            by: m.clone(),
        };
        let c = capture(&env(&decided), &decided);
        assert_eq!(c["event"], "approval_decided");
        assert_eq!(c["distinct_id"], "m1");
        assert_eq!(c["properties"]["outcome"], "denied");
        assert!(c["properties"].get("$process_person_profile").is_none());

        let removed = Event::MemberRemoved {
            member: m,
            actor: Actor::Directory,
        };
        let c = capture(&env(&removed), &removed);
        assert_eq!(c["distinct_id"], "organization:acme");
        assert_eq!(c["properties"]["$process_person_profile"], false);
        assert_eq!(c["properties"]["actor_kind"], "directory");
    }

    #[test]
    fn auth_and_settings_events_are_the_members_own() {
        use crate::events::SignInMethod;
        let m = crate::ids::MemberId::parse("m1").unwrap();
        let cases = [
            (
                Event::MemberSignedUp {
                    member: m.clone(),
                    method: SignInMethod::Sso,
                },
                "signed_up",
                ("method", Some("sso")),
            ),
            (
                Event::MemberSignedIn {
                    member: m.clone(),
                    method: SignInMethod::MagicCode,
                },
                "signed_in",
                ("method", Some("magic_code")),
            ),
            (
                Event::OrganizationChosen { member: m.clone() },
                "org_chosen",
                ("method", None),
            ),
            (
                Event::SettingChanged {
                    by: m.clone(),
                    setting: Setting::TwoStep,
                },
                "settings_changed",
                ("setting", Some("two_step")),
            ),
        ];
        for (event, name, (prop, value)) in cases {
            let c = capture(&env(&event), &event);
            assert_eq!(c["event"], name);
            assert_eq!(c["distinct_id"], "m1", "{name}");
            assert!(c["properties"].get("$process_person_profile").is_none());
            assert_eq!(c["properties"].get(prop).and_then(Value::as_str), value);
        }
    }

    /// A fake PostHog answering `status`; returns its `/batch/` URL and the bodies it got.
    async fn fake(status: http::StatusCode) -> (Url, std::sync::Arc<std::sync::Mutex<Vec<Value>>>) {
        use axum::{Json, Router, routing::post};
        let got = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let seen = got.clone();
        let app = Router::new().route(
            "/batch/",
            post(move |Json(v): Json<Value>| async move {
                seen.lock().unwrap().push(v);
                status
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await });
        (format!("http://{addr}/batch/").parse().unwrap(), got)
    }

    #[tokio::test]
    async fn sends_one_batch_and_retries_when_posthog_fails() {
        let event = Event::GrantCreated { grant: Uuid::nil() };
        let (url, got) = fake(http::StatusCode::OK).await;
        let p = Posthog::at(url, "phc_test".into()).unwrap();
        p.send(&env(&event)).await.unwrap();
        let body = got.lock().unwrap().pop().unwrap();
        assert_eq!(body["api_key"], "phc_test");
        assert_eq!(body["batch"][0]["event"], "grant_created");

        let (url, _) = fake(http::StatusCode::SERVICE_UNAVAILABLE).await;
        let p = Posthog::at(url, "k".into()).unwrap();
        assert!(matches!(
            p.send(&env(&event)).await,
            Err(HandlerError::Retry(_))
        ));
    }

    #[tokio::test]
    async fn app_events_and_undecodable_data_are_not_sent() {
        let (url, got) = fake(http::StatusCode::OK).await;
        let p = Posthog::at(url, "k".into()).unwrap();
        let event = Event::GrantCreated { grant: Uuid::nil() };
        let mut app = env(&event);
        app.source = Source::parse("mandate.app.slack").unwrap();
        p.send(&app).await.unwrap();
        let mut bad = env(&event);
        bad.data = json!({ "grant": "not a uuid" });
        p.send(&bad).await.unwrap();
        assert!(got.lock().unwrap().is_empty());
    }

    #[test]
    fn only_an_https_host_is_accepted() {
        assert!(Posthog::new("https://eu.i.posthog.com", "k".into()).is_ok());
        assert!(Posthog::new("http://eu.i.posthog.com", "k".into()).is_err());
        assert!(Posthog::new("eu.i.posthog.com", "k".into()).is_err());
        let p = Posthog::new("https://eu.i.posthog.com/ignored", "k".into()).unwrap();
        assert_eq!(p.batch.as_str(), "https://eu.i.posthog.com/batch/");
    }
}
