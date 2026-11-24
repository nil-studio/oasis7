#!/usr/bin/env bash
set -euo pipefail

if ! command -v shasum >/dev/null 2>&1; then
  shasum() {
    [[ "${1:-}" == "-a" && "${2:-}" == "256" ]] || {
      echo "test shasum shim supports only -a 256" >&2
      return 2
    }
    shift 2
    sha256sum "$@"
  }
fi

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TMP_DIR="$(mktemp -d "${TMPDIR:-/tmp}/oasis7-package-rollout-test.XXXXXX")"
trap 'rm -rf "$TMP_DIR"' EXIT

package_dir="$TMP_DIR/package"
bundle_src="$TMP_DIR/bundle/oasis7-linux-x64"
node_root="$TMP_DIR/node"
out_dir="$TMP_DIR/out"
package_version="0.0.0+testnet.89.419e119bc897"
commit="419e119bc897efaa34750bee04c63470d1156699"
run_id="27605906795"
macos_package_version="0.0.0+testnet.90.419e119bc897"
macos_run_id="27605906796"
checkpoint_closure_receipt="$TMP_DIR/observer-checkpoint-closure-receipt.json"

if ! command -v dpkg-deb >/dev/null 2>&1; then
  mkdir -p "$TMP_DIR/fake-bin"
  cat >"$TMP_DIR/fake-bin/dpkg-deb" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
case "${1:-}" in
  --build)
    source="${@: -2:1}"
    output="${@: -1}"
    mkdir -p "${output}.root"
    cp -a "$source/." "${output}.root/"
    : >"$output"
    ;;
  --extract)
    source="${2:?missing source}"
    destination="${3:?missing destination}"
    mkdir -p "$destination"
    cp -a "${source}.root/." "$destination/"
    ;;
  *)
    echo "unsupported dpkg-deb fixture operation" >&2
    exit 2
    ;;
esac
SH
  chmod +x "$TMP_DIR/fake-bin/dpkg-deb"
  export PATH="$TMP_DIR/fake-bin:$PATH"
fi

cat >"$checkpoint_closure_receipt" <<'EOF'
{
  "schema_version": "oasis7.observer_checkpoint_closure_receipt.v1",
  "checkpoint_id": "fixture-checkpoint-v2",
  "manifest_hash": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
  "height": 4242,
  "clean_state": true,
  "providers": [
    {
      "provider_id": "storage",
      "authorized": true,
      "connected": true,
      "complete": true,
      "hash_size_binding_verified": true,
      "missing_hashes": []
    }
  ]
}
EOF

# Test-only in-process injection.  The released rollout CLI has no hook for a
# caller-selected probe command; this driver substitutes only after importing
# the module in this test process.
rollout_driver="$TMP_DIR/rollout-test-driver.py"
cat >"$rollout_driver" <<EOF
#!/usr/bin/env python3
import importlib.util
import json
import os
import subprocess
import sys
from pathlib import Path

module_path = Path(os.environ.get(
    "OASIS7_TEST_ROLLOUT_MODULE_PATH",
    r'''$ROOT_DIR/scripts/p2p-public-testnet-package-rollout.py''',
))
spec = importlib.util.spec_from_file_location("rollout", module_path)
rollout = importlib.util.module_from_spec(spec)
assert spec.loader is not None
spec.loader.exec_module(rollout)

def fixture_probe(manifest, package_dir, out_dir):
    marker = os.environ.get("OASIS7_TEST_INPROCESS_PROBE_MARKER")
    if marker:
        Path(marker).write_text("started\n", encoding="utf-8")
    output = out_dir / "checkpoint-closure-probe.json"
    subprocess.run([
        sys.executable,
        str(module_path.parent / "p2p-observer-checkpoint-closure-probe.test.py"),
        "--manifest", str(manifest), "--package-dir", str(package_dir), "--out", str(output),
    ], check=True)
    return rollout.load_probe_result(manifest, package_dir, output)

rollout.run_checkpoint_closure_probe = fixture_probe
sys.argv = [str(module_path), *sys.argv[1:]]
raise SystemExit(rollout.main())
EOF
chmod +x "$rollout_driver"
export OASIS7_TEST_CLOSURE_FIXTURE="$checkpoint_closure_receipt"

# All positive rollout fixtures share one operator-local known-hosts pin.  Keep
# the path centralized so every plan exercises the same single-auth boundary;
# the negative missing/directory cases below intentionally override it.
operator_known_hosts="$TMP_DIR/operator pinned known-hosts"
printf '%s\n' 'fixture ssh-ed25519 AAAAoperatorlocalpin' >"$operator_known_hosts"

mkdir -p "$package_dir/windows" "$package_dir/macos" "$bundle_src/bin" "$node_root/releases/old/bin" "$node_root/config/doc/testing/evidence"
cat >"$bundle_src/bin/oasis7_chain_runtime" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
replication_root=''
world_id=''
status_bind=''
while (($#)); do
  case "$1" in
    --replication-root) replication_root="$2"; shift 2 ;;
    --world-id) world_id="$2"; shift 2 ;;
    --status-bind) status_bind="$2"; shift 2 ;;
    *) shift ;;
  esac
