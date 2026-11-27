//! The email subscriber and SES feedback. Live: need DATABASE_URL (a superuser), `--ignored`.
mod api_support;
mod common;
use api_support::*;
use async_trait::async_trait;
use common::app_store;
use ed25519_dalek::SigningKey;
use mandate::api::approvals::{Decision, decide_approval, deciders};
use mandate::email::feedback::{self, Feedback, Kind};
use mandate::email::templates::Templates;
use mandate::email::{Mail, MailError, Mailer, OrgNames, address_hash};
use mandate::events::consume::{Consumer, Outcome};
use mandate::events::email::{Email, TEMPLATES};
use mandate::events::sign::{Keyring, Signer};
use mandate::events::{EmailTemplate, Envelope, Event, EventType, Source, consumer};
use mandate::ids::{AgentId, MemberId, TenantId};
use mandate::store::Store;
use serde_json::json;
use std::sync::{Arc, Mutex};
use time::OffsetDateTime;
use uuid::Uuid;

/// Records what it sends; fails the first `refuse` sends as a throttled SES would, and rejects
/// every send to an address in `reject` for good.
#[derive(Default)]
struct Outbox {
    sent: Mutex<Vec<Mail>>,
    refuse: Mutex<usize>,
    reject: Mutex<Vec<String>>,
}

#[async_trait]
impl Mailer for Outbox {
    async fn send(&self, mail: &Mail) -> Result<(), MailError> {
        if self.reject.lock().unwrap().contains(&mail.to) {
            return Err(MailError::Rejected("MessageRejected".into()));
        }
        let mut refuse = self.refuse.lock().unwrap();
        if *refuse > 0 {
            *refuse -= 1;
            return Err(MailError::Retryable("TooManyRequestsException".into()));
        }
        self.sent.lock().unwrap().push(mail.clone());
        Ok(())
    }
}

impl Outbox {
    fn to(&self) -> Vec<String> {
        let mut v: Vec<_> = self
            .sent
            .lock()
            .unwrap()
            .iter()
            .map(|m| m.to.clone())
            .collect();
        v.sort();
        v
    }
}

struct Acme;

#[async_trait]
impl OrgNames for Acme {
    async fn name(&self, _: &str) -> Result<String, MailError> {
        Ok("Acme & Co".into())
    }
}

/// A template with every placeholder the real one uses, in each part.
fn templates() -> Arc<Templates> {
    let dir = tempfile::tempdir().unwrap();
    let all = "{{email}}|{{requester}}|{{agent}}|{{organization}}|{{url}}|{{expires_in}}";
    std::fs::write(
        dir.path().join("manifest.json"),
        json!({"approval-requested": {
            "subject": "{{agent}} is waiting for your yes",
            "placeholders": ["email", "requester", "agent", "organization", "url", "expires_in"],
        }})
        .to_string(),
    )
    .unwrap();
    std::fs::write(
        dir.path().join("approval-requested.html"),
        format!("<p>{all}</p>"),
    )
    .unwrap();
    std::fs::write(dir.path().join("approval-requested.txt"), all).unwrap();
    Arc::new(Templates::load(dir.path(), TEMPLATES).unwrap())
}

/// The real built emails (`cd emails && npm run build`) fill with what the core provides. CI's
/// Rust job has no dist, so there the core's start-up check is the guard.
#[test]
fn the_built_templates_load_when_present() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../emails/dist");
    if !dir.join("manifest.json").exists() {
        eprintln!("emails/dist not built: skipped");
        return;
    }
    let t = Templates::load(&dir, TEMPLATES).unwrap();
    let values: Vec<(&str, &str)> = TEMPLATES[0].1.iter().map(|n| (*n, "x")).collect();
    let filled = t.get("approval-requested").unwrap().fill(&values).unwrap();
    assert!(!filled.html.contains("{{") && !filled.text.contains("{{"));
}

struct Rig {
    store: Store,
    mail: Arc<Outbox>,
    consumer: Consumer,
    signer: Signer,
    keys: Keyring,
}

