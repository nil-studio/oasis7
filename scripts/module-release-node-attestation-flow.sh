#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

usage() {
  cat <<'USAGE'
Usage: ./scripts/module-release-node-attestation-flow.sh [options]

Purpose:
  Run the node-side module release attestation evidence flow locally:
  1. collect or import per-runner WASM summaries
  2. verify cross-host Docker canonical evidence
  3. canonicalize proof inputs into stable evidence files
  4. package `proof_payload.json` + `submit_request.json`
  5. optionally submit attestation to the local chain runtime

Outputs:
  <out-dir>/<timestamp>/
    staged_summaries/<module-set>/<runner>.json
    release_evidence/<timestamp>/{summary.md,summary.json,module_sets.tsv,...}
    proof_inputs/release_evidence_summary.json
    proof_inputs/summaries/<module-set>/<runner>.json
    proof/{proof_payload.json,submit_request.json,evidence/...}
    flow_summary.md
    flow_summary.json
    submit_response.json (optional)

Options:
  --out-dir <path>                Output root (default: .tmp/module_release_node_attestation_flow)
  --module-sets <csv>             Module sets to process (default: m1,m4,m5)
  --runner-label <label>          Local runner label used for collection (default: detected host platform)
  --platform <label>              Attestation platform label (default: same as --runner-label)
  --required-runners <csv>        Runner labels required for the stable gate (default: current runner only)
  --expected-runners <csv>        Runner labels expected for full cross-host evidence (default: same as required)
  --summary-import-dir <path>     Pre-collected summary dir to seed staged summaries
  --skip-local-collect            Skip local `ci-m1-wasm-summary.sh` collection
  --external-summary-bundle <p>   External bundle/dir/url to merge into staged summaries (repeatable)
  --require-cross-host-closed     Fail unless evidence summary reaches `gate_result=cross-host-closed`
  --request-id <u64>              Module release request id (required)
  --operator-agent-id <id>        Operator agent id used for submit (required)
  --signer-node-id <id>           Trusted signer node id (required)
  --build-manifest-hash <hex>     Build manifest hash (required)
  --source-hash <hex>             Source hash (required)
  --wasm-hash <hex>               Wasm hash (required)
  --builder-image-digest <value>  Builder image digest (required)
  --container-platform <label>    Container platform (required)
  --canonicalizer-version <id>    Canonicalizer version (required)
  --submit                        Submit `submit_request.json` after packaging
  --base-url <url>                Chain runtime base URL (default: http://127.0.0.1:5121)
  --submit-out <path>             Optional submit response path
  -h, --help                      Show help
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

find_single_run_dir() {
  local root="$1"
  local found
  found="$(find "$root" -mindepth 1 -maxdepth 1 -type d | sort | tail -n 1)"
  if [[ -z "$found" ]]; then
    echo "error: failed to locate generated run dir under $root" >&2
    exit 1
  fi
  printf '%s\n' "$found"
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

out_dir=".tmp/module_release_node_attestation_flow"
module_sets_csv="m1,m4,m5"
runner_label=""
platform=""
required_runners_csv=""
expected_runners_csv=""
summary_import_dir=""
skip_local_collect=0
require_cross_host_closed=0
request_id=""
operator_agent_id=""
signer_node_id=""
build_manifest_hash=""
source_hash=""
wasm_hash=""
builder_image_digest=""
container_platform=""
canonicalizer_version=""
submit=0
base_url="http://127.0.0.1:5121"
submit_out=""
declare -a external_summary_bundles=()

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
    --platform)
      platform=${2:-}
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
    --summary-import-dir)
      summary_import_dir=${2:-}
      shift 2
      ;;
    --skip-local-collect)
      skip_local_collect=1
      shift
      ;;
    --external-summary-bundle)
      external_summary_bundles+=("${2:-}")
      shift 2
      ;;
    --require-cross-host-closed)
      require_cross_host_closed=1
      shift
      ;;
    --request-id)
      request_id=${2:-}
      shift 2
      ;;
    --operator-agent-id)
      operator_agent_id=${2:-}
      shift 2
      ;;
    --signer-node-id)
      signer_node_id=${2:-}
      shift 2
      ;;
    --build-manifest-hash)
      build_manifest_hash=${2:-}
      shift 2
      ;;
    --source-hash)
      source_hash=${2:-}
      shift 2
      ;;
    --wasm-hash)
      wasm_hash=${2:-}
      shift 2
      ;;
    --builder-image-digest)
      builder_image_digest=${2:-}
      shift 2
      ;;
    --container-platform)
      container_platform=${2:-}
      shift 2
      ;;
    --canonicalizer-version)
      canonicalizer_version=${2:-}
      shift 2
      ;;
    --submit)
      submit=1
      shift
      ;;
    --base-url)
      base_url=${2:-}
      shift 2
      ;;
    --submit-out)
      submit_out=${2:-}
      shift 2
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

