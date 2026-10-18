#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

metadata_dir="$repo_root/.tmp/builtin-wasm-sync-modules"
module_set="m1"
sync_script_path=""
hash_manifest_path=""
identity_manifest_path=""
runner_label=""
out_path=""

wasm_env_key() {
  local suffix=$1
  printf 'OASIS7_WASM_%s\n' "$suffix"
}

wasm_env_or_default() {
  local suffix=$1
  local default_value=${2-}
  local key
  key=$(wasm_env_key "$suffix")
  if [[ -n "${!key+x}" ]]; then
    printf '%s\n' "${!key}"
  else
    printf '%s\n' "$default_value"
  fi
}

usage() {
  cat <<'USAGE'
Usage:
  scripts/ci-m1-wasm-summary.sh --out <path> [--module-set <m1|m4|m5>] [--runner-label <label>] [--metadata-dir <dir>]

Options:
  --out <path>            Output summary JSON path (required)
  --module-set <name>     Builtin wasm module set (default: m1, allowed: m1|m4|m5)
  --runner-label <label>  Runner label recorded in summary (default: detected platform key)
  --metadata-dir <dir>    Metadata dir produced by sync script
                          (default: .tmp/builtin-wasm-sync-modules)
  -h, --help              Show this help
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

configure_module_set() {
  case "$module_set" in
    m1)
      sync_script_path="./scripts/sync-m1-builtin-wasm-artifacts.sh"
      hash_manifest_path="$repo_root/crates/oasis7_runtime/src/world/artifacts/m1_builtin_modules.sha256"
      identity_manifest_path="$repo_root/crates/oasis7_runtime/src/world/artifacts/m1_builtin_modules.identity.json"
      ;;
    m4)
      sync_script_path="./scripts/sync-m4-builtin-wasm-artifacts.sh"
      hash_manifest_path="$repo_root/crates/oasis7_runtime/src/world/artifacts/m4_builtin_modules.sha256"
      identity_manifest_path="$repo_root/crates/oasis7_runtime/src/world/artifacts/m4_builtin_modules.identity.json"
      ;;
    m5)
      sync_script_path="./scripts/sync-m5-builtin-wasm-artifacts.sh"
      hash_manifest_path="$repo_root/crates/oasis7_runtime/src/world/artifacts/m5_builtin_modules.sha256"
      identity_manifest_path="$repo_root/crates/oasis7_runtime/src/world/artifacts/m5_builtin_modules.identity.json"
      ;;
    *)
      echo "error: unsupported --module-set value: $module_set" >&2
      echo "hint: allowed values are m1, m4, m5" >&2
      exit 2
      ;;
  esac
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --out)
      [[ $# -ge 2 ]] || { echo "error: --out requires a value" >&2; exit 2; }
      out_path="$2"
      shift 2
      ;;
    --module-set)
      [[ $# -ge 2 ]] || { echo "error: --module-set requires a value" >&2; exit 2; }
      module_set="$2"
      shift 2
      ;;
    --runner-label)
      [[ $# -ge 2 ]] || { echo "error: --runner-label requires a value" >&2; exit 2; }
      runner_label="$2"
      shift 2
      ;;
    --metadata-dir)
      [[ $# -ge 2 ]] || { echo "error: --metadata-dir requires a value" >&2; exit 2; }
      metadata_dir="$2"
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

if [[ -z "$out_path" ]]; then
  echo "error: --out is required" >&2
  usage
  exit 2
fi

configure_module_set
host_platform="$(detect_host_platform)"
canonical_platform="$(wasm_env_or_default CANONICAL_CONTAINER_PLATFORM "linux-x86_64")"
if [[ -z "$runner_label" ]]; then
  runner_label="$host_platform"
fi

"$sync_script_path" --check

mkdir -p "$(dirname "$out_path")"

python3 - "$module_set" "$metadata_dir" "$hash_manifest_path" "$identity_manifest_path" "$out_path" "$runner_label" "$host_platform" "$canonical_platform" <<'PY'
import datetime as dt
import glob
import json
import pathlib
import sys

module_set = sys.argv[1]
metadata_dir = pathlib.Path(sys.argv[2])
hash_manifest_path = pathlib.Path(sys.argv[3])
identity_manifest_path = pathlib.Path(sys.argv[4])
out_path = pathlib.Path(sys.argv[5])
runner_label = sys.argv[6]
host_platform = sys.argv[7]
canonical_platform = sys.argv[8]

manifest_platform_hashes = {}
canonical_platforms = set()
for lineno, raw_line in enumerate(hash_manifest_path.read_text().splitlines(), start=1):
    line = raw_line.strip()
    if not line:
        continue
    tokens = line.split()
    if len(tokens) < 2:
        raise SystemExit(
            f"error: invalid hash manifest line {lineno} in {hash_manifest_path}: {raw_line}"
        )
    module_id = tokens[0]
    keyed_tokens = [token for token in tokens[1:] if "=" in token]
    legacy_tokens = [token for token in tokens[1:] if "=" not in token]
    if legacy_tokens:
        raise SystemExit(
            f"error: legacy hash tokens are not allowed in strict mode for module {module_id}: {legacy_tokens}"
        )
    if not keyed_tokens:
        raise SystemExit(
            f"error: hash manifest line {lineno} has no keyed platform token for module {module_id}"
        )

    selected_hash = None
    for token in keyed_tokens:
        platform_key, hash_value = token.split("=", 1)
        if not platform_key:
            raise SystemExit(
                f"error: empty platform key in hash manifest line {lineno} token {token}"
            )
        canonical_platforms.add(platform_key)
        if platform_key == canonical_platform:
            selected_hash = hash_value

    if selected_hash is None:
        raise SystemExit(
            f"error: hash manifest missing canonical platform {canonical_platform} for module {module_id}"
        )
    manifest_platform_hashes[module_id] = selected_hash

manifest_module_ids = set(manifest_platform_hashes.keys())

metadata_files = sorted(glob.glob(str(metadata_dir / "*.metadata.json")))
if not metadata_files:
    raise SystemExit(f"error: no metadata files found in {metadata_dir}")

module_hashes = {}
for path in metadata_files:
    payload = json.loads(pathlib.Path(path).read_text())
    module_id = payload.get("module_id")
    wasm_hash = payload.get("wasm_hash_sha256")
    if not module_id or not wasm_hash:
        raise SystemExit(f"error: invalid metadata payload in {path}")
    if module_id not in manifest_module_ids:
        continue
    module_hashes[module_id] = wasm_hash

identity_payload = json.loads(identity_manifest_path.read_text())
identity_modules = identity_payload.get("modules", [])
if not isinstance(identity_modules, list):
    raise SystemExit(f"error: invalid identity manifest modules field in {identity_manifest_path}")
identity_build_recipe = identity_payload.get("build_recipe")
if not isinstance(identity_build_recipe, dict):
    raise SystemExit(f"error: invalid identity manifest build_recipe in {identity_manifest_path}")

identity_hashes = {}
identity_source_hashes = {}
identity_build_manifest_hashes = {}
for entry in identity_modules:
    module_id = entry.get("module_id")
    identity_hash = entry.get("identity_hash")
    source_hash = entry.get("source_hash")
    build_manifest_hash = entry.get("build_manifest_hash")
    if not module_id or not identity_hash or not source_hash or not build_manifest_hash:
        raise SystemExit(
            f"error: identity manifest entry missing module_id/identity_hash/source_hash/build_manifest_hash in {identity_manifest_path}"
        )
    if module_id not in manifest_module_ids:
        continue
    identity_hashes[module_id] = identity_hash
    identity_source_hashes[module_id] = source_hash
    identity_build_manifest_hashes[module_id] = build_manifest_hash

receipt_files = sorted(glob.glob(str(metadata_dir / "*.build-receipt.json")))
if not receipt_files:
    raise SystemExit(f"error: no build receipt files found in {metadata_dir}")

receipt_evidence = {}
for path in receipt_files:
    payload = json.loads(pathlib.Path(path).read_text())
    module_id = payload.get("module_id")
    source_hash = payload.get("source_hash")
    build_manifest_hash = payload.get("build_manifest_hash")
    wasm_hash = payload.get("wasm_hash_sha256")
    builder_image_digest = payload.get("builder_image_digest")
    container_platform = payload.get("container_platform")
    canonicalizer_version = payload.get("canonicalizer_version")
    if not all(
        [
            module_id,
            source_hash,
            build_manifest_hash,
            wasm_hash,
            builder_image_digest,
            container_platform,
            canonicalizer_version,
        ]
    ):
        raise SystemExit(f"error: invalid build receipt payload in {path}")
    if module_id not in manifest_module_ids:
        continue
    receipt_evidence[module_id] = {
        "source_hash": source_hash,
        "build_manifest_hash": build_manifest_hash,
        "wasm_hash": wasm_hash,
        "builder_image_digest": builder_image_digest,
        "container_platform": container_platform,
        "canonicalizer_version": canonicalizer_version,
    }

metadata_module_ids = set(module_hashes.keys())
if metadata_module_ids != manifest_module_ids:
    missing = sorted(manifest_module_ids - metadata_module_ids)
    extra = sorted(metadata_module_ids - manifest_module_ids)
    raise SystemExit(
        f"error: module set mismatch between metadata and hash manifest missing={missing} extra={extra}"
    )
identity_module_ids = set(identity_hashes.keys())
if metadata_module_ids != identity_module_ids:
    missing = sorted(metadata_module_ids - identity_module_ids)
    extra = sorted(identity_module_ids - metadata_module_ids)
    raise SystemExit(
        f"error: module set mismatch between metadata and identity manifest missing={missing} extra={extra}"
    )
receipt_module_ids = set(receipt_evidence.keys())
if metadata_module_ids != receipt_module_ids:
    missing = sorted(metadata_module_ids - receipt_module_ids)
    extra = sorted(receipt_module_ids - metadata_module_ids)
    raise SystemExit(
        f"error: module set mismatch between metadata and build receipts missing={missing} extra={extra}"
    )

for module_id, built_hash in sorted(module_hashes.items()):
    expected_hash = manifest_platform_hashes[module_id]
    if built_hash != expected_hash:
        raise SystemExit(
            f"error: built hash does not match manifest for module {module_id} built={built_hash} expected={expected_hash}"
        )
    receipt = receipt_evidence[module_id]
    if receipt["wasm_hash"] != built_hash:
        raise SystemExit(
            f"error: build receipt wasm hash mismatch for module {module_id} receipt={receipt['wasm_hash']} built={built_hash}"
        )
    if receipt["source_hash"] != identity_source_hashes[module_id]:
        raise SystemExit(
            f"error: build receipt source hash mismatch for module {module_id} receipt={receipt['source_hash']} identity={identity_source_hashes[module_id]}"
        )
    if receipt["build_manifest_hash"] != identity_build_manifest_hashes[module_id]:
        raise SystemExit(
            f"error: build receipt build manifest hash mismatch for module {module_id} receipt={receipt['build_manifest_hash']} identity={identity_build_manifest_hashes[module_id]}"
        )
    if receipt["container_platform"] != canonical_platform:
        raise SystemExit(
            f"error: build receipt container platform mismatch for module {module_id} receipt={receipt['container_platform']} canonical={canonical_platform}"
        )

build_recipe_container_platform = identity_build_recipe.get("container_platform")
build_recipe_builder_image_digest = identity_build_recipe.get("builder_image_digest")
build_recipe_canonicalizer_version = identity_build_recipe.get("canonicalizer_version")
if not all(
    [
        build_recipe_container_platform,
        build_recipe_builder_image_digest,
        build_recipe_canonicalizer_version,
    ]
):
    raise SystemExit(
        f"error: identity build_recipe missing container_platform/builder_image_digest/canonicalizer_version in {identity_manifest_path}"
    )
if build_recipe_container_platform != canonical_platform:
    raise SystemExit(
        f"error: identity build_recipe container platform mismatch recipe={build_recipe_container_platform} canonical={canonical_platform}"
    )
for module_id, receipt in sorted(receipt_evidence.items()):
    if receipt["builder_image_digest"] != build_recipe_builder_image_digest:
        raise SystemExit(
            f"error: build receipt builder image digest mismatch for module {module_id} receipt={receipt['builder_image_digest']} recipe={build_recipe_builder_image_digest}"
        )
    if receipt["canonicalizer_version"] != build_recipe_canonicalizer_version:
        raise SystemExit(
            f"error: build receipt canonicalizer version mismatch for module {module_id} receipt={receipt['canonicalizer_version']} recipe={build_recipe_canonicalizer_version}"
        )

summary = {
    "schema_version": 1,
    "module_set": module_set,
    "runner": runner_label,
    "current_platform": canonical_platform,
    "host_platform": host_platform,
    "canonical_platform": canonical_platform,
    "generated_at_utc": dt.datetime.now(dt.timezone.utc).isoformat().replace("+00:00", "Z"),
    "module_count": len(module_hashes),
    "module_hashes": dict(sorted(module_hashes.items())),
    "manifest_platform_hashes": dict(sorted(manifest_platform_hashes.items())),
    "identity_hashes": dict(sorted(identity_hashes.items())),
    "identity_build_recipe": identity_build_recipe,
    "receipt_evidence": dict(sorted(receipt_evidence.items())),
    "canonical_platforms": sorted(canonical_platforms),
    "hash_manifest_path": str(hash_manifest_path),
    "identity_manifest_path": str(identity_manifest_path),
}

out_path.write_text(json.dumps(summary, indent=2, sort_keys=True) + "\n")
print(f"wrote {module_set} wasm summary: {out_path}")
PY
