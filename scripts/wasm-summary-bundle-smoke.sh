#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

tmp_root="$(mktemp -d "${TMPDIR:-/tmp}/wasm-summary-bundle-smoke.XXXXXX")"
cleanup() {
  rm -rf "$tmp_root"
}
trap cleanup EXIT

write_summary() {
  local target="$1"
  local module_set="$2"
  local runner="$3"
  local host_platform="$4"
  local canonical_platform="$5"

  python3 - "$target" "$module_set" "$runner" "$host_platform" "$canonical_platform" <<'PY'
import json
import pathlib
import sys

target = pathlib.Path(sys.argv[1])
module_set, runner, host_platform, canonical_platform = sys.argv[2:]

summary = {
    "schema_version": 1,
    "module_set": module_set,
    "runner": runner,
    "current_platform": canonical_platform,
    "host_platform": host_platform,
    "canonical_platform": canonical_platform,
    "generated_at_utc": "2026-03-31T00:00:00Z",
    "module_count": 1,
    "module_hashes": {f"{module_set}.module": "wasm-hash"},
    "manifest_platform_hashes": {f"{module_set}.module": "wasm-hash"},
    "identity_hashes": {f"{module_set}.module": "identity-hash"},
    "identity_build_recipe": {
        "builder_image_digest": "sha256:builder",
        "container_platform": canonical_platform,
        "canonicalizer_version": "v1",
    },
    "receipt_evidence": {
        f"{module_set}.module": {
            "source_input_rule_version": 2,
            "source_inputs": [{"kind": "source-file", "path": "module/src/lib.rs", "sha256": "0" * 64}],
            "source_hash": "source-hash",
            "build_manifest_hash": "build-manifest-hash",
            "wasm_hash": "wasm-hash",
            "builder_image_digest": "sha256:builder",
            "container_platform": canonical_platform,
            "canonicalizer_version": "v1",
        }
    },
}
target.parent.mkdir(parents=True, exist_ok=True)
target.write_text(json.dumps(summary, indent=2, sort_keys=True) + "\n")
PY
}

source_root="$tmp_root/source"
for module_set in m1 m4 m5; do
  write_summary "$source_root/$module_set/darwin-arm64.json" "$module_set" "darwin-arm64" "darwin-arm64" "linux-x86_64"
done

bundle_dir="$tmp_root/bundle"
bundle_archive="$tmp_root/darwin-arm64-bundle.tar.gz"
./scripts/package-wasm-summary-bundle.sh \
  --out-dir "$bundle_dir" \
  --archive "$bundle_archive" \
  --runner-label "darwin-arm64" \
  --source-summary-root "$source_root"

for module_set in m1 m4 m5; do
  local_root="$tmp_root/local/$module_set"
  write_summary "$local_root/linux-x86_64.json" "$module_set" "linux-x86_64" "linux-x86_64" "linux-x86_64"

  staged_dir="$tmp_root/staged/$module_set"
  ./scripts/stage-wasm-summary-imports.sh \
    --module-set "$module_set" \
    --local-summary-dir "$local_root" \
    --out-dir "$staged_dir" \
    --external-summary-bundle "$bundle_archive"

  test -f "$staged_dir/linux-x86_64.json"
  test -f "$staged_dir/darwin-arm64.json"
  python3 ./scripts/ci-verify-m1-wasm-summaries.py --module-set "$module_set" --summary-dir "$staged_dir"
done

python3 - "$source_root/m1/darwin-arm64.json" <<'PY'
import contextlib
import copy
import importlib.util
import io
import json
import pathlib
import sys

path = pathlib.Path(sys.argv[1])
spec = importlib.util.spec_from_file_location("verify_wasm", "scripts/ci-verify-m1-wasm-summaries.py")
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
original = json.loads(path.read_text())
for mutation in ("legacy", "unknown", "missing-details", "invalid-path", "invalid-digest"):
    payload = copy.deepcopy(original)
    record = payload["receipt_evidence"]["m1.module"]
    if mutation == "legacy":
        del record["source_input_rule_version"]
    elif mutation == "unknown":
        record["source_input_rule_version"] = 99
    elif mutation == "missing-details":
        record["source_inputs"] = []
    elif mutation == "invalid-path":
        record["source_inputs"][0]["path"] = "../escape"
    else:
        record["source_inputs"][0]["sha256"] = "not-a-digest"
    try:
        with contextlib.redirect_stderr(io.StringIO()):
            module.verify_summary_shape(path, payload, "m1")
    except SystemExit as error:
        assert error.code == 1
    else:
        raise AssertionError(f"accepted {mutation} source-input evidence")
PY

bad_source_root="$tmp_root/bad-source"
for module_set in m1 m4 m5; do
  write_summary "$bad_source_root/$module_set/darwin-arm64.json" "$module_set" "darwin-arm64" "linux-x86_64" "linux-x86_64"
done
bad_bundle_dir="$tmp_root/bad-bundle"
./scripts/package-wasm-summary-bundle.sh \
  --out-dir "$bad_bundle_dir" \
  --runner-label "darwin-arm64" \
  --source-summary-root "$bad_source_root"

for module_set in m1 m4 m5; do
  local_root="$tmp_root/local/$module_set"
  if ./scripts/stage-wasm-summary-imports.sh \
    --module-set "$module_set" \
    --local-summary-dir "$local_root" \
    --out-dir "$tmp_root/bad-staged/$module_set" \
    --external-summary-bundle "$bad_bundle_dir" \
    >"$tmp_root/bad-stage-$module_set.stdout" 2>"$tmp_root/bad-stage-$module_set.stderr"; then
    echo "error: expected host_platform mismatch to fail staging for $module_set" >&2
    exit 1
  fi

  rg -n "host_platform mismatch" "$tmp_root/bad-stage-$module_set.stderr" >/dev/null
done

echo "wasm summary bundle smoke: OK"
