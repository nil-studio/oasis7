#!/usr/bin/env bash
set -euo pipefail
umask 022
# Cargo parses every workspace/path manifest even when installing one package.
# These targets exist only for metadata resolution; neither source qualification
# nor a new producer dependency can be inferred from them. The producer guard
# independently requires the actual reachable local package set to be exactly
# oasis7_wasm_build + wasm_build_suite, whose real sources are never modified.
root="${1:-/opt/oasis7}"
[[ -d "$root" && ! -L "$root" ]] || { echo 'error: invalid workspace root' >&2; exit 2; }
cd "$root"
for source in crates/oasis7_wasm_build/src/lib.rs tools/wasm_build_suite/src/main.rs; do
  [[ -f "$source" && ! -L "$source" ]] || {
    echo "error: missing real producer source: $source" >&2
    exit 2
  }
done
for directory in crates tools vendor-if-watch-3.2.2 vendor-libp2p-swarm-0.48.0 vendor-libp2p-rendezvous-0.18.1; do
  [[ -d "$directory" && ! -L "$directory" ]] || {
    echo "error: missing real manifest directory: $directory" >&2
    exit 2
  }
done
# Neither metadata traversal nor generated targets may follow context links.
links="$(find crates tools vendor-if-watch-3.2.2 vendor-libp2p-swarm-0.48.0 vendor-libp2p-rendezvous-0.18.1 -type l -print -quit)"
[[ -z "$links" ]] || { echo 'error: symlink in builder context' >&2; exit 2; }
manifest_list="$(mktemp)"
trap 'rm -f -- "$manifest_list"' EXIT
find crates tools vendor-if-watch-3.2.2 vendor-libp2p-swarm-0.48.0 vendor-libp2p-rendezvous-0.18.1 -type f -name Cargo.toml -print0 > "$manifest_list"
while IFS= read -r -d '' manifest; do
  case "$manifest" in
    crates/oasis7_wasm_build/*|tools/wasm_build_suite/*) continue ;;
  esac
  directory="${manifest%/Cargo.toml}"
  # The filtered context must not supply unrelated business source code.
  if [[ -e "$directory/src" || -L "$directory/src" ]]; then
    echo "error: non-producer source present in builder context: $directory" >&2
    exit 2
  fi
  mkdir "$directory/src"
  printf '%s\n' '// Metadata-only workspace target; never a producer source or guest qualification.' > "$directory/src/lib.rs"
done < "$manifest_list"
