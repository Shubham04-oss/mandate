# Mandate core slice Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The `mandate` Rust binary that decides, per tool call, whether an agent may run it, must ask, or is refused, and serves the allowed tools (including a browser) over MCP to an untrusted agent brain.

**Architecture:** One Rust crate with a binary. A signed turn assertion says who is asking; Cedar policies plus grants and one-time approvals stored in Postgres decide run / ask / refuse; every decision is audited; an MCP server built on `rmcp` lists only allowed tools and gates every call. Browser tools sit behind a trait with an AgentCore Browser backend.

**Tech Stack:** Rust 2024 edition, tokio, axum 0.8, rmcp 3.11, cedar-policy 4.15, sqlx 0.9 (Postgres), ed25519-dalek 3, workos 3.7 (official SDK), jsonwebtoken 12, aws-sdk-bedrockagentcore 1.88, chromiumoxide 0.9, cedar-policy-symcc (tests only), async-trait, clap, tracing, tracing-subscriber, sha2, base64, url, thiserror, serde, uuid, time.

**Spec:** `docs/design.md` (sections 7, 8.1, 8.4, 8.5) and `docs/security.md`.

This is Plan 1 of 3. Plan 2: eve and the Slack channel (identity at the edge, Slack account linking, output guard, AI gateway). Plan 3: the web app with AuthKit sign-in.

## Global Constraints

- Fail closed: any error in assertion checks, database access or policy evaluation returns `Refuse` (security rule 1).
- One gate: tool execution happens only after `gate::authorize` returns `Run` for that exact call (rule 2).
- Every request carries a tenant from the verified assertion; every table has `tenant_id` with row-level security keyed on `current_setting('mandate.tenant')` (rule 4).
- Ids are validated types: `[a-z0-9_-]`, length 1 to 64 (rule 5).
- Grants expire at decision time: a grant with `expires_at <= now` grants nothing (rule 6).
- Every decision is written to `audit` in the same transaction (rule 7).
- No provider or model name in code (design 8.6).
- `thiserror` in the library, no `unwrap`/`expect` outside tests and `main`; `cargo fmt`, `cargo clippy --all-targets -- -D warnings` pass; dependencies permissive (CLAUDE.md).
- Assertion lifetime: 300 seconds. Approval lifetime: 3600 seconds. Args are hashed with SHA-256 over canonical JSON (sorted keys, no whitespace).

## Review Focus

1. **An expired or tampered assertion** must be refused before any database work. (Task 2 tests.)
2. **An agent id from another tenant** in an otherwise valid call must be refused, even if a row with that id exists in the other tenant. (Task 5 test `cross_tenant_agent_is_refused`.)
3. **An approval replayed** (second use, different args, or two concurrent calls) must run at most once, for exactly the approved args. (Task 5 tests.)
4. **The database unreachable or the policy files invalid** must refuse, not run, and the server must not start with invalid policies. (Tasks 3 and 5 tests.)
5. **A tool name not in the catalog or oversized args (over 64 KiB)** must be refused without calling any backend. (Task 6 test `unknown_tool_and_huge_args_are_refused`.)

---

## File structure

```
Cargo.toml                      workspace with one member
core/Cargo.toml                 package `mandate`, bin `mandate`
core/src/lib.rs                 module list
core/src/ids.rs                 validated id types
core/src/assertion.rs           signed turn assertions
core/src/policy.rs              Cedar loading and decisions
core/src/store.rs               Postgres: migrations, context loading, approvals, audit
core/src/gate.rs                authorize(): the one gate
core/src/tools/mod.rs           Tool trait, catalog
core/src/tools/browser.rs       browser tools and the BrowserBackend trait
core/src/tools/agentcore.rs     AgentCore Browser backend
core/src/mcp.rs                 MCP server over streamable HTTP
core/src/identity.rs            WorkOS token verification, assertion exchange, directory sync
core/src/main.rs                `mandate serve | migrate | dev-assert`
core/migrations/0001_init.sql
policies/schema.cedarschema
policies/platform.cedar
core/tests/                     integration tests (Postgres-backed ones are #[ignore])
scripts/check.sh                fmt, clippy, tests, live tests against a local Postgres
```

