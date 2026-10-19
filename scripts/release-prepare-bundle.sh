#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PLATFORM=""
TARGET_TRIPLE="native"
WEB_DIST=""
WEB_LAUNCHER_DIST=""
OUT_DIR=""
PROFILE="packaging"

usage() {
  cat <<'USAGE'
Usage: ./scripts/release-prepare-bundle.sh [options]

Prepare deterministic launcher bundle directory for CI release packaging.

The player bundle contains only launch/runtime binaries and explicitly excludes
oasis7_world_repair_rebuild, oasis7_governance_registry_import, and
oasis7_governance_registry_audit. Operator repair and governance tools are emitted into a separate checksummed ops-tools archive so
they remain available for upgrade, rollback, and restore procedures without
inflating the normal player install.

Options:
  --platform <id>        required: linux-x64 | macos-x64 | macos-arm64 | windows-x64
  --target-triple <id>   optional rust target triple (default: native)
  --web-dist <path>      required: prebuilt viewer web dist directory
  --web-launcher-dist <path>
                         required: prebuilt launcher web dist directory
  --out-dir <path>       required: output root for prepared bundle directory
  --profile <name>       cargo profile: packaging|dev (default: packaging)
  -h, --help             show this help
USAGE
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --platform)
      PLATFORM="${2:-}"
      shift 2
      ;;
    --target-triple)
      TARGET_TRIPLE="${2:-}"
      shift 2
      ;;
    --web-dist)
      WEB_DIST="${2:-}"
      shift 2
      ;;
    --web-launcher-dist)
      WEB_LAUNCHER_DIST="${2:-}"
      shift 2
      ;;
    --out-dir)
      OUT_DIR="${2:-}"
      shift 2
      ;;
    --profile)
      PROFILE="${2:-}"
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

case "${PLATFORM}" in
  linux-x64|macos-x64|macos-arm64|windows-x64) ;;
  *)
    echo "error: --platform must be one of linux-x64|macos-x64|macos-arm64|windows-x64" >&2
    exit 1
    ;;
esac

if [[ -z "${WEB_DIST}" ]]; then
  echo "error: --web-dist is required" >&2
  exit 1
fi
if [[ -z "${WEB_LAUNCHER_DIST}" ]]; then
  echo "error: --web-launcher-dist is required" >&2
  exit 1
fi
if [[ -z "${OUT_DIR}" ]]; then
  echo "error: --out-dir is required" >&2
  exit 1
fi
if [[ -z "${TARGET_TRIPLE}" ]]; then
  echo "error: --target-triple must not be empty" >&2
  exit 1
fi
if [[ "${PROFILE}" != "packaging" && "${PROFILE}" != "dev" ]]; then
  echo "error: --profile must be packaging or dev" >&2
  exit 1
fi

if [[ "${WEB_DIST}" != /* ]]; then
  WEB_DIST="${ROOT_DIR}/${WEB_DIST}"
fi
if [[ "${WEB_LAUNCHER_DIST}" != /* ]]; then
  WEB_LAUNCHER_DIST="${ROOT_DIR}/${WEB_LAUNCHER_DIST}"
fi
if [[ "${OUT_DIR}" != /* ]]; then
  OUT_DIR="${ROOT_DIR}/${OUT_DIR}"
fi

if [[ ! -d "${WEB_DIST}" ]]; then
  echo "error: web dist path does not exist: ${WEB_DIST}" >&2
  exit 1
fi
if [[ ! -d "${WEB_LAUNCHER_DIST}" ]]; then
  echo "error: web launcher dist path does not exist: ${WEB_LAUNCHER_DIST}" >&2
  exit 1
fi

BUNDLE_DIR="${OUT_DIR}/oasis7-${PLATFORM}"
OPS_BUNDLE_DIR="${OUT_DIR}/oasis7-${PLATFORM}-ops-tools"
OPS_ARCHIVE="${OUT_DIR}/oasis7-${PLATFORM}-ops-tools.tar.gz"
rm -rf "${BUNDLE_DIR}" "${OPS_BUNDLE_DIR}" "${OPS_ARCHIVE}"
mkdir -p "${OUT_DIR}"

"${ROOT_DIR}/scripts/build-game-launcher-bundle.sh" \
  --out-dir "${BUNDLE_DIR}" \
  --profile "${PROFILE}" \
  --target-triple "${TARGET_TRIPLE}" \
  --web-dist "${WEB_DIST}" \
  --web-launcher-dist "${WEB_LAUNCHER_DIST}" \
  --ops-out-dir "${OPS_BUNDLE_DIR}"

[[ -f "${OPS_BUNDLE_DIR}/.oasis7-ops-tools-manifest.json" ]] || {
  echo "error: ops-tools manifest missing: ${OPS_BUNDLE_DIR}" >&2
  exit 1
}
[[ -f "${OPS_BUNDLE_DIR}/SHA256SUMS" ]] || {
  echo "error: ops-tools checksums missing: ${OPS_BUNDLE_DIR}" >&2
  exit 1
}
tar -C "${OUT_DIR}" -czf "${OPS_ARCHIVE}" "$(basename "${OPS_BUNDLE_DIR}")"

echo "Prepared bundle directory: ${BUNDLE_DIR}"
echo "Prepared ops-tools archive: ${OPS_ARCHIVE}"
