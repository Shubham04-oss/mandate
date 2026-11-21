//! Live tests: need DATABASE_URL (a superuser), run with `--ignored`.
mod common;
use common::{admin_pool, app_store};
use mandate::ids::{AgentId, MemberId, TenantId, ToolId};
use mandate::policy::Parent;
use mandate::store::{self, AuditEntry, Store, Tx};
use serde_json::json;
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

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

/// Runs as the superuser: the app role cannot create agents or grants.
async fn seed(t: &TenantId, sql: &'static str, binds: &[&str]) {
    let mut q = sqlx::query(sql).bind(t.as_str());
    for b in binds {
        q = q.bind(*b);
    }
    q.execute(&admin_pool().await).await.unwrap();
}

async fn personal_agent(t: &TenantId, a: &str, owner: &str) {
    seed(
        t,
        "insert into agents (tenant_id, id, kind, owner_member) values ($1, $2, 'personal', $3)",
        &[a, owner],
    )
    .await;
}

async fn grant(
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
    .execute(&admin_pool().await)
    .await
    .unwrap();
}

#[tokio::test]
#[ignore]
async fn rows_of_another_tenant_are_invisible() {
    let s = app_store().await;
    let (a, b) = (tenant(), tenant());
    personal_agent(&a, "bot", "ann").await;

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
    personal_agent(&t, "bot", "ann").await;
    grant(&t, "agent", "bot", "mail.send", false, now).await;
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
    seed(&t, "insert into agents (tenant_id, id, kind, owner_team) values ($1, 'teambot', 'team', 'ops')", &[]).await;
    grant(
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
    personal_agent(t, "bot", "ann").await;
    let id = store::request_approval(
        &mut tx,
        &agent("bot"),
        &member("ann"),
        &tool("mail.send"),
        &json!({"to": "a@b.c"}),
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
    let id = approved(&s, &t, now, &[1; 32]).await;
    let mut tx = s.begin(&t).await.unwrap();
    let (a, tl) = (agent("bot"), tool("mail.send"));
    assert!(
        store::consume_approval(&mut tx, &a, &member("ann"), &tl, &[2; 32], now)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        store::consume_approval(
            &mut tx,
            &a,
            &member("ann"),
            &tl,
            &[1; 32],
            now + Duration::seconds(3600)
        )
        .await
        .unwrap()
        .is_none()
    );
    // Spent: names the approval and who gave it, for the audit row.
    assert_eq!(
        store::consume_approval(&mut tx, &a, &member("ann"), &tl, &[1; 32], now)
            .await
            .unwrap(),
        Some((id, member("ann")))
    );
    assert!(
        store::consume_approval(&mut tx, &a, &member("ann"), &tl, &[1; 32], now)
            .await
            .unwrap()
            .is_none()
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
            let ok = store::consume_approval(
                &mut tx,
                &agent("bot"),
                &member("ann"),
                &tool("mail.send"),
                &[7; 32],
                now,
            )
            .await
            .unwrap();
            tx.commit().await.unwrap();
            ok
        }));
    }
    let mut wins = 0;
    for h in tasks {
        wins += usize::from(h.await.unwrap().is_some());
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
    personal_agent(&t, "bot", "ann").await;
    seed(&t, "insert into agents (tenant_id, id, kind, owner_team) values ($1, 'teambot', 'team', 'ops')", &[]).await;
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
        &json!({"to": "a@b.c"}),
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
        &json!({"to": "a@b.c"}),
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
        &json!({"to": "a@b.c"}),
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
            approval_id: None,
            approved_by: None,
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

#[tokio::test]
#[ignore]
async fn audit_is_append_only_for_the_app_role() {
    let s = app_store().await;
    let t = tenant();
    for q in ["update audit set reason = 'x'", "delete from audit"] {
        let mut tx = s.begin(&t).await.unwrap();
        let err = sqlx::query(q).execute(&mut *tx).await.unwrap_err();
        assert!(err.to_string().contains("permission denied"), "{q}: {err}");
    }
}

#[tokio::test]
#[ignore]
async fn approval_details_show_the_exact_args_to_the_tenant_only() {
    let s = app_store().await;
    let (t, other) = (tenant(), tenant());
    let now = OffsetDateTime::now_utc();
    let id = approved(&s, &t, now, &[4; 32]).await;
    let mut tx = s.begin(&t).await.unwrap();
    let d = store::approval_details(&mut tx, id).await.unwrap().unwrap();
    assert_eq!(d.agent, agent("bot"));
    assert_eq!(d.tool, tool("mail.send"));
    assert_eq!(d.args, json!({"to": "a@b.c"}));
    assert_eq!(d.requested_by, member("ann"));
    assert!(d.expires_at > now);
    let mut tx = s.begin(&other).await.unwrap();
    assert!(
        store::approval_details(&mut tx, id)
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
#[ignore]
async fn app_role_cannot_insert_agents_or_grants() {
    let s = app_store().await;
    let t = tenant();
    for q in [
        "insert into agents (tenant_id, id, kind, owner_member) values ($1, 'bot', 'personal', 'ann')",
        "insert into grants (tenant_id, principal_kind, principal_id, tool, standing, expires_at, granted_by) \
         values ($1, 'agent', 'bot', 'mail.send', true, now() + interval '1 hour', 'admin')",
    ] {
        let mut tx = s.begin(&t).await.unwrap();
        let err = sqlx::query(q)
            .bind(t.as_str())
            .execute(&mut *tx)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("permission denied"), "{q}: {err}");
    }
}

#[tokio::test]
#[ignore]
async fn leaving_a_team_drops_your_unspent_approvals() {
    let s = app_store().await;
    let t = tenant();
    let now = OffsetDateTime::now_utc();
    for (id, team) in [("opsbot", "ops"), ("devbot", "dev")] {
        seed(
            &t,
            "insert into agents (tenant_id, id, kind, owner_team) values ($1, $2, 'team', $3)",
            &[id, team],
        )
        .await;
    }
    let mut tx = s.begin(&t).await.unwrap();
    exec(
        &mut tx,
        &t,
        "insert into team_members (tenant_id, team_id, member_id) values ($1, 'ops', 'ann'), ($1, 'ops', 'bob'), ($1, 'dev', 'ann')",
        &[],
    )
    .await;
    // (agent, requested_by, approved_by, spent)
    let rows = [
        ("opsbot", "ann", None, false),        // she asked: dropped
        ("opsbot", "bob", Some("ann"), false), // she approved: dropped
        ("opsbot", "bob", None, false),        // not hers: stays
        ("opsbot", "ann", Some("bob"), true),  // spent, history: stays
        ("devbot", "ann", None, false),        // another team's agent: stays
    ];
    let mut ids = Vec::new();
    for (i, (a, req, appr, spent)) in rows.iter().enumerate() {
        let id = Uuid::new_v4();
        sqlx::query(
            "insert into approvals (tenant_id, id, agent, tool, args, args_hash, requested_by, approved_by, expires_at, used_at) \
             values ($1, $2, $3, 'mail.send', '{}', $4, $5, $6, $7, $8)",
        )
        .bind(t.as_str())
        .bind(id)
        .bind(*a)
        .bind(vec![i as u8; 32])
        .bind(*req)
        .bind(*appr)
        .bind(now + Duration::hours(1))
        .bind(spent.then_some(now))
        .execute(&mut *tx)
        .await
        .unwrap();
        ids.push(id);
    }
    store::set_team_member(&mut tx, &store_team("ops"), &member("ann"), false)
        .await
        .unwrap();
    let mut left: Vec<Uuid> = sqlx::query_scalar("select id from approvals")
        .fetch_all(&mut *tx)
        .await
        .unwrap();
    left.sort();
    let mut want = vec![ids[2], ids[3], ids[4]];
    want.sort();
    assert_eq!(left, want);
}

fn store_team(s: &str) -> mandate::ids::TeamId {
    mandate::ids::TeamId::parse(s).unwrap()
}