---

### Task 1: Workspace and validated ids

**Files:**
- Create: `Cargo.toml`, `core/Cargo.toml`, `core/src/lib.rs`, `core/src/ids.rs`, `scripts/check.sh`

**Interfaces:**
- Produces: `TenantId`, `MemberId`, `TeamId`, `AgentId`, `SpaceId`, `ToolId`, each `pub struct X(String)` with `X::parse(&str) -> Result<X, IdError>`, `as_str(&self) -> &str`, `Display`, `Serialize`/`Deserialize` (deserialize validates). `ToolId` also allows `.` (e.g. `web.read_page`).

- [ ] **Step 1: Write the failing tests** in `core/src/ids.rs`

```rust
#[test]
fn accepts_valid_and_rejects_invalid() {
    assert!(TenantId::parse("acme-co_1").is_ok());
    for bad in ["", "Acme", "a b", "a/b", &"x".repeat(65), "é"] {
        assert!(TenantId::parse(bad).is_err(), "{bad:?}");
    }
    assert!(ToolId::parse("web.read_page").is_ok());
    assert!(AgentId::parse("web.read_page").is_err());
}

#[test]
fn deserialize_validates() {
    assert!(serde_json::from_str::<MemberId>("\"ok\"").is_ok());
    assert!(serde_json::from_str::<MemberId>("\"NOT OK\"").is_err());
}
```

- [ ] **Step 2: Run** `cargo test -p mandate ids` — Expected: FAIL (types not defined).
- [ ] **Step 3: Implement the id types** with one private `validate(s, allow_dot) -> Result<(), IdError>`; a macro is fine to avoid six copies.
- [ ] **Step 4: Write `scripts/check.sh`**: `cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`, then, if `DATABASE_URL` is set, `cargo test --workspace -- --ignored`.
- [ ] **Step 5: Run** `scripts/check.sh` — Expected: PASS.
- [ ] **Step 6: Commit** `core: workspace and validated id types`.

### Task 2: Signed turn assertions

**Files:**
- Create: `core/src/assertion.rs`

**Interfaces:**
- Consumes: ids from Task 1.
- Produces:
  - `pub struct Assertion { pub tenant: TenantId, pub member: MemberId, pub agent: AgentId, pub space: Option<SpaceId>, pub expires_at: i64 }` (unix seconds)
  - `pub fn sign(a: &Assertion, key: &SigningKey) -> String` — token `base64url(json) + "." + base64url(signature over the json bytes)`
  - `pub fn verify(token: &str, key: &VerifyingKey, now: i64) -> Result<Assertion, AssertionError>`
  - `pub const TTL_SECS: i64 = 300;`
  - `AssertionError` variants: `Malformed`, `BadSignature`, `Expired`.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn round_trip() { /* sign then verify at now < expires_at returns equal Assertion */ }
