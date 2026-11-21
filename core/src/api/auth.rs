//! Sign-in through WorkOS's headless User Management API. WorkOS proves the person; the core
//! decides which tenant and member that is and issues its own session. The browser never sees
//! a WorkOS token, and WorkOS's error text never leaves this module.
use super::session::{self, NewPending};
use super::{ApiError, ApiState, Input, ratelimit};
use crate::identity::slug;
use crate::ids::{MemberId, TenantId};
use crate::store::{self, StoreError, Tx};
use async_trait::async_trait;
use axum::Router;
use axum::extract::State;
use axum::middleware;
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use http::{StatusCode, header};
use serde::{Deserialize, Deserializer};
use serde_json::{Value, json};
use std::fmt;
use std::sync::Arc;
use time::OffsetDateTime;

/// A WorkOS secret (a pending authentication token, a TOTP secret); never printed.
#[derive(Clone, PartialEq, Eq)]
pub struct Secret(pub String);

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(..)")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthUser {
    pub id: String,
    pub email: String,
    pub email_verified: bool,
    pub name: Option<String>,
    pub avatar_url: Option<String>,
}

/// A finished WorkOS authentication: who, and in which organization (if WorkOS chose one).
#[derive(Debug, Clone)]
pub struct Authenticated {
    pub user: AuthUser,
    pub org: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrgRef {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Membership {
    pub org: String,
    pub org_name: String,
    pub role: String,
    pub active: bool,
}

#[derive(Debug, Clone)]
pub struct TotpEnrollment {
    pub factor: String,
    pub challenge: String,
    pub secret: Secret,
    /// The `otpauth://` URI the QR code encodes.
    pub uri: Secret,
}

#[derive(Debug, Clone)]
pub struct Invitation {
    pub email: String,
    pub org: Option<String>,
    pub inviter: Option<String>,
    pub pending: bool,
}

/// Another step WorkOS asks for before it finishes a sign-in.
#[derive(Debug, Clone)]
pub enum Step {
    /// A TOTP code for the user's enrolled factor.
    Mfa {
        token: Secret,
        factor: String,
    },
    /// Two-step is required and the user has no factor yet.
    MfaSetup {
        token: Secret,
        user: String,
    },
    ChooseOrg {
        token: Secret,
        orgs: Vec<OrgRef>,
    },
    VerifyEmail {
        token: Secret,
        email: String,
        user: Option<String>,
    },
    Sso {
        token: Secret,
        connection: String,
    },
    Methods {
        token: Secret,
        methods: Vec<String>,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("another sign-in step is needed")]
    Step(Step),
    #[error("invalid credentials")]
    InvalidCredentials,
    #[error("invalid code")]
    CodeInvalid,
    #[error("expired code")]
    CodeExpired,
    #[error("pending step expired")]
    PendingExpired,
    #[error("rejected input")]
    Invalid,
    #[error("not found")]
    NotFound,
    #[error("already exists")]
    Conflict,
    /// By kind only (status and WorkOS error code), never WorkOS's message.
    #[error("workos request failed: {0}")]
    Upstream(String),
}

/// Every WorkOS call the sign-in API makes; WorkOS in production, a fake in tests.
#[async_trait]
pub trait AuthProvider: Send + Sync {
    async fn authenticate_with_password(
        &self,
        email: &str,
        password: &str,
        invitation: Option<&str>,
    ) -> Result<Authenticated, AuthError>;
    async fn create_magic_auth(&self, email: &str) -> Result<(), AuthError>;
    async fn authenticate_with_magic_auth(
        &self,
        email: &str,
        code: &str,
        invitation: Option<&str>,
    ) -> Result<Authenticated, AuthError>;
    async fn authenticate_with_email_verification(
        &self,
        token: &str,
        code: &str,
    ) -> Result<Authenticated, AuthError>;
    async fn authenticate_with_totp(
        &self,
        token: &str,
        challenge: &str,
        code: &str,
    ) -> Result<Authenticated, AuthError>;
    async fn authenticate_with_organization_selection(
        &self,
        token: &str,
        org: &str,
    ) -> Result<Authenticated, AuthError>;
    /// The SSO callback's authorization code.
    async fn authenticate_with_code(&self, code: &str) -> Result<Authenticated, AuthError>;
    /// The active SSO connection for an email domain, if any.
    async fn sso_connection(&self, domain: &str) -> Result<Option<String>, AuthError>;
    fn sso_authorization_url(
        &self,
        connection: &str,
        state: &str,
        redirect_uri: &str,
    ) -> Result<String, AuthError>;
    async fn create_password_reset(&self, email: &str) -> Result<(), AuthError>;
    async fn confirm_password_reset(&self, token: &str, password: &str) -> Result<(), AuthError>;
    async fn invitation(&self, token: &str) -> Result<Invitation, AuthError>;
    async fn user(&self, id: &str) -> Result<AuthUser, AuthError>;
    async fn create_user(
        &self,
        email: &str,
        password: &str,
        name: &str,
    ) -> Result<AuthUser, AuthError>;
    async fn send_verification_email(&self, user: &str) -> Result<(), AuthError>;
    async fn verify_email(&self, user: &str, code: &str) -> Result<AuthUser, AuthError>;
    async fn organization(&self, id: &str) -> Result<OrgRef, AuthError>;
    async fn create_organization(&self, name: &str) -> Result<OrgRef, AuthError>;
    async fn create_membership(&self, user: &str, org: &str, role: &str) -> Result<(), AuthError>;
    async fn memberships(&self, user: &str) -> Result<Vec<Membership>, AuthError>;
    async fn enroll_auth_factor(&self, user: &str) -> Result<TotpEnrollment, AuthError>;
    /// A new challenge on a factor; its id.
    async fn challenge_factor(&self, factor: &str) -> Result<String, AuthError>;
    async fn verify_challenge(&self, challenge: &str, code: &str) -> Result<bool, AuthError>;
    async fn has_totp(&self, user: &str) -> Result<bool, AuthError>;
}

impl From<AuthError> for ApiError {
    fn from(e: AuthError) -> Self {
        match e {
            AuthError::InvalidCredentials => ApiError::new(
                StatusCode::UNAUTHORIZED,
                "invalid_credentials",
                "That email and password do not match.",
            ),
            AuthError::CodeInvalid => ApiError::new(
                StatusCode::BAD_REQUEST,
                "code_invalid",
                "That code is not right.",
            ),
            AuthError::CodeExpired => ApiError::new(
                StatusCode::BAD_REQUEST,
                "code_expired",
                "That code has expired. Ask for a new one.",
            ),
            AuthError::PendingExpired => pending_expired(),
            AuthError::Invalid => ApiError::invalid_input(),
            AuthError::NotFound => ApiError::new(StatusCode::NOT_FOUND, "not_found", "Not found."),
            AuthError::Conflict => {
                ApiError::new(StatusCode::CONFLICT, "conflict", "That already exists.")
            }
            // A step where none is expected (for example after SSO) is not something we can finish.
            AuthError::Step(_) => no_access(),
            AuthError::Upstream(kind) => {
                tracing::warn!(%kind, "workos request failed");
                ApiError::new(
                    StatusCode::BAD_GATEWAY,
                    "internal",
                    "Sign-in is unavailable right now. Please try again.",
                )
            }
        }
    }
}

fn no_access() -> ApiError {
    ApiError::new(
        StatusCode::FORBIDDEN,
        "no_access",
        "You do not have access to this organization.",
    )
}

fn pending_expired() -> ApiError {
    ApiError::new(
        StatusCode::BAD_REQUEST,
        "pending_expired",
        "This sign-in has expired. Please start again.",
    )
}

/// Logs a failure the caller must not learn about (no account discovery), by kind only.
fn quietly(what: &str, r: Result<(), AuthError>) {
    match r {
        Ok(()) | Err(AuthError::NotFound | AuthError::Invalid) => {}
        Err(AuthError::Upstream(kind)) => tracing::warn!(%kind, what, "workos request failed"),
        Err(e) => tracing::info!(error = %e, what, "workos refused"),
    }
}

fn bounded<'de, D: Deserializer<'de>>(
    d: D,
    max: usize,
    ok: impl Fn(char) -> bool,
) -> Result<String, D::Error> {
    let s = String::deserialize(d)?;
    if s.is_empty() || s.chars().count() > max || !s.chars().all(ok) {
        return Err(serde::de::Error::custom("invalid value"));
    }
    Ok(s)
}

/// An email as typed: trimmed, at most 254 characters, one `@` with text on both sides.
#[derive(Debug, Clone)]
pub struct Email(String);

impl Email {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for Email {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        let s = s.trim();
        let ok = s.len() <= 254
            && !s.chars().any(|c| c.is_whitespace() || c.is_control())
            && s.split_once('@').is_some_and(|(local, domain)| {
                !local.is_empty() && domain.contains('.') && !domain.contains('@')
            });
        if !ok {
            return Err(serde::de::Error::custom("invalid email"));
        }
        Ok(Email(s.to_owned()))
    }
}

/// A typed password, passed to WorkOS and never stored or printed.
#[derive(Clone)]
pub struct Password(String);

impl fmt::Debug for Password {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Password(..)")
    }
}

impl<'de> Deserialize<'de> for Password {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        bounded(d, 256, |c| c != '\0').map(Password)
    }
}

