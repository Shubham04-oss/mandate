-- The approver must see what they approve: the args themselves, not only their hash.
alter table approvals add column args jsonb not null default '{}'::jsonb;
alter table approvals alter column args drop default;

-- Which approval a run spent and who gave it; a key, as every other table has.
alter table audit add column id bigserial;
alter table audit add primary key (tenant_id, id);
alter table audit add column approval_id uuid;
alter table audit add column approved_by text;
