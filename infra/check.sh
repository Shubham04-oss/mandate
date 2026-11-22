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
# Secret values are set by the owner with the AWS CLI, never through tofu, so they never reach state.
if grep -rnE 'aws_secretsmanager_secret_version|master_password *=' --include='*.tf' .; then
  echo "error: a secret value would be stored in tofu state" >&2
  exit 1
fi
