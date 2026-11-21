# Mandate v1, the rest: design

Date: 2026-11-21. Status: draft for the owner's review. Builds on `docs/design.md` (approved) and
the merged core slice (PR #2). One plan executes all of it, in parallel lanes.

## 1. Intent

Finish Mandate v1 so a real organization can use it end to end: people sign in on our own screens,
their personal operator and their team bots work in Slack, every risky action waits for a person
in Slack or the web app, and everything runs in an AWS dev environment behind Cloudflare.

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
- **Agents in Slack:** each member's personal operator answers in a direct message; team bots answer
  in channels they are added to. eve runs the conversations; the core verifies every Slack event,
  owns every credential and posts every reply.
- **Approvals in Slack and the web:** a waiting action shows as a card to the people who may approve
  it, in Slack and in the web app's inbox; the first decision wins everywhere.
- **Output guard on everything that leaves an agent** (Bedrock Guardrails).
- **AWS dev environment** in ap-south-1 (profile `mumbai`) and the web app on Cloudflare Workers.
- **Close the items parked by the core slice's review** before the first deploy.

## 3. Non-goals (this plan)

- Brokering between people's agents (agent-to-agent requests with owner consent). It is the next
  plan; this plan keeps its data model open (section 6.6).
- Microsoft Teams, email, iMessage and other channels. Slack only.
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
| POST `/v1/auth/sign-up` | `{ email, password, name }` | 202 `{ "status": "next", "next": "verify_email", "pending" }` |
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
deny and writes an outbox row `approval_decided` in the same transaction. Lane D consumes that
outbox to edit every Slack card and resume the waiting eve conversation; the web route (lane C) and
the Slack click handler (lane D) both call `decide_approval`, so the first decision wins in one
place. Lane C's migration adds `approvals.conversation` (opaque text, set by lane D when the turn
came from Slack) so the resume knows where to go.

## 5. Agents in Slack

### 5.1 Flow

```
Slack ──events/interactivity──> core /v1/slack/* (verify signature, dedupe by event id,
                                 resolve tenant + member + agent + space)
core ──turn + signed assertion──> eve (private, HTTP channel)
eve ──tools over MCP──> core (gate, browser tools, slack.reply)
core ──Slack Web API (bot token from the token vault)──> Slack
```

- **The core receives all Slack traffic.** It verifies Slack's signing secret in constant time,
  rejects replays older than 5 minutes, and acknowledges within 3 seconds, handing the turn to eve
  asynchronously (SQS or a tokio task with an outbox row; builder's call, one moving part at most).
- **eve never talks to Slack.** It uses eve's HTTP channel with the core as the only caller, and
  replies through the core's MCP tool `slack.reply` (effect `write`, run by default for the agent's
  own conversation; the output guard always runs). eve therefore needs no Slack token.
- **Mapping:** a Slack workspace installation belongs to one tenant. A DM with the app reaches the
  sender's personal operator. A channel message mentioning a team bot reaches that bot, if the
  sender is a member of its team; otherwise the bot replies that it can only work for its team.
