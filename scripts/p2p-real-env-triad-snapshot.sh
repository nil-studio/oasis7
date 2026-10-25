#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

CURL_MAX_TIME_SECS=8
original_args=("$@")

usage() {
  cat <<'USAGE'
Usage: ./scripts/p2p-real-env-triad-snapshot.sh [options]

Capture a P2PARCH-6 real-environment triad snapshot from:
  - local triad node (artifact label: local_node)
  - remote ECS node A (historical artifact/service alias: sequencer_ecs)
  - remote ECS node B (historical artifact/service alias: storage_ecs)

Options:
  --samples <n>                    number of status samples per node (default: 4)
  --interval-secs <n>              sleep interval between samples (default: 5)
  --ssh-timeout-secs <n>           SSH connect timeout in seconds (default: 8)
  --out-dir <path>                 output root (default: .tmp/p2p_real_env_triad)
  --world-id <id>                  expected world id (default: oasis7-unified-world-v1)
  --node <spec>                    explicit node spec; repeat for any number of nodes.
                                   When at least one --node is supplied, only the explicit
                                   nodes are sampled and legacy triad defaults are not added.
                                   Spec format:
                                     label=<safe-label>,mode=<local|remote>,service=<unit>,
                                     status_url=<url>,health_url=<url>,env_file=<path>,
                                     target=<user@host>,password_env=<ENV>,
                                     public_status_url=<url>,public_health_url=<url>
                                   Remote nodes require target. Password values must be
                                   supplied through password_env, never in the CLI.

  --local-service <name>           local node systemd unit
                                   (default: oasis7-triad-observer.service)
  --local-status-url <url>         local node status endpoint
                                   (default: http://127.0.0.1:5633/v1/chain/status)
  --local-health-url <url>         local node health endpoint
                                   (default: http://127.0.0.1:5633/healthz)
  --local-env-file <path>          local node env file
                                   (default: /opt/oasis7/p2p-triad-local/config/node.env)

  --sequencer-target <user@host>   remote sequencer SSH target
                                   (default: root@39.104.204.172)
  --sequencer-service <name>       remote sequencer systemd unit
                                   (default: oasis7-triad-sequencer.service)
  --sequencer-status-url <url>     remote sequencer status endpoint
                                   (default: http://127.0.0.1:5631/v1/chain/status)
  --sequencer-health-url <url>     remote sequencer health endpoint
                                   (default: http://127.0.0.1:5631/healthz)
  --sequencer-public-status-url <url>
                                   optional public fallback when SSH status fetch fails
  --sequencer-public-health-url <url>
                                   optional public fallback when SSH health fetch fails
  --sequencer-env-file <path>      remote sequencer env file
                                   (default: /opt/oasis7/p2p-triad/config/node.env)

  --storage-target <user@host>     remote storage SSH target
                                   (default: root@39.104.205.67)
  --storage-service <name>         remote storage systemd unit
                                   (default: oasis7-triad-storage.service)
  --storage-status-url <url>       remote storage status endpoint
                                   (default: http://127.0.0.1:5632/v1/chain/status)
  --storage-health-url <url>       remote storage health endpoint
                                   (default: http://127.0.0.1:5632/healthz)
  --storage-public-status-url <url>
                                   optional public fallback when SSH status fetch fails
  --storage-public-health-url <url>
                                   optional public fallback when SSH health fetch fails
  --storage-env-file <path>        remote storage env file
                                   (default: /opt/oasis7/p2p-triad/config/node.env)

Environment:
  P2PARCH6_SEQ_SSH_PASSWORD        optional sequencer SSH password for sshpass
  P2PARCH6_STORAGE_SSH_PASSWORD    optional storage SSH password for sshpass

Notes:
  - No secrets are written into the repository or the generated summary.
  - If the password env vars are unset, the script falls back to plain SSH and
    assumes key-based auth already works.
  - Artifact labels are placement-oriented (`local_node / sequencer_ecs /
    storage_ecs`) and do not imply the current runtime role.
  - Runtime role truth is always taken from each sampled `status.json`.
  - Artifacts are written to:
      <out-dir>/runs/<sequence>-<unique-id>/{
        config.json,
        samples.ndjson,
        summary.json,
        summary.md,
        nodes/<label>/{node.env,service_state.txt,healthz.json,status.json,samples/*}
      }
USAGE
}

normalize_positive_int() {
  local flag=$1 value=$2
  if [[ ! "$value" =~ ^[0-9]+$ ]]; then
    echo "invalid $flag: expected ASCII positive integer" >&2; exit 2
  fi
  while [[ ${#value} -gt 1 && "$value" == 0* ]]; do value=${value#0}; done
  if [[ ${#value} -gt 10 ]] || (( 10#$value < 1 || 10#$value > 2147483647 )); then
    echo "invalid $flag: outside 1..2147483647" >&2; exit 2
  fi
  printf '%s' "$value"
}

parse_node_spec() {
  python3 - "$1" <<'PY'
from __future__ import annotations

import json
import re
import sys

spec = sys.argv[1]
fields: dict[str, str] = {}
for part in spec.split(","):
    if "=" not in part:
        raise SystemExit(f"invalid --node segment (expected key=value): {part}")
    key, value = part.split("=", 1)
    key = key.strip()
    value = value.strip()
    if key in fields:
        raise SystemExit("duplicate --node key")
    if not key:
        raise SystemExit("invalid --node segment with empty key")
    fields[key] = value

allowed = {
    "label",
    "mode",
    "target",
    "service",
    "status_url",
    "health_url",
    "env_file",
    "public_status_url",
    "public_health_url",
    "password_env",
}
unknown = sorted(set(fields) - allowed)
if unknown:
    raise SystemExit(f"unknown --node keys: {', '.join(unknown)}")

label = fields.get("label", "")
if label in {".", ".."} or not re.fullmatch(r"[A-Za-z0-9_.-]+", label):
    raise SystemExit("invalid --node label: use only [A-Za-z0-9_.-]+")

mode = fields.get("mode") or ("remote" if fields.get("target") else "local")
if mode not in {"local", "remote"}:
    raise SystemExit("invalid --node mode: expected local or remote")

required = ["service", "status_url", "health_url", "env_file"]
missing = [key for key in required if not fields.get(key)]
if missing:
    raise SystemExit(f"missing --node keys for {label}: {', '.join(missing)}")
if mode == "remote" and not fields.get("target"):
    raise SystemExit(f"remote --node {label} requires target")
if mode == "local" and fields.get("target"):
    raise SystemExit(f"local --node {label} must not set target")

if fields.get("password_env") and not re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", fields["password_env"]):
    raise SystemExit("invalid password_env name")

node = {
    "label": label,
    "mode": mode,
    "target": fields.get("target", ""),
    "service": fields["service"],
    "status_url": fields["status_url"],
    "health_url": fields["health_url"],
    "env_file": fields["env_file"],
    "public_status_url": fields.get("public_status_url", ""),
    "public_health_url": fields.get("public_health_url", ""),
    "password_env": fields.get("password_env", ""),
}
print(json.dumps(node, separators=(",", ":")))
PY
}

node_password() {
  local password_env=$1
  if [[ -z "$password_env" ]]; then
    printf ''
    return 0
  fi
  printf '%s' "${!password_env:-}"
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

record_env_copy() {
  local label=$1
  local mode=$2
  local source=$3
  local password=${4:-}
  local node_dir="$nodes_root/$label"
  if [[ "$mode" == "local" ]]; then
    if [[ -r "$source" ]]; then
      if ! cp "$source" "$node_dir/node.env"; then
        printf 'failed to read local env file: %s\n' "$source" > "$node_dir/node.env.error.txt"
      fi
    elif [[ -f "$source" ]]; then
      printf 'local env file exists but is not readable: %s\n' "$source" > "$node_dir/node.env.error.txt"
    else
      printf 'missing local env file: %s\n' "$source" > "$node_dir/node.env.error.txt"
    fi
    return 0
  fi
  if run_ssh "$source" "$password" "cat '$5'" > "$node_dir/node.env" 2>"$node_dir/node.env.stderr.log"; then
    return 0
  fi
  printf 'failed to fetch remote env file from %s\n' "$source" > "$node_dir/node.env.error.txt"
}

capture_service_state() {
  local label=$1
  local mode=$2
  local service=$3
  local target=${4:-}
  local password=${5:-}
  local node_dir="$nodes_root/$label"
  local service_state_file="$node_dir/service_state.txt"
  if [[ "$mode" == "local" ]]; then
    if systemctl is-active "$service" >"$service_state_file" 2>"$node_dir/service_state.stderr.log"; then
      return 0
    fi
    return 0
  fi
  run_ssh "$target" "$password" "systemctl is-active '$service'" >"$service_state_file" 2>"$node_dir/service_state.stderr.log" || true
}

capture_health_and_status() {
  local label=$1
  local mode=$2
  local health_url=$3
  local status_url=$4
  local sample_index=$5
  local target=${6:-}
  local password=${7:-}
  local public_health_url=${8:-}
  local public_status_url=${9:-}
  local node_dir="$nodes_root/$label"
  local sample_dir="$node_dir/samples/sample-$(printf '%03d' "$sample_index")"
  mkdir -p "$sample_dir"

  if [[ "$mode" == "local" ]]; then
    if curl -fsS --max-time "$CURL_MAX_TIME_SECS" "$health_url" >"$sample_dir/healthz.json" 2>"$sample_dir/healthz.stderr.log"; then
      cp "$sample_dir/healthz.json" "$node_dir/healthz.json"
    else
      printf '{"ok":false,"fetch_error":"curl_failed"}\n' >"$sample_dir/healthz.json"
    fi
    if curl -fsS --max-time "$CURL_MAX_TIME_SECS" "$status_url" >"$sample_dir/status.json" 2>"$sample_dir/status.stderr.log"; then
      cp "$sample_dir/status.json" "$node_dir/status.json"
    else
      printf '{"ok":false,"fetch_error":"curl_failed"}\n' >"$sample_dir/status.json"
    fi
    return 0
  fi

  if run_ssh "$target" "$password" "curl -fsS --max-time '$CURL_MAX_TIME_SECS' '$health_url'" >"$sample_dir/healthz.json" 2>"$sample_dir/healthz.stderr.log"; then
    cp "$sample_dir/healthz.json" "$node_dir/healthz.json"
  elif [[ -n "$public_health_url" ]] && curl -fsS --max-time "$CURL_MAX_TIME_SECS" "$public_health_url" >"$sample_dir/healthz.json" 2>"$sample_dir/healthz.public.stderr.log"; then
    printf 'ssh_failed_public_fallback_used\n' >"$sample_dir/healthz.fallback.txt"
    cp "$sample_dir/healthz.json" "$node_dir/healthz.json"
  else
    printf '{"ok":false,"fetch_error":"ssh_or_curl_failed"}\n' >"$sample_dir/healthz.json"
  fi

  if run_ssh "$target" "$password" "curl -fsS --max-time '$CURL_MAX_TIME_SECS' '$status_url'" >"$sample_dir/status.json" 2>"$sample_dir/status.stderr.log"; then
    cp "$sample_dir/status.json" "$node_dir/status.json"
  elif [[ -n "$public_status_url" ]] && curl -fsS --max-time "$CURL_MAX_TIME_SECS" "$public_status_url" >"$sample_dir/status.json" 2>"$sample_dir/status.public.stderr.log"; then
    printf 'ssh_failed_public_fallback_used\n' >"$sample_dir/status.fallback.txt"
    cp "$sample_dir/status.json" "$node_dir/status.json"
  else
    printf '{"ok":false,"fetch_error":"ssh_or_curl_failed"}\n' >"$sample_dir/status.json"
  fi
}

normalize_responses() {
  local sample_dir="$nodes_root/$1/samples/sample-$(printf '%03d' "$sample_index")"
  python3 - "$sample_dir" <<'PYRESP'
import json, sys
from pathlib import Path
root = Path(sys.argv[1])
for name in ('healthz', 'status'):
    path = root / (name + '.json')
    try:
        value = json.loads(path.read_text())
        if not isinstance(value, dict):
            raise ValueError()
        if 'fetch_error' in value:
            raise ValueError()
        if name == 'healthz':
            if type(value.get('ok')) is not bool:
                raise ValueError()
        else:
            if not all(isinstance(value.get(k), str) and value[k] for k in ('node_id', 'world_id')) or type(value.get('running')) is not bool:
                raise ValueError()
    except (ValueError, OSError, TypeError):
        value = {'ok': False, 'fetch_error': 'response_unavailable_or_invalid'}
    path.write_text(json.dumps(value) + '\n')
    (root.parent.parent / (name + '.json')).write_text(json.dumps(value) + '\n')
PYRESP
}

append_sample_record() {
  local label=$1
  local service=$2
  local service_state_file="$nodes_root/$label/service_state.txt"
  local sample_dir="$nodes_root/$label/samples/sample-$(printf '%03d' "$sample_index")"
  local service_state="unknown"
  if [[ -f "$service_state_file" ]]; then
    service_state=$(tr -d '\r' < "$service_state_file" | tail -n 1)
  fi

  jq -n \
    --arg label "$label" \
    --arg service "$service" \
    --arg sample_index "$sample_index" \
    --arg captured_at "$captured_at" \
    --arg expected_world_id "$world_id" \
    --arg service_state "$service_state" \
    --slurpfile health "$sample_dir/healthz.json" \
    --slurpfile status "$sample_dir/status.json" \
    '{
      sample_index: ($sample_index | tonumber),
      captured_at: $captured_at,
      label: $label,
      service: $service,
      service_state: $service_state,
      expected_world_id: $expected_world_id,
      healthz_ok: ($health[0].ok // false),
      healthz_fetch_ok: (($health[0].fetch_error // null) == null),
      healthz_fetch_error: ($health[0].fetch_error // null),
      status_fetch_ok: (($status[0].fetch_error // null) == null),
      status_fetch_error: ($status[0].fetch_error // null),
      node_id: ($status[0].node_id // null),
      world_id: ($status[0].world_id // null),
      role: ($status[0].role // null),
      running: ($status[0].running // null),
      liveness: {
        status: ($status[0].liveness.status // null),
        running: ($status[0].liveness.running // null),
        runtime_last_error: ($status[0].liveness.runtime_last_error // null)
      },
      readiness: {
        status: ($status[0].readiness.status // null),
        ready: ($status[0].readiness.ready // false),
        failed_gates: ($status[0].readiness.failed_gates // []),
        policy: {
          tier: ($status[0].readiness.policy.tier // null),
          role: ($status[0].readiness.policy.role // null),
          peer_head_ttl_ms: ($status[0].readiness.policy.peer_head_ttl_ms // null),
          max_network_height_lag: ($status[0].readiness.policy.max_network_height_lag // null),
          sync_stalled_after_ms: ($status[0].readiness.policy.sync_stalled_after_ms // null),
          quorum_mode: ($status[0].readiness.policy.quorum_mode // null),
          relay_policy: ($status[0].readiness.policy.relay_policy // null),
          slashing_policy: ($status[0].readiness.policy.slashing_policy // null),
          slashing_enforced: ($status[0].readiness.policy.slashing_enforced // false)
        }
      },
      sync: {
        status: ($status[0].sync.status // null),
        network_head_source: ($status[0].sync.network_head_source // null),
        network_height_lag: ($status[0].sync.network_height_lag // null),
        fresh_peer_count: ($status[0].sync.fresh_peer_count // null),
        stale_peer_count: ($status[0].sync.stale_peer_count // null),
        conflicting_peer_count: ($status[0].sync.conflicting_peer_count // null)
      },
      observability: {
        status: ($status[0].observability.status // null),
        ready: ($status[0].observability.ready // false),
        network_head_available: ($status[0].observability.network_head_available // false),
        transport_stable: ($status[0].observability.transport_stable // false),
        transport_stability_score: ($status[0].observability.transport_stability_score // null),
        reachability_policy_ok: ($status[0].observability.reachability_policy_ok // false),
        misbehavior_evidence_count: ($status[0].observability.misbehavior_evidence_count // 0),
        quarantined_validator_count: ($status[0].observability.quarantined_validator_count // 0),
        slashable_stake_total: ($status[0].observability.slashable_stake_total // 0),
        storage_challenge_network_degraded: ($status[0].observability.storage_challenge_network_degraded // false),
        active_peer_count: ($status[0].observability.active_peer_count // null),
        known_peer_heads: ($status[0].observability.known_peer_heads // null),
        alerts: ($status[0].observability.alerts // [])
      },
      last_error: ($status[0].last_error // null),
      reward_runtime_last_error: ($status[0].reward_runtime.last_error // null),
      consensus: {
        slot: ($status[0].consensus.slot // null),
        latest_height: ($status[0].consensus.latest_height // null),
        committed_height: ($status[0].consensus.committed_height // null),
        network_committed_height: ($status[0].consensus.network_committed_height // null),
        known_peer_heads: ($status[0].consensus.known_peer_heads // null),
        validator_set_hash: ($status[0].consensus.validator_set_hash // null),
        validator_stake_root: ($status[0].consensus.validator_stake_root // null),
        validator_stake_proof_count: ($status[0].consensus.validator_stake_proof_count // 0),
        misbehavior_evidence_count: ($status[0].consensus.misbehavior_evidence_count // 0),
        slashing_intent_count: ($status[0].consensus.slashing_intent_count // 0),
        pending_slashing_intent_count: ($status[0].consensus.pending_slashing_intent_count // 0),
        slashing_receipt_count: ($status[0].consensus.slashing_receipt_count // 0),
        applied_slashing_receipt_count: ($status[0].consensus.applied_slashing_receipt_count // 0),
        quarantined_validator_count: ($status[0].consensus.quarantined_validator_count // 0),
        slashable_stake_total: ($status[0].consensus.slashable_stake_total // 0),
        missed_slot_count: ($status[0].consensus.missed_slot_count // null),
        missed_tick_count: ($status[0].consensus.missed_tick_count // null),
        storage_challenge_network_degraded_height: ($status[0].consensus.storage_challenge_network_degraded_height // null),
        storage_challenge_network_degraded_reason: ($status[0].consensus.storage_challenge_network_degraded_reason // null),
        storage_challenge_network_last_probe_at_ms: ($status[0].consensus.storage_challenge_network_last_probe_at_ms // null),
        storage_challenge_network_next_probe_after_ms: ($status[0].consensus.storage_challenge_network_next_probe_after_ms // null),
        network_head: {
          source: ($status[0].consensus.network_head.source // null),
          decision: ($status[0].consensus.network_head.decision // null),
          height: ($status[0].consensus.network_head.height // null),
          block_hash: ($status[0].consensus.network_head.block_hash // null),
          execution_state_root: ($status[0].consensus.network_head.execution_state_root // null),
          observed_peer_count: ($status[0].consensus.network_head.observed_peer_count // null),
          required_peer_count: ($status[0].consensus.network_head.required_peer_count // null),
          quorum_mode: ($status[0].consensus.network_head.quorum_mode // null),
          fresh_peer_count: ($status[0].consensus.network_head.fresh_peer_count // null),
          stale_peer_count: ($status[0].consensus.network_head.stale_peer_count // null),
          conflicting_peer_count: ($status[0].consensus.network_head.conflicting_peer_count // null),
          observed_stake: ($status[0].consensus.network_head.observed_stake // null),
          required_stake: ($status[0].consensus.network_head.required_stake // null),
          total_stake: ($status[0].consensus.network_head.total_stake // null),
          stake_quorum_met: ($status[0].consensus.network_head.stake_quorum_met // false),
          freshness_ttl_ms: ($status[0].consensus.network_head.freshness_ttl_ms // null)
        }
      },
      replication: {
        request_peer_scores: ($status[0].replication.request_peer_scores // {})
      }
    }' >> "$samples_ndjson"
}

samples=4
interval_secs=5
ssh_timeout_secs=8
out_root=".tmp/p2p_real_env_triad"
world_id="oasis7-unified-world-v1"

local_service="oasis7-triad-observer.service"
local_status_url="http://127.0.0.1:5633/v1/chain/status"
local_health_url="http://127.0.0.1:5633/healthz"
local_env_file="/opt/oasis7/p2p-triad-local/config/node.env"

sequencer_target="root@39.104.204.172"
sequencer_service="oasis7-triad-sequencer.service"
sequencer_status_url="http://127.0.0.1:5631/v1/chain/status"
sequencer_health_url="http://127.0.0.1:5631/healthz"
sequencer_public_status_url=""
sequencer_public_health_url=""
sequencer_env_file="/opt/oasis7/p2p-triad/config/node.env"

storage_target="root@39.104.205.67"
storage_service="oasis7-triad-storage.service"
storage_status_url="http://127.0.0.1:5632/v1/chain/status"
storage_health_url="http://127.0.0.1:5632/healthz"
storage_public_status_url=""
storage_public_health_url=""
storage_env_file="/opt/oasis7/p2p-triad/config/node.env"
explicit_node_specs=()

while [[ $# -gt 0 ]]; do
  if [[ "$1" != "--help" && "$1" != "-h" ]] && { [[ $# -lt 2 || -z "$2" || "$2" == --* ]]; }; then
    echo "missing or empty value for $1" >&2; exit 2
  fi
  case "$1" in
    --samples)
      samples=$(normalize_positive_int --samples "$2")
      shift 2
      ;;
    --interval-secs)
      interval_secs=$(normalize_positive_int --interval-secs "$2")
      shift 2
      ;;
    --ssh-timeout-secs)
      ssh_timeout_secs=$(normalize_positive_int --ssh-timeout-secs "$2")
      shift 2
      ;;
    --out-dir)
      out_root=${2:-}
      shift 2
      ;;
    --world-id)
      world_id=${2:-}
      shift 2
      ;;
    --node)
      explicit_node_specs+=("$(parse_node_spec "${2:-}")")
      shift 2
      ;;
    --local-service)
      local_service=${2:-}
      shift 2
      ;;
    --local-status-url)
      local_status_url=${2:-}
      shift 2
      ;;
    --local-health-url)
      local_health_url=${2:-}
      shift 2
      ;;
    --local-env-file)
      local_env_file=${2:-}
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
    --sequencer-status-url)
      sequencer_status_url=${2:-}
      shift 2
      ;;
    --sequencer-health-url)
      sequencer_health_url=${2:-}
      shift 2
      ;;
    --sequencer-public-status-url)
      sequencer_public_status_url=${2:-}
      shift 2
      ;;
    --sequencer-public-health-url)
      sequencer_public_health_url=${2:-}
      shift 2
      ;;
    --sequencer-env-file)
      sequencer_env_file=${2:-}
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
    --storage-status-url)
      storage_status_url=${2:-}
      shift 2
      ;;
    --storage-health-url)
      storage_health_url=${2:-}
      shift 2
      ;;
    --storage-public-status-url)
      storage_public_status_url=${2:-}
      shift 2
      ;;
    --storage-public-health-url)
      storage_public_health_url=${2:-}
      shift 2
      ;;
    --storage-env-file)
      storage_env_file=${2:-}
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

# All CLI validation precedes the first durable write or external probe.
if (( ${#explicit_node_specs[@]} > 0 )); then
  duplicate_labels=$(printf '%s\n' "${explicit_node_specs[@]}" | jq -sr 'group_by(.label)[] | select(length > 1) | .[0].label')
  if [[ -n "$duplicate_labels" ]]; then echo "duplicate --node label" >&2; exit 2; fi
fi
attempt_helper="$repo_root/scripts/p2p-real-env-observability-attempt.py"
scope_args=()
for arg in ${original_args[@]+"${original_args[@]}"}; do scope_args+=("--scope-arg=$arg"); done
reservation=$(python3 "$attempt_helper" begin --collector snapshot --out-dir "$out_root" ${scope_args[@]+"${scope_args[@]}"})
sequence=$(jq -r '.sequence' <<<"$reservation")
attempt_id=$(jq -r '.attempt_id' <<<"$reservation")
run_dir=$(jq -r '.run_dir' <<<"$reservation")
run_id="$sequence-$attempt_id"
nodes_root="$run_dir/nodes"
samples_ndjson="$run_dir/samples.ndjson"
summary_json="$run_dir/summary.json"
summary_md="$run_dir/summary.md"
config_json="$run_dir/config.json"
nodes_json="$run_dir/nodes.json"
node_labels_file="$run_dir/node-labels.txt"
mkdir -p "$nodes_root" "$run_dir/report"
: > "$samples_ndjson"
if (( ${#explicit_node_specs[@]} > 0 )); then
  printf '%s\n' "${explicit_node_specs[@]}" | jq -s '.' > "$nodes_json"
else
  jq -n \
    --arg local_service "$local_service" \
    --arg local_status_url "$local_status_url" \
    --arg local_health_url "$local_health_url" \
    --arg local_env_file "$local_env_file" \
    --arg sequencer_target "$sequencer_target" \
    --arg sequencer_service "$sequencer_service" \
    --arg sequencer_status_url "$sequencer_status_url" \
    --arg sequencer_health_url "$sequencer_health_url" \
    --arg sequencer_public_status_url "$sequencer_public_status_url" \
    --arg sequencer_public_health_url "$sequencer_public_health_url" \
    --arg sequencer_env_file "$sequencer_env_file" \
    --arg storage_target "$storage_target" \
    --arg storage_service "$storage_service" \
    --arg storage_status_url "$storage_status_url" \
    --arg storage_health_url "$storage_health_url" \
    --arg storage_public_status_url "$storage_public_status_url" \
    --arg storage_public_health_url "$storage_public_health_url" \
    --arg storage_env_file "$storage_env_file" \
    '[
      {
        label: "local_node",
        mode: "local",
        target: "",
        service: $local_service,
        status_url: $local_status_url,
        health_url: $local_health_url,
        env_file: $local_env_file,
        public_status_url: "",
        public_health_url: "",
        password_env: ""
      },
      {
        label: "sequencer_ecs",
        mode: "remote",
        target: $sequencer_target,
        service: $sequencer_service,
        status_url: $sequencer_status_url,
        health_url: $sequencer_health_url,
        public_status_url: $sequencer_public_status_url,
        public_health_url: $sequencer_public_health_url,
        env_file: $sequencer_env_file,
        password_env: "P2PARCH6_SEQ_SSH_PASSWORD"
      },
      {
        label: "storage_ecs",
        mode: "remote",
        target: $storage_target,
        service: $storage_service,
        status_url: $storage_status_url,
        health_url: $storage_health_url,
        public_status_url: $storage_public_status_url,
        public_health_url: $storage_public_health_url,
        env_file: $storage_env_file,
        password_env: "P2PARCH6_STORAGE_SSH_PASSWORD"
      }
    ]' > "$nodes_json"
fi

jq -r '.[].label' "$nodes_json" > "$node_labels_file"
duplicate_labels=$(jq -r 'group_by(.label)[] | select(length > 1) | .[0].label' "$nodes_json")
if [[ -n "$duplicate_labels" ]]; then
  printf 'duplicate --node label(s):\n%s\n' "$duplicate_labels" >&2
  exit 2
fi
while IFS= read -r label; do
  mkdir -p "$nodes_root/$label/samples"
done < "$node_labels_file"

jq -n \
  --arg run_id "$run_id" \
  --arg run_dir "$run_dir" \
  --arg world_id "$world_id" \
  --slurpfile nodes "$nodes_json" \
  --argjson explicit_node_mode "$([[ ${#explicit_node_specs[@]} -gt 0 ]] && echo true || echo false)" \
  --argjson samples "$samples" \
  --argjson interval_secs "$interval_secs" \
  --argjson ssh_timeout_secs "$ssh_timeout_secs" \
  '{
    run_id: $run_id,
    run_dir: $run_dir,
    world_id: $world_id,
    samples: $samples,
    interval_secs: $interval_secs,
    ssh_timeout_secs: $ssh_timeout_secs,
    explicit_node_mode: $explicit_node_mode,
    nodes: ($nodes[0] | map({(.label): .}) | add)
  }' > "$config_json"

while IFS= read -r node; do
  label=$(jq -r '.label' <<<"$node")
  mode=$(jq -r '.mode' <<<"$node")
  target=$(jq -r '.target' <<<"$node")
  env_file=$(jq -r '.env_file' <<<"$node")
  password=$(node_password "$(jq -r '.password_env' <<<"$node")")
  if [[ "$mode" == "remote" ]]; then
    record_env_copy "$label" "$mode" "$target" "$password" "$env_file"
  else
    record_env_copy "$label" "$mode" "$env_file"
  fi
done < <(jq -c '.[]' "$nodes_json")

while IFS= read -r node; do
  label=$(jq -r '.label' <<<"$node")
  mode=$(jq -r '.mode' <<<"$node")
  target=$(jq -r '.target' <<<"$node")
  service=$(jq -r '.service' <<<"$node")
  password=$(node_password "$(jq -r '.password_env' <<<"$node")")
  capture_service_state "$label" "$mode" "$service" "$target" "$password"
done < <(jq -c '.[]' "$nodes_json")

started_at=$(date -Iseconds)

for ((sample_index = 1; sample_index <= samples; sample_index++)); do
  captured_at=$(date -Iseconds)
  echo "sample $sample_index/$samples @ $captured_at"

  while IFS= read -r node; do
    label=$(jq -r '.label' <<<"$node")
    mode=$(jq -r '.mode' <<<"$node")
    target=$(jq -r '.target' <<<"$node")
    service=$(jq -r '.service' <<<"$node")
    health_url=$(jq -r '.health_url' <<<"$node")
    status_url=$(jq -r '.status_url' <<<"$node")
    public_health_url=$(jq -r '.public_health_url' <<<"$node")
    public_status_url=$(jq -r '.public_status_url' <<<"$node")
    password=$(node_password "$(jq -r '.password_env' <<<"$node")")
    capture_health_and_status "$label" "$mode" "$health_url" "$status_url" "$sample_index" "$target" "$password" "$public_health_url" "$public_status_url"
    normalize_responses "$label"
    append_sample_record "$label" "$service"
  done < <(jq -c '.[]' "$nodes_json")

  if (( sample_index < samples )); then
    sleep "$interval_secs"
  fi
done

ended_at=$(date -Iseconds)

jq -s \
  --arg started_at "$started_at" \
  --arg ended_at "$ended_at" \
  --arg run_id "$run_id" \
  --arg run_dir "$run_dir" \
  --arg world_id "$world_id" \
  --argjson explicit_node_mode "$([[ ${#explicit_node_specs[@]} -gt 0 ]] && echo true || echo false)" \
  '
  def heights_for($label):
    map(select(.label == $label) | .consensus.committed_height // 0);
  def values_for($label; $path):
    map(select(.label == $label) | getpath($path));
  def first_or_zero($arr):
    if ($arr | length) == 0 then 0 else ($arr[0] // 0) end;
  def last_or_zero($arr):
    if ($arr | length) == 0 then 0 else ($arr[-1] // 0) end;
  def max_or_zero($arr):
    if ($arr | length) == 0 then 0 else ($arr | map(. // 0) | max) end;
  def min_or_zero($arr):
    if ($arr | length) == 0 then 0 else ($arr | map(. // 0) | min) end;
  def all_true_or_false($arr):
    if ($arr | length) == 0 then false else ($arr | all(. == true)) end;
  def node_summary($label):
    {
      label: $label,
      sample_count: (map(select(.label == $label)) | length),
      success_sample_count: (map(select(.label == $label and .healthz_fetch_ok == true and .status_fetch_ok == true)) | length),
      failed_sample_count: (map(select(.label == $label and (.healthz_fetch_ok != true or .status_fetch_ok != true))) | length),
      service_states: (map(select(.label == $label) | .service_state) | unique),
      healthz_all_ok: (map(select(.label == $label) | .healthz_ok) | all(. == true)),
      status_fetch_all_ok: (map(select(.label == $label) | .status_fetch_ok) | all(. == true)),
      world_ids: (map(select(.label == $label) | .world_id) | unique),
      node_ids: (map(select(.label == $label) | .node_id) | unique),
      roles: (map(select(.label == $label) | .role) | unique),
      liveness_statuses: (map(select(.label == $label) | .liveness.status) | unique),
      readiness_statuses: (map(select(.label == $label) | .readiness.status) | unique),
      sync_statuses: (map(select(.label == $label) | .sync.status) | unique),
      service_active: ((map(select(.label == $label) | .service_state) | unique | index("active")) != null),
      readiness_policies: {
        tiers: (map(select(.label == $label) | .readiness.policy.tier) | unique),
        quorum_modes: (map(select(.label == $label) | .readiness.policy.quorum_mode) | unique),
        relay_policies: (map(select(.label == $label) | .readiness.policy.relay_policy) | unique),
        slashing_policies: (map(select(.label == $label) | .readiness.policy.slashing_policy) | unique),
        slashing_enforced_all: all_true_or_false(map(select(.label == $label) | .readiness.policy.slashing_enforced))
      },
      readiness_all_ready: all_true_or_false(map(select(.label == $label) | .readiness.ready)),
      observability_ready_all: all_true_or_false(map(select(.label == $label) | .observability.ready)),
      transport_stable_all: all_true_or_false(map(select(.label == $label) | .observability.transport_stable)),
      reachability_policy_ok_all: all_true_or_false(map(select(.label == $label) | .observability.reachability_policy_ok)),
      storage_challenge_network_degraded_any: (
        map(select(.label == $label) | (
          (.observability.storage_challenge_network_degraded // false)
          or ((.consensus.storage_challenge_network_degraded_height // null) != null)
        ))
        | any(. == true)
      ),
      storage_challenge_network_degraded_heights: (
        map(select(.label == $label) | .consensus.storage_challenge_network_degraded_height)
        | map(select(. != null))
        | unique
      ),
      storage_challenge_network_degraded_reasons: (
        map(select(.label == $label) | .consensus.storage_challenge_network_degraded_reason)
        | map(select(. != null))
        | unique
      ),
      last_errors: (map(select(.label == $label) | .last_error) | unique),
      heights: {
        first_committed_height: first_or_zero(heights_for($label)),
        last_committed_height: last_or_zero(heights_for($label)),
        max_committed_height: max_or_zero(heights_for($label)),
        min_committed_height: min_or_zero(heights_for($label))
      },
      network: {
        first_network_committed_height: first_or_zero(values_for($label; ["consensus","network_committed_height"])),
        last_network_committed_height: last_or_zero(values_for($label; ["consensus","network_committed_height"])),
        max_network_committed_height: max_or_zero(values_for($label; ["consensus","network_committed_height"]))
      },
      peers: {
        min_known_peer_heads: min_or_zero(values_for($label; ["consensus","known_peer_heads"])),
        max_known_peer_heads: max_or_zero(values_for($label; ["consensus","known_peer_heads"])),
        validator_set_hashes: (map(select(.label == $label) | .consensus.validator_set_hash) | unique),
        validator_stake_roots: (map(select(.label == $label) | .consensus.validator_stake_root) | unique),
        min_validator_stake_proof_count: min_or_zero(values_for($label; ["consensus","validator_stake_proof_count"])),
        max_validator_stake_proof_count: max_or_zero(values_for($label; ["consensus","validator_stake_proof_count"])),
        max_misbehavior_evidence_count: max_or_zero(values_for($label; ["consensus","misbehavior_evidence_count"])),
        max_slashing_intent_count: max_or_zero(values_for($label; ["consensus","slashing_intent_count"])),
        max_pending_slashing_intent_count: max_or_zero(values_for($label; ["consensus","pending_slashing_intent_count"])),
        max_slashing_receipt_count: max_or_zero(values_for($label; ["consensus","slashing_receipt_count"])),
        max_applied_slashing_receipt_count: max_or_zero(values_for($label; ["consensus","applied_slashing_receipt_count"])),
        max_quarantined_validator_count: max_or_zero(values_for($label; ["consensus","quarantined_validator_count"])),
        max_slashable_stake_total: max_or_zero(values_for($label; ["consensus","slashable_stake_total"]))
      },
      network_head: {
        sources: (map(select(.label == $label) | .consensus.network_head.source) | unique),
        decisions: (map(select(.label == $label) | .consensus.network_head.decision) | unique),
        quorum_modes: (map(select(.label == $label) | .consensus.network_head.quorum_mode) | unique),
        min_fresh_peer_count: min_or_zero(values_for($label; ["consensus","network_head","fresh_peer_count"])),
        max_fresh_peer_count: max_or_zero(values_for($label; ["consensus","network_head","fresh_peer_count"])),
        max_stale_peer_count: max_or_zero(values_for($label; ["consensus","network_head","stale_peer_count"])),
        max_conflicting_peer_count: max_or_zero(values_for($label; ["consensus","network_head","conflicting_peer_count"])),
        max_required_peer_count: max_or_zero(values_for($label; ["consensus","network_head","required_peer_count"])),
        max_observed_stake: max_or_zero(values_for($label; ["consensus","network_head","observed_stake"])),
        max_required_stake: max_or_zero(values_for($label; ["consensus","network_head","required_stake"])),
        max_total_stake: max_or_zero(values_for($label; ["consensus","network_head","total_stake"])),
        stake_quorum_all_met: all_true_or_false(map(select(.label == $label) | .consensus.network_head.stake_quorum_met))
      }
    };
  def all_roles_equal($label; $role):
    (.nodes[$label].roles | length) > 0 and (.nodes[$label].roles | all(. == $role));
  def node_summaries:
    . as $rows
    | ($rows | map(.label) | unique) as $labels
    | reduce $labels[] as $label ({}; .[$label] = ($rows | node_summary($label)));
  def generic_all($path; $value):
    (.nodes | to_entries | map(.value | getpath($path) == $value) | all(. == true));
  def generic_any($path; $value):
    (.nodes | to_entries | map(.value | getpath($path) == $value) | any(. == true));
  . as $samples
  | {
      run_id: $run_id,
      run_dir: $run_dir,
      world_id: $world_id,
      started_at: $started_at,
      ended_at: $ended_at,
      totals: {
        sample_record_count: length,
        node_count: (map(.label) | unique | length)
      },
      nodes: node_summaries,
      samples: $samples
    }
  | .generic_analysis = {
      all_healthz_ok: generic_all(["healthz_all_ok"]; true),
      all_status_fetch_ok: generic_all(["status_fetch_all_ok"]; true),
      all_services_active: generic_all(["service_active"]; true),
      all_nodes_ready: generic_all(["readiness_all_ready"]; true),
      all_transport_stable: generic_all(["transport_stable_all"]; true),
      all_reachability_policy_ok: generic_all(["reachability_policy_ok_all"]; true),
      any_storage_challenge_network_degraded: generic_any(["storage_challenge_network_degraded_any"]; true),
      any_last_error: (.nodes | to_entries | any(.value.last_errors | map(select(. != null)) | length > 0))
    }
  | if ($explicit_node_mode == true) then
      .analysis = { claim_mode: "explicit_nodes" }
      | .failure_signatures = (
          []
          + (if .generic_analysis.all_services_active then [] else ["node_service_inactive"] end)
          + (if .generic_analysis.all_healthz_ok then [] else ["node_healthz_unhealthy"] end)
          + (if .generic_analysis.all_status_fetch_ok then [] else ["node_status_fetch_failed"] end)
          + (if .generic_analysis.all_nodes_ready then [] else ["node_not_ready"] end)
          + (if .generic_analysis.all_transport_stable then [] else ["replication_transport_unstable"] end)
          + (if .generic_analysis.all_reachability_policy_ok then [] else ["p2p_reachability_degraded"] end)
          + (if .generic_analysis.any_storage_challenge_network_degraded then ["storage_challenge_network_degraded"] else [] end)
          + (if .generic_analysis.any_last_error then ["runtime_last_error"] else [] end)
        )
      | .claim_status = (
          if (.failure_signatures | length) == 0 then "pass_candidate"
          elif .generic_analysis.all_healthz_ok and .generic_analysis.all_status_fetch_ok then "partial_with_node_health_blocker"
          else "blocked"
          end
        )
    else
      .analysis = {
      local_service_healthy: (
        (.nodes.local_node.healthz_all_ok == true)
        and (.nodes.local_node.status_fetch_all_ok == true)
        and ((.nodes.local_node.service_states | index("active")) != null)
      ),
      local_chain_visible: (.nodes.local_node.heights.max_committed_height > 0),
      local_runtime_role_observer: all_roles_equal("local_node"; "observer"),
      local_runtime_role_sequencer: all_roles_equal("local_node"; "sequencer"),
      local_peer_visibility_ok: (.nodes.local_node.peers.max_known_peer_heads > 0),
      local_network_commit_visible: (.nodes.local_node.network.max_network_committed_height > 0),
      local_committed_height_progressing: (.nodes.local_node.heights.last_committed_height > .nodes.local_node.heights.first_committed_height),
      cloud_pair_service_healthy: (
        (.nodes.sequencer_ecs.healthz_all_ok == true)
        and (.nodes.storage_ecs.healthz_all_ok == true)
        and (.nodes.sequencer_ecs.status_fetch_all_ok == true)
        and (.nodes.storage_ecs.status_fetch_all_ok == true)
        and ((.nodes.sequencer_ecs.service_states | index("active")) != null)
        and ((.nodes.storage_ecs.service_states | index("active")) != null)
      ),
      sequencer_chain_visible: (.nodes.sequencer_ecs.heights.max_committed_height > 0),
      storage_chain_visible: (.nodes.storage_ecs.heights.max_committed_height > 0),
      triad_service_healthy: (
        (.nodes.local_node.healthz_all_ok == true)
        and (.nodes.local_node.status_fetch_all_ok == true)
        and ((.nodes.local_node.service_states | index("active")) != null)
        and (.nodes.sequencer_ecs.healthz_all_ok == true)
        and (.nodes.storage_ecs.healthz_all_ok == true)
        and (.nodes.sequencer_ecs.status_fetch_all_ok == true)
        and (.nodes.storage_ecs.status_fetch_all_ok == true)
        and ((.nodes.sequencer_ecs.service_states | index("active")) != null)
        and ((.nodes.storage_ecs.service_states | index("active")) != null)
      ),
      triad_chain_visible: (
        (.nodes.local_node.heights.max_committed_height > 0)
        and (.nodes.sequencer_ecs.heights.max_committed_height > 0)
        and (.nodes.storage_ecs.heights.max_committed_height > 0)
      ),
      triad_nodes_ready: (
        (.nodes.local_node.readiness_all_ready == true)
        and (.nodes.sequencer_ecs.readiness_all_ready == true)
        and (.nodes.storage_ecs.readiness_all_ready == true)
      ),
      triad_transport_stable: (
        (.nodes.local_node.transport_stable_all == true)
        and (.nodes.sequencer_ecs.transport_stable_all == true)
        and (.nodes.storage_ecs.transport_stable_all == true)
      ),
      triad_reachability_policy_ok: (
        (.nodes.local_node.reachability_policy_ok_all == true)
        and (.nodes.sequencer_ecs.reachability_policy_ok_all == true)
        and (.nodes.storage_ecs.reachability_policy_ok_all == true)
      ),
      network_head_conflict: (
        (.nodes.local_node.network_head.max_conflicting_peer_count > 0)
        or (.nodes.sequencer_ecs.network_head.max_conflicting_peer_count > 0)
        or (.nodes.storage_ecs.network_head.max_conflicting_peer_count > 0)
      ),
      sync_stalled: (
        ((.nodes.local_node.sync_statuses | index("stalled")) != null)
        or ((.nodes.sequencer_ecs.sync_statuses | index("stalled")) != null)
        or ((.nodes.storage_ecs.sync_statuses | index("stalled")) != null)
      ),
      stake_quorum_ready: (
        (
          (.nodes.local_node.network_head.quorum_modes | index("stake_weighted")) == null
          or (.nodes.local_node.network_head.stake_quorum_all_met == true)
        )
        and (
          (.nodes.sequencer_ecs.network_head.quorum_modes | index("stake_weighted")) == null
          or (.nodes.sequencer_ecs.network_head.stake_quorum_all_met == true)
        )
        and (
          (.nodes.storage_ecs.network_head.quorum_modes | index("stake_weighted")) == null
          or (.nodes.storage_ecs.network_head.stake_quorum_all_met == true)
        )
      ),
      stake_quorum_unavailable: (
        ((.nodes.local_node.readiness_policies.quorum_modes | index("count_fallback_stake_unavailable")) != null)
        or ((.nodes.sequencer_ecs.readiness_policies.quorum_modes | index("count_fallback_stake_unavailable")) != null)
        or ((.nodes.storage_ecs.readiness_policies.quorum_modes | index("count_fallback_stake_unavailable")) != null)
      ),
      validator_stake_proof_unavailable: (
        (
          (.nodes.local_node.network_head.max_total_stake > 0)
          and (.nodes.local_node.peers.min_validator_stake_proof_count == 0)
        )
        or (
          (.nodes.sequencer_ecs.network_head.max_total_stake > 0)
          and (.nodes.sequencer_ecs.peers.min_validator_stake_proof_count == 0)
        )
        or (
          (.nodes.storage_ecs.network_head.max_total_stake > 0)
          and (.nodes.storage_ecs.peers.min_validator_stake_proof_count == 0)
        )
      ),
      mainnet_slashing_evidence_only: (
        ((.nodes.local_node.readiness_policies.slashing_policies | index("evidence_only_readiness_gate")) != null)
        or ((.nodes.sequencer_ecs.readiness_policies.slashing_policies | index("evidence_only_readiness_gate")) != null)
        or ((.nodes.storage_ecs.readiness_policies.slashing_policies | index("evidence_only_readiness_gate")) != null)
      ),
      misbehavior_evidence_present: (
        (.nodes.local_node.peers.max_misbehavior_evidence_count > 0)
        or (.nodes.sequencer_ecs.peers.max_misbehavior_evidence_count > 0)
        or (.nodes.storage_ecs.peers.max_misbehavior_evidence_count > 0)
      ),
      validator_quarantined: (
        (.nodes.local_node.peers.max_quarantined_validator_count > 0)
        or (.nodes.sequencer_ecs.peers.max_quarantined_validator_count > 0)
        or (.nodes.storage_ecs.peers.max_quarantined_validator_count > 0)
      ),
      slashing_intent_pending: (
        (.nodes.local_node.peers.max_pending_slashing_intent_count > 0)
        or (.nodes.sequencer_ecs.peers.max_pending_slashing_intent_count > 0)
        or (.nodes.storage_ecs.peers.max_pending_slashing_intent_count > 0)
      ),
      peer_head_quorum_ready: (
        (
          (.nodes.local_node.network_head.max_required_peer_count == 0)
          or (.nodes.local_node.network_head.min_fresh_peer_count >= .nodes.local_node.network_head.max_required_peer_count)
        )
        and (
          (.nodes.sequencer_ecs.network_head.max_required_peer_count == 0)
          or (.nodes.sequencer_ecs.network_head.min_fresh_peer_count >= .nodes.sequencer_ecs.network_head.max_required_peer_count)
        )
        and (
          (.nodes.storage_ecs.network_head.max_required_peer_count == 0)
          or (.nodes.storage_ecs.network_head.min_fresh_peer_count >= .nodes.storage_ecs.network_head.max_required_peer_count)
        )
      ),
      cloud_pair_chain_visible: (
        (.nodes.sequencer_ecs.heights.max_committed_height > 0)
        and (.nodes.storage_ecs.heights.max_committed_height > 0)
      ),
      cloud_pair_progress_signal_present: (
        (.nodes.sequencer_ecs.heights.last_committed_height > .nodes.sequencer_ecs.heights.first_committed_height)
        or (.nodes.storage_ecs.heights.last_committed_height > .nodes.storage_ecs.heights.first_committed_height)
      ),
      triad_progress_signal_present: (
        (.nodes.sequencer_ecs.heights.last_committed_height > .nodes.sequencer_ecs.heights.first_committed_height)
        or (.nodes.storage_ecs.heights.last_committed_height > .nodes.storage_ecs.heights.first_committed_height)
        or (.nodes.local_node.heights.last_committed_height > .nodes.local_node.heights.first_committed_height)
      ),
      sequencer_execution_stale_height: (
        (.nodes.sequencer_ecs.last_errors | any(
          . != null and (. | test("execution driver received stale height"))
        ))
      ),
      triad_all_validator_roles: (
        all_roles_equal("local_node"; "sequencer")
        and all_roles_equal("sequencer_ecs"; "sequencer")
        and all_roles_equal("storage_ecs"; "sequencer")
      ),
      claim_mode: (
        if (
          all_roles_equal("local_node"; "sequencer")
          and all_roles_equal("sequencer_ecs"; "sequencer")
          and all_roles_equal("storage_ecs"; "sequencer")
        ) then "three_equal_validator"
        elif all_roles_equal("local_node"; "observer") then "observer_mixed_topology"
        else "transitional"
        end
      )
    }
  | .failure_signatures = (
      []
      + (if .analysis.local_service_healthy then [] else ["local_service_unhealthy"] end)
      + (if .analysis.cloud_pair_service_healthy then [] else ["cloud_pair_service_unhealthy"] end)
      + (
          if .analysis.cloud_pair_chain_visible then []
          elif ((.analysis.sequencer_chain_visible | not) and (.analysis.storage_chain_visible | not))
          then ["cloud_pair_chain_not_visible"]
          else []
          end
        )
      + (if .analysis.sequencer_chain_visible then [] else ["sequencer_committed_height_zero"] end)
      + (if .analysis.storage_chain_visible then [] else ["storage_committed_height_zero"] end)
      + (if .analysis.cloud_pair_progress_signal_present then [] else ["cloud_pair_no_recent_progress_signal"] end)
      + (if .analysis.triad_nodes_ready then [] else ["node_not_ready"] end)
      + (if .analysis.peer_head_quorum_ready then [] else ["peer_head_quorum_not_ready"] end)
      + (if .analysis.stake_quorum_ready then [] else ["stake_quorum_not_ready"] end)
      + (if .analysis.stake_quorum_unavailable then ["stake_quorum_unavailable"] else [] end)
      + (if .analysis.validator_stake_proof_unavailable then ["validator_stake_proof_unavailable"] else [] end)
      + (if .analysis.network_head_conflict then ["network_head_conflict"] else [] end)
      + (if .analysis.sync_stalled then ["sync_stalled"] else [] end)
      + (if .analysis.misbehavior_evidence_present then ["consensus_misbehavior_evidence_present"] else [] end)
      + (if .analysis.validator_quarantined then ["consensus_validator_quarantined"] else [] end)
      + (if .analysis.slashing_intent_pending then ["consensus_slashing_intent_pending"] else [] end)
      + (if .analysis.triad_transport_stable then [] else ["replication_transport_unstable"] end)
      + (if .analysis.triad_reachability_policy_ok then [] else ["p2p_reachability_degraded"] end)
      + (if .analysis.mainnet_slashing_evidence_only then ["mainnet_slashing_evidence_only"] else [] end)
      + (if .analysis.sequencer_execution_stale_height then ["sequencer_execution_stale_height"] else [] end)
      + (
          if .analysis.claim_mode == "observer_mixed_topology" then
            (if .analysis.local_peer_visibility_ok then [] else ["observer_known_peer_heads_zero"] end)
            + (if .analysis.local_network_commit_visible then [] else ["observer_network_committed_height_zero"] end)
            + (if .analysis.local_committed_height_progressing then [] else ["observer_committed_height_not_advancing"] end)
          elif .analysis.claim_mode == "three_equal_validator" then
            (if .analysis.local_chain_visible then [] else ["local_committed_height_zero"] end)
            + (if .analysis.local_peer_visibility_ok then [] else ["local_known_peer_heads_zero"] end)
            + (if .analysis.local_network_commit_visible then [] else ["local_network_committed_height_zero"] end)
            + (if .analysis.local_committed_height_progressing then [] else ["local_no_recent_progress_signal"] end)
          else
            (if .analysis.local_runtime_role_sequencer or .analysis.local_runtime_role_observer then [] else ["local_runtime_role_transitional"] end)
          end
        )
      + (if .analysis.triad_all_validator_roles or (.analysis.claim_mode != "three_equal_validator") then [] else ["triad_not_all_validator_roles"] end)
    )
  | .claim_status = (
      if .analysis.claim_mode == "three_equal_validator" then
        if .analysis.triad_service_healthy
           and .analysis.triad_chain_visible
           and .analysis.triad_nodes_ready
           and .analysis.triad_all_validator_roles
           and .analysis.local_peer_visibility_ok
           and .analysis.local_network_commit_visible
           and .analysis.local_committed_height_progressing
        then "pass_candidate"
        elif .analysis.cloud_pair_service_healthy
           and .analysis.cloud_pair_chain_visible
           and .analysis.triad_nodes_ready
           and .analysis.triad_all_validator_roles
        then "partial_with_local_validator_blocker"
        else "blocked"
        end
      elif .analysis.cloud_pair_service_healthy
           and .analysis.cloud_pair_chain_visible
           and .analysis.cloud_pair_progress_signal_present
           and .analysis.local_service_healthy
           and .analysis.triad_nodes_ready
           and .analysis.local_peer_visibility_ok
           and .analysis.local_network_commit_visible
           and .analysis.local_committed_height_progressing
      then "pass_candidate"
      elif .analysis.cloud_pair_service_healthy
           and .analysis.cloud_pair_chain_visible
           and .analysis.cloud_pair_progress_signal_present
      then "partial_with_observer_blocker"
      else "blocked"
      end
      )
    end
  ' "$samples_ndjson" > "$summary_json"

# Bind collection completion to the requested inventory, not only observed rows.
python3 - "$summary_json" "$config_json" <<'PYCOLLECTION'
import json, sys
from pathlib import Path
path = Path(sys.argv[1]); value = json.loads(path.read_text()); config = json.loads(Path(sys.argv[2]).read_text())
labels = list(config['nodes']); expected = len(labels) * config['samples']
failed = sum(n['failed_sample_count'] for n in value['nodes'].values())
complete = len(value['samples']) == expected and set(value['nodes']) == set(labels) and failed == 0
value['schema_version'] = 2
value['collection'] = dict(node_labels=labels, samples_per_node=config['samples'], expected_sample_count=expected, attempted_count=len(value['samples']), failed_sample_count=failed, complete=complete)
if not complete:
    value['claim_status'] = 'blocked'
    value['failure_signatures'].append('snapshot_collection_incomplete')
path.write_text(json.dumps(value, indent=2) + '\n')
PYCOLLECTION

{
  echo "# P2P Real-Environment Triad Snapshot"
  echo
  echo "- run_id: \`$run_id\`"
  echo "- started_at: \`$started_at\`"
  echo "- ended_at: \`$ended_at\`"
  echo "- world_id: \`$world_id\`"
  echo "- claim_status: \`$(jq -r '.claim_status' "$summary_json")\`"
  echo "- claim_mode: \`$(jq -r '.analysis.claim_mode' "$summary_json")\`"
  echo "- failure_signatures: \`$(jq -r '.failure_signatures | if length == 0 then "(none)" else join(", ") end' "$summary_json")\`"
  echo
  echo "## Node Summary"
  while IFS= read -r label; do
    echo "### \`$label\`"
    echo "- service_states: \`$(jq -r --arg label "$label" '.nodes[$label].service_states | join(", ")' "$summary_json")\`"
    echo "- healthz_all_ok: \`$(jq -r --arg label "$label" '.nodes[$label].healthz_all_ok' "$summary_json")\`"
    echo "- status_fetch_all_ok: \`$(jq -r --arg label "$label" '.nodes[$label].status_fetch_all_ok' "$summary_json")\`"
    echo "- node_ids: \`$(jq -r --arg label "$label" '.nodes[$label].node_ids | join(", ")' "$summary_json")\`"
    echo "- roles: \`$(jq -r --arg label "$label" '.nodes[$label].roles | join(", ")' "$summary_json")\`"
    echo "- liveness/readiness/sync: \`$(jq -r --arg label "$label" '.nodes[$label].liveness_statuses | join(", ")' "$summary_json") / $(jq -r --arg label "$label" '.nodes[$label].readiness_statuses | join(", ")' "$summary_json") / $(jq -r --arg label "$label" '.nodes[$label].sync_statuses | join(", ")' "$summary_json")\`"
    echo "- readiness_policy: \`tier=$(jq -r --arg label "$label" '.nodes[$label].readiness_policies.tiers | join(", ")' "$summary_json"), quorum=$(jq -r --arg label "$label" '.nodes[$label].readiness_policies.quorum_modes | join(", ")' "$summary_json"), relay=$(jq -r --arg label "$label" '.nodes[$label].readiness_policies.relay_policies | join(", ")' "$summary_json"), slashing=$(jq -r --arg label "$label" '.nodes[$label].readiness_policies.slashing_policies | join(", ")' "$summary_json")\`"
    echo "- readiness_all_ready: \`$(jq -r --arg label "$label" '.nodes[$label].readiness_all_ready' "$summary_json")\`"
    echo "- transport_stable_all: \`$(jq -r --arg label "$label" '.nodes[$label].transport_stable_all' "$summary_json")\`"
    echo "- reachability_policy_ok_all: \`$(jq -r --arg label "$label" '.nodes[$label].reachability_policy_ok_all' "$summary_json")\`"
    echo "- storage_challenge_network_degraded_any: \`$(jq -r --arg label "$label" '.nodes[$label].storage_challenge_network_degraded_any' "$summary_json")\`"
    echo "- storage_challenge_network_degraded_heights: \`$(jq -r --arg label "$label" '.nodes[$label].storage_challenge_network_degraded_heights | if length == 0 then "(none)" else join(", ") end' "$summary_json")\`"
    echo "- storage_challenge_network_degraded_reasons: \`$(jq -r --arg label "$label" '.nodes[$label].storage_challenge_network_degraded_reasons | if length == 0 then "(none)" else join(" | ") end' "$summary_json")\`"
    echo "- last_errors: \`$(jq -r --arg label "$label" '.nodes[$label].last_errors | map(select(. != null)) | if length == 0 then "(none)" else join(" | ") end' "$summary_json")\`"
    echo "- committed_height: \`$(jq -r --arg label "$label" '.nodes[$label].heights.first_committed_height' "$summary_json") -> $(jq -r --arg label "$label" '.nodes[$label].heights.last_committed_height' "$summary_json")\`"
    echo "- network_committed_height: \`$(jq -r --arg label "$label" '.nodes[$label].network.first_network_committed_height' "$summary_json") -> $(jq -r --arg label "$label" '.nodes[$label].network.last_network_committed_height' "$summary_json")\`"
    echo "- known_peer_heads: \`$(jq -r --arg label "$label" '.nodes[$label].peers.min_known_peer_heads' "$summary_json") -> $(jq -r --arg label "$label" '.nodes[$label].peers.max_known_peer_heads' "$summary_json")\`"
    echo "- network_head: \`source=$(jq -r --arg label "$label" '.nodes[$label].network_head.sources | join(", ")' "$summary_json"), decision=$(jq -r --arg label "$label" '.nodes[$label].network_head.decisions | join(", ")' "$summary_json"), quorum=$(jq -r --arg label "$label" '.nodes[$label].network_head.quorum_modes | join(", ")' "$summary_json"), fresh=$(jq -r --arg label "$label" '.nodes[$label].network_head.min_fresh_peer_count' "$summary_json")->$(jq -r --arg label "$label" '.nodes[$label].network_head.max_fresh_peer_count' "$summary_json"), required=$(jq -r --arg label "$label" '.nodes[$label].network_head.max_required_peer_count' "$summary_json"), stake=$(jq -r --arg label "$label" '.nodes[$label].network_head.max_observed_stake' "$summary_json")/$(jq -r --arg label "$label" '.nodes[$label].network_head.max_required_stake' "$summary_json")/$(jq -r --arg label "$label" '.nodes[$label].network_head.max_total_stake' "$summary_json"), stale_max=$(jq -r --arg label "$label" '.nodes[$label].network_head.max_stale_peer_count' "$summary_json"), conflict_max=$(jq -r --arg label "$label" '.nodes[$label].network_head.max_conflicting_peer_count' "$summary_json")\`"
  done < "$node_labels_file"
  echo
  echo "## Artifacts"
  echo "- config_json: \`$config_json\`"
  echo "- samples_ndjson: \`$samples_ndjson\`"
  echo "- summary_json: \`$summary_json\`"
} > "$summary_md"

echo "p2p real-env triad snapshot complete"
echo "  run_dir: $run_dir"
echo "  summary_json: $summary_json"
echo "  summary_md: $summary_md"

cp "$summary_json" "$run_dir/report/latest_summary.json"
cp "$summary_md" "$run_dir/report/latest_summary.md"
errors=()
if ! jq -e '.collection.complete == true' "$summary_json" >/dev/null; then errors+=(--error snapshot:1); fi
python3 "$attempt_helper" finish --collector snapshot --out-dir "$out_root" --sequence "$sequence" --attempt-id "$attempt_id" ${errors[@]+"${errors[@]}"}
