-- Least-privilege grants for the role `mandate serve` connects as. Run after `mandate migrate`:
--   psql "$MANDATE_MIGRATION_DATABASE_URL" --single-transaction -v ON_ERROR_STOP=1 -v role=mandate_app -f core/sql/app_role.sql
-- In one transaction: run bare, the gap between "revoke all" and the grants fails live queries.
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
-- Agents are created by admins (personal operators only via `ensure_personal_agent`); the app disables them.
grant select, update on agents to :"role";
-- Grants: org admins create and revoke them through the core's API (`/v1/grants`); never rewritten.
grant select, insert, delete on grants to :"role";
-- Append-only: the app can never rewrite or erase history.
grant select, insert on audit to :"role";
grant usage on sequence audit_id_seq to :"role";

-- RLS-free directory tables.
grant select, insert on tenants to :"role";
grant select, insert, update on directory_cursor to :"role";
grant select, insert on directory_dead_letters to :"role";

-- Web sessions (RLS forced): created at sign-in, touched per request, deleted at sign-out.
grant select, insert, update, delete on sessions to :"role";
-- Pending sign-in steps (RLS forced): stored, read and used up, never edited.
grant select, insert, delete on auth_pending to :"role";
-- Sign-in rate counters (RLS-free, hashes only): upserted per attempt, old windows dropped.
grant select, insert, update, delete on rate_limits to :"role";
-- Unconfirmed sign-up passwords (RLS-free; an email and a WorkOS user id, never a password):
-- recorded at sign-up, given the user id, marked when neutralised, deleted once settled.
grant select, insert, delete on sign_up_passwords to :"role";
grant update (workos_user_id, neutralised_at) on sign_up_passwords to :"role";
-- A password reset ends the user's sessions in every tenant only through this narrow function.
grant execute on function end_sessions_for_workos_user(text) to :"role";
-- Sign-in gives a member their personal operator only through this narrow function.
grant execute on function ensure_personal_agent(text) to :"role";
-- Events. The outbox is written by `publish`; the publisher may only mark rows sent.
grant select, insert on outbox to :"role";
grant update (sent_at) on outbox to :"role";
grant usage on sequence outbox_seq_seq to :"role";
-- Append-only: who joined and left is never rewritten.
grant select, insert on lifecycle_log to :"role";
-- RLS-free consumer bookkeeping: the core only records; `on conflict do nothing` needs no SELECT.
grant insert on events_inbox, events_quarantine to :"role";
grant usage on sequence events_quarantine_id_seq to :"role";
-- Email (RLS forced on sends): a claim is released (deleted) only when SES refused the send.
grant select, insert, delete on email_sends to :"role";
grant update (sent_at) on email_sends to :"role";
-- Suppressions are only ever added; an address comes off the list by hand.
grant select, insert on email_suppressions to :"role";

-- Conversations (RLS forced). The core moves only a conversation's state, eve session and cursor;
-- relayed events, approval links and calls are append-only.
grant select, insert on conversations to :"role";
grant update (state, eve_session, eve_cursor, updated_at, finished_at) on conversations to :"role";
grant select, insert on conversation_events, conversation_approvals, conversation_calls to :"role";

-- Channels (RLS forced). Installations are created and disabled, never rewritten otherwise.
grant select, insert on channel_installations to :"role";
grant update (agent, credential_ref, disabled_at) on channel_installations to :"role";
grant select, insert, delete on channel_identities, channel_links to :"role";
grant update (attempts) on channel_links to :"role";
grant select, insert on channel_spaces, channel_inbound to :"role";
grant select, insert on channel_threads to :"role";
grant update (conversation, last_inbound_at) on channel_threads to :"role";
grant select, insert, delete on channel_messages to :"role";
grant update (external_ref, sent_at) on channel_messages to :"role";
-- RLS-free routes: ids only; added and removed with their tenant rows, never edited.
grant select, insert, delete on channel_installation_routes, channel_identity_routes to :"role";
-- Settings: changed and reset (deleted) by the people settings.md names.
grant select, insert, delete on settings to :"role";
grant update (value, updated_by, updated_at) on settings to :"role";
-- Turns are appended and settled.
grant select, insert on conversation_turns to :"role";
grant update (settled_at) on conversation_turns to :"role";
