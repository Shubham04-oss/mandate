//! The web app's JSON API. The core authenticates every request from its own session cookie;
//! the web app decides nothing and never talks to WorkOS.
pub mod session;

use crate::store::{Store, StoreError};
use axum::Router;
use axum::extract::{Request, State};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode, header};
use serde_json::json;
use std::sync::Arc;
use tower_http::cors::{AllowOrigin, CorsLayer};
use tower_http::limit::RequestBodyLimitLayer;

pub(crate) const MAX_BODY_BYTES: usize = 64 * 1024;
const CSRF_HEADER: HeaderName = HeaderName::from_static("x-mandate");

pub struct ApiState {
    pub store: Store,
    /// `MANDATE_WEB_ORIGIN`: the only origin CORS allows and CSRF accepts, e.g. `https://app.example.com`.
    pub web_origin: String,
}

/// `{ "error": { "code", "message" } }`; messages are ours, never WorkOS's or the database's.
#[derive(Debug)]
pub struct ApiError {
    status: StatusCode,
    code: &'static str,
    message: &'static str,
    retry_after: Option<i64>,
}

impl ApiError {
    pub const fn new(status: StatusCode, code: &'static str, message: &'static str) -> Self {
        ApiError {
            status,
            code,
            message,
            retry_after: None,
        }
    }

    pub const fn invalid_input() -> Self {
        Self::new(
            StatusCode::BAD_REQUEST,
            "invalid_input",
            "Please check what you entered.",
        )
    }

    pub const fn rate_limited(retry_after: i64) -> Self {
        let mut e = Self::new(
            StatusCode::TOO_MANY_REQUESTS,
            "rate_limited",
            "Too many attempts. Please wait a minute and try again.",
        );
        e.retry_after = Some(retry_after);
        e
    }

    pub const fn signed_out() -> Self {
        Self::new(StatusCode::UNAUTHORIZED, "forbidden", "Please sign in.")
    }

    pub const fn forbidden() -> Self {
        Self::new(
            StatusCode::FORBIDDEN,
            "forbidden",
            "This request is not allowed.",
        )
    }

    pub const fn internal() -> Self {
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal",
            "Something went wrong. Please try again.",
        )
    }

    pub fn status(&self) -> StatusCode {
        self.status
    }

    pub fn code(&self) -> &'static str {
        self.code
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let body = json!({ "error": { "code": self.code, "message": self.message } });
        let mut r = (self.status, axum::Json(body)).into_response();
        if let Some(secs) = self.retry_after {
            r.headers_mut()
                .insert(header::RETRY_AFTER, HeaderValue::from(secs));
        }
        r
    }
}

impl From<StoreError> for ApiError {
    fn from(error: StoreError) -> Self {
        tracing::error!(%error, "api store error");
        ApiError::internal()
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(e: sqlx::Error) -> Self {
        StoreError::from(e).into()
    }
}

/// State-changing requests must carry `X-Mandate: 1` and come from the web origin. A form or
/// link on another site cannot set the header, and CORS keeps other origins from sending it.
pub(crate) fn csrf_ok(method: &Method, headers: &HeaderMap, web_origin: &str) -> bool {
    if matches!(*method, Method::GET | Method::HEAD | Method::OPTIONS) {
        return true;
    }
    headers.get(CSRF_HEADER).is_some_and(|v| v == "1")
        && headers
            .get(header::ORIGIN)
            .is_some_and(|v| v.as_bytes() == web_origin.as_bytes())
}

async fn csrf(State(s): State<Arc<ApiState>>, req: Request, next: Next) -> Response {
    if csrf_ok(req.method(), req.headers(), &s.web_origin) {
        next.run(req).await
    } else {
        ApiError::forbidden().into_response()
    }
}

fn cors(web_origin: &str) -> CorsLayer {
    // A list, not `exact`: other origins get no CORS headers at all. An origin that is not a
    // valid header value matches nothing: fail closed.
    let origin = AllowOrigin::list(HeaderValue::from_str(web_origin).ok());
    CorsLayer::new()
        .allow_origin(origin)
        .allow_credentials(true)
        .allow_methods([Method::GET, Method::POST, Method::DELETE])
        .allow_headers([header::CONTENT_TYPE, CSRF_HEADER])
}

/// Every `/v1` route of the web API, with its CSRF, CORS and body-size layers.
pub fn router(state: Arc<ApiState>) -> Router {
    with_routes(Router::new(), state)
}

/// `routes` behind the API's layers; the layers cover only routes added before this call.
pub fn with_routes(routes: Router<Arc<ApiState>>, state: Arc<ApiState>) -> Router {
    let cors = cors(&state.web_origin);
    routes
        .layer(middleware::from_fn_with_state(state.clone(), csrf))
        .layer(RequestBodyLimitLayer::new(MAX_BODY_BYTES))
        .layer(cors)
        .with_state(state)
}
