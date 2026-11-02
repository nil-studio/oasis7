#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
if [[ "$(worker-build --version)" != "0.7.5" ]]; then
  echo 'Install pinned worker-build 0.7.5 before building execution signer fixtures' >&2
  exit 2
fi
mkdir .role-build-lock 2>/dev/null || { echo 'another role build is running' >&2; exit 2; }
trap 'rmdir .role-build-lock' EXIT
# Isolated test artifact, never an ordinary Executor/Identity deployment build.
env -u RUSTC_WRAPPER worker-build --release --no-opt -- --locked --no-default-features --features executor,execution-signer-harness
mkdir -p artifacts
if [[ -e artifacts/execution-signer-harness ]]; then rm -rf artifacts/execution-signer-harness; fi
cp -R build artifacts/execution-signer-harness
cp platform/execution_signer_gateway_fixture_v2.mjs artifacts/execution-signer-harness/entry.mjs
