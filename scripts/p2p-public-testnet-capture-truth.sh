#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage:
  ./scripts/p2p-public-testnet-capture-truth.sh \
    --bundle <path> \
    --sequencer-status-url <url> \
    --storage-status-url <url> \
    [--sequencer-runtime-path <path>] \
    [--storage-runtime-path <path>] \
    [--sequencer-node-keypair-path <path>] \
    [--storage-node-keypair-path <path>] \
    [--sequencer-ssh-host <user@host>] \
    [--sequencer-sshpass-env <env-name>] \
    [--storage-ssh-host <user@host>] \
    [--storage-sshpass-env <env-name>] \
    [--stack-root <path>] \
    [--out <path>]

  ./scripts/p2p-public-testnet-capture-truth.sh \
    --bundle <path> \
    --sequencer-status-json <path> \
    --storage-status-json <path> \
    [--sequencer-runtime-path <path>] \
    [--storage-runtime-path <path>] \
    [--sequencer-node-keypair-path <path>] \
    [--storage-node-keypair-path <path>] \
    [--out <path>]

Description:
  Capture the deployment truth that must be aligned before a governed
  public_testnet rebuild:
    - bundle validation result
    - runtime sha256 per validator
    - node-keypair presence per validator
    - live libp2p local_peer_id per validator
    - live validator heights
EOF
}

die() {
  printf 'error: %s\n' "$*" >&2
  exit 1
}

require_file() {
  local path=$1
  [[ -f "$path" ]] || die "missing file: $path"
}

require_command() {
  local name=$1
  command -v "$name" >/dev/null 2>&1 || die "missing command: $name"
}

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

BUNDLE_PATH=""
SEQUENCER_STATUS_URL=""
STORAGE_STATUS_URL=""
SEQUENCER_STATUS_JSON=""
STORAGE_STATUS_JSON=""
SEQUENCER_RUNTIME_PATH=""
STORAGE_RUNTIME_PATH=""
SEQUENCER_NODE_KEYPAIR_PATH=""
STORAGE_NODE_KEYPAIR_PATH=""
SEQUENCER_SSH_HOST=""
STORAGE_SSH_HOST=""
SEQUENCER_SSHPASS_ENV=""
STORAGE_SSHPASS_ENV=""
STACK_ROOT="/opt/oasis7/p2p-testnet"
OUT_PATH="-"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --bundle)
      BUNDLE_PATH=${2:-}
      shift 2
      ;;
    --sequencer-status-url)
      SEQUENCER_STATUS_URL=${2:-}
      shift 2
      ;;
    --storage-status-url)
      STORAGE_STATUS_URL=${2:-}
      shift 2
      ;;
    --sequencer-status-json)
      SEQUENCER_STATUS_JSON=${2:-}
      shift 2
      ;;
    --storage-status-json)
      STORAGE_STATUS_JSON=${2:-}
      shift 2
      ;;
    --sequencer-runtime-path)
      SEQUENCER_RUNTIME_PATH=${2:-}
      shift 2
      ;;
    --storage-runtime-path)
      STORAGE_RUNTIME_PATH=${2:-}
      shift 2
      ;;
    --sequencer-node-keypair-path)
      SEQUENCER_NODE_KEYPAIR_PATH=${2:-}
      shift 2
      ;;
    --storage-node-keypair-path)
      STORAGE_NODE_KEYPAIR_PATH=${2:-}
      shift 2
      ;;
    --sequencer-ssh-host)
      SEQUENCER_SSH_HOST=${2:-}
      shift 2
      ;;
    --sequencer-sshpass-env)
      SEQUENCER_SSHPASS_ENV=${2:-}
      shift 2
      ;;
    --storage-ssh-host)
      STORAGE_SSH_HOST=${2:-}
      shift 2
      ;;
    --storage-sshpass-env)
      STORAGE_SSHPASS_ENV=${2:-}
      shift 2
      ;;
    --stack-root)
      STACK_ROOT=${2:-}
      shift 2
      ;;
    --out)
      OUT_PATH=${2:-}
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      die "unknown argument: $1"
      ;;
  esac
done

[[ -n "$BUNDLE_PATH" ]] || die "--bundle is required"
require_file "$BUNDLE_PATH"
require_command jq
require_command curl
require_command sha256sum

