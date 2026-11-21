//! Live tests: need DATABASE_URL (a superuser), run with `--ignored`.
mod common;
use async_trait::async_trait;
use common::app_store;
use ed25519_dalek::SigningKey;
use mandate::events::consume::Outcome;
use mandate::events::publisher::{InProcessSink, Sink, SinkError, publish_pending};
use mandate::events::sign::{Keyring, SignedEnvelope, Signer};
use mandate::events::{
    Actor, Envelope, Event, EventType, Handler, HandlerError, Source, consumer, publish,
};
use mandate::ids::{AgentId, MemberId, TenantId};
use mandate::store::{Store, Tx};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use time::OffsetDateTime;
use uuid::Uuid;

/// Publisher rounds take every tenant's unsent rows; run them one test at a time.
static PUBLISHER: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn tenant() -> TenantId {
    TenantId::parse(&format!("t-{}", Uuid::new_v4().simple())).unwrap()
}

fn member(s: &str) -> MemberId {
    MemberId::parse(s).unwrap()
}

/// A fresh key per test; never a fixed one.
fn key() -> SigningKey {
    let mut b = [0u8; 32];
    b[..16].copy_from_slice(Uuid::new_v4().as_bytes());
    b[16..].copy_from_slice(Uuid::new_v4().as_bytes());
    SigningKey::from_bytes(&b)
}

fn keys_for(signer: &Signer) -> Keyring {
    let mut k = Keyring::default();
    k.add(signer.source().clone(), signer.verifying_key());
    k
}

fn unique(prefix: &str) -> &'static str {
    Box::leak(format!("{prefix}-{}", Uuid::new_v4().simple()).into_boxed_str())
}

async fn publish_one(store: &Store, t: &TenantId, e: Event) {
    let mut tx = store.begin(t).await.unwrap();
    publish(&mut tx, e).await.unwrap();
    tx.commit().await.unwrap();
}

async fn count(store: &Store, t: &TenantId, sql: &'static str) -> i64 {
    let mut tx = store.begin(t).await.unwrap();
    sqlx::query_scalar(sql).fetch_one(&mut *tx).await.unwrap()
}

fn signed(signer: &Signer, t: &TenantId, e: &Event) -> Vec<u8> {
    let env = Envelope {
        id: Uuid::new_v4(),
        source: Source::core(),
        r#type: EventType::parse(e.event_type()).unwrap(),
        tenant: t.clone(),
        occurred_at: OffsetDateTime::now_utc(),
        data: e.data(),
    };
    serde_json::to_vec(&signer.sign(env).unwrap()).unwrap()
}

fn started(n: u128) -> Event {
    Event::ConversationStarted {
        conversation: Uuid::from_u128(n),
    }
}

/// Counts calls, checks the transaction is the event's tenant, and fails while `fail` > 0.
#[derive(Clone, Default)]
struct Counting {
    calls: Arc<AtomicUsize>,
    fail: Arc<AtomicUsize>,
    seen: Arc<Mutex<Vec<Envelope>>>,
}

#[async_trait]
impl Handler for Counting {
    async fn handle(&self, tx: &mut Tx, env: &Envelope) -> Result<(), HandlerError> {
        let t: String = sqlx::query_scalar("select current_setting('mandate.tenant')")
            .fetch_one(&mut **tx)
            .await
            .map_err(mandate::store::StoreError::from)?;
        assert_eq!(t, env.tenant.as_str());
        if self.fail.load(Ordering::SeqCst) > 0 {
            self.fail.fetch_sub(1, Ordering::SeqCst);
            return Err(HandlerError::Retry("not yet".into()));
        }
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.seen.lock().unwrap().push(env.clone());
        Ok(())
    }
}

#[derive(Default)]
struct Recording(Mutex<Vec<SignedEnvelope>>);

#[async_trait]
impl Sink for Recording {
    async fn send(&self, batch: &[SignedEnvelope]) -> Result<(), SinkError> {
        self.0.lock().unwrap().extend_from_slice(batch);
        Ok(())
    }
}

struct Failing;

#[async_trait]
impl Sink for Failing {
    async fn send(&self, _: &[SignedEnvelope]) -> Result<(), SinkError> {
        Err(SinkError::Bus("down".into()))
    }
}

