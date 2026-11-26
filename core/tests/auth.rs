//! The web sign-in API. Tests that touch the database need DATABASE_URL (a superuser) and
//! `--ignored`; WorkOS is always a fake, never the network.
mod common;
use async_trait::async_trait;
use axum::Router;
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::routing::{get, post};
use common::app_store;
use http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use mandate::api::auth::{
    AuthError, AuthProvider, AuthUser, Authenticated, Invitation, Membership, OrgRef, Secret, Step,
    TotpEnrollment,
};
use mandate::api::session::{self, Session};
use mandate::api::{self, ApiState, ratelimit};
use mandate::ids::{AgentId, MemberId, TenantId};
use mandate::store::Store;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::postgres::PgPoolOptions;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use time::{Duration, OffsetDateTime};
use tower::ServiceExt;
use uuid::Uuid;

const WEB: &str = "http://localhost:3000";

fn state(store: Store) -> Arc<ApiState> {
    auth_state(store, Arc::new(Fake::default()))
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
async fn cors_allows_event_stream_reconnects_and_exposes_retry_after() {
    let routes = Router::new()
        .route("/v1/conversations/{id}/events", get(|| async { "ok" }))
        .route("/limited", get(|| async { api::ApiError::rate_limited(7) }));
    let app = api::with_routes(routes, state(offline_store()));
    // An EventSource reconnect sends Last-Event-ID, which is not CORS-safelisted, so it is preflighted.
    let preflight = |origin: &str| {
        Request::options("/v1/conversations/00000000-0000-0000-0000-000000000000/events")
            .header(header::ORIGIN, origin)
            .header(header::ACCESS_CONTROL_REQUEST_METHOD, "GET")
            .header(header::ACCESS_CONTROL_REQUEST_HEADERS, "last-event-id")
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
    assert!(allowed.contains("last-event-id"), "{allowed}");

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
            Request::get("/limited")
                .header(header::ORIGIN, WEB)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(r.headers()[header::RETRY_AFTER], "7");
    let exposed = r.headers()[header::ACCESS_CONTROL_EXPOSE_HEADERS]
        .to_str()
        .unwrap()
        .to_owned();
    assert!(exposed.contains("retry-after"), "{exposed}");
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

/// A router with only the sign-in rate limit in front of an open route.
fn limited(state: Arc<ApiState>) -> Router {
    let routes = Router::new()
        .route("/v1/auth/probe", post(|| async { StatusCode::ACCEPTED }))
        .route_layer(axum::middleware::from_fn_with_state(
            state.clone(),
            ratelimit::limit,
        ));
    api::with_routes(routes, state)
}

fn random_ip() -> String {
    let b = Uuid::new_v4().into_bytes();
    format!("10.{}.{}.{}", b[0], b[1], b[2])
}

/// The core honours `CF-Connecting-IP` only from loopback, where the cloudflared sidecar is.
fn sidecar() -> ConnectInfo<SocketAddr> {
    ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 443)))
}

fn auth_post(uri: &str, ip: &str, body: Value) -> Request<Body> {
    Request::post(uri)
        .header("x-mandate", "1")
        .header(header::ORIGIN, WEB)
        .header(header::CONTENT_TYPE, "application/json")
        .header("cf-connecting-ip", ip)
        .extension(sidecar())
        .body(Body::from(body.to_string()))
        .unwrap()
}

/// Counting is per calendar minute: start a burst early in a minute so it fits in one window.
async fn fresh_minute() {
    let s = OffsetDateTime::now_utc().second();
    if s >= 40 {
        tokio::time::sleep(std::time::Duration::from_secs(u64::from(61 - s))).await;
    }
}

#[tokio::test]
#[ignore]
async fn sixth_attempt_per_email_is_429() {
    let app = limited(state(app_store().await));
    let email = format!("ann-{}@example.com", Uuid::new_v4().simple());
    fresh_minute().await;
    for i in 0..5 {
        // Case and spaces do not make a new subject.
        let typed = if i % 2 == 0 {
            email.to_uppercase()
        } else {
            format!(" {email}")
        };
        let r = app
            .clone()
            .oneshot(auth_post(
                "/v1/auth/probe",
                &random_ip(),
                json!({ "email": typed }),
            ))
            .await
            .unwrap();
        assert_eq!(r.status(), StatusCode::ACCEPTED, "attempt {}", i + 1);
    }
    let r = app
        .clone()
        .oneshot(auth_post(
            "/v1/auth/probe",
            &random_ip(),
            json!({ "email": email }),
        ))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::TOO_MANY_REQUESTS);
    let wait: i64 = r.headers()[header::RETRY_AFTER]
        .to_str()
        .unwrap()
        .parse()
        .unwrap();
    assert!((1..=60).contains(&wait), "{wait}");
    assert_eq!(body_json(r).await["error"]["code"], "rate_limited");
    // Another person is not held back.
    let other = format!("bob-{}@example.com", Uuid::new_v4().simple());
    let r = app
        .oneshot(auth_post(
            "/v1/auth/probe",
            &random_ip(),
            json!({ "email": other }),
        ))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::ACCEPTED);
}

#[tokio::test]
#[ignore]
async fn ip_limit_applies_across_emails() {
    let app = limited(state(app_store().await));
    let ip = random_ip();
    fresh_minute().await;
    for i in 0..30 {
        let email = format!("p{i}-{}@example.com", Uuid::new_v4().simple());
        let r = app
            .clone()
            .oneshot(auth_post("/v1/auth/probe", &ip, json!({ "email": email })))
            .await
            .unwrap();
        assert_eq!(r.status(), StatusCode::ACCEPTED, "attempt {}", i + 1);
    }
    let email = format!("late-{}@example.com", Uuid::new_v4().simple());
    let r = app
        .clone()
        .oneshot(auth_post("/v1/auth/probe", &ip, json!({ "email": email })))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::TOO_MANY_REQUESTS);
    assert!(r.headers().contains_key(header::RETRY_AFTER));
    // A pending step is a subject too; another IP is unaffected.
    let r = app
        .oneshot(auth_post(
            "/v1/auth/probe",
            &random_ip(),
            json!({ "pending": "abc" }),
        ))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::ACCEPTED);
}

/// The one code the fake accepts for every one-time code (email, TOTP).
const GOOD_CODE: &str = "123456";

#[derive(Clone)]
struct FakeUser {
    id: String,
    email: String,
    password: String,
    verified: bool,
    name: String,
    memberships: Vec<Membership>,
    totp: bool,
}

#[derive(Default)]
struct CreateRace {
    calls: std::sync::atomic::AtomicUsize,
    created: tokio::sync::Notify,
    release: tokio::sync::Notify,
}

/// WorkOS as the sign-in API sees it, in memory. Pending tokens look like `pat_<user>` and
/// must never reach the browser.
#[derive(Default)]
struct Fake {
    users: Mutex<Vec<FakeUser>>,
    /// The next `authenticate_*` call fails with this instead.
    forced: Mutex<Option<AuthError>>,
    calls: Mutex<Vec<String>>,
    sso: Mutex<HashMap<String, String>>,
    sso_codes: Mutex<HashMap<String, String>>,
    invitations: Mutex<HashMap<String, Invitation>>,
    orgs: Mutex<Vec<OrgRef>>,
    /// When set, `verify_email` waits here: lets a test line up two submits of one step.
    verify_barrier: Mutex<Option<Arc<tokio::sync::Barrier>>>,
    /// When set, `set_password` fails as WorkOS being down would.
    set_password_fails: Mutex<bool>,
    /// One-shot: the next `set_password` waits at the first barrier (arrived), then the second.
    set_password_gate: Mutex<Option<(Arc<tokio::sync::Barrier>, Arc<tokio::sync::Barrier>)>>,
    /// Lines up two `create_user` calls for one email: the first waits until the second has
    /// made the user; the second then waits for the first barrier's release.
    create_race: Mutex<Option<Arc<CreateRace>>>,
    /// One-shot: `create_user` makes the user, then answers with this error anyway.
    create_user_error: Mutex<Option<AuthError>>,
}

impl Fake {
    fn call(&self, c: impl Into<String>) {
        self.calls.lock().unwrap().push(c.into());
    }

    fn called(&self, prefix: &str) -> bool {
        self.calls
            .lock()
            .unwrap()
            .iter()
            .any(|c| c.starts_with(prefix))
    }

    fn force(&self, e: AuthError) {
        *self.forced.lock().unwrap() = Some(e);
    }

    fn by_email(&self, email: &str) -> Option<FakeUser> {
        let users = self.users.lock().unwrap();
        users
            .iter()
            .find(|u| u.email.eq_ignore_ascii_case(email))
            .cloned()
    }

    fn by_id(&self, id: &str) -> Option<FakeUser> {
        self.users
            .lock()
            .unwrap()
            .iter()
            .find(|u| u.id == id)
            .cloned()
    }

    fn update(&self, id: &str, f: impl FnOnce(&mut FakeUser)) {
        let mut users = self.users.lock().unwrap();
        if let Some(u) = users.iter_mut().find(|u| u.id == id) {
            f(u);
        }
    }

    fn user_of(&self, token: &str) -> Result<FakeUser, AuthError> {
        token
            .strip_prefix("pat_")
            .and_then(|id| self.by_id(id))
            .ok_or(AuthError::PendingExpired)
    }

    fn forced(&self) -> Result<(), AuthError> {
        match self.forced.lock().unwrap().take() {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }

    fn token(u: &FakeUser) -> Secret {
        Secret(format!("pat_{}", u.id))
    }

    fn auth_user(u: &FakeUser) -> AuthUser {
        AuthUser {
            id: u.id.clone(),
            email: u.email.clone(),
            email_verified: u.verified,
            name: Some(u.name.clone()),
            avatar_url: None,
        }
    }

    /// WorkOS's org rule: one active membership picks the org, several ask the user to choose.
    fn complete(u: &FakeUser) -> Result<Authenticated, AuthError> {
        let active: Vec<&Membership> = u.memberships.iter().filter(|m| m.active).collect();
        match active.as_slice() {
            [] => Ok(Authenticated {
                user: Self::auth_user(u),
                org: None,
            }),
            [one] => Ok(Authenticated {
                user: Self::auth_user(u),
                org: Some(one.org.clone()),
            }),
            many => Err(AuthError::Step(Step::ChooseOrg {
                token: Self::token(u),
                orgs: many
                    .iter()
                    .map(|m| OrgRef {
                        id: m.org.clone(),
                        name: m.org_name.clone(),
                    })
                    .collect(),
            })),
        }
    }

    /// Password sign-in: email verification, then two-step, then the org.
    fn password_flow(u: &FakeUser) -> Result<Authenticated, AuthError> {
        if !u.verified {
            return Err(AuthError::Step(Step::VerifyEmail {
                token: Self::token(u),
                email: u.email.clone(),
                user: Some(u.id.clone()),
            }));
        }
        if u.totp {
            return Err(AuthError::Step(Step::Mfa {
                token: Self::token(u),
                factor: format!("factor_{}", u.id),
            }));
        }
        Self::complete(u)
    }
}

#[async_trait]
impl AuthProvider for Fake {
    async fn authenticate_with_password(
        &self,
        email: &str,
        password: &str,
        invitation: Option<&str>,
    ) -> Result<Authenticated, AuthError> {
        self.call(format!("password {email} {invitation:?}"));
        self.forced()?;
        let u = self.by_email(email).filter(|u| u.password == password);
        let u = u.ok_or(AuthError::InvalidCredentials)?;
        if let Some(inv) = invitation.and_then(|t| self.invitations.lock().unwrap().get(t).cloned())
        {
            accept(self, &u, &inv);
            return Self::complete(&self.by_id(&u.id).unwrap());
        }
        Self::password_flow(&u)
    }

    async fn create_magic_auth(&self, email: &str) -> Result<(), AuthError> {
        self.call(format!("magic {email}"));
        self.by_email(email).map(|_| ()).ok_or(AuthError::NotFound)
    }

