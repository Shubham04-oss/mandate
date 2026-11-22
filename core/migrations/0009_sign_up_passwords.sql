-- WorkOS users our sign-up gave a password that its own verify step has not yet confirmed. Any
-- other sign-in for such a user replaces that password first (account pre-hijacking).
-- No tenant RLS: a sign-up has no tenant yet; rows hold only a WorkOS user id and its email.
create table sign_up_passwords (
  workos_user_id text primary key,
  -- Lowercased, to find the row from a password sign-in before WorkOS names the user.
  email text not null
);
create index sign_up_passwords_email on sign_up_passwords (email);
