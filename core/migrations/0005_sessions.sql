-- Web sessions. The cookie carries `<tenant>.<secret>`; only its SHA-256 is stored.
create table sessions (
  tenant_id text not null,
  token_hash bytea not null,
  member text not null,
  created_at timestamptz not null,
  last_seen_at timestamptz not null,
  expires_at timestamptz not null,
  -- What `/v1/me` shows, taken from WorkOS at sign-in: no WorkOS call per request, no WorkOS token kept.
  profile jsonb not null default '{}'::jsonb,
  primary key (tenant_id, token_hash)
);
create index sessions_member on sessions (tenant_id, member);
alter table sessions enable row level security;
alter table sessions force row level security;
create policy tenant_isolation on sessions
  using (tenant_id = current_setting('mandate.tenant', true))
  with check (tenant_id = current_setting('mandate.tenant', true));

-- Multi-step sign-in state, keyed by the SHA-256 of the id the browser holds. WorkOS's pending
-- token stays here and never reaches the browser.
create table auth_pending (
  -- Null before a tenant is known (most sign-in steps); set when a step belongs to a session.
  tenant_id text,
  id_hash bytea primary key,
  kind text not null,
  workos_token text,
  email text,
  data jsonb not null default '{}'::jsonb,
  expires_at timestamptz not null
);
create index auth_pending_expiry on auth_pending (expires_at);
alter table auth_pending enable row level security;
alter table auth_pending force row level security;
-- An untenanted transaction sees only tenantless rows; a tenant's transaction only its own.
create policy tenant_isolation on auth_pending
  using (tenant_id is not distinct from nullif(current_setting('mandate.tenant', true), ''))
  with check (tenant_id is not distinct from nullif(current_setting('mandate.tenant', true), ''));
