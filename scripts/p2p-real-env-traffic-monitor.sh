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
  <out-dir>/runs/<sequence>-<attempt-id>/...   per-run raw samples, config, and summary copies

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

decimal_value() {
  python3 - "$1" "$2" <<'PYDECIMAL'
import re, sys
flag, raw = sys.argv[1:]
try:
    if re.fullmatch(r'[0-9]+', raw) is None: raise ValueError()
    value = int(raw)
    if not (0 if flag == '--as-of-unix-ms' else 1) <= value <= (2**63-1 if flag == '--as-of-unix-ms' else 2**31-1): raise ValueError()
except ValueError:
    print(flag + ' requires an ASCII decimal integer within its supported range', file=sys.stderr)
    raise SystemExit(2)
print(value)
PYDECIMAL
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

  local remote_command
  printf -v remote_command 'curl -fsS %q' "$status_url"
  if run_ssh "$target" "$password" "$remote_command" >"$status_file" 2>"$sample_dir/status.stderr.log"; then
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

original_args=("$@")
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
    -h|--help) usage; exit 0 ;;
    --summary-only) summary_only=1; shift; continue ;;
    --samples|--interval-secs|--as-of-unix-ms|--max-sample-age-secs|--max-sample-gap-secs|--window-minutes|--history-retention-minutes|--top-n|--ssh-timeout-secs|--out-dir|--history-path|--local-status-url|--sequencer-target|--sequencer-status-url|--storage-target|--storage-status-url)
      if [[ $# -lt 2 || -z "$2" || "$2" == -* ]]; then
        echo "$1 requires a nonempty value" >&2; exit 2
      fi ;;
    *) echo "unsupported traffic monitor option" >&2; exit 2 ;;
  esac
  value=$2
  case "$1" in
    --samples|--interval-secs|--as-of-unix-ms|--max-sample-age-secs|--max-sample-gap-secs|--window-minutes|--history-retention-minutes|--top-n|--ssh-timeout-secs) value=$(decimal_value "$1" "$value") ;;
  esac
  case "$1" in
    --samples) samples=$value ;;
    --interval-secs) interval_secs=$value ;;
    --as-of-unix-ms) as_of_unix_ms=$value ;;
    --max-sample-age-secs) max_sample_age_secs=$value ;;
    --max-sample-gap-secs) max_sample_gap_secs=$value ;;
    --window-minutes) window_minutes=$value ;;
    --history-retention-minutes) history_retention_minutes=$value ;;
    --top-n) top_n=$value ;;
    --ssh-timeout-secs) ssh_timeout_secs=$value ;;
    --out-dir) out_root=$value ;;
    --history-path) history_path=$value ;;
    --local-status-url) local_status_url=$value ;;
    --sequencer-target) sequencer_target=$value ;;
    --sequencer-status-url) sequencer_status_url=$value ;;
    --storage-target) storage_target=$value ;;
    --storage-status-url) storage_status_url=$value ;;
  esac
  shift 2
done
summary_clock_args=(--max-sample-age-secs "$max_sample_age_secs" --max-sample-gap-secs "$max_sample_gap_secs" --read-only-history)
if [[ -n "$as_of_unix_ms" ]]; then
  if (( summary_only == 0 )); then echo "--as-of-unix-ms requires --summary-only" >&2; exit 2; fi
  python3 "$repo_root/scripts/traffic_monitor_freshness.py" --validate-as-of "$as_of_unix_ms"
  summary_clock_args+=(--as-of-unix-ms "$as_of_unix_ms")
fi
if [[ -z "$history_retention_minutes" ]]; then
  history_retention_minutes=$(( window_minutes + 30 ))
  if (( history_retention_minutes < 120 )); then history_retention_minutes=120; fi
fi
history_retention_minutes=$(decimal_value --history-retention-minutes "$history_retention_minutes")

if [[ -z "$history_path" ]]; then
  history_path="$out_root/history.ndjson"
fi

if (( summary_only == 1 )) && [[ ! -f "$history_path" ]]; then
  echo "history file does not exist: $history_path" >&2
  exit 2
fi

