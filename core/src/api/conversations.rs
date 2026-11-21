//! Your agent in the web app. The core starts eve turns for the signed-in member's personal
//! operator, relays eve's events to the browser as server-sent events, and resumes a turn when
//! its approval is decided. eve is untrusted: its stream is translated, never passed through.
use crate::assertion::{Assertion, TTL_SECS, sign};
use crate::eve::{EveClient, EveError, EveSessionId, Fact, Record};
use crate::gate::{self, args_hash};
use crate::guard::{Guard, NOTICE, screen};
use crate::ids::{AgentId, MemberId, TenantId, ToolId};
use crate::mcp::AppState;
use crate::policy::Decision;
use crate::store::{self, AuditEntry, StoreError, Tx};
use axum::extract::{FromRequestParts, Path, Query, State};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use futures::Stream;
use http::request::Parts;
use http::{HeaderMap, StatusCode};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet, VecDeque};
use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;
use time::OffsetDateTime;
use tokio::sync::Mutex;
use tower_http::limit::RequestBodyLimitLayer;
use uuid::Uuid;

const MAX_MESSAGE_BYTES: usize = 16 * 1024;
const MAX_BODY_BYTES: usize = 64 * 1024;
const PAGE: i64 = 50;
// A browser stream ends after this long; EventSource reconnects with Last-Event-ID.
const STREAM_LIFETIME: Duration = Duration::from_secs(15 * 60);
const MAX_RELAY_FAILURES: u32 = 5;
// eve prefixes connection tools with the connection's file name (`agents/agent/connections/mandate.ts`).
const TOOL_PREFIX: &str = "mandate__";
const TS: &str = r#"'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"'"#;

/// ponytail: stand-in for lane B's `Session` extractor (cookie, CSRF). It trusts only a value
/// a layer put in the request extensions, so in production, with no such layer, every request is 401.
#[derive(Debug, Clone)]
pub struct Session {
    pub tenant: TenantId,
    pub member: MemberId,
    pub is_admin: bool,
}

impl<S: Send + Sync> FromRequestParts<S> for Session {
    type Rejection = ApiError;
    async fn from_request_parts(parts: &mut Parts, _: &S) -> Result<Self, ApiError> {
        parts.extensions.get::<Session>().cloned().ok_or(ApiError(
            StatusCode::UNAUTHORIZED,
            "forbidden",
            "Sign in first.",
        ))
    }
}

/// `{ "error": { "code", "message" } }`; messages are short and never carry eve's text.
#[derive(Debug)]
pub struct ApiError(StatusCode, &'static str, &'static str);

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let body = json!({ "error": { "code": self.1, "message": self.2 } });
        (self.0, Json(body)).into_response()
    }
}

fn internal<E: std::fmt::Display>(e: E) -> ApiError {
    tracing::error!(error = %e, "conversations request failed");
    ApiError(
        StatusCode::INTERNAL_SERVER_ERROR,
        "internal",
        "Something went wrong.",
    )
}

fn eve_down(e: EveError) -> ApiError {
    tracing::warn!(error = %e, "eve call failed");
    ApiError(
        StatusCode::BAD_GATEWAY,
        "internal",
        "The agent is unavailable.",
    )
}

const NOT_FOUND: ApiError = ApiError(StatusCode::NOT_FOUND, "not_found", "No such conversation.");

