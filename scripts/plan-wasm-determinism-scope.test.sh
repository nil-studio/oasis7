#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

plan_for_path() {
  "$ROOT_DIR/scripts/plan-wasm-determinism-scope.sh" \
    --event-name pull_request \
    --changed-path "$1"
}

value_for_key() {
  local output="$1"
  local key="$2"
  printf '%s\n' "$output" | awk -F= -v key="$key" '$1 == key {print substr($0, length(key) + 2)}'
}

assert_key_equals() {
  local output="$1"
  local key="$2"
  local expected="$3"
  local actual
  actual="$(value_for_key "$output" "$key")"
  if [[ "$actual" != "$expected" ]]; then
    echo "expected $key=$expected, got $actual" >&2
    printf '%s\n' "$output" >&2
    exit 1
  fi
}

assert_reason_contains() {
  local output="$1"
  local expected="$2"
  local actual
  actual="$(value_for_key "$output" reason_summary)"
  if [[ "$actual" != *"$expected"* ]]; then
    echo "expected reason_summary to contain $expected, got $actual" >&2
    printf '%s\n' "$output" >&2
    exit 1
  fi
}

wasm_build_output="$(plan_for_path crates/oasis7_wasm_build/src/lib.rs)"
assert_key_equals "$wasm_build_output" scope all
assert_key_equals "$wasm_build_output" run_all true
assert_key_equals "$wasm_build_output" run_m1 true
assert_key_equals "$wasm_build_output" run_m4 true
assert_key_equals "$wasm_build_output" run_m5 true
assert_key_equals "$wasm_build_output" selected_module_sets m1,m4,m5
assert_reason_contains "$wasm_build_output" "shared_wasm_pipeline:crates/oasis7_wasm_build/src/lib.rs"

sdk_output="$(plan_for_path crates/oasis7_wasm_sdk/src/lib.rs)"
assert_key_equals "$sdk_output" scope all
assert_key_equals "$sdk_output" run_all true
assert_reason_contains "$sdk_output" "shared_wasm_pipeline:crates/oasis7_wasm_sdk/src/lib.rs"

# Artifact ownership moved with Runtime; each module set keeps its exact scope.
for module in m1 m4 m5; do
  for artifact in "${module}_builtin_module_ids.txt" "${module}_builtin_modules.sha256" "${module}_builtin_modules.identity.json"; do
    output="$(plan_for_path "crates/oasis7_runtime/src/world/artifacts/$artifact")"
    assert_key_equals "$output" scope partial
    assert_key_equals "$output" selected_module_sets "$module"
  done
done
output="$(plan_for_path crates/oasis7_runtime/src/world/artifacts/builtin_module_manifest_map.txt)"
assert_key_equals "$output" scope all
assert_key_equals "$output" selected_module_sets m1,m4,m5

unrelated_output="$(plan_for_path doc/engineering/project.md)"
assert_key_equals "$unrelated_output" scope skip
assert_key_equals "$unrelated_output" run_all false
assert_reason_contains "$unrelated_output" "no_builtin_wasm_inputs_changed"

assert_key_equals "$wasm_build_output" selected_module_sets_json '["m1","m4","m5"]'
assert_key_equals "$unrelated_output" selected_module_sets_json '[]'
for module in m1 m4 m5; do
  output="$(plan_for_path "crates/oasis7_builtin_wasm_modules/${module}_example/src/lib.rs")"
  assert_key_equals "$output" scope partial
  assert_key_equals "$output" selected_module_sets "$module"
  assert_key_equals "$output" selected_module_sets_json "[\"$module\"]"
