#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import time
from copy import deepcopy
from collections import Counter
from datetime import datetime, timezone
from pathlib import Path

NODE_LABELS = ("local_node", "sequencer_ecs", "storage_ecs")
SEVERITY_RANK = {"high": 0, "medium": 1, "low": 2}
IGNORED_WASM_DEGRADED_REASONS = {"build metrics path not configured"}


def parse_args():
    parser = argparse.ArgumentParser(
        description=(
            "Merge triad snapshot/host/traffic/wasm monitoring into one summary, "
            "including per-module breakdowns and optimization candidates."
        )
    )
    parser.add_argument("--as-of-unix-ms", type=int)
    parser.add_argument("--snapshot-summary", required=True)
    parser.add_argument("--host-summary", required=True)
    parser.add_argument("--traffic-summary", required=True)
    parser.add_argument("--local-wasm-summary", "--observer-wasm-summary", dest="local_wasm_summary", required=True)
    parser.add_argument("--sequencer-wasm-summary", required=True)
    parser.add_argument("--storage-wasm-summary", required=True)
    parser.add_argument("--local-status-json", "--observer-status-json", dest="local_status_json")
    parser.add_argument("--sequencer-status-json")
    parser.add_argument("--storage-status-json")
    parser.add_argument("--summary-json", required=True)
    parser.add_argument("--summary-md", required=True)
    parser.add_argument("--run-id")
    parser.add_argument("--run-dir")
    args = parser.parse_args()
    if args.as_of_unix_ms is not None and not 0 <= args.as_of_unix_ms <= time.time_ns() // 1_000_000:
        parser.error("--as-of-unix-ms must be non-negative and not later than current UTC time")
    return args


def load_json(path: str | None) -> dict:
    if not path:
        return {}
    raw = Path(path).read_text(encoding="utf-8")
    if not raw.strip():
        return {}
    return json.loads(raw)


def fmt_num(value):
    if value is None:
        return "n/a"
    if isinstance(value, float):
        return f"{value:.2f}"
    return f"{int(value):,}"


def fmt_percent(value):
    if value is None:
        return "n/a"
    return f"{float(value):.2f}%"


def fmt_ratio_as_percent(value):
    if value is None:
        return "n/a"
    return fmt_percent(float(value) * 100.0)


def fmt_bytes(value):
    if value is None:
        return "n/a"
    amount = float(value)
    for unit in ("B", "KiB", "MiB", "GiB", "TiB"):
        if amount < 1024.0 or unit == "TiB":
            return f"{amount:.2f} {unit}" if unit != "B" else f"{int(amount)} B"
        amount /= 1024.0
    return f"{int(value)} B"


def round_ratio(numerator, denominator):
    if numerator is None or denominator in (None, 0):
        return None
    return round(float(numerator) / float(denominator), 4)


def safe_int(value):
    if value is None:
        return 0
    return int(value)


def unique_sorted(values):
    return sorted({value for value in values if value not in (None, "")})


def determine_module_status(alerts: list[str], critical_alerts: set[str] | None = None) -> str:
    critical_alerts = critical_alerts or set()
    if any(alert in critical_alerts for alert in alerts):
        return "critical"
    if alerts:
        return "warn"
    return "ok"


def host_collection_complete(host: dict) -> bool:
    if "schema_version" not in host:
        return True  # Legacy summaries did not declare collection completeness.
    if host.get("schema_version") != 2:
        return False
    aggregate = host.get("aggregate") or {}
    expected = aggregate.get("expected_sample_count")
    if (type(expected) is not int or expected <= 0 or expected % len(NODE_LABELS)
            or aggregate.get("complete") is not True
            or aggregate.get("attempted_count") != expected
            or aggregate.get("failed_sample_count") != 0
            or aggregate.get("failed_nodes") != []):
        return False
    for label in NODE_LABELS:
        node = (host.get("nodes") or {}).get(label) or {}
        if (node.get("sample_count") != expected // len(NODE_LABELS)
                or node.get("success_sample_count") != node.get("sample_count")
                or node.get("failed_sample_count") != 0
                or node.get("available") is not True
                or (node.get("latest_attempt") or {}).get("result") != "success"
                or not node.get("latest")):
            return False
    return True


def summarize_host_runtime(host_node: dict) -> dict:
    latest_host = host_node.get("latest") or {}
    latest_runtime = latest_host.get("runtime") or {}
    latest_storage = latest_host.get("storage") or {}
    latest_service = latest_host.get("service") or {}
    alerts = list(host_node.get("alerts") or [])
    if latest_service.get("active_state") not in (None, "active"):
        alerts.append("service_inactive")
    unavailable = host_node.get("available") is False
    if unavailable:
        alerts.append("host_sample_unavailable")
    if not unavailable and latest_runtime.get("pid") is None:
        alerts.append("runtime_process_missing")
    alerts = unique_sorted(alerts)
    return {
        "status": determine_module_status(
            alerts,
            critical_alerts={"service_inactive", "runtime_process_missing", "host_sample_unavailable"},
        ),
        "alerts": alerts,
        "hostname": latest_host.get("hostname"),
        "cpu_cores": latest_host.get("cpu_cores"),
        "runtime_cpu_percent": latest_host.get("runtime_cpu_percent"),
        "runtime_cpu_core_ratio": latest_host.get("runtime_cpu_core_ratio"),
        "runtime_threads": latest_runtime.get("nlwp"),
        "loadavg_1m": latest_host.get("loadavg_1m"),
        "load_per_core_ratio_1m": latest_host.get("load_per_core_ratio_1m"),
        "mem_available_percent": latest_host.get("mem_available_percent"),
        "storage_used_percent": latest_storage.get("used_percent"),
        "service_active_state": latest_service.get("active_state"),
        "service_sub_state": latest_service.get("sub_state"),
    }


def summarize_runtime_perf(raw_status: dict) -> dict:
    runtime_perf = raw_status.get("runtime_perf") or {}
    alerts = []
    health = runtime_perf.get("health")
    bottleneck = runtime_perf.get("bottleneck")
    if health in ("warn", "critical"):
        alerts.append(f"runtime_perf_{health}")
    if health in ("warn", "critical") and bottleneck and bottleneck != "none":
        alerts.append(f"runtime_perf_bottleneck_{bottleneck}")
    return {
        "status": determine_module_status(
            alerts,
            critical_alerts={"runtime_perf_critical"},
        ) if runtime_perf else "unknown",
        "alerts": unique_sorted(alerts),
        "available": bool(runtime_perf),
        "health": health,
        "bottleneck": bottleneck,
        "tick_p95_ms": (runtime_perf.get("tick") or {}).get("p95_ms"),
        "decision_p95_ms": (runtime_perf.get("decision") or {}).get("p95_ms"),
        "action_execution_p95_ms": (runtime_perf.get("action_execution") or {}).get("p95_ms"),
        "callback_p95_ms": (runtime_perf.get("callback") or {}).get("p95_ms"),
        "llm_api_p95_ms": (runtime_perf.get("llm_api") or {}).get("p95_ms"),
        "decision_over_budget_ratio_ppm": (
            (runtime_perf.get("decision") or {}).get("over_budget_ratio_ppm")
        ),
        "raw": runtime_perf,
    }


def network_interface_coverage_warning(aggregate_network: dict) -> dict | None:
    if not aggregate_network or not aggregate_network.get("partial_coverage"):
        return None
    warning = aggregate_network.get("coverage_warning")
    if warning:
        return warning
    missing_nodes = aggregate_network.get("missing_network_interface_nodes") or []
    skipped_nodes = aggregate_network.get("skipped_unavailable_nodes") or []
    interface_counter_missing_nodes = (
        aggregate_network.get("interface_counter_missing_nodes") or []
    )
    if not skipped_nodes:
        successful = safe_int(aggregate_network.get("successful_node_count"))
        interface_available = safe_int(
            aggregate_network.get("network_interface_available_node_count")
        )
        skipped_count = max(len(missing_nodes) - max(successful - interface_available, 0), 0)
    else:
        skipped_count = len(skipped_nodes)
    if not interface_counter_missing_nodes:
        interface_counter_missing_count = max(len(missing_nodes) - skipped_count, 0)
    else:
        interface_counter_missing_count = len(interface_counter_missing_nodes)
    return {
        "code": "network_interface_partial_coverage",
        "severity": "warn",
        "summary": (
            "Network-interface aggregate excludes unavailable nodes or nodes "
            "without interface counters; totals only cover interface-available nodes."
        ),
        "expected_node_count": aggregate_network.get("expected_node_count", 0),
        "successful_node_count": aggregate_network.get("successful_node_count", 0),
        "network_interface_available_node_count": aggregate_network.get(
            "network_interface_available_node_count", 0
        ),
        "skipped_unavailable_node_count": skipped_count,
        "interface_counter_missing_node_count": interface_counter_missing_count,
        "missing_network_interface_node_count": len(missing_nodes),
        "missing_network_interface_nodes": missing_nodes,
        "skipped_unavailable_nodes": skipped_nodes,
        "interface_counter_missing_nodes": interface_counter_missing_nodes,
    }


