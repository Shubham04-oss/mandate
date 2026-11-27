//! `POST /v1/debug/error`: an org admin forces an error in the core or in eve, to check that error
//! reporting works end to end. Mounted only with `MANDATE_DEBUG_ROUTES=1`.
use crate::api::conversations::Conversations;
use crate::api::org::is_admin;
use crate::api::session::Session;
use crate::api::{ApiError, ApiState};
use crate::assertion::{Assertion, TTL_SECS, sign};
use crate::eve::EveError;
use crate::ids::AgentId;
use crate::store::StoreError;
use axum::extract::State;
use axum::extract::rejection::JsonRejection;
use axum::routing::post;
use axum::{Extension, Json, Router};
use http::StatusCode;
use serde::Deserialize;
use std::sync::Arc;
use time::OffsetDateTime;

// 503, not 502: Cloudflare replaces an origin 502 with its own page, which drops the CORS headers.
const EVE_DOWN: ApiError = ApiError::new(
    StatusCode::SERVICE_UNAVAILABLE,
    "internal",
    "The agent is unavailable.",
);
const EVE_FAILED: ApiError = ApiError::new(
    StatusCode::INTERNAL_SERVER_ERROR,
    "internal",
    "The agent reported the test error.",
);
// No agent has this id, so the assertion eve receives carries no tool access at the gate.
const AGENT: &str = "debug";

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
enum Service {
    Core,
    Eve,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Force {
    service: Service,
}

/// The route when `state.debug_routes` is set, else none (an unknown route's 404). For
/// `api::with_routes`; `eve` is `None` when eve is not configured.
pub fn routes(state: &ApiState, eve: Option<Arc<Conversations>>) -> Router<Arc<ApiState>> {
    if !state.debug_routes {
        return Router::new();
    }
    Router::new()
        .route("/v1/debug/error", post(force))
        .layer(Extension(eve))
}

/// Always an error: 500 when the service raised it, 503 when eve could not be asked.
async fn force(
    State(s): State<Arc<ApiState>>,
    Extension(eve): Extension<Option<Arc<Conversations>>>,
    session: Session,
    body: Result<Json<Force>, JsonRejection>,
) -> Result<(), ApiError> {
    let mut tx = s.store.begin(&session.tenant).await?;
    let admin = is_admin(&mut tx, &session).await?;
    tx.commit().await?;
    if !admin {
        return Err(ApiError::forbidden());
    }
    let Json(f) = body.map_err(|_| ApiError::invalid_input())?;
    Err(match f.service {
        Service::Core => {
            tracing::error!("forced test error");
            ApiError::internal()
        }
        Service::Eve => match eve {
            Some(conv) => ask_eve(&conv, &session).await?,
            None => EVE_DOWN,
        },
    })
}

/// eve's answer as the error to return; eve's body is never read or logged.
async fn ask_eve(conv: &Conversations, session: &Session) -> Result<ApiError, ApiError> {
    let a = Assertion {
        tenant: session.tenant.clone(),
        member: session.member.clone(),
        agent: AgentId::parse(AGENT).map_err(StoreError::from)?,
        space: None,
        expires_at: OffsetDateTime::now_utc().unix_timestamp() + TTL_SECS,
    };
    let token = sign(&a, &conv.app.assertion_signing_key);
    Ok(match conv.eve.debug_error(&token).await {
        Err(EveError::Status(500..=599)) => EVE_FAILED,
        Err(error) => {
            tracing::warn!(%error, "eve debug call failed");
            EVE_DOWN
        }
        Ok(()) => {
            tracing::warn!("eve did not raise the test error");
            EVE_DOWN
        }
    })
}
