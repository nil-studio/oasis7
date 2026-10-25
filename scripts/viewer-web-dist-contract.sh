#!/usr/bin/env bash

viewer_web_dist_contract_pairs() {
  cat <<'EOF'
viewer.html index.html
viewer.html viewer.html
dist/software_safe.html software_safe.html
viewer.js viewer.js
dist/software_safe.js software_safe.js
viewer_terminal_shell.css viewer_terminal_shell.css
viewer_first_agent_claim_evidence.html viewer_first_agent_claim_evidence.html
dist/software_safe_first_agent_claim_evidence.html software_safe_first_agent_claim_evidence.html
favicon.ico favicon.ico
EOF
}

viewer_web_dist_required_files() {
  cat <<'EOF'
index.html
viewer.html
software_safe.html
viewer.js
software_safe.js
viewer_terminal_shell.css
viewer_first_agent_claim_evidence.html
software_safe_first_agent_claim_evidence.html
favicon.ico
pixel-world-bridge/pixel_world_bridge.js
browser-game-client/oasis7_browser_game_client.js
browser-game-client/oasis7_browser_game_client_bg.wasm
EOF
}

viewer_web_dist_manifest_name() {
  printf '%s\n' '.oasis7-viewer-dist-manifest.json'
}

viewer_web_dist_source_metadata_json() {
  local repo_root=$1
  python3 - "$repo_root" <<'PY'
from __future__ import annotations

import hashlib
import json
import sys
from pathlib import Path

repo_root = Path(sys.argv[1]).resolve()
scope = [
    "Cargo.toml",
    "Cargo.lock",
    "crates/oasis7_viewer/viewer.html",
    "crates/oasis7_viewer/viewer.js",
    "crates/oasis7_viewer/viewer_terminal_shell.css",
    "crates/oasis7_viewer/viewer_first_agent_claim_evidence.html",
    "crates/oasis7_viewer/package.json",
    "crates/oasis7_viewer/package-lock.json",
    "crates/oasis7_viewer/vite.software-safe.config.mjs",
    "crates/oasis7_viewer/scripts",
    "crates/oasis7_viewer/software_safe_src",
    "crates/oasis7_viewer/favicon.ico",
    "crates/oasis7_browser_game_client",
    "crates/oasis7_client_api",
    "crates/oasis7_agent_api",
    "crates/oasis7_wasm_abi",
    "crates/pixel_world_bridge/Cargo.toml",
    "crates/pixel_world_bridge/src",
    "crates/oasis7_proto/Cargo.toml",
    "crates/oasis7_proto/src",
    "scripts/copy-viewer-web-dist.sh",
    "scripts/viewer-web-dist-contract.sh",
    "scripts/package-viewer-web-delivery.sh",
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

print(json.dumps(
    {
        "sourceFingerprint": hasher.hexdigest(),
        "sourceFileCount": len(unique_files),
        "sourceLatestPath": latest_rel,
        "sourceLatestMtimeNs": latest_mtime_ns,
    },
    ensure_ascii=False,
))
PY
}

viewer_web_dist_write_manifest() {
  local repo_root=$1
  local dist_dir=$2
  local metadata_json
  metadata_json=$(viewer_web_dist_source_metadata_json "$repo_root")
  python3 - "$metadata_json" "$dist_dir" "$(viewer_web_dist_manifest_name)" <<'PY'
from __future__ import annotations

import json
import sys
from pathlib import Path

source_metadata = json.loads(sys.argv[1])
dist_dir = Path(sys.argv[2]).resolve()
manifest_name = sys.argv[3]
manifest_path = dist_dir / manifest_name

dist_files: dict[str, dict[str, int]] = {}
for candidate in sorted(dist_dir.rglob("*")):
    if not candidate.is_file():
        continue
    if candidate == manifest_path:
        continue
    rel = candidate.relative_to(dist_dir).as_posix()
    dist_files[rel] = {"size": candidate.stat().st_size}

manifest = {
    **source_metadata,
    "distFiles": dist_files,
}
manifest_path.write_text(json.dumps(manifest, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
PY
}

viewer_web_dist_write_delivery_manifest() {
  local repo_root=$1
  local dist_dir=$2
  viewer_web_dist_write_manifest "$repo_root" "$dist_dir"
  python3 - "$dist_dir/$(viewer_web_dist_manifest_name)" <<'PY'
from __future__ import annotations

import json
import sys
from pathlib import Path

manifest_path = Path(sys.argv[1])
manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
# sourceLatestPath/sourceLatestMtimeNs are useful for local freshness checks,
# but they are checkout filesystem metadata and must not enter the published
# deterministic delivery archive. Keep the content fingerprint and file list.
manifest.pop("sourceLatestPath", None)
manifest.pop("sourceLatestMtimeNs", None)
manifest_path.write_text(json.dumps(manifest, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
PY
}

viewer_web_dist_check_freshness() {
  local repo_root=$1
  local dist_dir=$2
  local manifest_path current_json
  manifest_path="$dist_dir/$(viewer_web_dist_manifest_name)"
  if [[ ! -f "$manifest_path" ]]; then
    echo "viewer dist freshness manifest missing: $manifest_path" >&2
    return 1
  fi
  while read -r required_file; do
    if [[ ! -f "$dist_dir/$required_file" ]]; then
      echo "viewer dist required file missing: $dist_dir/$required_file" >&2
      return 1
    fi
  done < <(viewer_web_dist_required_files)
  current_json=$(viewer_web_dist_source_metadata_json "$repo_root")
  python3 - "$manifest_path" "$current_json" <<'PY'
from __future__ import annotations

import json
import sys
from pathlib import Path

manifest_path = Path(sys.argv[1])
manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
current = json.loads(sys.argv[2])
if manifest.get("sourceFingerprint") != current.get("sourceFingerprint"):
    print(
        "viewer dist is stale relative to current workspace: "
        f"dist latest={manifest.get('sourceLatestPath')}, "
        f"current latest={current.get('sourceLatestPath')}",
        file=sys.stderr,
    )
    sys.exit(1)
print("viewer dist freshness ok")
PY
}
