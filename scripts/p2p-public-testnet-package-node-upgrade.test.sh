#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
grep -Fq 'oasis7_governance_registry_audit service-readback' \
  "$ROOT_DIR/scripts/p2p-public-testnet-package-node-upgrade.sh" || {
  printf 'upgrade preflight must require service-readback in the ops archive\n' >&2
  exit 1
}
TMP_DIR="$(mktemp -d "${TMPDIR:-/tmp}/oasis7-package-node-upgrade-test.XXXXXX")"
trap 'rm -rf "$TMP_DIR"' EXIT

file_metadata() {
  python3 - "$1" <<'PY'
import stat
import sys
from pathlib import Path

st = Path(sys.argv[1]).stat()
print(st.st_uid, st.st_gid, stat.S_IMODE(st.st_mode))
PY
}

node_root="$TMP_DIR/node"
bundle_root="$TMP_DIR/bundle/oasis7-linux-x64"
package_deb="$TMP_DIR/oasis7-linux-x64.deb"
unlisted_package_deb="$TMP_DIR/oasis7-linux-x64-unlisted.deb"
ops_tools_tar="$TMP_DIR/oasis7-linux-x64-ops-tools.tar.gz"
ops_bundle_root="$TMP_DIR/bundle/oasis7-linux-x64-ops-tools"
package_version="0.0.0+testnet.test.abcdef123456"
commit="abcdef1234567890abcdef1234567890abcdef12"
run_id="12345"

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

mkdir -p \
  "$bundle_root/bin" \
  "$node_root/config/doc/testing/evidence" \
  "$node_root/releases/old/bin" \
  "$node_root/data" \
  "$node_root/backups"
printf 'runtime-v2\n' >"$bundle_root/bin/oasis7_chain_runtime"
chmod +x "$bundle_root/bin/oasis7_chain_runtime"
cat >"$bundle_root/BUILDINFO" <<EOF
workflow=Testnet Packages
commit=$commit
package_version=$package_version
run_id=$run_id
platform=linux-x64
EOF
(cd "$bundle_root" && shasum -a 256 BUILDINFO bin/oasis7_chain_runtime >SHA256SUMS)
printf 'runtime-v1\n' >"$node_root/releases/old/bin/oasis7_chain_runtime"
chmod +x "$node_root/releases/old/bin/oasis7_chain_runtime"
touch -t 202001010000 "$node_root/releases/old"
ln -s "$node_root/releases/old" "$node_root/current"
printf 'keep-data\n' >"$node_root/data/sentinel.txt"
printf 'keep-config\n' >"$node_root/config/sentinel.txt"
printf 'keep-backups\n' >"$node_root/backups/sentinel.txt"

for index in 1 2 3 4 5; do
  mkdir -p "$node_root/releases/retention-candidate-$index/bin"
  printf 'runtime-retention-%s\n' "$index" \
    >"$node_root/releases/retention-candidate-$index/bin/oasis7_chain_runtime"
  chmod +x "$node_root/releases/retention-candidate-$index/bin/oasis7_chain_runtime"
  touch -t "20260601010${index}" "$node_root/releases/retention-candidate-$index"
done

cat >"$node_root/config/doc/testing/evidence/public-testnet-governed-bootstrap-bundle-2026-06-06.json" <<'EOF'
{
  "schema_version": "oasis7.release_candidate_bundle.v1",
  "git_commit": "old",
  "runtime_build": {
    "path": "old",
    "ref": "old",
    "resolved_path": "old",
    "sha256": "0000000000000000000000000000000000000000000000000000000000000000",
    "size_bytes": 1
  }
}
EOF
governed_bundle="$node_root/config/doc/testing/evidence/public-testnet-governed-bootstrap-bundle-2026-06-06.json"
chmod 600 "$governed_bundle"
printf 'old-version\n' >"$node_root/CURRENT_VERSION"
chmod 600 "$node_root/CURRENT_VERSION"
printf 'old-buildinfo\n' >"$node_root/DEPLOYED_BUILDINFO"
chmod 600 "$node_root/DEPLOYED_BUILDINFO"
read -r governed_uid governed_gid governed_mode <<<"$(file_metadata "$governed_bundle")"
read -r current_version_uid current_version_gid current_version_mode \
  <<<"$(file_metadata "$node_root/CURRENT_VERSION")"
