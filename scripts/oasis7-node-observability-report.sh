#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage: ./scripts/oasis7-node-observability-report.sh [options]

Summarize live node observability from `/v1/chain/status`, optionally folding in a
recent traffic-window summary produced by `oasis7-node-traffic-monitor.sh`.

Options:
  --status-url <url>            status endpoint to fetch
                                (default: http://127.0.0.1:5633/v1/chain/status)
  --status-json-path <path>     read chain status or oasis7.local-node-status/v1 JSON
                                instead of HTTP; formal observer/full-sync readiness
                                uses the producer observation (maximum age: 60000ms),
                                and file replay does not refresh that observation
  --traffic-summary-json <path> optional traffic summary json to attach
  --node-label <label>          label written into summary output
                                (default: local_node)
  --out-dir <path>              output root
                                (default: .tmp/oasis7_node_observability)
  --summary-json <path>         override latest summary json path
  --summary-md <path>           override latest summary markdown path
  -h, --help                    show help

Artifacts:
  <out-dir>/latest_summary.json
  <out-dir>/latest_summary.md
USAGE
}

status_url="http://127.0.0.1:5633/v1/chain/status"
status_json_path=""
traffic_summary_json=""
node_label="local_node"
out_dir=".tmp/oasis7_node_observability"
summary_json_path=""
summary_md_path=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --status-url)
      status_url=${2:-}
      shift 2
      ;;
    --status-json-path)
      status_json_path=${2:-}
      shift 2
      ;;
    --traffic-summary-json)
      traffic_summary_json=${2:-}
      shift 2
      ;;
    --node-label)
      node_label=${2:-}
      shift 2
      ;;
    --out-dir)
      out_dir=${2:-}
      shift 2
      ;;
    --summary-json)
      summary_json_path=${2:-}
      shift 2
      ;;
    --summary-md)
      summary_md_path=${2:-}
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

if [[ -z "$summary_json_path" ]]; then
  summary_json_path="$out_dir/latest_summary.json"
fi
if [[ -z "$summary_md_path" ]]; then
  summary_md_path="$out_dir/latest_summary.md"
fi

mkdir -p "$out_dir"
mkdir -p "$(dirname "$summary_json_path")" "$(dirname "$summary_md_path")"

status_tmp="$(mktemp)"
status_fetch_ok=1
fetch_error=""

cleanup() {
  rm -f "$status_tmp" "$status_tmp.stderr"
}
trap cleanup EXIT

if [[ -n "$status_json_path" ]]; then
  if [[ ! -f "$status_json_path" ]]; then
    echo "status json does not exist: $status_json_path" >&2
    exit 2
  fi
  cp "$status_json_path" "$status_tmp"
else
  if ! curl -fsS "$status_url" >"$status_tmp" 2>"$status_tmp.stderr"; then
    status_fetch_ok=0
    fetch_error="$(tr '\n' ' ' <"$status_tmp.stderr" | sed 's/[[:space:]]\+/ /g; s/^ //; s/ $//')"
    printf '{}' >"$status_tmp"
  fi
fi

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
python3 - "$status_tmp" "$summary_json_path" "$summary_md_path" "$node_label" "$status_url" "$traffic_summary_json" "$status_fetch_ok" "$fetch_error" "$status_json_path" "$script_dir" <<'PY'
from __future__ import annotations

import json
import importlib.util
import sys
from datetime import datetime, timezone
from pathlib import Path


def load_json(path: str) -> dict:
    raw = Path(path).read_text(encoding="utf-8")
    if not raw.strip():
        return {}
    return json.loads(raw)


def fmt_bool(value):
    if value is None:
        return "n/a"
    return "yes" if value else "no"


def fmt_num(value):
    if value is None:
        return "n/a"
    if isinstance(value, str):
        return value
    return f"{int(value):,}"


def fmt_metric(value):
    if value is None:
        return "n/a"
    if isinstance(value, str):
        return value
    amount = float(value)
    if amount.is_integer():
        return f"{int(amount):,}"
    return f"{amount:.1f}"


