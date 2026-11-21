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
| Tools | AWS AgentCore, Bedrock | Browser, token vault, models and output guardrails |

The rule that shapes everything: **the Rust core holds all authority, the agent brain holds none.**

## Docs

- [docs/design.md](docs/design.md): what we are building, why, and the decisions behind it
- [docs/security.md](docs/security.md): the trust model and the rules every change follows
- [CLAUDE.md](CLAUDE.md): how to work in this repository

Status: design. No code yet.
