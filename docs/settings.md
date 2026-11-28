# Settings

Every setting Mandate has, in one place. The settings UI is built from this page. Started
2026-11-28 with the settings [the channel contract](channels.md) introduces (ORY-89).

## Rules

- **Typed in the core.** Each setting is a variant of one Rust enum in `core/src/settings.rs`, with
  its scope, value type, allowed values and default. An unknown name or value is refused (422). A
  test keeps this page's tables and that enum equal, as the events catalog does.
- **Stored only when changed.** A setting nobody changed has its default and no row. Clearing one
  deletes its row, so it falls back to the next scope up (space to org) or to the default.
- **Read where it is enforced,** in the transaction that uses it, never cached across requests.
- **A change publishes its event in the same transaction**: `setting.changed.v1` (`by`, `setting`,
  and the optional `space` or `kind` it applies to), unless the table names another event. Values
  never go in the event.
- **Defaults are the safe choice** when a setting could widen what is seen or done.
- **Scopes:** org (the tenant), agent, space (a channel place, see channels.md §4), member.

Storage (stream A, migration `0026_channels.sql`):

```sql
-- Settings someone changed; a missing row means the default.
create table settings (
  tenant_id text not null,
  scope text not null check (scope in ('org', 'agent', 'space', 'member')),
  -- '' for org; otherwise the agent, space or member id.
  scope_id text not null,
  name text not null,
  value jsonb not null,
  updated_by text not null,
  updated_at timestamptz not null default now(),
  primary key (tenant_id, scope, scope_id, name)
);
-- RLS forced, tenant_isolation policy as every tenant table.
-- Grants: select, insert, delete; update (value, updated_by, updated_at).
```

API (stream A): `GET /v1/settings?scope=<scope>&id=<id>` returns the effective value of each
setting in that scope and where it came from (`default`, `org`, `space`, `member`).
`PUT /v1/settings/<scope>/<name>` with `{ "id"?, "value" }` and `DELETE` (back to the default),
both with the web API's session and CSRF checks. Settings with their own routes (installations,
linked identities) are changed there, not here.

## Channels

| Setting | Scope | Type and values | Default | Who may change | Enforced in | Event on change |
| --- | --- | --- | --- | --- | --- | --- |
| `channels.enabled` | org, per kind | one installation per kind (`slack`, `whatsapp`, `sms`): on or off | off | org admins (`/v1/channels/slack/install`, `/v1/channels/{kind}/enable`, `DELETE /v1/channels/installations/{id}`) | `channels::gateway` (inbound refused), `channels::proxy` (outbound refused), `channels::notify` (no cards) | `channel.installed.v1`, `channel.removed.v1` |
| `channels.agent` | org, per installation | an agent id of a team bot in this org, or none | none: each member's personal operator | org admins (`PUT /v1/channels/installations/{id}/agent`) | `channels::gateway` (which agent a new thread binds to) | `channel.updated.v1` |
| `channels.streaming` | org, per kind | `on`, `off` | `slack`: `on`; `whatsapp`, `sms`: `off` (the provider cannot stream) | org admins | `channels::proxy`: when `off`, stream pieces are held and the guarded final text is posted once | `setting.changed.v1` (`channel_streaming`, `kind`) |
| `member.linked_identities` | member, per kind | at most one linked account per kind | none | the member (`/v1/channels/.../link`, `DELETE /v1/channels/identities/{kind}`); removed by the core when the member leaves the org | `channels::gateway` (sender to member), `channels::notify` (where cards go) | `channel_identity.linked.v1`, `channel_identity.unlinked.v1` |

## Conversations and multiplayer

| Setting | Scope | Type and values | Default | Who may change | Enforced in | Event on change |
| --- | --- | --- | --- | --- | --- | --- |
| `threads.multiplayer` | org, overridable per space | `join`: another linked member's message in a thread runs as their own turn in the same session; `starter_only`: only the person who started the thread is forwarded, others get a note | `join` | org admins (org and space) | `channels::gateway` (which senders in a bound thread are forwarded) | `setting.changed.v1` (`thread_multiplayer`, optional `space`) |
| `spaces.guest_tool_calls` | org, overridable per space | `ask`: a guest speaker's tool calls wait for the agent's deciders; `refuse`: they are refused | `ask` (owner to confirm, channels.md §19) | org admins (org and space) | `gate` (Cedar context `guest`, and `guest_refused` when `refuse`) | `setting.changed.v1` (`guest_tool_calls`, optional `space`) |

A guest speaker is a member of the org who may not use the thread's agent (channels.md §4). Members
who may use the agent (its owner; any member of a team bot's team) are not affected by either
setting's tool rule.

## Approvals

| Setting | Scope | Type and values | Default | Who may change | Enforced in | Event on change |
| --- | --- | --- | --- | --- | --- | --- |
| `approvals.channels` | member | an ordered list of `slack`, `whatsapp`, `sms`, `email`; only linked channels count; the web inbox is always on | every linked channel, in the order linked, then `email` | the member | `channels::notify` (`approval-notify`), `events::email` | `setting.changed.v1` (`approval_channels`) |
| `approvals.thread_cards` | org, overridable per space | `off`: no card in the asking thread; `status`: a neutral status card (who asked, waiting, no tool and no arguments) | `status` | org admins (org and space) | `channels::notify` (`approval-notify`, `card-update`) | `setting.changed.v1` (`thread_approval_cards`, optional `space`) |

`approvals.channels` order: in v1 every listed channel gets its card at once; the order is how the
cards and the web settings list them, and is kept for escalation later (first channel now, the
next if undecided after a while), which is not built. A decider whose list is empty still has the
web inbox. In a direct space that is the decider's own DM, the thread card is that decider's card,
with buttons, whatever `approvals.thread_cards` says.

## Agent tools

<!-- Next phase (ask_member, schedules, memory). Not designed yet. -->
