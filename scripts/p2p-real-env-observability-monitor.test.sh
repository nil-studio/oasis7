#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

tmp_root="$(mktemp -d)"
trap 'rm -rf "$tmp_root"' EXIT

python3 - <<'PY'
import importlib.util
from pathlib import Path

module_path = Path("scripts/p2p-real-env-observability-summary.py")
spec = importlib.util.spec_from_file_location("observability_summary", module_path)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)

summary = module.summarize_runtime_perf({
    "runtime_perf": {
        "health": "healthy",
        "bottleneck": "action_execution",
        "action_execution": {"p95_ms": 4.2, "over_budget_ratio_ppm": 0},
    },
})
assert summary["available"] is True
assert summary["alerts"] == []
assert summary["status"] == "ok"
PY

python3 ./scripts/p2p-real-env-observability-summary.py \
  --snapshot-summary fixtures/p2p_real_env_observability/snapshot_summary.json \
  --host-summary fixtures/p2p_real_env_observability/host_summary.json \
  --traffic-summary fixtures/p2p_real_env_observability/traffic_summary.json \
  --local-wasm-summary fixtures/p2p_real_env_observability/local_wasm_summary.json \
  --sequencer-wasm-summary fixtures/p2p_real_env_observability/sequencer_wasm_summary.json \
  --storage-wasm-summary fixtures/p2p_real_env_observability/storage_wasm_summary.json \
  --local-status-json fixtures/p2p_real_env_observability/local_status.json \
  --sequencer-status-json fixtures/p2p_real_env_observability/sequencer_status.json \
  --storage-status-json fixtures/p2p_real_env_observability/storage_status.json \
  --summary-json "$tmp_root/summary.json" \
  --summary-md "$tmp_root/summary.md" \
  --run-id test-run \
  --run-dir "$tmp_root/run"

python3 - "$tmp_root/summary.json" "$tmp_root/summary.md" <<'PY'
import json
import sys
from pathlib import Path

summary = json.loads(Path(sys.argv[1]).read_text())
markdown = Path(sys.argv[2]).read_text()
assert summary["nodes"]["sequencer_ecs"]["traffic"]["freshness"]["state"] == "unknown"
assert "traffic_freshness_unknown" in summary["nodes"]["sequencer_ecs"]["alerts"]
assert summary["snapshot"]["claim_status"] == "pass_candidate"
assert summary["overall"]["status"] == "pass_with_resource_alerts"
assert "sequencer_ecs" in summary["host"]["aggregate"]["alerted_nodes"]
local_node = summary["nodes"]["local_node"]
assert local_node["role"] == "observer"
assert local_node["host"]["runtime_cpu_percent"] == 47.3
assert local_node["runtime_perf"]["health"] == "warn"
assert local_node["runtime_perf"]["bottleneck"] == "decision"
assert local_node["runtime_perf"]["decision_p95_ms"] == 24.2
assert "runtime_perf_warn" in local_node["alerts"]
assert "runtime_perf_bottleneck_decision" in local_node["modules"]["runtime_perf"]["alerts"]
assert any(
    candidate["key"] == "runtime_perf_bottleneck_decision"
    for candidate in local_node["optimization_candidates"]
)
assert local_node["wasm"]["top_hotspot"] == "executor.entrypoint_call_ms_total"
assert "traffic_monitor_unavailable" in local_node["alerts"]
assert "traffic_samples_missing" in local_node["alerts"]
assert "traffic_window_uncovered" in local_node["alerts"]
assert "wasm_window_unavailable" in local_node["alerts"]
assert "wasm_metrics_unavailable" not in local_node["alerts"]
assert local_node["modules"]["consensus"]["status"] == "ok"
assert "traffic_monitor_unavailable" in local_node["modules"]["traffic_control_plane"]["alerts"]
assert "traffic_samples_missing" in local_node["modules"]["traffic_control_plane"]["alerts"]
assert "traffic_window_uncovered" in local_node["modules"]["traffic_control_plane"]["alerts"]
assert local_node["modules"]["traffic_control_plane"]["latest_fetch_error"] == "curl_failed"
assert "wasm" not in local_node["modules"]
assert "wasm_window_unavailable" in local_node["modules"]["wasm_executor_router"]["alerts"]
sequencer = summary["nodes"]["sequencer_ecs"]
assert "runtime_cpu_hot" in sequencer["alerts"]
assert sequencer["traffic"]["control_plane_total_events"] == 178
assert sequencer["modules"]["consensus"]["height_lag"] == 2
assert "control_plane_wire_share_high" in sequencer["modules"]["traffic_control_plane"]["alerts"]
assert "recent_replication_errors_high" in sequencer["modules"]["replication"]["alerts"]
assert "transaction_timeouts_present" in sequencer["modules"]["transactions"]["alerts"]
assert any(
    candidate["key"] == "libp2p_control_plane_churn"
    for candidate in sequencer["optimization_candidates"]
)
assert any(
    candidate["key"] == "replication_error_retry_churn"
    for candidate in summary["optimization_candidates"]
)
storage = summary["nodes"]["storage_ecs"]
assert storage["wasm"]["window_available"] is True
assert storage["modules"]["storage"]["status"] == "ok"
assert summary["traffic"]["aggregate"]["total_payload_bytes"] == 780646
assert summary["traffic"]["aggregate"]["network_interface"]["partial_coverage"] is True
coverage_warning = summary["traffic"]["aggregate"]["network_interface"]["coverage_warning"]
assert coverage_warning["code"] == "network_interface_partial_coverage"
assert coverage_warning["severity"] == "warn"
assert coverage_warning["skipped_unavailable_node_count"] == 1
assert coverage_warning["interface_counter_missing_node_count"] == 1
assert coverage_warning["missing_network_interface_node_count"] == 2
assert "traffic_network_interface_partial_coverage" in summary["overall"]["alerts"]
assert "Network interface coverage warning: partial" in markdown
assert "skipped_unavailable=`1`" in markdown
assert "interface_counter_missing=`1`" in markdown
assert "missing=`local_node, storage_ecs`" in markdown
assert summary["overall"]["optimization_candidate_count"] >= 3
PY

python3 ./scripts/p2p-real-env-observability-freshness.test.py

python3 ./scripts/p2p-real-env-observability-attempt.test.py

python3 ./scripts/p2p-real-env-observability-routing.test.py

bash ./scripts/p2p-real-env-host-monitor.test.sh

python3 ./scripts/p2p-real-env-host-report.test.py
