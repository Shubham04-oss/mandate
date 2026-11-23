//! Sign-in rate limits, counted in Postgres per one-minute window: no extra service.
use super::{ApiError, ApiState};
use crate::store::{Store, StoreError};
use axum::body::{Body, to_bytes};
use axum::extract::{ConnectInfo, Request, State};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::net::{IpAddr, Ipv6Addr, SocketAddr};
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

/// Cloudflare's `CF-Connecting-IP` when `trust_cf` and the peer is loopback (the cloudflared
/// sidecar; any other peer, eve included, could forge it), else the peer address. IPv6 counts per
/// /64, the block one host is usually given.
fn client_ip(req: &Request, trust_cf: bool) -> String {
    let cf = || {
        req.headers()
            .get("cf-connecting-ip")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.trim().parse::<IpAddr>().ok())
    };
    let peer = req
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|c| c.0.ip());
    let ip = match peer {
        Some(p) if trust_cf && p.to_canonical().is_loopback() => cf().or(peer),
        _ => peer,
    };
    // ponytail: without either (tests, a misconfigured server) all callers share one bucket.
    match ip.map(|ip| ip.to_canonical()) {
        Some(IpAddr::V6(v6)) => format!(
            "{}/64",
            Ipv6Addr::from(v6.to_bits() & !u128::from(u64::MAX))
        ),
        Some(v4) => v4.to_string(),
        None => "unknown".into(),
    }
}

/// What one person is trying to get into: the email, else the step (`/mfa/enable`'s challenge
/// before its `pending`, which it reads only as a flag), else the token.
fn subject(body: &[u8]) -> Option<String> {
    let v: Value = serde_json::from_slice(body).ok()?;
    if let Some(email) = v.get("email").and_then(Value::as_str) {
        return Some(email_key(email));
    }
    ["challenge", "pending", "token"]
        .into_iter()
        .find_map(|k| v.get(k).and_then(Value::as_str).map(|s| format!("{k}:{s}")))
}

/// One bucket per email, however it was typed.
pub fn email_key(email: &str) -> String {
    format!("email:{}", email.trim().to_lowercase())
}

/// Counts an attempt against an identity a handler learns only after the layer ran (an
/// invitation's email, a step's user); refused exactly like the layer refuses.
pub async fn charge(s: &ApiState, key: &str) -> Result<(), ApiError> {
    match hit(&s.store, key, PER_SUBJECT, OffsetDateTime::now_utc()).await? {
        Some(secs) => Err(ApiError::rate_limited(secs)),
        None => Ok(()),
    }
}

/// Layer for `/v1/auth/*`: 429 with `Retry-After` once either the caller's IP or the subject
/// of the request is over its limit. Every attempt counts against both.
pub async fn limit(State(s): State<Arc<ApiState>>, req: Request, next: Next) -> Response {
    let ip = client_ip(&req, s.trust_cf_ip);
    let (parts, body) = req.into_parts();
    let Ok(bytes) = to_bytes(body, super::MAX_BODY_BYTES).await else {
        return ApiError::invalid_input().into_response();
    };
    let now = OffsetDateTime::now_utc();
    let mut keys = vec![(format!("ip:{ip}"), PER_IP)];
    // An invitation's token is in its path, not its body.
    let invite = parts
        .uri
        .path()
        .strip_prefix("/v1/auth/invite/")
        .and_then(|rest| rest.split('/').next())
        .map(|token| format!("invite:{token}"));
    if let Some(subject) = subject(&bytes).or(invite) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn req(cf: Option<&str>, peer: &str) -> Request {
        let mut b = Request::builder();
        if let Some(ip) = cf {
            b = b.header("cf-connecting-ip", ip);
        }
        let mut r = b.body(Body::empty()).unwrap();
        r.extensions_mut()
            .insert(ConnectInfo(SocketAddr::new(peer.parse().unwrap(), 443)));
        r
    }

    #[test]
    fn cf_header_ignored_without_the_flag() {
        assert_eq!(
            client_ip(&req(Some("1.2.3.4"), "10.0.0.9"), false),
            "10.0.0.9"
        );
    }

    #[test]
    fn cf_header_used_with_the_flag_from_loopback() {
        assert_eq!(
            client_ip(&req(Some("1.2.3.4"), "127.0.0.1"), true),
            "1.2.3.4"
        );
        assert_eq!(client_ip(&req(Some("1.2.3.4"), "::1"), true), "1.2.3.4");
        // A header that is not an address falls back to the peer.
        assert_eq!(
            client_ip(&req(Some("nope"), "127.0.0.1"), true),
            "127.0.0.1"
        );
    }

    #[test]
    fn cf_header_from_a_non_loopback_peer_is_ignored() {
        // eve reaches the core's port directly and could forge the header.
        assert_eq!(
            client_ip(&req(Some("1.2.3.4"), "10.0.0.9"), true),
            "10.0.0.9"
        );
        assert_eq!(
            client_ip(&req(Some("1.2.3.4"), "2001:db8:1:2::1"), true),
            "2001:db8:1:2::/64"
        );
    }

    #[test]
    fn ipv6_in_one_slash_64_share_a_bucket() {
        let a = client_ip(&req(None, "2001:db8:1:2::1"), false);
        let b = client_ip(
            &req(Some("2001:db8:1:2:ffff:eeee:dddd:cccc"), "127.0.0.1"),
            true,
        );
        assert_eq!(a, "2001:db8:1:2::/64");
        assert_eq!(a, b);
        assert_ne!(a, client_ip(&req(None, "2001:db8:1:3::1"), false));
        // An IPv4-mapped peer is its IPv4 address, not a /64 of all of IPv4.
        assert_eq!(client_ip(&req(None, "::ffff:10.0.0.9"), false), "10.0.0.9");
    }

    #[test]
    fn mfa_enable_is_limited_per_challenge() {
        // `/mfa/enable` reads only the challenge; a fresh `pending` per guess must not reset it.
        let body = |pending: &str| {
            json!({ "pending": pending, "challenge": "c1", "code": "1" }).to_string()
        };
        assert_eq!(
            subject(body("a").as_bytes()).as_deref(),
            Some("challenge:c1")
        );
        assert_eq!(subject(body("a").as_bytes()), subject(body("b").as_bytes()));
        assert_eq!(
            subject(br#"{"challenge":"c1","code":"1"}"#).as_deref(),
            Some("challenge:c1")
        );
    }
}
