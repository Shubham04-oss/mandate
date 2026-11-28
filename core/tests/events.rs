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
        channel: None,
        handoff_from: None,
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

#[tokio::test]
#[ignore]
async fn publisher_sends_in_order_per_tenant() {
    let _one = PUBLISHER.lock().await;
    let store = app_store().await;
    let signer = Signer::new(Source::core(), key());
    let (a, b) = (tenant(), tenant());
    // Interleaved across tenants and transactions.
    for (t, n) in [(&a, 1), (&b, 101), (&a, 2), (&b, 102), (&a, 3)] {
        publish_one(&store, t, started(n)).await;
    }
    let sink = Recording::default();
    drain(&store, &signer, &sink).await;

    let keys = keys_for(&signer);
    let sent = sink.0.lock().unwrap().clone();
    for (t, want) in [(&a, vec![1u128, 2, 3]), (&b, vec![101, 102])] {
        let got: Vec<u128> = sent
            .iter()
            .filter(|s| &s.envelope.tenant == t)
            .map(|s| {
                // What went out verifies, and is exactly the stored event.
                let raw = serde_json::to_vec(s).unwrap();
                let env = keys.verify(&raw, OffsetDateTime::now_utc()).unwrap();
                match Event::from_envelope(&env).unwrap() {
                    Event::ConversationStarted { conversation, .. } => conversation.as_u128(),
                    other => panic!("{other:?}"),
                }
            })
            .collect();
        assert_eq!(got, want);
        assert_eq!(
            count(
                &store,
                t,
                "select count(*) from outbox where sent_at is null"
            )
            .await,
            0
        );
    }
    // Nothing left: a second round sends nothing again.
    let again = Recording::default();
    drain(&store, &signer, &again).await;
    assert!(
        again
            .0
            .lock()
            .unwrap()
            .iter()
            .all(|s| s.envelope.tenant != a && s.envelope.tenant != b)
    );
}

#[tokio::test]
#[ignore]
async fn failed_send_keeps_rows_unsent() {
    let _one = PUBLISHER.lock().await;
    let store = app_store().await;
    let signer = Signer::new(Source::core(), key());
    let t = tenant();
    publish_one(&store, &t, started(7)).await;
    assert!(publish_pending(&store, &signer, &Failing).await.is_err());
    assert_eq!(
        count(
            &store,
            &t,
            "select count(*) from outbox where sent_at is null"
        )
        .await,
        1
    );
    let sink = Recording::default();
    drain(&store, &signer, &sink).await;
    assert_eq!(
        sink.0
            .lock()
            .unwrap()
            .iter()
            .filter(|s| s.envelope.tenant == t)
            .count(),
        1
    );
}

#[tokio::test]
#[ignore]
async fn in_process_delivery_reaches_the_handler() {
    let _one = PUBLISHER.lock().await;
    let store = app_store().await;
    let signer = Signer::new(Source::core(), key());
    let t = tenant();
    let h = Counting::default();
    let sink = InProcessSink::new(
        store.clone(),
        keys_for(&signer),
        vec![Arc::new(consumer(unique("inproc"), h.clone()))],
    );
    publish_one(&store, &t, started(5)).await;
    drain(&store, &signer, &sink).await;
    let seen = h.seen.lock().unwrap().clone();
    let mine: Vec<_> = seen.iter().filter(|e| e.tenant == t).collect();
    assert_eq!(mine.len(), 1);
    assert_eq!(Event::from_envelope(mine[0]).unwrap(), started(5));
}

#[tokio::test]
#[ignore]
async fn replayed_event_is_skipped() {
    let store = app_store().await;
    let signer = Signer::new(Source::core(), key());
    let keys = keys_for(&signer);
    let t = tenant();
    let h = Counting::default();
    let c = consumer(unique("replay"), h.clone());
    let raw = signed(&signer, &t, &started(1));
    let now = OffsetDateTime::now_utc();
    assert_eq!(c.process(&store, &keys, &raw, now).await, Outcome::Handled);
    assert_eq!(c.process(&store, &keys, &raw, now).await, Outcome::Skipped);
    // The same event wrapped as an EventBridge delivery is still the same event.
    let wrapped = serde_json::to_vec(&serde_json::json!({
        "detail-type": "conversation.started.v1",
        "detail": serde_json::from_slice::<serde_json::Value>(&raw).unwrap(),
    }))
    .unwrap();
    assert_eq!(
        c.process(&store, &keys, &wrapped, now).await,
        Outcome::Skipped
    );
    // Concurrent duplicate deliveries: exactly one handles it.
    let raw2 = signed(&signer, &t, &started(2));
    let (x, y) = tokio::join!(
        c.process(&store, &keys, &raw2, now),
        c.process(&store, &keys, &raw2, now)
    );
    let mut got = [x, y];
    got.sort_by_key(|o| *o == Outcome::Skipped);
    assert_eq!(got, [Outcome::Handled, Outcome::Skipped]);
    assert_eq!(h.calls.load(Ordering::SeqCst), 2);
    // Another consumer keeps its own inbox.
    let other = consumer(unique("replay-other"), h.clone());
    assert_eq!(
        other.process(&store, &keys, &raw, now).await,
        Outcome::Handled
    );
}

async fn quarantined(name: &str) -> Vec<String> {
    // The app role may only insert there; read as the superuser.
    sqlx::query_scalar("select reason from events_quarantine where consumer = $1 order by id")
        .bind(name)
        .fetch_all(&common::admin_pool().await)
        .await
        .unwrap()
}

