#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

base_oid="$(git rev-parse HEAD^)"
head_oid="$(git rev-parse HEAD)"
tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

if ! python3 ./scripts/product-doc-content-check.py --repo-root "$PWD" --full-corpus >"$tmp_dir/full-corpus.out" 2>&1; then
  echo "product-doc-content-callers.test: governed full corpus unexpectedly failed" >&2
  cat "$tmp_dir/full-corpus.out" >&2
  exit 1
fi
grep -Fq 'product-doc-content: OK (full-corpus checked ' "$tmp_dir/full-corpus.out"

if (unset OASIS7_PRODUCT_DOC_BASE OASIS7_PRODUCT_DOC_HEAD; OASIS7_PRODUCT_DOC_BASE="$base_oid" ./scripts/doc-governance-check.sh) >"$tmp_dir/partial.out" 2>&1; then
  echo "product-doc-content-callers.test: partial local base/head unexpectedly passed" >&2
  exit 1
fi
grep -Fq "explicit base/head must be supplied together" "$tmp_dir/partial.out"

if (unset OASIS7_PRODUCT_DOC_BASE OASIS7_PRODUCT_DOC_HEAD; OASIS7_PRODUCT_DOC_HEAD="$head_oid" ./scripts/doc-governance-check.sh) >"$tmp_dir/partial-head.out" 2>&1; then
  echo "product-doc-content-callers.test: partial head/base unexpectedly passed" >&2
  exit 1
fi
grep -Fq "explicit base/head must be supplied together" "$tmp_dir/partial-head.out"

if python3 ./scripts/product-doc-content-check.py --base HEAD --head "$head_oid" >"$tmp_dir/label.out" 2>&1; then
  echo "product-doc-content-callers.test: symbolic checker identity unexpectedly passed" >&2
  exit 1
fi
grep -Fq "must be a full 40-character commit OID" "$tmp_dir/label.out"

