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
