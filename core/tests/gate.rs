//! Live tests: need DATABASE_URL (a superuser), run with `--ignored`.
use mandate::assertion::Assertion;
use mandate::gate::{Outcome, args_hash, authorize, list};
use mandate::ids::{AgentId, MemberId, TenantId, ToolId};
use mandate::policy::{Decision, Effect, Policy};
use mandate::store::{self, Store};
use serde_json::json;
use sqlx::{Connection, PgConnection};
use std::path::Path;
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

const APP_PASSWORD: &str = "mandate_app_test";

// Copied from tests/store.rs: migrate as superuser, connect as non-superuser `mandate_app`.
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

fn policy() -> Policy {
    Policy::load(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../policies"
    )))
    .unwrap()
}
fn tenant() -> TenantId {
    TenantId::parse(&format!("t-{}", Uuid::new_v4().simple())).unwrap()
}
fn tool(s: &str) -> ToolId {
    ToolId::parse(s).unwrap()
}
fn who(t: &TenantId) -> Assertion {
    Assertion {
        tenant: t.clone(),
        member: MemberId::parse("ann").unwrap(),
        agent: AgentId::parse("bot").unwrap(),
        space: None,
        expires_at: 0,
    }
}

/// Seeds agent `bot` owned by `ann`, with a live grant on each given tool.
async fn seed(s: &Store, t: &TenantId, grants: &[&str], standing: bool) {
    let mut tx = s.begin(t).await.unwrap();
    sqlx::query("insert into agents (tenant_id, id, kind, owner_member) values ($1, 'bot', 'personal', 'ann')")
        .bind(t.as_str())
        .execute(&mut *tx)
        .await
        .unwrap();
    for g in grants {
        sqlx::query(
            "insert into grants (tenant_id, principal_kind, principal_id, tool, standing, expires_at, granted_by) \
             values ($1, 'agent', 'bot', $2, $3, $4, 'admin')",
        )
        .bind(t.as_str())
        .bind(g)
        .bind(standing)
        .bind(OffsetDateTime::now_utc() + Duration::hours(1))
        .execute(&mut *tx)
        .await
        .unwrap();
    }
    tx.commit().await.unwrap();
}

async fn audit_rows(s: &Store, t: &TenantId) -> Vec<(String, Option<String>)> {
    let mut tx = s.begin(t).await.unwrap();
    sqlx::query_as("select decision, reason from audit order by at")
        .fetch_all(&mut *tx)
        .await
        .unwrap()
}

#[test]
fn args_hash_ignores_key_order_and_whitespace() {
    let a: serde_json::Value =
        serde_json::from_str(r#"{"a": 1, "b": {"x": [1, 2], "y": "z"}}"#).unwrap();
    let b: serde_json::Value = serde_json::from_str(r#"{"b":{"y":"z","x":[1,2]},"a":1}"#).unwrap();
    assert_eq!(args_hash(&a), args_hash(&b));
    assert_ne!(args_hash(&a), args_hash(&json!({"a": 2})));
    assert_ne!(args_hash(&json!([1, 2])), args_hash(&json!([2, 1])));
}

#[tokio::test]
#[ignore]
async fn read_under_grant_runs_and_is_audited() {
    let s = app_store().await;
    let t = tenant();
    seed(&s, &t, &["docs.read"], false).await;
    let now = OffsetDateTime::now_utc();
    let out = authorize(
        &s,
        &policy(),
        &who(&t),
        &tool("docs.read"),
        Effect::Read,
        &json!({}),
        now,
    )
    .await;
    assert_eq!(out, Outcome::Run);
    assert_eq!(audit_rows(&s, &t).await, vec![("run".to_owned(), None)]);

    // No grant: refused and audited.
    let out = authorize(
        &s,
        &policy(),
        &who(&t),
        &tool("docs.other"),
        Effect::Read,
        &json!({}),
        now,
    )
    .await;
    assert_eq!(out, Outcome::Refuse { reason: "no_grant" });
    let rows = audit_rows(&s, &t).await;
    assert_eq!(rows[1], ("refuse".to_owned(), Some("no_grant".to_owned())));

    // list: only what is not refused.
    let catalog = [
        (tool("docs.read"), Effect::Read),
        (tool("docs.other"), Effect::Read),
    ];
    let l = list(&s, &policy(), &who(&t), &catalog, now).await;
    assert_eq!(l, vec![(tool("docs.read"), Decision::Run)]);
}

#[tokio::test]
#[ignore]
async fn external_asks_then_runs_once_after_approval() {
    let s = app_store().await;
    let t = tenant();
    seed(&s, &t, &["mail.send"], true).await;
    let args = json!({"to": "a@b.c"});
    let now = OffsetDateTime::now_utc();
    let (p, w, tl) = (policy(), who(&t), tool("mail.send"));
    let call = || authorize(&s, &p, &w, &tl, Effect::External, &args, now);

    let Outcome::Ask { approval } = call().await else {
        panic!("expected ask")
    };
    let mut tx = s.begin(&t).await.unwrap();
    assert!(
        store::approve(&mut tx, approval, &MemberId::parse("ann").unwrap())
            .await
            .unwrap()
    );
    tx.commit().await.unwrap();

    assert_eq!(call().await, Outcome::Run);
    // Single use: the same call asks again.
    assert!(matches!(call().await, Outcome::Ask { .. }));
    let decisions: Vec<String> = audit_rows(&s, &t).await.into_iter().map(|r| r.0).collect();
    assert_eq!(decisions, ["ask", "run", "ask"]);

    // list reports Ask without consuming or creating approvals.
    let l = list(
        &s,
        &policy(),
        &who(&t),
        &[(tool("mail.send"), Effect::External)],
        now,
    )
    .await;
    assert_eq!(l, vec![(tool("mail.send"), Decision::Ask)]);
}

#[tokio::test]
#[ignore]
async fn cross_tenant_agent_is_refused() {
    let s = app_store().await;
    let (a, b) = (tenant(), tenant());
    seed(&s, &b, &["docs.read"], false).await;
    let now = OffsetDateTime::now_utc();
    let out = authorize(
        &s,
        &policy(),
        &who(&a),
        &tool("docs.read"),
        Effect::Read,
        &json!({}),
        now,
    )
    .await;
    assert_eq!(
        out,
        Outcome::Refuse {
            reason: "unknown_agent"
        }
    );
    assert_eq!(
        audit_rows(&s, &a).await,
        vec![("refuse".to_owned(), Some("unknown_agent".to_owned()))]
    );
    assert!(audit_rows(&s, &b).await.is_empty());
}

#[tokio::test]
#[ignore]
async fn database_down_refuses() {
    // Lazy pool on a closed port: the first query fails.
    let pool = sqlx::postgres::PgPoolOptions::new()
        .acquire_timeout(std::time::Duration::from_secs(1))
        .connect_lazy("postgres://x:y@127.0.0.1:1/none")
        .unwrap();
    let s = Store::from_pool(pool);
    let t = tenant();
    let out = authorize(
        &s,
        &policy(),
        &who(&t),
        &tool("docs.read"),
        Effect::Read,
        &json!({}),
        OffsetDateTime::now_utc(),
    )
    .await;
    assert_eq!(out, Outcome::Refuse { reason: "internal" });
}