- **Installing Slack (lead's call):** an org admin starts `GET /v1/slack/install` from the web app;
  the core runs Slack OAuth (`oauth.v2.access`), checks the admin's session, and binds the workspace
  to that tenant. The bot token is stored in the token vault (AgentCore Identity, one API-key
  credential provider per workspace); `slack_installations` keeps only the provider's name. A
  workspace can be bound to one tenant only.
- **Identity:** a Slack user is linked to a member by verified email within the tenant (Slack's
  `users.info` email, which needs the `users:read.email` scope, against the member's directory email).
  An unlinked sender gets a reply with a link to sign in on the web app, which completes the link.

### 5.2 Approvals in Slack

When the gate answers Ask, the core posts a card (Block Kit) to the people who may decide, in a DM
from the app: what the agent wants to do, the exact arguments (from `approvals.args`), who asked,
when it expires, and Approve / Deny buttons. Clicks arrive at `/v1/slack/interactivity`; the core
verifies the clicker is a linked member who may decide, records the decision, edits every card for
that approval to show the outcome ("Approved by Ada at 14:02"), and tells eve to continue. The web
inbox shows the same state; the first decision wins (409 for later ones).

### 5.3 Agents, grants and models

- **Personal operators** are created by directory sync for every member (enabled while the member
  exists). **Team bots** are created by a team admin through `POST /v1/agents { team, name }`.
  Minimal grant management: `GET/POST/DELETE /v1/grants` for org admins and team admins (principal,
  tool, standing, expires_at). The tool catalog for v1: `web.read_page`, `web.submit_form`,
  `slack.reply`.
- **Models** go through the owner's AI gateway: eve reads the gateway URL and model ids from its
  environment; the gateway key is an AWS Secrets Manager secret injected into eve's task. No model
  or provider name in code.
- **Output guard:** `slack.reply` text passes Bedrock Guardrails `ApplyGuardrail` (personal data,
  credentials, denied topics) before it is posted; a blocked reply is replaced by a short notice and
  audited.

## 6. Data

New migrations, numbered by lane to avoid collisions (section 8):

- 6.1 `sessions`, `auth_pending`, `rate_limits` (auth lane).
- 6.2 `approvals.denied_at`, `approvals.denied_by`; `agents.name`; `members.is_admin` (approvals
  lane). The approver-membership check from the core slice's review lands here: an approval can be
  approved or spent only while its approver may still decide.
- 6.3 `slack_installations (tenant, team_id, bot token reference)`, `slack_links (member, slack
  user)`, `slack_events_seen (event id, expiry)`, `approval_cards (approval, channel, ts)` (agents
  lane). Bot tokens live in the token vault (AgentCore Identity); the table holds only a reference.
- 6.4 All new tables are tenant-scoped with forced row-level security, except `rate_limits` and
  `slack_events_seen` (global bookkeeping, no tenant data), each with a one-line comment why.
- 6.5 `core/sql/app_role.sql` grows with each lane, least privilege.
- 6.6 Brokering later: nothing here assumes one agent per conversation or one approver per approval.

## 7. Deploy (AWS dev)

- **Infrastructure as code:** OpenTofu, in `infra/`, state in an S3 bucket with locking. The owner
  runs `tofu apply`; lanes only plan and validate.
- **AWS (ap-south-1):** one VPC; Aurora PostgreSQL serverless (scales to zero) with the migration
  role and the app role from `core/sql/app_role.sql`; ECS Fargate services for the core and eve;
  ECR; Secrets Manager (WorkOS key, assertion key, gateway key, Slack signing secret); KMS; a custom
  AgentCore browser in PUBLIC network mode with recordings to S3; a Bedrock guardrail; CloudWatch
  logs; an IAM role for GitHub Actions deploys through OIDC (no long-lived keys).
- **Cloudflare:** DNS for `api.` and `app.`; the API proxied to the core's load balancer; the web app
  on Workers (`wrangler deploy`); WAF rate limiting on `/v1/auth/*` in front of the core's own limits.
- **The owner provides:** a domain on Cloudflare, the WorkOS environment, the Slack app, and the AI
  gateway URL and key.

## 8. Lanes

One plan, five lanes, one account each. Each lane works on branch `lane/<name>`, opens one PR to
`main`, and touches only its files. The lead reviews and merges; on merge the lead rebases the
next lanes if they touched shared files.

| Lane | Owns | Migrations | Depends on |
| --- | --- | --- | --- |
| **A. Hardening** | `core/src/store.rs` (approval/role fixes), `core/sql/app_role.sql`, `core/src/tools/agentcore.rs`, `core/src/tools/browser.rs` | none | none; merges first |
| **B. Sign-in API** | `core/src/api/auth.rs`, `core/src/api/session.rs`, `core/src/api/mod.rs` (created here), `core/tests/auth.rs` | `0005`–`0009` | A |
| **C. Org, approvals and activity API** | `core/src/api/org.rs`, `core/src/api/approvals.rs`, `core/src/api/activity.rs`, `core/src/api/agents.rs`, `core/tests/api_*.rs` | `0010`–`0014` | A; uses B's session extractor (stubbed until B merges) |
| **D. Agents in Slack** | `core/src/slack/**`, `core/src/tools/slack.rs`, `core/src/guard.rs`, `agents/**` (eve), `core/tests/slack.rs` | `0015`–`0019` | A; uses C's approval decisions (shared store functions) |
| **E. Deploy** | `infra/**`, `.github/workflows/deploy.yml`, `core/Dockerfile`, `agents/Dockerfile`, `docs/deploy.md` | none | none; applies only with the owner |

Shared files (`core/src/main.rs`, `core/src/mcp.rs` router, `core/src/lib.rs`, `Cargo.toml`,
`docs/*.md`): lanes add the smallest possible hunk (one `mod` line, one `.merge(...)` line, one
dependency) and say so in the PR; the lead resolves conflicts at merge.

## 9. Left to the builder

Internal module layout inside each lane's files; the async hand-off mechanism for Slack turns
(within the one-moving-part limit); how eve's HTTP channel is configured; table column details
beyond section 6; OpenTofu module structure.

## 10. Proposed answers awaiting the owner

1. Slack identity: link by verified email within the tenant, with a web sign-in fallback (5.1).
2. Org admin: the WorkOS organization role `admin` (4.3).
3. Brokering waits for the next plan (3).
4. Session lifetimes: 12 h idle, 7 days absolute (4.1).
