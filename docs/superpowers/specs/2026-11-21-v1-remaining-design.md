# Mandate v1, the rest: design

Date: 2026-11-21. Status: approved by the owner, 2026-11-21 (decisions in section 10). Builds on `docs/design.md` (approved) and
the merged core slice (PR #2). One plan executes all of it, in parallel lanes.

## 1. Intent

Finish Mandate v1 so a real organization can use it end to end: people sign in on our own screens,
they ask their personal operator for work in the web app ("Ask your agent"), every risky action
waits for them in the web app's Approvals inbox, and everything runs in an AWS dev environment
behind Cloudflare. A proper end-to-end stack comes first; Slack, other channels and team bots come
later through eve's own channels (owner's decision, 2026-11-21).

The owner has several Claude accounts working in parallel. So the work is one big plan with
independent lanes, one lane per account, each lane one branch and one PR. This session is the
lead: it reviews and merges every lane. The web app's screens are already designed on the `web`
branch; whoever builds the web app wires them to the API contract in section 4, so wiring the
screens is not part of this plan.

## 2. Goals

- **Sign-in exactly as the screens show:** email magic code, password, sign-up with email
  verification, forgot and reset password, two-step (TOTP) setup and entry, SSO, invitations,
  choosing an organization, sign-out. All through the core, which calls WorkOS's headless User
  Management API. The web app never calls WorkOS and holds no WorkOS key.
- **A stable API for the web app:** session, me, organization members and teams, the approvals
  inbox with approve and deny, and the activity feed.
- **Your agent in the web app:** each member talks to their personal operator from the web app.
  eve runs the conversation behind the core; the core authenticates the member, holds every
  credential, gates every tool and streams the agent's messages to the browser.
- **Approvals in the web app:** a waiting action appears in the owner's Approvals inbox; approving
  resumes the agent, denying tells it so; the first decision wins.
