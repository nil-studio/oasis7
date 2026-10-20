#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"
source "$repo_root/scripts/cargo-dev-lib.sh"

usage() {
  cat <<'USAGE'
Usage: ./scripts/oasis7-runtime-finality-baseline.sh [options]

Purpose:
  Run fixed baselines for world-runtime finality:
  - required: stake_root mismatch verification latency
  - required: epoch snapshot signer verification latency
  - full: 2-epoch finality signer rotation convergence

Outputs:
  <out-dir>/<timestamp>-<unique>/
    summary.md
    summary.json
    cases.tsv
    logs/<case-id>/{warmup-*.log,sample-*.log}
    <case-id>.durations_ms

Options:
  --out-dir <path>         Output root (default: .tmp/world_runtime_finality_baseline)
  --required-samples <n>   Required-case measured samples (default: 5)
  --full-samples <n>       Full-case measured samples (default: 3)
  --warmup-samples <n>     Warmup rounds per case (default: 1)
  --skip-full              Skip full-tier convergence baseline
  --dry-run                Print commands and write empty baseline summary
  -h, --help               Show help
USAGE
}

ensure_non_negative_int() {
  local flag=$1
  local value=$2
  if [[ ! "$value" =~ ^[0-9]+$ ]]; then
    echo "error: $flag must be a non-negative integer (got: $value)" >&2
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

now_ns() {
  python3 - <<'PY'
import time
print(time.perf_counter_ns())
PY
}

out_dir=".tmp/world_runtime_finality_baseline"
required_samples=5
full_samples=3
warmup_samples=1
skip_full=0
dry_run=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --out-dir)
      out_dir=${2:-}
      shift 2
      ;;
    --required-samples)
      required_samples=${2:-}
      shift 2
      ;;
    --full-samples)
      full_samples=${2:-}
      shift 2
      ;;
    --warmup-samples)
      warmup_samples=${2:-}
      shift 2
      ;;
    --skip-full)
      skip_full=1
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

ensure_non_negative_int "--required-samples" "$required_samples"
ensure_non_negative_int "--full-samples" "$full_samples"
ensure_non_negative_int "--warmup-samples" "$warmup_samples"

if [[ "$skip_full" -eq 1 ]]; then
  full_samples=0
fi

timestamp=$(date '+%Y%m%d-%H%M%S')
mkdir -p "$out_dir"
run_dir=$(mktemp -d "$out_dir/$timestamp-XXXXXX")
summary_md="$run_dir/summary.md"
summary_json="$run_dir/summary.json"
cases_tsv="$run_dir/cases.tsv"
: > "$cases_tsv"

overall_status="PASS"

