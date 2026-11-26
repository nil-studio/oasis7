#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TMP_DIR="$(mktemp -d "${TMPDIR:-/tmp}/oasis7-rebuild-validators-test.XXXXXX")"
trap 'rm -rf "$TMP_DIR"' EXIT

help_output="$("$ROOT_DIR"/scripts/p2p-public-testnet-rebuild-validators.sh --help)"
if ! grep -Fq \
  'preflight both -> reset both -> stage both -> sequencer liveness -> storage' \
  <<<"$help_output"; then
  echo "expected --help to describe the safe validator rebuild order" >&2
  exit 1
fi
if ! grep -Fq -- '--consumer-impact-record <path>' <<<"$help_output"; then
  echo "expected --help to require a structured consumer-impact record" >&2
  exit 1
fi

mkdir -p "$TMP_DIR/bin" "$TMP_DIR/config/doc/testing/evidence" "$TMP_DIR/world" "$TMP_DIR/remote" "$TMP_DIR/status"

cat >"$TMP_DIR/consumer-impact-none.json" <<'JSON'
{
  "impact": "none",
  "evidence_source": "internal testnet inventory audit #2264",
  "timestamp": "2026-07-15T10:30:00+08:00",
  "validators_already_stopped": true,
  "outage_update_channel": "n/a",
  "recovery_update_checkpoint": "n/a",
  "producer_wording_approval": "n/a",
  "decision": "proceed"
}
JSON

cat >"$TMP_DIR/consumer-impact-active.json" <<'JSON'
{
  "impact": "active",
  "evidence_source": "public RPC traffic sample #2264",
  "timestamp": "2026-07-15T10:31:00+08:00",
  "validators_already_stopped": false,
  "outage_update_channel": "ops-status/testnet-outage-2264",
  "recovery_update_checkpoint": "Phase G fleet snapshot comment for #2264",
  "producer_wording_approval": "issue #2264 producer approval",
  "decision": "proceed"
}
JSON

cat >"$TMP_DIR/consumer-impact-unknown.json" <<'JSON'
{
  "impact": "unknown",
  "evidence_source": "consumer inventory unavailable",
  "timestamp": "2026-07-15T10:31:30+08:00",
  "validators_already_stopped": true,
  "outage_update_channel": "ops-status/testnet-outage-2264",
  "recovery_update_checkpoint": "Phase G fleet snapshot comment for #2264",
  "producer_wording_approval": "issue #2264 producer approval",
  "decision": "proceed"
}
JSON

cat >"$TMP_DIR/consumer-impact-missing-timezone.json" <<'JSON'
{
  "impact": "active",
  "evidence_source": "public RPC traffic sample #2264",
  "timestamp": "2026-07-15T10:31:00",
  "validators_already_stopped": false,
  "outage_update_channel": "ops-status/testnet-outage-2264",
  "recovery_update_checkpoint": "Phase G fleet snapshot comment for #2264",
  "producer_wording_approval": "issue #2264 producer approval",
  "decision": "proceed"
}
JSON

cat >"$TMP_DIR/consumer-impact-invalid-outage-update-channel.json" <<'JSON'
{
  "impact": "active",
  "evidence_source": "public RPC traffic sample #2264",
  "timestamp": "2026-07-15T10:31:00+08:00",
  "validators_already_stopped": false,
  "outage_update_channel": "n/a",
  "recovery_update_checkpoint": "Phase G fleet snapshot comment for #2264",
  "producer_wording_approval": "issue #2264 producer approval",
  "decision": "proceed"
}
JSON

cat >"$TMP_DIR/consumer-impact-invalid-recovery-update-checkpoint.json" <<'JSON'
{
  "impact": "unknown",
  "evidence_source": "consumer inventory unavailable",
  "timestamp": "2026-07-15T10:32:00Z",
  "validators_already_stopped": true,
  "outage_update_channel": "ops-status/testnet-outage-2264",
  "recovery_update_checkpoint": "n/a",
  "producer_wording_approval": "issue #2264 producer approval",
  "decision": "proceed"
}
JSON

cat >"$TMP_DIR/consumer-impact-invalid-producer-wording-approval.json" <<'JSON'
{
  "impact": "active",
  "evidence_source": "public RPC traffic sample #2264",
  "timestamp": "2026-07-15T10:33:00Z",
  "validators_already_stopped": false,
  "outage_update_channel": "ops-status/testnet-outage-2264",
  "recovery_update_checkpoint": "Phase G fleet snapshot comment for #2264",
  "producer_wording_approval": "n/a",
  "decision": "proceed"
}
JSON

cat >"$TMP_DIR/consumer-impact-impossible-date.json" <<'JSON'
{
  "impact": "none",
  "evidence_source": "internal testnet inventory audit #2264",
  "timestamp": "2026-02-31T10:30:00+08:00",
  "validators_already_stopped": true,
  "outage_update_channel": "n/a",
  "recovery_update_checkpoint": "n/a",
  "producer_wording_approval": "n/a",
  "decision": "proceed"
}
JSON

cat >"$TMP_DIR/consumer-impact-hold.json" <<'JSON'
{
  "impact": "unknown",
  "evidence_source": "consumer inventory unavailable",
  "timestamp": "2026-07-15T10:32:00Z",
  "validators_already_stopped": true,
  "outage_update_channel": "ops-status/testnet-outage-2264",
  "recovery_update_checkpoint": "Phase G fleet snapshot comment for #2264",
  "producer_wording_approval": "issue #2264 producer approval",
  "decision": "hold"
}
JSON

printf '{"impact":' >"$TMP_DIR/consumer-impact-malformed.json"

cat >"$TMP_DIR/config/public-testnet-governed-bootstrap-bundle-2026-06-06.json" <<'EOF'
{
  "ok": true,
  "runtime_build": {
    "sha256": "old-runtime-sha",
    "size_bytes": 1
  },
  "generated_world_sidecar": {
    "kind": "directory",
    "resolved_path": "/old/local/generated-scenario-world",
    "sha256_tree": "old-sidecar-sha"
  },
  "world_generation_provenance": {
    "kind": "file",
    "resolved_path": "/old/local/world-generation-provenance.json",
    "sha256": "old-provenance-sha",
    "size_bytes": 1
  }
}
EOF

cat >"$TMP_DIR/config/public-testnet-governed-bootstrap-manifest-2026-06-06.json" <<'EOF'
{
  "schema_version": "oasis7.network_tier_manifest.v1",
  "tier": "public_testnet",
  "network_id": "oasis7-public-testnet-governed-20260606",
  "chain_id": "oasis7-public-testnet-governed-20260606"
}
EOF

cat >"$TMP_DIR/config/public-testnet-governed-bootstrap-genesis-2026-06-06.json" <<'EOF'
{
  "governance_bootstrap_refs": {
    "governance_public_manifest_ref": "/tmp/public-testnet-governance-public-signers-2026-06-05.json"
  }
}
EOF

cat >"$TMP_DIR/config/doc/testing/evidence/public-testnet-governance-public-signers-2026-06-05.json" <<'EOF'
[
  {
    "slot_id": "governance.finality.v1",
    "node_id": "triad-testnet-sequencer",
    "scheme": "ed25519",
    "public_key_hex": "1111111111111111111111111111111111111111111111111111111111111111",
    "stake": 100,
    "threshold": 1
  },
  {
    "slot_id": "governance.controller.v1",
    "signer_id": "controller-a",
    "scheme": "ed25519",
    "public_key_hex": "2222222222222222222222222222222222222222222222222222222222222222",
    "threshold": 1
  }
]
EOF

cat >"$TMP_DIR/config/public-testnet-governed-bootstrap-validator-registry-2026-06-06.json" <<'EOF'
{
  "validators": [
    {
      "node_id": "triad-testnet-sequencer",
      "stake": 100,
      "finality_signer_public_key": "new-sequencer-signer"
    },
    {
      "node_id": "triad-testnet-storage",
      "stake": 100,
      "finality_signer_public_key": "new-storage-signer"
    }
  ]
}
EOF

cat >"$TMP_DIR/config/000-old-validator-registry.json" <<'EOF'
{
  "validators": [
    {
      "node_id": "triad-testnet-sequencer",
      "stake": 100,
      "finality_signer_public_key": "wrong-sorted-first-sequencer-signer"
    },
    {
      "node_id": "triad-testnet-storage",
      "stake": 100,
      "finality_signer_public_key": "wrong-sorted-first-storage-signer"
    }
  ]
}
EOF

cat >"$TMP_DIR/world/snapshot.json" <<'EOF'
{"world":true}
EOF

cat >"$TMP_DIR/world/journal.json" <<'EOF'
[]
EOF

mkdir -p "$TMP_DIR/world/generated-scenario-world"
cat >"$TMP_DIR/world/generated-scenario-world/manifest.json" <<'EOF'
{"sidecar":true}
EOF
cat >"$TMP_DIR/world/world-generation-provenance.json" <<'EOF'
{"provenance":true}
EOF
cat >"$TMP_DIR/world/.DS_Store" <<'EOF'
finder
EOF

