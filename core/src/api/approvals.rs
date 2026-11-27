//! The approvals inbox. A decision is one conditional UPDATE, so of two people deciding at once
//! exactly one wins and the other gets 409.
use crate::api::agents::{AGENT_COLS, AGENT_JOINS, agent_view};
use crate::api::org::{PAGE, bounded, person, ts};
use crate::api::session::Session;
use crate::api::{ApiError, ApiState};
use crate::events::{self, Event};
use crate::ids::{AgentId, MemberId, ToolId};
use crate::store::{self, AuditEntry, StoreError, Tx};
use axum::Json;
use axum::extract::{Path, Query, State as Ax};
use axum::routing::{get, post};
use serde::Deserialize;
use serde_json::Value;
use sqlx::Row;
use std::sync::Arc;
use time::OffsetDateTime;
use uuid::Uuid;

pub use crate::events::Decision;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Pending,
    Approved,
    Denied,
    Used,
    Expired,
}

impl State {
    pub fn as_str(self) -> &'static str {
        match self {
            State::Pending => "pending",
            State::Approved => "approved",
            State::Denied => "denied",
            State::Used => "used",
            State::Expired => "expired",
        }
    }
}

/// An approval as stored, with its state at the time it was read.
#[derive(Debug, Clone)]
pub struct Approval {
    pub id: Uuid,
    pub agent: AgentId,
    pub tool: ToolId,
    pub args: Value,
    pub requested_by: MemberId,
    pub requested_at: OffsetDateTime,
    pub expires_at: OffsetDateTime,
    pub state: State,
    /// The decision recorded, whatever the state now (an approved one may since be used or expired).
    pub decision: Option<Decision>,
    pub decided_by: Option<MemberId>,
    pub decided_at: Option<OffsetDateTime>,
    /// Set by the agent lane when the call came from a conversation.
    pub conversation: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum DecideError {
    #[error("no such approval")]
    NotFound,
    #[error("this member may not decide this approval")]
    Forbidden,
    #[error("the agent is disabled")]
    Disabled,
    #[error("already decided")]
    AlreadyDecided,
    #[error("expired")]
    Expired,
    #[error(transparent)]
    Store(#[from] StoreError),
}

/// Who would decide if agent `g` were enabled: its owner for a personal agent, or a member of the
/// owning team other than the requester. `a` is the approvals row alias, `$2` the member.
macro_rules! decider {
    () => {
        "(g.owner_member = $2 or (a.requested_by <> $2 and exists (select 1 from team_members tm \
         where tm.tenant_id = a.tenant_id and tm.team_id = g.owner_team and tm.member_id = $2)))"
    };
}
/// Who may decide: a decider of the approval's agent; never for a disabled agent.
const MAY_DECIDE: &str = concat!(
    "exists (select 1 from agents g where g.tenant_id = a.tenant_id and g.id = a.agent \
     and not g.disabled and ",
    decider!(),
    ")"
);
pub(crate) const PENDING: &str =
    "a.approved_by is null and a.denied_by is null and a.used_at is null";

/// A member who may decide an approval, and where to tell them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decider {
    pub member: MemberId,
    pub email: String,
}

/// The `decider!` rule as a set: the same text with each member in place of its `$2`, so who
/// is emailed can never drift from who may decide. `$2` then binds `now`.
static DECIDERS: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
    format!(
        "select m.id, m.email from approvals a \
         join agents g on g.tenant_id = a.tenant_id and g.id = a.agent and not g.disabled \
         join members m on m.tenant_id = a.tenant_id and {} \
         where a.tenant_id = current_setting('mandate.tenant', true) and a.id = $1 \
         and {PENDING} and a.expires_at > $2 and m.email is not null and m.email <> '' \
         order by m.id",
        decider!().replace("$2", "m.id")
    )
});

/// Everyone who may decide this approval now and has an address; empty once it is decided,
/// expired or unknown, or while its agent is disabled.
pub async fn deciders(
    tx: &mut Tx,
    approval: Uuid,
    now: OffsetDateTime,
) -> Result<Vec<Decider>, StoreError> {
    let rows: Vec<(String, String)> = sqlx::query_as(sqlx::AssertSqlSafe(DECIDERS.as_str()))
        .bind(approval)
        .bind(now)
        .fetch_all(&mut **tx)
        .await?;
    rows.into_iter()
        .map(|(m, email)| {
            Ok(Decider {
                member: MemberId::parse(&m)?,
                email,
            })
        })
        .collect()
}

