#!/usr/bin/env bash
usage() {
  cat <<'USAGE'
Usage:
  scripts/plan-wasm-determinism-scope.sh [options]

Options:
  --event-name <name>         GitHub event name (push, pull_request, workflow_dispatch)
  --base-ref <git-ref>        Base commit/ref used for diff
  --head-ref <git-ref>        Head commit/ref used for diff
  --changed-path <path>       Explicit changed path; may be passed multiple times
  --github-output <path>      Optional GitHub Actions output file
  -h, --help                  Show this help

Notes:
  - If one or more --changed-path values are provided, git diff is skipped.
  - workflow_dispatch always expands to all builtin module sets.
  - When the diff base cannot be resolved safely, the planner falls back to all module sets.
USAGE
}

append_reason() {
  local reason="$1"
  local existing
  for existing in "${reasons[@]-}"; do
    if [[ "$existing" == "$reason" ]]; then
      return 0
    fi
  done
  reasons+=("$reason")
}

mark_all() {
  run_all=1
  run_m1=1
  run_m4=1
  run_m5=1
  append_reason "$1"
}

mark_module() {
  local module_set="$1"
  local reason="$2"
  case "$module_set" in
    m1) run_m1=1 ;;
    m4) run_m4=1 ;;
    m5) run_m5=1 ;;
    *)
      echo "error: unsupported module set: $module_set" >&2
      exit 2
      ;;
  esac
  append_reason "$reason"
}

csv_join() {
  local IFS=,
  echo "$*"
}

resolve_changed_paths_from_git() {
  local diff_base="" diff_paths_file

  if [[ -z "$head_ref" ]]; then
    head_ref="HEAD"
  fi

  if [[ -z "$base_ref" ]]; then
    mark_all "missing_base_ref"
    return 0
  fi

  if [[ "$base_ref" =~ ^0+$ ]]; then
    mark_all "zero_before_sha"
    return 0
  fi

  if ! git rev-parse --verify --quiet "$head_ref^{commit}" >/dev/null; then
    mark_all "unresolvable_head_ref"
    return 0
  fi

  if ! git rev-parse --verify --quiet "$base_ref^{commit}" >/dev/null; then
    mark_all "unresolvable_base_ref"
    return 0
  fi

  case "$event_name" in
    pull_request)
      diff_base="$(git merge-base "$base_ref" "$head_ref")" || { mark_all "missing_diff_base"; return 0; }
      ;;
    *)
      diff_base="$base_ref"
      ;;
  esac

  if [[ -z "$diff_base" ]]; then
    mark_all "missing_diff_base"
    return 0
  fi

  diff_paths_file="$(mktemp)" || return 1
  if ! git diff --name-only -z "$diff_base" "$head_ref" -- > "$diff_paths_file"; then
    rm -f "$diff_paths_file"
    echo "error: unable to enumerate changed paths" >&2
    return 1
  fi
  while IFS= read -r -d '' path; do
    [[ -n "$path" ]] || continue
    changed_paths+=("$path")
  done < "$diff_paths_file"
  rm -f "$diff_paths_file"
}