cat >"$TMP_DIR/status/sequencer.json" <<'JSON'
{
  "running": true,
  "last_error": null,
  "readiness": {
    "status": "ready"
  },
  "observability": {
    "storage_challenge_network_degraded": false
  },
  "consensus": {
    "committed_height": 1,
    "last_execution_height": 1,
    "last_execution_block_hash": "sequencer-execution-block",
    "last_execution_state_root": "sequencer-execution-root",
    "storage_challenge_network_degraded_height": null,
    "network_head": {
      "height": 1
    }
  },
  "world_resource": {
    "readiness_status": "ready",
    "failed_gates": []
  },
  "replication": {
    "local_peer_id": "12D3KooWSequencer",
    "connected_peers": ["12D3KooWStorage"]
  }
}
JSON

cat >"$TMP_DIR/status/storage.json" <<'JSON'
{
  "running": true,
  "last_error": null,
  "readiness": {
    "status": "ready"
  },
  "observability": {
    "storage_challenge_network_degraded": false
  },
  "consensus": {
    "committed_height": 1,
    "last_execution_height": 1,
    "last_execution_block_hash": "storage-execution-block",
    "last_execution_state_root": "storage-execution-root",
    "storage_challenge_network_degraded_height": null,
    "network_head": {
      "height": 1
    }
  },
  "world_resource": {
    "readiness_status": "ready",
    "failed_gates": []
  },
  "replication": {
    "local_peer_id": "12D3KooWStorage",
    "connected_peers": ["12D3KooWSequencer"]
  }
}
JSON

cat >"$TMP_DIR/bin/sshpass" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
if [[ "$1" != "-e" ]]; then
  echo "expected -e" >&2
  exit 1
fi
shift
exec "$@"
EOF
chmod +x "$TMP_DIR/bin/sshpass"

cat >"$TMP_DIR/bin/systemctl" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
printf 'systemctl\t%s\n' "$*" >>"${TEST_SYSTEMCTL_LOG:?}"
effective_host="${TEST_SSH_HOST:-${TEST_SYSTEMD_RESTART_LOOP_HOST:-}}"
case "${1:-}" in
  list-unit-files|list-units)
    IFS=',' read -r -a services <<< "${TEST_SYSTEMD_STACK_OWNER_SERVICES:-}"
    for service in "${services[@]-}"; do
      [[ -n "$service" ]] && printf '%s enabled\n' "$service"
    done
    exit 0
    ;;
  show)
    service="${2:-}"
    IFS=',' read -r -a prefix_services <<< "${TEST_SYSTEMD_PREFIX_STACK_OWNER_SERVICES:-}"
    for owner in "${prefix_services[@]-}"; do
      if [[ "$service" == "$owner" ]]; then
        stack_root="${TEST_SYSTEMD_RESTART_LOOP_STACK_ROOT:-/opt/oasis7/p2p-testnet}-old"
        printf 'FragmentPath=/etc/systemd/system/%s\n' "$service"
        printf 'ExecStart={ path=%s/bin/start-node.sh ; argv[]=%s/bin/start-node.sh ; }\n' "$stack_root" "$stack_root"
        printf 'WorkingDirectory=%s\n' "$stack_root"
        exit 0
      fi
    done
    IFS=',' read -r -a services <<< "${TEST_SYSTEMD_STACK_OWNER_SERVICES:-}"
    for owner in "${services[@]-}"; do
      if [[ "$service" == "$owner" ]]; then
        stack_root="${TEST_SYSTEMD_RESTART_LOOP_STACK_ROOT:-/opt/oasis7/p2p-testnet}"
        printf 'FragmentPath=/etc/systemd/system/%s\n' "$service"
        printf 'ExecStart={ path=%s/bin/start-node.sh ; argv[]=%s/bin/start-node.sh ; }\n' "$stack_root" "$stack_root"
        printf 'WorkingDirectory=%s\n' "$stack_root"
        exit 0
      fi
    done
    printf 'FragmentPath=/etc/systemd/system/%s\n' "$service"
    printf 'ExecStart=\n'
    printf 'WorkingDirectory=\n'
    exit 0
    ;;
esac
if [[ "${TEST_FAIL_SYSTEMCTL_MASK_HOST:-}" == "$effective_host" && "${1:-}" == "mask" ]]; then
  echo "runtime mask denied" >&2
  exit 23
fi
if [[ "${TEST_SYSTEMD_RESTART_LOOP_HOST:-}" == "$effective_host" ]]; then
  service=${*: -1}
  state_dir="${TEST_REMOTE_ROOT:?}/systemd-${effective_host//[^A-Za-z0-9_.-]/_}-${service//[^A-Za-z0-9_.-]/_}"
  case "$1" in
    mask)
      mkdir -p "$state_dir"
      touch "$state_dir/masked"
      ;;
    unmask)
      mkdir -p "$state_dir"
      rm -f "$state_dir/masked"
      ;;
    start)
      mkdir -p "$state_dir"
      rm -f "$state_dir/killed"
      touch "$state_dir/armed"
      ;;
    stop|reset-failed)
      if [[ -f "$state_dir/armed" && ! -f "$state_dir/killed" && ! -f "$state_dir/masked" ]]; then
        stack_root="${TEST_SYSTEMD_RESTART_LOOP_STACK_ROOT:?}"
        mkdir -p "${TEST_REMOTE_ROOT:?}/${effective_host}${stack_root}/bin"
        printf 'spawned\t%s\t%s\n' "$effective_host" "$service" >>"${TEST_SYSTEMCTL_LOG:?}"
        (
          exec -a "${TEST_REMOTE_ROOT:?}/${effective_host}${stack_root}/bin/start-node.sh" sleep 30
        ) &
      fi
      ;;
    kill)
      mkdir -p "$state_dir"
      touch "$state_dir/killed"
      ;;
  esac
fi
exit 0
EOF
chmod +x "$TMP_DIR/bin/systemctl"

cat >"$TMP_DIR/bin/ssh" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
original_args=("$@")
master_mode=0
check_mode=0
control_path=""
while [[ $# -gt 0 && "$1" == -* ]]; do
  case "$1" in
    -o)
      shift 2
      ;;
    -S)
      control_path=$2
      shift 2
      ;;
    -O)
      [[ "$2" == "check" ]] && check_mode=1
      shift 2
      ;;
    -M)
      master_mode=1
      shift
      ;;
    -N|-f)
      shift
      ;;
    *)
      shift
      ;;
  esac
done
host=$1
shift
if [[ -n "${TEST_SSH_ARGS_LOG:-}" ]]; then
  printf '%s\t%s\t%s' "$host" "$master_mode" "$check_mode" >>"$TEST_SSH_ARGS_LOG"
  printf '\t%s' "${original_args[@]}" >>"$TEST_SSH_ARGS_LOG"
  printf '\n' >>"$TEST_SSH_ARGS_LOG"
fi
if [[ "$master_mode" -eq 1 && "${TEST_SSH_MASTER_FAIL_HOST:-}" == "$host" ]]; then
  exit 255
fi
if [[ "$master_mode" -eq 1 ]]; then
  python3 - "$control_path" <<'PY'
import socket
import sys

sock = socket.socket(socket.AF_UNIX)
sock.bind(sys.argv[1])
sock.close()
PY
  exit 0
fi
if [[ "$check_mode" -eq 1 && "${TEST_SSH_STALE_CHECK_HOST:-}" == "$host" ]]; then
  exit 255