/// Records `by`'s decision if the approval is still pending and `by` may decide it, and
/// publishes `approval.decided.v1` in the same transaction. The UPDATE is the only arbiter: a
/// concurrent decision waits on the row lock, then finds it decided.
pub async fn decide_approval(
    tx: &mut Tx,
    approval: Uuid,
    by: &MemberId,
    decision: Decision,
    now: OffsetDateTime,
) -> Result<Approval, DecideError> {
    let (by_col, at_col, verb) = match decision {
        Decision::Approved => ("approved_by", "approved_at", "approved"),
        Decision::Denied => ("denied_by", "denied_at", "denied"),
    };
    let won: Option<[u8; 32]> = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "update approvals a set {by_col} = $2, {at_col} = $3 \
         where a.tenant_id = current_setting('mandate.tenant', true) and a.id = $1 \
         and {PENDING} and a.expires_at > $3 and {MAY_DECIDE} returning a.args_hash"
    )))
    .bind(approval)
    .bind(by.as_str())
    .bind(now)
    .fetch_optional(&mut **tx)
    .await
    .map_err(StoreError::from)?;
    let a = load(tx, approval, now)
        .await?
        .ok_or(DecideError::NotFound)?;
    let Some(args_hash) = won else {
        return Err(why_not(tx, approval, by, a.state).await?);
    };
    let audit = AuditEntry {
        agent: a.agent.clone(),
        member: by.clone(),
        tool: a.tool.clone(),
        args_hash,
        decision: verb.to_owned(),
        reason: None,
        approval_id: Some(approval),
        approved_by: (decision == Decision::Approved).then(|| by.clone()),
    };
    store::audit(tx, &audit).await?;
    events::publish(
        tx,
        Event::ApprovalDecided {
            approval,
            decision,
            by: by.clone(),
        },
    )
    .await?;
    Ok(a)
}

/// Why `by` could not decide: hidden from them (404), their agent disabled (409), not theirs to
/// decide (403), or no longer pending (409).
async fn why_not(
    tx: &mut Tx,
    approval: Uuid,
    by: &MemberId,
    state: State,
) -> Result<DecideError, StoreError> {
    // A missing agent row counts as disabled.
    let (may, visible, disabled): (bool, bool, bool) = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "select {MAY_DECIDE}, a.requested_by = $2 or exists (select 1 from agents g \
         where g.tenant_id = a.tenant_id and g.id = a.agent and {}), \
         coalesce((select g.disabled from agents g where g.tenant_id = a.tenant_id and g.id = a.agent), true) \
         from approvals a where a.tenant_id = current_setting('mandate.tenant', true) and a.id = $1",
        decider!()
    )))
    .bind(approval)
    .bind(by.as_str())
    .fetch_one(&mut **tx)
    .await?;
    Ok(match (may, visible, disabled, state) {
        (true, _, _, State::Expired) => DecideError::Expired,
        (true, ..) => DecideError::AlreadyDecided,
        (false, false, ..) => DecideError::NotFound,
        (false, true, true, _) => DecideError::Disabled,
        (false, true, false, _) => DecideError::Forbidden,
    })
}

fn state_of(r: &sqlx::postgres::PgRow, now: OffsetDateTime) -> Result<State, sqlx::Error> {
    let set =
        |c: &str| -> Result<bool, sqlx::Error> { Ok(r.try_get::<Option<String>, _>(c)?.is_some()) };
    Ok(
        if r.try_get::<Option<OffsetDateTime>, _>("used_at")?.is_some() {
            State::Used
        } else if set("denied_by")? {
            State::Denied
        } else if r.try_get::<OffsetDateTime, _>("expires_at")? <= now {
            State::Expired
        } else if set("approved_by")? {
            State::Approved
        } else {
            State::Pending
        },
    )
}

/// `None` if there is no such approval in this tenant.
pub async fn load(
    tx: &mut Tx,
    approval: Uuid,
    now: OffsetDateTime,
) -> Result<Option<Approval>, StoreError> {
    let Some(r) = sqlx::query(
        "select a.id, a.agent, a.tool, a.args, a.requested_by, a.requested_at, a.expires_at, \
         a.approved_by, a.approved_at, a.denied_by, a.denied_at, a.used_at, a.conversation \
         from approvals a where a.tenant_id = current_setting('mandate.tenant', true) and a.id = $1",
    )
    .bind(approval)
    .fetch_optional(&mut **tx)
    .await?
    else {
        return Ok(None);
    };
    let approved: Option<String> = r.try_get("approved_by")?;
    let denied: Option<String> = r.try_get("denied_by")?;
    let decision = match (&approved, &denied) {
        // Denied first, as `state_of` reads it.
        (_, Some(_)) => Some(Decision::Denied),
        (Some(_), None) => Some(Decision::Approved),
        (None, None) => None,
    };
    let by = approved.or(denied);
    let at: Option<OffsetDateTime> = r
        .try_get::<Option<OffsetDateTime>, _>("approved_at")?
        .or(r.try_get("denied_at")?);
    Ok(Some(Approval {
        id: r.try_get("id")?,
        agent: AgentId::parse(&r.try_get::<String, _>("agent")?)?,
        tool: ToolId::parse(&r.try_get::<String, _>("tool")?)?,
        args: r.try_get::<sqlx::types::Json<Value>, _>("args")?.0,
        requested_by: MemberId::parse(&r.try_get::<String, _>("requested_by")?)?,
        requested_at: r.try_get("requested_at")?,
        expires_at: r.try_get("expires_at")?,
        state: state_of(&r, now)?,
        decision,
        decided_by: by.map(|m| MemberId::parse(&m)).transpose()?,
        decided_at: at,
        conversation: r.try_get("conversation")?,
    }))
}