async fn drain(store: &Store, signer: &Signer, sink: &dyn Sink) {
    while publish_pending(store, signer, sink).await.unwrap() > 0 {}
}

#[tokio::test]
#[ignore]
async fn publish_writes_in_the_callers_transaction() {
    let store = app_store().await;
    let t = tenant();
    let mut tx = store.begin(&t).await.unwrap();
    publish(&mut tx, started(1)).await.unwrap();
    tx.rollback().await.unwrap();
    assert_eq!(count(&store, &t, "select count(*) from outbox").await, 0);

    publish_one(&store, &t, started(1)).await;
    let mut tx = store.begin(&t).await.unwrap();
    let (ty, data, tenant_id): (String, serde_json::Value, String) =
        sqlx::query_as("select type, data, tenant_id from outbox")
            .fetch_one(&mut *tx)
            .await
            .unwrap();
    assert_eq!(ty, "conversation.started.v1");
    assert_eq!(data, started(1).data());
    assert_eq!(tenant_id, t.as_str());
}

#[tokio::test]
#[ignore]
async fn publish_without_a_tenant_fails() {
    let store = app_store().await;
    let mut tx = store.begin_untenanted().await.unwrap();
    assert!(publish(&mut tx, started(1)).await.is_err());
}

#[tokio::test]
#[ignore]
async fn outbox_is_tenant_isolated() {
    let store = app_store().await;
    let (a, b) = (tenant(), tenant());
    publish_one(&store, &a, started(1)).await;
    assert_eq!(count(&store, &b, "select count(*) from outbox").await, 0);
    assert_eq!(count(&store, &a, "select count(*) from outbox").await, 1);
}

#[tokio::test]
#[ignore]
async fn lifecycle_events_land_in_lifecycle_log() {
    let store = app_store().await;
    let t = tenant();
    let mut tx = store.begin(&t).await.unwrap();
    for e in [
        Event::OrganizationCreated {
            actor: Actor::Member(member("m1")),
        },
        Event::MemberCreated {
            member: member("m1"),
            actor: Actor::Member(member("m1")),
        },
        Event::MemberRemoved {
            member: member("m2"),
            actor: Actor::Directory,
        },
        Event::ApprovalRequested {
            approval: Uuid::new_v4(),
            agent: AgentId::parse("a1").unwrap(),
        },
    ] {
        publish(&mut tx, e).await.unwrap();
    }
    tx.commit().await.unwrap();

    let mut tx = store.begin(&t).await.unwrap();
    let rows: Vec<(String, Option<String>, String, Option<String>)> = sqlx::query_as(
        "select l.type, l.member, l.actor_kind, l.actor_member from lifecycle_log l \
         join outbox o on o.tenant_id = l.tenant_id and o.id = l.event_id order by o.seq",
    )
    .fetch_all(&mut *tx)
    .await
    .unwrap();
    let s = |x: &str| Some(x.to_owned());
    assert_eq!(
        rows,
        vec![
            (
                "organization.created.v1".into(),
                None,
                "member".into(),
                s("m1")
            ),
            (
                "member.created.v1".into(),
                s("m1"),
                "member".into(),
                s("m1")
            ),
            (
                "member.removed.v1".into(),
                s("m2"),
                "directory".into(),
                None
            ),
        ]
    );
    assert_eq!(count(&store, &t, "select count(*) from outbox").await, 4);
}

#[tokio::test]
#[ignore]
async fn app_role_cannot_edit_lifecycle_log() {
    let store = app_store().await;
    let t = tenant();
    publish_one(
        &store,
        &t,
        Event::MemberCreated {
            member: member("m1"),
            actor: Actor::System,
        },
    )
    .await;
    for sql in [
        "update lifecycle_log set member = 'm9'",
        "delete from lifecycle_log",
        // The outbox is not editable either, beyond marking a row sent.
        "update outbox set data = '{}'::jsonb",
        "delete from outbox",
    ] {
        let mut tx = store.begin(&t).await.unwrap();
        let err = sqlx::query(sql).execute(&mut *tx).await.unwrap_err();
        let code = err.as_database_error().and_then(|e| e.code());
        assert_eq!(code.as_deref(), Some("42501"), "{sql}: {err}");
    }
}
