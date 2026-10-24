#!/usr/bin/env bash
# Cross-platform maintenance: preserve Windows Git Bash/PowerShell and Linux/macOS toolchain discovery.
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage: ./scripts/cargo-dev.sh <cargo-args...>

Run cargo with local development profiles and optional sccache.
Cargo resolves its own target directory (normally this worktree's target/).

Default behavior:
- Uses a lean dev/test profile (debug=0) for local commands
- Enables incremental for local check/clippy and builds without sccache
- Uses installed sccache for build/test/run in lean mode
- Set OASIS7_CARGO_DEV_MODE=debug for full debug info and incremental builds
- Passes Cargo arguments and target configuration through unchanged

Environment:
  OASIS7_CARGO_DEV_MODE=lean|debug
      Lean is the default; debug enables full symbols and incremental builds
      and disables sccache. CI keeps its existing profile environment.
  OASIS7_CARGO_SCCACHE=0
      Disable automatic sccache use (also disabled in CI).

Notes:
- This wrapper is for local development commands such as `check`, `test`, `run`,
  and `build`.
- Do not use it for deterministic wasm / release flows that require
  `CARGO_TARGET_DIR` to stay unset.

Examples:
  ./scripts/cargo-dev.sh check -p oasis7
  ./scripts/cargo-dev.sh test -p oasis7
  ./scripts/cargo-dev.sh run -p oasis7_launcher_backend --bin oasis7_game_launcher
USAGE
}

if [[ $# -eq 0 ]]; then
  usage >&2
  exit 2
fi

case "${1:-}" in
  -h|--help)
    usage
    exit 0
    ;;
esac

resolve_rustc() {
  local candidate
  if candidate="$(command -v rustc 2>/dev/null)"; then
    printf '%s\n' "$candidate"
    return 0
  fi
  case "${OSTYPE:-}" in
    msys*|cygwin*)
      for candidate in "${RUSTUP_HOME:-$HOME/.rustup}"/toolchains/*/bin/rustc.exe; do
        [[ -x "$candidate" ]] || continue
        "$candidate" -vV >/dev/null 2>&1 || continue
        printf '%s\n' "$candidate"
        return 0
      done
      ;;
  esac
  return 1
}

if ! RUSTC_BIN="$(resolve_rustc)"; then
  echo "error: rustc not found on PATH or in the Windows Rustup toolchain directory" >&2
  exit 1
fi

resolve_cargo() {
  local candidate
  if candidate="$(command -v cargo 2>/dev/null)"; then
    printf '%s\n' "$candidate"
    return 0
  fi
  case "${OSTYPE:-}" in
    msys*|cygwin*)
      candidate="$(dirname "$RUSTC_BIN")/cargo.exe"
      if [[ -x "$candidate" ]] && "$candidate" --version >/dev/null 2>&1; then
        printf '%s\n' "$candidate"
        return 0
      fi
      ;;
  esac
  return 1
}

if ! CARGO_BIN="$(resolve_cargo)"; then
  echo "error: cargo not found on PATH or alongside the Windows Rustup toolchain rustc" >&2
  exit 1
fi
MODE="${OASIS7_CARGO_DEV_MODE:-lean}"
case "$MODE" in lean|debug) ;; *) echo "error: OASIS7_CARGO_DEV_MODE must be lean or debug" >&2; exit 2 ;; esac
# Resolve Cargo profile only before the program/test argument separator.
CARGO_PROFILE=dev
EXPECT_PROFILE=0
EXPECT_VALUE=0
for ARG in "$@"; do
  if [[ "$EXPECT_VALUE" == 1 ]]; then
    EXPECT_VALUE=0
    continue
  fi
  if [[ "$EXPECT_PROFILE" == 1 ]]; then
    CARGO_PROFILE="$ARG"
    EXPECT_PROFILE=0
    continue
  fi
  case "$ARG" in
    --) break ;;
    --release) CARGO_PROFILE=release ;;
    --profile) EXPECT_PROFILE=1 ;;
    --profile=*) CARGO_PROFILE="${ARG#--profile=}" ;;
    -?*)
      # Long options are not short clusters. Cargo validates unknown options.
      [[ "$ARG" != --* ]] || continue
      SHORT_OPTIONS="${ARG#-}"
      while [[ -n "$SHORT_OPTIONS" ]]; do
        SHORT_OPTION="${SHORT_OPTIONS:0:1}"
        SHORT_OPTIONS="${SHORT_OPTIONS:1}"
        case "$SHORT_OPTION" in
          r) CARGO_PROFILE=release ;;
          v|q|h) ;;
          p|F|j|Z)
            # The remaining characters (or next argv) belong to this value.
            [[ -n "$SHORT_OPTIONS" ]] || EXPECT_VALUE=1
            break
            ;;
          *) break ;;
        esac
      done
      ;;
  esac
done
LOCAL_DEV=0
if [[ "${CI:-}" != "1" && "${CI:-}" != "true" && "$EXPECT_PROFILE" == 0 ]]; then
  case "$CARGO_PROFILE" in dev|test) LOCAL_DEV=1 ;; esac
fi
SCCACHE_BIN=""
if [[ "$LOCAL_DEV" == 1 && "$MODE" == lean && "${OASIS7_CARGO_SCCACHE:-1}" != 0 ]]; then
  case "${1:-}" in
    build|test|run) SCCACHE_BIN="$(command -v sccache || true)" ;;
  esac
fi
PROFILE_ENV=()
if [[ "$LOCAL_DEV" == 1 ]]; then
  if [[ "$MODE" == debug ]]; then
    PROFILE_ENV=(CARGO_PROFILE_DEV_DEBUG=2 CARGO_PROFILE_TEST_DEBUG=2 CARGO_INCREMENTAL=1)
  else
    INCREMENTAL=0
    case "${1:-}" in
      check|clippy) INCREMENTAL=1 ;;
      build|test|run) [[ -n "$SCCACHE_BIN" ]] || INCREMENTAL=1 ;;
    esac
    PROFILE_ENV=(CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL="$INCREMENTAL")
  fi
fi
# Do not inherit an arbitrary compiler wrapper. Cargo owns target resolution
# and locking; exec preserves its exit status without cleanup or retry.
if [[ -n "$SCCACHE_BIN" ]]; then
  exec env -u RUSTC_WRAPPER RUSTC_WRAPPER="$SCCACHE_BIN" \
    ${PROFILE_ENV[@]+"${PROFILE_ENV[@]}"} "$CARGO_BIN" "$@"
fi
exec env -u RUSTC_WRAPPER ${PROFILE_ENV[@]+"${PROFILE_ENV[@]}"} "$CARGO_BIN" "$@"
