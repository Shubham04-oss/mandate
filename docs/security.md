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

## Sign-in rate limits

- Code and password checks share one 5-per-minute bucket per email, so anyone can use up a
  victim's minute, which also blocks their verify-email and invite-accept. That is an accepted
  trade-off: it bounds guessing whatever token, step or IP the guesser uses.
- The per-IP limit keys on `CF-Connecting-IP` (`MANDATE_TRUST_CF_IP=1`), set by Cloudflare and
  passed in by the tunnel's sidecar on localhost. The core honours it only from a loopback peer:
  eve also reaches the core's port (for `/mcp`), but from its own address, so a header it sends is
  ignored and eve is limited by that address.
- A leftover sign-up row (a sign-up nobody confirmed) blocks that email's password sign-ins until
  a password reset clears it. A sign-up for an email that already has an account blocks its
  password sign-in for one WorkOS round trip; the 5-per-minute limit caps how often.

## Browser egress

- **Document navigations** (top level, redirect hops, frames) are limited per call to the allowed
  hosts and blocked before they are sent: the read allowlist for a Read call, the requested and
  approved host otherwise.
- **Popups and extra targets** (new windows or tabs, out-of-process frames, workers) are closed
  after they are reported and fail the call; their first request may already have been sent.
- **Requests made by page script** (fetch, images, beacons) are not blocked in v1. Read calls only
  reach admin-allowlisted hosts, and every other call was approved by a person who saw the URL.
  Network-layer enforcement through AgentCore Browser proxy configuration is the planned upgrade.
- The configured AgentCore browser must use PUBLIC network mode.
- `MANDATE_READ_HOSTS` is a data-egress allowlist, not only a reading list: a Read call runs
  without approval, and its URL (query string included) can carry any data the agent holds to any
  listed host. List only hosts the tenant trusts with that data.
- `live_view_url` is not usable after a call returns: the browser session ends with the call. Live
  view and human takeover need sessions that outlive a call; that is future work.
- A backend failure reaches the agent as a generic error; the detail goes only to the operator log.

## Dev network (infra/envs/dev)

- The public API reaches the core only through a Cloudflare Tunnel: `cloudflared` beside the core
  dials out, so nothing listens on a public address. The tunnel forwards only `/v1/*` and
  `/healthz`; the core's security group admits eve and nothing else.
- To save the NAT gateway and VPC endpoints, every task (the one-off `migrate` and `db-bootstrap`
  included) runs in a public subnet with a public IP and HTTPS egress to the internet; security
  groups admit nothing from the internet. The database stays in private subnets with no route out
  and is not publicly accessible.
- The tunnel token is in tofu state (tofu creates the tunnel); the state bucket is private and
  encrypted, and only the owner reads it.
- The tunnel is remotely managed, so anyone with Cloudflare Tunnel Edit on the account (the tofu
  token or a dashboard admin) could add ingress rules into the VPC. Keep that token scoped to this
  account only; tofu reverts such drift on the next apply.

## What a compromise of each part can do

- **eve:** stall or confuse conversations; request tools the core will refuse. No credentials, no
  widened access.
- **A browser session:** act within the one task it was opened for, with whatever login that task
  had; recorded.
- **The web app:** show wrong information; cannot approve without the core verifying the approver.
- **The core:** everything. It is small, Rust, reviewed, and its policy is proven.