#[derive(Debug, thiserror::Error)]
pub enum ConversationError {
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Db(#[from] sqlx::Error),
    #[error(transparent)]
    Eve(#[from] EveError),
}

pub struct Conversations {
    pub app: Arc<AppState>,
    pub eve: EveClient,
    pub guard: Arc<dyn Guard>,
    /// Pause between relay reconnects and between browser polls.
    pub poll: Duration,
    /// Conversations with a relay running in this process; a second one would only waste work.
    pumps: Mutex<HashSet<(TenantId, Uuid)>>,
}

/// What the core keeps about one conversation.
struct Conv {
    member: MemberId,
    agent: AgentId,
    eve: EveSessionId,
    cursor: i64,
}

impl Conversations {
    pub fn new(
        app: Arc<AppState>,
        eve: EveClient,
        guard: Arc<dyn Guard>,
        poll: Duration,
    ) -> Arc<Self> {
        Arc::new(Self {
            app,
            eve,
            guard,
            poll,
            pumps: Mutex::new(HashSet::new()),
        })
    }

    fn who(&self, tenant: &TenantId, member: &MemberId, agent: &AgentId) -> Assertion {
        Assertion {
            tenant: tenant.clone(),
            member: member.clone(),
            agent: agent.clone(),
            space: None,
            expires_at: OffsetDateTime::now_utc().unix_timestamp() + TTL_SECS,
        }
    }

    /// A fresh assertion for every eve call: a waiting turn may resume long after the last one expired.
    fn token(&self, tenant: &TenantId, c: &Conv) -> String {
        sign(
            &self.who(tenant, &c.member, &c.agent),
            &self.app.assertion_signing_key,
        )
    }

    /// Resumes the eve turn waiting on `approval`: answers its prompt and relays what follows.
    /// False when no conversation waits on it. ponytail: lane F's consumer of
    /// `approval.decided.v1` (fed by lane C's `decide_approval`) calls this once both merge.
    pub async fn approval_decided(
        self: &Arc<Self>,
        tenant: &TenantId,
        approval: Uuid,
        approve: bool,
    ) -> Result<bool, ConversationError> {
        let mut tx = self.app.store.begin(tenant).await?;
        let row: Option<(Uuid, String)> = sqlx::query_as(
            "select conversation, interaction from conversation_approvals \
             where tenant_id = current_setting('mandate.tenant', true) and approval = $1",
        )
        .bind(approval)
        .fetch_optional(&mut *tx)
        .await?;
        tx.commit().await?;
        let Some((id, interaction)) = row else {
            return Ok(false);
        };
        let Some(c) = load(&self.app.store, tenant, id, None).await? else {
            return Ok(false);
        };
        // eve only runs the call; the core's MCP gate spends the approval, or asks again.
        self.eve
            .answer(&self.token(tenant, &c), &c.eve, &interaction, approve)
            .await?;
        let mut tx = self.app.store.begin(tenant).await?;
        set_state(&mut tx, id, "working").await?;
        tx.commit().await?;
        self.kick(tenant, id).await;
        Ok(true)
    }

    /// Starts a relay for this conversation unless one already runs here.
    pub async fn kick(self: &Arc<Self>, tenant: &TenantId, id: Uuid) {
        let mut pumps = self.pumps.lock().await;
        if pumps.insert((tenant.clone(), id)) {
            tokio::spawn(self.clone().pump(tenant.clone(), id));
        }
    }

    async fn pump(self: Arc<Self>, tenant: TenantId, id: Uuid) {
        let mut calls = HashMap::new();
        let mut failures = 0;
        loop {
            let ended = match self.relay(&tenant, id, &mut calls).await {
                Ok(ended) => {
                    failures = 0;
                    ended
                }
                Err(e) => {
                    failures += 1;
                    tracing::warn!(error = %e, conversation = %id, failures, "relay failed");
                    if failures >= MAX_RELAY_FAILURES {
                        let _ = self.give_up(&tenant, id).await;
                        true
                    } else {
                        false
                    }
                }
            };
            // Decided under the lock, so a `kick` racing this exit either sees us or starts anew.
            let mut pumps = self.pumps.lock().await;
            let working =
                !ended && matches!(self.state(&tenant, id).await, Ok(Some(s)) if s == "working");
            if !working {
                pumps.remove(&(tenant.clone(), id));
                return;
            }
            drop(pumps);
            tokio::time::sleep(self.poll * (1 + failures)).await;
        }
    }

    async fn state(&self, tenant: &TenantId, id: Uuid) -> Result<Option<String>, StoreError> {
        let mut tx = self.app.store.begin(tenant).await?;
        Ok(sqlx::query_scalar(
            "select state from conversations where tenant_id = current_setting('mandate.tenant', true) and id = $1",
        )
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?)
    }

    async fn give_up(&self, tenant: &TenantId, id: Uuid) -> Result<(), ConversationError> {
        let mut tx = self.app.store.begin(tenant).await?;
        lock(&mut tx, id).await?;
        let ev = json!({ "code": "internal", "message": "The agent is unavailable." });
        push(&mut tx, id, "error", ev).await?;
        set_state(&mut tx, id, "idle").await?;
        tx.commit().await?;
        Ok(())
    }

    /// Relays eve's stored lines from the conversation's cursor until eve ends the read.
    /// Returns true when the eve session has ended.
    async fn relay(
        &self,
        tenant: &TenantId,
        id: Uuid,
        calls: &mut HashMap<String, (ToolId, Value)>,
    ) -> Result<bool, ConversationError> {
        let Some(c) = load(&self.app.store, tenant, id, None).await? else {
            return Ok(true);
        };
        let start = u64::try_from(c.cursor).unwrap_or(0);
        let mut stream = self
            .eve
            .stream(&self.token(tenant, &c), &c.eve, start)
            .await?;
        while let Some(record) = stream.next().await? {
            match record {
                Record::Line { index, facts } => {
                    self.line(tenant, id, &c, index, &facts, calls).await?
                }
                Record::Ended => {
                    let mut tx = self.app.store.begin(tenant).await?;
                    lock(&mut tx, id).await?;
                    set_state(&mut tx, id, "idle").await?;
                    tx.commit().await?;
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }

    /// One eve line becomes zero or more browser events, written with the cursor in one transaction.
    async fn line(
        &self,
        tenant: &TenantId,
        id: Uuid,
        c: &Conv,
        index: u64,
        facts: &[Fact],
        calls: &mut HashMap<String, (ToolId, Value)>,
    ) -> Result<(), ConversationError> {
        let who = self.who(tenant, &c.member, &c.agent);
        // Network and policy work first, outside the transaction that holds the row lock.
        let mut prepared = Vec::with_capacity(facts.len());
        for f in facts {
            prepared.push(self.prepare(&who, id, f, calls).await?);
        }
        let mut answers = Vec::new();
        let mut tx = self.app.store.begin(tenant).await?;
        // Another relay (another process) already wrote this line: skip it.
        if lock(&mut tx, id).await? != Some(index as i64) {
            return Ok(());
        }
        for (f, p) in facts.iter().zip(prepared) {
            apply(&mut tx, id, &who, f, p, &mut answers).await?;
        }
        sqlx::query(
            "update conversations set eve_cursor = $2, updated_at = now() \
             where tenant_id = current_setting('mandate.tenant', true) and id = $1",
        )
        .bind(id)
        .bind(index as i64 + 1)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        // ponytail: an answer lost between this commit and eve leaves the turn waiting until cancelled.
        for (interaction, approve) in answers {
            self.eve
                .answer(&self.token(tenant, c), &c.eve, &interaction, approve)
                .await?;
        }
        Ok(())
    }

    /// What a fact needs decided before it is written: tool calls are remembered, approval
    /// prompts are put to the policy.
    async fn prepare(
        &self,
        who: &Assertion,
        id: Uuid,
        f: &Fact,
        calls: &mut HashMap<String, (ToolId, Value)>,
    ) -> Result<Prepared, ConversationError> {
        Ok(match f.kind.as_str() {
            "call.requested" => {
                let name = f.data["capability"]["name"].as_str().unwrap_or_default();
                let call = f.data["callId"].as_str().unwrap_or_default();
                if let Some(tool) = name
                    .strip_prefix(TOOL_PREFIX)
                    .and_then(|t| ToolId::parse(t).ok())
                {
                    let input = f.data.get("input").cloned().unwrap_or(Value::Null);
                    calls.insert(call.to_owned(), (tool, input));
                }
                Prepared::Call(calls.get(call).cloned())
            }
            "call.started" | "call.settled" => {
                let call = f.data["callId"].as_str().unwrap_or_default();
                Prepared::Call(self.call(who, id, call, calls).await?)
            }
            "interaction.opened" if f.data["request"]["kind"] == "approval" => {
                let call = f.data["subject"]["callId"].as_str().unwrap_or_default();
                let Some((tool, args)) = self.call(who, id, call, calls).await? else {
                    return Ok(Prepared::Approval(None));
                };
                let Some(t) = self.app.catalog.get(&tool) else {
                    return Ok(Prepared::Approval(None));
                };
                if t.validate(&args).is_err() {
                    return Ok(Prepared::Approval(None));
                }
                let effect = t.effect(&args);
                let now = OffsetDateTime::now_utc();
                let listed = gate::list(
                    &self.app.store,
                    &self.app.policy,
                    who,
                    &[(tool.clone(), effect)],
                    now,
                )
                .await;
                let decision = listed
                    .into_iter()
                    .find(|(id, _)| *id == tool)
                    .map(|(_, d)| d);
                Prepared::Approval(decision.map(|d| (d, tool, args)))
            }
            "content.completed" => match assistant_text(&f.data) {
                Some(text) => Prepared::Guarded(screen(self.guard.as_ref(), text).await),
                None => Prepared::Nothing,
            },
            _ => Prepared::Nothing,
        })
    }

    /// A core tool call eve asked for: from this relay's memory, else from the store.
    async fn call(
        &self,
        who: &Assertion,
        id: Uuid,
        call: &str,
        calls: &mut HashMap<String, (ToolId, Value)>,
    ) -> Result<Option<(ToolId, Value)>, ConversationError> {
        if let Some(c) = calls.get(call) {
            return Ok(Some(c.clone()));
        }
        let mut tx = self.app.store.begin(&who.tenant).await?;
        let row: Option<(String, sqlx::types::Json<Value>)> = sqlx::query_as(
            "select tool, args from conversation_calls where tenant_id = current_setting('mandate.tenant', true) \
             and conversation = $1 and call_id = $2",
        )
        .bind(id)
        .bind(call)
        .fetch_optional(&mut *tx)
        .await?;
        let Some((tool, args)) = row else {
            return Ok(None);
        };
        let c = (ToolId::parse(&tool).map_err(StoreError::from)?, args.0);
        calls.insert(call.to_owned(), c.clone());
        Ok(Some(c))
    }
}

/// A finished, uninterrupted text part the agent wrote for the person: what the guard checks.
fn assistant_text(d: &Value) -> Option<&str> {
    let ok = d["kind"] == "text"
        && (d["phase"] == "reply" || d["phase"] == "narration")
        && d.get("interrupted").is_none();
    d["value"].as_str().filter(|t| ok && !t.trim().is_empty())
}

enum Prepared {
    Nothing,
    Call(Option<(ToolId, Value)>),
    /// `None`: refused, unknown or invalid; the prompt is denied.
    Approval(Option<(Decision, ToolId, Value)>),
    /// The guard's answer for an assistant message; `Err` names why it was withheld.
    Guarded(Result<(), &'static str>),
}

async fn apply(
    tx: &mut Tx,
    id: Uuid,
    who: &Assertion,
    f: &Fact,
    p: Prepared,
    answers: &mut Vec<(String, bool)>,
) -> Result<(), ConversationError> {
    let d = &f.data;
    match (f.kind.as_str(), p) {
        ("delivery.admitted" | "turn.started", _) => set_state(tx, id, "working").await?,
        ("turn.paused", _) => set_state(tx, id, "waiting").await?,
        ("delivery.consumed", _) => {
            let text: Vec<&str> = d["parts"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|p| p["kind"] == "text")
                .filter_map(|p| p["text"].as_str())
                .collect();
            if !text.is_empty() {
                let ev = json!({ "role": "user", "text": text.join("\n") });
                push(tx, id, "message", ev).await?;
            }
        }
        ("content.completed", Prepared::Guarded(verdict)) => {
            let Some(text) = assistant_text(d) else {
                return Ok(());
            };
            let shown = match verdict {
                Ok(()) => text,
                Err(reason) => {
                    withheld(tx, who, text, reason).await?;
                    NOTICE
                }
            };
            push(
                tx,
                id,
                "message",
                json!({ "role": "assistant", "text": shown }),
            )
            .await?;
        }
        (
            "call.requested" | "call.started" | "call.settled",
            Prepared::Call(Some((tool, args))),
        ) => {
            if f.kind == "call.requested" {
                let call = d["callId"].as_str().unwrap_or_default();
                remember(tx, id, call, &tool, &args).await?;
            }
            let status = match (f.kind.as_str(), d["outcome"].as_str()) {
                ("call.requested", _) => "requested",
                ("call.started", _) => "running",
                (_, Some("completed")) => "completed",
                (_, Some("rejected")) => "denied",
                _ => "failed",
            };
            push(tx, id, "tool", json!({ "name": tool, "status": status })).await?;
        }
        ("interaction.opened", Prepared::Approval(decision)) => {
            let Some(interaction) = d["interactionId"].as_str().filter(|i| valid_eve_id(i)) else {
                return Ok(());
            };
            match decision {
                Some((Decision::Ask, tool, args)) => {
                    let approval = ask(tx, who, &tool, &args).await?;
                    link(tx, approval, id, interaction).await?;
                    let ev = json!({ "approval": approval_json(tx, approval).await? });
                    push(tx, id, "approval_wait", ev).await?;
                }
                // The core lets it run unattended; its MCP gate decides again when eve calls.
                Some((Decision::Run, ..)) => answers.push((interaction.to_owned(), true)),
                _ => answers.push((interaction.to_owned(), false)),
            }
        }
        ("interaction.settled", _) => {
            let interaction = d["interactionId"].as_str().unwrap_or_default();
            if let Some(approval) = linked(tx, id, interaction).await? {
                let state = match d["outcome"].as_str() {
                    Some("accepted") => "approved",
                    Some("declined") => "denied",
                    _ => "expired",
                };
                let ev = json!({ "approval_id": approval.to_string(), "state": state });
                push(tx, id, "approval_done", ev).await?;
            }
        }
        ("turn.settled", _) => {
            if d["outcome"] == "failed" {
                let ev = json!({ "code": "internal", "message": "The agent could not finish." });
                push(tx, id, "error", ev).await?;
            }
            push(tx, id, "done", json!({})).await?;
            set_state(tx, id, "idle").await?;
        }
        _ => {}
    }
    Ok(())
}

async fn remember(
    tx: &mut Tx,
    id: Uuid,
    call: &str,
    tool: &ToolId,
    args: &Value,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "insert into conversation_calls (tenant_id, conversation, call_id, tool, args) \
         values (current_setting('mandate.tenant'), $1, $2, $3, $4) on conflict do nothing",
    )
    .bind(id)
    .bind(call)
    .bind(tool.as_str())
    .bind(sqlx::types::Json(args))
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// Audits a withheld message by its hash only: the text itself is never stored.
async fn withheld(
    tx: &mut Tx,
    who: &Assertion,
    text: &str,
    reason: &str,
) -> Result<(), StoreError> {
    let entry = AuditEntry {
        agent: who.agent.clone(),
        member: who.member.clone(),
        tool: ToolId::parse("guard.output")?,
        args_hash: args_hash(&Value::String(text.to_owned())),
        decision: "refuse".into(),
        reason: Some(reason.to_owned()),
        approval_id: None,
        approved_by: None,
    };
    store::audit(tx, &entry).await
}

fn valid_eve_id(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 128
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

/// The approval the core's gate would ask for, created now so the owner's inbox shows it at once.
async fn ask(
    tx: &mut Tx,
    who: &Assertion,
    tool: &ToolId,
    args: &Value,
) -> Result<Uuid, StoreError> {
    let hash = args_hash(args);
    let now = OffsetDateTime::now_utc();
    let approval =
        store::request_approval(tx, &who.agent, &who.member, tool, args, &hash, now).await?;
    store::audit(
        tx,
        &AuditEntry {
            agent: who.agent.clone(),
            member: who.member.clone(),
            tool: tool.clone(),
            args_hash: hash,
            decision: "ask".into(),
            reason: None,
            approval_id: None,
            approved_by: None,
        },
    )
    .await?;
    Ok(approval)
}

/// ponytail: a subset of lane C's `Approval` shape; replace with its serializer when it merges.
async fn approval_json(tx: &mut Tx, approval: Uuid) -> Result<Value, sqlx::Error> {
    let q = format!(
        "select json_build_object('id', id, 'agent', json_build_object('id', agent), 'tool', tool, \
           'args', args, 'requested_by', json_build_object('id', requested_by), \
           'expires_at', to_char(expires_at at time zone 'utc', {TS}), 'state', 'pending', \
           'can_decide', true) \
         from approvals where tenant_id = current_setting('mandate.tenant', true) and id = $1"
    );
    sqlx::query_scalar(sqlx::AssertSqlSafe(q))
        .bind(approval)
        .fetch_one(&mut **tx)
        .await
}

async fn link(tx: &mut Tx, approval: Uuid, id: Uuid, interaction: &str) -> Result<(), sqlx::Error> {
    sqlx::query(
        "insert into conversation_approvals (tenant_id, approval, conversation, interaction) \
         values (current_setting('mandate.tenant'), $1, $2, $3) on conflict do nothing",
    )
    .bind(approval)
    .bind(id)
    .bind(interaction)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn linked(tx: &mut Tx, id: Uuid, interaction: &str) -> Result<Option<Uuid>, sqlx::Error> {
    sqlx::query_scalar(
        "select approval from conversation_approvals where tenant_id = current_setting('mandate.tenant', true) \
         and conversation = $1 and interaction = $2",
    )
    .bind(id)
    .bind(interaction)
    .fetch_optional(&mut **tx)
    .await
}

/// Locks the conversation row; returns its cursor, `None` if it is not in this tenant.
async fn lock(tx: &mut Tx, id: Uuid) -> Result<Option<i64>, sqlx::Error> {
    sqlx::query_scalar(
        "select eve_cursor from conversations where tenant_id = current_setting('mandate.tenant', true) \
         and id = $1 for update",
    )
    .bind(id)
    .fetch_optional(&mut **tx)
    .await
}

async fn set_state(tx: &mut Tx, id: Uuid, state: &str) -> Result<(), sqlx::Error> {
    sqlx::query(
        "update conversations set state = $2, updated_at = now() \
         where tenant_id = current_setting('mandate.tenant', true) and id = $1",
    )
    .bind(id)
    .bind(state)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// Appends one browser event; the caller holds the conversation's row lock, so `seq` is unique.
async fn push(tx: &mut Tx, id: Uuid, kind: &str, data: Value) -> Result<(), sqlx::Error> {
    sqlx::query(
        "insert into conversation_events (tenant_id, conversation, seq, kind, data) \
         select current_setting('mandate.tenant'), $1, coalesce(max(seq), 0) + 1, $2, $3 \
         from conversation_events where tenant_id = current_setting('mandate.tenant', true) and conversation = $1",
    )
    .bind(id)
    .bind(kind)
    .bind(sqlx::types::Json(data))
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// The conversation, if it is in this tenant and (when given) belongs to `member`.
async fn load(
    store: &store::Store,
    tenant: &TenantId,
    id: Uuid,
    member: Option<&MemberId>,
) -> Result<Option<Conv>, StoreError> {
    let mut tx = store.begin(tenant).await?;
    let row: Option<(String, String, String, i64)> = sqlx::query_as(
        "select member, agent, eve_session, eve_cursor from conversations \
         where tenant_id = current_setting('mandate.tenant', true) and id = $1 \
         and ($2::text is null or member = $2)",
    )
    .bind(id)
    .bind(member.map(MemberId::as_str))
    .fetch_optional(&mut *tx)
    .await?;
    let Some((m, a, e, cursor)) = row else {
        return Ok(None);
    };
    Ok(Some(Conv {
        member: MemberId::parse(&m)?,
        agent: AgentId::parse(&a)?,
        // A stored eve id that no longer parses is treated like a missing conversation.
        eve: match EveSessionId::parse(&e) {
            Ok(e) => e,
            Err(_) => return Ok(None),
        },
        cursor,
    }))
}

async fn conversation_json(tx: &mut Tx, id: Uuid) -> Result<Value, sqlx::Error> {
    let q = format!(
        "select json_build_object('id', id, 'title', title, 'state', state, \
           'created_at', to_char(created_at at time zone 'utc', {TS}), \
           'updated_at', to_char(updated_at at time zone 'utc', {TS})) \
         from conversations where tenant_id = current_setting('mandate.tenant', true) and id = $1"
    );
    sqlx::query_scalar(sqlx::AssertSqlSafe(q))
        .bind(id)
        .fetch_one(&mut **tx)
        .await
}

#[derive(Deserialize)]
struct MessageBody {
    message: String,
}

fn message(b: &MessageBody) -> Result<&str, ApiError> {
    let m = b.message.trim();
    if m.is_empty() || m.len() > MAX_MESSAGE_BYTES {
        return Err(ApiError(
            StatusCode::BAD_REQUEST,
            "invalid_input",
            "A message must be 1 to 16 KiB of text.",
        ));
    }
    Ok(m)
}

fn conversation_id(raw: &str) -> Result<Uuid, ApiError> {
    Uuid::parse_str(raw).map_err(|_| NOT_FOUND)
}

async fn start(
    State(s): State<Arc<Conversations>>,
    session: Session,
    Json(body): Json<MessageBody>,
) -> Result<Response, ApiError> {
    let text = message(&body)?;
    let store = &s.app.store;
    let mut tx = store.begin(&session.tenant).await.map_err(internal)?;
    let agent: Option<String> = sqlx::query_scalar(
        "select id from agents where tenant_id = current_setting('mandate.tenant', true) \
         and kind = 'personal' and owner_member = $1 and not disabled order by id limit 1",
    )
    .bind(session.member.as_str())
    .fetch_optional(&mut *tx)
    .await
    .map_err(internal)?;
    tx.commit().await.map_err(internal)?;
    let agent = agent
        .map(|a| AgentId::parse(&a))
        .transpose()
        .map_err(internal)?
        .ok_or(ApiError(
            StatusCode::FORBIDDEN,
            "no_access",
            "You have no agent yet.",
        ))?;

    let token = sign(
        &s.who(&session.tenant, &session.member, &agent),
        &s.app.assertion_signing_key,
    );
    let eve = s.eve.start(&token, text).await.map_err(eve_down)?;
    let id = Uuid::new_v4();
    let title: String = text.chars().take(80).collect();
    let mut tx = store.begin(&session.tenant).await.map_err(internal)?;
    sqlx::query(
        "insert into conversations (tenant_id, id, member, agent, eve_session, title, state) \
         values (current_setting('mandate.tenant'), $1, $2, $3, $4, $5, 'working')",
    )
    .bind(id)
    .bind(session.member.as_str())
    .bind(agent.as_str())
    .bind(eve.as_str())
    .bind(&title)
    .execute(&mut *tx)
    .await
    .map_err(internal)?;
    let json = conversation_json(&mut tx, id).await.map_err(internal)?;
    tx.commit().await.map_err(internal)?;
    s.kick(&session.tenant, id).await;
    Ok((StatusCode::CREATED, Json(json)).into_response())
}

#[derive(Deserialize)]
struct ListQuery {
    cursor: Option<String>,
}

async fn list(
    State(s): State<Arc<Conversations>>,
    session: Session,
    Query(q): Query<ListQuery>,
) -> Result<Json<Value>, ApiError> {
    let cursor = q
        .cursor
        .map(|c| Uuid::parse_str(&c))
        .transpose()
        .map_err(|_| ApiError(StatusCode::BAD_REQUEST, "invalid_input", "Bad cursor."))?;
    let mut tx = s.app.store.begin(&session.tenant).await.map_err(internal)?;
    let ids: Vec<Uuid> = sqlx::query_scalar(
        "select id from conversations c where tenant_id = current_setting('mandate.tenant', true) \
         and member = $1 and ($2::uuid is null or (created_at, id) < ( \
           select created_at, id from conversations where tenant_id = current_setting('mandate.tenant', true) \
           and member = $1 and id = $2)) \
         order by created_at desc, id desc limit $3",
    )
    .bind(session.member.as_str())
    .bind(cursor)
    .bind(PAGE + 1)
    .fetch_all(&mut *tx)
    .await
    .map_err(internal)?;
    let more = ids.len() as i64 > PAGE;
    let mut items = Vec::new();
    for id in ids.iter().take(PAGE as usize) {
        items.push(conversation_json(&mut tx, *id).await.map_err(internal)?);
    }
    let next = more.then(|| ids[PAGE as usize - 1].to_string());
    Ok(Json(json!({ "items": items, "next": next })))
}

async fn send(
    State(s): State<Arc<Conversations>>,
    session: Session,
    Path(raw): Path<String>,
    Json(body): Json<MessageBody>,
) -> Result<StatusCode, ApiError> {
    let text = message(&body)?;
    let id = conversation_id(&raw)?;
    let c = load(&s.app.store, &session.tenant, id, Some(&session.member))
        .await
        .map_err(internal)?
        .ok_or(NOT_FOUND)?;
    s.eve
        .send(&s.token(&session.tenant, &c), &c.eve, text)
        .await
        .map_err(eve_down)?;
    working(&s, &session.tenant, id).await?;
    Ok(StatusCode::ACCEPTED)
}

async fn cancel(
    State(s): State<Arc<Conversations>>,
    session: Session,
    Path(raw): Path<String>,
) -> Result<StatusCode, ApiError> {
    let id = conversation_id(&raw)?;
    let c = load(&s.app.store, &session.tenant, id, Some(&session.member))
        .await
        .map_err(internal)?
        .ok_or(NOT_FOUND)?;
    s.eve
        .cancel(&s.token(&session.tenant, &c), &c.eve)
        .await
        .map_err(eve_down)?;
    working(&s, &session.tenant, id).await?;
    Ok(StatusCode::ACCEPTED)
}

/// Marks the conversation working, then makes sure a relay picks up what eve does next.
async fn working(s: &Arc<Conversations>, tenant: &TenantId, id: Uuid) -> Result<(), ApiError> {
    let mut tx = s.app.store.begin(tenant).await.map_err(internal)?;
    set_state(&mut tx, id, "working").await.map_err(internal)?;
    tx.commit().await.map_err(internal)?;
    s.kick(tenant, id).await;
    Ok(())
}

/// Server-sent events from the stored, guarded log: an id is a `seq`, so a reconnect with
/// `Last-Event-ID` resumes exactly after the last event the browser saw.
async fn events(
    State(s): State<Arc<Conversations>>,
    session: Session,
    Path(raw): Path<String>,
    headers: HeaderMap,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, ApiError> {
    let id = conversation_id(&raw)?;
    load(&s.app.store, &session.tenant, id, Some(&session.member))
        .await
        .map_err(internal)?
        .ok_or(NOT_FOUND)?;
    let after: i64 = match headers.get("last-event-id") {
        None => 0,
        Some(v) => v
            .to_str()
            .ok()
            .and_then(|v| v.trim().parse::<i64>().ok())
            .filter(|n| *n >= 0)
            .ok_or(ApiError(
                StatusCode::BAD_REQUEST,
                "invalid_input",
                "Bad Last-Event-ID.",
            ))?,
    };
    let deadline = tokio::time::Instant::now() + STREAM_LIFETIME;
    let init = (
        s,
        session.tenant,
        id,
        after,
        VecDeque::<(i64, String, Value)>::new(),
    );
    let stream = futures::stream::unfold(
        init,
        move |(s, tenant, id, mut after, mut buf)| async move {
            loop {
                if let Some((seq, kind, data)) = buf.pop_front() {
                    after = seq;
                    let ev = Event::default()
                        .id(seq.to_string())
                        .event(kind)
                        .data(data.to_string());
                    return Some((Ok(ev), (s, tenant, id, after, buf)));
                }
                if tokio::time::Instant::now() >= deadline {
                    return None;
                }
                match read_events(&s.app.store, &tenant, id, after).await {
                    Ok(rows) if !rows.is_empty() => buf.extend(rows),
                    Ok(_) => tokio::time::sleep(s.poll).await,
                    Err(e) => {
                        tracing::warn!(error = %e, "reading conversation events failed");
                        return None;
                    }
                }
            }
        },
    );
    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

// ponytail: each open stream polls Postgres every `poll`; LISTEN/NOTIFY when that costs too much.
async fn read_events(
    store: &store::Store,
    tenant: &TenantId,
    id: Uuid,
    after: i64,
) -> Result<Vec<(i64, String, Value)>, StoreError> {
    let mut tx = store.begin(tenant).await?;
    let rows: Vec<(i64, String, sqlx::types::Json<Value>)> = sqlx::query_as(
        "select seq, kind, data from conversation_events \
         where tenant_id = current_setting('mandate.tenant', true) and conversation = $1 and seq > $2 \
         order by seq limit 100",
    )
    .bind(id)
    .bind(after)
    .fetch_all(&mut *tx)
    .await?;
    Ok(rows.into_iter().map(|(s, k, d)| (s, k, d.0)).collect())
}

/// `/v1/conversations…`, mounted outside the MCP router's 60-second timeout: event streams last.
pub fn router(state: Arc<Conversations>) -> Router {
    Router::new()
        .route("/v1/conversations", get(list).post(start))
        .route("/v1/conversations/{id}/messages", post(send))
        .route("/v1/conversations/{id}/events", get(events))
        .route("/v1/conversations/{id}/cancel", post(cancel))
        .layer(RequestBodyLimitLayer::new(MAX_BODY_BYTES))
        .with_state(state)
}