/// A one-time code (email or TOTP): letters and digits.
#[derive(Debug, Clone)]
pub struct Code(String);

impl<'de> Deserialize<'de> for Code {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        bounded(d, 64, |c| c.is_ascii_alphanumeric()).map(Code)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EmailOnly {
    email: Email,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EmailCode {
    email: Email,
    code: Code,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EmailPassword {
    email: Email,
    password: Password,
}

pub(crate) fn routes(state: &Arc<ApiState>) -> Router<Arc<ApiState>> {
    Router::new()
        .route("/v1/auth/code/start", post(code_start))
        .route("/v1/auth/code/verify", post(code_verify))
        .route("/v1/auth/password", post(password))
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            ratelimit::limit,
        ))
}

async fn code_start(State(s): State<Arc<ApiState>>, Input(b): Input<EmailOnly>) -> StatusCode {
    quietly(
        "magic code",
        s.auth.create_magic_auth(b.email.as_str()).await,
    );
    StatusCode::ACCEPTED
}

async fn code_verify(
    State(s): State<Arc<ApiState>>,
    Input(b): Input<EmailCode>,
) -> Result<Response, ApiError> {
    let r = s
        .auth
        .authenticate_with_magic_auth(b.email.as_str(), &b.code.0, None)
        .await;
    outcome(&s, r).await
}

async fn password(
    State(s): State<Arc<ApiState>>,
    Input(b): Input<EmailPassword>,
) -> Result<Response, ApiError> {
    let r = s
        .auth
        .authenticate_with_password(b.email.as_str(), &b.password.0, None)
        .await;
    outcome(&s, r).await
}

/// A new session and the `Me` it shows; becomes the JSON answer or the SSO redirect.
pub(crate) struct SignedIn {
    token: String,
    me: Value,
}

impl SignedIn {
    fn into_json(self) -> Result<Response, ApiError> {
        let mut r = axum::Json(json!({ "status": "signed_in", "me": self.me })).into_response();
        r.headers_mut()
            .insert(header::SET_COOKIE, session::set_cookie(&self.token)?);
        Ok(r)
    }
}

/// Spec 4.2: signed in, another step, or an error.
async fn outcome(s: &ApiState, r: Result<Authenticated, AuthError>) -> Result<Response, ApiError> {
    match r {
        Ok(a) => finish(s, a).await?.into_json(),
        Err(AuthError::Step(step)) => next(s, step).await,
        Err(e) => Err(e.into()),
    }
}

/// An org as the web app sees it; its id is the tenant id the org has (or will have).
fn org_json(id: &str, name: &str, summary: (i64, i64)) -> Value {
    json!({ "id": id, "name": name, "summary": { "people": summary.0, "agents": summary.1 } })
}

/// Stores a tenantless step for 10 minutes; returns the id the browser holds.
async fn store_pending(
    s: &ApiState,
    kind: &'static str,
    workos_token: Option<&Secret>,
    email: Option<&str>,
    data: Value,
) -> Result<String, ApiError> {
    let mut tx = s.store.begin_untenanted().await?;
    let id = session::create_pending(
        &mut tx,
        NewPending {
            kind,
            workos_token: workos_token.map(|t| t.0.as_str()),
            email,
            data,
            tenant: None,
        },
        OffsetDateTime::now_utc(),
    )
    .await?;
    tx.commit().await?;
    Ok(id)
}

/// An SSO redirect whose state is a pending step bound to this browser by a cookie.
async fn sso_redirect(
    s: &ApiState,
    connection: &str,
    workos_token: Option<&Secret>,
) -> Result<(String, String, http::HeaderValue), ApiError> {
    let state = store_pending(s, "sso", workos_token, None, json!({})).await?;
    let url = s
        .auth
        .sso_authorization_url(connection, &state, &s.sso_redirect_uri)?;
    let cookie = session::sso_cookie(&state)?;
    Ok((state, url, cookie))
}

/// Stores the step (with WorkOS's token) and tells the browser what comes next.
async fn next(s: &ApiState, step: Step) -> Result<Response, ApiError> {
    let mut body = json!({ "status": "next" });
    let (next, kind, token, email, data) = match step {
        Step::Mfa { token, factor } => {
            let challenge = s.auth.challenge_factor(&factor).await?;
            ("mfa", "mfa", token, None, json!({ "challenge": challenge }))
        }
        Step::MfaSetup { token, user } => (
            "mfa_setup",
            "mfa_setup",
            token,
            None,
            json!({ "user": user }),
        ),
        Step::ChooseOrg { token, orgs } => {
            body["orgs"] = orgs
                .iter()
                .map(|o| org_json(&slug(&o.id), &o.name, (0, 0)))
                .collect();
            let offered: Vec<Value> = orgs
                .iter()
                .map(|o| json!({ "id": o.id, "name": o.name }))
                .collect();
            (
                "choose_org",
                "choose_org",
                token,
                None,
                json!({ "orgs": offered }),
            )
        }
        Step::VerifyEmail { token, email, user } => (
            "verify_email",
            "verify_email",
            token,
            Some(email),
            json!({ "user": user }),
        ),
        Step::Sso { token, connection } => {
            let (pending, url, cookie) = sso_redirect(s, &connection, Some(&token)).await?;
            body["next"] = json!("sso_required");
            body["pending"] = json!(pending);
            body["redirect_url"] = json!(url);
            let mut r = axum::Json(body).into_response();
            r.headers_mut().insert(header::SET_COOKIE, cookie);
            return Ok(r);
        }
        Step::Methods { token, methods } => {
            body["methods"] = json!(methods);
            ("method_required", "method_required", token, None, json!({}))
        }
    };
    let pending = store_pending(s, kind, Some(&token), email.as_deref(), data).await?;
    body["next"] = json!(next);
    body["pending"] = json!(pending);
    Ok(axum::Json(body).into_response())
}

/// WorkOS proved the person: map the org to its tenant, the user to a member, and sign in.
async fn finish(s: &ApiState, a: Authenticated) -> Result<SignedIn, ApiError> {
    let org = a.org.ok_or_else(no_access)?;
    let memberships = s.auth.memberships(&a.user.id).await?;
    let current = memberships
        .iter()
        .find(|m| m.active && m.org == org)
        .ok_or_else(no_access)?;
    let is_admin = current.role == "admin";
    let mfa = s.auth.has_totp(&a.user.id).await?;
    let tenant = tenant_for(s, &org).await?;
    let profile = profile(&a.user, &current.org_name, &memberships, mfa);
    let mut tx = s.store.begin(&tenant).await?;
    let member = member_for(&mut tx, &a.user).await?;
    sign_in(tx, &tenant, &member, is_admin, &profile).await
}

/// The org's tenant, created on its first sign-in (orgs without directory sync still work).
async fn tenant_for(s: &ApiState, org: &str) -> Result<TenantId, ApiError> {
    if let Some(t) = s.store.tenant_for_org(org).await? {
        return Ok(t);
    }
    let id = TenantId::parse(&slug(org)).map_err(|_| no_access())?;
    // `None`: the id belongs to another org. Fail closed rather than share a tenant.
    s.store.ensure_tenant(org, &id).await?.ok_or_else(no_access)
}

fn profile(user: &AuthUser, org_name: &str, memberships: &[Membership], mfa: bool) -> Value {
    let orgs: Vec<Value> = memberships
        .iter()
        .filter(|m| m.active)
        .map(|m| json!({ "id": slug(&m.org), "name": m.org_name }))
        .collect();
    json!({
        "name": user.name.clone().unwrap_or_else(|| user.email.clone()),
        "email": user.email,
        "avatar_url": user.avatar_url,
        "org_name": org_name,
        "orgs": orgs,
        "mfa_enabled": mfa,
    })
}

/// The member this WorkOS user is in the transaction's tenant: already linked, else the one
/// directory member with their verified email, else a new member (first sign-in).
async fn member_for(tx: &mut Tx, user: &AuthUser) -> Result<MemberId, ApiError> {
    if let Some(m) = store::member_for_workos_user(tx, &user.id).await? {
        return Ok(m);
    }
    if user.email_verified
        && let Some(m) = store::link_member_by_email(tx, &user.email, &user.id).await?
    {
        return Ok(m);
    }
    if let Some(id) = insert_member(tx, user).await? {
        events::publish(
            tx,
            events::Event::MemberCreated {
                member: id.clone(),
                actor: events::Actor::System,
            },
        )
        .await?;
        return Ok(id);
    }
    // A concurrent first sign-in won; anything else holding the id is someone else: refuse.
    store::member_for_workos_user(tx, &user.id)
        .await?
        .ok_or_else(no_access)
}

/// A new member for this WorkOS user, its id taken from the user id; `None` if that is taken.
async fn insert_member(tx: &mut Tx, user: &AuthUser) -> Result<Option<MemberId>, ApiError> {
    let id = MemberId::parse(&slug(&user.id)).map_err(|_| no_access())?;
    let created: Option<String> = sqlx::query_scalar(
        "insert into members (tenant_id, id, workos_user_id, email) \
         values (current_setting('mandate.tenant'), $1, $2, $3) on conflict do nothing returning id",
    )
    .bind(id.as_str())
    .bind(&user.id)
    .bind(&user.email)
    .fetch_optional(&mut **tx)
    .await?;
    Ok(created.map(|_| id))
}

/// The member's personal operator, created if they have none; a disabled one stays disabled.
async fn ensure_personal_agent(tx: &mut Tx, member: &MemberId) -> Result<(), StoreError> {
    let id: String = format!("op-{member}").chars().take(64).collect();
    sqlx::query(
        "insert into agents (tenant_id, id, kind, owner_member) \
         select current_setting('mandate.tenant'), $1, 'personal', $2 \
         where not exists (select 1 from agents where tenant_id = current_setting('mandate.tenant', true) \
           and kind = 'personal' and owner_member = $2) \
         on conflict do nothing",
    )
    .bind(id)
    .bind(member.as_str())
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// Admin follows the WorkOS role at every sign-in; a session, and the `Me` to show.
async fn sign_in(
    mut tx: Tx,
    tenant: &TenantId,
    member: &MemberId,
    is_admin: bool,
    profile: &Value,
) -> Result<SignedIn, ApiError> {
    ensure_personal_agent(&mut tx, member).await?;
    sqlx::query(
        "update members set is_admin = $2 where tenant_id = current_setting('mandate.tenant', true) and id = $1",
    )
    .bind(member.as_str())
    .bind(is_admin)
    .execute(&mut *tx)
    .await?;
    let token =
        session::create(&mut tx, tenant, member, profile, OffsetDateTime::now_utc()).await?;
    let me = me(&mut tx, tenant, member, is_admin, profile).await?;
    tx.commit().await?;
    Ok(SignedIn { token, me })
}

/// `Me` (spec 4.3) from the session's profile and the tenant's own tables.
pub(crate) async fn me(
    tx: &mut Tx,
    tenant: &TenantId,
    member: &MemberId,
    is_admin: bool,
    profile: &Value,
) -> Result<Value, StoreError> {
    let (people, agents): (i64, i64) = sqlx::query_as(
        "select (select count(*) from members where tenant_id = current_setting('mandate.tenant', true)), \
                (select count(*) from agents where tenant_id = current_setting('mandate.tenant', true) and not disabled)",
    )
    .fetch_one(&mut **tx)
    .await?;
    let teams: Vec<String> = sqlx::query_scalar(
        "select team_id from team_members where tenant_id = current_setting('mandate.tenant', true) \
         and member_id = $1 order by team_id",
    )
    .bind(member.as_str())
    .fetch_all(&mut **tx)
    .await?;
    let name = |v: &Value| v.as_str().unwrap_or_default().to_owned();
    // Counts only for the tenant this session is in; other orgs are not read from here.
    let orgs: Vec<Value> = profile["orgs"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|o| {
            let id = name(&o["id"]);
            let summary = if id == tenant.as_str() {
                (people, agents)
            } else {
                (0, 0)
            };
            org_json(&id, &name(&o["name"]), summary)
        })
        .collect();
    Ok(json!({
        "person": {
            "id": member,
            "name": profile["name"],
            "email": profile["email"],
            "avatar_url": profile["avatar_url"],
        },
        "org": org_json(tenant.as_str(), &name(&profile["org_name"]), (people, agents)),
        "orgs": orgs,
        "mfa_enabled": profile["mfa_enabled"].as_bool().unwrap_or(false),
        "is_admin": is_admin,
        // ponytail: team names arrive with lane C's `teams.name`; the id stands in until then.
        "teams": teams.iter().map(|t| json!({ "id": t, "name": t })).collect::<Vec<_>>(),
    }))
}

/// ponytail: stands in for lane F's `events::publish` (an outbox row and, for lifecycle events,
/// a `lifecycle_log` row in the caller's transaction) until it merges; logs the event only.
pub mod events {
    use crate::ids::MemberId;
    use crate::store::{StoreError, Tx};

    pub enum Actor {
        Member(MemberId),
        System,
    }

    pub enum Event {
        MemberCreated { member: MemberId, actor: Actor },
        OrganizationCreated { actor: Actor },
    }

    impl Event {
        pub fn event_type(&self) -> &'static str {
            match self {
                Event::MemberCreated { .. } => "member.created.v1",
                Event::OrganizationCreated { .. } => "organization.created.v1",
            }
        }
    }

    pub async fn publish(_tx: &mut Tx, event: Event) -> Result<(), StoreError> {
        let (member, actor) = match &event {
            Event::MemberCreated { member, actor } => (Some(member.as_str()), actor),
            Event::OrganizationCreated { actor } => (None, actor),
        };
        let actor = match actor {
            Actor::Member(m) => m.as_str(),
            Actor::System => "system",
        };
        tracing::info!(event = event.event_type(), member, actor, "event published");
        Ok(())
    }
}

/// The production `AuthProvider`: WorkOS's User Management API.
pub struct WorkosAuth {
    client: workos::Client,
}

impl WorkosAuth {
    /// `client` must carry the client id (`workos::Client::builder().client_id(..)`).
    pub fn new(client: workos::Client) -> Self {
        WorkosAuth { client }
    }
}

/// A WorkOS failure by kind only: the status and WorkOS's fixed error code, never its message
/// or a transport error's text (which carries the request URL).
fn kind(e: &workos::Error) -> String {
    match e {
        workos::Error::Api(a) => {
            let code = a.code.as_deref().filter(|c| {
                c.len() <= 64 && c.bytes().all(|b| b.is_ascii_lowercase() || b == b'_')
            });
            match code {
                Some(c) => format!("status {} {c}", a.status),
                None => format!("status {}", a.status),
            }
        }
        workos::Error::Network(t) => format!("network ({:?})", t.kind),
        workos::Error::Decode(_) => "decode".into(),
        _ => "other".into(),
    }
}

/// The further step a WorkOS error body asks for; only the fields we need are read.
fn step(code: &str, b: &Value) -> Option<Step> {
    let token = Secret(b.get("pending_authentication_token")?.as_str()?.to_owned());
    let text = |v: &Value| v.as_str().map(str::to_owned);
    Some(match code {
        "mfa_challenge" => {
            let factors = b.get("authentication_factors")?.as_array()?;
            let totp = factors.iter().find(|f| f["type"] == "totp")?;
            Step::Mfa {
                token,
                factor: text(&totp["id"])?,
            }
        }
        "mfa_enrollment" => Step::MfaSetup {
            token,
            user: text(&b["user"]["id"])?,
        },
        "organization_selection_required" => Step::ChooseOrg {
            token,
            orgs: b
                .get("organizations")?
                .as_array()?
                .iter()
                .filter_map(|o| {
                    Some(OrgRef {
                        id: text(&o["id"])?,
                        name: text(&o["name"])?,
                    })
                })
                .collect(),
        },
        "email_verification_required" => Step::VerifyEmail {
            token,
            email: text(&b["email"])?,
            user: text(&b["user"]["id"]),
        },
        "sso_required" => Step::Sso {
            token,
            connection: text(b.get("connection_ids")?.as_array()?.first()?)?,
        },
        "organization_authentication_methods_required" => Step::Methods {
            token,
            methods: b
                .get("auth_methods")?
                .as_object()?
                .iter()
                .filter(|(_, on)| on.as_bool() == Some(true))
                .map(|(m, _)| m.clone())
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect(),
        },
        _ => return None,
    })
}

fn classify(e: workos::Error) -> AuthError {
    let workos::Error::Api(a) = &e else {
        return AuthError::Upstream(kind(&e));
    };
    let code = a.code.as_deref().unwrap_or_default();
    let body: Value = serde_json::from_slice(&a.raw_body).unwrap_or(Value::Null);
    if let Some(step) = step(code, &body) {
        return AuthError::Step(step);
    }
    match code {
        "invalid_credentials" | "invalid_grant" => AuthError::InvalidCredentials,
        "invalid_one_time_code"
        | "one_time_code_previously_used"
        | "one_time_code_too_many_attempts"
        | "email_verification_code_incorrect"
        | "email_verification_code_previously_used"
        | "too_many_failed_verification_attempts" => AuthError::CodeInvalid,
        "one_time_code_expired" | "email_verification_code_expired" => AuthError::CodeExpired,
        "invalid_pending_authentication_token" => AuthError::PendingExpired,
        "user_already_exists"
        | "email_not_available"
        | "email_already_in_use"
        | "user_already_organization_member" => AuthError::Conflict,
        "password_strength_error" | "invalid_email" | "invalid_request_parameters" => {
            AuthError::Invalid
        }
        "entity_not_found"
        | "organization_not_found"
        | "invalid_invite"
        | "invite_expired"
        | "invite_revoked"
        | "invite_accepted"
        | "invite_not_pending" => AuthError::NotFound,
        _ if a.status == 404 => AuthError::NotFound,
        _ => AuthError::Upstream(kind(&e)),
    }
}

fn auth_user(u: workos::User) -> AuthUser {
    let joined = [u.first_name.as_deref(), u.last_name.as_deref()]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" ");
    let name = u.name.or((!joined.is_empty()).then_some(joined));
    AuthUser {
        id: u.id,
        email: u.email,
        email_verified: u.email_verified,
        name,
        avatar_url: u.profile_picture_url,
    }
}

