#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage: scripts/newapi-bridge-service/package-newapi-bridge-service.sh [options]

Build and stage the deployable oasis7_newapi_bridge_service package.

Options:
  --profile <release|dev>  Cargo profile to build (default: release)
  --out-dir <path>         Staging directory (default: output/newapi-bridge-service)
  --archive <path>         Archive path (default: output/newapi-bridge-service-<host>.tar.gz)
  -h, --help               Show help
USAGE
}

die() {
  echo "package-newapi-bridge-service: $*" >&2
  exit 1
}

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"

resolve_output_path() {
  local path="$1"
  if [[ -z "$path" ]]; then
    die "path must not be empty"
  fi
  if [[ "$path" == /* ]]; then
    printf '%s\n' "$path"
  else
    printf '%s/%s\n' "$ROOT_DIR" "$path"
  fi
}

package_suffix_for_host() {
  case "$1" in
    x86_64-unknown-linux-gnu)
      printf '%s\n' "linux-x86_64"
      ;;
    aarch64-unknown-linux-gnu)
      printf '%s\n' "linux-aarch64"
      ;;
    aarch64-apple-darwin)
      printf '%s\n' "macos-aarch64"
      ;;
    x86_64-apple-darwin)
      printf '%s\n' "macos-x86_64"
      ;;
    *)
      printf '%s' "$1" | tr -c 'A-Za-z0-9._-' '-'
      ;;
  esac
}

validate_archive_label() {
  local archive_path="$1"
  local archive_name
  archive_name="$(basename "$archive_path")"
  case "$archive_name" in
    *linux-x86_64*|*linux_x86_64*)
      if [[ "$HOST_TRIPLE" != "x86_64-unknown-linux-gnu" ]]; then
        die "archive path implies linux-x86_64, but rustc host is ${HOST_TRIPLE}; run on x86_64 Linux or choose a host-specific archive name"
      fi
      ;;
  esac
}

HOST_TRIPLE="$(rustc -vV | awk '/^host: / { print $2 }')"
HOST_PACKAGE_SUFFIX="$(package_suffix_for_host "$HOST_TRIPLE")"
ARCHIVE_ROOT_NAME="newapi-bridge-service"
PROFILE="release"
OUT_DIR="output/newapi-bridge-service"
ARCHIVE_PATH="output/newapi-bridge-service-${HOST_PACKAGE_SUFFIX}.tar.gz"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --profile)
      PROFILE="${2:-}"
      shift 2
      ;;
    --out-dir)
      OUT_DIR="${2:-}"
      shift 2
      ;;
    --archive)
      ARCHIVE_PATH="${2:-}"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      die "unknown argument: $1"
      ;;
  esac
done

case "$PROFILE" in
  release)
    BINARY_DIR="release"
    ;;
  dev)
    BINARY_DIR="debug"
    ;;
  *)
    die "unsupported profile: $PROFILE"
    ;;
esac

sha256_emit() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$@"
    return
  fi
  shasum -a 256 "$@"
}

resolve_git_ref_name() {
  if [[ -n "${GITHUB_REF_NAME:-}" ]]; then
    printf '%s\n' "$GITHUB_REF_NAME"
    return
  fi
  git branch --show-current
}

resolve_git_ref() {
  if [[ -n "${GITHUB_REF:-}" ]]; then
    printf '%s\n' "$GITHUB_REF"
    return
  fi
  local branch
  branch="$(git branch --show-current)"
  if [[ -n "$branch" ]]; then
    printf 'refs/heads/%s\n' "$branch"
  else
    git rev-parse HEAD
  fi
}

GIT_SHA="${GITHUB_SHA:-$(git rev-parse HEAD)}"
GIT_REF="$(resolve_git_ref)"
GIT_REF_NAME="$(resolve_git_ref_name)"
TMP_DIR=".tmp/newapi-bridge-service"
TMP_DIR_ABS="$(resolve_output_path "$TMP_DIR")"
ARCHIVE_ROOT_DIR="${TMP_DIR_ABS}/archive-root"
OUT_DIR_ABS="$(resolve_output_path "$OUT_DIR")"
ARCHIVE_ABS_PATH="$(resolve_output_path "$ARCHIVE_PATH")"
TARGET_DIR_ABS="$(resolve_output_path "${CARGO_TARGET_DIR:-target}")"
BINARY_PATH="${TARGET_DIR_ABS}/${BINARY_DIR}/oasis7_newapi_bridge_service"

validate_archive_label "$ARCHIVE_ABS_PATH"

mkdir -p "$TMP_DIR_ABS" "$(dirname "$ARCHIVE_ABS_PATH")" "$(dirname "$OUT_DIR_ABS")"

CARGO_CMD=(env -u RUSTC_WRAPPER cargo build)
if [[ "$PROFILE" == "release" ]]; then
  CARGO_CMD+=(--release)
fi
CARGO_CMD+=(-p oasis7_newapi_bridge_service --bin oasis7_newapi_bridge_service)
"${CARGO_CMD[@]}"

if [[ ! -x "$BINARY_PATH" ]]; then
  die "expected bridge binary at ${BINARY_PATH}; check CARGO_TARGET_DIR and build profile"
fi

"$BINARY_PATH" \
  --help > "$TMP_DIR_ABS/HELP.txt"

rm -rf "$OUT_DIR_ABS"
mkdir -p \
  "$OUT_DIR_ABS/scripts/newapi-bridge-service" \
  "$OUT_DIR_ABS/doc/world-runtime/runtime"

install -m 0755 \
  "$BINARY_PATH" \
  "$OUT_DIR_ABS/oasis7_newapi_bridge_service"
install -m 0755 \
  scripts/newapi-bridge-service/start-newapi-bridge-service.sh \
  "$OUT_DIR_ABS/scripts/newapi-bridge-service/start-newapi-bridge-service.sh"
install -m 0644 \
  scripts/newapi-bridge-service/newapi-bridge-service.env.example \
  "$OUT_DIR_ABS/scripts/newapi-bridge-service/newapi-bridge-service.env.example"
install -m 0644 \
  scripts/newapi-bridge-service/pricing-rules.example.env \
  "$OUT_DIR_ABS/scripts/newapi-bridge-service/pricing-rules.example.env"
install -m 0644 \
  scripts/newapi-bridge-service/oasis7-newapi-bridge.service \
  "$OUT_DIR_ABS/scripts/newapi-bridge-service/oasis7-newapi-bridge.service"
install -m 0644 \
  doc/world-runtime/runtime/newapi-bridge-service-operator-runbook.md \
  "$OUT_DIR_ABS/doc/world-runtime/runtime/newapi-bridge-service-operator-runbook.md"
install -m 0644 \
  "$TMP_DIR_ABS/HELP.txt" \
  "$OUT_DIR_ABS/HELP.txt"

{
  echo "git_sha=${GIT_SHA}"
  echo "git_ref=${GIT_REF}"
  echo "git_ref_name=${GIT_REF_NAME}"
  echo "profile=${PROFILE}"
  echo "target=${HOST_TRIPLE}"
  echo "binary=oasis7_newapi_bridge_service"
  echo "service=scripts/newapi-bridge-service/oasis7-newapi-bridge.service"
  echo "start_script=scripts/newapi-bridge-service/start-newapi-bridge-service.sh"
  echo "env_example=scripts/newapi-bridge-service/newapi-bridge-service.env.example"
  echo "pricing_rules_example=scripts/newapi-bridge-service/pricing-rules.example.env"
  echo "runbook=doc/world-runtime/runtime/newapi-bridge-service-operator-runbook.md"
} > "$OUT_DIR_ABS/BUILDINFO"

(
  cd "$OUT_DIR_ABS"
  sha256_emit \
    oasis7_newapi_bridge_service \
    scripts/newapi-bridge-service/start-newapi-bridge-service.sh \
    scripts/newapi-bridge-service/newapi-bridge-service.env.example \
    scripts/newapi-bridge-service/pricing-rules.example.env \
    scripts/newapi-bridge-service/oasis7-newapi-bridge.service \
    doc/world-runtime/runtime/newapi-bridge-service-operator-runbook.md \
    HELP.txt \
    BUILDINFO > SHA256SUMS
)

rm -rf "$ARCHIVE_ROOT_DIR"
mkdir -p "$ARCHIVE_ROOT_DIR"
cp -R "$OUT_DIR_ABS" "$ARCHIVE_ROOT_DIR/$ARCHIVE_ROOT_NAME"
tar -czf "$ARCHIVE_ABS_PATH" -C "$ARCHIVE_ROOT_DIR" "$ARCHIVE_ROOT_NAME"

echo "NewAPI bridge service package written to $ARCHIVE_ABS_PATH"
