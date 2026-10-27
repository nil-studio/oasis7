#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

if (( BASH_VERSINFO[0] < 4 )); then
  echo "error: scripts/module-release-node-acceptance.sh requires Bash 4+ (uses associative arrays); current Bash is ${BASH_VERSION:-unknown}" >&2
  echo "hint: install/run a newer bash, then invoke this script with that interpreter before module release acceptance execution." >&2
  exit 2
fi

usage() {
  cat <<'USAGE'
Usage: ./scripts/module-release-node-acceptance.sh [options]

Purpose:
  Run decentralized node-side module release acceptance checks:
  - required node-side attestation flow smoke
  - required attestation submit API regression
  - required attestation submission regression
  - required attestation threshold rejection regression
  - required receipt evidence mismatch rejection regression
  - required production release policy binding/status regression
  - optional full-tier manifest fault signature regression
  - triage signature grep summary

Options:
  --out-dir <path>       Output root (default: .tmp/module_release_node_acceptance)
  --include-full         Include full-tier manifest fault signature regression
  --dry-run              Print commands and write summary without execution
  -h, --help             Show help
USAGE
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

declare -A step_status=()
declare -A step_note=()
declare -A step_log=()
declare -A step_cmd=()

all_steps=(required_attestation_flow required_submit_api required_attestation required_threshold required_receipt_evidence required_release_policy full_manifest_faults triage_signals)
selected_steps=(required_attestation_flow required_submit_api required_attestation required_threshold required_receipt_evidence required_release_policy triage_signals)

out_dir=".tmp/module_release_node_acceptance"
include_full=0
dry_run=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --out-dir)
      out_dir=${2:-}
      shift 2
      ;;
    --include-full)
      include_full=1
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

if [[ "$include_full" -eq 1 ]]; then
  selected_steps=(required_attestation_flow required_submit_api required_attestation required_threshold required_receipt_evidence required_release_policy full_manifest_faults triage_signals)
fi

timestamp=$(date '+%Y%m%d-%H%M%S')
run_dir="$out_dir/$timestamp"
summary_md="$run_dir/summary.md"
summary_json="$run_dir/summary.json"
steps_tsv="$run_dir/steps.tsv"
mkdir -p "$run_dir"
: > "$steps_tsv"

step=""
for step in "${all_steps[@]}"; do
  step_status["$step"]="skipped"
  step_note["$step"]="not scheduled"
  step_log["$step"]="$run_dir/$step.log"
  step_cmd["$step"]=""
done

run_step() {
  local step_name=$1
  shift
  local -a cmd=("$@")
  local step_log_path="${step_log[$step_name]}"
  local cmd_rendered=""
  local code=0

  cmd_rendered="$(format_cmd "${cmd[@]}")"
  step_cmd["$step_name"]="$cmd_rendered"

  {
    echo "step=$step_name"
    echo "started_at=$(date '+%Y-%m-%d %H:%M:%S %Z')"
    echo "command=$cmd_rendered"
  } >"$step_log_path"

  if [[ "$dry_run" -eq 1 ]]; then
    echo "+ $cmd_rendered (dry-run)"
    step_status["$step_name"]="passed"
    step_note["$step_name"]="dry_run"
    echo "result=dry_run_pass" >>"$step_log_path"
    return 0
  fi

  set +e
  {
    echo "+ $cmd_rendered"
    "${cmd[@]}"
  } > >(tee -a "$step_log_path") 2>&1
  code=$?
  set -e

  if [[ "$code" -eq 0 ]]; then
    step_status["$step_name"]="passed"
    step_note["$step_name"]="ok"
    echo "result=ok" >>"$step_log_path"
    return 0
  fi

  step_status["$step_name"]="failed"
  step_note["$step_name"]="exit_code=$code"
  echo "result=failed" >>"$step_log_path"
  echo "exit_code=$code" >>"$step_log_path"
  return 1
}

