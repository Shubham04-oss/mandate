# Mandate: design

Date: 2026-11-21. Status: approved by the owner, 2026-11-21. A fresh start: every choice below was made
on its merits, not to fit earlier repositories (oryxa-go, oryxa-platform), which serve as reference
only.

## 1. Intent

Mandate is an always-on AI coworker for teams and organizations, in the class of Grok Bot, Meta
Muse and Claude Duet, built for orgs instead of individuals. The consumer agents share one weakness
that keeps enterprises out: nobody governs what an agent may see and do, on whose behalf, and who
agreed to it. Mandate is that governance, with agents good enough that people want them.

It carries forward the Switchboard idea: agents broker answers across permission lines without
handing over documents, and each person's agent works for its owner within permissions the owner
granted.

## 2. Principles

1. **Correctness saves time.** Everything that decides access is Rust, typed end to end, failing
   closed.
2. **Fewest moving parts.** Every service must earn its place; managed services over self-run ones;
   one database.
3. **Restricted by default.** An agent has no tool, data or credential until a grant gives it one.
4. **The brain is untrusted.** The agent layer decides what to try, never what is allowed, and never
   holds a credential.

## 3. Goals

- **Hybrid agents.** Every member has a personal operator. Teams own bots with their own identity and
  the grants the team gave them.
- **Mandates.** Every grant names a principal (member, team or agent), a scope (tool, connection,
  source), conditions, and an expiry. Policy decides per call: run, ask, or refuse.
- **Brokering.** An agent that needs something its owner cannot see asks the owning person's agent;
  the owner approves; the owner's agent reads with the owner's credentials, guards the answer, and
  returns only the answer.
- **Working for the owner.** Agents act in the owner's apps, wake on events, and ask before anything
  that changes the world unless a standing grant covers it.
- **A browser per agent** with live view, takeover at approval points, and recordings.
- **Two airbags.** Requests screened on the way in; answers guarded on the way out.
- **Proven policy.** Policy changes are checked by an SMT prover before they ship.

## 4. Non-goals (v1)

- Consumer or single-user agents.
- Full desktop computer use (Amazon WorkSpaces for AI agents is the planned addition; see 9.3).
- Code execution sandboxes, unless the first customer needs them.
- Syncing customers' data into our store. Agents fetch live.
- A native desktop app (planned later as a Tauri wrapper of the web app).

## 5. Anti-goals (failure even if it works)

- Any path by which an agent, team bot, approval or broker reaches data its principal could not.
- An answer posted in a shared space that a reader there may not see.
- A credential reachable by the agent brain or stored outside the token vault.
- A policy change that widens access without the owner of that policy approving the widening.
- A tenant's data readable from another tenant's request, by any path.

## 6. Users and surfaces

- **Members** talk to their operator and to team bots in **Slack and Microsoft Teams** first, and in
  the **web app** (a PWA, push notifications for approvals).
- **Team admins** create team bots and grant them tools and connections.
- **Workspace admins** connect apps, set org policy, see audit.
- **Owners** approve requests from cards in Slack, Teams or the web app, and watch or take over a
  browser in live view.

## 7. Architecture

```
Cloudflare   DNS . WAF . Turnstile          web app (TypeScript Worker, PWA)
                 |                                   |
AWS              v                                   v
   eve (TypeScript, ECS Fargate)  <-- channels: Slack, Teams, web
      | MCP (every tool)      | approval hook, tool list
      v                       v
   mandate core (one Rust binary, ECS Fargate)
      policy (Cedar) . grants and consent . MCP tool server . brokering . output guard . audit
      |            |                  |                    |
   Aurora      AgentCore Identity  AgentCore Browser     Bedrock Guardrails
   PostgreSQL  (token vault)       (live view, S3        (output guard)
                                    recordings)
   models: our AI gateway, provider-agnostic, used by eve and the core
   + KMS . SQS . CloudTrail . S3
```

Three artifacts: the `mandate` Rust binary, the eve image, the web Worker.

## 8. Components

### 8.1 The core (Rust, one binary)
Owns every decision and every credential path. Modes of one binary (`mandate serve --modes ...`,
`mandate migrate`), deployable as one or several Fargate services.

- **Policy.** Cedar (`cedar-policy`) evaluated in-process. Members, teams, agents, grants and
  sources are Cedar entities loaded from Postgres per request (cached within a request). Partial
  evaluation lets a tool carry its remaining limits (for example a spend ceiling) to the agent.
- **Grants and consent.** A grant is a row: principal, scope, conditions, expiry, granted-by, reason.
  "Allow for an hour" is a grant with a one-hour expiry. Grants are read into Cedar context, so an
  expired grant stops working the moment it expires, with no clean-up job.
