-- One row per email the email subscriber claimed: written and committed before the send, so a
-- redelivered event finds it and never sends twice. sent_at stays null if the send's outcome was
-- lost (a crash between SES and the commit): that email is not retried, by design.
create table email_sends (
  tenant_id text not null,
  event_id uuid not null,
  member_id text not null,
  template text not null,
  claimed_at timestamptz not null default now(),
  sent_at timestamptz,
  primary key (tenant_id, event_id, member_id)
);
alter table email_sends enable row level security;
alter table email_sends force row level security;
create policy tenant_isolation on email_sends
  using (tenant_id = current_setting('mandate.tenant', true))
  with check (tenant_id = current_setting('mandate.tenant', true));

-- No tenant RLS: an address that bounced or complained is suppressed for every organization.
-- Keyed by SHA-256 of the lowercased address; members already holds addresses in this database,
-- so a keyed hash would add a secret without hiding anything.
create table email_suppressions (
  address_hash bytea primary key,
  reason text not null check (reason in ('bounce', 'complaint')),
  at timestamptz not null default now()
);
