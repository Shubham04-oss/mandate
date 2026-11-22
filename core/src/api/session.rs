//! Web sessions and pending sign-in steps. The browser holds random 256-bit values; the
//! database holds only their SHA-256, so a leaked table cannot be replayed as cookies.
use super::{ApiError, ApiState, csrf_ok};
use crate::ids::{MemberId, TenantId};
use crate::store::{StoreError, Tx};
use axum::extract::{FromRef, FromRequestParts};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD as B64};
use http::request::Parts;
use http::{HeaderMap, HeaderValue, header};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::sync::Arc;
use time::{Duration, OffsetDateTime};

pub const COOKIE: &str = "mandate_session";
pub const IDLE: Duration = Duration::hours(12);
pub const ABSOLUTE: Duration = Duration::days(7);
pub const PENDING_TTL: Duration = Duration::minutes(10);
const MAX_TOKEN_LEN: usize = 160;

/// Who is calling, from a valid session cookie. Extracting it also enforces CSRF on unsafe
/// methods, so a handler that takes a `Session` is protected wherever it is mounted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    pub tenant: TenantId,
    pub member: MemberId,
    pub is_admin: bool,
}

impl<S> FromRequestParts<S> for Session
where
    Arc<ApiState>: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, ApiError> {
        let s = Arc::<ApiState>::from_ref(state);
        if !csrf_ok(&parts.method, &parts.headers, &s.web_origin) {
            return Err(ApiError::forbidden());
        }
        current(&s, &parts.headers).await
    }
}

/// The session the request's cookie names, touched to now; no CSRF check (see `Session`).
pub async fn current(s: &ApiState, headers: &HeaderMap) -> Result<Session, ApiError> {
    let token = cookie(headers).ok_or_else(ApiError::signed_out)?;
    let tenant = token_tenant(token).ok_or_else(ApiError::signed_out)?;
    let mut tx = s.store.begin(&tenant).await?;
    let found = lookup(&mut tx, token, OffsetDateTime::now_utc()).await?;
    tx.commit().await?;
    found.ok_or_else(ApiError::signed_out)
}

/// A fresh random 256-bit value, base64url without padding (43 characters).
pub(crate) fn secret() -> Result<String, ApiError> {
    let mut b = [0u8; 32];
    getrandom::fill(&mut b).map_err(|_| {
        tracing::error!("system random source failed");
        ApiError::internal()
    })?;
    Ok(B64.encode(b))
}

pub(crate) fn hash(s: &str) -> [u8; 32] {
    Sha256::digest(s.as_bytes()).into()
}

/// The session cookie's value, if the request carries one.
pub fn cookie(headers: &HeaderMap) -> Option<&str> {
    named_cookie(headers, COOKIE)
}

pub(crate) fn named_cookie<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .filter_map(|c| c.trim().split_once('='))
        .find(|(k, _)| *k == name)
        .map(|(_, v)| v)
}

/// The tenant a cookie names; the secret must still match a row in that tenant.
pub(crate) fn token_tenant(token: &str) -> Option<TenantId> {
    if token.len() > MAX_TOKEN_LEN {
        return None;
    }
    let (tenant, secret) = token.split_once('.')?;
    if secret.is_empty() {
        return None;
    }
    TenantId::parse(tenant).ok()
}

fn cookie_header(value: &str, max_age: i64) -> Result<HeaderValue, ApiError> {
    HeaderValue::from_str(&format!(
        "{COOKIE}={value}; Path=/; Max-Age={max_age}; HttpOnly; Secure; SameSite=Lax"
    ))
    .map_err(|_| ApiError::internal())
}

/// `Set-Cookie` for a new session: host-only (no `Domain`), never readable by script.
pub fn set_cookie(value: &str) -> Result<HeaderValue, ApiError> {
    cookie_header(value, ABSOLUTE.whole_seconds())
}

pub fn clear_cookie() -> HeaderValue {
    HeaderValue::from_static("mandate_session=; Path=/; Max-Age=0; HttpOnly; Secure; SameSite=Lax")
}

