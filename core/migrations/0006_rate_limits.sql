-- No tenant RLS: sign-in attempts are counted before anyone is known. Keys are SHA-256 hashes of
-- an email, pending id or IP, so the table holds no personal data.
create table rate_limits (
  key bytea not null,
  window_start timestamptz not null,
  count integer not null,
  primary key (key, window_start)
);
create index rate_limits_window on rate_limits (window_start);
