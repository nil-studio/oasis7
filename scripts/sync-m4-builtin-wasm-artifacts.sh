#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  cat <<'USAGE'
Usage:
  scripts/sync-m4-builtin-wasm-artifacts.sh [options]

Options are forwarded to scripts/sync-m1-builtin-wasm-artifacts.sh with:
  --module-ids-path crates/oasis7_runtime/src/world/artifacts/m4_builtin_module_ids.txt
  --hash-path crates/oasis7_runtime/src/world/artifacts/m4_builtin_modules.sha256

Common options:
  --check                 Build and verify hash manifest vs built wasm, then hydrate DistFS blobs
  --profile <name>        Cargo profile forwarded to wasm build suite (default: release)
  --out-dir <dir>         Build output directory (default: .tmp/builtin-wasm-sync-modules)
  --identity-path <p>     Identity manifest path tracked by git
  --distfs-root <p>       DistFS builtin wasm root (default: .distfs/builtin_wasm)
  -h, --help              Show this help

Env:
  OASIS7_WASM_CANONICAL_PLATFORMS
  OASIS7_WASM_SYNC_WRITE_ALLOW
  CI
USAGE
  exit 0
fi

"$ROOT_DIR/scripts/sync-m1-builtin-wasm-artifacts.sh" \
  --module-ids-path "$ROOT_DIR/crates/oasis7_runtime/src/world/artifacts/m4_builtin_module_ids.txt" \
  --hash-path "$ROOT_DIR/crates/oasis7_runtime/src/world/artifacts/m4_builtin_modules.sha256" \
  "$@"
