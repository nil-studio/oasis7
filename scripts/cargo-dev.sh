#!/usr/bin/env bash
# Cross-platform maintenance: preserve Windows Git Bash/PowerShell and Linux/macOS toolchain discovery.
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage: ./scripts/cargo-dev.sh [--print-target-dir|--prepare-target] <cargo-args...>

Run cargo with a worktree-scoped shared development target dir so one worktree
can reuse build artifacts without selecting artifacts from a divergent source.

Default behavior:
- Computes a stable cache namespace from the git common dir and current
  worktree source root
- Stores shared targets under `<repo-parent>/.oasis7-cache/cargo-target/`
- Uses a lean dev/test profile (debug=0) for local commands
- Enables incremental for local check/clippy and builds without sccache
- Uses installed sccache for build/test/run in lean mode
- Set OASIS7_CARGO_DEV_MODE=debug for full debug info and incremental builds
- Keeps each worktree target isolated while sccache shares compiler results

Options:
  --prepare-target    Prepare the local target link without compiling
  --print-target-dir   Print the resolved shared target dir and exit
  -h, --help           Show this help

Environment:
  OASIS7_CARGO_DEV_MODE=lean|debug
      Lean is the default; debug enables full symbols and incremental builds
      and disables sccache. CI keeps its existing profile environment.
  OASIS7_CARGO_SCCACHE=0
      Disable automatic sccache use (also disabled in CI).
  OASIS7_CARGO_SHARED_TARGET_DIR
      Override the resolved target dir. This is an explicit escape hatch from
      worktree isolation and should only be used for compatible sources.

Notes:
- This wrapper is for local development commands such as `check`, `test`, `run`,
  and `build`.
- If the current worktree has a legacy managed `target` symlink, an actual
  development invocation atomically replaces only that symlink with the
  current worktree namespace and preserves the old cache directory.
- Do not use it for deterministic wasm / release flows that require
  `CARGO_TARGET_DIR` to stay unset.

Examples:
  ./scripts/cargo-dev.sh check -p oasis7
  ./scripts/cargo-dev.sh test -p oasis7
  ./scripts/cargo-dev.sh run -p oasis7_launcher_backend --bin oasis7_game_launcher
  ./scripts/cargo-dev.sh --print-target-dir
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