def summarize_consensus(raw_status: dict) -> dict:
    consensus = raw_status.get("consensus") or {}
    network_head = consensus.get("network_head") or {}
    pending_actions = consensus.get("pending_consensus_actions") or {}
    recent_finality = consensus.get("recent_finality_latency") or {}
    inbound_timing = consensus.get("inbound_timing_rejections") or {}
    network_committed_height = consensus.get("network_committed_height")
    committed_height = consensus.get("committed_height")
    known_peer_heads = safe_int(consensus.get("known_peer_heads"))
    validator_stake_proof_count = safe_int(consensus.get("validator_stake_proof_count"))
    misbehavior_evidence_count = safe_int(consensus.get("misbehavior_evidence_count"))
    pending_slashing_intent_count = safe_int(consensus.get("pending_slashing_intent_count"))
    quarantined_validator_count = safe_int(consensus.get("quarantined_validator_count"))
    slashable_stake_total = safe_int(consensus.get("slashable_stake_total"))
    height_lag = None
    if network_committed_height is not None and committed_height is not None:
        height_lag = max(int(network_committed_height) - int(committed_height), 0)
    timing_rejections_total = sum(
        safe_int(inbound_timing.get(key))
        for key in (
            "proposal_future_slot",
            "proposal_stale_slot",
            "attestation_future_slot",
            "attestation_stale_slot",
            "attestation_epoch_mismatch",
        )
    )
    alerts = []
    if height_lag:
        alerts.append("network_height_lag")
    if (
        network_head.get("source") == "unknown"
        or (
            network_head.get("source") is None
            and safe_int(network_head.get("required_peer_count")) > 0
            and safe_int(network_head.get("fresh_peer_count")) <= 0
            and known_peer_heads <= 0
        )
    ):
        alerts.append("network_head_unknown")
    if network_head.get("decision") == "critical":
        alerts.append("network_head_critical")
    if safe_int(network_head.get("stale_peer_count")) > 0:
        alerts.append("network_head_stale")
    if safe_int(network_head.get("conflicting_peer_count")) > 0:
        alerts.append("network_head_conflict")
    if network_head.get("quorum_mode") == "stake_weighted" and network_head.get("stake_quorum_met") is False:
        alerts.append("stake_quorum_not_ready")
    if network_head.get("quorum_mode") == "count_fallback_stake_unavailable":
        alerts.append("stake_quorum_unavailable")
    if safe_int(network_head.get("total_stake")) > 0 and validator_stake_proof_count <= 0:
        alerts.append("validator_stake_proof_unavailable")
    if misbehavior_evidence_count > 0:
        alerts.append("consensus_misbehavior_evidence_present")
    if pending_slashing_intent_count > 0:
        alerts.append("consensus_slashing_intent_pending")
    if quarantined_validator_count > 0:
        alerts.append("consensus_validator_quarantined")
    sync = raw_status.get("sync") or {}
    if sync.get("status") == "stalled":
        alerts.append("sync_stalled")
    if (consensus.get("last_commit_age_ms") or 0) >= 15000:
        alerts.append("commit_age_high")
    if (
        safe_int(pending_actions.get("queued_action_count")) >= 10
        or safe_int(pending_actions.get("submit_buffer_action_count")) >= 10
    ):
        alerts.append("consensus_queue_backlog")
    if timing_rejections_total > 0:
        alerts.append("inbound_timing_rejections_present")
    if (recent_finality.get("p95_latency_ms") or 0) >= 2000:
        alerts.append("finality_latency_high")
    pending_proposal = consensus.get("pending_proposal") or {}
    if (pending_proposal.get("age_ms") or 0) >= 5000:
        alerts.append("pending_proposal_open")
    alerts = unique_sorted(alerts)
    return {
        "status": determine_module_status(
            alerts,
            critical_alerts={
                "network_height_lag",
                "network_head_critical",
                "network_head_conflict",
                "stake_quorum_not_ready",
                "stake_quorum_unavailable",
                "validator_stake_proof_unavailable",
                "consensus_misbehavior_evidence_present",
                "consensus_slashing_intent_pending",
                "consensus_validator_quarantined",
                "sync_stalled",
            },
        ),
        "alerts": alerts,
        "committed_height": committed_height,
        "network_committed_height": network_committed_height,
        "height_lag": height_lag,
        "last_commit_age_ms": consensus.get("last_commit_age_ms"),
        "known_peer_heads": consensus.get("known_peer_heads"),
        "validator_set_hash": consensus.get("validator_set_hash"),
        "validator_stake_root": consensus.get("validator_stake_root"),
        "validator_stake_proof_count": validator_stake_proof_count,
        "misbehavior_evidence_count": misbehavior_evidence_count,
        "slashing_intent_count": consensus.get("slashing_intent_count"),
        "pending_slashing_intent_count": pending_slashing_intent_count,
        "slashing_receipt_count": consensus.get("slashing_receipt_count"),
        "applied_slashing_receipt_count": consensus.get("applied_slashing_receipt_count"),
        "quarantined_validator_count": quarantined_validator_count,
        "slashable_stake_total": slashable_stake_total,
        "network_head": {
            "source": network_head.get("source"),
            "decision": network_head.get("decision"),
            "height": network_head.get("height"),
            "fresh_peer_count": network_head.get("fresh_peer_count"),
            "stale_peer_count": network_head.get("stale_peer_count"),
            "conflicting_peer_count": network_head.get("conflicting_peer_count"),
            "required_peer_count": network_head.get("required_peer_count"),
            "quorum_mode": network_head.get("quorum_mode"),
            "observed_stake": network_head.get("observed_stake"),
            "required_stake": network_head.get("required_stake"),
            "total_stake": network_head.get("total_stake"),
            "stake_quorum_met": network_head.get("stake_quorum_met"),
        },
        "pending_consensus_actions": {
            "queued_action_count": pending_actions.get("queued_action_count"),
            "queued_payload_bytes": pending_actions.get("queued_payload_bytes"),
            "submit_buffer_action_count": pending_actions.get("submit_buffer_action_count"),
            "submit_buffer_payload_bytes": pending_actions.get("submit_buffer_payload_bytes"),
        },
        "recent_finality_latency": {
            "sample_count": recent_finality.get("sample_count"),
            "avg_latency_ms": recent_finality.get("avg_latency_ms"),
            "p95_latency_ms": recent_finality.get("p95_latency_ms"),
            "max_latency_ms": recent_finality.get("max_latency_ms"),
        },
        "inbound_timing_rejections_total": timing_rejections_total,
        "last_timing_rejection_reason": inbound_timing.get("last_reason"),
    }


