-- ensure_personal_agent(member): the member's one personal operator, `op-` + hex of the first 16
-- bytes of sha256(member) (a prefix would collide), unless they have one. The app role may not
-- insert agents (core/sql/app_role.sql); it may only give a member of the transaction's tenant this.
create function ensure_personal_agent(p_member text) returns void
language sql security definer set search_path = public, pg_temp as $$
  insert into agents (tenant_id, id, kind, owner_member)
  select current_setting('mandate.tenant'),
         'op-' || left(encode(sha256(convert_to(p_member, 'UTF8')), 'hex'), 32), 'personal', p_member
  where exists (select 1 from members m
                where m.tenant_id = current_setting('mandate.tenant') and m.id = p_member)
    and not exists (select 1 from agents g
                    where g.tenant_id = current_setting('mandate.tenant')
                    and g.kind = 'personal' and g.owner_member = p_member)
  on conflict do nothing
$$;
revoke all on function ensure_personal_agent(text) from public;
