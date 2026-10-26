#!/usr/bin/env bash
# Component-only artifact: never referenced by a deployment configuration.
set -euo pipefail
cd "$(dirname "$0")/.."
if [[ "$(worker-build --version)" != "0.7.5" ]]; then
  echo 'Install the pinned builder: cargo install worker-build --version 0.7.5 --locked' >&2
  exit 2
fi
mkdir .role-build-lock 2>/dev/null || { echo 'another role build is running' >&2; exit 2; }
trap 'rmdir .role-build-lock' EXIT
mkdir -p artifacts
OASIS7_CUSTODY_ADMISSION_FIXTURE_OUTPUT="$PWD/artifacts/approval-fixture.json" env -u RUSTC_WRAPPER cargo test -p oasis7_custody_core --test approval_admission export_developer_admission_fixture --locked
env -u RUSTC_WRAPPER worker-build --release --no-opt -- --locked --no-default-features --features owner,approval-harness
mkdir -p artifacts
if [[ -e artifacts/approval-harness ]]; then rm -rf artifacts/approval-harness; fi
cp -R build artifacts/approval-harness