read -r buildinfo_uid buildinfo_gid buildinfo_mode \
  <<<"$(file_metadata "$node_root/DEPLOYED_BUILDINFO")"

touch -t 202001010000 "$bundle_root"
mkdir -p "$ops_bundle_root/bin"
for binary in oasis7_world_repair_rebuild oasis7_governance_registry_import oasis7_governance_registry_audit service-readback; do
  printf '#!/usr/bin/env bash\n' >"$ops_bundle_root/bin/$binary"
  chmod +x "$ops_bundle_root/bin/$binary"
done
printf '{"opsToolsSchemaVersion":1}\n' >"$ops_bundle_root/.oasis7-ops-tools-manifest.json"
(cd "$ops_bundle_root" && shasum -a 256 .oasis7-ops-tools-manifest.json bin/* >SHA256SUMS)
tar -czf "$ops_tools_tar" -C "$TMP_DIR/bundle" oasis7-linux-x64-ops-tools
package_root="$TMP_DIR/deb-root"
mkdir -p "$package_root/DEBIAN" "$package_root/opt/oasis7"
cp -a "$bundle_root/." "$package_root/opt/oasis7/"
cat >"$package_root/DEBIAN/control" <<'EOF'
Package: oasis7
Version: 0.0.0
Section: games
Priority: optional
Architecture: amd64
Description: package node upgrade contract fixture
EOF
dpkg-deb --build --root-owner-group "$package_root" "$package_deb" >/dev/null

# Build a package whose regular payload contains an extra file that is absent
# from the embedded SHA256SUMS.  The upgrader must reject it before creating a
# transaction snapshot or changing current.
unlisted_package_root="$TMP_DIR/deb-root-unlisted"
cp -a "$package_root" "$unlisted_package_root"
printf 'unlisted payload\n' >"$unlisted_package_root/opt/oasis7/bin/UNLISTED"
dpkg-deb --build --root-owner-group "$unlisted_package_root" "$unlisted_package_deb" >/dev/null

before_bad_current=$(readlink "$node_root/current")
set +e
"$ROOT_DIR/scripts/p2p-public-testnet-package-node-upgrade.sh" \
  --node-root "$node_root" \
  --package-deb "$unlisted_package_deb" \
  --ops-tools-tar "$ops_tools_tar" \
  --package-version "$package_version" \
  --commit "$commit" \
  --run-id "$run_id" \
  >"$TMP_DIR/unlisted-upgrade.stdout" 2>"$TMP_DIR/unlisted-upgrade.stderr"
unlisted_upgrade_status=$?
set -e
test "$unlisted_upgrade_status" -ne 0
grep -q "SHA256SUMS does not cover bundle files: bin/UNLISTED" "$TMP_DIR/unlisted-upgrade.stderr"
test "$(readlink "$node_root/current")" = "$before_bad_current"
test ! -e "$node_root/releases/$package_version"

# The checksum closure helper must not silently skip an unreadable subtree.
unreadable_bundle_root="$TMP_DIR/unreadable-bundle"
mkdir -p "$unreadable_bundle_root/bin/unreadable-subtree"
printf 'runtime\n' >"$unreadable_bundle_root/bin/runtime"
printf 'hidden\n' >"$unreadable_bundle_root/bin/unreadable-subtree/hidden"
(cd "$unreadable_bundle_root" && shasum -a 256 bin/runtime >SHA256SUMS)
chmod 000 "$unreadable_bundle_root/bin/unreadable-subtree"
set +e
unreadable_rebuild_output=$(python3 "$ROOT_DIR/scripts/p2p-rebuild-linux-bundle-checksums.py" "$unreadable_bundle_root" 2>&1)
unreadable_rebuild_status=$?
set -e
chmod 755 "$unreadable_bundle_root/bin/unreadable-subtree"
test "$unreadable_rebuild_status" -ne 0
grep -q "cannot read bundle subtree" <<<"$unreadable_rebuild_output"

# A traversal-like CLI value must be rejected before temporary/release/
# rollback paths are constructed or package extraction is attempted.
set +e
"$ROOT_DIR/scripts/p2p-public-testnet-package-node-upgrade.sh" \
  --node-root "$node_root" \
  --package-deb "$package_deb" \
  --ops-tools-tar "$ops_tools_tar" \
  --package-version "../../outside" \
  --commit "$commit" \
  --run-id "$run_id" \
  >"$TMP_DIR/unsafe-version.stdout" 2>"$TMP_DIR/unsafe-version.stderr"
unsafe_version_status=$?
set -e
test "$unsafe_version_status" -ne 0
grep -q "safe single path token" "$TMP_DIR/unsafe-version.stderr"
test "$(readlink "$node_root/current")" = "$before_bad_current"
test ! -e "$TMP_DIR/outside"

"$ROOT_DIR/scripts/p2p-public-testnet-package-node-upgrade.sh" \
  --node-root "$node_root" \
  --package-deb "$package_deb" \
  --ops-tools-tar "$ops_tools_tar" \
  --package-version "$package_version" \
  --commit "$commit" \
  --run-id "$run_id" \
  --artifact-ref "testnet-package-linux-x64-$package_version/oasis7-linux-x64.deb!/opt/oasis7/bin/oasis7_chain_runtime" \
  >/dev/null

node_root_abs=$(python3 -c 'import pathlib,sys; print(pathlib.Path(sys.argv[1]).resolve())' "$node_root")
expected_sha=$(shasum -a 256 "$node_root_abs/current/bin/oasis7_chain_runtime" | awk '{print $1}')
expected_size=$(wc -c <"$node_root_abs/current/bin/oasis7_chain_runtime" | tr -d ' ')
expected_artifact_ref="testnet-package-linux-x64-$package_version/oasis7-linux-x64.deb!/opt/oasis7/bin/oasis7_chain_runtime"

test "$(readlink "$node_root_abs/current")" = "$node_root_abs/releases/$package_version"
test -x "$node_root_abs/releases/$package_version/bin/oasis7_chain_runtime"
python3 "$ROOT_DIR/scripts/p2p-verify-linux-package-bundle.py" \
  "$node_root_abs/releases/$package_version" "$package_version" "$commit" "$run_id"
test -f "$node_root_abs/CURRENT_VERSION"
test -f "$node_root_abs/DEPLOYED_BUILDINFO"
grep -q "^package_version=$package_version$" "$node_root_abs/DEPLOYED_BUILDINFO"
grep -q "^commit=$commit$" "$node_root_abs/DEPLOYED_BUILDINFO"
test "$(file_metadata "$governed_bundle")" = "$governed_uid $governed_gid $governed_mode"
test "$(file_metadata "$node_root_abs/CURRENT_VERSION")" = \
  "$current_version_uid $current_version_gid $current_version_mode"
test "$(file_metadata "$node_root_abs/DEPLOYED_BUILDINFO")" = \
  "$buildinfo_uid $buildinfo_gid $buildinfo_mode"
normal_transaction_manifest=$(find "$node_root_abs/package-upgrade-rollback" \
  -name transaction.json -type f -print -quit)
test -n "$normal_transaction_manifest"
if ! jq -e \
  --arg bundle_path "config/doc/testing/evidence/public-testnet-governed-bootstrap-bundle-2026-06-06.json" \
  --argjson bundle_uid "$governed_uid" \
  --argjson bundle_gid "$governed_gid" \
  --argjson bundle_mode "$governed_mode" \
  --argjson current_version_uid "$current_version_uid" \
  --argjson current_version_gid "$current_version_gid" \
  --argjson current_version_mode "$current_version_mode" \
  --argjson buildinfo_uid "$buildinfo_uid" \
  --argjson buildinfo_gid "$buildinfo_gid" \
  --argjson buildinfo_mode "$buildinfo_mode" \
  'any(.files[]; .path == $bundle_path
      and .uid == $bundle_uid and .gid == $bundle_gid and .mode == $bundle_mode)
   and any(.files[]; .path == "CURRENT_VERSION"
      and .uid == $current_version_uid
      and .gid == $current_version_gid
      and .mode == $current_version_mode)
   and any(.files[]; .path == "DEPLOYED_BUILDINFO"
      and .uid == $buildinfo_uid
      and .gid == $buildinfo_gid
      and .mode == $buildinfo_mode)' \
  "$normal_transaction_manifest" >/dev/null; then
  echo "transaction snapshot did not preserve governed metadata uid/gid/mode" >&2
  jq -S . "$normal_transaction_manifest" >&2
  exit 1
fi
release_count=$(find "$node_root_abs/releases" -mindepth 1 -maxdepth 1 -type d ! -name '.*' | wc -l | tr -d ' ')
test "$release_count" = "5"
test -d "$node_root_abs/releases/$package_version"
test -d "$node_root_abs/releases/old"
test -d "$node_root_abs/releases/retention-candidate-3"
test -d "$node_root_abs/releases/retention-candidate-4"
test -d "$node_root_abs/releases/retention-candidate-5"
test ! -e "$node_root_abs/releases/retention-candidate-1"
test ! -e "$node_root_abs/releases/retention-candidate-2"
test -f "$node_root_abs/data/sentinel.txt"
test -f "$node_root_abs/config/sentinel.txt"
test -f "$node_root_abs/backups/sentinel.txt"

jq -e \
  --arg expected "$expected_sha" \
  --arg commit "$commit" \
  --arg package_version "$package_version" \
  --arg run_id "$run_id" \
  --arg runtime "$node_root_abs/current/bin/oasis7_chain_runtime" \
  --arg artifact_ref "$expected_artifact_ref" \
  --argjson size_bytes "$expected_size" \
  '.git_commit == $commit
    and .runtime_build.git_commit == $commit
    and .runtime_build.package_version == $package_version
    and .runtime_build.run_id == $run_id
    and .runtime_build.sha256 == $expected
    and .runtime_build.size_bytes == $size_bytes
    and .runtime_build.path == $runtime
    and .runtime_build.resolved_path == $runtime
    and .runtime_build.ref == $artifact_ref' \
  "$node_root_abs/config/doc/testing/evidence/public-testnet-governed-bootstrap-bundle-2026-06-06.json" >/dev/null

directory_current_node="$TMP_DIR/directory-current-node"
mkdir -p "$directory_current_node/current/bin" "$directory_current_node/config/doc/testing/evidence"
printf 'runtime-v1\n' >"$directory_current_node/current/bin/oasis7_chain_runtime"
chmod +x "$directory_current_node/current/bin/oasis7_chain_runtime"
cp \
  "$node_root_abs/config/doc/testing/evidence/public-testnet-governed-bootstrap-bundle-2026-06-06.json" \
  "$directory_current_node/config/doc/testing/evidence/public-testnet-governed-bootstrap-bundle-2026-06-06.json"

"$ROOT_DIR/scripts/p2p-public-testnet-package-node-upgrade.sh" \
  --node-root "$directory_current_node" \
  --package-deb "$package_deb" \
  --ops-tools-tar "$ops_tools_tar" \
  --package-version "$package_version" \
  --commit "$commit" \
  --run-id "$run_id" \
  --artifact-ref "testnet-package-linux-x64-$package_version/oasis7-linux-x64.deb!/opt/oasis7/bin/oasis7_chain_runtime" \
  >/dev/null

directory_current_node_abs=$(python3 -c 'import pathlib,sys; print(pathlib.Path(sys.argv[1]).resolve())' "$directory_current_node")
test -L "$directory_current_node_abs/current"
test "$(readlink "$directory_current_node_abs/current")" = "$directory_current_node_abs/releases/$package_version"
test -x "$directory_current_node_abs/current/bin/oasis7_chain_runtime"
test "$(find "$directory_current_node_abs" -maxdepth 1 -type d -name 'current-pre-*.dir' | wc -l | tr -d ' ')" = "1"
test ! -e "$directory_current_node_abs/current/oasis7-linux-x64"

missing_bundle_node="$TMP_DIR/missing-bundle-node"
mkdir -p "$missing_bundle_node/releases/old/bin" "$missing_bundle_node/config"
printf 'runtime-v1\n' >"$missing_bundle_node/releases/old/bin/oasis7_chain_runtime"
chmod +x "$missing_bundle_node/releases/old/bin/oasis7_chain_runtime"
ln -s "$missing_bundle_node/releases/old" "$missing_bundle_node/current"
missing_bundle_node_abs=$(python3 -c 'import pathlib,sys; print(pathlib.Path(sys.argv[1]).resolve())' "$missing_bundle_node")
set +e
"$ROOT_DIR/scripts/p2p-public-testnet-package-node-upgrade.sh" \
  --node-root "$missing_bundle_node" \
  --package-deb "$package_deb" \
  --ops-tools-tar "$ops_tools_tar" \
  --package-version "$package_version" \
  --commit "$commit" \
  --run-id "$run_id" \
  >"$TMP_DIR/oasis7-package-node-upgrade-negative.out" 2>&1
negative_status=$?
set -e
test "$negative_status" -ne 0
grep -q "no governed bootstrap bundle found" "$TMP_DIR/oasis7-package-node-upgrade-negative.out"
negative_current=$(python3 -c 'import pathlib,sys; print(pathlib.Path(sys.argv[1]).resolve())' "$missing_bundle_node_abs/current")
test "$negative_current" = "$missing_bundle_node_abs/releases/old"

orphan_node="$TMP_DIR/orphan-node"
mkdir -p "$orphan_node/releases/old/bin" "$orphan_node/config/doc/testing/evidence" "$TMP_DIR/fake-bin"
printf 'runtime-v1\n' >"$orphan_node/releases/old/bin/oasis7_chain_runtime"
chmod +x "$orphan_node/releases/old/bin/oasis7_chain_runtime"
ln -s "$orphan_node/releases/old" "$orphan_node/current"
cp \
  "$node_root_abs/config/doc/testing/evidence/public-testnet-governed-bootstrap-bundle-2026-06-06.json" \
  "$orphan_node/config/doc/testing/evidence/public-testnet-governed-bootstrap-bundle-2026-06-06.json"
orphan_node_abs=$(python3 -c 'import pathlib,sys; print(pathlib.Path(sys.argv[1]).resolve())' "$orphan_node")
cat >"$TMP_DIR/fake-bin/systemctl" <<'SH'
#!/usr/bin/env bash
printf 'systemctl %s\n' "$*" >>"$FAKE_SYSTEMCTL_LOG"
exit 0
SH
cat >"$TMP_DIR/fake-bin/ps" <<SH
#!/usr/bin/env bash
printf '999 1 ${orphan_node_abs}/current/bin/oasis7_chain_runtime --runtime-root ${orphan_node_abs}/data/runtime-root\\n'
SH
chmod +x "$TMP_DIR/fake-bin/systemctl" "$TMP_DIR/fake-bin/ps"
set +e
FAKE_SYSTEMCTL_LOG="$TMP_DIR/fake-systemctl.log" \
PATH="$TMP_DIR/fake-bin:$PATH" \
"$ROOT_DIR/scripts/p2p-public-testnet-package-node-upgrade.sh" \
  --node-root "$orphan_node" \
  --package-deb "$package_deb" \
  --ops-tools-tar "$ops_tools_tar" \
  --package-version "$package_version" \
  --commit "$commit" \
  --run-id "$run_id" \
  --systemd-service oasis7-testnet-orphan.service \
  --restart-service \
  >"$TMP_DIR/oasis7-package-node-upgrade-orphan.out" 2>&1
orphan_status=$?
set -e
test "$orphan_status" -ne 0
grep -q "node-root still has running oasis7 process after stop" "$TMP_DIR/oasis7-package-node-upgrade-orphan.out"
grep -q "systemctl stop oasis7-testnet-orphan.service" "$TMP_DIR/fake-systemctl.log"
orphan_current=$(python3 -c 'import pathlib,sys; print(pathlib.Path(sys.argv[1]).resolve())' "$orphan_node_abs/current")
test "$orphan_current" = "$orphan_node_abs/releases/old"

blocked_post_restart_node="$TMP_DIR/blocked-post-restart-node"
mkdir -p "$blocked_post_restart_node/releases/old/bin" "$blocked_post_restart_node/config/doc/testing/evidence"
printf 'runtime-v1\n' >"$blocked_post_restart_node/releases/old/bin/oasis7_chain_runtime"
chmod +x "$blocked_post_restart_node/releases/old/bin/oasis7_chain_runtime"
ln -s "$blocked_post_restart_node/releases/old" "$blocked_post_restart_node/current"
cp \
  "$node_root_abs/config/doc/testing/evidence/public-testnet-governed-bootstrap-bundle-2026-06-06.json" \
  "$blocked_post_restart_node/config/doc/testing/evidence/public-testnet-governed-bootstrap-bundle-2026-06-06.json"
blocked_post_restart_node_abs=$(python3 -c 'import pathlib,sys; print(pathlib.Path(sys.argv[1]).resolve())' "$blocked_post_restart_node")
cat >"$TMP_DIR/fake-bin/ps" <<SH
#!/usr/bin/env bash
printf '777 1 harmless-process\\n'
SH
cat >"$TMP_DIR/fake-bin/curl" <<'SH'
#!/usr/bin/env bash
cat <<'JSON'
{
  "running": true,
  "last_error": null,
  "readiness": {"status": "ready", "failed_gates": []},
  "consensus": {
    "committed_height": 0,
    "network_committed_height": 0,
    "last_block_hash": "genesis",
    "last_execution_height": 0,
    "last_execution_block_hash": null,
    "last_execution_state_root": null,
    "network_head": {"height": null, "block_hash": null},
    "storage_challenge_network_degraded_height": null
  },
  "world_resource": {
    "readiness_status": "not_ready",
    "failed_gates": [
      "world_resource_world_id_mismatch",
      "world_resource_chain_id_mismatch",
      "world_resource_delta_commit_hash_missing",
      "world_resource_delta_height_mismatch"
    ],
    "last_delta_commit_height": 1
  },
  "observability": {"storage_challenge_network_degraded": false}
}
JSON
SH
chmod +x "$TMP_DIR/fake-bin/curl"
set +e
FAKE_SYSTEMCTL_LOG="$TMP_DIR/fake-systemctl.log" \
PATH="$TMP_DIR/fake-bin:$PATH" \
"$ROOT_DIR/scripts/p2p-public-testnet-package-node-upgrade.sh" \
  --node-root "$blocked_post_restart_node" \
  --package-deb "$package_deb" \
  --ops-tools-tar "$ops_tools_tar" \
  --package-version "$package_version" \
  --commit "$commit" \
  --run-id "$run_id" \
  --systemd-service oasis7-testnet-blocked-post-restart.service \
  --restart-service \
  --post-restart-status-url http://127.0.0.1:6631/v1/chain/status \
  --post-restart-timeout-secs 1 \
  >"$TMP_DIR/oasis7-package-node-upgrade-blocked-post-restart.out" 2>&1
blocked_post_restart_status=$?
set -e
test "$blocked_post_restart_status" -ne 0
grep -q "post-restart status did not become ready before timeout" "$TMP_DIR/oasis7-package-node-upgrade-blocked-post-restart.out"
grep -q "world_resource_delta_commit_hash_missing" "$TMP_DIR/oasis7-package-node-upgrade-blocked-post-restart.out"
blocked_post_restart_current=$(python3 -c 'import pathlib,sys; print(pathlib.Path(sys.argv[1]).resolve())' "$blocked_post_restart_node_abs/current")
test "$blocked_post_restart_current" = "$blocked_post_restart_node_abs/releases/old"

rollback_node="$TMP_DIR/rollback-node"
rollback_bundle_a="$rollback_node/config/a/doc/testing/evidence/public-testnet-governed-bootstrap-bundle-2026-06-06.json"
rollback_bundle_b="$rollback_node/config/b/public-testnet-governed-bootstrap-bundle-2026-06-06.json"
mkdir -p "$rollback_node/releases/old/bin" "$(dirname "$rollback_bundle_a")" "$(dirname "$rollback_bundle_b")"
printf 'runtime-old-rollback\n' >"$rollback_node/releases/old/bin/oasis7_chain_runtime"
chmod +x "$rollback_node/releases/old/bin/oasis7_chain_runtime"
rollback_node_abs=$(python3 -c 'import pathlib,sys; print(pathlib.Path(sys.argv[1]).resolve())' "$rollback_node")
ln -s "$rollback_node_abs/releases/old" "$rollback_node/current"
printf 'old-version-rollback\n' >"$rollback_node/CURRENT_VERSION"
chmod 600 "$rollback_node/CURRENT_VERSION"
printf 'old-deployed-buildinfo-rollback\n' >"$rollback_node/DEPLOYED_BUILDINFO"
chmod 600 "$rollback_node/DEPLOYED_BUILDINFO"
rollback_runtime_sha=$(shasum -a 256 "$rollback_node/releases/old/bin/oasis7_chain_runtime" | awk '{print $1}')
rollback_runtime_size=$(wc -c <"$rollback_node/releases/old/bin/oasis7_chain_runtime" | tr -d ' ')
cat >"$rollback_bundle_a" <<EOF
{
  "marker": "rollback-a",
  "runtime_build": {
    "version": "old-version-rollback",
    "sha256": "$rollback_runtime_sha",
    "size_bytes": $rollback_runtime_size,
    "path": "releases/old/bin/oasis7_chain_runtime"
  }
}
EOF
cat >"$rollback_bundle_b" <<EOF
{
  "marker": "rollback-b",
  "runtime_build": {
    "version": "old-version-rollback",
    "sha256": "$rollback_runtime_sha",
    "size_bytes": $rollback_runtime_size,
    "path": "releases/old/bin/oasis7_chain_runtime"
  }
}
EOF
chmod 600 "$rollback_bundle_a" "$rollback_bundle_b"
cp "$rollback_bundle_a" "$TMP_DIR/rollback-bundle-a.before"
cp "$rollback_bundle_b" "$TMP_DIR/rollback-bundle-b.before"
cp "$rollback_node/CURRENT_VERSION" "$TMP_DIR/rollback-current-version.before"
cp "$rollback_node/DEPLOYED_BUILDINFO" "$TMP_DIR/rollback-deployed-buildinfo.before"
rollback_current_before=$(readlink "$rollback_node/current")
read -r rollback_a_uid rollback_a_gid rollback_a_mode \
  <<<"$(file_metadata "$rollback_bundle_a")"
read -r rollback_b_uid rollback_b_gid rollback_b_mode \
  <<<"$(file_metadata "$rollback_bundle_b")"
read -r rollback_version_uid rollback_version_gid rollback_version_mode \
  <<<"$(file_metadata "$rollback_node/CURRENT_VERSION")"
read -r rollback_buildinfo_uid rollback_buildinfo_gid rollback_buildinfo_mode \
  <<<"$(file_metadata "$rollback_node/DEPLOYED_BUILDINFO")"

for rollback_attempt in first second; do
  set +e
  FAKE_SYSTEMCTL_LOG="$TMP_DIR/rollback-systemctl-$rollback_attempt.log" \
  PATH="$TMP_DIR/fake-bin:$PATH" \
  "$ROOT_DIR/scripts/p2p-public-testnet-package-node-upgrade.sh" \
    --node-root "$rollback_node" \
    --package-deb "$package_deb" \
    --ops-tools-tar "$ops_tools_tar" \
    --package-version "$package_version" \
    --commit "$commit" \
    --run-id "$run_id" \
    --systemd-service oasis7-testnet-rollback.service \
    --restart-service \
    --post-restart-status-url http://127.0.0.1:6631/v1/chain/status \
    --post-restart-timeout-secs 1 \
    >"$TMP_DIR/rollback-$rollback_attempt.out" 2>&1
  rollback_status=$?
  set -e
  test "$rollback_status" -ne 0
  grep -q "post-restart status did not become ready before timeout" "$TMP_DIR/rollback-$rollback_attempt.out"
  test "$(readlink "$rollback_node/current")" = "$rollback_current_before"
  test -d "$rollback_node/releases/old"
  cmp -s "$TMP_DIR/rollback-bundle-a.before" "$rollback_bundle_a"
  cmp -s "$TMP_DIR/rollback-bundle-b.before" "$rollback_bundle_b"
  cmp -s "$TMP_DIR/rollback-current-version.before" "$rollback_node/CURRENT_VERSION"
  cmp -s "$TMP_DIR/rollback-deployed-buildinfo.before" "$rollback_node/DEPLOYED_BUILDINFO"
  test "$(file_metadata "$rollback_bundle_a")" = \
    "$rollback_a_uid $rollback_a_gid $rollback_a_mode"
  test "$(file_metadata "$rollback_bundle_b")" = \
    "$rollback_b_uid $rollback_b_gid $rollback_b_mode"
  test "$(file_metadata "$rollback_node/CURRENT_VERSION")" = \
    "$rollback_version_uid $rollback_version_gid $rollback_version_mode"
  test "$(file_metadata "$rollback_node/DEPLOYED_BUILDINFO")" = \
    "$rollback_buildinfo_uid $rollback_buildinfo_gid $rollback_buildinfo_mode"
  rollback_transaction_manifest=$(find "$rollback_node/package-upgrade-rollback" \
    -name transaction.json -type f -print -quit)
  test -n "$rollback_transaction_manifest"
  if ! jq -e \
    --arg bundle_a_path "config/a/doc/testing/evidence/public-testnet-governed-bootstrap-bundle-2026-06-06.json" \
    --arg bundle_b_path "config/b/public-testnet-governed-bootstrap-bundle-2026-06-06.json" \
    --argjson bundle_a_uid "$rollback_a_uid" \
    --argjson bundle_a_gid "$rollback_a_gid" \
    --argjson bundle_a_mode "$rollback_a_mode" \
    --argjson bundle_b_uid "$rollback_b_uid" \
    --argjson bundle_b_gid "$rollback_b_gid" \
    --argjson bundle_b_mode "$rollback_b_mode" \
    --argjson version_uid "$rollback_version_uid" \
    --argjson version_gid "$rollback_version_gid" \
    --argjson version_mode "$rollback_version_mode" \
    --argjson buildinfo_uid "$rollback_buildinfo_uid" \
    --argjson buildinfo_gid "$rollback_buildinfo_gid" \
    --argjson buildinfo_mode "$rollback_buildinfo_mode" \
    'any(.files[]; .path == $bundle_a_path
        and .uid == $bundle_a_uid and .gid == $bundle_a_gid and .mode == $bundle_a_mode)
     and any(.files[]; .path == $bundle_b_path
        and .uid == $bundle_b_uid and .gid == $bundle_b_gid and .mode == $bundle_b_mode)
     and any(.files[]; .path == "CURRENT_VERSION"
        and .uid == $version_uid and .gid == $version_gid and .mode == $version_mode)
     and any(.files[]; .path == "DEPLOYED_BUILDINFO"
        and .uid == $buildinfo_uid
        and .gid == $buildinfo_gid
        and .mode == $buildinfo_mode)' \
    "$rollback_transaction_manifest" >/dev/null; then
    echo "rollback transaction snapshot did not preserve governed metadata uid/gid/mode" >&2
    jq -S . "$rollback_transaction_manifest" >&2
    exit 1
  fi
  test "$(shasum -a 256 "$rollback_node/current/bin/oasis7_chain_runtime" | awk '{print $1}')" = "$rollback_runtime_sha"
  python3 - "$rollback_bundle_a" "$rollback_bundle_b" "$rollback_runtime_sha" <<'PY'
import json
import pathlib
import sys

expected_sha = sys.argv[3]
for bundle_path in sys.argv[1:3]:
    bundle = json.loads(pathlib.Path(bundle_path).read_text())
    assert bundle["runtime_build"]["sha256"] == expected_sha
PY
done

scanner_node="$TMP_DIR/scanner-node"
mkdir -p "$scanner_node/releases/old/bin" "$scanner_node/config/doc/testing/evidence"
printf 'runtime-v1\n' >"$scanner_node/releases/old/bin/oasis7_chain_runtime"
chmod +x "$scanner_node/releases/old/bin/oasis7_chain_runtime"
ln -s "$scanner_node/releases/old" "$scanner_node/current"
cp \
  "$node_root_abs/config/doc/testing/evidence/public-testnet-governed-bootstrap-bundle-2026-06-06.json" \
  "$scanner_node/config/doc/testing/evidence/public-testnet-governed-bootstrap-bundle-2026-06-06.json"
scanner_node_abs=$(python3 -c 'import pathlib,sys; print(pathlib.Path(sys.argv[1]).resolve())' "$scanner_node")
cat >"$TMP_DIR/fake-bin/ps" <<SH
#!/usr/bin/env bash
printf '888 1 awk -v root=${scanner_node_abs} index root /oasis7_chain_runtime/ /start-node[.]sh/\\n'
SH
set +e
FAKE_SYSTEMCTL_LOG="$TMP_DIR/fake-systemctl.log" \
PATH="$TMP_DIR/fake-bin:$PATH" \
"$ROOT_DIR/scripts/p2p-public-testnet-package-node-upgrade.sh" \
  --node-root "$scanner_node" \
  --package-deb "$package_deb" \
  --ops-tools-tar "$ops_tools_tar" \
  --package-version "$package_version" \
  --commit "$commit" \
  --run-id "$run_id" \
  --systemd-service oasis7-testnet-scanner.service \
  --restart-service \
  >"$TMP_DIR/oasis7-package-node-upgrade-scanner.out" 2>&1
scanner_status=$?
set -e
test "$scanner_status" -eq 0
scanner_current=$(python3 -c 'import pathlib,sys; print(pathlib.Path(sys.argv[1]).resolve())' "$scanner_node_abs/current")
test "$scanner_current" = "$scanner_node_abs/releases/$package_version"

echo "ok: package node upgrade pins current runtime hash into governed bootstrap bundle"
