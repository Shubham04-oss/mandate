#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
if [ -n "${DATABASE_URL:-}" ]; then
  cargo test --workspace -- --ignored
fi
if cargo deny --version >/dev/null 2>&1; then
  cargo deny check
else
  echo "cargo deny not installed; skipping licence and advisory checks"
fi
