#!/usr/bin/env bash
set -euo pipefail

# Ensure the local Viewer npm install is usable immediately before a local
# build/test. Local wrappers source this file and call the function below; the
# command-line entrypoint is useful for operators and narrow shell tests.
viewer_dependency_preflight() {
  local repo_root="${1:?repository root is required}"
  local purpose="${2:-build}"
  repo_root="$(cd "$repo_root" && pwd -P)"
  local viewer_dir="$repo_root/crates/oasis7_viewer"
  local missing=()

  case "$purpose" in
    build|test|all) ;;
    *)
      echo "error: Viewer dependency preflight purpose must be build, test, or all" >&2
      return 2
      ;;
  esac

  if ! command -v npm >/dev/null 2>&1; then
    echo "error: Viewer npm dependencies are missing or incomplete." >&2
    echo "hint: install npm, then run npm --prefix crates/oasis7_viewer ci" >&2
    if [[ "${OASIS7_LOCAL_CHECK:-0}" == 1 ]]; then return 2; fi
    return 1
  fi

  [[ -f "$viewer_dir/package.json" ]] || missing+=("$viewer_dir/package.json")
  [[ -f "$viewer_dir/package-lock.json" ]] || missing+=("$viewer_dir/package-lock.json")
  [[ -d "$viewer_dir/node_modules" ]] || missing+=("$viewer_dir/node_modules")
  [[ -f "$viewer_dir/node_modules/.package-lock.json" ]] || missing+=("$viewer_dir/node_modules/.package-lock.json")
  if [[ -f "$viewer_dir/package-lock.json" && -f "$viewer_dir/node_modules/.package-lock.json" ]] \
    && [[ "$viewer_dir/node_modules/.package-lock.json" -ot "$viewer_dir/package-lock.json" ]]; then
    missing+=("stale node_modules/.package-lock.json")
  fi

  if [[ "$purpose" == "build" || "$purpose" == "all" ]]; then
    [[ -x "$viewer_dir/node_modules/.bin/vite" ]] || missing+=("$viewer_dir/node_modules/.bin/vite")
  fi
  if [[ "$purpose" == "test" || "$purpose" == "all" ]]; then
    [[ -x "$viewer_dir/node_modules/.bin/vitest" ]] || missing+=("$viewer_dir/node_modules/.bin/vitest")
  fi

  if ((${#missing[@]} > 0)); then
    if [[ "${OASIS7_LOCAL_CHECK:-0}" == 1 ]]; then
      echo "error: local checks require prepared Viewer dependencies; automatic installation is disabled" >&2
      printf 'detail: missing %s\n' "${missing[@]}" >&2
      return 2
    fi
    echo "info: Viewer npm dependencies are missing; running npm --prefix crates/oasis7_viewer ci" >&2
    if ! (
      cd "$repo_root"
      npm --prefix crates/oasis7_viewer ci
    ); then
      echo "error: Viewer npm dependency install failed; run npm --prefix crates/oasis7_viewer ci and retry." >&2
      return 1
    fi

    missing=()
    [[ -f "$viewer_dir/package.json" ]] || missing+=("$viewer_dir/package.json")
    [[ -f "$viewer_dir/package-lock.json" ]] || missing+=("$viewer_dir/package-lock.json")
    [[ -d "$viewer_dir/node_modules" ]] || missing+=("$viewer_dir/node_modules")
    [[ -f "$viewer_dir/node_modules/.package-lock.json" ]] || missing+=("$viewer_dir/node_modules/.package-lock.json")
    if [[ -f "$viewer_dir/package-lock.json" && -f "$viewer_dir/node_modules/.package-lock.json" ]] \
      && [[ "$viewer_dir/node_modules/.package-lock.json" -ot "$viewer_dir/package-lock.json" ]]; then
      missing+=("stale node_modules/.package-lock.json")
    fi
    if [[ "$purpose" == "build" || "$purpose" == "all" ]]; then
      [[ -x "$viewer_dir/node_modules/.bin/vite" ]] || missing+=("$viewer_dir/node_modules/.bin/vite")
    fi
    if [[ "$purpose" == "test" || "$purpose" == "all" ]]; then
      [[ -x "$viewer_dir/node_modules/.bin/vitest" ]] || missing+=("$viewer_dir/node_modules/.bin/vitest")
    fi
    if ((${#missing[@]} > 0)); then
      echo "error: Viewer npm dependency install completed but dependencies remain incomplete." >&2
      printf 'detail: missing %s\n' "$(IFS=', '; echo "${missing[*]}")" >&2
      return 1
    fi
  fi

  printf 'Viewer npm dependencies ready: %s\n' "$viewer_dir"
}

if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
  ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
  PURPOSE="build"
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --root)
        ROOT_DIR="${2:-}"
        shift 2
        ;;
      --purpose)
        PURPOSE="${2:-}"
        shift 2
        ;;
      -h|--help)
        cat <<'USAGE'
Usage: ./scripts/viewer-dependency-preflight.sh [--root <repo>] [--purpose build|test|all]

Ensure the local Viewer package has a fresh lockfile and required npm binaries.
Missing or stale dependencies trigger one deterministic
`npm --prefix crates/oasis7_viewer ci`, followed by revalidation.
USAGE
        exit 0
        ;;
      *)
        echo "error: unknown option: $1" >&2
        exit 2
        ;;
    esac
  done
  [[ -n "$ROOT_DIR" ]] || { echo "error: --root cannot be empty" >&2; exit 2; }
  viewer_dependency_preflight "$ROOT_DIR" "$PURPOSE"
fi
