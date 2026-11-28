//! Your agent in the web app. The core starts eve turns for the signed-in member's personal
//! operator, relays eve's events to the browser as server-sent events, and resumes a turn when
//! its approval is decided. eve is untrusted: its stream is translated, never passed through.
use crate::api::session::{self, Session};
use crate::api::{ApiError, ApiState, Input, approvals, ratelimit};
use crate::assertion::{Assertion, TTL_SECS, sign};
use crate::eve::{EveClient, EveError, EveSessionId, Fact, Record};
use crate::events::{self, Decision as Decided, Envelope, Handler, HandlerError, Source};
use crate::gate::{self, args_hash};
use crate::guard::{Guard, NOTICE, screen};
use crate::ids::{AgentId, MemberId, TenantId, ToolId};
use crate::mcp::AppState;
use crate::policy::Decision;
use crate::store::{self, AuditEntry, StoreError, Tx};
use async_trait::async_trait;
use axum::extract::rejection::QueryRejection;
use axum::extract::{Path, Query, State};
use axum::response::sse::{self, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Extension, Json, Router};
use futures::Stream;
use http::{HeaderMap, StatusCode};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::{HashMap, VecDeque};
use std::convert::Infallible;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;
use time::OffsetDateTime;
use uuid::Uuid;

const MAX_MESSAGE_BYTES: usize = 16 * 1024;
const PAGE: i64 = 50;
// A browser stream ends after this long; EventSource reconnects with Last-Event-ID.
const STREAM_LIFETIME: Duration = Duration::from_secs(5 * 60);
// Each open stream polls Postgres, so their number is bounded per member and per process.
const MAX_STREAMS_PER_MEMBER: usize = 5;
const MAX_STREAMS: usize = 500;
// A stream re-checks its session every this many polls, so signing out ends it.
const SESSION_RECHECK_POLLS: u32 = 30;
// Each start opens an eve session and each message runs a turn, so both are limited per member per minute.
const STARTS_PER_MINUTE: i32 = 10;
const SENDS_PER_MINUTE: i32 = 30;
const MAX_RELAY_FAILURES: u32 = 5;
// eve prefixes connection tools with the connection's file name (`agents/agent/connections/mandate.ts`).
const TOOL_PREFIX: &str = "mandate__";
const TS: &str = r#"'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"'"#;

// 503, not 502: Cloudflare replaces an origin 502 with its own page, which drops the CORS headers.
const EVE_DOWN: ApiError = ApiError::new(
    StatusCode::SERVICE_UNAVAILABLE,
    "internal",
    "The agent is unavailable.",
);
const NO_AGENT: ApiError =
    ApiError::new(StatusCode::FORBIDDEN, "no_access", "You have no agent yet.");
const TOO_MANY_STREAMS: ApiError = ApiError::new(
    StatusCode::TOO_MANY_REQUESTS,
    "rate_limited",
    "Too many open conversations. Close one and try again.",
);

const ENDED: ApiError = ApiError::conflict("This conversation has ended.");
const NOT_STARTED: ApiError = ApiError::conflict("This conversation could not start.");
const WAITING: ApiError = ApiError::conflict("Waiting for an OK first.");

/// A session eve says is over: 409. Anything else (a busy turn's 409, a transport error, an eve
/// fault) may pass: 503.
fn eve_down(e: EveError) -> ApiError {
    tracing::warn!(error = %e, "eve call failed");
    match e {
        EveError::Gone => ENDED,
        _ => EVE_DOWN,
    }
}

/// How `give_up` leaves a conversation.
#[derive(Clone, Copy, PartialEq, Eq)]
enum GiveUp {
    /// eve never started it: `failed` and finished.
    NeverStarted,
    /// eve could not be reached or failed: idle, unless a turn waits on an approval.
    Unavailable,
    /// eve says the session is over: idle and finished.
    Ended,
}

