//! The organization: its members and teams, and the shapes other routes share.
use crate::api::session::Session;
use crate::api::{ApiError, ApiState};
use crate::ids::MemberId;
use crate::store::{StoreError, Tx};
use axum::Json;
use axum::extract::rejection::QueryRejection;
use axum::extract::{Query, State};
use axum::routing::get;
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

/// Items per page on every list route.
pub(crate) const PAGE: i64 = 50;

pub fn routes() -> axum::Router<Arc<ApiState>> {
    axum::Router::new()
        .route("/v1/org/members", get(members))
        .route("/v1/org/teams", get(teams))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MembersQuery {
    #[serde(default)]
    q: Option<String>,
    #[serde(default)]
    cursor: Option<String>,
}

/// `q` matched as a literal substring: LIKE's wildcards and escape are escaped.
fn like_pattern(q: &str) -> String {
    let escaped = q
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    format!("%{escaped}%")
}

/// `GET /v1/org/members`: everyone in the org, in id order; `q` searches name and email.
async fn members(
    State(s): State<Arc<ApiState>>,
    session: Session,
    q: Result<Query<MembersQuery>, QueryRejection>,
) -> Result<Json<Value>, ApiError> {
    let Query(q) = q.map_err(|_| ApiError::invalid_input())?;
    let search = bounded(q.q, 100)?.map(|q| like_pattern(&q));
    let cursor = bounded(q.cursor, 64)?
        .map(|c| MemberId::parse(&c).map_err(|_| ApiError::invalid_input()))
        .transpose()?;
    let mut tx = s.store.begin(&session.tenant).await?;
    let rows: Vec<(String, Option<String>, Option<String>)> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "select id, name, email from members where tenant_id = current_setting('mandate.tenant', true) \
         and ($1::text is null or name ilike $1 or email ilike $1) \
         and ($2::text is null or id > $2) order by id limit {}",
        PAGE + 1
    )))
    .bind(search)
    .bind(cursor.as_ref().map(MemberId::as_str))
    .fetch_all(&mut *tx)
    .await
    .map_err(StoreError::from)?;
    tx.commit().await.map_err(StoreError::from)?;
    let more = rows.len() as i64 > PAGE;
    let rows = &rows[..rows.len().min(PAGE as usize)];
    let next = match rows.last() {
        Some(r) if more => Value::String(r.0.clone()),
        _ => Value::Null,
    };
    let items: Vec<Value> = rows
        .iter()
        .map(|(id, name, email)| person(id, name.clone(), email.clone()))
        .collect();
    Ok(Json(json!({ "items": items, "next": next })))
}

/// `GET /v1/org/teams`. ponytail: `is_admin` is always false; team admins do not exist until
/// team bots return (on hold).
async fn teams(State(s): State<Arc<ApiState>>, session: Session) -> Result<Json<Value>, ApiError> {
    let mut tx = s.store.begin(&session.tenant).await?;
    let rows: Vec<(String, Option<String>, i64)> = sqlx::query_as(
        "select t.id, t.name, (select count(*) from team_members tm \
           where tm.tenant_id = t.tenant_id and tm.team_id = t.id) \
         from teams t where t.tenant_id = current_setting('mandate.tenant', true) order by t.id",
    )
    .fetch_all(&mut *tx)
    .await
    .map_err(StoreError::from)?;
    tx.commit().await.map_err(StoreError::from)?;
    let items: Vec<Value> = rows
        .into_iter()
        .map(|(id, name, n)| {
            json!({ "id": id, "name": name.unwrap_or_else(|| id.clone()), "members": n, "is_admin": false })
        })
        .collect();
    Ok(Json(json!({ "items": items })))
}

/// RFC 3339, as every timestamp in the API.
pub(crate) fn ts(t: OffsetDateTime) -> Value {
    t.format(&Rfc3339).map(Value::String).unwrap_or(Value::Null)
}

/// `Person`: a member who left keeps their id, with no name or email to show.
pub(crate) fn person(id: &str, name: Option<String>, email: Option<String>) -> Value {
    let name = name
        .or_else(|| email.clone())
        .unwrap_or_else(|| id.to_owned());
    json!({ "id": id, "name": name, "email": email })
}

/// True only for a current org admin: read now, not trusted from the session.
pub(crate) async fn is_admin(tx: &mut Tx, s: &Session) -> Result<bool, StoreError> {
    Ok(sqlx::query_scalar(
        "select exists (select 1 from members where tenant_id = current_setting('mandate.tenant', true) \
         and id = $1 and is_admin)",
    )
    .bind(s.member.as_str())
    .fetch_one(&mut **tx)
    .await?)
}

/// An optional `?q=` or `?cursor=` value, bounded so it cannot carry a large payload.
pub(crate) fn bounded(v: Option<String>, max: usize) -> Result<Option<String>, ApiError> {
    match v {
        Some(v) if v.len() > max => Err(ApiError::invalid_input()),
        Some(v) if v.is_empty() => Ok(None),
        v => Ok(v),
    }
}