fi
if [[ $# -eq 0 ]]; then
  exit 0
fi
cmd="$*"
root="${TEST_REMOTE_ROOT:?}/$host"
mkdir -p "$root"
if [[ -n "${TEST_SSH_LOG:-}" ]]; then
  logged_cmd=${cmd//$'\n'/\\n}
  printf '%s\t%s\n' "$host" "$logged_cmd" >>"$TEST_SSH_LOG"
fi
if [[ -n "${TEST_EVENT_LOG:-}" ]]; then
  logged_cmd=${cmd//$'\n'/\\n}
  printf 'ssh\t%s\t%s\n' "$host" "$logged_cmd" >>"$TEST_EVENT_LOG"
fi
case "$cmd" in
  mkdir\ -p*)
    stack_root=$(printf '%s\n' "$cmd" | sed -n "s/.*mkdir -p '\([^']*\)\/config\/doc\/testing\/evidence'.*/\1/p")
    mkdir -p "$root$stack_root/config/doc/testing/evidence" "$root$stack_root/staged-world" "$root$stack_root/data/execution-world"
    ;;
  *oasis7_world_repair_rebuild*--help*)
    if [[ "${TEST_FAIL_PREFLIGHT_HOST:-}" == "$host" ]]; then
      exit 44
    fi
    ;;
  cat\ \>*)
    target=$(printf '%s\n' "$cmd" | sed -n "s/cat > '\([^']*\)'/\1/p")
    mkdir -p "$(dirname "$root$target")"
    cat >"$root$target"
    ;;
  cp\ \'*)
    src=$(printf '%s\n' "$cmd" | sed -n "s/cp '\([^']*\)' '\([^']*\)'/\1/p")
    dest=$(printf '%s\n' "$cmd" | sed -n "s/cp '\([^']*\)' '\([^']*\)'/\2/p")
    mkdir -p "$(dirname "$root$dest")"
    cp "$root$src" "$root$dest"
    ;;
  rm\ -rf*)
    if [[ "$cmd" == *"/staged-world"* ]]; then
      stack_root=$(printf '%s\n' "$cmd" | sed -n "s/.*rm -rf '\([^']*\)\/staged-world'.*/\1/p")
      rm -rf "$root$stack_root/staged-world" "$root$stack_root/data/execution-world"
      mkdir -p "$root$stack_root/staged-world" "$root$stack_root/data/execution-world"
    else
      stack_root=$(printf '%s\n' "$cmd" | sed -n "s/.*rm -rf '\([^']*\)\/data\/execution-records'.*/\1/p")
      rm -rf "$root$stack_root/data/execution-records" \
        "$root$stack_root/data/execution-world" \
        "$root$stack_root/data/execution-world-simulator-mirror" \
        "$root$stack_root/data/storage" \
        "$root$stack_root/data/runtime-root" \
        "$root$stack_root/data/replication-root" \
        "$root$stack_root/output/chain-runtime" \
        "$root$stack_root/output/node-distfs"
      mkdir -p "$root$stack_root/data/execution-records" \
        "$root$stack_root/data/execution-world" \
        "$root$stack_root/data/storage" \
        "$root$stack_root/data/runtime-root" \
        "$root$stack_root/data/replication-root" \
        "$root$stack_root/output/chain-runtime" \
        "$root$stack_root/output/node-distfs"
    fi
    ;;
  tar\ -C*)
    stack_root=$(printf '%s\n' "$cmd" | sed -n "s/tar -C '\([^']*\)\/staged-world'.*/\1/p")
    tar -C "$root$stack_root/staged-world" -xf -
    ;;
  find\ *\ -delete)
    stack_root=$(printf '%s\n' "$cmd" | sed -n "s/find '\([^']*\)\/staged-world'.*/\1/p")
    find "$root$stack_root/staged-world" \( -name '._*' -o -name '.DS_Store' \) -delete
    ;;
  *oasis7_world_repair_rebuild*)
    stack_root=$(printf '%s\n' "$cmd" | sed -n "s/'\([^']*\)\/current\/bin\/oasis7_world_repair_rebuild'.*/\1/p")
    output_dir=$(printf '%s\n' "$cmd" | sed -n "s/.*--output-world-dir '\([^']*\)'.*/\1/p")
    mkdir -p "$root$output_dir"
    printf '{"state":{"agents":{"starter-agent-0":{}}}}\n' >"$root$output_dir/snapshot.json"
    printf '[]\n' >"$root$output_dir/journal.json"
    if [[ "${TEST_REPAIR_WORLD_TIME_HOST:-}" == "$host" ]]; then
      printf 'generated_world_dir=%s/staged-world\n' "$stack_root"
      printf 'output_world_dir=%s\n' "$output_dir"
      printf 'journal_events=0\n'
      printf 'tick_consensus_records=0\n'
      printf 'world_time=2\n'
      printf 'agent_count=1\n'
      printf 'location_count=7\n'
    else
      printf 'generated_world_dir=%s/staged-world\n' "$stack_root"
      printf 'output_world_dir=%s\n' "$output_dir"
      printf 'journal_events=0\n'
      printf 'tick_consensus_records=0\n'
      printf 'world_time=0\n'
      printf 'agent_count=1\n'
      printf 'location_count=7\n'
    fi
    printf 'repair\t%s\t%s\n' "$host" "$cmd" >>"$TEST_EVENT_LOG"
    test -n "$stack_root"
    ;;
  cp\ -R\ *generated-scenario-world*\ \&\&\ cp\ *world-generation-provenance*)
    src_sidecar=$(printf '%s\n' "$cmd" | sed -n "s/cp -R '\([^']*\)' '\([^']*\)' && cp '\([^']*\)' '\([^']*\)'/\1/p")
    dest_sidecar=$(printf '%s\n' "$cmd" | sed -n "s/cp -R '\([^']*\)' '\([^']*\)' && cp '\([^']*\)' '\([^']*\)'/\2/p")
    src_provenance=$(printf '%s\n' "$cmd" | sed -n "s/cp -R '\([^']*\)' '\([^']*\)' && cp '\([^']*\)' '\([^']*\)'/\3/p")
    dest_provenance=$(printf '%s\n' "$cmd" | sed -n "s/cp -R '\([^']*\)' '\([^']*\)' && cp '\([^']*\)' '\([^']*\)'/\4/p")
    rm -rf "$root$dest_sidecar"
    mkdir -p "$(dirname "$root$dest_sidecar")" "$(dirname "$root$dest_provenance")"
    cp -R "$root$src_sidecar" "$root$dest_sidecar"
    cp "$root$src_provenance" "$root$dest_provenance"
    ;;
  SERVICE_NAME=*STACK_ROOT=*python3*|systemctl\ stop*\;*\ STACK_ROOT=*python3*|STACK_ROOT=*python3*)
    if [[ "${TEST_FAIL_CLEANUP_AFTER_START_HOST:-}" == "$host" ]] \
      && [[ -f "${TEST_REMOTE_ROOT:?}/started-${host//[^A-Za-z0-9_.-]/_}" ]]; then
      exit 43
    fi
    stack_root=$(printf '%s\n' "$cmd" | sed -n "s/STACK_ROOT='\([^']*\)'.*/\1/p")
    if [[ "$cmd" == SERVICE_NAME=* ]]; then
      cleanup_cmd="$cmd"
    else
      cleanup_cmd="STACK_ROOT=${cmd#*STACK_ROOT=}"
    fi
    mapped_cmd=${cleanup_cmd//STACK_ROOT=\'$stack_root\'/STACK_ROOT=\'$root$stack_root\'}
    if [[ "${TEST_SPAWN_DELAYED_CLEANUP_PROCESS_HOST:-}" == "$host" ]]; then
      mkdir -p "$root$stack_root/bin"
      (
        sleep 0.4
        exec -a "$root$stack_root/bin/start-node.sh" sleep 30
      ) &
    fi
    if [[ "${TEST_SPAWN_LATE_CLEANUP_PROCESS_HOST:-}" == "$host" ]]; then
      mapped_cmd="CLEANUP_DEADLINE_SECONDS=1 CLEANUP_QUIET_SECONDS=2 $mapped_cmd"
      mkdir -p "$root$stack_root/bin"
      (
        sleep 0.8
        exec -a "$root$stack_root/bin/start-node.sh" sleep 30
      ) &
    fi
    TEST_SSH_HOST="$host" bash -c "$mapped_cmd"
    ;;
  systemctl\ stop*)
    ;;
  systemctl\ unmask*\;*\ systemctl\ reset-failed*\;*\ systemctl\ start*|systemctl\ reset-failed*\;*\ systemctl\ start*|systemctl\ start*)
    touch "${TEST_REMOTE_ROOT:?}/started-${host//[^A-Za-z0-9_.-]/_}"
    if [[ "${TEST_SYSTEMD_RESTART_LOOP_HOST:-}" == "$host" ]]; then
      unmask_service=$(printf '%s\n' "$cmd" | sed -n "s/.*systemctl unmask '\([^']*\)'.*/\1/p")
      if [[ -n "$unmask_service" ]]; then
        TEST_SSH_HOST="$host" systemctl unmask "$unmask_service"
      fi
      service=$(printf '%s\n' "$cmd" | sed -n "s/.*systemctl start '\([^']*\)'.*/\1/p")
      if [[ -n "$service" ]]; then
        TEST_SSH_HOST="$host" systemctl start "$service"
      fi
    fi
    if [[ "${TEST_FAIL_START_HOST:-}" == "$host" ]]; then
      exit 42
    fi
    ;;
  *)
    echo "unhandled ssh command: $cmd" >&2
    exit 1
    ;;
esac
EOF
chmod +x "$TMP_DIR/bin/ssh"

cat >"$TMP_DIR/bin/curl" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
out=""
url=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    -o)
      out=$2
      shift 2
      ;;
    -f|-s|-S|-L|-fsSL|-fsS|-fs)
      shift
      ;;
    *)
      url=$1
      shift
      ;;
  esac
done
if [[ -n "${TEST_EVENT_LOG:-}" ]]; then
  printf 'curl\t%s\n' "$url" >>"$TEST_EVENT_LOG"
fi
case "$url" in
  http://sequencer/status)
    if [[ -n "${TEST_SEQUENCER_STATUS_OVERRIDE:-}" ]]; then
      cp "$TEST_SEQUENCER_STATUS_OVERRIDE" "$out"
    else
      cp "${TEST_STATUS_ROOT:?}/sequencer.json" "$out"
    fi
    ;;
  http://storage/status)
    if [[ -n "${TEST_STORAGE_STATUS_OVERRIDE:-}" ]]; then
      cp "$TEST_STORAGE_STATUS_OVERRIDE" "$out"
    else
      cp "${TEST_STATUS_ROOT:?}/storage.json" "$out"
    fi
    ;;
  *)
    echo "unexpected url: $url" >&2
    exit 1
    ;;
esac
EOF
chmod +x "$TMP_DIR/bin/curl"

export PATH="$TMP_DIR/bin:$PATH"
export TEST_REMOTE_ROOT="$TMP_DIR/remote"
export TEST_STATUS_ROOT="$TMP_DIR/status"
export TEST_SSH_LOG="$TMP_DIR/ssh.log"
export TEST_SSH_ARGS_LOG="$TMP_DIR/ssh-args.log"
export TEST_EVENT_LOG="$TMP_DIR/events.log"
export TEST_SYSTEMCTL_LOG="$TMP_DIR/systemctl.log"
export SEQ_PASS="sequencer-pass"
export STO_PASS="storage-pass"

