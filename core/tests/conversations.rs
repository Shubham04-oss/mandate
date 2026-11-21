//! The eve client and the conversations API against a fake eve server. Tests marked ignored need
//! DATABASE_URL (a superuser) and run with `--ignored`.
mod common;
use async_trait::async_trait;
use axum::Json;
use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::routing::{get, post};
use ed25519_dalek::SigningKey;
use http::{Request, StatusCode};
use http_body_util::BodyExt;
use mandate::api::conversations::{Conversations, Session, router};
use mandate::assertion::{self, Assertion};
use mandate::eve::{EveClient, EveSessionId, Record};
use mandate::guard::{Guard, GuardError, NOTICE, Verdict};
use mandate::ids::{MemberId, TenantId};
use mandate::mcp::AppState;
use mandate::policy::Policy;
use mandate::store::Store;
use mandate::tools::browser::{BrowserBackend, Page, ReadPage, SubmitForm};
use mandate::tools::{Catalog, ToolError};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tower::ServiceExt;
use url::Url;
use uuid::Uuid;

/// One request the fake eve received.
#[derive(Debug, Clone)]
struct Seen {
    path: String,
    bearer: Option<String>,
    body: Value,
}

/// A fake of eve's session API: records every request and serves a scripted NDJSON stream.
#[derive(Default)]
struct FakeEve {
    seen: Mutex<Vec<Seen>>,
    /// Stored stream lines, in order; a reader's `startIndex` counts these.
    lines: Mutex<Vec<String>>,
    /// Transport records interleaved before each read (heartbeats), not stored lines.
    heartbeat: bool,
}

impl FakeEve {
    fn push(&self, line: Value) {
        self.lines.lock().unwrap().push(line.to_string());
    }
    fn seen(&self) -> Vec<Seen> {
        self.seen.lock().unwrap().clone()
    }
    fn record(&self, path: String, headers: &HeaderMap, body: Value) {
        let bearer = headers
            .get("authorization")
            .and_then(|h| h.to_str().ok())
            .and_then(|h| h.strip_prefix("Bearer "))
            .map(str::to_owned);
        self.seen.lock().unwrap().push(Seen { path, bearer, body });
    }
}

async fn fake_start(
    State(f): State<Arc<FakeEve>>,
    headers: HeaderMap,
    body: Option<Json<Value>>,
) -> (axum::http::StatusCode, Json<Value>) {
    f.record(
        "/eve/v1/session".into(),
        &headers,
        body.map(|b| b.0).unwrap_or(Value::Null),
    );
    (
        axum::http::StatusCode::ACCEPTED,
        Json(json!({ "ok": true, "sessionId": "wrun_FAKE01", "deliveryId": "d1" })),
    )
}

async fn fake_post(
    State(f): State<Arc<FakeEve>>,
    Path(rest): Path<String>,
    headers: HeaderMap,
    body: Option<Json<Value>>,
) -> (axum::http::StatusCode, Json<Value>) {
    f.record(
        format!("/eve/v1/session/{rest}"),
        &headers,
        body.map(|b| b.0).unwrap_or(Value::Null),
    );
    (
        axum::http::StatusCode::ACCEPTED,
        Json(json!({ "ok": true, "sessionId": "wrun_FAKE01", "deliveryId": "d2" })),
    )
}

async fn fake_action(
    State(f): State<Arc<FakeEve>>,
    Path((id, action)): Path<(String, String)>,
    headers: HeaderMap,
    body: Option<Json<Value>>,
) -> (axum::http::StatusCode, Json<Value>) {
    fake_post(State(f), Path(format!("{id}/{action}")), headers, body).await
}

