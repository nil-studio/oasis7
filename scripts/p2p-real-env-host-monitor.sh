#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

usage() {
  cat <<'USAGE'
Usage: ./scripts/p2p-real-env-host-monitor.sh [options]

Collect triad host/process monitoring samples from:
  - local triad node
  - remote ECS sequencer node
  - remote ECS storage node

Options:
  --samples <n>                    number of samples per node (default: 4)
  --interval-secs <n>              sleep interval between samples (default: 5)
  --ssh-timeout-secs <n>           SSH connect timeout in seconds (default: 8)
  --out-dir <path>                 output root (default: .tmp/p2p_real_env_host_monitor)

  --local-service <name>           local node systemd unit
                                   (default: oasis7-triad-observer.service)
  --local-storage-path <path>      local node storage path
                                   (default: /opt/oasis7/p2p-triad-local/data/storage)
  --observer-*                     deprecated aliases for the local-* options above

  --sequencer-target <user@host>   remote sequencer SSH target
                                   (default: root@39.104.204.172)
  --sequencer-service <name>       remote sequencer systemd unit
                                   (default: oasis7-triad-sequencer.service)
  --sequencer-storage-path <path>  remote sequencer storage path
                                   (default: /opt/oasis7/p2p-triad/data/storage)

  --storage-target <user@host>     remote storage SSH target
                                   (default: root@39.104.205.67)
  --storage-service <name>         remote storage systemd unit
                                   (default: oasis7-triad-storage.service)
  --storage-storage-path <path>    remote storage storage path
                                   (default: /opt/oasis7/p2p-triad/data/storage)

Environment:
  P2PARCH6_SEQ_SSH_PASSWORD        optional sequencer SSH password for sshpass
  P2PARCH6_STORAGE_SSH_PASSWORD    optional storage SSH password for sshpass

Artifacts:
  <out-dir>/<timestamp>/samples.ndjson
  <out-dir>/<timestamp>/summary.json
  <out-dir>/<timestamp>/summary.md
  <out-dir>/latest_summary.json
  <out-dir>/latest_summary.md
USAGE
}

ensure_positive_int() {
  local flag=$1
  local value=$2
  if [[ ! "$value" =~ ^[0-9]+$ ]] || (( value <= 0 )); then
    echo "invalid $flag: $value" >&2
    exit 2
  fi
}

run_ssh() {
  local target=$1
  local password=${2:-}
  shift 2
  local cmd=(
    ssh
    -o StrictHostKeyChecking=no
    -o ConnectTimeout="$ssh_timeout_secs"
    -o ServerAliveInterval=5
    "$target"
    "$@"
  )
  if [[ -n "$password" ]]; then
    SSHPASS="$password" sshpass -e "${cmd[@]}"
  else
    "${cmd[@]}"
  fi
}

samples=4
interval_secs=5
ssh_timeout_secs=8
out_root=".tmp/p2p_real_env_host_monitor"

local_service="oasis7-triad-observer.service"
local_storage_path="/opt/oasis7/p2p-triad-local/data/storage"

sequencer_target="root@39.104.204.172"
sequencer_service="oasis7-triad-sequencer.service"
sequencer_storage_path="/opt/oasis7/p2p-triad/data/storage"

storage_target="root@39.104.205.67"
storage_service="oasis7-triad-storage.service"
storage_storage_path="/opt/oasis7/p2p-triad/data/storage"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --samples)
      samples=${2:-}
      shift 2
      ;;
    --interval-secs)
      interval_secs=${2:-}
      shift 2
      ;;
    --ssh-timeout-secs)
      ssh_timeout_secs=${2:-}
      shift 2
      ;;
    --out-dir)
      out_root=${2:-}
      shift 2
      ;;
    --local-service)
      local_service=${2:-}
      shift 2
      ;;
    --local-storage-path)
      local_storage_path=${2:-}
      shift 2
      ;;
    --sequencer-target)
      sequencer_target=${2:-}
      shift 2
      ;;
    --sequencer-service)
      sequencer_service=${2:-}
      shift 2
      ;;
    --sequencer-storage-path)
      sequencer_storage_path=${2:-}
      shift 2
      ;;
    --storage-target)
      storage_target=${2:-}
      shift 2
      ;;
    --storage-service)
      storage_service=${2:-}
      shift 2
      ;;
    --storage-storage-path)
      storage_storage_path=${2:-}
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "unknown option: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

