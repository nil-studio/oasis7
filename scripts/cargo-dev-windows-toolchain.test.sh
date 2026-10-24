#!/usr/bin/env bash
# Cross-platform test contract: Windows Git Bash fallback must not alter Linux/macOS PATH behavior.
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

case "${OSTYPE:-}" in
  msys*|cygwin*) ;;
  *)
    echo "cargo-dev-windows-toolchain.test: SKIP (Windows-only fallback contract)"
    exit 0
    ;;
esac

GIT_DIR="$(dirname "$(command -v git)")"
clean_path="$GIT_DIR:/usr/bin:/bin"
if ! cargo_version="$(PATH="$clean_path" "$ROOT_DIR/scripts/cargo-dev.sh" --version)"; then
  echo "cargo-dev-windows-toolchain.test: cargo execution failed without login-profile Rust PATH" >&2
  exit 1
fi
[[ "$cargo_version" == cargo\ * ]] || {
  echo "cargo-dev-windows-toolchain.test: unexpected cargo version output: $cargo_version" >&2
  exit 1
}

echo "cargo-dev-windows-toolchain.test: OK"