def fmt_bytes(value):
    if value is None:
        return "n/a"
    units = ["B", "KiB", "MiB", "GiB", "TiB"]
    amount = float(value)
    for unit in units:
        if amount < 1024.0 or unit == units[-1]:
            if unit == "B":
                return f"{int(amount)} {unit}"
            return f"{amount:.2f} {unit}"
        amount /= 1024.0
    return f"{int(value)} B"


status_path, summary_json_path, summary_md_path, node_label, status_url, traffic_summary_path, status_fetch_ok_raw, fetch_error, status_json_path = sys.argv[1:10]
as_of = datetime.now(timezone.utc)
generated_at = as_of.astimezone().isoformat()
status_fetch_ok = status_fetch_ok_raw == "1"
status = load_json(status_path)
spec = importlib.util.spec_from_file_location("profile_readiness", Path(sys.argv[10]) / "p2p-profile-readiness.py")
adapter = importlib.util.module_from_spec(spec)
spec.loader.exec_module(adapter)
profile_readiness = adapter.project(status, "file" if status_json_path else "http", int(as_of.timestamp() * 1000))
# Formal CLI inputs only enter the explicit allowlist adapter; unrelated fields
# and raw errors/configuration must not flow through the legacy HTTP projection.
if not isinstance(status, dict) or "api_version" in status:
    status = {}

traffic_summary = None
traffic_summary_missing = False
if traffic_summary_path:
    traffic_path = Path(traffic_summary_path)
    if traffic_path.is_file():
        traffic_summary = load_json(str(traffic_path))
    else:
        traffic_summary_missing = True

observability = status.get("observability") or {}
p2p = status.get("p2p") or {}
path_observability = observability.get("path_observability") or {}
if not path_observability and p2p:
    path_observability = {
        "selected_path_kind": p2p.get("active_transport_kind"),
        "selected_path_age_ms": None,
        "active_direct_path_count": p2p.get("active_direct_path_count"),
        "active_hole_punch_path_count": p2p.get("active_hole_punch_path_count"),
        "active_relay_path_count": p2p.get("active_relay_path_count"),
        "transition_count": p2p.get("transport_transition_count"),
        "transitions": p2p.get("transport_transitions") or {},
        "last_transition": p2p.get("last_transport_transition"),
    }
selected_path_kind = path_observability.get("selected_path_kind") or "not_reported"
selected_path_age_ms = path_observability.get("selected_path_age_ms")
active_path_mix = {
    "direct": path_observability.get("active_direct_path_count"),
    "hole_punched": path_observability.get("active_hole_punch_path_count"),
    "relay_reserved": path_observability.get("active_relay_path_count"),
}
active_path_mix = {
    key: ("not_reported" if value is None else value)
    for key, value in active_path_mix.items()
}
path_transition_counters = path_observability.get("transitions") or {}
if not path_transition_counters:
    path_transition_counters = {
        "direct_to_hole_punched": "not_reported",
        "direct_to_relay_reserved": "not_reported",
        "hole_punched_to_direct": "not_reported",
        "hole_punched_to_relay_reserved": "not_reported",
        "relay_reserved_to_direct": "not_reported",
        "relay_reserved_to_hole_punched": "not_reported",
    }
last_path_transition = path_observability.get("last_transition") or {}
if last_path_transition:
    recent_fallback_reason = "path_transition"
elif selected_path_kind == "relay_reserved":
    recent_fallback_reason = "relay_reserved"
elif selected_path_kind == "not_reported":
    recent_fallback_reason = "not_reported"
else:
    recent_fallback_reason = "unknown"
direct_addr_count = len(p2p.get("confirmed_external_direct_addrs") or [])
if selected_path_kind == "direct" and direct_addr_count > 0:
    reachability_confidence = "observed_direct"
elif selected_path_kind == "hole_punched":
    reachability_confidence = "punched_recently"
