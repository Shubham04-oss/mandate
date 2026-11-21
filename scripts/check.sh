#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
# In CI (or with REQUIRE_PROOFS=1) a missing prover or licence checker fails instead of skipping.
strict=""
if [ "${REQUIRE_PROOFS:-}" = 1 ] || [ "${CI:-}" = true ]; then
  strict=1
fi
missing() {
  if [ -n "$strict" ]; then
    echo "error: $1 not installed (required when REQUIRE_PROOFS=1 or CI=true)" >&2
    exit 1
  fi
  echo "$1 not installed; $2"
}
cvc5_ok=""
command -v "${CVC5:-cvc5}" >/dev/null 2>&1 && cvc5_ok=1
[ -n "$cvc5_ok" ] || missing cvc5 "policy proofs did not run (see docs/policy-proofs.md)"
deny_ok=""
cargo deny --version >/dev/null 2>&1 && deny_ok=1
[ -n "$deny_ok" ] || missing "cargo deny" "skipping licence and advisory checks"

cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
if [ -n "${DATABASE_URL:-}" ]; then
  cargo test --workspace -- --ignored
elif [ -n "$cvc5_ok" ]; then
  cargo test -p mandate --test proofs -- --ignored
fi
if [ -n "$deny_ok" ]; then
  cargo deny check
fi
