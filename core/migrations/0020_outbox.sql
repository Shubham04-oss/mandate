-- Events written in the transaction that made the change; the publisher signs and sends them.
create table outbox (
  tenant_id text not null,
  id uuid not null,
  -- Global send order; within a tenant it is the order the bus receives them in.
  seq bigserial not null unique,
  type text not null,
  data jsonb not null,
  created_at timestamptz not null default now(),
  sent_at timestamptz,
  primary key (tenant_id, id)
);
create index outbox_unsent on outbox (seq) where sent_at is null;

-- Who joined and left, and when: ids only, append-only for the app role.
create table lifecycle_log (
  tenant_id text not null,
  event_id uuid not null,
  type text not null,
  member text,
  actor_kind text not null check (actor_kind in ('member', 'directory', 'system')),
  actor_member text,
  at timestamptz not null default now(),
  primary key (tenant_id, event_id),
  check ((actor_kind = 'member') = (actor_member is not null))
);

do $$
declare t text;
begin
  foreach t in array array['outbox','lifecycle_log'] loop
    execute format('alter table %I enable row level security', t);
    execute format('alter table %I force row level security', t);
    execute format($p$create policy tenant_isolation on %I
      using (tenant_id = current_setting('mandate.tenant', true))
      with check (tenant_id = current_setting('mandate.tenant', true))$p$, t);
  end loop;
end $$;

-- The one RLS exception: the publisher reads and marks unsent rows of every tenant; it must
-- opt in per transaction, and the app role may only change sent_at (see app_role.sql).
create policy publisher on outbox for select
  using (current_setting('mandate.outbox_publisher', true) = 'on');
create policy publisher_marks_sent on outbox for update
  using (current_setting('mandate.outbox_publisher', true) = 'on')
  with check (current_setting('mandate.outbox_publisher', true) = 'on');