run_case() {
  local case_id=$1
  local tier=$2
  local measured_samples=$3
  local convergence_target_epochs=$4
  local test_identity="tests::governance::$5"
  local test_features=$6
  local -a cmd=(oasis7_cargo_dev test -p oasis7_runtime --lib "$test_identity"
    --features "$test_features" -- --exact --format pretty --color never)

  local cmd_rendered=""
  cmd_rendered="$(format_cmd "${cmd[@]}")"
  local case_log_dir="$run_dir/logs/$case_id"
  local duration_path="$run_dir/${case_id}.durations_ms"
  local status="passed"
  local note="ok"
  local attempt=0
  local start_ns=0
  local end_ns=0
  local elapsed_ms=0
  local code=0

  mkdir -p "$case_log_dir"
  : > "$duration_path"

  if [[ "$measured_samples" -eq 0 ]]; then
    status="skipped"
    note="no_samples"
  elif [[ "$dry_run" -eq 1 ]]; then
    status="passed"
    note="dry_run"
  else
    for ((attempt=1; attempt<=warmup_samples; attempt+=1)); do
      warmup_log="$case_log_dir/warmup-${attempt}.log"
      {
        echo "case=$case_id"
        echo "tier=$tier"
        echo "phase=warmup"
        echo "attempt=$attempt"
        echo "command=$cmd_rendered"
      } > "$warmup_log"
      set +e
      {
        echo "+ $cmd_rendered"
        "${cmd[@]}"
      } >>"$warmup_log" 2>&1
      code=$?
      set -e
      if ! python3 "$repo_root/scripts/oasis7-runtime-finality-result.py" \
        "$warmup_log" "$test_identity" "$code"; then
        status="failed"
        note="warmup_${attempt}_invalid_execution_exit_${code}"
        break
      fi
    done

    if [[ "$status" == "passed" ]]; then
      for ((attempt=1; attempt<=measured_samples; attempt+=1)); do
        sample_log="$case_log_dir/sample-${attempt}.log"
        {
          echo "case=$case_id"
          echo "tier=$tier"
          echo "phase=sample"
          echo "attempt=$attempt"
          echo "command=$cmd_rendered"
        } > "$sample_log"
        start_ns=$(now_ns)
        set +e
        {
          echo "+ $cmd_rendered"
          "${cmd[@]}"
        } >>"$sample_log" 2>&1
        code=$?
        set -e
        end_ns=$(now_ns)
        elapsed_ms=$(( (end_ns - start_ns) / 1000000 ))
        echo "elapsed_ms=$elapsed_ms" >> "$sample_log"
        if ! python3 "$repo_root/scripts/oasis7-runtime-finality-result.py" \
          "$sample_log" "$test_identity" "$code"; then
          status="failed"
          note="sample_${attempt}_invalid_execution_exit_${code}"
          break
        fi
        echo "$elapsed_ms" >> "$duration_path"
      done
    fi
  fi

  if [[ "$status" == "failed" ]]; then
    overall_status="FAIL"
  fi

  printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
    "$case_id" \
    "$tier" \
    "$status" \
    "$note" \
    "$measured_samples" \
    "$warmup_samples" \
    "$convergence_target_epochs" \
    "$duration_path" \
    "$cmd_rendered" \
    >> "$cases_tsv"
}

run_case \
  stake_root_verify \
  required \
  "$required_samples" \
  0 \
  governance_apply_with_finality_rejects_stake_root_mismatch \
  test_tier_required

run_case \
  epoch_snapshot_verify \
  required \
  "$required_samples" \
  0 \
  governance_apply_with_finality_rejects_signer_outside_epoch_snapshot \
  test_tier_required

run_case \
  two_epoch_convergence \
  full \
  "$full_samples" \
  2 \
  governance_finality_epoch_snapshot_rotation_rejects_stale_signers_and_accepts_rotated_set \
  test_tier_full

python3 - "$cases_tsv" "$summary_json" "$summary_md" "$run_dir" "$overall_status" "$dry_run" <<'PY'
import json
import statistics
import shlex
import sys
from pathlib import Path

cases_tsv, summary_json, summary_md, run_dir, overall_status, dry_run = sys.argv[1:]

def parse_int(value: str, default: int = 0) -> int:
    try:
        return int(value)
    except ValueError:
        return default

def load_durations(path: Path):
    values = []
    if not path.exists():
        return values
    for raw in path.read_text(encoding="utf-8").splitlines():
        raw = raw.strip()
        if not raw:
            continue
        try:
            values.append(int(raw))
        except ValueError:
            continue
    return values

def compute_p95(values):
    if not values:
        return None
    ordered = sorted(values)
    rank = (95 * len(ordered) + 99) // 100
    rank = max(1, rank)
    return ordered[rank - 1]

rows = []
with open(cases_tsv, "r", encoding="utf-8") as fh:
    for raw in fh:
        parts = raw.rstrip("\n").split("\t")
        if len(parts) != 9:
            continue
        (
            case_id,
            tier,
            status,
            note,
            sample_count,
            warmup_count,
            convergence_target_epochs,
            duration_path,
            command,
        ) = parts
        durations = load_durations(Path(duration_path))
        arguments = shlex.split(command)
        executions = []
        for result_path in sorted((Path(run_dir) / "logs" / case_id).glob("*.result.json")):
            execution = json.loads(result_path.read_text(encoding="utf-8"))
            execution["result_path"] = str(result_path)
            executions.append(execution)
        row = {
            "test_package": "oasis7_runtime",
            "test_target": "lib",
            "test_features": arguments[arguments.index("--features") + 1],
            "test_identity": arguments[arguments.index("--lib") + 1],
            "executions": executions,
            "case_id": case_id,
            "tier": tier,
            "status": status,
            "note": note,
            "sample_count": parse_int(sample_count),
            "warmup_count": parse_int(warmup_count),
            "convergence_target_epochs": parse_int(convergence_target_epochs),
            "duration_path": duration_path,
            "command": command,
            "samples_ms": durations,
            "stats": {
                "count": len(durations),
                "min_ms": min(durations) if durations else None,
                "max_ms": max(durations) if durations else None,
                "avg_ms": round(statistics.mean(durations), 2) if durations else None,
                "p95_ms": compute_p95(durations),
            },
        }
        rows.append(row)