async fn fake_stream(
    State(f): State<Arc<FakeEve>>,
    Path((id, _)): Path<(String, String)>,
    Query(q): Query<HashMap<String, String>>,
    headers: HeaderMap,
) -> String {
    f.record(
        format!("/eve/v1/session/{id}/stream"),
        &headers,
        json!({ "startIndex": q.get("startIndex") }),
    );
    let start: usize = q
        .get("startIndex")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let lines = f.lines.lock().unwrap().clone();
    let mut out = String::new();
    for l in lines.iter().skip(start) {
        if f.heartbeat {
            out.push_str("{\"$eve\":\"heartbeat\"}\n");
        }
        out.push_str(l);
        out.push('\n');
    }
    out.push_str("{\"$eve\":\"stream.lease-ended\"}\n");
    out
}

/// Serves the fake on a random local port; returns its base URL.
async fn serve_fake(f: Arc<FakeEve>) -> Url {
    let app = axum::Router::new()
        .route("/eve/v1/session", post(fake_start))
        .route("/eve/v1/session/{id}", post(fake_post))
        .route(
            "/eve/v1/session/{id}/{action}",
            get(fake_stream).post(fake_action),
        )
        .with_state(f);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    Url::parse(&format!("http://{addr}")).unwrap()
}

fn fact_line(facts: Value) -> Value {
    json!({ "at": "2026-11-21T15:02:24.897Z", "facts": facts })
}

#[tokio::test]
async fn starts_session_with_assertion() {
    let fake = Arc::new(FakeEve::default());
    let eve = EveClient::new(serve_fake(fake.clone()).await);
    let id = eve.start("signed.assertion", "hello there").await.unwrap();
    assert_eq!(id.as_str(), "wrun_FAKE01");
    eve.send("signed.assertion", &id, "and again")
        .await
        .unwrap();
    eve.answer("signed.assertion", &id, "int_1", true)
        .await
        .unwrap();
    eve.cancel("signed.assertion", &id).await.unwrap();
    let seen = fake.seen();
    assert_eq!(seen.len(), 4);
    assert!(
        seen.iter()
            .all(|s| s.bearer.as_deref() == Some("signed.assertion"))
    );
    assert_eq!(seen[0].path, "/eve/v1/session");
    assert_eq!(seen[0].body, json!({ "message": "hello there" }));
    assert_eq!(seen[1].path, "/eve/v1/session/wrun_FAKE01");
    assert_eq!(seen[1].body, json!({ "message": "and again" }));
    assert_eq!(
        seen[2].body,
        json!({ "answers": [{ "interactionId": "int_1", "optionId": "approve" }] })
    );
    assert_eq!(seen[3].path, "/eve/v1/session/wrun_FAKE01/cancel");
}

#[tokio::test]
async fn stream_resumes_from_cursor() {
    let fake = Arc::new(FakeEve {
        heartbeat: true,
        ..Default::default()
    });
    for i in 0..3 {
        fake.push(fact_line(json!([{ "type": "x.test", "data": { "n": i } }])));
    }
    // A progress record is a stored line too: it moves the cursor but carries no facts.
    fake.push(json!({ "progress": { "type": "content.delta", "data": { "delta": "Hi" } } }));
    fake.push(fact_line(json!([
        { "type": "a.test", "data": { "n": 4 } },
        { "type": "b.test", "data": { "n": 5 } }
    ])));
    let eve = EveClient::new(serve_fake(fake.clone()).await);
    let id = EveSessionId::parse("wrun_FAKE01").unwrap();
    let mut s = eve.stream("tok", &id, 2).await.unwrap();
    let mut got = Vec::new();
    while let Some(r) = s.next().await.unwrap() {
        got.push(r);
    }
    let summary: Vec<(u64, Vec<String>)> = got
        .iter()
        .map(|r| match r {
            Record::Line { index, facts } => {
                (*index, facts.iter().map(|f| f.kind.clone()).collect())
            }
            Record::Ended => (u64::MAX, vec![]),
        })
        .collect();
    assert_eq!(
        summary,
        vec![
            (2, vec!["x.test".to_owned()]),
            (3, vec![]),
            (4, vec!["a.test".to_owned(), "b.test".to_owned()]),
        ]
    );
    assert_eq!(fake.seen()[0].body, json!({ "startIndex": "2" }));
}