done
[[ -n "$replication_root" && -n "$world_id" ]]
[[ "$status_bind" =~ ^127\.0\.0\.1:[1-9][0-9]*$ ]] || {
  echo "fixture runtime requires a non-zero loopback status port: $status_bind" >&2
  exit 2
}
mkdir -p "$replication_root/checkpoint-verification"
cat >"$replication_root/checkpoint-verification/4242.json" <<JSON
{"schema_version":"oasis7.checkpoint_closure_verification_receipt.v1","world_id":"$world_id","probe_nonce":"$OASIS7_CHECKPOINT_PROBE_NONCE","height":4242,"execution_block_hash":"fixture-checkpoint-v2","execution_state_root":"fixture-state-root","manifest_hash":"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef","objects":[{"expected_content_hash":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","observed_content_hash":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","expected_size_bytes":7,"observed_size_bytes":7}],"fetch_observations":[{"source":"network_fetch","content_hash":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","observed_content_hash":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","observed_size_bytes":7,"response_found":true,"signed_request":true,"connected_candidate_ids":["fixture-provider"]}]}
JSON
trap 'exit 0' TERM INT
while :; do sleep 1; done
EOF
chmod +x "$bundle_src/bin/oasis7_chain_runtime"
cat >"$bundle_src/BUILDINFO" <<EOF
workflow=Testnet Packages
run_id=$run_id
run_number=89
repository=eng-cc/oasis7
requested_ref=$commit
commit=$commit
build_profile=release
package_scope=all_existing
platform=linux-x64
package_version=$package_version
published=false
EOF
(cd "$bundle_src" && shasum -a 256 BUILDINFO bin/oasis7_chain_runtime >SHA256SUMS)
linux_deb_root="$TMP_DIR/linux-deb-root"
mkdir -p "$linux_deb_root/DEBIAN" "$linux_deb_root/opt/oasis7"
cp -a "$bundle_src/." "$linux_deb_root/opt/oasis7/"
cat >"$linux_deb_root/DEBIAN/control" <<'EOF'
Package: oasis7
Version: 0.0.0
Section: games
Priority: optional
Architecture: amd64
Description: package rollout Linux fixture
EOF
dpkg-deb --build --root-owner-group "$linux_deb_root" "$package_dir/oasis7-linux-x64.deb" >/dev/null
make_ops_tools_archive() {
  local platform="$1" platform_dir="$2"
  local ops_root="$TMP_DIR/ops-bundle/oasis7-${platform}-ops-tools"
  mkdir -p "$ops_root/bin"
  for binary in oasis7_world_repair_rebuild oasis7_governance_registry_import oasis7_governance_registry_audit service-readback; do
    printf '#!/usr/bin/env bash\n' >"$ops_root/bin/$binary"
    chmod +x "$ops_root/bin/$binary"
  done
  printf '{"opsToolsSchemaVersion":1}\n' >"$ops_root/.oasis7-ops-tools-manifest.json"
  (
    cd "$ops_root"
    shasum -a 256 .oasis7-ops-tools-manifest.json bin/* >SHA256SUMS
  )
  tar -czf "$platform_dir/oasis7-${platform}-ops-tools.tar.gz" \
    -C "$TMP_DIR/ops-bundle" "oasis7-${platform}-ops-tools"
  (
    cd "$platform_dir"
    shasum -a 256 "oasis7-${platform}-ops-tools.tar.gz" \
      >"${platform}-ops-tools-SHA256SUMS"
  )
}

make_ops_tools_archive linux-x64 "$package_dir"
printf 'fake windows installer\n' >"$package_dir/windows/oasis7-windows-x64.exe"
printf 'fake macos arm64 dmg\n' >"$package_dir/macos/oasis7-macos-arm64.dmg"
make_ops_tools_archive windows-x64 "$package_dir/windows"
make_ops_tools_archive macos-arm64 "$package_dir/macos"

windows_bundle="$package_dir/windows/public-testnet-governed-bootstrap-bundle-2026-06-06.windows.json"
windows_genesis="$package_dir/windows/public-testnet-governed-bootstrap-genesis-2026-06-06.windows.json"
windows_manifest="$package_dir/windows/public-testnet-governed-bootstrap-manifest-2026-06-06.windows.json"
windows_bootstrap="$package_dir/windows/public-testnet-governed-bootstrap-bootstrap-peers-2026-06-06.windows.txt"
windows_evidence_dir="$package_dir/windows/doc/testing/evidence"
windows_world_dir="$package_dir/windows/generated-world/world"
windows_sidecar_dir="$package_dir/windows/generated-world/generated-scenario-world"
windows_provenance="$package_dir/windows/generated-world/world-generation-provenance.json"
mkdir -p "$windows_evidence_dir" "$windows_world_dir" "$windows_sidecar_dir/chunks"
for evidence_name in \
  governance-public-signers.json \
  liveops-public-signers.json \
  signer-truth-binding.md \
  genesis-validator-registry.json \
  governed-bootstrap-topology.md; do
  printf 'windows governed evidence: %s\n' "$evidence_name" >"$windows_evidence_dir/$evidence_name"
done
printf '{"height":17,"world_id":"windows-fixture"}\n' >"$windows_world_dir/snapshot.json"
printf '{"events":["bootstrap"]}\n' >"$windows_world_dir/journal.json"
printf '{"scenario_id":"asteroid_fragment_bootstrap"}\n' >"$windows_sidecar_dir/snapshot.json"
printf '{"entries":[1,2]}\n' >"$windows_sidecar_dir/journal.json"
printf '{"chunk":"alpha"}\n' >"$windows_sidecar_dir/chunks/0001.json"
printf '{"scenario_id":"asteroid_fragment_bootstrap","seed":7}\n' >"$windows_provenance"
printf '/ip4/192.0.2.10/tcp/6831/p2p/12D3KooWWindowsFixture\n' >"$windows_bootstrap"
cat >"$windows_genesis" <<'EOF'
{
  "schema_version": "test",
  "operator_note": "historical build note mentioned /Users/build/oasis7 but is not a path-bearing field",
  "governance_bootstrap_refs": {
    "governance_public_manifest_ref": "doc/testing/evidence/governance-public-signers.json",
    "liveops_public_manifest_ref": "doc/testing/evidence/liveops-public-signers.json",
    "binding_notes_ref": "doc/testing/evidence/signer-truth-binding.md",
    "genesis_validator_registry_ref": "doc/testing/evidence/genesis-validator-registry.json",
    "topology_ref": "doc/testing/evidence/governed-bootstrap-topology.md"
  }
}
EOF

python3 - \
  "$package_dir/windows" \
  "$windows_bundle" \
  "$windows_manifest" \
  "$windows_genesis" \
  "$windows_bootstrap" \
  "$windows_world_dir" \
  "$windows_sidecar_dir" \
  "$windows_provenance" \
  "$windows_evidence_dir/genesis-validator-registry.json" \
  "$windows_evidence_dir/governed-bootstrap-topology.md" \
  "$package_dir/windows/oasis7-windows-x64.exe" <<'PY'
from pathlib import Path
import hashlib
import json
import sys

(
    platform_dir_raw,
    bundle_raw,
    manifest_raw,
    genesis_raw,
    bootstrap_raw,
    world_raw,
    sidecar_raw,
    provenance_raw,
    governance_raw,
    topology_raw,
    installer_raw,
) = sys.argv[1:]
platform_dir = Path(platform_dir_raw)
build_root = r"D:\\a\\oasis7\\oasis7\\public-testnet-stage"


def file_sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def tree_metadata(path: Path) -> dict[str, object]:
    digest = hashlib.sha256()
    files = sorted(item for item in path.rglob("*") if item.is_file())
    total_bytes = 0
    for item in files:
        relative = item.relative_to(path).as_posix()
        payload = item.read_bytes()
        total_bytes += len(payload)
        digest.update(relative.encode("utf-8"))
        digest.update(b"\0")
        digest.update(hashlib.sha256(payload).hexdigest().encode("ascii"))
        digest.update(b"\0")
        digest.update(str(len(payload)).encode("ascii"))
        digest.update(b"\n")
    return {
        "kind": "directory",
        "sha256_tree": digest.hexdigest(),
        "file_count": len(files),
        "total_bytes": total_bytes,
    }


def file_metadata(path: Path, ref: str) -> dict[str, object]:
    return {
        "ref": ref,
        "resolved_path": build_root + "\\" + ref.replace("/", "\\"),
        "kind": "file",
        "sha256": file_sha256(path),
        "size_bytes": path.stat().st_size,
    }


def directory_metadata(path: Path, ref: str) -> dict[str, object]:
    result = {
        "ref": ref,
        "resolved_path": build_root + "\\" + ref.replace("/", "\\"),
    }
    result.update(tree_metadata(path))
    return result


bundle_path = Path(bundle_raw)
manifest_path = Path(manifest_raw)
genesis_path = Path(genesis_raw)
bootstrap_path = Path(bootstrap_raw)
world_path = Path(world_raw)
sidecar_path = Path(sidecar_raw)
provenance_path = Path(provenance_raw)
governance_path = Path(governance_raw)
topology_path = Path(topology_raw)
installer_path = Path(installer_raw)

bundle = {
    "schema_version": "oasis7.release_candidate_bundle.v1",
    "candidate_id": "windows-rollout-fixture",
    "track": "public_testnet",
    "runtime_build": file_metadata(installer_path, "oasis7-windows-x64.exe"),
    "world_snapshot": directory_metadata(world_path, "generated-world/world"),
    "generated_world_sidecar": directory_metadata(
        sidecar_path, "generated-world/generated-scenario-world"
    ),
    "world_generation_provenance": file_metadata(
        provenance_path, "generated-world/world-generation-provenance.json"
    ),
    "governance_manifest": file_metadata(
        governance_path, "doc/testing/evidence/genesis-validator-registry.json"
    ),
    "network_manifest": {},
    "evidence_refs": [
        file_metadata(
            topology_path, "doc/testing/evidence/governed-bootstrap-topology.md"
        )
    ],
}

manifest = {
    "schema_version": "oasis7.network_tier_manifest.v1",
    "tier": "public_testnet",
    "network_id": "oasis7-public-testnet-windows-fixture",
    "chain_id": "oasis7-public-testnet-windows-fixture",
    "runtime_refs": {
        "release_candidate_bundle_ref": build_root
        + r"\public-testnet-governed-bootstrap-bundle-2026-06-06.windows.json",
        "genesis_ref": build_root
        + r"\public-testnet-governed-bootstrap-genesis-2026-06-06.windows.json",
        "bootstrap_peer_ref": build_root
        + r"\public-testnet-governed-bootstrap-bootstrap-peers-2026-06-06.windows.txt",
        "generated_world_sidecar_ref": build_root
        + r"\generated-world\generated-scenario-world",
        "world_generation_provenance_ref": build_root
        + r"\generated-world\world-generation-provenance.json",
    },
}
manifest_path.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
# network_manifest metadata depends on the completed manifest bytes.
bundle["network_manifest"] = file_metadata(
    manifest_path,
    "public-testnet-governed-bootstrap-manifest-2026-06-06.windows.json",
)
bundle_path.write_text(json.dumps(bundle, indent=2) + "\n", encoding="utf-8")
PY

cat >"$package_dir/linux-x64-BUILDINFO" <<EOF
workflow=Testnet Packages
run_id=$run_id
run_number=89
repository=eng-cc/oasis7
requested_ref=$commit
commit=$commit
build_profile=release
package_scope=all_existing
platform=linux-x64
package_version=$package_version
published=false
EOF

cat >"$package_dir/windows/windows-x64-BUILDINFO" <<EOF
workflow=Testnet Packages
run_id=$run_id
run_number=89
repository=eng-cc/oasis7
requested_ref=$commit
commit=$commit
build_profile=release
package_scope=all_existing
platform=windows-x64
package_version=$package_version
published=false
EOF

cat >"$package_dir/macos/macos-arm64-BUILDINFO" <<EOF
workflow=Testnet Packages
run_id=$macos_run_id
run_number=90
repository=eng-cc/oasis7
requested_ref=$commit
commit=$commit
build_profile=release
package_scope=linux_macos_arm64
platform=macos-arm64
target_triple=aarch64-apple-darwin
package_version=$macos_package_version
published=false
EOF

(
  cd "$package_dir"
  shasum -a 256 \
    oasis7-linux-x64.deb \
    linux-x64-BUILDINFO \
    oasis7-linux-x64-ops-tools.tar.gz \
    >linux-x64-SHA256SUMS
  cd "$package_dir/windows"
  shasum -a 256 \
    oasis7-windows-x64.exe \
    windows-x64-BUILDINFO \
    oasis7-windows-x64-ops-tools.tar.gz \
    public-testnet-governed-bootstrap-bundle-2026-06-06.windows.json \
    public-testnet-governed-bootstrap-genesis-2026-06-06.windows.json \
    public-testnet-governed-bootstrap-manifest-2026-06-06.windows.json \
    public-testnet-governed-bootstrap-bootstrap-peers-2026-06-06.windows.txt \
    generated-world/world/snapshot.json \
    generated-world/world/journal.json \
    generated-world/generated-scenario-world/snapshot.json \
    generated-world/generated-scenario-world/journal.json \
    generated-world/generated-scenario-world/chunks/0001.json \
    generated-world/world-generation-provenance.json \
    doc/testing/evidence/governance-public-signers.json \
    doc/testing/evidence/liveops-public-signers.json \
    doc/testing/evidence/signer-truth-binding.md \
    doc/testing/evidence/genesis-validator-registry.json \
    doc/testing/evidence/governed-bootstrap-topology.md \
    >windows-x64-SHA256SUMS
  cd "$package_dir/macos"
  shasum -a 256 oasis7-macos-arm64.dmg macos-arm64-BUILDINFO oasis7-macos-arm64-ops-tools.tar.gz >macos-arm64-SHA256SUMS
)

# Windows-only: package fixture setup above is shared with the POSIX harness, but
# the native behavior harness must run before any POSIX symlink/readlink assertion.
windows_powershell_behavior_success_marker='OASIS7_WINDOWS_POWERSHELL_ROLLOUT_BEHAVIOR_COMPLETE=true'

require_windows_powershell_behavior_success() {
  local status="$1"
  local output_path="$2"
  local marker_path="$3"

  if [[ "$status" -ne 0 ]]; then
    echo "Windows PowerShell rollout behavior harness failed with exit status $status" >&2
    cat "$output_path" >&2
    return 1
  fi
  if [[ ! -f "$marker_path" ]]; then
    echo "Windows PowerShell rollout behavior harness did not write completion marker: $marker_path" >&2
    cat "$output_path" >&2
    return 1
  fi
  if ! printf '%s' "$windows_powershell_behavior_success_marker" | cmp -s - "$marker_path"; then
    echo "Windows PowerShell rollout behavior harness wrote an invalid completion marker: $marker_path" >&2
    cat "$output_path" >&2
    return 1
  fi
}

verify_windows_powershell_behavior_boundary_fails_closed() {
  local output_path="$TMP_DIR/windows-powershell-boundary-negative-self-check.log"
  local marker_path="$TMP_DIR/windows-powershell-boundary-negative-self-check.marker"

  printf 'ParserError: simulated fixture parser failure\n' >"$output_path"
  rm -f "$marker_path"
  if require_windows_powershell_behavior_success 0 "$output_path" "$marker_path" >/dev/null 2>&1; then
    echo "Windows PowerShell behavior boundary accepted missing marker file" >&2
    return 1
  fi
  printf 'OASIS7_WINDOWS_POWERSHELL_ROLLOUT_BEHAVIOR_COMPLETE=false' >"$marker_path"
  if require_windows_powershell_behavior_success 0 "$output_path" "$marker_path" >/dev/null 2>&1; then
    echo "Windows PowerShell behavior boundary accepted wrong marker file content" >&2
    return 1
  fi
  printf '%s' "$windows_powershell_behavior_success_marker" >"$marker_path"
  if require_windows_powershell_behavior_success 17 "$output_path" "$marker_path" >/dev/null 2>&1; then
    echo "Windows PowerShell behavior boundary accepted nonzero PowerShell status" >&2
    return 1
  fi
}

run_windows_powershell_behavior_harness() {
windows_powershell="$(command -v powershell.exe || command -v powershell || command -v pwsh || true)"
if [[ -z "$windows_powershell" ]]; then
  echo "OASIS7_WINDOWS_POWERSHELL_BEHAVIOR_TEST=1 requires Windows PowerShell" >&2
  exit 1
fi
if ! command -v cygpath >/dev/null 2>&1; then
  echo "OASIS7_WINDOWS_POWERSHELL_BEHAVIOR_TEST=1 requires Git Bash cygpath" >&2
  exit 1
fi
local powershell_output="$TMP_DIR/windows-powershell-behavior.log"
local powershell_completion_marker
local powershell_fixture_script
local powershell_fixture_script_windows
local powershell_status
if ! awk '
  /^[[:space:]]*"\$windows_powershell" -NoProfile -ExecutionPolicy Bypass -File "\$powershell_fixture_script_windows"/ {
    found = 1
  }
  END { exit !found }
' "${BASH_SOURCE[0]}"; then
  echo "Windows PowerShell behavior harness must invoke its fixture with -File" >&2
  exit 1
fi
powershell_completion_marker="$(mktemp "$TMP_DIR/windows-powershell-behavior-completion-marker.XXXXXX")"
rm -f "$powershell_completion_marker"
powershell_fixture_script="$(mktemp "$TMP_DIR/windows-powershell-behavior-fixture.XXXXXX.ps1")"
cat >"$powershell_fixture_script" <<'PS'
$ErrorActionPreference = 'Stop'

function Assert-FixtureNoReparseAncestor([string] $Path) {
$probe = [System.IO.Path]::GetFullPath($Path)
while (![string]::IsNullOrWhiteSpace($probe)) {
  $item = Get-Item -LiteralPath $probe -Force -ErrorAction SilentlyContinue
  if ($null -ne $item -and
      (($item.Attributes -band [System.IO.FileAttributes]::ReparsePoint) -ne 0)) {
    throw "fixture safe-root precondition failed: reparse-point component=$probe configured_base=$Path"
  }
  $parent = [System.IO.Path]::GetDirectoryName($probe)
  if ([string]::IsNullOrWhiteSpace($parent) -or
      $parent.Equals($probe, [System.StringComparison]::OrdinalIgnoreCase)) {
    break
  }
  $probe = $parent
}
}

function Assert-FixturePathUnderSafeBase([string] $Path, [string] $Label) {
$safeRoot = [System.IO.Path]::GetFullPath($fixtureSafeRootBase).TrimEnd('\')
$candidate = [System.IO.Path]::GetFullPath($Path).TrimEnd('\')
if (!$candidate.StartsWith($safeRoot + '\', [System.StringComparison]::OrdinalIgnoreCase)) {
  throw "fixture generated mutable path escapes verified safe base: label=$Label path=$Path canonical=$candidate safe_base=$safeRoot"
}
return $candidate
}

$fixtureWorkspaceRoot = [System.IO.Path]::GetFullPath($env:OASIS7_FIXTURE_WORKSPACE_ROOT)
if (!(Test-Path -LiteralPath $fixtureWorkspaceRoot -PathType Container)) {
  throw "fixture workspace root does not exist: $fixtureWorkspaceRoot"
}
$fixtureDriveRoot = [System.IO.Path]::GetPathRoot($fixtureWorkspaceRoot)
Assert-FixtureNoReparseAncestor $fixtureDriveRoot
$fixtureRunId = [Guid]::NewGuid().ToString('N')
$fixtureRoot = Join-Path $fixtureDriveRoot ("o7fx-" + $fixtureRunId)
if (Test-Path -LiteralPath $fixtureRoot) {
  throw "fixture unique drive-root path collision: $fixtureRoot"
}
New-Item -ItemType Directory -Path $fixtureRoot -Force | Out-Null
Assert-FixtureNoReparseAncestor $fixtureRoot
$fixtureSafeRootBase = $fixtureRoot
$fixtureTasks = [System.Collections.Generic.List[string]]::new()

function Get-Sha256([string] $Path) {
(Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
}

function Assert-Equal([object] $Actual, [object] $Expected, [string] $Label) {
if ($Actual -ne $Expected) { throw "${Label}: expected=[$Expected] actual=[$Actual]" }
}

function Get-FixtureCanonicalExistingPath([string] $Path) {
$item = Get-Item -LiteralPath $Path -Force -ErrorAction Stop
return [System.IO.Path]::GetFullPath($item.FullName).TrimEnd('\')
}

function Get-FixtureRelativeExistingPath([string] $Root, [string] $Path, [string] $Label) {
$rootCanonical = Get-FixtureCanonicalExistingPath $Root
$pathCanonical = Get-FixtureCanonicalExistingPath $Path
$rootPrefix = $rootCanonical + '\'
if (!$pathCanonical.StartsWith($rootPrefix, [System.StringComparison]::OrdinalIgnoreCase)) {
  throw "fixture relative path escapes canonical root: label=$Label root=$Root root_canonical=$rootCanonical path=$Path path_canonical=$pathCanonical"
}
$relative = $pathCanonical.Substring($rootPrefix.Length)
if ([string]::IsNullOrWhiteSpace($relative)) {
  throw "fixture relative path is empty: label=$Label root=$Root path=$Path"
}
return $relative
}

function Assert-FixtureSameExistingPath([string] $Actual, [string] $Expected, [string] $Label) {
$actualCanonical = Get-FixtureCanonicalExistingPath $Actual
$expectedCanonical = Get-FixtureCanonicalExistingPath $Expected
if (!$actualCanonical.Equals($expectedCanonical, [System.StringComparison]::OrdinalIgnoreCase)) {
  throw "${Label}: expected=[$Expected] expected_canonical=[$expectedCanonical] actual=[$Actual] actual_canonical=[$actualCanonical]"
}
}

function Assert-Utf8NoBom([string] $Path) {
$bytes = [System.IO.File]::ReadAllBytes($Path)
if ($bytes.Length -ge 3 -and $bytes[0] -eq 0xEF -and $bytes[1] -eq 0xBB -and $bytes[2] -eq 0xBF) {
  throw "fixture JSON must be UTF-8 without BOM: $Path"
}
}

function Get-FixturePhysicalCaseId([string] $LogicalName) {
if ([string]::IsNullOrWhiteSpace($LogicalName)) {
  throw 'fixture logical case name must not be empty'
}
$sha = [System.Security.Cryptography.SHA256]::Create()
try {
  $bytes = [System.Text.Encoding]::UTF8.GetBytes($LogicalName)
  $digest = ([System.BitConverter]::ToString($sha.ComputeHash($bytes))).Replace('-', '').ToLowerInvariant()
} finally {
  $sha.Dispose()
}
return 'c-' + $digest.Substring(0, 16)
}

function Invoke-FixtureRollout([string] $Rollout) {
$previousErrorActionPreference = $ErrorActionPreference
try {
  $ErrorActionPreference = 'Continue'
  $output = @(& powershell.exe -NoProfile -ExecutionPolicy Bypass -File $Rollout 2>&1)
  $exitCode = $LASTEXITCODE
} finally {
  $ErrorActionPreference = $previousErrorActionPreference
}
return [PSCustomObject]@{ ExitCode = $exitCode; Output = @($output) }
}

function Invoke-FixtureRolloutExpectingFailure([string] $Rollout) {
return Invoke-FixtureRollout $Rollout
}

function Write-FixtureExecutables {
param([string] $Directory)
$runtimeSource = @'
using System;
using System.Net;
using System.Text;
public static class FixtureRuntime {
public static void Main(string[] args) {
  int port = Int32.Parse(args[1]);
  var listener = new HttpListener();
  listener.Prefixes.Add("http://127.0.0.1:" + port + "/");
  listener.Start();
  while (true) {
    var context = listener.GetContext();
    byte[] payload = Encoding.UTF8.GetBytes("{\"running\":true,\"fixture\":\"candidate\"}");
    context.Response.ContentType = "application/json";
    context.Response.OutputStream.Write(payload, 0, payload.Length);
    context.Response.Close();
  }
}
}
'@
$installerSource = @'
using System;
using System.IO;
public static class FixtureInstaller {
public static void Main() {
  string installRoot = Environment.GetEnvironmentVariable("OASIS7_FIXTURE_INSTALL_ROOT");
  string runtime = Environment.GetEnvironmentVariable("OASIS7_FIXTURE_RUNTIME_TEMPLATE");
  string target = Path.Combine(installRoot, "bin", "oasis7_chain_runtime.exe");
  Directory.CreateDirectory(Path.GetDirectoryName(target));
  File.Copy(runtime, target, true);
}
}
'@
$providerStatusSource = @'
using System;
using System.Net;
using System.Text;
public static class FixtureProviderStatus {
public static void Main(string[] args) {
  int port = Int32.Parse(args[1]);
  var listener = new HttpListener();
  listener.Prefixes.Add("http://127.0.0.1:" + port + "/");
  listener.Start();
  while (true) {
    var context = listener.GetContext();
    byte[] payload = Encoding.UTF8.GetBytes("{\"chain_proof\":{\"latest_execution_checkpoint\":{\"schema_version\":2,\"checkpoint_id\":\"fixture-checkpoint-v2\",\"height\":4242,\"manifest_hash\":\"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\"}}}");
    context.Response.ContentType = "application/json";
    context.Response.OutputStream.Write(payload, 0, payload.Length);
    context.Response.Close();
  }
}
}
'@
$runtime = Join-Path $Directory 'oasis7_chain_runtime.exe'
$knownGoodRuntime = Join-Path $Directory 'known-good-oasis7_chain_runtime.exe'
$installer = Join-Path $Directory 'fixture-installer.exe'
$providerStatus = Join-Path $Directory 'fixture-provider-status.exe'
Add-Type -TypeDefinition $runtimeSource -OutputAssembly $runtime -OutputType ConsoleApplication
$knownGoodRuntimeSource = $runtimeSource.Replace('FixtureRuntime', 'FixtureKnownGoodRuntime').Replace('{\"running\":true,\"fixture\":\"candidate\"}', '{\"running\":true,\"fixture\":\"known-good\"}')
Add-Type -TypeDefinition $knownGoodRuntimeSource -OutputAssembly $knownGoodRuntime -OutputType ConsoleApplication
Add-Type -TypeDefinition $installerSource -OutputAssembly $installer -OutputType ConsoleApplication
Add-Type -TypeDefinition $providerStatusSource -OutputAssembly $providerStatus -OutputType ConsoleApplication
return @{ Runtime = $runtime; KnownGoodRuntime = $knownGoodRuntime; Installer = $installer; ProviderStatus = $providerStatus }
}

$executables = Write-FixtureExecutables -Directory $fixtureRoot
$fixtureProviderCheckpointId = 'fixture-checkpoint-v2'
$fixtureProviderCheckpointHeight = 4242
$fixtureProviderManifestHash = '0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef'
$fixtureProviderProcesses = [System.Collections.Generic.List[System.Diagnostics.Process]]::new()

function Start-FixtureProviderStatusServer([string] $Name) {
for ($attempt = 1; $attempt -le 10; $attempt++) {
  $listener = [System.Net.Sockets.TcpListener]::new([System.Net.IPAddress]::Loopback, 0)
  $listener.Start()
  $port = ([System.Net.IPEndPoint]$listener.LocalEndpoint).Port
  $listener.Stop()
  $process = Start-Process -FilePath $executables.ProviderStatus -ArgumentList '--port', "$port" -PassThru
  $url = "http://127.0.0.1:$port/v1/chain/status"
  for ($probe = 1; $probe -le 20; $probe++) {
    Start-Sleep -Milliseconds 100
    try {
      $status = Invoke-RestMethod -UseBasicParsing -Uri $url -TimeoutSec 1
      $checkpoint = $status.chain_proof.latest_execution_checkpoint
      if ($checkpoint.schema_version -ne 2 -or
          $checkpoint.checkpoint_id -ne $fixtureProviderCheckpointId -or
          $checkpoint.height -ne $fixtureProviderCheckpointHeight -or
          $checkpoint.manifest_hash -ne $fixtureProviderManifestHash) {
        throw "fixture provider returned unexpected checkpoint evidence: name=$Name payload=$($status | ConvertTo-Json -Compress)"
      }
      $fixtureProviderProcesses.Add($process)
      return $url
    } catch {
      if ($probe -eq 20) {
        Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue
        throw "fixture provider did not become ready: name=$Name url=$url error=$($_.Exception.Message)"
      }
    }
  }
}
throw "fixture provider failed to reserve a listener port: name=$Name"
}

function Assert-FixtureProviderStatus([string] $Name, [string] $Url) {
try {
  $status = Invoke-RestMethod -UseBasicParsing -Uri $Url -TimeoutSec 2
  $checkpoint = $status.chain_proof.latest_execution_checkpoint
  if ($checkpoint.schema_version -ne 2 -or
      $checkpoint.checkpoint_id -ne $fixtureProviderCheckpointId -or
      $checkpoint.height -ne $fixtureProviderCheckpointHeight -or
      $checkpoint.manifest_hash -ne $fixtureProviderManifestHash) {
    throw "unexpected checkpoint evidence: $($status | ConvertTo-Json -Compress)"
  }
} catch {
  throw "fixture canonical provider unavailable or invalid: name=$Name url=$Url error=$($_.Exception.Message)"
}
}

$fixtureProviderEndpoints = @{
  Sequencer = Start-FixtureProviderStatusServer 'sequencer'
  Storage = Start-FixtureProviderStatusServer 'storage'
}

function New-RolloutFixture {
param([string] $Name, [switch] $ActiveConfigJunction, [switch] $RollbackRootJunction)
$physicalCaseId = Get-FixturePhysicalCaseId $Name
if ($physicalCaseId -notmatch '^c-[0-9a-f]{16}$' -or $physicalCaseId.Length -ne 18) {
  throw "fixture physical case identifier is not deterministically bounded: logical_case=$Name physical_case=$physicalCaseId"
}
if ((Get-FixturePhysicalCaseId $Name) -ne $physicalCaseId) {
  throw "fixture physical case identifier is not deterministic: logical_case=$Name physical_case=$physicalCaseId"
}
$root = Join-Path $fixtureRoot $physicalCaseId
if (Test-Path -LiteralPath $root) {
  throw "fixture physical case identifier collision: logical_case=$Name physical_case=$physicalCaseId root=$root"
}
$deployReal = Join-Path $root 'deploy-real'
$deploy = if ($ActiveConfigJunction) { Join-Path $root 'deploy-link' } else { $deployReal }
$installRoot = Join-Path $root 'install'
$rollbackReal = Join-Path $deployReal 'backups/rollback-real'
$rollbackRoot = if ($RollbackRootJunction) { Join-Path $deploy 'backups/rollback-link' } else { $rollbackReal }
New-Item -ItemType Directory -Path $deployReal, $installRoot, $rollbackReal -Force | Out-Null
if ($ActiveConfigJunction) { New-Item -ItemType Junction -Path $deploy -Target $deployReal | Out-Null }
if ($RollbackRootJunction) { New-Item -ItemType Junction -Path $rollbackRoot -Target $rollbackReal | Out-Null }
if ($ActiveConfigJunction) {
  $deployJunction = Get-Item -LiteralPath $deploy -Force -ErrorAction Stop
  if (($deployJunction.Attributes -band [System.IO.FileAttributes]::ReparsePoint) -eq 0) {
    throw "fixture active deploy root is not a reparse point: $deploy"
  }
}
if ($RollbackRootJunction) {
  $rollbackJunction = Get-Item -LiteralPath $rollbackRoot -Force -ErrorAction Stop
  if (($rollbackJunction.Attributes -band [System.IO.FileAttributes]::ReparsePoint) -eq 0) {
    throw "fixture rollback root is not a reparse point: $rollbackRoot"
  }
}
$taskName = "Oasis7Fixture2269_" + [Guid]::NewGuid().ToString('N')
$port = Get-Random -Minimum 20000 -Maximum 45000
$config = Join-Path $deploy 'config'
$activeBundle = Join-Path $config 'public-testnet-governed-bootstrap-bundle-2026-06-06.windows.json'
$activeGenesis = Join-Path $config 'public-testnet-governed-bootstrap-genesis-2026-06-06.windows.json'
$activeManifest = Join-Path $config 'public-testnet-governed-bootstrap-manifest-2026-06-06.windows.json'
$activeBootstrap = Join-Path $config 'public-testnet-governed-bootstrap-bootstrap-peers-2026-06-06.windows.txt'
New-Item -ItemType Directory -Path $config, (Join-Path $installRoot 'bin'), (Join-Path $rollbackRoot 'runtime'), (Join-Path $rollbackRoot 'config') -Force | Out-Null
Copy-Item -LiteralPath $executables.KnownGoodRuntime -Destination (Join-Path $installRoot 'bin/oasis7_chain_runtime.exe') -Force
Copy-Item -LiteralPath $executables.KnownGoodRuntime -Destination (Join-Path $rollbackRoot 'runtime/oasis7_chain_runtime.exe') -Force
Set-Content -LiteralPath (Join-Path $deploy 'CURRENT_VERSION') -Value 'known-good-version' -NoNewline
Set-Content -LiteralPath (Join-Path $deploy 'DEPLOYED_BUILDINFO') -Value 'known-good-buildinfo' -NoNewline
Copy-Item -LiteralPath (Join-Path $deploy 'CURRENT_VERSION') -Destination (Join-Path $rollbackRoot 'CURRENT_VERSION') -Force
Copy-Item -LiteralPath (Join-Path $deploy 'DEPLOYED_BUILDINFO') -Destination (Join-Path $rollbackRoot 'DEPLOYED_BUILDINFO') -Force
$runtimeBackupHash = Get-Sha256 (Join-Path $rollbackRoot 'runtime/oasis7_chain_runtime.exe')
$backupManifestPath = Join-Path $rollbackRoot 'backup-manifest.json'
[System.IO.File]::WriteAllText(
  $backupManifestPath,
  (@{
  schema_version = 'oasis7.windows_observer_backup.v1'
  runtime_path = 'runtime\oasis7_chain_runtime.exe'
  runtime_sha256 = $runtimeBackupHash
  } | ConvertTo-Json),
  [System.Text.UTF8Encoding]::new($false)
)
$action = New-ScheduledTaskAction -Execute (Join-Path $installRoot 'bin/oasis7_chain_runtime.exe') -Argument "--port $port"
Register-ScheduledTask -TaskName $taskName -Action $action -Force | Out-Null
$fixtureTasks.Add($taskName)
$manifest = @{
  nodes = @(
  @{
    name = 'sequencer'; platform = 'linux-x64'; node_root = (Join-Path $root 'sequencer-node'); restart = $false
    status_url = $fixtureProviderEndpoints.Sequencer
  },
  @{
    name = 'storage'; platform = 'linux-x64'; node_root = (Join-Path $root 'storage-node'); restart = $false
    status_url = $fixtureProviderEndpoints.Storage
  },
  @{
    name = 'windows-fixture'; platform = 'windows-x64'; deploy_root = $deploy; install_root = $installRoot
    remote_installer = (Join-Path $deploy 'placeholder/oasis7-windows-x64.exe'); scheduled_task = $taskName
    status_url = "http://127.0.0.1:$port/v1/chain/status"; rollback_backup_root = $rollbackRoot; rollback_unlock_timeout_secs = 5
    governed_bundle_path = $activeBundle; governed_genesis_path = $activeGenesis; governed_manifest_path = $activeManifest; governed_bootstrap_path = $activeBootstrap
  })
}
$manifestPath = Join-Path $root 'manifest.json'
$outDir = Join-Path $root 'out'
[System.IO.File]::WriteAllText(
  $manifestPath,
  ($manifest | ConvertTo-Json -Depth 8),
  [System.Text.UTF8Encoding]::new($false)
)
Assert-Utf8NoBom $backupManifestPath
Assert-Utf8NoBom $manifestPath
Assert-FixtureProviderStatus 'sequencer' $fixtureProviderEndpoints.Sequencer
Assert-FixtureProviderStatus 'storage' $fixtureProviderEndpoints.Storage
& python $env:OASIS7_FIXTURE_ROLLOUT_PY --manifest $manifestPath --package-dir $env:OASIS7_FIXTURE_PACKAGE_DIR --out-dir $outDir --json | Out-Null
if ($LASTEXITCODE -ne 0) { throw "fixture rollout generation failed: $Name" }
$rollout = Join-Path $outDir 'windows-fixture-windows-upgrade.ps1'
$scriptText = [IO.File]::ReadAllText($rollout)
$pathGuardStart = $scriptText.IndexOf('function Test-NodeLocalPath')
$pathGuardEnd = $scriptText.IndexOf('function Preserve-AttemptDiagnostics', $pathGuardStart)
if ($pathGuardStart -lt 0 -or $pathGuardEnd -le $pathGuardStart) {
  throw 'fixture could not isolate generated node-local physical path guards'
}
$pathGuardDefinitions = [scriptblock]::Create($scriptText.Substring($pathGuardStart, $pathGuardEnd - $pathGuardStart))
. $pathGuardDefinitions
$deployRoot = $deploy
if ($ActiveConfigJunction -or $RollbackRootJunction) {
  $expectedReparseAncestor = if ($ActiveConfigJunction) { $deploy } else { $rollbackRoot }
  $nestedReparseProbe = Join-Path $expectedReparseAncestor 'nested-parent-traversal/leaf.txt'
  $ancestorRejected = $false
  try {
    Assert-NodeLocalPhysicalPath -Path $nestedReparseProbe -Label 'fixture parent traversal probe' | Out-Null
  } catch {
    $reportedReparseMatch = [regex]::Match($_.Exception.Message, 'reparse-point component:\s*(?<path>.+?)\s*$')
    if (!$reportedReparseMatch.Success) {
      throw "fixture parent traversal did not identify its reparse ancestor: expected=$expectedReparseAncestor error=$($_.Exception.Message)"
    }
    $reportedReparseAncestor = $reportedReparseMatch.Groups['path'].Value
    $reportedReparseItem = Get-Item -LiteralPath $reportedReparseAncestor -Force -ErrorAction Stop
    if (($reportedReparseItem.Attributes -band [System.IO.FileAttributes]::ReparsePoint) -eq 0) {
      throw "fixture parent traversal reported a non-reparse path: $reportedReparseAncestor"
    }
    Assert-FixtureSameExistingPath $reportedReparseAncestor $expectedReparseAncestor 'fixture parent traversal rejected the wrong reparse ancestor'
    $ancestorRejected = $true
  }
  if (!$ancestorRejected) {
    throw "fixture parent traversal accepted nested path below reparse ancestor: $nestedReparseProbe"
  }
}
$requiredGeneratedAssignments = @{
  deployRoot = $deploy
  installRoot = $installRoot
  rollbackBackupRoot = $rollbackRoot
  activeBundlePath = $activeBundle
  activeGenesisPath = $activeGenesis
  activeManifestPath = $activeManifest
  activeBootstrapPath = $activeBootstrap
}
foreach ($requiredGeneratedAssignment in $requiredGeneratedAssignments.GetEnumerator()) {
  $assignmentPattern = '\$' + [regex]::Escape($requiredGeneratedAssignment.Key) + ' = \[Environment\]::ExpandEnvironmentVariables\(''([^'']+)''\)'
  $assignmentMatch = [regex]::Match($scriptText, $assignmentPattern)
  if (!$assignmentMatch.Success -or $assignmentMatch.Groups[1].Value -ne $requiredGeneratedAssignment.Value) {
    throw "fixture generated mutable destination diverged: variable=$($requiredGeneratedAssignment.Key) expected=$($requiredGeneratedAssignment.Value) actual=$($assignmentMatch.Groups[1].Value)"
  }
  Assert-FixturePathUnderSafeBase $assignmentMatch.Groups[1].Value $requiredGeneratedAssignment.Key | Out-Null
}
$stagingRootAssignmentPattern = '\$stagingRoot = \[Environment\]::ExpandEnvironmentVariables\(''([^'']+)''\)'
$representativeStagingAssignment = '$stagingRoot = [Environment]::ExpandEnvironmentVariables(''C:\oasis7-deploy\staging\package-rollout\manual'')'
$representativeStagingMatch = [regex]::Match($representativeStagingAssignment, $stagingRootAssignmentPattern)
if (!$representativeStagingMatch.Success -or $representativeStagingMatch.Groups[1].Value -ne 'C:\oasis7-deploy\staging\package-rollout\manual') {
  throw 'fixture staging-root extraction regex does not match the generated PowerShell assignment contract'
}
$stagingMatch = [regex]::Match($scriptText, $stagingRootAssignmentPattern)
if (!$stagingMatch.Success -or [string]::IsNullOrWhiteSpace($stagingMatch.Groups[1].Value)) {
  throw "fixture could not locate generated staging root: $Name"
}
$staging = $stagingMatch.Groups[1].Value
Assert-FixturePathUnderSafeBase $staging 'stagingRoot' | Out-Null
foreach ($stagedAssignmentName in @('installer', 'bundlePath', 'genesisPath', 'manifestPath', 'bootstrapPath')) {
  $stagedAssignmentPattern = '\$' + [regex]::Escape($stagedAssignmentName) + ' = \[Environment\]::ExpandEnvironmentVariables\(''([^'']+)''\)'
  $stagedAssignmentMatches = [regex]::Matches($scriptText, $stagedAssignmentPattern)
  if ($stagedAssignmentMatches.Count -eq 0) {
    throw "fixture generated script omitted guarded staged assignment: $stagedAssignmentName"
  }
  foreach ($stagedAssignmentMatch in $stagedAssignmentMatches) {
    Assert-FixturePathUnderSafeBase $stagedAssignmentMatch.Groups[1].Value $stagedAssignmentName | Out-Null
  }
}
[Console]::Error.WriteLine("fixture_generated_staging_root logical_case=$Name physical_case=$physicalCaseId path=$staging safe_base=$fixtureSafeRootBase package_input=$env:OASIS7_FIXTURE_PACKAGE_DIR")
New-Item -ItemType Directory -Path (Join-Path $staging 'config') -Force | Out-Null
Copy-Item -LiteralPath $executables.Installer -Destination (Join-Path $staging 'oasis7-windows-x64.exe') -Force
$packageWindows = Join-Path $env:OASIS7_FIXTURE_PACKAGE_DIR 'windows'
$representativeGovernedLeaf = 'public-testnet-governed-bootstrap-bundle-2026-06-06.windows.json'
$representativePackagePath = Join-Path $packageWindows $representativeGovernedLeaf
$representativePackageRelative = Get-FixtureRelativeExistingPath $packageWindows $representativePackagePath 'representative package governed file'
Assert-Equal $representativePackageRelative $representativeGovernedLeaf 'fixture package projection introduced a spurious directory component'
Get-ChildItem -LiteralPath $packageWindows -Recurse -File | Where-Object {
  $_.Name -notin @(
    'oasis7-windows-x64.exe',
    'windows-x64-BUILDINFO',
    'windows-x64-SHA256SUMS',
    'oasis7-windows-x64-ops-tools.tar.gz',
    'windows-x64-ops-tools-SHA256SUMS'
  )
} | ForEach-Object {
  $relative = Get-FixtureRelativeExistingPath $packageWindows $_.FullName 'package governed file'
  $destination = Join-Path (Join-Path $staging 'config') $relative
  New-Item -ItemType Directory -Path (Split-Path $destination -Parent) -Force | Out-Null
  Copy-Item -LiteralPath $_.FullName -Destination $destination -Force
}
# Mirror every staged governed file into the active config and known-good
# backup. The generated rollout promotes this complete recursive closure,
# not only the three transformed JSON documents.
$stagingConfig = Join-Path $staging 'config'
$activeGovernedFiles = @(
  Get-ChildItem -LiteralPath $stagingConfig -Recurse -File | ForEach-Object {
    $relative = Get-FixtureRelativeExistingPath $stagingConfig $_.FullName 'staged governed file'
    $activePath = Join-Path $config $relative
    $backupPath = Join-Path (Join-Path $rollbackRoot 'config') $relative
    New-Item -ItemType Directory -Path (Split-Path $activePath -Parent), (Split-Path $backupPath -Parent) -Force | Out-Null
    Set-Content -LiteralPath $activePath -Value ("known-good-$relative") -NoNewline
    Copy-Item -LiteralPath $activePath -Destination $backupPath -Force
    $activePath
  }
)
foreach ($requiredActivePath in @($activeBundle, $activeGenesis, $activeManifest, $activeBootstrap)) {
  $requiredActiveLeaf = [System.IO.Path]::GetFileName($requiredActivePath)
  $projectedActivePaths = @(
    $activeGovernedFiles | Where-Object {
      [System.IO.Path]::GetFileName($_) -eq $requiredActiveLeaf
    }
  )
  if ($projectedActivePaths.Count -eq 0) {
    throw "fixture staged governed closure omitted required active target: $requiredActivePath"
  }
  if ($projectedActivePaths.Count -ne 1) {
    throw "fixture staged governed closure has ambiguous required active target: $requiredActiveLeaf"
  }
  if (!$ActiveConfigJunction) {
    $projectedActivePath = $projectedActivePaths[0]
    if (!(Test-Path -LiteralPath $requiredActivePath -PathType Leaf)) {
      throw "fixture logical active target is not reachable through its configured path: required=$requiredActivePath required_length=$($requiredActivePath.Length) projected=$projectedActivePath projected_length=$($projectedActivePath.Length) safe_base=$fixtureSafeRootBase"
    }
    Assert-Equal (Get-Sha256 $projectedActivePath) (Get-Sha256 $requiredActivePath) "fixture active target projection diverged for $requiredActiveLeaf"
  }
}
$atomicPromotionSuffixBudget = 48
$windowsLegacyPathStringBudget = 259
$governedPathBudget = $windowsLegacyPathStringBudget - $atomicPromotionSuffixBudget
if ($governedPathBudget -le 0) {
  throw 'fixture derived governed path budget must be positive'
}
$governedPathCandidates = @(
  Get-ChildItem -LiteralPath $stagingConfig -Recurse -File | ForEach-Object { $_.FullName }
  $activeGovernedFiles
  Get-ChildItem -LiteralPath (Join-Path $rollbackRoot 'config') -Recurse -File | ForEach-Object { $_.FullName }
)
$longestGovernedPath = @($governedPathCandidates | Sort-Object Length -Descending | Select-Object -First 1)[0]
if ([string]::IsNullOrWhiteSpace($longestGovernedPath)) {
  throw 'fixture governed path budget check found no governed files'
}
[Console]::Error.WriteLine("fixture_governed_path_budget logical_case=$Name physical_case=$physicalCaseId longest_length=$($longestGovernedPath.Length) budget=$governedPathBudget path=$longestGovernedPath safe_base=$fixtureSafeRootBase")
if ($longestGovernedPath.Length -gt $governedPathBudget) {
  throw "fixture governed path exceeds Windows PowerShell 5.1 safety budget: length=$($longestGovernedPath.Length) budget=$governedPathBudget path=$longestGovernedPath"
}
if (($longestGovernedPath.Length + $atomicPromotionSuffixBudget) -gt $windowsLegacyPathStringBudget) {
  throw "fixture governed path leaves insufficient atomic promotion headroom: length=$($longestGovernedPath.Length) suffix_budget=$atomicPromotionSuffixBudget max=$windowsLegacyPathStringBudget path=$longestGovernedPath"
}
return @{ LogicalName=$Name; PhysicalCaseId=$physicalCaseId; Root=$root; Deploy=$deploy; Config=$config; Install=$installRoot; Rollback=$rollbackRoot; Staging=$staging; StagingConfig=$stagingConfig; Task=$taskName; Port=$port; Rollout=$rollout; ActiveGovernedFiles=$activeGovernedFiles; ActiveBundle=$activeBundle; ActiveGenesis=$activeGenesis; ActiveManifest=$activeManifest; ActiveBootstrap=$activeBootstrap; Active=@($activeGovernedFiles + (Join-Path $deploy 'CURRENT_VERSION') + (Join-Path $deploy 'DEPLOYED_BUILDINFO')); Runtime=(Join-Path $installRoot 'bin/oasis7_chain_runtime.exe') }
}

function Get-FixtureSnapshot($Fixture) {
$files = @($Fixture.Runtime) + $Fixture.Active
@($files | ForEach-Object { "$($_)=$(Get-Sha256 $_)" }) -join "`n"
}

function Assert-FixtureCaseIdentity($Fixture, [string] $ExpectedLogicalName) {
Assert-Equal $Fixture.LogicalName $ExpectedLogicalName 'fixture logical case identity diverged'
Assert-Equal ([System.IO.Path]::GetFileName($Fixture.Root.TrimEnd('\'))) $Fixture.PhysicalCaseId 'fixture root does not use bounded physical case identity'
if ($Fixture.PhysicalCaseId.Length -ne 18) {
  throw "fixture returned unbounded physical case identity: logical_case=$ExpectedLogicalName physical_case=$($Fixture.PhysicalCaseId)"
}
}

function Assert-OriginalTaskAction($Fixture) {
$action = @((Get-ScheduledTask -TaskName $Fixture.Task).Actions)[0]
Assert-Equal $action.Execute $Fixture.Runtime "scheduled task execute changed for $($Fixture.Task)"
Assert-Equal $action.Arguments "--port $($Fixture.Port)" "scheduled task arguments changed for $($Fixture.Task)"
}

function Remove-RolloutFixture($Fixture) {
if ($null -eq $Fixture) { return }
Unregister-ScheduledTask -TaskName $Fixture.Task -Confirm:$false -ErrorAction SilentlyContinue
Get-Process oasis7_chain_runtime -ErrorAction SilentlyContinue | Where-Object { $_.Path -like "$($Fixture.Install)*" } | Stop-Process -Force -ErrorAction SilentlyContinue
}

try {
# Both cases use a live, uniquely named task. A reparse ancestor must reject
# before Stop-ScheduledTask: the task remains running and all persisted state
# is byte-identical.
foreach ($case in @(@{Name='active-config-junction'; Active=$true; Rollback=$false}, @{Name='rollback-root-junction'; Active=$false; Rollback=$true})) {
  $fixture = New-RolloutFixture -Name $case.Name -ActiveConfigJunction:$case.Active -RollbackRootJunction:$case.Rollback
  Assert-FixtureCaseIdentity $fixture $case.Name
  try {
    Start-ScheduledTask -TaskName $fixture.Task
    Start-Sleep -Seconds 1
    $before = Get-FixtureSnapshot $fixture
    $invocation = Invoke-FixtureRolloutExpectingFailure $fixture.Rollout
    if ($invocation.ExitCode -eq 0) { throw "junction fixture unexpectedly succeeded: $($case.Name)" }
    $junctionOutput = $invocation.Output -join "`n"
    [Console]::Error.WriteLine("fixture_expected_reparse case=$($case.Name) staging_root=$($fixture.Staging) output=$junctionOutput")
    if ($junctionOutput -notmatch 'reparse') { throw "junction fixture missing reparse rejection: $($case.Name) output=$junctionOutput" }
    Assert-Equal (Get-FixtureSnapshot $fixture) $before "junction fixture mutated persisted state: $($case.Name)"
    Assert-OriginalTaskAction $fixture
    if ((Get-ScheduledTask -TaskName $fixture.Task).State -ne 'Running') { throw "junction fixture reached Stop-ScheduledTask: $($case.Name)" }
  } finally { Remove-RolloutFixture $fixture }
}

# An incomplete known-good closure is a preflight failure: no task stop, runtime
# install, or active config promotion is permitted before all rollback members
# are available.  Keep this as a native fixture because a generated-script
# source check cannot prove the scheduled task stayed running.
$incompleteRollbackFixture = New-RolloutFixture -Name 'incomplete-rollback-closure'
Assert-FixtureCaseIdentity $incompleteRollbackFixture 'incomplete-rollback-closure'
try {
  Start-ScheduledTask -TaskName $incompleteRollbackFixture.Task
  Start-Sleep -Seconds 1
  $before = Get-FixtureSnapshot $incompleteRollbackFixture
  $missingRollbackMember = Join-Path $incompleteRollbackFixture.Rollback 'config/generated-world/world/journal.json'
  if (!(Test-Path -LiteralPath $missingRollbackMember -PathType Leaf)) {
    throw "incomplete rollback fixture lacks removable backup member: $missingRollbackMember"
  }
  Remove-Item -LiteralPath $missingRollbackMember -Force
  $invocation = Invoke-FixtureRolloutExpectingFailure $incompleteRollbackFixture.Rollout
  if ($invocation.ExitCode -eq 0) { throw 'incomplete rollback closure unexpectedly succeeded' }
  $incompleteOutput = $invocation.Output -join "`n"
  if ($incompleteOutput -notmatch 'known-good rollback config missing') {
    throw "incomplete rollback closure did not report its missing backup member: $incompleteOutput"
  }
  Assert-Equal (Get-FixtureSnapshot $incompleteRollbackFixture) $before 'incomplete rollback closure mutated persisted state'
  Assert-OriginalTaskAction $incompleteRollbackFixture
  if ((Get-ScheduledTask -TaskName $incompleteRollbackFixture.Task).State -ne 'Running') {
    throw 'incomplete rollback closure reached Stop-ScheduledTask'
  }
} finally { Remove-RolloutFixture $incompleteRollbackFixture }

$phases = @('installer','governed_copy','bundle_move','genesis_move','manifest_move','current_version_write','deployed_buildinfo_write','task_action_restore')
foreach ($phase in $phases) {
  $logicalCaseName = "phase-$phase"
  $fixture = New-RolloutFixture -Name $logicalCaseName
  Assert-FixtureCaseIdentity $fixture $logicalCaseName
  try {
    $before = Get-FixtureSnapshot $fixture
    for ($attempt = 1; $attempt -le 2; $attempt++) {
      $env:OASIS7_FIXTURE_INSTALL_ROOT = $fixture.Install
      $env:OASIS7_FIXTURE_RUNTIME_TEMPLATE = $executables.Runtime
      $env:OASIS7_ROLLOUT_FAIL_PHASE = $phase
      # The generated production interface currently consumes the explicit
      # INJECT spelling; retain FAIL_PHASE as the fixture contract alias.
      $env:OASIS7_ROLLOUT_INJECT_FAILURE_PHASE = $phase
      $invocation = Invoke-FixtureRolloutExpectingFailure $fixture.Rollout
      if ($invocation.ExitCode -eq 0) { throw "injected phase unexpectedly succeeded: phase=$phase attempt=$attempt" }
      Assert-Equal (Get-FixtureSnapshot $fixture) $before "rollback failed to restore known-good state: phase=$phase attempt=$attempt"
      Assert-OriginalTaskAction $fixture
      $diagnostics = (Get-ChildItem -LiteralPath $fixture.Deploy -Recurse -File | ForEach-Object { Get-Content -LiteralPath $_.FullName -Raw -ErrorAction SilentlyContinue }) -join "`n"
      foreach ($field in @('failure_phase=', 'rollback_path=', 'rollback_error=', 'rollback_exit_code=', 'rollback_required=true')) {
        if ($diagnostics -notmatch [regex]::Escape($field)) { throw "missing stable rollback diagnostic: phase=$phase field=$field" }
      }
    }
  } finally {
    Remove-Item Env:OASIS7_ROLLOUT_FAIL_PHASE -ErrorAction SilentlyContinue
    Remove-Item Env:OASIS7_ROLLOUT_INJECT_FAILURE_PHASE -ErrorAction SilentlyContinue
    Remove-Item Env:OASIS7_FIXTURE_INSTALL_ROOT -ErrorAction SilentlyContinue
    Remove-Item Env:OASIS7_FIXTURE_RUNTIME_TEMPLATE -ErrorAction SilentlyContinue
    Remove-RolloutFixture $fixture
  }
}

$positiveLogicalCaseName = 'positive-rollout'
$fixture = New-RolloutFixture -Name $positiveLogicalCaseName
Assert-FixtureCaseIdentity $fixture $positiveLogicalCaseName
try {
  $candidateRuntimeHash = Get-Sha256 $executables.Runtime
  $knownGoodRuntimeHash = Get-Sha256 $executables.KnownGoodRuntime
  if ($candidateRuntimeHash -eq $knownGoodRuntimeHash) {
    throw 'positive fixture candidate and known-good runtimes must have distinct hashes'
  }
  Assert-Equal (Get-Sha256 $fixture.Runtime) $knownGoodRuntimeHash 'positive fixture did not begin on known-good runtime'

  $stagedHashesBefore = [System.Collections.Generic.Dictionary[string, string]]::new(
    [System.StringComparer]::OrdinalIgnoreCase
  )
  $activeHashesBefore = [System.Collections.Generic.Dictionary[string, string]]::new(
    [System.StringComparer]::OrdinalIgnoreCase
  )
  Get-ChildItem -LiteralPath $fixture.StagingConfig -Recurse -File | ForEach-Object {
    $relative = Get-FixtureRelativeExistingPath $fixture.StagingConfig $_.FullName 'positive staged governed file'
    $activePath = Join-Path $fixture.Config $relative
    if (!(Test-Path -LiteralPath $activePath -PathType Leaf)) {
      throw "positive fixture active governed baseline missing: relative=$relative active=$activePath"
    }
    $stagedHashesBefore[$relative] = Get-Sha256 $_.FullName
    $activeHashesBefore[$relative] = Get-Sha256 $activePath
  }
  if ($stagedHashesBefore.Count -eq 0) {
    throw 'positive fixture staged governed closure is empty'
  }

  $env:OASIS7_FIXTURE_INSTALL_ROOT = $fixture.Install
  $env:OASIS7_FIXTURE_RUNTIME_TEMPLATE = $executables.Runtime
  Remove-Item Env:OASIS7_ROLLOUT_FAIL_PHASE -ErrorAction SilentlyContinue
  Remove-Item Env:OASIS7_ROLLOUT_INJECT_FAILURE_PHASE -ErrorAction SilentlyContinue
  $invocation = Invoke-FixtureRollout $fixture.Rollout
  $rolloutOutput = $invocation.Output -join "`n"
  [Console]::Error.WriteLine("fixture_positive_rollout logical_case=$positiveLogicalCaseName physical_case=$($fixture.PhysicalCaseId) output=$rolloutOutput")
  if ($invocation.ExitCode -ne 0) {
    throw "positive fixture rollout failed: exit_code=$($invocation.ExitCode) output=$rolloutOutput"
  }
  foreach ($successMarker in @(
    'staged_sha_closure_complete=true',
    'promotion_begin=true',
    'installer_exit_code=0',
    'promotion_complete=true',
    'startup_verified=true'
  )) {
    if ($rolloutOutput -notmatch [regex]::Escape($successMarker)) {
      throw "positive fixture rollout omitted success marker: marker=$successMarker output=$rolloutOutput"
    }
  }
  if ($rolloutOutput -match '(?i)rollback_required\s*=\s*true') {
    throw "positive fixture rollout emitted rollback-required output: $rolloutOutput"
  }

  Assert-Equal (Get-Sha256 $fixture.Runtime) $candidateRuntimeHash 'positive fixture installer did not promote candidate runtime bytes'
  if ($rolloutOutput -notmatch [regex]::Escape("new_runtime_sha256=$candidateRuntimeHash")) {
    throw "positive fixture rollout omitted promoted runtime hash: expected=$candidateRuntimeHash output=$rolloutOutput"
  }
  $transformedGovernedLeaves = @(
    [System.IO.Path]::GetFileName($fixture.ActiveBundle),
    [System.IO.Path]::GetFileName($fixture.ActiveGenesis),
    [System.IO.Path]::GetFileName($fixture.ActiveManifest)
  )
  foreach ($relative in @($stagedHashesBefore.Keys)) {
    $stagedPath = Join-Path $fixture.StagingConfig $relative
    $activePath = Join-Path $fixture.Config $relative
    Assert-Equal (Get-Sha256 $stagedPath) $stagedHashesBefore[$relative] "positive rollout mutated staged governed source: $relative"
    if (!(Test-Path -LiteralPath $activePath -PathType Leaf)) {
      throw "positive rollout omitted promoted governed target: relative=$relative active=$activePath"
    }
    $activeHash = Get-Sha256 $activePath
    if ([System.IO.Path]::GetFileName($relative) -in $transformedGovernedLeaves) {
      if ($activeHash -eq $activeHashesBefore[$relative]) {
        throw "positive rollout did not transform governed config: relative=$relative hash=$activeHash"
      }
    } else {
      Assert-Equal $activeHash $stagedHashesBefore[$relative] "positive rollout governed hash diverged: $relative"
    }
  }
  $provenanceRelative = 'generated-world\world-generation-provenance.json'
  Assert-Equal (Get-Sha256 (Join-Path $fixture.Config $provenanceRelative)) $stagedHashesBefore[$provenanceRelative] 'positive rollout provenance hash diverged'

  Assert-Equal ((Get-Content -LiteralPath (Join-Path $fixture.Deploy 'CURRENT_VERSION') -Raw).Trim()) $env:OASIS7_FIXTURE_PACKAGE_VERSION 'positive rollout current version diverged'
  $deployedBuildInfoLines = @(Get-Content -LiteralPath (Join-Path $fixture.Deploy 'DEPLOYED_BUILDINFO'))
  foreach ($buildInfoLine in @(
    "package_version=$env:OASIS7_FIXTURE_PACKAGE_VERSION",
    "runtime_sha256=$candidateRuntimeHash"
  )) {
    if ($deployedBuildInfoLines -notcontains $buildInfoLine) {
      throw "positive rollout deployment provenance omitted expected line: line=$buildInfoLine buildinfo=$($deployedBuildInfoLines -join ',')"
    }
  }
  Assert-OriginalTaskAction $fixture
  if ((Get-ScheduledTask -TaskName $fixture.Task).State -ne 'Running') {
    throw "positive rollout scheduled task is not running: task=$($fixture.Task)"
  }
  $status = Invoke-RestMethod -Uri "http://127.0.0.1:$($fixture.Port)/v1/chain/status" -TimeoutSec 8
  if ($status.running -ne $true -or $status.fixture -ne 'candidate') {
    throw "positive rollout RPC did not come from promoted candidate runtime: payload=$($status | ConvertTo-Json -Compress)"
  }
  $attemptLogRoot = Join-Path $fixture.Deploy 'logs\package-rollout-attempts'
  $attemptDiagnostics = @(
    Get-ChildItem -LiteralPath $attemptLogRoot -File -ErrorAction Stop | ForEach-Object {
      Get-Content -LiteralPath $_.FullName -Raw -ErrorAction SilentlyContinue
    }
  ) -join "`n"
  if ($attemptDiagnostics -match '(?i)rollback_required\s*=\s*true') {
    throw "positive fixture attempt diagnostics contain rollback-required state: $attemptDiagnostics"
  }
} finally {
  Remove-Item Env:OASIS7_FIXTURE_INSTALL_ROOT -ErrorAction SilentlyContinue
  Remove-Item Env:OASIS7_FIXTURE_RUNTIME_TEMPLATE -ErrorAction SilentlyContinue
  Remove-Item Env:OASIS7_ROLLOUT_FAIL_PHASE -ErrorAction SilentlyContinue
  Remove-Item Env:OASIS7_ROLLOUT_INJECT_FAILURE_PHASE -ErrorAction SilentlyContinue
  Remove-RolloutFixture $fixture
}
[System.IO.File]::WriteAllText(
  $env:OASIS7_FIXTURE_COMPLETION_MARKER,
  'OASIS7_WINDOWS_POWERSHELL_ROLLOUT_BEHAVIOR_COMPLETE=true',
  [System.Text.ASCIIEncoding]::new()
)
} finally {
foreach ($taskName in $fixtureTasks) { Unregister-ScheduledTask -TaskName $taskName -Confirm:$false -ErrorAction SilentlyContinue }
foreach ($providerProcess in $fixtureProviderProcesses) { Stop-Process -Id $providerProcess.Id -Force -ErrorAction SilentlyContinue }
Remove-Item -LiteralPath $fixtureRoot -Recurse -Force -ErrorAction SilentlyContinue
}
PS

if [[ "$(od -An -v -t x1 -N 3 "$powershell_fixture_script")" == *"ef bb bf"* ]]; then
  echo "Windows PowerShell behavior fixture must be UTF-8 without a BOM: $powershell_fixture_script" >&2
  exit 1
fi
powershell_fixture_script_windows="$(cygpath -w "$powershell_fixture_script")"
if [[ "$powershell_fixture_script_windows" != *.ps1 ]]; then
  echo "Windows PowerShell behavior fixture did not retain its .ps1 path: $powershell_fixture_script_windows" >&2
  exit 1
fi

set +e
OASIS7_FIXTURE_WORKSPACE_ROOT="$(cygpath -w "$ROOT_DIR")" \
  OASIS7_FIXTURE_PACKAGE_DIR="$(cygpath -w "$package_dir")" \
  OASIS7_FIXTURE_PACKAGE_VERSION="$package_version" \
  OASIS7_FIXTURE_ROLLOUT_PY="$(cygpath -w "$rollout_driver")" \
  OASIS7_TEST_ROLLOUT_MODULE_PATH="$(cygpath -w "$ROOT_DIR/scripts/p2p-public-testnet-package-rollout.py")" \
  OASIS7_TEST_CLOSURE_FIXTURE="$(cygpath -w "$checkpoint_closure_receipt")" \
  OASIS7_FIXTURE_COMPLETION_MARKER="$(cygpath -w "$powershell_completion_marker")" \
  "$windows_powershell" -NoProfile -ExecutionPolicy Bypass -File "$powershell_fixture_script_windows" >"$powershell_output" 2>&1
powershell_status=$?
set -e
cat "$powershell_output"
require_windows_powershell_behavior_success \
  "$powershell_status" "$powershell_output" "$powershell_completion_marker"
}
verify_windows_powershell_behavior_boundary_fails_closed
python3 - "${BASH_SOURCE[0]}" <<'PY'
from pathlib import Path
import re
import sys

source = Path(sys.argv[1]).read_text(encoding="utf-8")
fixture_match = re.search(
    r"cat >\"\$powershell_fixture_script\" <<'PS'\n(?P<body>.*?)\nPS$",
    source,
    re.MULTILINE | re.DOTALL,
)
assert fixture_match, "could not isolate native Windows PowerShell behavior fixture"
fixture = fixture_match.group("body")
assert "function Get-FixtureCanonicalExistingPath" in fixture
assert "Get-Item -LiteralPath $Path -Force -ErrorAction Stop" in fixture
assert "$item.FullName" in fixture
assert "function Get-FixtureRelativeExistingPath" in fixture
assert "$rootPrefix = $rootCanonical + '\\'" in fixture
assert "fixture relative path escapes canonical root:" in fixture
assert "Get-FixtureRelativeExistingPath $packageWindows $_.FullName 'package governed file'" in fixture
assert "Get-FixtureRelativeExistingPath $stagingConfig $_.FullName 'staged governed file'" in fixture
assert ".Substring($packageWindows.Length)" not in fixture
assert ".Substring($stagingConfig.Length)" not in fixture
projection_start = fixture.index("Get-ChildItem -LiteralPath $packageWindows")
projection_end = fixture.index("| ForEach-Object", projection_start)
projection_filter = fixture[projection_start:projection_end]
for delivery_asset in (
    "oasis7-windows-x64.exe",
    "windows-x64-BUILDINFO",
    "windows-x64-SHA256SUMS",
    "oasis7-windows-x64-ops-tools.tar.gz",
    "windows-x64-ops-tools-SHA256SUMS",
):
    assert f"'{delivery_asset}'" in projection_filter, (
        f"Windows fixture must exclude delivery asset from governed config projection: {delivery_asset}"
    )
assert "Assert-Equal $representativePackageRelative $representativeGovernedLeaf" in fixture
assert "function Get-FixturePhysicalCaseId" in fixture
assert "return 'c-' + $digest.Substring(0, 16)" in fixture
assert "$physicalCaseId.Length -ne 18" in fixture
assert "Get-FixturePhysicalCaseId $Name) -ne $physicalCaseId" in fixture
assert "fixture physical case identifier collision:" in fixture
assert "$root = Join-Path $fixtureRoot $physicalCaseId" in fixture
assert "$root = Join-Path $fixtureRoot $Name" not in fixture
assert "LogicalName=$Name; PhysicalCaseId=$physicalCaseId" in fixture
assert "function Assert-FixtureCaseIdentity" in fixture
assert "GetFileName($Fixture.Root.TrimEnd('\\'))" in fixture
assert fixture.count("Assert-FixtureCaseIdentity $fixture") == 3
assert "fixture_governed_path_budget logical_case=$Name physical_case=$physicalCaseId" in fixture
assert "[System.StringComparison]::OrdinalIgnoreCase" in fixture
assert "reportedReparseItem.Attributes" in fixture
assert "[System.IO.FileAttributes]::ReparsePoint" in fixture
assert "Assert-FixtureSameExistingPath $reportedReparseAncestor $expectedReparseAncestor" in fixture
assert "-notmatch [regex]::Escape($expectedReparseAncestor)" not in fixture, (
    "fixture must not compare raw short/long Windows path spellings"
)
assert "function Assert-FixtureNoReparseAncestor" in fixture
assert "function Assert-FixturePathUnderSafeBase" in fixture
assert "fixture safe-root precondition failed: reparse-point component=" in fixture
assert "fixture generated mutable path escapes verified safe base:" in fixture
assert "Assert-FixturePathUnderSafeBase $staging 'stagingRoot'" in fixture
assert "fixture_generated_staging_root logical_case=$Name physical_case=$physicalCaseId path=$staging" in fixture
assert "fixture_expected_reparse case=$($case.Name) staging_root=$($fixture.Staging)" in fixture
assert "function Invoke-FixtureRolloutExpectingFailure" in fixture
assert "function Invoke-FixtureRollout([string] $Rollout)" in fixture
assert "$ErrorActionPreference = 'Continue'" in fixture
assert "$ErrorActionPreference = $previousErrorActionPreference" in fixture
assert fixture.count("Invoke-FixtureRolloutExpectingFailure $fixture.Rollout") == 2
assert "incomplete-rollback-closure" in fixture
assert "$missingRollbackMember = Join-Path $incompleteRollbackFixture.Rollback 'config/generated-world/world/journal.json'" in fixture
assert "known-good rollback config missing" in fixture
assert "incomplete rollback closure mutated persisted state" in fixture
assert "incomplete rollback closure reached Stop-ScheduledTask" in fixture
assert fixture.count("Invoke-FixtureRollout $fixture.Rollout") == 1
assert "positive fixture candidate and known-good runtimes must have distinct hashes" in fixture
assert "staged_sha_closure_complete=true" in fixture
assert "promotion_complete=true" in fixture
assert "startup_verified=true" in fixture
assert "positive fixture rollout emitted rollback-required output" in fixture
assert "positive rollout provenance hash diverged" in fixture
assert "positive rollout RPC did not come from promoted candidate runtime" in fixture
assert "positive fixture attempt diagnostics contain rollback-required state" in fixture
assert "public static class FixtureProviderStatus" in fixture
assert '"schema_version\\":2' in fixture
assert "$fixtureProviderCheckpointId = 'fixture-checkpoint-v2'" in fixture
assert "$fixtureProviderCheckpointHeight = 4242" in fixture
assert "function Start-FixtureProviderStatusServer" in fixture
assert "function Assert-FixtureProviderStatus" in fixture
assert "name = 'sequencer'; platform = 'linux-x64'" in fixture
assert "name = 'storage'; platform = 'linux-x64'" in fixture
assert "status_url = $fixtureProviderEndpoints.Sequencer" in fixture
assert "status_url = $fixtureProviderEndpoints.Storage" in fixture
assert fixture.count("Assert-FixtureProviderStatus 'sequencer'") == 1
assert fixture.count("Assert-FixtureProviderStatus 'storage'") == 1
assert "foreach ($providerProcess in $fixtureProviderProcesses) { Stop-Process" in fixture
provider_validation = fixture.index("Assert-FixtureProviderStatus 'sequencer' $fixtureProviderEndpoints.Sequencer")
fixture_generation = fixture.index("& python $env:OASIS7_FIXTURE_ROLLOUT_PY --manifest $manifestPath")
assert provider_validation < fixture_generation, (
    "canonical provider checkpoint evidence must be available before every fixture rollout generation"
)
safe_root_check = fixture.index("Assert-FixtureNoReparseAncestor $fixtureDriveRoot\n")
fixture_root_check = fixture.index("Assert-FixtureNoReparseAncestor $fixtureRoot\n")
intentional_junction = fixture.index("New-Item -ItemType Junction")
assert safe_root_check < fixture_root_check < intentional_junction, (
    "native fixture must verify its workspace root before constructing intentional junctions"
)
assert not re.search(r'^OASIS7_FIXTURE_(?:ROOT_DIR|SAFE_ROOT_BASE)=', source, re.MULTILINE)
assert re.search(
    r'^OASIS7_FIXTURE_WORKSPACE_ROOT="\$\(cygpath -w "\$ROOT_DIR"\)" \\$',
    source,
    re.MULTILINE,
)
assert 'Join-Path $fixtureDriveRoot ("o7fx-" + $fixtureRunId)' in fixture
assert "fixture unique drive-root path collision:" in fixture
assert "longest_length=$($longestGovernedPath.Length) budget=$governedPathBudget" in fixture
assert "$atomicPromotionSuffixBudget = 48" in fixture
assert "$windowsLegacyPathStringBudget = 259" in fixture
assert "$governedPathBudget = $windowsLegacyPathStringBudget - $atomicPromotionSuffixBudget" in fixture
assert "$governedPathBudget = 200" not in fixture
assert "$longestGovernedPath.Length -gt $governedPathBudget" in fixture
assert "$longestGovernedPath.Length + $atomicPromotionSuffixBudget" in fixture
assert "required_length=$($requiredActivePath.Length)" in fixture
staging_descendant_check = fixture.index("Assert-FixturePathUnderSafeBase $staging 'stagingRoot'")
generated_script_execution = fixture.index("Invoke-FixtureRolloutExpectingFailure $fixture.Rollout")
assert staging_descendant_check < generated_script_execution, (
    "generated staging root must be proven beneath the safe base before native execution"
)
PY
if [[ "${OASIS7_WINDOWS_POWERSHELL_BEHAVIOR_TEST:-0}" == "1" ]]; then
  run_windows_powershell_behavior_harness
  exit 0
fi

printf 'runtime-v1\n' >"$node_root/releases/old/bin/oasis7_chain_runtime"
chmod +x "$node_root/releases/old/bin/oasis7_chain_runtime"
ln -s "$node_root/releases/old" "$node_root/current"
rollback_backup_fixture="$TMP_DIR/authorized-task-2269-backup"
mkdir -p "$rollback_backup_fixture/runtime" "$rollback_backup_fixture/config"
printf 'authorized-known-good-runtime\n' \
  >"$rollback_backup_fixture/runtime/oasis7_chain_runtime.exe"
cat >"$rollback_backup_fixture/backup-manifest.json" <<'EOF'
{
  "schema_version": "oasis7.windows_observer_backup.v1",
  "runtime": {
    "relative_path": "runtime/oasis7_chain_runtime.exe",
    "sha256": "6069ce6697cf15afc54e127fc748eb8de067cf7f69199125958ab59adf81b69d"
  },
  "provenance": {
    "deployed_buildinfo": "DEPLOYED_BUILDINFO"
  }
}
EOF
printf 'package_version=0.0.0+testnet.157.c62f1c78333f\n' \
  >"$rollback_backup_fixture/DEPLOYED_BUILDINFO"
test -f "$rollback_backup_fixture/runtime/oasis7_chain_runtime.exe"
test ! -e "$rollback_backup_fixture/bin/oasis7_chain_runtime.exe"
actual_rollback_backup_fixture="$TMP_DIR/authorized-task-2269-actual-backup"
mkdir -p "$actual_rollback_backup_fixture/runtime"
cp \
  "$rollback_backup_fixture/runtime/oasis7_chain_runtime.exe" \
  "$actual_rollback_backup_fixture/runtime/oasis7_chain_runtime.exe"
cat >"$actual_rollback_backup_fixture/backup-manifest.json" <<'EOF'
{
  "schema_version": "oasis7.windows_observer_backup.v1",
  "runtime_path": "runtime\\oasis7_chain_runtime.exe",
  "runtime_sha256": "6069ce6697cf15afc54e127fc748eb8de067cf7f69199125958ab59adf81b69d"
}
EOF
test -f "$actual_rollback_backup_fixture/runtime/oasis7_chain_runtime.exe"
test ! -e "$actual_rollback_backup_fixture/bin/oasis7_chain_runtime.exe"
legacy_rollback_backup_fixture="$TMP_DIR/authorized-task-2269-legacy-rooted-backup"
mkdir -p "$legacy_rollback_backup_fixture/runtime"
cp \
  "$rollback_backup_fixture/runtime/oasis7_chain_runtime.exe" \
  "$legacy_rollback_backup_fixture/runtime/oasis7_chain_runtime.exe"
cat >"$legacy_rollback_backup_fixture/backup-manifest.json" <<'EOF'
{
  "schema_version": "oasis7.windows_observer_backup.v1",
  "runtime_path": "C:\\Users\\Observer\\AppData\\Local\\Programs\\oasis7\\bin\\oasis7_chain_runtime.exe",
  "runtime_sha256": "6069ce6697cf15afc54e127fc748eb8de067cf7f69199125958ab59adf81b69d"
}
EOF
test -f "$legacy_rollback_backup_fixture/runtime/oasis7_chain_runtime.exe"
test ! -e "$legacy_rollback_backup_fixture/bin/oasis7_chain_runtime.exe"
cat >"$node_root/config/doc/testing/evidence/public-testnet-governed-bootstrap-bundle-2026-06-06.json" <<'EOF'
{
  "schema_version": "oasis7.release_candidate_bundle.v1",
  "runtime_build": {
    "path": "old",
    "ref": "old",
    "resolved_path": "old",
    "sha256": "0000000000000000000000000000000000000000000000000000000000000000",
    "size_bytes": 1
  }
}
EOF

cat >"$TMP_DIR/manifest.json" <<EOF
{
  "nodes": [
    {
      "name": "sequencer",
      "platform": "linux-x64",
      "node_root": "$node_root",
      "restart": false,
      "status_url": "http://127.0.0.1:6632/v1/chain/status"
    },
    {
      "name": "storage",
      "platform": "linux-x64",
      "host": "198.51.100.44",
      "user": "root",
      "node_root": "/opt/oasis7/p2p-testnet",
      "remote_package": "/tmp/oasis7-linux-x64.deb",
      "remote_ops_tools": "/tmp/oasis7-linux-x64-ops-tools.tar.gz",
      "remote_script": "/opt/oasis7/oasis7/scripts/p2p-public-testnet-package-node-upgrade.sh",
      "restart": true,
      "systemd_service": "oasis7-testnet-storage.service",
      "status_url": "http://127.0.0.1:6632/v1/chain/status"
    },
    {
      "name": "windows-observer",
      "platform": "windows-x64",
      "host": "192.0.2.33",
      "user": "Administrator",
      "deploy_root": "C:\\\\oasis7-deploy",
      "install_root": "C:\\\\Users\\\\Observer\\\\AppData\\\\Local\\\\Programs\\\\oasis7",
      "remote_installer": "C:/oasis7-deploy/oasis7-windows-x64.exe",
      "scheduled_task": "Oasis7Observer",
      "status_url": "http://127.0.0.1:5121/v1/chain/status",
      "rollback_backup_root": "C:\\\\oasis7-deploy\\\\backups\\\\task-2269-fixture",
      "rollback_unlock_timeout_secs": 30
    },
    {
      "name": "macos-observer",
      "platform": "macos-arm64",
      "host": "192.0.2.34",
      "node_root": "/Applications/oasis7",
      "launchd_target": "system/oasis7.testnet.fourth",
      "launchd_plist": "/Library/LaunchDaemons/oasis7.testnet.fourth.plist",
      "runtime_path": "/Applications/oasis7/current/bin/oasis7_chain_runtime",
      "governed_bundle_path": "/Applications/oasis7/public-testnet-governed-bootstrap-bundle-2026-06-06.json",
      "canonical_governed_bundle_path": "/Applications/oasis7/doc/testing/evidence/public-testnet-governed-bootstrap-bundle-2026-06-06.json",
      "config_paths": ["/Applications/oasis7/config/node.env"],
      "persistent_state_paths": ["/Applications/oasis7/data", "/Applications/oasis7/store"],
      "healthz_url": "http://127.0.0.1:19083/healthz",
      "status_url": "http://127.0.0.1:19083/v1/chain/status"
    }
  ]
}
EOF

"$rollout_driver" \
  --manifest "$TMP_DIR/manifest.json" \
  --package-dir "$package_dir" \
  --out-dir "$TMP_DIR/plan-only-out" \
  --known-hosts "$operator_known_hosts" \
  --json >"$TMP_DIR/plan-only.json"

if ! python3 - "$ROOT_DIR/doc/p2p/blockchain/public-testnet-governed-bootstrap.runbook.md" <<'PY'
from pathlib import Path
import sys

runbook = Path(sys.argv[1]).read_text(encoding="utf-8")
start = runbook.index("### Package version replacement")
end = runbook.index("默认模式只生成计划", start)
section = runbook[start:end]
assert "--known-hosts <operator-local-pinned-known-hosts>" in section, (
    "canonical package rollout command must pass an operator-local known-hosts pin"
)
PY
then
  echo "known-hosts RED: canonical runbook command omits operator-local --known-hosts" >&2
  exit 1
fi

# Provider SSH generation must consume an explicit operator-local known-hosts
# file.  The path intentionally contains spaces so the rendered UserKnownHostsFile
# option must survive both the local shell and the later SSH reparse boundary.
known_hosts_plan_out="$TMP_DIR/operator-known-hosts-plan-out"
known_hosts_plan_json="$TMP_DIR/operator-known-hosts-plan.json"
if ! "$rollout_driver" \
  --manifest "$TMP_DIR/manifest.json" \
  --package-dir "$package_dir" \
  --out-dir "$known_hosts_plan_out" \
  --known-hosts "$operator_known_hosts" \
  --json >"$known_hosts_plan_json"; then
  echo "known-hosts RED: expected explicit operator-local --known-hosts path to be accepted" >&2
  exit 1
fi

python3 - \
  "$known_hosts_plan_json" \
  "$operator_known_hosts" <<'PY'
from pathlib import Path
import json
import shlex
import sys

plan = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
known_hosts = str(Path(sys.argv[2]).resolve())
storage = next(node for node in plan["nodes"] if node["name"] == "storage")
command = storage["commands"][0]
tokens = shlex.split(command)
known_hosts_option = f"UserKnownHostsFile={known_hosts}"
assert known_hosts_option in tokens, (
    "provider SSH command must bind the explicit operator-local known-hosts file"
)
assert shlex.quote(known_hosts_option) in command, (
    "UserKnownHostsFile path must be shell-quoted in the rendered provider command"
)
assert "HostKeyAlgorithms=ssh-ed25519" in tokens
assert "StrictHostKeyChecking=yes" in tokens
assert "GlobalKnownHostsFile=/dev/null" in tokens
assert tokens.count("ssh") == 1
assert "scp" not in tokens
assert len(storage["commands"]) == 1
PY

# Missing and non-file operator pins must fail before a plan or provider
# transport can be generated.  The plan-only CLI never executes a provider;
# absence of its output directory is the bounded no-transport assertion.
missing_known_hosts="$TMP_DIR/missing operator known-hosts"
missing_known_hosts_out="$TMP_DIR/missing-known-hosts-out"
if "$rollout_driver" \
  --manifest "$TMP_DIR/manifest.json" \
  --package-dir "$package_dir" \
  --out-dir "$missing_known_hosts_out" \
  --known-hosts "$missing_known_hosts" \
  --json \
  >"$TMP_DIR/missing-known-hosts.stdout" \
  2>"$TMP_DIR/missing-known-hosts.stderr"; then
  echo "known-hosts RED: missing operator pin unexpectedly generated a rollout plan" >&2
  exit 1
fi
grep -Eqi 'known.?hosts|regular|file' "$TMP_DIR/missing-known-hosts.stderr"
test ! -e "$missing_known_hosts_out"

non_file_known_hosts="$TMP_DIR/non-file operator known-hosts"
mkdir -p "$non_file_known_hosts"
non_file_known_hosts_out="$TMP_DIR/non-file-known-hosts-out"
if "$rollout_driver" \
  --manifest "$TMP_DIR/manifest.json" \
  --package-dir "$package_dir" \
  --out-dir "$non_file_known_hosts_out" \
  --known-hosts "$non_file_known_hosts" \
  --json \
  >"$TMP_DIR/non-file-known-hosts.stdout" \
  2>"$TMP_DIR/non-file-known-hosts.stderr"; then
  echo "known-hosts RED: directory operator pin unexpectedly generated a rollout plan" >&2
  exit 1
fi
grep -Eqi 'known.?hosts|regular|file' "$TMP_DIR/non-file-known-hosts.stderr"
test ! -e "$non_file_known_hosts_out"

python3 - \
  "$ROOT_DIR/scripts/p2p-public-testnet-package-rollout.py" \
  "$TMP_DIR/powershell-injection.ps1" <<'PY'
import importlib.util
import json
import os
from pathlib import Path
import re
import sys

module_path = Path(sys.argv[1])
output_path = Path(sys.argv[2])
spec = importlib.util.spec_from_file_location("package_rollout", module_path)
module = importlib.util.module_from_spec(spec)
assert spec.loader is not None
spec.loader.exec_module(module)
closure_receipt = json.loads(
    Path(os.environ["OASIS7_TEST_CLOSURE_FIXTURE"]).read_text(encoding="utf-8")
)
injected = r"C:\safe'; Write-Output INJECTED; #"
node = {
    key: injected
    for key in (
        "deploy_root",
        "staging_root",
        "install_root",
        "installer_path",
        "configured_installer_path",
        "scheduled_task",
        "status_url",
        "governed_bundle_path",
        "governed_genesis_path",
        "governed_manifest_path",
        "governed_bootstrap_path",
        "active_governed_bundle_path",
        "active_governed_genesis_path",
        "active_governed_manifest_path",
        "active_governed_bootstrap_path",
        "rollback_backup_root",
    )
}
text = module.windows_script(
    node,
    injected,
    injected,
    injected,
    injected,
    {injected: "a" * 64},
    "rpc-running",
    "http://127.0.0.1:6631/v1/chain/status",
    "http://127.0.0.1:6632/v1/chain/status",
    closure_receipt,
)
output_path.write_text(text, encoding="utf-8")
assignments = (
    "$version",
    "$commit",
    "$runId",
    "$artifactRef",
    "$installRoot",
    "$installer",
    "$deployRoot",
    "$stagingRoot",
    "$taskName",
    "$bundlePath",
    "$genesisPath",
    "$manifestPath",
    "$bootstrapPath",
    "$activeBundlePath",
    "$activeGenesisPath",
    "$activeManifestPath",
    "$activeBootstrapPath",
    "$rollbackBackupRoot",
    "$statusUrl",
)
for variable in assignments:
    lines = [line for line in text.splitlines() if line.startswith(variable + " =")]
    assert lines, f"missing generated assignment for {variable}"
    for line in lines:
        assert "''; Write-Output INJECTED; #" in line, (
            f"manifest value was not encoded as one PowerShell single-quoted literal: {line}"
        )
        assert not re.search(r"(?<!')'; Write-Output INJECTED", line), (
            f"manifest value escaped its PowerShell literal: {line}"
        )
integrity_lines = [line for line in text.splitlines() if "INJECTED" in line and " = " in line]
assert integrity_lines
assert all("''; Write-Output INJECTED; #" in line for line in integrity_lines)
PY

node_root_abs=$(python3 -c 'import pathlib,sys; print(pathlib.Path(sys.argv[1]).resolve())' "$node_root")
plan_current_target=$(python3 -c 'import pathlib,sys; print(pathlib.Path(sys.argv[1]).resolve())' "$(readlink "$node_root/current")")
test "$plan_current_target" = "$node_root_abs/releases/old"
if ! jq -e '
  (.nodes[] | select(.name == "sequencer") | .applied == false)
  and (.nodes[] | select(.name == "storage") | (.commands | length == 1))
  and (.nodes[] | select(.name == "storage") | .commands[0] | contains("ssh "))
  and (.nodes[] | select(.name == "storage") | .commands[0] | contains("scp ") | not)
  and (.nodes[] | select(.name == "windows-observer") | any(.commands[]; contains("staging_parent_ready=")))
  and (.nodes[] | select(.name == "windows-observer") | .governed_bundle_path | endswith("public-testnet-governed-bootstrap-bundle-2026-06-06.windows.json"))
  and (.nodes[] | select(.name == "windows-observer") | .package_provenance.package_version == "0.0.0+testnet.89.419e119bc897")
  and (.nodes[] | select(.name == "windows-observer") | .package_provenance.run_id == "27605906795")
  and (.nodes[] | select(.name == "macos-observer") | .package_provenance.package_version == "0.0.0+testnet.90.419e119bc897")
  and (.nodes[] | select(.name == "macos-observer") | .package_provenance.run_id == "27605906796")
  and (.nodes[] | select(.name == "macos-observer") | .package_provenance.sha256sums_sha256 | length == 64)' \
  "$TMP_DIR/plan-only.json" >/dev/null; then
  echo "provider transport RED: expected one pinned SSH stream and zero SCP transfers" >&2
  exit 1
fi

observer_gate_manifest="$TMP_DIR/linux-observer-gate-manifest.json"
jq '{nodes: [
  (.nodes[] | select(.name == "sequencer")),
  (.nodes[] | select(.name == "storage")),
  {
    name: "linux-lan-observer",
    platform: "linux-x64",
    host: "192.0.2.44",
    user: "root",
    node_root: "/opt/oasis7/p2p-testnet-local",
    remote_package: "/tmp/oasis7-linux-x64.deb",
    remote_ops_tools: "/tmp/oasis7-linux-x64-ops-tools.tar.gz",
    remote_script: "/opt/oasis7/oasis7/scripts/p2p-public-testnet-package-node-upgrade.sh",
    status_url: "http://127.0.0.1:6633/v1/chain/status",
    restart: false
  }
]}' "$TMP_DIR/manifest.json" >"$observer_gate_manifest"
"$rollout_driver" \
  --manifest "$observer_gate_manifest" \
  --package-dir "$package_dir" \
  --out-dir "$TMP_DIR/linux-observer-gate-out" \
  --known-hosts "$operator_known_hosts" \
  --json >"$TMP_DIR/linux-observer-gate-plan.json"
python3 - "$TMP_DIR/linux-observer-gate-plan.json" <<'PY'
from pathlib import Path
import json
import sys

plan = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
observer = next(node for node in plan["nodes"] if node["name"] == "linux-lan-observer")
assert len(observer["commands"]) == 4
assert observer["commands"][0].startswith("scp ")
assert observer["commands"][1].startswith("scp ")
assert observer["commands"][2].startswith("scp ")
assert observer["commands"][3].startswith("ssh root@192.0.2.44 ")
wrapper = Path(observer["observer_checkpoint_gate_script"])
text = wrapper.read_text(encoding="utf-8")
assert "provider_checkpoint_gate=passed" in text
assert "latest_execution_checkpoint" in text
assert "exec /opt/oasis7/oasis7/scripts/p2p-public-testnet-package-node-upgrade.sh" in text
PY

cross_commit_package="$TMP_DIR/cross-commit-package"
cp -R "$package_dir" "$cross_commit_package"
python3 - "$cross_commit_package/macos/macos-arm64-BUILDINFO" <<'PY'
from pathlib import Path
import sys

path = Path(sys.argv[1])
path.write_text(
    path.read_text(encoding="utf-8").replace(
        "commit=419e119bc897efaa34750bee04c63470d1156699",
        "commit=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    ),
    encoding="utf-8",
)
PY
(
  cd "$cross_commit_package/macos"
  shasum -a 256 oasis7-macos-arm64.dmg macos-arm64-BUILDINFO oasis7-macos-arm64-ops-tools.tar.gz >macos-arm64-SHA256SUMS
)
if "$ROOT_DIR/scripts/p2p-public-testnet-package-rollout.py" \
  --manifest "$TMP_DIR/manifest.json" \
  --package-dir "$cross_commit_package" \
  --out-dir "$TMP_DIR/cross-commit-out" \
  --known-hosts "$operator_known_hosts" \
  >"$TMP_DIR/cross-commit.stdout" 2>"$TMP_DIR/cross-commit.stderr"; then
  echo "expected cross-commit package mix to fail planning" >&2
  exit 1
fi
grep -q 'BUILDINFO commit=' "$TMP_DIR/cross-commit.stderr"

missing_remote_rollback_manifest="$TMP_DIR/missing-remote-rollback-backup-root.json"
jq '(.nodes[] | select(.name == "windows-observer")) |= del(.rollback_backup_root)' \
  "$TMP_DIR/manifest.json" >"$missing_remote_rollback_manifest"
if "$rollout_driver" \
  --manifest "$missing_remote_rollback_manifest" \
  --package-dir "$package_dir" \
  --out-dir "$TMP_DIR/missing-remote-rollback-backup-root-out" \
  --known-hosts "$operator_known_hosts" \
  >"$TMP_DIR/missing-remote-rollback-backup-root.stdout" \
  2>"$TMP_DIR/missing-remote-rollback-backup-root.stderr"; then
  echo "expected remote Windows node without rollback_backup_root to fail plan generation" >&2
  exit 1
fi
grep -q "windows-observer" "$TMP_DIR/missing-remote-rollback-backup-root.stderr"
grep -q "rollback_backup_root" "$TMP_DIR/missing-remote-rollback-backup-root.stderr"

package_contract_failed=0

# A remote Linux provider must receive the package and operator tools through
# one pinned-host SSH session.  The old plan emitted two SCP transfers followed
# by a separate SSH mutation, which made the authentication boundary and the
# streamed payload impossible to attest as one transaction.
if ! python3 - "$TMP_DIR/plan-only.json" "$operator_known_hosts" <<'PY'
from pathlib import Path
import json
import shlex
import sys

plan = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
expected_known_hosts = str(Path(sys.argv[2]).resolve())
storage = next(node for node in plan["nodes"] if node["name"] == "storage")
commands = storage["commands"]
assert len(commands) == 1, (
    "remote Linux provider rollout must use one streamed SSH command, "
    f"got {len(commands)} commands: {commands!r}"
)
command = commands[0]
tokens = shlex.split(command)
ssh_tokens = [token for token in tokens if token == "ssh" or token.endswith("/ssh")]
scp_tokens = [token for token in tokens if token == "scp" or token.endswith("/scp")]
assert len(ssh_tokens) == 1, f"expected exactly one SSH invocation, got {ssh_tokens!r}"
assert not scp_tokens, f"streamed provider rollout must not invoke SCP: {scp_tokens!r}"
assert command.count("root@198.51.100.44") == 1, (
    "provider rollout must authenticate exactly the pinned storage host once"
)
assert "|" in tokens or "<" in tokens, (
    "provider rollout must stream the verified envelope over the SSH session"
)
assert any(marker in command.lower() for marker in ("envelope", "stdin", "tar")), (
    "provider rollout command must identify a streamed envelope"
)
for required in (
    "HostKeyAlgorithms=ssh-ed25519",
    "StrictHostKeyChecking=yes",
    "GlobalKnownHostsFile=/dev/null",
):
    assert required in command, f"missing governed SSH option: {required}"
known_hosts = next(
    (token.split("=", 1)[1] for token in tokens if token.startswith("UserKnownHostsFile=")),
    None,
)
assert known_hosts and known_hosts != "/dev/null", (
    "provider rollout must use a non-empty pinned UserKnownHostsFile"
)
assert known_hosts == expected_known_hosts, (
    "provider rollout must use the explicit operator-local pinned host-key file"
)
assert "StrictHostKeyChecking=no" not in command
assert "StrictHostKeyChecking=accept-new" not in command
PY
then
  package_contract_failed=1
fi

# The streamed envelope is part of the mutation boundary: completeness and
# hash/size validation must be visible before the restart/install operation.
# This is deliberately a plan-level RED contract; it never opens a provider
# connection or touches a node.
if ! python3 - "$TMP_DIR/plan-only.json" <<'PY'
from pathlib import Path
import json
import sys

plan = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
storage = next(node for node in plan["nodes"] if node["name"] == "storage")
command = storage["commands"][0].lower()
assert any(marker in command for marker in ("sha256sum", "sha256", "checksum")), (
    "streamed provider envelope must verify a SHA-256/checksum before mutation"
)
assert any(marker in command for marker in ("expected", "mismatch", " -c ", "compare")), (
    "streamed provider envelope must compare against an expected digest"
)
assert any(marker in command for marker in ("complete", "manifest", "size", "wc -c", "stat")), (
    "streamed provider envelope must reject incomplete or short payloads"
)
mutation_markers = ("--restart-service", "systemctl restart", "dpkg -i")
mutation_indexes = [command.find(marker) for marker in mutation_markers if marker in command]
assert mutation_indexes, "provider command must expose a transactional mutation boundary"
mutation_index = min(mutation_indexes)
for marker in ("sha256", "checksum", "complete", "manifest", "size", "wc -c", "stat"):
    if marker in command:
        assert command.find(marker) < mutation_index, (
            f"envelope guard {marker!r} must precede the mutation boundary"
        )
PY
then
  package_contract_failed=1
fi

# Reparse the generated provider command through a local fake server shell.
# OpenSSH joins remote argv elements with spaces before handing them to the
# server-side shell; an unquoted `bash -c <body>` therefore turns `set -e`
# into an argument to bash and can continue past a failed envelope guard.
# This is a deterministic execution regression: no provider, network, or
# credential is involved, and the sentinel stands in for the mutation helper.
if ! python3 - "$TMP_DIR/plan-only.json" <<'PY'
from pathlib import Path
import json
import shlex
import subprocess
import sys
import tempfile

plan = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
storage = next(node for node in plan["nodes"] if node["name"] == "storage")
command = storage["commands"][0]
argv = shlex.split(command)

try:
    ssh_index = argv.index("ssh")
except ValueError as exc:
    raise AssertionError("linux provider command does not invoke ssh") from exc

try:
    target_index = next(
        index
        for index in range(ssh_index + 1, len(argv))
        if argv[index].startswith("root@")
    )
except StopIteration as exc:
    raise AssertionError("linux provider command has no pinned remote target") from exc

remote_argv = argv[target_index + 1 :]
parsed_remote = shlex.split(" ".join(remote_argv))
if parsed_remote[:2] != ["bash", "-c"]:
    raise AssertionError(
        "linux provider command must use a remote bash -c envelope for this SSH reparse regression"
    )
original_body = parsed_remote[2]


def fake_server_command(injected_body: str) -> str:
    """Model sshd/OpenSSH joining remote argv with spaces before a shell."""

    raw_remote = " ".join(remote_argv)
    if remote_argv == ["bash", "-c", original_body]:
        # Vulnerable shape: an unquoted -c payload is re-parsed by the server
        # shell, so `set -euo pipefail` becomes an argument to bash.
        return " ".join(["bash", "-c", injected_body])

    quoted_body = shlex.quote(original_body)
    if quoted_body not in raw_remote:
        raise AssertionError("cannot locate quoted remote body in SSH argv")
    # Fixed shape: `bash -c '<body>'` remains one remote argv element.
    return raw_remote.replace(quoted_body, shlex.quote(injected_body), 1)


with tempfile.TemporaryDirectory(prefix="oasis7-ssh-reparse-") as tmp:
    sentinel = Path(tmp) / "mutation-sentinel"
    sentinel_literal = shlex.quote(str(sentinel))
    cases = {
        "incomplete-envelope": 'test "${STREAM_ENVELOPE_COMPLETE:-}" = true',
        "tampered-envelope": 'test "payload-tampered" = "payload-original"',
        "hash-mismatch": 'test "actual-sha256" = "expected-sha256"',
    }
    for label, guard in cases.items():
        injected_body = "\n".join(
            [
                "set -euo pipefail",
                guard,
                f"printf '%s' mutation > {sentinel_literal}",
            ]
        )
        result = subprocess.run(
            ["bash", "-c", fake_server_command(injected_body)],
            check=False,
            capture_output=True,
            text=True,
            env={},
        )
        if result.returncode == 0:
            raise AssertionError(
                f"{label}: fake server returned success after failed envelope guard"
            )
        if sentinel.exists():
            raise AssertionError(
                f"{label}: envelope validation failure reached mutation sentinel"
            )
PY
then
  package_contract_failed=1
fi

# Execute the generated one-SSH stream through a local fake ssh/sshd pair.
# The node-upgrade entrypoint directly invokes four sibling helpers; the
# streamed envelope must therefore carry every one before the remote shell
# reaches the mutation boundary.  This RED test reports the missing staged
# helpers while proving no mutation sentinel is reached after server-side
# reparse.
if ! python3 - "$TMP_DIR/plan-only.json" <<'PY'
from pathlib import Path
import json
import os
import shlex
import subprocess
import sys
import tempfile

plan = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
storage = next(node for node in plan["nodes"] if node["name"] == "storage")
command = storage["commands"][0]
argv = shlex.split(command)
ssh_index = argv.index("ssh")
target_index = next(
    index for index in range(ssh_index + 1, len(argv)) if argv[index].startswith("root@")
)
remote_argv = argv[target_index + 1 :]
parsed_remote = shlex.split(" ".join(remote_argv))
if parsed_remote[:2] != ["bash", "-c"]:
    raise AssertionError("expected a remote bash -c envelope")
original_body = parsed_remote[2]
helper_name = "p2p-public-testnet-package-node-upgrade.sh"
required_helpers = (
    "p2p-safe-extract-tar.py",
    "safe_git_archive.py",
    "p2p-safe-validate-deb-tree.py",
    "p2p-verify-linux-package-bundle.py",
    "p2p-rebuild-linux-bundle-checksums.py",
)
if helper_name not in original_body:
    raise AssertionError("generated stream does not invoke the node-upgrade helper")

with tempfile.TemporaryDirectory(prefix="oasis7-streamed-helper-reparse-") as tmp:
    tmp_path = Path(tmp)
    fake_bin = tmp_path / "fake-bin"
    fake_bin.mkdir()
    (fake_bin / "sshpass").write_text(
        "#!/usr/bin/env bash\n"
        "set -euo pipefail\n"
        "[[ \"${1:-}\" == -e && \"${2:-}\" == ssh ]]\n"
        "shift 2\n"
        "exec ssh \"$@\"\n",
        encoding="utf-8",
    )
    (fake_bin / "ssh").write_text(
        "#!/usr/bin/env python3\n"
        "import os\n"
        "import shlex\n"
        "import sys\n"
        "args = sys.argv[1:]\n"
        "while args and args[0] == '-o':\n"
        "    args = args[2:]\n"
        "if not args:\n"
        "    raise SystemExit('fake ssh missing target')\n"
        "remote = shlex.split(' '.join(args[1:]))\n"
        "if remote[:2] != ['bash', '-c']:\n"
        "    raise SystemExit('fake ssh missing remote bash -c')\n"
        "os.execvp(remote[0], remote)\n",
        encoding="utf-8",
    )
    for path in fake_bin.iterdir():
        path.chmod(0o755)

    sentinel = tmp_path / "mutation-sentinel"
    sentinel_literal = shlex.quote(str(sentinel))
    required_helper_literals = " ".join(shlex.quote(name) for name in required_helpers)
    remote_body = "\n".join(
        [
            "set -euo pipefail",
            'stage="$(mktemp -d /tmp/oasis7-package-rollout.XXXXXX)"',
            'trap \'rm -rf "$stage"\' EXIT',
            'tar -xzf - -C "$stage"',
            f'test -f "$stage/{helper_name}"',
            f'for required_helper in {required_helper_literals}; do',
            f'  reference="$stage/{helper_name}"',
            '  if [[ "$required_helper" == safe_git_archive.py ]]; then reference="$stage/p2p-safe-extract-tar.py"; fi',
            '  grep -Fq "$required_helper" "$reference" || {',
            '    printf \'%s\\n\' "node-upgrade helper does not reference $required_helper" >&2',
            "    exit 43",
            "  }",
            f'  if [[ ! -f "$stage/$required_helper" ]]; then',
            f'    printf \'%s\\n\' "missing direct staged helper: $required_helper" >&2',
            "    missing_helpers+=\" $required_helper\"",
            "  fi",
            "done",
            'if [[ -n "${missing_helpers:-}" ]]; then',
            '  printf \'%s\\n\' "direct helper closure incomplete:$missing_helpers" >&2',
            "  exit 42",
            "fi",
            f"printf '%s' mutation > {sentinel_literal}",
        ]
    )
    remote_command = command.replace(
        shlex.quote(remote_argv[2]), shlex.quote(shlex.quote(remote_body)), 1
    )
    env = os.environ.copy()
    env["PATH"] = f"{fake_bin}{os.pathsep}{env['PATH']}"
    env["PUBLIC_TESTNET_STORAGE_SSHPASS"] = "fixture-only"
    result = subprocess.run(
        ["bash", "-c", remote_command],
        check=False,
        capture_output=True,
        text=True,
        env=env,
    )
    if result.returncode != 0:
        if "direct helper closure incomplete:" in result.stderr:
            if sentinel.exists():
                raise AssertionError(
                    "direct helper closure failure reached the mutation sentinel"
                )
            print(
                f"{result.stderr.strip()} mutation_sentinel=False",
                file=sys.stderr,
            )
            raise SystemExit(42)
        raise AssertionError(
            "remote streamed-envelope reparse failed for an unexpected reason: "
            f"rc={result.returncode} stdout={result.stdout!r} stderr={result.stderr!r}"
        )
    if not sentinel.exists():
        raise AssertionError("streamed-envelope reparse did not reach its mutation sentinel")
PY
then
  package_contract_failed=1
fi

if ! python3 - \
  "$TMP_DIR/plan-only.json" \
  "$TMP_DIR/plan-only-out/macos-observer-macos-arm64-upgrade.sh" \
  "$package_dir/macos/oasis7-macos-arm64.dmg" \
  "$ROOT_DIR/doc/testing/evidence/public-testnet-five-node-inventory-2026-06-23.md" \
  "$ROOT_DIR/doc/p2p/blockchain/public-testnet-governed-bootstrap.runbook.md" \
  "$ROOT_DIR/scripts/p2p-public-testnet-package-rollout.py" <<'PY'
from pathlib import Path
import contextlib
import hashlib
import importlib.util
import io
import json
import re
import shlex
import subprocess
import sys
import os
import shutil
import tempfile

def fixture_tree_metadata(path):
    digest = hashlib.sha256()
    files = sorted(item for item in path.rglob("*") if item.is_file())
    total_bytes = 0
    for item in files:
        payload = item.read_bytes()
        total_bytes += len(payload)
        digest.update(item.relative_to(path).as_posix().encode())
        digest.update(b"\0")
        digest.update(hashlib.sha256(payload).hexdigest().encode())
        digest.update(b"\0")
        digest.update(str(len(payload)).encode())
        digest.update(b"\n")
    return digest.hexdigest(), len(files), total_bytes

plan = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
script_path = Path(sys.argv[2])
dmg_path = Path(sys.argv[3])
inventory_text = Path(sys.argv[4]).read_text(encoding="utf-8")
runbook_text = Path(sys.argv[5]).read_text(encoding="utf-8")
module_path = Path(sys.argv[6])
macos = next(node for node in plan["nodes"] if node["name"] == "macos-observer")
assert Path(macos["macos_script"]).resolve() == script_path.resolve(), (
    "macOS arm64 observer must have a generated operator script, not a verification-only note"
)
assert macos["native_identity"] == "aarch64-apple-darwin"
assert macos["state_sync_escalation"] == "explicit_on_authority_failure"
expected_dmg_sha256 = hashlib.sha256(dmg_path.read_bytes()).hexdigest()
assert macos["expected_dmg_sha256"] == expected_dmg_sha256
text = script_path.read_text(encoding="utf-8")
subprocess.run(["bash", "-n", str(script_path)], check=True)
spec = importlib.util.spec_from_file_location("package_rollout_macos_behavior", module_path)
module = importlib.util.module_from_spec(spec)
assert spec.loader is not None
spec.loader.exec_module(module)
closure_receipt = json.loads(
    Path(os.environ["OASIS7_TEST_CLOSURE_FIXTURE"]).read_text(encoding="utf-8")
)

# Execute the generated macOS rollback path against deterministic command mocks.
# The active root-level bundle deliberately uses the legacy flat metadata shape;
# promotion is then failed while writing the reconstructed full bundle.
with tempfile.TemporaryDirectory(prefix="oasis7-macos-rollback-") as tmp_raw:
    tmp = Path(tmp_raw).resolve()
    node_root = tmp / "node"
    runtime = node_root / "current/bin/oasis7_chain_runtime"
    config = node_root / "config/node.env"
    state_paths = [node_root / "data", node_root / "store"]
    evidence = node_root / "doc/testing/evidence"
    canonical = evidence / "public-testnet-governed-bootstrap-bundle-2026-06-06.json"
    legacy = node_root / "public-testnet-governed-bootstrap-bundle-2026-06-06.json"
    launchd_plist = tmp / "oasis7.testnet.plist"
    for path in [runtime.parent, config.parent, evidence, *state_paths]:
        path.mkdir(parents=True, exist_ok=True)
    runtime.write_bytes(b"runtime-before\\n")
    runtime.chmod(0o755)
    config.write_bytes(b"config-before\\n")
    for index, state_path in enumerate(state_paths):
        (state_path / "state.bin").write_bytes(f"state-before-{index}\\n".encode())
    (node_root / "CURRENT_VERSION").write_text("before\\n", encoding="utf-8")
    (node_root / "DEPLOYED_BUILDINFO").write_text("before-build\\n", encoding="utf-8")
    world = node_root / "world"
    world.mkdir(parents=True)
    (world / "snapshot.json").write_text("world snapshot\\n", encoding="utf-8")
    (world / "journal.json").write_text("world journal\\n", encoding="utf-8")
    sidecar = node_root / "generated-world/generated-scenario-world"
    sidecar.mkdir(parents=True)
    (sidecar / "snapshot.json").write_text("sidecar snapshot\\n", encoding="utf-8")
    (sidecar / "journal.json").write_text("sidecar journal\\n", encoding="utf-8")
    provenance = node_root / "generated-world/world-generation-provenance.json"
    provenance.write_text("{\"fixture\":true}\\n", encoding="utf-8")
    governance = evidence / "public-testnet-governed-bootstrap-validator-registry-2026-06-06.json"
    governance.write_text("{}\\n", encoding="utf-8")
    topology = evidence / "topology.md"
    topology.write_text("topology\\n", encoding="utf-8")
    world_hash, world_count, world_bytes = fixture_tree_metadata(world)
    sidecar_hash, sidecar_count, sidecar_bytes = fixture_tree_metadata(sidecar)
    canonical.write_text(json.dumps({
        "schema_version": "oasis7.release_candidate_bundle.v1",
        "runtime_build": {"ref": "old-runtime", "resolved_path": "/build-host/runtime", "kind": "file", "sha256": "0" * 64, "size_bytes": 1},
        "world_snapshot": {"ref": ".tmp/build/world", "resolved_path": "/build-host/world", "kind": "directory", "sha256_tree": world_hash, "file_count": world_count, "total_bytes": world_bytes},
        "generated_world_sidecar": {"ref": ".tmp/build/generated-scenario-world", "resolved_path": "/build-host/generated-scenario-world", "kind": "directory", "sha256_tree": sidecar_hash, "file_count": sidecar_count, "total_bytes": sidecar_bytes},
        "world_generation_provenance": {"ref": ".tmp/build/world-generation-provenance.json", "resolved_path": "/build-host/world-generation-provenance.json", "kind": "file", "sha256": hashlib.sha256(provenance.read_bytes()).hexdigest(), "size_bytes": provenance.stat().st_size},
        "governance_manifest": {"ref": ".tmp/build/validator-registry.json", "resolved_path": "/build-host/governance.json", "kind": "file", "sha256": hashlib.sha256(governance.read_bytes()).hexdigest(), "size_bytes": governance.stat().st_size},
        "evidence_refs": [
            {"ref": "deployment-truth.md", "resolved_path": "/build-host/deployment-truth.md", "kind": "file"},
            {"ref": ".tmp/build/topology.md", "resolved_path": "/build-host/topology.md", "kind": "file", "sha256": hashlib.sha256(topology.read_bytes()).hexdigest(), "size_bytes": topology.stat().st_size},
        ],
    }) + "\n", encoding="utf-8")
    legacy_bytes = json.dumps({"path": str(runtime), "ref": "legacy-runtime", "resolved_path": str(runtime), "sha256": "1" * 64, "size_bytes": 15}, indent=2).encode() + b"\n"
    legacy.write_bytes(legacy_bytes)
    launchd_plist.write_text("fixture", encoding="utf-8")
    package_runtime = tmp / "candidate-runtime"
    package_runtime.write_bytes(b"runtime-candidate\\n")
    package_runtime.chmod(0o755)
    dmg = tmp / "fixture.dmg"
    dmg.write_bytes(b"fixture-dmg\\n")
    mocks = tmp / "mocks"
    mocks.mkdir()
    def mock(name, body):
        path = mocks / name
        path.write_text("#!/usr/bin/env bash\nset -euo pipefail\n" + body.replace("\\n", "\n"), encoding="utf-8")
        path.chmod(0o755)
    mock("uname", '[[ "$1" == "-s" ]] && echo Darwin || echo arm64\\n')
    mock("sysctl", 'echo 1\\n')
    mock("lipo", 'echo arm64\\n')
    mock("stat", '[[ "$1" == "-f" ]] && wc -c < "$3" | tr -d "[:space:]" || /usr/bin/stat "$@"\\n')
    mock("ditto", 'cp -R "$1" "$2"\\n')
    mock("hdiutil", 'if [[ "$1" == attach ]]; then for ((i=1;i<=$#;i++)); do [[ "${!i}" == -mountpoint ]] && j=$((i+1)) && mkdir -p "${!j}" && cp "$FIXTURE_PACKAGE_RUNTIME" "${!j}/oasis7_chain_runtime"; done; fi; true\\n')
    mock("launchctl", 'echo "$*" >> "$FIXTURE_LAUNCH_LOG"; case "$1" in print) if [[ "${FIXTURE_FAIL_POST_BOOTSTRAP_PRINT:-}" == 1 && -f "$FIXTURE_POST_BOOTSTRAP_MARKER" && ! -f "$FIXTURE_POST_PRINT_FAILED" ]]; then : > "$FIXTURE_POST_PRINT_FAILED"; exit 1; fi; exit 0;; bootout) rm -f "$FIXTURE_LAUNCH_LOADED";; bootstrap) if [[ "${FIXTURE_FAIL_BOOTSTRAP_ONCE:-}" == 1 && ! -f "$FIXTURE_BOOTSTRAP_FAILED" ]]; then : > "$FIXTURE_BOOTSTRAP_FAILED"; exit 1; fi; : > "$FIXTURE_LAUNCH_LOADED"; : > "$FIXTURE_POST_BOOTSTRAP_MARKER";; esac\\n')
    mock("curl", """if [[ "$*" == *" -o "* ]]; then
  for ((i=1;i<=$#;i++)); do
    [[ "${!i}" == -o ]] && j=$((i+1)) && printf '{"chain_proof":{"latest_execution_checkpoint":{"schema_version":2,"checkpoint_id":"fixture","height":1,"manifest_hash":"0000000000000000000000000000000000000000000000000000000000000000"}}}' > "${!j}"
  done
elif [[ "$*" == *healthz* ]]; then
  printf '{"ok":true}'
elif [[ -n "${FIXTURE_STATUS_JSON+x}" ]]; then
  printf '%s' "$FIXTURE_STATUS_JSON"
else
  printf '%s' '{"running":true,"consensus":{"state_sync_fallback_required":false},"observability":{"network_head_available":true,"alerts":[]},"consensus_progress_observer_error":null,"last_error":null}'
fi
true
""")
    mock("plutil", r'''python3 - "$@" <<'PY2'
import json, os, sys
args = sys.argv[1:]
if args[0] == "-extract":
    key, output, path = args[1], args[2], args[-1]
    try:
        value = json.load(open(path))
        for part in key.split("."):
            value = value[int(part)] if isinstance(value, list) else value[part]
    except Exception:
        raise SystemExit(1)
    print(json.dumps(value) if output == "json" else str(value).lower() if isinstance(value, bool) else value)
elif args[0] in ("-replace", "-insert"):
    key, value, path = args[1], args[3], args[-1]
    if os.environ.get("FIXTURE_FAIL_METADATA_PROMOTION") == "1" and key == "runtime_build.path":
        raise SystemExit(1)
    data = json.load(open(path))
    target = data
    parts = key.split(".")
    for part in parts[:-1]:
        target = target[int(part)] if isinstance(target, list) else target.setdefault(part, {})
    target[parts[-1]] = int(value) if args[2] == "-integer" else value
    open(path, "w").write(json.dumps(data))
elif args[0] == "-convert":
    open(args[3], "w").write(open(args[4]).read())
else:
    raise SystemExit(1)
PY2
''')
    mock("PlistBuddy", 'echo oasis7.testnet.fixture\\n')
    generated = tmp / "rollout.sh"
    node = {
        "name": "fixture", "node_root": str(node_root), "launchd_target": "system/oasis7.testnet.fixture",
        "launchd_plist": str(launchd_plist), "runtime_path": str(runtime), "governed_bundle_path": str(legacy),
        "healthz_url": "http://fixture/healthz", "status_url": "http://fixture/v1/chain/status",
        "canonical_governed_bundle_path": str(canonical),
        "config_paths": [str(config)], "persistent_state_paths": [str(path) for path in state_paths],
    }
    macos_fixture_closure_receipt = {
        **closure_receipt,
        "checkpoint_id": "fixture",
        "manifest_hash": "0" * 64,
        "height": 1,
    }
    generated.write_text(module.macos_script(node, "fixture", "commit", "run", hashlib.sha256(dmg.read_bytes()).hexdigest(), "http://fixture/sequencer/v1/chain/status", "http://fixture/storage/v1/chain/status", macos_fixture_closure_receipt).replace("/usr/libexec/PlistBuddy", "PlistBuddy"), encoding="utf-8")
    generated.chmod(0o755)
    launch_loaded, launch_log = tmp / "loaded", tmp / "launch.log"
    bootstrap_failed, post_bootstrap_marker, post_print_failed = tmp / "bootstrap.failed", tmp / "post-bootstrap", tmp / "post-print.failed"
    launch_loaded.write_text("loaded", encoding="utf-8")
    before_runtime, before_config = runtime.read_bytes(), config.read_bytes()
    before_current = (node_root / "CURRENT_VERSION").read_bytes()
    before_deployed = (node_root / "DEPLOYED_BUILDINFO").read_bytes()
    before_states = [(path / "state.bin").read_bytes() for path in state_paths]
    rollout_env = {**os.environ, "PATH": f"{mocks}:{os.environ['PATH']}", "FIXTURE_PACKAGE_RUNTIME": str(package_runtime), "FIXTURE_LAUNCH_LOADED": str(launch_loaded), "FIXTURE_LAUNCH_LOG": str(launch_log), "FIXTURE_BOOTSTRAP_FAILED": str(bootstrap_failed), "FIXTURE_POST_BOOTSTRAP_MARKER": str(post_bootstrap_marker), "FIXTURE_POST_PRINT_FAILED": str(post_print_failed)}
    def assert_preupgrade_restored(label):
        assert runtime.read_bytes() == before_runtime, label
        assert config.read_bytes() == before_config, label
        assert legacy.read_bytes() == legacy_bytes, label
        assert (node_root / "CURRENT_VERSION").read_bytes() == before_current, label
        assert (node_root / "DEPLOYED_BUILDINFO").read_bytes() == before_deployed, label
        assert [(path / "state.bin").read_bytes() for path in state_paths] == before_states, label

    result = subprocess.run(["bash", str(generated), str(dmg)], text=True, capture_output=True, env={**rollout_env, "FIXTURE_FAIL_METADATA_PROMOTION": "1"})
    assert result.returncode != 0, "injected metadata promotion failure must fail rollout"
    assert "rollback_begin=true reason=fatal_error" in result.stdout, (
        f"metadata failure bypassed rollback: stdout={result.stdout!r} stderr={result.stderr!r}"
    )
    assert_preupgrade_restored("metadata promotion die rollback")
    assert launch_loaded.exists() and "bootstrap" in launch_log.read_text(encoding="utf-8")

    canonical_bytes = canonical.read_bytes()
    incomplete = json.loads(canonical_bytes)
    incomplete.pop("generated_world_sidecar")
    canonical.write_text(json.dumps(incomplete), encoding="utf-8")
    bootout_count_before = launch_log.read_text(encoding="utf-8").count("bootout")
    incomplete_result = subprocess.run(["bash", str(generated), str(dmg)], text=True, capture_output=True, env=rollout_env)
    assert incomplete_result.returncode != 0 and "canonical governed bundle schema invalid" in incomplete_result.stderr
    assert launch_log.read_text(encoding="utf-8").count("bootout") == bootout_count_before
    canonical.write_bytes(canonical_bytes)

    for missing_path, label in ((world, "world_snapshot"), (sidecar, "generated_world_sidecar"), (provenance, "world_generation_provenance"), (governance, "governance_manifest")):
        bootout_count_before = launch_log.read_text(encoding="utf-8").count("bootout")
        missing_was_dir = missing_path.is_dir()
        if missing_was_dir:
            saved_members = {
                member.relative_to(missing_path): member.read_bytes()
                for member in missing_path.rglob("*") if member.is_file()
            }
            shutil.rmtree(missing_path)
        else:
            saved_bytes = missing_path.read_bytes()
            missing_path.unlink()
        missing_required = subprocess.run(["bash", str(generated), str(dmg)], text=True, capture_output=True, env=rollout_env)
        assert missing_required.returncode != 0 and "required node-local artifacts invalid" in missing_required.stderr, label
        assert launch_log.read_text(encoding="utf-8").count("bootout") == bootout_count_before, f"{label} absence must fail before launchd stop"
        if missing_was_dir:
            for relative, payload in saved_members.items():
                target = missing_path / relative
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_bytes(payload)
        else:
            missing_path.write_bytes(saved_bytes)

    world_real = node_root / "world.real"
    world.rename(world_real)
    outside_world = tmp / "outside-world"
    outside_world.mkdir()
    (outside_world / "snapshot.json").write_text("outside snapshot\n", encoding="utf-8")
    (outside_world / "journal.json").write_text("outside journal\n", encoding="utf-8")
    world.symlink_to(outside_world, target_is_directory=True)
    bootout_count_before = launch_log.read_text(encoding="utf-8").count("bootout")
    symlinked_world = subprocess.run(
        ["bash", str(generated), str(dmg)], text=True, capture_output=True, env=rollout_env
    )
    assert symlinked_world.returncode != 0 and "required node-local artifacts invalid" in symlinked_world.stderr
    assert launch_log.read_text(encoding="utf-8").count("bootout") == bootout_count_before, (
        "symlinked world must fail before launchd stop"
    )
    world.unlink()

    world.write_text("not a directory\n", encoding="utf-8")
    bootout_count_before = launch_log.read_text(encoding="utf-8").count("bootout")
    file_world = subprocess.run(
        ["bash", str(generated), str(dmg)], text=True, capture_output=True, env=rollout_env
    )
    assert file_world.returncode != 0 and "required node-local artifacts invalid" in file_world.stderr
    assert launch_log.read_text(encoding="utf-8").count("bootout") == bootout_count_before, (
        "regular-file world must fail before launchd stop"
    )
    world.unlink()
    world_real.rename(world)

    # The live world is mutable chain state: it may advance after the governed
    # bootstrap snapshot was created, so its bootstrap tree metadata is
    # provenance rather than a preflight equality constraint.
    (world / "post-bootstrap-state.json").write_text("evolved live world\n", encoding="utf-8")
    bootout_count_before = launch_log.read_text(encoding="utf-8").count("bootout")
    evolved_world = subprocess.run(
        ["bash", str(generated), str(dmg)], text=True, capture_output=True,
        env={**rollout_env, "FIXTURE_FAIL_METADATA_PROMOTION": "1"},
    )
    assert evolved_world.returncode != 0 and "required node-local artifacts invalid" not in evolved_world.stderr, (
        "evolved mutable live world must pass bootstrap integrity preflight: "
        f"stdout={evolved_world.stdout!r} stderr={evolved_world.stderr!r}"
    )
    assert "rollback_begin=true reason=fatal_error" in evolved_world.stdout
    assert launch_log.read_text(encoding="utf-8").count("bootout") > bootout_count_before
    assert_preupgrade_restored("evolved mutable world rollback")

    for tamper_path, label in (
        (sidecar / "journal.json", "generated_world_sidecar"),
        (provenance, "world_generation_provenance"),
        (governance, "governance_manifest"),
    ):
        original = tamper_path.read_bytes()
        tamper_path.write_bytes(original + b"tampered")
        bootout_count_before = launch_log.read_text(encoding="utf-8").count("bootout")
        tampered = subprocess.run(["bash", str(generated), str(dmg)], text=True, capture_output=True, env=rollout_env)
        assert tampered.returncode != 0 and "required node-local artifacts invalid" in tampered.stderr, label
        assert launch_log.read_text(encoding="utf-8").count("bootout") == bootout_count_before, label
        tamper_path.write_bytes(original)

    for marker in (post_bootstrap_marker, post_print_failed, bootstrap_failed):
        marker.unlink(missing_ok=True)
    post_mv_die = subprocess.run(
        ["bash", str(generated), str(dmg)], text=True, capture_output=True,
        env={**rollout_env, "FIXTURE_FAIL_POST_BOOTSTRAP_PRINT": "1"},
    )
    assert post_mv_die.returncode != 0 and "rollback_begin=true reason=fatal_error" in post_mv_die.stdout
    assert "governed_bundle_runtime_metadata_verified=true" in post_mv_die.stdout
    assert_preupgrade_restored("post-bundle-mv die rollback")

    for marker in (post_bootstrap_marker, post_print_failed, bootstrap_failed):
        marker.unlink(missing_ok=True)
    direct_err = subprocess.run(
        ["bash", str(generated), str(dmg)], text=True, capture_output=True,
        env={**rollout_env, "FIXTURE_FAIL_BOOTSTRAP_ONCE": "1"},
    )
    assert direct_err.returncode != 0 and "rollback_begin=true reason=unexpected_failure" in direct_err.stdout
    assert "governed_bundle_runtime_metadata_verified=true" in direct_err.stdout
    assert_preupgrade_restored("direct ERR trap rollback")

    for marker in (post_bootstrap_marker, post_print_failed, bootstrap_failed):
        marker.unlink(missing_ok=True)
    success = subprocess.run(["bash", str(generated), str(dmg)], text=True, capture_output=True, env=rollout_env)
    assert success.returncode == 0, f"legacy flat success failed: stdout={success.stdout!r} stderr={success.stderr!r}"
    promoted = json.loads(legacy.read_text(encoding="utf-8"))
    assert promoted["runtime_build"]["resolved_path"] == str(runtime)
    assert promoted["world_snapshot"]["resolved_path"] == str(world)
    assert promoted["generated_world_sidecar"]["resolved_path"] == str(sidecar)
    assert promoted["world_generation_provenance"]["resolved_path"] == str(provenance)
    assert promoted["governance_manifest"]["resolved_path"] == str(governance)
    assert promoted["evidence_refs"][0]["deployment_status"] == "optional_unresolved"
    assert promoted["evidence_refs"][0]["resolved_path"] == ""
    assert promoted["evidence_refs"][1]["deployment_status"] == "localized"
    assert promoted["evidence_refs"][1]["resolved_path"] == str(topology)
    assert "/build-host/" not in json.dumps(promoted)
    if sys.platform == "darwin" and Path("/usr/bin/plutil").is_file():
        native_active = tmp / "native-active-governed-bundle.json"
        native_active_json = json.loads(canonical.read_text(encoding="utf-8"))
        native_active_json["runtime_build"]["path"] = "/build-host/runtime"
        native_active.write_text(json.dumps(native_active_json), encoding="utf-8")
        canonical_before = hashlib.sha256(canonical.read_bytes()).hexdigest()
        active_before = hashlib.sha256(native_active.read_bytes()).hexdigest()
        read_helpers = text[
            text.index("runtime_sha256() {"):text.index("promote_governed_bundle_runtime_metadata() {")
        ]
        native_preflight = tmp / "native-plutil-readonly-preflight.sh"
        native_preflight.write_text(
            "#!/usr/bin/env bash\n"
            "set -euo pipefail\n"
            "plutil() { /usr/bin/plutil \"$@\"; }\n"
            f"NODE_ROOT={shlex.quote(str(node_root))}\n"
            f"CANONICAL_GOVERNED_BUNDLE_PATH={shlex.quote(str(canonical))}\n"
            f"GOVERNED_BUNDLE_PATH={shlex.quote(str(native_active))}\n"
            "RUNTIME_PATH=/build-host/runtime\n"
            "RUNTIME_ARTIFACT_REF=old-runtime\n\n"
            + read_helpers
            + "\n\"$@\"\n",
            encoding="utf-8",
        )
        native_preflight.chmod(0o755)
        for native_helper in (
            ("preflight_governed_bundle_schema",),
            ("verify_full_governed_bundle_schema", str(canonical)),
            ("verify_required_node_local_bundle_artifacts", str(canonical)),
            ("verify_governed_bundle_runtime_metadata", "0" * 64, "1"),
        ):
            native_result = subprocess.run(
                ["bash", str(native_preflight), *native_helper], text=True, capture_output=True
            )
            assert native_result.returncode == 0, (
                "native macOS plutil readonly helper failed: "
                f"helper={native_helper[0]} stdout={native_result.stdout!r} stderr={native_result.stderr!r}"
            )
            assert hashlib.sha256(canonical.read_bytes()).hexdigest() == canonical_before, (
                "native macOS plutil helper mutated canonical governed bundle: " + native_helper[0]
            )
            assert hashlib.sha256(native_active.read_bytes()).hexdigest() == active_before, (
                "native macOS plutil helper mutated active governed bundle: " + native_helper[0]
            )
    status_helper = text[text.index("status_rollout_state() {"):text.index("wait_for_service_health() {")]
    status_helper_path = tmp / "status-rollout-state.sh"
    status_helper_path.write_text(
        "#!/usr/bin/env bash\nset -uo pipefail\n" + status_helper
        + "\nstatus_rollout_state \"$(cat \"$1\")\"\n",
        encoding="utf-8",
    )
    status_helper_path.chmod(0o755)
    def status_rollout_result(payload):
        fixture = tmp / "status-rollout-state.json"
        fixture.write_text(json.dumps(payload) if isinstance(payload, dict) else payload, encoding="utf-8")
        return subprocess.run(
            ["bash", str(status_helper_path), str(fixture)], text=True, capture_output=True,
            env=rollout_env,
        )

    healthy_status = {
        "running": True,
        "consensus": {"state_sync_fallback_required": False},
        "observability": {"network_head_available": True, "alerts": []},
        "consensus_progress_observer_error": None,
        "last_error": None,
    }
    assert status_rollout_result(healthy_status).returncode == 0, "false state-sync fallback must not trigger rollback"
    if sys.platform == "darwin" and Path("/usr/bin/plutil").is_file():
        native_status_result = subprocess.run(
            ["bash", str(status_helper_path), str(tmp / "status-rollout-state.json")],
            text=True,
            capture_output=True,
            env=os.environ.copy(),
        )
        assert native_status_result.returncode == 0, (
            "native plutil rejected healthy status semantics: "
            f"stdout={native_status_result.stdout!r} stderr={native_status_result.stderr!r}"
        )
    for failure_status in (
        {**healthy_status, "consensus": {"state_sync_fallback_required": True}},
        {"consensus": {"state_sync_fallback_required": True}},
        {**healthy_status, "consensus": {"state_sync_fallback_required": True}, "observability": {"network_head_available": False, "alerts": []}},
        {**healthy_status, "consensus": {"state_sync_fallback_required": True}, "observability": {"network_head_available": True, "alerts": [{"code": "consensus_peer_head_unavailable"}]}},
        {**healthy_status, "observability": {"network_head_available": True, "alerts": [{"code": "authority_failure"}]}},
        {**healthy_status, "observability": {"network_head_available": True, "alerts": [{"code": "execution_driver_peer_mismatch"}]}},
        {**healthy_status, "consensus_progress_observer_error": "execution driver peer mismatch at height 42"},
        {**healthy_status, "last_error": "authority_failure while reconnecting peers"},
    ):
        assert status_rollout_result(failure_status).returncode == 2, failure_status
    transient_alert = {"code": "consensus_peer_head_unavailable"}
    for fatal_alert_code in ("authority_failure", "execution_driver_peer_mismatch"):
        fatal_alert = {"code": fatal_alert_code}
        for ordered_alerts in (
            [transient_alert, fatal_alert],
            [fatal_alert, transient_alert],
        ):
            fatal_order_status = {
                **healthy_status,
                "observability": {
                    "network_head_available": False,
                    "alerts": ordered_alerts,
                },
            }
            assert status_rollout_result(fatal_order_status).returncode == 2, fatal_order_status
    for fatal_last_error in (
        "authority_failure while reconnecting peers",
        "execution driver peer mismatch at height 42",
    ):
        transient_with_fatal_error = {
            **healthy_status,
            "observability": {
                "network_head_available": False,
                "alerts": [transient_alert],
            },
            "last_error": fatal_last_error,
        }
        assert status_rollout_result(transient_with_fatal_error).returncode == 2, transient_with_fatal_error
    for retry_status in (
        {**healthy_status, "observability": {"network_head_available": False, "alerts": []}},
        {**healthy_status, "observability": {"network_head_available": True, "alerts": [{"code": "consensus_peer_head_unavailable"}]}},
    ):
        assert status_rollout_result(retry_status).returncode == 1, retry_status
    for invalid_status in (
        "not-json",
        {**healthy_status, "running": False},
        {"running": True},
    ):
        assert status_rollout_result(invalid_status).returncode == 1, invalid_status

    promotion_loop = text[text.index("authority_failure=0"):].replace(
        "deadline=$((SECONDS + 120))",
        "deadline=$((SECONDS + 2))",
        1,
    )
    readiness_timeout_path = tmp / "readiness-timeout-path.sh"
    readiness_timeout_path.write_text(
        "#!/usr/bin/env bash\nset -uo pipefail\n"
        + status_helper
        + "\nHEALTHZ_URL=http://fixture/healthz\n"
        + "STATUS_URL=http://fixture/status\n"
        + "curl() {\n"
        + "  if [[ \"${!#}\" == \"$STATUS_URL\" ]]; then\n"
        + "    printf '%s' '{\"running\":true,\"consensus\":{\"state_sync_fallback_required\":false},\"observability\":{\"network_head_available\":false,\"alerts\":[{\"code\":\"consensus_peer_head_unavailable\"}]},\"consensus_progress_observer_error\":null,\"last_error\":null}'\n"
        + "  else\n"
        + "    printf '%s' '{\"ok\":false}'\n"
        + "  fi\n"
        + "}\n"
        + "sleep() { SECONDS=$((SECONDS + $1)); }\n"
        + "rollback() { printf 'rollback_reason=%s\\n' \"$1\"; }\n"
        + "die() { printf 'die_message=%s\\n' \"$1\" >&2; exit 1; }\n"
        + promotion_loop,
        encoding="utf-8",
    )
    readiness_timeout_path.chmod(0o755)
    timeout_result = subprocess.run(
        ["bash", str(readiness_timeout_path)],
        text=True,
        capture_output=True,
        env=rollout_env,
        timeout=10,
    )
    assert timeout_result.returncode == 1, timeout_result
    assert "rollback_reason=readiness_timeout" in timeout_result.stdout, timeout_result
    assert "state_sync_escalation_required=true" not in timeout_result.stdout, timeout_result
    assert "macOS observer readiness timed out" in timeout_result.stderr, timeout_result
for artifact, relative_path in (
    ("world_snapshot", "world"),
    ("generated_world_sidecar", "generated-world/generated-scenario-world"),
    ("world_generation_provenance", "generated-world/world-generation-provenance.json"),
    ("governance_manifest", "doc/testing/evidence/public-testnet-governed-bootstrap-validator-registry-2026-06-06.json"),
):
    mapping_block = text[text.index("node_local_bundle_artifact_relative_path() {"):text.index("localize_bundle_artifact() {")]
    assert f"{artifact})" in mapping_block and f"'{relative_path}'" in mapping_block, (
        f"macOS bundle localization must use the live node path for {artifact}"
    )
for token in (
    f"EXPECTED_DMG_SHA256={expected_dmg_sha256}",
    'shasum -a 256 "$DMG_PATH"',
    "dmg_sha256_verified=true",
    'ATTEMPT_ROOT="$(mktemp -d',
    "preflight_backup_closure_complete=true",
    "state_backup_closure_complete=true",
    "STATE_ROLLBACK_POLICY=restore_pre_upgrade_snapshot",
    'backup_path "$RUNTIME_PATH"',
    'backup_path "$GOVERNED_BUNDLE_PATH" "$ATTEMPT_ROOT/governed-bundle.json"',
    'backup_path "$NODE_ROOT/CURRENT_VERSION"',
    'backup_path "$NODE_ROOT/DEPLOYED_BUILDINFO"',
    'backup_path "${CONFIG_TARGETS[$index]}"',
    'backup_path "${STATE_PATHS[$index]}"',
    "LAUNCHD_TARGET=system/oasis7.testnet.fourth",
    "LAUNCHD_BOOTSTRAP_DOMAIN=system",
    'launchctl bootout "$LAUNCHD_TARGET"',
    'launchctl bootstrap "$LAUNCHD_BOOTSTRAP_DOMAIN" "$LAUNCHD_PLIST"',
    'MOUNT_POINT="$(mktemp -d',
    '-mountpoint "$MOUNT_POINT"',
    "trap cleanup EXIT",
    "lipo -archs",
    "arm64",
    "rollback_closure_complete=true",
    "/healthz",
    "/v1/chain/status",
    "rollback_begin=true",
    "preserve_failed_state",
    "failed_state_evidence_complete=true",
    "restore_pre_upgrade_state",
    "original_service_recovery_verified=true",
    "state_backup_failed_before_promotion=true",
    'MUTATED=0',
    'trap - ERR',
    'restore_file "$ATTEMPT_ROOT/CURRENT_VERSION" "$NODE_ROOT/CURRENT_VERSION"',
    'restore_file "$ATTEMPT_ROOT/DEPLOYED_BUILDINFO" "$NODE_ROOT/DEPLOYED_BUILDINFO"',
    'restore_file "$ATTEMPT_ROOT/governed-bundle.json" "$GOVERNED_BUNDLE_PATH"',
    "rollback_governed_bundle_metadata_verified=true",
    "rollback_service_health_verified=true",
    "state_sync_escalation_required=true",
    "provider_checkpoint_gate=passed",
    "latest_execution_checkpoint",
    "provider_checkpoint_gate_macos",
    "oasis7-provider-checkpoint.json.XXXXXX",
    "plutil -extract",
    "authority_failure",
    'grep -Eq \'"ok"[[:space:]]*:[[:space:]]*true\' <<<"$health"',
    "status_rollout_state",
    "assert_state_roots_safe",
    "assert_no_symlink_components",
    "resolve_existing_physical_directory",
    "persistent state root escapes physical node root",
    '[[ -e "$state_path" && ! -d "$state_path" ]]',
    "GOVERNED_BUNDLE_PATH=/Applications/oasis7/public-testnet-governed-bootstrap-bundle-2026-06-06.json",
    "testnet-package-macos-arm64-0.0.0+testnet.90.419e119bc897/oasis7-macos-arm64.dmg!/bin/oasis7_chain_runtime",
    "promote_governed_bundle_runtime_metadata",
    "verify_governed_bundle_runtime_metadata",
    "runtime_build.path",
    "runtime_build.ref",
    "runtime_build.resolved_path",
    "runtime_build.sha256",
    "runtime_build.size_bytes",
    "plutil -replace",
    "plutil -convert json",
    "governed_bundle_runtime_metadata_verified=true",
):
    assert token in text, f"macOS generated rollout contract missing: {token}"
assert "oasis7-provider-checkpoint.XXXXXX.json" not in text
extract_lines = [line.strip() for line in text.splitlines() if "plutil -extract" in line]
assert extract_lines, "macOS generated rollout must contain plutil extraction reads"
for extract_line in extract_lines:
    assert " -o - " in extract_line or " -o /dev/null " in extract_line, (
        "macOS plutil extraction must use an explicit output target: " + extract_line
    )
checksum_index = text.index('shasum -a 256 "$DMG_PATH"')
mount_index = text.index("hdiutil attach")
preflight_backup_index = text.index("preflight_backup_closure_complete=true")
active_stop = text.index('launchctl bootout "$LAUNCHD_TARGET"', preflight_backup_index)
artifact_integrity_preflight = text.index('preflight_governed_bundle_schema || die "active governed bundle schema preflight failed"')
checkpoint_gate_index = text.index("provider_checkpoint_gate_macos")
state_backup_call_index = text.index("if ! backup_persistent_state", active_stop)
state_backup_index = text.index("rollback_closure_complete=true", state_backup_call_index)
promotion_index = text.index("promotion_begin=true", state_backup_index)
assert checksum_index < mount_index < preflight_backup_index < active_stop < state_backup_index < promotion_index, (
    "required order is checksum/mount, preflight backup, bootout, stopped-state backup, promotion"
)
assert artifact_integrity_preflight < active_stop, "all localized artifact integrity must be proven before launchd stop"
bundle_backup_index = text.index('backup_path "$GOVERNED_BUNDLE_PATH" "$ATTEMPT_ROOT/governed-bundle.json"')
bundle_promote_index = text.index(
    'promote_governed_bundle_runtime_metadata "$PROMOTED_RUNTIME_SHA256" "$PROMOTED_RUNTIME_SIZE_BYTES"',
    promotion_index,
)
bootstrap_after_promotion = text.index('launchctl bootstrap "$LAUNCHD_BOOTSTRAP_DOMAIN" "$LAUNCHD_PLIST"', promotion_index)
assert bundle_backup_index < active_stop < bundle_promote_index < bootstrap_after_promotion, (
    "governed bundle must join the backup/promotion/verification closure before startup"
)
bundle_promotion_function = text[
    text.index("promote_governed_bundle_runtime_metadata() {"):text.index("assert_launchd_plist_label() {")
]
assert bundle_promotion_function.index("set_bundle_string") < bundle_promotion_function.index(
    'mv -f "$temporary" "$GOVERNED_BUNDLE_PATH"'
) < bundle_promotion_function.index("verify_governed_bundle_runtime_metadata"), (
    "governed bundle metadata must be updated in a temporary JSON file, atomically promoted, then verified"
)
assert state_backup_call_index > active_stop
assert checkpoint_gate_index < active_stop, (
    "macOS provider checkpoint gate must fail closed before launchd mutation"
)
assert "python3" not in text, (
    "macOS operator script must not depend on an undeclared Python runtime"
)
assert 'IFS=$\'\\t\' read -r name url <<<"$provider"' in text
assert '"sequencer\t' in text and '"storage\t' in text, (
    "macOS provider checkpoint gate must delimit native provider records with tabs"
)
gate_match = re.search(
    r"(?ms)^(provider_checkpoint_gate_macos\(\) \{.*?^\})\n\nprovider_checkpoint_gate_macos$",
    text,
)
assert gate_match, "macOS rollout must invoke the generated native checkpoint gate before mutation"
gate_function = gate_match.group(1)
native_mock_dir = script_path.parent / "native-checkpoint-whitespace-mocks"
native_mock_dir.mkdir()
(native_mock_dir / "curl").write_text(
    """#!/usr/bin/env bash
set -euo pipefail
while (($#)); do
  if [[ "$1" == "-o" ]]; then
    printf '{}\\n' >"$2"
    exit 0
  fi
  shift
done
exit 2
""",
    encoding="utf-8",
)
(native_mock_dir / "plutil").write_text(
    """#!/usr/bin/env bash
set -euo pipefail
case "$2" in
  chain_proof.latest_execution_checkpoint.schema_version) printf '2\\n' ;;
  chain_proof.latest_execution_checkpoint.checkpoint_id) printf '   \\n' ;;
  chain_proof.latest_execution_checkpoint.height) printf '4242\\n' ;;
  chain_proof.latest_execution_checkpoint.manifest_hash) printf '%064d\\n' 0 ;;
  *) exit 2 ;;
esac
""",
    encoding="utf-8",
)
for native_mock in native_mock_dir.iterdir():
    native_mock.chmod(0o755)
whitespace_gate = script_path.parent / "native-checkpoint-whitespace-gate.sh"
whitespace_gate.write_text(
    "#!/usr/bin/env bash\nset -euo pipefail\n" + gate_function + "\nprovider_checkpoint_gate_macos\n",
    encoding="utf-8",
)
whitespace_result = subprocess.run(
    ["bash", str(whitespace_gate)],
    env={**__import__("os").environ, "PATH": f"{native_mock_dir}:{__import__('os').environ['PATH']}"},
    text=True,
    capture_output=True,
)
assert whitespace_result.returncode != 0, (
    "macOS checkpoint gate accepted identical whitespace-only checkpoint IDs: "
    f"stdout={whitespace_result.stdout!r} stderr={whitespace_result.stderr!r}"
)
assert "invalid_checkpoint_identity" in whitespace_result.stderr, (
    "macOS checkpoint gate must reject whitespace-only checkpoint IDs before mutation: "
    f"stderr={whitespace_result.stderr!r}"
)
main_before_stop = text[preflight_backup_index:active_stop]
assert "backup_persistent_state" not in main_before_stop
assert "restart_original_service" in text[active_stop:promotion_index]
rollback = text[text.index("rollback() {"):text.index("on_error() {")]
assert rollback.index("MUTATED=0") < rollback.index("launchctl bootout"), (
    "rollback must clear its mutation guard before any fallible rollback action"
)
assert "ROLLBACK_ROOT" not in text, "macOS rollback must use the attempt-local active-state backup"
assert rollback.index('restore_file "$ATTEMPT_ROOT/governed-bundle.json" "$GOVERNED_BUNDLE_PATH"') < rollback.index(
    'launchctl bootstrap "$LAUNCHD_BOOTSTRAP_DOMAIN" "$LAUNCHD_PLIST"'
), "rollback must restore governed bundle metadata before restarting the original service"
assert re.search(r"cleanup\(\).*hdiutil detach.*rm -rf", text, re.DOTALL)
authority_path = text[text.index('if [[ "$authority_failure" -eq 1 ]]'):]
assert authority_path.index("rollback") < authority_path.index("state_sync_escalation_required=true")
assert "state_sync_escalation_required=true" in authority_path
assert re.search(r"rollback.*\|\|.*rollback_status", authority_path, re.DOTALL), (
    "authority path must emit state-sync escalation even if rollback returns nonzero"
)
health_function = text[text.index("wait_for_service_health() {"):text.index("restart_original_service() {")]
assert '"ok"[[:space:]]*:[[:space:]]*true' in health_function
assert re.search(r"ok.*<<<\"\$health\"", health_function, re.DOTALL)
assert not re.search(r"running.*<<<\"\$health\"", health_function, re.DOTALL)
assert "status_rollout_state \"$status\"" in health_function
assert "state_sync_fallback_required" in text
assert "plutil -extract consensus.state_sync_fallback_required raw -expect bool" in text
assert "plutil -extract observability.alerts" in text
assert "grep -Eq 'authority_failure|state_sync_fallback_required" not in text
promotion_success_path = text[
    text.index("authority_failure=0"):text.index('if [[ "$authority_failure" -eq 1 ]]')
]
assert re.search(r"ok.*<<<\"\$health\"", promotion_success_path, re.DOTALL), (
    "post-promotion health verification must require healthz ok=true"
)
assert not re.search(r"running.*<<<\"\$health\"", promotion_success_path, re.DOTALL), (
    "post-promotion health verification must not require running=true from healthz"
)
assert "status_rollout_state \"$status\"" in promotion_success_path
backup_state_function = text[text.index("backup_persistent_state() {"):text.index("preserve_failed_state() {")]
restore_state_function = text[text.index("restore_pre_upgrade_state() {"):text.index("rollback() {")]
assert backup_state_function.index("assert_state_roots_safe") < backup_state_function.index("backup_path")
assert restore_state_function.index("assert_state_roots_safe") < restore_state_function.index("rm -rf")

spec = importlib.util.spec_from_file_location("package_rollout_launchd_contract", module_path)
module = importlib.util.module_from_spec(spec)
assert spec.loader is not None
spec.loader.exec_module(module)
base_node = {
    "name": "launchd-contract",
    "node_root": "/Applications/oasis7",
    "launchd_plist": "/Library/LaunchDaemons/oasis7.testnet.fourth.plist",
    "runtime_path": "/Applications/oasis7/current/bin/oasis7_chain_runtime",
    "governed_bundle_path": "/Applications/oasis7/public-testnet-governed-bootstrap-bundle-2026-06-06.json",
    "healthz_url": "http://127.0.0.1:19083/healthz",
    "status_url": "http://127.0.0.1:19083/v1/chain/status",
    "config_paths": ["/Applications/oasis7/config/node.env"],
    "persistent_state_paths": ["/Applications/oasis7/data"],
}
for target, domain in (
    ("system/oasis7.testnet.fourth", "system"),
    ("gui/501/oasis7.testnet.fourth", "gui/501"),
):
    generated = module.macos_script(
        {**base_node, "launchd_target": target}, "version", "commit", "run", "a" * 64,
        "http://127.0.0.1:6631/v1/chain/status", "http://127.0.0.1:6632/v1/chain/status",
        closure_receipt,
    )
    assert f"LAUNCHD_TARGET={target}" in generated
    assert f"LAUNCHD_BOOTSTRAP_DOMAIN={domain}" in generated
    assert "LAUNCHD_LABEL=" in generated
    assert "assert_launchd_plist_label" in generated
    assert "assert_launchd_target_loaded" in generated
    assert 'launchctl bootout "$LAUNCHD_TARGET"' in generated
    assert 'launchctl bootstrap "$LAUNCHD_BOOTSTRAP_DOMAIN" "$LAUNCHD_PLIST"' in generated
    bootstrap_positions = [
        match.start()
        for match in re.finditer('launchctl bootstrap "$LAUNCHD_BOOTSTRAP_DOMAIN" "$LAUNCHD_PLIST"', generated)
    ]
    assert all(
        "assert_launchd_target_loaded" in generated[position : position + 180]
        for position in bootstrap_positions
    ), "every launchd bootstrap path must re-assert the exact target before health acceptance"
assert "CANONICAL_GOVERNED_BUNDLE_PATH=/Applications/oasis7/doc/testing/evidence/public-testnet-governed-bootstrap-bundle-2026-06-06.json" in generated
explicit_canonical = "/Applications/oasis7/config/doc/testing/evidence/enriched-bundle.json"
explicit_generated = module.macos_script(
    {**base_node, "launchd_target": "system/oasis7.testnet.fourth", "canonical_governed_bundle_path": explicit_canonical},
    "version", "commit", "run", "a" * 64,
    "http://127.0.0.1:6631/v1/chain/status", "http://127.0.0.1:6632/v1/chain/status",
    closure_receipt,
)
assert f"CANONICAL_GOVERNED_BUNDLE_PATH={explicit_canonical}" in explicit_generated
try:
    with contextlib.redirect_stderr(io.StringIO()):
        module.macos_script(
            {**base_node, "launchd_target": "system/oasis7.testnet.fourth", "canonical_governed_bundle_path": "/tmp/enriched-bundle.json"},
            "version", "commit", "run", "a" * 64,
            "http://127.0.0.1:6631/v1/chain/status", "http://127.0.0.1:6632/v1/chain/status",
            closure_receipt,
        )
except SystemExit:
    pass
else:
    raise AssertionError("canonical governed bundle path escaping node_root was accepted")
for invalid_target in (
    "user/oasis7.testnet.fourth",
    "gui/not-a-uid/oasis7.testnet.fourth",
    "gui/501",
    "system/",
):
    try:
        with contextlib.redirect_stderr(io.StringIO()):
            module.macos_script(
                {**base_node, "launchd_target": invalid_target},
                "version", "commit", "run", "a" * 64,
                "http://127.0.0.1:6631/v1/chain/status", "http://127.0.0.1:6632/v1/chain/status",
                closure_receipt,
            )
    except SystemExit:
        pass
    else:
        raise AssertionError(f"invalid launchd target accepted: {invalid_target}")
invalid_state_sets = (
    ["/Applications/oasis7/data", "/Applications/oasis7/data"],
    ["/Applications/oasis7/data", "/Applications/oasis7/data/checkpoints"],
    ["/Applications/oasis7"],
    ["/Applications/oasis7/current"],
    ["/Applications/oasis7/config"],
    ["/Applications/oasis7/CURRENT_VERSION"],
    ["/Applications/oasis7/DEPLOYED_BUILDINFO"],
)
for invalid_states in invalid_state_sets:
    try:
        with contextlib.redirect_stderr(io.StringIO()):
            module.macos_script(
                {
                    **base_node,
                    "launchd_target": "gui/501/oasis7.testnet.fourth",
                    "persistent_state_paths": invalid_states,
                },
                "version", "commit", "run", "a" * 64,
                "http://127.0.0.1:6631/v1/chain/status", "http://127.0.0.1:6632/v1/chain/status",
                closure_receipt,
            )
    except SystemExit:
        pass
    else:
        raise AssertionError(f"unsafe persistent state topology accepted: {invalid_states}")
for invalid_bundle_path in (
    "/Applications/oasis7/config/public-testnet-governed-bootstrap-bundle-2026-06-06.json",
    "/Applications/oasis7/releases/public-testnet-governed-bootstrap-bundle-2026-06-06.json",
    "/tmp/public-testnet-governed-bootstrap-bundle-2026-06-06.json",
):
    try:
        with contextlib.redirect_stderr(io.StringIO()):
            module.macos_script(
                {**base_node, "launchd_target": "system/oasis7.testnet.fourth", "governed_bundle_path": invalid_bundle_path},
                "version", "commit", "run", "a" * 64,
                "http://127.0.0.1:6631/v1/chain/status", "http://127.0.0.1:6632/v1/chain/status",
                closure_receipt,
            )
    except SystemExit:
        pass
    else:
        raise AssertionError(f"non-root governed bundle path accepted: {invalid_bundle_path}")
launchd_overlap_node = {
    **base_node,
    "launchd_target": "gui/501/oasis7.testnet.fourth",
    "launchd_plist": "/Applications/oasis7/launchd/oasis7.testnet.fourth.plist",
    "persistent_state_paths": ["/Applications/oasis7/launchd"],
}
try:
    with contextlib.redirect_stderr(io.StringIO()):
        module.macos_script(
            launchd_overlap_node, "version", "commit", "run", "a" * 64,
            "http://127.0.0.1:6631/v1/chain/status", "http://127.0.0.1:6632/v1/chain/status",
            closure_receipt,
        )
except SystemExit:
    pass
else:
    raise AssertionError("persistent state path containing launchd plist was accepted")
assert "assert_no_symlink_components" in generated
assert "resolve_existing_physical_directory" in generated
assert "physical_node_root" in generated
assert "persistent state root escapes physical node root" in generated
assert "persistent state root has symlink component" in generated
assert "physical_state_path" in generated
scope_contract = "all_existing` plus `linux_macos_arm64"
assert scope_contract in inventory_text and scope_contract in runbook_text
assert "all_existing` | every package platform represented by the managed fleet" not in inventory_text
windows_script = Path(next(node for node in plan["nodes"] if node["name"] == "windows-observer")["windows_script"])
assert "provider_checkpoint_gate=passed" in windows_script.read_text(encoding="utf-8")
PY
then
  package_contract_failed=1
fi
if ! python3 - "$TMP_DIR/plan-only.json" <<'PY'
from pathlib import Path, PurePosixPath
import base64
import json
import re
import shlex
import sys

plan = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
windows = next(node for node in plan["nodes"] if node["name"] == "windows-observer")
commands = windows["commands"]


def decode_remote_powershell(command_index: int) -> str:
    command = commands[command_index]
    assert " -Command " not in command and "'\"'\"'" not in command, (
        "remote PowerShell must not embed raw statements through POSIX shell quoting: "
        f"command_index={command_index}"
    )
    tokens = shlex.split(command)
    ssh_index = tokens.index("ssh")
    remote = tokens[ssh_index + 2 :]
    expected_prefix = [
        "powershell.exe",
        "-NoProfile",
        "-NonInteractive",
        "-EncodedCommand",
    ]
    assert remote[:4] == expected_prefix and len(remote) == 5, (
        "remote PowerShell command must use powershell.exe -NoProfile "
        "-NonInteractive -EncodedCommand: "
        f"command_index={command_index} remote_argv={remote[:4]}"
    )
    encoded = remote[4]
    payload = base64.b64decode(encoded, validate=True)
    assert len(payload) % 2 == 0, (
        f"EncodedCommand payload is not UTF-16LE aligned: command_index={command_index}"
    )
    statement = payload.decode("utf-16le")
    assert not statement.startswith("\ufeff"), (
        f"EncodedCommand statement must not contain a BOM: command_index={command_index}"
    )
    assert base64.b64encode(statement.encode("utf-16le")).decode("ascii") == encoded, (
        f"EncodedCommand failed exact UTF-16LE/base64 round-trip: command_index={command_index}"
    )
    return statement


assert (len(commands) - 1) % 3 == 0
for parent_index in range(0, len(commands) - 1, 3):
    scp_index = parent_index + 1
    hash_index = parent_index + 2
    scp_tokens = shlex.split(commands[scp_index])
    remote_path = scp_tokens[-1].split(":", 1)[1]
    parent_path = PurePosixPath(remote_path).parent.as_posix()

    parent_statement = decode_remote_powershell(parent_index)
    assert "staging_parent_ready=" in parent_statement
    assert remote_path in parent_statement and parent_path in parent_statement
    assert "New-Item" in parent_statement

    hash_statement = decode_remote_powershell(hash_index)
    assert "staging_transfer_ack=" in hash_statement
    assert remote_path in hash_statement
    assert "Get-FileHash" in hash_statement and "SHA256" in hash_statement

apply_statement = decode_remote_powershell(len(commands) - 1)
assert re.search(r"task-2269-windows-upgrade\.ps1", apply_statement, re.IGNORECASE), (
    "decoded apply invocation omits the generated Windows upgrade script path"
)

staged_script_paths = {
    shlex.split(command)[-1].split(":", 1)[1]
    for command in commands
    if command.startswith("scp ")
    and shlex.split(command)[-1].lower().endswith("-windows-upgrade.ps1")
}
assert len(staged_script_paths) == 1, (
    "decoded apply contract requires exactly one staged Windows upgrade script: "
    f"paths={sorted(staged_script_paths)}"
)
staged_script_path = next(iter(staged_script_paths))
assert staged_script_path in apply_statement, (
    "decoded apply invocation must pass the exact staged upgrade script path: "
    f"path={staged_script_path!r}"
)

staged_script_transfers = [
    shlex.split(command)
    for command in commands
    if command.startswith("scp ")
    and shlex.split(command)[-1].split(":", 1)[1] == staged_script_path
]
assert len(staged_script_transfers) == 1, (
    "generated Windows upgrade script must have exactly one local transfer source"
)
generated_script_path = Path(staged_script_transfers[0][-2])
generated_script = generated_script_path.read_text(encoding="utf-8")

# Audit the emitted PowerShell rather than the Python template: a single
# backslash in a Python string can collapse a quoted path separator to ''.
empty_path_separator_findings = []
empty_trim_argument = re.compile(
    r"\.Trim(?P<direction>Start|End)\(\s*(?P<quote>['\"])(?P=quote)\s*\)"
)
empty_replace_old_value = re.compile(
    r"\.Replace\(\s*(?P<quote>['\"])(?P=quote)\s*,"
)
empty_boundary_concatenation = re.compile(r"(?:\+\s*''|''\s*\+)")
for line_number, line in enumerate(generated_script.splitlines(), start=1):
    for match in empty_trim_argument.finditer(line):
        empty_path_separator_findings.append(
            (line_number, f"Trim{match.group('direction')}", line.strip())
        )
    for _match in empty_replace_old_value.finditer(line):
        empty_path_separator_findings.append(
            (line_number, "Replace-old-value", line.strip())
        )
    if (
        empty_boundary_concatenation.search(line)
        and re.search(r"(?i)(?:path|root|prefix|suffix|StartsWith|EndsWith)", line)
    ):
        empty_path_separator_findings.append(
            (line_number, "path-boundary-concatenation", line.strip())
        )

# An empty replacement value is legitimate and must not be mistaken for an
# empty path-separator argument.
assert ".Replace('-', '')" in generated_script
assert not empty_path_separator_findings, (
    "generated PowerShell contains empty path-separator literals caused by "
    "Python escape collapse; legitimate empty Replace new-values are allowed: "
    f"findings={empty_path_separator_findings}"
)

tree_metadata_match = re.search(
    r"function Get-TreeMetadata\s*\{(?P<body>.*?)\n\}",
    generated_script,
    re.DOTALL,
)
assert tree_metadata_match, "generated script omits Get-TreeMetadata"
tree_metadata = tree_metadata_match.group("body")
assert ".TrimStart('')" not in tree_metadata, (
    "Python template escape collapse produced an empty TrimStart path separator"
)
assert ".Replace('', '/')" not in tree_metadata, (
    "Python template escape collapse produced an empty Replace path separator"
)
safe_relative_path_expression = (
    r"$file.FullName.Substring($rootItem.FullName.Length)"
    r".TrimStart('\').Replace('\', '/')"
)
assert safe_relative_path_expression in tree_metadata, (
    "Get-TreeMetadata must normalize Windows relative paths with non-empty "
    "literal backslash arguments: "
    f"expected={safe_relative_path_expression!r}"
)

normalized_apply = re.sub(r"\s+", " ", apply_statement).strip()
controlled_process = re.search(
    r"(?i)(?:&\s*)?powershell(?:\.exe)?\s+"
    r"-NoProfile\s+-NonInteractive\s+"
    r"-ExecutionPolicy\s+Bypass\s+-File\s+"
    r"(?P<path_quote>['\"]?)"
    + re.escape(staged_script_path)
    + r"(?P=path_quote)(?:\s|;|$)",
    normalized_apply,
)
assert controlled_process, (
    "decoded apply invocation must launch the staged script through an explicit "
    "powershell.exe -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "
    f"process boundary: statement={apply_statement!r}"
)

tail = normalized_apply[controlled_process.end() :]
assert re.search(r"(?i)(?:;|\|)\s*exit\s+\$LASTEXITCODE\s*$", tail), (
    "decoded apply invocation must wait synchronously and propagate the child "
    "PowerShell exit code exactly through SSH"
)

apply_tokens = shlex.split(commands[-1])
apply_ssh_index = apply_tokens.index("ssh")
apply_target = apply_tokens[apply_ssh_index + 1]
expected_target = "Administrator@192.0.2.33"
assert apply_target == expected_target, (
    "generated apply SSH destination must be the exact user@host target with no "
    f"suffix or trailing colon: expected={expected_target!r} actual={apply_target!r}"
)
assert not apply_target.endswith(":")
PY
then
  package_contract_failed=1
fi

if ! python3 - "$TMP_DIR/plan-only.json" <<'PY'
from pathlib import Path, PurePosixPath
import json
import re
import shlex
import sys

plan = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
windows = next(node for node in plan["nodes"] if node["name"] == "windows-observer")
commands = windows["commands"]
transfer_commands = [command for command in commands if command.startswith("scp ")]
assert transfer_commands, "Windows rollout fixture generated no transfers"

active_paths = (
    "C:/oasis7-deploy/config",
    "C:/oasis7-deploy/oasis7-windows-x64.exe",
    "C:/Users/Observer/AppData/Local/Programs/oasis7",
)
staging_roots = set()
transferred_names = set()
for command in transfer_commands:
    destination = shlex.split(command)[-1]
    remote_target, separator, remote_path = destination.partition(":")
    assert separator and remote_target == "Administrator@192.0.2.33", (
        f"Windows transfer target is not the exact observer host: {destination}"
    )
    normalized = PurePosixPath(remote_path).as_posix()
    assert not any(
        normalized == active or normalized.startswith(active + "/")
        for active in active_paths
    ), f"pre-apply transfer mutates an active deployment path: {normalized}"
    match = re.match(
        r"(?i)^(C:/oasis7-deploy/(?:\.staging|staging|attempts)/([^/]+))/(?:.+)$",
        normalized,
    )
    assert match, (
        "every pre-apply transfer must use an attempt-specific independent staging root: "
        f"path={normalized}"
    )
    attempt_id = match.group(2)
    assert attempt_id.lower() not in {"current", "config", "runtime", "bin"}
    staging_roots.add(match.group(1).lower())
    transferred_names.add(PurePosixPath(normalized).name)

assert len(staging_roots) == 1, (
    "one rollout plan must stage its complete closure under exactly one attempt root: "
    f"roots={sorted(staging_roots)}"
)
required_names = {
    "oasis7-windows-x64.exe",
    "public-testnet-governed-bootstrap-bundle-2026-06-06.windows.json",
    "public-testnet-governed-bootstrap-genesis-2026-06-06.windows.json",
    "public-testnet-governed-bootstrap-manifest-2026-06-06.windows.json",
    "public-testnet-governed-bootstrap-bootstrap-peers-2026-06-06.windows.txt",
    "windows-observer-windows-upgrade.ps1",
}
missing = sorted(required_names - transferred_names)
assert not missing, f"fresh Windows staging closure omits required files: {missing}"
PY
then
  package_contract_failed=1
fi

if ! python3 - "$package_dir/windows/windows-x64-SHA256SUMS" <<'PY'
from pathlib import Path, PurePosixPath
import sys

sums = Path(sys.argv[1])
covered = {
    PurePosixPath(line.split(maxsplit=1)[1].lstrip("*")).as_posix()
    for line in sums.read_text(encoding="utf-8").splitlines()
    if line.strip()
}
required = {
    "oasis7-windows-x64.exe",
    "public-testnet-governed-bootstrap-bundle-2026-06-06.windows.json",
    "public-testnet-governed-bootstrap-genesis-2026-06-06.windows.json",
    "public-testnet-governed-bootstrap-manifest-2026-06-06.windows.json",
    "public-testnet-governed-bootstrap-bootstrap-peers-2026-06-06.windows.txt",
    "generated-world/world/snapshot.json",
    "generated-world/world/journal.json",
    "generated-world/generated-scenario-world/snapshot.json",
    "generated-world/generated-scenario-world/journal.json",
    "generated-world/generated-scenario-world/chunks/0001.json",
    "generated-world/world-generation-provenance.json",
    "doc/testing/evidence/governance-public-signers.json",
    "doc/testing/evidence/liveops-public-signers.json",
    "doc/testing/evidence/signer-truth-binding.md",
    "doc/testing/evidence/genesis-validator-registry.json",
    "doc/testing/evidence/governed-bootstrap-topology.md",
}
missing = sorted(required - covered)
assert not missing, f"Windows runtime truth fixture lacks checksum coverage: {missing}"
PY
then
  package_contract_failed=1
fi

if ! python3 - "$TMP_DIR/plan-only.json" <<'PY'
from pathlib import PurePosixPath
from pathlib import Path
import json
import shlex
import sys

plan = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
windows = next(node for node in plan["nodes"] if node["name"] == "windows-observer")
commands = windows["commands"]
transfer_indices = [index for index, command in enumerate(commands) if command.startswith("scp ")]
assert transfer_indices, "Windows rollout fixture generated no transfers"
for transfer_index in transfer_indices:
    transfer = shlex.split(commands[transfer_index])
    remote_path = PurePosixPath(transfer[-1].split(":", 1)[1]).as_posix()
    parent = PurePosixPath(remote_path).parent.as_posix()
    parent_preflight = commands[:transfer_index]
    assert any(
        parent in command
        and ("New-Item -ItemType Directory" in command or "mkdir" in command)
        and "staging_parent_ready=" in command
        for command in parent_preflight
    ), f"staging parent not created before transfer: parent={parent} path={remote_path}"
PY
then
  package_contract_failed=1
fi

if ! python3 - "$TMP_DIR/plan-only.json" <<'PY'
from pathlib import Path, PurePosixPath
import hashlib
import json
import shlex
import sys

plan = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
windows = next(node for node in plan["nodes"] if node["name"] == "windows-observer")
commands = windows["commands"]
apply_index = len(commands) - 1
assert "ssh " in commands[apply_index] and "powershell" in commands[apply_index]
transfer_indices = [index for index, command in enumerate(commands) if command.startswith("scp ")]
for transfer_index in transfer_indices:
    transfer = shlex.split(commands[transfer_index])
    source = Path(transfer[1])
    remote_path = PurePosixPath(transfer[-1].split(":", 1)[1]).as_posix()
    expected_sha256 = hashlib.sha256(source.read_bytes()).hexdigest()
    closure_commands = commands[transfer_index + 1 : apply_index]
    assert any(
        remote_path in command
        and expected_sha256 in command
        and "Get-FileHash" in command
        and "staging_transfer_ack=" in command
        and "throw" in command
        for command in closure_commands
    ), (
        "transfer lacks explicit success acknowledgment and remote hash closure before apply: "
        f"path={remote_path} sha256={expected_sha256}"
    )
assert all(
    token not in command
    for command in commands[:apply_index]
    for token in ("Stop-ScheduledTask", "Set-ScheduledTask", "Start-Process")
), "apply/mutation command appeared before every staged transfer was acknowledged"
PY
then
  package_contract_failed=1
fi

if ! python3 - "$TMP_DIR/plan-only.json" "$TMP_DIR/plan-only-out/windows-observer-windows-upgrade.ps1" <<'PY'
from pathlib import Path
import base64
import json
import shlex
import sys

plan = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
script = Path(sys.argv[2]).read_text(encoding="utf-8")
windows = next(node for node in plan["nodes"] if node["name"] == "windows-observer")
commands = windows["commands"]
apply_index = len(commands) - 1

transfer_indices = [index for index, command in enumerate(commands) if command.startswith("scp ")]
hash_ack_indices = [
    index for index, command in enumerate(commands) if "staging_transfer_ack=" in command
]
assert transfer_indices and len(hash_ack_indices) == len(transfer_indices), (
    "apply must be preceded by one remote SHA acknowledgment per staged transfer"
)
assert max(hash_ack_indices) < apply_index, (
    "remote apply appeared before the complete staged SHA closure"
)

apply_tokens = shlex.split(commands[apply_index])
ssh_index = apply_tokens.index("ssh")
remote_argv = apply_tokens[ssh_index + 2 :]
assert remote_argv[:4] == [
    "powershell.exe", "-NoProfile", "-NonInteractive", "-EncodedCommand"
]
apply_statement = base64.b64decode(remote_argv[4], validate=True).decode("utf-16le")
assert any(token in apply_statement.lower() for token in ("/staging/", "/.staging/", "/attempts/")), (
    "remote apply must execute the upgrade script from the attempt-specific staging root"
)

closure_marker = "staged_sha_closure_complete=true"
promotion_begin = "promotion_begin=true"
promotion_complete = "promotion_complete=true"
for token in (closure_marker, promotion_begin, promotion_complete):
    assert token in script, f"remote apply omits transactional promotion evidence: {token}"
assert script.index(closure_marker) < script.index(promotion_begin) < script.index(promotion_complete), (
    "active deployment promotion must occur only after the full staged SHA closure"
)
pre_promotion = script[: script.index(promotion_begin)]
for mutation in ("Stop-ScheduledTask", "Start-Process"):
    assert mutation not in pre_promotion, (
        f"active mutation {mutation} occurred before staged closure and promotion"
    )
PY
then
  package_contract_failed=1
fi

if ! python3 - "$TMP_DIR/plan-only-out/windows-observer-windows-upgrade.ps1" <<'PY'
from pathlib import Path
import re
import sys

text = Path(sys.argv[1]).read_text(encoding="utf-8")
rollback = text[text.index("rollback_begin=true"):]

runtime_decision_marker = "rollback_runtime_restore_required="
component_restored_marker = "rollback_component_restored="
component_unchanged_marker = "rollback_component_unchanged="
for token in (
    runtime_decision_marker,
    component_restored_marker,
    component_unchanged_marker,
):
    assert token in rollback, f"rollback omits changed-component evidence: {token}"

runtime_decision_index = rollback.index(runtime_decision_marker)
unlock_deadline_index = rollback.index("$rollbackFileUnlockDeadline")
exclusive_open_index = rollback.index("[System.IO.FileShare]::None", unlock_deadline_index)
runtime_copy_match = re.search(
    r"Copy-Item\s+-LiteralPath\s+\$rollbackRuntimeSourceAtRestore\s+"
    r"-Destination\s+\$runtime\s+-Force",
    rollback,
)
assert runtime_copy_match, "rollback runtime restore copy is missing"
runtime_copy_index = runtime_copy_match.start()
assert runtime_decision_index < unlock_deadline_index < exclusive_open_index < runtime_copy_index, (
    "runtime hash decision must guard any lock probe and runtime replacement"
)
runtime_guard = rollback[runtime_decision_index:runtime_copy_index]
assert re.search(
    r"if\s*\(\s*\$rollbackRuntimeRestoreRequired\s*\)",
    runtime_guard,
    re.IGNORECASE,
), (
    "runtime unlock/replacement must be conditional on installed and backup SHA mismatch; "
    "an equal-hash runtime lock is irrelevant to config-only rollback"
)
for token in ("Get-FileHash", "$rollbackRuntimeSource", "$runtime"):
    assert token in rollback[:unlock_deadline_index], (
        f"runtime restore decision must close installed and backup SHA before lock polling: {token}"
    )

config_loop_index = rollback.index("foreach ($rollbackConfigTarget in $rollbackConfigTargets)")
config_loop = rollback[config_loop_index:]
for token in ("Get-FileHash", "$rollbackConfigSource", "$rollbackConfigTarget"):
    assert token in config_loop, (
        f"rollback must compare each config component before replacement: {token}"
    )
assert re.search(
    r"if\s*\([^)]*(?:Sha256|Hash)[^)]*-ne[^)]*(?:Sha256|Hash)[^)]*\).*?"
    r"Copy-Item\s+-LiteralPath\s+\$rollbackConfigSource\s+"
    r"-Destination\s+\$rollbackConfigTarget\s+-Force",
    config_loop,
    re.IGNORECASE | re.DOTALL,
), "rollback must restore each config only when its installed SHA differs from backup"
PY
then
  package_contract_failed=1
fi

if ! python3 - "$TMP_DIR/plan-only-out/windows-observer-windows-upgrade.ps1" <<'PY'
from pathlib import Path
import sys

text = Path(sys.argv[1]).read_text(encoding="utf-8")
try_start = text.index("try {\nInvoke-RolloutFailureInjection -Phase 'installer'")
catch_start = text.index("} catch {\n  $postStopFailure = $_", try_start)
mutation_boundary = text[try_start:catch_start]
phases = (
    "installer",
    "governed_copy",
    "bundle_move",
    "genesis_move",
    "manifest_move",
    "current_version_write",
    "deployed_buildinfo_write",
    "task_action_restore",
)
for phase in phases:
    token = f"Invoke-RolloutFailureInjection -Phase '{phase}'"
    assert token in mutation_boundary, (
        f"post-stop mutation lacks rollback-boundary failure injection: {phase}"
    )
assert "if (!$script:rollbackInvoked)" in text[catch_start:], (
    "post-stop catch must guard against duplicate rollback"
)
rollback_function = text[
    text.index("function Invoke-KnownGoodRollback"):
    text.index("function Invoke-RolloutFailureInjection")
]
assert "$script:rollbackInvoked = $true" in rollback_function
assert "rollback_already_invoked=true" in rollback_function
assert "Invoke-KnownGoodRollback" in text[catch_start:], (
    "post-stop catch does not invoke known-good rollback"
)
PY
then
  package_contract_failed=1
fi

for missing_runtime_truth in generated_world_sidecar world_generation_provenance; do
  missing_package="$TMP_DIR/missing-$missing_runtime_truth-package"
  cp -R "$package_dir" "$missing_package"
  missing_windows="$missing_package/windows"
  missing_bundle="$missing_windows/$(basename "$windows_bundle")"
  missing_manifest="$missing_windows/$(basename "$windows_manifest")"
  if [[ "$missing_runtime_truth" == "generated_world_sidecar" ]]; then
    jq 'del(.runtime_refs.generated_world_sidecar_ref)' \
      "$missing_manifest" >"$missing_manifest.tmp"
    mv "$missing_manifest.tmp" "$missing_manifest"
    jq 'del(.generated_world_sidecar)' "$missing_bundle" >"$missing_bundle.tmp"
    mv "$missing_bundle.tmp" "$missing_bundle"
    rm -rf "$missing_windows/generated-world/generated-scenario-world"
  else
    jq 'del(.runtime_refs.world_generation_provenance_ref)' \
      "$missing_manifest" >"$missing_manifest.tmp"
    mv "$missing_manifest.tmp" "$missing_manifest"
    jq 'del(.world_generation_provenance)' "$missing_bundle" >"$missing_bundle.tmp"
    mv "$missing_bundle.tmp" "$missing_bundle"
    rm "$missing_windows/generated-world/world-generation-provenance.json"
  fi
  python3 - "$missing_bundle" "$missing_manifest" <<'PY'
from pathlib import Path
import hashlib
import json
import sys

bundle_path = Path(sys.argv[1])
manifest_path = Path(sys.argv[2])
bundle = json.loads(bundle_path.read_text(encoding="utf-8"))
manifest_meta = bundle["network_manifest"]
manifest_meta["sha256"] = hashlib.sha256(manifest_path.read_bytes()).hexdigest()
manifest_meta["size_bytes"] = manifest_path.stat().st_size
bundle_path.write_text(json.dumps(bundle, indent=2) + "\n", encoding="utf-8")
PY
  (
    cd "$missing_windows"
    find . -type f ! -name windows-x64-SHA256SUMS -print \
      | LC_ALL=C sort \
      | sed 's#^./##' \
      | while IFS= read -r path; do shasum -a 256 "$path"; done \
      >windows-x64-SHA256SUMS
  )
  if "$rollout_driver" \
    --manifest "$TMP_DIR/manifest.json" \
    --package-dir "$missing_package" \
    --out-dir "$TMP_DIR/missing-$missing_runtime_truth-out" \
    --known-hosts "$operator_known_hosts" \
    >"$TMP_DIR/missing-$missing_runtime_truth.stdout" \
    2>"$TMP_DIR/missing-$missing_runtime_truth.stderr"; then
    echo "expected public_testnet Windows source missing $missing_runtime_truth to fail" >&2
    package_contract_failed=1
  elif ! grep -q "$missing_runtime_truth" \
    "$TMP_DIR/missing-$missing_runtime_truth.stderr"; then
    echo "missing-$missing_runtime_truth rejection did not identify the missing contract" >&2
    cat "$TMP_DIR/missing-$missing_runtime_truth.stderr" >&2
    package_contract_failed=1
  fi
done

for escaped_field in generated_world_sidecar world_generation_provenance; do
  for escaped_ref_kind in traversal absolute; do
    escaped_package="$TMP_DIR/escaped-$escaped_field-$escaped_ref_kind-package"
    cp -R "$package_dir" "$escaped_package"
    escaped_windows="$escaped_package/windows"
    escaped_bundle="$escaped_windows/$(basename "$windows_bundle")"
    if [[ "$escaped_field" == "generated_world_sidecar" ]]; then
      escaped_source="$TMP_DIR/escaped-$escaped_field-$escaped_ref_kind"
      mkdir -p "$escaped_source"
      printf 'escaped sidecar fixture\n' >"$escaped_source/snapshot.json"
    else
      escaped_source="$TMP_DIR/escaped-$escaped_field-$escaped_ref_kind.json"
      printf '{"escaped":true}\n' >"$escaped_source"
    fi
    if [[ "$escaped_ref_kind" == "traversal" ]]; then
      escaped_ref="../$(basename "$escaped_source")"
      # Make the traversal resolve outside windows/ while retaining an existing source.
      cp -R "$escaped_source" "$escaped_package/$(basename "$escaped_source")"
    else
      escaped_ref="$escaped_source"
    fi
    python3 - "$escaped_bundle" "$escaped_field" "$escaped_ref" <<'PY'
from pathlib import Path
import json
import sys

bundle_path = Path(sys.argv[1])
field = sys.argv[2]
raw_ref = sys.argv[3]
bundle = json.loads(bundle_path.read_text(encoding="utf-8"))
bundle[field]["ref"] = raw_ref
bundle_path.write_text(json.dumps(bundle, indent=2) + "\n", encoding="utf-8")
PY
    (
      cd "$escaped_windows"
      find . -type f ! -name windows-x64-SHA256SUMS -print \
        | LC_ALL=C sort \
        | sed 's#^./##' \
        | while IFS= read -r path; do shasum -a 256 "$path"; done \
        >windows-x64-SHA256SUMS
    )
    escaped_out="$TMP_DIR/escaped-$escaped_field-$escaped_ref_kind-out"
    if "$rollout_driver" \
      --manifest "$TMP_DIR/manifest.json" \
      --package-dir "$escaped_package" \
      --out-dir "$escaped_out" \
      --known-hosts "$operator_known_hosts" \
      >"$escaped_out.stdout" 2>"$escaped_out.stderr"; then
      echo "expected escaped Windows $escaped_field ref to fail before plan transfer generation" >&2
      package_contract_failed=1
    elif ! grep -q 'Windows runtime truth ref escapes platform closure' "$escaped_out.stderr"; then
      echo "escaped Windows $escaped_field ref did not fail with the platform-closure diagnostic" >&2
      cat "$escaped_out.stderr" >&2
      package_contract_failed=1
    fi
    if [[ -e "$escaped_out/windows-observer-windows-upgrade.ps1" \
      || -e "$escaped_out/rollout-plan.json" ]]; then
      echo "escaped Windows $escaped_field ref generated a transfer/apply plan before rejection" >&2
      package_contract_failed=1
    fi
  done
done

# A runtime-truth directory may not smuggle a host-owned member into the
# staged closure through either kind of symlink.  Both must fail before the
# rollout helper emits a plan, PowerShell script, or transfer command.
for symlink_kind in file directory; do
  symlink_package="$TMP_DIR/runtime-truth-$symlink_kind-symlink-package"
  cp -R "$package_dir" "$symlink_package"
  symlink_windows="$symlink_package/windows"
  symlink_sidecar="$symlink_windows/generated-world/generated-scenario-world"
  external_root="$TMP_DIR/runtime-truth-$symlink_kind-external"
  mkdir -p "$external_root"
  if [[ "$symlink_kind" == "file" ]]; then
    printf 'outside platform root\n' >"$external_root/outside.json"
    ln -s "$external_root/outside.json" "$symlink_sidecar/outside-file-link.json"
  else
    mkdir -p "$external_root/outside-dir"
    printf 'outside platform root\n' >"$external_root/outside-dir/outside.json"
    ln -s "$external_root/outside-dir" "$symlink_sidecar/outside-directory-link"
  fi

  symlink_out="$TMP_DIR/runtime-truth-$symlink_kind-symlink-out"
  if "$rollout_driver" \
    --manifest "$TMP_DIR/manifest.json" \
    --package-dir "$symlink_package" \
    --out-dir "$symlink_out" \
    --known-hosts "$operator_known_hosts" \
    >"$symlink_out.stdout" 2>"$symlink_out.stderr"; then
    echo "expected runtime-truth $symlink_kind symlink to fail before rollout generation" >&2
    package_contract_failed=1
  elif ! grep -Eq 'Windows runtime truth tree contains symlink for generated_world_sidecar|package tree contains symlink' \
    "$symlink_out.stderr"; then
    echo "runtime-truth $symlink_kind symlink did not produce the stable containment diagnostic" >&2
    cat "$symlink_out.stderr" >&2
    package_contract_failed=1
  fi
  if [[ -e "$symlink_out/windows-observer-windows-upgrade.ps1" \
    || -e "$symlink_out/rollout-plan.json" ]]; then
    echo "runtime-truth $symlink_kind symlink generated rollout output before rejection" >&2
    package_contract_failed=1
  fi
done

# Top-level governed truth is parsed before the recursive artifact graph, so it
# needs the same no-symlink contract as every nested runtime-truth member.
for governed_name in \
  public-testnet-governed-bootstrap-bundle-2026-06-06.windows.json \
  public-testnet-governed-bootstrap-genesis-2026-06-06.windows.json \
  public-testnet-governed-bootstrap-manifest-2026-06-06.windows.json \
  public-testnet-governed-bootstrap-bootstrap-peers-2026-06-06.windows.txt; do
  symlink_package="$TMP_DIR/governed-$(basename "$governed_name")-symlink-package"
  cp -R "$package_dir" "$symlink_package"
  governed_path="$symlink_package/windows/$governed_name"
  mv "$governed_path" "$governed_path.real"
  ln -s "$(basename "$governed_path").real" "$governed_path"
  symlink_out="$TMP_DIR/governed-$(basename "$governed_name")-symlink-out"
  if "$rollout_driver" \
    --manifest "$TMP_DIR/manifest.json" \
    --package-dir "$symlink_package" \
    --out-dir "$symlink_out" \
    --known-hosts "$operator_known_hosts" \
    >"$symlink_out.stdout" 2>"$symlink_out.stderr"; then
    echo "expected top-level governed symlink to fail before rollout generation: $governed_name" >&2
    package_contract_failed=1
  elif ! grep -Eq 'Windows governed bootstrap artifact contains symlink component|package tree contains symlink' \
    "$symlink_out.stderr"; then
    echo "top-level governed symlink did not produce stable rejection: $governed_name" >&2
    cat "$symlink_out.stderr" >&2
    package_contract_failed=1
  fi
  if [[ -e "$symlink_out/windows-observer-windows-upgrade.ps1" \
    || -e "$symlink_out/rollout-plan.json" ]]; then
    echo "top-level governed symlink generated rollout output: $governed_name" >&2
    package_contract_failed=1
  fi
done

ancestor_package="$TMP_DIR/governed-ancestor-symlink-package"
cp -R "$package_dir" "$ancestor_package"
mv "$ancestor_package/windows" "$ancestor_package/windows-real"
ln -s windows-real "$ancestor_package/windows"
ancestor_out="$TMP_DIR/governed-ancestor-symlink-out"
if "$rollout_driver" \
  --manifest "$TMP_DIR/manifest.json" \
  --package-dir "$ancestor_package" \
  --out-dir "$ancestor_out" \
  --known-hosts "$operator_known_hosts" \
  >"$ancestor_out.stdout" 2>"$ancestor_out.stderr"; then
  echo "expected governed ancestor symlink to fail before rollout generation" >&2
  package_contract_failed=1
elif ! grep -Eq 'Windows governed bootstrap artifact contains symlink component|platform package path contains symlink component|package tree contains symlink' \
  "$ancestor_out.stderr"; then
  echo "governed ancestor symlink did not produce stable rejection" >&2
  cat "$ancestor_out.stderr" >&2
  package_contract_failed=1
fi
if [[ -e "$ancestor_out/windows-observer-windows-upgrade.ps1" \
  || -e "$ancestor_out/rollout-plan.json" ]]; then
  echo "governed ancestor symlink generated rollout output" >&2
  package_contract_failed=1
fi

"$rollout_driver" \
  --manifest "$TMP_DIR/manifest.json" \
  --package-dir "$package_dir" \
  --out-dir "$out_dir" \
  --known-hosts "$operator_known_hosts" \
  --apply-local \
  --json >"$TMP_DIR/plan.json"

test "$(readlink "$node_root_abs/current")" = "$node_root_abs/releases/$package_version"
grep -q "^package_version=$package_version$" "$node_root_abs/DEPLOYED_BUILDINFO"
jq -e \
  --arg commit "$commit" \
  --arg version "$package_version" \
  '.commit == $commit
    and .platform_provenance["linux-x64"].package_version == $version
    and .readiness_policy == "rpc-running"
    and (.nodes[] | select(.name == "sequencer") | .applied == true)
    and (.nodes[] | select(.name == "windows-observer") | .windows_script | endswith("windows-observer-windows-upgrade.ps1"))' \
  "$TMP_DIR/plan.json" >/dev/null

windows_script="$out_dir/windows-observer-windows-upgrade.ps1"
test -f "$windows_script"
if ! python3 - "$windows_script" <<'PY'
from pathlib import Path
import sys

text = Path(sys.argv[1]).read_text(encoding="utf-8")
preflight_index = text.index("foreach ($entry in $expectedGovernedSha256.GetEnumerator())")
attempt_id_index = text.find("$attemptId")
assert 0 <= attempt_id_index < preflight_index, (
    "staging/preflight diagnostics must be initialized before checksum preflight"
)
for token in ("$attemptStdoutPath", "$attemptStderrPath", "$attemptExitMarkerPath"):
    assert text.find(token) < preflight_index, (
        f"unique staging/preflight diagnostic initialized too late: {token}"
    )
preflight_diagnostic_tokens = (
    "failure_phase=staging_preflight",
    "staging_preflight_failed_path=$($entry.Key)",
    "staging_preflight_error=$($_.Exception.Message)",
    "AppendAllText($attemptStderrPath",
    "AppendAllText($attemptExitMarkerPath",
    "rollback_required=true",
)
for token in preflight_diagnostic_tokens:
    assert token in text, (
        "staging/preflight failure must preserve exact failed path/error in unique attempt "
        f"diagnostics: missing={token}"
    )
PY
then
  package_contract_failed=1
fi

if ! python3 - \
  "$windows_script" \
  "$rollback_backup_fixture" \
  "$actual_rollback_backup_fixture" \
  "$legacy_rollback_backup_fixture" <<'PY'
from pathlib import Path, PureWindowsPath
import hashlib
import json
import re
import sys

text = Path(sys.argv[1]).read_text(encoding="utf-8")
backup_root = Path(sys.argv[2])
actual_backup_root = Path(sys.argv[3])
legacy_backup_root = Path(sys.argv[4])
backup_manifest = json.loads(
    (backup_root / "backup-manifest.json").read_text(encoding="utf-8")
)
actual_backup_manifest = json.loads(
    (actual_backup_root / "backup-manifest.json").read_text(encoding="utf-8")
)
legacy_backup_manifest = json.loads(
    (legacy_backup_root / "backup-manifest.json").read_text(encoding="utf-8")
)
assert (backup_root / "runtime/oasis7_chain_runtime.exe").is_file()
assert not (backup_root / "bin/oasis7_chain_runtime.exe").exists()
assert backup_manifest["runtime"]["relative_path"] == (
    "runtime/oasis7_chain_runtime.exe"
)
assert backup_manifest["runtime"]["sha256"] == hashlib.sha256(
    (backup_root / "runtime/oasis7_chain_runtime.exe").read_bytes()
).hexdigest()
assert actual_backup_manifest["runtime_path"] == r"runtime\oasis7_chain_runtime.exe"
assert actual_backup_manifest["runtime_sha256"] == hashlib.sha256(
    (actual_backup_root / "runtime/oasis7_chain_runtime.exe").read_bytes()
).hexdigest()
assert not (actual_backup_root / "bin/oasis7_chain_runtime.exe").exists()
legacy_runtime_path = PureWindowsPath(legacy_backup_manifest["runtime_path"])
assert legacy_runtime_path.is_absolute()
assert legacy_runtime_path.parts[0].lower() != "runtime"
assert legacy_backup_manifest["runtime_sha256"] == hashlib.sha256(
    (legacy_backup_root / "runtime/oasis7_chain_runtime.exe").read_bytes()
).hexdigest()
assert not (legacy_backup_root / "bin/oasis7_chain_runtime.exe").exists()
assert "$rollbackBackupRoot = [Environment]::ExpandEnvironmentVariables('C:\\oasis7-deploy\\backups\\task-2269-fixture')" in text, (
    "Windows rollback omits the configured known-good backup root"
)
resolver_match = re.search(
    r"function Resolve-RollbackRuntimeSource\s*\{(?P<body>.*?)\n\}",
    text,
    re.DOTALL,
)
assert resolver_match, (
    "Windows rollback must resolve authorized runtime/ backup layout through "
    "manifest/provenance before deterministic fallback candidates"
)
resolver = resolver_match.group("body")
counted_candidates = set(re.findall(r"\$(\w+)\.Count", resolver))
indexed_candidates = set(re.findall(r"\$(\w+)\[0\]", resolver))
pipeline_candidates = sorted(counted_candidates & indexed_candidates)
assert {"metadataPaths", "legacyConfinedCandidates", "fallbackCandidates"}.issubset(
    pipeline_candidates
), "resolver candidate-pipeline fixture no longer covers all known Count/index consumers"
unsafe_scalar_pipelines = []
for variable in pipeline_candidates:
    assignment = re.search(
        rf"\${variable}\s*=\s*(?P<expression>.*?)"
        rf"(?=\n\s*if\s*\(\${variable}\.Count)",
        resolver,
        re.DOTALL,
    )
    assert assignment, f"could not isolate candidate pipeline assignment: {variable}"
    expression = assignment.group("expression").strip()
    if "|" in expression and not (
        expression.startswith("@(") and expression.endswith(")")
    ):
        unsafe_scalar_pipelines.append(variable)
assert not unsafe_scalar_pipelines, (
    "PowerShell resolver candidate pipelines consumed by .Count/[0] must wrap "
    "the complete pipeline in @(...), so one candidate remains a full path: "
    f"unsafe={unsafe_scalar_pipelines}"
)
assert "runtime_path" in resolver and "runtime_sha256" in resolver, (
    "Windows rollback resolver must accept authorized top-level "
    "runtime_path/runtime_sha256 manifest schema"
)
for token in ("GetFullPath", "StartsWith", "Get-FileHash"):
    assert token in resolver, (
        "top-level rollback runtime metadata must retain path confinement and SHA closure: "
        f"missing={token}"
    )
legacy_rooted_tokens = (
    "legacy rooted runtime_path",
    "confined backup runtime candidate missing",
    "confined backup runtime candidate ambiguous",
    "confined backup runtime sha256 mismatch",
)
missing_legacy_rooted_tokens = [
    token for token in legacy_rooted_tokens if token not in resolver
]
assert not missing_legacy_rooted_tokens, (
    "Windows rollback resolver must map an escaped legacy runtime_path only to "
    "a unique confined SHA-matching backup candidate: "
    f"missing={missing_legacy_rooted_tokens}"
)
manifest_index = resolver.find("backup-manifest.json")
runtime_candidate_index = resolver.find(r"runtime\oasis7_chain_runtime.exe")
bin_candidate_index = resolver.find(r"bin\oasis7_chain_runtime.exe")
assert 0 <= manifest_index < runtime_candidate_index < bin_candidate_index, (
    "rollback runtime resolution order must be backup manifest/provenance, "
    "runtime/oasis7_chain_runtime.exe, then bin/oasis7_chain_runtime.exe"
)
assert "ConvertFrom-Json" in resolver, (
    "rollback runtime resolver must consume available backup manifest provenance"
)
assert re.search(r"runtime.*(relative_path|path)", resolver, re.IGNORECASE | re.DOTALL), (
    "rollback runtime resolver must accept a manifest-provenance runtime path"
)
assert "Get-FileHash" in resolver, (
    "rollback runtime resolver must close an available provenance SHA before restore"
)
assert "rollback runtime ambiguous" in resolver, (
    "rollback runtime resolver must reject multiple valid runtime candidates"
)
assert "known-good rollback runtime missing" in resolver, (
    "rollback runtime resolver must reject missing manifest and fallback candidates"
)
assert text.count("Resolve-RollbackRuntimeSource") >= 3, (
    "rollback preflight and restoration must use the same runtime resolver"
)
timeout_match = re.search(r"\$rollbackUnlockTimeoutSeconds\s*=\s*(\d+)", text)
assert timeout_match, "Windows rollback must define a bounded process-exit/file-unlock timeout"
timeout_seconds = int(timeout_match.group(1))
assert timeout_seconds == 30, (
    "Windows rollback ignored rollback_unlock_timeout_secs fixture: "
    f"expected=30 actual={timeout_seconds}"
)
rollback_index = text.index("rollback_begin=true")
rollback = text[rollback_index:]
PY
then
  package_contract_failed=1
fi

if ! python3 - "$windows_script" <<'PY'
from pathlib import Path
import re
import sys

data = Path(sys.argv[1]).read_bytes()
assert not data.startswith(b"\xef\xbb\xbf"), "PowerShell script must be UTF-8 without BOM"
text = data.decode("utf-8")
assert "Set-JsonProperty $json.runtime_build 'sha256' $hash" in text
assert "$installer = [Environment]::ExpandEnvironmentVariables('C:/oasis7-deploy/oasis7-windows-x64.exe')" in text
assert 'throw "governed bundle missing runtime_build' in text
assert "[System.Text.UTF8Encoding]::new($false)" in text
assert "Start-ScheduledTask -TaskName $taskName" in text
assert "public-testnet-governed-bootstrap-bundle-2026-06-06.windows.json" in text
assert "-Filter '*bundle*.json'" not in text
governed_keys = (
    "governance_public_manifest_ref",
    "liveops_public_manifest_ref",
    "binding_notes_ref",
    "genesis_validator_registry_ref",
    "topology_ref",
)
for key in governed_keys:
    assert key in text, f"Windows deployment omits governed ref localization for {key}"
assert "governance_bootstrap_refs" in text
assert "doc\\testing\\evidence" in text
localization_index = text.index("governance_bootstrap_refs")
missing_source_index = text.index("genesis governance ref source missing")
collision_index = text.index("genesis governance refs collide at localized target")
genesis_lookup_index = text.index("$genesis = Get-Item $genesisPath")
stop_index = text.index("Stop-ScheduledTask -TaskName $taskName")
install_index = text.index("$install = Start-Process")
start_index = text.index("Start-ScheduledTask -TaskName $taskName")
assert localization_index < start_index, "governed refs must be localized before observer start"
for label, index in (
    ("genesis lookup", genesis_lookup_index),
    ("missing-source preflight", missing_source_index),
    ("basename-collision preflight", collision_index),
):
    assert index < stop_index, f"{label} must complete before Stop-ScheduledTask"
    assert index < install_index, f"{label} must complete before installer mutation"
assert text.count("[System.Text.UTF8Encoding]::new($false)") >= 2, (
    "bundle and localized genesis must both be emitted as UTF-8 without BOM"
)

bundle_path_sections = (
    "runtime_build",
    "world_snapshot",
    "generated_world_sidecar",
    "world_generation_provenance",
    "governance_manifest",
    "network_manifest",
    "evidence_refs",
)
manifest_runtime_refs = (
    "release_candidate_bundle_ref",
    "genesis_ref",
    "bootstrap_peer_ref",
    "generated_world_sidecar_ref",
    "world_generation_provenance_ref",
)
for field in bundle_path_sections + manifest_runtime_refs:
    assert field in text, f"Windows deployment omits path localization for {field}"
assert "public-testnet-governed-bootstrap-manifest-2026-06-06.windows.json" in text
assert "public-testnet-governed-bootstrap-bootstrap-peers-2026-06-06.windows.txt" in text
assert "generated-world\\world" in text
assert "generated-world\\generated-scenario-world" in text
assert "build-host absolute path survived Windows localization" in text
assert "D:\\a\\oasis7" not in text, "generated PowerShell embeds a build-host path"

tree_contract_tokens = (
    "sha256_tree",
    "file_count",
    "total_bytes",
    "Get-ChildItem",
    "-Recurse",
    "world_snapshot tree integrity mismatch",
    "generated_world_sidecar tree integrity mismatch",
)
for token in tree_contract_tokens:
    assert token in text, f"Windows deployment omits tree integrity contract token: {token}"

# The package producer hashes every recursive member as
# relative-path NUL sha256 NUL decimal-size LF.  Omitting the size lets a
# Windows verifier accept a different tree-hash stream from the package
# metadata producer, even though it separately reports a total byte count.
tree_metadata_match = re.search(
    r"function Get-TreeMetadata\s*\{(?P<body>.*?)\n\}", text, re.DOTALL
)
assert tree_metadata_match, "generated Windows rollout omits Get-TreeMetadata"
tree_metadata = tree_metadata_match.group("body")
assert re.search(
    r"\$relative\s*\+\s*\[char\]0\s*\+\s*\$fileHash\s*\+\s*\[char\]0\s*"
    r"\+\s*\[string\]\$file\.Length\s*\+\s*\"`n\"",
    tree_metadata,
), (
    "Windows tree metadata must hash canonical relative-path NUL sha256 NUL "
    "decimal-size LF records, matching package metadata"
)
path_localization_index = min(text.index(field) for field in bundle_path_sections)
tree_integrity_index = min(
    text.index("world_snapshot tree integrity mismatch"),
    text.index("generated_world_sidecar tree integrity mismatch"),
)
assert path_localization_index < stop_index
assert tree_integrity_index < stop_index
assert path_localization_index < install_index
assert tree_integrity_index < install_index

timeout_match = re.search(r"\$verificationTimeoutSeconds\s*=\s*(\d+)", text)
assert timeout_match, "rpc-running verification must define a bounded startup timeout"
timeout_seconds = int(timeout_match.group(1))
assert 60 <= timeout_seconds <= 300, (
    "rpc-running verification timeout must allow normal startup without becoming unbounded: "
    f"{timeout_seconds}s"
)
assert "$verificationDeadline = (Get-Date).AddSeconds($verificationTimeoutSeconds)" in text
assert "while ((Get-Date) -lt $verificationDeadline)" in text
assert re.search(
    r"\$processRunning\s*=.*Get-Process\s+oasis7_chain_runtime",
    text,
), "rpc-running process evidence must come from the observer process"
assert re.search(
    r"\$statusRunning\s*=\s*\$status\.running\s*-eq\s*\$true",
    text,
), "rpc-running status evidence must require status running=true"
assert re.search(
    r"if\s*\(\s*\$processRunning\s+-and\s+\$statusRunning\s*\)",
    text,
), "rpc-running verification may succeed only when process and RPC status are both running"
assert "STATUS_ERROR=" not in text, "RPC failures must not be caught and printed as success"
deadline_failure_match = re.search(
    r"if\s*\(\s*!\$verified\s*\)\s*\{(?P<body>.*?)\}",
    text,
    re.DOTALL,
)
assert deadline_failure_match, "rpc-running verification must check failure after its poll deadline"
assert "throw " in deadline_failure_match.group("body"), (
    "rpc-running verification must exit nonzero after the poll deadline"
)
PY
then
  package_contract_failed=1
fi

if ! python3 - "$windows_script" <<'PY'
from pathlib import Path
import sys

text = Path(sys.argv[1]).read_text(encoding="utf-8")
stop_index = text.index("Stop-ScheduledTask -TaskName $taskName")
install_index = text.index("$install = Start-Process")

# The rollback source set must be completely validated before the first
# task/runtime/config mutation.  A success marker is deliberately required so
# the incomplete-backup branch cannot remain an implicit, rollout-time error.
rollback_closure_marker = "rollback_closure_complete=true"
assert rollback_closure_marker in text, (
    "Windows rollout must emit rollback closure completion only after every "
    "runtime, config, and provenance backup source has been preflighted"
)
rollback_closure_index = text.index(rollback_closure_marker)
assert rollback_closure_index < stop_index, (
    "incomplete rollback closure must fail before Stop-ScheduledTask"
)
assert rollback_closure_index < install_index, (
    "incomplete rollback closure must fail before installer mutation"
)
assert rollback_closure_index < text.index("Copy-StagedFileAtomically"), (
    "incomplete rollback closure must fail before active config promotion"
)
PY
then
  package_contract_failed=1
fi

if ! python3 - "$windows_script" <<'PY'
from pathlib import Path
import re
import sys

text = Path(sys.argv[1]).read_text(encoding="utf-8")
task_start = text.index("Start-ScheduledTask -TaskName $taskName")
startup_poll = text[task_start:text.index("if (!$verified)", task_start)]

# Fixture: Get-ScheduledTaskInfo can report a fresh LastRunTime with 0x41301
# while the wrapper is still running and has not yet emitted this attempt's
# terminal exit marker.  That is nonterminal and must keep polling, not roll
# back a healthy observer.
fresh_running_without_marker = {
    "last_run_time_is_fresh": True,
    "last_task_result": 267009,  # 0x41301: task is currently running
    "exact_attempt_exit_marker": None,
    "expects_terminal_failure": False,
    "expects_rollback": False,
}
terminal_child_failure = {
    "last_run_time_is_fresh": True,
    "last_task_result": 1,
    "exact_attempt_exit_marker": 255,
    "expects_terminal_failure": True,
    "expects_rollback": True,
}
assert fresh_running_without_marker["last_task_result"] == 267009
assert fresh_running_without_marker["exact_attempt_exit_marker"] is None
assert not fresh_running_without_marker["expects_rollback"]
assert terminal_child_failure["exact_attempt_exit_marker"] == 255
assert terminal_child_failure["expects_rollback"]

assert re.search(
    r"\$schedulerRunningResult\s*=\s*267009\b", startup_poll
), (
    "fresh LastTaskResult=267009 (0x41301 task running) needs an explicit "
    "nonterminal scheduler classification"
)
assert re.search(
    r"\$schedulerReportsRunning\s*=\s*\$lastTaskResult\s*-eq\s*\$schedulerRunningResult",
    startup_poll,
), "startup poll must classify 0x41301 from the fresh scheduler observation"
assert re.search(
    r"\$terminalExitCode\s*=\s*if\s*\(\$null\s*-ne\s*\$attemptChildExitCode\)\s*\{\s*\$attemptChildExitCode\s*\}\s*else\s*\{\s*\$null\s*\}",
    startup_poll,
), (
    "without the exact current-attempt exit marker, scheduler status alone "
    "must not manufacture a terminal child exit code"
)
terminal_guard = re.search(
    r"if\s*\((?P<condition>.*?)\)\s*\{\s*Preserve-AttemptDiagnostics",
    startup_poll,
    re.DOTALL,
)
assert terminal_guard, "startup poll must retain one marker-bound terminal failure guard"
condition = terminal_guard.group("condition")
assert "$attemptChildExitCode" in condition, (
    "terminal child failure and rollback must require the exact current-attempt exit marker"
)
assert "$newTaskResult" not in condition, (
    "a fresh scheduler result, including 0x41301, must not independently request rollback"
)
assert "$schedulerReportsRunning" in startup_poll, (
    "the running scheduler result must remain nonterminal while the attempt marker is absent"
)
PY
then
  package_contract_failed=1
fi

if ! python3 - "$windows_script" <<'PY'
from pathlib import Path
import re
import sys

text = Path(sys.argv[1]).read_text(encoding="utf-8")
assert "$installRoot = [Environment]::ExpandEnvironmentVariables('C:\\Users\\Observer\\AppData\\Local\\Programs\\oasis7')" in text, (
    "configured node-local install root must survive plan generation"
)

assert "$localizedPreview -match" not in text, (
    "node-local validation must inspect structured path-bearing fields, not arbitrary JSON text"
)
assert "function Test-NodeLocalPath" in text, (
    "Windows rollout must provide one explicit node-local path classifier"
)
classifier_match = re.search(
    r"function Test-NodeLocalPath\s*\{(?P<body>.*?)\n\}", text, re.DOTALL
)
assert classifier_match, "could not isolate Test-NodeLocalPath contract"
classifier = classifier_match.group("body")
for token in ("IsPathRooted", "$deployRoot", "$installRoot"):
    assert token in classifier, f"path classifier omits allowed-root boundary: {token}"
assert re.search(r"(?i)allowed.*root|root.*allowed", classifier), (
    "path classifier must accept only configured node-local roots, including "
    r"C:\oasis7-deploy and C:\Users\... install roots"
)
classifier_fixtures = {
    r"C:\oasis7-deploy\config\genesis.json": True,
    r"C:\Users\Observer\AppData\Local\Programs\oasis7\bin\oasis7_chain_runtime.exe": True,
    r"D:\a\oasis7\oasis7\public-testnet-stage\genesis.json": False,
    r"E:\foreign-node\genesis.json": False,
    r"/Users/build/oasis7/genesis.json": False,
    r"/home/build/oasis7/genesis.json": False,
}
assert set(classifier_fixtures.values()) == {False, True}
assert "operator_note" not in classifier, (
    "arbitrary non-path JSON text must remain outside node-local path classification"
)

physical_contract = re.search(
    r"function Assert-NodeLocalPhysicalPath\s*\{\s*param\((?P<params>.*?)\)\s*if",
    text,
    re.DOTALL,
)
assert physical_contract, "could not isolate Assert-NodeLocalPhysicalPath parameter contract"
physical_params = physical_contract.group("params")
assert re.findall(r"\[string\]\s+\$(\w+)", physical_params) == ["Path", "Label"], (
    "physical path guard must expose exactly one Path and one Label parameter"
)
assert physical_params.count("Mandatory = $true") == 2
assert "ParameterSetName" not in physical_params
physical_guard = text[
    physical_contract.start():text.index("function Preserve-AttemptDiagnostics")
]
assert "Split-Path -LiteralPath $probe -Parent" not in physical_guard, (
    "PowerShell 5.1 places LiteralPath and Parent in incompatible Split-Path parameter sets"
)
assert "$parent = [System.IO.Path]::GetDirectoryName($probe)" in physical_guard, (
    "physical path guard must use wildcard-safe .NET lexical parent traversal"
)
reparse_check = physical_guard.index("[System.IO.FileAttributes]::ReparsePoint")
parent_step = physical_guard.index("[System.IO.Path]::GetDirectoryName($probe)")
probe_advance = physical_guard.index("$probe = $parent")
assert reparse_check < parent_step < probe_advance, (
    "physical path guard must inspect each path before walking to its parent"
)
assert "$parent.Equals($probe, [System.StringComparison]::OrdinalIgnoreCase)" in physical_guard, (
    "physical path guard must retain explicit root/self-parent termination"
)

# Keep the explicit string casts at object/array call boundaries as independent
# enforcement of the function's declared string contract. They are not the
# cause or fix for the Split-Path parameter-set failure above.
normalized_calls = re.sub(r"`\r?\n\s*", " ", text)
physical_calls = [
    line.strip()
    for line in normalized_calls.splitlines()
    if line.lstrip().startswith("Assert-NodeLocalPhysicalPath ")
]
assert physical_calls, "generated rollout never invokes its physical path guard"
for call in physical_calls:
    assert len(re.findall(r"(?<!\w)-Path\b", call)) == 1, (
        f"physical path guard call must bind Path exactly once: {call}"
    )
    assert len(re.findall(r"(?<!\w)-Label\b", call)) == 1, (
        f"physical path guard call must bind Label exactly once: {call}"
    )
    assert not re.search(r"-(?:Path|Label)\s+\$\w+(?:\.|\[)", call), (
        "PowerShell 5.1-incompatible ungrouped member/index argument in physical "
        f"path guard call: {call}"
    )
assert any(
    "-Path ([string]$physicalPreflightTarget.path)" in call
    and "-Label ([string]$physicalPreflightTarget.label)" in call
    for call in physical_calls
), "native preflight fixture must retain the grouped member-expression call form"
assert any(
    "-Path ([string]$metadataPaths[0])" in call for call in physical_calls
), "native rollback fixture must retain the grouped index-expression call form"
assert any(
    "-Path $logRoot -Label 'active deploy log root'" in call
    for call in physical_calls
), "native fixture must retain the scalar-variable/literal call form"
PY
then
  package_contract_failed=1
fi

if ! python3 - "$windows_script" <<'PY'
from pathlib import Path
import sys

text = Path(sys.argv[1]).read_text(encoding="utf-8")

structured_path_fields = (
    "runtime_build.path",
    "runtime_build.resolved_path",
    "world_snapshot.resolved_path",
    "generated_world_sidecar.resolved_path",
    "world_generation_provenance.resolved_path",
    "governance_manifest.resolved_path",
    "network_manifest.resolved_path",
    "evidence_refs",
    "runtime_refs.release_candidate_bundle_ref",
    "runtime_refs.genesis_ref",
    "runtime_refs.bootstrap_peer_ref",
    "runtime_refs.generated_world_sidecar_ref",
    "runtime_refs.world_generation_provenance_ref",
    "governance_bootstrap_refs",
)
for field in structured_path_fields:
    assert field in text, f"structured node-local validation omits staged path field: {field}"
assert "staged target path is not node-local" in text, (
    "all staged target path fields need one fail-closed post-rewrite diagnostic"
)
rewrite_end = text.index("$manifestText = $manifestJson | ConvertTo-Json")
path_validation_index = text.index("staged target path is not node-local")
stop_index = text.index("Stop-ScheduledTask -TaskName $taskName")
assert rewrite_end <= path_validation_index < stop_index, (
    "structured staged-path validation must run after rewrite and before restart/install mutation"
)
PY
then
  package_contract_failed=1
fi

if ! python3 - "$windows_script" <<'PY'
from pathlib import Path
import re
import sys

text = Path(sys.argv[1]).read_text(encoding="utf-8")
stop_index = text.index("Stop-ScheduledTask -TaskName $taskName")

task_start_index = text.index("Start-ScheduledTask -TaskName $taskName")
assert "Get-ScheduledTaskInfo" in text[task_start_index:], (
    "Windows startup poll does not inspect terminal scheduled-task child results"
)
task_info_index = text.index("Get-ScheduledTaskInfo", task_start_index)
task_result_index = text.index("LastTaskResult", task_info_index)
terminal_child_failure_index = text.index("terminal task child exited nonzero", task_result_index)
poll_sleep_index = text.index("Start-Sleep -Seconds 2", task_result_index)
assert task_info_index < terminal_child_failure_index < poll_sleep_index, (
    "terminal scheduled-task child failure must be detected before another startup poll sleep"
)
verification_tail = text[task_start_index:]
assert "rollback_required=true" in verification_tail
assert "failure_diagnostics=" in verification_tail
assert "Get-Content" in verification_tail and "-Tail" in verification_tail, (
    "terminal child failure must emit bounded existing-log diagnostics"
)
assert "Set-Content" not in verification_tail and "Out-File" not in verification_tail, (
    "startup failure handling must not overwrite existing task logs"
)

attempt_contract = (
    "$attemptId",
    "$attemptStdoutPath",
    "$attemptStderrPath",
    "$attemptExitMarkerPath",
)
for token in attempt_contract:
    assert token in text, f"scheduled-task wrapper omits unique attempt artifact: {token}"
assert re.search(r"attemptId\s*=.*Guid.*NewGuid", text, re.IGNORECASE), (
    "scheduled-task wrapper must generate a unique identifier for every attempt"
)
assert "New-ScheduledTaskAction" in text or "Set-ScheduledTask" in text, (
    "rollout must install an attempt-aware scheduled-task wrapper before restart"
)
wrapper_index = min(
    index for token in ("New-ScheduledTaskAction", "Set-ScheduledTask")
    if (index := text.find(token)) >= 0
)
assert wrapper_index < task_start_index, "attempt wrapper must be installed before task start"
assert "AppendAllText" in text, (
    "attempt stdout/stderr/exit evidence must use append, non-truncating writes"
)
for truncating_token in (
    "[System.IO.File]::WriteAllText($attemptStdoutPath",
    "[System.IO.File]::WriteAllText($attemptStderrPath",
    "[System.IO.File]::WriteAllText($attemptExitMarkerPath",
):
    assert truncating_token not in text, (
        f"attempt evidence must not be truncated: {truncating_token}"
    )

preserve_occurrences = [
    match.start() for match in re.finditer("Preserve-AttemptDiagnostics", text)
]
assert len(preserve_occurrences) >= 3, (
    "attempt preservation contract needs a function plus pre-restart and pre-rollback calls"
)
assert any(index < stop_index for index in preserve_occurrences[1:]), (
    "pre-existing attempt diagnostics must be preserved before restart/install mutation"
)
rollback_index = text.index("rollback_required=true", task_start_index)
assert any(task_start_index < index < rollback_index for index in preserve_occurrences), (
    "current-attempt diagnostics must be preserved before rollback is requested"
)
diagnostic_tail = text[task_start_index:]
for token in attempt_contract[1:]:
    assert token in diagnostic_tail, (
        f"generated terminal diagnostics must reference exact attempt artifact: {token}"
    )
assert "Get-ChildItem -LiteralPath $logRoot" not in diagnostic_tail, (
    "terminal diagnostics must not substitute unrelated newest log files for exact attempt evidence"
)
PY
then
  package_contract_failed=1
fi

if ! python3 - "$windows_script" <<'PY'
from pathlib import Path
import re
import sys

text = Path(sys.argv[1]).read_text(encoding="utf-8")

exit_capture = re.search(
    r"\$(?P<name>[A-Za-z][A-Za-z0-9]*exit[A-Za-z0-9]*code)\s*="
    r"\s*\$childProcess\.ExitCode",
    text,
    re.IGNORECASE,
)
assert exit_capture, "scheduled-task wrapper must capture the exact Process.ExitCode"
exit_name = exit_capture.group("name")
assert re.search(
    rf"AppendAllText\([^\n]*attemptExitMarkerPath[^\n]*\${re.escape(exit_name)}",
    text,
    re.IGNORECASE,
), "exit marker must record the same unmodified child exit-code variable"
assert re.search(rf"exit\s+\${re.escape(exit_name)}\b", text, re.IGNORECASE), (
    "scheduled-task wrapper must propagate the recorded child exit code unchanged; "
    "the terminal-255 fixture must remain 255"
)
PY
then
  package_contract_failed=1
fi

if ! python3 - "$windows_script" <<'PY'
from pathlib import Path
import re
import sys

text = Path(sys.argv[1]).read_text(encoding="utf-8")

# The scheduled-task wrapper must invoke the action captured from the trusted
# task definition directly.  A second ComSpec /c changes cmd.exe parsing and
# caused the native Windows rollout action to exit 1 before selective rollback.
assert re.search(
    r"\$childExecute\s*=\s*(?:\[string\])?\$originalTaskAction\.Execute", text
), (
    "scheduled-task wrapper must preserve OriginalTaskAction.Execute as its direct child"
)
assert re.search(
    r"\$childArguments\s*=\s*(?:\[string\])?\$originalTaskAction\.Arguments", text
), (
    "scheduled-task wrapper must preserve OriginalTaskAction.Arguments as its direct child"
)
assert not re.search(r"\$env:ComSpec\s+(?:(?:/d|/s)\s+)*/c\b", text, re.IGNORECASE), (
    "scheduled-task wrapper must not launch the trusted task action through nested $env:ComSpec /c"
)

process_start_info = re.search(
    r"\$processStartInfo\s*=\s*\[System\.Diagnostics\.ProcessStartInfo\]::new\(\)"
    r"(?P<body>.*?)\$childProcess\s*=\s*\[System\.Diagnostics\.Process\]::new\(\)",
    text,
    re.DOTALL,
)
assert process_start_info, (
    "scheduled-task wrapper must construct System.Diagnostics.ProcessStartInfo for the trusted action"
)
start_info_body = process_start_info.group("body")
for assignment in (
    "$processStartInfo.FileName = $childExecute",
    "$processStartInfo.Arguments = $childArguments",
    "$processStartInfo.UseShellExecute = $false",
    "$processStartInfo.CreateNoWindow = $true",
):
    assert assignment in start_info_body, (
        "scheduled-task wrapper must directly configure ProcessStartInfo: " + assignment
    )

for async_contract in (
    "add_OutputDataReceived",
    "add_ErrorDataReceived",
    "BeginOutputReadLine()",
    "BeginErrorReadLine()",
    "WaitForExit()",
):
    assert async_contract in text, (
        "scheduled-task wrapper must capture stdout/stderr concurrently without pipe deadlock: "
        + async_contract
    )
assert "ReadToEnd()" not in text, (
    "scheduled-task wrapper must not sequentially drain redirected stdout/stderr pipes"
)
PY
then
  package_contract_failed=1
fi

"$rollout_driver" \
  --manifest "$TMP_DIR/manifest.json" \
  --package-dir "$package_dir" \
  --out-dir "$TMP_DIR/repeated-plan-out" \
  --known-hosts "$operator_known_hosts" \
  --json >"$TMP_DIR/repeated-plan.json"
cmp -s \
  "$TMP_DIR/plan-only-out/windows-observer-windows-upgrade.ps1" \
  "$TMP_DIR/repeated-plan-out/windows-observer-windows-upgrade.ps1"
python3 - "$windows_genesis" "$TMP_DIR/repeated-plan-out/windows-observer-windows-upgrade.ps1" <<'PY'
from pathlib import Path
import sys

for raw_path in sys.argv[1:]:
    path = Path(raw_path)
    assert not path.read_bytes().startswith(b"\xef\xbb\xbf"), f"UTF-8 BOM forbidden: {path}"
PY

if ! python3 - "$ROOT_DIR/doc/p2p/blockchain/public-testnet-governed-bootstrap.runbook.md" <<'PY'
from pathlib import Path
import sys

text = Path(sys.argv[1]).read_text(encoding="utf-8")
health_start = text.index("### Verdict rules")
health_end = text.index("### Operator communication boundary", health_start)
health = text[health_start:health_end]
assert "consensus.network_head.decision=ready" in health, (
    "fleet-healthy recovery updates must require ready network-head decisions"
)
assert "same capture window" in health, (
    "fleet-healthy recovery updates need one concrete same-window head criterion"
)
assert "exactly equal" in health, (
    "without a repository-defined lag threshold, fleet heads must be exact-equality gated"
)

handoff_start = text.index("### Windows package rollback 后的人工 handoff")
handoff_end = text.index("### What not to do", handoff_start)
handoff = text[handoff_start:handoff_end]
for marker in (
    "staged_sha_closure_complete=true",
    "promotion_begin=true",
    "promotion_complete=true",
):
    assert marker in handoff, f"Windows rollback handoff omits marker branch: {marker}"
for state in ("State=Running", "State=Ready"):
    assert state in handoff, f"Windows rollback handoff omits observed task state: {state}"
assert "不得执行 `Start-ScheduledTask`" in handoff, (
    "preflight failures must be remediated without restart"
)
assert "`rollback_applied=true`" in handoff and "`promotion_begin=true`" in handoff, (
    "manual restart eligibility must bind rollback and promotion evidence"
)
assert "`State=Ready`" in handoff and "可人工执行 `Start-ScheduledTask -TaskName Oasis7Observer`" in handoff, (
    "manual restart must be limited to a restored, stopped task"
)
common_start = text.index("### Current five-node status")
common_end = text.index("### Peer id truth", common_start)
common_commands = text[common_start:common_end]
assert "Start-ScheduledTask" not in common_commands, (
    "common fleet-status commands must never restart a Windows observer"
)
handoff_command_start = handoff.index("最小 Windows handoff 命令")
handoff_commands = handoff[handoff_command_start:]
assert "Start-ScheduledTask" not in handoff_commands, (
    "the common Windows handoff command block must not contain an unconditional restart; "
    "Start-ScheduledTask may appear only in the restored-post-stop eligibility branch"
)
PY
then
  package_contract_failed=1
fi

missing_network_metadata_package="$TMP_DIR/missing-network-manifest-metadata-package"
cp -R "$package_dir" "$missing_network_metadata_package"
python3 - "$missing_network_metadata_package/windows/public-testnet-governed-bootstrap-bundle-2026-06-06.windows.json" <<'PY'
import json
from pathlib import Path
import sys

bundle_path = Path(sys.argv[1])
bundle = json.loads(bundle_path.read_text(encoding="utf-8"))
bundle.pop("network_manifest", None)
bundle_path.write_text(json.dumps(bundle, indent=2) + "\n", encoding="utf-8")
PY
(
  cd "$missing_network_metadata_package/windows"
  find . -type f ! -name windows-x64-SHA256SUMS | sed 's#^\./##' | sort |
    while IFS= read -r file; do shasum -a 256 "$file"; done >windows-x64-SHA256SUMS
)
missing_network_metadata_out="$TMP_DIR/missing-network-manifest-metadata-out"
"$rollout_driver" \
  --manifest "$TMP_DIR/manifest.json" \
  --package-dir "$missing_network_metadata_package" \
  --out-dir "$missing_network_metadata_out" \
  --known-hosts "$operator_known_hosts" \
  --json >"$TMP_DIR/missing-network-manifest-metadata-plan.json"
if ! python3 - "$missing_network_metadata_out/windows-observer-windows-upgrade.ps1" <<'PY'
from pathlib import Path
import re
import sys

text = Path(sys.argv[1]).read_text(encoding="utf-8")
assert "$json.network_manifest" not in text, (
    "bundle network_manifest metadata is optional when the staged manifest is "
    "already independently checksum-verified"
)
metadata_match = re.search(
    r"\$networkManifestMetadata\s*=\s*\[PSCustomObject\]@\{(?P<body>.*?)\n\}",
    text,
    re.DOTALL,
)
assert metadata_match, (
    "generated rollout must derive network manifest metadata from the verified "
    "staged manifest when bundle metadata is absent"
)
metadata = metadata_match.group("body")
assert "$expectedGovernedSha256[$manifestPath]" in metadata, (
    "derived network manifest metadata must retain the staged checksum closure, "
    "not self-attest from unverified manifest bytes"
)
assert re.search(
    r"Assert-FileMetadata\s+\$networkManifestMetadata\s+\$manifestItem\.FullName\s+'network_manifest'",
    text,
), "network manifest integrity assertion must use verified staged metadata"
assert re.search(
    r"Set-ArtifactLocation\s+\$networkManifestMetadata\s+\$activeManifestPath",
    text,
), "localized network manifest must retain derived verified metadata"
PY
then
  package_contract_failed=1
fi

if [[ "$package_contract_failed" -ne 0 ]]; then
  exit 1
fi

python3 -W error::SyntaxWarning "$ROOT_DIR/scripts/p2p-public-testnet-package-rollout.py" --help >"$TMP_DIR/oasis7-package-rollout-help.out"
grep -q "mode is plan-only" "$TMP_DIR/oasis7-package-rollout-help.out"
grep -q "Mutation requires" "$TMP_DIR/oasis7-package-rollout-help.out"
grep -q "never reads or stores credentials" "$TMP_DIR/oasis7-package-rollout-help.out"
grep -q -- "--readiness-policy" "$TMP_DIR/oasis7-package-rollout-help.out"

bad_package_dir="$TMP_DIR/bad-package"
cp -R "$package_dir" "$bad_package_dir"
sed -i.bak "s/^commit=.*/commit=0000000000000000000000000000000000000000/" \
  "$bad_package_dir/windows/windows-x64-BUILDINFO"
rm "$bad_package_dir/windows/windows-x64-BUILDINFO.bak"
(
  cd "$bad_package_dir/windows"
  shasum -a 256 oasis7-windows-x64.exe windows-x64-BUILDINFO oasis7-windows-x64-ops-tools.tar.gz >windows-x64-SHA256SUMS
)
if "$rollout_driver" \
  --manifest "$TMP_DIR/manifest.json" \
  --package-dir "$bad_package_dir" \
  --out-dir "$TMP_DIR/bad-out" \
  --known-hosts "$operator_known_hosts" \
  >"$TMP_DIR/bad.out" 2>"$TMP_DIR/bad.err"; then
  echo "expected mismatched BUILDINFO to fail" >&2
  exit 1
fi
grep -q "does not match" "$TMP_DIR/bad.err"

bad_sums_dir="$TMP_DIR/bad-sums-package"
cp -R "$package_dir" "$bad_sums_dir"
(
  cd "$bad_sums_dir"
  shasum -a 256 linux-x64-BUILDINFO >linux-x64-SHA256SUMS
)
if "$rollout_driver" \
  --manifest "$TMP_DIR/manifest.json" \
  --package-dir "$bad_sums_dir" \
  --out-dir "$TMP_DIR/bad-sums-out" \
  --known-hosts "$operator_known_hosts" \
  >"$TMP_DIR/bad-sums.out" 2>"$TMP_DIR/bad-sums.err"; then
  echo "expected missing asset checksum coverage to fail" >&2
  exit 1
fi
grep -q "checksum file does not cover required artifact: oasis7-linux-x64.deb" "$TMP_DIR/bad-sums.err"

bad_ops_dir="$TMP_DIR/bad-ops-package"
cp -R "$package_dir" "$bad_ops_dir"
printf 'tampered-ops-tools\n' >>"$bad_ops_dir/oasis7-linux-x64-ops-tools.tar.gz"
(
  cd "$bad_ops_dir"
  # Keep the platform checksum current so the dedicated ops-tools checksum
  # contract, rather than the outer package checksum, is the failing guard.
  shasum -a 256 oasis7-linux-x64.deb linux-x64-BUILDINFO oasis7-linux-x64-ops-tools.tar.gz \
    >linux-x64-SHA256SUMS
)
if "$rollout_driver" \
  --manifest "$TMP_DIR/manifest.json" \
  --package-dir "$bad_ops_dir" \
  --out-dir "$TMP_DIR/bad-ops-out" \
  --known-hosts "$operator_known_hosts" \
  >"$TMP_DIR/bad-ops.out" 2>"$TMP_DIR/bad-ops.err"; then
  echo "expected tampered ops-tools archive to fail dedicated checksum validation" >&2
  exit 1
fi
grep -q 'checksum mismatch.*oasis7-linux-x64-ops-tools.tar.gz' "$TMP_DIR/bad-ops.err"

strict_node_root="$TMP_DIR/strict-node"
mkdir -p "$strict_node_root/releases/old/bin" "$strict_node_root/config/doc/testing/evidence"
printf 'runtime-v1\n' >"$strict_node_root/releases/old/bin/oasis7_chain_runtime"
chmod +x "$strict_node_root/releases/old/bin/oasis7_chain_runtime"
ln -s "$strict_node_root/releases/old" "$strict_node_root/current"
cp \
  "$node_root/config/doc/testing/evidence/public-testnet-governed-bootstrap-bundle-2026-06-06.json" \
  "$strict_node_root/config/doc/testing/evidence/public-testnet-governed-bootstrap-bundle-2026-06-06.json"
python3 - "$TMP_DIR/manifest.json" "$strict_node_root" <<'PY'
from pathlib import Path
import json
import sys

manifest = json.loads(Path(sys.argv[1]).read_text())
manifest["nodes"][0]["node_root"] = sys.argv[2]
manifest["nodes"][0]["restart"] = True
manifest["nodes"][0]["systemd_service"] = "oasis7-testnet-storage.service"
manifest["nodes"].append(
    {
        "name": "strict-linux-observer",
        "platform": "linux-x64",
        "node_root": sys.argv[2],
        "restart": True,
        "systemd_service": "oasis7-testnet-storage.service",
        "status_url": "http://127.0.0.1:6634/v1/chain/status",
    }
)
Path(sys.argv[1]).write_text(json.dumps(manifest, indent=2) + "\n")
PY
"$rollout_driver" \
  --manifest "$TMP_DIR/manifest.json" \
  --package-dir "$package_dir" \
  --out-dir "$TMP_DIR/strict-out" \
  --known-hosts "$operator_known_hosts" \
  --readiness-policy strict-ready \
  --json >"$TMP_DIR/strict-plan.json"
jq -e '
  .readiness_policy == "strict-ready"
  and (.nodes[] | select(.name == "sequencer") | .commands[0]
    | (index("--post-restart-health-url") != null)
    and (index("--post-restart-status-url") == null))' \
  "$TMP_DIR/strict-plan.json" >/dev/null

python3 - "$TMP_DIR/manifest.json" <<'PY'
from pathlib import Path
import json
import sys

manifest = json.loads(Path(sys.argv[1]).read_text())
next(node for node in manifest["nodes"] if node["name"] == "strict-linux-observer").pop(
    "status_url", None
)
Path(sys.argv[1]).write_text(json.dumps(manifest, indent=2) + "\n")
PY
if "$rollout_driver" \
  --manifest "$TMP_DIR/manifest.json" \
  --package-dir "$package_dir" \
  --out-dir "$TMP_DIR/strict-missing-status-out" \
  --known-hosts "$operator_known_hosts" \
  --readiness-policy strict-ready \
  >"$TMP_DIR/strict-missing-status.out" 2>"$TMP_DIR/strict-missing-status.err"; then
  echo "expected strict-ready restart without status_url to fail" >&2
  exit 1
fi
grep -q "uses strict-ready but has no status_url" "$TMP_DIR/strict-missing-status.err"

"$rollout_driver" \
  --manifest "$observer_gate_manifest" --package-dir "$package_dir" \
  --out-dir "$TMP_DIR/observer-closure-probe-out" \
  --known-hosts "$operator_known_hosts" --json >"$TMP_DIR/observer-closure-probe-plan.json"
probe_gate_script="$(python3 - "$TMP_DIR/observer-closure-probe-plan.json" <<'PY'
import json, sys
plan = json.load(open(sys.argv[1], encoding="utf-8"))
print(next(node["observer_checkpoint_gate_script"] for node in plan["nodes"] if "observer_checkpoint_gate_script" in node))
PY
)"
grep -q 'fixture-checkpoint-v2' "$probe_gate_script"

