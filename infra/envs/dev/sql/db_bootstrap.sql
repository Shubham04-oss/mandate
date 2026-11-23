-- Run once by the db-bootstrap task (docs/deploy.md step 4) as the master user; safe to rerun.
-- Each role's password is read from its own URL (core/database-url, eve/database-url). The runbook
-- generates hex passwords, so no URL decoding is needed. Nothing here echoes a password.
\getenv app_url APP_DATABASE_URL
\getenv eve_url EVE_DATABASE_URL
select substring(:'app_url' from '^postgres(?:ql)?://mandate_app:([^@/]+)@') as app_pw,
       substring(:'eve_url' from '^postgres(?:ql)?://mandate_eve:([^@/]+)@') as eve_pw,
       not exists (select from pg_roles where rolname = 'mandate_app') as new_app,
       not exists (select from pg_roles where rolname = 'mandate_eve') as new_eve,
       not exists (select from pg_database where datname = 'eve') as new_eve_db
\gset

-- Fail before any statement that would carry a password the URL did not hold.
\if :{?app_pw}
\else
  do $$ begin raise exception 'core/database-url is not postgres://mandate_app:<password>@...'; end $$;
\endif
\if :{?eve_pw}
\else
  do $$ begin raise exception 'eve/database-url is not postgres://mandate_eve:<password>@...'; end $$;
\endif

-- A rerun resets each password to what its secret holds (rotation is: put the new URL, rerun).
-- ALTER leaves out nosuperuser/nobypassrls: only a true superuser may name them, and RDS has none.
\if :new_app
  create role mandate_app login nosuperuser nobypassrls password :'app_pw';
\else
  alter role mandate_app login password :'app_pw';
\endif
\if :new_eve
  create role mandate_eve login nosuperuser nobypassrls password :'eve_pw';
\else
  alter role mandate_eve login password :'eve_pw';
\endif

-- mandate: only mandate_app (and the master) may connect; app_role.sql grants its table rights.
revoke all on database mandate from public;
revoke all on schema public from public;
grant connect on database mandate to mandate_app;

-- eve: only mandate_eve; it creates its own schemas (workflow, graphile_worker) and tables in public.
\if :new_eve_db
  create database eve;
\endif
revoke all on database eve from public;
grant connect, temporary, create on database eve to mandate_eve;
\connect eve
revoke all on schema public from public;
grant usage, create on schema public to mandate_eve;
