//! WorkOS sign-in and directory sync. Verifier tests are offline (local RSA key, fake JWKS);
//! live tests need DATABASE_URL (a superuser), `--ignored`, and are fed recorded event JSON.
use async_trait::async_trait;
use axum::body::Body;
use ed25519_dalek::SigningKey;
use futures::stream::{self, BoxStream, StreamExt};
use http::{Request, StatusCode};
use http_body_util::BodyExt;
use jsonwebtoken::jwk::JwkSet;
use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
use mandate::assertion;
use mandate::identity::{
    EventSource, IdentityError, KeySource, TokenVerifier, UserLookup, Workos, WorkosClaims,
    sync_directory,
};
use mandate::ids::{AgentId, TenantId};
use mandate::mcp::{AppState, router};
use mandate::policy::Policy;
use mandate::store::{self, Store};
use mandate::tools::Catalog;
use serde_json::{Value, json};
use sqlx::{Connection, PgConnection, Row};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use time::{Duration, OffsetDateTime};
use tower::ServiceExt;
use uuid::Uuid;

const APP_PASSWORD: &str = "mandate_app_test";
const CLIENT_ID: &str = "client_test";
const ISSUER: &str = "https://api.workos.com/user_management/client_test";
const NOW: i64 = 1_800_000_000;

// Generated locally for these tests only (openssl genrsa 2048).
const RSA_PEM: &str = "-----BEGIN RSA PRIVATE KEY-----
MIIEpAIBAAKCAQEAnIEwImMhwZ1AM5VawKe2uoEioo2VCeeEmJL2hKdnTkcM8945
cVD2jNzaXK9Am4qic+QgScm8G1twJj5j2FNaygy4u+RaPFgfhQV7Hz8tynTppNSR
ClAzcGwJ6xQ9mcIUUkq8ijQixJ1Y6v6ZnhSp0ND+2pXJHxvTqApAYmVP58s8SCAH
P4CZ0k9jz8GWop+v1L6f3FXbXvzFM6QL+s9D2k8vm42CA+miQyCht3KqXMgmXLfb
nbS6yo1LYEHMURv9iBpCXGGi4+D39DhBI9b/ul6n5aDhcTNvdEDNxpgrEn2rqAxM
t5SwXFIgmj7/suuDlAAeQKG8Qd1He2VDWeAX7wIDAQABAoIBABR7o/0cKaguwzRS
cSo9yQs9KMygPcf/ZmSW929X1DbJKHInvNo5KnY31tcX6MYdWDMAFxRmiW6wgku/
EU+YDo3bKbe4T0cOkYbWDM3YyvlwYRPoQ4NXXnlxlRVl9MSm7gSchh3gbPHRViM2
EeI/+hRmW189s1uVnMH6ytUV8BCkswE7Ap6DPRhpw0g0QlGhUZKvBqA2DoYRbncB
MULW8cXI6GnrkOBP18S3YkIXR/bCey6a2WprLa4UnTzXEausSBLhoiheFNMOiBOT
g83ImTzpTPZUkCzjhDv/83r2mAUfNlf7mRwaH8Wp/rxJXgWzBMA/WDLGnuczR1JP
dCUW1aECgYEA0tokC1uQkuyerXqbsA4qkyWNFyxO02Uy9ynkVm4yxuL5tuxkmPmF
S/XLID8OlL3beYm4rK7J+mher2QYApGRh89IJtCC0Lab2rujg42pM/x/fUN1/u6D
7XvgyQsyFoVvxOGa8/qqIZVz8I++NG1CNPHxpzepGSBtwQbehJKK80sCgYEAvgP9
ilmijuLwUD1xrJ9Ar6+xZTaiB6dVtR0PQEXjyDx1ALzjvjmbXvXuHBGidg/yyQvN
jv/4ow6VlC5+mHCwWiEcnL18xVQ1YZrtGoIwnFc+qG+VJEmrN1RgCLRA3NjEVjmC
3RCMSZ6phvbhle9lGvYj3H0CI0uNkxowW2UR420CgYEAz38YpYywU8ZDyVi1SaZH
Q5Naq4lQ8TvP0egJ6vx7P7oyIE+2F8zF3qC+CrfDM6E7EPn1IQrJfYGkXFnWPwDe
BJZ53Kipc5eg/LpXCako8B31oGH5Bq3WaqjT50T7P6qE+W1DPgJZ1r7q37u1ZIZ4
9v154CMoZ6/AEoseA7zqvM0CgYEArJxXX6yZXQ+OmnWWo+dvDamejoEYo7dAxmwF
hK94gldZOjhc8lrGZ3ckvF7QpEtMk5GjQ/oHVahT/O5XlgcYTGez5DbF90PN0kK4
jNIjcoJWqw/h/brmTDbbnkyQd456nJFV7ZewuNfmzHHM7zUvNVTPsuXkYUeq7ixm
LUAdBekCgYAVJvT33LxcsCN1q6MlKsxjkt+eM9GlXTCrMKI+uIago5iMOJaCE6QW
/Eg8OGJvSL6f8n4fOlDTvXjrTVrVVq4g0RudBtyeYlGUWtsYiv2wVWLFh6mjH9Zy
zygjXBxBhSn6r5QdjVYVI8JkCvmy7YFRd0Fmpjnb9kCT29jR/dt0QA==
-----END RSA PRIVATE KEY-----";
const RSA_N: &str = "nIEwImMhwZ1AM5VawKe2uoEioo2VCeeEmJL2hKdnTkcM8945cVD2jNzaXK9Am4qic-QgScm8G1twJj5j2FNaygy4u-RaPFgfhQV7Hz8tynTppNSRClAzcGwJ6xQ9mcIUUkq8ijQixJ1Y6v6ZnhSp0ND-2pXJHxvTqApAYmVP58s8SCAHP4CZ0k9jz8GWop-v1L6f3FXbXvzFM6QL-s9D2k8vm42CA-miQyCht3KqXMgmXLfbnbS6yo1LYEHMURv9iBpCXGGi4-D39DhBI9b_ul6n5aDhcTNvdEDNxpgrEn2rqAxMt5SwXFIgmj7_suuDlAAeQKG8Qd1He2VDWeAX7w";

