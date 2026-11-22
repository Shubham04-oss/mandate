-- No tenant RLS: consumer bookkeeping keyed by globally unique event ids, written inside the
-- handler's tenant transaction so an event is handled and marked exactly together.
-- ponytail: never pruned; rows older than the 24 h replay window could go once that matters.
create table events_inbox (
  consumer text not null,
  event_id uuid not null,
  at timestamptz not null default now(),
  primary key (consumer, event_id)
);
-- No tenant RLS: a message that fails verification has no tenant we can trust. Only a digest
-- and the reason are kept; the content may be attacker text.
create table events_quarantine (
  id bigserial primary key,
  consumer text not null,
  digest bytea not null,
  reason text not null,
  at timestamptz not null default now()
);