fn authenticated(r: workos::AuthenticateResponse) -> Authenticated {
    Authenticated {
        user: auth_user(r.user),
        org: r.organization_id,
    }
}

#[async_trait]
impl AuthProvider for WorkosAuth {
    async fn authenticate_with_password(
        &self,
        email: &str,
        password: &str,
        invitation: Option<&str>,
    ) -> Result<Authenticated, AuthError> {
        let mut p = workos::user_management::AuthenticateWithPasswordParams::new(email, password);
        p.invitation_token = invitation.map(Into::into);
        let r = self
            .client
            .user_management()
            .authenticate_with_password(p)
            .await;
        r.map(authenticated).map_err(classify)
    }

    async fn create_magic_auth(&self, email: &str) -> Result<(), AuthError> {
        let p =
            workos::user_management::CreateMagicAuthParams::new(workos::CreateMagicCodeAndReturn {
                email: email.to_owned(),
                invitation_token: None,
                ip_address: None,
                user_agent: None,
                radar_auth_attempt_id: None,
                signals_id: None,
                locale: None,
            });
        let r = self.client.user_management().create_magic_auth(p).await;
        r.map(|_| ()).map_err(classify)
    }

    async fn authenticate_with_magic_auth(
        &self,
        email: &str,
        code: &str,
        invitation: Option<&str>,
    ) -> Result<Authenticated, AuthError> {
        let mut p = workos::user_management::AuthenticateWithMagicAuthParams::new(code, email);
        p.invitation_token = invitation.map(Into::into);
        let r = self
            .client
            .user_management()
            .authenticate_with_magic_auth(p)
            .await;
        r.map(authenticated).map_err(classify)
    }