def summarize_observability(raw_status: dict) -> dict:
    observability = raw_status.get("observability") or {}
    alert_codes = unique_sorted(
        (alert or {}).get("code") for alert in (observability.get("alerts") or [])
    )
    alerts = list(alert_codes)
    if safe_int(observability.get("suspect_peer_count")) > 0:
        alerts.append("suspect_peers_present")
    if safe_int(observability.get("blocked_peer_count")) > 0:
        alerts.append("blocked_peers_present")
    if safe_int(observability.get("recent_replication_error_count")) > 0:
        alerts.append("recent_replication_errors_present")
    if observability.get("transport_stable") is False:
        alerts.append("replication_transport_unstable")
    if observability.get("reachability_policy_ok") is False:
        alerts.append("p2p_reachability_degraded")
    if safe_int(observability.get("misbehavior_evidence_count")) > 0:
        alerts.append("consensus_misbehavior_evidence_present")
    if safe_int(observability.get("pending_slashing_intent_count")) > 0:
        alerts.append("consensus_slashing_intent_pending")
    if safe_int(observability.get("quarantined_validator_count")) > 0:
        alerts.append("consensus_validator_quarantined")
    alerts = unique_sorted(alerts)
    status = observability.get("status") or determine_module_status(alerts)
    if status == "critical":
        status = "critical"
    elif status not in ("warn", "ok"):
        status = determine_module_status(alerts)
    return {
        "status": status,
        "alerts": alerts,
        "summary": observability.get("summary"),
        "connected_peer_count": observability.get("connected_peer_count"),
        "active_peer_count": observability.get("active_peer_count"),
        "candidate_peer_count": observability.get("candidate_peer_count"),
        "suspect_peer_count": observability.get("suspect_peer_count"),
        "blocked_peer_count": observability.get("blocked_peer_count"),
        "peer_with_issues_count": observability.get("peer_with_issues_count"),
        "known_peer_heads": observability.get("known_peer_heads"),
        "network_height_lag": observability.get("network_height_lag"),
        "transport_stable": observability.get("transport_stable"),
        "transport_stability_score": observability.get("transport_stability_score"),
        "reachability_policy_ok": observability.get("reachability_policy_ok"),
        "misbehavior_evidence_count": observability.get("misbehavior_evidence_count"),
        "slashing_intent_count": observability.get("slashing_intent_count"),
        "pending_slashing_intent_count": observability.get("pending_slashing_intent_count"),
        "slashing_receipt_count": observability.get("slashing_receipt_count"),
        "applied_slashing_receipt_count": observability.get("applied_slashing_receipt_count"),
        "quarantined_validator_count": observability.get("quarantined_validator_count"),
        "slashable_stake_total": observability.get("slashable_stake_total"),
        "recent_replication_error_count": observability.get("recent_replication_error_count"),
    }


def classify_replication_error(error_text: str) -> str:
    if "quarantine" in error_text:
        return "quarantine"
    if "request failed" in error_text:
        return "request_failed"
    if "connection closed" in error_text:
        return "connection_closed"
    if "connection established" in error_text:
        return "connection_established"
    return "other"


def summarize_replication(raw_status: dict) -> dict:
    replication = raw_status.get("replication") or {}
    peer_healths = replication.get("peer_healths") or []
    status_counts = Counter((peer or {}).get("status") for peer in peer_healths)
    recent_errors = replication.get("recent_errors") or []
    error_groups = Counter(classify_replication_error(error) for error in recent_errors)
    retry_cooldown = replication.get("protocol_retry_cooldown_peers") or {}
    transport_retry_cooldown_peers = replication.get("transport_retry_cooldown_peers") or []
    protocol_retry_cooldown_peer_count = sum(len(peers or []) for peers in retry_cooldown.values())
    transport_retry_cooldown_peer_count = len(transport_retry_cooldown_peers)
    retry_cooldown_peer_count = (
        protocol_retry_cooldown_peer_count + transport_retry_cooldown_peer_count
    )
    alerts = []
    if peer_healths and not (replication.get("connected_peers") or []):
        alerts.append("connected_peers_zero")
    if status_counts.get("blocked", 0) > 0:
        alerts.append("blocked_peers_present")
    if status_counts.get("suspect", 0) > 0:
        alerts.append("suspect_peers_present")
    if len(recent_errors) >= 20:
        alerts.append("recent_replication_errors_high")
    elif recent_errors:
        alerts.append("recent_replication_errors_present")
    if protocol_retry_cooldown_peer_count > 0:
        alerts.append("protocol_retry_cooldown_active")
    if transport_retry_cooldown_peer_count > 0:
        alerts.append("transport_retry_cooldown_active")
    alerts = unique_sorted(alerts)
    return {
        "status": determine_module_status(
            alerts,
            critical_alerts={"connected_peers_zero", "recent_replication_errors_high"},
        ),
        "alerts": alerts,
        "connected_peer_count": len(replication.get("connected_peers") or []),
        "peer_status_counts": dict(sorted((key, value) for key, value in status_counts.items() if key)),
        "registered_protocol_count": len(replication.get("registered_protocols") or []),
        "recent_error_count": len(recent_errors),
        "recent_error_groups": dict(sorted(error_groups.items())),
        "retry_cooldown_peer_count": retry_cooldown_peer_count,
        "protocol_retry_cooldown_peer_count": protocol_retry_cooldown_peer_count,
        "transport_retry_cooldown_peer_count": transport_retry_cooldown_peer_count,
    }


def summarize_storage(raw_status: dict) -> dict:
    storage = raw_status.get("storage") or {}
    bytes_by_dir = storage.get("bytes_by_dir") or {}
    total_bytes = sum(safe_int(value) for value in bytes_by_dir.values())
    replay_summary = storage.get("replay_summary") or {}
    alerts = []
    if storage.get("degraded_reason"):
        alerts.append("storage_degraded")
    if storage.get("last_gc_result") not in (None, "success"):
        alerts.append("storage_gc_failed")
    if safe_int(storage.get("orphan_blob_count")) > 0:
        alerts.append("orphan_blobs_present")
    alerts = unique_sorted(alerts)
    return {
        "status": determine_module_status(
            alerts,
            critical_alerts={"storage_degraded", "storage_gc_failed"},
        ),
        "alerts": alerts,
        "degraded_reason": storage.get("degraded_reason"),
        "checkpoint_count": storage.get("checkpoint_count"),
        "orphan_blob_count": storage.get("orphan_blob_count"),
        "ref_count": storage.get("ref_count"),
        "pin_count": storage.get("pin_count"),
        "total_bytes": total_bytes,
        "replay_summary": {
            "retained_height_count": replay_summary.get("retained_height_count"),
            "earliest_retained_height": replay_summary.get("earliest_retained_height"),
            "latest_retained_height": replay_summary.get("latest_retained_height"),
            "mode": replay_summary.get("mode"),
        },
        "last_gc_result": storage.get("last_gc_result"),
    }


def summarize_reward_runtime(raw_status: dict) -> dict:
    reward_runtime = raw_status.get("reward_runtime") or {}
    alerts = []
    if reward_runtime.get("enabled") and reward_runtime.get("metrics_available") is False:
        alerts.append("reward_metrics_unavailable")
    if reward_runtime.get("enabled") and reward_runtime.get("invariant_ok") is False:
        alerts.append("reward_invariant_failed")
    if reward_runtime.get("last_error"):
        alerts.append("reward_last_error")
    if (reward_runtime.get("distfs_failure_ratio") or 0.0) >= 0.05:
        alerts.append("reward_distfs_failure_ratio_high")
    if (reward_runtime.get("settlement_apply_failure_ratio") or 0.0) >= 0.05:
        alerts.append("reward_settlement_failure_ratio_high")
    alerts = unique_sorted(alerts)
    return {
        "status": determine_module_status(
            alerts,
            critical_alerts={"reward_invariant_failed", "reward_last_error"},
        ),
        "alerts": alerts,
        "enabled": reward_runtime.get("enabled"),
        "metrics_available": reward_runtime.get("metrics_available"),
        "invariant_ok": reward_runtime.get("invariant_ok"),
        "last_error": reward_runtime.get("last_error"),
        "latest_epoch_index": reward_runtime.get("latest_epoch_index"),
        "report_count": reward_runtime.get("report_count"),
        "distfs_failure_ratio": reward_runtime.get("distfs_failure_ratio"),
        "settlement_apply_failure_ratio": reward_runtime.get("settlement_apply_failure_ratio"),
    }


def summarize_transactions(raw_status: dict) -> dict:
    transactions = raw_status.get("transactions") or {}
    recent_latency = transactions.get("recent_confirmation_latency") or {}
    alerts = []
    if safe_int(transactions.get("pending_count")) >= 10:
        alerts.append("transaction_pending_backlog")
    if safe_int(transactions.get("inflight_count")) >= 5:
        alerts.append("transaction_inflight_backlog")
    if (transactions.get("oldest_inflight_age_ms") or 0) >= 5000:
        alerts.append("transaction_oldest_inflight_age_high")
    if safe_int(transactions.get("timeout_count")) > 0:
        alerts.append("transaction_timeouts_present")
    if (recent_latency.get("p95_latency_ms") or 0) >= 2000:
        alerts.append("transaction_confirmation_latency_high")
    alerts = unique_sorted(alerts)
    return {
        "status": determine_module_status(
            alerts,
            critical_alerts={"transaction_timeouts_present"},
        ),
        "alerts": alerts,
        "accepted_count": transactions.get("accepted_count"),
        "pending_count": transactions.get("pending_count"),
        "confirmed_count": transactions.get("confirmed_count"),
        "failed_count": transactions.get("failed_count"),
        "timeout_count": transactions.get("timeout_count"),
        "inflight_count": transactions.get("inflight_count"),
        "oldest_inflight_age_ms": transactions.get("oldest_inflight_age_ms"),
        "recent_confirmation_latency": {
            "sample_count": recent_latency.get("sample_count"),
            "p95_latency_ms": recent_latency.get("p95_latency_ms"),
            "max_latency_ms": recent_latency.get("max_latency_ms"),
        },
    }


