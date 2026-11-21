//! The organization: its members and teams, and the shapes other routes share.
use crate::api::stubs::ApiError;
use crate::mcp::AppState;
use serde_json::{Value, json};
use std::sync::Arc;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

/// Items per page on every list route.
pub(crate) const PAGE: i64 = 50;

pub fn routes() -> axum::Router<Arc<AppState>> {
    axum::Router::new()
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

/// An optional `?q=` or `?cursor=` value, bounded so it cannot carry a large payload.
pub(crate) fn bounded(v: Option<String>, max: usize) -> Result<Option<String>, ApiError> {
    match v {
        Some(v) if v.len() > max => Err(ApiError::InvalidInput("parameter too long")),
        Some(v) if v.is_empty() => Ok(None),
        v => Ok(v),
    }
}
