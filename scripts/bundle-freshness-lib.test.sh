#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$ROOT_DIR/scripts/bundle-freshness-lib.sh"

tmp_repo="$(mktemp -d)"
cleanup() {
  rm -rf "$tmp_repo"
}
trap cleanup EXIT

mkdir -p \
  "$tmp_repo/bundle" \
  "$tmp_repo/bundle/web" \
  "$tmp_repo/bundle/web-launcher" \
  "$tmp_repo/crates/oasis7_proto/src" \
  "$tmp_repo/crates/oasis7_chain_runtime/src" \
  "$tmp_repo/crates/oasis7_viewer" \
  "$tmp_repo/scripts"

printf '[workspace]\nmembers = []\n' > "$tmp_repo/Cargo.toml"
printf '# lock\n' > "$tmp_repo/Cargo.lock"
mkdir -p "$tmp_repo/bundle/web/browser-game-client" "$tmp_repo/crates/oasis7_browser_game_client/src"
printf 'export const protocol = true;\n' > "$tmp_repo/bundle/web/browser-game-client/oasis7_browser_game_client.js"
printf '\0asm\n' > "$tmp_repo/bundle/web/browser-game-client/oasis7_browser_game_client_bg.wasm"
printf 'pub fn protocol() {}\n' > "$tmp_repo/crates/oasis7_browser_game_client/src/lib.rs"
printf 'pub const VIEWER_PROTOCOL_VERSION: u32 = 7;\n' > "$tmp_repo/crates/oasis7_proto/src/viewer.rs"
printf '<!doctype html>viewer\n' > "$tmp_repo/crates/oasis7_viewer/viewer.html"
printf 'console.log("viewer");\n' > "$tmp_repo/crates/oasis7_viewer/viewer.js"
printf '<!doctype html>canonical claim evidence\n' > "$tmp_repo/crates/oasis7_viewer/viewer_first_agent_claim_evidence.html"
printf '#!/usr/bin/env bash\nset -euo pipefail\n' > "$tmp_repo/scripts/copy-viewer-web-dist.sh"
printf '#!/usr/bin/env bash\nset -euo pipefail\n' > "$tmp_repo/scripts/viewer-web-dist-contract.sh"
printf '<!doctype html><script type="module" src="./viewer.js"></script>\n' > "$tmp_repo/bundle/web/index.html"
printf 'console.log("bundle viewer");\n' > "$tmp_repo/bundle/web/viewer.js"
printf '<!doctype html><script type="module" src="./app.js"></script>\n' > "$tmp_repo/bundle/web-launcher/index.html"
printf 'console.log("launcher app");\n' > "$tmp_repo/bundle/web-launcher/app.js"

printf '[package]\nname = "oasis7_chain_runtime"\nversion = "0.1.0"\n' > "$tmp_repo/crates/oasis7_chain_runtime/Cargo.toml"
printf 'fn main() {}\n' > "$tmp_repo/crates/oasis7_chain_runtime/src/main.rs"

required_scope_entries=(
  "crates/oasis7_chain_runtime/Cargo.toml"
  "crates/oasis7_viewer/viewer.html"
  "crates/oasis7_viewer/viewer_first_agent_claim_evidence.html"
  "scripts/copy-viewer-web-dist.sh"
  "scripts/viewer-web-dist-contract.sh"
)

source_scope="$(bundle_source_metadata_json "$tmp_repo" | python3 -c 'import json,sys; print("\n".join(json.load(sys.stdin)["sourceScope"]))')"
for entry in "${required_scope_entries[@]}"; do
  if ! grep -Fxq "$entry" <<<"$source_scope"; then
    echo "expected bundle source scope to include $entry" >&2
    exit 1
  fi
done

for retired_root_alias in \
  "crates/oasis7_viewer/software_safe.html" \
  "crates/oasis7_viewer/software_safe.js" \
  "crates/oasis7_viewer/software_safe_first_agent_claim_evidence.html"; do
  if grep -Fxq "$retired_root_alias" <<<"$source_scope"; then
    echo "generated compatibility alias must not be a source input: $retired_root_alias" >&2
    exit 1
  fi