classify_changed_path() {
  local path="$1"

  case "$path" in
    Cargo.toml|Cargo.lock|rust-toolchain.toml|\
    .github/workflows/wasm-determinism-gate.yml|\
    scripts/plan-wasm-determinism-scope.sh|\
    scripts/build-builtin-wasm-modules.sh|\
    scripts/build-wasm-module.sh|\
    scripts/ci-m1-wasm-summary.sh|\
    scripts/ci-verify-m1-wasm-summaries.py|\
    scripts/dispatch-wasm-determinism-gate.sh|\
    scripts/module-release-node-attestation-flow.sh|\
    scripts/module-release-node-acceptance.sh|\
    scripts/package-module-release-attestation-proof.sh|\
    scripts/package-wasm-summary-bundle.sh|\
    scripts/stage-wasm-summary-imports.sh|\
    scripts/submit-module-release-attestation.sh|\
    scripts/sync-m1-builtin-wasm-artifacts.sh|\
    scripts/wasm-release-evidence-report.sh|\
    scripts/wasm-summary-bundle-smoke.sh|\
    crates/oasis7_wasm_build|crates/oasis7_wasm_build/*|crates/oasis7_wasm_build/**/*|\
    crates/oasis7_runtime/src/world/artifacts/builtin_module_manifest_map.txt|\
    crates/oasis7_wasm_sdk|crates/oasis7_wasm_sdk/*|crates/oasis7_wasm_sdk/**/*|\
    crates/oasis7_wasm_abi|crates/oasis7_wasm_abi/*|crates/oasis7_wasm_abi/**/*|\
    crates/oasis7_distfs/src/bin/sync_builtin_wasm_identity.rs)
      mark_all "shared_wasm_pipeline:${path}"
      ;;
    docker/wasm-builder/*|tools/wasm_build_suite/*|tools/wasm_build_suite/**/*)
      mark_all "shared_wasm_pipeline:${path}"
      ;;
    scripts/sync-m4-builtin-wasm-artifacts.sh|\
    crates/oasis7_runtime/src/world/artifacts/m4_*|\
    crates/oasis7_builtin_wasm_modules/m4_*|\
    crates/oasis7_builtin_wasm_modules/m4_*/**|\
    crates/oasis7_builtin_wasm_modules/_templates/m4_*)
      mark_module "m4" "module_set:m4:${path}"
      ;;
    scripts/sync-m5-builtin-wasm-artifacts.sh|\
    crates/oasis7_runtime/src/world/artifacts/m5_*|\
    crates/oasis7_builtin_wasm_modules/m5_*|\
    crates/oasis7_builtin_wasm_modules/m5_*/**)
      mark_module "m5" "module_set:m5:${path}"
      ;;
    crates/oasis7_runtime/src/world/artifacts/m1_*|\
    crates/oasis7_builtin_wasm_modules/m1_*|\
    crates/oasis7_builtin_wasm_modules/m1_*/**)
      mark_module "m1" "module_set:m1:${path}"
      ;;
  esac
}

module_sets_json() {
  local result="[" separator="" module
  for module in "$@"; do
    [[ -n "$module" ]] || continue
    result+="${separator}\"${module}\""
    separator=,
  done
  printf '%s]\n' "$result"
}

validate_plan() {
  local flag expected_csv expected_json
  local -a expected=()
  for flag in "$run_all" "$run_m1" "$run_m4" "$run_m5"; do
    [[ "$flag" == 0 || "$flag" == 1 ]] || { echo "error: invalid plan flag" >&2; return 1; }
  done
  [[ "$run_m1" == 1 ]] && expected+=(m1)
  [[ "$run_m4" == 1 ]] && expected+=(m4)
  [[ "$run_m5" == 1 ]] && expected+=(m5)
  expected_csv="$(csv_join "${expected[@]-}")"
  expected_json="$(module_sets_json "${expected[@]-}")"
  [[ ${#selected[@]} == ${#expected[@]} &&
     "$(csv_join "${selected[@]-}")" == "$expected_csv" &&
     "$selected_module_sets" == "$expected_csv" &&
     "$selected_module_sets_json" == "$expected_json" ]] || {
    echo "error: inconsistent selected module sets" >&2; return 1;
  }
  case "$scope" in
    skip) [[ "$run_all" == 0 && ${#expected[@]} == 0 ]] ;;
    all) [[ "$run_all" == 1 && ${#expected[@]} == 3 ]] ;;
    partial) [[ "$run_all" == 0 && ${#expected[@]} -gt 0 ]] ;;
    *) echo "error: invalid plan scope" >&2; return 1 ;;
  esac || { echo "error: inconsistent plan scope" >&2; return 1; }
}

emit_plan() {
  validate_plan || return 1
  local output
  output="$(
    printf 'scope=%s\n' "$scope"
    printf 'run_all=%s\n' "$([[ "$run_all" == 1 ]] && echo true || echo false)"
    printf 'run_m1=%s\n' "$([[ "$run_m1" == 1 ]] && echo true || echo false)"
    printf 'run_m4=%s\n' "$([[ "$run_m4" == 1 ]] && echo true || echo false)"
    printf 'run_m5=%s\n' "$([[ "$run_m5" == 1 ]] && echo true || echo false)"
    printf 'selected_module_sets=%s\n' "$selected_module_sets"
    printf 'selected_module_sets_json=%s\n' "$selected_module_sets_json"
    printf 'reason_summary=%s\n' "$reason_summary"
    printf 'changed_path_count=%s\n' "${#changed_paths[@]}"
    printf 'changed_paths=%s\n' "$changed_paths_summary"
  )"
  if [[ -n "$github_output_path" ]]; then
    printf '%s\n' "$output" >> "$github_output_path" || return 1
  fi
  printf '%s\n' "$output"
}

main() {
  set -euo pipefail
  local repo_root
  repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
  cd "$repo_root"
  local event_name="" base_ref="" head_ref="" github_output_path=""
  local -a changed_paths=() reasons=() selected=()
  local run_all=0 run_m1=0 run_m4=0 run_m5=0
  local scope selected_module_sets selected_module_sets_json reason_summary changed_paths_summary path
while [[ $# -gt 0 ]]; do
  case "$1" in
    --event-name)
      [[ $# -ge 2 ]] || { echo "error: --event-name requires a value" >&2; exit 2; }
      event_name="$2"
      shift 2
      ;;
    --base-ref)
      [[ $# -ge 2 ]] || { echo "error: --base-ref requires a value" >&2; exit 2; }
      base_ref="$2"
      shift 2
      ;;
    --head-ref)
      [[ $# -ge 2 ]] || { echo "error: --head-ref requires a value" >&2; exit 2; }
      head_ref="$2"
      shift 2
      ;;
    --changed-path)
      [[ $# -ge 2 ]] || { echo "error: --changed-path requires a value" >&2; exit 2; }
      changed_paths+=("$2")
      shift 2
      ;;
    --github-output)
      [[ $# -ge 2 ]] || { echo "error: --github-output requires a value" >&2; exit 2; }
      github_output_path="$2"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "error: unknown option: $1" >&2
      usage
      exit 2
      ;;
  esac
done

if [[ "$event_name" == "workflow_dispatch" ]]; then
  mark_all "workflow_dispatch"
elif [[ "${#changed_paths[@]}" -eq 0 ]]; then
  resolve_changed_paths_from_git
fi

for path in "${changed_paths[@]-}"; do
    if [[ "$path" =~ [[:cntrl:]] ]]; then
      printf 'error: changed path contains control characters\n' >&2
      exit 1
    fi
    if [[ "$run_all" -eq 0 ]]; then classify_changed_path "$path"; fi
done

scope="skip"
[[ "$run_all" == 1 ]] && scope="all"
if [[ "$run_m1" == 1 || "$run_m4" == 1 || "$run_m5" == 1 ]]; then
  [[ "$scope" == all ]] || scope="partial"
  [[ "$run_m1" == 1 ]] && selected+=(m1)
  [[ "$run_m4" == 1 ]] && selected+=(m4)
  [[ "$run_m5" == 1 ]] && selected+=(m5)
else
  append_reason "no_builtin_wasm_inputs_changed"
fi
selected_module_sets="$(csv_join "${selected[@]-}")"
selected_module_sets_json="$(module_sets_json "${selected[@]-}")"
reason_summary="$(printf '%s\n' "${reasons[@]-}" | paste -sd ';' -)"
changed_paths_summary="$(printf '%s\n' "${changed_paths[@]-}" | paste -sd ';' -)"
emit_plan
}

if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
  main "$@"
fi
