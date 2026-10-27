#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
if [[ "$(worker-build --version)" != "0.7.5" ]]; then
  echo 'Install pinned worker-build 0.7.5 before building native lifecycle fixtures' >&2
  exit 2
fi
mkdir .role-build-lock 2>/dev/null || { echo 'another role build is running' >&2; exit 2; }
trap 'rmdir .role-build-lock' EXIT
# Isolated test artifact; never overwrite artifacts/identity or enable in deployment.
env -u RUSTC_WRAPPER worker-build --release --no-opt -- --locked --no-default-features --features identity,native-runtime-harness
mkdir -p artifacts
if [[ -e artifacts/native-harness ]]; then rm -rf artifacts/native-harness; fi
cp -R build artifacts/native-harness
cp platform/identity-entry.mjs artifacts/native-harness/entry.mjs