assert_consumer_impact_rejected_without_host_access() {
  local label=$1
  local expected_message=$2
  shift 2
  : >"$TEST_SSH_LOG"
  : >"$TEST_SSH_ARGS_LOG"
  : >"$TEST_EVENT_LOG"
  if "$ROOT_DIR/scripts/p2p-public-testnet-rebuild-validators.sh" \
    --config-dir "$TMP_DIR/config" \
    --world-dir "$TMP_DIR/world" \
    "$@" \
    --sequencer-ssh-host root@sequencer \
    --sequencer-sshpass-env SEQ_PASS \
    --sequencer-service oasis7-triad-sequencer.service \
    --sequencer-status-url http://sequencer/status \
    --storage-ssh-host root@storage \
    --storage-sshpass-env STO_PASS \
    --storage-service oasis7-triad-storage.service \
    --storage-status-url http://storage/status \
    --stack-root /opt/oasis7/p2p-testnet \
    --out-dir "$TMP_DIR/out-consumer-impact-$label" \
    --poll-attempts 1 \
    --poll-sleep-seconds 0 \
    >"$TMP_DIR/consumer-impact-$label.stdout" \
    2>"$TMP_DIR/consumer-impact-$label.stderr"; then
    echo "expected consumer-impact case to fail: $label" >&2
    exit 1
  fi
  grep -Fq -- "$expected_message" "$TMP_DIR/consumer-impact-$label.stderr"
  test ! -s "$TEST_SSH_LOG"
  test ! -s "$TEST_SSH_ARGS_LOG"
  test ! -s "$TEST_EVENT_LOG"
}

assert_consumer_impact_rejected_without_host_access \
  missing \
  "--consumer-impact-record is required"
assert_consumer_impact_rejected_without_host_access \
  malformed \
  "consumer-impact record must contain valid JSON" \
  --consumer-impact-record "$TMP_DIR/consumer-impact-malformed.json"
assert_consumer_impact_rejected_without_host_access \
  missing-timezone \
  "consumer-impact record is invalid" \
  --consumer-impact-record "$TMP_DIR/consumer-impact-missing-timezone.json"
assert_consumer_impact_rejected_without_host_access \
  invalid-outage-update-channel \
  "consumer-impact record is invalid" \
  --consumer-impact-record "$TMP_DIR/consumer-impact-invalid-outage-update-channel.json"
assert_consumer_impact_rejected_without_host_access \
  invalid-recovery-update-checkpoint \
  "consumer-impact record is invalid" \
  --consumer-impact-record "$TMP_DIR/consumer-impact-invalid-recovery-update-checkpoint.json"
assert_consumer_impact_rejected_without_host_access \
  invalid-producer-wording-approval \
  "consumer-impact record is invalid" \
  --consumer-impact-record "$TMP_DIR/consumer-impact-invalid-producer-wording-approval.json"
assert_consumer_impact_rejected_without_host_access \
  impossible-date \
  "consumer-impact record is invalid" \
  --consumer-impact-record "$TMP_DIR/consumer-impact-impossible-date.json"
assert_consumer_impact_rejected_without_host_access \
  hold \
  "consumer-impact decision must be proceed" \
  --consumer-impact-record "$TMP_DIR/consumer-impact-hold.json"

for host in root@sequencer root@storage; do
  host_root="$TMP_DIR/remote/$host/opt/oasis7/p2p-testnet"
  mkdir -p \
    "$host_root/config" \
    "$host_root/current/bin" \
    "$host_root/data/execution-world-simulator-mirror"
  printf '{"world_time":13235}\n' \
    >"$host_root/data/execution-world-simulator-mirror/snapshot.json"
  cat >"$host_root/config/node.env" <<'EOF'
NODE_ID=triad-testnet-sequencer
NODE_VALIDATORS_CSV=triad-testnet-sequencer:100,triad-testnet-storage:100
NODE_VALIDATOR_SIGNERS_CSV=triad-testnet-sequencer:old-sequencer-signer,triad-testnet-storage:old-storage-signer
POS_ADAPTIVE_TICK_SCHEDULER=0
GENESIS_VALIDATOR_REGISTRY_PATH=${STACK_ROOT}/config/public-testnet-governed-bootstrap-validator-registry-2026-06-06.json
EOF
  printf 'runtime-v2' >"$host_root/current/bin/oasis7_chain_runtime"
  cat >"$host_root/current/bin/oasis7_governance_registry_import" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
world_dir=""
public_manifest=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --world-dir)
      world_dir=$2
      shift 2
      ;;
    --public-manifest)
      public_manifest=$2
      shift 2
      ;;
    *)
      shift
      ;;
  esac
done
test -n "$world_dir"
test -f "$public_manifest"
python3 - "$world_dir" <<'PY'
from __future__ import annotations

import json
import sys
from pathlib import Path

snapshot_path = Path(sys.argv[1]) / "snapshot.json"
data = json.loads(snapshot_path.read_text(encoding="utf-8"))
data.setdefault("state", {})["governance_finality_registry"] = {
    "slot_id": "governance.finality.v1",
    "signer_bindings": {"governance.finality.v1.triad-testnet-sequencer": "new-sequencer-signer"},
}
snapshot_path.write_text(json.dumps(data, sort_keys=True) + "\n", encoding="utf-8")
PY
EOF
  chmod +x "$host_root/current/bin/oasis7_governance_registry_import"
  cat >"$host_root/DEPLOYED_BUILDINFO" <<'EOF'
commit=test-commit
package_version=0.0.0+testnet.test
run_id=test-run
EOF
done

json=$(TEST_SSH_MASTER_FAIL_HOST=root@sequencer \
  TEST_SSH_STALE_CHECK_HOST=root@storage \
  "$ROOT_DIR/scripts/p2p-public-testnet-rebuild-validators.sh" \
  --config-dir "$TMP_DIR/config" \
  --world-dir "$TMP_DIR/world" \
  --consumer-impact-record "$TMP_DIR/consumer-impact-unknown.json" \
  --sequencer-ssh-host root@sequencer \
  --sequencer-sshpass-env SEQ_PASS \
  --sequencer-service oasis7-triad-sequencer.service \
  --sequencer-status-url http://sequencer/status \
  --storage-ssh-host root@storage \
  --storage-sshpass-env STO_PASS \
  --storage-service oasis7-triad-storage.service \
  --storage-status-url http://storage/status \
  --stack-root /opt/oasis7/p2p-testnet \
  --out-dir "$TMP_DIR/out" \
  --poll-attempts 1 \
  --poll-sleep-seconds 0)

python3 - "$TEST_EVENT_LOG" <<'PY'
import pathlib
import sys

lines = pathlib.Path(sys.argv[1]).read_text(encoding="utf-8").splitlines()

def first_index(needle: str) -> int:
    for index, line in enumerate(lines):
        if needle in line:
            return index
    raise SystemExit(f"missing event: {needle}")

first_curl_index = first_index("curl\t")
sequencer_start_index = first_index("systemctl start 'oasis7-triad-sequencer.service'")
storage_start_index = first_index("systemctl start 'oasis7-triad-storage.service'")
if not sequencer_start_index < first_curl_index:
    raise SystemExit("sequencer was not started before first status poll")
if not first_curl_index < storage_start_index:
    raise SystemExit("storage was started before sequencer liveness was confirmed")
PY

python3 - "$TEST_SSH_ARGS_LOG" <<'PY'
import pathlib
import sys

rows = [line.split("\t") for line in pathlib.Path(sys.argv[1]).read_text(encoding="utf-8").splitlines()]

def matching(host: str, master_mode: str, check_mode: str) -> list[list[str]]:
    return [row for row in rows if row[:3] == [host, master_mode, check_mode]]

for host in ("root@sequencer", "root@storage"):
    if not matching(host, "1", "0"):
        raise SystemExit(f"missing ControlMaster establishment attempt for {host}")
    preflight_rows = [
        row
        for row in matching(host, "0", "0")
        if any("oasis7_world_repair_rebuild' --help" in arg for arg in row[3:])
    ]
    if not preflight_rows:
        raise SystemExit(f"missing fallback preflight SSH command for {host}")
    args = preflight_rows[0][3:]
    required = {
        "ControlMaster=no",
        "ControlPath=none",
        "PreferredAuthentications=password",
        "PubkeyAuthentication=no",
    }
    missing = sorted(required - set(args))
    if missing:
        raise SystemExit(f"fallback SSH command for {host} is missing options: {missing}")

storage_checks = matching("root@storage", "0", "1")
if not storage_checks:
    raise SystemExit("missing stale ControlMaster check for storage")
if not any("-S" in row[3:] for row in storage_checks):
    raise SystemExit("stale ControlMaster check did not use the established socket")
PY

python3 - "$TEST_SSH_LOG" <<'PY'
import pathlib
import sys

lines = pathlib.Path(sys.argv[1]).read_text(encoding="utf-8").splitlines()
for host in ("root@sequencer", "root@storage"):
    commands = [line.split("\t", 1)[1] for line in lines if line.startswith(host + "\t")]
    help_indexes = [
        index
        for index, command in enumerate(commands)
        if "oasis7_world_repair_rebuild' --help" in command
    ]
    reset_indexes = [
        index
        for index, command in enumerate(commands)
        if "rm -rf '/opt/oasis7/p2p-testnet/staged-world' '/opt/oasis7/p2p-testnet/data/execution-world'" in command
    ]
    repair_indexes = [
        index
        for index, command in enumerate(commands)
        if "oasis7_world_repair_rebuild' --generated-world-dir" in command
    ]
    cleanup_indexes = [
        index
        for index, command in enumerate(commands)
        if command.startswith("SERVICE_NAME=")
    ]
    if not cleanup_indexes:
        raise SystemExit(f"missing pre-stage process cleanup for {host}")
    if not help_indexes:
        raise SystemExit(f"missing remote repair helper preflight for {host}")
    if not reset_indexes:
        raise SystemExit(f"missing destructive world reset for {host}")
    if not repair_indexes:
        raise SystemExit(f"missing remote repair rebuild for {host}")
    if not help_indexes[0] < cleanup_indexes[0] < reset_indexes[0] < repair_indexes[0]:
        raise SystemExit(
            f"expected repair preflight before process cleanup and destructive world reset for {host}"
        )
