-- A personal operator gets a name to show, so the web app never shows its id. Existing ones are
-- not backfilled: RLS is forced on agents, so a migration run as a non-superuser owner sees no rows.
create or replace function ensure_personal_agent(p_member text) returns void
language sql security definer set search_path = public, pg_temp as $$
  insert into agents (tenant_id, id, kind, owner_member, name)
  select current_setting('mandate.tenant'),
         'op-' || left(encode(sha256(convert_to(p_member, 'UTF8')), 'hex'), 32), 'personal', p_member,
         'Personal agent'
  where exists (select 1 from members m
                where m.tenant_id = current_setting('mandate.tenant') and m.id = p_member)
    and not exists (select 1 from agents g
                    where g.tenant_id = current_setting('mandate.tenant')
                    and g.kind = 'personal' and g.owner_member = p_member)
  on conflict do nothing
$$;
revoke all on function ensure_personal_agent(text) from public;
