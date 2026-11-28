-- Every eve session on any channel is a conversation; rows before this are web ones.
alter table conversations add column channel text not null default 'web'
  check (channel in ('web', 'slack', 'whatsapp', 'sms'));
-- channel_threads.id and the SpaceId; null on the web.
alter table conversations add column thread uuid;
alter table conversations add column space text;
-- The conversation this one continues (a handoff, or a thread whose session ended).
alter table conversations add column handoff_from uuid;

-- Who each turn ran as, so an approval names its requester and the web lists a conversation for
-- everyone who took part.
create table conversation_turns (
  tenant_id text not null,
  conversation uuid not null,
  -- eve's turn id.
  turn_id text not null,
  principal_kind text not null check (principal_kind in ('member', 'agent')),
  member text,
  started_at timestamptz not null default now(),
  settled_at timestamptz,
  primary key (tenant_id, conversation, turn_id),
  check ((principal_kind = 'member') = (member is not null))
);
create index conversation_turns_member on conversation_turns (tenant_id, member, conversation);
alter table conversation_turns enable row level security;
alter table conversation_turns force row level security;
create policy tenant_isolation on conversation_turns
  using (tenant_id = current_setting('mandate.tenant', true))
  with check (tenant_id = current_setting('mandate.tenant', true));
