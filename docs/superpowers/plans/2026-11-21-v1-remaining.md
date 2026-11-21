# Mandate v1, the rest: Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement your lane task by task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Sign-in on our own screens, your personal agent working from the web app with approvals in the inbox, core events on the EventBridge bus, all running in AWS dev behind Cloudflare.

**Architecture:** The Rust core (`core/`) gains a JSON API for the web app (`core/src/api/`), an events module (`core/src/events/`), an eve client and output guard; eve (`agents/`, TypeScript) runs conversations privately behind the core; OpenTofu (`infra/`) builds AWS dev.

**Tech Stack:** Rust (existing workspace: axum 0.8, sqlx 0.9, cedar-policy, rmcp, workos 3.7), `aws-sdk-eventbridgev2`, `aws-sdk-sqs`, `aws-sdk-bedrockruntime`, eve (Node, `eve` npm package), OpenTofu, Cloudflare Wrangler.

**Spec:** `docs/superpowers/specs/2026-11-21-v1-remaining-design.md` (binding), plus `docs/design.md` and `docs/security.md`.

## How the lanes work

- Six lanes, one Claude account each. Lane X works on branch `lane/<x>-<slug>` from `main`, opens **one PR** to `main`, and touches only the files its lane owns (spec section 8). The lead (this session) reviews and merges.
- **Order:** A merges first (small). B, C, D, E and F start at once; where a lane needs another lane's function before it merges, it codes against the interface named below with a local stub, and replaces the stub after the other lane merges.
- **Shared files** (`core/src/main.rs`, `core/src/lib.rs`, `core/src/mcp.rs`, root `Cargo.toml`, `core/Cargo.toml`, `core/sql/app_role.sql`, `docs/*.md`): add the smallest hunk (one `mod`, one `.merge(...)`, one dependency, your grants) and list it in the PR description.
- **Migrations:** only numbers in your range (spec section 8). Each new tenant table has forced RLS and an explicit tenant predicate in every query; extend `core/sql/app_role.sql` with least-privilege grants for your tables.
- **Done means:** `DATABASE_URL=… REQUIRE_PROOFS=1 scripts/check.sh` green locally and CI green on the PR; the PR description lists what changed, tests, shared-file hunks, and anything not verified.

## Global Constraints

- Fail closed everywhere (security rule 1). Every request carries a tenant from a verified session or assertion (rule 4). Typed ids for all input (rule 5).
- The web app holds no WorkOS key; every WorkOS call happens in the core. WorkOS errors reach logs by kind only, never their text (existing `upstream()` pattern in `core/src/identity.rs`).
- Session cookie `mandate_session`: `Secure`, `HttpOnly`, `SameSite=Lax`, `Path=/`, host-only; 256-bit random id; only its SHA-256 stored; idle 12 h, absolute 7 days.
- CSRF: state-changing requests need `X-Mandate: 1` and `Origin` equal to `MANDATE_WEB_ORIGIN`; CORS allows only that origin, with credentials.
- Pending sign-in steps: server-side row, 10-minute expiry; the browser never sees WorkOS's pending token.
- Auth rate limits: 5/min per email, 30/min per IP (`CF-Connecting-IP`, else peer); 429 with `Retry-After`.
- Code start, forgot password and sign-up always answer 202 the same way (no account discovery).
- Error body `{ "error": { "code", "message" } }` with the codes in spec 4.1.
- Event envelope `{ id, source, type, tenant, occurred_at, data, kid, signature }`, signed with Ed25519 per source and verified before use; `type` = `<entity>.<verb>.v<N>`; `data` ids only. Publish only inside the transaction that made the change.
- No model or provider name in code; models via the AI gateway (`MANDATE_AI_GATEWAY_URL`, model ids from env).
- `thiserror`, no `unwrap`/`expect` outside tests and `main`, fmt, clippy `-D warnings`, `cargo deny`; Biome for TypeScript.

## Review Focus

