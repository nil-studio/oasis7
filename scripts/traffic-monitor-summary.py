#!/usr/bin/env python3
import argparse
import json
from datetime import datetime, timezone
from pathlib import Path

from traffic_monitor_freshness import analyze, now_ms, timestamp


def parse_args():
    parser = argparse.ArgumentParser(
        description="Summarize Oasis7 traffic-monitor history with bounded retention."
    )
    parser.add_argument(
        "--layout",
        choices=("single-node", "triad"),
        required=True,
        help="summary layout to produce",
    )
    parser.add_argument("--history-path", required=True)
    parser.add_argument("--summary-json", required=True)
    parser.add_argument("--summary-md", required=True)
    parser.add_argument("--window-minutes", type=int, required=True)
    parser.add_argument("--history-retention-minutes", type=int, required=True)
    parser.add_argument("--top-n", type=int, required=True)
    parser.add_argument("--as-of-unix-ms", type=int)
    parser.add_argument("--max-sample-age-secs", type=int, default=60)
    parser.add_argument("--max-sample-gap-secs", type=int, default=60)
    parser.add_argument("--run-id")
    parser.add_argument("--run-dir")
    parser.add_argument("--label", action="append", default=[])
    args = parser.parse_args()
    for name in ('window_minutes', 'history_retention_minutes', 'top_n', 'max_sample_age_secs', 'max_sample_gap_secs'):
        if getattr(args, name) <= 0:
            parser.error(name + ' must be positive')
    if args.as_of_unix_ms is not None:
        if args.as_of_unix_ms < 0:
            parser.error('as_of_unix_ms must be nonnegative')
        if args.as_of_unix_ms > now_ms():
            parser.error('as_of_unix_ms cannot exceed current UTC time')
    return args


def record_ts_ms(record):
    return timestamp(record.get('captured_at_unix_ms'))


def prune_and_load_records(path, retention_minutes, as_of=None, historical=False):
    records = [json.loads(line) for line in path.read_text(encoding='utf-8').splitlines() if line.strip()]
    total = len(records)
    if historical:
        return records, total
    cutoff = (now_ms() if as_of is None else as_of) - retention_minutes * 60_000
    # Filter every row rather than deque prefixes; preserve append order for clock diagnostics.
    retained = [row for row in records if record_ts_ms(row) is None or record_ts_ms(row) >= cutoff]
    if len(retained) != total:
        temporary = path.with_name(path.name + '.tmp')
        temporary.write_text(''.join(json.dumps(row, ensure_ascii=False) + '\n' for row in retained), encoding='utf-8')
        temporary.replace(path)
    return retained, total


def clamp_delta(current, baseline):
    if current is None:
        current = 0
    if baseline is None:
        baseline = 0
    return max(0, int(current) - int(baseline))


def counter_key_for_lane(lane_name):
    return "datagrams" if lane_name == "udp_gossip" else "messages"


def delta_direction(current, baseline, counter_key):
    current = current or {}
    baseline = baseline or {}
    return {
        counter_key: clamp_delta(current.get(counter_key), baseline.get(counter_key)),
        "payload_bytes": clamp_delta(
            current.get("payload_bytes"), baseline.get("payload_bytes")
        ),
    }


def delta_lane_entry(current, baseline, counter_key):
    current = current or {}
    baseline = baseline or {}
    return {
        "inbound": delta_direction(current.get("inbound"), baseline.get("inbound"), counter_key),
        "outbound": delta_direction(
            current.get("outbound"), baseline.get("outbound"), counter_key
        ),
    }


def delta_wire_direction(current, baseline):
    current = current or {}
    baseline = baseline or {}
    return {
        "bytes": clamp_delta(current.get("bytes"), baseline.get("bytes")),
    }


def delta_wire_lane_entry(current, baseline):
    current = current or {}
    baseline = baseline or {}
    return {
        "inbound": delta_wire_direction(current.get("inbound"), baseline.get("inbound")),
        "outbound": delta_wire_direction(current.get("outbound"), baseline.get("outbound")),
    }


def delta_named_map(current_map, baseline_map, counter_key):
    current_map = current_map or {}
    baseline_map = baseline_map or {}
    result = {}
    for name in sorted(set(current_map) | set(baseline_map)):
        result[name] = delta_lane_entry(
            current_map.get(name), baseline_map.get(name), counter_key
        )
    return result


def total_count_and_payload(entry, counter_key):
    inbound = (entry or {}).get("inbound") or {}
    outbound = (entry or {}).get("outbound") or {}
    total_count = int(inbound.get(counter_key, 0)) + int(outbound.get(counter_key, 0))
    total_payload = int(inbound.get("payload_bytes", 0)) + int(
        outbound.get("payload_bytes", 0)
    )
    return total_count, total_payload


def nonzero_named_map(delta_map, counter_key):
    result = {}
    for name, entry in (delta_map or {}).items():
        total_count, total_payload = total_count_and_payload(entry, counter_key)
        if total_count == 0 and total_payload == 0:
            continue
        result[name] = entry
    return result


def top_entries(delta_map, counter_key, top_n):
    items = []
    for name, entry in (delta_map or {}).items():
        inbound = (entry or {}).get("inbound") or {}
        outbound = (entry or {}).get("outbound") or {}
        total_count, total_payload = total_count_and_payload(entry, counter_key)
        if total_count == 0 and total_payload == 0:
            continue
        items.append(
            {
                "name": name,
                "total_count": total_count,
                "total_payload_bytes": total_payload,
                "inbound": inbound,
                "outbound": outbound,
            }
        )
    items.sort(
        key=lambda item: (
            item["total_payload_bytes"],
            item["total_count"],
            item["name"],
        ),
        reverse=True,
    )
    return items[:top_n]


def delta_counter(current, baseline):
    return clamp_delta(current, baseline)


def delta_counter_map(current_map, baseline_map):
    current_map = current_map or {}
    baseline_map = baseline_map or {}
    result = {}
    for name in sorted(set(current_map) | set(baseline_map)):
        result[name] = delta_counter(current_map.get(name), baseline_map.get(name))
    return result


def nonzero_counter_map(counter_map):
    return {
        name: int(value)
        for name, value in (counter_map or {}).items()
        if int(value) > 0
    }


def top_counter_entries(counter_map, top_n):
    items = [
        {"name": name, "count": int(value)}
        for name, value in (counter_map or {}).items()
        if int(value) > 0
    ]
    items.sort(key=lambda item: (item["count"], item["name"]), reverse=True)
    return items[:top_n]


def sum_wire_direction(current, added):
    current = current or {}
    added = added or {}
    return {
        "bytes": int(current.get("bytes", 0)) + int(added.get("bytes", 0)),
    }


def sum_wire_lane_entry(current, added):
    current = current or {}
    added = added or {}
    return {
        "inbound": sum_wire_direction(current.get("inbound"), added.get("inbound")),
        "outbound": sum_wire_direction(current.get("outbound"), added.get("outbound")),
    }