/// The shape `GET /sso/jwks/<client_id>` returns, one entry per kid (same key material).
fn jwks(kids: &[&str]) -> JwkSet {
    let keys: Vec<Value> = kids
        .iter()
        .map(|k| json!({ "alg": "RS256", "kty": "RSA", "use": "sig", "x5c": [], "n": RSA_N, "e": "AQAB", "kid": k, "x5t#S256": "" }))
        .collect();
    serde_json::from_value(json!({ "keys": keys })).unwrap()
}

/// Serves the JWKS versions in order (the last one repeats) and counts fetches; when `slow`,
/// each fetch waits for `release`.
struct FakeKeys {
    versions: Vec<JwkSet>,
    fetches: AtomicUsize,
    slow: AtomicBool,
    release: tokio::sync::Notify,
}

#[async_trait]
impl KeySource for FakeKeys {
    async fn keys(&self) -> Result<JwkSet, IdentityError> {
        let n = self.fetches.fetch_add(1, Ordering::SeqCst);
        if self.slow.load(Ordering::SeqCst) {
            self.release.notified().await;
        }
        Ok(self.versions[n.min(self.versions.len() - 1)].clone())
    }
}

fn verifier(versions: Vec<JwkSet>) -> (Arc<FakeKeys>, TokenVerifier) {
    let keys = Arc::new(FakeKeys {
        versions,
        fetches: AtomicUsize::new(0),
        slow: AtomicBool::new(false),
        release: tokio::sync::Notify::new(),
    });
    let v = TokenVerifier::with_source(keys.clone(), CLIENT_ID.into());
    (keys, v)
}

fn rs256(kid: &str, claims: Value) -> String {
    let mut h = Header::new(Algorithm::RS256);
    h.kid = Some(kid.into());
    encode(
        &h,
        &claims,
        &EncodingKey::from_rsa_pem(RSA_PEM.as_bytes()).unwrap(),
    )
    .unwrap()
}

fn claims(sub: &str, org: &str, exp: i64) -> Value {
    json!({ "iss": ISSUER, "sub": sub, "org_id": org, "exp": exp, "sid": "session_1" })
}

