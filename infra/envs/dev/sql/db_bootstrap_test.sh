#!/usr/bin/env bash
# Runs db_bootstrap.sql against a scratch Postgres the way the db-bootstrap task does.
# DESTRUCTIVE: drops and recreates the mandate and eve databases and the mandate_* roles on the server
# INFRA_TEST_DATABASE_URL names (a superuser URL), so never point it at anything you keep.
set -euo pipefail
: "${INFRA_TEST_DATABASE_URL:?set INFRA_TEST_DATABASE_URL to a scratch superuser URL}"
cd "$(dirname "$0")"
admin() { psql "$INFRA_TEST_DATABASE_URL" -X -q -v ON_ERROR_STOP=1 -tA "$@"; }
# Later connections log in as other roles to the same server.
PGHOST=$(admin -c 'select host(inet_server_addr())') PGPORT=$(admin -c 'select inet_server_port()')
export PGHOST PGPORT
export PGSSLMODE=disable

# A non-superuser master with createrole and createdb, as on RDS.
admin -c 'drop database if exists eve' -c 'drop database if exists mandate'
admin -c 'drop role if exists mandate_app' -c 'drop role if exists mandate_eve' -c 'drop role if exists mandate_migrator'
admin -c "create role mandate_migrator login createrole createdb password 'migrator'"
admin -c 'create database mandate owner mandate_migrator'

APP=0123456789abcdef0123456789abcdef0123456789abcdef
EVE=fedcba9876543210fedcba9876543210fedcba9876543210
url() { echo "postgres://$1:$2@db.example:5432/$3?sslmode=verify-full&sslrootcert=/tmp/rds-ca.pem"; }
bootstrap() {
  PGUSER=mandate_migrator PGPASSWORD=migrator PGDATABASE=mandate psql -X -q -v ON_ERROR_STOP=1 -f db_bootstrap.sql
}
fail() { echo "db_bootstrap_test: $*" >&2; exit 1; }
roles() { admin -c "select count(*) from pg_roles where rolname in ('mandate_app', 'mandate_eve')"; }
login() { PGPASSWORD="$2" psql -X -tA -U "$1" -d "$3" -c 'select current_user' 2>/dev/null; }

# Missing and malformed URLs fail before any role exists.
(unset APP_DATABASE_URL; EVE_DATABASE_URL=$(url mandate_eve "$EVE" eve) bootstrap 2>/dev/null) && fail "missing URL passed"
APP_DATABASE_URL=$(url mandate_app "not-hex-password-not-hex-password" mandate) EVE_DATABASE_URL=$(url mandate_eve "$EVE" eve) \
  bootstrap 2>/dev/null && fail "non-hex password passed"
APP_DATABASE_URL=$(url mandate_app "$APP" mandate) EVE_DATABASE_URL="postgres://mandate_eve@db.example/eve" \
  bootstrap 2>/dev/null && fail "URL without a password passed"
[ "$(roles)" = 0 ] || fail "a failed run created a role"

# A good run, then a rerun that rotates the app password.
export APP_DATABASE_URL EVE_DATABASE_URL
APP_DATABASE_URL=$(url mandate_app "$APP" mandate) EVE_DATABASE_URL=$(url mandate_eve "$EVE" eve)
bootstrap >/dev/null || fail "good run failed"
APP=abcdef0123456789abcdef0123456789abcdef0123456789
APP_DATABASE_URL=$(url mandate_app "$APP" mandate)
bootstrap >/dev/null || fail "rerun failed"

[ "$(login mandate_app "$APP" mandate)" = mandate_app ] || fail "mandate_app cannot log in with the rotated password"
[ "$(login mandate_eve "$EVE" eve)" = mandate_eve ] || fail "mandate_eve cannot log in to eve"
[ -z "$(login mandate_eve "$EVE" mandate)" ] || fail "mandate_eve reached mandate"
[ -z "$(login mandate_app "$APP" eve)" ] || fail "mandate_app reached eve"
PGPASSWORD="$EVE" psql -X -q -v ON_ERROR_STOP=1 -U mandate_eve -d eve \
  -c 'create schema workflow' -c 'create table public.t (i int)' -c 'create temp table x (i int)' || fail "mandate_eve cannot build its store"
PGPASSWORD="$APP" psql -X -q -U mandate_app -d mandate -c 'create table t (i int)' 2>/dev/null && fail "mandate_app may create in public"
echo "db_bootstrap_test: ok"
