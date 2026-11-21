//! The web sign-in API. Tests that touch the database need DATABASE_URL (a superuser) and
//! `--ignored`; WorkOS is always a fake, never the network.
mod common;
use axum::Router;
use axum::body::Body;
use axum::routing::{get, post};
use common::app_store;
use http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use mandate::api::session::{self, Session};
use mandate::api::{self, ApiState};
use mandate::ids::{MemberId, TenantId};
use mandate::store::Store;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::postgres::PgPoolOptions;
use std::sync::Arc;
use time::{Duration, OffsetDateTime};
use tower::ServiceExt;
use uuid::Uuid;

const WEB: &str = "http://localhost:3000";

fn state(store: Store) -> Arc<ApiState> {
    Arc::new(ApiState {
        store,
        web_origin: WEB.into(),
    })
}

/// A store that never connects: for requests refused before any query.
fn offline_store() -> Store {
    Store::from_pool(
        PgPoolOptions::new()
            .connect_lazy("postgres://nobody@127.0.0.1:1/none")
            .unwrap(),
    )
}

async fn whoami(s: Session) -> axum::Json<Value> {
    axum::Json(json!({ "tenant": s.tenant, "member": s.member, "is_admin": s.is_admin }))
}

/// The API's layers around two probes: one behind the session extractor, one open.
fn probe(state: Arc<ApiState>) -> Router {
    let routes = Router::new()
        .route("/probe", get(whoami).post(whoami))
        .route("/open", post(|| async { "ok" }));
    api::with_routes(routes, state)
}

fn tenant() -> TenantId {
    TenantId::parse(&format!("t-{}", Uuid::new_v4().simple())).unwrap()
}

fn member(s: &str) -> MemberId {
    MemberId::parse(s).unwrap()
}

async fn add_member(store: &Store, t: &TenantId, m: &MemberId, admin: bool) {
    let mut tx = store.begin(t).await.unwrap();
    sqlx::query(
        "insert into members (tenant_id, id, is_admin) values (current_setting('mandate.tenant'), $1, $2)",
    )
    .bind(m.as_str())
    .bind(admin)
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
}

async fn new_session(store: &Store, t: &TenantId, m: &MemberId, now: OffsetDateTime) -> String {
    let mut tx = store.begin(t).await.unwrap();
    let token = session::create(&mut tx, t, m, &json!({}), now)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    token
}

fn get_with_cookie(uri: &str, cookie: &str) -> Request<Body> {
    Request::get(uri)
        .header(
            header::COOKIE,
            format!("theme=dark; mandate_session={cookie}"),
        )
        .body(Body::empty())
        .unwrap()
}

async fn body_json(r: axum::response::Response) -> Value {
    let b = r.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&b).unwrap_or(Value::Null)
}

#[test]
fn session_cookie_has_safe_attributes() {
    let set = session::set_cookie("acme.secret").unwrap();
    let set = set.to_str().unwrap();
    assert!(set.starts_with("mandate_session=acme.secret; "), "{set}");
    let attrs: Vec<&str> = set.split("; ").collect();
    for want in [
        "HttpOnly",
        "Secure",
        "SameSite=Lax",
        "Path=/",
        "Max-Age=604800",
    ] {
        assert!(attrs.contains(&want), "{want} missing from {set}");
    }
    // Host-only: no Domain attribute.
    assert!(!set.to_ascii_lowercase().contains("domain"), "{set}");
    let clear = session::clear_cookie();
    let clear = clear.to_str().unwrap();
    assert!(clear.starts_with("mandate_session=; "), "{clear}");
    for want in ["HttpOnly", "Secure", "SameSite=Lax", "Path=/", "Max-Age=0"] {
        assert!(
            clear.split("; ").any(|a| a == want),
            "{want} missing from {clear}"
        );
    }
}

