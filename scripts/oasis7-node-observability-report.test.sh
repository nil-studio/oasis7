#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

tmp_root="$(mktemp -d)"
trap 'rm -rf "$tmp_root"' EXIT

bash ./scripts/oasis7-node-observability-report.sh \
  --status-json-path fixtures/p2p_real_env_observability/local_status.json \
  --node-label local_node \
  --summary-json "$tmp_root/summary.json" \
  --summary-md "$tmp_root/summary.md"

python3 - "$tmp_root/summary.json" "$tmp_root/summary.md" <<'PY'
import json
import sys
from pathlib import Path

summary = json.loads(Path(sys.argv[1]).read_text())
markdown = Path(sys.argv[2]).read_text()

consensus = summary["consensus"]
assert consensus["recent_finality_latency"]["sample_count"] == 16
assert consensus["recent_finality_latency"]["p95_latency_ms"] == 220
transactions = summary["transactions"]
assert transactions["recent_confirmation_latency"]["sample_count"] == 0
assert transactions["recent_confirmation_latency"]["p95_latency_ms"] is None
assert "## Consensus / Transaction Latency" in markdown
assert "finality_latency: samples=`16`" in markdown
assert "transaction_confirmation_latency: samples=`0`" in markdown

runtime_perf = summary["runtime_perf"]
assert runtime_perf["health"] == "warn"
assert runtime_perf["bottleneck"] == "decision"
assert runtime_perf["decision"]["p95_ms"] == 24.2
assert runtime_perf["decision"]["over_budget_ratio_ppm"] == 125000
runtime_perf_smoke = summary["runtime_perf_smoke"]
assert runtime_perf_smoke["tick"]["p95_ms"] == 31.5
assert runtime_perf_smoke["tick"]["over_budget_ratio_ppm"] == 0
assert runtime_perf_smoke["action_execution"]["p95_ms"] == 14.8
assert "## Runtime Performance" in markdown
assert "health: `warn`" in markdown
assert "bottleneck: `decision`" in markdown
assert "smoke_tick: p95=`31.5`" in markdown
assert "smoke_action_execution: p95=`14.8`" in markdown
assert "decision: p95=`24.2`" in markdown
PY