def sum_direction(current, added, counter_key):
    current = current or {}
    added = added or {}
    return {
        counter_key: int(current.get(counter_key, 0)) + int(added.get(counter_key, 0)),
        "payload_bytes": int(current.get("payload_bytes", 0))
        + int(added.get("payload_bytes", 0)),
    }


def sum_lane_entry(current, added, counter_key):
    current = current or {}
    added = added or {}
    return {
        "inbound": sum_direction(current.get("inbound"), added.get("inbound"), counter_key),
        "outbound": sum_direction(current.get("outbound"), added.get("outbound"), counter_key),
    }


def sum_named_maps(named_maps, counter_key):
    result = {}
    for named_map in named_maps:
        for name, entry in (named_map or {}).items():
            result[name] = sum_lane_entry(result.get(name), entry, counter_key)
    return nonzero_named_map(result, counter_key)


def sum_counter_maps(named_maps):
    result = {}
    for named_map in named_maps:
        for name, value in (named_map or {}).items():
            result[name] = int(result.get(name, 0)) + int(value)
    return nonzero_counter_map(result)


def payload_totals_for_lane(lane):
    if not lane or lane.get("available") is not True:
        return 0
    totals = lane.get("totals") or {}
    inbound = totals.get("inbound") or {}
    outbound = totals.get("outbound") or {}
    return int(inbound.get("payload_bytes", 0)) + int(outbound.get("payload_bytes", 0))


def total_wire_bytes(wire_lane):
    wire_lane = wire_lane or {}
    inbound = (wire_lane.get("inbound") or {}).get("bytes", 0)
    outbound = (wire_lane.get("outbound") or {}).get("bytes", 0)
    return int(inbound) + int(outbound)


def share_percent(numerator, denominator):
    if denominator <= 0:
        return 0.0
    return round((float(numerator) * 100.0) / float(denominator), 2)


def average_bits_per_second(total_bytes, duration_seconds):
    if duration_seconds <= 0:
        return 0.0
    return round((float(total_bytes) * 8.0) / float(duration_seconds), 2)


def summarize_network_interface(latest, baseline, payload_total_bytes, duration_seconds):
    latest_interface = (latest or {}).get("network_interface") or {}
    baseline_interface = (baseline or {}).get("network_interface") or {}
    interface_name = latest_interface.get("name")
    baseline_interface_name = baseline_interface.get("name")
    if not interface_name:
        return {"available": False, "reason": "missing_latest_interface"}
    if not baseline_interface_name:
        return {"available": False, "reason": "missing_baseline_interface"}
    if baseline_interface_name != interface_name:
        return {"available": False, "reason": "baseline_interface_mismatch"}
    if latest_interface.get("rx_bytes") is None or latest_interface.get("tx_bytes") is None:
        return {"available": False, "reason": "missing_latest_counters"}
    if (
        baseline_interface.get("rx_bytes") is None
        or baseline_interface.get("tx_bytes") is None
    ):
        return {"available": False, "reason": "missing_baseline_counters"}

    rx_bytes = clamp_delta(latest_interface.get("rx_bytes"), baseline_interface.get("rx_bytes"))
    tx_bytes = clamp_delta(latest_interface.get("tx_bytes"), baseline_interface.get("tx_bytes"))
    total_bytes = rx_bytes + tx_bytes
    non_payload_bytes = max(0, total_bytes - int(payload_total_bytes))
    return {
        "available": True,
        "name": interface_name,
        "rx_bytes": rx_bytes,
        "tx_bytes": tx_bytes,
        "total_bytes": total_bytes,
        "average_rx_bps": average_bits_per_second(rx_bytes, duration_seconds),
        "average_tx_bps": average_bits_per_second(tx_bytes, duration_seconds),
        "average_total_bps": average_bits_per_second(total_bytes, duration_seconds),
        "payload_total_bytes": int(payload_total_bytes),
        "payload_share_percent": share_percent(payload_total_bytes, total_bytes),
        "non_payload_bytes": non_payload_bytes,
        "non_payload_share_percent": share_percent(non_payload_bytes, total_bytes),
    }


def lane_observed_since(record, lane_name):
    traffic = (record or {}).get("traffic") or {}
    lane = traffic.get(lane_name) or {}
    return lane.get("observed_since_unix_ms")


def compatible_with_latest(record, latest):
    if record.get("status_fetch_ok") is not True:
        return False
    latest_node = latest.get("node_id")
    if latest_node and record.get("node_id") not in (None, latest_node):
        return False
    for lane_name in ("udp_gossip", "libp2p_replication"):
        latest_since = lane_observed_since(latest, lane_name)
        if latest_since is None:
            continue
        if lane_observed_since(record, lane_name) != latest_since:
            return False
    return True


def summarize_lane(lane_name, current_lane, baseline_lane, top_n):
    if current_lane is None:
        return {"available": False}
    counter_key = counter_key_for_lane(lane_name)
    totals = delta_lane_entry(
        current_lane.get("totals"), (baseline_lane or {}).get("totals"), counter_key
    )
    result = {
        "available": True,
        "scope": current_lane.get("scope"),
        "observed_since_unix_ms": current_lane.get("observed_since_unix_ms"),
        "counter_key": counter_key,
        "totals": totals,
        "total_payload_bytes": payload_totals_for_lane({"available": True, "totals": totals}),
    }
    if lane_name == "udp_gossip":
        by_kind = nonzero_named_map(
            delta_named_map(
                current_lane.get("by_kind"), (baseline_lane or {}).get("by_kind"), counter_key
            ),
            counter_key,
        )
        result["by_kind"] = by_kind
        result["detail_entry_counts"] = {"by_kind": len(by_kind)}
        result["top_kinds"] = top_entries(by_kind, counter_key, top_n)
    else:
        result["gossip"] = delta_lane_entry(
            current_lane.get("gossip"), (baseline_lane or {}).get("gossip"), counter_key
        )
        result["request"] = delta_lane_entry(
            current_lane.get("request"), (baseline_lane or {}).get("request"), counter_key
        )
        result["response"] = delta_lane_entry(
            current_lane.get("response"), (baseline_lane or {}).get("response"), counter_key
        )
        result["wire_totals"] = delta_wire_lane_entry(
            current_lane.get("wire_totals"),
            (baseline_lane or {}).get("wire_totals"),
        )
        result["total_wire_bytes"] = total_wire_bytes(result.get("wire_totals"))
        by_topic = nonzero_named_map(
            delta_named_map(
                current_lane.get("by_topic"), (baseline_lane or {}).get("by_topic"), counter_key
            ),
            counter_key,
        )
        by_protocol = nonzero_named_map(
            delta_named_map(
                current_lane.get("by_protocol"),
                (baseline_lane or {}).get("by_protocol"),
                counter_key,
            ),
            counter_key,
        )
        result["by_topic"] = by_topic
        result["by_protocol"] = by_protocol
        control_plane_current = current_lane.get("control_plane") or {}
        control_plane_baseline = (baseline_lane or {}).get("control_plane") or {}
        control_plane_by_kind = nonzero_counter_map(
            delta_counter_map(
                control_plane_current.get("by_kind"),
                control_plane_baseline.get("by_kind"),
            )
        )
        result["control_plane"] = {
            "available": bool(control_plane_current),
            "units": control_plane_current.get("units", "events"),
            "total_events": delta_counter(
                control_plane_current.get("total_events"),
                control_plane_baseline.get("total_events"),
            ),
            "wire_scope": control_plane_current.get("wire_scope"),
            "excludes_transport_overhead": control_plane_current.get(
                "excludes_transport_overhead"
            ),
            "wire_bytes": delta_wire_lane_entry(
                control_plane_current.get("wire_bytes"),
                control_plane_baseline.get("wire_bytes"),
            ),
            "by_kind": control_plane_by_kind,
            "detail_entry_counts": {"by_kind": len(control_plane_by_kind)},
            "top_kinds": top_counter_entries(control_plane_by_kind, top_n),
        }
        result["control_plane"]["total_wire_bytes"] = total_wire_bytes(
            result["control_plane"].get("wire_bytes")
        )
        result["detail_entry_counts"] = {
            "by_topic": len(by_topic),
            "by_protocol": len(by_protocol),
        }
        result["top_topics"] = top_entries(by_topic, counter_key, top_n)
        result["top_protocols"] = top_entries(by_protocol, counter_key, top_n)
    return result