    async fn authenticate_with_magic_auth(
        &self,
        email: &str,
        code: &str,
        invitation: Option<&str>,
    ) -> Result<Authenticated, AuthError> {
        self.call(format!("magic-verify {email} {invitation:?}"));
        self.forced()?;
        let u = self.by_email(email).ok_or(AuthError::CodeInvalid)?;
        if code != GOOD_CODE {
            return Err(AuthError::CodeInvalid);
        }
        if let Some(inv) = invitation.and_then(|t| self.invitations.lock().unwrap().get(t).cloned())
        {
            accept(self, &u, &inv);
        }
        Self::complete(&self.by_id(&u.id).unwrap())
    }

    async fn authenticate_with_email_verification(
        &self,
        token: &str,
        code: &str,
    ) -> Result<Authenticated, AuthError> {
        self.call("email-verification");
        let u = self.user_of(token)?;
        if code != GOOD_CODE {
            return Err(AuthError::CodeInvalid);
        }
        self.update(&u.id, |u| u.verified = true);
        Self::password_flow(&self.by_id(&u.id).unwrap())
    }

    async fn authenticate_with_totp(
        &self,
        token: &str,
        challenge: &str,
        code: &str,
    ) -> Result<Authenticated, AuthError> {
        self.call(format!("totp {challenge}"));
        let u = self.user_of(token)?;
        if !challenge.starts_with("chal_") || code != GOOD_CODE {
            return Err(AuthError::CodeInvalid);
        }
        self.update(&u.id, |u| u.totp = true);
        Self::complete(&u)
    }

    async fn authenticate_with_organization_selection(
        &self,
        token: &str,
        org: &str,
    ) -> Result<Authenticated, AuthError> {
        self.call(format!("select-org {org}"));
        let u = self.user_of(token)?;
        if !u.memberships.iter().any(|m| m.active && m.org == org) {
            return Err(AuthError::NotFound);
        }
        Ok(Authenticated {
            user: Self::auth_user(&u),
            org: Some(org.to_owned()),
        })
    }

    async fn authenticate_with_code(&self, code: &str) -> Result<Authenticated, AuthError> {
        self.call("code");
        let id = self.sso_codes.lock().unwrap().get(code).cloned();
        let u = id
            .and_then(|id| self.by_id(&id))
            .ok_or(AuthError::InvalidCredentials)?;
        Self::complete(&u)
    }

    async fn sso_connection(&self, domain: &str) -> Result<Option<String>, AuthError> {
        Ok(self.sso.lock().unwrap().get(domain).cloned())
    }

    fn sso_authorization_url(
        &self,
        connection: &str,
        state: &str,
        redirect_uri: &str,
    ) -> Result<String, AuthError> {
        Ok(format!(
            "https://sso.test/authorize?connection={connection}&state={state}&redirect_uri={redirect_uri}"
        ))
    }

    async fn create_password_reset(&self, email: &str) -> Result<(), AuthError> {
        self.call(format!("reset {email}"));
        self.by_email(email).map(|_| ()).ok_or(AuthError::NotFound)
    }

    async fn confirm_password_reset(
        &self,
        token: &str,
        password: &str,
    ) -> Result<AuthUser, AuthError> {
        self.call(format!("reset-confirm {token}"));
        let u = token
            .strip_prefix("reset_")
            .and_then(|id| self.by_id(id))
            .ok_or(AuthError::NotFound)?;
        // WorkOS: a reset also verifies the email.
        self.update(&u.id, |u| {
            u.password = password.to_owned();
            u.verified = true;
        });
        Ok(Self::auth_user(&u))
    }

    async fn set_password(&self, user: &str, password: &str) -> Result<(), AuthError> {
        // The id only: a password never goes into the call log.
        self.call(format!("set-password {user}"));
        let gate = self.set_password_gate.lock().unwrap().take();
        if let Some((arrived, release)) = gate {
            arrived.wait().await;
            release.wait().await;
        }
        if *self.set_password_fails.lock().unwrap() {
            return Err(AuthError::Upstream("status 500".into()));
        }
        self.update(user, |u| u.password = password.to_owned());
        Ok(())
    }

    async fn invitation(&self, token: &str) -> Result<Invitation, AuthError> {
        self.invitations
            .lock()
            .unwrap()
            .get(token)
            .cloned()
            .ok_or(AuthError::NotFound)
    }

    async fn user(&self, id: &str) -> Result<AuthUser, AuthError> {
        self.by_id(id)
            .map(|u| Self::auth_user(&u))
            .ok_or(AuthError::NotFound)
    }

    async fn create_user(
        &self,
        email: &str,
        password: &str,
        name: &str,
    ) -> Result<AuthUser, AuthError> {
        self.call(format!("create-user {email}"));
        let race = self.create_race.lock().unwrap().clone();
        let second = match &race {
            Some(r) if r.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 => {
                r.created.notified().await;
                false
            }
            Some(_) => true,
            None => false,
        };
        if self.by_email(email).is_some() {
            return Err(AuthError::Conflict);
        }
        let u = FakeUser {
            id: format!("user_{}", Uuid::new_v4().simple()),
            email: email.to_owned(),
            password: password.to_owned(),
            verified: false,
            name: name.to_owned(),
            memberships: vec![],
            totp: false,
        };
        self.users.lock().unwrap().push(u.clone());
        if let Some(r) = race.filter(|_| second) {
            r.created.notify_one();
            r.release.notified().await;
        }
        let error = self.create_user_error.lock().unwrap().take();
        if let Some(e) = error {
            return Err(e);
        }
        Ok(Self::auth_user(&u))
    }

    async fn send_verification_email(&self, user: &str) -> Result<(), AuthError> {
        self.call(format!("send-verification {user}"));
        Ok(())
    }

    async fn verify_email(&self, user: &str, code: &str) -> Result<AuthUser, AuthError> {
        self.call(format!("verify-email {user}"));
        let barrier = self.verify_barrier.lock().unwrap().clone();
        if let Some(b) = barrier {
            b.wait().await;
        }
        if code != GOOD_CODE {
            return Err(AuthError::CodeInvalid);
        }
        self.update(user, |u| u.verified = true);
        self.by_id(user)
            .map(|u| Self::auth_user(&u))
            .ok_or(AuthError::NotFound)
    }

    async fn organization(&self, id: &str) -> Result<OrgRef, AuthError> {
        let orgs = self.orgs.lock().unwrap();
        orgs.iter()
            .find(|o| o.id == id)
            .cloned()
            .ok_or(AuthError::NotFound)
    }

    async fn create_organization(&self, name: &str) -> Result<OrgRef, AuthError> {
        self.call(format!("create-org {name}"));
        let org = OrgRef {
            id: format!("org_{}", Uuid::new_v4().simple()),
            name: name.to_owned(),
        };
        self.orgs.lock().unwrap().push(org.clone());
        Ok(org)
    }

    async fn create_membership(&self, user: &str, org: &str, role: &str) -> Result<(), AuthError> {
        self.call(format!("create-membership {org} {role}"));
        let name = self.organization(org).await?.name;
        self.update(user, |u| {
            u.memberships.push(Membership {
                org: org.to_owned(),
                org_name: name,
                role: role.to_owned(),
                active: true,
            })
        });
        Ok(())
    }

    async fn memberships(&self, user: &str) -> Result<Vec<Membership>, AuthError> {
        self.by_id(user)
            .map(|u| u.memberships)
            .ok_or(AuthError::NotFound)
    }

    async fn enroll_auth_factor(&self, user: &str) -> Result<TotpEnrollment, AuthError> {
        self.call(format!("enroll {user}"));
        Ok(TotpEnrollment {
            factor: format!("factor_{user}"),
            challenge: format!("chal_new_{user}"),
            secret: Secret("FAKESECRETFAKESE".into()),
            uri: Secret(
                "otpauth://totp/Mandate:test?secret=FAKESECRETFAKESE&issuer=Mandate".into(),
            ),
        })
    }

    async fn challenge_factor(&self, factor: &str) -> Result<String, AuthError> {
        Ok(format!("chal_{factor}"))
    }

    async fn verify_challenge(&self, challenge: &str, code: &str) -> Result<bool, AuthError> {
        self.call(format!("verify-challenge {challenge}"));
        let user = challenge
            .strip_prefix("chal_new_")
            .ok_or(AuthError::NotFound)?;
        let ok = code == GOOD_CODE;
        if ok {
            self.update(user, |u| u.totp = true);
        }
        Ok(ok)
    }

    async fn has_totp(&self, user: &str) -> Result<bool, AuthError> {
        Ok(self.by_id(user).is_some_and(|u| u.totp))
    }
}

/// What accepting an invitation does in WorkOS: the user joins the org, email verified.
fn accept(fake: &Fake, u: &FakeUser, inv: &Invitation) {
    let org = inv.org.clone().unwrap();
    let name = fake
        .orgs
        .lock()
        .unwrap()
        .iter()
        .find(|o| o.id == org)
        .unwrap()
        .name
        .clone();
    fake.update(&u.id, |u| {
        u.verified = true;
        u.memberships.push(Membership {
            org,
            org_name: name,
            role: "member".into(),
            active: true,
        });
    });
}

struct Harness {
    app: Router,
    fake: Arc<Fake>,
    store: Store,
    ip: String,
}

impl Harness {
    async fn new() -> Harness {
        let store = app_store().await;
        let fake = Arc::new(Fake::default());
        let app = api::router(auth_state(store.clone(), fake.clone()));
        Harness {
            app,
            fake,
            store,
            ip: random_ip(),
        }
    }

    /// A user in one new org (created in the fake), with role `role`.
    fn user(&self, role: &str) -> FakeUser {
        let org = OrgRef {
            id: format!("org_{}", Uuid::new_v4().simple()),
            name: "Acme".into(),
        };
        self.fake.orgs.lock().unwrap().push(org.clone());
        let u = FakeUser {
            id: format!("user_{}", Uuid::new_v4().simple()),
            email: format!("ann-{}@acme.test", Uuid::new_v4().simple()),
            password: "correct horse".into(),
            verified: true,
            name: "Ann Lee".into(),
            memberships: vec![Membership {
                org: org.id,
                org_name: org.name,
                role: role.into(),
                active: true,
            }],
            totp: false,
        };
        self.fake.users.lock().unwrap().push(u.clone());
        u
    }

    async fn post(&self, uri: &str, body: Value) -> axum::response::Response {
        self.app
            .clone()
            .oneshot(auth_post(uri, &self.ip, body))
            .await
            .unwrap()
    }

