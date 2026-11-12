#!/usr/bin/env bash
set -euo pipefail

version="${CARGO_DENY_VERSION:-0.19.9}"
timeout_seconds="${CARGO_DENY_INSTALL_TIMEOUT_SECONDS:-600}"

if command -v cargo-deny >/dev/null 2>&1; then
  current_version=$(cargo-deny --version | awk '{print $2}')
  if [[ "$current_version" == "$version" ]]; then
    echo "cargo-deny ${version} already installed"
    exit 0
  fi
fi

if [[ "${CI:-}" == "true" ]]; then
  echo "error: cargo-deny ${version} is required in CI; install it in the workflow before invoking this script" >&2
  exit 127
fi

echo "installing cargo-deny ${version}"
python3 - "$version" "$timeout_seconds" <<'PY'
from __future__ import annotations

import subprocess
import sys

version = sys.argv[1]
timeout_seconds = int(sys.argv[2])
try:
    result = subprocess.run(
        ["cargo", "install", "cargo-deny", "--version", version, "--locked"],
        timeout=timeout_seconds,
        check=False,
    )
except subprocess.TimeoutExpired:
    print(
        f"error: cargo-deny {version} install timed out after {timeout_seconds} seconds",
        file=sys.stderr,
    )
    raise SystemExit(124)
raise SystemExit(result.returncode)
PY
