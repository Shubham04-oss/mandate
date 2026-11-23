//! The web app's JSON API. The core authenticates every request from its own session cookie;
//! the web app decides nothing and never talks to WorkOS.
pub mod activity;
pub mod agents;
pub mod approvals;
pub mod auth;
pub mod conversations;
pub mod grants;
pub mod org;
pub mod ratelimit;
pub mod session;

use crate::store::{Store, StoreError};
use crate::tools::Catalog;
use auth::AuthProvider;
use axum::Router;
use axum::body::Bytes;
use axum::extract::{FromRequest, Request, State};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode, header};
use serde::de::DeserializeOwned;
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;
use tower_http::cors::{AllowOrigin, CorsLayer};
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::timeout::TimeoutLayer;

pub(crate) const MAX_BODY_BYTES: usize = 64 * 1024;
const CSRF_HEADER: HeaderName = HeaderName::from_static("x-mandate");
const TIMEOUT: Duration = Duration::from_secs(30);

pub struct ApiState {
    pub store: Store,
    /// `MANDATE_WEB_ORIGIN`: the only origin CORS allows and CSRF accepts, e.g. `https://app.example.com`.
    pub web_origin: String,
    pub auth: Arc<dyn AuthProvider>,
    /// Where WorkOS sends the browser after SSO: this API's `/v1/auth/sso/callback`.
    pub sso_redirect_uri: String,
    /// `MANDATE_TRUST_CF_IP=1`: rate limits key on `CF-Connecting-IP`; only behind Cloudflare.
    pub trust_cf_ip: bool,
    /// The served tools: grants name one, and an approval shows its call's effect.
    pub catalog: Catalog,
}

/// A JSON body (an empty one reads as `{}`); a malformed one is `invalid_input`, never serde's
/// text, which can quote what was typed.
pub struct Input<T>(pub T);

impl<S, T> FromRequest<S> for Input<T>
where
    S: Send + Sync,
    T: DeserializeOwned,
{
    type Rejection = ApiError;

    async fn from_request(req: Request, state: &S) -> Result<Self, ApiError> {
        let bytes = Bytes::from_request(req, state)
            .await
            .map_err(|_| ApiError::invalid_input())?;
        let bytes: &[u8] = if bytes.is_empty() { b"{}" } else { &bytes };
        serde_json::from_slice(bytes)
            .map(Input)
            .map_err(|_| ApiError::invalid_input())
    }
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

    pub const fn not_found() -> Self {
        Self::new(StatusCode::NOT_FOUND, "not_found", "Not found.")
    }

    pub const fn conflict(message: &'static str) -> Self {
        Self::new(StatusCode::CONFLICT, "conflict", message)
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
        // EventSource reconnects send Last-Event-ID; the web reads Retry-After on a 429.
        .allow_headers([
            header::CONTENT_TYPE,
            CSRF_HEADER,
            HeaderName::from_static("last-event-id"),
        ])
        .expose_headers([header::RETRY_AFTER])
}

/// Every `/v1` route of the web API, with its CSRF, CORS and body-size layers.
pub fn router(state: Arc<ApiState>) -> Router {
    let routes = Router::new()
        .merge(auth::routes(&state))
        .merge(approvals::routes())
        .merge(activity::routes())
        .merge(agents::routes())
        .merge(grants::routes())
        .merge(org::routes());
    with_routes(routes, state)
}

/// `routes` behind the API's layers; the layers cover only routes added before this call.
pub fn with_routes(routes: Router<Arc<ApiState>>, state: Arc<ApiState>) -> Router {
    let cors = cors(&state.web_origin);
    routes
        .layer(middleware::from_fn_with_state(state.clone(), csrf))
        .layer(RequestBodyLimitLayer::new(MAX_BODY_BYTES))
        .layer(TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            TIMEOUT,
        ))
        .layer(cors)
        .with_state(state)
}
