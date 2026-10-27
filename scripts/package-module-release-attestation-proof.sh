#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

usage() {
  cat <<'USAGE'
Usage: ./scripts/package-module-release-attestation-proof.sh [options]

Purpose:
  Package a node-side module release attestation proof payload and generate a
  matching submit_request.json for `/v1/chain/module-release/attestation/submit`.

Outputs:
  <out-dir>/
    proof_payload.json
    submit_request.json
    evidence/
      00-summary.json
      01-*.json|log|...

Options:
  --out-dir <path>                Output directory (required)
  --request-id <u64>              Module release request id (required)
  --operator-agent-id <id>        Operator agent id used for submit (required)
  --signer-node-id <id>           Trusted signer node id (required)
  --platform <label>              Attestation platform label (required)
  --build-manifest-hash <hex>     Build manifest hash (required)
  --source-hash <hex>             Source hash (required)
  --wasm-hash <hex>               Wasm hash (required)
  --builder-image-digest <value>  Builder image digest (required)
  --container-platform <label>    Container platform (required)
  --canonicalizer-version <id>    Canonicalizer version (required)
  --evidence-summary-json <path>  Primary release evidence summary json (required)
  --evidence-file <path>          Extra evidence file to copy into payload (repeatable)
  --archive <path>                Optional .tar.gz archive output
  -h, --help                      Show help
USAGE
}

out_dir=""
request_id=""
operator_agent_id=""
signer_node_id=""
platform=""
build_manifest_hash=""
source_hash=""
wasm_hash=""
builder_image_digest=""
container_platform=""
canonicalizer_version=""
evidence_summary_json=""
archive_path=""
declare -a evidence_files=()

while [[ $# -gt 0 ]]; do
  case "$1" in
    --out-dir)
      out_dir=${2:-}
      shift 2
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
    --platform)
      platform=${2:-}
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
    --evidence-summary-json)
      evidence_summary_json=${2:-}
      shift 2
      ;;
    --evidence-file)
      evidence_files+=("${2:-}")
      shift 2
      ;;
    --archive)
      archive_path=${2:-}
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
  out_dir request_id operator_agent_id signer_node_id platform build_manifest_hash \
  source_hash wasm_hash builder_image_digest container_platform canonicalizer_version \
  evidence_summary_json
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

if [[ ! -f "$evidence_summary_json" ]]; then
  echo "error: --evidence-summary-json not found: $evidence_summary_json" >&2
  exit 2
fi

for path in "${evidence_files[@]}"; do
  if [[ ! -f "$path" ]]; then
    echo "error: --evidence-file not found: $path" >&2
    exit 2
  fi
done

mkdir -p "$out_dir/evidence"
cp "$evidence_summary_json" "$out_dir/evidence/00-summary.json"

index=1
for path in "${evidence_files[@]}"; do
  base_name="$(basename "$path")"
  printf -v target_name '%02d-%s' "$index" "$base_name"
  cp "$path" "$out_dir/evidence/$target_name"
  index=$((index + 1))
done

proof_cid="$(python3 - "$out_dir" "$request_id" "$operator_agent_id" "$signer_node_id" "$platform" \
  "$build_manifest_hash" "$source_hash" "$wasm_hash" "$builder_image_digest" \
  "$container_platform" "$canonicalizer_version" <<'PY'
import datetime as dt
import hashlib
import json
import pathlib
import sys

(
    out_dir_raw,
    request_id_raw,
    operator_agent_id,
    signer_node_id,
    platform,
    build_manifest_hash,
    source_hash,
    wasm_hash,
    builder_image_digest,
    container_platform,
    canonicalizer_version,
) = sys.argv[1:]

out_dir = pathlib.Path(out_dir_raw)
evidence_dir = out_dir / "evidence"
evidence_entries = []
summary_relpath = None
for path in sorted(evidence_dir.iterdir()):
    if not path.is_file():
        continue
    digest = hashlib.sha256(path.read_bytes()).hexdigest()
    relpath = str(path.relative_to(out_dir))
    entry = {
        "path": relpath,
        "sha256": digest,
        "size_bytes": path.stat().st_size,
    }
    if path.name == "00-summary.json":
        summary = json.loads(path.read_text())
        if (
            type(summary.get("schema_version", 1)) is int
            and summary.get("schema_version", 1) == 1
            and "purpose" not in summary
            and "baseline_sha256" not in summary
        ):
            raise SystemExit("error: legacy-envelope/current-freshness-unverified: original evidence retained; canonical rebuild required for current gates or new proof assembly")
        if summary.get("purpose") != "release-build" or summary.get("baseline_sha256") != "":
            raise SystemExit("error: release proof rejects candidate/unknown evidence purpose")
        entry["role"] = "release_evidence_summary"
        summary_relpath = relpath
    else:
        entry["role"] = "attachment"
    evidence_entries.append(entry)

if summary_relpath is None:
    raise SystemExit("error: missing copied release evidence summary")

base_payload = {
    "proof_schema_version": 1,
    "request_id": int(request_id_raw),
    "operator_agent_id": operator_agent_id,
    "signer_node_id": signer_node_id,
    "platform": platform,
    "build_manifest_hash": build_manifest_hash,
    "source_hash": source_hash,
    "wasm_hash": wasm_hash,
    "builder_image_digest": builder_image_digest,
    "container_platform": container_platform,
    "canonicalizer_version": canonicalizer_version,
    "release_evidence_summary": summary_relpath,
    "attached_files": evidence_entries,
}
canonical_bytes = json.dumps(
    base_payload, ensure_ascii=True, sort_keys=True, separators=(",", ":")
).encode("utf-8")
payload_sha256 = hashlib.sha256(canonical_bytes).hexdigest()
proof_cid = f"sha256:{payload_sha256}"

proof_payload = dict(base_payload)
proof_payload["generated_at_utc"] = (
    dt.datetime.now(dt.timezone.utc).isoformat().replace("+00:00", "Z")
)
proof_payload["payload_sha256"] = payload_sha256
proof_payload["proof_cid"] = proof_cid

submit_request = {
    "operator_agent_id": operator_agent_id,
    "request_id": int(request_id_raw),
    "signer_node_id": signer_node_id,
    "platform": platform,
    "build_manifest_hash": build_manifest_hash,
    "source_hash": source_hash,
    "wasm_hash": wasm_hash,
    "proof_cid": proof_cid,
    "builder_image_digest": builder_image_digest,
    "container_platform": container_platform,
    "canonicalizer_version": canonicalizer_version,
}

(out_dir / "proof_payload.json").write_text(
    json.dumps(proof_payload, ensure_ascii=True, indent=2) + "\n"
)
(out_dir / "submit_request.json").write_text(
    json.dumps(submit_request, ensure_ascii=True, indent=2) + "\n"
)
print(proof_cid)
PY
)"

if [[ -n "$archive_path" ]]; then
  mkdir -p "$(dirname "$archive_path")"
  tar -C "$out_dir" -czf "$archive_path" proof_payload.json submit_request.json evidence
fi

echo "packaged module release attestation proof: $out_dir"
echo "proof_cid: $proof_cid"
echo "proof payload: $out_dir/proof_payload.json"
echo "submit request: $out_dir/submit_request.json"
if [[ -n "$archive_path" ]]; then
  echo "proof archive: $archive_path"
fi
