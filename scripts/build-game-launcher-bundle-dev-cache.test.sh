#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
mkdir -p "$TMP/repo/scripts" "$TMP/repo/crates/oasis7_proto/src" "$TMP/cache/debug" "$TMP/web" "$TMP/launcher"
cp "$ROOT/scripts/"{build-game-launcher-bundle.sh,bundle-freshness-lib.sh,validate-release-platform-entrypoints.sh,package-native-stage.py} "$TMP/repo/scripts/"
printf 'pub const VIEWER_PROTOCOL_VERSION: u32 = 1;\n' >"$TMP/repo/crates/oasis7_proto/src/viewer.rs"
printf '<!doctype html><script type="module" src="./viewer.js"></script>\n' >"$TMP/web/index.html"
printf 'export const viewer = true;\n' >"$TMP/web/viewer.js"
printf 'import "./viewer.js";\n' >"$TMP/web/software_safe.js"
printf '<!doctype html>\n' >"$TMP/launcher/index.html"
for binary in oasis7_game_launcher oasis7_web_launcher oasis7_viewer_live oasis7_chain_runtime oasis7_client_launcher; do
  printf '#!/usr/bin/env bash\nexit 0\n' >"$TMP/cache/debug/$binary"
  chmod +x "$TMP/cache/debug/$binary"
done
# Target has no source-side symlink or ordinary target; only helper resolution
# can find these artifacts. No operator binaries exist in this fixture.
cat >"$TMP/repo/scripts/cargo-dev-lib.sh" <<'LIB'
oasis7_cargo_dev_target_dir() { printf '%s\n' "$TEST_CACHE"; }
oasis7_cargo_dev() { printf '%s\n' "$*" >>"$TEST_CALLS"; }
LIB
TEST_CACHE="$TMP/cache" TEST_CALLS="$TMP/calls" bash "$TMP/repo/scripts/build-game-launcher-bundle.sh" \
  --profile dev --platform linux-x64 --target-triple x86_64-unknown-linux-gnu \
  --out-dir "$TMP/output" --web-dist "$TMP/web" --web-launcher-dist "$TMP/launcher" >"$TMP/log" 2>&1 && {
    echo 'cross-target fixture should require target-specific artifacts' >&2; exit 1;
  }
mkdir -p "$TMP/cache/x86_64-unknown-linux-gnu"
mv "$TMP/cache/debug" "$TMP/cache/x86_64-unknown-linux-gnu/debug"
TEST_CACHE="$TMP/cache" TEST_CALLS="$TMP/calls" bash "$TMP/repo/scripts/build-game-launcher-bundle.sh" \
  --profile dev --platform linux-x64 --target-triple x86_64-unknown-linux-gnu \
  --out-dir "$TMP/output" --web-dist "$TMP/web" --web-launcher-dist "$TMP/launcher" >"$TMP/log" 2>&1 || { cat "$TMP/log" >&2; exit 1; }
test -x "$TMP/output/bin/oasis7_client_launcher"
grep -Fq 'build --target x86_64-unknown-linux-gnu' "$TMP/calls"
if grep -Eq 'world_repair|registry_import|registry_audit' "$TMP/calls"; then exit 1; fi
# Native compilation can finish without Web inputs. Assembly must consume the exact
# stage without invoking Cargo, and must reject changed bytes/build tuples.
git -C "$TMP/repo" init -q
git -C "$TMP/repo" add scripts crates
git -C "$TMP/repo" -c user.name=fixture -c user.email=fixture@example.invalid commit -qm fixture
TEST_CACHE="$TMP/cache" TEST_CALLS="$TMP/calls" bash "$TMP/repo/scripts/build-game-launcher-bundle.sh" \
  --profile dev --platform linux-x64 --target-triple x86_64-unknown-linux-gnu \
  --out-dir "$TMP/native" --native-only >"$TMP/log" 2>&1 || { cat "$TMP/log" >&2; exit 1; }
test -f "$TMP/native/native-build.json"
test ! -f "$TMP/native/web/index.html"
cp "$TMP/calls" "$TMP/calls-before"
TEST_CACHE="$TMP/cache" TEST_CALLS="$TMP/calls" bash "$TMP/repo/scripts/build-game-launcher-bundle.sh" \
  --profile dev --platform linux-x64 --target-triple x86_64-unknown-linux-gnu \
  --native-binaries "$TMP/native" --out-dir "$TMP/assembled" \
  --web-dist "$TMP/web" --web-launcher-dist "$TMP/launcher" >"$TMP/log" 2>&1 || { cat "$TMP/log" >&2; exit 1; }
cmp "$TMP/calls-before" "$TMP/calls"
cmp "$TMP/native/bin/oasis7_viewer_live" "$TMP/assembled/bin/oasis7_viewer_live"
python3 "$ROOT/scripts/package-native-stage.test.py"
echo 'build-game-launcher-bundle-dev-cache.test: OK'