def aggregate_lane_summaries(lane_name, lanes, top_n):
    available = [lane for lane in lanes if lane and lane.get("available") is True]
    if not available:
        return {"available": False}

    counter_key = counter_key_for_lane(lane_name)
    aggregate = {
        "available": True,
        "counter_key": counter_key,
        "scope": sorted(
            {
                lane.get("scope")
                for lane in available
                if lane.get("scope") not in (None, "")
            }
        ),
        "node_count": len(available),
        "totals": {"inbound": {counter_key: 0, "payload_bytes": 0}, "outbound": {counter_key: 0, "payload_bytes": 0}},
    }

    for lane in available:
        aggregate["totals"] = sum_lane_entry(aggregate["totals"], lane.get("totals"), counter_key)

    aggregate["total_payload_bytes"] = payload_totals_for_lane(aggregate)

    if lane_name == "udp_gossip":
        by_kind = sum_named_maps([lane.get("by_kind") for lane in available], counter_key)
        aggregate["by_kind"] = by_kind
        aggregate["detail_entry_counts"] = {"by_kind": len(by_kind)}
        aggregate["top_kinds"] = top_entries(by_kind, counter_key, top_n)
    else:
        aggregate["gossip"] = {"inbound": {counter_key: 0, "payload_bytes": 0}, "outbound": {counter_key: 0, "payload_bytes": 0}}
        aggregate["request"] = {"inbound": {counter_key: 0, "payload_bytes": 0}, "outbound": {counter_key: 0, "payload_bytes": 0}}
        aggregate["response"] = {"inbound": {counter_key: 0, "payload_bytes": 0}, "outbound": {counter_key: 0, "payload_bytes": 0}}
        aggregate["wire_totals"] = {"inbound": {"bytes": 0}, "outbound": {"bytes": 0}}
        for lane in available:
            aggregate["gossip"] = sum_lane_entry(aggregate["gossip"], lane.get("gossip"), counter_key)
            aggregate["request"] = sum_lane_entry(aggregate["request"], lane.get("request"), counter_key)
            aggregate["response"] = sum_lane_entry(aggregate["response"], lane.get("response"), counter_key)
            aggregate["wire_totals"] = sum_wire_lane_entry(
                aggregate["wire_totals"], lane.get("wire_totals")
            )

        by_topic = sum_named_maps([lane.get("by_topic") for lane in available], counter_key)
        by_protocol = sum_named_maps(
            [lane.get("by_protocol") for lane in available], counter_key
        )
        control_plane_by_kind = sum_counter_maps(
            [
                (lane.get("control_plane") or {}).get("by_kind")
                for lane in available
            ]
        )
        aggregate["by_topic"] = by_topic
        aggregate["by_protocol"] = by_protocol
        aggregate["control_plane"] = {
            "available": any(
                (lane.get("control_plane") or {}).get("available") is True
                for lane in available
            ),
            "units": next(
                (
                    (lane.get("control_plane") or {}).get("units")
                    for lane in available
                    if (lane.get("control_plane") or {}).get("units")
                ),
                "events",
            ),
            "total_events": sum(
                int((lane.get("control_plane") or {}).get("total_events", 0))
                for lane in available
            ),
            "wire_scope": sorted(
                {
                    (lane.get("control_plane") or {}).get("wire_scope")
                    for lane in available
                    if (lane.get("control_plane") or {}).get("wire_scope") not in (None, "")
                }
            ),
            "excludes_transport_overhead": all(
                (lane.get("control_plane") or {}).get("excludes_transport_overhead") is True
                for lane in available
                if (lane.get("control_plane") or {}).get("available") is True
            ),
            "wire_bytes": {"inbound": {"bytes": 0}, "outbound": {"bytes": 0}},
            "by_kind": control_plane_by_kind,
            "detail_entry_counts": {"by_kind": len(control_plane_by_kind)},
            "top_kinds": top_counter_entries(control_plane_by_kind, top_n),
        }
        for lane in available:
            aggregate["control_plane"]["wire_bytes"] = sum_wire_lane_entry(
                aggregate["control_plane"]["wire_bytes"],
                (lane.get("control_plane") or {}).get("wire_bytes"),
            )
        aggregate["total_wire_bytes"] = total_wire_bytes(aggregate.get("wire_totals"))
        aggregate["control_plane"]["total_wire_bytes"] = total_wire_bytes(
            aggregate["control_plane"].get("wire_bytes")
        )
        aggregate["detail_entry_counts"] = {
            "by_topic": len(by_topic),
            "by_protocol": len(by_protocol),
        }
        aggregate["top_topics"] = top_entries(by_topic, counter_key, top_n)
        aggregate["top_protocols"] = top_entries(by_protocol, counter_key, top_n)
    return aggregate


def summarize_record_set(records, window_minutes, top_n, as_of=None, max_age=60_000, max_gap=60_000, historical=False):
    as_of = now_ms() if as_of is None else as_of
    metadata, segment, visible = analyze(records, as_of, window_minutes, max_age, max_gap, historical)
    result = summarize_historical_record_set(segment, window_minutes, top_n, as_of)
    result['sample_count_total'] = len(visible)
    result['sample_count_successful'] = sum(row.get('status_fetch_ok') is True for row in visible)
    result.setdefault('window', {}).update(metadata.pop('window'))
    result.update(metadata)
    result['latest_fetch_error'] = result['latest_attempt']['error']
    return result


