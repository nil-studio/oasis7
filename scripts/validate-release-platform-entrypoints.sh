#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PLATFORM=""
BUNDLE_DIR=""
OPS_BUNDLE_DIR=""

usage() {
  cat <<'USAGE'
Usage: ./scripts/validate-release-platform-entrypoints.sh [options]

Validate that a prepared release bundle exposes platform-native launch entrypoints.

Options:
  --platform <id>      required: linux-x64 | macos-x64 | macos-arm64 | windows-x64
  --bundle-dir <path>  required: prepared bundle directory
  --ops-bundle-dir <path>
                       optional: separately staged operator tools bundle
  -h, --help           show this help
USAGE
}

resolve_abs_path() {
  local input="$1"
  if [[ "$input" == /* ]]; then
    printf '%s\n' "$input"
  else
    printf '%s\n' "$ROOT_DIR/$input"
  fi
}

require_path() {
  local path="$1"
  local expected_type="${2:-exists}"

  case "$expected_type" in
    exists)
      [[ -e "$path" ]] || {
        echo "error: required release bundle path missing: $path" >&2
        exit 1
      }
      ;;
    dir)
      [[ -d "$path" ]] || {
        echo "error: required release bundle directory missing or invalid: $path" >&2
        exit 1
      }
      ;;
    file)
      [[ -f "$path" ]] || {
        echo "error: required release bundle file missing or invalid: $path" >&2
        exit 1
      }
      ;;
    executable)
      [[ -x "$path" ]] || {
        echo "error: required release bundle executable missing or not executable: $path" >&2
        exit 1
      }
      ;;
    *)
      echo "error: invalid require_path expected type: $expected_type" >&2
      exit 1
      ;;
  esac
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --platform)
      PLATFORM="${2:-}"
      shift 2
      ;;
    --bundle-dir)
      BUNDLE_DIR="${2:-}"
      shift 2
      ;;
    --ops-bundle-dir)
      OPS_BUNDLE_DIR="${2:-}"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "error: unknown option: $1" >&2
      usage
      exit 1
      ;;
  esac
done

case "$PLATFORM" in
  linux-x64|macos-x64|macos-arm64|windows-x64) ;;
  *)
    echo "error: --platform must be one of linux-x64|macos-x64|macos-arm64|windows-x64" >&2
    exit 1
    ;;
esac

[[ -n "$BUNDLE_DIR" ]] || { echo "error: --bundle-dir is required" >&2; exit 1; }
BUNDLE_DIR="$(resolve_abs_path "$BUNDLE_DIR")"
[[ -d "$BUNDLE_DIR" ]] || { echo "error: bundle dir does not exist: $BUNDLE_DIR" >&2; exit 1; }
if [[ -n "$OPS_BUNDLE_DIR" ]]; then
  OPS_BUNDLE_DIR="$(resolve_abs_path "$OPS_BUNDLE_DIR")"
  [[ -d "$OPS_BUNDLE_DIR" ]] || {
    echo "error: ops bundle dir does not exist: $OPS_BUNDLE_DIR" >&2
    exit 1
  }
fi

require_path "$BUNDLE_DIR/bin" dir
require_path "$BUNDLE_DIR/web/index.html" file
require_path "$BUNDLE_DIR/web/viewer.js" file
require_path "$BUNDLE_DIR/web/software_safe.js" file
require_path "$BUNDLE_DIR/web-launcher/index.html" file
require_path "$BUNDLE_DIR/README.txt" file
require_path "$BUNDLE_DIR/.oasis7-bundle-manifest.json" file

if ! grep -Fq 'src="./viewer.js"' "$BUNDLE_DIR/web/index.html"; then
  echo "error: release web entrypoint no longer references canonical viewer.js" >&2
  exit 1
fi
if ! grep -Fq 'import "./viewer.js";' "$BUNDLE_DIR/web/software_safe.js"; then
  echo "error: release web compat entrypoint no longer aliases viewer.js" >&2
  exit 1
fi
if cmp -s "$BUNDLE_DIR/web/viewer.js" "$BUNDLE_DIR/web/software_safe.js"; then
  echo "error: release web canonical and compat bundles unexpectedly match byte-for-byte" >&2
  exit 1
fi

case "$PLATFORM" in
  linux-x64)
    require_path "$BUNDLE_DIR/run-client.sh" executable
    require_path "$BUNDLE_DIR/run-web-launcher.sh" executable
    require_path "$BUNDLE_DIR/run-game.sh" executable
    require_path "$BUNDLE_DIR/run-chain-runtime.sh" executable
    require_path "$BUNDLE_DIR/bin/oasis7_client_launcher" executable
    require_path "$BUNDLE_DIR/bin/oasis7_game_launcher" executable
    require_path "$BUNDLE_DIR/bin/oasis7_web_launcher" executable
    require_path "$BUNDLE_DIR/bin/oasis7_viewer_live" executable
    require_path "$BUNDLE_DIR/bin/oasis7_chain_runtime" executable
    ;;
  macos-x64|macos-arm64)
    require_path "$BUNDLE_DIR/run-client.sh" executable
    require_path "$BUNDLE_DIR/run-web-launcher.sh" executable
    require_path "$BUNDLE_DIR/run-game.sh" executable
    require_path "$BUNDLE_DIR/run-chain-runtime.sh" executable
    require_path "$BUNDLE_DIR/bin/oasis7_client_launcher" executable
    require_path "$BUNDLE_DIR/bin/oasis7_game_launcher" executable
    require_path "$BUNDLE_DIR/bin/oasis7_web_launcher" executable
    require_path "$BUNDLE_DIR/bin/oasis7_viewer_live" executable
    require_path "$BUNDLE_DIR/bin/oasis7_chain_runtime" executable
    require_path "$BUNDLE_DIR/oasis7 Client Launcher.app/Contents/Info.plist" file
    require_path "$BUNDLE_DIR/oasis7 Client Launcher.app/Contents/MacOS/oasis7-client-launcher" executable
    if [[ "$PLATFORM" == "macos-arm64" ]]; then
      if command -v file >/dev/null 2>&1; then
        runtime_architecture="$(file -b "$BUNDLE_DIR/bin/oasis7_chain_runtime")"
      else
        runtime_architecture="$(python3 - "$BUNDLE_DIR/bin/oasis7_chain_runtime" <<'PY'
import os
import struct
import sys

ARM64 = 0x0100000C
HEADER_SIZE = 32
TABLE_READ_LIMIT = 4096
FAT_FORMATS = {
    0xCAFEBABE: struct.Struct(">5I"),
    0xCAFEBABF: struct.Struct(">IIQQII"),
}


def is_arm64_header(data, slice_size):
    if len(data) < HEADER_SIZE or slice_size < HEADER_SIZE:
        return False
    magic, cpu, _, _, command_count, command_bytes, _, _ = struct.unpack_from("<8I", data)
    return (
        magic == 0xFEEDFACF and cpu == ARM64
        and command_count * 8 <= command_bytes <= slice_size - HEADER_SIZE
    )


def architecture(stream):
    file_size = os.fstat(stream.fileno()).st_size
    data = stream.read(TABLE_READ_LIMIT)
    if is_arm64_header(data, file_size):
        return "Mach-O 64-bit arm64"
    if len(data) < 8:
        return "unsupported or incomplete Mach-O header"
    magic, count = struct.unpack_from(">II", data)
    descriptor = FAT_FORMATS.get(magic)
    if descriptor is None:
        return "unsupported or incomplete Mach-O header"
    # Check that the complete table fits in the bounded read before iteration.
    if not 0 < count <= (len(data) - 8) // descriptor.size:
        return "invalid Mach-O universal architecture table"
    end_of_table = 8 + count * descriptor.size
    has_arm64 = False
    for i in range(count):
        cpu, _, offset, size, *_ = descriptor.unpack_from(data, 8 + i * descriptor.size)
        if offset < end_of_table or not 0 < size <= file_size - offset:
            return "invalid Mach-O universal slice bounds"
        if cpu == ARM64:
            stream.seek(offset)
            if not is_arm64_header(stream.read(min(size, HEADER_SIZE)), size):
                return "invalid Mach-O universal slice header"
            has_arm64 = True
    return "Mach-O universal arm64" if has_arm64 else "unsupported Mach-O universal architecture"


with open(sys.argv[1], "rb") as stream:
    print(architecture(stream))
PY
)"
      fi
      if [[ "$runtime_architecture" != *Mach-O*arm64* ]]; then
        echo "error: macos-arm64 runtime is not Mach-O arm64: $runtime_architecture" >&2
        exit 1
      fi
    fi
    ;;
  windows-x64)
    require_path "$BUNDLE_DIR/bin/oasis7_client_launcher.exe" file
    require_path "$BUNDLE_DIR/bin/oasis7_game_launcher.exe" file
    require_path "$BUNDLE_DIR/bin/oasis7_web_launcher.exe" file
    require_path "$BUNDLE_DIR/bin/oasis7_viewer_live.exe" file
    require_path "$BUNDLE_DIR/bin/oasis7_chain_runtime.exe" file
    require_path "$BUNDLE_DIR/run-client.cmd" file
    require_path "$BUNDLE_DIR/run-web-launcher.cmd" file
    require_path "$BUNDLE_DIR/run-game.cmd" file
    require_path "$BUNDLE_DIR/run-chain-runtime.cmd" file
    ;;
esac

if [[ -n "$OPS_BUNDLE_DIR" ]]; then
  require_path "$OPS_BUNDLE_DIR/bin" dir
  require_path "$OPS_BUNDLE_DIR/.oasis7-ops-tools-manifest.json" file
  require_path "$OPS_BUNDLE_DIR/SHA256SUMS" file
  case "$PLATFORM" in
    windows-x64)
      require_path "$OPS_BUNDLE_DIR/bin/oasis7_world_repair_rebuild.exe" file
      require_path "$OPS_BUNDLE_DIR/bin/oasis7_governance_registry_import.exe" file
      require_path "$OPS_BUNDLE_DIR/bin/oasis7_governance_registry_audit.exe" file
      ;;
    *)
      require_path "$OPS_BUNDLE_DIR/bin/oasis7_world_repair_rebuild" executable
      require_path "$OPS_BUNDLE_DIR/bin/oasis7_governance_registry_import" executable
      require_path "$OPS_BUNDLE_DIR/bin/oasis7_governance_registry_audit" executable
      if [[ "$PLATFORM" == "linux-x64" ]]; then
        require_path "$OPS_BUNDLE_DIR/bin/service-readback" executable
      fi
      ;;
  esac
fi

echo "Release bundle entrypoints validated: $BUNDLE_DIR ($PLATFORM)"
