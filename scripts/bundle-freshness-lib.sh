#!/usr/bin/env bash

bundle_manifest_filename() {
  printf '%s\n' '.oasis7-bundle-manifest.json'
}

bundle_manifest_path() {
  local bundle_dir=$1
  printf '%s/%s\n' "$bundle_dir" "$(bundle_manifest_filename)"
}

bundle_source_metadata_json() {
  local repo_root=$1
  python3 - "$repo_root" <<'PY'
from __future__ import annotations

import hashlib
import json
import re
import sys
from pathlib import Path

repo_root = Path(sys.argv[1]).resolve()
scope = [
    "Cargo.toml",
    "Cargo.lock",
    "crates/oasis7/Cargo.toml",
    "crates/oasis7_chain_runtime/Cargo.toml",
    "crates/oasis7_chain_runtime/src",
    "crates/oasis7_launcher_backend/Cargo.toml",
    "crates/oasis7_launcher_backend/src",
    "crates/oasis7_hosted_access/Cargo.toml",
    "crates/oasis7_hosted_access/src",
    "crates/oasis7_cas/Cargo.toml",
    "crates/oasis7_cas/src",
    "crates/oasis7_formal_protocol/Cargo.toml",
    "crates/oasis7_formal_protocol/src",
    "crates/oasis7_client_storage/Cargo.toml",
    "crates/oasis7_client_storage/src",
    "crates/oasis7_runtime/Cargo.toml",
    "crates/oasis7_runtime/src",
    "crates/oasis7_world_service/Cargo.toml",
    "crates/oasis7_world_service/src",
    "crates/oasis7_simulator/Cargo.toml",
    "crates/oasis7_simulator/src",
    "crates/oasis7_simulator/scenarios",
    "crates/oasis7_game_client/Cargo.toml",
    "crates/oasis7_game_client/src",
    "crates/oasis7_launcher_config/Cargo.toml",
    "crates/oasis7_launcher_config/src",
    "crates/oasis7_wasm_materializer/Cargo.toml",
    "crates/oasis7_wasm_materializer/src",
    "crates/oasis7_observability/Cargo.toml",
    "crates/oasis7_observability/src",
    "crates/oasis7_viewer_runtime/Cargo.toml",
    "crates/oasis7_viewer_runtime/src",
    "crates/oasis7_ops_tools/Cargo.toml",
    "crates/oasis7_ops_tools/src",
    "crates/oasis7_provider_tools/Cargo.toml",
    "crates/oasis7_provider_tools/src",
    "crates/oasis7/src/viewer",
    "crates/oasis7_launcher_backend/src/bin/oasis7_game_launcher.rs",
    "crates/oasis7_launcher_backend/src/bin/oasis7_game_launcher",
    "crates/oasis7_launcher_backend/src/bin/oasis7_web_launcher.rs",
    "crates/oasis7_launcher_backend/src/bin/oasis7_web_launcher",
    "crates/oasis7_ops_tools/src/bin/oasis7_governance_registry_import.rs",
    "crates/oasis7_ops_tools/src/bin/oasis7_governance_registry_audit.rs",
    "crates/oasis7_proto/Cargo.toml",
    "crates/oasis7_proto/src",
    "crates/oasis7_viewer/viewer.html",
    "crates/oasis7_viewer/viewer.js",
    "crates/oasis7_viewer/viewer_first_agent_claim_evidence.html",
    "crates/oasis7_viewer/package.json",
    "crates/oasis7_viewer/package-lock.json",
    "crates/oasis7_viewer/vite.software-safe.config.mjs",
    "crates/oasis7_viewer/scripts",
    "crates/pixel_world_bridge/Cargo.toml",
    "crates/pixel_world_bridge/src",
    "crates/oasis7_viewer/software_safe_src",
    "crates/oasis7_viewer/favicon.ico",
    "scripts/copy-viewer-web-dist.sh",
    "scripts/viewer-web-dist-contract.sh",
    "scripts/service-readback",
    "crates/oasis7_client_launcher/Cargo.toml",
    "crates/oasis7_client_launcher/Trunk.toml",
    "crates/oasis7_client_launcher/index.html",
    "crates/oasis7_client_launcher/src",
]
files: list[Path] = []
for entry in scope:
    path = repo_root / entry
    if path.is_dir():
        files.extend(sorted(candidate for candidate in path.rglob("*") if candidate.is_file()))
    elif path.is_file():
        files.append(path)

unique_files = sorted({candidate.resolve() for candidate in files})
hasher = hashlib.sha256()
latest_mtime_ns = 0
latest_rel = ""
for candidate in unique_files:
    rel = candidate.relative_to(repo_root).as_posix()
    hasher.update(rel.encode("utf-8"))
    hasher.update(b"\0")
    hasher.update(candidate.read_bytes())
    hasher.update(b"\0")
    stat = candidate.stat()
    if stat.st_mtime_ns >= latest_mtime_ns:
        latest_mtime_ns = stat.st_mtime_ns
        latest_rel = rel

viewer_proto = (repo_root / "crates/oasis7_proto/src/viewer.rs").read_text(encoding="utf-8")
match = re.search(r"pub const VIEWER_PROTOCOL_VERSION: u32 = (\d+);", viewer_proto)
viewer_protocol_version = int(match.group(1)) if match else None

print(json.dumps(
    {
        "sourceFingerprint": hasher.hexdigest(),
        "sourceFileCount": len(unique_files),
        "sourceLatestPath": latest_rel,
        "sourceLatestMtimeNs": latest_mtime_ns,
        "viewerProtocolVersion": viewer_protocol_version,
        "sourceScope": scope,
    },
    ensure_ascii=False,
))
PY
}

