create table members (
  tenant_id text not null,
  id text not null,
  primary key (tenant_id, id)
);
create table teams (
  tenant_id text not null,
  id text not null,
  primary key (tenant_id, id)
);
create table team_members (
  tenant_id text not null,
  team_id text not null,
  member_id text not null,
  primary key (tenant_id, team_id, member_id)
);
create table agents (
  tenant_id text not null,
  id text not null,
  kind text not null check (kind in ('personal', 'team')),
  owner_member text,
  owner_team text,
  primary key (tenant_id, id),
  check ((kind = 'personal' and owner_member is not null and owner_team is null)
      or (kind = 'team' and owner_team is not null and owner_member is null))
);
create table grants (
  tenant_id text not null,
  principal_kind text not null check (principal_kind in ('agent', 'member', 'team')),
  principal_id text not null,
  tool text not null,
  standing boolean not null default false,
  expires_at timestamptz not null,
  granted_by text not null,
  reason text,
  primary key (tenant_id, principal_kind, principal_id, tool)
);
create table approvals (
  tenant_id text not null,
  id uuid not null,
  agent text not null,
  tool text not null,
  args_hash bytea not null,
  requested_by text not null,
  approved_by text,
  expires_at timestamptz not null,
  used_at timestamptz,
  primary key (tenant_id, id)
);
create table audit (
  tenant_id text not null,
  at timestamptz not null default now(),
  agent text not null,
  member text not null,
  tool text not null,
  args_hash bytea not null,
  decision text not null,
  reason text
);

-- Forced so the table owner is bound too; superusers still bypass, so the app must not be one.
do $$
declare t text;
begin
  foreach t in array array['members','teams','team_members','agents','grants','approvals','audit'] loop
    execute format('alter table %I enable row level security', t);
    execute format('alter table %I force row level security', t);
    execute format($p$create policy tenant_isolation on %I
      using (tenant_id = current_setting('mandate.tenant', true))
      with check (tenant_id = current_setting('mandate.tenant', true))$p$, t);
  end loop;
end $$;