status_source_count=0
[[ -n "$SEQUENCER_STATUS_URL" ]] && status_source_count=$((status_source_count + 1))
[[ -n "$SEQUENCER_STATUS_JSON" ]] && status_source_count=$((status_source_count + 1))
[[ "$status_source_count" -eq 1 ]] || die "provide exactly one of --sequencer-status-url or --sequencer-status-json"

status_source_count=0
[[ -n "$STORAGE_STATUS_URL" ]] && status_source_count=$((status_source_count + 1))
[[ -n "$STORAGE_STATUS_JSON" ]] && status_source_count=$((status_source_count + 1))
[[ "$status_source_count" -eq 1 ]] || die "provide exactly one of --storage-status-url or --storage-status-json"

run_bundle_validate() {
  ./scripts/release-candidate-bundle.sh validate --bundle "$BUNDLE_PATH" >/dev/null
}

ssh_run() {
  local host=$1
  local sshpass_env_name=${2:-}
  shift 2
  require_command ssh
  if [[ -n "$sshpass_env_name" ]]; then
    require_command sshpass
    local sshpass_value=${!sshpass_env_name:-}
    [[ -n "$sshpass_value" ]] || die "ssh password env is empty: $sshpass_env_name"
    SSHPASS="$sshpass_value" sshpass -e ssh \
      -o StrictHostKeyChecking=no \
      -o UserKnownHostsFile=/dev/null \
      "$host" \
      "$@"
  elif [[ -n "${SSHPASS:-}" ]]; then
    require_command sshpass
    sshpass -e ssh \
      -o StrictHostKeyChecking=no \
      -o UserKnownHostsFile=/dev/null \
      "$host" \
      "$@"
  else
    ssh \
      -o StrictHostKeyChecking=no \
      -o UserKnownHostsFile=/dev/null \
      "$host" \
      "$@"
  fi
}

runtime_sha_for_local_path() {
  local path=$1
  if [[ -z "$path" ]]; then
    printf 'null'
    return 0
  fi
  require_file "$path"
  sha256sum "$path" | awk '{print $1}'
}

runtime_sha_for_remote() {
  local host=$1
  local path=$2
  local sshpass_env_name=$3
  local default_path="$STACK_ROOT/current/bin/oasis7_chain_runtime"
  local use_path=${path:-$default_path}
  ssh_run "$host" "$sshpass_env_name" "sha256sum '$use_path' | awk '{print \$1}'"
}

path_exists_local_json() {
  local path=$1
  if [[ -z "$path" ]]; then
    printf 'null'
  elif [[ -e "$path" ]]; then
    printf 'true'
  else
    printf 'false'
  fi
}

path_exists_remote_json() {
  local host=$1
  local path=$2
  local sshpass_env_name=$3
  local default_path="$STACK_ROOT/config/node-keypair.toml"
  local use_path=${path:-$default_path}
  if ssh_run "$host" "$sshpass_env_name" "test -e '$use_path'"; then
    printf 'true'
  else
    printf 'false'
  fi
}

tmp_dir=$(mktemp -d "${TMPDIR:-/tmp}/oasis7-capture-truth.XXXXXX")
trap 'rm -rf "$tmp_dir"' EXIT

run_bundle_validate

sequencer_status_path="$tmp_dir/sequencer-status.json"
storage_status_path="$tmp_dir/storage-status.json"

if [[ -n "$SEQUENCER_STATUS_URL" ]]; then
  curl -fsSL "$SEQUENCER_STATUS_URL" -o "$sequencer_status_path"
else
  cp "$SEQUENCER_STATUS_JSON" "$sequencer_status_path"
fi

if [[ -n "$STORAGE_STATUS_URL" ]]; then
  curl -fsSL "$STORAGE_STATUS_URL" -o "$storage_status_path"
else
  cp "$STORAGE_STATUS_JSON" "$storage_status_path"
fi

sequencer_runtime_sha="null"
storage_runtime_sha="null"
sequencer_keypair_present="null"
storage_keypair_present="null"

if [[ -n "$SEQUENCER_SSH_HOST" ]]; then
  sequencer_runtime_sha=$(runtime_sha_for_remote "$SEQUENCER_SSH_HOST" "$SEQUENCER_RUNTIME_PATH" "$SEQUENCER_SSHPASS_ENV")
  sequencer_keypair_present=$(path_exists_remote_json "$SEQUENCER_SSH_HOST" "$SEQUENCER_NODE_KEYPAIR_PATH" "$SEQUENCER_SSHPASS_ENV")