elif selected_path_kind == "relay_reserved" or p2p.get("relay_available") is True:
    reachability_confidence = "relay_reserved"
elif selected_path_kind == "not_reported":
    reachability_confidence = "not_reported"
else:
    reachability_confidence = "unknown"
consensus = status.get("consensus") or {}
transactions = status.get("transactions") or {}
storage = status.get("storage") or {}
reward_runtime = status.get("reward_runtime") or {}
execution_bridge_commit_timing = status.get("execution_bridge_commit_timing") or {}
module_tick_routing_status = status.get("module_tick_routing") or {}
module_tick_routing = module_tick_routing_status.get("metrics") or {}
runtime_perf = status.get("runtime_perf") or {}
runtime_perf_smoke = {
    "available": bool(runtime_perf),
    "health": runtime_perf.get("health"),
    "bottleneck": runtime_perf.get("bottleneck"),
    "tick": {
        "p95_ms": (runtime_perf.get("tick") or {}).get("p95_ms"),
        "over_budget_ratio_ppm": (runtime_perf.get("tick") or {}).get("over_budget_ratio_ppm"),
    },
    "action_execution": {
        "p95_ms": (runtime_perf.get("action_execution") or {}).get("p95_ms"),
        "over_budget_ratio_ppm": (runtime_perf.get("action_execution") or {}).get("over_budget_ratio_ppm"),
    },
}
alerts = observability.get("alerts") or []

summary = {
    "generated_at": generated_at,
    "profile_readiness": profile_readiness,
    "node_label": node_label,
    "status_source": "file" if status_json_path else "http",
    "status_url": None if status_json_path else status_url,
    "status_json_path": status_json_path or None,
    "status_fetch_ok": status_fetch_ok,
    "fetch_error": None if status_fetch_ok else (fetch_error or "status fetch failed"),
    "latest": {
        "node_id": status.get("node_id"),
        "world_id": status.get("world_id"),
        "role": status.get("role"),
        "running": status.get("running"),
        "observed_at_unix_ms": status.get("observed_at_unix_ms"),
        "tick_count": status.get("tick_count"),
        "last_error": status.get("last_error"),
    },
    "observability": {
        "available": bool(observability),
        "status": observability.get("status"),
        "summary": observability.get("summary"),
        "connected_peer_count": observability.get("connected_peer_count"),
        "active_peer_count": observability.get("active_peer_count"),
        "candidate_peer_count": observability.get("candidate_peer_count"),
        "suspect_peer_count": observability.get("suspect_peer_count"),
        "blocked_peer_count": observability.get("blocked_peer_count"),
        "peer_with_issues_count": observability.get("peer_with_issues_count"),
        "known_peer_heads": observability.get("known_peer_heads"),
        "network_height_lag": observability.get("network_height_lag"),
        "recent_replication_error_count": observability.get("recent_replication_error_count"),
        "storage_degraded": observability.get("storage_degraded"),
        "reward_runtime_degraded": observability.get("reward_runtime_degraded"),
        "path_observability": path_observability,
        "alerts": alerts,
    },
    "p2p_reachability": {
        "available": bool(p2p),
        "detected_reachability": p2p.get("detected_reachability"),
        "deployment_mode": p2p.get("deployment_mode"),
        "node_role_claim": p2p.get("node_role_claim"),
        "relay_available": p2p.get("relay_available"),
        "probe_stable": p2p.get("probe_stable"),
        "path_observability": path_observability,
        "selected_path_kind": selected_path_kind,
        "selected_path_age_ms": "not_reported" if selected_path_age_ms is None else selected_path_age_ms,
        "path_transition_counters": path_transition_counters,
        "active_path_mix": active_path_mix,
        "recent_fallback_reason": recent_fallback_reason,
        "reachability_confidence": reachability_confidence,
    },
    "consensus": {
        "committed_height": consensus.get("committed_height"),
        "network_committed_height": consensus.get("network_committed_height"),
        "known_peer_heads": consensus.get("known_peer_heads"),
        "last_commit_age_ms": consensus.get("last_commit_age_ms"),
        "recent_finality_latency": consensus.get("recent_finality_latency"),
    },
    "transactions": {
        "accepted_count": transactions.get("accepted_count"),
        "pending_count": transactions.get("pending_count"),
        "confirmed_count": transactions.get("confirmed_count"),
        "failed_count": transactions.get("failed_count"),
        "timeout_count": transactions.get("timeout_count"),
        "inflight_count": transactions.get("inflight_count"),
        "recent_confirmation_latency": transactions.get("recent_confirmation_latency"),
    },
    "storage": {
        "degraded_reason": storage.get("degraded_reason"),
        "last_gc_result": storage.get("last_gc_result"),
        "last_gc_error": storage.get("last_gc_error"),
        "orphan_blob_count": storage.get("orphan_blob_count"),
        "checkpoint_count": storage.get("checkpoint_count"),
    },
    "reward_runtime": {
        "enabled": reward_runtime.get("enabled"),
        "metrics_available": reward_runtime.get("metrics_available"),
        "invariant_ok": reward_runtime.get("invariant_ok"),
        "last_error": reward_runtime.get("last_error"),
        "latest_epoch_index": reward_runtime.get("latest_epoch_index"),
        "report_count": reward_runtime.get("report_count"),
    },
    "execution_bridge_commit_timing": execution_bridge_commit_timing,
    "module_tick_routing": module_tick_routing_status,
    "runtime_perf": runtime_perf,
    "runtime_perf_smoke": runtime_perf_smoke,
    "traffic_window": traffic_summary,
    "traffic_summary_missing": traffic_summary_missing,
}

