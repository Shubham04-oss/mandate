//! Grants routes: org admins create, list and revoke. Live: need DATABASE_URL (a superuser),
//! `--ignored`.
mod api_support;
mod common;
use api_support::*;
use common::app_store;
use http::StatusCode;
use mandate::ids::{AgentId, TenantId, ToolId};
use mandate::store::{self, Store};
use serde_json::{Value, json};
use time::format_description::well_known::Rfc3339;
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

async fn org(s: &Store) -> TenantId {
    let t = tenant();
    add_member(s, &t, "ann", "Ann Lee", true).await;
    add_member(s, &t, "bob", "Bob Ray", false).await;
    personal_agent(s, &t, "bob-bot", "bob").await;
    t
}

fn at(d: Duration) -> String {
    (OffsetDateTime::now_utc() + d).format(&Rfc3339).unwrap()
}

fn body(kind: &str, id: &str, tool: &str, expires: String) -> Value {
    json!({ "principal": { "kind": kind, "id": id }, "tool": tool, "standing": false, "expires_at": expires, "reason": "pilot" })
}

async fn granted(s: &Store, t: &TenantId) -> bool {
    let mut tx = s.begin(t).await.unwrap();
    store::live_grant(
        &mut tx,
        &AgentId::parse("bob-bot").unwrap(),
        &[],
        &ToolId::parse("web.read_page").unwrap(),
        OffsetDateTime::now_utc(),
    )
    .await
    .unwrap()
    .granted
}

#[tokio::test]
#[ignore]
async fn admin_creates_and_revokes_grant() {
    let s = app_store().await;
    let t = org(&s).await;
    let app = app(s.clone());
    let expires = at(Duration::hours(1));
    let (st, g) = call(
        &app,
        "POST",
        "/v1/grants",
        Some((&t, "ann")),
        Some(body("agent", "bob-bot", "web.read_page", expires)),
    )
    .await;
    assert_eq!(st, StatusCode::CREATED, "{g}");
    assert_eq!(g["principal"], json!({ "kind": "agent", "id": "bob-bot" }));
    assert_eq!(g["tool"], json!("web.read_page"));
    assert_eq!(g["standing"], json!(false));
    assert_eq!(g["state"], json!("active"));
    assert_eq!(g["reason"], json!("pilot"));
    assert_eq!(g["granted_by"]["id"], json!("ann"));
    let id: Uuid = g["id"].as_str().unwrap().parse().unwrap();
    assert!(granted(&s, &t).await, "the gate sees the new grant");

    let (st, v) = call(&app, "GET", "/v1/grants", Some((&t, "ann")), None).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(v["items"], json!([g]));
    // The same grant twice is a conflict, not a silent overwrite.
    let (st, _) = call(
        &app,
        "POST",
        "/v1/grants",
        Some((&t, "ann")),
        Some(body(
            "agent",
            "bob-bot",
            "web.read_page",
            at(Duration::hours(2)),
        )),
    )
    .await;
    assert_eq!(st, StatusCode::CONFLICT);

    let (st, _) = call(
        &app,
        "DELETE",
        &format!("/v1/grants/{id}"),
        Some((&t, "ann")),
        None,
    )
    .await;
    assert_eq!(st, StatusCode::NO_CONTENT);
    assert!(!granted(&s, &t).await);
    let (st, _) = call(
        &app,
        "DELETE",
        &format!("/v1/grants/{id}"),
        Some((&t, "ann")),
        None,
    )
    .await;
    assert_eq!(st, StatusCode::NOT_FOUND);
    assert_eq!(
        outbox(&s, &t).await,
        [
            ("grant.created.v1".to_owned(), json!({ "grant": id })),
            ("grant.revoked.v1".to_owned(), json!({ "grant": id }))
        ]
    );
    // Both changes are audited, with fixed reasons and no arguments.
    let mut tx = s.begin(&t).await.unwrap();
    let rows: Vec<(String, Vec<u8>)> = sqlx::query_as(
        "select concat_ws(':', agent, member, tool, decision, reason), args_hash from audit order by id",
    )
    .fetch_all(&mut *tx)
    .await
    .unwrap();
    drop(tx);
    let row = |d: &str| {
        (
            format!("bob-bot:ann:web.read_page:{d}:principal is an agent"),
            vec![0u8; 32],
        )
    };
    assert_eq!(rows, [row("grant_created"), row("grant_revoked")]);

    // Refused input: unknown tool or principal, a past expiry or one past 90 days, unknown fields,
    // bad kinds.
    for b in [
        body("agent", "bob-bot", "mail.send", at(Duration::hours(1))),
        body("agent", "nobody", "web.read_page", at(Duration::hours(1))),
        body("member", "zed", "web.read_page", at(Duration::hours(1))),
        body("team", "eng", "web.read_page", at(Duration::hours(1))),
        body(
            "agent",
            "bob-bot",
            "web.read_page",
            at(Duration::seconds(-1)),
        ),
        body("agent", "bob-bot", "web.read_page", "tomorrow".into()),
        body("agent", "bob-bot", "web.read_page", at(Duration::days(91))),
        body("org", "bob-bot", "web.read_page", at(Duration::hours(1))),
        body("agent", "Bob Bot", "web.read_page", at(Duration::hours(1))),
        json!({ "principal": { "kind": "agent", "id": "bob-bot" }, "tool": "web.read_page", "expires_at": at(Duration::hours(1)), "granted_by": "bob" }),
    ] {
        let (st, v) = call(
            &app,
            "POST",
            "/v1/grants",
            Some((&t, "ann")),
            Some(b.clone()),
        )
        .await;
        assert_eq!(st, StatusCode::BAD_REQUEST, "{b} {v}");
        assert_eq!(v["error"]["code"], json!("invalid_input"));
    }
    // Another tenant's agent cannot be granted from here.
    let other = org(&s).await;
    let (st, _) = call(
        &app,
        "POST",
        "/v1/grants",
        Some((&other, "ann")),
        Some(body(
            "agent",
            "bob-bot",
            "web.read_page",
            at(Duration::hours(1)),
        )),
    )
    .await;
    assert_eq!(st, StatusCode::CREATED);
    let (_, v) = call(&app, "GET", "/v1/grants", Some((&t, "ann")), None).await;
    assert_eq!(v["items"], json!([]));
}

