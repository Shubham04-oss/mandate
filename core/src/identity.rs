//! WorkOS proves who a person is; Mandate exchanges that for its own short-lived assertion and
//! mirrors the customer's directory, so a person removed there loses access on the next sync.
use crate::assertion::{Assertion, TTL_SECS, sign};
use crate::ids::{AgentId, MemberId, SpaceId, TeamId, TenantId};
use crate::mcp::AppState;
use crate::store::{self, Store, StoreError};
use async_trait::async_trait;
use axum::Json;
use axum::extract::State;
use axum::response::{IntoResponse, Response};
use futures::stream::{BoxStream, StreamExt, TryStreamExt};
use http::{HeaderMap, StatusCode};
use jsonwebtoken::jwk::JwkSet;
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode, decode_header};
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;
use time::OffsetDateTime;
use tokio::sync::Mutex;
use workos::EventSchema;
use workos::events::ListEventsParams;

const KEYS_TTL_SECS: i64 = 600;
const MAX_TOKEN_LEN: usize = 8192;
const EVENTS: [&str; 8] = [
    "dsync.user.created",
    "dsync.user.updated",
    "dsync.user.deleted",
    "dsync.group.created",
    "dsync.group.updated",
    "dsync.group.deleted",
    "dsync.group.user_added",
    "dsync.group.user_removed",
];

#[derive(Debug, thiserror::Error)]
pub enum IdentityError {
    #[error("invalid token")]
    InvalidToken,
    #[error("token expired")]
    Expired,
    #[error("token names no organization")]
    NoOrg,
    #[error("workos request failed: {0}")]
    Upstream(String),
    #[error("cannot apply directory event {0}")]
    BadEvent(String),
    #[error(transparent)]
    Store(#[from] StoreError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkosClaims {
    pub user: String,
    pub org: String,
    pub expires_at: i64,
}

/// Where signing keys come from: WorkOS in production, a fixed set in tests.
#[async_trait]
pub trait KeySource: Send + Sync {
    async fn keys(&self) -> Result<JwkSet, IdentityError>;
}

struct WorkosKeys {
    client: workos::Client,
    client_id: String,
}

#[async_trait]
impl KeySource for WorkosKeys {
    async fn keys(&self) -> Result<JwkSet, IdentityError> {
        let upstream = |e: String| IdentityError::Upstream(e);
        let r = self
            .client
            .user_management()
            .get_jwks(&self.client_id)
            .await
            .map_err(|e| upstream(e.to_string()))?;
        // The SDK's typed JWKS is the same JSON jsonwebtoken reads.
        serde_json::to_value(r)
            .and_then(serde_json::from_value)
            .map_err(|e| upstream(e.to_string()))
    }
}

#[derive(Deserialize)]
struct Claims {
    sub: String,
    exp: i64,
    #[serde(default)]
    org_id: Option<String>,
}

pub struct TokenVerifier {
    source: Arc<dyn KeySource>,
    validation: Validation,
    cache: Mutex<Option<(i64, Arc<JwkSet>)>>,
}

impl TokenVerifier {
    pub fn new(client: workos::Client, client_id: String) -> Self {
        let source = Arc::new(WorkosKeys {
            client,
            client_id: client_id.clone(),
        });
        Self::with_source(source, client_id)
    }

    pub fn with_source(source: Arc<dyn KeySource>, client_id: String) -> Self {
        // RS256 only: HS* would let the public key act as a shared secret; `none` never parses.
        let mut v = Validation::new(Algorithm::RS256);
        v.set_issuer(&[format!(
            "https://api.workos.com/user_management/{client_id}"
        )]);
        v.set_required_spec_claims(&["exp", "iss", "sub"]);
        // Expiry is checked against the caller's clock below; WorkOS access tokens carry no audience.
        v.validate_exp = false;
        v.validate_aud = false;
        TokenVerifier {
            source,
            validation: v,
            cache: Mutex::new(None),
        }
    }

    /// The lock is held across the fetch so concurrent misses share one request.
    async fn keys(&self, now: i64, refresh: bool) -> Result<Arc<JwkSet>, IdentityError> {
        let mut cache = self.cache.lock().await;
        if let Some((at, set)) = &*cache
            && !refresh
            && now - at < KEYS_TTL_SECS
        {
            return Ok(set.clone());
        }
        let set = Arc::new(self.source.keys().await?);
        *cache = Some((now, set.clone()));
        Ok(set)
    }

