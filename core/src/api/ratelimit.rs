//! Sign-in rate limits, counted in Postgres per one-minute window: no extra service.
use super::{ApiError, ApiState};
use crate::store::{Store, StoreError};
use axum::body::{Body, to_bytes};
use axum::extract::{ConnectInfo, Request, State};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use time::OffsetDateTime;

/// Per email (or pending step, or token) per minute.
pub const PER_SUBJECT: i32 = 5;
pub const PER_IP: i32 = 30;
const WINDOW_SECS: i64 = 60;

/// Counts one attempt against `key`; `Some(seconds)` to wait when it is over `limit`.
pub async fn hit(
    store: &Store,
    key: &str,
    limit: i32,
    now: OffsetDateTime,
) -> Result<Option<i64>, StoreError> {
    let t = now.unix_timestamp();
    let start = OffsetDateTime::from_unix_timestamp(t - t.rem_euclid(WINDOW_SECS))
        .map_err(|_| sqlx::Error::Protocol("clock out of range".into()))?;
    let digest: [u8; 32] = Sha256::digest(key.as_bytes()).into();
    let mut tx = store.begin_untenanted().await?;
    // Old windows go in the same statement, so the table stays a minute or two deep.
    let count: i32 = sqlx::query_scalar(
        "with gone as (delete from rate_limits where window_start < $3) \
         insert into rate_limits (key, window_start, count) values ($1, $2, 1) \
         on conflict (key, window_start) do update set count = rate_limits.count + 1 \
         returning count",
    )
    .bind(digest.as_slice())
    .bind(start)
    .bind(start - time::Duration::seconds(WINDOW_SECS))
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok((count > limit).then(|| (WINDOW_SECS - t.rem_euclid(WINDOW_SECS)).max(1)))
}

/// Cloudflare's `CF-Connecting-IP` (the ALB only accepts Cloudflare), else the peer address.
fn client_ip(req: &Request) -> String {
    let cf = req
        .headers()
        .get("cf-connecting-ip")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.trim().parse::<IpAddr>().ok());
    let peer = || {
        req.extensions()
            .get::<ConnectInfo<SocketAddr>>()
            .map(|c| c.0.ip())
    };
    // ponytail: without either (tests, a misconfigured server) all callers share one bucket.
    cf.or_else(peer)
        .map_or_else(|| "unknown".into(), |ip| ip.to_string())
}

/// What one person is trying to get into: the email, else the pending step, else the token.
fn subject(body: &[u8]) -> Option<String> {
    let v: Value = serde_json::from_slice(body).ok()?;
    if let Some(email) = v.get("email").and_then(Value::as_str) {
        return Some(format!("email:{}", email.trim().to_lowercase()));
    }
    ["pending", "token"]
        .into_iter()
        .find_map(|k| v.get(k).and_then(Value::as_str).map(|s| format!("{k}:{s}")))
}

/// Layer for `/v1/auth/*`: 429 with `Retry-After` once either the caller's IP or the subject
/// of the request is over its limit. Every attempt counts against both.
pub async fn limit(State(s): State<Arc<ApiState>>, req: Request, next: Next) -> Response {
    let ip = client_ip(&req);
    let (parts, body) = req.into_parts();
    let Ok(bytes) = to_bytes(body, super::MAX_BODY_BYTES).await else {
        return ApiError::invalid_input().into_response();
    };
    let now = OffsetDateTime::now_utc();
    let mut keys = vec![(format!("ip:{ip}"), PER_IP)];
    if let Some(subject) = subject(&bytes) {
        keys.push((subject, PER_SUBJECT));
    }
    let mut wait = None;
    for (key, max) in keys {
        match hit(&s.store, &key, max, now).await {
            Ok(w) => wait = wait.max(w),
            // Fail closed: an attempt that cannot be counted is not let through.
            Err(e) => return ApiError::from(e).into_response(),
        }
    }
    if let Some(secs) = wait {
        return ApiError::rate_limited(secs).into_response();
    }
    next.run(Request::from_parts(parts, Body::from(bytes)))
        .await
}
