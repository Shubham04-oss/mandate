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

-- Tenant tables (RLS forced). The core is the only writer of grants, agents and approvals.
grant select, insert, update, delete on members, teams, team_members, agents, grants, approvals to :"role";
-- Append-only: the app can never rewrite or erase history.
grant select, insert on audit to :"role";

-- RLS-free directory tables.
grant select, insert on tenants to :"role";
grant select, insert, update on directory_cursor to :"role";
grant select, insert on directory_dead_letters to :"role";