for mode in bad-digest non-network empty-candidates hash-size stale; do
  if OASIS7_TEST_PROBE_MODE="$mode" "$rollout_driver" \
    --manifest "$observer_gate_manifest" --package-dir "$package_dir" \
    --out-dir "$TMP_DIR/observer-probe-$mode-out" \
    --known-hosts "$operator_known_hosts" >/dev/null 2>"$TMP_DIR/observer-probe-$mode.err"; then
    echo "expected controlled probe mode $mode to fail" >&2; exit 1
  fi
  grep -q 'checkpoint closure probe' "$TMP_DIR/observer-probe-$mode.err"
done

# Legacy external input is rejected by argparse, and the legacy environment is
# rejected before the harness can be consulted.
if "$ROOT_DIR/scripts/p2p-public-testnet-package-rollout.py" --manifest "$observer_gate_manifest" \
  --package-dir "$package_dir" --known-hosts "$operator_known_hosts" \
  --checkpoint-closure-receipt "$checkpoint_closure_receipt" \
  --out-dir "$TMP_DIR/legacy-arg-out" >/dev/null 2>"$TMP_DIR/legacy-arg.err"; then
  echo "expected legacy checkpoint receipt argument to fail" >&2; exit 1
fi
grep -q 'unrecognized arguments' "$TMP_DIR/legacy-arg.err"
if OASIS7_CHECKPOINT_CLOSURE_RECEIPT="$checkpoint_closure_receipt" \
  "$ROOT_DIR/scripts/p2p-public-testnet-package-rollout.py" --manifest "$observer_gate_manifest" \
  --package-dir "$package_dir" --known-hosts "$operator_known_hosts" \
  --out-dir "$TMP_DIR/legacy-env-out" >/dev/null 2>"$TMP_DIR/legacy-env.err"; then
  echo "expected legacy checkpoint receipt environment to fail" >&2; exit 1