bundle_write_manifest() {
  local repo_root=$1
  local bundle_dir=$2
  local build_profile=${3:-}
  local target_triple=${4:-}
  local metadata_json manifest_path
  metadata_json=$(bundle_source_metadata_json "$repo_root")
  manifest_path=$(bundle_manifest_path "$bundle_dir")
  python3 - "$metadata_json" "$bundle_dir" "$manifest_path" "$build_profile" "$target_triple" <<'PY'
from __future__ import annotations

import hashlib
import json
import time
import sys
from pathlib import Path

source_metadata = json.loads(sys.argv[1])
bundle_dir = Path(sys.argv[2]).resolve()
manifest_path = Path(sys.argv[3]).resolve()
build_profile = sys.argv[4].strip()
target_triple = sys.argv[5].strip()


def hash_first(pattern: str) -> tuple[str | None, str | None]:
    matches = sorted(bundle_dir.glob(pattern))
    if not matches:
        return None, None
    candidate = matches[0]
    digest = hashlib.sha256(candidate.read_bytes()).hexdigest()
    return candidate.relative_to(bundle_dir).as_posix(), digest

def hash_exact(rel_path: str) -> tuple[str | None, str | None]:
    candidate = bundle_dir / rel_path
    if not candidate.is_file():
        return None, None
    digest = hashlib.sha256(candidate.read_bytes()).hexdigest()
    return candidate.relative_to(bundle_dir).as_posix(), digest

viewer_index_path, viewer_index_sha256 = hash_exact("web/index.html")
viewer_js_path, viewer_js_sha256 = hash_exact("web/viewer.js")
viewer_wasm_path, viewer_wasm_sha256 = hash_first("web/*.wasm")
pixel_world_runtime_module_path, pixel_world_runtime_module_sha256 = hash_exact(
    "web/pixel-world-bridge/pixel_world_bridge.js"
)
pixel_world_webgl2_runtime_module_path, pixel_world_webgl2_runtime_module_sha256 = hash_exact(
    "web/pixel-world-bridge/webgl2/pixel_world_bridge.js"
)
pixel_world_webgl2_bindgen_js_path, pixel_world_webgl2_bindgen_js_sha256 = hash_exact(
    "web/pixel-world-bridge/webgl2/pixel_world_bridge_bindgen.js"
)
pixel_world_webgl2_bindgen_wasm_path, pixel_world_webgl2_bindgen_wasm_sha256 = hash_exact(
    "web/pixel-world-bridge/webgl2/pixel_world_bridge_bindgen_bg.wasm"
)
launcher_index_path, launcher_index_sha256 = hash_exact("web-launcher/index.html")
launcher_js_path, launcher_js_sha256 = hash_first("web-launcher/*.js")
launcher_wasm_path, launcher_wasm_sha256 = hash_first("web-launcher/*.wasm")

manifest = {
    "bundleSchemaVersion": 1,
    "generatedAtUnixMs": int(time.time() * 1000),
    **source_metadata,
    "assets": {
        "viewerIndexPath": viewer_index_path,
        "viewerIndexSha256": viewer_index_sha256,
        "viewerJsPath": viewer_js_path,
        "viewerJsSha256": viewer_js_sha256,
        "viewerWasmPath": viewer_wasm_path,
        "viewerWasmSha256": viewer_wasm_sha256,
        "pixelWorldRuntimeModulePath": pixel_world_runtime_module_path,
        "pixelWorldRuntimeModuleSha256": pixel_world_runtime_module_sha256,
        "pixelWorldWebgl2RuntimeModulePath": pixel_world_webgl2_runtime_module_path,
        "pixelWorldWebgl2RuntimeModuleSha256": pixel_world_webgl2_runtime_module_sha256,
        "pixelWorldWebgl2BindgenJsPath": pixel_world_webgl2_bindgen_js_path,
        "pixelWorldWebgl2BindgenJsSha256": pixel_world_webgl2_bindgen_js_sha256,
        "pixelWorldWebgl2BindgenWasmPath": pixel_world_webgl2_bindgen_wasm_path,
        "pixelWorldWebgl2BindgenWasmSha256": pixel_world_webgl2_bindgen_wasm_sha256,
        "launcherIndexPath": launcher_index_path,
        "launcherIndexSha256": launcher_index_sha256,
        "launcherJsPath": launcher_js_path,
        "launcherJsSha256": launcher_js_sha256,
        "launcherWasmPath": launcher_wasm_path,
        "launcherWasmSha256": launcher_wasm_sha256,
    },
}
if build_profile:
    manifest["buildProfile"] = build_profile
if target_triple:
    manifest["targetTriple"] = target_triple
manifest_path.write_text(json.dumps(manifest, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
PY
}

bundle_check_freshness() {
  local repo_root=$1
  local bundle_dir=$2
  local requested_profile=${3:-}
  local requested_target_triple=${4:-}
  local manifest_path current_json
  manifest_path=$(bundle_manifest_path "$bundle_dir")
  if [[ ! -f "$manifest_path" ]]; then
    echo "bundle freshness manifest missing: $manifest_path" >&2
    return 1
  fi
  current_json=$(bundle_source_metadata_json "$repo_root")
  python3 - "$manifest_path" "$current_json" "$requested_profile" "$requested_target_triple" <<'PY'
from __future__ import annotations

import hashlib
import json
import sys
from pathlib import Path

manifest_path = Path(sys.argv[1])
bundle_dir = manifest_path.parent
manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
current = json.loads(sys.argv[2])
requested_profile = sys.argv[3].strip()
requested_target_triple = sys.argv[4].strip()
errors: list[str] = []
if manifest.get("sourceFingerprint") != current.get("sourceFingerprint"):
    errors.append(
        "source fingerprint drift "
        f"(bundle latest={manifest.get('sourceLatestPath')}, current latest={current.get('sourceLatestPath')})"
    )
if manifest.get("viewerProtocolVersion") != current.get("viewerProtocolVersion"):
    errors.append(
        "viewer protocol version drift "
        f"(bundle={manifest.get('viewerProtocolVersion')}, current={current.get('viewerProtocolVersion')})"
    )
if requested_profile and manifest.get("buildProfile") != requested_profile:
    errors.append(
        "bundle profile drift "
        f"(bundle={manifest.get('buildProfile')}, requested={requested_profile})"
    )
if requested_target_triple and manifest.get("targetTriple") != requested_target_triple:
    errors.append(
        "bundle target drift "
        f"(bundle={manifest.get('targetTriple')}, requested={requested_target_triple})"
    )
assets = manifest.get("assets") or {}
for path_key, rel_path in sorted(assets.items()):
    if not path_key.endswith("Path") or not rel_path:
        continue
    sha_key = path_key[:-4] + "Sha256"
    expected_sha = assets.get(sha_key)
    if not expected_sha:
        errors.append(f"bundle asset drift: {rel_path} missing manifest {sha_key}")
        continue
    asset_path = bundle_dir / rel_path
    if not asset_path.is_file():
        errors.append(f"bundle asset drift: {rel_path} missing")
        continue
    actual_sha = hashlib.sha256(asset_path.read_bytes()).hexdigest()
    if actual_sha != expected_sha:
        errors.append(
            f"bundle asset drift: {rel_path} sha256 mismatch "
            f"(bundle={expected_sha}, current={actual_sha})"
        )
if errors:
    print("bundle is stale relative to current workspace: " + "; ".join(errors))
    sys.exit(1)
print("bundle freshness ok")
PY
}