PY

python3 - "$TEST_EVENT_LOG" <<'PY'
import pathlib
import sys

lines = pathlib.Path(sys.argv[1]).read_text(encoding="utf-8").splitlines()

def first_index(host: str, needle: str) -> int:
    for index, line in enumerate(lines):
        if line.startswith(f"ssh\t{host}\t") and needle in line:
            return index
    raise SystemExit(f"missing event for {host}: {needle}")

cleanup_indexes = [
    first_index(host, "SERVICE_NAME=")
    for host in ("root@sequencer", "root@storage")
]
reset_indexes = [
    first_index(host, "data/execution-records")
    for host in ("root@sequencer", "root@storage")
]
stage_indexes = [
    first_index(host, "mkdir -p '/opt/oasis7/p2p-testnet/config/doc/testing/evidence'")
    for host in ("root@sequencer", "root@storage")
]
if not max(cleanup_indexes + reset_indexes) < min(stage_indexes):
    raise SystemExit("staging began before both hosts were quiesced and destructively reset")
PY

jq -e '
  .sequencer.running == true
  and .sequencer.committed_height == 1
  and .storage.running == true
  and .storage.last_execution_height == 1
' <<<"$json" >/dev/null

test -f "$TMP_DIR/out/rebuild-summary.json"
test -f "$TMP_DIR/out/sequencer-repair-rebuild.log"
test -f "$TMP_DIR/out/storage-repair-rebuild.log"
jq -e '
  .sequencer_repair_rebuild_log
  | endswith("sequencer-repair-rebuild.log")
' "$TMP_DIR/out/rebuild-summary.json" >/dev/null
jq -e '
  .storage_repair_rebuild_log
  | endswith("storage-repair-rebuild.log")
' "$TMP_DIR/out/rebuild-summary.json" >/dev/null
grep -Fx 'world_time=0' "$TMP_DIR/out/sequencer-repair-rebuild.log" >/dev/null
grep -Fx 'journal_events=0' "$TMP_DIR/out/sequencer-repair-rebuild.log" >/dev/null
grep -Fx 'tick_consensus_records=0' "$TMP_DIR/out/sequencer-repair-rebuild.log" >/dev/null
grep -Fx 'world_time=0' "$TMP_DIR/out/storage-repair-rebuild.log" >/dev/null
test -f "$TMP_DIR/remote/root@sequencer/opt/oasis7/p2p-testnet/config/public-testnet-governed-bootstrap-bundle-2026-06-06.json"
test -f "$TMP_DIR/remote/root@storage/opt/oasis7/p2p-testnet/data/execution-world/snapshot.json"
test -f "$TMP_DIR/remote/root@sequencer/opt/oasis7/p2p-testnet/config/doc/testing/evidence/public-testnet-repair-rebuild-sequencer.log"
test -f "$TMP_DIR/remote/root@storage/opt/oasis7/p2p-testnet/config/doc/testing/evidence/public-testnet-repair-rebuild-storage.log"
grep -Fx 'world_time=0' \
  "$TMP_DIR/remote/root@sequencer/opt/oasis7/p2p-testnet/config/doc/testing/evidence/public-testnet-repair-rebuild-sequencer.log" >/dev/null
jq -e '.state.governance_finality_registry.slot_id == "governance.finality.v1"' \
  "$TMP_DIR/remote/root@sequencer/opt/oasis7/p2p-testnet/data/execution-world/snapshot.json" >/dev/null
jq -e '.state.governance_finality_registry.slot_id == "governance.finality.v1"' \
  "$TMP_DIR/remote/root@storage/opt/oasis7/p2p-testnet/data/execution-world/snapshot.json" >/dev/null
test ! -e "$TMP_DIR/remote/root@storage/opt/oasis7/p2p-testnet/data/execution-world/.DS_Store"
grep -q '^NODE_VALIDATOR_SIGNERS_CSV=triad-testnet-sequencer:new-sequencer-signer,triad-testnet-storage:new-storage-signer$' \
  "$TMP_DIR/remote/root@sequencer/opt/oasis7/p2p-testnet/config/node.env"
grep -q '^NODE_VALIDATOR_SIGNERS_CSV=triad-testnet-sequencer:new-sequencer-signer,triad-testnet-storage:new-storage-signer$' \
  "$TMP_DIR/remote/root@storage/opt/oasis7/p2p-testnet/config/node.env"
grep -q '^POS_ADAPTIVE_TICK_SCHEDULER=1$' \
  "$TMP_DIR/remote/root@sequencer/opt/oasis7/p2p-testnet/config/node.env"
grep -q '^POS_ADAPTIVE_TICK_SCHEDULER=1$' \
  "$TMP_DIR/remote/root@storage/opt/oasis7/p2p-testnet/config/node.env"
grep -q -- "--world-id 'oasis7-public-testnet-governed-20260606' --chain-id 'oasis7-public-testnet-governed-20260606' --resource-commit-height 0 --resource-commit-hash genesis" \
  "$TEST_EVENT_LOG"
expected_runtime_sha=$(printf 'runtime-v2' | shasum -a 256 | awk '{print $1}')
expected_provenance_sha=$(printf '{"provenance":true}\n' | shasum -a 256 | awk '{print $1}')
for host in root@sequencer root@storage; do
  expected_sidecar=$(python3 -c 'import pathlib,sys; print(pathlib.Path(sys.argv[1]).absolute())' \
    "$TMP_DIR/remote/$host/opt/oasis7/p2p-testnet/data/execution-world/generated-scenario-world")
  expected_provenance=$(python3 -c 'import pathlib,sys; print(pathlib.Path(sys.argv[1]).absolute())' \
    "$TMP_DIR/remote/$host/opt/oasis7/p2p-testnet/data/execution-world/world-generation-provenance.json")
  for bundle in \
    "$TMP_DIR/remote/$host/opt/oasis7/p2p-testnet/config/public-testnet-governed-bootstrap-bundle-2026-06-06.json" \
    "$TMP_DIR/remote/$host/opt/oasis7/p2p-testnet/config/doc/testing/evidence/public-testnet-governed-bootstrap-bundle-2026-06-06.json"; do
    jq -e \
      --arg expected "$expected_runtime_sha" \
      --arg sidecar "$expected_sidecar" \
      --arg provenance "$expected_provenance" \
      --arg provenance_sha "$expected_provenance_sha" '
      .runtime_build.sha256 == $expected
      and .runtime_build.size_bytes == 10
      and .runtime_build.git_commit == "test-commit"
      and .runtime_build.package_version == "0.0.0+testnet.test"
      and .runtime_build.run_id == "test-run"
      and .generated_world_sidecar.resolved_path == $sidecar
      and .generated_world_sidecar.path == $sidecar
      and .world_generation_provenance.resolved_path == $provenance
      and .world_generation_provenance.path == $provenance
      and .world_generation_provenance.sha256 == $provenance_sha
    ' "$bundle" >/dev/null
  done
done
test -d "$TMP_DIR/remote/root@sequencer/opt/oasis7/p2p-testnet/data/runtime-root"
test -d "$TMP_DIR/remote/root@sequencer/opt/oasis7/p2p-testnet/data/replication-root"
test -d "$TMP_DIR/remote/root@storage/opt/oasis7/p2p-testnet/data/runtime-root"
test -d "$TMP_DIR/remote/root@storage/opt/oasis7/p2p-testnet/data/replication-root"
test ! -e "$TMP_DIR/remote/root@sequencer/opt/oasis7/p2p-testnet/data/execution-world-simulator-mirror"
test ! -e "$TMP_DIR/remote/root@storage/opt/oasis7/p2p-testnet/data/execution-world-simulator-mirror"

: >"$TEST_SSH_LOG"
: >"$TEST_EVENT_LOG"
: >"$TEST_SSH_ARGS_LOG"
if TEST_FAIL_PREFLIGHT_HOST=root@storage "$ROOT_DIR/scripts/p2p-public-testnet-rebuild-validators.sh" \
  --config-dir "$TMP_DIR/config" \
  --world-dir "$TMP_DIR/world" \
  --consumer-impact-record "$TMP_DIR/consumer-impact-active.json" \
  --sequencer-ssh-host root@sequencer \
  --sequencer-sshpass-env SEQ_PASS \
  --sequencer-service oasis7-triad-sequencer.service \
  --sequencer-status-url http://sequencer/status \
  --storage-ssh-host root@storage \
  --storage-sshpass-env STO_PASS \
  --storage-service oasis7-triad-storage.service \
  --storage-status-url http://storage/status \
  --stack-root /opt/oasis7/p2p-testnet \
  --out-dir "$TMP_DIR/out-preflight-fail" \
  --poll-attempts 1 \
  --poll-sleep-seconds 0 >"$TMP_DIR/preflight-fail.stdout" 2>"$TMP_DIR/preflight-fail.stderr"; then
  echo "expected rebuild to fail when storage preflight fails" >&2
  exit 1
