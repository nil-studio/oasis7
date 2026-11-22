#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORKFLOW="$ROOT_DIR/.github/workflows/testnet-packages.yml"

python3 - "$WORKFLOW" <<'PY'
from pathlib import Path
import re
import sys

workflow = Path(sys.argv[1]).read_text(encoding="utf-8")


def step(name: str) -> tuple[int, str]:
    match = re.search(
        rf"^      - name: {re.escape(name)}\n(?P<body>.*?)(?=^      - name: |\Z)",
        workflow,
        re.MULTILINE | re.DOTALL,
    )
    assert match, f"missing Testnet Packages workflow step: {name}"
    return match.start(), match.group("body")


buildinfo_offset, buildinfo = step("Write BUILDINFO")
package_offset, _ = step("Package native installer")
_, outer_checksums = step("Generate checksums")
assert buildinfo_offset < package_offset, (
    "BUILDINFO must be staged before package-native-installer runs"
)
assert '"platform":"linux-x64"' in workflow
assert '"asset_name":"oasis7-linux-x64.deb"' in workflow, (
    "Linux package must publish the Debian installer as the sole primary asset"
)
assert "AppImage" not in workflow, "Linux AppImage must not remain a published artifact"
assert "Archive raw Linux bundle" not in workflow, "raw Linux tar must not remain a release artifact"
assert "Package secondary Linux .deb" not in workflow, "duplicate Linux deb step must be removed"
assert 'buildinfo="${assets}/${{ matrix.platform }}-BUILDINFO"' in buildinfo, (
    "external Linux BUILDINFO artifact must remain available for outer checksums"
)
assert '"${{ matrix.platform }}-BUILDINFO"' in outer_checksums, (
    "outer artifact checksums must continue to cover BUILDINFO"
)
assert 'cp "${buildinfo}" "${bundle}/BUILDINFO"' in buildinfo, (
    "Linux BUILDINFO must be copied into the Debian payload"
)
assert 'sha256sum "${files[@]}" > SHA256SUMS' in buildinfo or \
    'shasum -a 256 "${files[@]}" > SHA256SUMS' in buildinfo, (
    "Linux bundle must generate an internal SHA256SUMS before packaging"
)
assert 'shasum -a 256 -c SHA256SUMS' in buildinfo, (
    "Linux bundle metadata must be self-verified before packaging"
)
assert "ops package" in workflow.lower(), "Linux workflow must publish a separate ops package"
assert "oasis7-${{ matrix.platform }}-ops-tools.tar.gz" in workflow, (
    "Linux workflow must upload the checksummed ops-tools archive"
)
PY

TMP_DIR="$(mktemp -d "${TMPDIR:-/tmp}/oasis7-linux-bundle-contract.XXXXXX")"
trap 'rm -rf "$TMP_DIR"' EXIT
bundle="$TMP_DIR/oasis7-linux-x64-ops-tools"
mkdir -p "$bundle/bin"
printf 'repair\n' >"$bundle/bin/oasis7_world_repair_rebuild"
printf 'registry-import\n' >"$bundle/bin/oasis7_governance_registry_import"
printf 'registry-audit\n' >"$bundle/bin/oasis7_governance_registry_audit"
printf '{"opsToolsSchemaVersion":1}\n' >"$bundle/.oasis7-ops-tools-manifest.json"
(
  cd "$bundle"
  files=()
  while IFS= read -r file; do
    files+=("$file")
  done < <(find . -type f ! -name SHA256SUMS -print | sort)
  shasum -a 256 "${files[@]}" > SHA256SUMS
  shasum -a 256 -c SHA256SUMS >/dev/null
)
archive="$TMP_DIR/oasis7-linux-x64-ops-tools.tar.gz"
members="$TMP_DIR/ops-archive-members.txt"
tar -C "$TMP_DIR" -czf "$archive" oasis7-linux-x64-ops-tools
# Under pipefail, grep -q closes early and can make a still-writing tar fail with SIGPIPE.
# Verify the full archive exactly once, then check the required members from that file.
tar -tzf "$archive" >"$members"
grep -Fxq 'oasis7-linux-x64-ops-tools/.oasis7-ops-tools-manifest.json' "$members"
grep -Fxq 'oasis7-linux-x64-ops-tools/SHA256SUMS' "$members"