#[test]
fn expired_is_rejected() { /* verify at now == expires_at -> Err(Expired) */ }
#[test]
fn tampered_payload_or_signature_is_rejected() { /* flip one byte in each part -> Err(BadSignature) or Err(Malformed) */ }
#[test]
fn wrong_key_is_rejected() { /* verify with another key -> Err(BadSignature) */ }
#[test]
fn invalid_ids_inside_a_valid_signature_are_rejected() { /* sign json with tenant "BAD ID" via raw bytes -> Err(Malformed) */ }
#[test]
fn oversized_token_is_rejected_before_decoding() { /* 8 KiB token -> Err(Malformed) */ }
```

- [ ] **Step 2: Run** `cargo test -p mandate assertion` — Expected: FAIL.
- [ ] **Step 3: Implement `sign` and `verify`.** Check length (max 4096 bytes) first, then signature, then parse JSON into `Assertion` (ids validate on deserialize), then expiry (`now >= expires_at` is expired).
- [ ] **Step 4: Run** — Expected: PASS.
- [ ] **Step 5: Commit** `core: signed turn assertions`.

### Task 3: Policy

**Files:**
- Create: `policies/schema.cedarschema`, `policies/platform.cedar`, `core/src/policy.rs`

**Interfaces:**
- Produces:
  - `pub enum Effect { Read, Write, External, Delete }` (serde lowercase)
  - `pub enum Decision { Run, Ask, Refuse }`
  - `pub struct PolicyInput { pub tenant: TenantId, pub agent: AgentId, pub agent_parents: Vec<Parent>, pub tool: ToolId, pub effect: Effect, pub granted: bool, pub standing: bool, pub approved: bool }` where `pub enum Parent { Member(MemberId), Team(TeamId) }`
  - `pub struct Policy` with `Policy::load(dir: &Path) -> Result<Policy, PolicyError>` (parses and validates policies against the schema; error on any warning-free validation failure) and `fn decide(&self, input: &PolicyInput) -> Decision`.

**Model (decided here):** entity types `Tenant`, `Member`, `Team`, `Agent` (parents: its owning `Member` or `Team`), `Tool` (attr `effect: String`). Actions `invoke` and `invoke_unattended`. Context: `granted: Bool`, `standing: Bool`, `approved: Bool`.

`policies/platform.cedar`:

```cedar
// An agent may attempt a tool only under a live grant.
permit (principal is Agent, action == Action::"invoke", resource is Tool)
when { context.granted };

// Reads under a grant run without asking; other effects need a standing grant.
permit (principal is Agent, action == Action::"invoke_unattended", resource is Tool)
when { context.granted && (resource.effect == "read" || context.standing) };

// Anything leaving the company or deleting is never unattended, whatever the grant.
forbid (principal, action == Action::"invoke_unattended", resource is Tool)
when { resource.effect == "external" || resource.effect == "delete" };
```

`decide`: not `invoke` → `Refuse`; `invoke_unattended` permitted → `Run`; else `approved` → `Run`; else `Ask`. Any evaluation error → `Refuse`.

- [ ] **Step 1: Write the failing tests** (table-driven, one row per case)

```rust
// granted, standing, approved, effect -> expected
// false, *,     *,     read     -> Refuse
// true,  false, false, read     -> Run
// true,  false, false, write    -> Ask
// true,  true,  false, write    -> Run
// true,  true,  false, external -> Ask
// true,  true,  true,  external -> Run
// true,  false, true,  delete   -> Run
#[test]
fn decision_table() { /* ... */ }