for required in \
  request_id operator_agent_id signer_node_id build_manifest_hash source_hash wasm_hash \
  builder_image_digest container_platform canonicalizer_version
do
  if [[ -z "${!required}" ]]; then
    echo "error: missing required option for --${required//_/-}" >&2
    usage >&2
    exit 2
  fi
done

if [[ ! "$request_id" =~ ^[0-9]+$ ]] || [[ "$request_id" == "0" ]]; then
  echo "error: --request-id must be a positive integer" >&2
  exit 2
fi

if [[ -z "$runner_label" ]]; then
  runner_label="$(detect_host_platform)"
fi
if [[ -z "$platform" ]]; then
  platform="$runner_label"
fi
if [[ -z "$required_runners_csv" ]]; then
  required_runners_csv="$runner_label"
fi
if [[ -z "$expected_runners_csv" ]]; then
  expected_runners_csv="$required_runners_csv"
fi

if [[ "$skip_local_collect" -eq 1 && -z "$summary_import_dir" && "${#external_summary_bundles[@]}" -eq 0 ]]; then
  echo "error: --skip-local-collect requires --summary-import-dir or --external-summary-bundle" >&2
  exit 2
fi

timestamp="$(date '+%Y%m%d-%H%M%S')"
run_dir="$out_dir/$timestamp"
staged_root="$run_dir/staged_summaries"
report_root="$run_dir/release_evidence"
proof_inputs_dir="$run_dir/proof_inputs"
proof_dir="$run_dir/proof"
merge_root="$run_dir/.merge"
mkdir -p "$staged_root" "$report_root" "$proof_inputs_dir" "$proof_dir" "$merge_root"

IFS=',' read -r -a module_sets <<< "$module_sets_csv"
module_set_count=0
for module_set in "${module_sets[@]}"; do
  module_set="$(echo "$module_set" | xargs)"
  [[ -n "$module_set" ]] || continue
  module_set_count=$((module_set_count + 1))
done

if [[ "$module_set_count" -eq 0 ]]; then
  echo "error: --module-sets has no valid entries" >&2
  exit 2
fi

for module_set in "${module_sets[@]}"; do
  module_set="$(echo "$module_set" | xargs)"
  [[ -n "$module_set" ]] || continue

  module_stage_dir="$staged_root/$module_set"
  mkdir -p "$module_stage_dir"
  copy_summary_imports "$module_set" "$summary_import_dir" "$module_stage_dir" "$module_set_count"

  if [[ "$skip_local_collect" -eq 0 ]]; then
    ./scripts/ci-m1-wasm-summary.sh \
      --module-set "$module_set" \
      --runner-label "$runner_label" \
      --out "$module_stage_dir/$runner_label.json"
  fi

  if [[ "${#external_summary_bundles[@]}" -gt 0 ]]; then
    current_dir="$module_stage_dir"
    bundle_index=0
    for bundle_path in "${external_summary_bundles[@]}"; do
      bundle_index=$((bundle_index + 1))
      next_dir="$merge_root/${module_set}-${bundle_index}"
      ./scripts/stage-wasm-summary-imports.sh \
        --module-set "$module_set" \
        --local-summary-dir "$current_dir" \
        --out-dir "$next_dir" \
        --external-summary-bundle "$bundle_path" \
        --expected-external-runner ""
      if [[ "$current_dir" != "$module_stage_dir" ]]; then
        rm -rf "$current_dir"
      fi
      rm -rf "$module_stage_dir"
      mv "$next_dir" "$module_stage_dir"
      current_dir="$module_stage_dir"
    done
  fi
