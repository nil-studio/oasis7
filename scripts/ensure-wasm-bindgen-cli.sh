#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)"

usage() {
  cat <<'USAGE'
Usage: ./scripts/ensure-wasm-bindgen-cli.sh [--ensure-cache] [--print-bin]

Provision the pinned `wasm-bindgen` CLI version required by the viewer build.

Options:
  --ensure-cache  Prepare the version directory, ignoring external executables.
  --print-bin   Print the resolved executable path.
  -h, --help    Show this help.
USAGE
}

PRINT_BIN=0
ENSURE_CACHE=0
while [[ $# -gt 0 ]]; do
  case "$1" in
    --ensure-cache)
      ENSURE_CACHE=1
      shift
      ;;
    --print-bin)
      PRINT_BIN=1
      shift
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

source "$SCRIPT_DIR/wasm-bindgen-cli-common.sh"

cache_root="$WASM_BINDGEN_CACHE_ROOT"
cached_cli="$cache_root/bin/wasm-bindgen"

cli_version_matches() {
  local candidate="${1:-}"
  [[ -x "$candidate" ]] || return 1
  local version_output
  version_output="$("$candidate" --version 2>/dev/null)" || return 1
  [[ "$version_output" == "wasm-bindgen $WASM_BINDGEN_LOCK_VERSION" ]]
}

install_cli() {
  mkdir -p "$cache_root"
  local force_arg=""
  if [[ "$ENSURE_CACHE" == "1" ]]; then
    force_arg="--force"
  fi
  env -u RUSTC_WRAPPER cargo install ${force_arg:+"$force_arg"} \
    --locked \
    --root "$cache_root" \
    --version "$WASM_BINDGEN_LOCK_VERSION" \
    wasm-bindgen-cli >&2
}

resolved_cli=""
if cli_version_matches "$cached_cli"; then
  resolved_cli="$cached_cli"
elif [[ "$ENSURE_CACHE" == "0" ]] && cli_version_matches "${WASM_BINDGEN_BIN:-}"; then
  resolved_cli="${WASM_BINDGEN_BIN}"
else
  system_cli="$(command -v wasm-bindgen 2>/dev/null || true)"
  if [[ "$ENSURE_CACHE" == "0" ]] && cli_version_matches "$system_cli"; then
    resolved_cli="$system_cli"
  else
    install_cli
    if ! cli_version_matches "$cached_cli"; then
      echo "error: failed to provision wasm-bindgen $WASM_BINDGEN_LOCK_VERSION under $cache_root" >&2
      exit 1
    fi
    resolved_cli="$cached_cli"
  fi
fi

if [[ "$PRINT_BIN" == "1" ]]; then
  printf '%s\n' "$resolved_cli"
fi
