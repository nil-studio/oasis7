#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

usage() {
  cat <<'USAGE'
Usage: ./scripts/wasm-release-evidence-report.sh [options]

Purpose:
  Collect and verify Docker canonical WASM release evidence for builtin module sets.
  The script can:
  - collect current-runner summaries for m1/m4/m5
  - verify per-module-set multi-runner summary directories
  - write a human/machine readable evidence report

Outputs:
  <out-dir>/<timestamp>/
    summary.md
    summary.json
    module_sets.tsv
    summaries/<module-set>/<runner>.json
    logs/<module-set>.verify.log

Options:
  --out-dir <path>           Output root (default: .tmp/wasm_release_evidence_report)
  --module-sets <csv>        Module sets to process (default: m1,m4,m5)
  --runner-label <label>     Runner label used for collection (default: detected host platform)
  --required-runners <csv>   Runner labels required for the stable gate
                             (default: current runner only)
  --expected-runners <csv>   Runner labels expected for full cross-host evidence
                             (default: same as required runners)
  --expected-canonical-platform <platform>
                             Canonical container platform expected in every summary
                             (default: linux-x86_64)
  --summary-import-dir <path>
                             Import pre-collected summary jsons before verify.
                             Accepts either <path>/<module-set>/*.json or, when only one
                             module set is requested, a flat <path>/*.json directory.
  --skip-collect             Verify/report only; do not collect current-runner summaries
  --dry-run                  Print actions and write placeholder report without execution
  -h, --help                 Show help
USAGE
}

normalize_platform_os() {
  local raw="$1"
  case "$raw" in
    Darwin) echo "darwin" ;;
    Linux) echo "linux" ;;
    *) echo "$raw" | tr '[:upper:]' '[:lower:]' ;;
  esac
}

normalize_platform_arch() {
  local raw="$1"
  case "$raw" in
    arm64|aarch64) echo "arm64" ;;
    x86_64|amd64) echo "x86_64" ;;
    *) echo "$raw" ;;
  esac
}

detect_host_platform() {
  local os arch
  os="$(normalize_platform_os "$(uname -s)")"
  arch="$(normalize_platform_arch "$(uname -m)")"
  echo "${os}-${arch}"
}

ensure_csv_non_empty() {
  local flag="$1"
  local csv="$2"
  if [[ -z "$csv" ]]; then
    echo "error: $flag must not be empty" >&2
    exit 2
  fi
}

format_cmd() {
  local formatted=""
  local token=""
  for token in "$@"; do
    local quoted=""
    printf -v quoted '%q' "$token"
    if [[ -z "$formatted" ]]; then
      formatted="$quoted"
    else
      formatted="$formatted $quoted"
    fi
  done
  printf '%s' "$formatted"
}

copy_summary_imports() {
  local module_set="$1"
  local import_root="$2"
  local target_dir="$3"
  local module_set_count="$4"

  [[ -n "$import_root" ]] || return 0
  [[ -d "$import_root" ]] || {
    echo "error: summary import dir not found: $import_root" >&2
    exit 2
  }

  local source_dir="$import_root/$module_set"
  if [[ ! -d "$source_dir" ]]; then
    if [[ "$module_set_count" -eq 1 ]]; then
      source_dir="$import_root"
    else
      return 0
    fi
  fi

  local found=0
  local path=""
  shopt -s nullglob
  for path in "$source_dir"/*.json; do
    found=1
    cp "$path" "$target_dir/$(basename "$path")"
  done
  shopt -u nullglob

  if [[ "$found" -eq 0 && "$module_set_count" -eq 1 && "$source_dir" == "$import_root" ]]; then
    echo "error: summary import dir has no .json files: $import_root" >&2
    exit 2
  fi
}

out_dir=".tmp/wasm_release_evidence_report"
module_sets_csv="m1,m4,m5"
runner_label=""
required_runners_csv=""
expected_runners_csv=""
expected_canonical_platform="linux-x86_64"
summary_import_dir=""
skip_collect=0
dry_run=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --out-dir)
      out_dir=${2:-}
      shift 2
      ;;
    --module-sets)
      module_sets_csv=${2:-}
      shift 2
      ;;
    --runner-label)
      runner_label=${2:-}
      shift 2
      ;;
    --required-runners)
      required_runners_csv=${2:-}
      shift 2
      ;;
    --expected-runners)
      expected_runners_csv=${2:-}
      shift 2
      ;;
    --expected-canonical-platform)
      expected_canonical_platform=${2:-}
      shift 2
      ;;
    --summary-import-dir)
      summary_import_dir=${2:-}
      shift 2
      ;;
    --skip-collect)
      skip_collect=1
      shift
      ;;
    --dry-run)
      dry_run=1
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "error: unknown option: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

if [[ -z "$runner_label" ]]; then
  runner_label="$(detect_host_platform)"
fi
if [[ -z "$required_runners_csv" ]]; then
  required_runners_csv="$runner_label"
fi
if [[ -z "$expected_runners_csv" ]]; then
  expected_runners_csv="$required_runners_csv"
fi
ensure_csv_non_empty "--module-sets" "$module_sets_csv"
ensure_csv_non_empty "--required-runners" "$required_runners_csv"
ensure_csv_non_empty "--expected-runners" "$expected_runners_csv"
if [[ -z "$expected_canonical_platform" ]]; then
  echo "error: --expected-canonical-platform must not be empty" >&2
  exit 2
fi

timestamp=$(date '+%Y%m%d-%H%M%S')
run_dir="$out_dir/$timestamp"
summary_md="$run_dir/summary.md"
summary_json="$run_dir/summary.json"
module_sets_tsv="$run_dir/module_sets.tsv"
logs_dir="$run_dir/logs"
summaries_dir="$run_dir/summaries"
mkdir -p "$logs_dir" "$summaries_dir"
: > "$module_sets_tsv"

overall_status="PASS"

IFS=',' read -r -a module_sets <<< "$module_sets_csv"
module_set_count=0
for module_set in "${module_sets[@]}"; do
  module_set="$(echo "$module_set" | xargs)"
  [[ -n "$module_set" ]] || continue
  module_set_count=$((module_set_count + 1))
done

for module_set in "${module_sets[@]}"; do
  module_set="$(echo "$module_set" | xargs)"
  [[ -n "$module_set" ]] || continue

  module_summary_dir="$summaries_dir/$module_set"
  module_summary_path="$module_summary_dir/$runner_label.json"
  verify_log="$logs_dir/${module_set}.verify.log"
  mkdir -p "$module_summary_dir"

  collect_cmd=(
    ./scripts/ci-m1-wasm-summary.sh
    --module-set "$module_set"
    --runner-label "$runner_label"
    --out "$module_summary_path"
  )
  verify_cmd=(
    python3 ./scripts/ci-verify-m1-wasm-summaries.py
    --module-set "$module_set"
    --summary-dir "$module_summary_dir"
    --required-runners "$required_runners_csv"
    --expected-runners "$expected_runners_csv"
    --expected-canonical-platform "$expected_canonical_platform"
  )

  collect_status="skipped"
  verify_status="skipped"
  module_note="ok"

  {
    echo "module_set=$module_set"
    echo "runner_label=$runner_label"
    echo "required_runners=$required_runners_csv"
    echo "expected_runners=$expected_runners_csv"
    echo "summary_import_dir=$summary_import_dir"
    echo "collect_cmd=$(format_cmd "${collect_cmd[@]}")"
    echo "verify_cmd=$(format_cmd "${verify_cmd[@]}")"
  } > "$verify_log"

  if [[ "$dry_run" -eq 1 ]]; then
    if [[ -n "$summary_import_dir" ]]; then
      echo "+ import summaries from $summary_import_dir (dry-run)"
    fi
    if [[ "$skip_collect" -eq 0 ]]; then
      echo "+ $(format_cmd "${collect_cmd[@]}") (dry-run)"
      collect_status="dry_run"
    fi
    echo "+ $(format_cmd "${verify_cmd[@]}") (dry-run)"
    verify_status="dry_run"
    module_note="dry_run"
  else
    copy_summary_imports "$module_set" "$summary_import_dir" "$module_summary_dir" "$module_set_count"

    if [[ "$skip_collect" -eq 0 ]]; then
      collect_status="passed"
      set +e
      {
        echo "+ $(format_cmd "${collect_cmd[@]}")"
        "${collect_cmd[@]}"
      } >> "$verify_log" 2>&1
      code=$?
      set -e
      if [[ "$code" -ne 0 ]]; then
        collect_status="failed"
        verify_status="skipped"
        module_note="collect_exit_${code}"
        overall_status="FAIL"
      fi
    fi

    if [[ "$collect_status" != "failed" ]]; then
      verify_status="passed"
      set +e
      {
        echo "+ $(format_cmd "${verify_cmd[@]}")"
        "${verify_cmd[@]}"
      } >> "$verify_log" 2>&1
      code=$?
      set -e
      if [[ "$code" -ne 0 ]]; then
        verify_status="failed"
        module_note="verify_exit_${code}"
        overall_status="FAIL"
      fi
    fi
  fi

  printf '%s\t%s\t%s\t%s\t%s\t%s\n' \
    "$module_set" \
    "$collect_status" \
    "$verify_status" \
    "$module_note" \
    "$module_summary_dir" \
    "$verify_log" \
    >> "$module_sets_tsv"
done

python3 - "$module_sets_tsv" "$summary_json" "$run_dir" "$runner_label" "$required_runners_csv" "$expected_runners_csv" "$overall_status" "$skip_collect" "$dry_run" "$summary_import_dir" <<'PY'
import json
import pathlib
import sys

module_sets_tsv, summary_json, run_dir, runner_label, required_runners_csv, expected_runners_csv, overall_status, skip_collect, dry_run, summary_import_dir = sys.argv[1:]


def parse_csv(raw: str) -> list[str]:
    return [item for item in (value.strip() for value in raw.split(",")) if item]


required_runners = parse_csv(required_runners_csv)
expected_runners = parse_csv(expected_runners_csv)
required_runner_set = set(required_runners)
expected_runner_set = set(expected_runners)

module_sets = []
baseline_identity = None
received_runner_union = set()
missing_required_union = set()
missing_expected_union = set()
extra_runner_union = set()
with open(module_sets_tsv, "r", encoding="utf-8") as fh:
    for raw in fh:
        module_set, collect_status, verify_status, note, summary_dir, verify_log = raw.rstrip("\n").split("\t")
        summary_paths = sorted(pathlib.Path(summary_dir).glob("*.json"))
        for summary_path in summary_paths:
            summary = json.loads(summary_path.read_text())
            if (
                type(summary.get("schema_version")) is int
                and summary["schema_version"] == 1
                and "purpose" not in summary
                and "baseline_sha256" not in summary
            ):
                raise SystemExit("error: legacy-envelope/current-freshness-unverified: original evidence retained; canonical rebuild required for current gates or new proof assembly")
            purpose, digest = summary.get("purpose"), summary.get("baseline_sha256")
            valid_digest = isinstance(digest, str) and len(digest) == 64 and all(c in "0123456789abcdef" for c in digest)
            if purpose not in ("candidate-build", "release-build") or (purpose == "release-build" and digest != "") or (purpose == "candidate-build" and not valid_digest):
                raise SystemExit("error: report rejects unknown/malformed evidence purpose")
            identity = (purpose, digest)
            if baseline_identity is not None and baseline_identity != identity:
                raise SystemExit("error: report rejects mixed candidate/release baseline evidence")
            baseline_identity = identity
        found_runners = []
        module_count = None
        if summary_paths:
            try:
                payload = json.loads(summary_paths[0].read_text())
                module_count = payload.get("module_count")
            except Exception:
                module_count = None
            found_runners = [path.stem for path in summary_paths]
        found_runner_set = set(found_runners)
        missing_required_runners = sorted(required_runner_set - found_runner_set)
        missing_runners = sorted(expected_runner_set - found_runner_set)
        extra_runners = sorted(found_runner_set - expected_runner_set)
        stable_gate_passed = verify_status == "passed" and not missing_required_runners
        cross_host_evidence_pending = stable_gate_passed and bool(missing_runners)
        cross_host_closed = stable_gate_passed and not missing_runners
        if stable_gate_passed:
            gate_result = "conditional-go" if cross_host_evidence_pending else "cross-host-closed"
        else:
            gate_result = "no-go"
        received_runner_union |= found_runner_set
        missing_required_union |= set(missing_required_runners)
        missing_expected_union |= set(missing_runners)
        extra_runner_union |= set(extra_runners)
        module_sets.append(
            {
                "module_set": module_set,
                "collect_status": collect_status,
                "verify_status": verify_status,
                "note": note,
                "summary_dir": summary_dir,
                "verify_log": verify_log,
                "required_runners": required_runners,
                "expected_runners": expected_runners,
                "received_runners": found_runners,
                "missing_required_runners": missing_required_runners,
                "missing_runners": missing_runners,
                "extra_runners": extra_runners,
                "stable_gate_passed": stable_gate_passed,
                "cross_host_evidence_pending": cross_host_evidence_pending,
                "cross_host_closed": cross_host_closed,
                "canonical_hash_consistent": verify_status == "passed",
                "receipt_evidence_consistent": verify_status == "passed",
                "gate_result": gate_result,
                "module_count": module_count,
            }
        )

stable_gate_passed = overall_status == "PASS" and not missing_required_union
cross_host_evidence_pending = stable_gate_passed and bool(missing_expected_union)
cross_host_closed = stable_gate_passed and not missing_expected_union
if stable_gate_passed:
    gate_result = "conditional-go" if cross_host_evidence_pending else "cross-host-closed"
else:
    gate_result = "no-go"

payload = {
    "purpose": baseline_identity[0] if baseline_identity else "release-build",
    "baseline_sha256": baseline_identity[1] if baseline_identity else "",
    "run_dir": run_dir,
    "runner_label": runner_label,
    "required_runners": required_runners,
    "expected_runners": expected_runners,
    "received_runners": sorted(received_runner_union),
    "missing_required_runners": sorted(missing_required_union),
    "missing_runners": sorted(missing_expected_union),
    "extra_runners": sorted(extra_runner_union),
    "stable_gate_passed": stable_gate_passed,
    "cross_host_evidence_pending": cross_host_evidence_pending,
    "cross_host_closed": cross_host_closed,
    "gate_result": gate_result,
    "overall_status": overall_status,
    "skip_collect": skip_collect == "1",
    "dry_run": dry_run == "1",
    "summary_import_dir": summary_import_dir or None,
    "module_sets": module_sets,
}
with open(summary_json, "w", encoding="utf-8") as fh:
    json.dump(payload, fh, ensure_ascii=True, indent=2)
PY

{
  echo "# WASM Release Evidence Report"
  echo ""
  echo "- Timestamp: $(date '+%Y-%m-%d %H:%M:%S %Z')"
  echo "- Run dir: \`$run_dir\`"
  echo "- Runner label: \`$runner_label\`"
  echo "- Required runners: \`$required_runners_csv\`"
  echo "- Expected runners: \`$expected_runners_csv\`"
  echo "- Summary import dir: \`${summary_import_dir:-none}\`"
  echo "- Skip collect: \`$skip_collect\`"
  echo "- Dry run: \`$dry_run\`"
  echo "- Overall: $overall_status"
  echo "- Stable gate passed: \`$(jq -r '.stable_gate_passed' "$summary_json")\`"
  echo "- Cross-host evidence pending: \`$(jq -r '.cross_host_evidence_pending' "$summary_json")\`"
  echo "- Gate result: \`$(jq -r '.gate_result' "$summary_json")\`"
  echo "- Received runners: \`$(jq -r '.received_runners | join(",")' "$summary_json")\`"
  echo "- Missing runners: \`$(jq -r '.missing_runners | join(",")' "$summary_json")\`"
  echo ""
  echo "## Module Sets"
  while IFS=$'\t' read -r module_set collect_status verify_status note summary_dir verify_log; do
    [[ -n "$module_set" ]] || continue
    module_json="$(jq -c --arg module_set "$module_set" '.module_sets[] | select(.module_set == $module_set)' "$summary_json")"
    echo "- $module_set: collect=$collect_status verify=$verify_status gate=$(jq -r '.gate_result' <<< "$module_json") note=$note"
    echo "  required_runners: \`$(jq -r '.required_runners | join(",")' <<< "$module_json")\`"
    echo "  expected_runners: \`$(jq -r '.expected_runners | join(",")' <<< "$module_json")\`"
    echo "  received_runners: \`$(jq -r '.received_runners | join(",")' <<< "$module_json")\`"
    echo "  missing_runners: \`$(jq -r '.missing_runners | join(",")' <<< "$module_json")\`"
    echo "  stable_gate_passed: \`$(jq -r '.stable_gate_passed' <<< "$module_json")\`"
    echo "  cross_host_evidence_pending: \`$(jq -r '.cross_host_evidence_pending' <<< "$module_json")\`"
    echo "  summary_dir: \`$summary_dir\`"
    echo "  verify_log: \`$verify_log\`"
  done < "$module_sets_tsv"
} > "$summary_md"

echo "wasm release evidence summary: $summary_md"
echo "wasm release evidence summary json: $summary_json"