done
output="$("$ROOT_DIR/scripts/plan-wasm-determinism-scope.sh" --changed-path crates/oasis7_builtin_wasm_modules/m5_example/src/lib.rs --changed-path crates/oasis7_builtin_wasm_modules/m1_example/src/lib.rs)"
assert_key_equals "$output" selected_module_sets_json '["m1","m5"]'
output="$("$ROOT_DIR/scripts/plan-wasm-determinism-scope.sh" --changed-path crates/oasis7_builtin_wasm_modules/m5_example/src/lib.rs --changed-path crates/oasis7_builtin_wasm_modules/m4_example/src/lib.rs --changed-path crates/oasis7_builtin_wasm_modules/m1_example/src/lib.rs)"
assert_key_equals "$output" scope partial
assert_key_equals "$output" selected_module_sets_json '["m1","m4","m5"]'
for args in missing zero base head dispatch; do
  case "$args" in
    missing) options=(--event-name pull_request) ;;
    zero) options=(--base-ref 00000000) ;;
    base) options=(--base-ref not-a-real-ref --head-ref HEAD) ;;
    head) options=(--base-ref HEAD --head-ref not-a-real-ref) ;;
    dispatch) options=(--event-name workflow_dispatch --changed-path README.md) ;;
  esac
  output="$("$ROOT_DIR/scripts/plan-wasm-determinism-scope.sh" "${options[@]}")"
  assert_key_equals "$output" scope all
  assert_key_equals "$output" selected_module_sets_json '["m1","m4","m5"]'
done
output="$("$ROOT_DIR/scripts/plan-wasm-determinism-scope.sh" --event-name pull_request --base-ref HEAD --head-ref HEAD)"
assert_key_equals "$output" scope skip
assert_key_equals "$output" selected_module_sets_json '[]'
scratch="$(mktemp -d)"
trap 'rm -rf "$scratch"' EXIT
output="$("$ROOT_DIR/scripts/plan-wasm-determinism-scope.sh" --changed-path README.md --github-output "$scratch/output")"
printf '%s\n' "$output" > "$scratch/stdout"
cmp "$scratch/stdout" "$scratch/output"
if "$ROOT_DIR/scripts/plan-wasm-determinism-scope.sh" --changed-path $'bad\npath' --github-output "$scratch/bad-output" > "$scratch/bad-stdout" 2>/dev/null; then
  echo "expected control-character path rejection" >&2; exit 1
fi
[[ ! -s "$scratch/bad-output" && ! -s "$scratch/bad-stdout" ]]

# Source is passive: no output, directory change, state initialization, or shell-option change.
source_output="$(source "$ROOT_DIR/scripts/plan-wasm-determinism-scope.sh")"
[[ -z "$source_output" ]]
before_dir="$PWD"
before_options="$-"
source "$ROOT_DIR/scripts/plan-wasm-determinism-scope.sh"
[[ "$PWD" == "$before_dir" && "$-" == "$before_options" ]]

valid_state() {
  scope=partial; run_all=0; run_m1=1; run_m4=0; run_m5=0
  selected=(m1); selected_module_sets=m1; selected_module_sets_json='["m1"]'
  changed_paths=(); reason_summary=""; changed_paths_summary=""
  github_output_path="$scratch/invalid-output"
}
reject_state() {
  if emit_plan > "$scratch/invalid-stdout" 2>/dev/null; then
    echo "expected invalid state rejection" >&2; exit 1
  fi
  [[ ! -s "$scratch/invalid-output" && ! -s "$scratch/invalid-stdout" ]]
}
valid_state; validate_plan
valid_state; scope=unknown; reject_state
valid_state; run_m1=2; reject_state
valid_state; run_all=true; reject_state
valid_state; scope=all; run_all=1; reject_state
valid_state; scope=skip; reject_state
valid_state; run_m1=0; selected=(); selected_module_sets=""; selected_module_sets_json='[]'; reject_state
valid_state; selected_module_sets_json='not-json'; reject_state
valid_state; selected_module_sets=m4; reject_state
valid_state; selected=(m4); reject_state
valid_state; selected=(m1 m1); reject_state
valid_state; selected=(m9); reject_state
valid_state; run_m4=1; selected=(m4 m1); selected_module_sets=m4,m1; selected_module_sets_json='["m4","m1"]'; reject_state
valid_state; scope=all; run_all=1; run_m1=0; selected=(); selected_module_sets=""; selected_module_sets_json='[]'; reject_state
valid_state; scope=skip; run_m1=0; selected=(""); selected_module_sets=""; selected_module_sets_json='[]'; reject_state

echo "plan-wasm-determinism-scope.test: OK"