#[tokio::test]
async fn valid_token_verifies_and_hs256_or_expired_or_wrong_issuer_is_rejected() {
    let (_, v) = verifier(vec![jwks(&["k1"])]);
    let good = rs256("k1", claims("user_ann", "org_1", NOW + 60));
    assert_eq!(
        v.verify(&good, NOW).await.unwrap(),
        WorkosClaims {
            user: "user_ann".into(),
            org: "org_1".into(),
            expires_at: NOW + 60
        }
    );

    // HS256 keyed with the public modulus: the classic algorithm-confusion forgery.
    let mut h = Header::new(Algorithm::HS256);
    h.kid = Some("k1".into());
    let hs = encode(
        &h,
        &claims("user_ann", "org_1", NOW + 60),
        &EncodingKey::from_secret(RSA_N.as_bytes()).unwrap(),
    )
    .unwrap();
    // alg none: the good token's payload under an unsigned header.
    let payload = good.split('.').nth(1).unwrap();
    let none = format!("eyJhbGciOiJub25lIiwia2lkIjoiazEifQ.{payload}.");
    let wrong_iss = rs256(
        "k1",
        json!({ "iss": "https://api.workos.com/user_management/other", "sub": "user_ann", "org_id": "org_1", "exp": NOW + 60 }),
    );
    let no_org = rs256(
        "k1",
        json!({ "iss": ISSUER, "sub": "user_ann", "exp": NOW + 60 }),
    );
    for (name, t) in [
        ("hs256", hs),
        ("none", none),
        ("wrong issuer", wrong_iss),
        ("garbage", "a.b.c".into()),
    ] {
        assert!(
            matches!(v.verify(&t, NOW).await, Err(IdentityError::InvalidToken)),
            "{name}"
        );
    }
    assert!(matches!(
        v.verify(&no_org, NOW).await,
        Err(IdentityError::NoOrg)
    ));
    for exp in [NOW - 1, NOW] {
        let t = rs256("k1", claims("user_ann", "org_1", exp));
        assert!(matches!(
            v.verify(&t, NOW).await,
            Err(IdentityError::Expired)
        ));
    }
}

#[tokio::test]
async fn unknown_kid_refetches_keys_once() {
    let (keys, v) = verifier(vec![jwks(&["k1"]), jwks(&["k1", "k2"])]);
    let fetches = || keys.fetches.load(Ordering::SeqCst);
    let tok = |kid| rs256(kid, claims("user_ann", "org_1", NOW + 3600));

    v.verify(&tok("k1"), NOW).await.unwrap();
    v.verify(&tok("k1"), NOW + 1).await.unwrap();
    assert_eq!(fetches(), 1, "cached");
    // Rotation: an unknown kid triggers exactly one refetch, which finds it.
    v.verify(&tok("k2"), NOW + 2).await.unwrap();
    assert_eq!(fetches(), 2);
    // Unknown within a minute of that forced refetch: rejected from the cached set, no fetch.
    assert!(matches!(
        v.verify(&tok("k9"), NOW + 3).await,
        Err(IdentityError::InvalidToken)
    ));
    assert_eq!(fetches(), 2);
    // A minute later an unknown kid may force one refetch again; still unknown, rejected.
    assert!(matches!(
        v.verify(&tok("k9"), NOW + 62).await,
        Err(IdentityError::InvalidToken)
    ));
    assert_eq!(fetches(), 3);
    // The cache expires after ten minutes.
    v.verify(&tok("k1"), NOW + 63).await.unwrap();
    assert_eq!(fetches(), 3);
    v.verify(&tok("k1"), NOW + 62 + 600).await.unwrap();
    assert_eq!(fetches(), 4);
}

#[tokio::test]
async fn unknown_kids_cannot_flood_or_stall_key_fetches() {
    let (keys, v) = verifier(vec![jwks(&["k1"])]);
    let v = Arc::new(v);
    let fetches = || keys.fetches.load(Ordering::SeqCst);
    let tok = |kid: &str| rs256(kid, claims("user_ann", "org_1", NOW + 3600));
    v.verify(&tok("k1"), NOW).await.unwrap();
    for i in 0..50 {
        assert!(v.verify(&tok(&format!("x{i}")), NOW + 1 + i).await.is_err());
    }
    assert_eq!(fetches(), 2, "50 unknown kids in a minute: one extra fetch");

    // A slow forced refresh in flight does not hold up tokens whose kid is cached.
    keys.slow.store(true, Ordering::SeqCst);
    let v2 = v.clone();
    let stuck = tokio::spawn(async move { v2.verify(&tok("x99"), NOW + 120).await });
    while fetches() < 3 {
        tokio::task::yield_now().await;
    }
    let quick = tokio::time::timeout(
        std::time::Duration::from_secs(1),
        v.verify(
            &rs256("k1", claims("user_ann", "org_1", NOW + 3600)),
            NOW + 121,
        ),
    )
    .await
    .expect("a cached kid does not wait on the refresh");
    assert!(quick.is_ok());
    keys.release.notify_one();
    assert!(stuck.await.unwrap().is_err());
}

