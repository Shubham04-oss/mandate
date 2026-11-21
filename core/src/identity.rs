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
use std::sync::{Arc, RwLock};
use time::OffsetDateTime;
use tokio::sync::Mutex;
use workos::EventSchema;
use workos::events::ListEventsParams;
use workos::user_management::ListUsersParams;

const KEYS_TTL_SECS: i64 = 600;
const FORCED_REFRESH_COOLDOWN_SECS: i64 = 60;
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
    #[error(transparent)]
    Store(#[from] StoreError),
}

/// A WorkOS failure by kind only, never its text: a transport error's text carries the request
/// URL (query parameters such as `email=`), and an API message may echo request data.
fn upstream(e: workos::Error) -> IdentityError {
    IdentityError::Upstream(match &e {
        workos::Error::Api(a) => format!("status {}", a.status),
        workos::Error::Network(t) => format!("network ({:?})", t.kind),
        workos::Error::Decode(_) => "decode".into(),
        _ => "other".into(),
    })
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
        let r = self
            .client
            .user_management()
            .get_jwks(&self.client_id)
            .await
            .map_err(upstream)?;
        // The SDK's typed JWKS is the same JSON jsonwebtoken reads.
        serde_json::to_value(r)
            .and_then(serde_json::from_value)
            .map_err(|_| IdentityError::Upstream("jwks decode".into()))
    }
}

/// AuthKit users, which are not directory users: the two are linked by verified email.
#[async_trait]
pub trait UserLookup: Send + Sync {
    /// The AuthKit user in `org` whose verified email is `email`.
    async fn user_by_email(&self, email: &str, org: &str) -> Result<Option<String>, IdentityError>;
    /// The user's email, only if verified.
    async fn verified_email(&self, user: &str) -> Result<Option<String>, IdentityError>;
}

#[async_trait]
impl UserLookup for workos::Client {
    async fn user_by_email(&self, email: &str, org: &str) -> Result<Option<String>, IdentityError> {
        let p = ListUsersParams {
            email: Some(email.to_owned()),
            organization_id: Some(org.to_owned()),
            ..Default::default()
        };
        let users = self
            .user_management()
            .list_users(p)
            .await
            .map_err(upstream)?;
        Ok(users
            .data
            .into_iter()
            .find(|u| u.email_verified && u.email.eq_ignore_ascii_case(email))
            .map(|u| u.id))
    }

    async fn verified_email(&self, user: &str) -> Result<Option<String>, IdentityError> {
        let u = self
            .user_management()
            .get_user(user)
            .await
            .map_err(upstream)?;
        Ok(u.email_verified.then_some(u.email))
    }
}

#[derive(Deserialize)]
struct Claims {
    sub: String,
    exp: i64,
    #[serde(default)]
    org_id: Option<String>,
}

type CachedKeys = Option<(i64, Arc<JwkSet>)>;

