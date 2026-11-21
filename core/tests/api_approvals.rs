//! `decide_approval` and the approvals routes. Live: need DATABASE_URL (a superuser), `--ignored`.
mod api_support;
mod common;
use api_support::*;
use common::app_store;
use mandate::api::approvals::{DecideError, Decision, State, decide_approval};
use mandate::api::stubs::{Event, published};
use mandate::store::Store;
use serde_json::json;
use sqlx::Row;
use time::OffsetDateTime;
use uuid::Uuid;

/// Ann owns `ann-bot`; Bob is a stranger to it; Ann and Cy are on team `eng` with `eng-bot`.
async fn org(s: &Store) -> mandate::ids::TenantId {
    let t = tenant();
    add_member(s, &t, "ann", "Ann Lee", false).await;
    add_member(s, &t, "bob", "Bob Ray", false).await;
    add_member(s, &t, "cy", "Cy Ng", false).await;
    personal_agent(s, &t, "ann-bot", "ann").await;
    personal_agent(s, &t, "bob-bot", "bob").await;
    team_agent(s, &t, "eng-bot", "eng", &["ann", "cy"]).await;
    t
}

async fn decide(
    s: &Store,
    t: &mandate::ids::TenantId,
    id: Uuid,
    by: &str,
    d: Decision,
) -> Result<mandate::api::approvals::Approval, DecideError> {
    let mut tx = s.begin(t).await.unwrap();
    let r = decide_approval(&mut tx, id, &member(by), d, OffsetDateTime::now_utc()).await;
    tx.commit().await.unwrap();
    r
}

#[tokio::test]
#[ignore]
async fn owner_can_approve() {
    let s = app_store().await;
    let t = org(&s).await;
    let id = ask(&s, &t, "ann-bot", "ann", json!({ "n": 1 })).await;
    let a = decide(&s, &t, id, "ann", Decision::Approve).await.unwrap();
    assert_eq!(a.state, State::Approved);
    assert_eq!(a.decided_by, Some(member("ann")));
    assert!(a.decided_at.is_some());
    // Any member of the owning team may decide a team bot's request.
    let id = ask(&s, &t, "eng-bot", "ann", json!({ "n": 2 })).await;
    let a = decide(&s, &t, id, "cy", Decision::Approve).await.unwrap();
    assert_eq!(a.decided_by, Some(member("cy")));
    // An approval decided here is the one the gate then spends.
    let mut tx = s.begin(&t).await.unwrap();
    let args = json!({ "n": 2 });
    let spent = mandate::store::consume_approval(
        &mut tx,
        &mandate::ids::AgentId::parse("eng-bot").unwrap(),
        &member("ann"),
        &mandate::ids::ToolId::parse("web.submit_form").unwrap(),
        &mandate::gate::args_hash(&args),
        OffsetDateTime::now_utc(),
    )
    .await
    .unwrap();
    assert_eq!(spent, Some((id, member("cy"))));
}

#[tokio::test]
#[ignore]
async fn others_cannot_decide() {
    let s = app_store().await;
    let t = org(&s).await;
    let id = ask(&s, &t, "ann-bot", "ann", json!({})).await;
    for d in [Decision::Approve, Decision::Deny] {
        // Bob is not Ann; a member of another tenant cannot even see it.
        assert!(matches!(
            decide(&s, &t, id, "bob", d).await,
            Err(DecideError::Forbidden)
        ));
        assert!(matches!(
            decide(&s, &tenant(), id, "ann", d).await,
            Err(DecideError::NotFound)
        ));
    }
    assert!(matches!(
        decide(&s, &t, Uuid::new_v4(), "ann", Decision::Approve).await,
        Err(DecideError::NotFound)
    ));
    // A disabled agent's requests can no longer be decided.
    exec(
        &s,
        &t,
        "update agents set disabled = true where tenant_id = $1 and id = 'ann-bot'",
        &[],
    )
    .await;
    assert!(matches!(
        decide(&s, &t, id, "ann", Decision::Approve).await,
        Err(DecideError::Forbidden)
    ));
    // Expired: no decision, and the caller learns why.
    let id = ask(&s, &t, "bob-bot", "bob", json!({})).await;
    exec(
        &s,
        &t,
        "update approvals set expires_at = now() - interval '1 second' where tenant_id = $1",
        &[],
    )
    .await;
    assert!(matches!(
        decide(&s, &t, id, "bob", Decision::Approve).await,
        Err(DecideError::Expired)
    ));
}

