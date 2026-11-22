//! Grants, managed by org admins: who may use which tool, until when. The gate reads them on
//! every call, so a revoked or expired grant stops working at once.
use crate::api::org::{is_admin, person, ts};
use crate::api::session::Session;
use crate::api::{ApiError, ApiState};
use crate::events::{self, Event};
use crate::ids::{AgentId, MemberId, TeamId, ToolId};
use crate::store::{self, AuditEntry, StoreError, Tx};
use axum::Json;
use axum::extract::rejection::JsonRejection;
use axum::extract::{Path, State};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get};
use http::StatusCode;
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::Row;
use sqlx::postgres::PgRow;
use std::sync::Arc;
use time::format_description::well_known::Rfc3339;
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

const MAX_REASON_CHARS: usize = 500;
/// The longest a grant may run; a standing need is granted again, not forever.
const MAX_GRANT: Duration = Duration::days(90);

pub fn routes() -> axum::Router<Arc<ApiState>> {
    axum::Router::new()
        .route("/v1/grants", get(list).post(create))
        .route("/v1/grants/{id}", delete(revoke))
}

#[derive(Deserialize, Clone, Copy)]
#[serde(rename_all = "lowercase")]
enum Kind {
    Agent,
    Member,
    Team,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Principal {
    kind: Kind,
    id: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NewGrant {
    principal: Principal,
    tool: ToolId,
    #[serde(default)]
    standing: bool,
    expires_at: String,
    #[serde(default)]
    reason: Option<String>,
}

const GRANT_SQL: &str = "select r.id, r.principal_kind, r.principal_id, r.tool, r.standing, r.expires_at, \
     r.granted_by, r.reason, m.name as m_name, m.email as m_email from grants r \
     left join members m on m.tenant_id = r.tenant_id and m.id = r.granted_by \
     where r.tenant_id = current_setting('mandate.tenant', true)";

fn view(r: &PgRow, now: OffsetDateTime) -> Result<Value, sqlx::Error> {
    let expires: OffsetDateTime = r.try_get("expires_at")?;
    let by: String = r.try_get("granted_by")?;
    Ok(json!({
        "id": r.try_get::<Uuid, _>("id")?.to_string(),
        "principal": { "kind": r.try_get::<String, _>("principal_kind")?, "id": r.try_get::<String, _>("principal_id")? },
        "tool": r.try_get::<String, _>("tool")?,
        "standing": r.try_get::<bool, _>("standing")?,
        "expires_at": ts(expires),
        "state": if expires > now { "active" } else { "expired" },
        "granted_by": person(&by, r.try_get("m_name")?, r.try_get("m_email")?),
        "reason": r.try_get::<Option<String>, _>("reason")?,
    }))
}

fn kind_str(k: Kind) -> &'static str {
    match k {
        Kind::Agent => "agent",
        Kind::Member => "member",
        Kind::Team => "team",
    }
}

/// An audit row for a grant change, written in the change's transaction. The audit table is
/// keyed for tool calls: the principal's id goes in `agent`, its kind in the fixed reason.
async fn audit(
    tx: &mut Tx,
    by: &MemberId,
    kind: &str,
    principal: &str,
    tool: &ToolId,
    decision: &str,
) -> Result<(), StoreError> {
    let reason = match kind {
        "agent" => "principal is an agent",
        "member" => "principal is a member",
        _ => "principal is a team",
    };
    store::audit(
        tx,
        &AuditEntry {
            agent: AgentId::parse(principal)?,
            member: by.clone(),
            tool: tool.clone(),
            // A grant has no call arguments.
            args_hash: [0; 32],
            decision: decision.to_owned(),
            reason: Some(reason.to_owned()),
            approval_id: None,
            approved_by: None,
        },
    )
    .await
}

/// Every route here is for org admins only, checked in the request's own transaction.
async fn admin_tx(s: &ApiState, session: &Session) -> Result<Tx, ApiError> {
    let mut tx = s.store.begin(&session.tenant).await?;
    if !is_admin(&mut tx, session).await? {
        return Err(ApiError::forbidden());
    }
    Ok(tx)
}

/// `GET /v1/grants`: every grant in the org, expired ones marked so.
async fn list(State(s): State<Arc<ApiState>>, session: Session) -> Result<Json<Value>, ApiError> {
    let mut tx = admin_tx(&s, &session).await?;
    // ponytail: unpaged, capped at 1000; page it when an org has that many grants.
    let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
        "{GRANT_SQL} order by r.principal_kind, r.principal_id, r.tool limit 1000"
    )))
    .fetch_all(&mut *tx)
    .await
    .map_err(StoreError::from)?;
    tx.commit().await.map_err(StoreError::from)?;
    let now = OffsetDateTime::now_utc();
    let items = rows
        .iter()
        .map(|r| view(r, now))
        .collect::<Result<Vec<_>, _>>()
        .map_err(StoreError::from)?;
    Ok(Json(json!({ "items": items })))
}

