//! MCP server over the in-process router. Live tests need DATABASE_URL (a superuser), `--ignored`.
use async_trait::async_trait;
use axum::body::Body;
use ed25519_dalek::SigningKey;
use http::{Request, StatusCode};
use http_body_util::BodyExt;
use mandate::assertion::{self, Assertion};
use mandate::ids::{AgentId, MemberId, TenantId};
use mandate::mcp::{AppState, router};
use mandate::policy::Policy;
use mandate::store::{self, Store};
use mandate::tools::browser::{BrowserBackend, Page, ReadPage, SubmitForm};
use mandate::tools::{Catalog, ToolError};
use serde_json::{Value, json};
use sqlx::{Connection, PgConnection};
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use time::{Duration, OffsetDateTime};
use tower::ServiceExt;
use url::Url;
use uuid::Uuid;

const APP_PASSWORD: &str = "mandate_app_test";

/// Counts every backend call, so a test can prove a refused call never reached it.
#[derive(Default)]
struct Recorder(AtomicUsize);

#[async_trait]
impl BrowserBackend for Recorder {
    async fn read_page(&self, _: &Url, _: &[String]) -> Result<Page, ToolError> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Ok(Page {
            title: "read".into(),
            text: String::new(),
            live_view_url: None,
        })
    }
    async fn fill_and_submit(
        &self,
        _: &Url,
        _: &BTreeMap<String, String>,
        _: &[String],
    ) -> Result<Page, ToolError> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Ok(Page {
            title: "submitted".into(),
            text: String::new(),
            live_view_url: None,
        })
    }
}

/// Fails every call with a detail the agent must never see.
struct Failing;

#[async_trait]
impl BrowserBackend for Failing {
    async fn read_page(&self, _: &Url, _: &[String]) -> Result<Page, ToolError> {
        Err(ToolError::Backend(
            "SECRET https://x.test/?token=abc".into(),
        ))
    }
    async fn fill_and_submit(
        &self,
        _: &Url,
        _: &BTreeMap<String, String>,
        _: &[String],
    ) -> Result<Page, ToolError> {
        Err(ToolError::Backend(
            "SECRET https://x.test/?token=abc".into(),
        ))
    }
}

fn key() -> SigningKey {
    SigningKey::from_bytes(&[7; 32])
}

fn policy() -> Policy {
    Policy::load(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../policies"
    )))
    .unwrap()
}

fn token(t: &TenantId, expires_at: i64) -> String {
    let a = Assertion {
        tenant: t.clone(),
        member: MemberId::parse("ann").unwrap(),
        agent: AgentId::parse("bot").unwrap(),
        space: None,
        expires_at,
    };
    assertion::sign(&a, &key())
}

fn live_token(t: &TenantId) -> String {
    token(t, OffsetDateTime::now_utc().unix_timestamp() + 300)
}

fn tenant() -> TenantId {
    TenantId::parse(&format!("t-{}", Uuid::new_v4().simple())).unwrap()
}

fn app(store: Store, backend: Arc<dyn BrowserBackend>) -> axum::Router {
    let catalog = Catalog::new(vec![
        Arc::new(ReadPage::new(backend.clone(), vec!["docs.test".into()])),
        Arc::new(SubmitForm::new(backend)),
    ]);
    router(Arc::new(AppState {
        store,
        policy: policy(),
        catalog,
        assertion_key: key().verifying_key(),
    }))
}

/// One JSON-RPC request; returns the HTTP status and the parsed body (Null if not JSON).
async fn rpc(
    app: &axum::Router,
    bearer: Option<&str>,
    method: &str,
    params: Value,
) -> (StatusCode, Value) {
    let body = json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params });
    let mut req = Request::post("/mcp")
        .header("content-type", "application/json")
        .header("accept", "application/json, text/event-stream")
        .header("host", "localhost");
    if let Some(b) = bearer {
        req = req.header("authorization", format!("Bearer {b}"));
    }
    let res = app
        .clone()
        .oneshot(req.body(Body::from(body.to_string())).unwrap())
        .await
        .unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

