-- Emails our sign-up gave a password that its own verify step has not yet confirmed. Any other
-- sign-in for that user replaces the password first (account pre-hijacking) and marks the row;
-- a password sign-in is refused while any row exists. Only a confirmation or a reset deletes it.
-- No tenant RLS: a sign-up has no tenant yet; rows hold an email and a WorkOS user id.
-- ponytail: abandoned rows (a sign-up nobody ever finishes) are not swept yet; add a sweep if the
-- table grows.
create table sign_up_passwords (
  -- Lowercased; recorded before WorkOS creates the user, so a failure later leaves the guard on.
  email text primary key,
  workos_user_id text unique,
  neutralised_at timestamptz
);

-- end_sessions_for_workos_user(user): deletes that WorkOS user's web sessions in every tenant.
-- It crosses tenants because a password reset must end them all, including tenants WorkOS no
-- longer lists; it does so one tenant at a time, so forced RLS binds it even if its owner is not a
-- superuser. ponytail: one pass over every tenant; fine until there are many thousands.
create function end_sessions_for_workos_user(p_user text) returns void
language plpgsql security definer set search_path = public, pg_temp as $$
declare
  t text;
  prev text := current_setting('mandate.tenant', true);
begin
  for t in select id from tenants loop
    perform set_config('mandate.tenant', t, true);
    delete from sessions s using members m
    where s.tenant_id = t and m.tenant_id = t and m.id = s.member and m.workos_user_id = p_user;
  end loop;
  perform set_config('mandate.tenant', coalesce(prev, ''), true);
end
$$;
revoke all on function end_sessions_for_workos_user(text) from public;