for step in "${selected_steps[@]}"; do
  case "$step" in
    required_attestation_flow)
      cmd=(
        bash -lc
        "set -euo pipefail
tmp_root=\$(mktemp -d \"\${TMPDIR:-/tmp}/module-release-flow-smoke.XXXXXX\")
trap 'rm -rf \"\$tmp_root\"' EXIT
mkdir -p \"\$tmp_root/import/m1\"
python3 - \"\$tmp_root/import/m1/linux-x86_64.json\" \"\$tmp_root/import/m1/darwin-arm64.json\" <<'PY'
import json
import pathlib
import sys

linux_path = pathlib.Path(sys.argv[1])
darwin_path = pathlib.Path(sys.argv[2])

wasm_hash = 'a' * 64
source_hash = 'b' * 64
build_manifest_hash = 'c' * 64
builder_image_digest = 'sha256:' + ('d' * 64)
identity_hash = 'sha256:' + ('e' * 64)
canonicalizer_version = 'strip-custom-sections-v1'

base = {
    'purpose': 'release-build',
    'baseline_sha256': '',
    'schema_version': 1,
    'module_set': 'm1',
    'current_platform': 'linux-x86_64',
    'canonical_platform': 'linux-x86_64',
    'module_count': 1,
    'module_hashes': {'m1_demo': wasm_hash},
    'manifest_platform_hashes': {'m1_demo': wasm_hash},
    'identity_hashes': {'m1_demo': identity_hash},
    'identity_build_recipe': {
        'builder_image_digest': builder_image_digest,
        'container_platform': 'linux-x86_64',
        'canonicalizer_version': canonicalizer_version,
    },
    'receipt_evidence': {
        'm1_demo': {
            'source_input_rule_version': 2,
            'source_inputs': [{'kind': 'source-file', 'path': 'module/src/lib.rs', 'sha256': '0' * 64}],
            'source_hash': source_hash,
            'build_manifest_hash': build_manifest_hash,
            'wasm_hash': wasm_hash,
            'builder_image_digest': builder_image_digest,
            'container_platform': 'linux-x86_64',
            'canonicalizer_version': canonicalizer_version,
        }
    },
}

for path, runner, host_platform in (
    (linux_path, 'linux-x86_64', 'linux-x86_64'),
    (darwin_path, 'darwin-arm64', 'darwin-arm64'),
):
    payload = dict(base)
    payload['runner'] = runner
    payload['host_platform'] = host_platform
    path.write_text(json.dumps(payload, ensure_ascii=True, indent=2, sort_keys=True) + '\\n')
PY
./scripts/module-release-node-attestation-flow.sh \
  --out-dir \"\$tmp_root/out\" \
  --module-sets m1 \
  --summary-import-dir \"\$tmp_root/import\" \
  --skip-local-collect \
  --required-runners linux-x86_64 \
  --expected-runners linux-x86_64,darwin-arm64 \
  --require-cross-host-closed \
  --request-id 17 \
  --operator-agent-id operator-1 \
  --signer-node-id attestor-node-1 \
  --platform darwin-arm64 \
  --build-manifest-hash cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc \
  --source-hash bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb \
  --wasm-hash aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa \
  --builder-image-digest sha256:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd \
  --container-platform linux-x86_64 \
  --canonicalizer-version strip-custom-sections-v1
run_dir=\$(find \"\$tmp_root/out\" -mindepth 1 -maxdepth 1 -type d | sort | tail -n 1)
test -f \"\$run_dir/proof/proof_payload.json\"
test -f \"\$run_dir/proof/submit_request.json\"
jq -e '.gate_result == \"cross-host-closed\"' \"\$run_dir/proof_inputs/release_evidence_summary.json\" >/dev/null
jq -e '.proof_cid | startswith(\"sha256:\")' \"\$run_dir/proof/proof_payload.json\" >/dev/null"
      )
      ;;
    required_submit_api)
      cmd=(
        env -u RUSTC_WRAPPER cargo test -p oasis7_chain_runtime
        module_release_attestation_submit
        --bin oasis7_chain_runtime
        -- --nocapture
      )
      ;;
    required_attestation)
      cmd=(
        env -u RUSTC_WRAPPER cargo test -p oasis7_chain_runtime
        module_release_submit_attestation_
        --features test_tier_required
        -- --nocapture
      )
      ;;
    required_threshold)
      cmd=(
        env -u RUSTC_WRAPPER cargo test -p oasis7_chain_runtime
        module_release_apply_rejects_when_attestation_threshold_not_met
        --features test_tier_required
        -- --nocapture
      )
      ;;
    required_receipt_evidence)
      cmd=(
        env -u RUSTC_WRAPPER cargo test -p oasis7_chain_runtime
        module_release_apply_rejects_when_attestation_receipt_evidence_mismatches
        --features test_tier_required
        -- --nocapture
      )
      ;;
    required_release_policy)
      cmd=(
        env -u RUSTC_WRAPPER cargo test -p oasis7_chain_runtime
        production_release_policy_
        -- --nocapture
      )
      ;;
    full_manifest_faults)
      cmd=(
        env -u RUSTC_WRAPPER cargo test -p oasis7_chain_runtime
        power_bootstrap_release_manifest_full
        --features test_tier_full
        -- --nocapture
      )
      ;;
    triage_signals)
      cmd=(
        bash -lc
        "set +e; rc=0; if [[ -d output ]]; then rg -n \"conflicting attestation already exists|attestation threshold not met|attestation receipt evidence mismatch|fault_signature=builtin_release_manifest_\" output; r=\$?; if [[ \$r -gt 1 ]]; then rc=\$r; fi; fi; if [[ -d .tmp ]]; then rg -n \"conflicting attestation already exists|attestation threshold not met|attestation receipt evidence mismatch|fault_signature=builtin_release_manifest_\" .tmp --glob '!.tmp/module_release_node_acceptance/**' --glob '!module_release_node_acceptance/**'; r=\$?; if [[ \$r -gt 1 ]]; then rc=\$r; fi; fi; exit \$rc"
      )
      ;;
    *)
      echo "error: internal unknown step: $step" >&2
      exit 2
      ;;
  esac

  if ! run_step "$step" "${cmd[@]}"; then
    break
  fi