    pub async fn verify(&self, token: &str, now: i64) -> Result<WorkosClaims, IdentityError> {
        use IdentityError::*;
        if token.len() > MAX_TOKEN_LEN {
            return Err(InvalidToken);
        }
        let header = decode_header(token).map_err(|_| InvalidToken)?;
        if header.alg != Algorithm::RS256 {
            return Err(InvalidToken);
        }
        let kid = header.kid.ok_or(InvalidToken)?;
        let mut keys = self.keys(now, false).await?;
        if keys.find(&kid).is_none() {
            // ponytail: any unknown kid costs one JWKS fetch; add a refetch cooldown if abused.
            keys = self.keys(now, true).await?;
        }
        let jwk = keys.find(&kid).ok_or(InvalidToken)?;
        let key = DecodingKey::from_jwk(jwk).map_err(|_| InvalidToken)?;
        let c = decode::<Claims>(token, &key, &self.validation)
            .map_err(|_| InvalidToken)?
            .claims;
        if now >= c.exp {
            return Err(Expired);
        }
        Ok(WorkosClaims {
            user: c.sub,
            org: c.org_id.ok_or(NoOrg)?,
            expires_at: c.exp,
        })
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ExchangeRequest {
    agent: AgentId,
    #[serde(default)]
    space: Option<SpaceId>,
}

/// `POST /v1/assertions`: a WorkOS access token in, a Mandate assertion for one agent out.
pub(crate) async fn exchange(
    State(s): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<ExchangeRequest>,
) -> Response {
    let Some(workos) = &s.workos else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Some(token) = headers
        .get(http::header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "))
    else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let now = OffsetDateTime::now_utc().unix_timestamp();
    let claims = match workos.verify(token, now).await {
        Ok(c) => c,
        Err(IdentityError::Upstream(detail)) => {
            tracing::warn!(%detail, "workos key fetch failed");
            return StatusCode::SERVICE_UNAVAILABLE.into_response();
        }
        Err(_) => return StatusCode::UNAUTHORIZED.into_response(),
    };
    match issue(&s.store, &claims, req, now).await {
        Ok(Some(a)) => Json(json!({
            "assertion": sign(&a, &s.assertion_signing_key),
            "expires_at": a.expires_at,
        }))
        .into_response(),
        Ok(None) => StatusCode::FORBIDDEN.into_response(),
        Err(error) => {
            tracing::warn!(%error, "assertion exchange failed");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

/// `None` unless the org is a tenant, the user a member, and the agent theirs or their team's.
async fn issue(
    store: &Store,
    c: &WorkosClaims,
    req: ExchangeRequest,
    now: i64,
) -> Result<Option<Assertion>, StoreError> {
    let Some(tenant) = store.tenant_for_org(&c.org).await? else {
        return Ok(None);
    };
    let mut tx = store.begin(&tenant).await?;
    let Some(member) = store::member_for_workos_user(&mut tx, &c.user).await? else {
        return Ok(None);
    };
    if !store::may_use_agent(&mut tx, &member, &req.agent).await? {
        return Ok(None);
    }
    Ok(Some(Assertion {
        tenant,
        member,
        agent: req.agent,
        space: req.space,
        // Never outlives the WorkOS proof it came from.
        expires_at: (now + TTL_SECS).min(c.expires_at),
    }))
}

/// The directory's event stream after a cursor: WorkOS in production, recorded events in tests.
pub trait EventSource: Send + Sync {
    fn events_after(
        &self,
        after: Option<String>,
    ) -> BoxStream<'_, Result<EventSchema, IdentityError>>;
}

impl EventSource for workos::Client {
    // `list_events_auto_paging` borrows a temporary `EventsApi`, so its stream cannot be
    // returned; this is the same pager over `list_events`, borrowing the client instead.
    fn events_after(
        &self,
        after: Option<String>,
    ) -> BoxStream<'_, Result<EventSchema, IdentityError>> {
        let mut start = Some(after);
        workos::auto_paginate(move |page: Option<String>| {
            let mut p = ListEventsParams::new(EVENTS.map(String::from).to_vec());
            p.limit = Some(100);
            p.after = page.or_else(|| start.take().flatten());
            async move {
                let l = self.events().list_events(p).await?;
                Ok(workos::Page {
                    data: l.data,
                    list_metadata: workos::ListMetadata {
                        before: None,
                        after: l.list_metadata.after,
                    },
                })
            }
        })
        .map_err(|e| IdentityError::Upstream(e.to_string()))
        .boxed()
    }
}

#[derive(Deserialize)]
struct DirUser {
    id: String,
    organization_id: String,
    state: String,
}

#[derive(Deserialize)]
struct DirGroup {
    id: String,
    organization_id: String,
}

#[derive(Deserialize)]
struct GroupUser {
    user: DirUser,
    group: DirGroup,
}

enum Change {
    User { user: DirUser, gone: bool },
    Group { group: DirGroup, gone: bool },
    Membership { m: GroupUser, added: bool },
}

impl Change {
    fn org(&self) -> &str {
        match self {
            Change::User { user, .. } => &user.organization_id,
            Change::Group { group, .. } => &group.organization_id,
            Change::Membership { m, .. } => &m.group.organization_id,
        }
    }
}

/// A WorkOS id as a Mandate id: lowercase, other characters as `_`, at most 64 long.
fn slug(s: &str) -> String {
    s.chars()
        .map(|c| c.to_ascii_lowercase())
        .map(|c| {
            if c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .take(64)
        .collect()
}

/// `None` for an event type Mandate does not act on.
fn parse(e: &EventSchema) -> Result<Option<Change>, IdentityError> {
    let data = serde_json::Value::Object(e.data.clone().into_iter().collect());
    let bad = |_| IdentityError::BadEvent(e.id.clone());
    let user = |gone: bool| -> Result<Change, IdentityError> {
        let user: DirUser = serde_json::from_value(data.clone()).map_err(bad)?;
        // Suspended or inactive in the directory counts as gone.
        let gone = gone || user.state != "active";
        Ok(Change::User { user, gone })
    };
    let group = |gone| -> Result<Change, IdentityError> {
        Ok(Change::Group {
            group: serde_json::from_value(data.clone()).map_err(bad)?,
            gone,
        })
    };
    let membership = |added| -> Result<Change, IdentityError> {
        Ok(Change::Membership {
            m: serde_json::from_value(data.clone()).map_err(bad)?,
            added,
        })
    };
    Ok(Some(match e.event.as_str() {
        "dsync.user.created" | "dsync.user.updated" => user(false)?,
        "dsync.user.deleted" => user(true)?,
        "dsync.group.created" | "dsync.group.updated" => group(false)?,
        "dsync.group.deleted" => group(true)?,
        "dsync.group.user_added" => membership(true)?,
        "dsync.group.user_removed" => membership(false)?,
        _ => return Ok(None),
    }))
}

async fn tenant_for(store: &Store, org: &str, event: &str) -> Result<TenantId, IdentityError> {
    if let Some(t) = store.tenant_for_org(org).await? {
        return Ok(t);
    }
    let bad = || IdentityError::BadEvent(event.to_owned());
    let id = TenantId::parse(&slug(org)).map_err(|_| bad())?;
    store.ensure_tenant(org, &id).await?.ok_or_else(bad)
}

/// One event, one transaction: the change and the cursor commit together or not at all.
async fn apply(store: &Store, e: &EventSchema, change: Change) -> Result<(), IdentityError> {
    let tenant = tenant_for(store, change.org(), &e.id).await?;
    let bad = |_| IdentityError::BadEvent(e.id.clone());
    let mut tx = store.begin(&tenant).await?;
    match change {
        Change::User { user, gone } => {
            let m = MemberId::parse(&slug(&user.id)).map_err(bad)?;
            if gone {
                store::remove_member(&mut tx, &m).await?;
            } else {
                store::upsert_member(&mut tx, &m, &user.id).await?;
            }
        }
        Change::Group { group, gone } => {
            let t = TeamId::parse(&slug(&group.id)).map_err(bad)?;
            if gone {
                store::delete_team(&mut tx, &t).await?;
            } else {
                store::upsert_team(&mut tx, &t, &group.id).await?;
            }
        }
        Change::Membership { m, added } => {
            let member = MemberId::parse(&slug(&m.user.id)).map_err(bad)?;
            let team = TeamId::parse(&slug(&m.group.id)).map_err(bad)?;
            if !added {
                store::set_team_member(&mut tx, &team, &member, false).await?;
            } else if m.user.state == "active" {
                store::upsert_member(&mut tx, &member, &m.user.id).await?;
                store::upsert_team(&mut tx, &team, &m.group.id).await?;
                store::set_team_member(&mut tx, &team, &member, true).await?;
            }
        }
    }
    store::set_directory_cursor(&mut tx, &e.id).await?;
    tx.commit().await.map_err(StoreError::from)?;
    tracing::info!(event = %e.event, id = %e.id, tenant = %tenant, "directory event applied");
    Ok(())
}

/// Pulls directory events after the stored cursor and applies each idempotently; returns
/// how many were applied. Stops at the first failure, leaving the cursor before it.
pub async fn sync_directory(
    store: &Store,
    source: &impl EventSource,
) -> Result<u64, IdentityError> {
    let mut events = source.events_after(store.directory_cursor().await?);
    let mut applied = 0;
    while let Some(e) = events.next().await {
        let e = e?;
        // An unknown type is skipped; the next applied event's cursor moves past it.
        if let Some(change) = parse(&e)? {
            apply(store, &e, change).await?;
            applied += 1;
        }
    }
    Ok(applied)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs_are_valid_ids() {
        assert_eq!(slug("org_01HXYZ"), "org_01hxyz");
        assert_eq!(slug("a.b c/é"), "a_b_c__");
        assert_eq!(slug(&"X".repeat(80)).len(), 64);
        assert!(TenantId::parse(&slug("directory_user_01E1JG7J09H96KYP8HM9B0G5SJ")).is_ok());
    }
}