fn rig(store: Store) -> Rig {
    let mail = Arc::new(Outbox::default());
    let consumer = consumer(
        "email",
        Email::new(
            store.clone(),
            templates(),
            mail.clone(),
            Arc::new(Acme),
            "https://app.example.com/",
        ),
    );
    let mut b = [0u8; 32];
    b[..16].copy_from_slice(Uuid::new_v4().as_bytes());
    b[16..].copy_from_slice(Uuid::new_v4().as_bytes());
    let signer = Signer::new(Source::core(), SigningKey::from_bytes(&b));
    let mut keys = Keyring::default();
    keys.add(signer.source().clone(), signer.verifying_key());
    Rig {
        store,
        mail,
        consumer,
        signer,
        keys,
    }
}

impl Rig {
    /// The signed delivery of `approval.requested.v1` for `approval`, as the bus would send it.
    fn delivery(&self, t: &TenantId, approval: Uuid, agent: &str) -> Vec<u8> {
        let e = Event::ApprovalRequested {
            approval,
            agent: AgentId::parse(agent).unwrap(),
        };
        let env = Envelope {
            id: Uuid::new_v4(),
            source: Source::core(),
            r#type: EventType::parse(e.event_type()).unwrap(),
            tenant: t.clone(),
            occurred_at: OffsetDateTime::now_utc(),
            data: e.data(),
        };
        serde_json::to_vec(&self.signer.sign(env).unwrap()).unwrap()
    }

    async fn deliver(&self, raw: &[u8]) -> Outcome {
        self.consumer
            .process(&self.store, &self.keys, raw, OffsetDateTime::now_utc())
            .await
    }
}

/// Ann owns `ann-bot`; Bob is a stranger to it; Ann, Cy and Dee are on team `eng` with `eng-bot`.
async fn org(s: &Store) -> TenantId {
    let t = tenant();
    admin_exec(
        "insert into tenants (id, workos_org_id) values ($1, $2)",
        &[t.as_str(), &format!("org_{}", t.as_str())],
    )
    .await;
    for (id, name) in [
        ("ann", "Ann Lee"),
        ("bob", "Bob Ray"),
        ("cy", "Cy Ng"),
        ("dee", "Dee Ko"),
    ] {
        add_member(s, &t, id, name, false).await;
    }
    personal_agent(s, &t, "ann-bot", "ann").await;
    personal_agent(s, &t, "bob-bot", "bob").await;
    team_agent(s, &t, "eng-bot", "eng", &["ann", "cy", "dee"]).await;
    t
}

#[tokio::test]
#[ignore]
async fn a_new_approval_publishes_approval_requested_once() {
    let s = app_store().await;
    let t = org(&s).await;
    let id = ask(&s, &t, "eng-bot", "ann", json!({ "n": 1 })).await;
    // The same pending ask again reuses the approval and publishes nothing new.
    assert_eq!(ask(&s, &t, "eng-bot", "ann", json!({ "n": 1 })).await, id);
    let requested: Vec<_> = outbox(&s, &t)
        .await
        .into_iter()
        .filter(|(k, _)| k == "approval.requested.v1")
        .collect();
    assert_eq!(
        requested,
        vec![(
            "approval.requested.v1".to_owned(),
            json!({ "approval": id.to_string(), "agent": "eng-bot" })
        )]
    );
}

#[tokio::test]
#[ignore]
async fn deciders_are_exactly_who_may_decide() {
    let s = app_store().await;
    let t = org(&s).await;
    let other = org(&s).await;
    for (agent, by) in [
        ("eng-bot", "ann"),
        ("eng-bot", "bob"),
        ("ann-bot", "ann"),
        ("bob-bot", "bob"),
    ] {
        let id = ask(&s, &t, agent, by, json!({ agent: by })).await;
        let now = OffsetDateTime::now_utc();
        let mut tx = s.begin(&t).await.unwrap();
        let got: Vec<String> = deciders(&mut tx, id, now)
            .await
            .unwrap()
            .into_iter()
            .map(|d| d.member.as_str().to_owned())
            .collect();
        tx.rollback().await.unwrap();
        // Each member's decision, tried and rolled back, is the rule deciders() must match.
        let mut may = Vec::new();
        for m in ["ann", "bob", "cy", "dee"] {
            let mut tx = s.begin(&t).await.unwrap();
            if decide_approval(&mut tx, id, &member(m), Decision::Approved, now)
                .await
                .is_ok()
            {
                may.push(m.to_owned());
            }
            tx.rollback().await.unwrap();
        }
        assert_eq!(got, may, "{agent} asked by {by}");
        // Another organization's members with the same ids never see it.
        let mut tx = s.begin(&other).await.unwrap();
        assert!(deciders(&mut tx, id, now).await.unwrap().is_empty());
    }
}

