-- Names the web app shows; display only, never used to decide access.
alter table members add column name text;
alter table teams add column name text;
alter table agents add column name text;
-- A denial is a decision like an approval, so the first decision of either kind wins.
alter table approvals add column denied_by text;
alter table approvals add column denied_at timestamptz;
-- Opaque reference set by the agent lane, so a decision knows which conversation to resume.
alter table approvals add column conversation text;