ensure_positive_int --samples "$samples"
ensure_positive_int --interval-secs "$interval_secs"
ensure_positive_int --ssh-timeout-secs "$ssh_timeout_secs"

run_id=$(date +"%Y%m%d-%H%M%S")
run_dir="$out_root/$run_id"
nodes_root="$run_dir/nodes"
samples_ndjson="$run_dir/samples.ndjson"
summary_json="$run_dir/summary.json"
summary_md="$run_dir/summary.md"
latest_summary_json="$out_root/latest_summary.json"
latest_summary_md="$out_root/latest_summary.md"

mkdir -p \
  "$nodes_root/local_node" \
  "$nodes_root/sequencer_ecs" \
  "$nodes_root/storage_ecs"
: > "$samples_ndjson"

seq_password=${P2PARCH6_SEQ_SSH_PASSWORD:-}
storage_password=${P2PARCH6_STORAGE_SSH_PASSWORD:-}
helper_script="$repo_root/scripts/p2p-real-env-node-host-sample.sh"

capture_sample() {
  local label=$1
  local mode=$2
  local service=$3
  local storage_path=$4
  local target=${5:-}
  local password=${6:-}
  local sample_dir="$nodes_root/$label/sample-$(printf '%03d' "$sample_index")"
  mkdir -p "$sample_dir"
  local sample_env="$sample_dir/sample.env"

  local stage="" error=""
  if [[ "$mode" == "local" ]]; then
    if bash "$helper_script" "$service" "$storage_path" >"$sample_env" 2>/dev/null; then :; else
      stage=local_probe; error=local_probe_failed
    fi
  else
    local remote_command
    printf -v remote_command 'bash -s -- %q %q' "$service" "$storage_path"
    if run_ssh "$target" "$password" "$remote_command" <"$helper_script" >"$sample_env" 2>/dev/null; then :; else
      stage=ssh; error=ssh_failed
    fi
  fi
  if python3 "$repo_root/scripts/p2p-real-env-host-summary.py" --sample-env "$sample_env" \
    "$label" "$sample_index" "$service" "$storage_path" "$stage" "$error" >>"$samples_ndjson"; then
    :
  else
    collection_failed=1
  fi
}

collection_failed=0
started_at=$(date -Iseconds)
for ((sample_index = 1; sample_index <= samples; sample_index++)); do
  captured_at=$(date -Iseconds)
  captured_at_unix_ms=$(( $(date +%s) * 1000 ))
  echo "host sample $sample_index/$samples @ $captured_at"

  capture_sample local_node local "$local_service" "$local_storage_path"
  capture_sample sequencer_ecs remote "$sequencer_service" "$sequencer_storage_path" "$sequencer_target" "$seq_password"
  capture_sample storage_ecs remote "$storage_service" "$storage_storage_path" "$storage_target" "$storage_password"

  if (( sample_index < samples )); then
    sleep "$interval_secs"
  fi
done
ended_at=$(date -Iseconds)

python3 "$repo_root/scripts/p2p-real-env-host-summary.py" \
  --history-path "$samples_ndjson" \
  --summary-json "$summary_json" \
  --summary-md "$summary_md" \
  --expected-samples-per-node "$samples" \
  --run-id "$run_id" \
  --run-dir "$run_dir"

cp "$summary_json" "$latest_summary_json"
cp "$summary_md" "$latest_summary_md"

echo "p2p real-env host monitor complete"
echo "  run_dir: $run_dir"
echo "  started_at: $started_at"
echo "  ended_at: $ended_at"
echo "  summary_json: $summary_json"
echo "  summary_md: $summary_md"

exit "$collection_failed"