// ---- live ----

// Copied from tests/store.rs, plus the grants an operator gives on the RLS-free identity tables.
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
    for g in [
        "grant select, insert, update, delete on all tables in schema public to mandate_app",
        "revoke all on tenants, directory_cursor, directory_dead_letters from mandate_app",
        "grant select, insert on directory_dead_letters to mandate_app",
        "grant select, insert on tenants to mandate_app",
        "grant select, insert, update on directory_cursor to mandate_app",
    ] {
        sqlx::query(g).execute(&mut c).await.unwrap();
    }
    sqlx::query("select pg_advisory_unlock(4242)")
        .execute(&mut c)
        .await
        .unwrap();
    let host = url.split_once('@').unwrap().1;
    Store::connect(&format!("postgres://mandate_app:{APP_PASSWORD}@{host}"))
        .await
        .unwrap()
}

/// Serves a fixed batch whatever the cursor (so replays really replay) and records each cursor.
struct FakeEvents {
    batch: Vec<workos::EventSchema>,
    seen_after: Mutex<Vec<Option<String>>>,
}

impl EventSource for FakeEvents {
    fn events_after(
        &self,
        after: Option<String>,
    ) -> BoxStream<'_, Result<workos::EventSchema, IdentityError>> {
        self.seen_after.lock().unwrap().push(after);
        stream::iter(self.batch.clone().into_iter().map(Ok)).boxed()
    }
}

fn source(events: Vec<Value>) -> FakeEvents {
    FakeEvents {
        batch: events
            .into_iter()
            .map(|e| serde_json::from_value(e).unwrap())
            .collect(),
        seen_after: Mutex::new(Vec::new()),
    }
}

// Recorded shapes of WorkOS `dsync.*` events (trimmed of attributes Mandate ignores).
fn user(id: &str, org: &str, state: &str) -> Value {
    json!({
        "object": "directory_user", "id": id, "directory_id": "directory_01", "organization_id": org,
        "idp_id": format!("idp-{id}"), "email": format!("{id}@example.com"), "first_name": "A", "last_name": "B",
        "state": state, "raw_attributes": {}, "custom_attributes": {}, "role": { "slug": "member" },
        "created_at": "2026-11-01T00:00:00.000Z", "updated_at": "2026-11-01T00:00:00.000Z"
    })
}

fn group(id: &str, org: &str) -> Value {
    json!({
        "object": "directory_group", "id": id, "idp_id": format!("idp-{id}"), "directory_id": "directory_01",
        "organization_id": org, "name": "Engineering", "raw_attributes": {},
        "created_at": "2026-11-01T00:00:00.000Z", "updated_at": "2026-11-01T00:00:00.000Z"
    })
}

/// Event ids sort in stream order and are unique per test process (dead letters are keyed by them).
fn eid(n: usize) -> String {
    static RUN: std::sync::LazyLock<u128> = std::sync::LazyLock::new(|| {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    });
    format!("event_{:020}_{n:04}", *RUN)
}

fn event(n: usize, kind: &str, data: Value) -> Value {
    json!({
        "object": "event", "id": eid(n), "event": kind, "data": data,
        "created_at": "2026-11-21T00:00:00.000Z"
    })
}

fn membership(n: usize, kind: &str, u: &str, g: &str, org: &str) -> Value {
    event(
        n,
        kind,
        json!({ "directory_id": "directory_01", "user": user(u, org, "active"), "group": group(g, org) }),
    )
}

fn org() -> String {
    format!("org_{}", Uuid::new_v4().simple().to_string().to_uppercase())
}

/// Sync tests share the singleton cursor, so they take turns, each from a fresh cursor.
static SYNC: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

async fn reset_cursor() {
    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL");
    let mut c = PgConnection::connect(&url).await.unwrap();
    sqlx::query("delete from directory_cursor")
        .execute(&mut c)
        .await
        .unwrap();
}

/// AuthKit users: (id, email, org, verified).
struct FakeUsers(Vec<(String, String, String, bool)>);

