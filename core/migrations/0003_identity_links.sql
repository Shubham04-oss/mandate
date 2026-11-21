-- Directory users and AuthKit users are different WorkOS objects; a member carries both ids, linked by verified email.
alter table members add column workos_directory_user_id text;
alter table members add column email text;
update members set workos_directory_user_id = workos_user_id, workos_user_id = null
  where workos_user_id like 'directory_user_%';
create unique index members_workos_directory_user on members (tenant_id, workos_directory_user_id);

-- No tenant RLS: an event that cannot be applied may have no tenant; kept here so the cursor can move past it.
create table directory_dead_letters (
  event_id text primary key,
  event_type text not null,
  org_id text,
  error text not null,
  at timestamptz not null default now()
);