else
  local_sha=$(runtime_sha_for_local_path "$SEQUENCER_RUNTIME_PATH" || true)
  if [[ -n "${local_sha:-}" && "$local_sha" != "null" ]]; then
    sequencer_runtime_sha=$local_sha
  fi
  sequencer_keypair_present=$(path_exists_local_json "$SEQUENCER_NODE_KEYPAIR_PATH")
fi

if [[ -n "$STORAGE_SSH_HOST" ]]; then
  storage_runtime_sha=$(runtime_sha_for_remote "$STORAGE_SSH_HOST" "$STORAGE_RUNTIME_PATH" "$STORAGE_SSHPASS_ENV")
  storage_keypair_present=$(path_exists_remote_json "$STORAGE_SSH_HOST" "$STORAGE_NODE_KEYPAIR_PATH" "$STORAGE_SSHPASS_ENV")
else
  local_sha=$(runtime_sha_for_local_path "$STORAGE_RUNTIME_PATH" || true)
  if [[ -n "${local_sha:-}" && "$local_sha" != "null" ]]; then
    storage_runtime_sha=$local_sha
  fi
  storage_keypair_present=$(path_exists_local_json "$STORAGE_NODE_KEYPAIR_PATH")
fi

json_output=$(jq -n \
  --arg bundle_path "$BUNDLE_PATH" \
  --argjson sequencer_status "$(jq -c '.' "$sequencer_status_path")" \
  --argjson storage_status "$(jq -c '.' "$storage_status_path")" \
  --arg sequencer_runtime_sha "$sequencer_runtime_sha" \
  --arg storage_runtime_sha "$storage_runtime_sha" \
  --argjson sequencer_keypair_present "$sequencer_keypair_present" \
  --argjson storage_keypair_present "$storage_keypair_present" \
  '{
    bundle_validate: {ok: true, bundle_path: $bundle_path},
    validators: {
      sequencer: {
        runtime_sha256: (if $sequencer_runtime_sha == "null" then null else $sequencer_runtime_sha end),
        node_keypair_present: $sequencer_keypair_present,
        node_id: ($sequencer_status.node_id // null),
        local_peer_id: ($sequencer_status.replication.local_peer_id // null),
        committed_height: ($sequencer_status.consensus.committed_height // null),
        last_block_hash: ($sequencer_status.consensus.last_block_hash // null),
        last_execution_height: ($sequencer_status.consensus.last_execution_height // null),
        last_execution_block_hash: ($sequencer_status.consensus.last_execution_block_hash // null),
        last_execution_state_root: ($sequencer_status.consensus.last_execution_state_root // null),
        network_head: ($sequencer_status.consensus.network_head // null),
        readiness: {
          status: ($sequencer_status.readiness.status // null),
          failed_gates: ($sequencer_status.readiness.failed_gates // [])
        },
        network_tier: ($sequencer_status.network_tier // null),
        chain_proof: ($sequencer_status.chain_proof // null),
        status_url: ($sequencer_status.status_url // null)
      },
      storage: {
        runtime_sha256: (if $storage_runtime_sha == "null" then null else $storage_runtime_sha end),
        node_keypair_present: $storage_keypair_present,
        node_id: ($storage_status.node_id // null),
        local_peer_id: ($storage_status.replication.local_peer_id // null),
        committed_height: ($storage_status.consensus.committed_height // null),
        last_block_hash: ($storage_status.consensus.last_block_hash // null),
        last_execution_height: ($storage_status.consensus.last_execution_height // null),
        last_execution_block_hash: ($storage_status.consensus.last_execution_block_hash // null),
        last_execution_state_root: ($storage_status.consensus.last_execution_state_root // null),
        network_head: ($storage_status.consensus.network_head // null),
        readiness: {
          status: ($storage_status.readiness.status // null),
          failed_gates: ($storage_status.readiness.failed_gates // [])
        },
        network_tier: ($storage_status.network_tier // null),
        chain_proof: ($storage_status.chain_proof // null),
        status_url: ($storage_status.status_url // null)
      }
    }
  }')

if [[ "$OUT_PATH" == "-" ]]; then
  printf '%s\n' "$json_output"
else
  mkdir -p "$(dirname "$OUT_PATH")"
  printf '%s\n' "$json_output" >"$OUT_PATH"
fi
