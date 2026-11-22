//! Agents a member may use: their personal operator and their teams' bots.
use crate::api::org::person;
use crate::api::session::Session;
use crate::api::{ApiError, ApiState};
use crate::store::StoreError;
use axum::Json;
use axum::extract::State;
use axum::routing::get;
use serde_json::{Value, json};
use sqlx::Row;
use sqlx::postgres::PgRow;
use std::sync::Arc;

/// Columns for `agent_view`, from agents `g` joined with `AGENT_JOINS`.
pub(crate) const AGENT_COLS: &str = "g.id as g_id, g.name as g_name, g.kind as g_kind, \
     g.owner_member as g_om, om.name as g_om_name, om.email as g_om_email, \
     g.owner_team as g_ot, ot.name as g_ot_name, g.disabled as g_disabled";
pub(crate) const AGENT_JOINS: &str = "left join members om on om.tenant_id = g.tenant_id and om.id = g.owner_member \
     left join teams ot on ot.tenant_id = g.tenant_id and ot.id = g.owner_team";

pub fn routes() -> axum::Router<Arc<ApiState>> {
    axum::Router::new().route("/v1/agents", get(list))
}

/// `GET /v1/agents`: the agents the caller may use, disabled ones marked so.
async fn list(State(s): State<Arc<ApiState>>, session: Session) -> Result<Json<Value>, ApiError> {
    let mut tx = s.store.begin(&session.tenant).await?;
    let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
        "select {AGENT_COLS} from agents g {AGENT_JOINS} \
         where g.tenant_id = current_setting('mandate.tenant', true) \
         and (g.owner_member = $1 or exists (select 1 from team_members tm \
           where tm.tenant_id = g.tenant_id and tm.team_id = g.owner_team and tm.member_id = $1)) \
         order by g.kind, g.id"
    )))
    .bind(session.member.as_str())
    .fetch_all(&mut *tx)
    .await
    .map_err(StoreError::from)?;
    tx.commit().await.map_err(StoreError::from)?;
    let items = rows
        .iter()
        .map(|r| agent_view(r, ""))
        .collect::<Result<Vec<_>, _>>()
        .map_err(StoreError::from)?;
    Ok(Json(json!({ "items": items })))
}

/// `Agent`; `id` stands in when the agent row is gone, shown as disabled.
pub(crate) fn agent_view(r: &PgRow, id: &str) -> Result<Value, sqlx::Error> {
    let Some(gid) = r.try_get::<Option<String>, _>("g_id")? else {
        return Ok(json!({ "id": id, "name": id, "kind": null, "owner": null, "enabled": false }));
    };
    let owner = match r.try_get::<Option<String>, _>("g_om")? {
        Some(m) => person(&m, r.try_get("g_om_name")?, r.try_get("g_om_email")?),
        None => match r.try_get::<Option<String>, _>("g_ot")? {
            Some(t) => {
                let name: Option<String> = r.try_get("g_ot_name")?;
                json!({ "id": t, "name": name.unwrap_or_else(|| t.clone()) })
            }
            None => Value::Null,
        },
    };
    let name: Option<String> = r.try_get("g_name")?;
    Ok(json!({
        "id": gid,
        "name": name.unwrap_or_else(|| gid.clone()),
        "kind": r.try_get::<String, _>("g_kind")?,
        "owner": owner,
        "enabled": !r.try_get::<bool, _>("g_disabled")?,
    }))
}