range_function="$(sed -n '/^product_doc_range()/,/^run_product_doc_governance_check()/p' ./scripts/ci-tests.sh | sed '$d')"
eval "$range_function"
printf '{"pull_request":{"base":{"sha":"%s"},"head":{"sha":"%s"}}}\n' "$base_oid" "$head_oid" >"$tmp_dir/pull_request.json"
valid_range="$(unset OASIS7_PRODUCT_DOC_BASE OASIS7_PRODUCT_DOC_HEAD; GITHUB_EVENT_PATH="$tmp_dir/pull_request.json" GITHUB_EVENT_NAME=pull_request GITHUB_SHA="$head_oid" product_doc_range)"
[[ "$valid_range" == "$base_oid
$head_oid" ]]

governance_function="$(sed -n '/^run_product_doc_governance_check()/,/^run_standalone_tool_lockfiles_checks()/p' ./scripts/ci-tests.sh | sed '$d')"
eval "$governance_function"
run() {
  printf '%s\n' "$*" >>"$tmp_dir/governance-calls.out"
  printf '%s\n%s\n' "${OASIS7_PRODUCT_DOC_BASE:-}" "${OASIS7_PRODUCT_DOC_HEAD:-}" >>"$tmp_dir/governance-range.out"
}

# Required fast feedback keeps the exact paired range; hosted full still runs
# the full-corpus report through the same baseline job and selected profile.
for mode in required: required:pr-fast required:main-fast required:periodic-full required:candidate-full full: full-standard: commit:; do
  test_tier="${mode%%:*}"
  test_profile="${mode#*:}"
  : >"$tmp_dir/governance-calls.out"
  : >"$tmp_dir/governance-range.out"
  tier="$test_tier" OASIS7_CI_PROFILE="$test_profile" \
    OASIS7_PRODUCT_DOC_BASE="$base_oid" OASIS7_PRODUCT_DOC_HEAD="$head_oid" \
    run_product_doc_governance_check
  expected='./scripts/doc-governance-check.sh --full-corpus'
  if [[ "$test_tier" == required && "$test_profile" != periodic-full && "$test_profile" != candidate-full ]]; then
    expected='./scripts/doc-governance-check.sh'
  fi
  grep -Fqx "$expected" "$tmp_dir/governance-calls.out"
  [[ "$(wc -l <"$tmp_dir/governance-calls.out" | tr -d ' ')" == 1 ]]
  [[ "$(cat "$tmp_dir/governance-range.out")" == "$base_oid
$head_oid" ]]
done

# The legacy event-derived range follows the same quick path and exports the
# resolved pair to the checker rather than losing it while choosing a profile.
: >"$tmp_dir/governance-calls.out"
: >"$tmp_dir/governance-range.out"
(unset OASIS7_PRODUCT_DOC_BASE OASIS7_PRODUCT_DOC_HEAD; \
  tier=required OASIS7_CI_PROFILE=pr-fast GITHUB_EVENT_PATH="$tmp_dir/pull_request.json" \
    GITHUB_EVENT_NAME=pull_request GITHUB_SHA="$head_oid" run_product_doc_governance_check)
grep -Fqx './scripts/doc-governance-check.sh' "$tmp_dir/governance-calls.out"
[[ "$(cat "$tmp_dir/governance-range.out")" == "$base_oid
$head_oid" ]]

# Neither quick nor full may turn an incomplete explicit pair into an all-doc
# fallback; the governance command must not run after range validation fails.
for test_profile in pr-fast periodic-full candidate-full; do
  for partial in OASIS7_PRODUCT_DOC_BASE OASIS7_PRODUCT_DOC_HEAD; do
    : >"$tmp_dir/governance-calls.out"
    if (unset OASIS7_PRODUCT_DOC_BASE OASIS7_PRODUCT_DOC_HEAD; \
        export "$partial=$head_oid"; \
        tier=required OASIS7_CI_PROFILE="$test_profile" run_product_doc_governance_check) \
        >"$tmp_dir/partial-driver.out" 2>&1; then
      echo "product-doc-content-callers.test: $test_profile accepted partial $partial" >&2
      exit 1
    fi
    grep -Fq 'explicit base/head must be supplied together' "$tmp_dir/partial-driver.out"
    [[ ! -s "$tmp_dir/governance-calls.out" ]]
  done
done

# An explicitly local invocation without any CI range retains full coverage.
: >"$tmp_dir/governance-calls.out"
(unset OASIS7_PRODUCT_DOC_BASE OASIS7_PRODUCT_DOC_HEAD GITHUB_EVENT_PATH GITHUB_EVENT_NAME; \
  CI=false GITHUB_ACTIONS=false tier=required OASIS7_CI_PROFILE=pr-fast run_product_doc_governance_check)
grep -Fqx './scripts/doc-governance-check.sh --full-corpus' "$tmp_dir/governance-calls.out"

printf '{}\n' >"$tmp_dir/empty.json"
if (unset OASIS7_PRODUCT_DOC_BASE OASIS7_PRODUCT_DOC_HEAD; GITHUB_EVENT_PATH="$tmp_dir/empty.json" GITHUB_EVENT_NAME=unknown GITHUB_SHA="$head_oid" product_doc_range) >"$tmp_dir/event.out" 2>&1; then
  echo "product-doc-content-callers.test: malformed CI event unexpectedly fell back" >&2
  exit 1
fi
grep -Fq "unsupported CI event range" "$tmp_dir/event.out"

printf '{"before":"%s"}\n' "$base_oid" >"$tmp_dir/push-missing-head.json"
if (unset OASIS7_PRODUCT_DOC_BASE OASIS7_PRODUCT_DOC_HEAD; GITHUB_EVENT_PATH="$tmp_dir/push-missing-head.json" GITHUB_EVENT_NAME=push GITHUB_SHA="$head_oid" product_doc_range) >"$tmp_dir/push.out" 2>&1; then
  echo "product-doc-content-callers.test: incomplete push event unexpectedly fell back" >&2
  exit 1
fi
grep -Fq "did not provide both base/head OIDs" "$tmp_dir/push.out"

printf '{}\n' >"$tmp_dir/schedule.json"
if (unset OASIS7_PRODUCT_DOC_BASE OASIS7_PRODUCT_DOC_HEAD; GITHUB_EVENT_PATH="$tmp_dir/schedule.json" GITHUB_EVENT_NAME=schedule GITHUB_SHA="$head_oid" product_doc_range) >"$tmp_dir/schedule.out" 2>&1; then
  echo "product-doc-content-callers.test: schedule event unexpectedly supplied a range" >&2
  exit 1
fi
grep -Fq "unsupported CI event range: schedule" "$tmp_dir/schedule.out"

printf '{}\n' >"$tmp_dir/manual.json"
if (unset OASIS7_PRODUCT_DOC_BASE OASIS7_PRODUCT_DOC_HEAD; GITHUB_EVENT_PATH="$tmp_dir/manual.json" GITHUB_EVENT_NAME=workflow_dispatch GITHUB_SHA="$head_oid" product_doc_range) >"$tmp_dir/manual.out" 2>&1; then
  echo "product-doc-content-callers.test: manual CI without an explicit range unexpectedly passed" >&2
  exit 1
fi
grep -Fq "unsupported CI event range: workflow_dispatch" "$tmp_dir/manual.out"

if (unset OASIS7_PRODUCT_DOC_BASE OASIS7_PRODUCT_DOC_HEAD; CI=true GITHUB_ACTIONS=true GITHUB_EVENT_PATH="" GITHUB_EVENT_NAME="" product_doc_range) >"$tmp_dir/no-event.out" 2>&1; then
  echo "product-doc-content-callers.test: CI without event/range unexpectedly fell back" >&2
  exit 1
fi
grep -Fq "CI requires explicit base/head OIDs" "$tmp_dir/no-event.out"

grep -Fq 'run_system_design_traceability_check()' ./scripts/doc-governance-check.sh

sed -n '/^  full-regression:/,/^  newapi-bridge-package:/p' .github/workflows/rust.yml >"$tmp_dir/full-regression.yml"
grep -Fq 'fetch-depth: 0' "$tmp_dir/full-regression.yml"
grep -Fq 'name: Resolve product-document gate range' "$tmp_dir/full-regression.yml"
grep -Fq "git rev-parse --verify 'HEAD^1'" "$tmp_dir/full-regression.yml"
grep -Fq "git rev-parse --verify 'HEAD^{commit}'" "$tmp_dir/full-regression.yml"
grep -Fq 'OASIS7_PRODUCT_DOC_BASE=' "$tmp_dir/full-regression.yml"
grep -Fq 'OASIS7_PRODUCT_DOC_HEAD=' "$tmp_dir/full-regression.yml"
grep -Fq 'GITHUB_ENV' "$tmp_dir/full-regression.yml"
echo "product-doc-content-callers.test: OK"