#[tokio::test]
async fn unsafe_method_without_csrf_header_is_403() {
    let app = probe(state(offline_store()));
    let post = |uri: &str, mandate: Option<&str>, origin: Option<&str>| {
        let mut r = Request::post(uri);
        if let Some(v) = mandate {
            r = r.header("x-mandate", v);
        }
        if let Some(v) = origin {
            r = r.header(header::ORIGIN, v);
        }
        r.body(Body::empty()).unwrap()
    };
    for uri in ["/probe", "/open"] {
        for (mandate, origin) in [
            (None, None),
            (None, Some(WEB)),
            (Some("1"), None),
            (Some("1"), Some("https://evil.example")),
            (Some("0"), Some(WEB)),
            (Some("1"), Some("http://localhost:3000.evil.example")),
        ] {
            let r = app
                .clone()
                .oneshot(post(uri, mandate, origin))
                .await
                .unwrap();
            assert_eq!(
                r.status(),
                StatusCode::FORBIDDEN,
                "{uri} {mandate:?} {origin:?}"
            );
            assert_eq!(body_json(r).await["error"]["code"], "forbidden");
        }
    }
    // With both, the open route runs and the session route asks for a session.
    let r = app
        .clone()
        .oneshot(post("/open", Some("1"), Some(WEB)))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::OK);
    let r = app
        .clone()
        .oneshot(post("/probe", Some("1"), Some(WEB)))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(body_json(r).await["error"]["code"], "forbidden");
    // Safe methods need no header.
    let r = app
        .oneshot(Request::get("/probe").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn cors_allows_only_the_web_origin() {
    let app = probe(state(offline_store()));
    let preflight = |origin: &str| {
        Request::options("/open")
            .header(header::ORIGIN, origin)
            .header(header::ACCESS_CONTROL_REQUEST_METHOD, "POST")
            .header(
                header::ACCESS_CONTROL_REQUEST_HEADERS,
                "content-type,x-mandate",
            )
            .body(Body::empty())
            .unwrap()
    };
    let r = app.clone().oneshot(preflight(WEB)).await.unwrap();
    let h = r.headers();
    assert_eq!(h[header::ACCESS_CONTROL_ALLOW_ORIGIN], WEB);
    assert_eq!(h[header::ACCESS_CONTROL_ALLOW_CREDENTIALS], "true");
    let allowed = h[header::ACCESS_CONTROL_ALLOW_HEADERS]
        .to_str()
        .unwrap()
        .to_owned();
    assert!(allowed.contains("x-mandate"), "{allowed}");

    let r = app
        .clone()
        .oneshot(preflight("https://evil.example"))
        .await
        .unwrap();
    assert!(
        r.headers()
            .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
            .is_none()
    );

    let r = app
        .oneshot(
            Request::get("/probe")
                .header(header::ORIGIN, "https://evil.example")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(
        r.headers()
            .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
            .is_none()
    );
}

#[tokio::test]
#[ignore]
async fn session_is_bound_to_its_tenant() {
    let store = app_store().await;
    let (a, b) = (tenant(), tenant());
    let ann = member("ann");
    // The same member id in both tenants: only the session's own tenant may answer.
    add_member(&store, &a, &ann, true).await;
    add_member(&store, &b, &ann, true).await;
    let token = new_session(&store, &a, &ann, OffsetDateTime::now_utc()).await;
    let (prefix, secret) = token.split_once('.').unwrap();
    assert_eq!(prefix, a.as_str());
    // 256 random bits, base64url without padding.
    assert_eq!(secret.len(), 43, "{secret}");
    // Only the cookie's SHA-256 is stored.
    let stored: Vec<Vec<u8>> =
        sqlx::query_scalar("select token_hash from sessions where tenant_id = $1")
            .bind(a.as_str())
            .fetch_all(&common::admin_pool().await)
            .await
            .unwrap();
    assert_eq!(stored, vec![Sha256::digest(token.as_bytes()).to_vec()]);

    let app = probe(state(store.clone()));
    let r = app
        .clone()
        .oneshot(get_with_cookie("/probe", &token))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::OK);
    let me = body_json(r).await;
    assert_eq!(me["tenant"], a.as_str());
    assert_eq!(me["member"], "ann");
    assert_eq!(me["is_admin"], true);

    // The same secret presented for the other tenant is refused.
    let forged = format!("{b}.{secret}");
    let r = app
        .clone()
        .oneshot(get_with_cookie("/probe", &forged))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::UNAUTHORIZED);
    // Not even the store finds it from the other tenant.
    let mut tx = store.begin(&b).await.unwrap();
    let now = OffsetDateTime::now_utc();
    assert!(
        session::lookup(&mut tx, &token, now)
            .await
            .unwrap()
            .is_none()
    );
    drop(tx);
    for bad in [
        "",
        "nodot",
        "Bad Tenant.x",
        &format!("{a}."),
        &format!("{a}.{}", "x".repeat(43)),
    ] {
        let r = app
            .clone()
            .oneshot(get_with_cookie("/probe", bad))
            .await
            .unwrap();
        assert_eq!(r.status(), StatusCode::UNAUTHORIZED, "{bad:?}");
    }

    // A pending step bound to tenant A is invisible from B and from untenanted sign-in.
    let mut tx = store.begin(&a).await.unwrap();
    let pending = session::create_pending(
        &mut tx,
        session::NewPending {
            kind: "mfa_enable",
            workos_token: None,
            email: None,
            data: json!({ "member": "ann" }),
            tenant: Some(&a),
        },
        now,
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    let mut tx = store.begin(&b).await.unwrap();
    assert!(
        session::pending(&mut tx, &pending, &["mfa_enable"], now)
            .await
            .unwrap()
            .is_none()
    );
    drop(tx);
    let mut tx = store.begin_untenanted().await.unwrap();
    assert!(
        session::pending(&mut tx, &pending, &["mfa_enable"], now)
            .await
            .unwrap()
            .is_none()
    );
    drop(tx);
    let mut tx = store.begin(&a).await.unwrap();
    let p = session::pending(&mut tx, &pending, &["mfa_enable"], now)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(p.data["member"], "ann");
    // The wrong kind is not found either.
    assert!(
        session::pending(&mut tx, &pending, &["sso"], now)
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
#[ignore]
async fn idle_and_absolute_expiry() {
    let store = app_store().await;
    let t = tenant();
    let ann = member("ann");
    add_member(&store, &t, &ann, false).await;
    let t0 = OffsetDateTime::now_utc() - Duration::days(30);
    let hour = Duration::hours(1);

    // Idle: 12 hours without a request ends it.
    let idle = new_session(&store, &t, &ann, t0).await;
    let mut tx = store.begin(&t).await.unwrap();
    assert!(
        session::lookup(&mut tx, &idle, t0 + 11 * hour)
            .await
            .unwrap()
            .is_some()
    );
    assert!(
        session::lookup(&mut tx, &idle, t0 + 22 * hour)
            .await
            .unwrap()
            .is_some()
    );
    assert!(
        session::lookup(&mut tx, &idle, t0 + 35 * hour)
            .await
            .unwrap()
            .is_none()
    );
    tx.commit().await.unwrap();

    // Absolute: 7 days, however active.
    let active = new_session(&store, &t, &ann, t0).await;
    let mut tx = store.begin(&t).await.unwrap();
    for k in 1..=15 {
        let s = session::lookup(&mut tx, &active, t0 + 11 * k * hour)
            .await
            .unwrap();
        assert!(s.is_some(), "touch {k}");
    }
    assert!(
        session::lookup(&mut tx, &active, t0 + Duration::days(7))
            .await
            .unwrap()
            .is_none()
    );
    tx.commit().await.unwrap();

    // Over HTTP: a session last seen 13 hours ago is gone.
    let stale = new_session(&store, &t, &ann, OffsetDateTime::now_utc() - 13 * hour).await;
    let app = probe(state(store));
    let r = app
        .oneshot(get_with_cookie("/probe", &stale))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::UNAUTHORIZED);
}