done

overall="PASS"
for step in "${selected_steps[@]}"; do
  if [[ "${step_status[$step]}" == "failed" ]]; then
    overall="FAIL"
    break
  fi
done

for step in "${all_steps[@]}"; do
  printf '%s\t%s\t%s\t%s\t%s\n' \
    "$step" \
    "${step_status[$step]}" \
    "${step_note[$step]}" \
    "${step_log[$step]}" \
    "${step_cmd[$step]}" \
    >> "$steps_tsv"
done

{
  echo "# Module Release Node Acceptance Summary"
  echo ""
  echo "- Timestamp: $(date '+%Y-%m-%d %H:%M:%S %Z')"
  echo "- Run dir: \`$run_dir\`"
  echo "- Dry run: \`$dry_run\`"
  echo "- Include full: \`$include_full\`"
  echo "- Overall: $overall"
  echo ""
  echo "## Step Status"
  for step in "${all_steps[@]}"; do
    echo "- $step: ${step_status[$step]} (${step_note[$step]})"
    if [[ -n "${step_cmd[$step]}" ]]; then
      echo "  - command: \`${step_cmd[$step]}\`"
    fi
    echo "  - log: \`${step_log[$step]}\`"
  done
} > "$summary_md"

python3 - "$steps_tsv" "$summary_json" "$run_dir" "$overall" "$dry_run" "$include_full" <<'PY'
import json
import sys

steps_tsv, summary_json, run_dir, overall, dry_run, include_full = sys.argv[1:]
steps = []
with open(steps_tsv, "r", encoding="utf-8") as fh:
    for raw in fh:
        parts = raw.rstrip("\n").split("\t")
        if len(parts) != 5:
            continue
        name, status, note, log_path, command = parts
        steps.append(
            {
                "name": name,
                "status": status,
                "note": note,
                "log": log_path,
                "command": command,
            }
        )

payload = {
    "run_dir": run_dir,
    "overall": overall,
    "dry_run": dry_run == "1",
    "include_full": include_full == "1",
    "steps": steps,
}
with open(summary_json, "w", encoding="utf-8") as fh:
    json.dump(payload, fh, ensure_ascii=True, indent=2)
PY

echo "module release node acceptance summary: $summary_md"
echo "module release node acceptance summary json: $summary_json"

if [[ "$overall" != "PASS" ]]; then
  echo "error: module release node acceptance failed" >&2
  exit 1
fi