def summarize_wasm(raw_status: dict, wasm_summary: dict) -> dict:
    raw_wasm = raw_status.get("wasm") or {}
    latest_wasm = wasm_summary.get("latest") or {}
    latest_wasm_metrics = latest_wasm.get("wasm") or latest_wasm
    wasm_window = wasm_summary.get("window") or {}
    build = raw_wasm.get("build") or {}
    executor = raw_wasm.get("executor") or {}
    router = raw_wasm.get("router") or {}
    degraded_reason = latest_wasm_metrics.get("degraded_reason") or raw_wasm.get("degraded_reason")
    alerts = []
    if latest_wasm_metrics.get("metrics_available") is False:
        alerts.append("wasm_metrics_unavailable")
    if latest_wasm_metrics.get("metrics_available") is True and wasm_window.get("available") is False:
        alerts.append("wasm_window_unavailable")
    if degraded_reason and degraded_reason not in IGNORED_WASM_DEGRADED_REASONS:
        alerts.append("wasm_degraded")
    if wasm_window.get("window_reset_detected") is True:
        alerts.append("wasm_counter_reset_detected")
    if safe_int(executor.get("compile_misses")) >= 5:
        alerts.append("wasm_compile_misses_high")
    elif safe_int(executor.get("compile_misses")) > 0:
        alerts.append("wasm_compile_misses_present")
    if safe_int(router.get("parse_fallbacks")) >= 10:
        alerts.append("wasm_router_parse_fallbacks_high")
    elif safe_int(router.get("parse_fallbacks")) > 0:
        alerts.append("wasm_router_parse_fallbacks_present")
    if (router.get("regex_compile_ms_total") or 0) >= 500:
        alerts.append("wasm_router_regex_compile_cost_high")
    alerts = unique_sorted(alerts)
    return {
        "status": determine_module_status(
            alerts,
            critical_alerts={"wasm_metrics_unavailable", "wasm_degraded"},
        ),
        "alerts": alerts,
        "metrics_available": latest_wasm_metrics.get("metrics_available"),
        "degraded_reason": degraded_reason,
        "window_available": wasm_window.get("available"),
        "window_notes": wasm_window.get("notes"),
        "window_reset_detected": wasm_window.get("window_reset_detected"),
        "top_hotspot": wasm_window.get("top_hotspot"),
        "build_metrics_available": build.get("metrics_available"),
        "executor_compile_misses": executor.get("compile_misses"),
        "executor_calls_total_delta": (wasm_window.get("executor") or {}).get("calls_total_delta"),
        "executor_entrypoint_call_ms_total": executor.get("entrypoint_call_ms_total"),
        "router_parse_fallbacks": router.get("parse_fallbacks"),
        "router_regex_compile_ms_total": router.get("regex_compile_ms_total"),
    }


def reevaluate_traffic(traffic: dict, as_of: int) -> dict:
    """Retain historical counters; never refresh evidence by report generation time."""
    traffic = deepcopy(traffic)
    policy = traffic.get("freshness_policy") or {}
    positive = lambda value: type(value) is int and value > 0
    valid_policy = (policy.get("version") == 1 and positive(policy.get("max_sample_age_ms"))
                    and positive(policy.get("max_sample_gap_ms")))
    stamp = lambda value: value if type(value) is int and value >= 0 else None
    child_as_of = stamp(traffic.get("as_of_unix_ms"))
    schema_ok = traffic.get("schema_version") == 2 and valid_policy
    for node in (traffic.get("nodes") or {}).values():
        prior = node.get("freshness") or {}
        attempt = node.get("latest_attempt") or {}
        success = node.get("last_success") or {}
        captured = stamp(attempt.get("captured_at_unix_ms"))
        successful = stamp(success.get("captured_at_unix_ms"))
        observed = stamp(success.get("status_observed_at_unix_ms"))
        reasons = list(prior.get("reasons") or [])
        boundary = stamp(node.get("as_of_unix_ms"))
        window = node.get("window") or {}
        end = stamp(window.get("end_unix_ms"))
        consistent = (schema_ok and child_as_of is not None and boundary == child_as_of
                      and end == child_as_of and child_as_of <= as_of)
        valid_times = (captured is not None and captured <= as_of and captured <= (child_as_of or 0)
                       and (successful is None or successful <= captured))
        age = as_of - captured if valid_times else None
        success_age = as_of - successful if successful is not None and successful <= as_of else None
        result = attempt.get("result")
        state = prior.get("state", "unknown")
        if state not in ("fresh", "failed", "stale", "unknown"):
            state = "unknown"
        if not consistent or not valid_times or result not in ("success", "failed"):
            state = "unknown"
            reasons.append("unsupported_or_inconsistent_freshness")
        elif state == "unknown":
            pass  # Clock anomalies discovered by the reducer remain fail-closed.
        elif result == "failed":
            state = "failed"
        elif age > policy["max_sample_age_ms"]:
            state = "stale"
        else:
            state = "fresh"
        producer_age = as_of - observed if observed is not None and observed <= as_of else None
        producer_state = (prior.get("producer") or {}).get("state", "unknown")
        if (not consistent or observed is None or successful is None or observed > successful
                or observed > as_of or producer_state not in ("fresh", "stale")):
            producer_state = "unknown"
        elif producer_age > policy["max_sample_age_ms"]:
            producer_state = "stale"
        else:
            producer_state = "fresh"
        if state != "fresh":
            reasons.append("report_sample_" + state)
        if producer_state != "fresh":
            reasons.append("report_producer_" + producer_state)
        right_gap = as_of - end if end is not None and end <= as_of else None
        current_window = (consistent and producer_state == "fresh" and state == "fresh" and right_gap is not None
                          and right_gap <= policy["max_sample_gap_ms"]
                          and window.get("full_window_covered") is True)
        node["reported_window"] = deepcopy(window)
        node["window"] = dict(window, full_window_covered=current_window)
        node["freshness"] = {"state": state, "sample_age_ms": age,
                             "last_success_age_ms": success_age,
                             "producer": {"state": producer_state, "observed_at_unix_ms": observed,
                                          "age_ms": producer_age}, "reasons": unique_sorted(reasons)}
        node["current_measurement_available"] = (state == "fresh" and producer_state == "fresh"
                                                  and current_window and node.get("available") is True)
        node["available"] = node["current_measurement_available"]
    traffic["report_as_of_unix_ms"] = as_of
    traffic["current_measurement_available"] = bool(traffic.get("nodes")) and all(
        node.get("current_measurement_available") for node in (traffic.get("nodes") or {}).values())
    return traffic


