//! Live tests: need DATABASE_URL (a superuser), run with `--ignored`.
use mandate::ids::{AgentId, MemberId, TenantId, ToolId};
use mandate::policy::Parent;
use mandate::store::{self, AuditEntry, Store, Tx};
use sqlx::{Connection, PgConnection};
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

const APP_PASSWORD: &str = "mandate_app_test";

/// Migrates as the superuser, ensures the non-superuser `mandate_app` role, and returns a
/// store connected as that role, so RLS actually applies.
async fn app_store() -> Store {
    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL");
    Store::connect_for_migration(&url)
        .await
        .unwrap()
        .migrate()
        .await
        .unwrap();
    let mut c = PgConnection::connect(&url).await.unwrap();
    // Serialise role creation across parallel tests.
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

fn tenant() -> TenantId {
    TenantId::parse(&format!("t-{}", Uuid::new_v4().simple())).unwrap()
}
fn member(s: &str) -> MemberId {
    MemberId::parse(s).unwrap()
}
fn agent(s: &str) -> AgentId {
    AgentId::parse(s).unwrap()
}
fn tool(s: &str) -> ToolId {
    ToolId::parse(s).unwrap()
}

async fn exec(tx: &mut Tx, t: &TenantId, sql: &'static str, binds: &[&str]) {
    let mut q = sqlx::query(sql).bind(t.as_str());
    for b in binds {
        q = q.bind(*b);
    }
    q.execute(&mut **tx).await.unwrap();
}

async fn personal_agent(tx: &mut Tx, t: &TenantId, a: &str, owner: &str) {
    exec(
        tx,
        t,
        "insert into agents (tenant_id, id, kind, owner_member) values ($1, $2, 'personal', $3)",
        &[a, owner],
    )
    .await;
}

async fn grant(
    tx: &mut Tx,
    t: &TenantId,
    kind: &str,
    who: &str,
    tl: &str,
    standing: bool,
    expires: OffsetDateTime,
) {
    sqlx::query(
        "insert into grants (tenant_id, principal_kind, principal_id, tool, standing, expires_at, granted_by) \
         values ($1, $2, $3, $4, $5, $6, 'admin')",
    )
    .bind(t.as_str())
    .bind(kind)
    .bind(who)
    .bind(tl)
    .bind(standing)
    .bind(expires)
    .execute(&mut **tx)
    .await
    .unwrap();
}

#[tokio::test]
#[ignore]
async fn rows_of_another_tenant_are_invisible() {
    let s = app_store().await;
    let (a, b) = (tenant(), tenant());
    let mut tx = s.begin(&a).await.unwrap();
    personal_agent(&mut tx, &a, "bot", "ann").await;
    tx.commit().await.unwrap();

    let mut tx = s.begin(&b).await.unwrap();
    assert!(
        store::agent_parents(&mut tx, &agent("bot"))
            .await
            .unwrap()
            .is_none()
    );
    // Writing another tenant's row is refused by the policy's check.
    let r = sqlx::query("insert into members (tenant_id, id) values ($1, 'x')")
        .bind(a.as_str())
        .execute(&mut *tx)
        .await;
    assert!(r.is_err());
    drop(tx);

    let mut tx = s.begin(&a).await.unwrap();
    let p = store::agent_parents(&mut tx, &agent("bot"))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(p, vec![Parent::Member(member("ann"))]);
}

#[tokio::test]
#[ignore]
async fn expired_grant_grants_nothing() {
    let s = app_store().await;
    let t = tenant();
    let now = OffsetDateTime::now_utc();
    let mut tx = s.begin(&t).await.unwrap();
    personal_agent(&mut tx, &t, "bot", "ann").await;
    grant(&mut tx, &t, "agent", "bot", "mail.send", false, now).await;
    let g = store::live_grant(&mut tx, &agent("bot"), &[], &tool("mail.send"), now)
        .await
        .unwrap();
    assert!(!g.granted);
    let g = store::live_grant(
        &mut tx,
        &agent("bot"),
        &[],
        &tool("mail.send"),
        now - Duration::seconds(1),
    )
    .await
    .unwrap();
    assert!(g.granted && !g.standing);
}

#[tokio::test]
#[ignore]
async fn team_grant_reaches_the_team_bot() {
    let s = app_store().await;
    let t = tenant();
    let now = OffsetDateTime::now_utc();
    let mut tx = s.begin(&t).await.unwrap();
    exec(&mut tx, &t, "insert into agents (tenant_id, id, kind, owner_team) values ($1, 'teambot', 'team', 'ops')", &[]).await;
    grant(
        &mut tx,
        &t,
        "team",
        "ops",
        "db.query",
        true,
        now + Duration::hours(1),
    )
    .await;
    let parents = store::agent_parents(&mut tx, &agent("teambot"))
        .await
        .unwrap()
        .unwrap();
    let g = store::live_grant(&mut tx, &agent("teambot"), &parents, &tool("db.query"), now)
        .await
        .unwrap();
    assert!(g.granted && g.standing);
    let g = store::live_grant(&mut tx, &agent("teambot"), &parents, &tool("db.drop"), now)
        .await
        .unwrap();
    assert!(!g.granted);
}

async fn approved(s: &Store, t: &TenantId, now: OffsetDateTime, hash: &[u8; 32]) -> Uuid {
    let mut tx = s.begin(t).await.unwrap();
    personal_agent(&mut tx, t, "bot", "ann").await;
    let id = store::request_approval(
        &mut tx,
        &agent("bot"),
        &member("ann"),
        &tool("mail.send"),
        hash,
        now,
    )
    .await
    .unwrap();
    assert!(store::approve(&mut tx, id, &member("ann")).await.unwrap());
    tx.commit().await.unwrap();
    id
}

#[tokio::test]
#[ignore]
async fn approval_is_spent_once_for_its_exact_args() {
    let s = app_store().await;
    let t = tenant();
    let now = OffsetDateTime::now_utc();
    approved(&s, &t, now, &[1; 32]).await;
    let mut tx = s.begin(&t).await.unwrap();
    let (a, tl) = (agent("bot"), tool("mail.send"));
    assert!(
        !store::consume_approval(&mut tx, &a, &tl, &[2; 32], now)
            .await
            .unwrap()
    );
    assert!(
        !store::consume_approval(&mut tx, &a, &tl, &[1; 32], now + Duration::seconds(3600))
            .await
            .unwrap()
    );
    assert!(
        store::consume_approval(&mut tx, &a, &tl, &[1; 32], now)
            .await
            .unwrap()
    );
    assert!(
        !store::consume_approval(&mut tx, &a, &tl, &[1; 32], now)
            .await
            .unwrap()
    );
}

#[tokio::test]
#[ignore]
async fn concurrent_consumes_spend_one_approval() {
    let s = app_store().await;
    let t = tenant();
    let now = OffsetDateTime::now_utc();
    approved(&s, &t, now, &[7; 32]).await;
    let mut tasks = Vec::new();
    for _ in 0..8 {
        let (s, t) = (s.clone(), t.clone());
        tasks.push(tokio::spawn(async move {
            let mut tx = s.begin(&t).await.unwrap();
            let ok =
                store::consume_approval(&mut tx, &agent("bot"), &tool("mail.send"), &[7; 32], now)
                    .await
                    .unwrap();
            tx.commit().await.unwrap();
            ok
        }));
    }
    let mut wins = 0;
    for h in tasks {
        wins += usize::from(h.await.unwrap());
    }
    assert_eq!(wins, 1);
}

#[tokio::test]
#[ignore]
async fn only_the_owner_can_approve() {
    let s = app_store().await;
    let t = tenant();
    let now = OffsetDateTime::now_utc();
    let mut tx = s.begin(&t).await.unwrap();
    personal_agent(&mut tx, &t, "bot", "ann").await;
    exec(&mut tx, &t, "insert into agents (tenant_id, id, kind, owner_team) values ($1, 'teambot', 'team', 'ops')", &[]).await;
    exec(
        &mut tx,
        &t,
        "insert into team_members (tenant_id, team_id, member_id) values ($1, 'ops', 'bob')",
        &[],
    )
    .await;
    let h = [3; 32];
    let id = store::request_approval(
        &mut tx,
        &agent("bot"),
        &member("ann"),
        &tool("mail.send"),
        &h,
        now,
    )
    .await
    .unwrap();
    assert!(
        !store::approve(&mut tx, id, &member("mallory"))
            .await
            .unwrap()
    );
    assert!(!store::approve(&mut tx, id, &member("bob")).await.unwrap());
    assert!(store::approve(&mut tx, id, &member("ann")).await.unwrap());

    let tid = store::request_approval(
        &mut tx,
        &agent("teambot"),
        &member("ann"),
        &tool("db.drop"),
        &h,
        now,
    )
    .await
    .unwrap();
    assert!(
        !store::approve(&mut tx, tid, &member("mallory"))
            .await
            .unwrap()
    );
    assert!(store::approve(&mut tx, tid, &member("bob")).await.unwrap());

    // An unapproved or unknown approval is never consumable; expired ones cannot be approved.
    let old = store::request_approval(
        &mut tx,
        &agent("bot"),
        &member("ann"),
        &tool("x"),
        &h,
        now - Duration::hours(2),
    )
    .await
    .unwrap();
    assert!(!store::approve(&mut tx, old, &member("ann")).await.unwrap());
    assert!(
        !store::approve(&mut tx, Uuid::new_v4(), &member("ann"))
            .await
            .unwrap()
    );

    store::audit(
        &mut tx,
        &AuditEntry {
            agent: agent("bot"),
            member: member("ann"),
            tool: tool("mail.send"),
            args_hash: h,
            decision: "allow".into(),
            reason: None,
        },
    )
    .await
    .unwrap();
}

#[tokio::test]
#[ignore]
async fn superuser_role_is_refused() {
    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL");
    assert!(matches!(
        Store::connect(&url).await,
        Err(store::StoreError::UnsafeRole)
    ));
}
