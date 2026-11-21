//! Stand-ins for interfaces other lanes own and have not merged yet. Each item names the lane
//! and the interface it stands for; the lead replaces them at integration.
use crate::api::approvals::Decision;
use crate::ids::{MemberId, TenantId};
use crate::store::{StoreError, Tx};
use axum::Json;
use axum::extract::FromRequestParts;
use axum::response::{IntoResponse, Response};
use http::StatusCode;
use http::request::Parts;
use serde_json::json;
use std::sync::Mutex;
use uuid::Uuid;

/// ponytail: lane B's cookie session (`api::session::Session`). Nothing in production inserts
/// this extension, so until B's extractor replaces it every route here answers 401 (fails
/// closed); tests insert it. It also skips B's CSRF check, which B's extractor adds.
#[derive(Debug, Clone)]
pub struct Session {
    pub tenant: TenantId,
    pub member: MemberId,
    pub is_admin: bool,
}

impl<S: Send + Sync> FromRequestParts<S> for Session {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, _: &S) -> Result<Self, ApiError> {
        parts
            .extensions
            .get::<Session>()
            .cloned()
            .ok_or(ApiError::Unauthenticated)
    }
}

/// ponytail: the API error body of spec 4.1; lane B may own the shared type, merge into it.
#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("sign in first")]
    Unauthenticated,
    #[error("{0}")]
    InvalidInput(&'static str),
    #[error("you may not do that")]
    Forbidden,
    #[error("not found")]
    NotFound,
    #[error("{0}")]
    Conflict(&'static str),
    #[error("something went wrong")]
    Internal,
}

impl From<StoreError> for ApiError {
    fn from(e: StoreError) -> Self {
        // Detail to the operator log only; the browser gets the generic message.
        tracing::error!(error = %e, "api store error");
        ApiError::Internal
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, code) = match self {
            ApiError::Unauthenticated => (StatusCode::UNAUTHORIZED, "forbidden"),
            ApiError::InvalidInput(_) => (StatusCode::BAD_REQUEST, "invalid_input"),
            ApiError::Forbidden => (StatusCode::FORBIDDEN, "forbidden"),
            ApiError::NotFound => (StatusCode::NOT_FOUND, "not_found"),
            ApiError::Conflict(_) => (StatusCode::CONFLICT, "conflict"),
            ApiError::Internal => (StatusCode::INTERNAL_SERVER_ERROR, "internal"),
        };
        let body = json!({ "error": { "code": code, "message": self.to_string() } });
        (status, Json(body)).into_response()
    }
}

/// Who changed a person's record (lane F's `events::Actor`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Actor {
    Member(MemberId),
    Directory,
    System,
}

/// The subset of lane F's `events::Event` this lane emits; ids only, as in F's envelope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    ApprovalDecided {
        approval: Uuid,
        decision: Decision,
        by: MemberId,
    },
    GrantCreated {
        grant: Uuid,
    },
    GrantRevoked {
        grant: Uuid,
    },
    MemberCreated {
        member: MemberId,
        actor: Actor,
    },
    MemberUpdated {
        member: MemberId,
        actor: Actor,
    },
    MemberRemoved {
        member: MemberId,
        actor: Actor,
    },
}

static PUBLISHED: Mutex<Vec<(String, Event)>> = Mutex::new(Vec::new());

/// ponytail: lane F's `events::publish(tx, event)`, which writes an outbox row in the caller's
/// transaction. This stand-in only records (tenant, event) in memory, so a rolled-back
/// transaction still leaves its record and nothing is delivered.
pub async fn publish(tx: &mut Tx, event: Event) -> Result<(), StoreError> {
    let tenant: String = sqlx::query_scalar("select current_setting('mandate.tenant', true)")
        .fetch_one(&mut **tx)
        .await?;
    PUBLISHED
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .push((tenant, event));
    Ok(())
}

/// What `publish` recorded for `tenant`, oldest first; for tests until lane F's outbox lands.
#[doc(hidden)]
pub fn published(tenant: &TenantId) -> Vec<Event> {
    PUBLISHED
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .iter()
        .filter(|(t, _)| t == tenant.as_str())
        .map(|(_, e)| e.clone())
        .collect()
}
