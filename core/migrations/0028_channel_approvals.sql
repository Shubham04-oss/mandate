-- An approval is spent only by the exact call it was asked for (docs/channels.md, section 8).
alter table approvals add column binding bytea;
create index approvals_binding on approvals (tenant_id, binding) where used_at is null;
-- Set by the expiry sweep, which publishes approval.expired.v1 in the same transaction.
alter table approvals add column expired_at timestamptz;
-- Where the decision came from.
alter table approvals add column decided_via text
  check (decided_via in ('web', 'slack', 'whatsapp', 'sms'));
-- The space a call was decided in; null on the web.
alter table audit add column space text;
