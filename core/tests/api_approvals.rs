//! `decide_approval` and the approvals routes. Live: need DATABASE_URL (a superuser), `--ignored`.
mod api_support;
mod common;
use api_support::*;
use common::app_store;
use http::StatusCode;
use mandate::api::approvals::{DecideError, Decision, State, decide_approval};
use mandate::store::Store;
use serde_json::json;
use sqlx::Row;
use time::OffsetDateTime;
use uuid::Uuid;

/// Ann owns `ann-bot`; Bob is a stranger to it; Ann, Cy and Dee are on team `eng` with `eng-bot`.
async fn org(s: &Store) -> mandate::ids::TenantId {
    let t = tenant();
    add_member(s, &t, "ann", "Ann Lee", false).await;
    add_member(s, &t, "bob", "Bob Ray", false).await;
    add_member(s, &t, "cy", "Cy Ng", false).await;
    add_member(s, &t, "dee", "Dee Ko", false).await;
    personal_agent(s, &t, "ann-bot", "ann").await;
    personal_agent(s, &t, "bob-bot", "bob").await;
    team_agent(s, &t, "eng-bot", "eng", &["ann", "cy", "dee"]).await;
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
    let a = decide(&s, &t, id, "ann", Decision::Approved).await.unwrap();
    assert_eq!(a.state, State::Approved);
    assert_eq!(a.decided_by, Some(member("ann")));
    assert!(a.decided_at.is_some());
    // Any member of the owning team may decide a team bot's request.
    let id = ask(&s, &t, "eng-bot", "ann", json!({ "n": 2 })).await;
    let a = decide(&s, &t, id, "cy", Decision::Approved).await.unwrap();
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
    for d in [Decision::Approved, Decision::Denied] {
        // Bob can neither see nor decide it, nor can a member of another tenant: not found.
        assert!(matches!(
            decide(&s, &t, id, "bob", d).await,
            Err(DecideError::NotFound)
        ));
        assert!(matches!(
            decide(&s, &tenant(), id, "ann", d).await,
            Err(DecideError::NotFound)
        ));
    }
    assert!(matches!(
        decide(&s, &t, Uuid::new_v4(), "ann", Decision::Approved).await,
        Err(DecideError::NotFound)
    ));
    // A disabled agent's requests can no longer be decided: a conflict, not a refusal.
    exec(
        &s,
        &t,
        "update agents set disabled = true where tenant_id = $1 and id = 'ann-bot'",
        &[],
    )
    .await;
    assert!(matches!(
        decide(&s, &t, id, "ann", Decision::Approved).await,
        Err(DecideError::Disabled)
    ));
    // ...and still unknown to a stranger.
    assert!(matches!(
        decide(&s, &t, id, "bob", Decision::Approved).await,
        Err(DecideError::NotFound)
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
        decide(&s, &t, id, "bob", Decision::Approved).await,
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
        &member("dee"),
        Decision::Approved,
        OffsetDateTime::now_utc(),
    )
    .await
    .unwrap();
    let (s2, t2) = (s.clone(), t.clone());
    let second = tokio::spawn(async move { decide(&s2, &t2, id, "cy", Decision::Denied).await });
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
            decide(&s, &t, id, "dee", Decision::Approved),
            decide(&s, &t, id, "cy", Decision::Denied)
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

    // Over HTTP (web and a stale tab): one 200, one 409.
    let app = app(s.clone());
    let id = ask(&s, &t, "ann-bot", "ann", json!({ "k": "http" })).await;
    let (approve, deny) = (
        format!("/v1/approvals/{id}/approve"),
        format!("/v1/approvals/{id}/deny"),
    );
    let (x, y) = tokio::join!(
        call(&app, "POST", &approve, Some((&t, "ann")), None),
        call(&app, "POST", &deny, Some((&t, "ann")), None)
    );
    let mut codes = [x.0, y.0];
    codes.sort();
    assert_eq!(codes, [StatusCode::OK, StatusCode::CONFLICT]);
}

#[tokio::test]
#[ignore]
async fn deny_records_denied_by() {
    let s = app_store().await;
    let t = org(&s).await;
    let args = json!({ "url": "https://x.test/" });
    let id = ask(&s, &t, "ann-bot", "ann", args.clone()).await;
    let a = decide(&s, &t, id, "ann", Decision::Denied).await.unwrap();
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
        decide(&s, &t, id, "ann", Decision::Approved).await,
        Err(DecideError::AlreadyDecided)
    ));
    assert!(
        !common::approve(&mut s.begin(&t).await.unwrap(), id, &member("ann"))
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
    decide(&s, &t, id, "ann", Decision::Denied).await.unwrap();
    // A losing decision publishes nothing.
    assert!(decide(&s, &t, id, "ann", Decision::Approved).await.is_err());
    assert_eq!(
        outbox(&s, &t).await,
        [(
            "approval.decided.v1".to_owned(),
            json!({ "approval": id, "decision": "denied", "by": "ann" })
        )]
    );
}