#[async_trait]
impl UserLookup for FakeUsers {
    async fn user_by_email(&self, email: &str, org: &str) -> Result<Option<String>, IdentityError> {
        Ok(self
            .0
            .iter()
            .find(|u| u.3 && u.1.eq_ignore_ascii_case(email) && u.2 == org)
            .map(|u| u.0.clone()))
    }
    async fn verified_email(&self, user: &str) -> Result<Option<String>, IdentityError> {
        Ok(self
            .0
            .iter()
            .find(|u| u.0 == user && u.3)
            .map(|u| u.1.clone()))
    }
}

fn nobody() -> FakeUsers {
    FakeUsers(vec![])
}

fn authkit(
    id: &str,
    directory_user: &str,
    org: &str,
    verified: bool,
) -> (String, String, String, bool) {
    (
        id.into(),
        format!("{directory_user}@example.com"),
        org.into(),
        verified,
    )
}

async fn tenant_of(s: &Store, org: &str) -> TenantId {
    s.tenant_for_org(org)
        .await
        .unwrap()
        .expect("tenant created")
}

/// Ann and Bob in group Eng with Ann a member; agents: ann's, bob's and eng's.
async fn seed_directory(s: &Store, org: &str, users: &FakeUsers) -> TenantId {
    reset_cursor().await;
    let src = source(vec![
        event(
            1,
            "dsync.user.created",
            user("directory_user_ANN", org, "active"),
        ),
        event(
            2,
            "dsync.user.created",
            user("directory_user_BOB", org, "active"),
        ),
        event(3, "dsync.group.created", group("directory_group_ENG", org)),
        membership(
            4,
            "dsync.group.user_added",
            "directory_user_ANN",
            "directory_group_ENG",
            org,
        ),
    ]);
    assert_eq!(sync_directory(s, &src, users).await.unwrap(), 4);
    let t = tenant_of(s, org).await;
    let mut tx = s.begin(&t).await.unwrap();
    for (id, kind, m, team) in [
        ("ann-bot", "personal", Some("directory_user_ann"), None),
        ("bob-bot", "personal", Some("directory_user_bob"), None),
        ("eng-bot", "team", None, Some("directory_group_eng")),
    ] {
        sqlx::query("insert into agents (tenant_id, id, kind, owner_member, owner_team) values ($1, $2, $3, $4, $5)")
            .bind(t.as_str())
            .bind(id)
            .bind(kind)
            .bind(m)
            .bind(team)
            .execute(&mut *tx)
            .await
            .unwrap();
    }
    tx.commit().await.unwrap();
    t
}

fn app(store: Store, verifier: TokenVerifier, users: FakeUsers) -> axum::Router {
    let key = SigningKey::from_bytes(&[7; 32]);
    router(Arc::new(AppState {
        store,
        policy: Policy::load(Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../policies"
        )))
        .unwrap(),
        catalog: Catalog::new(vec![]),
        assertion_key: key.verifying_key(),
        assertion_signing_key: key,
        workos: Some(Arc::new(Workos {
            verifier,
            users: Arc::new(users),
        })),
    }))
}

