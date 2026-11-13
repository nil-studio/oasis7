#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
source "$ROOT_DIR/scripts/viewer-web-dist-contract.sh"
source "$ROOT_DIR/scripts/viewer-dependency-preflight.sh"
viewer_dist_dir="$ROOT_DIR/crates/oasis7_viewer/dist"
local_output_helper="$ROOT_DIR/crates/oasis7_viewer/scripts/viewer-local-output.mjs"
if [[ "${OASIS7_LOCAL_CHECK:-0}" == 1 ]]; then
  viewer_dist_dir="$(node "$local_output_helper" dist release)"
fi
optional_payload_dir="${OASIS7_VIEWER_OPTIONAL_PAYLOAD_DIR:-}"
viewer_optional_payload_source="$viewer_dist_dir/pixel-world-bridge/webgl2/pixel_world_bridge_bindgen_bg.wasm"

usage() {
  cat <<'USAGE'
Usage: ./scripts/build-viewer-software-safe.sh [--optional-payload-dir <path>]

Build the viewer software-safe dist. When an optional payload directory is
provided, the WebGL2 bridge WASM is staged there instead of the primary dist.
USAGE
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --optional-payload-dir)
      optional_payload_dir="${2:-}"
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

if [[ -n "$optional_payload_dir" && "$optional_payload_dir" != /* ]]; then
  optional_payload_dir="$ROOT_DIR/$optional_payload_dir"
fi

copy_viewer_dist() {
  local args=(--dist-dir "$viewer_dist_dir")
  if [[ "${OASIS7_LOCAL_CHECK:-0}" == 1 ]]; then
    args+=(--generated-dist-dir "$viewer_dist_dir")
  fi
  if [[ -n "$optional_payload_dir" ]]; then
    args+=(--optional-payload-dir "$optional_payload_dir")
  fi
  "$ROOT_DIR/scripts/copy-viewer-web-dist.sh" "${args[@]}" >/dev/null
}

if [[ "${OASIS7_LOCAL_CHECK:-0}" != 1 \
  && "${OASIS7_FORCE_VIEWER_SOFTWARE_SAFE_BUILD:-0}" != "1" \
  && -f "$viewer_optional_payload_source" ]] \
  && viewer_web_dist_check_freshness "$ROOT_DIR" "$viewer_dist_dir" >/dev/null 2>&1; then
  echo "+ viewer software-safe dist is fresh; skipping rebuild"
  copy_viewer_dist
  exit 0
fi

viewer_dependency_preflight "$ROOT_DIR" build

bindgen_args=(--print-bin)
if [[ "${OASIS7_LOCAL_CHECK:-0}" == 1 ]]; then
  [[ -z "$optional_payload_dir" ]] || { echo 'error: local Viewer checks do not publish optional payloads' >&2; exit 2; }
  bindgen_args+=(--check)
fi
viewer_wasm_bindgen_bin="$("$ROOT_DIR/scripts/ensure-wasm-bindgen-cli.sh" "${bindgen_args[@]}")"
echo "+ WASM_BINDGEN_BIN=$viewer_wasm_bindgen_bin npm --prefix crates/oasis7_viewer run build:software-safe"
(
  cd "$ROOT_DIR"
  WASM_BINDGEN_BIN="$viewer_wasm_bindgen_bin" npm --prefix crates/oasis7_viewer run build:software-safe
)

copy_viewer_dist