def summarize_traffic_control_plane(traffic_node: dict) -> dict:
    latest_traffic = traffic_node.get("latest") or {}
    traffic_window = traffic_node.get("window") or {}
    traffic = traffic_node.get("traffic") or {}
    libp2p = traffic.get("libp2p_replication") or {}
    udp = traffic.get("udp_gossip") or {}
    control_plane = libp2p.get("control_plane") or {}
    covered_minutes = traffic_window.get("covered_minutes")
    sample_count_successful = traffic_window.get("sample_count_successful", traffic_node.get("sample_count_successful"))
    payload_values = [libp2p.get("total_payload_bytes"), udp.get("total_payload_bytes")]
    payload_total_bytes = sum(value for value in payload_values if value is not None) if any(value is not None for value in payload_values) else None
    libp2p_wire_bytes = libp2p.get("total_wire_bytes")
    control_plane_wire_bytes = control_plane.get("total_wire_bytes")
    control_plane_total_events = control_plane.get("total_events")
    control_plane_wire_ratio = round_ratio(control_plane_wire_bytes, libp2p_wire_bytes)
    wire_over_payload_ratio = round_ratio(libp2p_wire_bytes, payload_total_bytes)
    control_plane_events_per_minute = None
    if covered_minutes not in (None, 0):
        control_plane_events_per_minute = round(float(control_plane_total_events or 0) / float(covered_minutes), 2)
    freshness = traffic_node.get("freshness") or {"state": "unknown", "producer": {"state": "unknown"}}
    alerts = []
    if freshness.get("state") != "fresh":
        alerts.append("traffic_freshness_" + freshness.get("state", "unknown"))
    if (freshness.get("producer") or {}).get("state") != "fresh":
        alerts.append("traffic_producer_" + (freshness.get("producer") or {}).get("state", "unknown"))
    if traffic_node.get("available") is False:
        alerts.append("traffic_monitor_unavailable")
    if latest_traffic.get("last_error"):
        alerts.append("traffic_monitor_error")
    if sample_count_successful == 0 or (sample_count_successful is None and covered_minutes == 0):
        alerts.append("traffic_samples_missing")
    if traffic_window.get("full_window_covered") is False:
        alerts.append("traffic_window_uncovered")
    if safe_int(control_plane_total_events) >= 100:
        alerts.append("control_plane_events_high")
    if (control_plane_wire_ratio or 0.0) >= 0.6:
        alerts.append("control_plane_wire_share_high")
    if (wire_over_payload_ratio or 0.0) >= 2.0:
        alerts.append("traffic_wire_overhead_high")
    alerts = unique_sorted(alerts)
    return {
        "status": determine_module_status(alerts),
        "alerts": alerts,
        "freshness": freshness,
        "current_measurement_available": traffic_node.get("current_measurement_available", False),
        "payload_total_bytes": payload_total_bytes,
        "libp2p_total_wire_bytes": libp2p_wire_bytes,
        "control_plane_total_events": control_plane_total_events,
        "control_plane_total_wire_bytes": control_plane_wire_bytes,
        "control_plane_wire_ratio": control_plane_wire_ratio,
        "wire_over_payload_ratio": wire_over_payload_ratio,
        "control_plane_events_per_minute": control_plane_events_per_minute,
        "window_covered_minutes": covered_minutes,
        "window_full_covered": traffic_window.get("full_window_covered"),
        "sample_count_successful": sample_count_successful,
        "last_error": latest_traffic.get("last_error"),
        "latest_fetch_error": latest_traffic.get("latest_fetch_error") or traffic_node.get("latest_fetch_error"),
    }


def summarize_p2p_reachability(raw_status: dict) -> dict:
    p2p = raw_status.get("p2p") or {}
    observability = raw_status.get("observability") or {}
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
    direct_addr_count = len(p2p.get("confirmed_external_direct_addrs") or [])
    alerts = []
    if p2p.get("probe_stable") is False:
        alerts.append("reachability_probe_unstable")
    effective_mode = p2p.get("effective_user_mode")
    if effective_mode == "public_entry" and direct_addr_count == 0 and p2p.get("relay_available") is not True:
        alerts.append("public_entry_without_reachable_path")
    if effective_mode == "relay_only" and p2p.get("relay_available") is not True:
        alerts.append("relay_path_unavailable")
    alerts = unique_sorted(alerts)
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
    last_transition = path_observability.get("last_transition") or {}
    if last_transition:
        recent_fallback_reason = "path_transition"
    elif selected_path_kind == "relay_reserved":
        recent_fallback_reason = "relay_reserved"
    elif selected_path_kind == "not_reported":
        recent_fallback_reason = "not_reported"
    else:
        recent_fallback_reason = "unknown"
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
    return {
        "status": determine_module_status(alerts),
        "alerts": alerts,
        "requested_user_mode": p2p.get("requested_user_mode"),
        "recommended_user_mode": p2p.get("recommended_user_mode"),
        "effective_user_mode": effective_mode,
        "deployment_mode": p2p.get("deployment_mode"),
        "node_role_claim": p2p.get("node_role_claim"),
        "autonat_status": p2p.get("autonat_status"),
        "public_port_reachability": p2p.get("public_port_reachability"),
        "relay_available": p2p.get("relay_available"),
        "probe_stable": p2p.get("probe_stable"),
        "direct_addr_count": direct_addr_count,
        "selected_path_kind": selected_path_kind,
        "selected_path_age_ms": "not_reported" if selected_path_age_ms is None else selected_path_age_ms,
        "path_transition_counters": path_transition_counters,
        "active_path_mix": active_path_mix,
        "recent_fallback_reason": recent_fallback_reason,
        "reachability_confidence": reachability_confidence,
    }


def summarize_modules(snapshot_node: dict, host_node: dict, traffic_node: dict, raw_status: dict, wasm_summary: dict) -> dict:
    return {
        "host_runtime": summarize_host_runtime(host_node),
        "runtime_perf": summarize_runtime_perf(raw_status),
        "consensus": summarize_consensus(raw_status),
        "observability": summarize_observability(raw_status),
        "replication": summarize_replication(raw_status),
        "storage": summarize_storage(raw_status),
        "reward_runtime": summarize_reward_runtime(raw_status),
        "transactions": summarize_transactions(raw_status),
        "wasm_executor_router": summarize_wasm(raw_status, wasm_summary),
        "traffic_control_plane": summarize_traffic_control_plane(traffic_node),
        "p2p_reachability": summarize_p2p_reachability(raw_status),
        "snapshot_window": {
            "status": "ok" if snapshot_node.get("healthz_all_ok") and snapshot_node.get("status_fetch_all_ok") else "warn",
            "alerts": unique_sorted(snapshot_node.get("last_errors") or []),
            "sample_count": snapshot_node.get("sample_count"),
            "healthz_all_ok": snapshot_node.get("healthz_all_ok"),
            "status_fetch_all_ok": snapshot_node.get("status_fetch_all_ok"),
            "committed_height_first": ((snapshot_node.get("heights") or {}).get("first_committed_height")),
            "committed_height_last": ((snapshot_node.get("heights") or {}).get("last_committed_height")),
        },
    }


def build_candidate(node_label: str, module: str, severity: str, key: str, summary: str, evidence: dict, suggested_optimizations: list[str]) -> dict:
    return {
        "node_label": node_label,
        "module": module,
        "severity": severity,
        "key": key,
        "summary": summary,
        "evidence": evidence,
        "suggested_optimizations": suggested_optimizations,
    }


def sort_candidates(candidates: list[dict]) -> list[dict]:
    return sorted(
        candidates,
        key=lambda candidate: (
            SEVERITY_RANK.get(candidate.get("severity"), 99),
            candidate.get("node_label") or "",
            candidate.get("module") or "",
            candidate.get("key") or "",
        ),
    )