#[tokio::test]
#[ignore]
async fn bad_signature_is_quarantined() {
    let store = app_store().await;
    let signer = Signer::new(Source::core(), key());
    let keys = keys_for(&signer);
    let t = tenant();
    let h = Counting::default();
    let name = unique("forged");
    let c = consumer(name, h.clone());
    let now = OffsetDateTime::now_utc();

    let good: serde_json::Value =
        serde_json::from_slice(&signed(&signer, &t, &started(1))).unwrap();
    let mut tampered = good.clone();
    tampered["tenant"] = tenant().as_str().into();
    let mut unsigned = good.clone();
    unsigned.as_object_mut().unwrap().remove("signature");
    let stranger = Signer::new(Source::core(), key());
    let foreign: serde_json::Value =
        serde_json::from_slice(&signed(&stranger, &t, &started(1))).unwrap();
    for bad in [tampered, unsigned, foreign] {
        let raw = serde_json::to_vec(&bad).unwrap();
        assert_eq!(
            c.process(&store, &keys, &raw, now).await,
            Outcome::Quarantined
        );
    }
    assert_eq!(h.calls.load(Ordering::SeqCst), 0);
    assert_eq!(
        quarantined(name).await,
        vec!["bad signature", "unsigned", "unknown key"]
    );
    // A forged copy marked nothing: the genuine event is still handled.
    let raw = serde_json::to_vec(&good).unwrap();
    assert_eq!(c.process(&store, &keys, &raw, now).await, Outcome::Handled);
}

#[tokio::test]
#[ignore]
async fn undecodable_message_is_quarantined() {
    let store = app_store().await;
    let signer = Signer::new(Source::core(), key());
    let keys = keys_for(&signer);
    let h = Counting::default();
    let name = unique("garbage");
    let c = consumer(name, h.clone());
    let now = OffsetDateTime::now_utc();
    for raw in [&b"not json"[..], b"{\"detail\": 5}", b"[]"] {
        assert_eq!(
            c.process(&store, &keys, raw, now).await,
            Outcome::Quarantined
        );
    }
    assert_eq!(h.calls.load(Ordering::SeqCst), 0);
    assert_eq!(quarantined(name).await.len(), 3);
}

#[tokio::test]
#[ignore]
async fn handler_failure_retries() {
    let store = app_store().await;
    let signer = Signer::new(Source::core(), key());
    let keys = keys_for(&signer);
    let t = tenant();
    let h = Counting::default();
    h.fail.store(1, Ordering::SeqCst);
    let c = consumer(unique("retry"), h.clone());
    let raw = signed(&signer, &t, &started(1));
    let now = OffsetDateTime::now_utc();
    // The failed attempt rolled back its inbox row, so the redelivery is handled.
    assert_eq!(c.process(&store, &keys, &raw, now).await, Outcome::Failed);
    assert_eq!(c.process(&store, &keys, &raw, now).await, Outcome::Handled);
    assert_eq!(c.process(&store, &keys, &raw, now).await, Outcome::Skipped);
    assert_eq!(h.calls.load(Ordering::SeqCst), 1);
}

/// The dev bus and its test queue; `None` (skipped) without them or without AWS credentials.
async fn live_bus() -> Option<(aws_config::SdkConfig, String, String)> {
    let (Ok(bus), Ok(queue)) = (
        std::env::var("MANDATE_EVENT_BUS"),
        std::env::var("MANDATE_QUEUE_TEST"),
    ) else {
        println!("skipped: MANDATE_EVENT_BUS or MANDATE_QUEUE_TEST not set");
        return None;
    };
    let cfg = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
    if cfg.credentials_provider().is_none() {
        println!("skipped: no AWS credentials");
        return None;
    }
    Some((cfg, bus, queue))
}

#[tokio::test]
#[ignore]
async fn bus_round_trip() {
    let Some((cfg, bus, queue)) = live_bus().await else {
        return;
    };
    let store = app_store().await;
    let signer = Signer::new(Source::core(), key());
    let t = tenant();
    let env = Envelope {
        id: Uuid::new_v4(),
        source: Source::core(),
        r#type: EventType::parse("conversation.started.v1").unwrap(),
        tenant: t.clone(),
        occurred_at: OffsetDateTime::now_utc(),
        data: started(9).data(),
    };
    mandate::events::publisher::BusSink::new(&cfg, bus)
        .send(&[signer.sign(env.clone()).unwrap()])
        .await
        .unwrap();

    let sqs = aws_sdk_sqs::Client::new(&cfg);
    let h = Counting::default();
    let c = consumer("test", h.clone());
    let keys = keys_for(&signer);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(90);
    while !h.seen.lock().unwrap().iter().any(|e| e.id == env.id) {
        assert!(std::time::Instant::now() < deadline, "event did not arrive");
        let r = sqs
            .receive_message()
            .queue_url(&queue)
            .wait_time_seconds(10)
            .send()
            .await
            .unwrap();
        for m in r.messages.unwrap_or_default() {
            // Messages signed by an earlier run's key are quarantined, which is right.
            c.process(
                &store,
                &keys,
                m.body().unwrap_or_default().as_bytes(),
                OffsetDateTime::now_utc(),
            )
            .await;
            sqs.delete_message()
                .queue_url(&queue)
                .receipt_handle(m.receipt_handle().unwrap_or_default())
                .send()
                .await
                .unwrap();
        }
    }
    let seen = h.seen.lock().unwrap().clone();
    assert_eq!(seen.iter().find(|e| e.id == env.id), Some(&env));
}