fi

python3 - "$TEST_SSH_LOG" <<'PY'
import pathlib
import sys

lines = pathlib.Path(sys.argv[1]).read_text(encoding="utf-8").splitlines()
for host in ("root@sequencer", "root@storage"):
    host_preflights = [
        line
        for line in lines
        if line.startswith(f"{host}\t") and "oasis7_world_repair_rebuild' --help" in line
    ]
    if not host_preflights:
        raise SystemExit(f"missing preflight attempt for {host}")
    for required in ("oasis7_governance_registry_import", "command -v python3", "command -v tar", "command -v systemctl", "command -v ps"):
        if required not in host_preflights[0]:
            raise SystemExit(f"preflight for {host} is missing required dependency: {required}")
for forbidden in ("SERVICE_NAME=", "data/execution-records", "mkdir -p"):
    if any(forbidden in line for line in lines):
        raise SystemExit(f"preflight failure allowed mutating command containing: {forbidden}")
PY

cat >"$TMP_DIR/status/sequencer-not-live.json" <<'JSON'
{
  "running": false,
  "last_error": "startup failed"
}
JSON

: >"$TEST_SSH_LOG"
: >"$TEST_EVENT_LOG"
rm -f "$TEST_REMOTE_ROOT"/started-*
if TEST_SEQUENCER_STATUS_OVERRIDE="$TMP_DIR/status/sequencer-not-live.json" \
  "$ROOT_DIR/scripts/p2p-public-testnet-rebuild-validators.sh" \
  --config-dir "$TMP_DIR/config" \
  --world-dir "$TMP_DIR/world" \
  --consumer-impact-record "$TMP_DIR/consumer-impact-none.json" \
  --sequencer-ssh-host root@sequencer \
  --sequencer-sshpass-env SEQ_PASS \
  --sequencer-service oasis7-triad-sequencer.service \
  --sequencer-status-url http://sequencer/status \
  --storage-ssh-host root@storage \
  --storage-sshpass-env STO_PASS \
  --storage-service oasis7-triad-storage.service \
  --storage-status-url http://storage/status \
  --stack-root /opt/oasis7/p2p-testnet \
  --out-dir "$TMP_DIR/out-sequencer-liveness-fail" \
  --poll-attempts 1 \
  --poll-sleep-seconds 0 >"$TMP_DIR/sequencer-liveness-fail.stdout" 2>"$TMP_DIR/sequencer-liveness-fail.stderr"; then
  echo "expected rebuild to fail when sequencer liveness check fails" >&2
  exit 1
fi
grep -q "sequencer liveness failed checks after restart" "$TMP_DIR/sequencer-liveness-fail.stderr"
grep -q "systemctl start 'oasis7-triad-sequencer.service'" "$TEST_EVENT_LOG"
if grep -q "systemctl start 'oasis7-triad-storage.service'" "$TEST_EVENT_LOG"; then
  echo "storage started despite failed sequencer liveness" >&2
  exit 1
fi

start_count_before=$(grep -c "systemctl start '" "$TEST_EVENT_LOG" || true)
if TEST_REPAIR_WORLD_TIME_HOST=root@sequencer "$ROOT_DIR/scripts/p2p-public-testnet-rebuild-validators.sh" \
  --config-dir "$TMP_DIR/config" \
  --world-dir "$TMP_DIR/world" \
  --consumer-impact-record "$TMP_DIR/consumer-impact-none.json" \
  --sequencer-ssh-host root@sequencer \
  --sequencer-sshpass-env SEQ_PASS \
  --sequencer-service oasis7-triad-sequencer.service \
  --sequencer-status-url http://sequencer/status \
  --storage-ssh-host root@storage \
  --storage-sshpass-env STO_PASS \
  --storage-service oasis7-triad-storage.service \
  --storage-status-url http://storage/status \
  --stack-root /opt/oasis7/p2p-testnet \
  --out-dir "$TMP_DIR/out-repair-time-drift" \
  --poll-attempts 1 \
  --poll-sleep-seconds 0 \
  >"$TMP_DIR/out-repair-time-drift.stdout" 2>"$TMP_DIR/out-repair-time-drift.stderr"; then
  echo "expected rebuild to fail when repair rebuild reports world_time=2" >&2
  exit 1
fi
grep -Fx 'world_time=2' "$TMP_DIR/out-repair-time-drift/sequencer-repair-rebuild.log" >/dev/null
grep -q 'sequencer repair rebuild produced world_time=2, expected 0' \
  "$TMP_DIR/out-repair-time-drift.stderr"
start_count_after=$(grep -c "systemctl start '" "$TEST_EVENT_LOG" || true)
if [[ "$start_count_before" != "$start_count_after" ]]; then
  echo "repair time drift failure must stop before starting services" >&2
  exit 1
fi

cat >"$TMP_DIR/status/sequencer.json" <<'JSON'
{
  "running": true,
  "last_error": null,
  "readiness": {
    "status": "ready",
    "failed_gates": []
  },
  "observability": {
    "storage_challenge_network_degraded": false
  },
  "consensus": {
    "committed_height": null,
    "last_execution_height": null,
    "storage_challenge_network_degraded_height": null,
    "network_head": {
      "source": "self_only",
      "height": null,
      "required_peer_count": 0,
      "decision": "ready"
    }
  },
  "replication": {
    "local_peer_id": "12D3KooWSequencer",
    "connected_peers": ["12D3KooWStorage"]
  }
}
JSON

cat >"$TMP_DIR/status/storage.json" <<'JSON'
{
  "running": true,
  "last_error": null,
  "readiness": {
    "status": "ready",
    "failed_gates": []
  },
  "observability": {
    "storage_challenge_network_degraded": false
  },
  "consensus": {
    "committed_height": null,
    "last_execution_height": null,
    "storage_challenge_network_degraded_height": null,
    "network_head": {
      "source": "self_only",
      "height": null,
      "required_peer_count": 0,
      "decision": "ready"
    }
  },
  "replication": {
    "local_peer_id": "12D3KooWStorage",
    "connected_peers": ["12D3KooWSequencer"]
  }
}
JSON

: >"$TEST_SSH_LOG"
rm -f "$TEST_REMOTE_ROOT"/started-*
if "$ROOT_DIR/scripts/p2p-public-testnet-rebuild-validators.sh" \
  --config-dir "$TMP_DIR/config" \
  --world-dir "$TMP_DIR/world" \
  --consumer-impact-record "$TMP_DIR/consumer-impact-none.json" \
  --sequencer-ssh-host root@sequencer \
  --sequencer-sshpass-env SEQ_PASS \
  --sequencer-service oasis7-triad-sequencer.service \
  --sequencer-status-url http://sequencer/status \
  --storage-ssh-host root@storage \
  --storage-sshpass-env STO_PASS \
  --storage-service oasis7-triad-storage.service \
  --storage-status-url http://storage/status \
  --stack-root /opt/oasis7/p2p-testnet \
  --out-dir "$TMP_DIR/out-clean-genesis" \
  --poll-attempts 1 \
  --poll-sleep-seconds 0 >"$TMP_DIR/oasis7-rebuild-validators-clean-genesis.out" 2>&1; then
  echo "expected rebuild to fail when status is still clean genesis/self_only" >&2
  exit 1
fi

grep -q "sequencer readiness failed checks after restart" \
  "$TMP_DIR/oasis7-rebuild-validators-clean-genesis.out"

python3 - "$TEST_SSH_LOG" <<'PY'
import pathlib
import sys

lines = pathlib.Path(sys.argv[1]).read_text(encoding="utf-8").splitlines()
for host, service in (
    ("root@sequencer", "oasis7-triad-sequencer.service"),
    ("root@storage", "oasis7-triad-storage.service"),
):
    commands = [line.split("\t", 1)[1] for line in lines if line.startswith(f"{host}\t")]
    start_indexes = [
        index for index, command in enumerate(commands) if f"systemctl start '{service}'" in command
    ]
    cleanup_indexes = [
        index
        for index, command in enumerate(commands)
        if (
            f"SERVICE_NAME='{service}'" in command
            or f"systemctl stop '{service}'" in command
        )
        and "STACK_ROOT='/opt/oasis7/p2p-testnet' python3" in command
        and "oasis7_chain_runtime" in command
        and "start-node.sh" in command
    ]
    if not start_indexes:
        raise SystemExit(f"missing {host} start command for clean-genesis path")
    if not any(index > start_indexes[-1] for index in cleanup_indexes):
        raise SystemExit(f"missing post-start cleanup for clean-genesis failure {host}")
PY

cat >"$TMP_DIR/status/sequencer.json" <<'JSON'
{
  "running": true,
  "last_error": null,
  "readiness": {
    "status": "not_ready"
  },
  "observability": {
    "storage_challenge_network_degraded": false
  },
  "consensus": {
    "committed_height": 0,
    "last_execution_height": 0,
    "storage_challenge_network_degraded_height": null
  },
  "replication": {
    "connected_peers": []
  }
}
JSON