async fn call(app: &axum::Router, t: &TenantId, name: &str, args: Value) -> Value {
    let (status, body) = rpc(
        app,
        Some(&live_token(t)),
        "tools/call",
        json!({ "name": name, "arguments": args }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body
}

// Copied from tests/gate.rs: migrate as superuser, connect as non-superuser `mandate_app`.
async fn app_store() -> Store {
    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL");
    Store::connect_for_migration(&url)
        .await
        .unwrap()
        .migrate()
        .await
        .unwrap();
    let mut c = PgConnection::connect(&url).await.unwrap();
    sqlx::query("select pg_advisory_lock(4242)")
        .execute(&mut c)
        .await
        .unwrap();
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "do $$ begin if not exists (select from pg_roles where rolname = 'mandate_app') then \
         create role mandate_app login password '{APP_PASSWORD}'; end if; end $$"
    )))
    .execute(&mut c)
    .await
    .unwrap();
    sqlx::query(
        "grant select, insert, update, delete on all tables in schema public to mandate_app",
    )
    .execute(&mut c)
    .await
    .unwrap();
    sqlx::query("select pg_advisory_unlock(4242)")
        .execute(&mut c)
        .await
        .unwrap();
    let host = url.split_once('@').unwrap().1;
    Store::connect(&format!("postgres://mandate_app:{APP_PASSWORD}@{host}"))
        .await
        .unwrap()
}

/// Seeds agent `bot` owned by `ann`, with a live non-standing grant on each given tool.
async fn seed(s: &Store, t: &TenantId, grants: &[&str]) {
    let mut tx = s.begin(t).await.unwrap();
    sqlx::query("insert into agents (tenant_id, id, kind, owner_member) values ($1, 'bot', 'personal', 'ann')")
        .bind(t.as_str())
        .execute(&mut *tx)
        .await
        .unwrap();
    for g in grants {
        sqlx::query(
            "insert into grants (tenant_id, principal_kind, principal_id, tool, standing, expires_at, granted_by) \
             values ($1, 'agent', 'bot', $2, false, $3, 'admin')",
        )
        .bind(t.as_str())
        .bind(g)
        .bind(OffsetDateTime::now_utc() + Duration::hours(1))
        .execute(&mut *tx)
        .await
        .unwrap();
    }
    tx.commit().await.unwrap();
}

async fn audit_count(s: &Store, t: &TenantId) -> i64 {
    let mut tx = s.begin(t).await.unwrap();
    sqlx::query_scalar("select count(*) from audit")
        .fetch_one(&mut *tx)
        .await
        .unwrap()
}

#[tokio::test]
async fn missing_or_bad_assertion_is_401() {
    // Lazy pool on a closed port: the bearer check must answer before any database work.
    let pool = sqlx::postgres::PgPoolOptions::new()
        .acquire_timeout(std::time::Duration::from_secs(1))
        .connect_lazy("postgres://x:y@127.0.0.1:1/none")
        .unwrap();
    let backend = Arc::new(Recorder::default());
    let app = app(Store::from_pool(pool), backend.clone());
    let t = tenant();
    let other_key = assertion::sign(
        &Assertion {
            tenant: t.clone(),
            member: MemberId::parse("ann").unwrap(),
            agent: AgentId::parse("bot").unwrap(),
            space: None,
            expires_at: i64::MAX,
        },
        &SigningKey::from_bytes(&[8; 32]),
    );
    let expired = token(&t, 1);
    for bad in [
        None,
        Some("garbage"),
        Some(expired.as_str()),
        Some(other_key.as_str()),
    ] {
        let (status, _) = rpc(&app, bad, "tools/list", json!({})).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{bad:?}");
    }
    // A valid assertion gets through to MCP (an empty list: the database is down, fail closed).
    let (status, body) = rpc(&app, Some(&live_token(&t)), "tools/list", json!({})).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["result"]["tools"], json!([]));
    assert_eq!(backend.0.load(Ordering::SeqCst), 0);
}

