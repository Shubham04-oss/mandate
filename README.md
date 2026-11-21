# Mandate

Always-on AI coworkers for teams and organizations, where every agent acts under a mandate: an
explicit, scoped, expiring grant of what it may see and do, and on whose behalf.

Consumer agents (Grok Bot, Meta Muse, Claude Duet) act for one person with that person's access.
Organizations need more: agents for every member and every team, that ask each other for what they
cannot see, and that never reach past the permissions someone actually granted. That governance
layer is the product.

## What it does

- **A personal operator for every member.** It watches their apps, sorts what arrives, does tasks
  with their tools, and asks before anything that changes the world.
- **Team bots with their own identity.** A team creates and owns them, grants them tools, and hands
  them work. A team bot never borrows a member's access.
- **Brokered answers.** When an agent needs something its owner cannot see, it asks the owning
  person's agent. The owner approves in one click; the answer comes back, the document never does.
- **A browser for every agent**, with live view, human takeover and recordings.
- **Restricted by default.** No tool exists for an agent until a policy grants it.

## How it is built

| Layer | Runs on | What |
| --- | --- | --- |
| Edge and web UI | Cloudflare | DNS, WAF, Turnstile; the web app (PWA) on Workers |
| Core | AWS, one Rust binary | Policy (Cedar), consent and grants, the governed tool server (MCP), audit |
| Agent brain | AWS, eve (TypeScript) | Conversations, channels (Slack, Teams, web), durable waits. Untrusted: holds no credentials and decides nothing about access |
| Data | AWS Aurora PostgreSQL | One database: system of record and conversation state |
| Tools | AWS AgentCore, Bedrock Guardrails | Browser, token vault, output guardrails |
| Models | Our AI gateway | Any provider; model ids are configuration, never code |

The rule that shapes everything: **the Rust core holds all authority, the agent brain holds none.**

## Docs

- [docs/design.md](docs/design.md): what we are building, why, and the decisions behind it
- [docs/security.md](docs/security.md): the trust model and the rules every change follows
- [CLAUDE.md](CLAUDE.md): how to work in this repository

## Status

The core slice is built: the Rust core (`core/`) with Cedar policy proven by SymCC, grants and
single-use approvals in Postgres under row-level security, WorkOS sign-in and directory sync, and
the MCP tool server with a governed browser (AgentCore). eve, the web app and infrastructure are
not built yet.

## Run

`scripts/check.sh` runs fmt, clippy, tests, `cargo deny` and the policy proofs (needs `cvc5`). With
`DATABASE_URL` set to a Postgres superuser it also runs the live database tests.

1. `mandate migrate`, connected as the migration role (`MANDATE_MIGRATION_DATABASE_URL`, falling
   back to `DATABASE_URL`).
2. Create the app role (login, not superuser, not bypassrls) and apply its grants:
   `psql "$MANDATE_MIGRATION_DATABASE_URL" -v role=mandate_app -f core/sql/app_role.sql`.
3. `mandate serve`, connected as the app role. It refuses a role that bypasses row-level security.

| Variable | Used by | What |
| --- | --- | --- |
| `DATABASE_URL` | serve | Postgres as the app role from `core/sql/app_role.sql` |
| `MANDATE_MIGRATION_DATABASE_URL` | migrate | Postgres as the role that owns the schema |
| `MANDATE_ASSERTION_PRIVATE_KEY` | serve | Path to the Ed25519 PEM key that signs assertions |
| `MANDATE_ASSERTION_PUBLIC_KEY` | serve | Path to its public key (optional; checked against the private key) |
| `MANDATE_BROWSER_ID` | serve | AgentCore browser id; the browser must use PUBLIC network mode |
| `MANDATE_READ_HOSTS` | serve | Comma-separated hosts a page read may reach without approval |
| `WORKOS_API_KEY`, `WORKOS_CLIENT_ID` | serve | WorkOS; without them `/v1/assertions` answers 503 and directory sync is off |
| `MANDATE_WEB_ORIGIN`, `MANDATE_API_ORIGIN` | serve | The web app's origin (the only one CORS and CSRF accept) and this API's public origin (for the SSO callback); with WorkOS set, they turn on the web API under `/v1` |
| `MANDATE_BIND` | serve | Listen address (default `127.0.0.1:8080`) |
| `MANDATE_POLICY_DIR` | serve | Cedar policies (default `policies`) |
| `RUST_LOG` | all | Log filter (default `info`) |