#[test]
fn eve_session_ids_are_validated() {
    assert!(EveSessionId::parse("wrun_01M7ZA2F94KXCED6BCBEN5H57D").is_ok());
    for bad in ["", "a/b", "a b", "../x", &"x".repeat(129)] {
        assert!(EveSessionId::parse(bad).is_err(), "{bad:?}");
    }
}

// ---- conversations API (live: needs DATABASE_URL) ----

/// Never reached in these tests: tools run only when eve calls the MCP server.
struct NoBrowser;

#[async_trait]
impl BrowserBackend for NoBrowser {
    async fn read_page(&self, _: &Url, _: &[String]) -> Result<Page, ToolError> {
        Err(ToolError::Backend("unused".into()))
    }
    async fn fill_and_submit(
        &self,
        _: &Url,
        _: &BTreeMap<String, String>,
        _: &[String],
    ) -> Result<Page, ToolError> {
        Err(ToolError::Backend("unused".into()))
    }
}

fn key() -> SigningKey {
    SigningKey::from_bytes(&[9; 32])
}

fn tenant() -> TenantId {
    TenantId::parse(&format!("t-{}", Uuid::new_v4().simple())).unwrap()
}

/// Ann's personal operator `ann-op` in `t`, seeded as the superuser (the app role may not create agents).
async fn seed(t: &TenantId) {
    sqlx::query(
        "insert into agents (tenant_id, id, kind, owner_member) values ($1, 'ann-op', 'personal', 'ann')",
    )
    .bind(t.as_str())
    .execute(&common::admin_pool().await)
    .await
    .unwrap();
}

/// Blocks any text containing "SECRET"; fails every check when `broken`.
#[derive(Default)]
struct FakeGuard {
    broken: bool,
    seen: Mutex<Vec<String>>,
}

#[async_trait]
impl Guard for FakeGuard {
    async fn check(&self, text: &str) -> Result<Verdict, GuardError> {
        self.seen.lock().unwrap().push(text.to_owned());
        if self.broken {
            return Err(GuardError("guardrail unreachable".into()));
        }
        Ok(if text.contains("SECRET") {
            Verdict::Block
        } else {
            Verdict::Pass
        })
    }
}

struct Harness {
    store: Store,
    guard: Arc<FakeGuard>,
    conv: Arc<Conversations>,
    fake: Arc<FakeEve>,
}

async fn harness() -> Harness {
    harness_with(FakeGuard::default()).await
}

async fn harness_with(guard: FakeGuard) -> Harness {
    let guard = Arc::new(guard);
    let store = common::app_store().await;
    let fake = Arc::new(FakeEve::default());
    let backend: Arc<dyn BrowserBackend> = Arc::new(NoBrowser);
    let app = Arc::new(AppState {
        store: store.clone(),
        policy: Policy::load(std::path::Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../policies"
        )))
        .unwrap(),
        catalog: Catalog::new(vec![
            Arc::new(ReadPage::new(backend.clone(), vec!["docs.test".into()])),
            Arc::new(SubmitForm::new(backend)),
        ]),
        assertion_key: key().verifying_key(),
        assertion_signing_key: key(),
        workos: None,
    });
    let eve = EveClient::new(serve_fake(fake.clone()).await);
    let conv = Conversations::new(app, eve, guard.clone(), Duration::from_millis(20));
    Harness {
        store,
        guard,
        conv,
        fake,
    }
}

impl Harness {
    fn app(&self, t: &TenantId, member: &str) -> axum::Router {
        router(self.conv.clone()).layer(axum::Extension(Session {
            tenant: t.clone(),
            member: MemberId::parse(member).unwrap(),
            is_admin: false,
        }))
    }

