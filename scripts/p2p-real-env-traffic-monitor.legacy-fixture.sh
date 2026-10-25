#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

usage() {
  cat <<'USAGE'
Usage: ./scripts/p2p-real-env-traffic-monitor.sh [options]

Sample `/v1/chain/status.traffic` from the local node + ECS sequencer/storage
triad, persist samples into a reusable history file, and derive recent-window
traffic summaries from cumulative counters.

Options:
  --samples <n>                    number of samples to collect this run (default: 3)
  --interval-secs <n>              sleep between samples (default: 20)
  --as-of-unix-ms <n>              historical boundary (requires --summary-only; history read-only)
  --max-sample-age-secs <n>         maximum collector/producer age (default: 60)
  --max-sample-gap-secs <n>         maximum contiguous sample gap (default: 120)
  --window-minutes <n>             recent summary window in minutes (default: 10)
  --history-retention-minutes <n>  keep only recent history for summary+buffer
                                   (default: max(window+30, 120))
  --top-n <n>                      top protocol/topic/kind entries per node (default: 5)
  --ssh-timeout-secs <n>           SSH connect timeout in seconds (default: 8)
  --out-dir <path>                 output root (default: .tmp/p2p_real_env_traffic_monitor)
  --history-path <path>            override persistent history file path
  --summary-only                   skip sampling, recompute summary from history only

  --local-status-url <url>         local node status endpoint
                                   (default: http://127.0.0.1:5633/v1/chain/status)

  --sequencer-target <user@host>   remote sequencer SSH target
                                   (default: root@39.104.204.172)
  --sequencer-status-url <url>     remote sequencer status endpoint
                                   (default: http://127.0.0.1:5631/v1/chain/status)

  --storage-target <user@host>     remote storage SSH target
                                   (default: root@39.104.205.67)
  --storage-status-url <url>       remote storage status endpoint
                                   (default: http://127.0.0.1:5632/v1/chain/status)

Environment:
  P2PARCH6_SEQ_SSH_PASSWORD        optional sequencer SSH password for sshpass
  P2PARCH6_STORAGE_SSH_PASSWORD    optional storage SSH password for sshpass

Artifacts:
  <out-dir>/history.ndjson         persistent sample history (default)
  <out-dir>/latest_summary.json    latest recent-window summary
  <out-dir>/latest_summary.md      latest recent-window summary in Markdown
  <out-dir>/runs/<timestamp>/...   per-run raw samples, config, and summary copies

Notes:
  - Counter deltas are derived only from samples whose `observed_since_unix_ms`
    matches the latest sample for that node, so process restarts/reset windows
    shrink coverage instead of producing negative deltas.
  - Each sample also records host-level `network_interface` rx/tx byte counters
    when the default route interface can be resolved, so summaries can compare
    payload-only traffic against whole-interface bandwidth.
  - History is pruned to a bounded retention window before summarization, so
    repeated cron/systemd runs do not grow `history.ndjson` without bound.
  - Use repeated invocations (cron/systemd timer) or a longer single run to
    accumulate enough history for a full 10-minute answer.
  - `latest_summary.json` includes both per-node full detail maps and a triad
    aggregate section with merged total-flow distribution, plus libp2p
    control-plane event counters for the currently unattributed overhead.
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

capture_status() {
  local label=$1
  local mode=$2
  local status_url=$3
  local sample_index=$4
  local target=${5:-}
  local password=${6:-}
  local sample_dir="$run_dir/raw/$label/sample-$(printf '%03d' "$sample_index")"
  mkdir -p "$sample_dir"
  local status_file="$sample_dir/status.json"

  if [[ "$mode" == "local" ]]; then
    if curl -fsS "$status_url" >"$status_file" 2>"$sample_dir/status.stderr.log"; then
      return 0
    fi
    printf '{"ok":false,"fetch_error":"curl_failed"}\n' >"$status_file"
    return 0
  fi

  if run_ssh "$target" "$password" "curl -fsS '$status_url'" >"$status_file" 2>"$sample_dir/status.stderr.log"; then
    return 0
  fi
  printf '{"ok":false,"fetch_error":"ssh_or_curl_failed"}\n' >"$status_file"
}

capture_network_interface() {
  local label=$1
  local mode=$2
  local sample_index=$3
  local target=${4:-}
  local password=${5:-}
  local sample_dir="$run_dir/raw/$label/sample-$(printf '%03d' "$sample_index")"
  mkdir -p "$sample_dir"
  local network_file="$sample_dir/network.json"

  if [[ "$mode" == "local" ]]; then
    local iface=""
    local rx_bytes=""
    local tx_bytes=""
    if command -v ip >/dev/null 2>&1; then
      iface=$(ip route get 1.1.1.1 2>/dev/null | awk '
        /dev/ {
          for (i = 1; i <= NF; i++) {
            if ($i == "dev" && (i + 1) <= NF) {
              print $(i + 1)
              exit
            }
          }
        }
      ') || true
    fi
    if [[ -n "$iface" ]] \
      && [[ -r "/sys/class/net/$iface/statistics/rx_bytes" ]] \
      && [[ -r "/sys/class/net/$iface/statistics/tx_bytes" ]]; then
      rx_bytes=$(cat "/sys/class/net/$iface/statistics/rx_bytes")
      tx_bytes=$(cat "/sys/class/net/$iface/statistics/tx_bytes")
      jq -n \
        --arg iface "$iface" \
        --arg rx_bytes "$rx_bytes" \
        --arg tx_bytes "$tx_bytes" \
        '{available: true, name: $iface, rx_bytes: ($rx_bytes | tonumber), tx_bytes: ($tx_bytes | tonumber)}' \
        >"$network_file"
      return 0
    fi
    printf '{"available":false}\n' >"$network_file"
    return 0
  fi

  if run_ssh "$target" "$password" "iface=''; if command -v ip >/dev/null 2>&1; then iface=\$(ip route get 1.1.1.1 2>/dev/null | awk '/dev/ {for (i = 1; i <= NF; i++) if (\$i == \"dev\" && (i + 1) <= NF) {print \$(i + 1); exit}}'); fi; if [ -n \"\$iface\" ] && [ -r \"/sys/class/net/\$iface/statistics/rx_bytes\" ] && [ -r \"/sys/class/net/\$iface/statistics/tx_bytes\" ]; then rx=\$(cat \"/sys/class/net/\$iface/statistics/rx_bytes\"); tx=\$(cat \"/sys/class/net/\$iface/statistics/tx_bytes\"); printf '{\"available\":true,\"name\":\"%s\",\"rx_bytes\":%s,\"tx_bytes\":%s}\n' \"\$iface\" \"\$rx\" \"\$tx\"; else printf '{\"available\":false}\n'; fi" >"$network_file" 2>"$sample_dir/network.stderr.log"; then
    return 0
  fi
  printf '{"available":false,"fetch_error":"ssh_failed"}\n' >"$network_file"
}

append_sample_record() {
  local label=$1
  local mode=$2
  local status_url=$3
  local sample_dir="$run_dir/raw/$label/sample-$(printf '%03d' "$sample_index")"

  local status_file="$sample_dir/status.json"
  local captured_at
  local captured_at_unix_ms
  captured_at_unix_ms=$(python3 "$repo_root/scripts/traffic_monitor_freshness.py" "$status_file" --capture-time)
  captured_at=$(date -Iseconds)

  jq -c -n \
    --arg run_id "$run_id" \
    --arg label "$label" \
    --arg mode "$mode" \
    --arg status_url "$status_url" \
    --arg sample_index "$sample_index" \
    --arg captured_at "$captured_at" \
    --arg captured_at_unix_ms "$captured_at_unix_ms" \
    --slurpfile network "$sample_dir/network.json" \
    --slurpfile status "$sample_dir/status.json" \
    '{
      run_id: $run_id,
      sample_index: ($sample_index | tonumber),
      captured_at: $captured_at,
      captured_at_unix_ms: ($captured_at_unix_ms | tonumber),
      label: $label,
      mode: $mode,
      status_url: $status_url,
      status_fetch_ok: ($status[0].ok // false),
      fetch_error: ($status[0].fetch_error // null),
      node_id: ($status[0].node_id // null),
      world_id: ($status[0].world_id // null),
      role: ($status[0].role // null),
      running: ($status[0].running // null),
      status_observed_at_unix_ms: ($status[0].observed_at_unix_ms // null),
      last_error: ($status[0].last_error // null),
      consensus: {
        committed_height: ($status[0].consensus.committed_height // null),
        network_committed_height: ($status[0].consensus.network_committed_height // null),
        known_peer_heads: ($status[0].consensus.known_peer_heads // null)
      },
      storage_recent_errors_count: ($status[0].storage.recent_errors_count // null),
      reward_runtime_recent_errors_count: ($status[0].reward_runtime.recent_errors_count // null),
      network_interface: (
        if ($network[0].available // false)
        then {
          name: ($network[0].name // null),
          rx_bytes: ($network[0].rx_bytes // null),
          tx_bytes: ($network[0].tx_bytes // null)
        }
        else null
        end
      ),
      traffic: {
        udp_gossip: ($status[0].traffic.udp_gossip // null),
        libp2p_replication: ($status[0].traffic.libp2p_replication // null)
      }
    }' >> "$run_samples_ndjson"
}

samples=3
interval_secs=20
window_minutes=10
history_retention_minutes=""
top_n=5
max_sample_age_secs=60
max_sample_gap_secs=120
as_of_unix_ms=""
ssh_timeout_secs=8
out_root=".tmp/p2p_real_env_traffic_monitor"
history_path=""
summary_only=0

local_status_url="http://127.0.0.1:5633/v1/chain/status"

sequencer_target="root@39.104.204.172"
sequencer_status_url="http://127.0.0.1:5631/v1/chain/status"

storage_target="root@39.104.205.67"
storage_status_url="http://127.0.0.1:5632/v1/chain/status"

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
    --as-of-unix-ms)
      as_of_unix_ms=${2:-}
      shift 2
      ;;
    --max-sample-age-secs)
      max_sample_age_secs=${2:-}
      shift 2
      ;;
    --max-sample-gap-secs)
      max_sample_gap_secs=${2:-}
      shift 2
      ;;
    --window-minutes)
      window_minutes=${2:-}
      shift 2
      ;;
    --history-retention-minutes)
      history_retention_minutes=${2:-}
      shift 2
      ;;
    --top-n)
      top_n=${2:-}
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
    --history-path)
      history_path=${2:-}
      shift 2
      ;;
    --summary-only)
      summary_only=1
      shift
      ;;
    --local-status-url)
      local_status_url=${2:-}
      shift 2
      ;;
    --sequencer-target)
      sequencer_target=${2:-}
      shift 2
      ;;
    --sequencer-status-url)
      sequencer_status_url=${2:-}
      shift 2
      ;;
    --storage-target)
      storage_target=${2:-}
      shift 2
      ;;
    --storage-status-url)
      storage_status_url=${2:-}
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

if (( summary_only == 0 )); then
  ensure_positive_int --samples "$samples"
  ensure_positive_int --interval-secs "$interval_secs"
fi
ensure_positive_int --max-sample-age-secs "$max_sample_age_secs"
ensure_positive_int --max-sample-gap-secs "$max_sample_gap_secs"
summary_clock_args=(--max-sample-age-secs "$max_sample_age_secs" --max-sample-gap-secs "$max_sample_gap_secs")
if (( summary_only == 1 )); then
  summary_clock_args+=(--read-only-history)
fi
if [[ -n "$as_of_unix_ms" ]]; then
  if [[ ! "$as_of_unix_ms" =~ ^[0-9]+$ ]] || (( summary_only == 0 )); then
    echo "--as-of-unix-ms requires a nonnegative integer and --summary-only" >&2
    exit 2
  fi
  python3 "$repo_root/scripts/traffic_monitor_freshness.py" --validate-as-of "$as_of_unix_ms"
  summary_clock_args+=(--as-of-unix-ms "$as_of_unix_ms")
fi
ensure_positive_int --window-minutes "$window_minutes"
if [[ -n "$history_retention_minutes" ]]; then
  ensure_positive_int --history-retention-minutes "$history_retention_minutes"
else
  history_retention_minutes=$(( window_minutes + 30 ))
  if (( history_retention_minutes < 120 )); then
    history_retention_minutes=120
  fi
fi
ensure_positive_int --top-n "$top_n"
ensure_positive_int --ssh-timeout-secs "$ssh_timeout_secs"

if [[ -z "$history_path" ]]; then
  history_path="$out_root/history.ndjson"
fi

if (( summary_only == 1 )) && [[ ! -f "$history_path" ]]; then
  echo "history file does not exist: $history_path" >&2
  exit 2
fi

run_id=$(date +"%Y%m%d-%H%M%S")
run_dir="$out_root/runs/$run_id"
run_samples_ndjson="$run_dir/samples.ndjson"
run_summary_json="$run_dir/summary.json"
run_summary_md="$run_dir/summary.md"
latest_summary_json="$out_root/latest_summary.json"
latest_summary_md="$out_root/latest_summary.md"
config_json="$run_dir/config.json"
summary_script="$repo_root/scripts/traffic-monitor-summary.py"

mkdir -p "$run_dir/raw/local_node" "$run_dir/raw/sequencer_ecs" "$run_dir/raw/storage_ecs"
mkdir -p "$(dirname "$history_path")"
: > "$run_samples_ndjson"
if (( summary_only == 0 )); then touch "$history_path"; fi
[[ -f "$summary_script" ]] || {
  echo "missing summary helper: $summary_script" >&2
  exit 1
}

seq_password=${P2PARCH6_SEQ_SSH_PASSWORD:-}
storage_password=${P2PARCH6_STORAGE_SSH_PASSWORD:-}

jq -n \
  --arg run_id "$run_id" \
  --arg run_dir "$run_dir" \
  --arg history_path "$history_path" \
  --arg latest_summary_json "$latest_summary_json" \
  --arg latest_summary_md "$latest_summary_md" \
  --arg local_status_url "$local_status_url" \
  --arg sequencer_target "$sequencer_target" \
  --arg sequencer_status_url "$sequencer_status_url" \
  --arg storage_target "$storage_target" \
  --arg storage_status_url "$storage_status_url" \
  --argjson samples "$samples" \
  --argjson interval_secs "$interval_secs" \
  --argjson window_minutes "$window_minutes" \
  --argjson history_retention_minutes "$history_retention_minutes" \
  --argjson top_n "$top_n" \
  --argjson ssh_timeout_secs "$ssh_timeout_secs" \
  --argjson summary_only "$summary_only" \
  '{
    run_id: $run_id,
    run_dir: $run_dir,
    history_path: $history_path,
    latest_summary_json: $latest_summary_json,
    latest_summary_md: $latest_summary_md,
    samples: $samples,
    interval_secs: $interval_secs,
    window_minutes: $window_minutes,
    history_retention_minutes: $history_retention_minutes,
    top_n: $top_n,
    ssh_timeout_secs: $ssh_timeout_secs,
    summary_only: ($summary_only == 1),
    nodes: {
      local_node: {
        mode: "local",
        status_url: $local_status_url
      },
      sequencer_ecs: {
        mode: "remote",
        target: $sequencer_target,
        status_url: $sequencer_status_url
      },
      storage_ecs: {
        mode: "remote",
        target: $storage_target,
        status_url: $storage_status_url
      }
    }
  }' > "$config_json"

if (( summary_only == 0 )); then
  for ((sample_index = 1; sample_index <= samples; sample_index++)); do
    echo "sample $sample_index/$samples"

    capture_status local_node local "$local_status_url" "$sample_index"
    capture_network_interface local_node local "$sample_index"
    append_sample_record local_node local "$local_status_url"

    capture_status sequencer_ecs remote "$sequencer_status_url" "$sample_index" "$sequencer_target" "$seq_password"
    capture_network_interface sequencer_ecs remote "$sample_index" "$sequencer_target" "$seq_password"
    append_sample_record sequencer_ecs remote "$sequencer_status_url"

    capture_status storage_ecs remote "$storage_status_url" "$sample_index" "$storage_target" "$storage_password"
    capture_network_interface storage_ecs remote "$sample_index" "$storage_target" "$storage_password"
    append_sample_record storage_ecs remote "$storage_status_url"

    if (( sample_index < samples )); then
      sleep "$interval_secs"
    fi
  done

  cat "$run_samples_ndjson" >> "$history_path"
fi

if [[ ! -s "$history_path" ]]; then
  echo "history file is empty: $history_path" >&2
  exit 1
fi

python3 "$summary_script" \
  --layout triad \
  --history-path "$history_path" \
  --summary-json "$latest_summary_json" \
  --summary-md "$latest_summary_md" \
  --window-minutes "$window_minutes" \
  --history-retention-minutes "$history_retention_minutes" \
  "${summary_clock_args[@]}" \
  --top-n "$top_n" \
  --run-id "$run_id" \
  --run-dir "$run_dir" \
  --label local_node \
  --label sequencer_ecs \
  --label storage_ecs

cp "$latest_summary_json" "$run_summary_json"
cp "$latest_summary_md" "$run_summary_md"

printf 'history: %s\nlatest_summary_json: %s\nlatest_summary_md: %s\nrun_dir: %s\n' \
  "$history_path" "$latest_summary_json" "$latest_summary_md" "$run_dir"