def summarize_historical_record_set(records, window_minutes, top_n, as_of):
    successful = [record for record in records if record.get("status_fetch_ok") is True]
    if not successful:
        latest_any = records[-1] if records else None
        return {
            "available": False,
            "sample_count_total": len(records),
            "sample_count_successful": 0,
            "latest_fetch_error": (latest_any or {}).get("fetch_error"),
        }

    latest = successful[-1]
    window_start_ms = as_of - window_minutes * 60 * 1000
    in_window = [
        record for record in successful if int(record["captured_at_unix_ms"]) >= window_start_ms
    ]
    compatible = [record for record in in_window if compatible_with_latest(record, latest)]
    if compatible:
        baseline = compatible[0]
    else:
        baseline = latest
        compatible = [latest]

    coverage_minutes = round(
        max(
            0,
            int(latest["captured_at_unix_ms"]) - int(baseline["captured_at_unix_ms"]),
        )
        / 60000.0,
        2,
    )
    duration_seconds = max(
        0.0,
        (
            int(latest["captured_at_unix_ms"]) - int(baseline["captured_at_unix_ms"])
        )
        / 1000.0,
    )
    restart_detected = any(not compatible_with_latest(record, latest) for record in in_window)

    traffic_latest = latest.get("traffic") or {}
    traffic_baseline = baseline.get("traffic") or {}
    udp_summary = summarize_lane(
        "udp_gossip",
        traffic_latest.get("udp_gossip"),
        traffic_baseline.get("udp_gossip"),
        top_n,
    )
    libp2p_summary = summarize_lane(
        "libp2p_replication",
        traffic_latest.get("libp2p_replication"),
        traffic_baseline.get("libp2p_replication"),
        top_n,
    )
    payload_total_bytes = payload_totals_for_lane(udp_summary) + payload_totals_for_lane(
        libp2p_summary
    )
    return {
        "available": True,
        "sample_count_total": len(records),
        "sample_count_successful": len(successful),
        "window": {
            "requested_minutes": window_minutes,
            "covered_minutes": coverage_minutes,
            "sample_count_in_window": len(in_window),
            "compatible_sample_count": len(compatible),
            "baseline_captured_at": baseline.get("captured_at"),
            "baseline_captured_at_unix_ms": baseline.get("captured_at_unix_ms"),
            "latest_captured_at": latest.get("captured_at"),
            "latest_captured_at_unix_ms": latest.get("captured_at_unix_ms"),
            "restart_or_counter_reset_detected_within_window": restart_detected,
            "full_window_covered": coverage_minutes >= max(0.0, window_minutes - 0.01),
        },
        "latest": {
            "node_label": latest.get("node_label"),
            "node_id": latest.get("node_id"),
            "world_id": latest.get("world_id"),
            "role": latest.get("role"),
            "running": latest.get("running"),
            "status_url": latest.get("status_url"),
            "status_observed_at_unix_ms": latest.get("status_observed_at_unix_ms"),
            "last_error": latest.get("last_error"),
        },
        "consensus": {
            "committed_height": {
                "baseline": baseline.get("consensus", {}).get("committed_height"),
                "latest": latest.get("consensus", {}).get("committed_height"),
                "delta": clamp_delta(
                    latest.get("consensus", {}).get("committed_height"),
                    baseline.get("consensus", {}).get("committed_height"),
                ),
            },
            "network_committed_height": {
                "baseline": baseline.get("consensus", {}).get("network_committed_height"),
                "latest": latest.get("consensus", {}).get("network_committed_height"),
                "delta": clamp_delta(
                    latest.get("consensus", {}).get("network_committed_height"),
                    baseline.get("consensus", {}).get("network_committed_height"),
                ),
            },
            "known_peer_heads": {
                "baseline": baseline.get("consensus", {}).get("known_peer_heads"),
                "latest": latest.get("consensus", {}).get("known_peer_heads"),
            },
        },
        "recent_errors": {
            "storage_recent_errors_count": {
                "baseline": baseline.get("storage_recent_errors_count"),
                "latest": latest.get("storage_recent_errors_count"),
                "delta": clamp_delta(
                    latest.get("storage_recent_errors_count"),
                    baseline.get("storage_recent_errors_count"),
                ),
            },
            "reward_runtime_recent_errors_count": {
                "baseline": baseline.get("reward_runtime_recent_errors_count"),
                "latest": latest.get("reward_runtime_recent_errors_count"),
                "delta": clamp_delta(
                    latest.get("reward_runtime_recent_errors_count"),
                    baseline.get("reward_runtime_recent_errors_count"),
                ),
            },
        },
        "network_interface": summarize_network_interface(
            latest, baseline, payload_total_bytes, duration_seconds
        ),
        "traffic": {
            "udp_gossip": udp_summary,
            "libp2p_replication": libp2p_summary,
        },
    }


def aggregate_node_payload_distribution(nodes):
    rows = []
    total_payload_bytes = 0
    for label, node in nodes.items():
        if node.get("available") is not True:
            continue
        udp_payload_bytes = payload_totals_for_lane(node["traffic"].get("udp_gossip"))
        libp2p_payload_bytes = payload_totals_for_lane(
            node["traffic"].get("libp2p_replication")
        )
        node_total = udp_payload_bytes + libp2p_payload_bytes
        total_payload_bytes += node_total
        latest = node.get("latest") or {}
        rows.append(
            {
                "label": label,
                "node_id": latest.get("node_id"),
                "role": latest.get("role"),
                "udp_payload_bytes": udp_payload_bytes,
                "libp2p_payload_bytes": libp2p_payload_bytes,
                "total_payload_bytes": node_total,
            }
        )

    rows.sort(
        key=lambda row: (
            row["total_payload_bytes"],
            row["label"],
        ),
        reverse=True,
    )
    for row in rows:
        row["share_percent"] = share_percent(row["total_payload_bytes"], total_payload_bytes)
    return total_payload_bytes, rows