    async fn state(&self, t: &TenantId, id: &str) -> String {
        let mut tx = self.store.begin(t).await.unwrap();
        sqlx::query_scalar("select state from conversations where id = $1::uuid")
            .bind(id)
            .fetch_one(&mut *tx)
            .await
            .unwrap()
    }

    async fn event_count(&self, t: &TenantId) -> i64 {
        let mut tx = self.store.begin(t).await.unwrap();
        sqlx::query_scalar("select count(*) from conversation_events")
            .fetch_one(&mut *tx)
            .await
            .unwrap()
    }

    /// Waits until the conversation is idle and its events stop changing.
    async fn settle(&self, t: &TenantId, id: &str) {
        let mut last = -1;
        for _ in 0..200 {
            let n = self.event_count(t).await;
            if self.state(t, id).await == "idle" && n == last {
                return;
            }
            last = n;
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        panic!("conversation never settled");
    }
}

async fn call(
    app: &axum::Router,
    method: &str,
    path: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut req = Request::builder().method(method).uri(path);
    if body.is_some() {
        req = req.header("content-type", "application/json");
    }
    let body = body
        .map(|b| Body::from(b.to_string()))
        .unwrap_or_else(Body::empty);
    let res = app.clone().oneshot(req.body(body).unwrap()).await.unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

/// One parsed server-sent event.
#[derive(Debug, Clone, PartialEq)]
struct Sse {
    id: u64,
    event: String,
    data: Value,
}

/// Reads `n` events from the stream (after `last`, if given), then drops the connection.
async fn read_sse(app: &axum::Router, id: &str, last: Option<u64>, n: usize) -> Vec<Sse> {
    let mut req = Request::get(format!("/v1/conversations/{id}/events"));
    if let Some(l) = last {
        req = req.header("last-event-id", l.to_string());
    }
    let res = app
        .clone()
        .oneshot(req.body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(res.headers()["content-type"], "text/event-stream");
    let mut body = res.into_body();
    let mut text = String::new();
    let mut out = Vec::new();
    while out.len() < n {
        let frame = tokio::time::timeout(Duration::from_secs(10), body.frame())
            .await
            .expect("an event in time")
            .unwrap()
            .unwrap();
        if let Ok(b) = frame.into_data() {
            text.push_str(std::str::from_utf8(&b).unwrap());
        }
        while let Some(end) = text.find("\n\n") {
            let block: String = text.drain(..end + 2).collect();
            let (mut eid, mut ev, mut data) = (None, String::new(), String::new());
            for l in block.lines() {
                if let Some(v) = l.strip_prefix("id: ") {
                    eid = v.parse().ok();
                } else if let Some(v) = l.strip_prefix("event: ") {
                    ev = v.into();
                } else if let Some(v) = l.strip_prefix("data: ") {
                    data.push_str(v);
                }
            }
            if let Some(eid) = eid {
                out.push(Sse {
                    id: eid,
                    event: ev,
                    data: serde_json::from_str(&data).unwrap(),
                });
            }
        }
    }
    out.truncate(n);
    out
}

fn turn(fake: &FakeEve, n: u32, user: &str, reply: &str) {
    let t = format!("turn_{n}");
    fake.push(fact_line(json!([
        { "type": "delivery.admitted", "data": { "deliveryId": format!("d{n}") } }
    ])));
    fake.push(fact_line(json!([
        { "type": "turn.started", "scope": { "turnId": t }, "data": { "turnId": t } },
        { "type": "delivery.consumed", "data": { "parts": [{ "kind": "text", "text": user }] } }
    ])));
    fake.push(json!({ "progress": { "type": "content.delta", "data": { "delta": "x" } } }));
    fake.push(fact_line(json!([
        { "type": "content.completed", "data": { "partId": "p", "kind": "text", "phase": "reply", "value": reply } }
    ])));
    fake.push(fact_line(json!([
        { "type": "turn.settled", "data": { "turnId": t, "outcome": "completed" } },
        { "type": "delivery.settled", "data": { "outcome": "handled" } }
    ])));
}

async fn start(h: &Harness, t: &TenantId, message: &str) -> String {
    let (status, body) = call(
        &h.app(t, "ann"),
        "POST",
        "/v1/conversations",
        Some(json!({ "message": message })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    body["id"].as_str().unwrap().to_owned()
}

fn assertion_of(seen: &Seen) -> Assertion {
    assertion::verify(seen.bearer.as_deref().unwrap(), &key().verifying_key(), 0).unwrap()
}

#[tokio::test]
#[ignore]
async fn start_conversation_returns_201_and_starts_eve() {
    let h = harness().await;
    let t = tenant();
    seed(&t).await;
    let (status, body) = call(
        &h.app(&t, "ann"),
        "POST",
        "/v1/conversations",
        Some(json!({ "message": "  hello there  " })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["title"], "hello there");
    assert_eq!(body["state"], "working");
    assert!(Uuid::parse_str(body["id"].as_str().unwrap()).is_ok());
    // eve's session id never reaches the browser.
    assert!(!body.to_string().contains("wrun_"));
    let seen = h.fake.seen();
    assert_eq!(seen[0].path, "/eve/v1/session");
    assert_eq!(seen[0].body, json!({ "message": "hello there" }));
    let a = assertion_of(&seen[0]);
    assert_eq!(
        (a.tenant, a.member.as_str(), a.agent.as_str()),
        (t.clone(), "ann", "ann-op")
    );

    let (status, list) = call(&h.app(&t, "ann"), "GET", "/v1/conversations", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(list["items"].as_array().unwrap().len(), 1);
    assert_eq!(list["next"], Value::Null);

    // No personal operator: no conversation, and eve is never called.
    let (status, body) = call(
        &h.app(&t, "bob"),
        "POST",
        "/v1/conversations",
        Some(json!({ "message": "hi" })),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"]["code"], "no_access");
    // An empty message is invalid.
    let (status, _) = call(
        &h.app(&t, "ann"),
        "POST",
        "/v1/conversations",
        Some(json!({ "message": " " })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        h.fake
            .seen()
            .iter()
            .filter(|s| s.path == "/eve/v1/session")
            .count(),
        1
    );
}

#[tokio::test]
#[ignore]
async fn events_stream_as_sse() {
    let h = harness().await;
    let t = tenant();
    seed(&t).await;
    h.fake.push(fact_line(
        json!([{ "type": "session.started", "data": {} }]),
    ));
    turn(&h.fake, 0, "hello", "Hi Ann");
    let id = start(&h, &t, "hello").await;
    let got = read_sse(&h.app(&t, "ann"), &id, None, 3).await;
    assert_eq!(
        got,
        vec![
            Sse {
                id: 1,
                event: "message".into(),
                data: json!({ "role": "user", "text": "hello" })
            },
            Sse {
                id: 2,
                event: "message".into(),
                data: json!({ "role": "assistant", "text": "Hi Ann" })
            },
            Sse {
                id: 3,
                event: "done".into(),
                data: json!({})
            },
        ]
    );
    h.settle(&t, &id).await;
    assert_eq!(h.state(&t, &id).await, "idle");
}

#[tokio::test]
#[ignore]
async fn reconnect_resumes_after_last_event_id() {
    let h = harness().await;
    let t = tenant();
    seed(&t).await;
    turn(&h.fake, 0, "first", "One");
    let id = start(&h, &t, "first").await;
    let app = h.app(&t, "ann");
    // The browser sees two events, then its connection drops mid-conversation.
    let before = read_sse(&app, &id, None, 2).await;
    assert_eq!(before.iter().map(|e| e.id).collect::<Vec<_>>(), vec![1, 2]);
    h.settle(&t, &id).await;
    // While it is away, the agent works on another turn.
    turn(&h.fake, 1, "second", "Two");
    let (status, _) = call(
        &app,
        "POST",
        &format!("/v1/conversations/{id}/messages"),
        Some(json!({ "message": "second" })),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED);
    h.settle(&t, &id).await;
    let total = h.event_count(&t).await as usize;
    assert_eq!(total, 6);
    let all = read_sse(&app, &id, None, total).await;
    let resumed = read_sse(&app, &id, Some(2), total - 2).await;
    // Exactly what came after event 2: nothing repeated, nothing lost.
    assert_eq!(resumed, all[2..].to_vec());
    assert_eq!(
        all.iter()
            .map(|e| (e.event.as_str(), e.data["text"].as_str().unwrap_or("")))
            .collect::<Vec<_>>(),
        vec![
            ("message", "first"),
            ("message", "One"),
            ("done", ""),
            ("message", "second"),
            ("message", "Two"),
            ("done", ""),
        ]
    );
    // A malformed Last-Event-ID is refused, not read as zero.
    let req = Request::get(format!("/v1/conversations/{id}/events"))
        .header("last-event-id", "abc")
        .body(Body::empty())
        .unwrap();
    assert_eq!(
        app.clone().oneshot(req).await.unwrap().status(),
        StatusCode::BAD_REQUEST
    );
}

#[tokio::test]
#[ignore]
async fn other_members_conversation_is_404() {
    let h = harness().await;
    let t = tenant();
    seed(&t).await;
    let id = start(&h, &t, "mine").await;
    let other_tenant = tenant();
    let commands = |h: &Harness| {
        h.fake
            .seen()
            .iter()
            .filter(|s| !s.path.ends_with("/stream"))
            .count()
    };
    let calls_before = commands(&h);
    for app in [h.app(&t, "bob"), h.app(&other_tenant, "ann")] {
        let (s1, b1) = call(
            &app,
            "POST",
            &format!("/v1/conversations/{id}/messages"),
            Some(json!({ "message": "x" })),
        )
        .await;
        let (s2, _) = call(
            &app,
            "POST",
            &format!("/v1/conversations/{id}/cancel"),
            None,
        )
        .await;
        let (s3, _) = call(&app, "GET", &format!("/v1/conversations/{id}/events"), None).await;
        assert_eq!(
            (s1, s2, s3),
            (
                StatusCode::NOT_FOUND,
                StatusCode::NOT_FOUND,
                StatusCode::NOT_FOUND
            )
        );
        assert_eq!(b1["error"]["code"], "not_found");
        let (_, list) = call(&app, "GET", "/v1/conversations", None).await;
        assert_eq!(list["items"], json!([]));
    }
    // Nothing reached eve for them.
    assert_eq!(commands(&h), calls_before);
    // Without a session at all: 401.
    let (status, _) = call(&router(h.conv.clone()), "GET", "/v1/conversations", None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
#[ignore]
async fn cancel_stops_the_turn() {
    let h = harness().await;
    let t = tenant();
    seed(&t).await;
    h.fake.push(fact_line(json!([
        { "type": "turn.started", "data": { "turnId": "turn_0" } }
    ])));
    let id = start(&h, &t, "long job").await;
    let app = h.app(&t, "ann");
    let (status, _) = call(
        &app,
        "POST",
        &format!("/v1/conversations/{id}/cancel"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED);
    let cancel = h
        .fake
        .seen()
        .into_iter()
        .find(|s| s.path.ends_with("/cancel"))
        .unwrap();
    assert_eq!(cancel.path, "/eve/v1/session/wrun_FAKE01/cancel");
    assert_eq!(assertion_of(&cancel).member.as_str(), "ann");
    h.fake.push(fact_line(json!([
        { "type": "turn.settled", "data": { "turnId": "turn_0", "outcome": "cancelled" } }
    ])));
    h.settle(&t, &id).await;
    let got = read_sse(&app, &id, None, 1).await;
    assert_eq!(got[0].event, "done");
}

/// The guard audit rows of tenant `t`: (decision, reason).
async fn guard_audit(h: &Harness, t: &TenantId) -> Vec<(String, Option<String>)> {
    let mut tx = h.store.begin(t).await.unwrap();
    sqlx::query_as("select decision, reason from audit where tool = 'guard.output' order by id")
        .fetch_all(&mut *tx)
        .await
        .unwrap()
}

async fn stored_text(h: &Harness, t: &TenantId) -> String {
    let mut tx = h.store.begin(t).await.unwrap();
    let rows: Vec<String> =
        sqlx::query_scalar("select data::text from conversation_events order by seq")
            .fetch_all(&mut *tx)
            .await
            .unwrap();
    rows.join("\n")
}

#[tokio::test]
#[ignore]
async fn blocked_message_is_replaced_and_audited() {
    let h = harness().await;
    let t = tenant();
    seed(&t).await;
    turn(&h.fake, 0, "show me the key", "Sure: SECRET-abc123");
    let id = start(&h, &t, "show me the key").await;
    h.settle(&t, &id).await;
    let got = read_sse(&h.app(&t, "ann"), &id, None, 3).await;
    assert_eq!(got[1].data, json!({ "role": "assistant", "text": NOTICE }));
    // The blocked text is not stored, so no reconnect can ever replay it.
    assert!(!stored_text(&h, &t).await.contains("SECRET"));
    assert_eq!(
        guard_audit(&h, &t).await,
        vec![("refuse".into(), Some("guard_blocked".into()))]
    );
    // Only the agent's words are guarded, each exactly once.
    assert_eq!(
        *h.guard.seen.lock().unwrap(),
        vec!["Sure: SECRET-abc123".to_owned()]
    );
}

#[tokio::test]
#[ignore]
async fn guard_error_blocks() {
    let h = harness_with(FakeGuard {
        broken: true,
        ..Default::default()
    })
    .await;
    let t = tenant();
    seed(&t).await;
    turn(&h.fake, 0, "hello", "A harmless answer");
    let id = start(&h, &t, "hello").await;
    h.settle(&t, &id).await;
    let got = read_sse(&h.app(&t, "ann"), &id, None, 3).await;
    assert_eq!(got[1].data, json!({ "role": "assistant", "text": NOTICE }));
    assert!(!stored_text(&h, &t).await.contains("harmless"));
    assert_eq!(
        guard_audit(&h, &t).await,
        vec![("refuse".into(), Some("guard_error".into()))]
    );
}

/// The real Bedrock guardrail; skips without its configuration or AWS credentials.
#[tokio::test]
#[ignore]
async fn guardrail_blocks_a_credential() {
    let (Ok(id), Ok(version)) = (
        std::env::var("MANDATE_GUARDRAIL_ID"),
        std::env::var("MANDATE_GUARDRAIL_VERSION"),
    ) else {
        println!("skipped: MANDATE_GUARDRAIL_ID or MANDATE_GUARDRAIL_VERSION not set");
        return;
    };
    let cfg = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
    if cfg.credentials_provider().is_none() {
        println!("skipped: no AWS credentials");
        return;
    }
    let guard = mandate::guard::BedrockGuard::new(&cfg, id, version);
    // Generated per run and shaped like an AWS access key, so nothing secret is in the repo.
    let fake_key = format!(
        "AKIA{}",
        Uuid::new_v4().simple().to_string()[..16].to_uppercase()
    );
    let text = format!("Here are the credentials: aws_access_key_id={fake_key}");
    assert_eq!(guard.check(&text).await.unwrap(), Verdict::Block);
    assert_eq!(
        guard.check("The meeting moved to Thursday.").await.unwrap(),
        Verdict::Pass
    );
}
