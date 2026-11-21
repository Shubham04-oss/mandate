#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
if [ -n "${DATABASE_URL:-}" ]; then
  cargo test --workspace -- --ignored
elif command -v "${CVC5:-cvc5}" >/dev/null 2>&1; then
  cargo test -p mandate --test proofs -- --ignored
fi
if ! command -v "${CVC5:-cvc5}" >/dev/null 2>&1; then
  echo "cvc5 not installed; policy proofs did not run (see docs/policy-proofs.md)"
fi
if cargo deny --version >/dev/null 2>&1; then
  cargo deny check
else
  echo "cargo deny not installed; skipping licence and advisory checks"
fi