#[tokio::test]
#[ignore]
async fn non_admin_cannot_manage_grants() {
    let s = app_store().await;
    let t = org(&s).await;
    let app = app(s.clone());
    let b = body("agent", "bob-bot", "web.read_page", at(Duration::hours(1)));
    let (st, g) = call(
        &app,
        "POST",
        "/v1/grants",
        Some((&t, "ann")),
        Some(b.clone()),
    )
    .await;
    assert_eq!(st, StatusCode::CREATED);
    let id = g["id"].as_str().unwrap();
    // Bob may not grant himself anything, list grants, or revoke them.
    let (st, v) = call(
        &app,
        "POST",
        "/v1/grants",
        Some((&t, "bob")),
        Some(body(
            "agent",
            "bob-bot",
            "web.submit_form",
            at(Duration::hours(1)),
        )),
    )
    .await;
    assert_eq!(st, StatusCode::FORBIDDEN);
    assert_eq!(v["error"]["code"], json!("forbidden"));
    assert_eq!(
        call(&app, "GET", "/v1/grants", Some((&t, "bob")), None)
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        call(
            &app,
            "DELETE",
            &format!("/v1/grants/{id}"),
            Some((&t, "bob")),
            None
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert!(granted(&s, &t).await);
    // Admin of another tenant: their tenant has no such grant.
    let other = org(&s).await;
    assert_eq!(
        call(
            &app,
            "DELETE",
            &format!("/v1/grants/{id}"),
            Some((&other, "ann")),
            None
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    assert!(granted(&s, &t).await);
    // No session at all.
    assert_eq!(
        call(&app, "POST", "/v1/grants", None, Some(b)).await.0,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
#[ignore]
async fn expired_grant_is_listed_as_expired() {
    let s = app_store().await;
    let t = org(&s).await;
    let app = app(s.clone());
    let (st, _) = call(
        &app,
        "POST",
        "/v1/grants",
        Some((&t, "ann")),
        Some(body(
            "member",
            "bob",
            "web.read_page",
            at(Duration::hours(1)),
        )),
    )
    .await;
    assert_eq!(st, StatusCode::CREATED);
    admin_exec(
        "update grants set expires_at = now() - interval '1 second' where tenant_id = $1",
        &[t.as_str()],
    )
    .await;
    let (_, v) = call(&app, "GET", "/v1/grants", Some((&t, "ann")), None).await;
    assert_eq!(v["items"][0]["state"], json!("expired"));
    assert_eq!(
        v["items"][0]["principal"],
        json!({ "kind": "member", "id": "bob" })
    );
}

#[tokio::test]
#[ignore]
async fn expired_grant_gives_way_to_a_new_one() {
    let s = app_store().await;
    let t = org(&s).await;
    let app = app(s.clone());
    let new = |d| body("agent", "bob-bot", "web.read_page", at(d));
    let (st, old) = call(
        &app,
        "POST",
        "/v1/grants",
        Some((&t, "ann")),
        Some(new(Duration::hours(1))),
    )
    .await;
    assert_eq!(st, StatusCode::CREATED);
    admin_exec(
        "update grants set expires_at = now() - interval '1 second' where tenant_id = $1",
        &[t.as_str()],
    )
    .await;
    assert!(!granted(&s, &t).await);
    // Expired: granting again succeeds, as a new grant.
    let (st, g) = call(
        &app,
        "POST",
        "/v1/grants",
        Some((&t, "ann")),
        Some(new(Duration::days(90))),
    )
    .await;
    assert_eq!(st, StatusCode::CREATED, "{g}");
    assert_ne!(g["id"], old["id"]);
    assert_eq!(g["state"], json!("active"));
    assert!(granted(&s, &t).await);
    let (_, v) = call(&app, "GET", "/v1/grants", Some((&t, "ann")), None).await;
    assert_eq!(v["items"], json!([g]));
    // Live: still a conflict, and the live grant is untouched.
    let (st, v) = call(
        &app,
        "POST",
        "/v1/grants",
        Some((&t, "ann")),
        Some(new(Duration::hours(1))),
    )
    .await;
    assert_eq!(st, StatusCode::CONFLICT);
    assert_eq!(v["error"]["code"], json!("conflict"));
    let (_, v) = call(&app, "GET", "/v1/grants", Some((&t, "ann")), None).await;
    assert_eq!(v["items"], json!([g]));
}