def derive_optimization_candidates(node_label: str, modules: dict) -> list[dict]:
    candidates = []
    host = modules.get("host_runtime") or {}
    consensus = modules.get("consensus") or {}
    replication = modules.get("replication") or {}
    transactions = modules.get("transactions") or {}
    wasm = modules.get("wasm_executor_router") or {}
    traffic = modules.get("traffic_control_plane") or {}
    storage = modules.get("storage") or {}
    reward_runtime = modules.get("reward_runtime") or {}
    p2p = modules.get("p2p_reachability") or {}
    runtime_perf = modules.get("runtime_perf") or {}

    runtime_hot = (host.get("runtime_cpu_core_ratio") or 0.0) >= 0.75
    traffic_chatter = (
        (traffic.get("control_plane_total_events") or 0) >= 100
        or (traffic.get("control_plane_wire_ratio") or 0.0) >= 0.6
    )
    if runtime_hot and traffic_chatter:
        candidates.append(
            build_candidate(
                node_label=node_label,
                module="traffic_control_plane",
                severity="high",
                key="libp2p_control_plane_churn",
                summary=(
                    "Runtime CPU is hot while libp2p control-plane chatter dominates the traffic window."
                ),
                evidence={
                    "runtime_cpu_core_ratio": host.get("runtime_cpu_core_ratio"),
                    "control_plane_total_events": traffic.get("control_plane_total_events"),
                    "control_plane_wire_ratio": traffic.get("control_plane_wire_ratio"),
                    "wire_over_payload_ratio": traffic.get("wire_over_payload_ratio"),
                },
                suggested_optimizations=[
                    "Reduce peer-manager reconnect and discovery churn on the hot node.",
                    "Throttle or dedupe high-frequency control-plane messages before they hit libp2p replication.",
                ],
            )
        )

    runtime_perf_health = runtime_perf.get("health")
    runtime_perf_bottleneck = runtime_perf.get("bottleneck")
    if runtime_perf_health in ("warn", "critical") and runtime_perf_bottleneck not in (
        None,
        "",
        "none",
    ):
        candidates.append(
            build_candidate(
                node_label=node_label,
                module="runtime_perf",
                severity="high" if runtime_perf_health == "critical" else "medium",
                key=f"runtime_perf_bottleneck_{runtime_perf_bottleneck}",
                summary=(
                    "Runtime performance snapshot reports a bottleneck that should be visible in operator triad summaries."
                ),
                evidence={
                    "health": runtime_perf_health,
                    "bottleneck": runtime_perf_bottleneck,
                    "tick_p95_ms": runtime_perf.get("tick_p95_ms"),
                    "decision_p95_ms": runtime_perf.get("decision_p95_ms"),
                    "action_execution_p95_ms": runtime_perf.get("action_execution_p95_ms"),
                    "callback_p95_ms": runtime_perf.get("callback_p95_ms"),
                    "llm_api_p95_ms": runtime_perf.get("llm_api_p95_ms"),
                    "decision_over_budget_ratio_ppm": runtime_perf.get(
                        "decision_over_budget_ratio_ppm"
                    ),
                },
                suggested_optimizations=[
                    "Inspect the reported runtime_perf bottleneck before treating node health as only resource or network pressure.",
                    "Correlate runtime_perf p95 and over-budget ratios with host and traffic windows for the same node.",
                ],
            )
        )

    if runtime_hot and (replication.get("recent_error_count") or 0) >= 20:
        candidates.append(
            build_candidate(
                node_label=node_label,
                module="replication",
                severity="high",
                key="replication_error_retry_churn",
                summary=(
                    "Replication retry/error churn is high on a CPU-hot node, suggesting reconnect backoff or quarantine handling is amplifying cost."
                ),
                evidence={
                    "runtime_cpu_core_ratio": host.get("runtime_cpu_core_ratio"),
                    "recent_error_count": replication.get("recent_error_count"),
                    "recent_error_groups": replication.get("recent_error_groups"),
                    "peer_status_counts": replication.get("peer_status_counts"),
                },
                suggested_optimizations=[
                    "Collapse repeated connection-closed retry loops and raise quarantine gating earlier.",
                    "Trim noisy retry paths or logging on repeated peer failures.",
                ],
            )
        )

    if (
        (consensus.get("height_lag") or 0) > 0
        or safe_int((consensus.get("pending_consensus_actions") or {}).get("queued_action_count")) >= 10
        or (consensus.get("last_commit_age_ms") or 0) >= 15000
    ):
        candidates.append(
            build_candidate(
                node_label=node_label,
                module="consensus",
                severity="medium",
                key="consensus_pipeline_backlog",
                summary=(
                    "Consensus is showing backlog or lag signals that can be reduced before they become visible chain stalls."
                ),
                evidence={
                    "height_lag": consensus.get("height_lag"),
                    "last_commit_age_ms": consensus.get("last_commit_age_ms"),
                    "queued_action_count": ((consensus.get("pending_consensus_actions") or {}).get("queued_action_count")),
                    "p95_finality_latency_ms": ((consensus.get("recent_finality_latency") or {}).get("p95_latency_ms")),
                },
                suggested_optimizations=[
                    "Inspect submit-buffer pressure and action batching on the lagging node.",
                    "Profile consensus scheduling around commit age spikes and timing rejection paths.",
                ],
            )
        )

    if (
        (transactions.get("pending_count") or 0) >= 10
        or (transactions.get("oldest_inflight_age_ms") or 0) >= 5000
        or (transactions.get("timeout_count") or 0) > 0
    ):
        candidates.append(
            build_candidate(
                node_label=node_label,
                module="transactions",
                severity="medium",
                key="transaction_submit_backlog",
                summary=(
                    "Transaction submit/confirm metrics show a backlog that warrants queue and retry-path tuning."
                ),
                evidence={
                    "pending_count": transactions.get("pending_count"),
                    "inflight_count": transactions.get("inflight_count"),
                    "oldest_inflight_age_ms": transactions.get("oldest_inflight_age_ms"),
                    "timeout_count": transactions.get("timeout_count"),
                },
                suggested_optimizations=[
                    "Shorten slow retry paths for inflight submissions and inspect timeouts.",
                    "Reduce queue buildup before it surfaces as user-visible confirmation latency.",
                ],
            )
        )

    if (
        (wasm.get("executor_compile_misses") or 0) >= 5
        or (wasm.get("router_parse_fallbacks") or 0) >= 10
        or (
            runtime_hot
            and wasm.get("top_hotspot") == "executor.entrypoint_call_ms_total"
            and (wasm.get("executor_calls_total_delta") or 0) >= 8
        )
    ):
        candidates.append(
            build_candidate(
                node_label=node_label,
                module="wasm_executor_router",
                severity="medium",
                key="wasm_hotspot_tuning",
                summary=(
                    "WASM execution or routing metrics show repeated hotspot activity that is likely contributing to runtime pressure."
                ),
                evidence={
                    "top_hotspot": wasm.get("top_hotspot"),
                    "executor_calls_total_delta": wasm.get("executor_calls_total_delta"),
                    "executor_compile_misses": wasm.get("executor_compile_misses"),
                    "router_parse_fallbacks": wasm.get("router_parse_fallbacks"),
                    "router_regex_compile_ms_total": wasm.get("router_regex_compile_ms_total"),
                },
                suggested_optimizations=[
                    "Cache module/router preparation work more aggressively on the hot node.",
                    "Inspect repeated entrypoint or regex-compile paths before adding more load.",
                ],
            )
        )

    if storage.get("alerts"):
        candidates.append(
            build_candidate(
                node_label=node_label,
                module="storage",
                severity="medium",
                key="storage_gc_or_retention_tuning",
                summary=(
                    "Storage degradation or orphan/GC signals indicate retention and cleanup tuning work is still needed."
                ),
                evidence={
                    "alerts": storage.get("alerts"),
                    "orphan_blob_count": storage.get("orphan_blob_count"),
                    "last_gc_result": storage.get("last_gc_result"),
                    "degraded_reason": storage.get("degraded_reason"),
                },
                suggested_optimizations=[
                    "Audit retention and GC cadence before the node hits storage pressure.",
                    "Investigate orphaned blobs or failed GC runs on the affected node.",
                ],
            )
        )

    if reward_runtime.get("alerts"):
        candidates.append(
            build_candidate(
                node_label=node_label,
                module="reward_runtime",
                severity="medium",
                key="reward_runtime_stability",
                summary=(
                    "Reward runtime metrics show degradation or failures that should be stabilized before they accumulate."
                ),
                evidence={
                    "alerts": reward_runtime.get("alerts"),
                    "distfs_failure_ratio": reward_runtime.get("distfs_failure_ratio"),
                    "settlement_apply_failure_ratio": reward_runtime.get("settlement_apply_failure_ratio"),
                    "last_error": reward_runtime.get("last_error"),
                },
                suggested_optimizations=[
                    "Inspect failing reward report or settlement paths before they become persistent debt.",
                    "Gate the hot path on the first invariant/error signal instead of letting failures accumulate.",
                ],
            )
        )

    if p2p.get("alerts"):
        candidates.append(
            build_candidate(
                node_label=node_label,
                module="p2p_reachability",
                severity="low",
                key="p2p_reachability_stabilization",
                summary=(
                    "P2P reachability posture still has unstable or incomplete routing signals that can magnify retry cost."
                ),
                evidence={
                    "alerts": p2p.get("alerts"),
                    "effective_user_mode": p2p.get("effective_user_mode"),
                    "relay_available": p2p.get("relay_available"),
                    "probe_stable": p2p.get("probe_stable"),
                },
                suggested_optimizations=[
                    "Stabilize reachability probes or relay availability before expanding the node's public role.",
                    "Keep P2P mode-specific routing fallbacks explicit when direct reachability is absent.",
                ],
            )
        )

    return sort_candidates(candidates)


