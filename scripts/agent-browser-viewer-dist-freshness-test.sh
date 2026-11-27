#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$ROOT_DIR/scripts/agent-browser-lib.sh"

tmp_repo="$(mktemp -d)"
cleanup() {
  rm -rf "$tmp_repo"
}
trap cleanup EXIT

mkdir -p \
  "$tmp_repo/bin" \
  "$tmp_repo/crates/oasis7_viewer/dist" \
  "$tmp_repo/crates/oasis7_proto/src" \
  "$tmp_repo/crates/oasis7_viewer/dist/pixel-world-bridge/webgl2"

printf '<!doctype html>old dist\n' > "$tmp_repo/crates/oasis7_viewer/dist/index.html"
printf '<!doctype html>old dist\n' > "$tmp_repo/crates/oasis7_viewer/dist/viewer.html"
printf '<!doctype html>old dist\n' > "$tmp_repo/crates/oasis7_viewer/dist/software_safe.html"
printf 'console.log("dist viewer old");\n' > "$tmp_repo/crates/oasis7_viewer/dist/viewer.js"
printf 'import "./viewer.js";\n' > "$tmp_repo/crates/oasis7_viewer/dist/software_safe.js"
printf '<!doctype html>canonical claim evidence old\n' > "$tmp_repo/crates/oasis7_viewer/dist/viewer_first_agent_claim_evidence.html"
printf '<!doctype html>claim evidence old\n' > "$tmp_repo/crates/oasis7_viewer/dist/software_safe_first_agent_claim_evidence.html"
printf '<!doctype html>viewer\n' > "$tmp_repo/crates/oasis7_viewer/viewer.html"
cat > "$tmp_repo/crates/oasis7_viewer/viewer_first_agent_claim_evidence.html" <<'HTML'
<!doctype html>
<html><head><title>viewer first agent claim evidence</title></head><body>
<iframe id="viewer-frame" src="./viewer.html?test_api=1&connect=0" title="viewer first agent claim evidence"></iframe>
<script>const snapshot = { eligible_balance_after: 0, upkeep_runway_epochs: 0 };
document.getElementById("viewer-frame");</script>
</body></html>
HTML
printf '{"name":"oasis7-viewer-ui","scripts":{"build:viewer":"echo ok","build:software-safe":"echo ok"}}\n' > "$tmp_repo/crates/oasis7_viewer/package.json"
printf '{"lockfileVersion":3}\n' > "$tmp_repo/crates/oasis7_viewer/package-lock.json"
printf 'export default {};\n' > "$tmp_repo/crates/oasis7_viewer/vite.software-safe.config.mjs"
mkdir -p \
  "$tmp_repo/crates/oasis7_viewer/scripts" \
  "$tmp_repo/crates/oasis7_viewer/software_safe_src" \
  "$tmp_repo/crates/pixel_world_bridge/src"
printf 'console.log("finalize");\n' > "$tmp_repo/crates/oasis7_viewer/scripts/finalize-software-safe-build.mjs"
printf 'console.log("src");\n' > "$tmp_repo/crates/oasis7_viewer/software_safe_src/main.jsx"
printf 'export function createPixelWorldBridge() { return \"old\"; }\n' > "$tmp_repo/crates/oasis7_viewer/dist/pixel-world-bridge/pixel_world_bridge.js"
printf 'export function createPixelWorldBridge() { return "old-webgl2"; }\n' > "$tmp_repo/crates/oasis7_viewer/dist/pixel-world-bridge/webgl2/pixel_world_bridge.js"
printf 'export function initSync() {}\n' > "$tmp_repo/crates/oasis7_viewer/dist/pixel-world-bridge/webgl2/pixel_world_bridge_bindgen.js"
printf 'wasm\n' > "$tmp_repo/crates/oasis7_viewer/dist/pixel-world-bridge/webgl2/pixel_world_bridge_bindgen_bg.wasm"
printf 'icon\n' > "$tmp_repo/crates/oasis7_viewer/favicon.ico"
printf 'icon\n' > "$tmp_repo/crates/oasis7_viewer/dist/favicon.ico"
printf '[package]\nname = "pixel_world_bridge"\nversion = "0.0.0"\n' > "$tmp_repo/crates/pixel_world_bridge/Cargo.toml"
printf 'pub fn ping() {}\n' > "$tmp_repo/crates/pixel_world_bridge/src/lib.rs"
printf '[package]\nname = "oasis7_proto"\nversion = "0.0.0"\n' > "$tmp_repo/crates/oasis7_proto/Cargo.toml"
printf 'pub const VIEWER_PROTOCOL_VERSION: u32 = 1;\n' > "$tmp_repo/crates/oasis7_proto/src/viewer.rs"
printf '# lock\n' > "$tmp_repo/Cargo.lock"
printf '[workspace]\nmembers = []\n' > "$tmp_repo/Cargo.toml"