stake_epoch_samples = []
for row in rows:
    if row["case_id"] in ("stake_root_verify", "epoch_snapshot_verify"):
        stake_epoch_samples.extend(row["samples_ms"])

stake_epoch_stats = {
    "count": len(stake_epoch_samples),
    "min_ms": min(stake_epoch_samples) if stake_epoch_samples else None,
    "max_ms": max(stake_epoch_samples) if stake_epoch_samples else None,
    "avg_ms": round(statistics.mean(stake_epoch_samples), 2) if stake_epoch_samples else None,
    "p95_ms": compute_p95(stake_epoch_samples),
}

convergence_case = next((row for row in rows if row["case_id"] == "two_epoch_convergence"), None)
convergence_status = "skipped"
convergence_target_epochs = 2
if convergence_case is not None:
    convergence_status = convergence_case["status"]
    convergence_target_epochs = convergence_case.get("convergence_target_epochs", 2)

payload = {
    "run_dir": run_dir,
    "overall_status": overall_status,
    "dry_run": dry_run == "1",
    "cases": rows,
    "stake_epoch_verification_latency_ms": stake_epoch_stats,
    "two_epoch_convergence": {
        "target_epochs": convergence_target_epochs,
        "status": convergence_status,
    },
}

with open(summary_json, "w", encoding="utf-8") as fh:
    json.dump(payload, fh, ensure_ascii=True, indent=2)

lines = []
lines.append("# World Runtime Finality Baseline Summary")
lines.append("")
lines.append(f"- Run dir: `{run_dir}`")
lines.append(f"- Overall: {overall_status}")
lines.append(f"- Dry run: `{dry_run}`")
lines.append("")
lines.append("## Aggregated Metrics")
lines.append(
    f"- stake/epoch verification latency (required aggregate): "
    f"count={stake_epoch_stats['count']}, p95_ms={stake_epoch_stats['p95_ms']}, avg_ms={stake_epoch_stats['avg_ms']}"
)
lines.append(
    f"- 2-epoch convergence (full): target_epochs={convergence_target_epochs}, status={convergence_status}"
)
lines.append("")
lines.append("## Case Details")
lines.append("| Case | Tier | Status | Samples | p95_ms | avg_ms | min_ms | max_ms | Note |")
lines.append("| --- | --- | --- | --- | --- | --- | --- | --- | --- |")
for row in rows:
    stats = row["stats"]
    lines.append(
        "| {case_id} | {tier} | {status} | {count} | {p95} | {avg} | {min_v} | {max_v} | {note} |".format(
            case_id=row["case_id"],
            tier=row["tier"],
            status=row["status"],
            count=stats["count"],
            p95=stats["p95_ms"],
            avg=stats["avg_ms"],
            min_v=stats["min_ms"],
            max_v=stats["max_ms"],
            note=row["note"],
        )
    )
lines.append("")
lines.append("## Fixed Commands")
for row in rows:
    lines.append(f"- `{row['case_id']}`: `{row['command']}`")
with open(summary_md, "w", encoding="utf-8") as fh:
    fh.write("\n".join(lines) + "\n")
PY

echo "world-runtime finality baseline summary: $summary_md"
echo "world-runtime finality baseline summary json: $summary_json"

if [[ "$overall_status" != "PASS" ]]; then
  echo "error: world-runtime finality baseline failed" >&2
  exit 1
fi