#[test]
fn invalid_policy_dir_fails_to_load() { /* temp dir with a policy referencing an unknown attribute -> Err */ }
```

- [ ] **Step 2: Run** `cargo test -p mandate policy` — Expected: FAIL.
- [ ] **Step 3: Write the schema and implement `Policy`** with `cedar_policy::{PolicySet, Schema, Validator, Authorizer, Entities}`; build entities per call from `PolicyInput`.
- [ ] **Step 4: Run** — Expected: PASS.
- [ ] **Step 5: Commit** `core: Cedar policy with run, ask, refuse`.

### Task 4: Store

**Files:**
- Create: `core/migrations/0001_init.sql`, `core/src/store.rs`, `core/tests/store.rs`

**Interfaces:**
- Consumes: ids, `Effect`, `Parent`.
- Produces:
  - `pub struct Store { pool: PgPool }`, `Store::connect(url: &str) -> Result<Store, StoreError>`, `Store::migrate(&self) -> Result<(), StoreError>`
  - `pub async fn begin(&self, tenant: &TenantId) -> Result<Tx, StoreError>` — starts a transaction and runs `select set_config('mandate.tenant', $1, true)`
  - `pub async fn agent_parents(tx: &mut Tx, agent: &AgentId) -> Result<Option<Vec<Parent>>, StoreError>` — `None` if the agent is not in this tenant
  - `pub async fn tool_effect(tx: &mut Tx, tool: &ToolId) -> Result<Option<Effect>, StoreError>`
  - `pub async fn live_grant(tx: &mut Tx, agent: &AgentId, parents: &[Parent], tool: &ToolId, now: OffsetDateTime) -> Result<GrantState, StoreError>` with `pub struct GrantState { pub granted: bool, pub standing: bool }` — a grant to the agent or any parent counts
  - `pub async fn consume_approval(tx: &mut Tx, agent: &AgentId, tool: &ToolId, args_hash: &[u8; 32], now: OffsetDateTime) -> Result<bool, StoreError>` — single use
  - `pub async fn request_approval(tx: &mut Tx, agent: &AgentId, member: &MemberId, tool: &ToolId, args_hash: &[u8; 32], now: OffsetDateTime) -> Result<Uuid, StoreError>` — pending row, expires in 3600 s
  - `pub async fn approve(tx: &mut Tx, approval: Uuid, approver: &MemberId) -> Result<bool, StoreError>` — only the owning member of a personal agent, or a member of the owning team; returns false otherwise
  - `pub async fn audit(tx: &mut Tx, e: &AuditEntry) -> Result<(), StoreError>`

Tables (all with `tenant_id text not null`, RLS enabled and forced, policy `tenant_id = current_setting('mandate.tenant')`): `members`, `teams`, `team_members`, `agents (id, kind 'personal'|'team', owner_member, owner_team)`, `tools (id, effect)`, `grants (principal_kind, principal_id, tool, standing, expires_at, granted_by, reason)`, `approvals (id uuid, agent, tool, args_hash bytea, requested_by, approved_by, expires_at, used_at)`, `audit (at, agent, member, tool, args_hash, decision, reason)`. Primary keys include `tenant_id`. The app connects as a non-owner role so RLS applies.

- [ ] **Step 1: Write the failing live tests** (`#[ignore]`, need `DATABASE_URL`)

```rust
#[tokio::test] #[ignore] async fn rows_of_another_tenant_are_invisible() {}
#[tokio::test] #[ignore] async fn expired_grant_grants_nothing() {}      // expires_at == now -> granted false
#[tokio::test] #[ignore] async fn team_grant_reaches_the_team_bot() {}
#[tokio::test] #[ignore] async fn approval_is_spent_once_for_its_exact_args() {} // second consume false; other hash false
#[tokio::test] #[ignore] async fn concurrent_consumes_spend_one_approval() {}   // 8 tasks, exactly one true
#[tokio::test] #[ignore] async fn only_the_owner_can_approve() {}
```

- [ ] **Step 2: Run** `DATABASE_URL=... cargo test -p mandate --test store -- --ignored` — Expected: FAIL.
- [ ] **Step 3: Write the migration and implement `Store`.** `consume_approval` is one `UPDATE ... SET used_at = now WHERE ... AND approved_by IS NOT NULL AND used_at IS NULL AND expires_at > now RETURNING id`.
- [ ] **Step 4: Run** — Expected: PASS.
- [ ] **Step 5: Commit** `core: Postgres store with row-level security`.

### Task 5: The gate

**Files:**
- Create: `core/src/gate.rs`, `core/tests/gate.rs`

**Interfaces:**
- Consumes: `Assertion`, `Policy`, `Store`, `PolicyInput`, `Decision`.
- Produces:
  - `pub fn args_hash(args: &serde_json::Value) -> [u8; 32]` (canonical JSON, SHA-256)
  - `pub async fn authorize(store: &Store, policy: &Policy, who: &Assertion, tool: &ToolId, args: &serde_json::Value, now: OffsetDateTime) -> Outcome` with `pub enum Outcome { Run, Ask { approval: Uuid }, Refuse { reason: &'static str } }`
  - `pub async fn list(store: &Store, policy: &Policy, who: &Assertion, catalog: &[ToolId], now: OffsetDateTime) -> Vec<(ToolId, Decision)>` — excludes `Refuse`

