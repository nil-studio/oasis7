#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
ci_tests="$repo_root/scripts/ci-tests.sh"
clippy_helper=$(sed -n '/^run_cargo_clippy()/,/^}/p' "$ci_tests")

if ! grep -Fqx '    -D warnings' <<<"$clippy_helper"; then
  echo "ci-tests Clippy helper must deny every Rust and Clippy warning" >&2
  exit 1
fi

if [[ $(grep -Fxc '    -D warnings' <<<"$clippy_helper") -ne 1 ]]; then
  echo "ci-tests Clippy helper must define the warning-zero gate exactly once" >&2
  exit 1
fi

# Exercise real helper calls in local checks, required artifact consumer,
# ordinary CI groups, and an unrelated required partition. Only the consumer
# and local mode add an implicit lock; caller-supplied --locked is preserved.
for context in consumer local ordinary controls; do
  for verbose in 0 1; do
    for supplied_locked in 0 1; do
      arguments=(-p oasis7_net --tests)
      if [[ "$supplied_locked" == 1 ]]; then
        arguments=(--locked "${arguments[@]}")
      fi
      actions=true job=workflow-governance partition=all local_check=0
      case "$context" in
        consumer) job=oasis7-required ;;
        controls) job=oasis7-required; partition=controls ;;
        local) local_check=1 ;;
      esac
      actual=$(
        run() { printf '%s\n' "$*"; }
        eval "$clippy_helper"
        GITHUB_ACTIONS="$actions" GITHUB_JOB="$job" ordinary_partition="$partition" \
          OASIS7_LOCAL_CHECK="$local_check" CI_VERBOSE="$verbose" \
          run_cargo_clippy "${arguments[@]}"
      )
      expected="cargo clippy"
      if [[ "$verbose" == 1 ]]; then
        expected+=" --verbose"
      fi
      if [[ "$context" == consumer || "$context" == local || "$supplied_locked" == 1 ]]; then
        expected+=" --locked"
      fi
      expected+=" -p oasis7_net --tests -- -D warnings -D clippy::correctness -D clippy::suspicious"
      if [[ "$actual" != *"$expected" ]]; then
        echo "ci-tests Clippy lockfile/warning-zero contract failed ($context, $verbose, $supplied_locked): $actual" >&2
        exit 1
      fi
    done
  done
done

echo "ci-tests warning-zero and artifact-consumer lockfile contract: passed"