done

for entry in "${required_scope_entries[@]}" "crates/oasis7_chain_runtime/src/main.rs"; do
  rm -f "$(bundle_manifest_path "$tmp_repo/bundle")"
  bundle_write_manifest "$tmp_repo" "$tmp_repo/bundle"

  printf '\n# freshness drift for %s\n' "$entry" >> "$tmp_repo/$entry"

  if freshness_output="$(bundle_check_freshness "$tmp_repo" "$tmp_repo/bundle" 2>&1)"; then
    echo "expected bundle freshness to fail after mutating $entry" >&2
    exit 1
  fi
  if ! grep -Fq "source fingerprint drift" <<<"$freshness_output"; then
    echo "expected source fingerprint drift after mutating $entry, got: $freshness_output" >&2
    exit 1
  fi
done

expect_asset_drift() {
  local label=$1
  if freshness_output="$(bundle_check_freshness "$tmp_repo" "$tmp_repo/bundle" 2>&1)"; then
    echo "expected bundle freshness to fail after $label" >&2
    exit 1
  fi
  if ! grep -Fq "bundle asset drift" <<<"$freshness_output"; then
    echo "expected bundle asset drift after $label, got: $freshness_output" >&2
    exit 1
  fi
}

bundle_write_manifest "$tmp_repo" "$tmp_repo/bundle"
printf '<!doctype html>mutated viewer entry\n' > "$tmp_repo/bundle/web/index.html"
expect_asset_drift "mutating bundle/web/index.html"

printf '<!doctype html><script type="module" src="./viewer.js"></script>\n' > "$tmp_repo/bundle/web/index.html"
bundle_write_manifest "$tmp_repo" "$tmp_repo/bundle"
rm -f "$tmp_repo/bundle/web/index.html"
expect_asset_drift "deleting bundle/web/index.html"

printf '<!doctype html><script type="module" src="./viewer.js"></script>\n' > "$tmp_repo/bundle/web/index.html"
bundle_write_manifest "$tmp_repo" "$tmp_repo/bundle"
printf 'console.log("mutated viewer");\n' > "$tmp_repo/bundle/web/viewer.js"
expect_asset_drift "mutating bundle/web/viewer.js"

printf '<!doctype html><script type="module" src="./viewer.js"></script>\n' > "$tmp_repo/bundle/web/index.html"
printf 'console.log("bundle viewer");\n' > "$tmp_repo/bundle/web/viewer.js"
bundle_write_manifest "$tmp_repo" "$tmp_repo/bundle"
rm -f "$tmp_repo/bundle/web/viewer.js"
expect_asset_drift "deleting bundle/web/viewer.js"

printf 'console.log("bundle viewer");\n' > "$tmp_repo/bundle/web/viewer.js"
printf '<!doctype html><script type="module" src="./app.js"></script>\n' > "$tmp_repo/bundle/web-launcher/index.html"
printf 'console.log("launcher app");\n' > "$tmp_repo/bundle/web-launcher/app.js"
bundle_write_manifest "$tmp_repo" "$tmp_repo/bundle"
printf '<!doctype html>mutated launcher entry\n' > "$tmp_repo/bundle/web-launcher/index.html"
expect_asset_drift "mutating bundle/web-launcher/index.html"

printf '<!doctype html><script type="module" src="./app.js"></script>\n' > "$tmp_repo/bundle/web-launcher/index.html"
bundle_write_manifest "$tmp_repo" "$tmp_repo/bundle"
rm -f "$tmp_repo/bundle/web-launcher/index.html"
expect_asset_drift "deleting bundle/web-launcher/index.html"

