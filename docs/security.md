# Security model

## Trust boundaries

| Zone | Trusted with | Never trusted with |
| --- | --- | --- |
| Rust core | Policy decisions, grants, credential requests, audit | Input from anyone, including eve |
| eve (agent brain) | Running a conversation | Credentials, access decisions, tenant selection |
| Models | Text in, text out | Anything they say about permissions |
| Browser sessions | Acting on a page for one task | Long-lived credentials |
| Web app | Showing what the core returns | Any decision |

## Rules

1. **Fail closed.** If policy, the token vault, signing or decoding errors, the answer is no.
2. **One gate.** Every tool call goes through the core's MCP server and its policy check, even when
   eve already asked. eve's approval hook only makes cards appear early.
3. **No credential leaves the vault for longer than one call.** The core asks AgentCore Identity for
   a short-lived token per call; it is never logged, stored or returned to eve.
4. **Tenant on everything.** Every request carries a tenant from verified auth; every table has a
   tenant id with row-level security; the core refuses any cross-tenant reference.
5. **Typed input.** Ids from users, chat platforms, eve, models and webhooks are parsed into
   validated types before they reach SQL, policy, logs or tools.
6. **Grants expire.** Time-boxed access is a grant with an expiry evaluated at decision time.
7. **Everything decided is audited.** Allowed, asked, refused, approved by whom, with the grant used.
8. **Guard what leaves.** Answers and agent-to-agent messages pass the output guard before they are
   posted or sent.
9. **Policy changes are proven.** Cedar SymCC checks every change to policies in CI; a change that
   widens access needs explicit approval from the policy's owner.
10. **Outbound HTTP** from the core only to allowlisted hosts, with resolved addresses checked
    against private and metadata ranges.
11. **Dependencies** carry permissive licences and pass `cargo deny`; secrets never appear in code,
    tests, logs, commits or PR text.

## What a compromise of each part can do

- **eve:** stall or confuse conversations; request tools the core will refuse. No credentials, no
  widened access.
- **A browser session:** act within the one task it was opened for, with whatever login that task
  had; recorded.
- **The web app:** show wrong information; cannot approve without the core verifying the approver.
- **The core:** everything. It is small, Rust, reviewed, and its policy is proven.
