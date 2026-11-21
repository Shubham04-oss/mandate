//! The web app's JSON API under `/v1`. Every route takes a verified session and runs in that
//! session's tenant transaction.
pub mod agents;
pub mod approvals;
pub mod org;
pub mod stubs;

use crate::mcp::AppState;
use std::sync::Arc;

pub fn router(state: Arc<AppState>) -> axum::Router {
    axum::Router::new()
        .merge(approvals::routes())
        .merge(agents::routes())
        .merge(org::routes())
        .with_state(state)
}
