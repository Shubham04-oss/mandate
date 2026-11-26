//! Live tests: need DATABASE_URL (a superuser), run with `--ignored`.
mod common;
use common::{admin_pool, app_store};
use mandate::assertion::Assertion;
use mandate::gate::{Outcome, args_hash, authorize, list};
use mandate::ids::{AgentId, MemberId, TenantId, ToolId};
use mandate::policy::{Decision, Effect, Policy};
use mandate::store::{self, Store};
use serde_json::json;
use std::path::Path;
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

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
async fn seed(t: &TenantId, grants: &[&str], standing: bool) {
    let mut tx = admin_pool().await.begin().await.unwrap();
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
    sqlx::query_as("select decision, reason from audit order by id")
        .fetch_all(&mut *tx)
        .await
        .unwrap()
}

/// (decision, approval_id, approved_by) of each audit row, oldest first.
async fn audit_approvals(s: &Store, t: &TenantId) -> Vec<(String, Option<Uuid>, Option<String>)> {
    let mut tx = s.begin(t).await.unwrap();
    sqlx::query_as("select decision, approval_id, approved_by from audit order by id")
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
    seed(&t, &["docs.read"], false).await;
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
    // Unattended: no approval spent, none recorded.
    assert_eq!(
        audit_approvals(&s, &t).await,
        vec![("run".to_owned(), None, None)]
    );

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
    seed(&t, &["mail.send"], true).await;
    let args = json!({"to": "a@b.c"});
    let now = OffsetDateTime::now_utc();
    let (p, w, tl) = (policy(), who(&t), tool("mail.send"));
    let call = || authorize(&s, &p, &w, &tl, Effect::External, &args, now);

    let Outcome::Ask { approval } = call().await else {
        panic!("expected ask")
    };
    let mut tx = s.begin(&t).await.unwrap();
    // The approver can be shown exactly what the call will do.
    let d = store::approval_details(&mut tx, approval)
        .await
        .unwrap()
        .unwrap();
    assert_eq!((d.tool, d.args), (tool("mail.send"), args.clone()));
    assert!(
        common::approve(&mut tx, approval, &MemberId::parse("ann").unwrap())
            .await
            .unwrap()
    );
    tx.commit().await.unwrap();

    assert_eq!(call().await, Outcome::Run);
    // Single use: the same call asks again.
    assert!(matches!(call().await, Outcome::Ask { .. }));
    let decisions: Vec<String> = audit_rows(&s, &t).await.into_iter().map(|r| r.0).collect();
    // The approval itself is audited between the ask and the run it allowed.
    assert_eq!(decisions, ["ask", "approved", "run", "ask"]);
    // The ask records the approval it opened, so the feed can show how it ended.
    assert_eq!(
        audit_approvals(&s, &t).await[0],
        ("ask".to_owned(), Some(approval), None)
    );
    // The run that spent the approval records which one and who gave it.
    assert_eq!(
        audit_approvals(&s, &t).await[2],
        ("run".to_owned(), Some(approval), Some("ann".to_owned()))
    );

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
    seed(&b, &["docs.read"], false).await;
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

fn as_member(t: &TenantId, m: &str) -> Assertion {
    Assertion {
        member: MemberId::parse(m).unwrap(),
        agent: AgentId::parse("tbot").unwrap(),
        ..who(t)
    }
}

/// Seeds team agent `tbot` owned by team `ops` (members ann and bob), with a standing grant on `tool`.
async fn seed_team(t: &TenantId, tool: &str) {
    let mut tx = admin_pool().await.begin().await.unwrap();
    for q in [
        "insert into agents (tenant_id, id, kind, owner_team) values ($1, 'tbot', 'team', 'ops')",
        "insert into team_members (tenant_id, team_id, member_id) values ($1, 'ops', 'ann'), ($1, 'ops', 'bob')",
    ] {
        sqlx::query(q)
            .bind(t.as_str())
            .execute(&mut *tx)
            .await
            .unwrap();
    }
    sqlx::query(
        "insert into grants (tenant_id, principal_kind, principal_id, tool, standing, expires_at, granted_by) \
         values ($1, 'agent', 'tbot', $2, true, $3, 'admin')",
    )
    .bind(t.as_str())
    .bind(tool)
    .bind(OffsetDateTime::now_utc() + Duration::hours(1))
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
}

async fn approve_as(s: &Store, t: &TenantId, approval: Uuid, m: &str) {
    let mut tx = s.begin(t).await.unwrap();
    assert!(
        common::approve(&mut tx, approval, &MemberId::parse(m).unwrap())
            .await
            .unwrap()
    );
    tx.commit().await.unwrap();
}

#[tokio::test]
#[ignore]
async fn removed_team_member_is_refused_on_the_next_call() {
    let s = app_store().await;
    let t = tenant();
    seed_team(&t, "docs.read").await;
    let (p, w, tl, now) = (
        policy(),
        as_member(&t, "ann"),
        tool("docs.read"),
        OffsetDateTime::now_utc(),
    );
    let args = json!({});
    let call = || authorize(&s, &p, &w, &tl, Effect::Read, &args, now);
    assert_eq!(call().await, Outcome::Run);

    let mut tx = s.begin(&t).await.unwrap();
    store::set_team_member(
        &mut tx,
        &mandate::ids::TeamId::parse("ops").unwrap(),
        &MemberId::parse("ann").unwrap(),
        false,
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();

    assert_eq!(
        call().await,
        Outcome::Refuse {
            reason: "not_member"
        }
    );
    assert_eq!(
        audit_rows(&s, &t).await[1],
        ("refuse".to_owned(), Some("not_member".to_owned()))
    );
    let l = list(&s, &p, &w, &[(tool("docs.read"), Effect::Read)], now).await;
    assert!(l.is_empty());
}

#[tokio::test]
#[ignore]
async fn approval_is_spent_only_by_its_requester() {
    let s = app_store().await;
    let t = tenant();
    seed_team(&t, "mail.send").await;
    let (p, tl, now) = (policy(), tool("mail.send"), OffsetDateTime::now_utc());
    let args = json!({"to": "a@b.c"});
    let (ann, bob) = (as_member(&t, "ann"), as_member(&t, "bob"));

    let Outcome::Ask { approval } =
        authorize(&s, &p, &ann, &tl, Effect::External, &args, now).await
    else {
        panic!("expected ask")
    };
    approve_as(&s, &t, approval, "bob").await;

    // Bob cannot spend Ann's approval, even with the same args on the same bot.
    let bob_out = authorize(&s, &p, &bob, &tl, Effect::External, &args, now).await;
    assert!(
        matches!(bob_out, Outcome::Ask { approval: a } if a != approval),
        "{bob_out:?}"
    );
    assert_eq!(
        authorize(&s, &p, &ann, &tl, Effect::External, &args, now).await,
        Outcome::Run
    );
}

#[tokio::test]
#[ignore]
async fn repeated_ask_reuses_the_pending_approval() {
    let s = app_store().await;
    let t = tenant();
    seed_team(&t, "mail.send").await;
    let (p, w, tl, now) = (
        policy(),
        as_member(&t, "ann"),
        tool("mail.send"),
        OffsetDateTime::now_utc(),
    );
    let args = json!({"to": "a@b.c"});
    let call = || authorize(&s, &p, &w, &tl, Effect::External, &args, now);

    let (Outcome::Ask { approval: a }, Outcome::Ask { approval: b }) = (call().await, call().await)
    else {
        panic!("expected two asks")
    };
    assert_eq!(a, b);
    let mut tx = s.begin(&t).await.unwrap();
    let n: i64 = sqlx::query_scalar("select count(*) from approvals")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert_eq!(n, 1);
    // Both asks are still audited.
    let decisions: Vec<String> = audit_rows(&s, &t).await.into_iter().map(|r| r.0).collect();
    assert_eq!(decisions, ["ask", "ask"]);
}

#[tokio::test]
#[ignore]
async fn approval_covers_only_its_exact_args() {
    let s = app_store().await;
    let t = tenant();
    seed_team(&t, "mail.send").await;
    let (p, w, tl, now) = (
        policy(),
        as_member(&t, "ann"),
        tool("mail.send"),
        OffsetDateTime::now_utc(),
    );
    let (a, b) = (json!({"to": "a@b.c"}), json!({"to": "evil@x.y"}));

    let Outcome::Ask { approval } = authorize(&s, &p, &w, &tl, Effect::External, &a, now).await
    else {
        panic!("expected ask")
    };
    approve_as(&s, &t, approval, "ann").await;

    assert!(matches!(
        authorize(&s, &p, &w, &tl, Effect::External, &b, now).await,
        Outcome::Ask { .. }
    ));
    assert_eq!(
        authorize(&s, &p, &w, &tl, Effect::External, &a, now).await,
        Outcome::Run
    );
}

#[tokio::test]
#[ignore]
async fn approval_dies_with_approver_membership() {
    let s = app_store().await;
    let t = tenant();
    seed_team(&t, "mail.send").await;
    let (p, w, tl, now) = (
        policy(),
        as_member(&t, "ann"),
        tool("mail.send"),
        OffsetDateTime::now_utc(),
    );
    let args = json!({"to": "a@b.c"});
    let ask = || async {
        match authorize(&s, &p, &w, &tl, Effect::External, &args, now).await {
            Outcome::Ask { approval } => approval,
            o => panic!("expected ask, got {o:?}"),
        }
    };
    let ops = mandate::ids::TeamId::parse("ops").unwrap();
    let bob = MemberId::parse("bob").unwrap();
    let set_bob = |present| {
        let (s, t, ops, bob) = (&s, &t, &ops, &bob);
        async move {
            let mut tx = s.begin(t).await.unwrap();
            store::set_team_member(&mut tx, ops, bob, present)
                .await
                .unwrap();
            tx.commit().await.unwrap();
        }
    };

    // Bob approves, then leaves the team: his approval no longer lets Ann run.
    approve_as(&s, &t, ask().await, "bob").await;
    set_bob(false).await;
    ask().await;

    // Even if his membership goes some other way, spending checks he is still allowed.
    set_bob(true).await;
    approve_as(&s, &t, ask().await, "bob").await;
    sqlx::query("delete from team_members where tenant_id = $1 and member_id = 'bob'")
        .bind(t.as_str())
        .execute(&admin_pool().await)
        .await
        .unwrap();
    ask().await;
}