done

report_cmd=(
  ./scripts/wasm-release-evidence-report.sh
  --out-dir "$report_root"
  --module-sets "$module_sets_csv"
  --runner-label "$runner_label"
  --required-runners "$required_runners_csv"
  --expected-runners "$expected_runners_csv"
  --skip-collect
  --summary-import-dir "$staged_root"
)
"${report_cmd[@]}"

report_run_dir="$(find_single_run_dir "$report_root")"
report_summary_json="$report_run_dir/summary.json"

python3 - "$staged_root" "$report_summary_json" "$proof_inputs_dir" <<'PY'
import hashlib
import json
import pathlib
import sys

staged_root = pathlib.Path(sys.argv[1])
report_summary_path = pathlib.Path(sys.argv[2])
proof_inputs_dir = pathlib.Path(sys.argv[3])
proof_summaries_root = proof_inputs_dir / "summaries"
proof_summaries_root.mkdir(parents=True, exist_ok=True)

report = json.loads(report_summary_path.read_text())
if (
    type(report.get("schema_version", 1)) is int
    and report.get("schema_version", 1) == 1
    and "purpose" not in report
    and "baseline_sha256" not in report
):
    raise SystemExit("error: legacy-envelope/current-freshness-unverified: original evidence retained; canonical rebuild required for current gates or new proof assembly")
if report.get("purpose") != "release-build" or report.get("baseline_sha256") != "":
    raise SystemExit("error: node attestation rejects candidate/unknown report purpose")
summary_file_index: dict[str, dict[str, dict[str, str]]] = {}

canonical_summary_keys = (
    "schema_version",
    "purpose",
    "baseline_sha256",
    "module_set",
    "runner",
    "host_platform",
    "canonical_platform",
    "module_count",
    "module_hashes",
    "manifest_platform_hashes",
    "identity_hashes",
    "identity_build_recipe",
    "receipt_evidence",
    "canonical_platforms",
)

for module_dir in sorted(path for path in staged_root.iterdir() if path.is_dir()):
    module_set = module_dir.name
    module_target_dir = proof_summaries_root / module_set
    module_target_dir.mkdir(parents=True, exist_ok=True)
    runner_index: dict[str, dict[str, str]] = {}
    for summary_path in sorted(module_dir.glob("*.json")):
      payload = json.loads(summary_path.read_text())
      if (
          type(payload.get("schema_version")) is int
          and payload["schema_version"] == 1
          and "purpose" not in payload
          and "baseline_sha256" not in payload
      ):
          raise SystemExit("error: legacy-envelope/current-freshness-unverified: original evidence retained; canonical rebuild required for current gates or new proof assembly")
      if payload.get("purpose") != "release-build" or payload.get("baseline_sha256") != "":
          raise SystemExit("error: candidate/unknown build baseline cannot authorize a release proof")
      canonical_payload = {
          key: payload[key]
          for key in canonical_summary_keys
          if key in payload
      }
      runner = canonical_payload.get("runner")
      if not isinstance(runner, str) or not runner:
          raise SystemExit(f"error: summary missing runner field: {summary_path}")
      canonical_path = module_target_dir / f"{runner}.json"
      canonical_text = json.dumps(
          canonical_payload, ensure_ascii=True, indent=2, sort_keys=True
      ) + "\n"
      canonical_path.write_text(canonical_text)
      digest = hashlib.sha256(canonical_text.encode("utf-8")).hexdigest()
      runner_index[runner] = {
          "path": str(canonical_path.relative_to(proof_inputs_dir)),
          "sha256": digest,
      }
    summary_file_index[module_set] = runner_index

