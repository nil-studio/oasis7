#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
TMP_DIR="$(mktemp -d "${TMPDIR:-/tmp}/oasis7-bundle-macos-bash3.XXXXXX")"
trap 'rm -rf "$TMP_DIR"' EXIT

# Run the real bundle script from an isolated fixture root so the regression
# exercises its packaging path without compiling or mutating the workspace
# target tree.  /bin/bash is the macOS system Bash 3.2 on the affected runner.
FIXTURE_ROOT="$TMP_DIR/repo"
mkdir -p \
  "$FIXTURE_ROOT/scripts" \
  "$FIXTURE_ROOT/bin" \
  "$FIXTURE_ROOT/target/x86_64-apple-darwin/packaging" \
  "$FIXTURE_ROOT/crates/oasis7_proto/src" \
  "$TMP_DIR/web" \
  "$TMP_DIR/web-launcher"
cp "$ROOT_DIR/scripts/build-game-launcher-bundle.sh" "$FIXTURE_ROOT/scripts/"
cp "$ROOT_DIR/scripts/bundle-freshness-lib.sh" "$FIXTURE_ROOT/scripts/"
cp "$ROOT_DIR/scripts/validate-release-platform-entrypoints.sh" "$FIXTURE_ROOT/scripts/"
cp "$ROOT_DIR/scripts/service-readback" "$FIXTURE_ROOT/scripts/"
cp "$ROOT_DIR/scripts/package-native-stage.py" "$FIXTURE_ROOT/scripts/"


cat >"$FIXTURE_ROOT/crates/oasis7_proto/src/viewer.rs" <<'EOF'
pub const VIEWER_PROTOCOL_VERSION: u32 = 1;
EOF

cat >"$TMP_DIR/web/index.html" <<'EOF'
<!doctype html><script type="module" src="./viewer.js"></script>
EOF
printf 'export const viewer = true;\n' >"$TMP_DIR/web/viewer.js"
printf 'import "./viewer.js";\n' >"$TMP_DIR/web/software_safe.js"
cat >"$TMP_DIR/web-launcher/index.html" <<'EOF'
<!doctype html><title>fixture launcher</title>
EOF

for binary in \
  oasis7_client_launcher \
  oasis7_game_launcher \
  oasis7_web_launcher \
  oasis7_viewer_live \
  oasis7_chain_runtime \
  oasis7_world_repair_rebuild \
  oasis7_governance_registry_import \
  oasis7_governance_registry_audit; do
  printf '#!/usr/bin/env bash\nexit 0\n' \
    >"$FIXTURE_ROOT/target/x86_64-apple-darwin/packaging/$binary"
  chmod +x "$FIXTURE_ROOT/target/x86_64-apple-darwin/packaging/$binary"
done

# The fixture cargo command only proves that the script reaches the staged
# artifacts; no compiler or network work is performed by this test.
cat >"$FIXTURE_ROOT/bin/cargo" <<'EOF'
#!/usr/bin/env bash
exit 0
EOF
chmod +x "$FIXTURE_ROOT/bin/cargo"
export PATH="$FIXTURE_ROOT/bin:$PATH"

# Shadow mapfile in every non-interactive Bash launched by the fixture.  This
# makes the macOS system-Bash constraint deterministic even when the test is
# run on a host whose default Bash is newer.
cat >"$TMP_DIR/bash-without-mapfile.env" <<'EOF'
mapfile() {
  echo "mapfile: command not found" >&2
  return 127
}
EOF
export BASH_ENV="$TMP_DIR/bash-without-mapfile.env"

BUNDLE_DIR="$TMP_DIR/bundle"
OPS_DIR="$TMP_DIR/ops"
/bin/bash "$FIXTURE_ROOT/scripts/build-game-launcher-bundle.sh" \
  --profile packaging \
  --target-triple x86_64-apple-darwin \
  --out-dir "$BUNDLE_DIR" \
  --ops-out-dir "$OPS_DIR" \
  --web-dist "$TMP_DIR/web" \
  --web-launcher-dist "$TMP_DIR/web-launcher"

test -s "$OPS_DIR/SHA256SUMS"
(cd "$OPS_DIR" && shasum -a 256 -c SHA256SUMS >/dev/null)
test -x "$BUNDLE_DIR/oasis7 Client Launcher.app/Contents/MacOS/oasis7-client-launcher"

# Exercise the actual native -> archive -> assembly path, including ops tools.
git -C "$FIXTURE_ROOT" init -q
git -C "$FIXTURE_ROOT" add scripts crates
git -C "$FIXTURE_ROOT" -c user.name=fixture -c user.email=fixture@example.invalid commit -qm fixture
/bin/bash "$FIXTURE_ROOT/scripts/build-game-launcher-bundle.sh" \
  --profile packaging --target-triple x86_64-apple-darwin --native-only \
  --out-dir "$TMP_DIR/native" --ops-out-dir "$TMP_DIR/native/ops" >"$TMP_DIR/stage.log" 2>&1 || { cat "$TMP_DIR/stage.log"; exit 1; }
tar -C "$TMP_DIR/native" -czf "$TMP_DIR/native.tar.gz" .
mkdir "$TMP_DIR/restored"
tar -C "$TMP_DIR/restored" -xzf "$TMP_DIR/native.tar.gz"
printf '#!/usr/bin/env bash\nexit 77\n' >"$FIXTURE_ROOT/bin/cargo"
/bin/bash "$FIXTURE_ROOT/scripts/build-game-launcher-bundle.sh" \
  --profile packaging --platform macos-x64 --target-triple x86_64-apple-darwin \
  --native-binaries "$TMP_DIR/restored" --out-dir "$TMP_DIR/assembled/oasis7-macos-x64" \
  --ops-out-dir "$TMP_DIR/assembled/oasis7-macos-x64-ops-tools" \
  --web-dist "$TMP_DIR/web" --web-launcher-dist "$TMP_DIR/web-launcher" \
  >"$TMP_DIR/assemble.log" 2>&1 || { cat "$TMP_DIR/assemble.log"; exit 1; }
tar -C "$TMP_DIR/assembled" -czf "$TMP_DIR/ops.tar.gz" oasis7-macos-x64-ops-tools
test -s "$TMP_DIR/ops.tar.gz"
(cd "$TMP_DIR/assembled/oasis7-macos-x64-ops-tools" && shasum -a 256 -c SHA256SUMS >/dev/null)
cmp "$TMP_DIR/restored/bin/oasis7_chain_runtime" "$TMP_DIR/assembled/oasis7-macos-x64/bin/oasis7_chain_runtime"
echo "build-game-launcher-bundle-macos-bash3.test: OK"
