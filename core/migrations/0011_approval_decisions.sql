-- When each side happened, for the inbox and its order.
alter table approvals add column requested_at timestamptz not null default now();
alter table approvals add column approved_at timestamptz;
-- One decision per approval, whichever came first.
alter table approvals add constraint approvals_one_decision check (approved_by is null or denied_by is null);
create index approvals_inbox on approvals (tenant_id, requested_at desc, id desc);
