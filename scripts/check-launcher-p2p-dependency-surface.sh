#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

package="oasis7_launcher_backend"
forbidden_specs=(
  "libp2p"
  "ring@0.16.20"
  "rustls-webpki@0.101.7"
  "hickory-proto@0.24.4"
)

status=0
for spec in "${forbidden_specs[@]}"; do
  tmp_output=$(mktemp)
  if env -u RUSTC_WRAPPER cargo tree --offline --locked -p "$package" --no-default-features -i "$spec" >"$tmp_output" 2>&1; then
    echo "error: $package dependency closure still includes forbidden p2p surface: $spec"
    cat "$tmp_output"
    status=1
  elif grep -Eq "package ID specification .* did not match any packages" "$tmp_output"; then
    echo "ok: $package dependency closure excludes $spec"
  else
    echo "error: cargo tree check failed while inspecting forbidden p2p surface: $spec"
    cat "$tmp_output"
    status=1
  fi
  rm -f "$tmp_output"
done

exit "$status"