/// True if the principal is in this tenant (and, for an agent, enabled).
async fn principal_exists(tx: &mut Tx, kind: Kind, id: &str) -> Result<bool, StoreError> {
    let q = match kind {
        Kind::Agent => {
            "select exists (select 1 from agents where tenant_id = current_setting('mandate.tenant', true) and id = $1 and not disabled)"
        }
        Kind::Member => {
            "select exists (select 1 from members where tenant_id = current_setting('mandate.tenant', true) and id = $1)"
        }
        Kind::Team => {
            "select exists (select 1 from teams where tenant_id = current_setting('mandate.tenant', true) and id = $1)"
        }
    };
    Ok(sqlx::query_scalar(q).bind(id).fetch_one(&mut **tx).await?)
}

/// `POST /v1/grants`: 201 with the grant; 409 if the principal already has a grant for the tool.
async fn create(
    State(s): State<Arc<ApiState>>,
    session: Session,
    body: Result<Json<NewGrant>, JsonRejection>,
) -> Result<Response, ApiError> {
    let mut tx = admin_tx(&s, &session).await?;
    let Json(g) = body.map_err(|_| ApiError::invalid_input())?;
    let valid_id = match g.principal.kind {
        Kind::Agent => AgentId::parse(&g.principal.id).is_ok(),
        Kind::Member => MemberId::parse(&g.principal.id).is_ok(),
        Kind::Team => TeamId::parse(&g.principal.id).is_ok(),
    };
    if !valid_id || !principal_exists(&mut tx, g.principal.kind, &g.principal.id).await? {
        return Err(ApiError::invalid_input());
    }
    if s.catalog.get(&g.tool).is_none() {
        return Err(ApiError::invalid_input());
    }
    let now = OffsetDateTime::now_utc();
    let expires =
        OffsetDateTime::parse(&g.expires_at, &Rfc3339).map_err(|_| ApiError::invalid_input())?;
    if expires <= now || expires > now + MAX_GRANT {
        return Err(ApiError::invalid_input());
    }
    if g.reason
        .as_ref()
        .is_some_and(|r| r.chars().count() > MAX_REASON_CHARS)
    {
        return Err(ApiError::invalid_input());
    }
    let kind = kind_str(g.principal.kind);
    // An expired grant for the same principal and tool gives way; a live one is a conflict.
    sqlx::query(
        "delete from grants where tenant_id = current_setting('mandate.tenant', true) \
         and principal_kind = $1 and principal_id = $2 and tool = $3 and expires_at <= $4",
    )
    .bind(kind)
    .bind(&g.principal.id)
    .bind(g.tool.as_str())
    .bind(now)
    .execute(&mut *tx)
    .await
    .map_err(StoreError::from)?;
    let id: Option<Uuid> = sqlx::query_scalar(
        "insert into grants (tenant_id, principal_kind, principal_id, tool, standing, expires_at, granted_by, reason) \
         values (current_setting('mandate.tenant'), $1, $2, $3, $4, $5, $6, $7) \
         on conflict do nothing returning id",
    )
    .bind(kind)
    .bind(&g.principal.id)
    .bind(g.tool.as_str())
    .bind(g.standing)
    .bind(expires)
    .bind(session.member.as_str())
    .bind(&g.reason)
    .fetch_optional(&mut *tx)
    .await
    .map_err(StoreError::from)?;
    let id = id.ok_or(ApiError::conflict(
        "This principal already has a grant for this tool.",
    ))?;
    audit(
        &mut tx,
        &session.member,
        kind,
        &g.principal.id,
        &g.tool,
        "grant_created",
    )
    .await?;
    events::publish(&mut tx, Event::GrantCreated { grant: id }).await?;
    let r = sqlx::query(sqlx::AssertSqlSafe(format!("{GRANT_SQL} and r.id = $1")))
        .bind(id)
        .fetch_one(&mut *tx)
        .await
        .map_err(StoreError::from)?;
    let v = view(&r, now).map_err(StoreError::from)?;
    tx.commit().await.map_err(StoreError::from)?;
    Ok((StatusCode::CREATED, Json(v)).into_response())
}

/// `DELETE /v1/grants/{id}`: 204; the gate stops honouring it with this commit.
async fn revoke(
    State(s): State<Arc<ApiState>>,
    session: Session,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let mut tx = admin_tx(&s, &session).await?;
    let id = Uuid::parse_str(&id).map_err(|_| ApiError::not_found())?;
    let gone: Option<(String, String, String)> = sqlx::query_as(
        "delete from grants where tenant_id = current_setting('mandate.tenant', true) and id = $1 \
         returning principal_kind, principal_id, tool",
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(StoreError::from)?;
    let Some((kind, principal, tool)) = gone else {
        return Err(ApiError::not_found());
    };
    let tool = ToolId::parse(&tool).map_err(StoreError::from)?;
    audit(
        &mut tx,
        &session.member,
        &kind,
        &principal,
        &tool,
        "grant_revoked",
    )
    .await?;
    events::publish(&mut tx, Event::GrantRevoked { grant: id }).await?;
    tx.commit().await.map_err(StoreError::from)?;
    Ok(StatusCode::NO_CONTENT)
}
