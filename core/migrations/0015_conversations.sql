-- A member's conversation with their personal operator; eve keeps the transcript, the core the mapping.
create table conversations (
  tenant_id text not null,
  id uuid not null,
  member text not null,
  agent text not null,
  -- eve's session id: never shown to the browser.
  eve_session text not null,
  title text not null,
  -- failed: eve never started it.
  state text not null check (state in ('working', 'waiting', 'idle', 'failed')),
  -- The next eve stream line to relay; advanced in the same transaction as the events it made.
  eve_cursor bigint not null default 0,
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now(),
  primary key (tenant_id, id)
);
create index conversations_member on conversations (tenant_id, member, created_at desc, id desc);

-- What the browser is sent, already guarded: a reconnect replays exactly these, by `seq`.
create table conversation_events (
  tenant_id text not null,
  conversation uuid not null,
  seq bigint not null,
  kind text not null,
  data jsonb not null,
  at timestamptz not null default now(),
  primary key (tenant_id, conversation, seq)
);

-- Which eve approval prompts a core approval answers, so a decision resumes every turn waiting on
-- it: an approval reused by a second conversation or a retried prompt is linked too.
create table conversation_approvals (
  tenant_id text not null,
  approval uuid not null,
  conversation uuid not null,
  interaction text not null,
  primary key (tenant_id, approval, conversation, interaction)
);

-- The core tools eve asked to call, so a relay that restarts mid-turn still knows each call.
create table conversation_calls (
  tenant_id text not null,
  conversation uuid not null,
  call_id text not null,
  tool text not null,
  args jsonb not null,
  primary key (tenant_id, conversation, call_id)
);

do $$
declare t text;
begin
  foreach t in array array['conversations','conversation_events','conversation_approvals','conversation_calls'] loop
    execute format('alter table %I enable row level security', t);
    execute format('alter table %I force row level security', t);
    execute format($p$create policy tenant_isolation on %I
      using (tenant_id = current_setting('mandate.tenant', true))
      with check (tenant_id = current_setting('mandate.tenant', true))$p$, t);
  end loop;
end $$;