fi
grep -q 'caller-supplied checkpoint closure receipt is forbidden' "$TMP_DIR/legacy-env.err"

# The probe executes a Linux runtime even for a macOS-only mutation plan.  A
# damaged Linux Debian package must therefore stop before the in-process test probe is
# reached and before any observer gate script is generated.
non_linux_only_manifest="$TMP_DIR/non-linux-only-observer-manifest.json"
cat >"$non_linux_only_manifest" <<'EOF'
{"nodes":[
  {"name":"sequencer","platform":"macos-arm64","status_url":"http://127.0.0.1:6631/v1/chain/status"},
  {"name":"storage","platform":"macos-arm64","status_url":"http://127.0.0.1:6632/v1/chain/status"},
  {"name":"macos-observer-only","platform":"macos-arm64","node_root":"/Applications/oasis7","status_url":"http://127.0.0.1:19083/v1/chain/status"}
]}
EOF
tampered_linux_probe_package="$TMP_DIR/tampered-linux-probe-package"
cp -R "$package_dir" "$tampered_linux_probe_package"
printf 'tampered-before-probe\n' >>"$tampered_linux_probe_package/oasis7-linux-x64.deb"
non_linux_probe_marker="$TMP_DIR/non-linux-probe-started"
if OASIS7_TEST_INPROCESS_PROBE_MARKER="$non_linux_probe_marker" \
  "$rollout_driver" --manifest "$non_linux_only_manifest" \
  --package-dir "$tampered_linux_probe_package" \
  --out-dir "$TMP_DIR/non-linux-tampered-out" \
  --known-hosts "$operator_known_hosts" \
  >"$TMP_DIR/non-linux-tampered.out" 2>"$TMP_DIR/non-linux-tampered.err"; then
  echo "expected a tampered Linux probe bundle to fail a non-Linux observer plan" >&2
  exit 1
