//! The eve client and the conversations API against a fake eve server. Tests marked ignored need
//! DATABASE_URL (a superuser) and run with `--ignored`.
mod common;
use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::routing::{get, post};
use mandate::eve::{EveClient, EveSessionId, Record};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use url::Url;

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
    let start: usize = q.get("startIndex").and_then(|s| s.parse().ok()).unwrap_or(0);
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
    eve.send("signed.assertion", &id, "and again").await.unwrap();
    eve.answer("signed.assertion", &id, "int_1", true)
        .await
        .unwrap();
    eve.cancel("signed.assertion", &id).await.unwrap();
    let seen = fake.seen();
    assert_eq!(seen.len(), 4);
    assert!(seen.iter().all(|s| s.bearer.as_deref() == Some("signed.assertion")));
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
