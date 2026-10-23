#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

tmp_root="$(mktemp -d)"
trap 'rm -rf "$tmp_root"' EXIT

history="$tmp_root/history.jsonl"
summary_json="$tmp_root/summary.json"
summary_md="$tmp_root/summary.md"

cat >"$history" <<'JSONL'
{"node_label":"local_node","captured_at":"2026-07-06T00:00:00Z","captured_at_unix_ms":1000000,"status_fetch_ok":true,"node_id":"local","role":"observer","running":true,"consensus":{"committed_height":10,"network_committed_height":10,"known_peer_heads":1},"traffic":{"udp_gossip":{"inbound":{"datagrams":1,"payload_bytes":100},"outbound":{"datagrams":1,"payload_bytes":100}},"libp2p_replication":{"gossip":{"inbound":{"messages":1,"payload_bytes":100},"outbound":{"messages":1,"payload_bytes":100}},"request":{"inbound":{"messages":0,"payload_bytes":0},"outbound":{"messages":0,"payload_bytes":0}},"response":{"inbound":{"messages":0,"payload_bytes":0},"outbound":{"messages":0,"payload_bytes":0}}}},"network_interface":{"available":true,"name":"en0","rx_bytes":1000,"tx_bytes":1000}}
{"node_label":"sequencer_ecs","captured_at":"2026-07-06T00:00:00Z","captured_at_unix_ms":1000000,"status_fetch_ok":true,"node_id":"seq","role":"sequencer","running":true,"consensus":{"committed_height":10,"network_committed_height":10,"known_peer_heads":1},"traffic":{"udp_gossip":{"inbound":{"datagrams":1,"payload_bytes":100},"outbound":{"datagrams":1,"payload_bytes":100}},"libp2p_replication":{"gossip":{"inbound":{"messages":1,"payload_bytes":100},"outbound":{"messages":1,"payload_bytes":100}},"request":{"inbound":{"messages":0,"payload_bytes":0},"outbound":{"messages":0,"payload_bytes":0}},"response":{"inbound":{"messages":0,"payload_bytes":0},"outbound":{"messages":0,"payload_bytes":0}}}},"network_interface":{"available":false,"reason":"interface counters unavailable"}}
{"node_label":"storage_ecs","captured_at":"2026-07-06T00:00:00Z","captured_at_unix_ms":1000000,"status_fetch_ok":false,"fetch_error":"ssh_failed"}
{"node_label":"local_node","captured_at":"2026-07-06T00:10:00Z","captured_at_unix_ms":1600000,"status_fetch_ok":true,"node_id":"local","role":"observer","running":true,"consensus":{"committed_height":20,"network_committed_height":20,"known_peer_heads":1},"traffic":{"udp_gossip":{"inbound":{"datagrams":2,"payload_bytes":200},"outbound":{"datagrams":2,"payload_bytes":200}},"libp2p_replication":{"gossip":{"inbound":{"messages":2,"payload_bytes":200},"outbound":{"messages":2,"payload_bytes":200}},"request":{"inbound":{"messages":0,"payload_bytes":0},"outbound":{"messages":0,"payload_bytes":0}},"response":{"inbound":{"messages":0,"payload_bytes":0},"outbound":{"messages":0,"payload_bytes":0}}}},"network_interface":{"available":true,"name":"en0","rx_bytes":2500,"tx_bytes":2500}}
{"node_label":"sequencer_ecs","captured_at":"2026-07-06T00:10:00Z","captured_at_unix_ms":1600000,"status_fetch_ok":true,"node_id":"seq","role":"sequencer","running":true,"consensus":{"committed_height":20,"network_committed_height":20,"known_peer_heads":1},"traffic":{"udp_gossip":{"inbound":{"datagrams":2,"payload_bytes":200},"outbound":{"datagrams":2,"payload_bytes":200}},"libp2p_replication":{"gossip":{"inbound":{"messages":2,"payload_bytes":200},"outbound":{"messages":2,"payload_bytes":200}},"request":{"inbound":{"messages":0,"payload_bytes":0},"outbound":{"messages":0,"payload_bytes":0}},"response":{"inbound":{"messages":0,"payload_bytes":0},"outbound":{"messages":0,"payload_bytes":0}}}},"network_interface":{"available":false,"reason":"interface counters unavailable"}}
{"node_label":"storage_ecs","captured_at":"2026-07-06T00:10:00Z","captured_at_unix_ms":1600000,"status_fetch_ok":false,"fetch_error":"ssh_failed"}
JSONL

python3 ./scripts/traffic-monitor-summary.py \
  --layout triad \
  --as-of-unix-ms 1600000 \
  --history-path "$history" \
  --summary-json "$summary_json" \
  --summary-md "$summary_md" \
  --window-minutes 10 \
  --history-retention-minutes 60 \
  --top-n 3 \
  --run-id traffic-partial-coverage-test \
  --label local_node \
  --label sequencer_ecs \
  --label storage_ecs

python3 - "$summary_json" "$summary_md" <<'PY'
import json
import sys
from pathlib import Path

summary = json.loads(Path(sys.argv[1]).read_text())
markdown = Path(sys.argv[2]).read_text()
network = summary["aggregate"]["network_interface"]

assert network["expected_node_count"] == 3
assert network["successful_node_count"] == 2
assert network["network_interface_available_node_count"] == 1
assert network["partial_coverage"] is True
assert network["missing_network_interface_nodes"] == ["sequencer_ecs", "storage_ecs"]
warning = network["coverage_warning"]
assert warning["code"] == "network_interface_partial_coverage"
assert warning["severity"] == "warn"
assert warning["skipped_unavailable_node_count"] == 1
assert warning["interface_counter_missing_node_count"] == 1
assert warning["missing_network_interface_node_count"] == 2
assert warning["skipped_unavailable_nodes"] == ["storage_ecs"]
assert warning["interface_counter_missing_nodes"] == ["sequencer_ecs"]
assert "Network interface coverage warning: partial" in markdown
assert "skipped_unavailable=`1`" in markdown
assert "skipped_nodes=`storage_ecs`" in markdown
assert "interface_counter_missing=`1`" in markdown
assert "interface_counter_missing_nodes=`sequencer_ecs`" in markdown
assert "missing=`sequencer_ecs, storage_ecs`" in markdown
PY

python3 ./scripts/traffic-monitor-freshness.test.py