pub fn routes() -> axum::Router<Arc<ApiState>> {
    axum::Router::new()
        .route("/v1/approvals", get(list))
        .route("/v1/approvals/{id}/approve", post(approve))
        .route("/v1/approvals/{id}/deny", post(deny))
}

/// Everything the `Approval` shape needs, for the approvals `a` the caller may see.
fn view_sql(filter: &str) -> String {
    format!(
        "select a.id, a.agent, a.tool, a.args, a.requested_by, a.requested_at, a.expires_at, \
         a.approved_by, a.denied_by, a.used_at, coalesce(a.approved_at, a.denied_at) as decided_at, \
         a.conversation, \
         rm.name as rm_name, rm.email as rm_email, dm.name as dm_name, dm.email as dm_email, \
         {MAY_DECIDE} as may_decide, {AGENT_COLS} \
         from approvals a \
         left join agents g on g.tenant_id = a.tenant_id and g.id = a.agent {AGENT_JOINS} \
         left join members rm on rm.tenant_id = a.tenant_id and rm.id = a.requested_by \
         left join members dm on dm.tenant_id = a.tenant_id and dm.id = coalesce(a.approved_by, a.denied_by) \
         where a.tenant_id = current_setting('mandate.tenant', true) \
         and (a.requested_by = $2 or {MAY_DECIDE}) and {filter}"
    )
}

fn view(s: &ApiState, r: &sqlx::postgres::PgRow, now: OffsetDateTime) -> Result<Value, ApiError> {
    let row = |e: sqlx::Error| ApiError::from(StoreError::from(e));
    let state = state_of(r, now).map_err(row)?;
    let agent: String = r.try_get("agent").map_err(row)?;
    let tool: String = r.try_get("tool").map_err(row)?;
    let args = r
        .try_get::<sqlx::types::Json<Value>, _>("args")
        .map_err(row)?
        .0;
    // The effect of these exact args, as the gate judged them; `null` for a tool no longer served.
    let effect = ToolId::parse(&tool)
        .ok()
        .and_then(|t| s.catalog.get(&t))
        .map(|t| t.effect(&args));
    let requested_by: String = r.try_get("requested_by").map_err(row)?;
    let decided_by: Option<String> = r
        .try_get::<Option<String>, _>("approved_by")
        .map_err(row)?
        .or(r.try_get("denied_by").map_err(row)?);
    let decided_at: Option<OffsetDateTime> = r.try_get("decided_at").map_err(row)?;
    let id: Uuid = r.try_get("id").map_err(row)?;
    Ok(serde_json::json!({
        "id": id.to_string(),
        "agent": agent_view(r, &agent).map_err(row)?,
        "tool": tool,
        "args": args,
        "effect": effect,
        "requested_by": person(&requested_by, r.try_get("rm_name").map_err(row)?, r.try_get("rm_email").map_err(row)?),
        "requested_at": ts(r.try_get("requested_at").map_err(row)?),
        "expires_at": ts(r.try_get("expires_at").map_err(row)?),
        "state": state.as_str(),
        "decided_by": decided_by.map(|d| -> Result<Value, ApiError> {
            Ok(person(&d, r.try_get("dm_name").map_err(row)?, r.try_get("dm_email").map_err(row)?))
        }).transpose()?,
        "decided_at": decided_at.map(ts),
        "can_decide": state == State::Pending && r.try_get::<bool, _>("may_decide").map_err(row)?,
        // The conversation that asked, when one did: the link is within this tenant's row.
        "conversation": r.try_get::<Option<String>, _>("conversation").map_err(row)?,
    }))
}

