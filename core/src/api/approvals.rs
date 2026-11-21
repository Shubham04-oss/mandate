//! The approvals inbox. A decision is one conditional UPDATE, so of two people deciding at once
//! exactly one wins and the other gets 409.
use crate::api::stubs::{self, Event};
use crate::ids::{AgentId, MemberId, ToolId};
use crate::store::{StoreError, Tx};
use serde_json::Value;
use sqlx::Row;
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Approve,
    Deny,
}

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
    #[error("already decided")]
    AlreadyDecided,
    #[error("expired")]
    Expired,
    #[error(transparent)]
    Store(#[from] StoreError),
}

/// Who may decide: the owner of a personal agent, or a member of the team that owns a bot; never
/// for a disabled agent. `a` is the approvals row alias.
const MAY_DECIDE: &str = "exists (select 1 from agents g where g.tenant_id = a.tenant_id and g.id = a.agent \
     and not g.disabled and (g.owner_member = $2 or exists (select 1 from team_members tm \
     where tm.tenant_id = a.tenant_id and tm.team_id = g.owner_team and tm.member_id = $2)))";
const PENDING: &str = "a.approved_by is null and a.denied_by is null and a.used_at is null";

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
    let (by_col, at_col) = match decision {
        Decision::Approve => ("approved_by", "approved_at"),
        Decision::Deny => ("denied_by", "denied_at"),
    };
    let won: Option<Uuid> = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "update approvals a set {by_col} = $2, {at_col} = $3 \
         where a.tenant_id = current_setting('mandate.tenant', true) and a.id = $1 \
         and {PENDING} and a.expires_at > $3 and {MAY_DECIDE} returning a.id"
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
    if won.is_none() {
        if !may_decide(tx, approval, by).await? {
            return Err(DecideError::Forbidden);
        }
        return Err(match a.state {
            State::Expired => DecideError::Expired,
            _ => DecideError::AlreadyDecided,
        });
    }
    stubs::publish(
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

async fn may_decide(tx: &mut Tx, approval: Uuid, by: &MemberId) -> Result<bool, StoreError> {
    Ok(sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "select exists (select 1 from approvals a where a.tenant_id = current_setting('mandate.tenant', true) \
         and a.id = $1 and {MAY_DECIDE})"
    )))
    .bind(approval)
    .bind(by.as_str())
    .fetch_one(&mut **tx)
    .await?)
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
    let by: Option<String> = r
        .try_get::<Option<String>, _>("approved_by")?
        .or(r.try_get("denied_by")?);
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
        decided_by: by.map(|m| MemberId::parse(&m)).transpose()?,
        decided_at: at,
        conversation: r.try_get("conversation")?,
    }))
}
