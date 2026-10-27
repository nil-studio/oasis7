#!/usr/bin/env bash
set -euo pipefail
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

usage() {
  cat <<'USAGE'
Usage: ./scripts/stage-wasm-summary-imports.sh [options]

Purpose:
  Merge local GitHub-hosted summaries with an optional external summary bundle
  into one verify-ready directory for `wasm-release-evidence-report.sh`.

Options:
  --module-set <name>               Module set to stage (required)
  --local-summary-dir <path>        Local summary directory to seed from (required)
  --out-dir <path>                  Output directory with merged summaries (required)
  --external-summary-bundle <path>  Optional external bundle path/URL (.tar/.tar.gz/.tgz/.zip or dir)
  --expected-external-runner <id>   Expected external runner label (default: darwin-arm64)
  --expected-external-host <id>     Expected host platform recorded by the external bundle (default: same as runner)
  --expected-canonical-platform <id>
                                    Expected canonical container platform for imported summaries
                                    (default: linux-x86_64)
  -h, --help                        Show help
USAGE
}

module_set=""
local_summary_dir=""
out_dir=""
external_summary_bundle=""
expected_external_runner="darwin-arm64"
expected_external_host=""
expected_canonical_platform="linux-x86_64"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --module-set)
      module_set=${2:-}
      shift 2
      ;;
    --local-summary-dir)
      local_summary_dir=${2:-}
      shift 2
      ;;
    --out-dir)
      out_dir=${2:-}
      shift 2
      ;;
    --external-summary-bundle)
      external_summary_bundle=${2:-}
      shift 2
      ;;
    --expected-external-runner)
      expected_external_runner=${2:-}
      shift 2
      ;;
    --expected-external-host)
      expected_external_host=${2:-}
      shift 2
      ;;
    --expected-canonical-platform)
      expected_canonical_platform=${2:-}
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

if [[ -z "$expected_external_host" ]]; then
  expected_external_host="$expected_external_runner"
fi

if [[ -z "$module_set" || -z "$local_summary_dir" || -z "$out_dir" ]]; then
  echo "error: --module-set, --local-summary-dir and --out-dir are required" >&2
  usage >&2
  exit 2
fi

if [[ ! -d "$local_summary_dir" ]]; then
  echo "error: local summary dir not found: $local_summary_dir" >&2
  exit 2
fi

mkdir -p "$out_dir"
shopt -s nullglob
local_files=("$local_summary_dir"/*.json)
shopt -u nullglob
if [[ "${#local_files[@]}" -eq 0 ]]; then
  echo "error: no local summary json files found in $local_summary_dir" >&2
  exit 2
fi
cp "${local_files[@]}" "$out_dir/"

if [[ -z "$external_summary_bundle" ]]; then
  echo "staged wasm summaries: $out_dir"
  exit 0
fi

tmp_root="$(mktemp -d "${TMPDIR:-/tmp}/wasm-summary-stage.XXXXXX")"
cleanup() {
  rm -rf "$tmp_root"
}
trap cleanup EXIT

bundle_source="$external_summary_bundle"
if [[ "$external_summary_bundle" =~ ^https?:// ]]; then
  bundle_name="$(basename "${external_summary_bundle%%\?*}")"
  if [[ -z "$bundle_name" || "$bundle_name" == "/" ]]; then
    bundle_name="external-summary-bundle.tar.gz"
  fi
  bundle_source="$tmp_root/$bundle_name"
  curl -fsSL "$external_summary_bundle" -o "$bundle_source"
fi

python3 -I - "$bundle_source" "$tmp_root/extracted" "$ROOT_DIR/scripts/safe_git_archive.py" <<'PY'
import pathlib
import shutil
import sys
import importlib.util
import zipfile

spec = importlib.util.spec_from_file_location("safe_git_archive", pathlib.Path(sys.argv[3]).resolve())
helper = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = helper
spec.loader.exec_module(helper)
source = pathlib.Path(sys.argv[1])
extract_root = pathlib.Path(sys.argv[2])
extract_root.mkdir(parents=True, exist_ok=True)

if source.is_dir():
    if any(path.is_symlink() for path in source.rglob("*")):
        raise SystemExit("error: directory bundle contains a symlink")
    shutil.copytree(source, extract_root / "bundle", dirs_exist_ok=True)
    print(extract_root / "bundle")
    raise SystemExit(0)

if not source.exists():
    raise SystemExit(f"error: external summary bundle not found: {source}")

name = source.name.lower()
target = extract_root / "bundle"
if name.endswith((".tar.gz", ".tgz", ".tar")):
    helper.extract_archive(source, target)
elif name.endswith(".zip"):
    with zipfile.ZipFile(source) as archive:
        entries = archive.infolist()
        limits = helper.ArchiveLimits()
        if len(entries) > limits.members or sum(e.file_size for e in entries) > limits.expanded_bytes:
            raise SystemExit("error: zip bundle exceeds extraction budget")
        seen = set()
        for entry in entries:
            path = helper._path(entry.filename)
            if path in seen or entry.file_size > limits.member_bytes or ((entry.external_attr >> 16) & 0o170000) == 0o120000:
                raise SystemExit("error: unsafe zip bundle entry")
            seen.add(path)
        target.mkdir(parents=True)
        try:
            archive.extractall(target)
        except BaseException:
            shutil.rmtree(target)
            raise
else:
    raise SystemExit(
        "error: external summary bundle must be a directory or archive (.tar/.tar.gz/.tgz/.zip)"
    )
print(target)
PY

bundle_root="$(python3 - "$bundle_source" "$tmp_root/extracted" <<'PY'
import pathlib
import sys

extract_root = pathlib.Path(sys.argv[2]) / "bundle"
manifest_candidates = [extract_root / "bundle_manifest.json"]
manifest_candidates.extend(extract_root.glob("*/bundle_manifest.json"))
for candidate in manifest_candidates:
    if candidate.is_file():
        print(candidate.parent)
        raise SystemExit(0)
raise SystemExit("error: bundle_manifest.json not found after extraction")
PY
)"