#[tokio::test]
#[ignore]
async fn decisions_are_audited() {
    let s = app_store().await;
    let t = org(&s).await;
    let args = json!({ "n": 1 });
    let yes = ask(&s, &t, "ann-bot", "ann", args.clone()).await;
    let no = ask(&s, &t, "eng-bot", "ann", json!({ "n": 2 })).await;
    decide(&s, &t, yes, "ann", Decision::Approved)
        .await
        .unwrap();
    decide(&s, &t, no, "cy", Decision::Denied).await.unwrap();
    // A refused decision leaves no row.
    assert!(decide(&s, &t, no, "ann", Decision::Approved).await.is_err());
    let mut tx = s.begin(&t).await.unwrap();
    let rows: Vec<(String, Vec<u8>)> = sqlx::query_as(
        "select concat_ws(':', agent, member, tool, decision, coalesce(reason, '-'), approval_id, \
         coalesce(approved_by, '-')), args_hash from audit order by id",
    )
    .fetch_all(&mut *tx)
    .await
    .unwrap();
    let hash = |a: serde_json::Value| mandate::gate::args_hash(&a).to_vec();
    assert_eq!(
        rows,
        [
            (
                format!("ann-bot:ann:web.submit_form:approved:-:{yes}:ann"),
                hash(args)
            ),
            (
                format!("eng-bot:cy:web.submit_form:denied:-:{no}:-"),
                hash(json!({ "n": 2 }))
            ),
        ]
    );
    drop(tx);
    // The activity feed shows what agents did, not these decisions.
    let (_, v) = call(
        &app(s.clone()),
        "GET",
        "/v1/activity",
        Some((&t, "ann")),
        None,
    )
    .await;
    assert_eq!(v["items"], json!([]), "{v}");
}

// ---- routes ----

