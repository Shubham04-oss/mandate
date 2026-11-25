#!/usr/bin/env bash
# Offline checks for infra/: format, validate and the plan-time tests against mock providers.
# Never touches AWS or Cloudflare: no backend, no credentials, no apply.
set -euo pipefail
cd "$(dirname "$0")"
tofu fmt -check -recursive
cd envs/dev
tofu init -backend=false -input=false >/dev/null
tofu validate
tofu test
# Destructive, so only against a scratch server (CI's infra job has one): see the script's header.
if [ -n "${INFRA_TEST_DATABASE_URL:-}" ]; then
  sql/db_bootstrap_test.sh
fi
# Secret values are set by the owner with the AWS CLI, never through tofu, so they never reach state.
# The one exception is the tunnel token: tofu creates the tunnel, so the token is in state anyway.
if grep -rnE 'aws_secretsmanager_secret_version|master_password *=' --include='*.tf' . \
  | grep -v 'resource "aws_secretsmanager_secret_version" "tunnel_token"'; then
  echo "error: a secret value would be stored in tofu state" >&2
  exit 1
fi
