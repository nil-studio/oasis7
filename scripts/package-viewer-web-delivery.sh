#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
source "$ROOT_DIR/scripts/viewer-web-dist-contract.sh"
OPTIONAL_PAYLOAD_NAME="pixel_world_bridge_bindgen_bg.wasm"
OPTIONAL_PAYLOAD_PUBLIC_PATH="../viewer-optional-payload/${OPTIONAL_PAYLOAD_NAME}"
WEB_DIST=""
OPTIONAL_PAYLOAD_DIR=""
OUT_FILE=""

usage() {
  cat <<'USAGE'
Usage: ./scripts/package-viewer-web-delivery.sh \
  --web-dist <path> --optional-payload-dir <path> --out-file <path>

Create the final split Viewer Web delivery archive. The primary web-dist
remains payload-free; the archive serves the web-dist contents from its root
beside viewer-optional-payload/ so optional-payloads.json resolves directly.
USAGE
}

resolve_abs_path() {
  local raw="$1"
  if [[ "$raw" == /* ]]; then
    printf '%s\n' "$raw"
  else
    printf '%s\n' "$ROOT_DIR/$raw"
  fi
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --web-dist)
      WEB_DIST="${2:-}"
      shift 2
      ;;
    --optional-payload-dir)
      OPTIONAL_PAYLOAD_DIR="${2:-}"
      shift 2
      ;;
    --out-file)
      OUT_FILE="${2:-}"
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

[[ -n "$WEB_DIST" ]] || { echo "error: --web-dist is required" >&2; exit 2; }
[[ -n "$OPTIONAL_PAYLOAD_DIR" ]] || {
  echo "error: --optional-payload-dir is required" >&2
  exit 2
}
[[ -n "$OUT_FILE" ]] || { echo "error: --out-file is required" >&2; exit 2; }

WEB_DIST="$(resolve_abs_path "$WEB_DIST")"
OPTIONAL_PAYLOAD_DIR="$(resolve_abs_path "$OPTIONAL_PAYLOAD_DIR")"
OUT_FILE="$(resolve_abs_path "$OUT_FILE")"
PAYLOAD="$OPTIONAL_PAYLOAD_DIR/$OPTIONAL_PAYLOAD_NAME"

[[ -d "$WEB_DIST" ]] || { echo "error: web dist does not exist: $WEB_DIST" >&2; exit 1; }
[[ -f "$PAYLOAD" ]] || { echo "error: optional payload does not exist: $PAYLOAD" >&2; exit 1; }
[[ ! -e "$WEB_DIST/pixel-world-bridge/webgl2/$OPTIONAL_PAYLOAD_NAME" ]] || {
  echo "error: optional payload must remain outside primary web dist: $WEB_DIST" >&2
  exit 1
}

python3 - "$WEB_DIST/optional-payloads.json" "$OPTIONAL_PAYLOAD_NAME" \
  "$OPTIONAL_PAYLOAD_PUBLIC_PATH" "$PAYLOAD" <<'PY'
from __future__ import annotations

import hashlib
import json
import sys
from pathlib import Path

manifest_path = Path(sys.argv[1])
payload_name = sys.argv[2]
expected_path = sys.argv[3]
payload_path = Path(sys.argv[4])
manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
payload = manifest.get(payload_name)
if not isinstance(payload, dict):
    raise SystemExit(f"missing optional payload manifest entry: {payload_name}")
if payload.get("available") is True and payload.get("path") != expected_path:
    raise SystemExit(f"optional payload path mismatch: {payload.get('path')!r} != {expected_path!r}")
if payload.get("available") is not True and payload.get("reason") != "separate_artifact":
    raise SystemExit(f"optional payload manifest is not a publishable split artifact: {payload}")
content = payload_path.read_bytes()
if payload.get("available") is True and payload.get("sha256") != hashlib.sha256(content).hexdigest():
    raise SystemExit("optional payload manifest sha256 mismatch")
if payload.get("available") is True and payload.get("size_bytes") != len(content):
    raise SystemExit("optional payload manifest size mismatch")
if payload.get("available") is True and payload.get("delivery") != "separate_artifact":
    raise SystemExit("optional payload manifest delivery must be separate_artifact")
if payload.get("available") is True and payload.get("provenance") != "viewer-web-build":
    raise SystemExit("optional payload manifest provenance must be viewer-web-build")
PY

stage_dir="$(mktemp -d)"
cleanup() {
  rm -rf "$stage_dir"
}
trap cleanup EXIT

mkdir -p "$stage_dir/viewer-optional-payload" "$(dirname "$OUT_FILE")"
python3 - "$WEB_DIST" "$PAYLOAD" "$stage_dir" <<'PY'
from __future__ import annotations

import os
import shutil
import sys
from pathlib import Path

web_dist = Path(sys.argv[1])
payload_path = Path(sys.argv[2])
stage_dir = Path(sys.argv[3])


def copy_regular_tree(source: Path, destination: Path) -> None:
    for candidate in sorted(source.rglob("*")):
        relative = candidate.relative_to(source)
        if any(part.startswith("._") for part in relative.parts):
            continue
        target = destination / relative
        if candidate.is_symlink():
            raise SystemExit(f"viewer delivery does not permit symlink members: {relative}")
        if candidate.is_dir():
            target.mkdir(parents=True, exist_ok=True)
            continue
        if not candidate.is_file():
            raise SystemExit(f"viewer delivery encountered unsupported member: {relative}")
        target.parent.mkdir(parents=True, exist_ok=True)
        with candidate.open("rb") as source_file, target.open("wb") as target_file:
            shutil.copyfileobj(source_file, target_file)
        os.chmod(target, 0o644)


copy_regular_tree(web_dist, stage_dir)

PY
python3 - "$stage_dir/optional-payloads.json" "$OPTIONAL_PAYLOAD_NAME" "$PAYLOAD" <<'PY'
import gzip
import hashlib
import json
import sys
from pathlib import Path

manifest_path = Path(sys.argv[1])
payload_name = sys.argv[2]
payload_path = Path(sys.argv[3])
manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
content = payload_path.read_bytes()
digest = hashlib.sha256(content).hexdigest()
public_path = f"viewer-optional-payload/{digest}.wasm"
published = manifest_path.parent / public_path
published.parent.mkdir(parents=True, exist_ok=True)
published.write_bytes(content)
compressed = gzip.compress(content, compresslevel=9, mtime=0)
Path(str(published) + ".gz").write_bytes(compressed)
manifest[payload_name] = {
    "available": True,
    "path": public_path,
    "sha256": digest,
    "size_bytes": len(content),
    "delivery": "separate_artifact",
    "provenance": "viewer-web-build",
    "encodings": {"gzip": {"path": public_path + ".gz", "size_bytes": len(compressed)}},
    "render_input_schema_version": 1,
}
# Only static code/styles are precompressed. HTML can contain injected session
# bootstrap data and must always go through its existing response pipeline.
for asset in manifest_path.parent.rglob("*"):
    if asset.is_file() and asset.suffix in {".js", ".css"}:
        Path(str(asset) + ".gz").write_bytes(gzip.compress(asset.read_bytes(), compresslevel=9, mtime=0))
manifest_path.write_text(json.dumps(manifest, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
PY
rm -f "$OUT_FILE"
viewer_web_dist_write_delivery_manifest "$ROOT_DIR" "$stage_dir"
python3 - "$stage_dir" "$OUT_FILE" <<'PY'
from __future__ import annotations

import gzip
import sys
import tarfile
from pathlib import Path

stage_dir = Path(sys.argv[1])
out_file = Path(sys.argv[2])

with out_file.open("wb") as raw_file:
    # Keep the gzip header stable across macOS/Linux and across repeated builds.
    with gzip.GzipFile(
        fileobj=raw_file,
        mode="wb",
        filename="",
        mtime=0,
        compresslevel=9,
    ) as gzip_file:
        with tarfile.open(fileobj=gzip_file, mode="w", format=tarfile.USTAR_FORMAT) as archive:
            members = sorted(
                candidate
                for candidate in stage_dir.rglob("*")
                if candidate.is_file()
            )
            for candidate in members:
                relative = candidate.relative_to(stage_dir).as_posix()
                if relative.startswith("._") or "/._" in relative:
                    continue
                if len(relative.encode("utf-8")) > 100:
                    raise SystemExit(f"viewer delivery member path exceeds USTAR limit: {relative}")
                info = tarfile.TarInfo(relative)
                info.size = candidate.stat().st_size
                info.mode = 0o644
                info.mtime = 0
                info.uid = 0
                info.gid = 0
                info.uname = ""
                info.gname = ""
                with candidate.open("rb") as member_file:
                    archive.addfile(info, member_file)
PY

echo "Created Viewer Web delivery archive: $OUT_FILE"
