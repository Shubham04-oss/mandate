//! The activity feed: what agents did, from the audit log. Members see their own agent and their
//! teams' bots; org admins see everything.
use crate::api::agents::{AGENT_COLS, AGENT_JOINS, agent_view};
use crate::api::org::{PAGE, bounded, is_admin, person, ts};
use crate::api::session::Session;
use crate::api::{ApiError, ApiState};
use crate::store::StoreError;
use axum::Json;
use axum::extract::{Query, State};
use axum::routing::get;
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::Row;
use sqlx::postgres::PgRow;
use std::sync::Arc;
use time::OffsetDateTime;
use uuid::Uuid;

pub fn routes() -> axum::Router<Arc<ApiState>> {
    axum::Router::new().route("/v1/activity", get(list))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ListQuery {
    #[serde(default)]
    cursor: Option<String>,
}

fn item(r: &PgRow) -> Result<Value, sqlx::Error> {
    let agent: String = r.try_get("agent")?;
    let member: String = r.try_get("member")?;
    let approved_by: Option<String> = r.try_get("approved_by")?;
    let approval: Option<Uuid> = r.try_get("approval_id")?;
    Ok(json!({
        "id": r.try_get::<i64, _>("id")?.to_string(),
        "at": ts(r.try_get::<OffsetDateTime, _>("at")?),
        "agent": agent_view(r, &agent)?,
        "member": person(&member, r.try_get("m_name")?, r.try_get("m_email")?),
        "tool": r.try_get::<String, _>("tool")?,
        "decision": r.try_get::<String, _>("decision")?,
        "reason": r.try_get::<Option<String>, _>("reason")?,
        "approval_id": approval.map(|a| a.to_string()),
        "approved_by": match approved_by {
            Some(a) => person(&a, r.try_get("ab_name")?, r.try_get("ab_email")?),
            None => Value::Null,
        },
        "outcome": r.try_get::<Option<String>, _>("outcome")?,
    }))
}

/// `GET /v1/activity`: newest first; the cursor is the last audit id shown. An ask carries how its
/// approval ended (`outcome`); an approval approved but never used still reads as approved.
async fn list(
    State(s): State<Arc<ApiState>>,
    session: Session,
    q: Result<Query<ListQuery>, axum::extract::rejection::QueryRejection>,
) -> Result<Json<Value>, ApiError> {
    let Query(q) = q.map_err(|_| ApiError::invalid_input())?;
    let cursor = bounded(q.cursor, 20)?
        .map(|c| c.parse::<u64>().ok().and_then(|c| i64::try_from(c).ok()))
        .map(|c| c.ok_or(ApiError::invalid_input()))
        .transpose()?;
    let mut tx = s.store.begin(&session.tenant).await?;
    let admin = is_admin(&mut tx, &session).await?;
    // Decisions and grant changes are audited too; the feed shows only what agents did.
    let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
        "select u.id, u.at, u.agent, u.member, u.tool, u.decision, u.reason, u.approval_id, u.approved_by, \
         m.name as m_name, m.email as m_email, ab.name as ab_name, ab.email as ab_email, {AGENT_COLS}, \
         case when u.decision = 'ask' and a.id is not null then case \
           when a.denied_by is not null then 'denied' when a.approved_by is not null then 'approved' \
           when a.expires_at <= now() then 'expired' else 'pending' end end as outcome \
         from audit u \
         left join agents g on g.tenant_id = u.tenant_id and g.id = u.agent {AGENT_JOINS} \
         left join members m on m.tenant_id = u.tenant_id and m.id = u.member \
         left join members ab on ab.tenant_id = u.tenant_id and ab.id = u.approved_by \
         left join approvals a on a.tenant_id = u.tenant_id and a.id = u.approval_id \
         where u.tenant_id = current_setting('mandate.tenant', true) \
         and u.decision in ('run', 'ask', 'refuse') \
         and ($3 or g.owner_member = $1 or exists (select 1 from team_members tm \
           where tm.tenant_id = u.tenant_id and tm.team_id = g.owner_team and tm.member_id = $1)) \
         and ($2::bigint is null or u.id < $2) \
         order by u.id desc limit {}",
        PAGE + 1
    )))
    .bind(session.member.as_str())
    .bind(cursor)
    .bind(admin)
    .fetch_all(&mut *tx)
    .await
    .map_err(StoreError::from)?;
    tx.commit().await.map_err(StoreError::from)?;
    let more = rows.len() as i64 > PAGE;
    let rows = &rows[..rows.len().min(PAGE as usize)];
    let next = match rows.last() {
        Some(r) if more => Value::String(
            r.try_get::<i64, _>("id")
                .map_err(StoreError::from)?
                .to_string(),
        ),
        _ => Value::Null,
    };
    let items = rows
        .iter()
        .map(item)
        .collect::<Result<Vec<_>, _>>()
        .map_err(StoreError::from)?;
    Ok(Json(json!({ "items": items, "next": next })))
}
