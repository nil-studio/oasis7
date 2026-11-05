#!/usr/bin/env bash
# Component-only physical observations; never referenced by deployment configuration.
set -euo pipefail
role="${1:?usage: build-owner-setup-trace-harness.sh owner|identity|recovery-fence}"
case "$role" in owner|identity|recovery-fence) ;; *) echo "invalid trace role" >&2; exit 2 ;; esac
cd "$(dirname "$0")/.."
if [[ "$(worker-build --version)" != "0.7.5" ]]; then
  echo "the pinned worker-build 0.7.5 is required" >&2
  exit 2
fi
mkdir .role-build-lock 2>/dev/null || { echo "another role build is running" >&2; exit 2; }
trap 'rmdir .role-build-lock' EXIT
env -u RUSTC_WRAPPER worker-build --release --no-opt -- --locked --no-default-features --features "$role,owner-setup-physical-trace-harness"
output="artifacts/owner-setup-trace-$role"
mkdir -p artifacts
if [[ -e "$output" ]]; then rm -rf "$output"; fi
cp -R build "$output"
case "$role" in
  owner|identity) cp "platform/$role-entry.mjs" "$output/entry.mjs" ;;
  recovery-fence) cp platform/fence-entry.mjs "$output/entry.mjs" ;;
esac