printf '<!doctype html><script type="module" src="./app.js"></script>\n' > "$tmp_repo/bundle/web-launcher/index.html"
bundle_write_manifest "$tmp_repo" "$tmp_repo/bundle"
printf 'console.log("mutated launcher");\n' > "$tmp_repo/bundle/web-launcher/app.js"
expect_asset_drift "mutating bundle/web-launcher/app.js"

printf '<!doctype html><script type="module" src="./app.js"></script>\n' > "$tmp_repo/bundle/web-launcher/index.html"
printf 'console.log("launcher app");\n' > "$tmp_repo/bundle/web-launcher/app.js"
bundle_write_manifest "$tmp_repo" "$tmp_repo/bundle"
rm -f "$tmp_repo/bundle/web-launcher/app.js"
expect_asset_drift "deleting bundle/web-launcher/app.js"

bundle_write_manifest "$tmp_repo" "$tmp_repo/bundle" packaging native
if freshness_output="$(bundle_check_freshness "$tmp_repo" "$tmp_repo/bundle" packaging native 2>&1)"; then
  :
else
  echo "expected matching packaging profile/target to remain fresh: $freshness_output" >&2
  exit 1
fi
if freshness_output="$(bundle_check_freshness "$tmp_repo" "$tmp_repo/bundle" dev native 2>&1)"; then
  echo "expected profile mismatch to invalidate a reusable bundle" >&2
  exit 1
fi
grep -Fq "bundle profile drift" <<<"$freshness_output" || {
  echo "expected profile drift diagnostic, got: $freshness_output" >&2
  exit 1
}
if freshness_output="$(bundle_check_freshness "$tmp_repo" "$tmp_repo/bundle" packaging x86_64-apple-darwin 2>&1)"; then
  echo "expected target mismatch to invalidate a reusable bundle" >&2
  exit 1
fi
grep -Fq "bundle target drift" <<<"$freshness_output" || {
  echo "expected target drift diagnostic, got: $freshness_output" >&2
  exit 1
}

grep -Fq 'bundle_check_freshness "$ROOT_DIR" "$BUNDLE_DIR" "$BUNDLE_PROFILE" "$BUNDLE_TARGET_TRIPLE"' \
  "$ROOT_DIR/scripts/run-launcher-stack.sh" || {
  echo "run-launcher-stack must bind bundle profile and target freshness requests" >&2
  exit 1
}
grep -Fq 'bundle_check_freshness "$ROOT_DIR" "$BUNDLE_DIR" dev native' \
  "$ROOT_DIR/scripts/worktree-harness.sh" || {
  echo "worktree harness must bind its dev/native bundle freshness request" >&2
  exit 1
}
grep -Fq -- '--bundle-profile "$PROFILE" --bundle-target-triple native' \
  "$ROOT_DIR/scripts/run-producer-playtest.sh" || {
  echo "producer playtest must propagate its bundle profile and target" >&2
  exit 1
}


# Both generated protocol artifacts participate in bundle reuse receipts.
for name in oasis7_browser_game_client.js oasis7_browser_game_client_bg.wasm; do
  path="$tmp_repo/bundle/web/browser-game-client/$name"
  printf 'original protocol\n' > "$path"
  bundle_write_manifest "$tmp_repo" "$tmp_repo/bundle"
  printf 'mutated protocol\n' > "$path"
  expect_asset_drift "mutating browser protocol $name"
  bundle_write_manifest "$tmp_repo" "$tmp_repo/bundle"
  rm "$path"
  expect_asset_drift "deleting browser protocol $name"
  printf 'restored protocol\n' > "$path"
done
bundle_write_manifest "$tmp_repo" "$tmp_repo/bundle"
printf 'pub fn changed_protocol() {}\n' >> "$tmp_repo/crates/oasis7_browser_game_client/src/lib.rs"
if bundle_check_freshness "$tmp_repo" "$tmp_repo/bundle" > /dev/null 2>&1; then
  echo "browser Game Client source drift must invalidate bundle reuse" >&2
  exit 1
fi

echo "bundle freshness lib tests passed"