- **Self-serve sign-up** creates a new organization with the person as its admin (owner's decision).
- **Output guard on everything that leaves an agent** (Bedrock Guardrails).
- **AWS dev environment** in ap-south-1 (profile `mumbai`) and the web app on Cloudflare Workers.
- **Close the items parked by the core slice's review** before the first deploy.

## 3. Non-goals (this plan)

- Brokering between people's agents (agent-to-agent requests with owner consent). It is the next
  plan; this plan keeps its data model open (section 6.6).
- Slack, Microsoft Teams, email, iMessage and other channels (later, through eve's channels).
- Team bots (on hold until eve is used to its full capacity; owner's decision). Only personal
  operators exist in this plan.
- Desktop computer use (WorkSpaces), Composio, code sandboxes.
- A production environment, billing, the desktop app.
- Wiring the web screens (the web builder does it against section 4).

## 4. The web app's API contract

### 4.1 Hosts, session and protection

- The core serves the API at `https://api.<domain>`; the web app is at `https://app.<domain>`
  (same registrable domain, so they are same-site). Locally: core on `127.0.0.1:8080`, web on
  `localhost:3000`, with the core's allowed origin set to the web origin.
- **Session:** a random 256-bit id in the cookie `mandate_session` (`Secure`, `HttpOnly`,
  `SameSite=Lax`, `Path=/`, host-only on the API host). The core stores only its SHA-256 hash in a
  `sessions` table with member, tenant, created, last seen, expiry. Idle timeout 12 h, absolute 7 days.
  Sign-out and member removal delete sessions at once. The core keeps no WorkOS tokens after
  sign-in.
- **CSRF:** every state-changing request must carry the header `X-Mandate: 1` and an `Origin` equal
  to the configured web origin; otherwise 403. CORS allows only the web origin, with credentials.
- **Pending steps:** multi-step sign-ins (two-step, choose organization, verify email) return a
  `pending` id: a random value whose server-side row holds WorkOS's pending authentication token,
  the email and an expiry of 10 minutes. The browser never sees WorkOS's token.
- **Rate limits** on every `/v1/auth/*` endpoint, counted in Postgres (no new service): at most 5 per
  minute per email and 30 per minute per IP (from Cloudflare's `CF-Connecting-IP`, else the peer
  address); over the limit is 429 with `Retry-After`.
- **No account discovery:** code start, forgot password and sign-up never reveal whether an email
  exists; they answer 202 the same way.
- **Errors** are JSON `{ "error": { "code": "...", "message": "..." } }`. Codes: `invalid_input`,
  `invalid_credentials`, `code_invalid`, `code_expired`, `pending_expired`, `mfa_required`,
  `email_unverified`, `org_required`, `no_access`, `rate_limited`, `forbidden`, `not_found`,
  `conflict`, `internal`. Messages are short, human and never contain WorkOS's text.

### 4.2 Sign-in outcomes

Every sign-in step answers with one of:

- `200 { "status": "signed_in", "me": Me }` and sets the session cookie.
- `200 { "status": "next", "next": "mfa" | "mfa_setup" | "choose_org" | "verify_email" | "sso_required" |
  "method_required", "pending": "<id>", "orgs"?: [Org], "redirect_url"?: string, "methods"?: [string] }`
  (orgs only for `choose_org`; `redirect_url` for `sso_required`; `methods` for `method_required`,
  matching WorkOS's organization authentication rules and the `sign-in_.required` screen).
- An error (above).

Two-step (TOTP) applies to password sign-ins: WorkOS enrolls and challenges factors on the
password flow; magic-code and SSO sign-ins rely on the mailbox and the identity provider. (Lead's
call; confirm against WorkOS docs when lane B starts.)

Switching organizations signs in again for the target organization (the web shows choose-org and
runs the normal flow with `org`), so each organization's SSO and method rules always apply. There is
no session-only org switch. (Lead's call.)

Local development uses `localhost` for both the core and the web app so the cookie is same-site.

### 4.3 Endpoints

All bodies are JSON. Ids are the core's validated ids.

| Method and path | Body / query | Success |
| --- | --- | --- |
| POST `/v1/auth/code/start` | `{ email }` | 202 |
| POST `/v1/auth/code/verify` | `{ email, code }` | outcome |
| POST `/v1/auth/password` | `{ email, password }` | outcome |
| POST `/v1/auth/sign-up` | `{ email, password, name, org_name }` | 202 `{ "status": "next", "next": "verify_email", "pending" }`; after verification the core creates the WorkOS organization and tenant with this person as admin |
| POST `/v1/auth/verify-email` | `{ pending, code }` | outcome |
| POST `/v1/auth/verify-email/resend` | `{ pending }` | 202 |
| POST `/v1/auth/mfa/verify` | `{ pending, code }` | outcome |
| POST `/v1/auth/mfa/setup` | `{ pending }` or signed in | 200 `{ "qr_svg", "secret", "challenge" }` |
| POST `/v1/auth/mfa/enable` | `{ pending?, challenge, code }` | outcome (or 204 when already signed in) |
| POST `/v1/auth/choose-org` | `{ pending, org }` | outcome |
| POST `/v1/auth/sso/start` | `{ email }` | 200 `{ "redirect_url" }`, or 404 `not_found` when the domain has no SSO |
| GET `/v1/auth/sso/callback` | `?code&state` | 302 to the web app (`/` or `/sign-in?error=…`), cookie set on success |
| POST `/v1/auth/password/forgot` | `{ email }` | 202 |
| POST `/v1/auth/password/reset` | `{ token, password }` | 204 |
| GET `/v1/auth/invite/{token}` | | 200 `{ org: Org, inviter: Person, email }` |
| POST `/v1/auth/invite/{token}/accept` | `{ password, name }` (new account) or `{ code }` (after `code/start` for the invited email) | outcome |
| POST `/v1/auth/sign-out` | | 204, cookie cleared |
| GET `/v1/me` | | 200 `Me` |
| GET `/v1/org/members` | `?q&cursor` | 200 `{ items: [Person], next }` |
| GET `/v1/org/teams` | | 200 `{ items: [Team] }` |
| GET `/v1/approvals` | `?state=pending|decided&cursor` | 200 `{ items: [Approval], next }` |
| POST `/v1/approvals/{id}/approve` | | 200 `Approval` (409 `conflict` if already decided) |
| POST `/v1/approvals/{id}/deny` | | 200 `Approval` (409 if already decided) |
| GET `/v1/activity` | `?cursor` | 200 `{ items: [Activity], next }` |
| GET `/v1/agents` | | 200 `{ items: [Agent] }` |

Shapes:

- `Me { person: Person, org: Org, orgs: [Org], mfa_enabled: bool, is_admin: bool, teams: [TeamRef] }`
- `Person { id, name, email, avatar_url? }`, `Org { id, name, summary: { people, agents } }`
- `Team { id, name, members: number, is_admin: bool }`, `TeamRef { id, name }`
- `Agent { id, name, kind: "personal"|"team", owner: Person|TeamRef, enabled }`
- `Approval { id, agent: Agent, tool, args, effect, requested_by: Person, requested_at, expires_at,
  state: "pending"|"approved"|"denied"|"used"|"expired", decided_by?: Person, decided_at?,
  can_decide: bool }`
- `Activity { id, at, agent: Agent, member: Person, tool, decision: "run"|"ask"|"refuse",
  reason?, approval_id?, approved_by?: Person }` — the web maps these to the screens' done,
  refused and access-ended items.

Who sees what: approvals the caller may decide plus the caller's own requests; activity for the
caller's own agent, team bots of the caller's teams, and everything for org admins.

**Members, names and admins (lead's calls).** Directory sync (lane C extends `core/src/identity.rs`)
stores member and team names and creates each member's personal operator. Organizations without
SCIM still work: the first sign-in upserts the member from the WorkOS user and organization
membership (lane B). `members.is_admin` is set from the WorkOS organization membership role (`admin`)
at every sign-in (lane B), so it follows WorkOS without a directory event.

**After a decision (lead's call).** One store function `decide_approval` (lane C) records approve or
deny (first decision wins; later ones get 409) and writes an outbox row `approval_decided` in the
same transaction. Lane D consumes that outbox to resume or cancel the waiting eve call. Lane C's
migration adds `approvals.conversation` (opaque text, set by lane D when the call came from a
conversation) so the resume knows where to go.

## 5. Your agent in the web app

### 5.1 Flow

```
web app ──(session cookie)──> core /v1/conversations/*   (authenticate, tenant + member + personal agent)
core ──turn + signed assertion──> eve (private network only, eve's HTTP session API)
eve ──tools over MCP──> core (gate, web.read_page, web.submit_form)
eve ──event stream──> core ──output guard per assistant message──> web app (server-sent events)
```

- **Conversations** belong to one member and their personal operator. The core keeps a
  `conversations` row (tenant, member, eve session id, title, created, updated); eve keeps the
  transcript in its own workflow store. The core never exposes eve's session id.
- **Streaming:** the core relays eve's session events to the browser as server-sent events, one
  event per finished assistant message, tool call status, approval wait and completion. Each
  assistant message passes the output guard before it is sent (whole messages, not tokens, in v1;
  lead's call). A reconnecting browser resumes from the last event id.
- **Approvals:** when a tool call is Ask, eve's turn pauses and the stream emits `approval_wait`
  with the approval id. `decide_approval` (lane C) writes the decision and an outbox row; lane D
  consumes it and resumes or cancels the waiting eve call, and the stream continues.
- **Agents and grants:** personal operators are created for every member (directory sync or first
  sign-in). Org admins manage grants with `GET/POST/DELETE /v1/grants` (principal, tool, standing,
  expires_at). Tool catalog for v1: `web.read_page`, `web.submit_form`.
- **Models** go through the owner's AI gateway: eve reads the gateway URL and model ids from its
  environment; the key is an AWS Secrets Manager secret injected into eve's task. No model or
  provider name in code.
- **Output guard:** Bedrock Guardrails `ApplyGuardrail` (personal data, credentials, denied topics)
  on every assistant message before the browser sees it; a blocked message is replaced by a short
  notice and audited.

### 5.2 Conversation endpoints (added to the contract)

| Method and path | Body / query | Success |
| --- | --- | --- |
| GET `/v1/conversations` | `?cursor` | 200 `{ items: [Conversation], next }` |
| POST `/v1/conversations` | `{ message }` | 201 `Conversation` (the agent starts working) |
| POST `/v1/conversations/{id}/messages` | `{ message }` | 202 |
| GET `/v1/conversations/{id}/events` | `Last-Event-ID` header | 200 `text/event-stream` |
| POST `/v1/conversations/{id}/cancel` | | 202 |

`Conversation { id, title, created_at, updated_at, state: "working"|"waiting"|"idle" }`. Events:
`message { role, text }`, `tool { name, status }`, `approval_wait { approval: Approval }`,
`approval_done { approval_id, state }`, `done`, `error { code, message }`.

## 6. Data

New migrations, numbered by lane to avoid collisions (section 8):

- 6.1 `sessions`, `auth_pending`, `rate_limits` (auth lane).
- 6.2 `approvals.denied_at`, `approvals.denied_by`; `agents.name`; `members.is_admin` (approvals
  lane). The approver-membership check from the core slice's review lands here: an approval can be
  approved or spent only while its approver may still decide.
- 6.3 `conversations (tenant, member, agent, eve session id, title, timestamps, state)` and an
  `outbox` consumer for `approval_decided` (agent lane).
- 6.4 All new tables are tenant-scoped with forced row-level security, except `rate_limits`
  (global bookkeeping, no tenant data), with a one-line comment why.
- 6.5 `core/sql/app_role.sql` grows with each lane, least privilege.
- 6.6 Brokering later: nothing here assumes one agent per conversation or one approver per approval.

## 6a. Events (EventBridge enhanced custom bus)

Owner's decision (2026-11-21): the core's own events go on the bus first; app events (a new email,
a ticket assigned) follow later and are equally important, so the bus is built to take new sources
and event types again and again without changing what already works.

- **Bus:** one EventBridge enhanced custom bus per environment, `mandate-<env>` (the relaunched
  custom bus, API `eventsv2`, Rust crate `aws-sdk-eventbridgev2`).
- **Envelope (stable, versioned):** `{ id, source, type, tenant, occurred_at, data }`.
  `source` is `mandate.core` now; app events will use `mandate.app.<provider>` (for example
  `mandate.app.gmail`). `type` is `<entity>.<verb>.v<N>` (for example `approval.decided.v1`); a
  breaking change is a new version, never an edit. `data` carries ids only, never message text,
  arguments, emails or tokens. The envelope and each event's `data` are Rust types in
  `core/src/events/`, with a JSON Schema generated from them and checked in, so new sources add
  types, not code paths.
- **Publishing:** state changes write an `outbox` row in the same transaction (lane F adds the
  table and a `publish(tx, event)` function the other lanes call); a publisher sends unsent rows to the bus with
  `EventGroupId` = tenant, so each organization's events arrive in order, and
  `DeduplicationId` = event id. At least once; consumers deduplicate.
- **Consuming:** each consumer has its own FIFO subscriber delivering to its own SQS FIFO queue with
  a dead-letter queue. A small consumer kit in `core/src/events/consume.rs` decodes the envelope,
  skips duplicates through an `events_inbox` table in the handler's transaction, and quarantines
  events it cannot decode. Adding a consumer is: one handler, one subscription rule, one queue.
- **Signed events (owner's decision):** every envelope is signed with Ed25519 over its canonical
  JSON (sorted keys, no whitespace), with one signing key per source (`mandate.core` now; each app
  source gets its own key later). The signature and key id travel with the envelope
  (`signature`, `kid`). Consumers verify before anything else and quarantine an event that fails
  verification, is unsigned, comes from a source the key does not belong to, or is older than 24
  hours. The signing key lives in Secrets Manager; the public keys ship with the consumers.
- **First events:** `approval.requested.v1`, `approval.decided.v1`, `conversation.started.v1`,
  `conversation.finished.v1`, `grant.created.v1`, `grant.revoked.v1`, and the people lifecycle below.
- **People lifecycle, for compliance (owner's decision, start small):** `member.created.v1`,
  `member.updated.v1`, `member.removed.v1`, `organization.created.v1`. They are emitted wherever
  the change happens (sign-up, first sign-in, directory sync, removal) and also written to an
  append-only `lifecycle_log` table (tenant, event id, type, member id, actor, at; ids only, no
  names or emails), so there is a durable record of who joined and left and when. The app role may
  only insert and read it. Erasing a person's data on request (right to be forgotten) and retention
  rules come in a later plan; this one makes sure every create and delete is recorded.
- **First consumer:** the agent lane's resume step consumes `approval.decided.v1` from its queue
  (instead of polling the outbox in-process).
- **Local and tests:** the same publisher and consumer kit run against an in-process sink when no
  bus is configured (`MANDATE_EVENT_BUS` unset), so tests need no AWS; one ignored live test
  publishes to and consumes from the real dev bus.
- **Later, app events:** they enter through a connector that turns a provider's webhook or poll
  into envelopes with `source = mandate.app.<provider>`; agents subscribe by rule. Nothing in this
  plan builds a connector, but nothing blocks one.

## 7. Deploy (AWS dev)

- **Infrastructure as code:** OpenTofu, in `infra/`, state in an S3 bucket with locking. The owner
  runs `tofu apply`; lanes only plan and validate.
- **AWS (ap-south-1):** one VPC; Aurora PostgreSQL serverless (scales to zero) with the migration
  role and the app role from `core/sql/app_role.sql`; ECS Fargate services for the core and eve;
  ECR; the EventBridge enhanced custom bus with FIFO subscribers and SQS FIFO queues plus
  dead-letter queues per consumer; Secrets Manager (WorkOS key, assertion key, gateway key); KMS; a custom
  AgentCore browser in PUBLIC network mode with recordings to S3; a Bedrock guardrail; CloudWatch
  logs; an IAM role for GitHub Actions deploys through OIDC (no long-lived keys).
- **Cloudflare:** DNS for `api.` and `app.`; the API proxied to the core's load balancer; the web app
  on Workers (`wrangler deploy`); WAF rate limiting on `/v1/auth/*` in front of the core's own limits.
- **The owner provides:** a domain on Cloudflare, the WorkOS environment, and the AI gateway URL
  and key.

## 8. Lanes

One plan, six lanes, one account each. Each lane works on branch `lane/<name>`, opens one PR to
`main`, and touches only its files. The lead reviews and merges; on merge the lead rebases the
next lanes if they touched shared files.

| Lane | Owns | Migrations | Depends on |
| --- | --- | --- | --- |
| **A. Hardening** | `core/src/store.rs` (approval/role fixes), `core/sql/app_role.sql`, `core/src/tools/agentcore.rs`, `core/src/tools/browser.rs` | none | none; merges first |
| **B. Sign-in API** | `core/src/api/auth.rs`, `core/src/api/session.rs`, `core/src/api/mod.rs` (created here), `core/tests/auth.rs` | `0005`–`0009` | A |
| **C. Org, approvals and activity API** | `core/src/api/org.rs`, `core/src/api/approvals.rs`, `core/src/api/activity.rs`, `core/src/api/agents.rs`, `core/tests/api_*.rs` | `0010`–`0014` | A; uses B's session extractor (stubbed until B merges) |
| **D. Your agent in the web app** | `core/src/api/conversations.rs`, `core/src/eve.rs`, `core/src/guard.rs`, `agents/**` (eve), `core/tests/conversations.rs` | `0015`–`0019` | A; uses B's session extractor and C's `decide_approval` outbox (stubbed until they merge) |
| **F. Events** | `core/src/events/**`, `core/tests/events.rs`, `schema/events/**` | `0020`–`0024` | A; lanes C and D call its `publish` within their transactions (stubbed until F merges) |
| **E. Deploy** | `infra/**`, `.github/workflows/deploy.yml`, `core/Dockerfile`, `agents/Dockerfile`, `docs/deploy.md` | none | none; applies only with the owner |

Shared files (`core/src/main.rs`, `core/src/mcp.rs` router, `core/src/lib.rs`, `Cargo.toml`,
`docs/*.md`): lanes add the smallest possible hunk (one `mod` line, one `.merge(...)` line, one
dependency) and say so in the PR; the lead resolves conflicts at merge.

## 9. Left to the builder

Internal module layout inside each lane's files; the async hand-off from the core to eve (within the
one-moving-part limit); how eve's HTTP channel is configured; table column details
beyond section 6; OpenTofu module structure.

## 10. Decided by the owner (2026-11-21)

1. Passwords, password reset and two-step are in v1 (matching the screens).
2. Self-serve sign-up creates a new organization with the person as its admin.
3. Team bots are on hold until eve is used to its full capacity.
4. Events: the core's own events go on the EventBridge enhanced custom bus first; app events come
   later and the bus is built so new sources and types can be added over and over. Events are
   signed, and people being created and removed is recorded for compliance from the start.
5. The first end-to-end agent path is the web app; Slack and other channels come later through eve.

## 11. Lead's calls the owner can overrule

Org admin is the WorkOS organization role `admin`; sessions last 12 h idle and 7 days at most;
brokering waits for the next plan; assistant messages are guarded whole (no token streaming) in v1.