def aggregate_node_network_distribution(nodes):
    rows = []
    expected_nodes = list(nodes.keys())
    successful_nodes = []
    missing_nodes = []
    unavailable_nodes = []
    interface_counter_missing_nodes = []
    total_network_bytes = 0
    total_payload_bytes = 0
    combined_rx_bps = 0.0
    combined_tx_bps = 0.0
    combined_total_bps = 0.0
    for label, node in nodes.items():
        if node.get("available") is not True:
            missing_nodes.append(label)
            unavailable_nodes.append(label)
            continue
        successful_nodes.append(label)
        network = node.get("network_interface") or {}
        if network.get("available") is not True:
            missing_nodes.append(label)
            interface_counter_missing_nodes.append(label)
            continue
        latest = node.get("latest") or {}
        total_network_bytes += int(network.get("total_bytes", 0))
        total_payload_bytes += int(network.get("payload_total_bytes", 0))
        combined_rx_bps += float(network.get("average_rx_bps", 0.0))
        combined_tx_bps += float(network.get("average_tx_bps", 0.0))
        combined_total_bps += float(network.get("average_total_bps", 0.0))
        rows.append(
            {
                "label": label,
                "node_id": latest.get("node_id"),
                "role": latest.get("role"),
                "interface_name": network.get("name"),
                "network_total_bytes": int(network.get("total_bytes", 0)),
                "payload_total_bytes": int(network.get("payload_total_bytes", 0)),
                "payload_share_percent": float(network.get("payload_share_percent", 0.0)),
                "average_total_bps": float(network.get("average_total_bps", 0.0)),
            }
        )

    rows.sort(
        key=lambda row: (
            row["network_total_bytes"],
            row["label"],
        ),
        reverse=True,
    )
    for row in rows:
        row["share_percent"] = share_percent(row["network_total_bytes"], total_network_bytes)
    partial_coverage = len(missing_nodes) > 0
    coverage_warning = None
    if partial_coverage:
        coverage_warning = {
            "code": "network_interface_partial_coverage",
            "severity": "warn",
            "summary": (
                "Network-interface aggregate excludes unavailable nodes or nodes "
                "without interface counters; totals only cover interface-available nodes."
            ),
            "expected_node_count": len(expected_nodes),
            "successful_node_count": len(successful_nodes),
            "network_interface_available_node_count": len(rows),
            "skipped_unavailable_node_count": len(unavailable_nodes),
            "interface_counter_missing_node_count": len(interface_counter_missing_nodes),
            "missing_network_interface_node_count": len(missing_nodes),
            "missing_network_interface_nodes": sorted(missing_nodes),
            "skipped_unavailable_nodes": sorted(unavailable_nodes),
            "interface_counter_missing_nodes": sorted(interface_counter_missing_nodes),
        }
    return {
        "expected_node_count": len(expected_nodes),
        "successful_node_count": len(successful_nodes),
        "network_interface_available_node_count": len(rows),
        "missing_network_interface_nodes": sorted(missing_nodes),
        "skipped_unavailable_nodes": sorted(unavailable_nodes),
        "interface_counter_missing_nodes": sorted(interface_counter_missing_nodes),
        "partial_coverage": partial_coverage,
        "coverage_warning": coverage_warning,
        "node_count": len(rows),
        "total_bytes": total_network_bytes,
        "payload_total_bytes": total_payload_bytes,
        "payload_share_percent": share_percent(total_payload_bytes, total_network_bytes),
        "non_payload_bytes": max(0, total_network_bytes - total_payload_bytes),
        "non_payload_share_percent": share_percent(
            max(0, total_network_bytes - total_payload_bytes), total_network_bytes
        ),
        "combined_rx_bps": round(combined_rx_bps, 2),
        "combined_tx_bps": round(combined_tx_bps, 2),
        "combined_total_bps": round(combined_total_bps, 2),
        "nodes": rows,
    }


def summarize_triad_aggregate(nodes, top_n):
    udp = aggregate_lane_summaries(
        "udp_gossip", [node.get("traffic", {}).get("udp_gossip") for node in nodes.values()], top_n
    )
    libp2p = aggregate_lane_summaries(
        "libp2p_replication",
        [node.get("traffic", {}).get("libp2p_replication") for node in nodes.values()],
        top_n,
    )
    total_payload_bytes, node_payload_distribution = aggregate_node_payload_distribution(nodes)
    lane_distribution = {
        "udp_gossip": {
            "payload_bytes": payload_totals_for_lane(udp),
        },
        "libp2p_replication": {
            "payload_bytes": payload_totals_for_lane(libp2p),
        },
    }
    for entry in lane_distribution.values():
        entry["share_percent"] = share_percent(entry["payload_bytes"], total_payload_bytes)
    network_interface = aggregate_node_network_distribution(nodes)

    return {
        "node_count": len([node for node in nodes.values() if node.get("available") is True]),
        "total_payload_bytes": total_payload_bytes,
        "lane_distribution": lane_distribution,
        "node_payload_distribution": node_payload_distribution,
        "network_interface": network_interface,
        "traffic": {
            "udp_gossip": udp,
            "libp2p_replication": libp2p,
        },
    }


def fmt_num(value):
    if value is None:
        return "n/a"
    return f"{int(value):,}"


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


def fmt_bps(value):
    if value is None:
        return "n/a"
    units = ["bit/s", "Kbit/s", "Mbit/s", "Gbit/s"]
    amount = float(value)
    for unit in units:
        if amount < 1000.0 or unit == units[-1]:
            if unit == "bit/s":
                return f"{amount:.0f} {unit}"
            return f"{amount:.2f} {unit}"
        amount /= 1000.0
    return f"{float(value):.2f} bit/s"


def render_top_block(title, entries, counter_key):
    if not entries:
        return [f"- {title}: none"]
    lines = [f"- {title}:"]
    for entry in entries:
        inbound = entry["inbound"]
        outbound = entry["outbound"]
        lines.append(
            "  "
            + f"{entry['name']}: total +{fmt_bytes(entry['total_payload_bytes'])}, "
            + f"inbound +{fmt_num(inbound.get(counter_key))} / {fmt_bytes(inbound.get('payload_bytes'))}, "
            + f"outbound +{fmt_num(outbound.get(counter_key))} / {fmt_bytes(outbound.get('payload_bytes'))}"
        )
    return lines


def render_top_counter_block(title, entries, units):
    if not entries:
        return [f"- {title}: none"]
    lines = [f"- {title}:"]
    for entry in entries:
        lines.append("  " + f"{entry['name']}: +{fmt_num(entry['count'])} {units}")
    return lines


def render_traffic_totals(name, lane):
    if not lane.get("available"):
        return f"- {name}: unavailable"
    counter_key = lane["counter_key"]
    totals = lane["totals"]
    inbound = totals["inbound"]
    outbound = totals["outbound"]
    line = (
        f"- {name}: inbound +{fmt_num(inbound[counter_key])} {counter_key}, "
        f"+{fmt_bytes(inbound['payload_bytes'])}; outbound +{fmt_num(outbound[counter_key])} "
        f"{counter_key}, +{fmt_bytes(outbound['payload_bytes'])}"
    )
    wire_totals = lane.get("wire_totals") or {}
    if wire_totals:
        line += (
            f"; substream wire inbound +{fmt_bytes((wire_totals.get('inbound') or {}).get('bytes'))}, "
            f"outbound +{fmt_bytes((wire_totals.get('outbound') or {}).get('bytes'))}"
        )
    return line


def render_payload_distribution_block(title, entries):
    if not entries:
        return [f"- {title}: none"]
    lines = [f"- {title}:"]
    for entry in entries:
        lines.append(
            "  "
            + f"{entry['label']} ({entry.get('role') or 'unknown'}): "
            + f"{fmt_bytes(entry['total_payload_bytes'])} ({entry['share_percent']:.2f}%), "
            + f"udp {fmt_bytes(entry['udp_payload_bytes'])}, "
            + f"libp2p {fmt_bytes(entry['libp2p_payload_bytes'])}"
        )
    return lines


def render_network_distribution_block(title, entries):
    if not entries:
        return [f"- {title}: none"]
    lines = [f"- {title}:"]
    for entry in entries:
        lines.append(
            "  "
            + f"{entry['label']} ({entry.get('role') or 'unknown'} {entry.get('interface_name') or 'n/a'}): "
            + f"{fmt_bytes(entry['network_total_bytes'])} ({entry['share_percent']:.2f}%), "
            + f"payload share {entry['payload_share_percent']:.2f}%, "
            + f"avg {fmt_bps(entry['average_total_bps'])}"
        )
    return lines