#[tokio::test]
#[ignore]
async fn concurrent_decisions_one_wins() {
    let s = app_store().await;
    let t = org(&s).await;
    // The first decision holds the row; the second waits on it and then finds it decided.
    let id = ask(&s, &t, "eng-bot", "ann", json!({ "k": "held" })).await;
    let mut first = s.begin(&t).await.unwrap();
    decide_approval(
        &mut first,
        id,
        &member("ann"),
        Decision::Approve,
        OffsetDateTime::now_utc(),
    )
    .await
    .unwrap();
    let (s2, t2) = (s.clone(), t.clone());
    let second = tokio::spawn(async move { decide(&s2, &t2, id, "cy", Decision::Deny).await });
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    assert!(
        !second.is_finished(),
        "the second decision waits on the row"
    );
    first.commit().await.unwrap();
    assert!(matches!(
        second.await.unwrap(),
        Err(DecideError::AlreadyDecided)
    ));

    // Raced for real, many times, approve against deny: always exactly one winner.
    for i in 0..20 {
        let id = ask(&s, &t, "eng-bot", "ann", json!({ "race": i })).await;
        let (a, b) = tokio::join!(
            decide(&s, &t, id, "ann", Decision::Approve),
            decide(&s, &t, id, "cy", Decision::Deny)
        );
        let wins = [&a, &b].iter().filter(|r| r.is_ok()).count();
        assert_eq!(wins, 1, "race {i}");
        let lost = if a.is_ok() { b } else { a };
        assert!(matches!(lost, Err(DecideError::AlreadyDecided)), "race {i}");
        let mut tx = s.begin(&t).await.unwrap();
        let both: bool = sqlx::query(
            "select approved_by is not null and denied_by is not null from approvals where id = $1",
        )
        .bind(id)
        .fetch_one(&mut *tx)
        .await
        .unwrap()
        .get(0);
        assert!(!both);
    }
}

#[tokio::test]
#[ignore]
async fn deny_records_denied_by() {
    let s = app_store().await;
    let t = org(&s).await;
    let args = json!({ "url": "https://x.test/" });
    let id = ask(&s, &t, "ann-bot", "ann", args.clone()).await;
    let a = decide(&s, &t, id, "ann", Decision::Deny).await.unwrap();
    assert_eq!(a.state, State::Denied);
    assert_eq!(a.decided_by, Some(member("ann")));
    let mut tx = s.begin(&t).await.unwrap();
    let r = sqlx::query(
        "select denied_by, denied_at is not null, approved_by from approvals where id = $1",
    )
    .bind(id)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    assert_eq!(r.get::<Option<String>, _>(0).as_deref(), Some("ann"));
    assert!(r.get::<bool, _>(1));
    assert_eq!(r.get::<Option<String>, _>(2), None);
    drop(tx);
    // A denial can be neither approved afterwards nor spent.
    assert!(matches!(
        decide(&s, &t, id, "ann", Decision::Approve).await,
        Err(DecideError::AlreadyDecided)
    ));
    assert!(
        !mandate::store::approve(&mut s.begin(&t).await.unwrap(), id, &member("ann"))
            .await
            .unwrap()
    );
    // Asking again after a denial is a fresh request, not the denied one again.
    let again = ask(&s, &t, "ann-bot", "ann", args).await;
    assert_ne!(again, id);
}

#[tokio::test]
#[ignore]
async fn decision_publishes_approval_decided() {
    let s = app_store().await;
    let t = org(&s).await;
    let id = ask(&s, &t, "ann-bot", "ann", json!({})).await;
    decide(&s, &t, id, "ann", Decision::Deny).await.unwrap();
    // A losing decision publishes nothing.
    assert!(decide(&s, &t, id, "ann", Decision::Approve).await.is_err());
    assert_eq!(
        published(&t),
        [Event::ApprovalDecided {
            approval: id,
            decision: Decision::Deny,
            by: member("ann"),
        }]
    );
}