    async fn get(&self, uri: &str, cookie: Option<&str>) -> axum::response::Response {
        let mut r = Request::get(uri)
            .header("cf-connecting-ip", &self.ip)
            .extension(sidecar());
        if let Some(c) = cookie {
            r = r.header(header::COOKIE, format!("mandate_session={c}"));
        }
        self.app
            .clone()
            .oneshot(r.body(Body::empty()).unwrap())
            .await
            .unwrap()
    }
}

fn auth_state(store: Store, auth: Arc<dyn AuthProvider>) -> Arc<ApiState> {
    Arc::new(ApiState {
        store,
        auth,
        web_origin: WEB.into(),
        sso_redirect_uri: "http://localhost:8080/v1/auth/sso/callback".into(),
        // Each test sends its own `CF-Connecting-IP`, as Cloudflare would, so buckets stay apart.
        trust_cf_ip: true,
        catalog: mandate::tools::Catalog::new(vec![]),
    })
}

/// The session cookie a response set, if any.
fn session_cookie(r: &axum::response::Response) -> Option<String> {
    r.headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .find_map(|v| v.strip_prefix("mandate_session="))
        .and_then(|v| v.split(';').next())
        .filter(|v| !v.is_empty())
        .map(str::to_owned)
}

/// Nothing WorkOS-internal may reach the browser: no pending token, no TOTP challenge id.
fn assert_no_workos_token(v: &Value) {
    let s = v.to_string();
    assert!(!s.contains("pat_"), "{s}");
}

/// The id `ensure_personal_agent` gives `m`'s operator: `op-` + 16 bytes of sha256(m), hex.
fn personal_operator(m: &MemberId) -> AgentId {
    let hex: String = Sha256::digest(m.as_str().as_bytes())[..16]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    AgentId::parse(&format!("op-{hex}")).unwrap()
}

fn tenant_of(org: &str) -> TenantId {
    TenantId::parse(&org.to_lowercase()).unwrap()
}

#[tokio::test]
#[ignore]
async fn password_sign_in_signs_in() {
    let h = Harness::new().await;
    let u = h.user("admin");
    let org = u.memberships[0].org.clone();
    let r = h
        .post(
            "/v1/auth/password",
            json!({ "email": u.email, "password": u.password }),
        )
        .await;
    assert_eq!(r.status(), StatusCode::OK);
    let set = r.headers()[header::SET_COOKIE].to_str().unwrap().to_owned();
    for want in ["HttpOnly", "Secure", "SameSite=Lax", "Path=/"] {
        assert!(
            set.split("; ").any(|a| a == want),
            "{want} missing from {set}"
        );
    }
    let cookie = session_cookie(&r).unwrap();
    let v = body_json(r).await;
    assert_eq!(v["status"], "signed_in");
    let me = &v["me"];
    let tenant = tenant_of(&org);
    assert_eq!(me["org"]["id"], tenant.as_str());
    assert_eq!(me["org"]["name"], "Acme");
    assert_eq!(me["person"]["email"], u.email.as_str());
    assert_eq!(me["person"]["name"], "Ann Lee");
    assert_eq!(me["is_admin"], true);
    assert_eq!(me["mfa_enabled"], false);
    assert_eq!(me["org"]["summary"]["people"], 1);
    assert_eq!(
        me["org"]["summary"]["agents"], 1,
        "a personal operator was created"
    );
    assert_no_workos_token(&v);

    // The cookie is a live session in that tenant, for the member linked to the WorkOS user.
    let mut tx = h.store.begin(&tenant).await.unwrap();
    let s = session::lookup(&mut tx, &cookie, OffsetDateTime::now_utc())
        .await
        .unwrap()
        .unwrap();
    assert!(s.is_admin);
    let linked = mandate::store::member_for_workos_user(&mut tx, &u.id)
        .await
        .unwrap();
    assert_eq!(linked.as_ref(), Some(&s.member));
    tx.commit().await.unwrap();

    // Admin follows the WorkOS role at the next sign-in; no second member or operator.
    h.fake
        .update(&u.id, |u| u.memberships[0].role = "member".into());
    let r = h
        .post(
            "/v1/auth/password",
            json!({ "email": u.email, "password": u.password }),
        )
        .await;
    let v = body_json(r).await;
    assert_eq!(v["me"]["is_admin"], false);
    assert_eq!(v["me"]["org"]["summary"]["people"], 1);
    assert_eq!(v["me"]["org"]["summary"]["agents"], 1);
    let mut tx = h.store.begin(&tenant).await.unwrap();
    let s = session::lookup(&mut tx, &cookie, OffsetDateTime::now_utc())
        .await
        .unwrap()
        .unwrap();
    assert!(!s.is_admin, "the old session follows the role too");
}

#[tokio::test]
#[ignore]
async fn wrong_password_is_invalid_credentials() {
    let h = Harness::new().await;
    let u = h.user("member");
    let r = h
        .post(
            "/v1/auth/password",
            json!({ "email": u.email, "password": "wrong" }),
        )
        .await;
    assert_eq!(r.status(), StatusCode::UNAUTHORIZED);
    assert!(session_cookie(&r).is_none());
    assert_eq!(body_json(r).await["error"]["code"], "invalid_credentials");
    // Malformed input never reaches WorkOS, and the error is ours, not serde's.
    for bad in [
        json!({ "email": "not-an-email", "password": "x" }),
        json!({ "email": u.email }),
        json!({ "email": u.email, "password": "" }),
        json!({ "email": u.email, "password": "x", "extra": 1 }),
    ] {
        let r = h.post("/v1/auth/password", bad.clone()).await;
        assert_eq!(r.status(), StatusCode::BAD_REQUEST, "{bad}");
        let v = body_json(r).await;
        assert_eq!(v["error"]["code"], "invalid_input");
        assert!(!v.to_string().contains("not-an-email"), "{v}");
    }
    // State-changing sign-in routes need the CSRF header too.
    let r = h
        .app
        .clone()
        .oneshot(
            Request::post("/v1/auth/password")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({ "email": u.email, "password": u.password }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
#[ignore]
async fn mfa_required_returns_next_mfa() {
    let h = Harness::new().await;
    let u = h.user("member");
    h.fake.update(&u.id, |u| u.totp = true);
    let r = h
        .post(
            "/v1/auth/password",
            json!({ "email": u.email, "password": u.password }),
        )
        .await;
    assert_eq!(r.status(), StatusCode::OK);
    assert!(session_cookie(&r).is_none());
    let v = body_json(r).await;
    assert_eq!(v["status"], "next");
    assert_eq!(v["next"], "mfa");
    assert!(v["pending"].as_str().is_some_and(|p| p.len() == 43), "{v}");
    assert_no_workos_token(&v);
}

#[tokio::test]
#[ignore]
async fn org_selection_returns_choose_org_with_orgs() {
    let h = Harness::new().await;
    let u = h.user("member");
    let second = OrgRef {
        id: format!("org_{}", Uuid::new_v4().simple()),
        name: "Beta".into(),
    };
    h.fake.orgs.lock().unwrap().push(second.clone());
    h.fake.update(&u.id, |u| {
        u.memberships.push(Membership {
            org: second.id.clone(),
            org_name: "Beta".into(),
            role: "admin".into(),
            active: true,
        })
    });
    let r = h
        .post(
            "/v1/auth/password",
            json!({ "email": u.email, "password": u.password }),
        )
        .await;
    let v = body_json(r).await;
    assert_eq!(v["next"], "choose_org", "{v}");
    let orgs = v["orgs"].as_array().unwrap();
    assert_eq!(orgs.len(), 2);
    assert_eq!(orgs[1]["id"], tenant_of(&second.id).as_str());
    assert_eq!(orgs[1]["name"], "Beta");
    assert_eq!(orgs[1]["summary"]["people"], 0);
    assert_no_workos_token(&v);
}

#[tokio::test]
#[ignore]
async fn email_unverified_returns_verify_email() {
    let h = Harness::new().await;
    let u = h.user("member");
    h.fake.update(&u.id, |u| u.verified = false);
    let r = h
        .post(
            "/v1/auth/password",
            json!({ "email": u.email, "password": u.password }),
        )
        .await;
    let v = body_json(r).await;
    assert_eq!(v["status"], "next");
    assert_eq!(v["next"], "verify_email");
    assert!(v["pending"].is_string());
    assert_no_workos_token(&v);
}

#[tokio::test]
#[ignore]
async fn sso_required_returns_redirect() {
    let h = Harness::new().await;
    let u = h.user("member");
    // WorkOS's sso_required body has no pending token.
    h.fake.force(AuthError::Step(Step::Sso {
        token: None,
        connection: "conn_1".into(),
    }));
    let r = h
        .post(
            "/v1/auth/password",
            json!({ "email": u.email, "password": u.password }),
        )
        .await;
    let set: Vec<String> = r
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .map(|v| v.to_str().unwrap().to_owned())
        .collect();
    let v = body_json(r).await;
    assert_eq!(v["next"], "sso_required", "{v}");
    let pending = v["pending"].as_str().unwrap();
    let url = v["redirect_url"].as_str().unwrap();
    assert!(url.contains("connection=conn_1"), "{url}");
    assert!(url.contains(&format!("state={pending}")), "{url}");
    // The state is bound to this browser.
    assert!(
        set.iter()
            .any(|c| c.starts_with(&format!("mandate_sso={pending};")) && c.contains("HttpOnly")),
        "{set:?}"
    );
    assert_no_workos_token(&v);
}

#[tokio::test]
#[ignore]
async fn method_required_lists_methods() {
    let h = Harness::new().await;
    let u = h.user("member");
    h.fake.force(AuthError::Step(Step::Methods {
        token: Secret("pat_x".into()),
        methods: vec!["google_oauth".into(), "magic_auth".into()],
    }));
    let r = h
        .post(
            "/v1/auth/password",
            json!({ "email": u.email, "password": u.password }),
        )
        .await;
    let v = body_json(r).await;
    assert_eq!(v["next"], "method_required", "{v}");
    assert_eq!(v["methods"], json!(["google_oauth", "magic_auth"]));
    assert_no_workos_token(&v);
}

#[tokio::test]
#[ignore]
async fn code_start_is_202_for_unknown_email() {
    let h = Harness::new().await;
    let u = h.user("member");
    let known = h
        .post("/v1/auth/code/start", json!({ "email": u.email }))
        .await;
    let unknown = h
        .post(
            "/v1/auth/code/start",
            json!({ "email": format!("nobody-{}@acme.test", Uuid::new_v4().simple()) }),
        )
        .await;
    assert_eq!(known.status(), StatusCode::ACCEPTED);
    assert_eq!(unknown.status(), StatusCode::ACCEPTED);
    let (k, n) = (
        known.into_body().collect().await.unwrap().to_bytes(),
        unknown.into_body().collect().await.unwrap().to_bytes(),
    );
    assert_eq!(k, n, "the answer is the same either way");
    assert!(h.fake.called(&format!("magic {}", u.email)));
}

#[tokio::test]
#[ignore]
async fn code_verify_signs_in() {
    let h = Harness::new().await;
    let u = h.user("member");
    let r = h
        .post(
            "/v1/auth/code/verify",
            json!({ "email": u.email, "code": "999999" }),
        )
        .await;
    assert_eq!(r.status(), StatusCode::BAD_REQUEST);
    assert_eq!(body_json(r).await["error"]["code"], "code_invalid");
    let r = h
        .post(
            "/v1/auth/code/verify",
            json!({ "email": u.email, "code": GOOD_CODE }),
        )
        .await;
    assert_eq!(r.status(), StatusCode::OK);
    assert!(session_cookie(&r).is_some());
    let v = body_json(r).await;
    assert_eq!(v["status"], "signed_in");
    assert_eq!(v["me"]["is_admin"], false);
}

/// `(type, member, actor_kind, actor_member)` of the tenant's lifecycle events, each with its
/// outbox row, in publish order.
async fn lifecycle(
    store: &Store,
    t: &TenantId,
) -> Vec<(String, Option<String>, String, Option<String>)> {
    let mut tx = store.begin(t).await.unwrap();
    sqlx::query_as(
        "select l.type, l.member, l.actor_kind, l.actor_member from lifecycle_log l \
         join outbox o on o.tenant_id = l.tenant_id and o.id = l.event_id and o.type = l.type \
         order by o.seq",
    )
    .fetch_all(&mut *tx)
    .await
    .unwrap()
}

fn sign_up_body(email: &str) -> Value {
    json!({ "email": email, "password": "a long passphrase", "name": "Ann Lee", "org_name": "Acme Robotics" })
}

#[tokio::test]
#[ignore]
async fn sign_up_answers_next_verify_email() {
    let h = Harness::new().await;
    let email = format!("new-{}@acme.test", Uuid::new_v4().simple());
    let r = h.post("/v1/auth/sign-up", sign_up_body(&email)).await;
    assert_eq!(r.status(), StatusCode::ACCEPTED);
    assert!(session_cookie(&r).is_none());
    let v = body_json(r).await;
    assert_eq!(v["status"], "next");
    assert_eq!(v["next"], "verify_email");
    assert!(v["pending"].as_str().is_some_and(|p| p.len() == 43));
    let user = h.fake.by_email(&email).unwrap();
    assert!(h.fake.called(&format!("send-verification {}", user.id)));
    // An existing account gets exactly the same answer, and no second user or email.
    let existing = h.user("member");
    let r = h
        .post("/v1/auth/sign-up", sign_up_body(&existing.email))
        .await;
    assert_eq!(r.status(), StatusCode::ACCEPTED);
    let w = body_json(r).await;
    assert_eq!(w["status"], v["status"]);
    assert_eq!(w["next"], v["next"]);
    assert!(w["pending"].as_str().is_some_and(|p| p.len() == 43));
    assert!(!h.fake.called(&format!("send-verification {}", existing.id)));
    // Too short a password is refused for every email alike, before WorkOS.
    let mut short = sign_up_body(&format!("x-{}@acme.test", Uuid::new_v4().simple()));
    short["password"] = json!("short");
    let r = h.post("/v1/auth/sign-up", short).await;
    assert_eq!(body_json(r).await["error"]["code"], "invalid_input");
}

#[tokio::test]
#[ignore]
async fn verify_email_creates_org_and_admin_member() {
    let h = Harness::new().await;
    let email = format!("new-{}@acme.test", Uuid::new_v4().simple());
    let v = body_json(h.post("/v1/auth/sign-up", sign_up_body(&email)).await).await;
    let pending = v["pending"].as_str().unwrap().to_owned();
    // A wrong code keeps the step.
    let r = h
        .post(
            "/v1/auth/verify-email",
            json!({ "pending": pending, "code": "000000" }),
        )
        .await;
    assert_eq!(body_json(r).await["error"]["code"], "code_invalid");
    let r = h
        .post(
            "/v1/auth/verify-email",
            json!({ "pending": pending, "code": GOOD_CODE }),
        )
        .await;
    assert_eq!(r.status(), StatusCode::OK);
    assert!(session_cookie(&r).is_some());
    let v = body_json(r).await;
    assert_eq!(v["status"], "signed_in", "{v}");
    assert_eq!(v["me"]["is_admin"], true);
    assert_eq!(v["me"]["org"]["name"], "Acme Robotics");
    assert_eq!(
        v["me"]["org"]["summary"],
        json!({ "people": 1, "agents": 1 })
    );
    // WorkOS: an org with that name, the person its admin.
    let user = h.fake.by_email(&email).unwrap();
    assert!(user.verified);
    let m = &user.memberships[0];
    assert_eq!(
        (m.org_name.as_str(), m.role.as_str()),
        ("Acme Robotics", "admin")
    );
    assert!(h.fake.called("create-org Acme Robotics"));
    // Mandate: a tenant for that org, an admin member linked to the user, a personal operator.
    let tenant = h.store.tenant_for_org(&m.org).await.unwrap().unwrap();
    assert_eq!(v["me"]["org"]["id"], tenant.as_str());
    let mut tx = h.store.begin(&tenant).await.unwrap();
    let member = mandate::store::member_for_workos_user(&mut tx, &user.id)
        .await
        .unwrap()
        .unwrap();
    let agent = personal_operator(&member);
    assert!(
        mandate::store::may_use_agent(&mut tx, &member, &agent)
            .await
            .unwrap()
    );
    drop(tx);
    // The step is used up.
    let r = h
        .post(
            "/v1/auth/verify-email",
            json!({ "pending": pending, "code": GOOD_CODE }),
        )
        .await;
    assert_eq!(body_json(r).await["error"]["code"], "pending_expired");
    // A decoy step (existing email) never verifies.
    let existing = h.user("member");
    let v = body_json(
        h.post("/v1/auth/sign-up", sign_up_body(&existing.email))
            .await,
    )
    .await;
    let r = h
        .post(
            "/v1/auth/verify-email",
            json!({ "pending": v["pending"], "code": GOOD_CODE }),
        )
        .await;
    assert_eq!(body_json(r).await["error"]["code"], "code_invalid");
}

#[tokio::test]
#[ignore]
async fn sign_up_step_is_spent_once() {
    let h = Harness::new().await;
    let email = format!("new-{}@acme.test", Uuid::new_v4().simple());
    let v = body_json(h.post("/v1/auth/sign-up", sign_up_body(&email)).await).await;
    // A double submit: both codes check out at WorkOS before either finishes.
    *h.fake.verify_barrier.lock().unwrap() = Some(Arc::new(tokio::sync::Barrier::new(2)));
    let body = json!({ "pending": v["pending"], "code": GOOD_CODE });
    let (a, b) = tokio::join!(
        h.post("/v1/auth/verify-email", body.clone()),
        h.post("/v1/auth/verify-email", body)
    );
    let mut codes = [a.status(), b.status()];
    codes.sort();
    assert_eq!(codes, [StatusCode::OK, StatusCode::BAD_REQUEST]);
    let orgs = h
        .fake
        .calls
        .lock()
        .unwrap()
        .iter()
        .filter(|c| c.starts_with("create-org"))
        .count();
    assert_eq!(orgs, 1, "one organization");
}

#[tokio::test]
#[ignore]
async fn resend_is_202() {
    let h = Harness::new().await;
    let email = format!("new-{}@acme.test", Uuid::new_v4().simple());
    let v = body_json(h.post("/v1/auth/sign-up", sign_up_body(&email)).await).await;
    let user = h.fake.by_email(&email).unwrap();
    h.fake.calls.lock().unwrap().clear();
    let r = h
        .post(
            "/v1/auth/verify-email/resend",
            json!({ "pending": v["pending"] }),
        )
        .await;
    assert_eq!(r.status(), StatusCode::ACCEPTED);
    assert!(h.fake.called(&format!("send-verification {}", user.id)));
    // A decoy answers the same.
    let existing = h.user("member");
    let w = body_json(
        h.post("/v1/auth/sign-up", sign_up_body(&existing.email))
            .await,
    )
    .await;
    let r = h
        .post(
            "/v1/auth/verify-email/resend",
            json!({ "pending": w["pending"] }),
        )
        .await;
    assert_eq!(r.status(), StatusCode::ACCEPTED);
}

#[tokio::test]
#[ignore]
async fn sign_up_records_org_and_member_created() {
    let h = Harness::new().await;
    let email = format!("new-{}@acme.test", Uuid::new_v4().simple());
    let v = body_json(h.post("/v1/auth/sign-up", sign_up_body(&email)).await).await;
    let r = h
        .post(
            "/v1/auth/verify-email",
            json!({ "pending": v["pending"], "code": GOOD_CODE }),
        )
        .await;
    let me = body_json(r).await["me"].clone();
    let member = me["person"]["id"].as_str().unwrap().to_owned();
    let tenant = TenantId::parse(me["org"]["id"].as_str().unwrap()).unwrap();
    let by_member = ("member".to_owned(), Some(member.clone()));
    assert_eq!(
        lifecycle(&h.store, &tenant).await,
        vec![
            (
                "organization.created.v1".into(),
                None,
                by_member.0.clone(),
                by_member.1.clone()
            ),
            (
                "member.created.v1".into(),
                Some(member),
                by_member.0,
                by_member.1
            ),
        ]
    );
}

#[tokio::test]
#[ignore]
async fn first_sign_in_records_member_created() {
    let h = Harness::new().await;
    let u = h.user("member");
    let tenant = tenant_of(&u.memberships[0].org);
    let body = json!({ "email": u.email, "password": u.password });
    let r = h.post("/v1/auth/password", body.clone()).await;
    let member = body_json(r).await["me"]["person"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let created = vec![(
        "member.created.v1".to_owned(),
        Some(member),
        "system".to_owned(),
        None,
    )];
    assert_eq!(lifecycle(&h.store, &tenant).await, created);
    // The second sign-in creates nobody.
    h.post("/v1/auth/password", body).await;
    assert_eq!(lifecycle(&h.store, &tenant).await, created);
}

/// Signs `u` in by password and returns the session cookie.
async fn signed_in(h: &Harness, u: &FakeUser) -> String {
    let r = h
        .post(
            "/v1/auth/password",
            json!({ "email": u.email, "password": u.password }),
        )
        .await;
    assert_eq!(r.status(), StatusCode::OK);
    session_cookie(&r).unwrap()
}

fn with_cookie(mut r: Request<Body>, cookie: &str) -> Request<Body> {
    r.headers_mut().insert(
        header::COOKIE,
        http::HeaderValue::from_str(&format!("mandate_session={cookie}")).unwrap(),
    );
    r
}

async fn pending_for(h: &Harness, u: &FakeUser, want: &str) -> String {
    let r = h
        .post(
            "/v1/auth/password",
            json!({ "email": u.email, "password": u.password }),
        )
        .await;
    let v = body_json(r).await;
    assert_eq!(v["next"], want, "{v}");
    v["pending"].as_str().unwrap().to_owned()
}

#[tokio::test]
#[ignore]
async fn mfa_verify_signs_in() {
    let h = Harness::new().await;
    let u = h.user("member");
    h.fake.update(&u.id, |u| u.totp = true);
    let pending = pending_for(&h, &u, "mfa").await;
    let r = h
        .post(
            "/v1/auth/mfa/verify",
            json!({ "pending": pending, "code": GOOD_CODE }),
        )
        .await;
    assert_eq!(r.status(), StatusCode::OK);
    assert!(session_cookie(&r).is_some());
    let v = body_json(r).await;
    assert_eq!(v["status"], "signed_in");
    assert_eq!(v["me"]["mfa_enabled"], true);
    // WorkOS got its own challenge for the user's factor, kept server-side.
    assert!(h.fake.called(&format!("totp chal_factor_{}", u.id)));
    let r = h
        .post(
            "/v1/auth/mfa/verify",
            json!({ "pending": pending, "code": GOOD_CODE }),
        )
        .await;
    assert_eq!(body_json(r).await["error"]["code"], "pending_expired");
}

#[tokio::test]
#[ignore]
async fn bad_totp_is_code_invalid() {
    let h = Harness::new().await;
    let u = h.user("member");
    h.fake.update(&u.id, |u| u.totp = true);
    let pending = pending_for(&h, &u, "mfa").await;
    let r = h
        .post(
            "/v1/auth/mfa/verify",
            json!({ "pending": pending, "code": "111111" }),
        )
        .await;
    assert_eq!(r.status(), StatusCode::BAD_REQUEST);
    assert!(session_cookie(&r).is_none());
    assert_eq!(body_json(r).await["error"]["code"], "code_invalid");
    // The step survives a typo.
    let r = h
        .post(
            "/v1/auth/mfa/verify",
            json!({ "pending": pending, "code": GOOD_CODE }),
        )
        .await;
    assert_eq!(r.status(), StatusCode::OK);
}

#[tokio::test]
#[ignore]
async fn mfa_setup_returns_qr_and_secret() {
    let h = Harness::new().await;
    let u = h.user("member");
    h.fake.force(AuthError::Step(Step::MfaSetup {
        token: Secret(format!("pat_{}", u.id)),
        user: u.id.clone(),
    }));
    let pending = pending_for(&h, &u, "mfa_setup").await;
    let r = h
        .post("/v1/auth/mfa/setup", json!({ "pending": pending }))
        .await;
    assert_eq!(r.status(), StatusCode::OK);
    let v = body_json(r).await;
    assert!(
        v["qr_svg"].as_str().is_some_and(|s| s.contains("<svg")),
        "{v}"
    );
    assert_eq!(v["secret"], "FAKESECRETFAKESE");
    let challenge = v["challenge"].as_str().unwrap();
    assert_eq!(challenge.len(), 43, "our own id, not WorkOS's challenge");
    assert!(!v.to_string().contains("chal_"), "{v}");
    // The setup step is used up.
    let r = h
        .post("/v1/auth/mfa/setup", json!({ "pending": pending }))
        .await;
    assert_eq!(body_json(r).await["error"]["code"], "pending_expired");

    // Signed in, without a step.
    let other = h.user("member");
    let cookie = signed_in(&h, &other).await;
    let r = h
        .app
        .clone()
        .oneshot(with_cookie(
            auth_post("/v1/auth/mfa/setup", &h.ip, json!({})),
            &cookie,
        ))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::OK);
    assert!(h.fake.called(&format!("enroll {}", other.id)));
    // Neither a step nor a session: refused.
    let r = h.post("/v1/auth/mfa/setup", json!({})).await;
    assert_eq!(r.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
#[ignore]
async fn mfa_enable_with_valid_code_signs_in() {
    let h = Harness::new().await;
    let u = h.user("member");
    h.fake.force(AuthError::Step(Step::MfaSetup {
        token: Secret(format!("pat_{}", u.id)),
        user: u.id.clone(),
    }));
    let pending = pending_for(&h, &u, "mfa_setup").await;
    let v = body_json(
        h.post("/v1/auth/mfa/setup", json!({ "pending": pending }))
            .await,
    )
    .await;
    let challenge = v["challenge"].as_str().unwrap().to_owned();
    let r = h
        .post(
            "/v1/auth/mfa/enable",
            json!({ "pending": pending, "challenge": challenge, "code": GOOD_CODE }),
        )
        .await;
    assert_eq!(r.status(), StatusCode::OK);
    assert!(session_cookie(&r).is_some());
    assert_eq!(body_json(r).await["me"]["mfa_enabled"], true);

    // Signed in: 204, and `me` now says two-step is on.
    let other = h.user("member");
    let cookie = signed_in(&h, &other).await;
    let send = |uri: &str, body: Value, cookie: Option<&str>| {
        let r = auth_post(uri, &h.ip, body);
        let r = match cookie {
            Some(c) => with_cookie(r, c),
            None => r,
        };
        h.app.clone().oneshot(r)
    };
    let v = body_json(
        send("/v1/auth/mfa/setup", json!({}), Some(&cookie))
            .await
            .unwrap(),
    )
    .await;
    let challenge = v["challenge"].as_str().unwrap().to_owned();
    // Without the session, a signed-in challenge is invisible.
    let r = send(
        "/v1/auth/mfa/enable",
        json!({ "pending": "x", "challenge": challenge, "code": GOOD_CODE }),
        None,
    )
    .await
    .unwrap();
    assert_eq!(body_json(r).await["error"]["code"], "pending_expired");
    let r = send(
        "/v1/auth/mfa/enable",
        json!({ "challenge": challenge, "code": "111111" }),
        Some(&cookie),
    )
    .await
    .unwrap();
    assert_eq!(body_json(r).await["error"]["code"], "code_invalid");
    let r = send(
        "/v1/auth/mfa/enable",
        json!({ "challenge": challenge, "code": GOOD_CODE }),
        Some(&cookie),
    )
    .await
    .unwrap();
    assert_eq!(r.status(), StatusCode::NO_CONTENT);
    assert!(h.fake.has_totp(&other.id).await.unwrap());
    let tenant = tenant_of(&other.memberships[0].org);
    let mut tx = h.store.begin(&tenant).await.unwrap();
    let profile = session::profile(&mut tx, &cookie).await.unwrap().unwrap();
    assert_eq!(profile["mfa_enabled"], true);
}

#[tokio::test]
#[ignore]
async fn choose_org_signs_into_that_org() {
    let h = Harness::new().await;
    let u = h.user("member");
    let beta = OrgRef {
        id: format!("org_{}", Uuid::new_v4().simple()),
        name: "Beta".into(),
    };
    h.fake.orgs.lock().unwrap().push(beta.clone());
    h.fake.update(&u.id, |u| {
        u.memberships.push(Membership {
            org: beta.id.clone(),
            org_name: "Beta".into(),
            role: "admin".into(),
            active: true,
        })
    });
    let pending = pending_for(&h, &u, "choose_org").await;
    // An org that was not offered is refused, and WorkOS is not asked.
    let r = h
        .post(
            "/v1/auth/choose-org",
            json!({ "pending": pending, "org": "org_other" }),
        )
        .await;
    assert_eq!(body_json(r).await["error"]["code"], "no_access");
    let beta_tenant = tenant_of(&beta.id);
    let r = h
        .post(
            "/v1/auth/choose-org",
            json!({ "pending": pending, "org": beta_tenant }),
        )
        .await;
    assert_eq!(r.status(), StatusCode::OK);
    let cookie = session_cookie(&r).unwrap();
    let v = body_json(r).await;
    assert_eq!(v["me"]["org"]["id"], beta_tenant.as_str());
    assert_eq!(v["me"]["org"]["name"], "Beta");
    assert_eq!(v["me"]["is_admin"], true);
    assert_eq!(v["me"]["orgs"].as_array().unwrap().len(), 2);
    assert!(cookie.starts_with(&format!("{beta_tenant}.")));
    assert!(h.fake.called(&format!("select-org {}", beta.id)));
}

#[tokio::test]
#[ignore]
async fn sso_start_returns_redirect() {
    let h = Harness::new().await;
    h.fake
        .sso
        .lock()
        .unwrap()
        .insert("bigcorp.test".into(), "conn_big".into());
    let r = h
        .post("/v1/auth/sso/start", json!({ "email": "Ann@BigCorp.test" }))
        .await;
    assert_eq!(r.status(), StatusCode::OK);
    let cookie = r.headers()[header::SET_COOKIE].to_str().unwrap().to_owned();
    let v = body_json(r).await;
    let url = v["redirect_url"].as_str().unwrap();
    assert!(url.contains("connection=conn_big"), "{url}");
    let state = url
        .split("state=")
        .nth(1)
        .unwrap()
        .split('&')
        .next()
        .unwrap();
    assert_eq!(state.len(), 43);
    assert!(
        cookie.starts_with(&format!("mandate_sso={state};")),
        "{cookie}"
    );
    for want in ["HttpOnly", "Secure", "SameSite=Lax"] {
        assert!(
            cookie.split("; ").any(|a| a == want),
            "{want} missing from {cookie}"
        );
    }
}

#[tokio::test]
#[ignore]
async fn sso_start_404_without_sso() {
    let h = Harness::new().await;
    let r = h
        .post("/v1/auth/sso/start", json!({ "email": "ann@nosso.test" }))
        .await;
    assert_eq!(r.status(), StatusCode::NOT_FOUND);
    assert!(r.headers().get(header::SET_COOKIE).is_none());
    assert_eq!(body_json(r).await["error"]["code"], "not_found");
}

/// Starts SSO for a user of `bigcorp.test` and returns the state.
async fn sso_state(h: &Harness, u: &FakeUser, code: &str) -> String {
    h.fake
        .sso
        .lock()
        .unwrap()
        .insert("acme.test".into(), "conn_acme".into());
    h.fake
        .sso_codes
        .lock()
        .unwrap()
        .insert(code.into(), u.id.clone());
    let v = body_json(
        h.post("/v1/auth/sso/start", json!({ "email": u.email }))
            .await,
    )
    .await;
    let url = v["redirect_url"].as_str().unwrap().to_owned();
    url.split("state=")
        .nth(1)
        .unwrap()
        .split('&')
        .next()
        .unwrap()
        .to_owned()
}

fn callback(query: &str, sso_cookie: Option<&str>) -> Request<Body> {
    let mut r = Request::get(format!("/v1/auth/sso/callback?{query}"));
    if let Some(c) = sso_cookie {
        r = r.header(header::COOKIE, format!("mandate_sso={c}"));
    }
    r.body(Body::empty()).unwrap()
}

#[tokio::test]
#[ignore]
async fn sso_callback_sets_cookie_and_redirects() {
    let h = Harness::new().await;
    let u = h.user("member");
    let state = sso_state(&h, &u, "code_ok").await;
    let r = h
        .app
        .clone()
        .oneshot(callback(
            &format!("code=code_ok&state={state}"),
            Some(&state),
        ))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::FOUND);
    assert_eq!(r.headers()[header::LOCATION], format!("{WEB}/"));
    let cookie = session_cookie(&r).unwrap();
    let tenant = tenant_of(&u.memberships[0].org);
    assert!(cookie.starts_with(&format!("{tenant}.")));
    let cleared = r
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .any(|v| v.to_str().unwrap().starts_with("mandate_sso=;"));
    assert!(cleared);
}

#[tokio::test]
#[ignore]
async fn sso_state_is_spent_once() {
    let h = Harness::new().await;
    let u = h.user("member");
    let state = sso_state(&h, &u, "code_ok").await;
    let go = || {
        h.app.clone().oneshot(callback(
            &format!("code=code_ok&state={state}"),
            Some(&state),
        ))
    };
    let (a, b) = tokio::join!(go(), go());
    let signed = [a.unwrap(), b.unwrap()]
        .iter()
        .filter(|r| session_cookie(r).is_some())
        .count();
    assert_eq!(signed, 1);
    let codes = h
        .fake
        .calls
        .lock()
        .unwrap()
        .iter()
        .filter(|c| *c == "code")
        .count();
    assert_eq!(codes, 1, "WorkOS is asked once");
}

#[tokio::test]
#[ignore]
async fn sso_callback_bad_state_redirects_with_error() {
    let h = Harness::new().await;
    let u = h.user("member");
    let state = sso_state(&h, &u, "code_ok").await;
    let error_landing = format!("{WEB}/sign-in?error=forbidden");
    for (query, cookie) in [
        // No cookie: the state was not started in this browser.
        (format!("code=code_ok&state={state}"), None),
        // The cookie and the state disagree.
        (format!("code=code_ok&state={state}"), Some("other")),
        // A state never issued, even if the browser echoes it.
        ("code=code_ok&state=forged".to_owned(), Some("forged")),
        // Missing code or state.
        (format!("state={state}"), Some(state.as_str())),
        ("code=code_ok".to_owned(), Some(state.as_str())),
    ] {
        let r = h
            .app
            .clone()
            .oneshot(callback(&query, cookie))
            .await
            .unwrap();
        assert_eq!(r.status(), StatusCode::FOUND, "{query}");
        assert_eq!(
            r.headers()[header::LOCATION],
            error_landing,
            "{query} {cookie:?}"
        );
        assert!(session_cookie(&r).is_none());
    }
    assert!(
        !h.fake.called("code"),
        "WorkOS is never asked with a bad state"
    );
    // A good state works once; replayed, it is refused.
    let r = h
        .app
        .clone()
        .oneshot(callback(
            &format!("code=code_ok&state={state}"),
            Some(&state),
        ))
        .await
        .unwrap();
    assert!(session_cookie(&r).is_some());
    let r = h
        .app
        .clone()
        .oneshot(callback(
            &format!("code=code_ok&state={state}"),
            Some(&state),
        ))
        .await
        .unwrap();
    assert_eq!(r.headers()[header::LOCATION], error_landing);
    // A code WorkOS rejects lands with that error.
    let state = sso_state(&h, &u, "code_ok2").await;
    let r = h
        .app
        .clone()
        .oneshot(callback(&format!("code=bad&state={state}"), Some(&state)))
        .await
        .unwrap();
    assert_eq!(
        r.headers()[header::LOCATION],
        format!("{WEB}/sign-in?error=invalid_credentials")
    );
}

#[tokio::test]
#[ignore]
async fn forgot_is_202_always() {
    let h = Harness::new().await;
    let u = h.user("member");
    let known = h
        .post("/v1/auth/password/forgot", json!({ "email": u.email }))
        .await;
    let unknown = h
        .post(
            "/v1/auth/password/forgot",
            json!({ "email": format!("nobody-{}@acme.test", Uuid::new_v4().simple()) }),
        )
        .await;
    assert_eq!(known.status(), StatusCode::ACCEPTED);
    assert_eq!(unknown.status(), StatusCode::ACCEPTED);
    let k = known.into_body().collect().await.unwrap().to_bytes();
    let n = unknown.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(k, n);
    assert!(h.fake.called(&format!("reset {}", u.email)));
}

#[tokio::test]
#[ignore]
async fn reset_with_valid_token_is_204() {
    let h = Harness::new().await;
    let u = h.user("member");
    let r = h
        .post(
            "/v1/auth/password/reset",
            json!({ "token": "nope", "password": "new password 1" }),
        )
        .await;
    assert_eq!(body_json(r).await["error"]["code"], "code_invalid");
    let token = format!("reset_{}", u.id);
    let r = h
        .post(
            "/v1/auth/password/reset",
            json!({ "token": token, "password": "new password 1" }),
        )
        .await;
    assert_eq!(r.status(), StatusCode::NO_CONTENT);
    let r = h
        .post(
            "/v1/auth/password",
            json!({ "email": u.email, "password": "new password 1" }),
        )
        .await;
    assert_eq!(body_json(r).await["status"], "signed_in");
}

/// An invitation to a new org from `inviter`, for `email`.
fn invite_to(h: &Harness, inviter: &FakeUser, email: &str) -> String {
    let token = format!("inv_{}", Uuid::new_v4().simple());
    let org = inviter.memberships[0].org.clone();
    h.fake.invitations.lock().unwrap().insert(
        token.clone(),
        Invitation {
            email: email.into(),
            org: Some(org),
            inviter: Some(inviter.id.clone()),
            pending: true,
        },
    );
    token
}

#[tokio::test]
#[ignore]
async fn invite_shows_org_and_inviter() {
    let h = Harness::new().await;
    let inviter = h.user("admin");
    // The inviter has signed in, so the org is a tenant with one member.
    signed_in(&h, &inviter).await;
    let email = format!("new-{}@acme.test", Uuid::new_v4().simple());
    let token = invite_to(&h, &inviter, &email);
    let r = h.get(&format!("/v1/auth/invite/{token}"), None).await;
    assert_eq!(r.status(), StatusCode::OK);
    let v = body_json(r).await;
    let tenant = tenant_of(&inviter.memberships[0].org);
    assert_eq!(v["email"], email.as_str());
    assert_eq!(v["org"]["id"], tenant.as_str());
    assert_eq!(v["org"]["name"], "Acme");
    assert_eq!(v["org"]["summary"]["people"], 1);
    assert_eq!(v["inviter"]["name"], "Ann Lee");
    assert_eq!(v["inviter"]["email"], inviter.email.as_str());
    let mut tx = h.store.begin(&tenant).await.unwrap();
    let member = mandate::store::member_for_workos_user(&mut tx, &inviter.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(v["inviter"]["id"], member.as_str());
    // Unknown, used up, or malformed: 404.
    h.fake
        .invitations
        .lock()
        .unwrap()
        .get_mut(&token)
        .unwrap()
        .pending = false;
    for t in [token.as_str(), "inv_unknown", "bad%20token"] {
        let r = h.get(&format!("/v1/auth/invite/{t}"), None).await;
        assert_eq!(r.status(), StatusCode::NOT_FOUND, "{t}");
    }
}

#[tokio::test]
#[ignore]
async fn invite_accept_with_password() {
    let h = Harness::new().await;
    let inviter = h.user("admin");
    let email = format!("new-{}@acme.test", Uuid::new_v4().simple());
    let token = invite_to(&h, &inviter, &email);
    let r = h
        .post(
            &format!("/v1/auth/invite/{token}/accept"),
            json!({ "password": "a long passphrase", "name": "Bo Chen" }),
        )
        .await;
    assert_eq!(r.status(), StatusCode::OK);
    assert!(session_cookie(&r).is_some());
    let v = body_json(r).await;
    assert_eq!(v["status"], "signed_in", "{v}");
    assert_eq!(
        v["me"]["org"]["id"],
        tenant_of(&inviter.memberships[0].org).as_str()
    );
    assert_eq!(v["me"]["person"]["name"], "Bo Chen");
    assert_eq!(v["me"]["is_admin"], false);
    assert!(
        h.fake
            .called(&format!("password {email} Some(\"{token}\")"))
    );
    // Mixed or empty bodies are refused.
    for bad in [
        json!({}),
        json!({ "password": "a long passphrase", "code": GOOD_CODE }),
    ] {
        let r = h
            .post(&format!("/v1/auth/invite/{token}/accept"), bad)
            .await;
        assert_eq!(body_json(r).await["error"]["code"], "invalid_input");
    }
}

#[tokio::test]
#[ignore]
async fn invite_accept_with_code() {
    let h = Harness::new().await;
    let inviter = h.user("admin");
    // An existing WorkOS user with no org yet.
    let invitee = h.user("member");
    h.fake.update(&invitee.id, |u| u.memberships.clear());
    let token = invite_to(&h, &inviter, &invitee.email);
    let r = h
        .post("/v1/auth/code/start", json!({ "email": invitee.email }))
        .await;
    assert_eq!(r.status(), StatusCode::ACCEPTED);
    let r = h
        .post(
            &format!("/v1/auth/invite/{token}/accept"),
            json!({ "code": GOOD_CODE }),
        )
        .await;
    assert_eq!(r.status(), StatusCode::OK);
    let v = body_json(r).await;
    assert_eq!(v["status"], "signed_in", "{v}");
    assert_eq!(
        v["me"]["org"]["id"],
        tenant_of(&inviter.memberships[0].org).as_str()
    );
}

#[tokio::test]
#[ignore]
async fn me_returns_person_org_orgs_and_flags() {
    let h = Harness::new().await;
    let u = h.user("admin");
    let cookie = signed_in(&h, &u).await;
    let tenant = tenant_of(&u.memberships[0].org);
    // A team the member belongs to shows up.
    let mut tx = h.store.begin(&tenant).await.unwrap();
    let member = mandate::store::member_for_workos_user(&mut tx, &u.id)
        .await
        .unwrap()
        .unwrap();
    let team = mandate::ids::TeamId::parse("eng").unwrap();
    mandate::store::upsert_team(&mut tx, &team, "group_eng")
        .await
        .unwrap();
    mandate::store::set_team_member(&mut tx, &team, &member, true)
        .await
        .unwrap();
    tx.commit().await.unwrap();

    let r = h.get("/v1/me", Some(&cookie)).await;
    assert_eq!(r.status(), StatusCode::OK);
    let me = body_json(r).await;
    assert_eq!(
        me["person"],
        json!({ "id": member, "name": "Ann Lee", "email": u.email, "avatar_url": null })
    );
    assert_eq!(
        me["org"],
        json!({ "id": tenant, "name": "Acme", "summary": { "people": 1, "agents": 1 } })
    );
    assert_eq!(
        me["orgs"],
        json!([{ "id": tenant, "name": "Acme", "summary": { "people": 1, "agents": 1 } }])
    );
    assert_eq!(me["mfa_enabled"], false);
    assert_eq!(me["is_admin"], true);
    assert_eq!(me["teams"], json!([{ "id": "eng", "name": "eng" }]));

    let r = h.get("/v1/me", None).await;
    assert_eq!(r.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(body_json(r).await["error"]["code"], "forbidden");
}

#[tokio::test]
#[ignore]
async fn signing_in_again_ends_the_old_session() {
    let h = Harness::new().await;
    let u = h.user("member");
    let tenant = tenant_of(&u.memberships[0].org);
    let old = signed_in(&h, &u).await;
    let again = |cookie: &str, password: &str| {
        let body = json!({ "email": u.email, "password": password });
        h.app.clone().oneshot(with_cookie(
            auth_post("/v1/auth/password", &h.ip, body),
            cookie,
        ))
    };
    let (store, tenant) = (&h.store, &tenant);
    let live = |cookie: String| async move {
        let mut tx = store.begin(tenant).await.unwrap();
        session::profile(&mut tx, &cookie).await.unwrap().is_some()
    };
    // A failed attempt leaves the session alone.
    let r = again(&old, "wrong password").await.unwrap();
    assert_eq!(r.status(), StatusCode::UNAUTHORIZED);
    assert!(live(old.clone()).await);
    let r = again(&old, &u.password).await.unwrap();
    assert_eq!(r.status(), StatusCode::OK);
    let new = session_cookie(&r).unwrap();
    assert_ne!(new, old);
    assert!(!live(old).await, "the session the request carried is gone");
    assert!(live(new).await);
}

#[tokio::test]
#[ignore]
async fn sign_out_deletes_session_and_clears_cookie() {
    let h = Harness::new().await;
    let u = h.user("member");
    let cookie = signed_in(&h, &u).await;
    let r = h
        .app
        .clone()
        .oneshot(with_cookie(
            auth_post("/v1/auth/sign-out", &h.ip, json!({})),
            &cookie,
        ))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::NO_CONTENT);
    let set = r.headers()[header::SET_COOKIE].to_str().unwrap();
    assert!(
        set.starts_with("mandate_session=;") && set.contains("Max-Age=0"),
        "{set}"
    );
    let r = h.get("/v1/me", Some(&cookie)).await;
    assert_eq!(r.status(), StatusCode::UNAUTHORIZED);
    // The row itself is gone, not only expired.
    let tenant = tenant_of(&u.memberships[0].org);
    let mut tx = h.store.begin(&tenant).await.unwrap();
    assert!(session::profile(&mut tx, &cookie).await.unwrap().is_none());
}

#[tokio::test]
#[ignore]
async fn removed_member_session_is_gone() {
    let h = Harness::new().await;
    let u = h.user("member");
    let cookie = signed_in(&h, &u).await;
    let tenant = tenant_of(&u.memberships[0].org);
    let mut tx = h.store.begin(&tenant).await.unwrap();
    let member = mandate::store::member_for_workos_user(&mut tx, &u.id)
        .await
        .unwrap()
        .unwrap();
    mandate::store::remove_member(&mut tx, &member)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let mut tx = h.store.begin(&tenant).await.unwrap();
    assert!(
        session::profile(&mut tx, &cookie).await.unwrap().is_none(),
        "deleted at once"
    );
    drop(tx);
    let r = h.get("/v1/me", Some(&cookie)).await;
    assert_eq!(r.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
#[ignore]
async fn personal_operator_only_through_the_narrow_function() {
    let store = app_store().await;
    let (a, b) = (tenant(), tenant());
    let ann = member("ann");
    add_member(&store, &a, &ann, false).await;
    // Two members whose ids share their first 61 characters still get one operator each.
    let long = |c: char| format!("{}{c}{c}{c}", "m".repeat(61));
    let (x, y) = (long('x'), long('y'));
    add_member(&store, &a, &member(&x), false).await;
    add_member(&store, &a, &member(&y), false).await;
    let ensure = |t: TenantId, m: String| {
        let store = store.clone();
        async move {
            let mut tx = store.begin(&t).await.unwrap();
            sqlx::query("select ensure_personal_agent($1)")
                .bind(m)
                .execute(&mut *tx)
                .await
                .unwrap();
            tx.commit().await.unwrap();
        }
    };
    let operators = |t: TenantId| async move {
        let rows: Vec<(String, String, bool, Option<String>)> = sqlx::query_as(
            "select id, kind, disabled, name from agents where tenant_id = $1 order by id",
        )
        .bind(t.as_str())
        .fetch_all(&common::admin_pool().await)
        .await
        .unwrap();
        rows
    };
    // Not a member of the transaction's tenant: nothing, here or there.
    ensure(b.clone(), "ann".into()).await;
    ensure(a.clone(), "bob".into()).await;
    assert!(operators(b.clone()).await.is_empty());
    assert!(operators(a.clone()).await.is_empty());
    // A member gets exactly one operator, however often it runs; its id is pinned here.
    ensure(a.clone(), "ann".into()).await;
    ensure(a.clone(), "ann".into()).await;
    assert_eq!(
        operators(a.clone()).await,
        vec![(
            "op-49915e0d7d4b402e3017d010bc1c0e83".into(),
            "personal".into(),
            false,
            // A name to show, not the id.
            Some("Personal agent".into())
        )]
    );
    assert_eq!(
        personal_operator(&ann).as_str(),
        "op-49915e0d7d4b402e3017d010bc1c0e83"
    );
    ensure(a.clone(), x.clone()).await;
    ensure(a.clone(), y.clone()).await;
    let owners: Vec<(String, String)> = sqlx::query_as(
        "select id, owner_member from agents where tenant_id = $1 and owner_member <> 'ann' order by owner_member",
    )
    .bind(a.as_str())
    .fetch_all(&common::admin_pool().await)
    .await
    .unwrap();
    assert_eq!(
        owners,
        [&x, &y].map(|m| (personal_operator(&member(m)).to_string(), m.clone()))
    );
    // A disabled operator stays disabled.
    let mut tx = store.begin(&a).await.unwrap();
    sqlx::query(
        "update agents set disabled = true where tenant_id = current_setting('mandate.tenant')",
    )
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    ensure(a.clone(), "ann".into()).await;
    assert!(
        operators(a)
            .await
            .iter()
            .all(|(_, kind, disabled, _)| kind == "personal" && *disabled)
    );
}

/// Our sign-up's row for this WorkOS user: `Some(neutralised)`, or `None` once settled.
async fn sign_up_row(h: &Harness, user: &str) -> Option<bool> {
    let mut tx = h.store.begin_untenanted().await.unwrap();
    sqlx::query_scalar(
        "select neutralised_at is not null from sign_up_passwords where workos_user_id = $1",
    )
    .bind(user)
    .fetch_optional(&mut *tx)
    .await
    .unwrap()
}

/// The password `sign_up_body` sets: the attacker's, in the pre-hijacking tests.
const SIGN_UP_PASSWORD: &str = "a long passphrase";

/// An attacker signs up with someone else's email; returns that WorkOS user.
async fn pre_hijacked(h: &Harness) -> FakeUser {
    let email = format!("victim-{}@acme.test", Uuid::new_v4().simple());
    let r = h.post("/v1/auth/sign-up", sign_up_body(&email)).await;
    assert_eq!(r.status(), StatusCode::ACCEPTED);
    let u = h.fake.by_email(&email).unwrap();
    assert_eq!(sign_up_row(h, &u.id).await, Some(false));
    u
}

/// The user's password was replaced by a random one that never reached the call log.
fn assert_neutralised(h: &Harness, user: &str) {
    let now = h.fake.by_id(user).unwrap().password;
    assert_ne!(now, SIGN_UP_PASSWORD);
    assert_eq!(now.len(), 43, "256 random bits, base64url");
    assert!(h.fake.called(&format!("set-password {user}")));
    assert!(
        !h.fake
            .calls
            .lock()
            .unwrap()
            .iter()
            .any(|c| c.contains(&now))
    );
}

#[tokio::test]
#[ignore]
async fn another_sign_in_neutralises_an_unconfirmed_sign_up_password() {
    let h = Harness::new().await;
    let inviter = h.user("admin");
    let victim = pre_hijacked(&h).await;
    // The owner accepts an invitation with an emailed code (which verifies the email in WorkOS).
    let token = invite_to(&h, &inviter, &victim.email);
    let r = h
        .post(
            &format!("/v1/auth/invite/{token}/accept"),
            json!({ "code": GOOD_CODE }),
        )
        .await;
    assert_eq!(body_json(r).await["status"], "signed_in");
    assert_neutralised(&h, &victim.id);
    assert_eq!(
        sign_up_row(&h, &victim.id).await,
        Some(true),
        "kept, marked"
    );
    let r = h
        .post(
            "/v1/auth/password",
            json!({ "email": victim.email, "password": SIGN_UP_PASSWORD }),
        )
        .await;
    assert_eq!(body_json(r).await["error"]["code"], "invalid_credentials");
}

#[tokio::test]
#[ignore]
async fn magic_code_neutralises_even_without_an_org() {
    let h = Harness::new().await;
    let victim = pre_hijacked(&h).await;
    let r = h
        .post(
            "/v1/auth/code/verify",
            json!({ "email": victim.email, "code": GOOD_CODE }),
        )
        .await;
    assert_eq!(body_json(r).await["error"]["code"], "no_access");
    assert_neutralised(&h, &victim.id);
    assert_eq!(
        sign_up_row(&h, &victim.id).await,
        Some(true),
        "kept, marked"
    );
}

#[tokio::test]
#[ignore]
async fn unconfirmed_sign_up_password_never_signs_in() {
    let h = Harness::new().await;
    let inviter = h.user("admin");
    let victim = pre_hijacked(&h).await;
    // The email got verified outside our sign-up, and the owner joined an org.
    let org = inviter.memberships[0].clone();
    h.fake.update(&victim.id, |u| {
        u.verified = true;
        u.memberships.push(Membership {
            role: "member".into(),
            ..org
        });
    });
    let r = h
        .post(
            "/v1/auth/password",
            json!({ "email": victim.email.to_uppercase(), "password": SIGN_UP_PASSWORD }),
        )
        .await;
    assert!(session_cookie(&r).is_none());
    assert_eq!(body_json(r).await["error"]["code"], "invalid_credentials");
    assert_neutralised(&h, &victim.id);
    assert_eq!(
        sign_up_row(&h, &victim.id).await,
        Some(true),
        "kept, marked"
    );
}

#[tokio::test]
#[ignore]
async fn failed_neutralisation_issues_no_session() {
    let h = Harness::new().await;
    let inviter = h.user("admin");
    let victim = pre_hijacked(&h).await;
    *h.fake.set_password_fails.lock().unwrap() = true;
    let token = invite_to(&h, &inviter, &victim.email);
    let r = h
        .post(
            &format!("/v1/auth/invite/{token}/accept"),
            json!({ "code": GOOD_CODE }),
        )
        .await;
    assert_eq!(r.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert!(session_cookie(&r).is_none());
    assert_eq!(
        sign_up_row(&h, &victim.id).await,
        Some(false),
        "still to be neutralised"
    );
}

#[tokio::test]
#[ignore]
async fn sign_up_verify_confirms_its_password() {
    let h = Harness::new().await;
    let email = format!("new-{}@acme.test", Uuid::new_v4().simple());
    let v = body_json(h.post("/v1/auth/sign-up", sign_up_body(&email)).await).await;
    let u = h.fake.by_email(&email).unwrap();
    let r = h
        .post(
            "/v1/auth/verify-email",
            json!({ "pending": v["pending"], "code": GOOD_CODE }),
        )
        .await;
    assert_eq!(body_json(r).await["status"], "signed_in");
    assert_eq!(sign_up_row(&h, &u.id).await, None);
    let r = h
        .post(
            "/v1/auth/password",
            json!({ "email": email, "password": SIGN_UP_PASSWORD }),
        )
        .await;
    assert_eq!(body_json(r).await["status"], "signed_in");
    assert!(!h.fake.called("set-password"));
}

#[tokio::test]
#[ignore]
async fn password_and_emailed_code_confirm_a_sign_up_password() {
    let h = Harness::new().await;
    let inviter = h.user("admin");
    let u = pre_hijacked(&h).await;
    let org = inviter.memberships[0].clone();
    h.fake.update(&u.id, |u| u.memberships.push(org));
    // The password, then the code WorkOS emails: the same proof as sign-up's own step.
    let pending = pending_for(&h, &h.fake.by_id(&u.id).unwrap(), "verify_email").await;
    let r = h
        .post(
            "/v1/auth/verify-email",
            json!({ "pending": pending, "code": GOOD_CODE }),
        )
        .await;
    assert_eq!(body_json(r).await["status"], "signed_in");
    assert_eq!(sign_up_row(&h, &u.id).await, None);
    assert!(!h.fake.called("set-password"));
    assert_eq!(h.fake.by_id(&u.id).unwrap().password, SIGN_UP_PASSWORD);
}

#[tokio::test]
#[ignore]
async fn reset_settles_an_unconfirmed_sign_up_password() {
    let h = Harness::new().await;
    let inviter = h.user("admin");
    let u = pre_hijacked(&h).await;
    let r = h
        .post(
            "/v1/auth/password/reset",
            json!({ "token": format!("reset_{}", u.id), "password": "the owner's own one" }),
        )
        .await;
    assert_eq!(r.status(), StatusCode::NO_CONTENT);
    assert_eq!(sign_up_row(&h, &u.id).await, None);
    // The owner's new password keeps working.
    let org = inviter.memberships[0].clone();
    h.fake.update(&u.id, |u| u.memberships.push(org));
    let r = h
        .post(
            "/v1/auth/password",
            json!({ "email": u.email, "password": "the owner's own one" }),
        )
        .await;
    assert_eq!(body_json(r).await["status"], "signed_in");
    assert!(!h.fake.called("set-password"));
}

#[tokio::test]
#[ignore]
async fn reset_ends_every_session_of_that_user() {
    let h = Harness::new().await;
    let u = h.user("member");
    let a = signed_in(&h, &u).await;
    // The same person in two more orgs, signed into each with only that membership active.
    let mut cookies = vec![a];
    for name in ["Beta", "Gamma"] {
        let org = OrgRef {
            id: format!("org_{}", Uuid::new_v4().simple()),
            name: name.into(),
        };
        h.fake.orgs.lock().unwrap().push(org.clone());
        h.fake.update(&u.id, |u| {
            u.memberships.iter_mut().for_each(|m| m.active = false);
            u.memberships.push(Membership {
                org: org.id.clone(),
                org_name: org.name.clone(),
                role: "member".into(),
                active: true,
            });
        });
        cookies.push(signed_in(&h, &u).await);
    }
    let tenants: std::collections::HashSet<_> = cookies
        .iter()
        .map(|c| c.split('.').next().unwrap().to_owned())
        .collect();
    assert_eq!(tenants.len(), 3);
    // WorkOS no longer lists the third org at all; its member row (and session) is still here.
    h.fake.update(&u.id, |u| {
        u.memberships.pop();
    });
    let other = signed_in(&h, &h.user("member")).await;
    for c in cookies.iter().chain([&other]) {
        assert_eq!(h.get("/v1/me", Some(c)).await.status(), StatusCode::OK);
    }
    let r = h
        .post(
            "/v1/auth/password/reset",
            json!({ "token": format!("reset_{}", u.id), "password": "new password 1" }),
        )
        .await;
    assert_eq!(r.status(), StatusCode::NO_CONTENT);
    for c in &cookies {
        assert_eq!(
            h.get("/v1/me", Some(c)).await.status(),
            StatusCode::UNAUTHORIZED
        );
    }
    assert_eq!(h.get("/v1/me", Some(&other)).await.status(), StatusCode::OK);
}

#[tokio::test]
#[ignore]
async fn invite_accept_codes_are_limited_per_email() {
    let h = Harness::new().await;
    let inviter = h.user("admin");
    let invitee = h.user("member");
    h.fake.update(&invitee.id, |u| u.memberships.clear());
    let accept = |code: &str| {
        // A fresh invitation token and IP every time: only the email stays the same.
        let token = invite_to(&h, &inviter, &invitee.email);
        auth_post(
            &format!("/v1/auth/invite/{token}/accept"),
            &random_ip(),
            json!({ "code": code }),
        )
    };
    fresh_minute().await;
    for i in 0..5 {
        let r = h.app.clone().oneshot(accept("000000")).await.unwrap();
        assert_eq!(
            body_json(r).await["error"]["code"],
            "code_invalid",
            "attempt {}",
            i + 1
        );
    }
    let r = h.app.clone().oneshot(accept(GOOD_CODE)).await.unwrap();
    assert_eq!(r.status(), StatusCode::TOO_MANY_REQUESTS);
    assert!(r.headers().contains_key(header::RETRY_AFTER));
    assert!(session_cookie(&r).is_none());
    // `/code/verify` for that email draws on the same bucket.
    let r = h
        .app
        .clone()
        .oneshot(auth_post(
            "/v1/auth/code/verify",
            &random_ip(),
            json!({ "email": invitee.email.to_uppercase(), "code": GOOD_CODE }),
        ))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::TOO_MANY_REQUESTS);
}

/// The victim's org and a verified email, as if they proved it somewhere; the attacker's
/// password is what WorkOS still holds (its answer raced our scramble).
fn victim_ready(h: &Harness, inviter: &FakeUser, victim: &FakeUser) {
    let org = inviter.memberships[0].clone();
    h.fake.update(&victim.id, |u| {
        u.verified = true;
        u.password = SIGN_UP_PASSWORD.into();
        if u.memberships.is_empty() {
            u.memberships.push(Membership {
                role: "member".into(),
                ..org
            });
        }
    });
}

#[tokio::test]
#[ignore]
async fn password_accepted_after_neutralisation_gets_no_session() {
    let h = Harness::new().await;
    let inviter = h.user("admin");
    let victim = pre_hijacked(&h).await;
    let token = invite_to(&h, &inviter, &victim.email);
    let r = h
        .post(
            &format!("/v1/auth/invite/{token}/accept"),
            json!({ "code": GOOD_CODE }),
        )
        .await;
    assert_eq!(body_json(r).await["status"], "signed_in");
    // WorkOS answers the attacker's password as if the scramble had not landed yet.
    victim_ready(&h, &inviter, &victim);
    let r = h
        .post(
            "/v1/auth/password",
            json!({ "email": victim.email, "password": SIGN_UP_PASSWORD }),
        )
        .await;
    assert!(session_cookie(&r).is_none());
    assert_eq!(body_json(r).await["error"]["code"], "invalid_credentials");
}

#[tokio::test]
#[ignore]
async fn password_accepted_during_neutralisation_gets_no_session() {
    let h = Harness::new().await;
    let inviter = h.user("admin");
    let victim = pre_hijacked(&h).await;
    let token = invite_to(&h, &inviter, &victim.email);
    let (arrived, release) = (
        Arc::new(tokio::sync::Barrier::new(2)),
        Arc::new(tokio::sync::Barrier::new(2)),
    );
    *h.fake.set_password_gate.lock().unwrap() = Some((arrived.clone(), release.clone()));
    let owner = tokio::spawn(h.app.clone().oneshot(auth_post(
        &format!("/v1/auth/invite/{token}/accept"),
        &random_ip(),
        json!({ "code": GOOD_CODE }),
    )));
    // The owner's scramble is in flight; WorkOS still takes the attacker's password.
    arrived.wait().await;
    victim_ready(&h, &inviter, &victim);
    let r = h
        .post(
            "/v1/auth/password",
            json!({ "email": victim.email, "password": SIGN_UP_PASSWORD }),
        )
        .await;
    assert!(session_cookie(&r).is_none());
    assert_eq!(body_json(r).await["error"]["code"], "invalid_credentials");
    release.wait().await;
    let r = owner.await.unwrap().unwrap();
    assert_eq!(body_json(r).await["status"], "signed_in");
    assert_eq!(sign_up_row(&h, &victim.id).await, Some(true));
}

#[tokio::test]
#[ignore]
async fn guard_finds_the_row_by_user_not_typed_email() {
    let h = Harness::new().await;
    let inviter = h.user("admin");
    let victim = pre_hijacked(&h).await;
    // WorkOS now knows the user by another email than the one sign-up recorded.
    let renamed = format!("renamed-{}@acme.test", Uuid::new_v4().simple());
    h.fake.update(&victim.id, |u| u.email = renamed.clone());
    victim_ready(&h, &inviter, &victim);
    let r = h
        .post(
            "/v1/auth/password",
            json!({ "email": renamed, "password": SIGN_UP_PASSWORD }),
        )
        .await;
    assert!(session_cookie(&r).is_none());
    assert_eq!(body_json(r).await["error"]["code"], "invalid_credentials");
    assert_neutralised(&h, &victim.id);
}

#[tokio::test]
#[ignore]
async fn sign_up_for_an_existing_account_records_nothing() {
    let h = Harness::new().await;
    let existing = h.user("member");
    let r = h
        .post("/v1/auth/sign-up", sign_up_body(&existing.email))
        .await;
    assert_eq!(r.status(), StatusCode::ACCEPTED);
    let mut tx = h.store.begin_untenanted().await.unwrap();
    let rows: i64 =
        sqlx::query_scalar("select count(*) from sign_up_passwords where email = lower($1)")
            .bind(&existing.email)
            .fetch_one(&mut *tx)
            .await
            .unwrap();
    assert_eq!(rows, 0);
}

#[tokio::test]
#[ignore]
async fn untagged_verify_email_step_leaves_the_row() {
    let h = Harness::new().await;
    let inviter = h.user("admin");
    let victim = pre_hijacked(&h).await;
    let org = inviter.memberships[0].clone();
    h.fake.update(&victim.id, |u| u.memberships.push(org));
    // A verify-email step that no password started (here, after a magic code).
    h.fake.force(AuthError::Step(Step::VerifyEmail {
        token: Secret(format!("pat_{}", victim.id)),
        email: victim.email.clone(),
        user: Some(victim.id.clone()),
    }));
    let r = h
        .post(
            "/v1/auth/code/verify",
            json!({ "email": victim.email, "code": GOOD_CODE }),
        )
        .await;
    let v = body_json(r).await;
    assert_eq!(v["next"], "verify_email", "{v}");
    let r = h
        .post(
            "/v1/auth/verify-email",
            json!({ "pending": v["pending"], "code": GOOD_CODE }),
        )
        .await;
    assert_eq!(body_json(r).await["status"], "signed_in");
    // Not a confirmation of the password: neutralised, and the row stays.
    assert_neutralised(&h, &victim.id);
    assert_eq!(sign_up_row(&h, &victim.id).await, Some(true));
}

#[tokio::test]
#[ignore]
async fn verify_email_codes_are_limited_per_email() {
    let h = Harness::new().await;
    let email = format!("new-{}@acme.test", Uuid::new_v4().simple());
    fresh_minute().await;
    // Sign-up counts once against the email; then two guesses on its step.
    let first = body_json(h.post("/v1/auth/sign-up", sign_up_body(&email)).await).await;
    for _ in 0..2 {
        let r = h
            .post(
                "/v1/auth/verify-email",
                json!({ "pending": first["pending"], "code": "000000" }),
            )
            .await;
        assert_eq!(body_json(r).await["error"]["code"], "code_invalid");
    }
    // A second sign-up (4th) gives a fresh step; one guess (5th) is fine, the next is not.
    let second = body_json(h.post("/v1/auth/sign-up", sign_up_body(&email)).await).await;
    let guess = || {
        h.post(
            "/v1/auth/verify-email",
            json!({ "pending": second["pending"], "code": "000000" }),
        )
    };
    assert_eq!(
        body_json(guess().await).await["error"]["code"],
        "code_invalid"
    );
    assert_eq!(guess().await.status(), StatusCode::TOO_MANY_REQUESTS);
}

#[tokio::test]
#[ignore]
async fn totp_guesses_are_limited_per_factor_across_steps() {
    let h = Harness::new().await;
    let u = h.user("member");
    h.fake.update(&u.id, |u| u.totp = true);
    fresh_minute().await;
    let guess = |pending: String| {
        h.post(
            "/v1/auth/mfa/verify",
            json!({ "pending": pending, "code": "000000" }),
        )
    };
    // Three guesses on one step, two on a fresh one: the factor's five.
    let a = pending_for(&h, &u, "mfa").await;
    for _ in 0..3 {
        assert_eq!(
            body_json(guess(a.clone()).await).await["error"]["code"],
            "code_invalid"
        );
    }
    let b = pending_for(&h, &u, "mfa").await;
    for _ in 0..2 {
        assert_eq!(
            body_json(guess(b.clone()).await).await["error"]["code"],
            "code_invalid"
        );
    }
    assert_eq!(guess(b).await.status(), StatusCode::TOO_MANY_REQUESTS);
}

#[tokio::test]
#[ignore]
async fn totp_setup_guesses_are_limited_per_user_across_challenges() {
    let h = Harness::new().await;
    let u = h.user("member");
    let cookie = signed_in(&h, &u).await;
    fresh_minute().await;
    let send = |uri: &str, body: Value| {
        h.app
            .clone()
            .oneshot(with_cookie(auth_post(uri, &h.ip, body), &cookie))
    };
    let challenge = || async {
        let v = body_json(send("/v1/auth/mfa/setup", json!({})).await.unwrap()).await;
        v["challenge"].as_str().unwrap().to_owned()
    };
    let guess = |c: String| {
        send(
            "/v1/auth/mfa/enable",
            json!({ "challenge": c, "code": "000000" }),
        )
    };
    let c1 = challenge().await;
    for _ in 0..3 {
        assert_eq!(
            body_json(guess(c1.clone()).await.unwrap()).await["error"]["code"],
            "code_invalid"
        );
    }
    let c2 = challenge().await;
    for _ in 0..2 {
        assert_eq!(
            body_json(guess(c2.clone()).await.unwrap()).await["error"]["code"],
            "code_invalid"
        );
    }
    assert_eq!(
        guess(c2).await.unwrap().status(),
        StatusCode::TOO_MANY_REQUESTS
    );
}

/// `(workos_user_id, neutralised)` of the sign-up row for an email.
async fn sign_up_row_by_email(h: &Harness, email: &str) -> Option<(Option<String>, bool)> {
    let mut tx = h.store.begin_untenanted().await.unwrap();
    sqlx::query_as(
        "select workos_user_id, neutralised_at is not null from sign_up_passwords where email = lower($1)",
    )
    .bind(email)
    .fetch_optional(&mut *tx)
    .await
    .unwrap()
}

#[tokio::test]
#[ignore]
async fn concurrent_sign_ups_leave_the_row_with_the_created_user() {
    let h = Harness::new().await;
    let email = format!("race-{}@acme.test", Uuid::new_v4().simple());
    let race = Arc::new(CreateRace::default());
    *h.fake.create_race.lock().unwrap() = Some(race.clone());
    let sign_up = || {
        h.app.clone().oneshot(auth_post(
            "/v1/auth/sign-up",
            &random_ip(),
            sign_up_body(&email),
        ))
    };
    // A records the row first, then waits in WorkOS until B has made the user.
    let a = tokio::spawn(sign_up());
    while !h.fake.called(&format!("create-user {email}")) {
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    let b = tokio::spawn(sign_up());
    // A gets Conflict and drops the row it made, all before B records its user.
    assert_eq!(a.await.unwrap().unwrap().status(), StatusCode::ACCEPTED);
    race.release.notify_one();
    assert_eq!(b.await.unwrap().unwrap().status(), StatusCode::ACCEPTED);
    let user = h.fake.by_email(&email).unwrap();
    assert_eq!(
        sign_up_row_by_email(&h, &email).await,
        Some((Some(user.id), false))
    );
}

#[tokio::test]
#[ignore]
async fn ambiguous_create_user_failure_keeps_the_row() {
    let h = Harness::new().await;
    let email = format!("timeout-{}@acme.test", Uuid::new_v4().simple());
    // WorkOS made the user but the answer was lost.
    *h.fake.create_user_error.lock().unwrap() = Some(AuthError::Upstream("status 504".into()));
    let r = h.post("/v1/auth/sign-up", sign_up_body(&email)).await;
    assert_eq!(r.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(sign_up_row_by_email(&h, &email).await, Some((None, false)));
    // So that password is refused even after the email gets verified some other way.
    let inviter = h.user("admin");
    let u = h.fake.by_email(&email).unwrap();
    victim_ready(&h, &inviter, &u);
    let r = h
        .post(
            "/v1/auth/password",
            json!({ "email": email, "password": SIGN_UP_PASSWORD }),
        )
        .await;
    assert!(session_cookie(&r).is_none());
    assert_eq!(body_json(r).await["error"]["code"], "invalid_credentials");
}