Path(summary_json_path).write_text(
    json.dumps(summary, ensure_ascii=False, indent=2) + "\n",
    encoding="utf-8",
)

lines = [
    "# Oasis7 Node Observability Summary",
    "",
    f"- generated_at: `{generated_at}`",
    f"- node_label: `{node_label}`",
    f"- status_source: `{'file' if status_json_path else 'http'}`",
    f"- status_fetch_ok: `{fmt_bool(status_fetch_ok)}`",
]
if status_fetch_ok:
    lines.extend(
        [
            f"- node_id: `{status.get('node_id')}`",
            f"- world_id: `{status.get('world_id')}`",
            f"- role: `{status.get('role')}`",
            f"- running: `{fmt_bool(status.get('running'))}`",
            f"- last_error: `{status.get('last_error')}`",
        ]
    )
else:
    lines.append(f"- fetch_error: `{summary['fetch_error']}`")

lines.extend(
    [
        "",
        "## Live Health",
        f"- status: `{observability.get('status')}`",
        f"- summary: `{observability.get('summary')}`",
        f"- connected_peers: `{fmt_num(observability.get('connected_peer_count'))}`",
        f"- peer_health_counts: `active={fmt_num(observability.get('active_peer_count'))} candidate={fmt_num(observability.get('candidate_peer_count'))} suspect={fmt_num(observability.get('suspect_peer_count'))} blocked={fmt_num(observability.get('blocked_peer_count'))}`",
        f"- peers_with_issues: `{fmt_num(observability.get('peer_with_issues_count'))}`",
        f"- known_peer_heads: `{fmt_num(observability.get('known_peer_heads'))}`",
        f"- network_height_lag: `{fmt_num(observability.get('network_height_lag'))}`",
        f"- recent_replication_error_count: `{fmt_num(observability.get('recent_replication_error_count'))}`",
        f"- storage_degraded: `{fmt_bool(observability.get('storage_degraded'))}`",
        f"- reward_runtime_degraded: `{fmt_bool(observability.get('reward_runtime_degraded'))}`",
    ]
)

