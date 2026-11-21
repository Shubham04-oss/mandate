//! Agents a member may use: their personal operator and their teams' bots.
use crate::api::org::person;
use crate::mcp::AppState;
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

pub fn routes() -> axum::Router<Arc<AppState>> {
    axum::Router::new()
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