: >"$TEST_SSH_LOG"
if "$ROOT_DIR/scripts/p2p-public-testnet-rebuild-validators.sh" \
  --config-dir "$TMP_DIR/config" \
  --world-dir "$TMP_DIR/world" \
  --consumer-impact-record "$TMP_DIR/consumer-impact-none.json" \
  --sequencer-ssh-host root@sequencer \
  --sequencer-sshpass-env SEQ_PASS \
  --sequencer-service oasis7-triad-sequencer.service \
  --sequencer-status-url http://sequencer/status \
  --storage-ssh-host root@storage \
  --storage-sshpass-env STO_PASS \
  --storage-service oasis7-triad-storage.service \
  --storage-status-url http://storage/status \
  --stack-root /opt/oasis7/p2p-testnet \
  --out-dir "$TMP_DIR/out-fail" \
  --poll-attempts 1 \
  --poll-sleep-seconds 0 >"$TMP_DIR/oasis7-rebuild-validators-fail.out" 2>&1; then
  echo "expected rebuild to fail when sequencer readiness stays not_ready" >&2
  exit 1
fi

python3 - "$TEST_SSH_LOG" <<'PY'
import pathlib
import sys

log_path = pathlib.Path(sys.argv[1])
lines = log_path.read_text(encoding="utf-8").splitlines()
sequencer_commands = [line.split("\t", 1)[1] for line in lines if line.startswith("root@sequencer\t")]
start_indexes = [
    index
    for index, command in enumerate(sequencer_commands)
    if "systemctl start 'oasis7-triad-sequencer.service'" in command
]
cleanup_indexes = [
    index
    for index, command in enumerate(sequencer_commands)
    if (
        "SERVICE_NAME='oasis7-triad-sequencer.service'" in command
        or "systemctl stop 'oasis7-triad-sequencer.service'" in command
    )
    and "STACK_ROOT='/opt/oasis7/p2p-testnet' python3" in command
    and "oasis7_chain_runtime" in command
    and "start-node.sh" in command
]
if not start_indexes:
    raise SystemExit("missing sequencer start command")
if not any(index > start_indexes[-1] for index in cleanup_indexes):
    raise SystemExit("missing post-start cleanup after failed sequencer readiness")
PY

: >"$TEST_SSH_LOG"
: >"$TEST_SYSTEMCTL_LOG"
rm -f "$TEST_REMOTE_ROOT"/started-*
if TEST_SYSTEMD_RESTART_LOOP_HOST=root@sequencer \
  TEST_SYSTEMD_RESTART_LOOP_STACK_ROOT=/opt/oasis7/p2p-testnet \
  "$ROOT_DIR/scripts/p2p-public-testnet-rebuild-validators.sh" \
  --config-dir "$TMP_DIR/config" \
  --world-dir "$TMP_DIR/world" \
  --consumer-impact-record "$TMP_DIR/consumer-impact-none.json" \
  --sequencer-ssh-host root@sequencer \
  --sequencer-sshpass-env SEQ_PASS \
  --sequencer-service oasis7-triad-sequencer.service \
  --sequencer-status-url http://sequencer/status \
  --storage-ssh-host root@storage \
  --storage-sshpass-env STO_PASS \
  --storage-service oasis7-triad-storage.service \
  --storage-status-url http://storage/status \
  --stack-root /opt/oasis7/p2p-testnet \
  --out-dir "$TMP_DIR/out-delayed-cleanup" \
  --poll-attempts 1 \
  --poll-sleep-seconds 0 >"$TMP_DIR/oasis7-rebuild-validators-delayed-cleanup.out" 2>&1; then
  :
fi

if pgrep -f "$TEST_REMOTE_ROOT/root@sequencer/opt/oasis7/p2p-testnet/bin/start-node.sh" >/dev/null; then
  pkill -f "$TEST_REMOTE_ROOT/root@sequencer/opt/oasis7/p2p-testnet/bin/start-node.sh" || true
  echo "systemd restart-loop cleanup process survived stable quiet cleanup" >&2
  exit 1
fi
if [[ ! -f "$TEST_REMOTE_ROOT/systemd-root_sequencer-oasis7-triad-sequencer.service/masked" ]]; then
  echo "expected failed cleanup path to leave sequencer service runtime-masked" >&2
  exit 1
fi

python3 - "$TEST_SSH_LOG" <<'PY'
import pathlib
import sys

log_path = pathlib.Path(sys.argv[1])
lines = log_path.read_text(encoding="utf-8").splitlines()
sequencer_commands = [line.split("\t", 1)[1] for line in lines if line.startswith("root@sequencer\t")]
if not any(
    (
        "SERVICE_NAME='oasis7-triad-sequencer.service'" in command
        or "systemctl stop 'oasis7-triad-sequencer.service'" in command
    )
    and "STACK_ROOT='/opt/oasis7/p2p-testnet' python3" in command
    and "oasis7_chain_runtime" in command
    and "start-node.sh" in command
    for command in sequencer_commands
):
    raise SystemExit("missing sequencer cleanup command for delayed cleanup process test")
PY

python3 - "$TEST_SYSTEMCTL_LOG" <<'PY'
import pathlib
import sys

log_path = pathlib.Path(sys.argv[1])
lines = log_path.read_text(encoding="utf-8").splitlines()
unmask_count = sum(
    1
    for line in lines
    if line == "systemctl\tunmask oasis7-triad-sequencer.service"
)
mask_count = sum(
    1
    for line in lines
    if line == "systemctl\tmask --runtime oasis7-triad-sequencer.service"
)
stop_count = sum(
    1
    for line in lines
    if line == "systemctl\tstop oasis7-triad-sequencer.service"
)
kill_count = sum(
    1
    for line in lines
    if line == "systemctl\tkill --kill-who=all --signal=SIGKILL oasis7-triad-sequencer.service"
)
reset_count = sum(
    1
    for line in lines
    if line == "systemctl\treset-failed oasis7-triad-sequencer.service"
)
if stop_count < 2 or kill_count < 2 or reset_count < 2:
    raise SystemExit(
        "cleanup did not repeatedly quiesce systemd while waiting for a stable quiet window"
    )
if unmask_count < 1:
    raise SystemExit("start path did not unmask the sequencer service before systemctl start")
if mask_count < 2:
    raise SystemExit("cleanup did not repeatedly mask the sequencer service while waiting for quiet")
if any(line == "spawned\troot@sequencer\toasis7-triad-sequencer.service" for line in lines):
    raise SystemExit("fake systemd restart-loop spawned despite runtime service mask")
PY

: >"$TEST_SSH_LOG"
: >"$TEST_SYSTEMCTL_LOG"
rm -f "$TEST_REMOTE_ROOT"/started-*
legacy_state_dir="$TEST_REMOTE_ROOT/systemd-root_sequencer-oasis7-triad-sequencer.service"
mkdir -p "$legacy_state_dir"
touch "$legacy_state_dir/armed"
if TEST_SYSTEMD_RESTART_LOOP_HOST=root@sequencer \
  TEST_SYSTEMD_RESTART_LOOP_STACK_ROOT=/opt/oasis7/p2p-testnet \
  TEST_SYSTEMD_STACK_OWNER_SERVICES=oasis7-testnet-sequencer.service,oasis7-triad-sequencer.service \
  TEST_SYSTEMD_PREFIX_STACK_OWNER_SERVICES=oasis7-prefix-sequencer.service \
  "$ROOT_DIR/scripts/p2p-public-testnet-rebuild-validators.sh" \
  --config-dir "$TMP_DIR/config" \
  --world-dir "$TMP_DIR/world" \
  --consumer-impact-record "$TMP_DIR/consumer-impact-none.json" \
  --sequencer-ssh-host root@sequencer \
  --sequencer-sshpass-env SEQ_PASS \
  --sequencer-service oasis7-testnet-sequencer.service \
  --sequencer-status-url http://sequencer/status \
  --storage-ssh-host root@storage \
  --storage-sshpass-env STO_PASS \
  --storage-service oasis7-testnet-storage.service \
  --storage-status-url http://storage/status \
  --stack-root /opt/oasis7/p2p-testnet \
  --out-dir "$TMP_DIR/out-stack-owner-services" \
  --poll-attempts 1 \
  --poll-sleep-seconds 0 >"$TMP_DIR/oasis7-rebuild-validators-stack-owner-services.out" 2>&1; then
  :
fi

python3 - "$TEST_SYSTEMCTL_LOG" <<'PY'
import pathlib
import sys

lines = pathlib.Path(sys.argv[1]).read_text(encoding="utf-8").splitlines()
required = {
    "systemctl\tmask --runtime oasis7-testnet-sequencer.service",
    "systemctl\tmask --runtime oasis7-triad-sequencer.service",
}
missing = sorted(required - set(lines))
if missing:
    raise SystemExit(f"missing stack-owner runtime mask calls: {missing}")
if "systemctl\tunmask oasis7-triad-sequencer.service" in lines:
    raise SystemExit("legacy stack-owner service was unmasked by explicit start path")
if "systemctl\tmask --runtime oasis7-prefix-sequencer.service" in lines:
    raise SystemExit("prefix-only stack-root service was incorrectly masked")
if any(line == "spawned\troot@sequencer\toasis7-triad-sequencer.service" for line in lines):
    raise SystemExit("legacy stack-owner service respawned during cleanup")
PY

