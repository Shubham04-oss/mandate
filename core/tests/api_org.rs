//! Activity, organization and agents routes. Live: need DATABASE_URL (a superuser), `--ignored`.
mod api_support;
mod common;
use api_support::*;
use common::app_store;
use http::StatusCode;
use mandate::ids::{AgentId, TenantId, ToolId};
use mandate::store::{self, AuditEntry, Store};
use serde_json::{Value, json};
use uuid::Uuid;

async fn audit(
    s: &Store,
    t: &TenantId,
    agent: &str,
    by: &str,
    decision: &str,
    approved: Option<(Uuid, &str)>,
) {
    let mut tx = s.begin(t).await.unwrap();
    store::audit(
        &mut tx,
        &AuditEntry {
            agent: AgentId::parse(agent).unwrap(),
            member: member(by),
            tool: ToolId::parse("web.submit_form").unwrap(),
            args_hash: [0; 32],
            decision: decision.into(),
            reason: (decision == "refuse").then(|| "no_grant".into()),
            approval_id: approved.map(|a| a.0),
            approved_by: approved.map(|a| member(a.1)),
        },
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
}

/// Ann (admin) and Bob with personal agents; Bob and Cy on team `eng` with `eng-bot`.
async fn org(s: &Store) -> TenantId {
    let t = tenant();
    add_member(s, &t, "ann", "Ann Lee", true).await;
    add_member(s, &t, "bob", "Bob Ray", false).await;
    add_member(s, &t, "cy", "Cy Ng", false).await;
    personal_agent(s, &t, "ann-bot", "ann").await;
    personal_agent(s, &t, "bob-bot", "bob").await;
    team_agent(s, &t, "eng-bot", "eng", &["bob", "cy"]).await;
    t
}

fn agents_of(v: &Value) -> Vec<String> {
    v["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["agent"]["id"].as_str().unwrap().to_owned())
        .collect()
}

#[tokio::test]
#[ignore]
async fn activity_shows_my_agent_only() {
    let s = app_store().await;
    let t = org(&s).await;
    let app = app(s.clone());
    let approval = Uuid::new_v4();
    audit(&s, &t, "ann-bot", "ann", "run", None).await;
    audit(&s, &t, "bob-bot", "bob", "ask", None).await;
    audit(&s, &t, "bob-bot", "bob", "run", Some((approval, "bob"))).await;
    audit(&s, &t, "eng-bot", "cy", "refuse", None).await;

    // Bob: his own agent and his team's bot, newest first; never Ann's.
    let (st, v) = call(&app, "GET", "/v1/activity", Some((&t, "bob")), None).await;
    assert_eq!(st, StatusCode::OK, "{v}");
    assert_eq!(agents_of(&v), ["eng-bot", "bob-bot", "bob-bot"]);
    let refused = &v["items"][0];
    assert_eq!(refused["decision"], json!("refuse"));
    assert_eq!(refused["reason"], json!("no_grant"));
    assert_eq!(
        refused["member"],
        json!({ "id": "cy", "name": "Cy Ng", "email": "cy@example.com" })
    );
    assert_eq!(refused["tool"], json!("web.submit_form"));
    let run = &v["items"][1];
    assert_eq!(run["approval_id"], json!(approval.to_string()));
    assert_eq!(run["approved_by"]["name"], json!("Bob Ray"));
    assert!(run["at"].is_string() && run["id"].is_string());
    // Cy: only the team's bot. Another tenant: nothing.
    let (_, v) = call(&app, "GET", "/v1/activity", Some((&t, "cy")), None).await;
    assert_eq!(agents_of(&v), ["eng-bot"]);
    let (_, v) = call(&app, "GET", "/v1/activity", Some((&tenant(), "bob")), None).await;
    assert_eq!(agents_of(&v), Vec::<String>::new());
    // Leaving the team takes the team's activity with it.
    exec(
        &s,
        &t,
        "delete from team_members where tenant_id = $1 and member_id = 'cy'",
        &[],
    )
    .await;
    let (_, v) = call(&app, "GET", "/v1/activity", Some((&t, "cy")), None).await;
    assert_eq!(agents_of(&v), Vec::<String>::new());
    let (st, _) = call(&app, "GET", "/v1/activity", None, None).await;
    assert_eq!(st, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
#[ignore]
async fn admin_sees_all_activity() {
    let s = app_store().await;
    let t = org(&s).await;
    let app = app(s.clone());
    for i in 0..55 {
        let agent = ["ann-bot", "bob-bot", "eng-bot"][i % 3];
        audit(&s, &t, agent, "bob", "run", None).await;
    }
    let (_, p1) = call(&app, "GET", "/v1/activity", Some((&t, "ann")), None).await;
    assert_eq!(agents_of(&p1).len(), 50);
    let next = p1["next"].as_str().unwrap();
    let (_, p2) = call(
        &app,
        "GET",
        &format!("/v1/activity?cursor={next}"),
        Some((&t, "ann")),
        None,
    )
    .await;
    assert_eq!(agents_of(&p2).len(), 5);
    assert_eq!(p2["next"], json!(null));
    let all: Vec<String> = [agents_of(&p1), agents_of(&p2)].concat();
    assert!(all.contains(&"bob-bot".into()) && all.contains(&"eng-bot".into()));
    // Admin is read now: revoked, Ann sees only her own agent.
    exec(
        &s,
        &t,
        "update members set is_admin = false where tenant_id = $1 and id = 'ann'",
        &[],
    )
    .await;
    let (_, v) = call(&app, "GET", "/v1/activity", Some((&t, "ann")), None).await;
    assert!(agents_of(&v).iter().all(|a| a == "ann-bot"));
    for q in ["?cursor=abc", "?cursor=-1", "?x=1"] {
        let (st, _) = call(
            &app,
            "GET",
            &format!("/v1/activity{q}"),
            Some((&t, "ann")),
            None,
        )
        .await;
        assert_eq!(st, StatusCode::BAD_REQUEST, "{q}");
    }
}

fn ids(v: &Value) -> Vec<String> {
    v["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["id"].as_str().unwrap().to_owned())
        .collect()
}

#[tokio::test]
#[ignore]
async fn members_search_and_paging() {
    let s = app_store().await;
    let t = org(&s).await;
    let app = app(s.clone());
    let (st, v) = call(&app, "GET", "/v1/org/members", Some((&t, "bob")), None).await;
    assert_eq!(st, StatusCode::OK, "{v}");
    assert_eq!(ids(&v), ["ann", "bob", "cy"]);
    assert_eq!(
        v["items"][0],
        json!({ "id": "ann", "name": "Ann Lee", "email": "ann@example.com" })
    );
    // Search by name or email, case-insensitive; LIKE wildcards are literal.
    for (q, want) in [
        ("lee", vec!["ann"]),
        ("BOB%40", vec!["bob"]),
        ("cy@", vec!["cy"]),
        ("%25", vec![]),
        ("_", vec![]),
    ] {
        let (_, v) = call(
            &app,
            "GET",
            &format!("/v1/org/members?q={q}"),
            Some((&t, "bob")),
            None,
        )
        .await;
        assert_eq!(ids(&v), want, "{q}");
    }
    let (_, v) = call(
        &app,
        "GET",
        "/v1/org/members?q=bob%40example",
        Some((&t, "bob")),
        None,
    )
    .await;
    assert_eq!(ids(&v), ["bob"]);
    // Paging: 60 members come back as 50 then 10, in id order.
    for i in 0..57 {
        add_member(&s, &t, &format!("m{i:02}"), "Someone", false).await;
    }
    let (_, p1) = call(&app, "GET", "/v1/org/members", Some((&t, "bob")), None).await;
    assert_eq!(ids(&p1).len(), 50);
    let next = p1["next"].as_str().unwrap();
    let (_, p2) = call(
        &app,
        "GET",
        &format!("/v1/org/members?cursor={next}"),
        Some((&t, "bob")),
        None,
    )
    .await;
    assert_eq!(ids(&p2).len(), 10);
    assert_eq!(p2["next"], json!(null));
    // Another tenant's members are not listed; a bad cursor or an overlong query is 400.
    let (_, v) = call(
        &app,
        "GET",
        "/v1/org/members",
        Some((&tenant(), "bob")),
        None,
    )
    .await;
    assert_eq!(ids(&v), Vec::<String>::new());
    for q in ["?cursor=NOT%20AN%20ID", &format!("?q={}", "a".repeat(101))] {
        let (st, _) = call(
            &app,
            "GET",
            &format!("/v1/org/members{q}"),
            Some((&t, "bob")),
            None,
        )
        .await;
        assert_eq!(st, StatusCode::BAD_REQUEST, "{q}");
    }
}

#[tokio::test]
#[ignore]
async fn teams_list() {
    let s = app_store().await;
    let t = org(&s).await;
    exec(
        &s,
        &t,
        "insert into teams (tenant_id, id, name) values ($1, 'ops', 'Operations')",
        &[],
    )
    .await;
    let app = app(s.clone());
    let (st, v) = call(&app, "GET", "/v1/org/teams", Some((&t, "ann")), None).await;
    assert_eq!(st, StatusCode::OK, "{v}");
    assert_eq!(
        v["items"],
        json!([
            { "id": "eng", "name": "eng", "members": 2, "is_admin": false },
            { "id": "ops", "name": "Operations", "members": 0, "is_admin": false },
        ])
    );
    let (_, v) = call(&app, "GET", "/v1/org/teams", Some((&tenant(), "ann")), None).await;
    assert_eq!(v["items"], json!([]));
}

#[tokio::test]
#[ignore]
async fn agents_list_shows_my_operator() {
    let s = app_store().await;
    let t = org(&s).await;
    let app = app(s.clone());
    let (st, v) = call(&app, "GET", "/v1/agents", Some((&t, "bob")), None).await;
    assert_eq!(st, StatusCode::OK, "{v}");
    assert_eq!(
        v["items"],
        json!([
            {
                "id": "bob-bot", "name": "Personal agent", "kind": "personal", "enabled": true,
                "owner": { "id": "bob", "name": "Bob Ray", "email": "bob@example.com" }
            },
            {
                "id": "eng-bot", "name": "Team bot", "kind": "team", "enabled": true,
                "owner": { "id": "eng", "name": "eng" }
            },
        ])
    );
    // Ann is admin but sees only agents she may use: her own.
    let (_, v) = call(&app, "GET", "/v1/agents", Some((&t, "ann")), None).await;
    assert_eq!(ids(&v), ["ann-bot"]);
    // A disabled operator is listed as disabled.
    exec(
        &s,
        &t,
        "update agents set disabled = true where tenant_id = $1 and id = 'ann-bot'",
        &[],
    )
    .await;
    let (_, v) = call(&app, "GET", "/v1/agents", Some((&t, "ann")), None).await;
    assert_eq!(v["items"][0]["enabled"], json!(false));
    let (_, v) = call(&app, "GET", "/v1/agents", Some((&tenant(), "bob")), None).await;
    assert_eq!(v["items"], json!([]));
}
