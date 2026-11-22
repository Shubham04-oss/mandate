-- Org admin follows the WorkOS organization membership role, refreshed at every sign-in.
alter table members add column if not exists is_admin boolean not null default false;