proof_summary = {
    "purpose": report["purpose"],
    "baseline_sha256": report["baseline_sha256"],
    "schema_version": 1,
    "required_runners": report.get("required_runners", []),
    "expected_runners": report.get("expected_runners", []),
    "received_runners": report.get("received_runners", []),
    "missing_required_runners": report.get("missing_required_runners", []),
    "missing_runners": report.get("missing_runners", []),
    "extra_runners": report.get("extra_runners", []),
    "stable_gate_passed": report.get("stable_gate_passed", False),
    "cross_host_evidence_pending": report.get("cross_host_evidence_pending", False),
    "cross_host_closed": report.get("cross_host_closed", False),
    "gate_result": report.get("gate_result"),
    "module_sets": [],
}

for module_entry in report.get("module_sets", []):
    module_set = module_entry["module_set"]
    runner_index = summary_file_index.get(module_set, {})
    summaries = []
    for runner in sorted(runner_index):
        payload = runner_index[runner]
        summaries.append(
            {
                "runner": runner,
                "path": payload["path"],
                "sha256": payload["sha256"],
            }
        )
    proof_summary["module_sets"].append(
        {
            "module_set": module_set,
            "required_runners": module_entry.get("required_runners", []),
            "expected_runners": module_entry.get("expected_runners", []),
            "received_runners": module_entry.get("received_runners", []),
            "missing_required_runners": module_entry.get("missing_required_runners", []),
            "missing_runners": module_entry.get("missing_runners", []),
            "extra_runners": module_entry.get("extra_runners", []),
            "stable_gate_passed": module_entry.get("stable_gate_passed", False),
            "cross_host_evidence_pending": module_entry.get("cross_host_evidence_pending", False),
            "cross_host_closed": module_entry.get("cross_host_closed", False),
            "canonical_hash_consistent": module_entry.get("canonical_hash_consistent", False),
            "receipt_evidence_consistent": module_entry.get("receipt_evidence_consistent", False),
            "gate_result": module_entry.get("gate_result"),
            "summaries": summaries,
        }
    )

(proof_inputs_dir / "release_evidence_summary.json").write_text(
    json.dumps(proof_summary, ensure_ascii=True, indent=2, sort_keys=True) + "\n"
)
PY

proof_summary_json="$proof_inputs_dir/release_evidence_summary.json"
if [[ "$require_cross_host_closed" -eq 1 ]]; then
  gate_result="$(jq -r '.gate_result' "$proof_summary_json")"
  if [[ "$gate_result" != "cross-host-closed" ]]; then
    echo "error: proof evidence gate is not cross-host-closed: $gate_result" >&2
    exit 1
  fi
fi

declare -a proof_evidence_args=()
while IFS= read -r -d '' path; do
  proof_evidence_args+=(--evidence-file "$path")
done < <(find "$proof_inputs_dir/summaries" -type f -name '*.json' -print0 | sort -z)

package_cmd=(
  ./scripts/package-module-release-attestation-proof.sh
  --out-dir "$proof_dir"
  --request-id "$request_id"
  --operator-agent-id "$operator_agent_id"
  --signer-node-id "$signer_node_id"
  --platform "$platform"
  --build-manifest-hash "$build_manifest_hash"
  --source-hash "$source_hash"
  --wasm-hash "$wasm_hash"
  --builder-image-digest "$builder_image_digest"
  --container-platform "$container_platform"
  --canonicalizer-version "$canonicalizer_version"
  --evidence-summary-json "$proof_summary_json"
  "${proof_evidence_args[@]}"
)
"${package_cmd[@]}"

