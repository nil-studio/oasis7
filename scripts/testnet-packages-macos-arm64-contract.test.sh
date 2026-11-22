#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORKFLOW="$ROOT_DIR/.github/workflows/testnet-packages.yml"

python3 - "$WORKFLOW" <<'PY'
from pathlib import Path
import json
import re
import sys

workflow = Path(sys.argv[1]).read_text(encoding="utf-8")
assert "INPUT_SCOPE: ${{ inputs.package_scope }}" in workflow, "package scope must enter shell through env"
plan = re.search(
    r"case \"\$\{INPUT_SCOPE\}\" in(?P<body>.*?)^          esac",
    workflow,
    re.MULTILINE | re.DOTALL,
)
assert plan, "missing Testnet Packages package-scope matrix"
matrix = plan.group("body")


def scope_entries(scope: str) -> list[dict[str, str]]:
    entry = re.search(
        rf"^            {re.escape(scope)}\)\n"
        r"              matrix='(?P<matrix>[^']+)'",
        matrix,
        re.MULTILINE,
    )
    assert entry, f"missing package scope: {scope}"
    return json.loads(entry.group("matrix"))["include"]


assert scope_entries("linux_macos_x64") == [
    {
        "os": "ubuntu-24.04",
        "platform": "linux-x64",
        "asset_name": "oasis7-linux-x64.deb",
        "target_triple": "native",
    },
    {
        "os": "macos-14",
        "platform": "macos-x64",
        "asset_name": "oasis7-macos-x64.dmg",
        "target_triple": "x86_64-apple-darwin",
    },
], "linux_macos_x64 must remain the exact existing x64 contract"
assert scope_entries("all_existing") == [
    {
        "os": "ubuntu-24.04",
        "platform": "linux-x64",
        "asset_name": "oasis7-linux-x64.deb",
        "target_triple": "native",
    },
    {
        "os": "macos-14",
        "platform": "macos-x64",
        "asset_name": "oasis7-macos-x64.dmg",
        "target_triple": "x86_64-apple-darwin",
    },
    {
        "os": "windows-2022",
        "platform": "windows-x64",
        "asset_name": "oasis7-windows-x64.exe",
        "target_triple": "native",
    },
], "all_existing must remain the exact existing x64/Windows contract"

assert scope_entries("linux_macos_arm64") == [
    {
        "os": "ubuntu-24.04",
        "platform": "linux-x64",
        "asset_name": "oasis7-linux-x64.deb",
        "target_triple": "native",
    },
    {
        "os": "macos-14",
        "platform": "macos-arm64",
        "asset_name": "oasis7-macos-arm64.dmg",
        "target_triple": "aarch64-apple-darwin",
    },
], "linux_macos_arm64 must add the native Apple Silicon package contract"

validation = re.search(
    r"^      - name: Validate bundle entrypoints\n(?P<body>.*?)(?=^      - name: |\Z)",
    workflow,
    re.MULTILINE | re.DOTALL,
)
assert validation, "missing bundle entrypoint validation step"
assert "./scripts/validate-release-platform-entrypoints.sh" in validation.group("body"), (
    "Testnet Packages must call the shared release entrypoint validator"
)
PY

# Use real Mach-O headers and real file(1), not a mocked classification string.
# Keep the bundle path named macos-arm64 so file(1)'s filename prefix cannot
# satisfy the expected architecture for the wrong payload.
TMP_DIR="$(mktemp -d "${TMPDIR:-/tmp}/oasis7-package-arm64.XXXXXX")"
trap 'rm -rf "$TMP_DIR"' EXIT
bundle="$TMP_DIR/macos-arm64"
python3 - "$bundle" <<'PY'
import struct
import sys
from pathlib import Path

bundle = Path(sys.argv[1])
for rel, contents in {
    "web/index.html": '<script src="./viewer.js"></script>',
    "web/viewer.js": "/* viewer */",
    "web/software_safe.js": 'import "./viewer.js";',
    "web-launcher/index.html": '<html></html>',
    "README.txt": "entrypoint fixture",
    ".oasis7-bundle-manifest.json": "{}",
    "oasis7 Client Launcher.app/Contents/Info.plist": "",
}.items():
    path = bundle / rel
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(contents, encoding="utf-8")
for rel in (
    "run-client.sh", "run-web-launcher.sh", "run-game.sh", "run-chain-runtime.sh",
    "bin/oasis7_client_launcher", "bin/oasis7_game_launcher",
    "bin/oasis7_web_launcher", "bin/oasis7_viewer_live", "bin/oasis7_chain_runtime",
    "oasis7 Client Launcher.app/Contents/MacOS/oasis7-client-launcher",
):
    path = bundle / rel
    path.parent.mkdir(parents=True, exist_ok=True)
    path.touch()
    path.chmod(0o755)
for name, cpu, subtype in (("arm64", 0x0100000C, 0), ("x64", 0x01000007, 3)):
    # mach_header_64, with MH_EXECUTE and no load commands; never executed.
    (bundle / f"{name}.header").write_bytes(
        struct.pack("<8I", 0xFEEDFACF, cpu, subtype, 2, 0, 0, 0, 0)
    )
PY
validator="$ROOT_DIR/scripts/validate-release-platform-entrypoints.sh"
cat "$bundle/arm64.header" >"$bundle/bin/oasis7_chain_runtime"
"$validator" --platform macos-arm64 --bundle-dir "$bundle"
cat "$bundle/x64.header" >"$bundle/bin/oasis7_chain_runtime"
if "$validator" --platform macos-arm64 --bundle-dir "$bundle" >"$TMP_DIR/wrong-arch.log" 2>&1; then
  echo "error: x86_64 runtime accepted in the macos-arm64 bundle" >&2
  exit 1
fi
grep -Fq 'macos-arm64 runtime is not Mach-O arm64' "$TMP_DIR/wrong-arch.log"
echo "ok: macOS matrix contract and actual arm64 entrypoint validation"