`authorize`: begin tx for `who.tenant`; unknown agent or tool → Refuse; load grant; if `decide` with `approved: false` is `Ask`, try `consume_approval` and decide again with `approved: true` if consumed; on `Ask` create a pending approval; audit; commit. Any error → `Refuse { reason: "internal" }` (audited best-effort in a fresh transaction).

- [ ] **Step 1: Write the failing tests**

```rust
#[test] fn args_hash_ignores_key_order_and_whitespace() {}
#[tokio::test] #[ignore] async fn read_under_grant_runs_and_is_audited() {}
#[tokio::test] #[ignore] async fn external_asks_then_runs_once_after_approval() {}
#[tokio::test] #[ignore] async fn cross_tenant_agent_is_refused() {}   // agent exists only in tenant B, assertion for tenant A
#[tokio::test] #[ignore] async fn database_down_refuses() {}           // Store pointed at a closed port -> Refuse
```

- [ ] **Step 2: Run** — Expected: FAIL.
- [ ] **Step 3: Implement `args_hash`, `authorize`, `list`.**
- [ ] **Step 4: Run** — Expected: PASS.
- [ ] **Step 5: Commit** `core: the gate`.

### Task 6: Tools and the browser

**Files:**
- Create: `core/src/tools/mod.rs`, `core/src/tools/browser.rs`, `core/src/tools/agentcore.rs`, `core/tests/tools.rs`

**Interfaces:**
- Produces:
  - `#[async_trait] pub trait Tool: Send + Sync { fn id(&self) -> &ToolId; fn effect(&self) -> Effect; fn input_schema(&self) -> serde_json::Value; async fn call(&self, who: &Assertion, args: serde_json::Value) -> Result<serde_json::Value, ToolError>; }`
  - `pub struct Catalog` with `Catalog::new(tools: Vec<Arc<dyn Tool>>)`, `get(&ToolId) -> Option<Arc<dyn Tool>>`, `ids() -> Vec<ToolId>`
  - `#[async_trait] pub trait BrowserBackend: Send + Sync { async fn read_page(&self, url: &Url) -> Result<Page, ToolError>; async fn fill_and_submit(&self, url: &Url, fields: &BTreeMap<String, String>) -> Result<Page, ToolError>; }` with `pub struct Page { pub title: String, pub text: String, pub live_view_url: Option<String> }`
  - Tools: `web.read_page` (effect `read`, args `{ "url": string }`) and `web.submit_form` (effect `external`, args `{ "url": string, "fields": object of strings }`). Only `https` URLs; text capped at 100 000 characters.
  - `pub struct AgentCoreBrowser` implementing `BrowserBackend`: `StartBrowserSession` via `aws-sdk-bedrockagentcore` with a configured browser id, then CDP over the session's automation stream with `chromiumoxide`; session stopped after each call; recording is a property of the configured browser.
- The MCP layer (Task 7) enforces the 64 KiB args limit and the catalog lookup before `authorize`.

- [ ] **Step 1: Write the failing tests** with a fake `BrowserBackend`

```rust
#[tokio::test] async fn read_page_rejects_non_https_and_caps_text() {}
#[tokio::test] async fn submit_form_rejects_non_string_fields() {}
#[tokio::test] #[ignore] async fn agentcore_reads_example_com() {}   // needs AWS credentials and MANDATE_BROWSER_ID
```

- [ ] **Step 2: Run** — Expected: FAIL.
- [ ] **Step 3: Implement** the trait, catalog, two tools and `AgentCoreBrowser`.
- [ ] **Step 4: Run** unit tests — Expected: PASS. Run the ignored AgentCore test only where credentials exist.
- [ ] **Step 5: Commit** `core: browser tools behind a backend trait, AgentCore backend`.

### Task 7: MCP server