# Validate shared-history ownership through the same no-write protocol checks.
history_path=$(python3 - "$history_path" <<'PYPATH'
import os, sys
print(os.path.abspath(sys.argv[1]))
PYPATH
)
attempt_helper="$repo_root/scripts/p2p-real-env-observability-attempt.py"
history_args=()
if (( summary_only == 1 )); then history_args+=(--read-only-history); fi
python3 "$attempt_helper" history-validate --collector traffic --out-dir "$out_root" --history-path "$history_path" ${history_args[@]+"${history_args[@]}"}
summary_script="$repo_root/scripts/traffic-monitor-summary.py"
[[ -f "$summary_script" ]] || { echo "missing summary helper" >&2; exit 1; }
scope_args=()
for argument in "${original_args[@]+"${original_args[@]}"}"; do scope_args+=("--scope-arg=$argument"); done
reservation=$(python3 "$attempt_helper" begin --collector traffic --out-dir "$out_root" ${scope_args[@]+"${scope_args[@]}"})
sequence=$(printf '%s' "$reservation" | python3 -c 'import json,sys; print(json.load(sys.stdin)["sequence"])')
attempt_id=$(printf '%s' "$reservation" | python3 -c 'import json,sys; print(json.load(sys.stdin)["attempt_id"])')
run_dir=$(printf '%s' "$reservation" | python3 -c 'import json,sys; print(json.load(sys.stdin)["run_dir"])')
run_id="$sequence-$attempt_id"
run_samples_ndjson="$run_dir/samples.ndjson"
run_summary_json="$run_dir/report/latest_summary.json"
run_summary_md="$run_dir/report/latest_summary.md"
latest_summary_json="$out_root/latest_summary.json"
latest_summary_md="$out_root/latest_summary.md"
config_json="$run_dir/config.json"
history_input_path="$run_dir/history.ndjson"
errors=()
mkdir -p "$run_dir/raw/local_node" "$run_dir/raw/sequencer_ecs" "$run_dir/raw/storage_ecs" "$run_dir/report"
: > "$run_samples_ndjson"

seq_password=${P2PARCH6_SEQ_SSH_PASSWORD:-}
storage_password=${P2PARCH6_STORAGE_SSH_PASSWORD:-}

jq -n \
  --arg run_id "$run_id" \
  --arg run_dir "$run_dir" \
  --arg history_path "$history_path" \
  --arg history_input_path "$history_input_path" \
  --argjson max_sample_age_secs "$max_sample_age_secs" \
  --argjson max_sample_gap_secs "$max_sample_gap_secs" \
  --argjson as_of_unix_ms "${as_of_unix_ms:-null}" \
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
    history_input_path: $history_input_path,
    max_sample_age_secs: $max_sample_age_secs,
    max_sample_gap_secs: $max_sample_gap_secs,
    as_of_unix_ms: $as_of_unix_ms,
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

fi

history_result=0
if python3 "$attempt_helper" history --collector traffic --out-dir "$out_root" --sequence "$sequence" --attempt-id "$attempt_id" --history-path "$history_path" --retention-minutes "$history_retention_minutes" ${history_args[@]+"${history_args[@]}"}; then
  :
else
  history_result=$?
  errors+=(--error traffic:1)
fi
if (( history_result == 0 )) && [[ ! -s "$history_input_path" ]]; then
  history_result=1
  errors+=(--error traffic:1)
fi
if (( history_result == 0 )); then
  if python3 "$summary_script" \
    --layout triad \
    --history-path "$history_input_path" \
    --summary-json "$run_summary_json" \
    --summary-md "$run_summary_md" \
    --window-minutes "$window_minutes" \
    --history-retention-minutes "$history_retention_minutes" \
    "${summary_clock_args[@]}" \
    --top-n "$top_n" \
    --run-id "$run_id" \
    --run-dir "$run_dir" \
    --label local_node --label sequencer_ecs --label storage_ecs; then
    if python3 - "$config_json" "$run_samples_ndjson" "$run_summary_json" "$run_summary_md" <<'PYCOLLECTION'
import json, sys
from pathlib import Path
config = json.loads(Path(sys.argv[1]).read_text())
rows = [json.loads(line) for line in Path(sys.argv[2]).read_text().splitlines() if line.strip()]
replay = config['summary_only']
failed = 0 if replay else sum(row.get('status_fetch_ok') is not True for row in rows)
expected = 0 if replay else 3 * config['samples']
attempted = 0 if replay else len(rows)
collection = dict(mode='history_replay' if replay else 'live', complete=attempted == expected and failed == 0, samples_per_node=config['samples'], expected_sample_count=expected, attempted_count=attempted, failed_sample_count=failed, node_labels=['local_node','sequencer_ecs','storage_ecs'])
path = Path(sys.argv[3]); value = json.loads(path.read_text()); value['schema_version'] = 2; value['collection'] = collection
path.write_text(json.dumps(value, indent=2) + '\n')
with Path(sys.argv[4]).open('a') as stream: stream.write('\n- Collection complete: `' + str(collection['complete']).lower() + '`\n')
raise SystemExit(0 if collection['complete'] else 1)
PYCOLLECTION
    then :; else errors+=(--error traffic:1); fi
  else
    errors+=(--error summary:1)
  fi
fi
if python3 "$attempt_helper" finish --collector traffic --out-dir "$out_root" --sequence "$sequence" --attempt-id "$attempt_id" ${errors[@]+"${errors[@]}"}; then result=0; else result=$?; fi
printf 'history: %s\nlatest_summary_json: %s\nlatest_summary_md: %s\nrun_dir: %s\n' "$history_path" "$latest_summary_json" "$latest_summary_md" "$run_dir"
exit "$result"
