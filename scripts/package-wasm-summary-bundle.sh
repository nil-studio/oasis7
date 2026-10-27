#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

usage() {
  cat <<'USAGE'
Usage: ./scripts/package-wasm-summary-bundle.sh [options]

Purpose:
  Create a normalized multi-module WASM summary bundle for external evidence import.

Outputs:
  <out-dir>/
    bundle_manifest.json
    summaries/<module-set>/<runner>.json

Options:
  --out-dir <path>              Output bundle directory (required)
  --archive <path>              Optional .tar.gz archive path
  --module-sets <csv>           Module sets to package (default: m1,m4,m5)
  --runner-label <label>        Runner label recorded in bundle (default: detected host platform)
  --source-summary-root <path>  Repackage existing summary files instead of collecting fresh ones
  -h, --help                    Show help
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

out_dir=""
archive_path=""
module_sets_csv="m1,m4,m5"
runner_label=""
baseline_kind="release-build"
source_summary_root=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --out-dir)
      out_dir=${2:-}
      shift 2
      ;;
    --archive)
      archive_path=${2:-}
      shift 2
      ;;
    --baseline-kind)
      baseline_kind=${2:-}
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
    --source-summary-root)
      source_summary_root=${2:-}
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

if [[ -z "$out_dir" ]]; then
  echo "error: --out-dir is required" >&2
  usage >&2
  exit 2
fi

if [[ -z "$runner_label" ]]; then
  runner_label="$(detect_host_platform)"
fi

mkdir -p "$out_dir"
summaries_root="$out_dir/summaries"
mkdir -p "$summaries_root"

IFS=',' read -r -a module_sets <<< "$module_sets_csv"
bundle_modules=()
for module_set in "${module_sets[@]}"; do
  module_set="$(echo "$module_set" | xargs)"
  [[ -n "$module_set" ]] || continue
  bundle_modules+=("$module_set")
  module_dir="$summaries_root/$module_set"
  mkdir -p "$module_dir"
  target_summary="$module_dir/$runner_label.json"

  if [[ -n "$source_summary_root" ]]; then
    source_summary="$source_summary_root/$module_set/$runner_label.json"
    if [[ ! -f "$source_summary" && "${#bundle_modules[@]}" -eq 1 ]]; then
      source_summary="$source_summary_root/$runner_label.json"
    fi
    if [[ ! -f "$source_summary" ]]; then
      echo "error: summary not found for module_set=$module_set runner=$runner_label under $source_summary_root" >&2
      exit 2
    fi
    cp "$source_summary" "$target_summary"
  else
    ./scripts/ci-m1-wasm-summary.sh --baseline-kind "$baseline_kind" \
      --module-set "$module_set" \
      --runner-label "$runner_label" \
      --out "$target_summary"
  fi
done

python3 - "$out_dir" "$runner_label" "${bundle_modules[@]}" <<'PY'
import datetime as dt
import json
import pathlib
import sys

out_dir = pathlib.Path(sys.argv[1])
runner_label = sys.argv[2]
module_sets = sys.argv[3:]
if not module_sets:
    raise SystemExit("error: no module sets selected for bundle")

summary_files = {}
host_platform = None
canonical_platforms = set()
baseline_identity = None
for module_set in module_sets:
    summary_path = out_dir / "summaries" / module_set / f"{runner_label}.json"
    if not summary_path.is_file():
        raise SystemExit(f"error: summary missing from bundle: {summary_path}")
    payload = json.loads(summary_path.read_text())
    purpose = payload.get("purpose")
    digest = payload.get("baseline_sha256")
    if purpose not in ("candidate-build", "release-build") or (purpose == "release-build" and digest != "") or (purpose == "candidate-build" and (not isinstance(digest, str) or len(digest) != 64 or any(c not in "0123456789abcdef" for c in digest))):
        raise SystemExit("error: unknown/malformed summary baseline purpose")

    identity = (purpose, digest)
    if baseline_identity is not None and baseline_identity != identity:
        raise SystemExit("error: mixed summary baseline purposes")
    baseline_identity = identity

    if payload.get("schema_version") != 1:
        raise SystemExit(
            f"error: summary {summary_path} schema_version must be 1, got {payload.get('schema_version')}"
        )
    if payload.get("module_set") != module_set:
        raise SystemExit(
            f"error: summary {summary_path} module_set mismatch expected={module_set} actual={payload.get('module_set')}"
        )
    if payload.get("runner") != runner_label:
        raise SystemExit(
            f"error: summary {summary_path} runner mismatch expected={runner_label} actual={payload.get('runner')}"
        )
    summary_host_platform = payload.get("host_platform")
    if not isinstance(summary_host_platform, str) or not summary_host_platform:
        raise SystemExit(f"error: summary {summary_path} missing host_platform")
    if host_platform is None:
        host_platform = summary_host_platform
    elif host_platform != summary_host_platform:
        raise SystemExit(
            f"error: bundle host_platform mismatch expected={host_platform} actual={summary_host_platform} for {summary_path}"
        )
    canonical_platform = payload.get("canonical_platform")
    if not isinstance(canonical_platform, str) or not canonical_platform:
        raise SystemExit(f"error: summary {summary_path} missing canonical_platform")
    build_recipe = payload.get("identity_build_recipe")
    if not isinstance(build_recipe, dict):
        raise SystemExit(f"error: summary {summary_path} missing identity_build_recipe")
    recipe_platform = build_recipe.get("container_platform")
    if canonical_platform != recipe_platform:
        raise SystemExit(
            f"error: summary {summary_path} canonical_platform mismatch summary={canonical_platform} recipe={recipe_platform}"
        )
    canonical_platforms.add(canonical_platform)
    summary_files[module_set] = str(summary_path.relative_to(out_dir))

manifest = {
    "schema_version": 1,
    "runner_label": runner_label,
    "host_platform": host_platform,
    "generated_at_utc": dt.datetime.now(dt.timezone.utc).isoformat().replace("+00:00", "Z"),
    "module_sets": module_sets,
    "canonical_platforms": sorted(canonical_platforms),
    "summary_files": summary_files,
}
(out_dir / "bundle_manifest.json").write_text(
    json.dumps(manifest, ensure_ascii=True, indent=2) + "\n"
)
PY

if [[ -n "$archive_path" ]]; then
  mkdir -p "$(dirname "$archive_path")"
  tar -C "$out_dir" -czf "$archive_path" bundle_manifest.json summaries
fi

echo "packaged wasm summary bundle: $out_dir"
if [[ -n "$archive_path" ]]; then
  echo "packaged wasm summary bundle archive: $archive_path"
fi