    async fn authenticate_with_email_verification(
        &self,
        token: &str,
        code: &str,
    ) -> Result<Authenticated, AuthError> {
        let p = workos::user_management::AuthenticateWithEmailVerificationParams::new(code, token);
        let r = self
            .client
            .user_management()
            .authenticate_with_email_verification(p)
            .await;
        r.map(authenticated).map_err(classify)
    }

    async fn authenticate_with_totp(
        &self,
        token: &str,
        challenge: &str,
        code: &str,
    ) -> Result<Authenticated, AuthError> {
        let p = workos::user_management::AuthenticateWithTotpParams::new(code, token, challenge);
        let r = self
            .client
            .user_management()
            .authenticate_with_totp(p)
            .await;
        r.map(authenticated).map_err(classify)
    }

    async fn authenticate_with_organization_selection(
        &self,
        token: &str,
        org: &str,
    ) -> Result<Authenticated, AuthError> {
        let p =
            workos::user_management::AuthenticateWithOrganizationSelectionParams::new(token, org);
        let r = self
            .client
            .user_management()
            .authenticate_with_organization_selection(p)
            .await;
        r.map(authenticated).map_err(classify)
    }

    async fn authenticate_with_code(&self, code: &str) -> Result<Authenticated, AuthError> {
        let p = workos::user_management::AuthenticateWithCodeParams::new(code);
        let r = self
            .client
            .user_management()
            .authenticate_with_code(p)
            .await;
        r.map(authenticated).map_err(classify)
    }

