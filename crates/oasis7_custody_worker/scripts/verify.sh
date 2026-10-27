#!/usr/bin/env bash
# Full role/profile verification; never builds the unsupported --all-features artifact.
set -euo pipefail
cd "$(dirname "$0")/../../.."
env -u RUSTC_WRAPPER cargo test -p oasis7_custody_core -p oasis7_custody_worker --locked
env -u RUSTC_WRAPPER cargo check -p oasis7_custody_core --target wasm32-unknown-unknown --locked
scratch=$(mktemp -d)
trap 'rm -rf "$scratch"' EXIT
if env -u RUSTC_WRAPPER cargo check -p oasis7_custody_worker --target wasm32-unknown-unknown --no-default-features --locked >"$scratch/no-role" 2>&1; then
  echo 'invalid role-less Worker unexpectedly built' >&2; exit 1
fi
grep -q 'requires exactly one role feature' "$scratch/no-role"
for first in identity owner executor recovery-fence; do
  for second in identity owner executor recovery-fence; do
    [[ "$first" < "$second" ]] || continue
    if env -u RUSTC_WRAPPER cargo check -p oasis7_custody_worker --target wasm32-unknown-unknown --no-default-features --features "$first,$second" --locked >"$scratch/multi-role" 2>&1; then
      echo 'invalid multi-role Worker unexpectedly built' >&2; exit 1
    fi
    grep -q 'requires exactly one role feature' "$scratch/multi-role"
  done
done
if env -u RUSTC_WRAPPER cargo check -p oasis7_custody_worker --target wasm32-unknown-unknown --no-default-features --features identity,approval-harness --locked >"$scratch/harness-without-owner" 2>&1; then
  echo 'approval harness without Owner unexpectedly built' >&2; exit 1
fi
grep -q 'approval-harness requires owner' "$scratch/harness-without-owner"
for role in owner executor recovery-fence; do
  if env -u RUSTC_WRAPPER cargo check -p oasis7_custody_worker --target wasm32-unknown-unknown --no-default-features --features "$role,native-runtime-harness" --locked >"$scratch/native-harness-$role" 2>&1; then
    echo 'native runtime harness without Identity unexpectedly built' >&2; exit 1
  fi
  grep -q 'native-runtime-harness requires identity' "$scratch/native-harness-$role"
done
for role in identity owner executor recovery-fence; do
  env -u RUSTC_WRAPPER cargo check -p oasis7_custody_worker --target wasm32-unknown-unknown --no-default-features --features "$role" --locked
done
case "${1:-}" in
  '') ;;
  --platform)
    if ! command -v worker-build >/dev/null || [[ "$(worker-build --version)" != "0.7.5" ]]; then
      cargo install worker-build --version 0.7.5 --locked
    fi
    npm ci --prefix crates/oasis7_custody_worker --ignore-scripts
    for role in identity owner executor recovery-fence; do
      bash crates/oasis7_custody_worker/scripts/build-role.sh "$role"
      (cd crates/oasis7_custody_worker && npx --no-install wrangler deploy --dry-run --config "deploy/$role/wrangler.toml" --outdir "artifacts/$role/dry-run" --no-bundle)
    done
    bash crates/oasis7_custody_worker/scripts/build-approval-harness.sh
    bash crates/oasis7_custody_worker/scripts/build-native-runtime-harness.sh
    # Execute native custody regressions, including the Provision bootstrap wire
    # contract; compiling the HTTP fixture alone does not run these assertions.
    env -u RUSTC_WRAPPER cargo test -p oasis7_launcher_backend --bin oasis7_game_launcher hosted_identity::custody::tests --locked
    bash crates/oasis7_custody_worker/scripts/build-native-fixture.sh
    bash crates/oasis7_custody_worker/scripts/build-world-fixture.sh
    node crates/oasis7_custody_worker/scripts/contract.mjs check
    npm test --prefix crates/oasis7_custody_worker
    ;;
  *) echo 'usage: verify.sh [--platform]' >&2; exit 2 ;;
esac