def render_freshness(node):
    freshness = node['freshness']
    producer = freshness['producer']
    return [f"- As of: `{node['as_of_unix_ms']}`; collector `{freshness['state']}`; latest attempt `{node['latest_attempt']['result']}`; sample age ms `{freshness['sample_age_ms']}`",
            f"- Producer observation: `{producer['state']}`; observed at `{producer['observed_at_unix_ms']}`; age ms `{producer['age_ms']}`",
            f"- Freshness reasons: `{', '.join(freshness['reasons']) or 'none'}`"]


def render_single_node_markdown(summary, history_path, generated_at):
    lines = [
        "# Oasis7 Node Traffic Monitor Summary",
        "",
        f"- Generated at: `{generated_at}`",
        f"- History file: `{history_path}`",
        f"- History retention: `{summary['history_retention_minutes']}` minutes",
        f"- History record count after prune: `{summary['history_record_count']}`",
        f"- History records pruned this run: `{summary['history_pruned_count']}`",
        f"- Requested window: `{summary['window_minutes_requested']}` minutes",
        f"- Top contributors per map: `{summary['top_n']}`",
        "",
    ]

    node = summary["node"]
    lines.extend(render_freshness(node))
    if not node.get("historical_data_available"):
        lines.append(
            f"- Current collector unavailable; historical_data_available=`{node.get('historical_data_available')}`. latest_fetch_error=`{node.get('latest_fetch_error')}`"
        )
        return lines

    if not node["available"]:
        lines.append("- Historical diagnostics below are from the last successful sample; current measurement is unavailable.")
    window = node["window"]
    latest = node["latest"]
    consensus = node["consensus"]
    errors = node["recent_errors"]
    network = node["network_interface"]
    udp = node["traffic"]["udp_gossip"]
    libp2p = node["traffic"]["libp2p_replication"]
    lines.extend(
        [
            f"- Node: `{latest['node_label']}` node_id=`{latest['node_id']}` role=`{latest['role']}` running=`{latest['running']}`",
            f"- Status URL: `{latest['status_url']}`",
            f"- Window coverage: `{window['covered_minutes']}` / `{window['requested_minutes']}` minutes across `{window['compatible_sample_count']}` compatible samples",
            f"- Baseline sample: `{window['baseline_captured_at']}` | Latest sample: `{window['latest_captured_at']}`",
            f"- Restart/counter reset inside requested window: `{window['restart_or_counter_reset_detected_within_window']}`",
            f"- Height delta: committed `+{fmt_num(consensus['committed_height']['delta'])}` (`{fmt_num(consensus['committed_height']['baseline'])}` -> `{fmt_num(consensus['committed_height']['latest'])}`), network `+{fmt_num(consensus['network_committed_height']['delta'])}` (`{fmt_num(consensus['network_committed_height']['baseline'])}` -> `{fmt_num(consensus['network_committed_height']['latest'])}`)",
            f"- Known peer heads: `{fmt_num(consensus['known_peer_heads']['baseline'])}` -> `{fmt_num(consensus['known_peer_heads']['latest'])}`",
            f"- Recent errors: storage `+{fmt_num(errors['storage_recent_errors_count']['delta'])}` (`{fmt_num(errors['storage_recent_errors_count']['baseline'])}` -> `{fmt_num(errors['storage_recent_errors_count']['latest'])}`), reward runtime `+{fmt_num(errors['reward_runtime_recent_errors_count']['delta'])}` (`{fmt_num(errors['reward_runtime_recent_errors_count']['baseline'])}` -> `{fmt_num(errors['reward_runtime_recent_errors_count']['latest'])}`)",
            f"- Last error: `{latest['last_error']}`",
        ]
    )
    if network.get("available"):
        lines.append(
            f"- Network interface `{network['name']}`: rx +{fmt_bytes(network['rx_bytes'])}, tx +{fmt_bytes(network['tx_bytes'])}, total +{fmt_bytes(network['total_bytes'])}, avg rx {fmt_bps(network['average_rx_bps'])}, avg tx {fmt_bps(network['average_tx_bps'])}, payload share {network['payload_share_percent']:.2f}%"
        )
    else:
        lines.append(f"- Network interface: unavailable ({network.get('reason', 'missing')})")

    lines.append(render_traffic_totals("UDP gossip", udp))
    if udp.get("available"):
        lines.append(
            f"- Full UDP detail rows in JSON: `{udp['detail_entry_counts']['by_kind']}` by_kind entries"
        )
        lines.extend(render_top_block("Top UDP kinds", udp.get("top_kinds"), "datagrams"))

    lines.append(render_traffic_totals("Libp2p replication", libp2p))
    if libp2p.get("available"):
        if libp2p.get("wire_totals"):
            lines.append(
                f"- Libp2p total substream wire bytes: `{fmt_bytes(libp2p.get('total_wire_bytes'))}`"
            )
        lines.append(
            "- Libp2p lanes: "
            + f"gossip +{fmt_bytes(libp2p['gossip']['inbound']['payload_bytes'] + libp2p['gossip']['outbound']['payload_bytes'])}, "
            + f"request +{fmt_bytes(libp2p['request']['inbound']['payload_bytes'] + libp2p['request']['outbound']['payload_bytes'])}, "
            + f"response +{fmt_bytes(libp2p['response']['inbound']['payload_bytes'] + libp2p['response']['outbound']['payload_bytes'])}"
        )
        lines.append(
            f"- Full libp2p detail rows in JSON: `{libp2p['detail_entry_counts']['by_protocol']}` by_protocol, `{libp2p['detail_entry_counts']['by_topic']}` by_topic"
        )
        control_plane = libp2p.get("control_plane") or {}
        if control_plane.get("available"):
            lines.append(
                f"- Libp2p control-plane counters: `+{fmt_num(control_plane['total_events'])}` {control_plane.get('units', 'events')} over `{control_plane['detail_entry_counts']['by_kind']}` by_kind rows in JSON"
            )
            lines.append(
                f"- Libp2p control-plane wire bytes: total `+{fmt_bytes(control_plane.get('total_wire_bytes'))}`, inbound `+{fmt_bytes((control_plane.get('wire_bytes') or {}).get('inbound', {}).get('bytes'))}`, outbound `+{fmt_bytes((control_plane.get('wire_bytes') or {}).get('outbound', {}).get('bytes'))}`; scope=`{control_plane.get('wire_scope')}` excludes_transport_overhead=`{control_plane.get('excludes_transport_overhead')}`"
            )
            lines.extend(
                render_top_counter_block(
                    "Top libp2p control-plane kinds",
                    control_plane.get("top_kinds"),
                    control_plane.get("units", "events"),
                )
            )
        lines.extend(
            render_top_block(
                "Top libp2p protocols", libp2p.get("top_protocols"), "messages"
            )
        )
        lines.extend(
            render_top_block("Top libp2p topics", libp2p.get("top_topics"), "messages")
        )
    return lines


