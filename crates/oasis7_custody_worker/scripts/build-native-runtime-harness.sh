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
# Keep identical release profile flags as build-role.sh so wasm32 deps stay shared.
env -u RUSTC_WRAPPER worker-build --release --no-opt -- \
  --locked --no-default-features --features identity,native-runtime-harness \
  --config 'profile.release.lto="off"' \
  --config 'profile.release.package.oasis7_custody_core.opt-level="s"' \
  --config 'profile.release.package.oasis7_custody_worker.opt-level="s"'
mkdir -p artifacts
if [[ -e artifacts/native-harness ]]; then rm -rf artifacts/native-harness; fi
cp -R build artifacts/native-harness
cp platform/identity-entry.mjs artifacts/native-harness/entry.mjs
cat >> artifacts/native-harness/entry.mjs <<'CONTEXT_FIXTURE_EXPORT'
export { ContextBodyCryptoHarness, ContextBodyChecksHarness } from './index.js';
CONTEXT_FIXTURE_EXPORT