lines.extend(
    [
        "",
        "## P2P Path Observability",
        f"- detected_reachability: `{p2p.get('detected_reachability')}`",
        f"- deployment_mode: `{p2p.get('deployment_mode')}`",
        f"- node_role_claim: `{p2p.get('node_role_claim')}`",
        f"- selected_path_kind: `{selected_path_kind}`",
        f"- selected_path_age_ms: `{fmt_num(selected_path_age_ms) if selected_path_age_ms is not None else 'not_reported'}`",
        f"- active_path_counts: `direct={active_path_mix.get('direct')} hole_punched={active_path_mix.get('hole_punched')} relay={active_path_mix.get('relay_reserved')}`",
        f"- transition_count: `{fmt_num(path_observability.get('transition_count'))}`",
        f"- recent_fallback_reason: `{recent_fallback_reason}`",
        f"- reachability_confidence: `{reachability_confidence}`",
        f"- relay_available: `{fmt_bool(p2p.get('relay_available'))}`",
        f"- probe_stable: `{fmt_bool(p2p.get('probe_stable'))}`",
    ]
)
transitions = path_transition_counters
if transitions:
    lines.append(
        "- transition_counters: "
        f"`direct_to_hole_punched={fmt_num(transitions.get('direct_to_hole_punched'))} "
        f"direct_to_relay_reserved={fmt_num(transitions.get('direct_to_relay_reserved'))} "
        f"hole_punched_to_direct={fmt_num(transitions.get('hole_punched_to_direct'))} "
        f"hole_punched_to_relay_reserved={fmt_num(transitions.get('hole_punched_to_relay_reserved'))} "
        f"relay_reserved_to_direct={fmt_num(transitions.get('relay_reserved_to_direct'))} "
        f"relay_reserved_to_hole_punched={fmt_num(transitions.get('relay_reserved_to_hole_punched'))}`"
    )
last_transition = path_observability.get("last_transition") or {}
if last_transition:
    lines.append(
        f"- last_transition: `from={last_transition.get('from_kind')} to={last_transition.get('to_kind')} age_ms={fmt_num(last_transition.get('age_ms'))}`"
    )

if alerts:
    lines.extend(["", "## Active Alerts"])
    for alert in alerts:
        lines.append(
            f"- [{alert.get('severity', 'unknown')}] `{alert.get('code', 'unknown')}`: {alert.get('summary', '')}"
        )
else:
    lines.extend(["", "## Active Alerts", "- none"])

