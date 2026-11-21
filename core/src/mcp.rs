//! The MCP server: the only door from the agent to tools. Lists what the gate allows, gates every call.
use crate::assertion::{Assertion, verify};
use crate::gate::{Outcome, authorize, list};
use crate::identity::{Workos, exchange};
use crate::ids::ToolId;
use crate::policy::{Decision, Effect, Policy};
use crate::store::Store;
use crate::tools::{Catalog, ToolError};
use axum::extract::{Request, State};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use ed25519_dalek::{SigningKey, VerifyingKey};
use http::StatusCode;
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, ListToolsResult,
    PaginatedRequestParams, ServerCapabilities, ServerConfig, Tool as McpTool,
};
use rmcp::service::RequestContext;
use rmcp::transport::streamable_http_server::session::never::NeverSessionManager;
use rmcp::transport::streamable_http_server::{StreamableHttpServerConfig, StreamableHttpService};
use rmcp::{ErrorData, RoleServer, ServerHandler};
use serde_json::{Value, json};
use std::sync::Arc;
use std::time::Duration;
use time::OffsetDateTime;
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::timeout::TimeoutLayer;

const MAX_ARGS_BYTES: usize = 64 * 1024;
const MAX_BODY_BYTES: usize = 1024 * 1024;
const TIMEOUT: Duration = Duration::from_secs(60);

pub struct AppState {
    pub store: Store,
    pub policy: Policy,
    pub catalog: Catalog,
    pub assertion_key: VerifyingKey,
    pub assertion_signing_key: SigningKey,
    /// `None` when WorkOS is not configured: the exchange route answers 503.
    pub workos: Option<Arc<Workos>>,
}

/// The verified assertion of this very request; never cached across requests.
fn caller(ctx: &RequestContext<RoleServer>) -> Result<Assertion, ErrorData> {
    ctx.extensions
        .get::<http::request::Parts>()
        .and_then(|p| p.extensions.get::<Assertion>())
        .cloned()
        .ok_or_else(|| ErrorData::internal_error("no verified assertion", None))
}

#[derive(Clone)]
struct Mcp(Arc<AppState>);

impl ServerHandler for Mcp {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
    }

    async fn list_tools(
        &self,
        _: Option<PaginatedRequestParams>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        let who = caller(&ctx)?;
        let s = &self.0;
        // No args at listing time; Null gives each tool its most conservative effect.
        let catalog: Vec<(ToolId, Effect)> = s
            .catalog
            .ids()
            .into_iter()
            .filter_map(|id| s.catalog.get(&id).map(|t| (id, t.effect(&Value::Null))))
            .collect();
        let allowed = list(
            &s.store,
            &s.policy,
            &who,
            &catalog,
            OffsetDateTime::now_utc(),
        )
        .await;
        let tools = allowed
            .into_iter()
            .filter_map(|(id, d)| {
                let t = s.catalog.get(&id)?;
                let mut desc = t.description().to_owned();
                if d == Decision::Ask {
                    desc.push_str(" (asks for approval)");
                }
                let Value::Object(schema) = t.input_schema() else {
                    return None;
                };
                Some(McpTool::new(id.to_string(), desc, schema))
            })
            .collect();
        Ok(ListToolsResult::with_all_items(tools))
    }

    async fn call_tool(
        &self,
        req: CallToolRequestParams,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        let who = caller(&ctx)?;
        let s = &self.0;
        let args = Value::Object(req.arguments.unwrap_or_default());
        if serde_json::to_vec(&args).map_or(true, |b| b.len() > MAX_ARGS_BYTES) {
            return Err(ErrorData::invalid_params("arguments over 64 KiB", None));
        }
        let unknown = || ErrorData::invalid_params("unknown tool", None);
        let id = ToolId::parse(&req.name).map_err(|_| unknown())?;
        let tool = s.catalog.get(&id).ok_or_else(unknown)?;
        tool.validate(&args)
            .map_err(|e| ErrorData::invalid_params(e.to_string(), None))?;
        // The effect of these exact args: the same tool can be Read for one call and External for another.
        let effect = tool.effect(&args);
        let now = OffsetDateTime::now_utc();
        let result = match authorize(&s.store, &s.policy, &who, &id, effect, &args, now).await {
            Outcome::Run => match tool.call(&who, args).await {
                Ok(v) => CallToolResult::structured(v),
                Err(e @ ToolError::InvalidArgs(_)) => {
                    CallToolResult::error(vec![ContentBlock::text(e.to_string())])
                }
                // The agent is untrusted: backend detail goes to the operator log only.
                Err(ToolError::Backend(detail)) => {
                    tracing::warn!(tool = %id, %detail, "tool backend failed");
                    CallToolResult::error(vec![ContentBlock::text("the tool failed")])
                }
            },
            Outcome::Ask { approval } => CallToolResult::structured(
                json!({ "status": "needs_approval", "approval": approval.to_string() }),
            ),
            Outcome::Refuse { reason } => return Err(ErrorData::invalid_request(reason, None)),
        };
        Ok(result.into())
    }
}

async fn bearer(State(s): State<Arc<AppState>>, mut req: Request, next: Next) -> Response {
    let now = OffsetDateTime::now_utc().unix_timestamp();
    let who = req
        .headers()
        .get(http::header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "))
        .and_then(|t| verify(t, &s.assertion_key, now).ok());
    match who {
        Some(a) => {
            req.extensions_mut().insert(a);
            next.run(req).await
        }
        None => StatusCode::UNAUTHORIZED.into_response(),
    }
}

/// MCP streamable HTTP at `/mcp`, stateless: every request stands on its own bearer assertion.
/// `POST /v1/assertions` sits outside that check: its bearer is a WorkOS token, not an assertion.
pub fn router(state: Arc<AppState>) -> axum::Router {
    let handler = Mcp(state.clone());
    let config = StreamableHttpServerConfig::default()
        .with_legacy_session_mode(false)
        .with_json_response(true)
        .with_sse_keep_alive(None)
        .with_max_request_body_bytes(MAX_BODY_BYTES)
        // DNS rebinding needs ambient credentials; every request here carries an explicit bearer.
        .disable_allowed_hosts();
    let mcp = StreamableHttpService::new(
        move || Ok(handler.clone()),
        Arc::new(NeverSessionManager::default()),
        config,
    );
    let exchange = axum::Router::new()
        .route("/v1/assertions", axum::routing::post(exchange))
        .with_state(state.clone());
    axum::Router::new()
        .nest_service("/mcp", mcp)
        .layer(middleware::from_fn_with_state(state, bearer))
        .merge(exchange)
        .layer(RequestBodyLimitLayer::new(MAX_BODY_BYTES))
        .layer(TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            TIMEOUT,
        ))
}