: >"$TEST_SSH_LOG"
rm -f "$TEST_REMOTE_ROOT"/started-*
if TEST_SPAWN_LATE_CLEANUP_PROCESS_HOST=root@sequencer "$ROOT_DIR/scripts/p2p-public-testnet-rebuild-validators.sh" \
  --config-dir "$TMP_DIR/config" \
  --world-dir "$TMP_DIR/world" \
  --consumer-impact-record "$TMP_DIR/consumer-impact-none.json" \
  --sequencer-ssh-host root@sequencer \
  --sequencer-sshpass-env SEQ_PASS \
  --sequencer-service oasis7-triad-sequencer.service \
  --sequencer-status-url http://sequencer/status \
  --storage-ssh-host root@storage \
  --storage-sshpass-env STO_PASS \
  --storage-service oasis7-triad-storage.service \
  --storage-status-url http://storage/status \
  --stack-root /opt/oasis7/p2p-testnet \
  --out-dir "$TMP_DIR/out-late-cleanup" \
  --poll-attempts 1 \
  --poll-sleep-seconds 0 >"$TMP_DIR/oasis7-rebuild-validators-late-cleanup.out" 2>&1; then
  echo "expected rebuild cleanup to fail when stable quiet window is not observed" >&2
  exit 1
fi
grep -q "cleanup failed: stable quiet window was not observed before deadline" \
  "$TMP_DIR/oasis7-rebuild-validators-late-cleanup.out"
pkill -f "$TEST_REMOTE_ROOT/root@sequencer/opt/oasis7/p2p-testnet/bin/start-node.sh" || true

: >"$TEST_SSH_LOG"
if TEST_FAIL_SYSTEMCTL_MASK_HOST=root@sequencer "$ROOT_DIR/scripts/p2p-public-testnet-rebuild-validators.sh" \
  --config-dir "$TMP_DIR/config" \
  --world-dir "$TMP_DIR/world" \
  --consumer-impact-record "$TMP_DIR/consumer-impact-none.json" \
  --sequencer-ssh-host root@sequencer \
  --sequencer-sshpass-env SEQ_PASS \
  --sequencer-service oasis7-triad-sequencer.service \
  --sequencer-status-url http://sequencer/status \
  --storage-ssh-host root@storage \
  --storage-sshpass-env STO_PASS \
  --storage-service oasis7-triad-storage.service \
  --storage-status-url http://storage/status \
  --stack-root /opt/oasis7/p2p-testnet \
  --out-dir "$TMP_DIR/out-mask-fail" \
  --poll-attempts 1 \
  --poll-sleep-seconds 0 >"$TMP_DIR/oasis7-rebuild-validators-mask-fail.out" 2>&1; then
  echo "expected rebuild cleanup to fail when systemctl runtime mask fails" >&2
  exit 1
fi
grep -q "cleanup failed: systemctl runtime mask failed for oasis7-triad-sequencer.service: runtime mask denied" \
  "$TMP_DIR/oasis7-rebuild-validators-mask-fail.out"

: >"$TEST_SSH_LOG"
if TEST_FAIL_START_HOST=root@sequencer "$ROOT_DIR/scripts/p2p-public-testnet-rebuild-validators.sh" \
  --config-dir "$TMP_DIR/config" \
  --world-dir "$TMP_DIR/world" \
  --consumer-impact-record "$TMP_DIR/consumer-impact-none.json" \
  --sequencer-ssh-host root@sequencer \
  --sequencer-sshpass-env SEQ_PASS \
  --sequencer-service oasis7-triad-sequencer.service \
  --sequencer-status-url http://sequencer/status \
  --storage-ssh-host root@storage \
  --storage-sshpass-env STO_PASS \
  --storage-service oasis7-triad-storage.service \
  --storage-status-url http://storage/status \
  --stack-root /opt/oasis7/p2p-testnet \
  --out-dir "$TMP_DIR/out-start-fail" \
  --poll-attempts 1 \
  --poll-sleep-seconds 0 >"$TMP_DIR/oasis7-rebuild-validators-start-fail.out" 2>&1; then
  echo "expected rebuild to fail when sequencer systemctl start fails" >&2
  exit 1
fi

python3 - "$TEST_SSH_LOG" <<'PY'
import pathlib
import sys

log_path = pathlib.Path(sys.argv[1])
lines = log_path.read_text(encoding="utf-8").splitlines()
sequencer_commands = [line.split("\t", 1)[1] for line in lines if line.startswith("root@sequencer\t")]
start_indexes = [
    index
    for index, command in enumerate(sequencer_commands)
    if "systemctl start 'oasis7-triad-sequencer.service'" in command
]
cleanup_indexes = [
    index
    for index, command in enumerate(sequencer_commands)
    if (
        "SERVICE_NAME='oasis7-triad-sequencer.service'" in command
        or "systemctl stop 'oasis7-triad-sequencer.service'" in command
    )
    and "STACK_ROOT='/opt/oasis7/p2p-testnet' python3" in command
    and "oasis7_chain_runtime" in command
    and "start-node.sh" in command
]
if not start_indexes:
    raise SystemExit("missing sequencer start command for start-failure path")
if not any(index > start_indexes[-1] for index in cleanup_indexes):
    raise SystemExit("missing post-start cleanup after failed sequencer systemctl start")
PY

: >"$TEST_SSH_LOG"
rm -f "$TEST_REMOTE_ROOT"/started-*
if TEST_FAIL_CLEANUP_AFTER_START_HOST=root@sequencer "$ROOT_DIR/scripts/p2p-public-testnet-rebuild-validators.sh" \
  --config-dir "$TMP_DIR/config" \
  --world-dir "$TMP_DIR/world" \
  --consumer-impact-record "$TMP_DIR/consumer-impact-none.json" \
  --sequencer-ssh-host root@sequencer \
  --sequencer-sshpass-env SEQ_PASS \
  --sequencer-service oasis7-triad-sequencer.service \
  --sequencer-status-url http://sequencer/status \
  --storage-ssh-host root@storage \
  --storage-sshpass-env STO_PASS \
  --storage-service oasis7-triad-storage.service \
  --storage-status-url http://storage/status \
  --stack-root /opt/oasis7/p2p-testnet \
  --out-dir "$TMP_DIR/out-cleanup-fail" \
  --poll-attempts 1 \
  --poll-sleep-seconds 0 >"$TMP_DIR/oasis7-rebuild-validators-cleanup-fail.out" 2>&1; then
  echo "expected rebuild to fail when post-start cleanup command fails" >&2
  exit 1
fi

grep -q "sequencer readiness failed checks after restart and cleanup failed" \
  "$TMP_DIR/oasis7-rebuild-validators-cleanup-fail.out"

cat >"$TMP_DIR/status/storage-not-ready.json" <<'JSON'
{
  "running": true,
  "last_error": null,
  "readiness": {
    "status": "not_ready"
  },
  "observability": {
    "storage_challenge_network_degraded": false
  },
  "consensus": {
    "committed_height": 0,
    "last_execution_height": 0,
    "storage_challenge_network_degraded_height": null,
    "network_head": {
      "height": null
    }
  },
  "replication": {
    "connected_peers": []
  }
}
JSON

: >"$TEST_SSH_LOG"
rm -f "$TEST_REMOTE_ROOT"/started-*
if TEST_STORAGE_STATUS_OVERRIDE="$TMP_DIR/status/storage-not-ready.json" "$ROOT_DIR/scripts/p2p-public-testnet-rebuild-validators.sh" \
  --config-dir "$TMP_DIR/config" \
  --world-dir "$TMP_DIR/world" \
  --consumer-impact-record "$TMP_DIR/consumer-impact-none.json" \
  --sequencer-ssh-host root@sequencer \
  --sequencer-sshpass-env SEQ_PASS \
  --sequencer-service oasis7-triad-sequencer.service \
  --sequencer-status-url http://sequencer/status \
  --storage-ssh-host root@storage \
  --storage-sshpass-env STO_PASS \
  --storage-service oasis7-triad-storage.service \
  --storage-status-url http://storage/status \
  --stack-root /opt/oasis7/p2p-testnet \
  --out-dir "$TMP_DIR/out-storage-fail" \
  --poll-attempts 1 \
  --poll-sleep-seconds 0 >"$TMP_DIR/oasis7-rebuild-validators-storage-fail.out" 2>&1; then
  echo "expected rebuild to fail when storage readiness stays not_ready" >&2
  exit 1
fi

python3 - "$TEST_SSH_LOG" <<'PY'
import pathlib
import sys

log_path = pathlib.Path(sys.argv[1])
lines = log_path.read_text(encoding="utf-8").splitlines()

def commands_for(host: str) -> list[str]:
    prefix = f"{host}\t"
    return [line.split("\t", 1)[1] for line in lines if line.startswith(prefix)]

for host, service in (
    ("root@sequencer", "oasis7-triad-sequencer.service"),
    ("root@storage", "oasis7-triad-storage.service"),
):
    commands = commands_for(host)
    start_indexes = [
        index
        for index, command in enumerate(commands)
        if f"systemctl start '{service}'" in command
    ]
    cleanup_indexes = [
        index
        for index, command in enumerate(commands)
        if (
            f"SERVICE_NAME='{service}'" in command
            or f"systemctl stop '{service}'" in command
        )
        and "STACK_ROOT='/opt/oasis7/p2p-testnet' python3" in command
        and "oasis7_chain_runtime" in command
        and "start-node.sh" in command
    ]
    if not start_indexes:
        raise SystemExit(f"missing {host} start command for storage-failure path")
    if not any(index > start_indexes[-1] for index in cleanup_indexes):
        raise SystemExit(f"missing post-start cleanup for {host} after storage readiness failure")
PY