pub const SSO_COOKIE: &str = "mandate_sso";

/// Binds an SSO state to the browser that started it, so a callback link made elsewhere
/// (login CSRF) is refused. Lax still sends it on WorkOS's top-level redirect back.
pub fn sso_cookie(state: &str) -> Result<HeaderValue, ApiError> {
    HeaderValue::from_str(&format!(
        "{SSO_COOKIE}={state}; Path=/v1/auth/sso; Max-Age={}; HttpOnly; Secure; SameSite=Lax",
        PENDING_TTL.whole_seconds()
    ))
    .map_err(|_| ApiError::internal())
}

pub fn clear_sso_cookie() -> HeaderValue {
    HeaderValue::from_static(
        "mandate_sso=; Path=/v1/auth/sso; Max-Age=0; HttpOnly; Secure; SameSite=Lax",
    )
}

/// A new session for `member` in `tenant` (the transaction's tenant); returns the cookie
/// value `<tenant>.<secret>`. Clears the member's dead sessions while here.
pub async fn create(
    tx: &mut Tx,
    tenant: &TenantId,
    member: &MemberId,
    profile: &Value,
    now: OffsetDateTime,
) -> Result<String, ApiError> {
    let token = format!("{tenant}.{}", secret()?);
    sqlx::query(
        "delete from sessions where tenant_id = current_setting('mandate.tenant', true) \
         and member = $1 and (expires_at <= $2 or last_seen_at <= $3)",
    )
    .bind(member.as_str())
    .bind(now)
    .bind(now - IDLE)
    .execute(&mut **tx)
    .await?;
    sqlx::query(
        "insert into sessions (tenant_id, token_hash, member, created_at, last_seen_at, expires_at, profile) \
         values (current_setting('mandate.tenant'), $1, $2, $3, $3, $4, $5)",
    )
    .bind(hash(&token).as_slice())
    .bind(member.as_str())
    .bind(now)
    .bind(now + ABSOLUTE)
    .bind(sqlx::types::Json(profile))
    .execute(&mut **tx)
    .await?;
    Ok(token)
}

/// The live session for this cookie in the transaction's tenant, touched to `now`; `None` if
/// it is unknown, idle for 12 hours, past its 7 days, or its member is gone.
pub async fn lookup(
    tx: &mut Tx,
    token: &str,
    now: OffsetDateTime,
) -> Result<Option<Session>, StoreError> {
    let Some(tenant) = token_tenant(token) else {
        return Ok(None);
    };
    let row: Option<(String, bool)> = sqlx::query_as(
        "update sessions s set last_seen_at = $2 from members m \
         where s.tenant_id = current_setting('mandate.tenant', true) and s.token_hash = $1 \
         and s.expires_at > $2 and s.last_seen_at > $3 \
         and m.tenant_id = current_setting('mandate.tenant', true) and m.id = s.member \
         returning s.member, m.is_admin",
    )
    .bind(hash(token).as_slice())
    .bind(now)
    .bind(now - IDLE)
    .fetch_optional(&mut **tx)
    .await?;
    let Some((member, is_admin)) = row else {
        return Ok(None);
    };
    Ok(Some(Session {
        tenant,
        member: MemberId::parse(&member)?,
        is_admin,
    }))
}

/// The profile snapshot stored at sign-in, for a live session.
pub async fn profile(tx: &mut Tx, token: &str) -> Result<Option<Value>, StoreError> {
    let p: Option<sqlx::types::Json<Value>> = sqlx::query_scalar(
        "select profile from sessions where tenant_id = current_setting('mandate.tenant', true) \
         and token_hash = $1",
    )
    .bind(hash(token).as_slice())
    .fetch_optional(&mut **tx)
    .await?;
    Ok(p.map(|p| p.0))
}

pub async fn delete(tx: &mut Tx, token: &str) -> Result<(), StoreError> {
    sqlx::query(
        "delete from sessions where tenant_id = current_setting('mandate.tenant', true) and token_hash = $1",
    )
    .bind(hash(token).as_slice())
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub struct NewPending<'a> {
    pub kind: &'static str,
    /// WorkOS's pending authentication token; stays in the database.
    pub workos_token: Option<&'a str>,
    pub email: Option<&'a str>,
    pub data: Value,
    /// Set only for a step that belongs to a session; must be the transaction's tenant.
    pub tenant: Option<&'a TenantId>,
}