fi
grep -q 'checksum mismatch' "$TMP_DIR/non-linux-tampered.err"
test ! -e "$non_linux_probe_marker"
test ! -e "$TMP_DIR/non-linux-tampered-out/macos-observer-only-macos-arm64-upgrade.sh"

# The release CLI must not offer a caller-controlled process substitution for
# its trust-producing probe.  This was a RED regression on the pre-fix helper.
if rg -q 'OASIS7_TESTING|OASIS7_TEST_CHECKPOINT_CLOSURE_PROBE' \
  "$ROOT_DIR/scripts/p2p-public-testnet-package-rollout.py"; then
  echo "release rollout helper must not expose test probe environment overrides" >&2
  exit 1
fi
# Exercise the release executable with both retired environment names set.  A
# marker-writing replacement command must remain unreachable; the real probe
# still supplies the generated gate.
forbidden_override_marker="$TMP_DIR/forbidden-release-override-started"
forbidden_override="$TMP_DIR/forbidden-release-override.sh"
printf '#!/usr/bin/env bash\nprintf forbidden > %q\n' "$forbidden_override_marker" >"$forbidden_override"
chmod +x "$forbidden_override"
OASIS7_TESTING=1 OASIS7_TEST_CHECKPOINT_CLOSURE_PROBE="$forbidden_override" \
  "$ROOT_DIR/scripts/p2p-public-testnet-package-rollout.py" \
  --manifest "$observer_gate_manifest" --package-dir "$package_dir" \
  --out-dir "$TMP_DIR/release-cli-retired-env-out" \
  --known-hosts "$operator_known_hosts" --json \
  >"$TMP_DIR/release-cli-retired-env.out" \
  2>"$TMP_DIR/release-cli-retired-env.err" && {
    echo "fixture without governed node.env unexpectedly completed the real probe" >&2
    exit 1
  }
test ! -e "$forbidden_override_marker"
grep -q 'checkpoint closure probe failed:.*node_root must provide config/node.env' \
  "$TMP_DIR/release-cli-retired-env.err"
python3 - "$ROOT_DIR/scripts/p2p-public-testnet-package-rollout.py" <<'PY'
from pathlib import Path
import sys

source = Path(sys.argv[1]).read_text(encoding="utf-8")
main_body = source[source.index("def main()") :]
assert main_body.index("verify_package_trust(") < main_body.index(
    "run_checkpoint_closure_probe("
), "package trust verification must precede probe runtime execution"
PY

echo "ok: package rollout helper validates artifacts and standardizes linux/windows/macos replacement plans"
