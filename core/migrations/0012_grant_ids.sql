-- A grant gets an id, so the web app can revoke that one grant and events can name it.
alter table grants add column id uuid not null default gen_random_uuid();
create unique index grants_id on grants (tenant_id, id);
