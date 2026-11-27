//! Shared by the `api_*` tests: the in-process web API behind its real layers, with a real
//! cookie session per call, seeding helpers and a JSON request helper.
// Each api_* binary uses a different subset of these helpers.
#![allow(dead_code)]
use async_trait::async_trait;
use axum::body::Body;
use http::StatusCode;
use http_body_util::BodyExt;
use mandate::api::auth::WorkosAuth;
use mandate::api::{self, ApiState, session};
use mandate::assertion::Assertion;
use mandate::ids::{AgentId, MemberId, TenantId, ToolId};
use mandate::policy::Effect;
use mandate::store::{self, Store};
use mandate::tools::{Catalog, Tool, ToolError};
use serde_json::Value;
use std::sync::Arc;
use time::OffsetDateTime;
use tower::ServiceExt;
use uuid::Uuid;

pub fn tenant() -> TenantId {
    TenantId::parse(&format!("t-{}", Uuid::new_v4().simple())).unwrap()
}

pub fn member(s: &str) -> MemberId {
    MemberId::parse(s).unwrap()
}

/// A tool whose effect is fixed; the API only reads ids and effects from the catalog.
struct Fake(ToolId, Effect);

#[async_trait]
impl Tool for Fake {
    fn id(&self) -> &ToolId {
        &self.0
    }
    fn description(&self) -> &'static str {
        "fake"
    }
    fn effect(&self, _: &Value) -> Effect {
        self.1
    }
    fn input_schema(&self) -> Value {
        serde_json::json!({ "type": "object" })
    }
    fn validate(&self, _: &Value) -> Result<(), ToolError> {
        Ok(())
    }
    async fn call(&self, _: &Assertion, _: Value) -> Result<Value, ToolError> {
        Err(ToolError::Backend("not callable in api tests".into()))
    }
}

pub const WEB: &str = "http://localhost:3000";

pub struct App {
    router: axum::Router,
    store: Store,
}

/// The web API router, exactly as served.
pub fn app(store: Store) -> App {
    let router = api::router(state(store.clone()));
    App { router, store }
}

/// The web API's state, with a catalog whose tools only report their effects.
pub fn state(store: Store) -> Arc<ApiState> {
    state_with(store, false)
}

/// `state`, with `debug_routes` as `MANDATE_DEBUG_ROUTES` would set it.
pub fn state_with(store: Store, debug_routes: bool) -> Arc<ApiState> {
    Arc::new(ApiState {
        store: store.clone(),
        web_origin: WEB.into(),
        // These routes never sign anyone in, so WorkOS is never called.
        auth: Arc::new(WorkosAuth::new(
            workos::Client::builder()
                .api_key("unused")
                .client_id("unused")
                .build(),
        )),
        sso_redirect_uri: "http://localhost:8080/v1/auth/sso/callback".into(),
        trust_cf_ip: false,
        debug_routes,
        catalog: Catalog::new(vec![
            Arc::new(Fake(ToolId::parse("web.read_page").unwrap(), Effect::Read)),
            Arc::new(Fake(
                ToolId::parse("web.submit_form").unwrap(),
                Effect::External,
            )),
        ]),
    })
}

/// A fresh session cookie value: signed in as `m` in `t`, as the browser would be.
pub async fn sign_in(s: &Store, t: &TenantId, m: &str) -> String {
    let mut tx = s.begin(t).await.unwrap();
    let token = session::create(
        &mut tx,
        t,
        &member(m),
        &Value::Null,
        OffsetDateTime::now_utc(),
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    token
}

/// One request as `who` (`None`: no session); the JSON body, or `Null` when there is none.
pub async fn call(
    app: &App,
    method: &str,
    uri: &str,
    who: Option<(&TenantId, &str)>,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut req = http::Request::builder()
        .method(method)
        .uri(uri)
        .header("x-mandate", "1")
        .header(http::header::ORIGIN, WEB);
    if let Some((t, m)) = who {
        // A fresh session per call.
        let token = sign_in(&app.store, t, m).await;
        req = req.header(http::header::COOKIE, format!("mandate_session={token}"));
    }
    let req = match body {
        Some(b) => req
            .header("content-type", "application/json")
            .body(Body::from(b.to_string())),
        None => req.body(Body::empty()),
    }
    .unwrap();
    let res = app.router.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

pub async fn exec(s: &Store, t: &TenantId, sql: &'static str, binds: &[&str]) {
    let mut tx = s.begin(t).await.unwrap();
    let mut q = sqlx::query(sql).bind(t.as_str());
    for b in binds {
        q = q.bind(*b);
    }
    q.execute(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();
}

/// As the superuser: for what the app role may not write (team bots, grant expiry).
pub async fn admin_exec(sql: &'static str, binds: &[&str]) {
    let pool = crate::common::admin_pool().await;
    let mut q = sqlx::query(sql);
    for b in binds {
        q = q.bind(*b);
    }
    q.execute(&pool).await.unwrap();
}

/// A member with a name and email; `admin` sets `members.is_admin`.
pub async fn add_member(s: &Store, t: &TenantId, id: &str, name: &str, admin: bool) {
    let mut tx = s.begin(t).await.unwrap();
    sqlx::query(
        "insert into members (tenant_id, id, name, email, is_admin) values ($1, $2, $3, $4, $5)",
    )
    .bind(t.as_str())
    .bind(id)
    .bind(name)
    .bind(format!("{id}@example.com"))
    .bind(admin)
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
}

pub async fn personal_agent(_: &Store, t: &TenantId, agent: &str, owner: &str) {
    admin_exec(
        "insert into agents (tenant_id, id, kind, owner_member, name) values ($1, $2, 'personal', $3, 'Personal agent')",
        &[t.as_str(), agent, owner],
    )
    .await;
}

pub async fn team_agent(s: &Store, t: &TenantId, agent: &str, team: &str, members: &[&str]) {
    exec(
        s,
        t,
        "insert into teams (tenant_id, id, name) values ($1, $2, $2) on conflict do nothing",
        &[team],
    )
    .await;
    admin_exec(
        "insert into agents (tenant_id, id, kind, owner_team, name) values ($1, $2, 'team', $3, 'Team bot')",
        &[t.as_str(), agent, team],
    )
    .await;
    for m in members {
        exec(
            s,
            t,
            "insert into team_members (tenant_id, team_id, member_id) values ($1, $2, $3)",
            &[team, m],
        )
        .await;
    }
}

/// A pending approval for `agent` asked by `by`, through the gate's own store function.
pub async fn ask(s: &Store, t: &TenantId, agent: &str, by: &str, args: Value) -> Uuid {
    let mut tx = s.begin(t).await.unwrap();
    let id = store::request_approval(
        &mut tx,
        &AgentId::parse(agent).unwrap(),
        &member(by),
        &ToolId::parse("web.submit_form").unwrap(),
        &args,
        &mandate::gate::args_hash(&args),
        OffsetDateTime::now_utc(),
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    id
}

/// `(type, data)` of the tenant's outbox, in publish order.
pub async fn outbox(s: &Store, t: &TenantId) -> Vec<(String, Value)> {
    let mut tx = s.begin(t).await.unwrap();
    let rows: Vec<(String, sqlx::types::Json<Value>)> =
        sqlx::query_as("select type, data from outbox order by seq")
            .fetch_all(&mut *tx)
            .await
            .unwrap();
    rows.into_iter().map(|(k, d)| (k, d.0)).collect()
}