touch -d '2026-03-17 00:00:00' \
  "$tmp_repo/crates/oasis7_viewer/dist/index.html" \
  "$tmp_repo/crates/oasis7_viewer/dist/viewer.html" \
  "$tmp_repo/crates/oasis7_viewer/dist/software_safe.html" \
  "$tmp_repo/crates/oasis7_viewer/dist/viewer.js" \
  "$tmp_repo/crates/oasis7_viewer/dist/software_safe.js" \
  "$tmp_repo/crates/oasis7_viewer/dist/viewer_first_agent_claim_evidence.html" \
  "$tmp_repo/crates/oasis7_viewer/dist/software_safe_first_agent_claim_evidence.html" \
  "$tmp_repo/crates/oasis7_viewer/dist/pixel-world-bridge/pixel_world_bridge.js" \
  "$tmp_repo/crates/oasis7_viewer/dist/pixel-world-bridge/webgl2/pixel_world_bridge.js" \
  "$tmp_repo/crates/oasis7_viewer/dist/pixel-world-bridge/webgl2/pixel_world_bridge_bindgen.js" \
  "$tmp_repo/crates/oasis7_viewer/dist/pixel-world-bridge/webgl2/pixel_world_bridge_bindgen_bg.wasm" \
  "$tmp_repo/crates/oasis7_viewer/dist/favicon.ico" \
  "$tmp_repo/crates/oasis7_viewer/viewer.html" \
  "$tmp_repo/crates/oasis7_viewer/viewer_first_agent_claim_evidence.html" \
  "$tmp_repo/crates/oasis7_viewer/favicon.ico"

cat > "$tmp_repo/bin/npm" <<'NPM'
#!/usr/bin/env bash
set -euo pipefail
if [[ "$1" != "--prefix" || "$3" != "run" || "$4" != "build:software-safe" ]]; then
  echo "unexpected npm args: $*" >&2
  exit 1
fi
printf 'software safe rebuild\n'
cp crates/oasis7_viewer/viewer.html crates/oasis7_viewer/dist/software_safe.html
printf '// Generated canonical Viewer bundle; source truth lives in ./software_safe_src/.\nconsole.log("rebuilt dist viewer");\n' \
  > crates/oasis7_viewer/dist/viewer.js
printf '// Generated compat alias; canonical bundle truth lives in ./viewer.js.\nimport "./viewer.js";\n' \
  > crates/oasis7_viewer/dist/software_safe.js
sed \
  -e 's/<title>viewer first agent claim evidence<\/title>/<title>software_safe compatibility first agent claim evidence<\/title>/' \
  -e 's/id="viewer-frame"/id="software-safe-frame"/' \
  -e 's#./viewer.html?test_api=1\&connect=0#./software_safe.html?test_api=1\&connect=0#' \
  -e 's/title="viewer first agent claim evidence"/title="software_safe compatibility first agent claim evidence"/' \
  -e 's/document.getElementById("viewer-frame")/document.getElementById("software-safe-frame")/' \
  crates/oasis7_viewer/viewer_first_agent_claim_evidence.html \
  > crates/oasis7_viewer/dist/software_safe_first_agent_claim_evidence.html
NPM
chmod +x "$tmp_repo/bin/npm"

resolved_dir="$({ PATH="$tmp_repo/bin:$PATH" resolve_viewer_static_dir_for_web_closure "$tmp_repo" web "$tmp_repo/output/check"; } 2>"$tmp_repo/stderr.log")"
expected_dir="$tmp_repo/output/check/web-dist"

if [[ "$resolved_dir" != "$expected_dir" ]]; then
  echo "expected rebuilt dir '$expected_dir', got '$resolved_dir'" >&2
  exit 1
fi

if [[ ! -f "$expected_dir/index.html" ]]; then
  echo "expected rebuilt dist index at $expected_dir/index.html" >&2
  exit 1
fi
if ! cmp -s "$tmp_repo/crates/oasis7_viewer/viewer.html" "$expected_dir/software_safe.html"; then
  echo "expected rebuilt dist compatibility HTML to come from canonical viewer.html" >&2
  exit 1
fi
if ! cmp -s "$tmp_repo/crates/oasis7_viewer/dist/viewer.js" "$expected_dir/viewer.js"; then
  echo "expected rebuilt dist viewer.js to come from dist/viewer.js" >&2
  exit 1
fi
if ! grep -Fq 'import "./viewer.js";' "$expected_dir/software_safe.js"; then
  echo "expected rebuilt dist compatibility JS to import canonical viewer.js" >&2
  exit 1
fi
if ! grep -Fq 'src="./software_safe.html?test_api=1&connect=0"' \
  "$expected_dir/software_safe_first_agent_claim_evidence.html" \
  || ! grep -Fq 'eligible_balance_after:' "$expected_dir/software_safe_first_agent_claim_evidence.html" \
  || ! grep -Fq 'upkeep_runway_epochs:' "$expected_dir/software_safe_first_agent_claim_evidence.html"; then
  echo "expected rebuilt compatibility evidence page to preserve served route and current claim fields" >&2
  exit 1
fi

if [[ ! -f "$expected_dir/pixel-world-bridge/pixel_world_bridge.js" ]]; then
  echo "expected rebuilt dist pixel world runtime at $expected_dir/pixel-world-bridge/pixel_world_bridge.js" >&2
  exit 1
