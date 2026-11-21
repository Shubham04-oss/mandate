-- The app role may insert agents only to create a member's personal operator (directory sync,
-- first sign-in); team bots and agents for non-members need a role that bypasses RLS.
create policy app_creates_only_personal_operators on agents as restrictive for insert
  with check (kind = 'personal' and exists (select 1 from members m
    where m.tenant_id = agents.tenant_id and m.id = agents.owner_member));
