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

# Verify the actual helper command for both CI verbosity modes. A clippy
# invocation without --locked can rewrite Cargo.lock after provider artifact
# generation, making the consumer's checkout digest diverge.
for verbose in 0 1; do
  for supplied_locked in 0 1; do
    arguments=(-p oasis7_net --tests)
    if [[ "$supplied_locked" == 1 ]]; then
      arguments=(--locked "${arguments[@]}")
    fi
    actual=$(
      run() { printf '%s\n' "$*"; }
      eval "$clippy_helper"
      CI_VERBOSE="$verbose" run_cargo_clippy "${arguments[@]}"
    )
    expected="cargo clippy"
    if [[ "$verbose" == 1 ]]; then
      expected+=" --verbose"
    fi
    expected+=" --locked -p oasis7_net --tests -- -D warnings -D clippy::correctness -D clippy::suspicious"
    if [[ "$actual" != *"$expected" ]]; then
      echo "ci-tests Clippy must preserve the lockfile and warning-zero flags in every mode: $actual" >&2
      exit 1
    fi
  done
done

echo "ci-tests warning-zero and lockfile contract: passed"