**Files:**
- Create: `core/src/mcp.rs`, `core/tests/mcp.rs`

**Interfaces:**
- Consumes: `verify`, `authorize`, `list`, `Catalog`.
- Produces: `pub fn router(state: Arc<AppState>) -> axum::Router` serving MCP streamable HTTP at `/mcp`, where `pub struct AppState { pub store: Store, pub policy: Policy, pub catalog: Catalog, pub assertion_key: VerifyingKey }`.

Behavior: every request needs `Authorization: Bearer <assertion>`; invalid → HTTP 401 before MCP handling. `tools/list` returns `list(...)`, each tool's description suffixed with ` (asks for approval)` when `Ask`. `tools/call`: args over 64 KiB or unknown tool → MCP error, no backend call; `authorize` → `Run`: call the tool; `Ask`: return a tool result `{ "status": "needs_approval", "approval": "<uuid>" }` with `isError: false`; `Refuse`: MCP error with the reason. Request body limit 1 MiB; handler timeout 60 s.

- [ ] **Step 1: Write the failing tests** (in-process router, fake backend, live Postgres where noted)

```rust
#[tokio::test] async fn missing_or_bad_assertion_is_401() {}
#[tokio::test] #[ignore] async fn list_shows_only_granted_tools() {}
#[tokio::test] #[ignore] async fn unknown_tool_and_huge_args_are_refused() {}  // fake backend records zero calls
#[tokio::test] #[ignore] async fn submit_form_returns_needs_approval_then_runs_after_approve() {}
```

- [ ] **Step 2: Run** — Expected: FAIL.
- [ ] **Step 3: Implement** with `rmcp`'s server handler and its axum streamable-HTTP service; the bearer check is an axum middleware that puts the verified `Assertion` in request extensions.
- [ ] **Step 4: Run** — Expected: PASS.
- [ ] **Step 5: Commit** `core: MCP server with listing and the gate`.

### Task 8: Sign-in and directory sync (WorkOS)

**Files:**
- Create: `core/src/identity.rs`, `core/migrations/0002_identity.sql`, `core/tests/identity.rs`
- Modify: `core/src/mcp.rs` (mount the exchange route), `core/src/store.rs` (directory writes)

**Interfaces:**
- Consumes: `Store`, `sign`, `Assertion`, `TTL_SECS`, ids.
- Produces:
  - `pub struct WorkosClaims { pub user: String, pub org: String, pub expires_at: i64 }`
  - `pub struct TokenVerifier` with `TokenVerifier::new(client: workos::Client, client_id: String)` and `async fn verify(&self, token: &str, now: i64) -> Result<WorkosClaims, IdentityError>` — RS256 only; checks signature against keys from `user_management().get_jwks(client_id)` (cached 10 minutes, refetched once on an unknown `kid`), issuer, expiry; an unknown `org` is an error.
  - Route `POST /v1/assertions` with `Authorization: Bearer <WorkOS access token>` and body `{ "agent": "<id>", "space": "<id>"? }` → `{ "assertion": "<token>", "expires_at": <unix> }`. Issued only if the agent is the member's personal agent or a bot of a team the member belongs to, in the tenant mapped from `org`. Otherwise 403.
  - `pub async fn sync_directory(store: &Store, client: &workos::Client) -> Result<u64, IdentityError>` — pulls `events().list_events_auto_paging` for `dsync.user.*`, `dsync.group.*`, `dsync.group.user_added`, `dsync.group.user_removed` after the stored cursor; applies each event idempotently; stores the cursor in the same transaction; returns events applied. `mandate serve` runs it every 60 s.
  - Tables: `tenants (id, workos_org_id unique)`, `members.workos_user_id`, `teams.workos_group_id`, `directory_cursor (singleton)`.
  - Effects of a user deleted or deactivated: member removed; grants with that member as principal or `granted_by` deleted; pending approvals they requested or could approve deleted; their personal agent disabled. One transaction per event.
  - Env: `WORKOS_API_KEY`, `WORKOS_CLIENT_ID`.

