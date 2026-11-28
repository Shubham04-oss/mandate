-- An org's channel (docs/channels.md). Slack: one install of Mandate's app per workspace.
create table channel_installations (
  tenant_id text not null,
  id uuid not null,
  kind text not null check (kind in ('slack', 'whatsapp', 'sms')),
  -- The Slack team id; null for the shared Twilio number.
  external_id text,
  -- The agent this channel speaks as (a team bot); null means each member's personal operator.
  agent text,
  -- The provider token's name in the credential store; never the token.
  credential_ref text,
  installed_by text not null,
  created_at timestamptz not null default now(),
  disabled_at timestamptz,
  primary key (tenant_id, id)
);
-- One live installation per kind per org in v1.
create unique index channel_installations_live on channel_installations (tenant_id, kind)
  where disabled_at is null;

-- No tenant RLS: a Slack request names a team, not a tenant. Ids only.
create table channel_installation_routes (
  kind text not null,
  external_id text not null,
  tenant_id text not null,
  installation uuid not null,
  primary key (kind, external_id)
);

-- A member's account on a channel, linked from the web (Slack OAuth, or a code for phones).
create table channel_identities (
  tenant_id text not null,
  member text not null,
  kind text not null check (kind in ('slack', 'whatsapp', 'sms')),
  -- Slack "<team>:<user>"; E.164 for phones.
  external_id text not null,
  linked_via text not null check (linked_via in ('oauth', 'code')),
  linked_at timestamptz not null default now(),
  primary key (tenant_id, kind, external_id),
  unique (tenant_id, member, kind)
);
-- No tenant RLS: a WhatsApp message names a phone, not a tenant. One account links to one member
-- across all tenants, so this key is unique; written with the row above, in its transaction.
create table channel_identity_routes (
  kind text not null,
  external_id text not null,
  tenant_id text not null,
  member text not null,
  primary key (kind, external_id)
);

-- A link in progress: the OAuth state or the phone code, kept only as a hash.
create table channel_links (
  tenant_id text not null,
  id uuid not null,
  member text not null,
  kind text not null check (kind in ('slack', 'whatsapp', 'sms')),
  -- The phone being verified; null for OAuth.
  external_id text,
  secret_hash bytea not null,
  attempts int not null default 0,
  expires_at timestamptz not null,
  primary key (tenant_id, id)
);

-- A place on a channel, named by an opaque SpaceId so no provider id reaches an assertion or event.
create table channel_spaces (
  tenant_id text not null,
  id text not null,
  installation uuid not null,
  -- The Slack channel id; the member's number for a phone thread.
  external_id text not null,
  kind text not null check (kind in ('direct', 'shared')),
  created_at timestamptz not null default now(),
  primary key (tenant_id, id),
  unique (tenant_id, installation, external_id)
);

-- A thread the gateway forwarded to eve: its agent, and its conversation once eve reports it.
create table channel_threads (
  tenant_id text not null,
  id uuid not null,
  installation uuid not null,
  space text not null,
  -- eve's continuation token: Slack "<channel>:<thread_ts>", Twilio "<from>:<to>".
  thread_key text not null,
  agent text not null,
  conversation uuid,
  last_inbound_at timestamptz not null default now(),
  primary key (tenant_id, id),
  unique (tenant_id, installation, thread_key)
);

-- Provider event ids already handled, so a provider retry reaches eve once.
-- ponytail: never pruned, like events_inbox.
create table channel_inbound (
  tenant_id text not null,
  installation uuid not null,
  event_id text not null,
  at timestamptz not null default now(),
  primary key (tenant_id, installation, event_id)
);

-- Messages the core posted itself (approval cards), claimed before the send so a redelivered event
-- never sends twice, and kept so a later event can update them.
create table channel_messages (
  tenant_id text not null,
  id uuid not null,
  installation uuid not null,
  approval uuid,
  -- The decider it went to; null for a thread status card.
  member text,
  -- The event that asked for it; with installation and member, the idempotency key.
  cause uuid not null,
  -- Where it landed (Slack "<channel>:<ts>", Twilio message sid); null until sent.
  external_ref text,
  claimed_at timestamptz not null default now(),
  sent_at timestamptz,
  primary key (tenant_id, id),
  unique nulls not distinct (tenant_id, cause, installation, member)
);
create index channel_messages_approval on channel_messages (tenant_id, approval);

-- Settings someone changed (docs/settings.md); a missing row means the default.
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

do $$
declare t text;
begin
  foreach t in array array['channel_installations','channel_identities','channel_links',
    'channel_spaces','channel_threads','channel_inbound','channel_messages','settings'] loop
    execute format('alter table %I enable row level security', t);
    execute format('alter table %I force row level security', t);
    execute format($p$create policy tenant_isolation on %I
      using (tenant_id = current_setting('mandate.tenant', true))
      with check (tenant_id = current_setting('mandate.tenant', true))$p$, t);
  end loop;
end $$;