- **Tool listing.** For a caller, agent, space and run, returns the tools the agent may see, each
  tagged `run` or `ask`; refused tools are absent.
- **MCP tool server.** The only way the brain reaches a tool. Every call: authenticate the run token,
  evaluate policy, fetch a short-lived credential from the token vault, call, audit. Built on the
  official Rust MCP SDK (`rmcp`).
- **Brokering.** Agent-to-agent requests (eve's A2A channel) land in the core, are authorized, wait
  for the owner's consent, and run in the owner's agent with the owner's credentials.
- **Output guard.** Bedrock Guardrails `ApplyGuardrail` on everything an agent posts and everything
  one agent sends another: personal data, credentials, denied topics, grounding.
- **Audit.** Every decision (allowed, asked, refused, approved by whom) is an append-only row and a
  CloudTrail-adjacent log.
- **Identity at the edge.** The core, not eve, verifies who people are. Slack and Teams events and
  card clicks hit the core first; it checks the platform signature, resolves the sender to a member,
  and forwards the turn to eve with a signed, short-lived assertion (tenant, member, space). eve
  passes that assertion back on every tool call and approval; the core rejects anything without a
  valid one. The web app signs in through the core (OIDC with the org's identity provider). So a
  broken eve cannot forge who asked, who approved, or which tenant a run belongs to.
- **Authentication: WorkOS** (official Rust SDK, `workos` crate). People sign in through AuthKit;
  enterprise SSO is set up by the customer's IT in WorkOS's admin portal. A WorkOS organization is a
  tenant. Directory sync (SCIM) feeds members, teams and team membership, pulled from the WorkOS
  Events API (no inbound webhook endpoint); a user removed from the directory loses their grants and
  pending approvals at once. The core verifies WorkOS access tokens against WorkOS's published keys,
  then mints its own assertion: WorkOS proves the person, Mandate decides access. Slack and Teams
  users are linked to members once, through a WorkOS sign-in.
- **Policy layers.** Platform policies are Cedar files in the repo, proven by SymCC in CI. Tenant
  settings are data chosen from templates (no raw Cedar in v1) and enter evaluation as entities and
  context. Grants are data, not policy. SymCC proves the platform invariants hold for any grant or
  setting data, so runtime writes never need the prover in the request path.

### 8.2 The brain (eve, untrusted)
[eve](https://github.com/vercel/eve) (Vercel, Apache-2.0) runs conversations: durable sessions that
wait for approvals without holding compute, channels (Slack, Teams, web first), multiplayer identity
(who sent each turn, who started the session, which space it is in), A2A, compaction, evals.

It holds no credentials and decides nothing about access:
- Its agent-wide `approval` policy and `defineDynamic` tools call the core, so cards appear early and
  the model only sees allowed tools, but the core checks again on every execution.
- It reaches tools only through the core's MCP server, with a short-lived run token the core issued.
- Its workflow state lives in Aurora under its own schema.
- It is never reachable from the internet. Only the core talks to it, over the private network, and
  its route auth accepts only the core's signed assertion. eve has no per-session access list and no
  tenant separation in its store, so the core alone maps a conversation to a session id, and only
  within the caller's tenant. Per-tenant eve deployments are an enterprise option (same image, own
  schema).
- A bug in eve can break a conversation; it cannot widen access or leak a credential.

### 8.3 Data
One Aurora PostgreSQL cluster ("Aurora serverless", scales to zero). Every table has a tenant id and
row-level security keyed on it; the core sets the tenant per transaction. eve's workflow world uses
a separate schema in the same cluster.

### 8.4 Browser
AgentCore Browser, one session per task: live view with takeover at approval points, recordings to
S3 for any session that writes, browser profiles for repeat logins, OS-level input when a page needs
it. Driven from the core over CDP; the brain sees only the core's browser tools.

### 8.5 Apps and credentials
- **AgentCore Identity** is the token vault: OAuth (on behalf of users and machine-to-machine), API
  keys, KMS-encrypted. The core requests a short-lived token per call; nothing else holds one.
- **Tools per app** are written in the core for the first apps (chosen by the first customer).
- **Composio** covers the long tail, off by default and opted into per org, its tools labelled
  "credentials held by a third party" so policy can hide them.

### 8.6 Models
All model calls go through our AI gateway. Nothing in code names a provider or a model: model ids
(a default, a "hard work" tier, a cheap tier for screening and summaries) are per-environment
configuration, overridable per tenant. eve reaches the gateway through the AI SDK's gateway support;
the core does the same for its own calls (input screening). The gateway's credential is a
server-side secret on AWS, never on Cloudflare and never in prompts. Bedrock Guardrails'
`ApplyGuardrail` checks text from any model, so the output guard does not depend on the provider.

## 9. Decisions and approaches considered

### 9.1 Language: Rust core
Picked Rust: correctness by construction, Cedar's reference implementation (partial evaluation,
SymCC) in-process. Considered all-TypeScript (one language, fastest to build) and a Go core (simple,
native SpiceDB client); rejected because the owner does not trust TypeScript for the core and Go
lacks Cedar's partial evaluation.

### 9.2 Agent layer: eve, untrusted
Picked eve behind the core: it brings durability, channels, multiplayer and A2A. Considered a Rust
agent loop (one language, but months of channel and durability work) and Rig (removed).

### 9.3 Computer use: browser only in v1
AgentCore Browser covers web apps. Amazon WorkSpaces for AI agents (GA June 2026, managed Windows
and Linux desktops over MCP) is the planned second backend, added as one more tool server backend
when a customer needs desktop-only apps. Sessions do not transfer between the two; org SSO is the
bridge.

### 9.4 Cloud: AWS for everything but the edge and web
Considered all-Cloudflare (Workers with Rust, Durable Objects, Cedar): rejected for 128 MB isolate
memory, per-object databases (migrations, cross-tenant queries, hot tenants), and a second home for
conversation state. Considered eve in Cloudflare Containers: rejected for a tunnel to the database
and model credentials on Cloudflare. Durable Objects may return later as a stateless relay for live
multiplayer in the web app.

### 9.5 Authorization: Cedar alone, no SpiceDB
Cedar entity hierarchies (member in team, team owns bot, grant with expiry) cover org scale
in-process. SpiceDB was considered and dropped to remove a server and its datastore.

### 9.6 Client: web app first
A PWA plus Slack and Teams. A Tauri desktop app later for local computer use and presence.

## 10. Decided by the owner (2026-11-21)

1. **What brokering may reveal.** The owner approves the question and by default sees the final
   answer before it is sent. "Allow for an hour" grants one requester access to one source through
   the owner's agent. Same workspace only, never across tenants.
2. **Shared spaces.** In a channel, answer only from sources the whole workspace may see; anything
   restricted goes to the asker by direct message with a note in the thread.
3. **Tenancy of eve.** One shared deployment; every session pinned to a tenant at route auth; the core
   refuses anything crossing tenants. Dedicated deployments are an enterprise option later.
4. **Getting to production.** Build against local stand-ins (Postgres, local Chrome, eve locally),
   then a dev environment on AWS once the first slice works.

5. **What a principal "could see".** Both layers apply: Mandate's grants decide what an agent may
   attempt, and the source app's own permissions are enforced by acting with the principal's own
   credentials, so the app itself refuses what the person cannot see. No permission sync. A team
   bot on a machine-to-machine credential may read only sources explicitly granted to it.
6. **Several people in one thread.** An agent always acts under its own owner's (or team's) grants.
   Others may ask it; it answers them only from what both they and its owner may see, and any action
   that changes something needs its owner's approval. For a team bot, any team member may ask; the
   team's admins choose who may approve, and one approval is enough by default.