#[derive(Debug, thiserror::Error)]
pub enum ConversationError {
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Db(#[from] sqlx::Error),
    #[error(transparent)]
    Eve(#[from] EveError),
    #[error("the approval could not be shown")]
    View,
}

/// A plain lock: no one holds it across an await, and a panic elsewhere does not poison the count.
fn locked<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

type PumpKey = (TenantId, Uuid);
type StreamKey = (TenantId, MemberId);

pub struct Conversations {
    pub app: Arc<AppState>,
    /// The web API's state: an `approval_wait` carries the inbox's own view of the approval.
    pub api: Arc<ApiState>,
    pub eve: EveClient,
    pub guard: Arc<dyn Guard>,
    /// Pause between relay reconnects and between browser polls.
    pub poll: Duration,
    /// Conversations with a relay running in this process, each with "kicked again since it last looked".
    pumps: Mutex<HashMap<PumpKey, bool>>,
    /// Open browser streams: the total, and per member.
    streams: Mutex<(usize, HashMap<StreamKey, usize>)>,
}

/// What the core keeps about one conversation.
struct Conv {
    member: MemberId,
    agent: AgentId,
    eve: EveSessionId,
    cursor: i64,
}

/// Removes a relay's `pumps` entry if the relay ends without doing so (a panic or an abort).
struct PumpEntry {
    conv: Arc<Conversations>,
    key: PumpKey,
    armed: bool,
}

impl Drop for PumpEntry {
    fn drop(&mut self) {
        if self.armed {
            locked(&self.conv.pumps).remove(&self.key);
        }
    }
}

/// One open browser stream, counted until it is dropped: when it ends or the browser leaves.
struct StreamSlot {
    conv: Arc<Conversations>,
    key: StreamKey,
}

impl Drop for StreamSlot {
    fn drop(&mut self) {
        let mut s = locked(&self.conv.streams);
        s.0 = s.0.saturating_sub(1);
        if let Some(n) = s.1.get_mut(&self.key) {
            *n = n.saturating_sub(1);
            if *n == 0 {
                s.1.remove(&self.key);
            }
        }
    }
}

impl Conversations {
    pub fn new(
        app: Arc<AppState>,
        api: Arc<ApiState>,
        eve: EveClient,
        guard: Arc<dyn Guard>,
        poll: Duration,
    ) -> Arc<Self> {
        Arc::new(Self {
            app,
            api,
            eve,
            guard,
            poll,
            pumps: Mutex::new(HashMap::new()),
            streams: Mutex::new((0, HashMap::new())),
        })
    }

    fn who(&self, tenant: &TenantId, member: &MemberId, agent: &AgentId) -> Assertion {
        Assertion {
            tenant: tenant.clone(),
            member: member.clone(),
            agent: agent.clone(),
            space: None,
            principal: None,
            conversation: None,
            thread: None,
            body: None,
            purpose: None,
            expires_at: OffsetDateTime::now_utc().unix_timestamp() + TTL_SECS,
        }
    }

    /// A fresh assertion for every eve call: a waiting turn may resume long after the last one expired.
    // ponytail: one assertion lasts TTL_SECS; a single eve turn longer than that loses its tool access.
    fn token(&self, tenant: &TenantId, c: &Conv) -> String {
        sign(
            &self.who(tenant, &c.member, &c.agent),
            &self.app.assertion_signing_key,
        )
    }

    fn open_stream(
        self: &Arc<Self>,
        tenant: &TenantId,
        member: &MemberId,
    ) -> Result<StreamSlot, ApiError> {
        let key = (tenant.clone(), member.clone());
        let mut s = locked(&self.streams);
        let mine = s.1.get(&key).copied().unwrap_or(0);
        if mine >= MAX_STREAMS_PER_MEMBER || s.0 >= MAX_STREAMS {
            return Err(TOO_MANY_STREAMS);
        }
        s.0 += 1;
        s.1.insert(key.clone(), mine + 1);
        Ok(StreamSlot {
            conv: self.clone(),
            key,
        })
    }

    /// Answers every eve prompt waiting on `approval` and relays what follows, if the store still
    /// shows the decision the event names. An eve error leaves the event for redelivery.
    // ponytail: an approval that expires undecided resumes nothing; its turn waits until cancelled.
    async fn resume(
        self: &Arc<Self>,
        tx: &mut Tx,
        tenant: &TenantId,
        approval: Uuid,
        decision: Decided,
    ) -> Result<(), HandlerError> {
        // The recorded decision, not the derived state: one handled after expiry or after its use
        // still resumes. The consumer's inbox, in this transaction, keeps a redelivery from resuming twice.
        match approvals::load(tx, approval, OffsetDateTime::now_utc()).await? {
            Some(a) if a.decision == Some(decision) => {}
            _ => {
                tracing::warn!(%approval, "approval is not in the decided state; nothing resumed");
                return Ok(());
            }
        }
        let links: Vec<(Uuid, String)> = sqlx::query_as(
            "select conversation, interaction from conversation_approvals \
             where tenant_id = current_setting('mandate.tenant', true) and approval = $1 \
             order by conversation, interaction",
        )
        .bind(approval)
        .fetch_all(&mut **tx)
        .await
        .map_err(StoreError::from)?;
        for (id, interaction) in links {
            let Some(c) = load(&self.app.store, tenant, id, None).await? else {
                continue;
            };
            // eve only runs the call; the core's MCP gate spends the approval, or asks again.
            // ponytail: this runs inside the consumer's inbox transaction, holding a connection for up
            // to ~15 s per link (and the outbox lock in in-process mode); answer after commit if that bites.
            let answered = self
                .eve
                .answer(
                    &self.token(tenant, &c),
                    &c.eve,
                    &interaction,
                    decision == Decided::Approved,
                )
                .await;
            match answered {
                Ok(()) => working(self, tenant, id).await?,
                // Only a transport error or an eve fault can go better on redelivery.
                Err(e @ EveError::Request(_)) => return Err(HandlerError::Retry(e.to_string())),
                Err(EveError::Status(code)) if code >= 500 => {
                    return Err(HandlerError::Retry(format!("eve answered {code}")));
                }
                // A cancelled turn, an ended session or a refused answer never changes: skip it.
                Err(e) => {
                    tracing::warn!(error = %e, conversation = %id, "eve refused a resume; skipped")
                }
            }
        }
        Ok(())
    }

    /// Hands eve the member's message logged at `seq`, keyed by it, so a transport failure is
    /// retried once without running it twice. On failure the browser is told; if eve may have
    /// taken it anyway, the relay still runs to show what it does.
    async fn deliver(
        self: &Arc<Self>,
        tenant: &TenantId,
        id: Uuid,
        eve: &EveSessionId,
        token: &str,
        text: &str,
        seq: i64,
    ) -> Result<(), ApiError> {
        let key = format!("{id}:{seq}");
        let mut sent = self.eve.send(token, eve, text, &key).await;
        if matches!(sent, Err(EveError::Request(_))) {
            sent = self.eve.send(token, eve, text, &key).await;
        }
        match sent {
            Ok(()) => {
                self.kick(tenant, id);
                Ok(())
            }
            Err(e) => {
                let how = match e {
                    EveError::Gone => GiveUp::Ended,
                    _ => GiveUp::Unavailable,
                };
                self.give_up(tenant, id, how).await?;
                if matches!(e, EveError::Request(_)) {
                    self.kick(tenant, id);
                }
                Err(eve_down(e))
            }
        }
    }

    /// Starts a relay for this conversation unless one already runs here; that one looks again.
    pub fn kick(self: &Arc<Self>, tenant: &TenantId, id: Uuid) {
        let key = (tenant.clone(), id);
        let mut pumps = locked(&self.pumps);
        match pumps.get_mut(&key) {
            Some(again) => *again = true,
            None => {
                pumps.insert(key.clone(), false);
                tokio::spawn(self.clone().pump(key));
            }
        }
    }

    // ponytail: a conversation left `working` by a restart waits for its next kick (a message,
    // cancel or decision); re-kick those at startup if that is too slow.
    async fn pump(self: Arc<Self>, key: PumpKey) {
        let mut entry = PumpEntry {
            conv: self.clone(),
            key: key.clone(),
            armed: true,
        };
        let (tenant, id) = (&key.0, key.1);
        let mut calls = HashMap::new();
        let mut failures = 0;
        loop {
            let ended = match self.relay(tenant, id, &mut calls).await {
                Ok(ended) => {
                    failures = 0;
                    ended
                }
                Err(e) => {
                    failures += 1;
                    tracing::warn!(error = %e, conversation = %id, failures, "relay failed");
                    if failures >= MAX_RELAY_FAILURES {
                        let _ = self.give_up(tenant, id, GiveUp::Unavailable).await;
                        true
                    } else {
                        false
                    }
                }
            };
            let working =
                !ended && matches!(self.state(tenant, id).await, Ok(Some(s)) if s == "working");
            {
                let mut pumps = locked(&self.pumps);
                // A kick since the last look may mean new work: look once more before stopping.
                let again = pumps.get_mut(&key).is_some_and(std::mem::take);
                if !working && !again {
                    pumps.remove(&key);
                    entry.armed = false;
                    return;
                }
            }
            if working {
                tokio::time::sleep(self.poll * (1 + failures)).await;
            }
        }
    }

    async fn state(&self, tenant: &TenantId, id: Uuid) -> Result<Option<String>, StoreError> {
        let mut tx = self.app.store.begin(tenant).await?;
        Ok(state_in(&mut tx, id).await?)
    }

    /// Tells the browser why the agent stopped and records it (see `GiveUp`). A turn `waiting`
    /// on an approval gets no error: a transient failure leaves it waiting for its decision.
    async fn give_up(&self, tenant: &TenantId, id: Uuid, how: GiveUp) -> Result<(), StoreError> {
        let mut tx = self.app.store.begin(tenant).await?;
        lock(&mut tx, id).await?;
        let waiting = state_in(&mut tx, id).await?.as_deref() == Some("waiting");
        if waiting && how == GiveUp::Unavailable {
            return Ok(());
        }
        if !waiting {
            let message = match how {
                GiveUp::Ended => "This conversation has ended.",
                _ => "The agent is unavailable.",
            };
            let ev = json!({ "code": "internal", "message": message });
            push(&mut tx, id, "error", ev).await?;
        }
        let state = match how {
            GiveUp::NeverStarted => "failed",
            _ => "idle",
        };
        set_state(&mut tx, id, state).await?;
        if how != GiveUp::Unavailable {
            finish(&mut tx, id).await?;
        }
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
                    finish(&mut tx, id).await?;
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
            apply(&mut tx, &self.api, id, &who, f, p, &mut answers).await?;
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
                let Some(call) = call_id(&f.data["callId"]) else {
                    return Ok(Prepared::Call(None));
                };
                // Only a tool the core serves; anything else eve names is not shown.
                if let Some(tool) = name
                    .strip_prefix(TOOL_PREFIX)
                    .and_then(|t| ToolId::parse(t).ok())
                    .filter(|t| self.app.catalog.get(t).is_some())
                {
                    let input = f.data.get("input").cloned().unwrap_or(Value::Null);
                    calls.insert(call.to_owned(), (tool, input));
                }
                Prepared::Call(calls.get(call).cloned())
            }
            "call.started" | "call.settled" => match call_id(&f.data["callId"]) {
                Some(call) => Prepared::Call(self.call(who, id, call, calls).await?),
                None => Prepared::Call(None),
            },
            "interaction.opened" if f.data["request"]["kind"] == "approval" => {
                let Some(call) = call_id(&f.data["subject"]["callId"]) else {
                    return Ok(Prepared::Approval(None));
                };
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
    api: &ApiState,
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
                    // The owner is who reads this conversation, so `can_decide` is theirs.
                    let viewer = Session {
                        tenant: who.tenant.clone(),
                        member: who.member.clone(),
                        is_admin: false,
                    };
                    let view =
                        approvals::view_one(tx, api, &viewer, approval, OffsetDateTime::now_utc())
                            .await
                            .map_err(|_| ConversationError::View)?
                            .ok_or(ConversationError::View)?;
                    push(tx, id, "approval_wait", json!({ "approval": view })).await?;
                }
                // The core lets it run unattended; its MCP gate decides again when eve calls.
                Some((Decision::Run, ..)) => answers.push((interaction.to_owned(), true)),
                _ => answers.push((interaction.to_owned(), false)),
            }
        }
        ("interaction.settled", _) => {
            let interaction = d["interactionId"].as_str().unwrap_or_default();
            if let Some(approval) = linked(tx, id, interaction).await? {
                // The store's state, not eve's word for it.
                let Some(a) = approvals::load(tx, approval, OffsetDateTime::now_utc()).await?
                else {
                    return Ok(());
                };
                let ev = json!({ "approval_id": approval.to_string(), "state": a.state.as_str() });
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

/// An eve call id, only if it is a well-formed eve id: anything else is never stored or matched.
fn call_id(v: &Value) -> Option<&str> {
    v.as_str().filter(|c| valid_eve_id(c))
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
            // Names the approval it opened, so Activity can show how the ask ended.
            approval_id: Some(approval),
            approved_by: None,
        },
    )
    .await?;
    Ok(approval)
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
    // So the approvals inbox can point at the conversation; the first one to ask keeps it.
    sqlx::query(
        "update approvals set conversation = coalesce(conversation, $2) \
         where tenant_id = current_setting('mandate.tenant', true) and id = $1",
    )
    .bind(approval)
    .bind(id.to_string())
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

async fn state_in(tx: &mut Tx, id: Uuid) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar(
        "select state from conversations where tenant_id = current_setting('mandate.tenant', true) and id = $1",
    )
    .bind(id)
    .fetch_optional(&mut **tx)
    .await
}

/// Marks the conversation finished; `conversation.finished.v1` is published only the first time.
async fn finish(tx: &mut Tx, id: Uuid) -> Result<(), StoreError> {
    let first: Option<Uuid> = sqlx::query_scalar(
        "update conversations set finished_at = now() \
         where tenant_id = current_setting('mandate.tenant', true) and id = $1 and finished_at is null \
         returning id",
    )
    .bind(id)
    .fetch_optional(&mut **tx)
    .await?;
    if first.is_some() {
        events::publish(tx, events::Event::ConversationFinished { conversation: id }).await?;
    }
    Ok(())
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
/// Returns the event's `seq`.
async fn push(tx: &mut Tx, id: Uuid, kind: &str, data: Value) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar(
        "insert into conversation_events (tenant_id, conversation, seq, kind, data) \
         select current_setting('mandate.tenant'), $1, coalesce(max(seq), 0) + 1, $2, $3 \
         from conversation_events where tenant_id = current_setting('mandate.tenant', true) and conversation = $1 \
         returning seq",
    )
    .bind(id)
    .bind(kind)
    .bind(sqlx::types::Json(data))
    .fetch_one(&mut **tx)
    .await
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

/// The caller's own conversation with an eve session: one eve never started or one that has
/// finished is 409, not 404.
async fn mine(store: &store::Store, session: &Session, id: Uuid) -> Result<Conv, ApiError> {
    let mut tx = store.begin(&session.tenant).await?;
    let row: Option<(String, bool)> = sqlx::query_as(
        "select state, finished_at is not null from conversations \
         where tenant_id = current_setting('mandate.tenant', true) and id = $1 and member = $2",
    )
    .bind(id)
    .bind(session.member.as_str())
    .fetch_optional(&mut *tx)
    .await
    .map_err(StoreError::from)?;
    drop(tx);
    match row {
        None => return Err(ApiError::not_found()),
        Some((state, _)) if state == "failed" => return Err(NOT_STARTED),
        Some((_, true)) => return Err(ENDED),
        Some(_) => {}
    }
    load(store, &session.tenant, id, Some(&session.member))
        .await?
        .ok_or(ApiError::not_found())
}

async fn conversation_json(tx: &mut Tx, id: Uuid) -> Result<Value, sqlx::Error> {
    // A wait whose latest ask since the member last wrote expired undecided waits for no one:
    // it reads `expired`. An older turn's ask never counts.
    // ponytail: eve's turn stays paused until the member stops it; a sweep could cancel it.
    let pending = approvals::PENDING;
    let q = format!(
        "select json_build_object('id', c.id, 'title', c.title, 'state', \
           case when c.state = 'waiting' and coalesce(( \
             select {pending} and a.expires_at <= now() from conversation_events e \
             join approvals a on a.tenant_id = e.tenant_id and a.id::text = e.data->'approval'->>'id' \
             where e.tenant_id = c.tenant_id and e.conversation = c.id and e.kind = 'approval_wait' \
             and e.seq > coalesce((select max(u.seq) from conversation_events u \
               where u.tenant_id = c.tenant_id and u.conversation = c.id \
               and u.kind = 'message' and u.data->>'role' = 'user'), 0) \
             order by e.seq desc limit 1), false) then 'expired' else c.state end, \
           'created_at', to_char(c.created_at at time zone 'utc', {TS}), \
           'updated_at', to_char(c.updated_at at time zone 'utc', {TS})) \
         from conversations c where c.tenant_id = current_setting('mandate.tenant', true) and c.id = $1"
    );
    sqlx::query_scalar(sqlx::AssertSqlSafe(q))
        .bind(id)
        .fetch_one(&mut **tx)
        .await
}

/// The `agent-resume` consumer of `approval.decided.v1`: resumes the turns waiting on it.
pub struct Resume(pub Arc<Conversations>);

#[async_trait]
impl Handler for Resume {
    async fn handle(&self, tx: &mut Tx, env: &Envelope) -> Result<(), HandlerError> {
        // Only the core decides approvals: an app source's event resumes nothing.
        if env.source != Source::core() {
            return Ok(());
        }
        // Every other event, and one whose data does not decode, resumes nothing.
        let Ok(events::Event::ApprovalDecided {
            approval, decision, ..
        }) = events::Event::from_envelope(env)
        else {
            return Ok(());
        };
        self.0.resume(tx, &env.tenant, approval, decision).await
    }
}

#[derive(Deserialize)]
struct MessageBody {
    message: String,
}

fn message(b: &MessageBody) -> Result<&str, ApiError> {
    let m = b.message.trim();
    if m.is_empty() || m.len() > MAX_MESSAGE_BYTES {
        return Err(ApiError::invalid_input());
    }
    Ok(m)
}

fn conversation_id(raw: &str) -> Result<Uuid, ApiError> {
    Uuid::parse_str(raw).map_err(|_| ApiError::not_found())
}

async fn start(
    State(api): State<Arc<ApiState>>,
    Extension(s): Extension<Arc<Conversations>>,
    session: Session,
    Input(body): Input<MessageBody>,
) -> Result<Response, ApiError> {
    let text = message(&body)?;
    let key = format!("conversation-start:{}:{}", session.tenant, session.member);
    if let Some(secs) = ratelimit::hit(
        &api.store,
        &key,
        STARTS_PER_MINUTE,
        OffsetDateTime::now_utc(),
    )
    .await?
    {
        return Err(ApiError::rate_limited(secs));
    }
    let mut tx = api.store.begin(&session.tenant).await?;
    let agent: Option<String> = sqlx::query_scalar(
        "select id from agents where tenant_id = current_setting('mandate.tenant', true) \
         and kind = 'personal' and owner_member = $1 and not disabled order by id limit 1",
    )
    .bind(session.member.as_str())
    .fetch_optional(&mut *tx)
    .await?;
    let agent = agent
        .map(|a| AgentId::parse(&a))
        .transpose()
        .map_err(StoreError::from)?
        .ok_or(NO_AGENT)?;

    // Row, message and event are committed before eve is called: no transaction waits on eve.
    let id = Uuid::new_v4();
    let title: String = text.chars().take(80).collect();
    sqlx::query(
        "insert into conversations (tenant_id, id, member, agent, eve_session, title, state) \
         values (current_setting('mandate.tenant'), $1, $2, $3, '', $4, 'working')",
    )
    .bind(id)
    .bind(session.member.as_str())
    .bind(agent.as_str())
    .bind(&title)
    .execute(&mut *tx)
    .await?;
    let seq = push(
        &mut tx,
        id,
        "message",
        json!({ "role": "user", "text": text }),
    )
    .await?;
    events::publish(
        &mut tx,
        events::Event::ConversationStarted {
            conversation: id,
            channel: None,
            handoff_from: None,
        },
    )
    .await?;
    tx.commit().await?;

    let token = sign(
        &s.who(&session.tenant, &session.member, &agent),
        &s.app.assertion_signing_key,
    );
    // A session first, with no turn: a create that is lost or repeated leaves at most an idle session.
    let failed = |err: StoreError| {
        tracing::error!(error = %err, conversation = %id, "failed start not recorded; row left working");
    };
    let eve = match s.eve.create(&token, &id.to_string()).await {
        Ok(eve) => eve,
        Err(e) => {
            s.give_up(&session.tenant, id, GiveUp::NeverStarted)
                .await
                .unwrap_or_else(failed);
            tracing::warn!(error = %e, "eve create failed");
            // eve refused the create itself: retrying will not help.
            return Err(match e {
                EveError::Gone | EveError::Status(400..=499) => NOT_STARTED,
                _ => EVE_DOWN,
            });
        }
    };
    // A session eve made but the row never names is never used: the conversation failed to start.
    let recorded = async {
        let mut tx = api.store.begin(&session.tenant).await?;
        sqlx::query(
            "update conversations set eve_session = $2 \
             where tenant_id = current_setting('mandate.tenant', true) and id = $1",
        )
        .bind(id)
        .bind(eve.as_str())
        .execute(&mut *tx)
        .await?;
        let json = conversation_json(&mut tx, id).await?;
        tx.commit().await?;
        Ok::<_, StoreError>(json)
    }
    .await;
    let json = match recorded {
        Ok(json) => json,
        Err(e) => {
            s.give_up(&session.tenant, id, GiveUp::NeverStarted)
                .await
                .unwrap_or_else(failed);
            return Err(e.into());
        }
    };
    s.deliver(&session.tenant, id, &eve, &token, text, seq)
        .await?;
    Ok((StatusCode::CREATED, Json(json)).into_response())
}

#[derive(Deserialize)]
struct ListQuery {
    cursor: Option<String>,
}

async fn list(
    State(api): State<Arc<ApiState>>,
    session: Session,
    q: Result<Query<ListQuery>, QueryRejection>,
) -> Result<Json<Value>, ApiError> {
    let Query(q) = q.map_err(|_| ApiError::invalid_input())?;
    let cursor = q
        .cursor
        .map(|c| Uuid::parse_str(&c))
        .transpose()
        .map_err(|_| ApiError::invalid_input())?;
    let mut tx = api.store.begin(&session.tenant).await?;
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
    .await?;
    let more = ids.len() as i64 > PAGE;
    let mut items = Vec::new();
    for id in ids.iter().take(PAGE as usize) {
        items.push(conversation_json(&mut tx, *id).await?);
    }
    let next = more.then(|| ids[PAGE as usize - 1].to_string());
    Ok(Json(json!({ "items": items, "next": next })))
}

/// One of the caller's own conversations, in the list's shape; a failed one is shown too.
async fn get_one(
    State(api): State<Arc<ApiState>>,
    session: Session,
    Path(raw): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let id = conversation_id(&raw)?;
    let mut tx = api.store.begin(&session.tenant).await?;
    let mine: bool = sqlx::query_scalar(
        "select exists (select 1 from conversations where tenant_id = current_setting('mandate.tenant', true) \
         and id = $1 and member = $2)",
    )
    .bind(id)
    .bind(session.member.as_str())
    .fetch_one(&mut *tx)
    .await?;
    if !mine {
        return Err(ApiError::not_found());
    }
    Ok(Json(conversation_json(&mut tx, id).await?))
}

async fn send(
    State(api): State<Arc<ApiState>>,
    Extension(s): Extension<Arc<Conversations>>,
    session: Session,
    Path(raw): Path<String>,
    Input(body): Input<MessageBody>,
) -> Result<StatusCode, ApiError> {
    let text = message(&body)?;
    let id = conversation_id(&raw)?;
    let c = mine(&api.store, &session, id).await?;
    let key = format!("conversation-send:{}:{}", session.tenant, session.member);
    if let Some(secs) = ratelimit::hit(
        &api.store,
        &key,
        SENDS_PER_MINUTE,
        OffsetDateTime::now_utc(),
    )
    .await?
    {
        return Err(ApiError::rate_limited(secs));
    }
    // The member's own words enter the log in a short transaction; eve is called outside any lock.
    let mut tx = api.store.begin(&session.tenant).await?;
    lock(&mut tx, id).await?;
    // Not logged: a turn waiting on an approval takes no message, and the wait's expiry is read
    // from the asks since the member last wrote.
    if state_in(&mut tx, id).await?.as_deref() == Some("waiting") {
        return Err(WAITING);
    }
    let seq = push(
        &mut tx,
        id,
        "message",
        json!({ "role": "user", "text": text }),
    )
    .await?;
    set_state(&mut tx, id, "working").await?;
    tx.commit().await?;
    let token = s.token(&session.tenant, &c);
    s.deliver(&session.tenant, id, &c.eve, &token, text, seq)
        .await?;
    Ok(StatusCode::ACCEPTED)
}

async fn cancel(
    State(api): State<Arc<ApiState>>,
    Extension(s): Extension<Arc<Conversations>>,
    session: Session,
    Path(raw): Path<String>,
) -> Result<StatusCode, ApiError> {
    let id = conversation_id(&raw)?;
    let c = mine(&api.store, &session, id).await?;
    // Nothing is running: cancelling must not restart the relay or mark the conversation working.
    if !matches!(
        s.state(&session.tenant, id).await?.as_deref(),
        Some("working" | "waiting")
    ) {
        return Ok(StatusCode::ACCEPTED);
    }
    if let Err(e) = s.eve.cancel(&s.token(&session.tenant, &c), &c.eve).await {
        if matches!(e, EveError::Gone) {
            s.give_up(&session.tenant, id, GiveUp::Ended).await?;
        }
        return Err(eve_down(e));
    }
    working(&s, &session.tenant, id).await?;
    Ok(StatusCode::ACCEPTED)
}

/// Marks the conversation working, then makes sure a relay picks up what eve does next.
async fn working(s: &Arc<Conversations>, tenant: &TenantId, id: Uuid) -> Result<(), StoreError> {
    let mut tx = s.app.store.begin(tenant).await?;
    set_state(&mut tx, id, "working").await?;
    tx.commit().await?;
    s.kick(tenant, id);
    Ok(())
}

/// One browser stream's position in the log.
struct Tail {
    s: Arc<Conversations>,
    api: Arc<ApiState>,
    headers: HeaderMap,
    session: Session,
    id: Uuid,
    after: i64,
    buf: VecDeque<(i64, String, Value)>,
    polls: u32,
    _slot: StreamSlot,
}

impl Tail {
    /// The request's session still exists and is the same member's.
    async fn signed_in(&self) -> bool {
        matches!(session::current(&self.api, &self.headers).await, Ok(now) if now == self.session)
    }
}

/// Server-sent events from the stored, guarded log: an id is a `seq`, so a reconnect with
/// `Last-Event-ID` resumes exactly after the last event the browser saw.
async fn events(
    State(api): State<Arc<ApiState>>,
    Extension(s): Extension<Arc<Conversations>>,
    session: Session,
    Path(raw): Path<String>,
    headers: HeaderMap,
) -> Result<Sse<impl Stream<Item = Result<sse::Event, Infallible>>>, ApiError> {
    let id = conversation_id(&raw)?;
    // Ownership only: a conversation eve never started still shows its log.
    let mut tx = api.store.begin(&session.tenant).await?;
    let mine: bool = sqlx::query_scalar(
        "select exists (select 1 from conversations where tenant_id = current_setting('mandate.tenant', true) \
         and id = $1 and member = $2)",
    )
    .bind(id)
    .bind(session.member.as_str())
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;
    if !mine {
        return Err(ApiError::not_found());
    }
    let after: i64 = match headers.get("last-event-id") {
        None => 0,
        Some(v) => v
            .to_str()
            .ok()
            .and_then(|v| v.trim().parse::<i64>().ok())
            .filter(|n| *n >= 0)
            .ok_or(ApiError::invalid_input())?,
    };
    let slot = s.open_stream(&session.tenant, &session.member)?;
    let deadline = tokio::time::Instant::now() + STREAM_LIFETIME;
    let init = Tail {
        s,
        api,
        headers,
        session,
        id,
        after,
        buf: VecDeque::new(),
        polls: 0,
        _slot: slot,
    };
    let stream = futures::stream::unfold(init, move |mut t| async move {
        loop {
            if let Some((seq, kind, data)) = t.buf.pop_front() {
                t.after = seq;
                let ev = sse::Event::default()
                    .id(seq.to_string())
                    .event(kind)
                    .data(data.to_string());
                return Some((Ok(ev), t));
            }
            if tokio::time::Instant::now() >= deadline {
                return None;
            }
            t.polls += 1;
            if t.polls % SESSION_RECHECK_POLLS == 0 && !t.signed_in().await {
                return None;
            }
            match read_events(&t.s.app.store, &t.session.tenant, t.id, t.after).await {
                Ok(rows) if !rows.is_empty() => t.buf.extend(rows),
                Ok(_) => tokio::time::sleep(t.s.poll).await,
                Err(e) => {
                    tracing::warn!(error = %e, "reading conversation events failed");
                    return None;
                }
            }
        }
    });
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

/// `/v1/conversations…`, for `api::with_routes`: its session, CSRF, CORS, body and time limits
/// apply. The timeout bounds only the response head, so an event stream outlives it.
pub fn routes(conv: Arc<Conversations>) -> Router<Arc<ApiState>> {
    Router::new()
        .route("/v1/conversations", get(list).post(start))
        .route("/v1/conversations/{id}", get(get_one))
        .route("/v1/conversations/{id}/messages", post(send))
        .route("/v1/conversations/{id}/events", get(events))
        .route("/v1/conversations/{id}/cancel", post(cancel))
        .layer(Extension(conv))
}