    async fn sso_connection(&self, domain: &str) -> Result<Option<String>, AuthError> {
        let p = workos::sso::ListConnectionsParams {
            domain: Some(domain.to_owned()),
            ..Default::default()
        };
        let r = self
            .client
            .sso()
            .list_connections(p)
            .await
            .map_err(classify)?;
        Ok(r.data
            .into_iter()
            .find(|c| matches!(c.state, workos::ConnectionState::Active))
            .map(|c| c.id))
    }

    fn sso_authorization_url(
        &self,
        connection: &str,
        state: &str,
        redirect_uri: &str,
    ) -> Result<String, AuthError> {
        let mut p = workos::user_management::GetAuthorizationUrlParams::new(redirect_uri);
        p.connection_id = Some(connection.to_owned());
        p.state = Some(state.to_owned());
        self.client
            .user_management()
            .get_authorization_url(p)
            .map_err(classify)
    }

    async fn create_password_reset(&self, email: &str) -> Result<(), AuthError> {
        let p =
            workos::user_management::ResetPasswordParams::new(workos::CreatePasswordResetToken {
                email: email.to_owned(),
                locale: None,
            });
        let r = self.client.user_management().reset_password(p).await;
        r.map(|_| ()).map_err(classify)
    }

    async fn confirm_password_reset(&self, token: &str, password: &str) -> Result<(), AuthError> {
        let p =
            workos::user_management::ConfirmPasswordResetParams::new(workos::CreatePasswordReset {
                token: token.into(),
                new_password: password.into(),
            });
        let r = self
            .client
            .user_management()
            .confirm_password_reset(p)
            .await;
        r.map(|_| ()).map_err(classify)
    }

