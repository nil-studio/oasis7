#!/usr/bin/env bash
# Full role/profile verification; never builds the unsupported --all-features artifact.
set -euo pipefail
cd "$(dirname "$0")/../../.."
usage() {
  echo 'usage: verify.sh [--platform | --stage <native|native-contracts|native-fixtures|wasm-production|wasm-approval>]' >&2
  exit 2
}
mode=checks
if [[ "$#" == 1 && "$1" == --platform ]]; then
  mode=platform
elif [[ "$#" == 2 && "$1" == --stage ]]; then
  case "$2" in
    native|native-contracts|native-fixtures|wasm-production|wasm-approval) mode=$2 ;;
    *) usage ;;
  esac
elif [[ "$#" != 0 ]]; then
  usage
fi
if [[ "${OASIS7_SUPPORT_CAPACITY:-0}" == 1 ]]; then
  source scripts/ci-tests.sh
fi
scratch=$(mktemp -d)
trap 'rm -rf "$scratch"' EXIT
case "$mode" in
  checks|platform|native|native-contracts)
    env -u RUSTC_WRAPPER cargo test -p oasis7_custody_core -p oasis7_custody_worker --locked
    ;;
esac
[[ "$mode" != native-contracts ]] || exit 0
case "$mode" in
  # Keep the complete role/profile proof on the approval runner, which also
  # executes the trusted dynamic WASM profiles with the same dev dependencies.
  checks|platform|wasm-approval)
    env -u RUSTC_WRAPPER cargo check -p oasis7_custody_core --target wasm32-unknown-unknown --locked
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
    ;;
esac
[[ "$mode" != checks ]] || exit 0

# Each stage owns a disjoint artifact set. Separate runners must execute the two
# WASM producers: worker-build uses one locked build/ directory per checkout.
case "$mode" in
  platform|wasm-production|wasm-approval)
    worker_build_bin="$(python3 scripts/ci-worker-build.py ensure --print-bin)"
    export PATH="$(dirname "$worker_build_bin"):$PATH"
    npm ci --prefix crates/oasis7_custody_worker --ignore-scripts
    [[ "${OASIS7_SUPPORT_CAPACITY:-0}" != 1 ]] || support_capacity_snapshot custody-platform-dependencies
    ;;
esac
case "$mode" in
  platform|wasm-production)
    for role in identity owner executor recovery-fence; do
      bash crates/oasis7_custody_worker/scripts/build-role.sh "$role"
      (cd crates/oasis7_custody_worker && npx --no-install wrangler deploy --dry-run --config "deploy/$role/wrangler.toml" --outdir "artifacts/$role/dry-run" --no-bundle)
    done
    [[ "${OASIS7_SUPPORT_CAPACITY:-0}" != 1 ]] || support_capacity_snapshot custody-roles-complete
    ;;
esac
case "$mode" in
  platform|wasm-approval)
    bash crates/oasis7_custody_worker/scripts/build-approval-harness.sh
    ;;
esac
case "$mode" in
  platform|wasm-production)
    bash crates/oasis7_custody_worker/scripts/build-native-runtime-harness.sh
    ;;
esac
case "$mode" in
  platform|wasm-production|wasm-approval)
    [[ "${OASIS7_SUPPORT_CAPACITY:-0}" != 1 ]] || support_capacity_snapshot custody-wasm-complete
    ;;
esac
case "$mode" in
  platform|native|native-fixtures)
    # Execute native custody regressions, including the Provision bootstrap wire
    # contract; compiling the HTTP fixture alone does not run these assertions.
    env -u RUSTC_WRAPPER cargo test -p oasis7_launcher_backend --bin oasis7_game_launcher hosted_identity::custody::tests --locked
    [[ "${OASIS7_SUPPORT_CAPACITY:-0}" != 1 ]] || support_capacity_snapshot custody-native-complete
    bash crates/oasis7_custody_worker/scripts/build-native-fixture.sh
    [[ "${OASIS7_SUPPORT_CAPACITY:-0}" != 1 ]] || support_capacity_snapshot custody-native-fixture-complete
    bash crates/oasis7_custody_worker/scripts/build-world-fixture.sh
    [[ "${OASIS7_SUPPORT_CAPACITY:-0}" != 1 ]] || support_capacity_snapshot custody-world-fixture-complete
    # Exercise the opt-in real commit barrier; compiling its fixture is not evidence.
    world_fixture=$(cat crates/oasis7_custody_worker/artifacts/world-test-binary.txt)
    world_regression=custody_happy_submit_barrier_preserves_unknown_and_settles_exact_originals
    world_case=$("$world_fixture" --list | sed -n "s/^\(.*::${world_regression}\): test$/\1/p")
    if [[ -z "$world_case" || "$world_case" == *$'\n'* ]]; then
      echo 'expected exactly one real custody commit-barrier regression' >&2; exit 1
    fi
    "$world_fixture" --exact "$world_case" --nocapture | tee "$scratch/world-commit-barrier.log"
    grep -Eq '^test result: ok\. 1 passed; 0 failed; 0 ignored;' "$scratch/world-commit-barrier.log"
    node crates/oasis7_custody_worker/scripts/contract.mjs check
    bash crates/oasis7_custody_worker/scripts/export-context-body-fixture.sh
    [[ "${OASIS7_SUPPORT_CAPACITY:-0}" != 1 ]] || support_capacity_snapshot custody-platform-before-tests
    ;;
esac
case "$mode" in
  platform)
    npm test --prefix crates/oasis7_custody_worker
    [[ "${OASIS7_SUPPORT_CAPACITY:-0}" != 1 ]] || support_capacity_snapshot custody-platform-complete
    ;;
esac
