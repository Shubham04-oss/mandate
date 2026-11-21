//! Shared by the `api_*` tests: seeding helpers, and (with the routes) an in-process app whose
//! session comes from a test header, as lane B's cookie session is stubbed.
// Each api_* binary uses a different subset of these helpers.
#![allow(dead_code)]
use mandate::ids::{AgentId, MemberId, TenantId, ToolId};
use mandate::store::{self, Store};
use serde_json::Value;
use time::OffsetDateTime;
use uuid::Uuid;

pub fn tenant() -> TenantId {
    TenantId::parse(&format!("t-{}", Uuid::new_v4().simple())).unwrap()
}

pub fn member(s: &str) -> MemberId {
    MemberId::parse(s).unwrap()
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