## 11. Open questions

- **The first customer** and what their agents must do on day one. This orders the first apps, the
  first team bots and the first channel.
- **Compliance and residency:** regions, retention of audit logs and browser recordings, SOC 2 timing,
  limits on customer content in model prompts.
- **Team, budget and first-demo date:** sets how much is built against local stand-ins first.

## 12. Left to the builder

- Mode names of the binary and how they group into Fargate services.
- Table layout, migrations tooling, the Cedar schema's exact shape.
- The MCP tool naming scheme and how tool annotations map to Cedar effects.
- How the core caches entities and grants within a request.

## 13. References (checked 2026-11-21)

eve: https://github.com/vercel/eve . Cedar SymCC: https://docs.rs/cedar-policy-symcc . AgentCore
Browser profiles: https://docs.aws.amazon.com/bedrock-agentcore/latest/devguide/browser-profiles.html .
AgentCore Identity: https://docs.aws.amazon.com/bedrock-agentcore/latest/devguide/key-features-and-benefits.html .
WorkSpaces for AI agents: https://aws.amazon.com/about-aws/whats-new/2026/06/amazon-workspaces-ai/ .
Bedrock Guardrails: https://aws.amazon.com/bedrock/guardrails/ .
Aurora auto-pause: https://docs.aws.amazon.com/AmazonRDS/latest/AuroraUserGuide/aurora-serverless-v2-auto-pause.html .
Rust MCP SDK: https://crates.io/crates/rmcp .
