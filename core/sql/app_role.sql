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
-- Agents and grants are created by admins, not the app: it only disables agents and revokes grants.
grant select, update on agents to :"role";
grant select, delete on grants to :"role";
-- Append-only: the app can never rewrite or erase history.
grant select, insert on audit to :"role";
grant usage on sequence audit_id_seq to :"role";

-- RLS-free directory tables.
grant select, insert on tenants to :"role";
grant select, insert, update on directory_cursor to :"role";
grant select, insert on directory_dead_letters to :"role";

-- Events. The outbox is written by `publish`; the publisher may only mark rows sent.
grant select, insert on outbox to :"role";
grant update (sent_at) on outbox to :"role";
grant usage on sequence outbox_seq_seq to :"role";
-- Append-only: who joined and left is never rewritten.
grant select, insert on lifecycle_log to :"role";
-- RLS-free consumer bookkeeping: the core only records; `on conflict do nothing` needs no SELECT.
grant insert on events_inbox, events_quarantine to :"role";
grant usage on sequence events_quarantine_id_seq to :"role";