pub(crate) async fn view_one(
    tx: &mut Tx,
    s: &ApiState,
    session: &Session,
    approval: Uuid,
    now: OffsetDateTime,
) -> Result<Option<Value>, ApiError> {
    let r = sqlx::query(sqlx::AssertSqlSafe(view_sql("a.id = $1")))
        .bind(approval)
        .bind(session.member.as_str())
        .fetch_optional(&mut **tx)
        .await
        .map_err(StoreError::from)?;
    r.map(|r| view(s, &r, now)).transpose()
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ListQuery {
    #[serde(default)]
    state: Option<String>,
    #[serde(default)]
    cursor: Option<String>,
}

/// `<requested_at as unix nanoseconds>.<id>`: the position of the last item shown.
fn parse_cursor(c: &str) -> Result<(OffsetDateTime, Uuid), ApiError> {
    let bad = || ApiError::invalid_input();
    let (t, id) = c.split_once('.').ok_or_else(bad)?;
    let t = t.parse::<i128>().map_err(|_| bad())?;
    Ok((
        OffsetDateTime::from_unix_timestamp_nanos(t).map_err(|_| bad())?,
        Uuid::parse_str(id).map_err(|_| bad())?,
    ))
}

/// `GET /v1/approvals`: what the caller may decide plus the caller's own requests, newest first.
async fn list(
    Ax(s): Ax<Arc<ApiState>>,
    session: Session,
    q: Result<Query<ListQuery>, axum::extract::rejection::QueryRejection>,
) -> Result<Json<Value>, ApiError> {
    let Query(q) = q.map_err(|_| ApiError::invalid_input())?;
    let now = OffsetDateTime::now_utc();
    let state = match q.state.as_deref() {
        None | Some("pending") => format!("({PENDING} and a.expires_at > $1)"),
        Some("decided") => format!("not ({PENDING} and a.expires_at > $1)"),
        Some(_) => return Err(ApiError::invalid_input()),
    };
    let cursor = bounded(q.cursor, 128)?
        .map(|c| parse_cursor(&c))
        .transpose()?;
    let filter = format!(
        "{state} and ($3::timestamptz is null or (a.requested_at, a.id) < ($3, $4)) \
         order by a.requested_at desc, a.id desc limit {}",
        PAGE + 1
    );
    let mut tx = s.store.begin(&session.tenant).await?;
    let rows = sqlx::query(sqlx::AssertSqlSafe(view_sql(&filter)))
        .bind(now)
        .bind(session.member.as_str())
        .bind(cursor.map(|c| c.0))
        .bind(cursor.map(|c| c.1))
        .fetch_all(&mut *tx)
        .await
        .map_err(StoreError::from)?;
    tx.commit().await.map_err(StoreError::from)?;
    let more = rows.len() as i64 > PAGE;
    let rows = &rows[..rows.len().min(PAGE as usize)];
    let next = match rows.last() {
        Some(r) if more => {
            let at: OffsetDateTime = r.try_get("requested_at").map_err(StoreError::from)?;
            let id: Uuid = r.try_get("id").map_err(StoreError::from)?;
            Value::String(format!("{}.{id}", at.unix_timestamp_nanos()))
        }
        _ => Value::Null,
    };
    let items = rows
        .iter()
        .map(|r| view(&s, r, now))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Json(serde_json::json!({ "items": items, "next": next })))
}

async fn approve(
    Ax(s): Ax<Arc<ApiState>>,
    session: Session,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    decide(&s, &session, &id, Decision::Approved).await
}

async fn deny(
    Ax(s): Ax<Arc<ApiState>>,
    session: Session,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    decide(&s, &session, &id, Decision::Denied).await
}

async fn decide(
    s: &ApiState,
    session: &Session,
    id: &str,
    d: Decision,
) -> Result<Json<Value>, ApiError> {
    let id = Uuid::parse_str(id).map_err(|_| ApiError::not_found())?;
    let now = OffsetDateTime::now_utc();
    let mut tx = s.store.begin(&session.tenant).await?;
    match decide_approval(&mut tx, id, &session.member, d, now).await {
        Ok(_) => {}
        Err(DecideError::NotFound) => return Err(ApiError::not_found()),
        Err(DecideError::Forbidden) => return Err(ApiError::forbidden()),
        Err(DecideError::Disabled) => return Err(ApiError::conflict("This agent is disabled.")),
        Err(DecideError::AlreadyDecided) => {
            return Err(ApiError::conflict("This request was already decided."));
        }
        Err(DecideError::Expired) => return Err(ApiError::conflict("This request has expired.")),
        Err(DecideError::Store(e)) => return Err(e.into()),
    }
    let v = view_one(&mut tx, s, session, id, now)
        .await?
        .ok_or(ApiError::not_found())?;
    tx.commit().await.map_err(StoreError::from)?;
    Ok(Json(v))
}