    async fn invitation(&self, token: &str) -> Result<Invitation, AuthError> {
        let i = self
            .client
            .user_management()
            .find_invitation_by_token(token)
            .await
            .map_err(classify)?;
        Ok(Invitation {
            email: i.email,
            org: i.organization_id,
            inviter: i.inviter_user_id,
            pending: matches!(i.state, workos::UserInviteState::Pending),
        })
    }

    async fn user(&self, id: &str) -> Result<AuthUser, AuthError> {
        let r = self.client.user_management().get_user(id).await;
        r.map(auth_user).map_err(classify)
    }

    async fn create_user(
        &self,
        email: &str,
        password: &str,
        name: &str,
    ) -> Result<AuthUser, AuthError> {
        let (first, last) = match name.trim().split_once(char::is_whitespace) {
            Some((f, l)) => (f.to_owned(), Some(l.trim().to_owned())),
            None => (name.trim().to_owned(), None),
        };
        let mut body = workos::user_management::CreateUserParamsBody::new(email);
        body.first_name = Some(first);
        body.last_name = last;
        body.password = Some(workos::user_management::Password::Plaintext {
            password: password.to_owned(),
        });
        let p = workos::user_management::CreateUserParams::new(body);
        let u = self
            .client
            .user_management()
            .create_user(p)
            .await
            .map_err(classify)?;
        Ok(AuthUser {
            id: u.id,
            email: u.email,
            email_verified: u.email_verified,
            name: Some(name.trim().to_owned()),
            avatar_url: u.profile_picture_url,
        })
    }

