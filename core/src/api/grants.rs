//! Grants, managed by org admins: who may use which tool, until when. The gate reads them on
//! every call, so a revoked or expired grant stops working at once.
use crate::api::org::{is_admin, person, ts};
use crate::api::stubs::{self, ApiError, Event, Session};
use crate::ids::{AgentId, MemberId, TeamId, ToolId};
use crate::mcp::AppState;
use crate::store::{StoreError, Tx};
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
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use uuid::Uuid;

const MAX_REASON_CHARS: usize = 500;

pub fn routes() -> axum::Router<Arc<AppState>> {
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

/// Every route here is for org admins only, checked in the request's own transaction.
async fn admin_tx(s: &AppState, session: &Session) -> Result<Tx, ApiError> {
    let mut tx = s.store.begin(&session.tenant).await?;
    if !is_admin(&mut tx, session).await? {
        return Err(ApiError::Forbidden);
    }
    Ok(tx)
}

/// `GET /v1/grants`: every grant in the org, expired ones marked so.
async fn list(State(s): State<Arc<AppState>>, session: Session) -> Result<Json<Value>, ApiError> {
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
    State(s): State<Arc<AppState>>,
    session: Session,
    body: Result<Json<NewGrant>, JsonRejection>,
) -> Result<Response, ApiError> {
    let mut tx = admin_tx(&s, &session).await?;
    let Json(g) = body.map_err(|_| ApiError::InvalidInput("invalid grant"))?;
    let bad = ApiError::InvalidInput;
    let valid_id = match g.principal.kind {
        Kind::Agent => AgentId::parse(&g.principal.id).is_ok(),
        Kind::Member => MemberId::parse(&g.principal.id).is_ok(),
        Kind::Team => TeamId::parse(&g.principal.id).is_ok(),
    };
    if !valid_id || !principal_exists(&mut tx, g.principal.kind, &g.principal.id).await? {
        return Err(bad("no such principal"));
    }
    if s.catalog.get(&g.tool).is_none() {
        return Err(bad("no such tool"));
    }
    let now = OffsetDateTime::now_utc();
    let expires = OffsetDateTime::parse(&g.expires_at, &Rfc3339)
        .map_err(|_| bad("expires_at must be an RFC 3339 time"))?;
    if expires <= now {
        return Err(bad("expires_at must be in the future"));
    }
    if g.reason
        .as_ref()
        .is_some_and(|r| r.chars().count() > MAX_REASON_CHARS)
    {
        return Err(bad("reason is too long"));
    }
    let kind = match g.principal.kind {
        Kind::Agent => "agent",
        Kind::Member => "member",
        Kind::Team => "team",
    };
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
    let id = id.ok_or(ApiError::Conflict(
        "this principal already has a grant for this tool",
    ))?;
    stubs::publish(&mut tx, Event::GrantCreated { grant: id }).await?;
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
    State(s): State<Arc<AppState>>,
    session: Session,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let mut tx = admin_tx(&s, &session).await?;
    let id = Uuid::parse_str(&id).map_err(|_| ApiError::NotFound)?;
    let gone: Option<Uuid> = sqlx::query_scalar(
        "delete from grants where tenant_id = current_setting('mandate.tenant', true) and id = $1 returning id",
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(StoreError::from)?;
    if gone.is_none() {
        return Err(ApiError::NotFound);
    }
    stubs::publish(&mut tx, Event::GrantRevoked { grant: id }).await?;
    tx.commit().await.map_err(StoreError::from)?;
    Ok(StatusCode::NO_CONTENT)
}