1. **A session cookie or pending id from one tenant used against another** must be refused (lane B test `session_is_bound_to_its_tenant`).
2. **Two people deciding the same approval at once** (web and stale tab): exactly one wins, the other gets 409 (lane C test `concurrent_decisions_one_wins`).
3. **An approver removed from the team after approving** cannot have that approval spent (lane A test `approval_dies_with_approver_membership`).
4. **A duplicated, replayed or forged event** changes nothing (lane F tests `replayed_event_is_skipped`, `bad_signature_is_quarantined`).
5. **A browser reconnecting mid-conversation** gets no duplicated or lost messages (lane D test `reconnect_resumes_after_last_event_id`).

---

## Lane A: Hardening (merges first)

**Owns:** `core/sql/app_role.sql`, `core/tests/common/mod.rs`, `core/src/store.rs` (approval/team functions only), `core/src/tools/browser.rs`, `core/src/tools/agentcore.rs`, the matching tests.

### Task A1: Least-privilege app role

- [ ] Write a failing test in `core/tests/store.rs`: `app_role_cannot_insert_agents_or_grants` (insert into `agents` and `grants` as the app role fails with a permission error).
- [ ] Run it: `DATABASE_URL=… cargo test -p mandate --test store -- --ignored app_role_cannot` → FAIL.
- [ ] Narrow `core/sql/app_role.sql`: `agents` SELECT, UPDATE; `grants` SELECT, DELETE; keep the rest. Fix the misleading comment. Add `pub async fn admin_pool() -> PgPool` (superuser, `DATABASE_URL`) to `core/tests/common/mod.rs`; seed `agents` and `grants` through it in every test file.
- [ ] Run the whole suite with `DATABASE_URL` → PASS. Commit `core: app role gets only the privileges the core uses`.

### Task A2: Approvals die with the approver's membership