if [[ "${1:-}" == "--prepare-target" && $# -ne 1 ]]; then
  echo "error: --prepare-target does not accept cargo arguments" >&2
  exit 2
fi

COMMON_GIT_DIR="$(cd "$(git rev-parse --git-common-dir)" && pwd -P)"
REPO_ROOT="$(git rev-parse --show-toplevel)"
CANONICAL_REPO_ROOT="$(cd "$COMMON_GIT_DIR/.." && pwd -P)"
if ! PYTHON_BIN="$("$REPO_ROOT/scripts/find-python-with-module.sh" ast)"; then
  echo "error: cannot find a functional Python interpreter for cargo target discovery" >&2
  exit 1
fi

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
HOST_TRIPLE="$("$RUSTC_BIN" -vV | sed -n 's/^host: //p')"
RUSTC_RELEASE="$("$RUSTC_BIN" -vV | sed -n 's/^release: //p')"

cache_namespace() {
  "$PYTHON_BIN" - "$COMMON_GIT_DIR" "$REPO_ROOT" <<'PY'
from __future__ import annotations

import hashlib
import pathlib
import sys

common_dir = pathlib.Path(sys.argv[1]).resolve()
source_root = pathlib.Path(sys.argv[2]).resolve()
common_digest = hashlib.sha256(str(common_dir).encode("utf-8")).hexdigest()[:12]
source_digest = hashlib.sha256(str(source_root).encode("utf-8")).hexdigest()[:12]
print(f"git-{common_digest}/worktree-{source_digest}")
PY
}

NAMESPACE="$(cache_namespace)"
if [[ -n "${OASIS7_CARGO_SHARED_TARGET_DIR:-}" ]]; then
  TARGET_DIR="$OASIS7_CARGO_SHARED_TARGET_DIR"
else
  CACHE_BASE_DIR="$(dirname "$CANONICAL_REPO_ROOT")/.oasis7-cache"
  TARGET_DIR="$CACHE_BASE_DIR/cargo-target/$NAMESPACE/rustc-$RUSTC_RELEASE-$HOST_TRIPLE"
fi

case "${1:-}" in
  --print-target-dir)
    if [[ $# -ne 1 ]]; then
      echo "error: --print-target-dir does not accept cargo arguments" >&2
      exit 2
    fi
    printf '%s\n' "$TARGET_DIR"
    exit 0
    ;;
esac

# Raw Cargo entrypoints can reuse this namespace without changing build flags.
# Never replace a real directory or affect CI checkouts.
if [[ "${CI:-}" != "1" && "${CI:-}" != "true" && -z "${OASIS7_CARGO_SHARED_TARGET_DIR:-}" ]]; then
  if [[ ! -e "$REPO_ROOT/target" && ! -L "$REPO_ROOT/target" ]]; then
    mkdir -p "$TARGET_DIR"
    "$PYTHON_BIN" - "$TARGET_DIR" "$REPO_ROOT/target" "$1" <<'LINK'
import os, sys
try:
    os.symlink(sys.argv[1], sys.argv[2], target_is_directory=True)
except FileExistsError:
    if not os.path.islink(sys.argv[2]):
        raise SystemExit("error: target appeared during cache preparation; preserved")
except OSError as exc:
    if os.name == "nt" and getattr(exc, "winerror", None) in (5, 1314) and sys.argv[3] != "--prepare-target":
        print("warning: target link needs Windows symlink privileges; Cargo will use the shared target directly", file=sys.stderr)
    else:
        raise SystemExit(f"error: cannot prepare target link: {exc}")
LINK
  elif [[ ! -L "$REPO_ROOT/target" && "${1:-}" == "--prepare-target" ]]; then
    echo "error: existing target directory must be reviewed and cleaned before preparing the shared link" >&2
    exit 1
  fi
fi

if [[ -L "$REPO_ROOT/target" && -z "${OASIS7_CARGO_SHARED_TARGET_DIR:-}" ]]; then
  CURRENT_TARGET="$("$PYTHON_BIN" - "$REPO_ROOT/target" <<'PY'
import os
import sys

print(os.path.realpath(sys.argv[1]))
PY
)"
  EXPECTED_TARGET="$("$PYTHON_BIN" - "$TARGET_DIR" <<'PY'
import os
import sys

print(os.path.realpath(sys.argv[1]))
PY
)"
  if [[ "$CURRENT_TARGET" != "$EXPECTED_TARGET" ]]; then
    if ! "$PYTHON_BIN" - "$REPO_ROOT/target" "$CANONICAL_REPO_ROOT" <<'PY'
import os
import pathlib
import sys

old_target = pathlib.Path(os.path.realpath(sys.argv[1]))
cache_root = pathlib.Path(sys.argv[2]).resolve().parent / ".oasis7-cache" / "cargo-target"
try:
    old_target.relative_to(cache_root)
except ValueError:
    raise SystemExit(1)
raise SystemExit(0)
PY
    then
      echo "error: worktree target symlink points outside the managed Cargo cache; repair $REPO_ROOT/target before using cargo-dev.sh" >&2
      exit 1
    fi
    MIGRATION_DIR="$(mktemp -d "$REPO_ROOT/target.migration.XXXXXX")"
    if ! ln -s "$TARGET_DIR" "$MIGRATION_DIR/target" || \
      ! "$PYTHON_BIN" - "$MIGRATION_DIR/target" "$REPO_ROOT/target" <<'PY'
import os
import sys

os.replace(sys.argv[1], sys.argv[2])
PY
    then
      unlink "$MIGRATION_DIR/target" >/dev/null 2>&1 || true
      rmdir "$MIGRATION_DIR" >/dev/null 2>&1 || true
      echo "error: failed to migrate worktree target symlink; legacy cache was preserved" >&2
      exit 1
    fi
    rmdir "$MIGRATION_DIR" >/dev/null 2>&1 || true
  fi
fi

if [[ "${1:-}" == "--prepare-target" ]]; then
  exec "$PYTHON_BIN" "$REPO_ROOT/scripts/cargo-cache.py" lease \
    --target "$TARGET_DIR" --repo-root "$REPO_ROOT" --common-dir "$COMMON_GIT_DIR" -- true
fi

MODE="${OASIS7_CARGO_DEV_MODE:-lean}"
case "$MODE" in lean|debug) ;; *) echo "error: OASIS7_CARGO_DEV_MODE must be lean or debug" >&2; exit 2 ;; esac
# Resolve Cargo profile only before the program/test argument separator.
CARGO_PROFILE=dev
EXPECT_PROFILE=0
for ARG in "$@"; do
  if [[ "$EXPECT_PROFILE" == 1 ]]; then
    CARGO_PROFILE="$ARG"
    EXPECT_PROFILE=0
    continue
  fi
  case "$ARG" in
    --) break ;;
    --release|-r) CARGO_PROFILE=release ;;
    --profile) EXPECT_PROFILE=1 ;;
    --profile=*) CARGO_PROFILE="${ARG#--profile=}" ;;
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
# Lease covers compilation and cargo-run children; cache GC uses the same lock.
CARGO_LEASE=("$PYTHON_BIN" "$REPO_ROOT/scripts/cargo-cache.py" lease
  --target "$TARGET_DIR" --repo-root "$REPO_ROOT" --common-dir "$COMMON_GIT_DIR" --)
if [[ -n "${OASIS7_CARGO_SHARED_TARGET_DIR:-}" ]]; then
  # Explicit overrides are outside automatic ownership and reclamation policy.
  mkdir -p "$TARGET_DIR"
  CARGO_LEASE=(env)
fi
# Only development commands which emit linkable Rust libraries benefit from
# sccache. Formal CI/release entrypoints retain their raw Cargo path.
# Do not inherit an arbitrary compiler wrapper.
CARGO_ENV=(env -u RUSTC_WRAPPER ${PROFILE_ENV[@]+"${PROFILE_ENV[@]}"} CARGO_TARGET_DIR="$TARGET_DIR")
if [[ -n "$SCCACHE_BIN" ]]; then
  # Pass target as an argument to preserve cross-worktree registry cache keys.
  exec "${CARGO_LEASE[@]}" env -u RUSTC_WRAPPER -u CARGO_TARGET_DIR RUSTC_WRAPPER="$SCCACHE_BIN" \
    ${PROFILE_ENV[@]+"${PROFILE_ENV[@]}"} \
    "$CARGO_BIN" "$1" --target-dir "$TARGET_DIR" "${@:2}"
fi
exec "${CARGO_LEASE[@]}" "${CARGO_ENV[@]}" "$CARGO_BIN" "$@"
