#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
# All commands run against this worktree's canonical Cargo target and lock.
env -u RUSTC_WRAPPER cargo test -p oasis7_dev_test_worker --lib --locked
env -u RUSTC_WRAPPER cargo test -p oasis7_client_storage --example dev_test_worker_material --locked
env -u RUSTC_WRAPPER cargo clippy -p oasis7_dev_test_worker --target wasm32-unknown-unknown --locked --no-deps -- -D warnings
npm ci --ignore-scripts --no-audit --no-fund
./scripts/build.sh
npm test
npx --no-install wrangler deploy --dry-run --outdir build/dry-run