pub struct TokenVerifier {
    source: Arc<dyn KeySource>,
    validation: Validation,
    /// Read without waiting on a fetch in flight.
    keys: RwLock<CachedKeys>,
    /// Held across a fetch so there is one at a time; holds the last forced refresh time.
    refreshing: Mutex<Option<i64>>,
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
            keys: RwLock::new(None),
            refreshing: Mutex::new(None),
        }
    }

    fn cached(&self) -> CachedKeys {
        self.keys.read().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// Fetches unless another caller refreshed while this one waited, or (`forced`, for an
    /// unknown kid) a forced refresh ran in the last minute: unknown kids cannot flood WorkOS.
    async fn refresh(
        &self,
        now: i64,
        seen: Option<&Arc<JwkSet>>,
        forced: bool,
    ) -> Result<Arc<JwkSet>, IdentityError> {
        let mut last_forced = self.refreshing.lock().await;
        if let Some((at, set)) = self.cached() {
            let replaced = seen.is_none_or(|s| !Arc::ptr_eq(s, &set));
            let cooling = last_forced.is_some_and(|t| now - t < FORCED_REFRESH_COOLDOWN_SECS);
            if (replaced && now - at < KEYS_TTL_SECS) || (forced && cooling) {
                return Ok(set);
            }
        }
        // Before the fetch: a failing WorkOS must not be retried on every unknown kid either.
        if forced {
            *last_forced = Some(now);
        }
        let set = Arc::new(self.source.keys().await?);
        *self.keys.write().unwrap_or_else(|e| e.into_inner()) = Some((now, set.clone()));
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
        let mut keys = match self.cached() {
            Some((at, set)) if now - at < KEYS_TTL_SECS => set,
            stale => {
                self.refresh(now, stale.as_ref().map(|c| &c.1), false)
                    .await?
            }
        };
        if keys.find(&kid).is_none() {
            keys = self.refresh(now, Some(&keys), true).await?;
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

/// What the exchange needs from WorkOS.
pub struct Workos {
    pub verifier: TokenVerifier,
    pub users: Arc<dyn UserLookup>,
}

impl Workos {
    pub fn new(client: workos::Client, client_id: String) -> Self {
        Workos {
            verifier: TokenVerifier::new(client.clone(), client_id),
            users: Arc::new(client),
        }
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
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let Some(token) = headers
        .get(http::header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "))
    else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let now = OffsetDateTime::now_utc().unix_timestamp();
    let claims = match workos.verifier.verify(token, now).await {
        Ok(c) => c,
        Err(IdentityError::Upstream(detail)) => {
            tracing::warn!(%detail, "workos key fetch failed");
            return StatusCode::SERVICE_UNAVAILABLE.into_response();
        }
        Err(_) => return StatusCode::UNAUTHORIZED.into_response(),
    };
    match issue(&s.store, workos.users.as_ref(), &claims, req, now).await {
        Ok(Some(a)) => Json(json!({
            "assertion": sign(&a, &s.assertion_signing_key),
            "expires_at": a.expires_at,
        }))
        .into_response(),
        Ok(None) => StatusCode::FORBIDDEN.into_response(),
        Err(IdentityError::Upstream(detail)) => {
            tracing::warn!(%detail, "workos user lookup failed");
            StatusCode::SERVICE_UNAVAILABLE.into_response()
        }
        Err(error) => {
            tracing::warn!(%error, "assertion exchange failed");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

/// `None` unless the org is a tenant, the user a member, and the agent theirs or their team's.
async fn issue(
    store: &Store,
    users: &dyn UserLookup,
    c: &WorkosClaims,
    req: ExchangeRequest,
    now: i64,
) -> Result<Option<Assertion>, IdentityError> {
    let Some(tenant) = store.tenant_for_org(&c.org).await? else {
        return Ok(None);
    };
    let mut tx = store.begin(&tenant).await?;
    let member = match store::member_for_workos_user(&mut tx, &c.user).await? {
        Some(m) => m,
        None => {
            // First sign-in: link by verified email, within this org's tenant only. The
            // lookup is a network call, so no transaction is held across it.
            drop(tx);
            let Some(email) = users.verified_email(&c.user).await? else {
                return Ok(None);
            };
            tx = store.begin(&tenant).await?;
            let Some(m) = store::link_member_by_email(&mut tx, &email, &c.user).await? else {
                return Ok(None);
            };
            m
        }
    };
    if !store::may_use_agent(&mut tx, &member, &req.agent).await? {
        return Ok(None);
    }
    tx.commit().await.map_err(StoreError::from)?;
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
        .map_err(upstream)
        .boxed()
    }
}

#[derive(Deserialize)]
struct DirUser {
    id: String,
    organization_id: String,
    state: String,
    #[serde(default)]
    email: Option<String>,
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

/// An event reduced to what it does, with every id already validated.
enum Change {
    Member {
        id: MemberId,
        directory_user: String,
        email: Option<String>,
        workos_user: Option<String>,
    },
    RemoveMember(MemberId),
    Team {
        id: TeamId,
        group: String,
    },
    RemoveTeam(TeamId),
    Join {
        team: TeamId,
        group: String,
        member: MemberId,
        directory_user: String,
        email: Option<String>,
        workos_user: Option<String>,
    },
    Leave {
        team: TeamId,
        member: MemberId,
    },
    /// Nothing to change (an inactive user joining a group); only the cursor moves.
    Nothing,
}

/// Why an event was not applied: a permanent fault is dead-lettered and passed; a transient
/// one stops the run so the next tick retries it.
enum Fail {
    Permanent { org: Option<String>, error: String },
    Transient(IdentityError),
}

impl From<StoreError> for Fail {
    fn from(e: StoreError) -> Self {
        Fail::Transient(e.into())
    }
}

impl From<IdentityError> for Fail {
    fn from(e: IdentityError) -> Self {
        Fail::Transient(e)
    }
}

fn permanent(org: Option<&str>, error: impl std::fmt::Display) -> Fail {
    Fail::Permanent {
        org: org.map(str::to_owned),
        error: error.to_string(),
    }
}

/// Only the category and position: serde's message can quote values such as emails.
fn malformed(err: &serde_json::Error) -> Fail {
    let category = match err.classify() {
        serde_json::error::Category::Io => "io",
        serde_json::error::Category::Syntax => "syntax",
        serde_json::error::Category::Data => "data",
        serde_json::error::Category::Eof => "eof",
    };
    permanent(
        None,
        format!(
            "malformed data ({category} error at line {} column {})",
            err.line(),
            err.column()
        ),
    )
}

/// A WorkOS id as a Mandate id: lowercase, other characters as `_`, at most 64 long.
pub(crate) fn slug(s: &str) -> String {
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

/// The AuthKit user to link at sync. A lookup error means "no link" (first sign-in links
/// instead), so a WorkOS outage cannot hold back removals in every tenant.
async fn link(
    users: &dyn UserLookup,
    email: Option<&str>,
    org: &str,
    e: &EventSchema,
) -> Option<String> {
    match users.user_by_email(email?, org).await {
        Ok(user) => user,
        Err(error) => {
            tracing::warn!(id = %e.id, %error, "authkit user lookup failed; member left unlinked");
            None
        }
    }
}

/// `None` for an event type Mandate does not act on; otherwise the org and its change.
async fn prepare(
    store: &Store,
    users: &dyn UserLookup,
    e: &EventSchema,
) -> Result<Option<(TenantId, Change)>, Fail> {
    let data = serde_json::Value::Object(e.data.clone().into_iter().collect());
    let shape = |err: serde_json::Error| malformed(&err);
    let member_id =
        |id: &str, org: &str| MemberId::parse(&slug(id)).map_err(|err| permanent(Some(org), err));
    let team_id =
        |id: &str, org: &str| TeamId::parse(&slug(id)).map_err(|err| permanent(Some(org), err));
    let (org, change) = match e.event.as_str() {
        kind @ ("dsync.user.created" | "dsync.user.updated" | "dsync.user.deleted") => {
            let u: DirUser = serde_json::from_value(data).map_err(shape)?;
            let id = member_id(&u.id, &u.organization_id)?;
            // Suspended or inactive in the directory counts as gone.
            let change = if kind == "dsync.user.deleted" || u.state != "active" {
                Change::RemoveMember(id)
            } else {
                let workos_user = link(users, u.email.as_deref(), &u.organization_id, e).await;
                Change::Member {
                    id,
                    directory_user: u.id,
                    email: u.email,
                    workos_user,
                }
            };
            (u.organization_id, change)
        }
        kind @ ("dsync.group.created" | "dsync.group.updated" | "dsync.group.deleted") => {
            let g: DirGroup = serde_json::from_value(data).map_err(shape)?;
            let id = team_id(&g.id, &g.organization_id)?;
            let change = if kind == "dsync.group.deleted" {
                Change::RemoveTeam(id)
            } else {
                Change::Team { id, group: g.id }
            };
            (g.organization_id, change)
        }
        kind @ ("dsync.group.user_added" | "dsync.group.user_removed") => {
            let GroupUser { user, group } = serde_json::from_value(data).map_err(shape)?;
            let org = group.organization_id;
            if user.organization_id != org {
                return Err(permanent(
                    Some(&org),
                    "user and group are in different orgs",
                ));
            }
            let member = member_id(&user.id, &org)?;
            let team = team_id(&group.id, &org)?;
            let change = if kind == "dsync.group.user_removed" {
                Change::Leave { team, member }
            } else if user.state == "active" {
                Change::Join {
                    team,
                    group: group.id,
                    member,
                    workos_user: link(users, user.email.as_deref(), &org, e).await,
                    directory_user: user.id,
                    email: user.email,
                }
            } else {
                Change::Nothing
            };
            (org, change)
        }
        _ => return Ok(None),
    };
    let tenant = match store.tenant_for_org(&org).await? {
        Some(t) => t,
        None => {
            let id = TenantId::parse(&slug(&org)).map_err(|err| permanent(Some(&org), err))?;
            store
                .ensure_tenant(&org, &id)
                .await?
                .ok_or_else(|| permanent(Some(&org), format!("tenant id {id} is taken")))?
        }
    };
    Ok(Some((tenant, change)))
}

/// One event, one transaction: the change and the cursor commit together or not at all.
/// False if another runner already passed this event.
async fn apply(
    store: &Store,
    e: &EventSchema,
    tenant: &TenantId,
    change: Change,
) -> Result<bool, StoreError> {
    let mut tx = store.begin(tenant).await?;
    if store::cursor_passed(&mut tx, &e.id).await? {
        return Ok(false);
    }
    match change {
        Change::Member {
            id,
            directory_user,
            email,
            workos_user,
        } => {
            store::upsert_member(
                &mut tx,
                &id,
                &directory_user,
                email.as_deref(),
                workos_user.as_deref(),
            )
            .await?
        }
        Change::RemoveMember(id) => store::remove_member(&mut tx, &id).await?,
        Change::Team { id, group } => store::upsert_team(&mut tx, &id, &group).await?,
        Change::RemoveTeam(id) => store::delete_team(&mut tx, &id).await?,
        Change::Join {
            team,
            group,
            member,
            directory_user,
            email,
            workos_user,
        } => {
            store::upsert_member(
                &mut tx,
                &member,
                &directory_user,
                email.as_deref(),
                workos_user.as_deref(),
            )
            .await?;
            store::upsert_team(&mut tx, &team, &group).await?;
            store::set_team_member(&mut tx, &team, &member, true).await?;
        }
        Change::Leave { team, member } => {
            store::set_team_member(&mut tx, &team, &member, false).await?
        }
        Change::Nothing => {}
    }
    store::set_directory_cursor(&mut tx, &e.id).await?;
    tx.commit().await?;
    tracing::info!(event = %e.event, id = %e.id, tenant = %tenant, "directory event applied");
    Ok(true)
}

/// Records an event that can never apply and moves the cursor past it, so one bad event
/// cannot hold back removals in every tenant.
async fn dead_letter(
    store: &Store,
    e: &EventSchema,
    org: Option<&str>,
    error: &str,
) -> Result<(), StoreError> {
    let mut tx = store.begin_untenanted().await?;
    if store::cursor_passed(&mut tx, &e.id).await? {
        return Ok(());
    }
    store::dead_letter(&mut tx, &e.id, &e.event, org, error).await?;
    store::set_directory_cursor(&mut tx, &e.id).await?;
    tx.commit().await?;
    tracing::error!(id = %e.id, event = %e.event, %error, "directory event dead-lettered");
    Ok(())
}

/// Pulls directory events after the stored cursor and applies each idempotently; returns how
/// many were applied. Returns 0 at once if another runner holds the sync lock.
pub async fn sync_directory(
    store: &Store,
    source: &impl EventSource,
    users: &impl UserLookup,
) -> Result<u64, IdentityError> {
    // Held (as an open session) until this function returns.
    let Some(_lock) = store.sync_lock().await? else {
        return Ok(0);
    };
    let mut events = source.events_after(store.directory_cursor().await?);
    let mut applied = 0;
    while let Some(e) = events.next().await {
        let e = e?;
        match prepare(store, users, &e).await {
            // An unknown type is skipped; the next applied event's cursor moves past it.
            Ok(None) => {}
            Ok(Some((tenant, change))) => {
                if apply(store, &e, &tenant, change).await? {
                    applied += 1;
                }
            }
            Err(Fail::Permanent { org, error }) => {
                dead_letter(store, &e, org.as_deref(), &error).await?
            }
            Err(Fail::Transient(err)) => return Err(err),
        }
    }
    Ok(applied)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upstream_errors_never_carry_request_text() {
        use workos::transport::{TransportError, TransportErrorKind};
        let leak = "error sending request for url (https://api.workos.com/user_management/users?email=secret%40x.com)";
        let network =
            workos::Error::Network(TransportError::new(TransportErrorKind::Connect, leak));
        assert!(
            network.to_string().contains("secret"),
            "the raw text does leak"
        );
        let IdentityError::Upstream(s) = upstream(network) else {
            unreachable!()
        };
        assert!(!s.contains("secret"));
        assert_eq!(s, "network (Connect)");
        let api = workos::Error::Api(Box::new(workos::ApiError {
            status: 422,
            code: None,
            message: "secret@x.com is invalid".into(),
            request_id: None,
            retry_after: None,
            raw_body: Default::default(),
            headers: Default::default(),
        }));
        assert_eq!(
            upstream(api).to_string(),
            "workos request failed: status 422"
        );
    }

    #[test]
    fn slugs_are_valid_ids() {
        assert_eq!(slug("org_01HXYZ"), "org_01hxyz");
        assert_eq!(slug("a.b c/é"), "a_b_c__");
        assert_eq!(slug(&"X".repeat(80)).len(), 64);
        assert!(TenantId::parse(&slug("directory_user_01E1JG7J09H96KYP8HM9B0G5SJ")).is_ok());
    }

    #[test]
    fn malformed_data_never_quotes_values() {
        let data =
            serde_json::json!({"user": "ann@x.com", "group": {"id": "g", "organization_id": "o"}});
        let err = serde_json::from_value::<GroupUser>(data).err().unwrap();
        assert!(
            err.to_string().contains("ann@x.com"),
            "serde's text does leak"
        );
        let Fail::Permanent { error, .. } = malformed(&err) else {
            unreachable!()
        };
        assert!(!error.contains("ann@x.com"), "{error}");
        assert_eq!(error, "malformed data (data error at line 0 column 0)");
    }
}