    async fn send_verification_email(&self, user: &str) -> Result<(), AuthError> {
        let p = workos::user_management::SendVerificationEmailParams::new(
            workos::SendVerificationEmail { locale: None },
        );
        let r = self
            .client
            .user_management()
            .send_verification_email(user, p)
            .await;
        r.map(|_| ()).map_err(classify)
    }

    async fn verify_email(&self, user: &str, code: &str) -> Result<AuthUser, AuthError> {
        let p = workos::user_management::VerifyEmailParams::new(workos::VerifyEmailAddress {
            code: code.into(),
        });
        let r = self.client.user_management().verify_email(user, p).await;
        r.map(|r| auth_user(r.user)).map_err(classify)
    }

    async fn organization(&self, id: &str) -> Result<OrgRef, AuthError> {
        let o = self
            .client
            .organizations()
            .get_organization(id)
            .await
            .map_err(classify)?;
        Ok(OrgRef {
            id: o.id,
            name: o.name,
        })
    }

    // `OrganizationInput` has no constructor, so its deprecated `domains` must be named.
    #[allow(deprecated)]
    async fn create_organization(&self, name: &str) -> Result<OrgRef, AuthError> {
        let p = workos::organizations::CreateOrganizationParams::new(workos::OrganizationInput {
            name: name.to_owned(),
            allow_profiles_outside_organization: None,
            domains: None,
            domain_data: None,
            metadata: None,
            external_id: None,
        });
        let o = self
            .client
            .organizations()
            .create_organization(p)
            .await
            .map_err(classify)?;
        Ok(OrgRef {
            id: o.id,
            name: o.name,
        })
    }

    async fn create_membership(&self, user: &str, org: &str, role: &str) -> Result<(), AuthError> {
        let mut body =
            workos::organization_membership::CreateOrganizationMembershipParamsBody::new(user, org);
        body.role = Some(workos::organization_membership::Role::Single {
            role_slug: role.to_owned(),
        });
        let p = workos::organization_membership::CreateOrganizationMembershipParams::new(body);
        let r = self
            .client
            .organization_membership()
            .create_organization_membership(p)
            .await;
        r.map(|_| ()).map_err(classify)
    }

    async fn memberships(&self, user: &str) -> Result<Vec<Membership>, AuthError> {
        let p = workos::organization_membership::ListOrganizationMembershipsParams {
            user_id: Some(user.to_owned()),
            limit: Some(100),
            ..Default::default()
        };
        let r = self
            .client
            .organization_membership()
            .list_organization_memberships(p)
            .await
            .map_err(classify)?;
        Ok(r.data
            .into_iter()
            .map(|m| Membership {
                org_name: m.organization_name.unwrap_or_default(),
                org: m.organization_id,
                role: m.role.slug,
                active: matches!(m.status, workos::UserOrganizationMembershipStatus::Active),
            })
            .collect())
    }

    async fn enroll_auth_factor(&self, user: &str) -> Result<TotpEnrollment, AuthError> {
        let p = workos::multi_factor_auth::CreateUserAuthFactorParams::new(
            workos::EnrollUserAuthenticationFactor {
                type_: "totp".into(),
                totp_issuer: Some("Mandate".into()),
                totp_user: None,
                totp_secret: None,
            },
        );
        let r = self
            .client
            .multi_factor_auth()
            .create_user_auth_factor(user, p)
            .await
            .map_err(classify)?;
        let totp = r
            .authentication_factor
            .totp
            .ok_or_else(|| AuthError::Upstream("enroll: no totp".into()))?;
        Ok(TotpEnrollment {
            factor: r.authentication_factor.id,
            challenge: r.authentication_challenge.id,
            secret: Secret(totp.secret.into_inner()),
            uri: Secret(totp.uri.into_inner()),
        })
    }

    async fn challenge_factor(&self, factor: &str) -> Result<String, AuthError> {
        let p = workos::multi_factor_auth::ChallengeFactorParams::new(
            workos::ChallengeAuthenticationFactor { sms_template: None },
        );
        let r = self
            .client
            .multi_factor_auth()
            .challenge_factor(factor, p)
            .await;
        r.map(|c| c.id).map_err(classify)
    }

    async fn verify_challenge(&self, challenge: &str, code: &str) -> Result<bool, AuthError> {
        let p = workos::multi_factor_auth::VerifyChallengeParams::new(
            workos::AuthenticationChallengesVerifyRequest { code: code.into() },
        );
        let r = self
            .client
            .multi_factor_auth()
            .verify_challenge(challenge, p)
            .await;
        r.map(|r| r.valid).map_err(classify)
    }

