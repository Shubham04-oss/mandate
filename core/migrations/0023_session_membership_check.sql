-- When each session's WorkOS membership was last confirmed, and last asked about: the core
-- re-checks role and status every 5 minutes and, while WorkOS is down, serves at most 1 hour past
-- the last good check. Existing sessions count as checked now, so their first re-check is soon.
alter table sessions add column verified_at timestamptz not null default now();
alter table sessions add column checked_at timestamptz not null default now();