async fn exchange(app: &axum::Router, bearer: &str, body: Value) -> (StatusCode, Value) {
    let req = Request::post("/v1/assertions")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {bearer}"))
        .body(Body::from(body.to_string()))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

#[tokio::test]
#[ignore]
async fn assertion_only_for_own_or_team_agents() {
    let _turn = SYNC.lock().await;
    let s = app_store().await;
    let o = org();
    // Ann's AuthKit account exists at sync time and is linked then; Bob's is linked at first sign-in.
    let t = seed_directory(
        &s,
        &o,
        &FakeUsers(vec![authkit("user_ann", "directory_user_ANN", &o, true)]),
    )
    .await;
    let (_, v) = verifier(vec![jwks(&["k1"])]);
    let app = app(
        s,
        v,
        FakeUsers(vec![
            authkit("user_ann", "directory_user_ANN", &o, true),
            authkit("user_bob", "directory_user_BOB", &o, true),
            authkit("user_eve", "eve", &o, true),
            authkit("user_zed", "directory_user_BOB", &o, false),
        ]),
    );
    let now = OffsetDateTime::now_utc().unix_timestamp();
    let ann = rs256("k1", claims("user_ann", &o, now + 600));

    for agent in ["ann-bot", "eng-bot"] {
        let (status, body) = exchange(&app, &ann, json!({ "agent": agent, "space": "s1" })).await;
        assert_eq!(status, StatusCode::OK, "{agent}: {body}");
        let a = assertion::verify(
            body["assertion"].as_str().unwrap(),
            &SigningKey::from_bytes(&[7; 32]).verifying_key(),
            now,
        )
        .unwrap();
        assert_eq!(a.tenant, t);
        assert_eq!(a.member.as_str(), "directory_user_ann");
        assert_eq!(a.agent, AgentId::parse(agent).unwrap());
        assert_eq!(a.space.as_ref().map(|s| s.as_str()), Some("s1"));
        assert!((now + 300..=now + 302).contains(&a.expires_at));
        assert_eq!(body["expires_at"], json!(a.expires_at));
    }
    // Another member's personal agent, an unknown agent, an unknown org, an AuthKit user whose
    // verified email matches no member, one whose matching email is unverified, and a
    // directory id as `sub` (not an AuthKit user): 403.
    let tok = |sub: &str, org: &str| rs256("k1", claims(sub, org, now + 600));
    let (stranger_org, eve, zed, dir) = (
        tok("user_ann", &org()),
        tok("user_eve", &o),
        tok("user_zed", &o),
        tok("directory_user_ANN", &o),
    );
    for (bearer, agent) in [
        (&ann, "bob-bot"),
        (&ann, "nobody"),
        (&stranger_org, "ann-bot"),
        (&eve, "bob-bot"),
        (&zed, "bob-bot"),
        (&dir, "ann-bot"),
    ] {
        let (status, _) = exchange(&app, bearer, json!({ "agent": agent })).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{agent}");
    }
    // Bob signs in for the first time: linked by verified email, then served.
    let bob = tok("user_bob", &o);
    let (status, body) = exchange(&app, &bob, json!({ "agent": "bob-bot" })).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let a = assertion::verify(
        body["assertion"].as_str().unwrap(),
        &SigningKey::from_bytes(&[7; 32]).verifying_key(),
        now,
    )
    .unwrap();
    assert_eq!(a.member.as_str(), "directory_user_bob");
    // A bad or missing WorkOS token is 401, and a Mandate assertion is not a WorkOS token.
    let (status, _) = exchange(&app, "garbage", json!({ "agent": "ann-bot" })).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let expired = rs256("k1", claims("user_ann", &o, now - 1));
    let (status, _) = exchange(&app, &expired, json!({ "agent": "ann-bot" })).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
#[ignore]
async fn removed_user_loses_grants_and_pending_approvals() {
    let _turn = SYNC.lock().await;
    let s = app_store().await;
    let o = org();
    let t = seed_directory(&s, &o, &nobody()).await;
    let (ann, bob) = ("directory_user_ann", "directory_user_bob");
    let later = OffsetDateTime::now_utc() + Duration::hours(1);
    let mut tx = s.begin(&t).await.unwrap();
    for (kind, id, by) in [
        ("member", ann, "admin"),
        ("agent", "bob-bot", ann),
        ("team", "directory_group_eng", bob),
    ] {
        sqlx::query("insert into grants (tenant_id, principal_kind, principal_id, tool, expires_at, granted_by) values ($1, $2, $3, 'web.read_page', $4, $5)")
            .bind(t.as_str()).bind(kind).bind(id).bind(later).bind(by)
            .execute(&mut *tx).await.unwrap();
    }
    // (agent, requested_by, approved_by, used): which survive Ann's removal is asserted below.
    for (agent, req, appr, used) in [
        ("bob-bot", ann, None, false),      // she requested it
        ("ann-bot", bob, None, false),      // she could approve it
        ("eng-bot", bob, None, false),      // she could approve it via Eng
        ("bob-bot", bob, Some(ann), false), // she approved it, not yet used
        ("bob-bot", bob, None, false),      // nothing to do with her: stays
        ("bob-bot", ann, Some(bob), true),  // spent: history, stays
    ] {
        sqlx::query("insert into approvals (tenant_id, id, agent, tool, args_hash, requested_by, approved_by, expires_at, used_at) values ($1, $2, $3, 'web.submit_form', '\\x00', $4, $5, $6, $7)")
            .bind(t.as_str()).bind(Uuid::new_v4()).bind(agent).bind(req).bind(appr).bind(later)
            .bind(used.then_some(later))
            .execute(&mut *tx).await.unwrap();
    }
    tx.commit().await.unwrap();

    let gone = source(vec![event(
        5,
        "dsync.user.deleted",
        user("directory_user_ANN", &o, "inactive"),
    )]);
    assert_eq!(sync_directory(&s, &gone, &nobody()).await.unwrap(), 1);

    let mut tx = s.begin(&t).await.unwrap();
    let members: Vec<String> = sqlx::query_scalar("select id from members order by id")
        .fetch_all(&mut *tx)
        .await
        .unwrap();
    assert_eq!(members, [bob]);
    let tm: i64 = sqlx::query_scalar("select count(*) from team_members")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert_eq!(tm, 0);
    let grants: Vec<String> = sqlx::query_scalar("select principal_id from grants")
        .fetch_all(&mut *tx)
        .await
        .unwrap();
    assert_eq!(grants, ["directory_group_eng"]);
    let left: Vec<(String, String, bool)> = sqlx::query(
        "select agent, requested_by, used_at is not null as used from approvals order by used",
    )
    .fetch_all(&mut *tx)
    .await
    .unwrap()
    .into_iter()
    .map(|r| (r.get(0), r.get(1), r.get(2)))
    .collect();
    assert_eq!(
        left,
        [
            ("bob-bot".into(), bob.into(), false),
            ("bob-bot".into(), ann.into(), true)
        ]
    );
    // Her personal agent is disabled: the gate no longer finds it; Bob's still works.
    let ann_bot = AgentId::parse("ann-bot").unwrap();
    assert!(
        store::agent_parents(&mut tx, &ann_bot)
            .await
            .unwrap()
            .is_none()
    );
    let bob_bot = AgentId::parse("bob-bot").unwrap();
    assert!(
        store::agent_parents(&mut tx, &bob_bot)
            .await
            .unwrap()
            .is_some()
    );
    drop(tx);

    // Deactivation (suspended) has the same effect as deletion.
    let suspended = source(vec![event(
        6,
        "dsync.user.updated",
        user("directory_user_BOB", &o, "suspended"),
    )]);
    sync_directory(&s, &suspended, &nobody()).await.unwrap();
    let mut tx = s.begin(&t).await.unwrap();
    let n: i64 = sqlx::query_scalar("select count(*) from members")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert_eq!(n, 0);
    assert!(
        store::agent_parents(&mut tx, &bob_bot)
            .await
            .unwrap()
            .is_none()
    );
}

async fn snapshot(s: &Store, t: &TenantId) -> Vec<String> {
    let mut tx = s.begin(t).await.unwrap();
    let mut out = Vec::new();
    for q in [
        "select id || ':' || coalesce(workos_directory_user_id, '') || ':' || coalesce(workos_user_id, '') from members order by 1",
        "select id || ':' || coalesce(workos_group_id, '') from teams order by 1",
        "select team_id || ':' || member_id from team_members order by 1",
    ] {
        let rows: Vec<String> = sqlx::query_scalar(sqlx::AssertSqlSafe(q))
            .fetch_all(&mut *tx)
            .await
            .unwrap();
        out.push(rows.join(","));
    }
    out
}

#[tokio::test]
#[ignore]
async fn replayed_events_change_nothing() {
    let _turn = SYNC.lock().await;
    let s = app_store().await;
    let o = org();
    reset_cursor().await;
    let users = FakeUsers(vec![authkit("user_ann", "directory_user_ANN", &o, true)]);
    let src = source(vec![
        event(
            1,
            "dsync.user.created",
            user("directory_user_ANN", &o, "active"),
        ),
        event(
            2,
            "dsync.user.created",
            user("directory_user_BOB", &o, "active"),
        ),
        event(
            3,
            "dsync.user.created",
            user("directory_user_CY", &o, "active"),
        ),
        event(4, "dsync.group.created", group("directory_group_ENG", &o)),
        membership(
            5,
            "dsync.group.user_added",
            "directory_user_ANN",
            "directory_group_ENG",
            &o,
        ),
        membership(
            6,
            "dsync.group.user_added",
            "directory_user_BOB",
            "directory_group_ENG",
            &o,
        ),
        membership(
            7,
            "dsync.group.user_removed",
            "directory_user_BOB",
            "directory_group_ENG",
            &o,
        ),
        event(
            8,
            "dsync.user.updated",
            user("directory_user_BOB", &o, "active"),
        ),
        event(
            9,
            "dsync.user.deleted",
            user("directory_user_CY", &o, "inactive"),
        ),
        event(10, "dsync.group.updated", group("directory_group_ENG", &o)),
    ]);
    assert_eq!(sync_directory(&s, &src, &users).await.unwrap(), 10);
    let t = tenant_of(&s, &o).await;
    let first = snapshot(&s, &t).await;
    assert_eq!(
        first,
        [
            "directory_user_ann:directory_user_ANN:user_ann,directory_user_bob:directory_user_BOB:",
            "directory_group_eng:directory_group_ENG",
            "directory_group_eng:directory_user_ann",
        ]
    );
    // The cursor was committed with the last event: the next pull starts after it, and an
    // event the source sends again anyway is skipped.
    assert_eq!(sync_directory(&s, &src, &users).await.unwrap(), 0);
    assert_eq!(
        src.seen_after.lock().unwrap().last().unwrap().as_deref(),
        Some(eid(10).as_str())
    );
    // With the cursor gone, the whole batch really is applied again: same rows.
    reset_cursor().await;
    assert_eq!(sync_directory(&s, &src, &users).await.unwrap(), 10);
    assert_eq!(snapshot(&s, &t).await, first);
    // Deleting the group drops its memberships.
    let del = source(vec![event(
        11,
        "dsync.group.deleted",
        group("directory_group_ENG", &o),
    )]);
    sync_directory(&s, &del, &users).await.unwrap();
    assert_eq!(snapshot(&s, &t).await[1..], ["", ""]);
}

#[tokio::test]
#[ignore]
async fn concurrent_syncs_apply_each_event_once() {
    let _turn = SYNC.lock().await;
    let s = app_store().await;
    let o = org();
    reset_cursor().await;
    let src = source(vec![
        event(
            1,
            "dsync.user.created",
            user("directory_user_ANN", &o, "active"),
        ),
        event(
            2,
            "dsync.user.deleted",
            user("directory_user_ANN", &o, "inactive"),
        ),
        event(
            3,
            "dsync.user.created",
            user("directory_user_BOB", &o, "active"),
        ),
    ]);
    let users = nobody();
    let (a, b) = tokio::join!(
        sync_directory(&s, &src, &users),
        sync_directory(&s, &src, &users)
    );
    assert_eq!(a.unwrap() + b.unwrap(), 3);
    assert_eq!(s.directory_cursor().await.unwrap(), Some(eid(3)));
    let t = tenant_of(&s, &o).await;
    assert_eq!(
        snapshot(&s, &t).await[0],
        "directory_user_bob:directory_user_BOB:"
    );
}

#[tokio::test]
#[ignore]
async fn bad_events_are_dead_lettered_and_removals_keep_flowing() {
    let _turn = SYNC.lock().await;
    let s = app_store().await;
    let (o, other) = (org(), org());
    reset_cursor().await;
    let mut cross = membership(
        3,
        "dsync.group.user_added",
        "directory_user_ANN",
        "directory_group_ENG",
        &o,
    );
    cross["data"]["user"]["organization_id"] = json!(other);
    let src = source(vec![
        event(
            1,
            "dsync.user.created",
            user("directory_user_ANN", &o, "active"),
        ),
        event(2, "dsync.user.created", json!({ "unexpected": true })),
        cross,
        event(
            4,
            "dsync.user.deleted",
            user("directory_user_ANN", &o, "inactive"),
        ),
    ]);
    assert_eq!(sync_directory(&s, &src, &nobody()).await.unwrap(), 2);
    assert_eq!(s.directory_cursor().await.unwrap(), Some(eid(4)));
    let t = tenant_of(&s, &o).await;
    assert_eq!(
        snapshot(&s, &t).await,
        ["", "", ""],
        "removal applied, cross-org join not"
    );
    let url = std::env::var("DATABASE_URL").unwrap();
    let mut c = PgConnection::connect(&url).await.unwrap();
    let dead: Vec<(String, Option<String>)> = sqlx::query(
        "select event_id, org_id from directory_dead_letters where event_id = any($1) order by event_id",
    )
    .bind(vec![eid(2), eid(3)])
    .fetch_all(&mut c)
    .await
    .unwrap()
    .into_iter()
    .map(|r| (r.get(0), r.get(1)))
    .collect();
    assert_eq!(dead, [(eid(2), None), (eid(3), Some(o.clone()))]);
}