def render_triad_markdown(summary, history_path, generated_at, labels):
    lines = [
        "# P2P Real Env Traffic Monitor Summary",
        "",
        f"- Generated at: `{generated_at}`",
        f"- History file: `{history_path}`",
        f"- History retention: `{summary['history_retention_minutes']}` minutes",
        f"- History record count after prune: `{summary['history_record_count']}`",
        f"- History records pruned this run: `{summary['history_pruned_count']}`",
        f"- Requested window: `{summary['window_minutes_requested']}` minutes",
        f"- Top contributors per map: `{summary['top_n']}`",
        "",
    ]

    aggregate = summary.get("aggregate") or {}
    aggregate_udp = (aggregate.get("traffic") or {}).get("udp_gossip") or {}
    aggregate_libp2p = (aggregate.get("traffic") or {}).get("libp2p_replication") or {}
    aggregate_network = aggregate.get("network_interface") or {}
    if aggregate:
        aggregate_libp2p_wire_total = (
            fmt_bytes(aggregate_libp2p.get("total_wire_bytes"))
            if aggregate_libp2p.get("available")
            else "n/a"
        )
        aggregate_control_plane_wire_total = (
            fmt_bytes((aggregate_libp2p.get("control_plane") or {}).get("total_wire_bytes"))
            if aggregate_libp2p.get("available")
            else "n/a"
        )
        lines.extend(
            [
                "## aggregate",
                f"- Nodes with successful data: `{aggregate.get('node_count', 0)}`",
                f"- Total payload across all nodes: `{fmt_bytes(aggregate.get('total_payload_bytes'))}`",
                f"- Total libp2p substream wire bytes across all nodes: `{aggregate_libp2p_wire_total}`, derived control-plane wire bytes `{aggregate_control_plane_wire_total}`",
                f"- Lane distribution: udp `{fmt_bytes(aggregate['lane_distribution']['udp_gossip']['payload_bytes'])}` ({aggregate['lane_distribution']['udp_gossip']['share_percent']:.2f}%), libp2p `{fmt_bytes(aggregate['lane_distribution']['libp2p_replication']['payload_bytes'])}` ({aggregate['lane_distribution']['libp2p_replication']['share_percent']:.2f}%)",
            ]
        )
        if aggregate_network.get("node_count", 0) > 0:
            total_scope = (
                "nodes with network interface data"
                if aggregate_network.get("partial_coverage")
                else "all nodes"
            )
            lines.append(
                f"- Total network interface bytes across {total_scope}: `{fmt_bytes(aggregate_network['total_bytes'])}`, payload share `{aggregate_network['payload_share_percent']:.2f}%`, combined rx `{fmt_bps(aggregate_network['combined_rx_bps'])}`, combined tx `{fmt_bps(aggregate_network['combined_tx_bps'])}`"
            )
        coverage_label = "partial" if aggregate_network.get("partial_coverage") else "complete"
        missing_nodes = ", ".join(aggregate_network.get("missing_network_interface_nodes") or [])
        coverage_warning = aggregate_network.get("coverage_warning") or {}
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
        lines.extend(
            render_payload_distribution_block(
                "Node payload distribution", aggregate.get("node_payload_distribution")
            )
        )
        if aggregate_network.get("node_count", 0) > 0:
            lines.extend(
                render_network_distribution_block(
                    "Node network distribution", aggregate_network.get("nodes")
                )
            )
        if aggregate_udp.get("available"):
            lines.append(
                f"- Full aggregate UDP detail rows in JSON: `{aggregate_udp['detail_entry_counts']['by_kind']}` by_kind entries"
            )
            lines.extend(
                render_top_block(
                    "Top aggregate UDP kinds", aggregate_udp.get("top_kinds"), "datagrams"
                )
            )
        if aggregate_libp2p.get("available"):
            lines.append(
                f"- Full aggregate libp2p detail rows in JSON: `{aggregate_libp2p['detail_entry_counts']['by_protocol']}` by_protocol, `{aggregate_libp2p['detail_entry_counts']['by_topic']}` by_topic"
            )
            aggregate_control_plane = aggregate_libp2p.get("control_plane") or {}
            if aggregate_control_plane.get("available"):
                lines.append(
                    f"- Aggregate libp2p control-plane counters: `+{fmt_num(aggregate_control_plane['total_events'])}` {aggregate_control_plane.get('units', 'events')} over `{aggregate_control_plane['detail_entry_counts']['by_kind']}` by_kind rows in JSON"
                )
                lines.append(
                    f"- Aggregate libp2p control-plane wire bytes: total `+{fmt_bytes(aggregate_control_plane.get('total_wire_bytes'))}`, inbound `+{fmt_bytes((aggregate_control_plane.get('wire_bytes') or {}).get('inbound', {}).get('bytes'))}`, outbound `+{fmt_bytes((aggregate_control_plane.get('wire_bytes') or {}).get('outbound', {}).get('bytes'))}`; scope=`{', '.join(aggregate_control_plane.get('wire_scope') or [])}` excludes_transport_overhead=`{aggregate_control_plane.get('excludes_transport_overhead')}`"
                )
                lines.extend(
                    render_top_counter_block(
                        "Top aggregate libp2p control-plane kinds",
                        aggregate_control_plane.get("top_kinds"),
                        aggregate_control_plane.get("units", "events"),
                    )
                )
            lines.extend(
                render_top_block(
                    "Top aggregate libp2p protocols",
                    aggregate_libp2p.get("top_protocols"),
                    "messages",
                )
            )
            lines.extend(
                render_top_block(
                    "Top aggregate libp2p topics",
                    aggregate_libp2p.get("top_topics"),
                    "messages",
                )
            )
        lines.append("")

    for label in labels:
        node = summary["nodes"][label]
        lines.append(f"## {label}")
        lines.extend(render_freshness(node))
        if not node.get("historical_data_available"):
            lines.append(
                f"- Current collector unavailable; historical_data_available=`{node.get('historical_data_available')}`. latest_fetch_error=`{node.get('latest_fetch_error')}`"
            )
            lines.append("")
            continue

        if not node["available"]:
            lines.append("- Historical diagnostics below are from the last successful sample; current measurement is unavailable.")
        window = node["window"]
        latest = node["latest"]
        consensus = node["consensus"]
        errors = node["recent_errors"]
        network = node["network_interface"]
        lines.extend(
            [
                f"- Node: `{latest.get('node_id')}` role=`{latest.get('role')}` running=`{latest.get('running')}`",
                f"- Window coverage: `{window['covered_minutes']}` / `{window['requested_minutes']}` minutes across `{window['compatible_sample_count']}` compatible samples",
                f"- Baseline sample: `{window['baseline_captured_at']}` | Latest sample: `{window['latest_captured_at']}`",
                f"- Restart/counter reset inside requested window: `{window['restart_or_counter_reset_detected_within_window']}`",
                f"- Height delta: committed `+{fmt_num(consensus['committed_height']['delta'])}` (`{fmt_num(consensus['committed_height']['baseline'])}` -> `{fmt_num(consensus['committed_height']['latest'])}`), network `+{fmt_num(consensus['network_committed_height']['delta'])}` (`{fmt_num(consensus['network_committed_height']['baseline'])}` -> `{fmt_num(consensus['network_committed_height']['latest'])}`)",
                f"- Known peer heads: `{fmt_num(consensus['known_peer_heads']['baseline'])}` -> `{fmt_num(consensus['known_peer_heads']['latest'])}`",
                f"- Recent errors: storage `+{fmt_num(errors['storage_recent_errors_count']['delta'])}` (`{fmt_num(errors['storage_recent_errors_count']['baseline'])}` -> `{fmt_num(errors['storage_recent_errors_count']['latest'])}`), reward runtime `+{fmt_num(errors['reward_runtime_recent_errors_count']['delta'])}` (`{fmt_num(errors['reward_runtime_recent_errors_count']['baseline'])}` -> `{fmt_num(errors['reward_runtime_recent_errors_count']['latest'])}`)",
                f"- Last error: `{latest.get('last_error')}`",
                render_traffic_totals("UDP gossip", node["traffic"]["udp_gossip"]),
                render_traffic_totals(
                    "Libp2p replication", node["traffic"]["libp2p_replication"]
                ),
            ]
        )
        if network.get("available"):
            lines.append(
                f"- Network interface `{network['name']}`: rx +{fmt_bytes(network['rx_bytes'])}, tx +{fmt_bytes(network['tx_bytes'])}, total +{fmt_bytes(network['total_bytes'])}, avg total {fmt_bps(network['average_total_bps'])}, payload share {network['payload_share_percent']:.2f}%"
            )
        else:
            lines.append(f"- Network interface: unavailable ({network.get('reason', 'missing')})")

        udp = node["traffic"]["udp_gossip"]
        if udp.get("available"):
            lines.append(
                f"- Full UDP detail rows in JSON: `{udp['detail_entry_counts']['by_kind']}` by_kind entries"
            )
            lines.extend(render_top_block("Top UDP kinds", udp.get("top_kinds"), "datagrams"))

        libp2p = node["traffic"]["libp2p_replication"]
        if libp2p.get("available"):
            if libp2p.get("wire_totals"):
                lines.append(
                    f"- Libp2p total substream wire bytes: `{fmt_bytes(libp2p.get('total_wire_bytes'))}`"
                )
            lines.append(
                "- Libp2p lanes: "
                + f"gossip +{fmt_bytes(libp2p['gossip']['inbound']['payload_bytes'] + libp2p['gossip']['outbound']['payload_bytes'])}, "
                + f"request +{fmt_bytes(libp2p['request']['inbound']['payload_bytes'] + libp2p['request']['outbound']['payload_bytes'])}, "
                + f"response +{fmt_bytes(libp2p['response']['inbound']['payload_bytes'] + libp2p['response']['outbound']['payload_bytes'])}"
            )
            lines.append(
                f"- Full libp2p detail rows in JSON: `{libp2p['detail_entry_counts']['by_protocol']}` by_protocol, `{libp2p['detail_entry_counts']['by_topic']}` by_topic"
            )
            control_plane = libp2p.get("control_plane") or {}
            if control_plane.get("available"):
                lines.append(
                    f"- Libp2p control-plane counters: `+{fmt_num(control_plane['total_events'])}` {control_plane.get('units', 'events')} over `{control_plane['detail_entry_counts']['by_kind']}` by_kind rows in JSON"
                )
                lines.append(
                    f"- Libp2p control-plane wire bytes: total `+{fmt_bytes(control_plane.get('total_wire_bytes'))}`, inbound `+{fmt_bytes((control_plane.get('wire_bytes') or {}).get('inbound', {}).get('bytes'))}`, outbound `+{fmt_bytes((control_plane.get('wire_bytes') or {}).get('outbound', {}).get('bytes'))}`; scope=`{control_plane.get('wire_scope')}` excludes_transport_overhead=`{control_plane.get('excludes_transport_overhead')}`"
                )
                lines.extend(
                    render_top_counter_block(
                        "Top libp2p control-plane kinds",
                        control_plane.get("top_kinds"),
                        control_plane.get("units", "events"),
                    )
                )
            lines.extend(
                render_top_block(
                    "Top libp2p protocols", libp2p.get("top_protocols"), "messages"
                )
            )
            lines.extend(
                render_top_block("Top libp2p topics", libp2p.get("top_topics"), "messages")
            )
        lines.append("")
    return lines


