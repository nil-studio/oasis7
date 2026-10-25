#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
source "$ROOT_DIR/scripts/viewer-web-dist-contract.sh"
VIEWER_ROOT="$ROOT_DIR/crates/oasis7_viewer"
DIST_DIR=""
OPTIONAL_PAYLOAD_DIR=""
OPTIONAL_PAYLOAD_PUBLIC_PATH=""
OPTIONAL_PAYLOAD_NAME="pixel_world_bridge_bindgen_bg.wasm"

usage() {
  cat <<'USAGE'
Usage: ./scripts/copy-viewer-web-dist.sh --dist-dir <path> [--viewer-root <path>] \
  [--optional-payload-dir <path>] [--optional-payload-public-path <path>]

Copy the canonical viewer web dist into a prepared output directory.

When --optional-payload-dir is provided, the WebGL2 bridge WASM is staged in
that directory instead of the primary viewer dist. The primary dist receives
optional-payloads.json with available=false because the separately uploaded
payload is not part of the player bundle. Pass
--optional-payload-public-path when a final delivery archive publishes that
separate payload at a resolvable relative URL; the manifest then records
available=true plus integrity metadata. A missing source is reported as the
deterministic source_missing result and never falls back to staged bytes.
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
    --dist-dir)
      DIST_DIR="${2:-}"
      shift 2
      ;;
    --viewer-root)
      VIEWER_ROOT="${2:-}"
      shift 2
      ;;
    --optional-payload-dir)
      OPTIONAL_PAYLOAD_DIR="${2:-}"
      shift 2
      ;;
    --optional-payload-public-path)
      OPTIONAL_PAYLOAD_PUBLIC_PATH="${2:-}"
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

[[ -n "$DIST_DIR" ]] || { echo "error: --dist-dir is required" >&2; exit 2; }
VIEWER_ROOT="$(resolve_abs_path "$VIEWER_ROOT")"
DIST_DIR="$(resolve_abs_path "$DIST_DIR")"
if [[ -n "$OPTIONAL_PAYLOAD_DIR" ]]; then
  OPTIONAL_PAYLOAD_DIR="$(resolve_abs_path "$OPTIONAL_PAYLOAD_DIR")"
fi
if [[ -n "$OPTIONAL_PAYLOAD_PUBLIC_PATH" && -z "$OPTIONAL_PAYLOAD_DIR" ]]; then
  echo "error: --optional-payload-public-path requires --optional-payload-dir" >&2
  exit 2