def derive_node_summary(label: str, snapshot: dict, host: dict, traffic: dict, raw_status: dict, wasm: dict) -> dict:
    snapshot_node = ((snapshot.get("nodes") or {}).get(label)) or {}
    host_node = ((host.get("nodes") or {}).get(label)) or {}
    traffic_node = ((traffic.get("nodes") or {}).get(label)) or {}
    wasm_window = wasm.get("window") or {}
    latest_wasm = wasm.get("latest") or {}
    latest_wasm_metrics = latest_wasm.get("wasm") or latest_wasm
    raw_status = raw_status or {}

    roles = snapshot_node.get("roles") or []
    role = roles[0] if roles else raw_status.get("role") or (traffic_node.get("latest") or {}).get("role")
    latest_host = host_node.get("latest") or {}
    latest_runtime = latest_host.get("runtime") or {}
    latest_storage = latest_host.get("storage") or {}
    latest_traffic = traffic_node.get("latest") or {}
    libp2p = (traffic_node.get("traffic") or {}).get("libp2p_replication") or {}
    payload_values = [libp2p.get("total_payload_bytes"),
                      ((traffic_node.get("traffic") or {}).get("udp_gossip") or {}).get("total_payload_bytes")]
    payload_bytes = sum(value for value in payload_values if value is not None) if any(value is not None for value in payload_values) else None

    modules = summarize_modules(snapshot_node, host_node, traffic_node, raw_status, wasm)
    optimization_candidates = derive_optimization_candidates(label, modules)

    alerts = []
    alerts.extend(snapshot.get("failure_signatures") or [])
    alerts.extend(host_node.get("alerts") or [])
    if latest_wasm_metrics.get("metrics_available") is not True:
        alerts.append("wasm_metrics_unavailable")
    if (latest_wasm_metrics.get("degraded_reason") or ""):
        alerts.append("wasm_degraded")
    if wasm_window.get("window_reset_detected") is True:
        alerts.append("wasm_counter_reset_detected")
    for module_summary in modules.values():
        alerts.extend(module_summary.get("alerts") or [])

    return {
        "label": label,
        "role": role,
        "node_id": raw_status.get("node_id") or ((snapshot_node.get("node_ids") or [None])[0]),
        "snapshot": {
            "sample_count": snapshot_node.get("sample_count"),
            "healthz_all_ok": snapshot_node.get("healthz_all_ok"),
            "status_fetch_all_ok": snapshot_node.get("status_fetch_all_ok"),
            "committed_height_first": ((snapshot_node.get("heights") or {}).get("first_committed_height")),
            "committed_height_last": ((snapshot_node.get("heights") or {}).get("last_committed_height")),
            "known_peer_heads_max": ((snapshot_node.get("peers") or {}).get("max_known_peer_heads")),
            "last_errors": snapshot_node.get("last_errors") or [],
        },
        "host": {
            **{key: host_node.get(key) for key in ("available", "latest_attempt", "last_success", "sample_count", "success_sample_count", "failed_sample_count")},
            "hostname": latest_host.get("hostname"),
            "cpu_cores": latest_host.get("cpu_cores"),
            "runtime_cpu_percent": latest_host.get("runtime_cpu_percent"),
            "runtime_cpu_core_ratio": latest_host.get("runtime_cpu_core_ratio"),
            "runtime_threads": latest_runtime.get("nlwp"),
            "loadavg_1m": latest_host.get("loadavg_1m"),
            "load_per_core_ratio_1m": latest_host.get("load_per_core_ratio_1m"),
            "mem_available_percent": latest_host.get("mem_available_percent"),
            "storage_used_percent": latest_storage.get("used_percent"),
            "status": host_node.get("status") or {},
        },
        "runtime_perf": {
            "available": (modules.get("runtime_perf") or {}).get("available"),
            "health": (modules.get("runtime_perf") or {}).get("health"),
            "bottleneck": (modules.get("runtime_perf") or {}).get("bottleneck"),
            "tick_p95_ms": (modules.get("runtime_perf") or {}).get("tick_p95_ms"),
            "decision_p95_ms": (modules.get("runtime_perf") or {}).get("decision_p95_ms"),
            "action_execution_p95_ms": (
                (modules.get("runtime_perf") or {}).get("action_execution_p95_ms")
            ),
            "callback_p95_ms": (modules.get("runtime_perf") or {}).get("callback_p95_ms"),
            "llm_api_p95_ms": (modules.get("runtime_perf") or {}).get("llm_api_p95_ms"),
            "decision_over_budget_ratio_ppm": (
                (modules.get("runtime_perf") or {}).get("decision_over_budget_ratio_ppm")
            ),
        },
        "traffic": {
            "freshness": traffic_node.get("freshness"),
            "latest_attempt": traffic_node.get("latest_attempt"),
            "last_success": traffic_node.get("last_success"),
            "as_of_unix_ms": traffic_node.get("as_of_unix_ms"),
            "reported_window": traffic_node.get("reported_window"),
            "current_measurement_available": traffic_node.get("current_measurement_available", False),
            "counter_semantics": "historical_window",
            "payload_total_bytes": payload_bytes,
            "window_covered_minutes": ((traffic_node.get("window") or {}).get("covered_minutes")),
            "libp2p_total_wire_bytes": libp2p.get("total_wire_bytes"),
            "control_plane_total_events": ((libp2p.get("control_plane") or {}).get("total_events")),
            "control_plane_total_wire_bytes": ((libp2p.get("control_plane") or {}).get("total_wire_bytes")),
            "last_error": latest_traffic.get("last_error"),
        },
        "wasm": {
            "metrics_available": latest_wasm_metrics.get("metrics_available"),
            "degraded_reason": latest_wasm_metrics.get("degraded_reason"),
            "window_available": wasm_window.get("available"),
            "window_reset_detected": wasm_window.get("window_reset_detected"),
            "top_hotspot": wasm_window.get("top_hotspot"),
            "executor_calls_total_delta": (((wasm_window.get("executor") or {}).get("calls_total_delta"))),
        },
        "modules": modules,
        "optimization_candidates": optimization_candidates,
        "alerts": unique_sorted(alerts),
    }


