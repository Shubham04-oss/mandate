# Working in mandate

Read [docs/design.md](docs/design.md) and [docs/security.md](docs/security.md) first. The security
rules there are not optional.

## Layout (planned)

```
core/     the Rust binary: policy, grants, MCP tool server, brokering, guard, audit
agents/   eve (TypeScript): conversations and channels; holds no authority
web/      the web app (TypeScript, Cloudflare Worker)
policies/ Cedar policies and schema, proven in CI
infra/    infrastructure as code (AWS, Cloudflare)
docs/     design, security, decisions
```

## Rules

- **The core decides; eve never does.** Do not add access logic, credential handling or tenant
  selection to `agents/` or `web/`.
- **Smallest change that fully solves the task.** No speculative abstractions. Match surrounding code.
- **Rust:** `thiserror` in libraries; no `unwrap`/`expect` outside tests and `main`; validated id
  types instead of raw strings; `cargo fmt`, `cargo clippy -- -D warnings`, `cargo deny check` pass.
- **Every non-trivial branch, parser or security rule gets a test.** Bug fix: failing test first.
- **Comments explain why**, in one line. A deliberate shortcut gets a `ponytail:` comment naming its
  limit.
- **One task, one branch, one PR.** Never push to `main` directly once the repo is shared.

## Commits

Conventional, imperative subject (`core: add grant expiry check`). End commit messages with
`Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