submit_status="skipped"
submit_response_path=""
if [[ "$submit" -eq 1 ]]; then
  submit_status="submitted"
  if [[ -z "$submit_out" ]]; then
    submit_out="$run_dir/submit_response.json"
  fi
  ./scripts/submit-module-release-attestation.sh \
    --request-json "$proof_dir/submit_request.json" \
    --base-url "$base_url" \
    --out "$submit_out"
  submit_response_path="$submit_out"
fi

python3 - "$report_summary_json" "$proof_summary_json" "$proof_dir/proof_payload.json" "$run_dir" "$submit_status" "$submit_response_path" "$report_run_dir" <<'PY'
import json
import pathlib
import sys

report_summary_path = pathlib.Path(sys.argv[1])
proof_summary_path = pathlib.Path(sys.argv[2])
proof_payload_path = pathlib.Path(sys.argv[3])
run_dir = pathlib.Path(sys.argv[4])
submit_status = sys.argv[5]
submit_response_path = sys.argv[6] or None
report_run_dir = pathlib.Path(sys.argv[7])

report_summary = json.loads(report_summary_path.read_text())
proof_summary = json.loads(proof_summary_path.read_text())
proof_payload = json.loads(proof_payload_path.read_text())

payload = {
    "run_dir": str(run_dir),
    "report_run_dir": str(report_run_dir),
    "report_gate_result": report_summary.get("gate_result"),
    "proof_gate_result": proof_summary.get("gate_result"),
    "received_runners": proof_summary.get("received_runners", []),
    "cross_host_evidence_pending": proof_summary.get("cross_host_evidence_pending"),
    "proof_cid": proof_payload.get("proof_cid"),
    "proof_payload_sha256": proof_payload.get("payload_sha256"),
    "submit_status": submit_status,
    "submit_response_path": submit_response_path,
    "proof_payload_path": str(proof_payload_path),
    "submit_request_path": str(run_dir / "proof" / "submit_request.json"),
    "proof_evidence_summary_path": str(proof_summary_path),
}
(run_dir / "flow_summary.json").write_text(
    json.dumps(payload, ensure_ascii=True, indent=2) + "\n"
)
PY

{
  echo "# Module Release Node Attestation Flow"
  echo ""
  echo "- Run dir: \`$run_dir\`"
  echo "- Runner label: \`$runner_label\`"
  echo "- Attestation platform: \`$platform\`"
  echo "- Required runners: \`$required_runners_csv\`"
  echo "- Expected runners: \`$expected_runners_csv\`"
  echo "- External bundles: \`${#external_summary_bundles[@]}\`"
  echo "- Skip local collect: \`$skip_local_collect\`"
  echo "- Require cross-host closed: \`$require_cross_host_closed\`"
  echo "- Report gate result: \`$(jq -r '.gate_result' "$report_summary_json")\`"
  echo "- Proof gate result: \`$(jq -r '.gate_result' "$proof_summary_json")\`"
  echo "- Received runners: \`$(jq -r '.received_runners | join(",")' "$proof_summary_json")\`"
  echo "- Proof CID: \`$(jq -r '.proof_cid' "$proof_dir/proof_payload.json")\`"
  echo "- Submit status: \`$submit_status\`"
  echo "- Release evidence report: \`$report_run_dir\`"
  echo "- Proof payload: \`$proof_dir/proof_payload.json\`"
  echo "- Submit request: \`$proof_dir/submit_request.json\`"
  if [[ -n "$submit_response_path" ]]; then
    echo "- Submit response: \`$submit_response_path\`"
  fi
} > "$run_dir/flow_summary.md"

echo "module release node attestation flow summary: $run_dir/flow_summary.md"
echo "module release node attestation flow summary json: $run_dir/flow_summary.json"
