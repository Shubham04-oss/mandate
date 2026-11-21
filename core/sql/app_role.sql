-- Least-privilege grants for the role `mandate serve` connects as. Run after `mandate migrate`:
--   psql "$MANDATE_MIGRATION_DATABASE_URL" -v role=mandate_app -f core/sql/app_role.sql
-- The role must already exist, must log in, and must be neither superuser nor bypassrls (the
-- core refuses such a role). It owns nothing: forced RLS binds it, and it cannot alter tables.
\if :{?role}
\else
  \set role mandate_app
\endif

-- Start from nothing, so a privilege granted by hand earlier does not linger.
revoke all on all tables in schema public from :"role";
revoke all on all sequences in schema public from :"role";
grant usage on schema public to :"role";

-- Tenant tables (RLS forced), each with only what the core does to it.
grant select, insert, update, delete on members, teams, team_members, approvals to :"role";
-- Grants: org admins create and revoke them through the core's API (`/v1/grants`); never edited.
-- Agents: the app disables them and creates each member's personal operator (directory sync,
-- first sign-in); migration 0013's restrictive policy keeps app-made agents to exactly that.
grant select, insert, update on agents to :"role";
grant select, insert, delete on grants to :"role";
-- Append-only: the app can never rewrite or erase history.
grant select, insert on audit to :"role";
grant usage on sequence audit_id_seq to :"role";

-- RLS-free directory tables.
grant select, insert on tenants to :"role";
grant select, insert, update on directory_cursor to :"role";
grant select, insert on directory_dead_letters to :"role";