python3 - "$bundle_root" "$module_set" "$expected_external_runner" "$expected_external_host" "$expected_canonical_platform" "$out_dir" <<'PY'
import json
import pathlib
import shutil
import sys

bundle_root = pathlib.Path(sys.argv[1])
module_set = sys.argv[2]
expected_runner = sys.argv[3]
expected_host = sys.argv[4]
expected_canonical_platform = sys.argv[5]
out_dir = pathlib.Path(sys.argv[6])

manifest_path = bundle_root / "bundle_manifest.json"
manifest = json.loads(manifest_path.read_text())
if manifest.get("schema_version") != 1:
    raise SystemExit(f"error: unsupported bundle manifest schema: {manifest.get('schema_version')}")

runner_label = manifest.get("runner_label")
if not runner_label:
    raise SystemExit(f"error: bundle manifest missing runner_label: {manifest_path}")
if expected_runner and runner_label != expected_runner:
    raise SystemExit(
        f"error: external bundle runner mismatch expected={expected_runner} actual={runner_label}"
    )
bundle_host_platform = manifest.get("host_platform")
if not isinstance(bundle_host_platform, str) or not bundle_host_platform:
    raise SystemExit(f"error: bundle manifest missing host_platform: {manifest_path}")
if expected_host and bundle_host_platform != expected_host:
    raise SystemExit(
        f"error: external bundle host_platform mismatch expected={expected_host} actual={bundle_host_platform}"
    )
bundle_canonical_platforms = manifest.get("canonical_platforms")
if not isinstance(bundle_canonical_platforms, list) or not bundle_canonical_platforms:
    raise SystemExit(f"error: bundle manifest missing canonical_platforms: {manifest_path}")
if expected_canonical_platform and bundle_canonical_platforms != [expected_canonical_platform]:
    raise SystemExit(
        "error: external bundle canonical_platforms mismatch expected={} actual={}".format(
            [expected_canonical_platform], bundle_canonical_platforms
        )
    )

summary_files = manifest.get("summary_files")
if not isinstance(summary_files, dict):
    raise SystemExit(f"error: bundle manifest missing summary_files object: {manifest_path}")
relative_summary = summary_files.get(module_set)
if not relative_summary:
    raise SystemExit(
        f"error: bundle manifest missing module_set={module_set} summary entry: {manifest_path}"
    )

summary_path = (bundle_root / relative_summary).resolve()
if not summary_path.is_file():
    raise SystemExit(f"error: bundle summary file missing: {summary_path}")

payload = json.loads(summary_path.read_text())
purpose = payload.get("purpose")
digest = payload.get("baseline_sha256")
if purpose not in ("candidate-build", "release-build") or (purpose == "release-build" and digest != "") or (purpose == "candidate-build" and (not isinstance(digest, str) or len(digest) != 64 or any(c not in "0123456789abcdef" for c in digest))):
    raise SystemExit("error: unknown/malformed summary baseline purpose")

if (manifest.get("purpose"), manifest.get("baseline_sha256")) != (purpose, digest):
    raise SystemExit("error: bundle manifest/summary purpose mismatch")

if payload.get("module_set") != module_set:
    raise SystemExit(
        f"error: external summary module_set mismatch expected={module_set} actual={payload.get('module_set')}"
    )
if payload.get("runner") != runner_label:
    raise SystemExit(
        f"error: external summary runner mismatch expected={runner_label} actual={payload.get('runner')}"
    )
summary_host_platform = payload.get("host_platform")
if not isinstance(summary_host_platform, str) or not summary_host_platform:
    raise SystemExit(
        f"error: external summary missing host_platform: {summary_path}"
    )
if expected_host and summary_host_platform != expected_host:
    raise SystemExit(
        f"error: external summary host_platform mismatch expected={expected_host} actual={summary_host_platform}"
    )
summary_canonical_platform = payload.get("canonical_platform")
if not isinstance(summary_canonical_platform, str) or not summary_canonical_platform:
    raise SystemExit(
        f"error: external summary missing canonical_platform: {summary_path}"
    )
if expected_canonical_platform and summary_canonical_platform != expected_canonical_platform:
    raise SystemExit(
        f"error: external summary canonical_platform mismatch expected={expected_canonical_platform} actual={summary_canonical_platform}"
    )
build_recipe = payload.get("identity_build_recipe")
if not isinstance(build_recipe, dict):
    raise SystemExit(
        f"error: external summary missing identity_build_recipe: {summary_path}"
    )
if build_recipe.get("container_platform") != summary_canonical_platform:
    raise SystemExit(
        "error: external summary identity_build_recipe container_platform mismatch summary={} recipe={}".format(
            summary_canonical_platform, build_recipe.get("container_platform")
        )
    )

for local_path in out_dir.glob("*.json"):
    local = json.loads(local_path.read_text())
    if (local.get("purpose"), local.get("baseline_sha256")) != (purpose, digest):
        raise SystemExit("error: mixed imported/local summary baseline purposes")
target_path = out_dir / f"{runner_label}.json"
shutil.copyfile(summary_path, target_path)
print(target_path)
PY

echo "staged wasm summaries: $out_dir"
