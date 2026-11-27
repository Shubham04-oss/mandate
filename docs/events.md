# Events

Mandate is built on events (ORY-70). A request changes state and records what happened; everything
that follows (email, analytics, resuming agents, later Slack and notifications) reacts to the event
in its own subscriber, with its own queue, retries and dead-letter queue.

```
request ─► core: state change + events::publish(tx) ─► outbox (same transaction)
                                                       └─► publisher: sign, PutEvents
                                                           (group = tenant, dedupe = event id)
                                                           └─► EventBridge v2 bus (7-day retention)
                                                               ├─► agent-resume  SQS FIFO + DLQ
                                                               ├─► posthog       SQS FIFO + DLQ
                                                               └─► email         SQS FIFO + DLQ (email_enabled)
```

## Rules

1. **State change and event in one transaction**, always through `events::publish(tx, …)`. Never
   call another feature directly after a state change.
2. **Security consequences stay in the transaction**, never in a subscriber: revoking access,
   consuming an approval, the audit row and policy decisions are synchronous and fail closed.
3. **One subscriber per concern**, each with its own SQS FIFO queue and dead-letter queue, so one
   failing feature never blocks another.
4. **Handlers are idempotent.** The consumer kit (`core/src/events/consume.rs`) dedupes through
   `events_inbox` in the handler's own transaction; a side effect outside the database (email,
   PostHog) uses the event id as its key.
5. **The contract only grows.** Types are `<entity>.<verb>.v<N>`. `data` carries ids and enum
   values only, never text, addresses, arguments or tokens. A change may add event types, optional
   fields and enum values; removing or retyping a field, or making one required, is a new version.
   CI enforces this: `events::contract` compares `schema/events/v1.json` with the one on `main`.
6. **Order per tenant only** (FIFO group = tenant); a handler must not assume order across tenants.
7. **Every subscriber is observable:** an alarm when its dead-letter queue holds anything, and one
   when its oldest message is older than 15 minutes (a consumer that is stuck, not only failing).

The JSON Schema of every type is `schema/events/v1.json` (generated from `core/src/events/mod.rs`
with `MANDATE_WRITE_SCHEMA=1`; a test keeps it current).

## Catalog

Every type the core publishes. A test fails if one is missing here or listed here but gone.

| Type | `data` | Published by | Subscribers |
| --- | --- | --- | --- |
| `approval.requested.v1` | `approval`, `agent` | `store::request_approval`, for a new approval only | `posthog`, `email` |
| `approval.decided.v1` | `approval`, `decision`, `by` | `api::approvals::decide_approval` | `agent-resume`, `posthog` |
| `conversation.started.v1` | `conversation` | `api::conversations` (start) | `posthog` |
| `conversation.finished.v1` | `conversation` | `api::conversations` (finish) | `posthog` |
| `grant.created.v1` | `grant` | `api::grants` (create) | `posthog` |
| `grant.revoked.v1` | `grant` | `api::grants` (revoke) | `posthog` |
| `member.created.v1` | `member`, `actor` | directory sync (`identity`), sign-up and first sign-in (`api::auth`) | `posthog` |
| `member.updated.v1` | `member`, `actor` | directory sync | `posthog` |
| `member.removed.v1` | `member`, `actor` | directory sync | `posthog` |
| `organization.created.v1` | `actor` | directory sync, sign-up | `posthog` |
| `member.signed_up.v1` | `member`, `method` | sign-up and first sign-in (`api::auth`) | `posthog` |
| `member.signed_in.v1` | `member`, `method` | every new session (`api::auth`) | `posthog` |
| `organization.chosen.v1` | `member` | the organization picker (`api::auth`) | `posthog` |
| `setting.changed.v1` | `by`, `setting` | turning two-step on (`api::auth`) | `posthog` |
| `email.sent.v1` | `member`, `template`, `cause` | the `email` subscriber, once SES accepts an email (`cause` is the event that asked for it) | `posthog` |
| `email.bounced.v1` | `member`, `template` | the SES feedback reader, on a permanent bounce | `posthog` |
| `email.complained.v1` | `member`, `template` | the SES feedback reader, on a complaint | `posthog` |

`member.*` and `organization.created.v1` are also written to `lifecycle_log` in the same
transaction. `email.bounced.v1` and `email.complained.v1` come from outside (SES, through
the feedback queue only its EventBridge rule may write to) and are published by the core after it
suppresses the address, as rule 8 of ORY-70 asks of outside sources. Subscribers and the types they take are `local.consumers` in
`infra/envs/dev/events.tf`; the `email` subscriber exists only while `email_enabled` is on.

## Adding an event or a subscriber

- **An event:** add the variant to `Event` (ids and enums only) and to `samples()`, publish it in
  the transaction that makes the change, regenerate the schema, and add its row above.
- **A subscriber:** a `Handler` run through the consumer kit, registered in `core/src/main.rs`
  under the consumer's name; one entry in `local.consumers` (and `receive_limits` if it calls an
  outside service); its queue URL reaches the core as `MANDATE_QUEUE_<NAME>`. Both alarms cover it
  without more work.

## Replay

Consumers refuse an envelope whose `occurred_at` is more than **24 hours** old (`sign::MAX_AGE`):
that bounds how long a captured event can be replayed against us. `occurred_at` is when the event
was written to the outbox, so every replay below works only inside that window. The bus keeps 7
days, but an event older than a day is quarantined by every consumer if it comes back; rebuilding a
subscriber from older history means reading the database, not the bus.

**A dead-letter queue** (after an outage, once the cause is fixed):

```sh
aws sqs start-message-move-task --source-arn <mandate-dev-<name>-dlq.fifo ARN>
```

Each consumer's inbox skips what it already handled, and side effects are keyed by event id, so a
redrive never repeats work that finished.

**Re-publish from the outbox** (a subscriber that was down or newly added, still inside 24 hours),
as the migration role:

```sql
update outbox set sent_at = null
 where created_at > now() - interval '23 hours'
   and type = any(array['approval.requested.v1']);   -- only what the subscriber takes
```

The publisher signs and sends them again within a second. Every other subscriber of those types
receives them too and skips them through its inbox; the new or recovered one handles them.

**Quarantine:** `events_quarantine` keeps only a digest and the reason (forged, wrong source,
stale, undecodable) of what a consumer refused; the content is never stored.