#[tokio::test]
#[ignore]
async fn list_shows_only_granted_tools() {
    let s = app_store().await;
    let t = tenant();
    seed(&s, &t, &["web.read_page"]).await;
    let backend = Arc::new(Recorder::default());
    let app = app(s, backend.clone());

    let (status, body) = rpc(&app, Some(&live_token(&t)), "tools/list", json!({})).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let tools = body["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 1, "{body}");
    assert_eq!(tools[0]["name"], "web.read_page");
    // Listed without args, so classified External: granted but not standing asks.
    assert!(
        tools[0]["description"]
            .as_str()
            .unwrap()
            .ends_with(" (asks for approval)"),
        "{body}"
    );
    assert_eq!(tools[0]["inputSchema"]["required"], json!(["url"]));

    // The gate uses this call's effect: a read-allowlisted URL is Read and runs.
    let out = call(
        &app,
        &t,
        "web.read_page",
        json!({ "url": "https://docs.test/a" }),
    )
    .await;
    assert_eq!(out["result"]["isError"], false, "{out}");
    assert_eq!(out["result"]["structuredContent"]["title"], "read");
    assert_eq!(backend.0.load(Ordering::SeqCst), 1);

    // Not granted: an MCP error carrying the reason, and the backend is untouched.
    let args = json!({ "url": "https://example.com", "fields": {} });
    let out = call(&app, &t, "web.submit_form", args).await;
    assert!(
        out["error"]["message"]
            .as_str()
            .unwrap()
            .contains("no_grant"),
        "{out}"
    );
    assert_eq!(backend.0.load(Ordering::SeqCst), 1);
}

#[tokio::test]
#[ignore]
async fn unknown_tool_and_huge_args_are_refused() {
    let s = app_store().await;
    let t = tenant();
    seed(&s, &t, &["web.read_page", "web.submit_form"]).await;
    let backend = Arc::new(Recorder::default());
    let app = app(s.clone(), backend.clone());

    for name in ["web.nope", "NOT A TOOL"] {
        let out = call(&app, &t, name, json!({})).await;
        assert!(out["error"].is_object(), "{out}");
    }
    let huge = "x".repeat(65 * 1024);
    let out = call(
        &app,
        &t,
        "web.read_page",
        json!({ "url": "https://docs.test/", "pad": huge }),
    )
    .await;
    assert!(out["error"].is_object(), "{out}");

    assert_eq!(backend.0.load(Ordering::SeqCst), 0);
    // Neither reached the gate: nothing was audited.
    assert_eq!(audit_count(&s, &t).await, 0);
}

#[tokio::test]
#[ignore]
async fn submit_form_returns_needs_approval_then_runs_after_approve() {
    let s = app_store().await;
    let t = tenant();
    seed(&s, &t, &["web.submit_form"]).await;
    let backend = Arc::new(Recorder::default());
    let app = app(s.clone(), backend.clone());
    let args = json!({ "url": "https://example.com/f", "fields": { "q": "x" } });

    let out = call(&app, &t, "web.submit_form", args.clone()).await;
    let result = &out["result"];
    assert_eq!(result["isError"], false, "{out}");
    assert_eq!(result["structuredContent"]["status"], "needs_approval");
    let approval: Uuid = result["structuredContent"]["approval"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    assert_eq!(backend.0.load(Ordering::SeqCst), 0);

    let mut tx = s.begin(&t).await.unwrap();
    assert!(
        store::approve(&mut tx, approval, &MemberId::parse("ann").unwrap())
            .await
            .unwrap()
    );
    tx.commit().await.unwrap();

    let out = call(&app, &t, "web.submit_form", args).await;
    assert_eq!(out["result"]["isError"], false, "{out}");
    assert_eq!(out["result"]["structuredContent"]["title"], "submitted");
    assert_eq!(backend.0.load(Ordering::SeqCst), 1);
}

#[tokio::test]
#[ignore]
async fn backend_error_detail_does_not_reach_the_agent() {
    let s = app_store().await;
    let t = tenant();
    seed(&s, &t, &["web.read_page"]).await;
    let app = app(s, Arc::new(Failing));

    let out = call(
        &app,
        &t,
        "web.read_page",
        json!({ "url": "https://docs.test/a" }),
    )
    .await;
    assert_eq!(out["result"]["isError"], true, "{out}");
    assert_eq!(
        out["result"]["content"][0]["text"],
        "the tool failed; see audit"
    );
    assert!(!out.to_string().contains("SECRET"), "{out}");
}