Use the SDK's session helper for token verification if it exposes it publicly; otherwise verify with `jsonwebtoken` 12 against the SDK's `get_jwks` response.

- [ ] **Step 1: Write the failing tests** (a local RSA key pair and a fake JWKS for verification; live Postgres for sync, fed with recorded event JSON, no network)

```rust
#[tokio::test] async fn valid_token_verifies_and_hs256_or_expired_or_wrong_issuer_is_rejected() {}
#[tokio::test] async fn unknown_kid_refetches_keys_once() {}
#[tokio::test] #[ignore] async fn assertion_only_for_own_or_team_agents() {}       // other member's personal agent -> 403
#[tokio::test] #[ignore] async fn removed_user_loses_grants_and_pending_approvals() {}
#[tokio::test] #[ignore] async fn replayed_events_change_nothing() {}              // apply the same batch twice -> same rows
```

- [ ] **Step 2: Run** — Expected: FAIL.
- [ ] **Step 3: Implement** `TokenVerifier`, the route, `sync_directory` and the migration.
- [ ] **Step 4: Run** — Expected: PASS.
- [ ] **Step 5: Commit** `core: WorkOS sign-in exchange and directory sync`.

### Task 9: The binary

**Files:**
- Create: `core/src/main.rs`, `core/tests/cli.rs`

**Interfaces:**
- Consumes: everything above.
- Produces the `mandate` binary:
  - `mandate migrate` — runs migrations, exits.
  - `mandate serve` — loads policies (refuses to start if invalid), connects, serves `router` plus `GET /healthz`.
  - `mandate dev-assert --tenant T --member M --agent A [--space S]` — prints a token signed with `MANDATE_ASSERTION_PRIVATE_KEY`; refuses unless `MANDATE_ENV=dev`.
  - Env: `DATABASE_URL`, `MANDATE_BIND` (default `127.0.0.1:8080`), `MANDATE_POLICY_DIR` (default `policies`), `MANDATE_ASSERTION_PUBLIC_KEY` and `MANDATE_ASSERTION_PRIVATE_KEY` (paths to PEM files), `MANDATE_BROWSER_ID`, `MANDATE_ENV`.

- [ ] **Step 1: Write the failing tests**

```rust
#[test] fn serve_refuses_invalid_policies() {}        // run binary with a broken MANDATE_POLICY_DIR -> non-zero exit, message names the file
#[test] fn dev_assert_refuses_outside_dev() {}
```

- [ ] **Step 2: Run** — Expected: FAIL.
- [ ] **Step 3: Implement `main`** with `clap` subcommands; JSON logs via `tracing-subscriber`, never logging tokens or args.
- [ ] **Step 4: Run** `scripts/check.sh` with `DATABASE_URL` set — Expected: PASS.
- [ ] **Step 5: Commit** `core: mandate binary`.

### Task 10: Policy proofs in CI

**Files:**
- Create: `core/tests/proofs.rs`, `docs/policy-proofs.md`

**Interfaces:**
- Consumes: `policies/`.

Proofs (with `cedar-policy-symcc`, cvc5 1.3.1 on PATH; test is `#[ignore]` and run by `scripts/check.sh` when `cvc5` exists):
1. `invoke_unattended` is never allowed for a tool whose effect is `external` or `delete`.
2. Nothing is allowed when `context.granted` is false.

- [ ] **Step 1: Write the two proof tests**, each asserting the property holds and printing the counterexample if not.
- [ ] **Step 2: Temporarily delete the `forbid` from `platform.cedar` and run** — Expected: proof 1 FAILS with a counterexample. Restore it.
- [ ] **Step 3: Run with the file restored** — Expected: PASS.
- [ ] **Step 4: Document** in `docs/policy-proofs.md` how to add a proof and that a widening needs the policy owner's approval.
- [ ] **Step 5: Commit** `policies: SMT proofs of platform invariants`.
