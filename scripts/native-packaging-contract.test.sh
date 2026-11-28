#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
TMP_DIR="$(mktemp -d "${TMPDIR:-/tmp}/oasis7-native-packaging-contract.XXXXXX")"
trap 'rm -rf "$TMP_DIR"' EXIT

bundle="$TMP_DIR/bundle"
mkdir -p \
  "$bundle/bin" \
  "$bundle/web"
printf '<!doctype html><script type="module" src="./viewer.js"></script>\n' >"$bundle/web/index.html"
printf 'console.log("canonical viewer");\n' >"$bundle/web/viewer.js"
printf 'import "./viewer.js";\n' >"$bundle/web/software_safe.js"
printf 'contract fixture\n' >"$bundle/README.txt"
printf '{"schema":1}\n' >"$bundle/.oasis7-bundle-manifest.json"
for launcher in run-client.sh run-web-launcher.sh run-game.sh run-chain-runtime.sh; do
  printf '#!/usr/bin/env bash\nexit 0\n' >"$bundle/$launcher"
  chmod +x "$bundle/$launcher"
done
for binary in oasis7_game_launcher oasis7_web_launcher oasis7_viewer_live oasis7_chain_runtime; do
  printf '#!/usr/bin/env bash\nexit 0\n' >"$bundle/bin/$binary"
  chmod +x "$bundle/bin/$binary"
done

"$ROOT_DIR/scripts/validate-release-platform-entrypoints.sh" \
  --platform linux-x64 \
  --bundle-dir "$bundle" >"$TMP_DIR/validate.stdout"
grep -Fq "Release bundle entrypoints validated" "$TMP_DIR/validate.stdout"

set +e
rm "$bundle/run-game.sh"
"$ROOT_DIR/scripts/validate-release-platform-entrypoints.sh" \
  --platform linux-x64 \
  --bundle-dir "$bundle" >"$TMP_DIR/validate-missing.stdout" 2>"$TMP_DIR/validate-missing.stderr"
missing_status=$?
set -e
test "$missing_status" -ne 0
grep -Fq "run-game.sh" "$TMP_DIR/validate-missing.stderr"
printf '#!/usr/bin/env bash\nexit 0\n' >"$bundle/run-game.sh"
chmod +x "$bundle/run-game.sh"

dry_run_output="$($ROOT_DIR/scripts/package-native-installer.sh \
  --platform linux-x64 \
  --bundle-dir "$bundle" \
  --out-dir "$TMP_DIR/out" \
  --asset-name oasis7-linux-x64.deb \
  --version v1.2.3 \
  --dry-run)"
grep -Fq "dpkg-deb --build" <<<"$dry_run_output"
grep -Fq "oasis7-linux-x64.deb" <<<"$dry_run_output"
if grep -Eiq '(^|[[:space:];|&()])(cargo|rustup)([[:space:]]|$)' <<<"$dry_run_output"; then
  echo "native packaging dry-run unexpectedly invokes Rust tooling" >&2
  exit 1
fi

set +e
"$ROOT_DIR/scripts/package-native-installer.sh" \
  --platform linux-x64 \
  --bundle-dir "$bundle" \
  --out-dir "$TMP_DIR/out" \
  --asset-name oasis7-linux-x64.tar.gz \
  --version v1.2.3 \
  --dry-run >"$TMP_DIR/asset.stdout" 2>"$TMP_DIR/asset.stderr"
asset_status=$?
set -e
test "$asset_status" -ne 0
grep -Fq "linux-x64 asset must end with .AppImage or .deb" "$TMP_DIR/asset.stderr"

echo "native-packaging-contract.test: OK"