/// Not `Debug`: it holds WorkOS's pending token.
#[derive(Clone)]
pub struct Pending {
    pub kind: String,
    pub workos_token: Option<String>,
    pub email: Option<String>,
    pub data: Value,
}

/// Stores a step for 10 minutes and returns the id the browser holds; clears expired steps.
pub async fn create_pending(
    tx: &mut Tx,
    p: NewPending<'_>,
    now: OffsetDateTime,
) -> Result<String, ApiError> {
    let id = secret()?;
    sqlx::query("delete from auth_pending where expires_at <= $1")
        .bind(now)
        .execute(&mut **tx)
        .await?;
    sqlx::query(
        "insert into auth_pending (tenant_id, id_hash, kind, workos_token, email, data, expires_at) \
         values ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(p.tenant.map(TenantId::as_str))
    .bind(hash(&id).as_slice())
    .bind(p.kind)
    .bind(p.workos_token)
    .bind(p.email)
    .bind(sqlx::types::Json(&p.data))
    .bind(now + PENDING_TTL)
    .execute(&mut **tx)
    .await?;
    Ok(id)
}

type PendingRow = (
    String,
    Option<String>,
    Option<String>,
    sqlx::types::Json<Value>,
);

/// The unexpired step with this id and one of `kinds`, visible to the transaction's tenant
/// (or to an untenanted transaction, if it has none).
pub async fn pending(
    tx: &mut Tx,
    id: &str,
    kinds: &[&str],
    now: OffsetDateTime,
) -> Result<Option<Pending>, StoreError> {
    if id.is_empty() || id.len() > MAX_TOKEN_LEN {
        return Ok(None);
    }
    let kinds: Vec<String> = kinds.iter().map(|k| (*k).to_owned()).collect();
    let row: Option<PendingRow> =
        sqlx::query_as(
            "select kind, workos_token, email, data from auth_pending \
             where id_hash = $1 and kind = any($2) and expires_at > $3 \
             and tenant_id is not distinct from nullif(current_setting('mandate.tenant', true), '')",
        )
        .bind(hash(id).as_slice())
        .bind(&kinds)
        .bind(now)
        .fetch_optional(&mut **tx)
        .await?;
    Ok(row.map(|(kind, workos_token, email, data)| Pending {
        kind,
        workos_token,
        email,
        data: data.0,
    }))
}

/// Like `pending`, but deletes the step in the same statement: of two concurrent callers, one
/// gets it and the other `None`.
pub async fn take_pending(
    tx: &mut Tx,
    id: &str,
    kinds: &[&str],
    now: OffsetDateTime,
) -> Result<Option<Pending>, StoreError> {
    if id.is_empty() || id.len() > MAX_TOKEN_LEN {
        return Ok(None);
    }
    let kinds: Vec<String> = kinds.iter().map(|k| (*k).to_owned()).collect();
    let row: Option<PendingRow> = sqlx::query_as(
        "delete from auth_pending \
         where id_hash = $1 and kind = any($2) and expires_at > $3 \
         and tenant_id is not distinct from nullif(current_setting('mandate.tenant', true), '') \
         returning kind, workos_token, email, data",
    )
    .bind(hash(id).as_slice())
    .bind(&kinds)
    .bind(now)
    .fetch_optional(&mut **tx)
    .await?;
    Ok(row.map(|(kind, workos_token, email, data)| Pending {
        kind,
        workos_token,
        email,
        data: data.0,
    }))
}

/// Single use: a finished or superseded step is removed.
pub async fn delete_pending(tx: &mut Tx, id: &str) -> Result<(), StoreError> {
    sqlx::query("delete from auth_pending where id_hash = $1")
        .bind(hash(id).as_slice())
        .execute(&mut **tx)
        .await?;
    Ok(())
}
