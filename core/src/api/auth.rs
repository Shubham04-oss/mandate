//! Sign-in through WorkOS's headless User Management API. WorkOS proves the person; the core
//! decides which tenant and member that is and issues its own session. The browser never sees
//! a WorkOS token, and WorkOS's error text never leaves this module.
use super::session::{self, NewPending, Session};
use super::{ApiError, ApiState, Input, ratelimit};
use crate::events;
use crate::identity::slug;
use crate::ids::{MemberId, TenantId};
use crate::store::{self, StoreError, Tx};
use async_trait::async_trait;
use axum::Router;
use axum::extract::{Path, RawQuery, Request, State};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use http::{HeaderMap, StatusCode, header};
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
        /// WorkOS's own sso_required carries none; then SSO starts afresh.
        token: Option<Secret>,
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
    /// The user whose password was reset.
    async fn confirm_password_reset(
        &self,
        token: &str,
        password: &str,
    ) -> Result<AuthUser, AuthError>;
    /// Replaces the user's password (used only to neutralise an unconfirmed sign-up password).
    async fn set_password(&self, user: &str, password: &str) -> Result<(), AuthError>;
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
                // 503, not 502: Cloudflare replaces an origin 502 with its own page, which drops the CORS headers.
                ApiError::new(
                    StatusCode::SERVICE_UNAVAILABLE,
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

    fn domain(&self) -> &str {
        self.0.rsplit_once('@').map_or("", |(_, d)| d)
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

/// An opaque id or token from a link or a previous step: URL-safe characters only.
#[derive(Debug, Clone)]
pub struct Opaque(String);

impl<'de> Deserialize<'de> for Opaque {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        bounded(d, 512, |c| {
            c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.')
        })
        .map(Opaque)
    }
}

/// A person's or organization's name: trimmed, 1 to 128 characters, no control characters.
#[derive(Debug, Clone)]
pub struct Name(String);

impl<'de> Deserialize<'de> for Name {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        let s = s.trim();
        if s.is_empty() || s.chars().count() > 128 || s.chars().any(char::is_control) {
            return Err(serde::de::Error::custom("invalid name"));
        }
        Ok(Name(s.to_owned()))
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
        .route("/v1/auth/sign-up", post(sign_up))
        .route("/v1/auth/verify-email", post(verify_email))
        .route("/v1/auth/verify-email/resend", post(resend))
        .route("/v1/auth/mfa/verify", post(mfa_verify))
        .route("/v1/auth/mfa/setup", post(mfa_setup))
        .route("/v1/auth/mfa/enable", post(mfa_enable))
        .route("/v1/auth/choose-org", post(choose_org))
        .route("/v1/auth/sso/start", post(sso_start))
        .route("/v1/auth/sso/callback", get(sso_callback))
        .route("/v1/auth/password/forgot", post(forgot))
        .route("/v1/auth/password/reset", post(reset))
        .route("/v1/auth/invite/{token}", get(invite))
        .route("/v1/auth/invite/{token}/accept", post(invite_accept))
        .route("/v1/auth/sign-out", post(sign_out))
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            replace_session,
        ))
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            ratelimit::limit,
        ))
        .route("/v1/me", get(me_route))
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
    outcome(&s, r, false).await
}

async fn password(
    State(s): State<Arc<ApiState>>,
    Input(b): Input<EmailPassword>,
) -> Result<Response, ApiError> {
    let r = s
        .auth
        .authenticate_with_password(b.email.as_str(), &b.password.0, None)
        .await;
    let r = guard_password(&s, b.email.as_str(), r).await?;
    outcome(&s, r, true).await
}

/// Whether WorkOS took the credentials: signed in, or a further step other than proving the email.
fn accepted(r: &Result<Authenticated, AuthError>) -> bool {
    match r {
        Ok(_) => true,
        Err(AuthError::Step(step)) => !matches!(step, Step::VerifyEmail { .. }),
        Err(_) => false,
    }
}