def main():
    args = parse_args()
    history_path = Path(args.history_path)
    summary_json_path = Path(args.summary_json)
    summary_md_path = Path(args.summary_md)
    as_of = now_ms() if args.as_of_unix_ms is None else args.as_of_unix_ms
    generated_at = datetime.now(tz=timezone.utc).isoformat()

    records, total_records = prune_and_load_records(
        history_path, args.history_retention_minutes, as_of, args.as_of_unix_ms is not None
    )
    history_pruned_count = max(0, total_records - len(records))

    common = {
        "ok": True,
        "schema_version": 2,
        "as_of_unix_ms": as_of,
        "freshness_policy": {"version": 1, "max_sample_age_ms": args.max_sample_age_secs * 1000, "max_sample_gap_ms": args.max_sample_gap_secs * 1000},
        "generated_at": generated_at,
        "history_path": str(history_path),
        "history_retention_minutes": args.history_retention_minutes,
        "history_record_count_before_prune": total_records,
        "history_record_count": len(records),
        "history_pruned_count": history_pruned_count,
        "window_minutes_requested": args.window_minutes,
        "top_n": args.top_n,
    }

    if args.layout == "single-node":
        summary = dict(common)
        summary["node"] = summarize_record_set(records, args.window_minutes, args.top_n, as_of, args.max_sample_age_secs * 1000, args.max_sample_gap_secs * 1000, args.as_of_unix_ms is not None)
        markdown_lines = render_single_node_markdown(summary, history_path, generated_at)
    else:
        labels = args.label or ["local_node", "sequencer_ecs", "storage_ecs"]
        records_by_label = {label: [] for label in labels}
        for record in records:
            label = record.get("label") or record.get("node_label")
            if label in records_by_label:
                records_by_label[label].append(record)

        summary = dict(common)
        summary["run_id"] = args.run_id
        summary["run_dir"] = args.run_dir
        summary["nodes"] = {}
        for label in labels:
            summary["nodes"][label] = summarize_record_set(
                records_by_label[label], args.window_minutes, args.top_n, as_of, args.max_sample_age_secs * 1000, args.max_sample_gap_secs * 1000, args.as_of_unix_ms is not None
            )
            summary["nodes"][label]["label"] = label
        summary["aggregate"] = summarize_triad_aggregate(summary["nodes"], args.top_n)
        markdown_lines = render_triad_markdown(
            summary, history_path, generated_at, labels
        )

    summary_json_path.write_text(
        json.dumps(summary, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
    )
    summary_md_path.write_text("\n".join(markdown_lines) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