fi
if [[ -n "$OPTIONAL_PAYLOAD_PUBLIC_PATH" \
  && ("$OPTIONAL_PAYLOAD_PUBLIC_PATH" == /* || "$OPTIONAL_PAYLOAD_PUBLIC_PATH" == *"://"*) ]]; then
  echo "error: --optional-payload-public-path must be a relative URL path" >&2
  exit 2
fi

require_file() {
  local path="$1"
  [[ -f "$path" ]] || {
    echo "error: required viewer dist input missing: $path" >&2
    exit 1
  }
}

viewer_html="$VIEWER_ROOT/viewer.html"
viewer_js="$VIEWER_ROOT/viewer.js"
compat_html="$VIEWER_ROOT/dist/software_safe.html"
compat_js="$VIEWER_ROOT/dist/software_safe.js"
compat_claim_evidence="$VIEWER_ROOT/dist/software_safe_first_agent_claim_evidence.html"
browser_game_client_dir="$VIEWER_ROOT/dist/browser-game-client"
require_file "$browser_game_client_dir/oasis7_browser_game_client.js"
require_file "$browser_game_client_dir/oasis7_browser_game_client_bg.wasm"
pixel_world_bridge_dir="$VIEWER_ROOT/dist/pixel-world-bridge"
pixel_world_bridge_js="$pixel_world_bridge_dir/pixel_world_bridge.js"
pixel_world_webgl2_bridge_js="$pixel_world_bridge_dir/webgl2/pixel_world_bridge.js"
pixel_world_webgl2_bindgen_js="$pixel_world_bridge_dir/webgl2/pixel_world_bridge_bindgen.js"
pixel_world_webgl2_wasm="$pixel_world_bridge_dir/webgl2/pixel_world_bridge_bindgen_bg.wasm"

require_file "$viewer_html"
require_file "$viewer_js"
require_file "$compat_html"
require_file "$compat_js"
require_file "$compat_claim_evidence"
require_file "$pixel_world_bridge_js"
require_file "$pixel_world_webgl2_bridge_js"
require_file "$pixel_world_webgl2_bindgen_js"
if [[ -z "$OPTIONAL_PAYLOAD_DIR" ]]; then
  require_file "$pixel_world_webgl2_wasm"
fi
while read -r source_rel _; do
  require_file "$VIEWER_ROOT/$source_rel"
done < <(viewer_web_dist_contract_pairs)

if ! grep -Fq 'src="./viewer.js"' "$viewer_html"; then
  echo "error: viewer.html no longer points at canonical viewer.js" >&2
  exit 1
fi
if ! grep -Fq 'import "./viewer.js";' "$compat_js"; then
  echo "error: software_safe.js is no longer a compat alias to viewer.js" >&2
  exit 1
fi
if cmp -s "$viewer_js" "$compat_js"; then
  echo "error: viewer.js and software_safe.js unexpectedly contain identical payloads" >&2
  exit 1
fi
if ! cmp -s "$viewer_html" "$compat_html"; then
  echo "error: dist/software_safe.html must be generated from canonical viewer.html" >&2
  exit 1
fi
if ! grep -Fq 'src="./software_safe.html?test_api=1&connect=0"' "$compat_claim_evidence" \
  || ! grep -Fq 'id="software-safe-frame"' "$compat_claim_evidence" \
  || ! grep -Fq 'eligible_balance_after:' "$compat_claim_evidence" \
  || ! grep -Fq 'upkeep_runway_epochs:' "$compat_claim_evidence"; then
  echo "error: dist/software_safe_first_agent_claim_evidence.html is missing its compatibility route or current claim fields" >&2
  exit 1
fi

mkdir -p "$DIST_DIR"
while read -r source_rel dist_rel; do
  source_path="$VIEWER_ROOT/$source_rel"
  dist_path="$DIST_DIR/$dist_rel"
  if [[ "$source_path" != "$dist_path" ]]; then
    cp "$source_path" "$dist_path"
  fi
done < <(viewer_web_dist_contract_pairs)

if ! cmp -s "$DIST_DIR/viewer.html" "$DIST_DIR/software_safe.html"; then
  echo "error: generated dist/software_safe.html must match canonical dist/viewer.html" >&2
  exit 1
fi

if [[ -d "$pixel_world_bridge_dir" ]]; then
  if [[ "$pixel_world_bridge_dir" != "$DIST_DIR/pixel-world-bridge" ]]; then
    rm -rf "$DIST_DIR/pixel-world-bridge"
    cp -R "$pixel_world_bridge_dir" "$DIST_DIR/pixel-world-bridge"
  fi
fi

if [[ "$browser_game_client_dir" != "$DIST_DIR/browser-game-client" ]]; then
  rm -rf "$DIST_DIR/browser-game-client"
  cp -R "$browser_game_client_dir" "$DIST_DIR/browser-game-client"
fi

write_optional_payload_manifest() {
  local reason="$1"
  local staged_path="${2:-}"
  python3 - "$DIST_DIR/optional-payloads.json" "$OPTIONAL_PAYLOAD_NAME" "$reason" \
    "$OPTIONAL_PAYLOAD_PUBLIC_PATH" "$staged_path" <<'PY'
from __future__ import annotations

import hashlib
import json
import sys
from pathlib import Path

manifest_path = Path(sys.argv[1])
payload_name = sys.argv[2]
reason = sys.argv[3]
public_path = sys.argv[4]
staged_path = Path(sys.argv[5]) if sys.argv[5] else None
payload = {"available": False, "reason": reason}
if public_path and staged_path is not None:
    payload_bytes = staged_path.read_bytes()
    payload = {
        "available": True,
        "path": public_path,
        "sha256": hashlib.sha256(payload_bytes).hexdigest(),
        "size_bytes": len(payload_bytes),
        "delivery": "separate_artifact",
        "provenance": "viewer-web-build",
    }
manifest_path.write_text(
    json.dumps(
        {payload_name: payload},
        ensure_ascii=False,
        indent=2,
    )
    + "\n",
    encoding="utf-8",
)
PY
}

if [[ -n "$OPTIONAL_PAYLOAD_DIR" ]]; then
  mkdir -p "$OPTIONAL_PAYLOAD_DIR"
  staged_optional_payload="$OPTIONAL_PAYLOAD_DIR/$OPTIONAL_PAYLOAD_NAME"

  if [[ -f "$pixel_world_webgl2_wasm" ]]; then
    cp "$pixel_world_webgl2_wasm" "$staged_optional_payload"
    # The player dist deliberately excludes the separately uploaded payload.
    # Keep the canonical source dist intact when this helper is called
    # in-place by build-viewer-software-safe.sh.
    if [[ "$DIST_DIR" != "$VIEWER_ROOT/dist" ]]; then
      rm -f "$DIST_DIR/pixel-world-bridge/webgl2/$OPTIONAL_PAYLOAD_NAME"
    fi
    write_optional_payload_manifest "separate_artifact" "$staged_optional_payload"
  else
    # Never trust a staged file from an earlier build. A missing generated
    # source invalidates the optional artifact and must be visible to the
    # player instead of being masked by stale bytes.
    rm -f "$staged_optional_payload"
    if [[ "$DIST_DIR" != "$VIEWER_ROOT/dist" ]]; then
      rm -f "$DIST_DIR/pixel-world-bridge/webgl2/$OPTIONAL_PAYLOAD_NAME"
    fi
    write_optional_payload_manifest "source_missing"
  fi
else
  # A regular player/developer copy keeps the adjacent WASM and must not
  # carry split-mode metadata from a reused output directory.
  rm -f "$DIST_DIR/optional-payloads.json"
fi

viewer_web_dist_write_manifest "$ROOT_DIR" "$DIST_DIR"