runtime_perf_stages = [
    ("tick", runtime_perf.get("tick") or {}),
    ("decision", runtime_perf.get("decision") or {}),
    ("action_execution", runtime_perf.get("action_execution") or {}),
    ("callback", runtime_perf.get("callback") or {}),
    ("llm_api", runtime_perf.get("llm_api") or {}),
]
lines.extend(
    [
        "",
        "## Consensus / Transaction Latency",
        f"- committed_height: `{fmt_num(consensus.get('committed_height'))}` network_committed_height: `{fmt_num(consensus.get('network_committed_height'))}` known_peer_heads: `{fmt_num(consensus.get('known_peer_heads'))}`",
        f"- last_commit_age_ms: `{fmt_num(consensus.get('last_commit_age_ms'))}`",
        f"- finality_latency: samples=`{fmt_num((consensus.get('recent_finality_latency') or {}).get('sample_count'))}` avg_ms=`{fmt_num((consensus.get('recent_finality_latency') or {}).get('avg_latency_ms'))}` p95_ms=`{fmt_num((consensus.get('recent_finality_latency') or {}).get('p95_latency_ms'))}` max_ms=`{fmt_num((consensus.get('recent_finality_latency') or {}).get('max_latency_ms'))}`",
        f"- transaction_counts: accepted=`{fmt_num(transactions.get('accepted_count'))}` pending=`{fmt_num(transactions.get('pending_count'))}` confirmed=`{fmt_num(transactions.get('confirmed_count'))}` failed=`{fmt_num(transactions.get('failed_count'))}` timeout=`{fmt_num(transactions.get('timeout_count'))}` inflight=`{fmt_num(transactions.get('inflight_count'))}`",
        f"- transaction_confirmation_latency: samples=`{fmt_num((transactions.get('recent_confirmation_latency') or {}).get('sample_count'))}` avg_ms=`{fmt_num((transactions.get('recent_confirmation_latency') or {}).get('avg_latency_ms'))}` p95_ms=`{fmt_num((transactions.get('recent_confirmation_latency') or {}).get('p95_latency_ms'))}` max_ms=`{fmt_num((transactions.get('recent_confirmation_latency') or {}).get('max_latency_ms'))}`",
        "",
        "## Runtime Performance",
        f"- available: `{fmt_bool(bool(runtime_perf))}`",
        f"- health: `{runtime_perf.get('health', 'not_reported')}`",
        f"- bottleneck: `{runtime_perf.get('bottleneck', 'not_reported')}`",
        f"- smoke_tick: p95=`{fmt_metric(runtime_perf_smoke['tick'].get('p95_ms'))}` over_budget_ratio_ppm=`{fmt_num(runtime_perf_smoke['tick'].get('over_budget_ratio_ppm'))}`",
        f"- smoke_action_execution: p95=`{fmt_metric(runtime_perf_smoke['action_execution'].get('p95_ms'))}` over_budget_ratio_ppm=`{fmt_num(runtime_perf_smoke['action_execution'].get('over_budget_ratio_ppm'))}`",
    ]
)
for stage_name, stage in runtime_perf_stages:
    lines.append(
        f"- {stage_name}: p95=`{fmt_metric(stage.get('p95_ms'))}` p99=`{fmt_metric(stage.get('p99_ms'))}` max=`{fmt_metric(stage.get('max_ms'))}` over_budget_ratio_ppm=`{fmt_num(stage.get('over_budget_ratio_ppm'))}` samples=`{fmt_num(stage.get('samples_window'))}`"
    )

lines.extend(
    [
        "",
        "## Storage / Reward",
        f"- storage_degraded_reason: `{storage.get('degraded_reason')}`",
        f"- storage_last_gc_result: `{storage.get('last_gc_result')}`",
        f"- storage_last_gc_error: `{storage.get('last_gc_error')}`",
        f"- reward_runtime_enabled: `{fmt_bool(reward_runtime.get('enabled'))}`",
        f"- reward_runtime_metrics_available: `{fmt_bool(reward_runtime.get('metrics_available'))}`",
        f"- reward_runtime_invariant_ok: `{fmt_bool(reward_runtime.get('invariant_ok'))}`",
        f"- reward_runtime_last_error: `{reward_runtime.get('last_error')}`",
    ]
)

lines.extend(
    [
        "",
        "## Execution Bridge Commit Timing",
        f"- recent_commit_count: `{fmt_num(execution_bridge_commit_timing.get('recent_commit_count'))}`",
        f"- total_ms: `p50={fmt_num(execution_bridge_commit_timing.get('p50_total_ms'))} p95={fmt_num(execution_bridge_commit_timing.get('p95_total_ms'))} max={fmt_num(execution_bridge_commit_timing.get('max_total_ms'))}`",
        f"- slow_count: `{fmt_num(execution_bridge_commit_timing.get('slow_count'))}`",
        f"- last_slow_stage: `{execution_bridge_commit_timing.get('last_slow_stage')}`",
    ]
)
stage_timings = execution_bridge_commit_timing.get("stages") or {}
if stage_timings:
    stage_parts = []
    for stage_name in sorted(stage_timings):
        stage = stage_timings.get(stage_name) or {}
        stage_parts.append(
            f"{stage_name}:count={fmt_num(stage.get('count'))},cumulative_ms={fmt_num(stage.get('cumulative_ms'))}"
        )
    lines.append(f"- stage_counters: `{' | '.join(stage_parts)}`")
else:
    lines.append("- stage_counters: `not_reported`")

