#!/usr/bin/env bash
# Fails on a destructive statement (DROP, RENAME, ALTER ... TYPE) in the given migration files:
# a circuit-breaker rollback runs the previous code on the new schema (docs/deploy.md, Migrations).
# Usage: scripts/lint-migrations.sh FILE...
set -euo pipefail
status=0
for f in "$@"; do
  # One statement per line, comments dropped, so ALTER ... TYPE is caught across line breaks.
  # ponytail: a ';' or '--' inside a string or $$ body is split as if it were SQL; review those by eye.
  stmts="$(sed 's/--.*//' "$f" | tr '\n' ' ' | tr ';' '\n' | sed 's/^ *//')"
  hits="$(printf '%s\n' "$stmts" | grep -Eiw 'drop|rename|alter.*type' || true)"
  if [ -n "$hits" ]; then
    printf '%s\n' "$hits" | while IFS= read -r stmt; do echo "::error file=$f::destructive statement: $stmt"; done
    status=1
  fi
done
exit "$status"