def render_markdown(summary: dict) -> list[str]:
    lines = [
        "# P2P Real Env Observability Summary",
        "",
        f"- Report as-of Unix ms: `{summary.get('as_of_unix_ms')}` (generation time does not renew evidence)",
        f"- Generated at: `{summary['generated_at']}`",
        f"- Run id: `{summary.get('run_id') or 'n/a'}`",
        f"- Snapshot claim_status: `{summary['snapshot'].get('claim_status')}`",
        f"- Overall verdict: `{summary['overall']['status']}`",
        f"- Overall alerts: `{', '.join(summary['overall'].get('alerts') or ['(none)'])}`",
        f"- Optimization candidates: `{fmt_num(len(summary.get('optimization_candidates') or []))}`",
        f"- Traffic total payload: `{fmt_bytes(((summary.get('traffic') or {}).get('aggregate') or {}).get('total_payload_bytes'))}`",
        "",
    ]
    aggregate = (summary.get("traffic") or {}).get("aggregate") or {}
    lane_distribution = aggregate.get("lane_distribution") or {}
    aggregate_network = aggregate.get("network_interface") or {}
    if lane_distribution:
        lines.append(
            "- Aggregate lanes: "
            + f"udp `{fmt_bytes(((lane_distribution.get('udp_gossip') or {}).get('payload_bytes')))}` ({fmt_percent(((lane_distribution.get('udp_gossip') or {}).get('share_percent')))}), "
            + f"libp2p `{fmt_bytes(((lane_distribution.get('libp2p_replication') or {}).get('payload_bytes')))}` ({fmt_percent(((lane_distribution.get('libp2p_replication') or {}).get('share_percent')))})."
        )
        lines.append("")
    if aggregate_network:
        coverage_label = "partial" if aggregate_network.get("partial_coverage") else "complete"
        missing_nodes = ", ".join(aggregate_network.get("missing_network_interface_nodes") or [])
        coverage_warning = network_interface_coverage_warning(aggregate_network) or {}
        if coverage_warning:
            aggregate_network["coverage_warning"] = coverage_warning
        skipped_unavailable = coverage_warning.get(
            "skipped_unavailable_node_count",
            len(aggregate_network.get("skipped_unavailable_nodes") or []),
        )
        interface_counter_missing = coverage_warning.get(
            "interface_counter_missing_node_count",
            len(aggregate_network.get("interface_counter_missing_nodes") or []),
        )
        skipped_unavailable_nodes = ", ".join(
            coverage_warning.get("skipped_unavailable_nodes")
            or aggregate_network.get("skipped_unavailable_nodes")
            or []
        )
        interface_counter_missing_nodes = ", ".join(
            coverage_warning.get("interface_counter_missing_nodes")
            or aggregate_network.get("interface_counter_missing_nodes")
            or []
        )
        coverage_prefix = (
            "Network interface coverage warning"
            if aggregate_network.get("partial_coverage")
            else "Network interface coverage"
        )
        lines.append(
            f"- {coverage_prefix}: {coverage_label}; expected=`{aggregate_network.get('expected_node_count', 0)}` successful=`{aggregate_network.get('successful_node_count', 0)}` interface_available=`{aggregate_network.get('network_interface_available_node_count', 0)}` skipped_unavailable=`{skipped_unavailable}` skipped_nodes=`{skipped_unavailable_nodes or 'none'}` interface_counter_missing=`{interface_counter_missing}` interface_counter_missing_nodes=`{interface_counter_missing_nodes or 'none'}` missing=`{missing_nodes or 'none'}`"
        )
        lines.append("")
    if summary.get("optimization_candidates"):
        lines.append("## Aggregate Optimization Candidates")
        for candidate in summary.get("optimization_candidates") or []:
            lines.append(
                f"- [{candidate.get('severity')}] `{candidate.get('node_label')}/{candidate.get('module')}/{candidate.get('key')}`: {candidate.get('summary')}"
            )
        lines.append("")
    for label in NODE_LABELS:
        node = (summary.get("nodes") or {}).get(label) or {}
        lines.extend(
            [
                f"## {label}",
                f"- role=`{node.get('role')}` node_id=`{node.get('node_id')}`",
                f"- heights=`{fmt_num((node.get('snapshot') or {}).get('committed_height_first'))} -> {fmt_num((node.get('snapshot') or {}).get('committed_height_last'))}` known_peer_heads_max=`{fmt_num((node.get('snapshot') or {}).get('known_peer_heads_max'))}`",
                f"- Host current available=`{(node.get('host') or {}).get('available')}` latest_attempt=`{json.dumps((node.get('host') or {}).get('latest_attempt'), ensure_ascii=False)}` last_success is historical=`{bool((node.get('host') or {}).get('last_success'))}`",
                f"- runtime_cpu=`{fmt_percent((node.get('host') or {}).get('runtime_cpu_percent'))}` core_ratio=`{fmt_ratio_as_percent((node.get('host') or {}).get('runtime_cpu_core_ratio'))}` load1/core=`{fmt_num((node.get('host') or {}).get('load_per_core_ratio_1m'))}` mem_available=`{fmt_percent((node.get('host') or {}).get('mem_available_percent'))}` storage_used=`{fmt_percent((node.get('host') or {}).get('storage_used_percent'))}`",
                f"- runtime_perf=`{(node.get('runtime_perf') or {}).get('health') or 'not_reported'}` bottleneck=`{(node.get('runtime_perf') or {}).get('bottleneck') or 'not_reported'}` decision_p95_ms=`{fmt_num((node.get('runtime_perf') or {}).get('decision_p95_ms'))}`",
                f"- Traffic freshness: `{((node.get('traffic') or {}).get('freshness') or {}).get('state', 'unknown')}` producer=`{(((node.get('traffic') or {}).get('freshness') or {}).get('producer') or {}).get('state', 'unknown')}` current_measurement_available=`{(node.get('traffic') or {}).get('current_measurement_available', False)}`; counters below are historical window values",
                f"- traffic_payload=`{fmt_bytes((node.get('traffic') or {}).get('payload_total_bytes'))}` control_plane_events=`{fmt_num((node.get('traffic') or {}).get('control_plane_total_events'))}`",
                f"- wasm_metrics_available=`{(node.get('wasm') or {}).get('metrics_available')}` window_available=`{(node.get('wasm') or {}).get('window_available')}` hotspot=`{(node.get('wasm') or {}).get('top_hotspot')}`",
                f"- alerts=`{', '.join(node.get('alerts') or ['(none)'])}`",
                "",
                "### Module Breakdown",
            ]
        )
        for module_name, module_summary in (node.get("modules") or {}).items():
            lines.append(
                f"- `{module_name}` status=`{module_summary.get('status')}` alerts=`{', '.join(module_summary.get('alerts') or ['(none)'])}`"
            )
        lines.append("")
        lines.append("### Optimization Candidates")
        if node.get("optimization_candidates"):
            for candidate in node.get("optimization_candidates") or []:
                lines.append(
                    f"- [{candidate.get('severity')}] `{candidate.get('module')}/{candidate.get('key')}`: {candidate.get('summary')}"
                )
        else:
            lines.append("- `(none)`")
        lines.append("")
    return lines


def main():
    args = parse_args()
    snapshot = load_json(args.snapshot_summary)
    host = load_json(args.host_summary)
    as_of = args.as_of_unix_ms if args.as_of_unix_ms is not None else time.time_ns() // 1_000_000
    if as_of < 0:
        raise SystemExit("--as-of-unix-ms must be non-negative")
    traffic = reevaluate_traffic(load_json(args.traffic_summary), as_of)
    wasm = {
        "local_node": load_json(args.local_wasm_summary),
        "sequencer_ecs": load_json(args.sequencer_wasm_summary),
        "storage_ecs": load_json(args.storage_wasm_summary),
    }
    raw_statuses = {
        "local_node": load_json(args.local_status_json),
        "sequencer_ecs": load_json(args.sequencer_status_json),
        "storage_ecs": load_json(args.storage_status_json),
    }

    nodes = {
        label: derive_node_summary(label, snapshot, host, traffic, raw_statuses[label], wasm[label])
        for label in NODE_LABELS
    }

    host_complete = host_collection_complete(host)
    overall_alerts = []
    if not host_complete:
        overall_alerts.append("host_collection_incomplete" if host.get("schema_version") == 2 else "host_schema_unknown")
    if snapshot.get("claim_status") != "pass_candidate":
        overall_alerts.extend(snapshot.get("failure_signatures") or [])
    overall_alerts.extend(host.get("aggregate", {}).get("alerted_nodes") or [])
    traffic_network = (traffic.get("aggregate") or {}).get("network_interface") or {}
    if traffic_network.get("partial_coverage"):
        coverage_warning = network_interface_coverage_warning(traffic_network)
        if coverage_warning:
            traffic_network["coverage_warning"] = coverage_warning
        overall_alerts.append("traffic_network_interface_partial_coverage")
    for label, node in nodes.items():
        for alert in node.get("alerts") or []:
            overall_alerts.append(f"{label}:{alert}")

    any_module_alerts = any(node.get("alerts") for node in nodes.values())
    overall_status = "pass_candidate"
    if snapshot.get("claim_status") != "pass_candidate" or not host_complete:
        overall_status = "blocked"
    elif host.get("aggregate", {}).get("alerted_node_count", 0) > 0:
        overall_status = "pass_with_resource_alerts"
    elif any_module_alerts:
        overall_status = "pass_with_module_alerts"

    optimization_candidates = sort_candidates(
        [
            candidate
            for label in NODE_LABELS
            for candidate in ((nodes.get(label) or {}).get("optimization_candidates") or [])
        ]
    )

    summary = {
        "schema_version": 2,
        "as_of_unix_ms": as_of,
        "generated_at": datetime.now(timezone.utc).astimezone().isoformat(),
        "run_id": args.run_id,
        "run_dir": args.run_dir,
        "snapshot_summary_path": args.snapshot_summary,
        "host_summary_path": args.host_summary,
        "traffic_summary_path": args.traffic_summary,
        "wasm_summary_paths": {
            "local_node": args.local_wasm_summary,
            "sequencer_ecs": args.sequencer_wasm_summary,
            "storage_ecs": args.storage_wasm_summary,
        },
        "status_json_paths": {
            "local_node": args.local_status_json,
            "sequencer_ecs": args.sequencer_status_json,
            "storage_ecs": args.storage_status_json,
        },
        "snapshot": {
            "claim_status": snapshot.get("claim_status"),
            "failure_signatures": snapshot.get("failure_signatures") or [],
            "analysis": snapshot.get("analysis") or {},
        },
        "host": {
            "schema_version": host.get("schema_version"),
            "collection_complete": host_complete,
            "aggregate": host.get("aggregate") or {},
        },
        "traffic": {
            "schema_version": traffic.get("schema_version"),
            "as_of_unix_ms": traffic.get("as_of_unix_ms"),
            "freshness_policy": traffic.get("freshness_policy"),
            "current_measurement_available": traffic.get("current_measurement_available", False),
            "aggregate_semantics": "historical_child_window",
            "aggregate": traffic.get("aggregate") or {},
        },
        "nodes": nodes,
        "optimization_candidates": optimization_candidates,
        "overall": {
            "status": overall_status,
            "alerts": unique_sorted(overall_alerts),
            "optimization_candidate_count": len(optimization_candidates),
        },
    }

    Path(args.summary_json).write_text(
        json.dumps(summary, ensure_ascii=False, indent=2) + "\n",
        encoding="utf-8",
    )
    Path(args.summary_md).write_text(
        "\n".join(render_markdown(summary)) + "\n",
        encoding="utf-8",
    )


if __name__ == "__main__":
    main()
