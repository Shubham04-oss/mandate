# Working in mandate

Rules for every agent and every Claude account working here. Several sessions work in parallel, so
these are the contract.

Read [docs/design.md](docs/design.md) and [docs/security.md](docs/security.md) first. The security
rules there are not optional.

## Linear is the central context hub

Workspace: Oryxa AI Private Limited, team **Oryxa** (key `ORY`), project **Mandate — v1 launch**.
GitHub: `Shubham04-oss/mandate`.

- **Before planning or coding,** find the matching Linear issue and read its description, checklist and
  comments. They are the source of truth for what is left and why. No issue yet? Ask the user before
  creating one.
- **Keep it current as you go:** move the issue to In Progress when you start, and to Done only after
  the change is verified live. Tick checklist items as they land. Put `Fixes ORY-N` in the PR
  description. GitHub's integration closes issues on merge, so move one back if live checks are
  still pending.
- **Found new work** (a bug, a follow-up, a gap)? File it as an issue in the project, with a checklist,
  priority and blockers, instead of leaving it only in chat.
- **Decisions and status** go in a comment on the issue (what is live, what is blocked, what was
  decided and by whom), so another session can pick up from Linear alone.
- **Never** put secrets, tokens, keys, passwords, sign-in codes or presigned URLs in Linear.
- Ask before deleting, archiving or cancelling issues or projects.

## Working with the user and other sessions

- **The user decides features.** Ask which option they want, propose concrete choices with your pick,
  and wait for a yes before building. Show previews of anything people will see.
- **Stay in your issue's lane.** Changes outside it go through the user first, so parallel sessions
  don't collide on the same files.
- Ask before anything that costs money, changes DNS or production settings, or deletes data.

## Layout

```
core/     the Rust binary: policy, grants, MCP tool server, brokering, guard, audit
agents/   eve (TypeScript): conversations and channels; holds no authority
web/      the web app (TypeScript, Cloudflare Worker)
policies/ Cedar policies and schema, proven in CI
infra/    infrastructure as code (AWS, Cloudflare)
docs/     design, security, deploy, decisions
```

## Rules

- **The core decides; eve never does.** Do not add access logic, credential handling or tenant
  selection to `agents/` or `web/`.
- **Event-driven (ORY-70).** A state change publishes its event in the same transaction
  (`events::publish`); everything that follows (notifications, email, analytics, Slack…) is its own
  subscriber on the EventBridge bus with its own queue and DLQ, idempotent by event id. Never call
  another feature directly after a state change. Security consequences (revoking access, consuming
  an approval, audit) stay in the transaction, never in a subscriber. Events carry ids only and
  change additively.
- **Smallest change that fully solves the task.** No speculative abstractions. Match surrounding code.
- **Rust:** `thiserror` in libraries; no `unwrap`/`expect` outside tests and `main`; validated id
  types instead of raw strings; `cargo fmt`, `cargo clippy -- -D warnings`, `cargo deny check` pass.
- **Every non-trivial branch, parser or security rule gets a test.** Bug fix: failing test first.
- **Comments explain why**, in one line. A deliberate shortcut gets a `ponytail:` comment naming its
  limit.
- **Telemetry ships with the feature** once ORY-61 lands: named events, errors reported, spans for slow
  steps, ids only (no emails, message text, prompts or codes).
- **One task, one branch, one PR.** Never push to `main`. Merge only with green CI.

## Commits and PRs

Conventional, imperative subject (`core: add grant expiry check`). End commit messages with
`Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`. PR descriptions include `Fixes ORY-N` and
end with `🤖 Generated with [Claude Code](https://claude.com/claude-code)`.
