#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
[[ "$(worker-build --version)" == "0.7.5" ]] || { echo 'worker-build 0.7.5 required' >&2; exit 2; }
mkdir .build-lock 2>/dev/null || { echo 'another package build is running' >&2; exit 2; }
trap 'rmdir .build-lock' EXIT
env -u RUSTC_WRAPPER CARGO_INCREMENTAL=0 worker-build --release --no-opt -- --locked
cp platform/entry.mjs build/entry.mjs
