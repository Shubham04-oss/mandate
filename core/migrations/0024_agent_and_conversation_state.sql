-- Why an agent is disabled, so a member who returns gets back the operator their removal disabled
-- but not one an admin disabled. Agents disabled before this keep no reason and stay disabled.
alter table agents add column disabled_reason text check (disabled_reason in ('owner_removed', 'admin'));

-- When a conversation finished (eve ended its session or never started it), so
-- `conversation.finished.v1` is published once however often the end is seen.
alter table conversations add column finished_at timestamptz;
