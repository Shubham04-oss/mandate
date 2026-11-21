-- No tenant RLS: an org must map to its tenant before any tenant is known; the app role gets SELECT (and INSERT for sync), never UPDATE or DELETE.
create table tenants (
  id text primary key,
  workos_org_id text not null unique
);
-- No tenant RLS: one cursor over the event stream of every org, advanced in each event's tenant transaction.
create table directory_cursor (
  id boolean primary key default true check (id),
  after text not null
);

alter table members add column workos_user_id text;
create unique index members_workos_user on members (tenant_id, workos_user_id);
alter table teams add column workos_group_id text;
create unique index teams_workos_group on teams (tenant_id, workos_group_id);
alter table agents add column disabled boolean not null default false;