fi
if [[ ! -f "$expected_dir/pixel-world-bridge/webgl2/pixel_world_bridge.js" ]]; then
  echo "expected rebuilt dist webgl2 pixel world wrapper" >&2
  exit 1
fi
if [[ ! -f "$expected_dir/pixel-world-bridge/webgl2/pixel_world_bridge_bindgen_bg.wasm" ]]; then
  echo "expected rebuilt dist webgl2 pixel world wasm" >&2
  exit 1
fi

if [[ ! -f "$expected_dir/.oasis7-viewer-dist-manifest.json" ]]; then
  echo "expected rebuilt dist manifest at $expected_dir/.oasis7-viewer-dist-manifest.json" >&2
  exit 1
fi

if ! grep -Fq 'npm --prefix' "$tmp_repo/stderr.log"; then
  echo "expected freshness helper to trigger viewer rebuild" >&2
  cat "$tmp_repo/stderr.log" >&2
  exit 1
fi

viewer_web_dist_write_manifest "$tmp_repo" "$tmp_repo/crates/oasis7_viewer/dist"
rm -rf "$tmp_repo/crates/oasis7_viewer/dist/pixel-world-bridge"
cat > "$tmp_repo/bin/npm" <<'NPM'
#!/usr/bin/env bash
set -euo pipefail
if [[ "$1" != "--prefix" || "$3" != "run" || "$4" != "build:software-safe" ]]; then
  echo "unexpected npm args: $*" >&2
  exit 1
fi
mkdir -p crates/oasis7_viewer/dist/pixel-world-bridge
printf 'export function createPixelWorldBridge() { return "rebuilt"; }\n' \
  > crates/oasis7_viewer/dist/pixel-world-bridge/pixel_world_bridge.js
mkdir -p crates/oasis7_viewer/dist/pixel-world-bridge/webgl2
printf 'export function createPixelWorldBridge() { return "rebuilt-webgl2"; }\n' \
  > crates/oasis7_viewer/dist/pixel-world-bridge/webgl2/pixel_world_bridge.js
printf 'export function initSync() {}\n' \
  > crates/oasis7_viewer/dist/pixel-world-bridge/webgl2/pixel_world_bridge_bindgen.js
printf 'wasm\n' \
  > crates/oasis7_viewer/dist/pixel-world-bridge/webgl2/pixel_world_bridge_bindgen_bg.wasm
printf 'software safe rebuild missing bridge\n'
NPM
chmod +x "$tmp_repo/bin/npm"

resolved_missing_bridge_dir="$({ PATH="$tmp_repo/bin:$PATH" resolve_viewer_static_dir_for_web_closure "$tmp_repo" web "$tmp_repo/output/missing-bridge"; } 2>"$tmp_repo/missing-bridge-stderr.log")"
expected_missing_bridge_dir="$tmp_repo/output/missing-bridge/web-dist"

if [[ "$resolved_missing_bridge_dir" != "$expected_missing_bridge_dir" ]]; then
  echo "expected missing pixel bridge to trigger rebuilt dir '$expected_missing_bridge_dir', got '$resolved_missing_bridge_dir'" >&2
  exit 1
fi
if [[ ! -f "$expected_missing_bridge_dir/pixel-world-bridge/pixel_world_bridge.js" ]]; then
  echo "expected rebuilt missing-bridge dist pixel world runtime" >&2
  exit 1
fi
if [[ ! -f "$expected_missing_bridge_dir/pixel-world-bridge/webgl2/pixel_world_bridge.js" ]]; then
  echo "expected rebuilt missing-bridge dist webgl2 pixel world wrapper" >&2
  exit 1
fi
if [[ ! -f "$expected_missing_bridge_dir/pixel-world-bridge/webgl2/pixel_world_bridge_bindgen_bg.wasm" ]]; then
  echo "expected rebuilt missing-bridge dist webgl2 pixel world wasm" >&2
  exit 1
fi
if ! grep -Fq 'npm --prefix' "$tmp_repo/missing-bridge-stderr.log"; then
  echo "expected missing pixel bridge to trigger viewer rebuild" >&2
  cat "$tmp_repo/missing-bridge-stderr.log" >&2
  exit 1
fi

cat > "$tmp_repo/bin/npm" <<'NPM'
#!/usr/bin/env bash
set -euo pipefail
echo "simulated npm failure" >&2
exit 42
NPM
chmod +x "$tmp_repo/bin/npm"

if failing_resolved_dir="$({ PATH="$tmp_repo/bin:$PATH" resolve_viewer_static_dir_for_web_closure "$tmp_repo" web "$tmp_repo/output/failing"; } 2>"$tmp_repo/failing-stderr.log")"; then
  echo "expected freshness helper to fail when viewer rebuild fails, got '$failing_resolved_dir'" >&2
  exit 1
fi
if ! grep -Fq "viewer software-safe build failed" "$tmp_repo/failing-stderr.log"; then
  echo "expected explicit viewer build failure in stderr" >&2
  cat "$tmp_repo/failing-stderr.log" >&2
  exit 1
fi

echo "agent-browser viewer dist freshness tests passed"