    async fn has_totp(&self, user: &str) -> Result<bool, AuthError> {
        let p = workos::multi_factor_auth::ListUserAuthFactorsParams::default();
        let r = self
            .client
            .multi_factor_auth()
            .list_user_auth_factors(user, p)
            .await
            .map_err(classify)?;
        Ok(r.data
            .iter()
            .any(|f| matches!(f.type_, workos::AuthenticationFactorType::Totp)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn api_error(status: u16, body: Value) -> workos::Error {
        let code = body
            .get("code")
            .or_else(|| body.get("error"))
            .and_then(Value::as_str)
            .map(str::to_owned);
        workos::Error::Api(Box::new(workos::ApiError {
            status,
            code,
            message: "ann@x.com: secret detail".into(),
            request_id: None,
            retry_after: None,
            raw_body: serde_json::to_vec(&body).unwrap().into(),
            headers: Default::default(),
        }))
    }

    #[test]
    fn workos_steps_are_parsed_from_error_bodies() {
        let user = json!({ "id": "user_1", "email": "ann@x.com" });
        let mfa = api_error(
            403,
            json!({ "code": "mfa_challenge", "pending_authentication_token": "pat",
                    "authentication_factors": [{ "id": "f_sms", "type": "sms" }, { "id": "f_totp", "type": "totp" }],
                    "user": user }),
        );
        assert!(matches!(classify(mfa),
            AuthError::Step(Step::Mfa { token, factor }) if token.0 == "pat" && factor == "f_totp"));
        let setup = api_error(
            403,
            json!({ "code": "mfa_enrollment", "pending_authentication_token": "pat", "user": user }),
        );
        assert!(
            matches!(classify(setup), AuthError::Step(Step::MfaSetup { user, .. }) if user == "user_1")
        );
        let orgs = api_error(
            403,
            json!({ "code": "organization_selection_required", "pending_authentication_token": "pat",
                    "organizations": [{ "id": "org_a", "name": "A" }, { "id": "org_b", "name": "B" }] }),
        );
        let AuthError::Step(Step::ChooseOrg { orgs, .. }) = classify(orgs) else {
            panic!("not choose_org")
        };
        assert_eq!(
            orgs.iter().map(|o| o.id.as_str()).collect::<Vec<_>>(),
            ["org_a", "org_b"]
        );
        let verify = api_error(
            403,
            json!({ "code": "email_verification_required", "pending_authentication_token": "pat", "email": "ann@x.com" }),
        );
        assert!(matches!(
            classify(verify),
            AuthError::Step(Step::VerifyEmail { user: None, .. })
        ));
        let sso = api_error(
            400,
            json!({ "error": "sso_required", "pending_authentication_token": "pat", "connection_ids": ["conn_1"] }),
        );
        assert!(
            matches!(classify(sso), AuthError::Step(Step::Sso { connection, .. }) if connection == "conn_1")
        );
        let methods = api_error(
            400,
            json!({ "error": "organization_authentication_methods_required", "pending_authentication_token": "pat",
                    "auth_methods": { "password": false, "magic_auth": true, "google_oauth": true } }),
        );
        let AuthError::Step(Step::Methods { methods, .. }) = classify(methods) else {
            panic!("not methods")
        };
        assert_eq!(methods, ["google_oauth", "magic_auth"]);
        // A step without its token, or a TOTP challenge without a TOTP factor, is not a step.
        let tokenless = api_error(403, json!({ "code": "mfa_enrollment", "user": user }));
        assert!(matches!(classify(tokenless), AuthError::Upstream(_)));
        let sms_only = api_error(
            403,
            json!({ "code": "mfa_challenge", "pending_authentication_token": "pat",
                    "authentication_factors": [{ "id": "f_sms", "type": "sms" }] }),
        );
        assert!(matches!(classify(sms_only), AuthError::Upstream(_)));
    }

    #[test]
    fn workos_errors_map_by_code_and_never_carry_text() {
        let cases = [
            ("invalid_credentials", "invalid credentials"),
            ("invalid_one_time_code", "invalid code"),
            ("email_verification_code_expired", "expired code"),
            (
                "invalid_pending_authentication_token",
                "pending step expired",
            ),
            ("user_already_exists", "already exists"),
            ("password_strength_error", "rejected input"),
            ("invite_revoked", "not found"),
        ];
        for (code, want) in cases {
            let e = classify(api_error(
                400,
                json!({ "code": code, "message": "ann@x.com" }),
            ));
            assert_eq!(e.to_string(), want, "{code}");
        }
        let e = classify(api_error(
            500,
            json!({ "code": "Weird Code ann@x.com", "message": "ann@x.com" }),
        ));
        assert_eq!(e.to_string(), "workos request failed: status 500");
        let e = classify(api_error(
            429,
            json!({ "code": "rate_limit_exceeded", "message": "ann@x.com" }),
        ));
        assert_eq!(
            e.to_string(),
            "workos request failed: status 429 rate_limit_exceeded"
        );
        let leak = "error sending request for url (https://api.workos.com/x?email=ann%40x.com)";
        let net = workos::Error::Network(workos::transport::TransportError::new(
            workos::transport::TransportErrorKind::Connect,
            leak,
        ));
        assert_eq!(
            classify(net).to_string(),
            "workos request failed: network (Connect)"
        );
    }

    #[test]
    fn typed_input_is_validated() {
        let email = |s: &str| serde_json::from_value::<Email>(json!(s)).ok().map(|e| e.0);
        assert_eq!(email(" ann@x.com ").as_deref(), Some("ann@x.com"));
        assert_eq!(email("ann@x.com\n").as_deref(), Some("ann@x.com"));
        for bad in [
            "",
            "ann",
            "@x.com",
            "ann@",
            "ann@x",
            "a b@x.com",
            "ann@x@y.com",
        ] {
            assert!(email(bad).is_none(), "{bad:?}");
        }
        assert!(email("ann@x.com\u{0}").is_none());
        let code = |s: &str| serde_json::from_value::<Code>(json!(s)).is_ok();
        assert!(code("123456") && !code("12 34") && !code("") && !code(&"1".repeat(65)));
        let password = |s: &str| serde_json::from_value::<Password>(json!(s)).is_ok();
        assert!(password("p@ss wörd") && !password("") && !password("a\u{0}b"));
        assert_eq!(format!("{:?}", Password("hunter22".into())), "Password(..)");
        assert_eq!(format!("{:?}", Secret("pat".into())), "Secret(..)");
    }
}