#[tokio::test]
#[ignore]
async fn emails_each_decider_once_with_escaped_names_and_ids_only_tags() {
    let s = app_store().await;
    let t = org(&s).await;
    exec(
        &s,
        &t,
        "update members set name = $2 where tenant_id = $1 and id = 'ann'",
        &["<b>Ann</b>"],
    )
    .await;
    let r = rig(s.clone());
    let id = ask(&s, &t, "eng-bot", "ann", json!({ "n": 1 })).await;
    let raw = r.delivery(&t, id, "eng-bot");
    assert_eq!(r.deliver(&raw).await, Outcome::Handled);
    assert_eq!(r.mail.to(), ["cy@example.com", "dee@example.com"]);
    let sent = r.mail.sent.lock().unwrap().clone();
    let cy = sent.iter().find(|m| m.to == "cy@example.com").unwrap();
    assert_eq!(cy.subject, "Team bot is waiting for your yes");
    assert!(
        cy.html
            .contains("&lt;b&gt;Ann&lt;/b&gt;|Team bot|Acme &amp; Co")
    );
    assert!(cy.html.contains("|https://app.example.com/approvals|"));
    assert!(
        cy.text
            .starts_with("cy@example.com|<b>Ann</b>|Team bot|Acme & Co|")
    );
    let tags: Vec<_> = cy.tags.iter().map(|(k, _)| *k).collect();
    assert_eq!(tags, ["tenant", "member", "event", "template"]);
    assert!(cy.tags.iter().all(|(_, v)| !v.contains('@')));
    // Each send is recorded, and email.sent.v1 names the member and the event that caused it.
    let sent_events: Vec<_> = outbox(&s, &t)
        .await
        .into_iter()
        .filter(|(k, _)| k == "email.sent.v1")
        .map(|(_, d)| d["member"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(sent_events, ["cy", "dee"]);
}

#[tokio::test]
#[ignore]
async fn a_redelivery_or_a_lost_commit_never_sends_twice() {
    let s = app_store().await;
    let t = org(&s).await;
    let r = rig(s.clone());
    let id = ask(&s, &t, "ann-bot", "bob", json!({ "n": 1 })).await;
    let raw = r.delivery(&t, id, "ann-bot");
    assert_eq!(r.deliver(&raw).await, Outcome::Handled);
    assert_eq!(r.deliver(&raw).await, Outcome::Skipped);
    // As if the core died after the send but before the inbox committed: the claim still holds.
    admin_exec("delete from events_inbox where consumer = 'email'", &[]).await;
    assert_eq!(r.deliver(&raw).await, Outcome::Handled);
    assert_eq!(r.mail.to(), ["ann@example.com"]);
}

#[tokio::test]
#[ignore]
async fn a_refused_send_releases_its_claim_and_the_retry_sends() {
    let s = app_store().await;
    let t = org(&s).await;
    let r = rig(s.clone());
    *r.mail.refuse.lock().unwrap() = 1;
    let id = ask(&s, &t, "ann-bot", "bob", json!({ "n": 1 })).await;
    let raw = r.delivery(&t, id, "ann-bot");
    assert_eq!(r.deliver(&raw).await, Outcome::Failed);
    assert!(r.mail.to().is_empty());
    assert_eq!(r.deliver(&raw).await, Outcome::Handled);
    assert_eq!(r.mail.to(), ["ann@example.com"]);
}

#[tokio::test]
#[ignore]
async fn a_rejected_address_doesnt_stop_the_others_or_retry() {
    let s = app_store().await;
    let t = org(&s).await;
    let r = rig(s.clone());
    // Bob asks the team bot, so Ann, Cy and Dee may all decide; Cy (in the middle) is rejected.
    r.mail.reject.lock().unwrap().push("cy@example.com".into());
    let id = ask(&s, &t, "eng-bot", "bob", json!({ "n": 1 })).await;
    let raw = r.delivery(&t, id, "eng-bot");
    assert_eq!(r.deliver(&raw).await, Outcome::Handled);
    assert_eq!(r.mail.to(), ["ann@example.com", "dee@example.com"]);
    // Handled, so not retried; and the rejected claim stays, so no later delivery tries Cy again.
    assert_eq!(r.deliver(&raw).await, Outcome::Skipped);
    admin_exec("delete from events_inbox where consumer = 'email'", &[]).await;
    r.mail.reject.lock().unwrap().clear();
    assert_eq!(r.deliver(&raw).await, Outcome::Handled);
    assert_eq!(r.mail.to(), ["ann@example.com", "dee@example.com"]);
}

#[tokio::test]
#[ignore]
async fn nothing_goes_to_a_suppressed_address_a_disabled_agent_or_a_decided_approval() {
    let s = app_store().await;
    let t = org(&s).await;
    // A member of its own, so the global suppression can't touch other tests' addresses.
    let zed = format!("zed-{}", &Uuid::new_v4().simple().to_string()[..12]);
    add_member(&s, &t, &zed, "Zed", false).await;
    exec(
        &s,
        &t,
        "insert into team_members (tenant_id, team_id, member_id) values ($1, 'eng', $2)",
        &[&zed],
    )
    .await;
    let mut tx = s.begin_untenanted().await.unwrap();
    sqlx::query("insert into email_suppressions (address_hash, reason) values ($1, 'bounce')")
        .bind(address_hash(&format!("{zed}@EXAMPLE.com")))
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let r = rig(s.clone());
    let id = ask(&s, &t, "eng-bot", "ann", json!({ "n": 1 })).await;
    assert_eq!(
        r.deliver(&r.delivery(&t, id, "eng-bot")).await,
        Outcome::Handled
    );
    assert_eq!(r.mail.to(), ["cy@example.com", "dee@example.com"]);

    let r = rig(s.clone());
    let decided = ask(&s, &t, "ann-bot", "bob", json!({ "n": 2 })).await;
    let mut tx = s.begin(&t).await.unwrap();
    decide_approval(
        &mut tx,
        decided,
        &member("ann"),
        Decision::Denied,
        OffsetDateTime::now_utc(),
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(
        r.deliver(&r.delivery(&t, decided, "ann-bot")).await,
        Outcome::Handled
    );
    let off = ask(&s, &t, "bob-bot", "ann", json!({ "n": 3 })).await;
    admin_exec(
        "update agents set disabled = true where tenant_id = $1 and id = 'bob-bot'",
        &[t.as_str()],
    )
    .await;
    assert_eq!(
        r.deliver(&r.delivery(&t, off, "bob-bot")).await,
        Outcome::Handled
    );
    assert!(r.mail.to().is_empty());
}

#[tokio::test]
#[ignore]
async fn a_bounce_suppresses_the_address_and_tells_the_member() {
    let s = app_store().await;
    let t = org(&s).await;
    let address = format!("{}@example.com", Uuid::new_v4().simple());
    feedback::apply(
        &s,
        Feedback::Suppress {
            kind: Kind::Bounce,
            addresses: vec![address.clone()],
            tagged: Some((
                t.clone(),
                MemberId::parse("cy").unwrap(),
                EmailTemplate::ApprovalRequested,
            )),
        },
    )
    .await
    .unwrap();
    let mut tx = s.begin_untenanted().await.unwrap();
    let reason: String =
        sqlx::query_scalar("select reason from email_suppressions where address_hash = $1")
            .bind(address_hash(&address.to_uppercase()))
            .fetch_one(&mut *tx)
            .await
            .unwrap();
    assert_eq!(reason, "bounce");
    let bounced: Vec<_> = outbox(&s, &t)
        .await
        .into_iter()
        .filter(|(k, _)| k == "email.bounced.v1")
        .collect();
    assert_eq!(
        bounced,
        vec![(
            "email.bounced.v1".to_owned(),
            json!({ "member": "cy", "template": "approval_requested" })
        )]
    );
    // A complaint SES returned without our tags still suppresses, and tells no one.
    let other = format!("{}@example.com", Uuid::new_v4().simple());
    feedback::apply(
        &s,
        Feedback::Suppress {
            kind: Kind::Complaint,
            addresses: vec![other.clone()],
            tagged: None,
        },
    )
    .await
    .unwrap();
    let reason: String =
        sqlx::query_scalar("select reason from email_suppressions where address_hash = $1")
            .bind(address_hash(&other))
            .fetch_one(&mut *tx)
            .await
            .unwrap();
    assert_eq!(reason, "complaint");
}