/// Our sign-up's row for this WorkOS user or email: its user id (unknown if WorkOS never
/// answered) and whether its password was already neutralised.
async fn sign_up_row(
    s: &ApiState,
    user: Option<&str>,
    email: &str,
) -> Result<Option<(Option<String>, bool)>, ApiError> {
    let mut tx = s.store.begin_untenanted().await?;
    let row = sqlx::query_as(
        "select workos_user_id, neutralised_at is not null from sign_up_passwords \
         where workos_user_id = $1 or email = lower($2) order by workos_user_id = $1 desc nulls last limit 1",
    )
    .bind(user)
    .bind(email.trim())
    .fetch_optional(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(row)
}

/// Deletes the row: its password was confirmed (only while not neutralised) or, with `all`,
/// replaced by its owner through a reset.
async fn forget_sign_up_password(
    s: &ApiState,
    user: Option<&str>,
    email: &str,
    all: bool,
) -> Result<(), ApiError> {
    let mut tx = s.store.begin_untenanted().await?;
    sqlx::query(
        "delete from sign_up_passwords where (workos_user_id = $1 or email = lower($2)) \
         and ($3 or neutralised_at is null)",
    )
    .bind(user)
    .bind(email.trim())
    .bind(all)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

/// Sets 256 random bits as the user's password through WorkOS (nobody ever learns it), then
/// marks the row. The row stays: a password WorkOS took before the new one landed is still refused.
async fn neutralise(s: &ApiState, user: &str) -> Result<(), ApiError> {
    s.auth.set_password(user, &session::secret()?).await?;
    let mut tx = s.store.begin_untenanted().await?;
    sqlx::query(
        "update sign_up_passwords set neutralised_at = now() \
         where workos_user_id = $1 and neutralised_at is null",
    )
    .bind(user)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

/// A password WorkOS took while our sign-up row exists for that user (confirmed by nobody, or
/// already neutralised) is refused; an unneutralised password is neutralised first.
async fn guard_password(
    s: &ApiState,
    typed_email: &str,
    r: Result<Authenticated, AuthError>,
) -> Result<Result<Authenticated, AuthError>, ApiError> {
    if !accepted(&r) {
        return Ok(r);
    }
    // By the user WorkOS names; only a step that names nobody falls back to the typed email.
    let (user, email) = match &r {
        Ok(a) => (Some(a.user.id.as_str()), a.user.email.as_str()),
        Err(_) => (None, typed_email),
    };
    let Some((row_user, neutralised)) = sign_up_row(s, user, email).await? else {
        return Ok(r);
    };
    if !neutralised
        && let Some(row_user) = &row_user
        && user.is_none_or(|u| u == row_user)
    {
        neutralise(s, row_user).await?;
    }
    Ok(Err(AuthError::InvalidCredentials))
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
/// `via_password`: the sign-in began with a password (directly or through the step it left).
async fn outcome(
    s: &ApiState,
    r: Result<Authenticated, AuthError>,
    via_password: bool,
) -> Result<Response, ApiError> {
    match r {
        Ok(a) => finish(s, a, via_password).await?.into_json(),
        Err(AuthError::Step(step)) => next(s, step, via_password).await,
        Err(e) => Err(e.into()),
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SignUp {
    email: Email,
    password: Password,
    name: Name,
    org_name: Name,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PendingCode {
    pending: Opaque,
    code: Code,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PendingOnly {
    pending: Opaque,
}

/// The unexpired tenantless step with this id and one of `kinds`.
async fn find_pending(
    s: &ApiState,
    id: &str,
    kinds: &[&str],
) -> Result<session::Pending, ApiError> {
    let mut tx = s.store.begin_untenanted().await?;
    let p = session::pending(&mut tx, id, kinds, OffsetDateTime::now_utc()).await?;
    tx.commit().await?;
    p.ok_or_else(pending_expired)
}

/// `find_pending`, spent in the same statement: a double submit gets `pending_expired`.
async fn take_pending(
    s: &ApiState,
    id: &str,
    kinds: &[&str],
) -> Result<session::Pending, ApiError> {
    let mut tx = s.store.begin_untenanted().await?;
    let p = session::take_pending(&mut tx, id, kinds, OffsetDateTime::now_utc()).await?;
    tx.commit().await?;
    p.ok_or_else(pending_expired)
}

async fn drop_pending(s: &ApiState, id: &str) -> Result<(), ApiError> {
    let mut tx = s.store.begin_untenanted().await?;
    session::delete_pending(&mut tx, id).await?;
    tx.commit().await?;
    Ok(())
}

/// The outcome of finishing a step: a wrong code may be retried on the same step (the rate
/// limit caps guesses); anything else uses the step up.
/// Whether a stored step was left by a password sign-in.
fn via_password(data: &Value) -> bool {
    data["via"] == "password"
}

async fn settle(
    s: &ApiState,
    pending: &str,
    r: Result<Authenticated, AuthError>,
    via_password: bool,
) -> Result<Response, ApiError> {
    if !matches!(
        r,
        Err(AuthError::CodeInvalid | AuthError::CodeExpired | AuthError::Invalid)
    ) {
        drop_pending(s, pending).await?;
    }
    outcome(s, r, via_password).await
}

/// WorkOS's own password rules apply too; this floor is checked first, for every email alike.
const MIN_PASSWORD: usize = 10;

/// Answers the same whether or not the email already has an account (no account discovery).
async fn sign_up(
    State(s): State<Arc<ApiState>>,
    Input(b): Input<SignUp>,
) -> Result<Response, ApiError> {
    if b.password.0.chars().count() < MIN_PASSWORD {
        return Err(ApiError::invalid_input());
    }
    // Recorded before WorkOS has the user, so any failure later leaves the guard in place.
    let recorded = record_sign_up_email(&s, b.email.as_str()).await?;
    let data = match s
        .auth
        .create_user(b.email.as_str(), &b.password.0, &b.name.0)
        .await
    {
        Ok(user) => {
            // If this fails the row still guards by email.
            claim_sign_up_row(&s, b.email.as_str(), &user.id).await?;
            quietly(
                "verification email",
                s.auth.send_verification_email(&user.id).await,
            );
            json!({ "user": user.id, "org_name": b.org_name.0 })
        }
        // An existing account (or a password WorkOS refuses, whose answer could differ for
        // one): a step that never verifies, answered exactly like a new one.
        Err(AuthError::Conflict | AuthError::Invalid) => {
            if recorded {
                unrecord_sign_up_email(&s, b.email.as_str()).await?;
            }
            json!({ "decoy": true })
        }
        // Ambiguous (a timeout, a 5xx): WorkOS may have made the user, so the row stays.
        Err(e) => return Err(e.into()),
    };
    let pending = store_pending(&s, "sign_up", None, Some(b.email.as_str()), data).await?;
    let body = json!({ "status": "next", "next": "verify_email", "pending": pending });
    Ok((StatusCode::ACCEPTED, axum::Json(body)).into_response())
}

/// A row for this email with no user yet; `false` if one exists already (kept as it is).
async fn record_sign_up_email(s: &ApiState, email: &str) -> Result<bool, ApiError> {
    let mut tx = s.store.begin_untenanted().await?;
    let made: Option<bool> = sqlx::query_scalar(
        "insert into sign_up_passwords (email) values (lower($1)) on conflict do nothing returning true",
    )
    .bind(email.trim())
    .fetch_optional(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(made.is_some())
}

/// The new WorkOS user owns the email's row (a stale row from a deleted user starts over). An
/// upsert: a concurrent sign-up that got Conflict may have dropped the row this one relied on.
async fn claim_sign_up_row(s: &ApiState, email: &str, user: &str) -> Result<(), ApiError> {
    let mut tx = s.store.begin_untenanted().await?;
    sqlx::query(
        "insert into sign_up_passwords (email, workos_user_id) values (lower($1), $2) \
         on conflict (email) do update set workos_user_id = excluded.workos_user_id, neutralised_at = null",
    )
    .bind(email.trim())
    .bind(user)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

/// Drops the row this sign-up just made, when WorkOS made no user for it.
async fn unrecord_sign_up_email(s: &ApiState, email: &str) -> Result<(), ApiError> {
    let mut tx = s.store.begin_untenanted().await?;
    sqlx::query("delete from sign_up_passwords where email = lower($1) and workos_user_id is null")
        .bind(email.trim())
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

async fn verify_email(
    State(s): State<Arc<ApiState>>,
    Input(b): Input<PendingCode>,
) -> Result<Response, ApiError> {
    let p = find_pending(&s, &b.pending.0, &["verify_email", "sign_up"]).await?;
    // Per email too: a new step (a new sign-up, another password sign-in) must not mean new guesses.
    let email = p.email.as_deref().ok_or_else(pending_expired)?;
    ratelimit::charge(&s, &ratelimit::email_key(email)).await?;
    if p.kind == "verify_email" {
        let token = p.workos_token.ok_or_else(pending_expired)?;
        let r = s
            .auth
            .authenticate_with_email_verification(&token, &b.code.0)
            .await;
        let via = via_password(&p.data);
        // A password sign-in's step: the password and the emailed code together are as good as
        // sign-up's own verify step. Any other step confirms nothing.
        if via && accepted(&r) {
            forget_sign_up_password(&s, None, email, false).await?;
        }
        return settle(&s, &b.pending.0, r, via).await;
    }
    let (Some(user), Some(org_name)) = (p.data["user"].as_str(), p.data["org_name"].as_str())
    else {
        return Err(AuthError::CodeInvalid.into());
    };
    // A wrong code keeps the step; a right one spends it before an org is made.
    let user = s.auth.verify_email(user, &b.code.0).await?;
    take_pending(&s, &b.pending.0, &["sign_up"]).await?;
    forget_sign_up_password(&s, Some(&user.id), &user.email, false).await?;
    create_org(&s, user, org_name).await?.into_json()
}

/// Self-serve sign-up: a new WorkOS organization and tenant with this person as its admin.
async fn create_org(s: &ApiState, user: AuthUser, org_name: &str) -> Result<SignedIn, ApiError> {
    // ponytail: a failure after the org is created leaves it without a tenant; signing up again
    // makes another. Fine for v1; a retry key would make it idempotent.
    let org = s.auth.create_organization(org_name).await?;
    s.auth.create_membership(&user.id, &org.id, "admin").await?;
    let memberships = s.auth.memberships(&user.id).await?;
    let tenant = tenant_for(s, &org.id).await?;
    let profile = profile(&user, &org.name, &memberships, false);
    let mut tx = s.store.begin(&tenant).await?;
    let member = insert_member(&mut tx, &user).await?.ok_or_else(no_access)?;
    let actor = || events::Actor::Member(member.clone());
    events::publish(
        &mut tx,
        events::Event::OrganizationCreated { actor: actor() },
    )
    .await?;
    events::publish(
        &mut tx,
        events::Event::MemberCreated {
            member: member.clone(),
            actor: actor(),
        },
    )
    .await?;
    sign_in(tx, &tenant, &member, true, &profile).await
}

/// Always 202 for a step that exists; WorkOS sends the code again.
async fn resend(
    State(s): State<Arc<ApiState>>,
    Input(b): Input<PendingOnly>,
) -> Result<StatusCode, ApiError> {
    let p = find_pending(&s, &b.pending.0, &["verify_email", "sign_up"]).await?;
    if let Some(user) = p.data["user"].as_str() {
        quietly(
            "verification email",
            s.auth.send_verification_email(user).await,
        );
    }
    Ok(StatusCode::ACCEPTED)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MfaSetup {
    #[serde(default)]
    pending: Option<Opaque>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MfaEnable {
    #[serde(default)]
    pending: Option<Opaque>,
    challenge: Opaque,
    code: Code,
}

async fn mfa_verify(
    State(s): State<Arc<ApiState>>,
    Input(b): Input<PendingCode>,
) -> Result<Response, ApiError> {
    let p = find_pending(&s, &b.pending.0, &["mfa"]).await?;
    let via = via_password(&p.data);
    let (Some(token), Some(challenge), Some(factor)) = (
        p.workos_token,
        p.data["challenge"].as_str(),
        p.data["factor"].as_str(),
    ) else {
        return Err(pending_expired());
    };
    // The factor is the user's own: a fresh step per guess draws on the same bucket.
    ratelimit::charge(&s, &format!("factor:{factor}")).await?;
    let r = s
        .auth
        .authenticate_with_totp(&token, challenge, &b.code.0)
        .await;
    settle(&s, &b.pending.0, r, via).await
}

fn qr_svg(uri: &str) -> Result<String, ApiError> {
    let code = qrcode::QrCode::new(uri.as_bytes()).map_err(|_| ApiError::internal())?;
    Ok(code
        .render::<qrcode::render::svg::Color>()
        .min_dimensions(200, 200)
        .build())
}

/// Enrolls a TOTP factor, either to finish a sign-in that requires two-step (`pending`) or
/// for the signed-in member. `challenge` is our own step id, never WorkOS's.
async fn mfa_setup(
    State(s): State<Arc<ApiState>>,
    headers: HeaderMap,
    Input(b): Input<MfaSetup>,
) -> Result<Response, ApiError> {
    let now = OffsetDateTime::now_utc();
    let (mut tx, user, token, member, tenant, via) = match &b.pending {
        Some(pending) => {
            let p = find_pending(&s, &pending.0, &["mfa_setup"]).await?;
            let user = p.data["user"]
                .as_str()
                .ok_or_else(pending_expired)?
                .to_owned();
            let tx = s.store.begin_untenanted().await?;
            let via = via_password(&p.data);
            (tx, user, p.workos_token, None, None, via)
        }
        None => {
            let me = session::current(&s, &headers).await?;
            let mut tx = s.store.begin(&me.tenant).await?;
            let user: Option<String> = sqlx::query_scalar(
                "select workos_user_id from members where tenant_id = current_setting('mandate.tenant', true) and id = $1",
            )
            .bind(me.member.as_str())
            .fetch_one(&mut *tx)
            .await?;
            let user = user.ok_or_else(ApiError::signed_out)?;
            (tx, user, None, Some(me.member), Some(me.tenant), false)
        }
    };
    let e = s.auth.enroll_auth_factor(&user).await?;
    let challenge = session::create_pending(
        &mut tx,
        NewPending {
            kind: "mfa_enable",
            workos_token: token.as_deref(),
            email: None,
            data: json!({ "challenge": e.challenge, "member": member, "user": user, "via": via.then_some("password") }),
            tenant: tenant.as_ref(),
        },
        now,
    )
    .await?;
    if let Some(pending) = &b.pending {
        session::delete_pending(&mut tx, &pending.0).await?;
    }
    tx.commit().await?;
    let body = json!({ "qr_svg": qr_svg(&e.uri.0)?, "secret": e.secret.0, "challenge": challenge });
    Ok(axum::Json(body).into_response())
}

/// With `pending`, finishes the sign-in; without, turns two-step on for the signed-in member.
async fn mfa_enable(
    State(s): State<Arc<ApiState>>,
    headers: HeaderMap,
    Input(b): Input<MfaEnable>,
) -> Result<Response, ApiError> {
    if b.pending.is_some() {
        let p = find_pending(&s, &b.challenge.0, &["mfa_enable"]).await?;
        let (Some(token), Some(challenge), Some(user)) = (
            p.workos_token,
            p.data["challenge"].as_str(),
            p.data["user"].as_str(),
        ) else {
            return Err(pending_expired());
        };
        // Per WorkOS user: setting up again gives a new challenge, not new guesses.
        ratelimit::charge(&s, &format!("user:{user}")).await?;
        let r = s
            .auth
            .authenticate_with_totp(&token, challenge, &b.code.0)
            .await;
        let via = via_password(&p.data);
        return settle(&s, &b.challenge.0, r, via).await;
    }
    let me = session::current(&s, &headers).await?;
    let now = OffsetDateTime::now_utc();
    let mut tx = s.store.begin(&me.tenant).await?;
    let p = session::pending(&mut tx, &b.challenge.0, &["mfa_enable"], now).await?;
    tx.commit().await?;
    let p = p.ok_or_else(pending_expired)?;
    let (challenge, user) = match (
        p.data["member"].as_str(),
        p.data["challenge"].as_str(),
        p.data["user"].as_str(),
    ) {
        (Some(m), Some(c), Some(u)) if m == me.member.as_str() && p.workos_token.is_none() => {
            (c, u)
        }
        _ => return Err(pending_expired()),
    };
    ratelimit::charge(&s, &format!("user:{user}")).await?;
    if !s.auth.verify_challenge(challenge, &b.code.0).await? {
        return Err(AuthError::CodeInvalid.into());
    }
    let mut tx = s.store.begin(&me.tenant).await?;
    session::delete_pending(&mut tx, &b.challenge.0).await?;
    sqlx::query(
        "update sessions set profile = jsonb_set(profile, '{mfa_enabled}', 'true') \
         where tenant_id = current_setting('mandate.tenant', true) and member = $1",
    )
    .bind(me.member.as_str())
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ChooseOrg {
    pending: Opaque,
    org: TenantId,
}

/// Only an org WorkOS offered for this step; WorkOS checks the membership again.
async fn choose_org(
    State(s): State<Arc<ApiState>>,
    Input(b): Input<ChooseOrg>,
) -> Result<Response, ApiError> {
    let p = find_pending(&s, &b.pending.0, &["choose_org"]).await?;
    let org = p.data["orgs"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|o| o["id"].as_str())
        .find(|id| slug(id) == b.org.as_str())
        .ok_or_else(no_access)?
        .to_owned();
    let token = p.workos_token.ok_or_else(pending_expired)?;
    let r = s
        .auth
        .authenticate_with_organization_selection(&token, &org)
        .await;
    settle(&s, &b.pending.0, r, via_password(&p.data)).await
}

async fn sso_start(
    State(s): State<Arc<ApiState>>,
    Input(b): Input<EmailOnly>,
) -> Result<Response, ApiError> {
    let domain = b.email.domain().to_lowercase();
    let connection = s
        .auth
        .sso_connection(&domain)
        .await?
        .ok_or(AuthError::NotFound)?;
    let (_, url, cookie) = sso_redirect(&s, &connection, None).await?;
    let mut r = axum::Json(json!({ "redirect_url": url })).into_response();
    r.headers_mut().insert(header::SET_COOKIE, cookie);
    Ok(r)
}

fn redirect(to: &str, cookies: &[http::HeaderValue]) -> Response {
    let mut r = StatusCode::FOUND.into_response();
    if let Ok(v) = http::HeaderValue::from_str(to) {
        r.headers_mut().insert(header::LOCATION, v);
    }
    for c in cookies {
        r.headers_mut().append(header::SET_COOKIE, c.clone());
    }
    r
}

/// WorkOS sends the browser back here: the state must match this browser's cookie and an
/// unused SSO step. Lands on the web app either way, signed in or with an error code.
async fn sso_callback(
    State(s): State<Arc<ApiState>>,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let clear = session::clear_sso_cookie();
    match sso_finish(&s, &headers, query.as_deref().unwrap_or_default()).await {
        Ok(signed_in) => match session::set_cookie(&signed_in.token) {
            Ok(c) => redirect(&format!("{}/", s.web_origin), &[c, clear]),
            Err(e) => redirect(
                &format!("{}/sign-in?error={}", s.web_origin, e.code()),
                &[clear],
            ),
        },
        Err(e) => redirect(
            &format!("{}/sign-in?error={}", s.web_origin, e.code()),
            &[clear],
        ),
    }
}

async fn sso_finish(s: &ApiState, headers: &HeaderMap, query: &str) -> Result<SignedIn, ApiError> {
    let param = |name: &str| {
        url::form_urlencoded::parse(query.as_bytes())
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.into_owned())
            .filter(|v| !v.is_empty() && v.len() <= 512)
    };
    let (Some(code), Some(state)) = (param("code"), param("state")) else {
        return Err(ApiError::forbidden());
    };
    if session::named_cookie(headers, session::SSO_COOKIE) != Some(state.as_str()) {
        return Err(ApiError::forbidden());
    }
    // Single use, whatever happens next.
    take_pending(s, &state, &["sso"])
        .await
        .map_err(|_| ApiError::forbidden())?;
    match s.auth.authenticate_with_code(&code).await {
        Ok(a) => finish(s, a, false).await,
        Err(AuthError::Step(step)) => Err(step_error(&step)),
        Err(e) => Err(e.into()),
    }
}

/// A step the SSO landing cannot run: tells the sign-in page which one.
fn step_error(step: &Step) -> ApiError {
    match step {
        Step::Mfa { .. } | Step::MfaSetup { .. } => ApiError::new(
            StatusCode::UNAUTHORIZED,
            "mfa_required",
            "Two-step verification is required.",
        ),
        Step::ChooseOrg { .. } => ApiError::new(
            StatusCode::BAD_REQUEST,
            "org_required",
            "Choose an organization.",
        ),
        Step::VerifyEmail { .. } => ApiError::new(
            StatusCode::FORBIDDEN,
            "email_unverified",
            "Please verify your email.",
        ),
        Step::Sso { .. } | Step::Methods { .. } => no_access(),
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Reset {
    token: Opaque,
    password: Password,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Accept {
    #[serde(default)]
    password: Option<Password>,
    #[serde(default)]
    name: Option<Name>,
    #[serde(default)]
    code: Option<Code>,
}

/// Always 202: whether the email has an account is not the caller's to learn.
async fn forgot(State(s): State<Arc<ApiState>>, Input(b): Input<EmailOnly>) -> StatusCode {
    quietly(
        "password reset",
        s.auth.create_password_reset(b.email.as_str()).await,
    );
    StatusCode::ACCEPTED
}

async fn reset(
    State(s): State<Arc<ApiState>>,
    Input(b): Input<Reset>,
) -> Result<StatusCode, ApiError> {
    let user = match s
        .auth
        .confirm_password_reset(&b.token.0, &b.password.0)
        .await
    {
        Ok(user) => user,
        Err(AuthError::NotFound) => return Err(AuthError::CodeInvalid.into()),
        Err(e) => return Err(e.into()),
    };
    // First, while nothing else can fail first: every session in every tenant ends.
    // ponytail: the password is changed and the token spent before this runs, so if it fails the
    // reset cannot be retried and sessions live on; a failure here needs an operator.
    let mut tx = s.store.begin_untenanted().await?;
    sqlx::query("select end_sessions_for_workos_user($1)")
        .bind(&user.id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    // The owner chose this password through the emailed link: nothing unconfirmed is left.
    forget_sign_up_password(&s, Some(&user.id), &user.email, true).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// A pending invitation, or 404 for anything else (unknown, used, revoked, malformed).
async fn pending_invitation(s: &ApiState, token: &str) -> Result<Invitation, ApiError> {
    let ok = !token.is_empty()
        && token.len() <= 512
        && token
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'));
    if !ok {
        return Err(AuthError::NotFound.into());
    }
    match s.auth.invitation(token).await? {
        i if i.pending && i.org.is_some() => Ok(i),
        _ => Err(AuthError::NotFound.into()),
    }
}

async fn invite(
    State(s): State<Arc<ApiState>>,
    Path(token): Path<String>,
) -> Result<axum::Json<Value>, ApiError> {
    let inv = pending_invitation(&s, &token).await?;
    let org_id = inv.org.as_deref().unwrap_or_default();
    let org = s.auth.organization(org_id).await?;
    let inviter = match &inv.inviter {
        Some(id) => Some(s.auth.user(id).await?),
        None => None,
    };
    let mut summary = (0, 0);
    let mut inviter_id = inviter.as_ref().map(|u| slug(&u.id));
    if let Some(tenant) = s.store.tenant_for_org(org_id).await? {
        let mut tx = s.store.begin(&tenant).await?;
        summary = sqlx::query_as(
            "select (select count(*) from members where tenant_id = current_setting('mandate.tenant', true)), \
                    (select count(*) from agents where tenant_id = current_setting('mandate.tenant', true) and not disabled)",
        )
        .fetch_one(&mut *tx)
        .await?;
        if let Some(u) = &inviter
            && let Some(m) = store::member_for_workos_user(&mut tx, &u.id).await?
        {
            inviter_id = Some(m.to_string());
        }
        tx.commit().await?;
    }
    let inviter = inviter.map(|u| {
        json!({
            "id": inviter_id,
            "name": u.name.unwrap_or_else(|| u.email.clone()),
            "email": u.email,
            "avatar_url": u.avatar_url,
        })
    });
    Ok(axum::Json(json!({
        "org": org_json(&slug(&org.id), &org.name, summary),
        "inviter": inviter,
        "email": inv.email,
    })))
}

/// A new account (`password`, `name`) or an existing one (`code` from `code/start`) accepts the
/// invitation through WorkOS; then the normal outcome.
async fn invite_accept(
    State(s): State<Arc<ApiState>>,
    Path(token): Path<String>,
    Input(b): Input<Accept>,
) -> Result<Response, ApiError> {
    let inv = pending_invitation(&s, &token).await?;
    // The layer counted the token; another invitation for the same email must not mean new guesses.
    ratelimit::charge(&s, &ratelimit::email_key(&inv.email)).await?;
    let via = b.password.is_some();
    let r = match (b.password, b.name, b.code) {
        (Some(password), Some(name), None) => {
            match s.auth.create_user(&inv.email, &password.0, &name.0).await {
                Ok(_) | Err(AuthError::Conflict) => {}
                Err(e) => return Err(e.into()),
            }
            let r = s
                .auth
                .authenticate_with_password(&inv.email, &password.0, Some(&token))
                .await;
            guard_password(&s, &inv.email, r).await?
        }
        (None, None, Some(code)) => {
            s.auth
                .authenticate_with_magic_auth(&inv.email, &code.0, Some(&token))
                .await
        }
        _ => return Err(ApiError::invalid_input()),
    };
    outcome(&s, r, via).await
}

async fn me_route(
    State(s): State<Arc<ApiState>>,
    who: Session,
    headers: HeaderMap,
) -> Result<axum::Json<Value>, ApiError> {
    let token = session::cookie(&headers).ok_or_else(ApiError::signed_out)?;
    let mut tx = s.store.begin(&who.tenant).await?;
    let profile = session::profile(&mut tx, token)
        .await?
        .ok_or_else(ApiError::signed_out)?;
    let me = me(&mut tx, &who.tenant, &who.member, who.is_admin, &profile).await?;
    tx.commit().await?;
    Ok(axum::Json(me))
}

/// Deletes the session a cookie names, in whichever tenant it is.
async fn end_session(s: &ApiState, token: &str) -> Result<(), ApiError> {
    if let Some(tenant) = session::token_tenant(token) {
        let mut tx = s.store.begin(&tenant).await?;
        session::delete(&mut tx, token).await?;
        tx.commit().await?;
    }
    Ok(())
}

/// A response that signs someone in ends the session the request carried, so the previous
/// person's (or this person's other org's) session does not live on unseen in this browser.
async fn replace_session(State(s): State<Arc<ApiState>>, req: Request, next: Next) -> Response {
    let old = session::cookie(req.headers()).map(str::to_owned);
    let r = next.run(req).await;
    let signed_in = r
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .any(|v| {
            v.starts_with(&format!("{}=", session::COOKIE))
                && !v.starts_with(&format!("{}=;", session::COOKIE))
        });
    match old {
        Some(old) if signed_in => match end_session(&s, &old).await {
            Ok(()) => r,
            // Fail closed: the new cookie is not handed out while the old session may live.
            Err(e) => e.into_response(),
        },
        _ => r,
    }
}

/// Ends this browser's session at once; 204 even without one, so a stale tab can sign out.
async fn sign_out(
    State(s): State<Arc<ApiState>>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    if let Some(token) = session::cookie(&headers) {
        end_session(&s, token).await?;
    }
    let mut r = StatusCode::NO_CONTENT.into_response();
    r.headers_mut()
        .insert(header::SET_COOKIE, session::clear_cookie());
    Ok(r)
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
async fn next(s: &ApiState, step: Step, via_password: bool) -> Result<Response, ApiError> {
    let mut body = json!({ "status": "next" });
    let (next, kind, token, email, data) = match step {
        Step::Mfa { token, factor } => {
            let challenge = s.auth.challenge_factor(&factor).await?;
            let data = json!({ "challenge": challenge, "factor": factor });
            ("mfa", "mfa", token, None, data)
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
            let (pending, url, cookie) = sso_redirect(s, &connection, token.as_ref()).await?;
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
    let mut data = data;
    // Kept with the step, so whatever finishes it knows a password started this sign-in.
    if via_password {
        data["via"] = json!("password");
    }
    let pending = store_pending(s, kind, Some(&token), email.as_deref(), data).await?;
    body["next"] = json!(next);
    body["pending"] = json!(pending);
    Ok(axum::Json(body).into_response())
}

/// WorkOS proved the person: map the org to its tenant, the user to a member, and sign in.
async fn finish(s: &ApiState, a: Authenticated, via_password: bool) -> Result<SignedIn, ApiError> {
    // First, whatever happens next: this sign-in may have just verified the email in WorkOS.
    if let Some((row_user, neutralised)) = sign_up_row(s, Some(&a.user.id), &a.user.email).await? {
        if !neutralised && row_user.as_deref() == Some(a.user.id.as_str()) {
            neutralise(s, &a.user.id).await?;
        }
        // A password never signs in while our sign-up's password is unconfirmed.
        if via_password {
            return Err(AuthError::InvalidCredentials.into());
        }
    }
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

/// Admin follows the WorkOS role at every sign-in; a session, and the `Me` to show.
async fn sign_in(
    mut tx: Tx,
    tenant: &TenantId,
    member: &MemberId,
    is_admin: bool,
    profile: &Value,
) -> Result<SignedIn, ApiError> {
    store::ensure_personal_agent(&mut tx, member).await?;
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
            let safe =
                |c: &&str| c.len() <= 64 && c.bytes().all(|b| b.is_ascii_lowercase() || b == b'_');
            let mut out = match a.code.as_deref().filter(safe) {
                Some(c) => format!("status {} {c}", a.status),
                None => format!("status {}", a.status),
            };
            // WorkOS names the reason in `errors[].code`; only codes are kept, never messages (they can hold emails).
            let body: Value = serde_json::from_slice(&a.raw_body).unwrap_or(Value::Null);
            let reasons: Vec<&str> = body
                .get("errors")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|e| e.get("code")?.as_str())
                .filter(safe)
                .take(5)
                .collect();
            if !reasons.is_empty() {
                out.push_str(&format!(" ({})", reasons.join(", ")));
            }
            out
        }
        workos::Error::Network(t) => format!("network ({:?})", t.kind),
        workos::Error::Decode(_) => "decode".into(),
        _ => "other".into(),
    }
}

/// The further step a WorkOS error body asks for; only the fields we need are read.
fn step(code: &str, b: &Value) -> Option<Step> {
    let text = |v: &Value| v.as_str().map(str::to_owned);
    let pending = b
        .get("pending_authentication_token")
        .and_then(Value::as_str);
    if code == "sso_required" {
        return Some(Step::Sso {
            token: pending.map(|t| Secret(t.to_owned())),
            connection: text(b.get("connection_ids")?.as_array()?.first()?)?,
        });
    }
    let token = Secret(pending?.to_owned());
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
    // A failed create names its real reason in `errors[].code`; classify by that reason.
    let reason = (code == "user_creation_error")
        .then(|| body.pointer("/errors/0/code").and_then(Value::as_str))
        .flatten();
    match reason.unwrap_or(code) {
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
        "password_strength_error"
        | "password_not_allowed"
        | "invalid_email"
        | "invalid_request_parameters" => AuthError::Invalid,
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

    async fn confirm_password_reset(
        &self,
        token: &str,
        password: &str,
    ) -> Result<AuthUser, AuthError> {
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
        r.map(|r| auth_user(r.user)).map_err(classify)
    }

    async fn set_password(&self, user: &str, password: &str) -> Result<(), AuthError> {
        let mut body = workos::user_management::UpdateUserParamsBody::new();
        body.password = Some(workos::user_management::Password::Plaintext {
            password: password.to_owned(),
        });
        let p = workos::user_management::UpdateUserParams::new(body);
        let r = self.client.user_management().update_user(user, p).await;
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
        // WorkOS's real sso_required body carries no pending token; it is still the SSO step.
        let sso_bare = api_error(
            400,
            json!({ "error": "sso_required", "error_description": "use SSO", "connection_ids": ["conn_2"] }),
        );
        assert!(
            matches!(classify(sso_bare), AuthError::Step(Step::Sso { token: None, connection }) if connection == "conn_2")
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
        // WorkOS wraps the real reason of a failed create in `errors[].code`.
        for (reason, want) in [
            ("email_not_available", "already exists"),
            ("email_already_in_use", "already exists"),
            ("user_already_exists", "already exists"),
            ("password_strength_error", "rejected input"),
            ("password_not_allowed", "rejected input"),
            ("invalid_email", "rejected input"),
        ] {
            let e = classify(api_error(
                400,
                json!({ "code": "user_creation_error", "message": "ann@x.com",
                        "errors": [{ "code": reason, "message": "ann@x.com" }] }),
            ));
            assert_eq!(e.to_string(), want, "{reason}");
        }
        let e = classify(api_error(
            400,
            json!({ "code": "user_creation_error", "message": "ann@x.com",
                    "errors": [{ "code": "something_new", "message": "ann@x.com" }, { "code": "Bad ann@x.com" }] }),
        ));
        assert_eq!(
            e.to_string(),
            "workos request failed: status 400 user_creation_error (something_new)"
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
