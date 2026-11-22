//! The web sign-in API. Tests that touch the database need DATABASE_URL (a superuser) and
//! `--ignored`; WorkOS is always a fake, never the network.
mod common;
use async_trait::async_trait;
use axum::Router;
use axum::body::Body;
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

fn auth_post(uri: &str, ip: &str, body: Value) -> Request<Body> {
    Request::post(uri)
        .header("x-mandate", "1")
        .header(header::ORIGIN, WEB)
        .header(header::CONTENT_TYPE, "application/json")
        .header("cf-connecting-ip", ip)
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

    async fn confirm_password_reset(&self, token: &str, password: &str) -> Result<(), AuthError> {
        self.call(format!("reset-confirm {token}"));
        let Some(id) = token.strip_prefix("reset_") else {
            return Err(AuthError::CodeInvalid);
        };
        self.update(id, |u| u.password = password.to_owned());
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
        Ok(Self::auth_user(&u))
    }

    async fn send_verification_email(&self, user: &str) -> Result<(), AuthError> {
        self.call(format!("send-verification {user}"));
        Ok(())
    }

    async fn verify_email(&self, user: &str, code: &str) -> Result<AuthUser, AuthError> {
        self.call(format!("verify-email {user}"));
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
        let mut r = Request::get(uri).header("cf-connecting-ip", &self.ip);
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
    h.fake.force(AuthError::Step(Step::Sso {
        token: Secret("pat_x".into()),
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

/// Every log line of this test binary, through one global subscriber: thread-scoped
/// subscribers can miss events while other tests run in parallel.
#[derive(Clone, Default)]
struct Logs(Arc<Mutex<Vec<u8>>>);

impl std::io::Write for Logs {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(b);
        Ok(b.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Logs {
    type Writer = Logs;
    fn make_writer(&'a self) -> Logs {
        self.clone()
    }
}

static LOGS: std::sync::OnceLock<Logs> = std::sync::OnceLock::new();

impl Logs {
    fn get() -> &'static Logs {
        LOGS.get_or_init(|| {
            let logs = Logs::default();
            let sub = tracing_subscriber::fmt()
                .json()
                .with_writer(logs.clone())
                .finish();
            tracing::subscriber::set_global_default(sub).unwrap();
            logs
        })
    }

    /// `(type, member, actor)` of every event published about or by `id`.
    fn events(&self, id: &str) -> Vec<(String, Option<String>, String)> {
        let bytes = self.0.lock().unwrap().clone();
        String::from_utf8_lossy(&bytes)
            .lines()
            .filter_map(|l| serde_json::from_str::<Value>(l).ok())
            .filter(|v| v["fields"]["message"] == "event published")
            .map(|v| {
                let f = &v["fields"];
                let text = |k: &str| f[k].as_str().map(str::to_owned);
                (
                    text("event").unwrap(),
                    text("member"),
                    text("actor").unwrap(),
                )
            })
            .filter(|(_, member, actor)| member.as_deref() == Some(id) || actor == id)
            .collect()
    }
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
    let logs = Logs::get();
    let email = format!("new-{}@acme.test", Uuid::new_v4().simple());
    let v = body_json(h.post("/v1/auth/sign-up", sign_up_body(&email)).await).await;
    let r = h
        .post(
            "/v1/auth/verify-email",
            json!({ "pending": v["pending"], "code": GOOD_CODE }),
        )
        .await;
    let member = body_json(r).await["me"]["person"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(
        logs.events(&member),
        vec![
            ("organization.created.v1".into(), None, member.clone()),
            ("member.created.v1".into(), Some(member.clone()), member),
        ]
    );
}

#[tokio::test]
#[ignore]
async fn first_sign_in_records_member_created() {
    let h = Harness::new().await;
    let logs = Logs::get();
    let u = h.user("member");
    let body = json!({ "email": u.email, "password": u.password });
    let r = h.post("/v1/auth/password", body.clone()).await;
    let member = body_json(r).await["me"]["person"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(
        logs.events(&member),
        vec![(
            "member.created.v1".into(),
            Some(member.clone()),
            "system".into()
        )]
    );
    // The second sign-in creates nobody.
    h.post("/v1/auth/password", body).await;
    assert_eq!(logs.events(&member).len(), 1);
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
        let rows: Vec<(String, String, bool)> = sqlx::query_as(
            "select id, kind, disabled from agents where tenant_id = $1 order by id",
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
            false
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
            .all(|(_, kind, disabled)| kind == "personal" && *disabled)
    );
}