#[tokio::test]
#[ignore]
async fn inbox_lists_pending_i_can_decide_and_my_requests() {
    let s = app_store().await;
    let t = org(&s).await;
    let app = app(s.clone());
    let mine = ask(&s, &t, "ann-bot", "ann", json!({ "a": 1 })).await;
    let teams = ask(&s, &t, "eng-bot", "cy", json!({ "a": 2 })).await;
    let bobs = ask(&s, &t, "bob-bot", "bob", json!({ "a": 3 })).await;
    let decided = ask(&s, &t, "ann-bot", "ann", json!({ "a": 4 })).await;
    decide(&s, &t, decided, "ann", Decision::Approved)
        .await
        .unwrap();
    let ids = |v: &serde_json::Value| -> Vec<String> {
        v["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| i["id"].as_str().unwrap().to_owned())
            .collect()
    };

    let (st, v) = call(&app, "GET", "/v1/approvals", Some((&t, "ann")), None).await;
    assert_eq!(st, StatusCode::OK, "{v}");
    let got = ids(&v);
    assert_eq!(got.len(), 2, "{v}");
    assert!(got.contains(&mine.to_string()) && got.contains(&teams.to_string()));
    assert!(
        v["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|i| i["can_decide"] == json!(true))
    );
    assert_eq!(v["next"], json!(null));

    let (_, v) = call(
        &app,
        "GET",
        "/v1/approvals?state=decided",
        Some((&t, "ann")),
        None,
    )
    .await;
    assert_eq!(ids(&v), [decided.to_string()]);
    assert_eq!(v["items"][0]["state"], json!("approved"));
    assert_eq!(v["items"][0]["decided_by"]["name"], json!("Ann Lee"));
    assert_eq!(v["items"][0]["can_decide"], json!(false));

    // Bob sees only his own; his own request is decidable by him (his agent).
    let (_, v) = call(&app, "GET", "/v1/approvals", Some((&t, "bob")), None).await;
    assert_eq!(ids(&v), [bobs.to_string()]);
    // Cy requested the team's one: he sees his request but may not decide it himself.
    let (_, v) = call(&app, "GET", "/v1/approvals", Some((&t, "cy")), None).await;
    assert_eq!(ids(&v), [teams.to_string()]);
    assert_eq!(v["items"][0]["can_decide"], json!(false));
    // Another tenant with the same member id sees nothing of this one.
    let other = tenant();
    add_member(&s, &other, "ann", "Ann Other", false).await;
    let (_, v) = call(&app, "GET", "/v1/approvals", Some((&other, "ann")), None).await;
    assert_eq!(ids(&v), Vec::<String>::new());

    // No session: 401 with the API's error body; a bad state or cursor: 400.
    let (st, v) = call(&app, "GET", "/v1/approvals", None, None).await;
    assert_eq!(st, StatusCode::UNAUTHORIZED);
    assert_eq!(v["error"]["code"], json!("forbidden"));
    for q in ["?state=all", "?cursor=nope", "?cursor=1.x", "?other=1"] {
        let (st, v) = call(
            &app,
            "GET",
            &format!("/v1/approvals{q}"),
            Some((&t, "ann")),
            None,
        )
        .await;
        assert_eq!(st, StatusCode::BAD_REQUEST, "{q}");
        assert_eq!(v["error"]["code"], json!("invalid_input"));
    }

    // Paging: 55 requests come back as 50 then 5, newest first, with no overlap.
    for i in 0..55 {
        ask(&s, &t, "bob-bot", "bob", json!({ "page": i })).await;
    }
    let (_, p1) = call(&app, "GET", "/v1/approvals", Some((&t, "bob")), None).await;
    assert_eq!(ids(&p1).len(), 50);
    let next = p1["next"].as_str().unwrap();
    let (_, p2) = call(
        &app,
        "GET",
        &format!("/v1/approvals?cursor={next}"),
        Some((&t, "bob")),
        None,
    )
    .await;
    assert_eq!(ids(&p2).len(), 6);
    assert_eq!(p2["next"], json!(null));
    assert!(ids(&p2).iter().all(|i| !ids(&p1).contains(i)));
    assert_eq!(ids(&p2).last().unwrap(), &bobs.to_string(), "oldest last");
}

#[tokio::test]
#[ignore]
async fn approve_route_returns_approval() {
    let s = app_store().await;
    let t = org(&s).await;
    let app = app(s.clone());
    let id = ask(
        &s,
        &t,
        "eng-bot",
        "ann",
        json!({ "url": "https://x.test/" }),
    )
    .await;
    let (st, v) = call(
        &app,
        "POST",
        &format!("/v1/approvals/{id}/approve"),
        Some((&t, "cy")),
        None,
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{v}");
    assert_eq!(v["id"], json!(id.to_string()));
    assert_eq!(v["state"], json!("approved"));
    assert_eq!(v["tool"], json!("web.submit_form"));
    assert_eq!(v["effect"], json!("external"));
    assert_eq!(v["agent"]["id"], json!("eng-bot"));
    assert_eq!(v["agent"]["kind"], json!("team"));
    assert_eq!(v["agent"]["owner"], json!({ "id": "eng", "name": "eng" }));
    assert_eq!(v["agent"]["enabled"], json!(true));
    assert_eq!(
        v["requested_by"],
        json!({ "id": "ann", "name": "Ann Lee", "email": "ann@example.com" })
    );
    assert_eq!(v["decided_by"]["id"], json!("cy"));
    assert_eq!(v["can_decide"], json!(false));
    // Asked outside a conversation: no link.
    assert_eq!(v["conversation"], json!(null));
    for k in ["requested_at", "expires_at", "decided_at"] {
        let ts = v[k].as_str().unwrap();
        assert!(
            OffsetDateTime::parse(ts, &time::format_description::well_known::Rfc3339).is_ok(),
            "{k}"
        );
    }
    // Deny works the same way; a stranger, an unknown or a malformed id get 404; a team
    // requester deciding their own request gets 403.
    let own = ask(&s, &t, "eng-bot", "cy", json!({ "own": 1 })).await;
    let (st, _) = call(
        &app,
        "POST",
        &format!("/v1/approvals/{own}/approve"),
        Some((&t, "cy")),
        None,
    )
    .await;
    assert_eq!(st, StatusCode::FORBIDDEN);
    let id = ask(&s, &t, "ann-bot", "ann", json!({})).await;
    let (st, _) = call(
        &app,
        "POST",
        &format!("/v1/approvals/{id}/deny"),
        Some((&t, "bob")),
        None,
    )
    .await;
    assert_eq!(st, StatusCode::NOT_FOUND);
    let (st, v) = call(
        &app,
        "POST",
        &format!("/v1/approvals/{id}/deny"),
        Some((&t, "ann")),
        None,
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(v["state"], json!("denied"));
    for path in [
        format!("/v1/approvals/{}/approve", Uuid::new_v4()),
        "/v1/approvals/x/approve".into(),
    ] {
        let (st, _) = call(&app, "POST", &path, Some((&t, "ann")), None).await;
        assert_eq!(st, StatusCode::NOT_FOUND, "{path}");
    }
    let (st, _) = call(
        &app,
        "POST",
        &format!("/v1/approvals/{id}/approve"),
        None,
        None,
    )
    .await;
    assert_eq!(st, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
#[ignore]
async fn second_decision_is_409() {
    let s = app_store().await;
    let t = org(&s).await;
    let app = app(s.clone());
    let id = ask(&s, &t, "ann-bot", "ann", json!({})).await;
    let path = format!("/v1/approvals/{id}/approve");
    assert_eq!(
        call(&app, "POST", &path, Some((&t, "ann")), None).await.0,
        StatusCode::OK
    );
    for p in [path.clone(), format!("/v1/approvals/{id}/deny")] {
        let (st, v) = call(&app, "POST", &p, Some((&t, "ann")), None).await;
        assert_eq!(st, StatusCode::CONFLICT);
        assert_eq!(v["error"]["code"], json!("conflict"));
    }
    // An expired one is 409 too.
    let id = ask(&s, &t, "bob-bot", "bob", json!({})).await;
    exec(&s, &t, "update approvals set expires_at = now() - interval '1 second' where tenant_id = $1 and used_at is null and approved_by is null", &[]).await;
    let (st, _) = call(
        &app,
        "POST",
        &format!("/v1/approvals/{id}/approve"),
        Some((&t, "bob")),
        None,
    )
    .await;
    assert_eq!(st, StatusCode::CONFLICT);
}

#[tokio::test]
#[ignore]
async fn approval_args_are_shown_exactly() {
    let s = app_store().await;
    let t = org(&s).await;
    let app = app(s.clone());
    // Nested, unicode, odd keys and numbers: the approver sees exactly what will run.
    let args = json!({
        "url": "https://pay.example/transfer?to=acct-9&amount=10000",
        "fields": { "amount": "10000", "memo": "rent \u{1F3E0} <b>x</b>", "z": [1, 2.5, null, true] },
        "": "empty key"
    });
    let id = ask(&s, &t, "ann-bot", "ann", args.clone()).await;
    let (_, v) = call(&app, "GET", "/v1/approvals", Some((&t, "ann")), None).await;
    assert_eq!(v["items"][0]["id"], json!(id.to_string()));
    assert_eq!(v["items"][0]["args"], args);
    let (_, v) = call(
        &app,
        "POST",
        &format!("/v1/approvals/{id}/approve"),
        Some((&t, "ann")),
        None,
    )
    .await;
    assert_eq!(v["args"], args);
}

#[tokio::test]
#[ignore]
async fn team_requester_cannot_decide_their_own_request() {
    let s = app_store().await;
    let t = org(&s).await;
    let id = ask(&s, &t, "eng-bot", "ann", json!({ "self": 1 })).await;
    for d in [Decision::Approved, Decision::Denied] {
        assert!(matches!(
            decide(&s, &t, id, "ann", d).await,
            Err(DecideError::Forbidden)
        ));
    }
    // A teammate still can; a personal agent's owner still decides their own agent's ask.
    decide(&s, &t, id, "cy", Decision::Approved).await.unwrap();
    let own = ask(&s, &t, "ann-bot", "ann", json!({ "self": 2 })).await;
    decide(&s, &t, own, "ann", Decision::Approved)
        .await
        .unwrap();
}

#[tokio::test]
#[ignore]
async fn disabled_agent_decision_is_409() {
    let s = app_store().await;
    let t = org(&s).await;
    let app = app(s.clone());
    let id = ask(&s, &t, "ann-bot", "ann", json!({})).await;
    exec(
        &s,
        &t,
        "update agents set disabled = true where tenant_id = $1 and id = 'ann-bot'",
        &[],
    )
    .await;
    let path = format!("/v1/approvals/{id}/approve");
    let (st, v) = call(&app, "POST", &path, Some((&t, "ann")), None).await;
    assert_eq!(st, StatusCode::CONFLICT, "{v}");
    assert_eq!(v["error"]["code"], json!("conflict"));
    let (st, _) = call(&app, "POST", &path, Some((&t, "bob")), None).await;
    assert_eq!(st, StatusCode::NOT_FOUND);
}