duration_buckets = module_tick_routing.get("duration_buckets") or {}
lines.extend(
    [
        "",
        "## Module Tick Routing",
        f"- available: `{fmt_bool(module_tick_routing_status.get('available'))}`",
        f"- source: `{module_tick_routing_status.get('source')}`",
        f"- load_error: `{module_tick_routing_status.get('load_error')}`",
        f"- schedule_len: `{fmt_num(module_tick_routing.get('schedule_len'))}`",
        f"- last_due_count: `{fmt_num(module_tick_routing.get('last_due_count'))}`",
        f"- last_invoked_count: `{fmt_num(module_tick_routing.get('last_invoked_count'))}`",
        f"- missing_invocation_count: `{fmt_num(module_tick_routing.get('missing_invocation_count'))}`",
        f"- last_missing_invocation_count: `{fmt_num(module_tick_routing.get('last_missing_invocation_count'))}`",
        f"- oldest_overdue_ticks: `{fmt_num(module_tick_routing.get('oldest_overdue_ticks'))}`",
        f"- routing_count: `{fmt_num(module_tick_routing.get('routing_count'))}`",
        f"- route_duration_ms: `last={fmt_num(module_tick_routing.get('last_route_duration_ms'))} max={fmt_num(module_tick_routing.get('max_route_duration_ms'))} cumulative={fmt_num(module_tick_routing.get('cumulative_route_duration_ms'))}`",
        f"- duration_buckets: `lt_1ms={fmt_num(duration_buckets.get('lt_1ms'))} ms_1_to_5={fmt_num(duration_buckets.get('ms_1_to_5'))} ms_5_to_25={fmt_num(duration_buckets.get('ms_5_to_25'))} ms_25_to_100={fmt_num(duration_buckets.get('ms_25_to_100'))} ge_100ms={fmt_num(duration_buckets.get('ge_100ms'))}`",
    ]
)

lines.extend(["", "## Traffic Window"])
if traffic_summary is None:
    if traffic_summary_missing:
        lines.append(f"- traffic summary file missing: `{traffic_summary_path}`")
    else:
        lines.append("- no traffic summary attached")
else:
    latest = traffic_summary.get("latest") or {}
    window = traffic_summary.get("window") or {}
    lines.extend(
        [
            f"- covered_minutes: `{window.get('covered_minutes')}`",
            f"- full_window_covered: `{fmt_bool(window.get('full_window_covered'))}`",
            f"- restart_or_counter_reset_detected_within_window: `{fmt_bool(window.get('restart_or_counter_reset_detected_within_window'))}`",
            f"- latest_last_error: `{latest.get('last_error')}`",
        ]
    )
    traffic = traffic_summary.get("traffic") or {}
    for lane_name in ("udp_gossip", "libp2p_replication"):
        lane = traffic.get(lane_name) or {}
        totals = lane.get("totals") or {}
        inbound = totals.get("inbound") or {}
        outbound = totals.get("outbound") or {}
        counter_key = lane.get("counter_key") or ("datagrams" if lane_name == "udp_gossip" else "messages")
        line = (
            f"- {lane_name}: `in_{counter_key}={fmt_num(inbound.get(counter_key))} "
            f"out_{counter_key}={fmt_num(outbound.get(counter_key))} "
            f"in_payload={fmt_num(inbound.get('payload_bytes'))} "
            f"out_payload={fmt_num(outbound.get('payload_bytes'))}`"
        )
        if lane_name == "libp2p_replication":
            wire_totals = lane.get("wire_totals") or {}
            control_plane = lane.get("control_plane") or {}
            line += (
                f", substream_wire_total=`{fmt_bytes(((wire_totals.get('inbound') or {}).get('bytes', 0)) + ((wire_totals.get('outbound') or {}).get('bytes', 0)))}`"
            )
            if control_plane.get("available"):
                line += (
                    f", control_plane_wire_total=`{fmt_bytes(control_plane.get('total_wire_bytes'))}`"
                )
        lines.append(line)

lines.extend(adapter.markdown(profile_readiness))

Path(summary_md_path).write_text("\n".join(lines) + "\n", encoding="utf-8")
PY