echo "ok: Linux workflow contract and synthetic ops-tools archive fixture"
if [[ "$(uname -s)" != Linux ]]; then
  echo "SKIP: synthetic Debian payload verifier fixture runs on Linux only"
  exit 0
fi
if ! command -v dpkg-deb >/dev/null 2>&1; then
  echo "error: Linux requires dpkg-deb to run the Debian payload verifier fixture" >&2
  exit 1
fi

# This fixture exercises the verifier against an actual DEB container; it
# is not evidence for a package built by the release pipeline.
deb_root="$TMP_DIR/deb-root"
mkdir -p "$deb_root/DEBIAN" "$deb_root/opt/oasis7/bin"
printf 'Package: oasis7\nVersion: 0.0.0\nArchitecture: amd64\nDescription: contract fixture\n' \
  >"$deb_root/DEBIAN/control"
printf 'workflow=Testnet Packages\ncommit=abcdef1234567890abcdef1234567890abcdef12\npackage_version=0.0.0\nrun_id=1\nplatform=linux-x64\n' \
  >"$deb_root/opt/oasis7/BUILDINFO"
printf '#!/usr/bin/env bash\n' >"$deb_root/opt/oasis7/bin/oasis7_chain_runtime"
chmod +x "$deb_root/opt/oasis7/bin/oasis7_chain_runtime"
(
  cd "$deb_root/opt/oasis7"
  sha256sum bin/oasis7_chain_runtime BUILDINFO >SHA256SUMS
)
dpkg-deb --build --root-owner-group "$deb_root" "$TMP_DIR/oasis7-linux-x64.deb" >/dev/null
extract_dir="$TMP_DIR/deb-extract"
dpkg-deb --extract "$TMP_DIR/oasis7-linux-x64.deb" "$extract_dir"
test -f "$extract_dir/opt/oasis7/BUILDINFO"
test -f "$extract_dir/opt/oasis7/SHA256SUMS"
(cd "$extract_dir/opt/oasis7" && sha256sum -c SHA256SUMS >/dev/null)

verifier="$ROOT_DIR/scripts/p2p-verify-linux-package-bundle.py"
bundle="$extract_dir/opt/oasis7"
python3 "$verifier" "$bundle" 0.0.0 abcdef1234567890abcdef1234567890abcdef12 1

# RED/GREEN regression: a regular payload file that is deployed by the
# upgrader must not be accepted when it is absent from SHA256SUMS.
printf 'unlisted payload\n' >"$bundle/bin/UNLISTED"
set +e
python3 "$verifier" "$bundle" 0.0.0 abcdef1234567890abcdef1234567890abcdef12 1 \
  >"$TMP_DIR/unlisted.stdout" 2>"$TMP_DIR/unlisted.stderr"
unlisted_status=$?
set -e
test "$unlisted_status" -ne 0
grep -q "SHA256SUMS does not cover bundle files: bin/UNLISTED" "$TMP_DIR/unlisted.stderr"

# BUILDINFO is also a rollout input to fresh-host bootstrap.  Reject an
# unsafe version before any caller can interpolate it into releases/.
sed -i 's/^package_version=.*/package_version=..\/outside/' "$bundle/BUILDINFO"
(cd "$bundle" && sha256sum bin/oasis7_chain_runtime BUILDINFO >SHA256SUMS)
set +e
python3 "$verifier" "$bundle" ../outside abcdef1234567890abcdef1234567890abcdef12 1 \
  >"$TMP_DIR/version.stdout" 2>"$TMP_DIR/version.stderr"
version_status=$?
set -e
test "$version_status" -ne 0
grep -q "safe single path token" "$TMP_DIR/version.stderr"

echo "ok: synthetic Debian payload extraction and verifier cases"