- [ ] Failing tests: `approval_dies_with_approver_membership` (approve, then `set_team_member(approver, false)`, then the requester's call gets Ask again); `leaving_a_team_drops_your_unspent_approvals`.
- [ ] Implement: `consume_approval` also requires `approved_by` to still satisfy `may_use_agent` rules for that agent (join in the same UPDATE); `set_team_member(…, false)` deletes unspent approvals that member requested or approved on that team's agents.
- [ ] Run → PASS. Commit `core: approvals need the approver to still be allowed`.

### Task A3: Reject NUL in tool args; stop guard disarms after the stop

- [ ] Failing tests: `nul_in_args_is_invalid` (MCP call with `"\u0000"` in a field → invalid_params, no gate call); `stop_completes_before_disarm` (unit: cancel during an awaited stop still triggers the drop stop).
- [ ] Implement: `validate` rejects any string containing `\0` (browser tools); `StopOnDrop::stop` awaits `stop_session` before setting `self.stop = None`.
- [ ] Run → PASS. Commit `core: reject NUL in args; stop guard disarms only after the stop`.

---

## Lane F: Events

**Owns:** `core/src/events/**`, `core/tests/events.rs`, `schema/events/**`, migrations `0020`–`0024`.

**Produces (other lanes code against these from day one):**
- `pub struct Envelope { pub id: Uuid, pub source: Source, pub r#type: EventType, pub tenant: TenantId, pub occurred_at: OffsetDateTime, pub data: serde_json::Value }`
- `pub enum Event { ApprovalRequested{approval: Uuid, agent: AgentId}, ApprovalDecided{approval: Uuid, decision: Decision, by: MemberId}, ConversationStarted{conversation: Uuid}, ConversationFinished{conversation: Uuid}, GrantCreated{grant: Uuid}, GrantRevoked{grant: Uuid}, MemberCreated{member: MemberId, actor: Actor}, MemberUpdated{member: MemberId, actor: Actor}, MemberRemoved{member: MemberId, actor: Actor}, OrganizationCreated{actor: Actor} }` where `Actor` is `Member(MemberId) | Directory | System`.
- Lifecycle variants (`Member*`, `OrganizationCreated`) are also written to `lifecycle_log` by `publish`, in the same transaction. with `fn event_type(&self) -> &'static str` returning the `…v1` names from spec 6a.
- `pub async fn publish(tx: &mut Tx, event: Event) -> Result<(), StoreError>` (writes the outbox row in the caller's transaction).
- `pub trait Handler { async fn handle(&self, tx: &mut Tx, env: &Envelope) -> Result<(), HandlerError>; }` and `pub fn consumer(name: &'static str, handler: impl Handler)` run by the binary.

### Task F1: Envelope, event types and schema

- [ ] Failing unit tests in `core/src/events/mod.rs`: `type_names_are_versioned` (every `Event` maps to `<entity>.<verb>.v1`), `data_carries_ids_only` (serialized `data` of each variant has only id/enum fields), `schema_matches_checked_in_file`.
- [ ] Implement the types; generate `schema/events/v1.json` with `schemars` (add the dependency), and the test compares it to the checked-in file.
- [ ] Run → PASS. Commit `events: versioned envelope and core event types`.

### Task F1b: Signed envelopes

- [ ] Failing unit tests: `envelope_signature_round_trips`, `tampered_envelope_fails`, `wrong_source_key_fails`, `unsigned_or_older_than_24h_is_rejected`.
- [ ] Implement Ed25519 signing over canonical JSON (sorted keys, no whitespace) with `kid` and `signature` fields; keys from `MANDATE_EVENT_SIGNING_KEY` (PEM path) and a public-key map per source for verification. Run → PASS. Commit `events: sign and verify every envelope`.

### Task F2: Outbox and publish

- [ ] Migration `0020_outbox.sql`: `outbox (tenant_id, id uuid, type, data jsonb, created_at, sent_at null)` and `lifecycle_log (tenant_id, event_id, type, member, actor, at)`, both RLS forced; app role: outbox INSERT/SELECT/UPDATE, lifecycle_log INSERT/SELECT only.
- [ ] Failing live tests `publish_writes_in_the_callers_transaction` (rollback leaves no row) and `lifecycle_events_land_in_lifecycle_log` and `app_role_cannot_edit_lifecycle_log` → implement `publish` → PASS. Commit.

### Task F3: Publisher with two sinks

- [ ] `pub trait Sink { async fn send(&self, batch: &[Envelope]) -> Result<(), SinkError>; }`; `BusSink` uses `aws-sdk-eventbridgev2` PutEvents with `EventGroupId` = tenant and `DeduplicationId` = event id, source/detail-type from the envelope; `InProcessSink` hands envelopes to registered consumers. The publisher loop (spawned by `serve` when `MANDATE_EVENT_BUS` is set, else in-process) reads unsent rows across tenants with a dedicated role-free bookkeeping query (document the RLS exception in one line, like `directory_cursor`), sends, marks sent.
- [ ] Tests: `publisher_sends_in_order_per_tenant` (in-process), `failed_send_keeps_rows_unsent`. Commit.

### Task F4: Consumer kit

- [ ] Migration `0021_events_inbox.sql`: `events_inbox (consumer, event_id, at)`, global bookkeeping (comment why).
- [ ] `consume.rs`: verify the signature first (Task F1b), then SQS FIFO long-poll (`MANDATE_QUEUE_<NAME>` env), decode, `events_inbox` dedupe in the handler's transaction, delete the message on success, leave it for redrive on failure, quarantine undecodable messages to a dead letter log. In-process consumers share the same dedupe.
- [ ] Tests: `replayed_event_is_skipped`, `undecodable_message_is_quarantined`, `bad_signature_is_quarantined`, `handler_failure_retries`. Commit.

### Task F5: Live bus test

- [ ] Ignored test `bus_round_trip` (`AWS_PROFILE=mumbai`, `MANDATE_EVENT_BUS`, `MANDATE_QUEUE_TEST`): publish one event, receive it on the test queue. Runs after lane E creates the bus. Commit `events: live round trip on the dev bus`.

---

## Lane B: Sign-in API

**Owns:** `core/src/api/mod.rs` (created here), `core/src/api/auth.rs`, `core/src/api/session.rs`, `core/src/api/ratelimit.rs`, `core/tests/auth.rs`, migrations `0005`–`0009`.

**Produces:** `pub struct Session { pub tenant: TenantId, pub member: MemberId, pub is_admin: bool }` as an axum extractor (401 `forbidden` without a valid cookie; 403 on CSRF failure for unsafe methods); `pub fn router(state) -> Router` mounted at `/v1`. Lanes C and D use `Session`.

**Interfaces it uses:** a `trait AuthProvider` in `auth.rs` wrapping the WorkOS SDK calls (`authenticate_with_password`, `authenticate_with_magic_auth`, `create_magic_auth`, `authenticate_with_email_verification`, `authenticate_with_totp`, `authenticate_with_organization_selection`, `authenticate_with_code`, SSO authorization URL, password reset create/confirm, invitations, `create_organization`, organization memberships, `enroll_auth_factor`) so tests run against a fake.

### Task B1: Sessions, pending steps, CSRF and CORS

- [ ] Migrations `0005_sessions.sql` (`sessions`, `auth_pending`, RLS forced) and `0006_rate_limits.sql` (global bookkeeping).
- [ ] Failing tests: `session_cookie_has_safe_attributes`, `session_is_bound_to_its_tenant`, `idle_and_absolute_expiry`, `unsafe_method_without_csrf_header_is_403`, `cors_allows_only_the_web_origin`.
- [ ] Implement `session.rs` (create, look up by hash, touch, delete; extractor) and the CSRF/CORS layers (`MANDATE_WEB_ORIGIN`). Run → PASS. Commit.

### Task B2: Rate limits

- [ ] Failing test `sixth_attempt_per_email_is_429` and `ip_limit_applies_across_emails` → implement `ratelimit.rs` (Postgres counters per minute window) as a layer on `/v1/auth/*` → PASS. Commit.

### Task B3: Password and magic-code sign-in

- [ ] Failing tests (fake provider): `password_sign_in_signs_in`, `wrong_password_is_invalid_credentials`, `mfa_required_returns_next_mfa`, `org_selection_returns_choose_org_with_orgs`, `email_unverified_returns_verify_email`, `sso_required_returns_redirect`, `method_required_lists_methods`, `code_start_is_202_for_unknown_email`, `code_verify_signs_in`.
- [ ] Implement `/v1/auth/password`, `/code/start`, `/code/verify`, the outcome mapping (spec 4.2), member upsert on first sign-in, and `members.is_admin` from the organization membership role. Run → PASS. Commit.

### Task B4: Sign-up with a new organization; verify email

- [ ] Failing tests: `sign_up_answers_next_verify_email`, `verify_email_creates_org_and_admin_member` (fake: organization created with `org_name`, membership role admin, tenant row, personal operator via the directory/upsert path), `resend_is_202`.
- [ ] Also: sign-up publishes `organization.created.v1` and `member.created.v1` (actor = the new member); a first sign-in that upserts a member publishes `member.created.v1` (actor = System). Tests: `sign_up_records_org_and_member_created`, `first_sign_in_records_member_created`. Commit.

### Task B5: Two-step

- [ ] Failing tests: `mfa_verify_signs_in`, `mfa_setup_returns_qr_and_secret`, `mfa_enable_with_valid_code_signs_in`, `bad_totp_is_code_invalid`. Implement. Commit.

### Task B6: Choose organization and SSO

- [ ] Failing tests: `choose_org_signs_into_that_org`, `sso_start_returns_redirect`, `sso_start_404_without_sso`, `sso_callback_sets_cookie_and_redirects`, `sso_callback_bad_state_redirects_with_error`. Implement (state stored in `auth_pending`). Commit.

### Task B7: Password reset and invitations

- [ ] Failing tests: `forgot_is_202_always`, `reset_with_valid_token_is_204`, `invite_shows_org_and_inviter`, `invite_accept_with_password`, `invite_accept_with_code`. Implement. Commit.

### Task B8: Me and sign-out

- [ ] Failing tests: `me_returns_person_org_orgs_and_flags`, `sign_out_deletes_session_and_clears_cookie`, `removed_member_session_is_gone` (directory removal deletes sessions; add that one line to `remove_member` and list it as a shared hunk). Implement. Commit.

---

## Lane C: Org, approvals and activity API

**Owns:** `core/src/api/org.rs`, `core/src/api/approvals.rs`, `core/src/api/activity.rs`, `core/src/api/agents.rs`, `core/src/api/grants.rs`, `core/tests/api_*.rs`, the names/personal-operator parts of `core/src/identity.rs`, migrations `0010`–`0014`.

**Produces:** `pub async fn decide_approval(tx: &mut Tx, approval: Uuid, by: &MemberId, decision: Decision, now: OffsetDateTime) -> Result<Approval, DecideError>` (`DecideError::AlreadyDecided` → 409; `Forbidden` when `by` may not decide); it calls `events::publish(ApprovalDecided)`.

### Task C1: Names, admins and personal operators

- [ ] Migration `0010_names.sql`: `members.name`, `teams.name`, `agents.name`, `approvals.denied_at`, `approvals.denied_by`, `approvals.conversation`.
- [ ] Failing tests: `directory_sync_stores_names`, `every_member_gets_a_personal_operator`, `removed_member_operator_is_disabled`. Implement in `identity.rs`. Commit.

### Task C1b: Lifecycle events from directory sync

- [ ] Failing tests: `directory_add_emits_member_created`, `directory_change_emits_member_updated`, `directory_removal_emits_member_removed_with_directory_actor`. Implement by calling `events::publish` in each sync transaction (stub until lane F merges). Commit.

### Task C2: decide_approval

- [ ] Failing tests: `owner_can_approve`, `others_cannot_decide`, `concurrent_decisions_one_wins`, `deny_records_denied_by`, `decision_publishes_approval_decided`. Implement (single UPDATE with state guard). Commit.

### Task C3: Approvals routes

- [ ] Failing tests: `inbox_lists_pending_i_can_decide_and_my_requests`, `approve_route_returns_approval`, `second_decision_is_409`, `approval_args_are_shown_exactly`. Implement `GET /v1/approvals`, `POST …/approve`, `POST …/deny` with the `Approval` shape (spec 4.3). Commit.

### Task C4: Activity, org and agents

- [ ] Failing tests: `activity_shows_my_agent_only`, `admin_sees_all_activity`, `members_search_and_paging`, `teams_list`, `agents_list_shows_my_operator`. Implement. Commit.

### Task C5: Grants

- [ ] Failing tests: `admin_creates_and_revokes_grant` (publishes `grant.created.v1` / `grant.revoked.v1`), `non_admin_cannot_manage_grants`, `expired_grant_is_listed_as_expired`. Implement `GET/POST/DELETE /v1/grants`; grant writes need a new app-role privilege (INSERT on `grants`), listed as a shared hunk. Commit.

---

## Lane D: Your agent in the web app

**Owns:** `agents/**` (eve app), `core/src/eve.rs`, `core/src/guard.rs`, `core/src/api/conversations.rs`, `core/tests/conversations.rs`, migrations `0015`–`0019`.

### Task D1: The eve app

- [ ] `agents/`: `npx eve@latest init` into this folder; `agent/instructions.md` (personal operator), `agent/agent.ts` with the model from env (`MANDATE_MODEL_DEFAULT`) through the AI gateway, `defaultTools: false`; an MCP connection to the core (`MANDATE_CORE_MCP_URL`) whose auth forwards the turn's assertion; an agent-wide `approval` policy that calls the core's tool listing so cards appear early; the HTTP channel with route auth that accepts only the core's signed assertion (Ed25519 public key from env); Postgres workflow world (`WORKFLOW_POSTGRES_URL`).
- [ ] Tests: an eve eval `answers_with_core_tools_only` with tool stubs; `npm run check` (Biome) clean. Commit.

### Task D2: Core eve client

- [ ] `eve.rs`: start session with a message and assertion, send a message, cancel, stream session events (NDJSON) with cursor; private URL `MANDATE_EVE_URL`.
- [ ] Tests against a fake eve server: `starts_session_with_assertion`, `stream_resumes_from_cursor`. Commit.

### Task D3: Conversations API with streaming

- [ ] Migration `0015_conversations.sql` (RLS forced).
- [ ] Failing tests: `start_conversation_returns_201_and_starts_eve`, `events_stream_as_sse`, `reconnect_resumes_after_last_event_id`, `other_members_conversation_is_404`, `cancel_stops_the_turn`. Implement spec 5.2. Commit.

### Task D4: Output guard

- [ ] `guard.rs`: Bedrock `ApplyGuardrail` (`MANDATE_GUARDRAIL_ID`, `MANDATE_GUARDRAIL_VERSION`) on each assistant message before relay; blocked → notice + audit row.
- [ ] Tests with a fake guard: `blocked_message_is_replaced_and_audited`, `guard_error_blocks` (fail closed). Live ignored test `guardrail_blocks_a_credential`. Commit.

### Task D5: Approval wait and resume

- [ ] Failing tests: `ask_emits_approval_wait_with_the_approval`, `approval_decided_resumes_the_turn` (consumer of `approval.decided.v1` through lane F's kit), `deny_tells_the_agent_and_continues`. Implement. Commit.

---

## Lane E: Deploy (AWS dev, ap-south-1, profile `mumbai`)

**Owns:** `infra/**`, `.github/workflows/deploy.yml`, `core/Dockerfile`, `agents/Dockerfile`, `docs/deploy.md`. **Never runs `tofu apply`, creates AWS resources or deploys; the owner does, with the runbook.** `tofu fmt`, `tofu validate` and `tofu plan` (read-only) are allowed.

### Task E1: Skeleton and network

- [ ] `infra/envs/dev` with an S3 backend and lock (bucket created by the owner from the runbook), provider pins, a VPC with private subnets and NAT-free egress where possible (VPC endpoints for ECR, Secrets Manager, logs; regional NAT only if eve or the core need the internet, which they do for WorkOS, the AI gateway and AgentCore). `tofu validate` passes. Commit.

### Task E2: Database and secrets

- [ ] Aurora PostgreSQL serverless (min 0) with the migration role; a step in the runbook applies `core/sql/app_role.sql`; Secrets Manager entries (WorkOS key and client id, assertion private key, AI gateway key); KMS key. Commit.

### Task E3: Services and the edge

- [ ] ECR repos; `core/Dockerfile` (static release build, non-root) and `agents/Dockerfile`; ECS Fargate services for the core (public through an ALB that only accepts Cloudflare's IP ranges) and eve (private, reachable only from the core's security group); a one-off ECS task for `mandate migrate`; Cloudflare DNS for `api.` (proxied) with a WAF rate limit on `/v1/auth/*`. Commit.

### Task E4: Event bus

- [ ] The enhanced custom bus `mandate-dev`, one FIFO subscriber + SQS FIFO queue + DLQ per consumer (start with `agent-resume`), IAM for the core to publish and consume. Commit.

### Task E5: Browser, guardrail, recordings

- [ ] A custom AgentCore browser (PUBLIC network mode, recordings to an S3 bucket), a Bedrock guardrail (personal data, credentials, denied topics), outputs wired into the core's task definition. Commit.

### Task E6: CI deploy and the web app

- [ ] `.github/workflows/deploy.yml`: on push to `main`, build and push images with an OIDC role (no long-lived keys), run the migrate task, update services. `web/wrangler.jsonc` environment for `app.` with `MANDATE_API_URL`. `docs/deploy.md`: the owner's runbook (create the state bucket, apply, set secrets, apply grants, DNS, first deploy, rollback). Commit.

---

## Integration (lead, after lanes merge)

- [ ] Replace each lane's stubs with the merged interfaces; run the full suite.
- [ ] With the owner: `tofu apply`, set secrets, first deploy; run the live tests (`bus_round_trip`, `guardrail_blocks_a_credential`, AgentCore tests) against dev.
- [ ] End-to-end check in the deployed web app: sign up → sign in with two-step → ask the agent to read a page → ask it to submit a form → approve in the inbox → see it finish in Activity.
