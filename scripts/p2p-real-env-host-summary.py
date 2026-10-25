#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import math
import re
import shlex
import sys
import time
from datetime import datetime, timezone
from pathlib import Path

NODE_LABELS = ("local_node", "sequencer_ecs", "storage_ecs")



def sample_record(argv):
    path, label, index, service, storage_path, stage, error = argv
    now = datetime.now(timezone.utc).astimezone().isoformat()
    row = {"label": label, "sample_index": int(index), "captured_at": now,
           "captured_at_unix_ms": time.time_ns() // 1_000_000,
           "sample_fetch_ok": False, "stage": stage, "fetch_error": error}
    try:
        if error:
            raise ValueError("fetch")
        text = Path(path).read_text(encoding="utf-8")
        if len(text) > 65536:
            raise ValueError("payload")
        values = {}
        for line in text.splitlines():
            key, sep, value = line.partition("=")
            if not sep or not re.fullmatch(r"[a-z][a-z0-9_]*", key) or key in values:
                raise ValueError("payload")
            # Bash printf %q uses ANSI-C quotes for control characters. Decode
            # its escape alphabet as data; never invoke a shell on the payload.
            if value.startswith("$'"):
                if not value.endswith("'"):
                    raise ValueError("payload")
                escapes = {"a": "\a", "b": "\b", "e": "\x1b", "E": "\x1b", "f": "\f", "n": "\n", "r": "\r", "t": "\t", "v": "\v", "\\": "\\", "'": "'", '"': '"', "?": "?"}
                body = value[2:-1]
                decoded = []
                while body:
                    if body[0] != "\\":
                        if body[0] == "'":
                            raise ValueError("payload")
                        decoded.append(body[0].encode("utf-8")); body = body[1:]; continue
                    match = re.match(r"\\([0-7]{1,3}|x[0-9a-fA-F]{1,2}|u[0-9a-fA-F]{4}|U[0-9a-fA-F]{8}|.)", body)
                    if not match:
                        raise ValueError("payload")
                    token = match[1]
                    if token in escapes:
                        decoded.append(escapes[token].encode("utf-8"))
                    elif token[0] == "x":
                        decoded.append(bytes([int(token[1:], 16)]))
                    elif token[0] in "uU":
                        decoded.append(chr(int(token[1:], 16)).encode("utf-8"))
                    elif token[0] in "01234567":
                        decoded.append(bytes([int(token, 8)]))
                    else:
                        raise ValueError("payload")
                    body = body[len(match[0]):]
                parts = [b"".join(decoded).decode("utf-8")]
            else:
                parts = shlex.split(value)
            if len(parts) != 1:
                raise ValueError("payload")
            values[key] = parts[0]
        expected = {"hostname", "cpu_cores", "load1", "load5", "load15", "mem_total_bytes", "mem_available_bytes", "storage_path", "service_active_state", "service_sub_state", "service_main_pid", "runtime_pid"}
        expected.update("storage_" + key for key in ("total_bytes", "used_bytes", "available_bytes", "used_percent"))
        expected.update(prefix + "_" + key for prefix in ("wrapper", "runtime") for key in ("pcpu", "pmem", "elapsed_secs", "nlwp", "psr"))
        if set(values) != expected or values.get("storage_path") != storage_path:
            raise ValueError("payload")
        def number(key, optional=False):
            value = values.get(key)
            if optional and value == "":
                return None
            integer = (key == "cpu_cores" or key.endswith(("_bytes", "_pid", "_elapsed_secs", "_nlwp", "_psr")))
            pattern = r"[0-9]+" if integer else r"[0-9]+(?:\.[0-9]+)?"
            if value is None or not re.fullmatch(pattern, value):
                raise ValueError("numeric")
            result = float(value) if "." in value else int(value)
            if not math.isfinite(result):
                raise ValueError("numeric")
            return result
        def process(prefix, pid):
            result = {key: number(prefix + "_" + key, True)
                      for key in ("pcpu", "pmem", "elapsed_secs", "nlwp", "psr")}
            result.update(pid=pid, present=pid is not None and pid != 0)
            return result
        host = {"hostname": values["hostname"], "cpu_cores": number("cpu_cores"),
                "mem_total_bytes": number("mem_total_bytes"), "mem_available_bytes": number("mem_available_bytes")}
        host.update({"loadavg_" + suffix: number(key) for suffix, key in (("1m", "load1"), ("5m", "load5"), ("15m", "load15"))})
        pid = number("service_main_pid", True)
        row.update(service_name=service, host=host,
                   storage={"path": storage_path, **{key: number("storage_" + key, True) for key in ("total_bytes", "used_bytes", "available_bytes", "used_percent")}},
                   service={"active_state": values["service_active_state"], "sub_state": values["service_sub_state"], "main_pid": pid},
                   wrapper=process("wrapper", pid), runtime=process("runtime", number("runtime_pid", True)))
        row.update(sample_fetch_ok=True, stage="complete", fetch_error=None)
    except (ValueError, KeyError, OSError, UnicodeError, OverflowError):
        if not error:
            row.update(stage="parse", fetch_error="invalid_sample_payload")
    print(json.dumps(row, allow_nan=False))
    return 0 if row["sample_fetch_ok"] else 1

def parse_args():
    parser = argparse.ArgumentParser(
        description="Summarize triad host/process monitoring samples."
    )
    parser.add_argument("--history-path", required=True)
    parser.add_argument("--summary-json", required=True)
    parser.add_argument("--summary-md", required=True)
    parser.add_argument("--run-id")
    parser.add_argument("--expected-samples-per-node", type=int)
    parser.add_argument("--run-dir")
    return parser.parse_args()


def load_grouped_records(path: Path) -> tuple[dict[str, list[dict]], int]:
    grouped: dict[str, list[dict]] = {label: [] for label in NODE_LABELS}
    record_count = 0
    if not path.is_file():
        return grouped, record_count
    with path.open("r", encoding="utf-8") as handle:
        for raw_line in handle:
            line = raw_line.strip()
            if not line:
                continue
            record_count += 1
            record = json.loads(line)
            label = record.get("label")
            if label in grouped:
                grouped[label].append(record)
    return grouped, record_count


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


def fmt_bytes(value):
    if value is None:
        return "n/a"
    amount = float(value)
    for unit in ("B", "KiB", "MiB", "GiB", "TiB"):
        if amount < 1024.0 or unit == "TiB":
            return f"{amount:.2f} {unit}" if unit != "B" else f"{int(amount)} B"
        amount /= 1024.0
    return f"{int(value)} B"


def to_float(value):
    if value in (None, ""):
        return None
    return float(value)


def to_int(value):
    if value in (None, ""):
        return None
    return int(value)


def ratio_percent(part, total):
    if part in (None, "") or total in (None, ""):
        return None
    total_value = float(total)
    if total_value <= 0:
        return None
    return round(float(part) * 100.0 / total_value, 2)


def summarize_successes(records: list[dict]) -> dict:
    if not records:
        return {"available": False, "sample_count": 0}

    latest = max(records, key=lambda item: int(item.get("captured_at_unix_ms") or 0))
    latest_host = latest.get("host") or {}
    latest_runtime = latest.get("runtime") or {}
    cpu_cores = to_int(latest_host.get("cpu_cores")) or 0

    runtime_cpu_values = [
        to_float((record.get("runtime") or {}).get("pcpu"))
        for record in records
        if (record.get("runtime") or {}).get("present") is True
    ]
    runtime_mem_values = [
        to_float((record.get("runtime") or {}).get("pmem"))
        for record in records
        if (record.get("runtime") or {}).get("present") is True
    ]
    load1_values = [to_float((record.get("host") or {}).get("loadavg_1m")) for record in records]
    load_ratio_values = [
        (load / cpu_cores) if load is not None and cpu_cores > 0 else None
        for load in load1_values
    ]
    mem_available_values = [
        ratio_percent(
            (record.get("host") or {}).get("mem_available_bytes"),
            (record.get("host") or {}).get("mem_total_bytes"),
        )
        for record in records
    ]
    storage_used_values = [
        to_float((record.get("storage") or {}).get("used_percent")) for record in records
    ]

    latest_runtime_cpu = to_float(latest_runtime.get("pcpu"))
    latest_runtime_cpu_ratio = (
        round(latest_runtime_cpu / (cpu_cores * 100.0), 4)
        if latest_runtime_cpu is not None and cpu_cores > 0
        else None
    )
    latest_load1 = to_float(latest_host.get("loadavg_1m"))
    latest_load_ratio = round(latest_load1 / cpu_cores, 4) if latest_load1 is not None and cpu_cores > 0 else None
    latest_mem_available_percent = ratio_percent(
        latest_host.get("mem_available_bytes"),
        latest_host.get("mem_total_bytes"),
    )
    latest_storage_used_percent = to_float((latest.get("storage") or {}).get("used_percent"))

    alerts = []
    runtime_cpu_status = "unavailable"
    if latest_runtime.get("present") is True and latest_runtime_cpu_ratio is not None:
        if latest_runtime_cpu_ratio >= 0.75:
            runtime_cpu_status = "hot"
            alerts.append("runtime_cpu_hot")
        elif latest_runtime_cpu_ratio >= 0.50:
            runtime_cpu_status = "elevated"
        else:
            runtime_cpu_status = "normal"
    elif latest_runtime.get("present") is not True:
        alerts.append("runtime_process_missing")

    load_status = "unavailable"
    if latest_load_ratio is not None:
        if latest_load_ratio >= 1.0:
            load_status = "hot"
            alerts.append("host_load_hot")
        elif latest_load_ratio >= 0.75:
            load_status = "elevated"
        else:
            load_status = "normal"

    memory_status = "unavailable"
    if latest_mem_available_percent is not None:
        if latest_mem_available_percent < 15.0:
            memory_status = "low"
            alerts.append("memory_available_low")
        elif latest_mem_available_percent < 25.0:
            memory_status = "tight"
        else:
            memory_status = "normal"

    storage_status = "unavailable"
    if latest_storage_used_percent is not None:
        if latest_storage_used_percent >= 90.0:
            storage_status = "critical"
            alerts.append("storage_usage_critical")
        elif latest_storage_used_percent >= 85.0:
            storage_status = "high"
            alerts.append("storage_usage_high")
        else:
            storage_status = "normal"
    else:
        alerts.append("storage_metrics_unavailable")

    service_state = latest.get("service") or {}
    if service_state.get("active_state") != "active" or service_state.get("sub_state") != "running":
        alerts.append("service_not_running")

    return {
        "available": True,
        "sample_count": len(records),
        "latest": {
            "captured_at": latest.get("captured_at"),
            "hostname": latest_host.get("hostname"),
            "cpu_cores": cpu_cores,
            "loadavg_1m": latest_load1,
            "loadavg_5m": to_float(latest_host.get("loadavg_5m")),
            "loadavg_15m": to_float(latest_host.get("loadavg_15m")),
            "load_per_core_ratio_1m": latest_load_ratio,
            "mem_total_bytes": to_int(latest_host.get("mem_total_bytes")),
            "mem_available_bytes": to_int(latest_host.get("mem_available_bytes")),
            "mem_available_percent": latest_mem_available_percent,
            "storage": latest.get("storage"),
            "service": service_state,
            "wrapper": latest.get("wrapper"),
            "runtime": latest_runtime,
            "runtime_cpu_percent": latest_runtime_cpu,
            "runtime_cpu_core_ratio": latest_runtime_cpu_ratio,
        },
        "peaks": {
            "max_runtime_cpu_percent": max((value for value in runtime_cpu_values if value is not None), default=None),
            "max_runtime_cpu_core_ratio": max(
                (
                    round(value / (cpu_cores * 100.0), 4)
                    for value in runtime_cpu_values
                    if value is not None and cpu_cores > 0
                ),
                default=None,
            ),
            "max_runtime_mem_percent": max((value for value in runtime_mem_values if value is not None), default=None),
            "max_loadavg_1m": max((value for value in load1_values if value is not None), default=None),
            "max_load_per_core_ratio_1m": max((value for value in load_ratio_values if value is not None), default=None),
            "min_mem_available_percent": min(
                (value for value in mem_available_values if value is not None),
                default=None,
            ),
            "max_storage_used_percent": max(
                (value for value in storage_used_values if value is not None),
                default=None,
            ),
        },
        "status": {
            "runtime_cpu": runtime_cpu_status,
            "host_load": load_status,
            "memory": memory_status,
            "storage": storage_status,
        },
        "alerts": sorted(set(alerts)),
    }



def summarize_node(records: list[dict]) -> dict:
    successes = [r for r in records if r.get("sample_fetch_ok", True) is True]
    failed = len(records) - len(successes)
    node = summarize_successes(successes)
    latest = records[-1] if records else {}
    node.update(sample_count=len(records), success_sample_count=len(successes),
                failed_sample_count=failed, latest_attempt=({k: latest.get(k) for k in
                ("sample_fetch_ok", "sample_index", "captured_at", "captured_at_unix_ms", "stage", "fetch_error")} if latest else {}),
                last_success=successes[-1] if successes else None)
    if latest:
        node["latest_attempt"]["sample_fetch_ok"] = latest.get("sample_fetch_ok", True)
        node["latest_attempt"]["result"] = "success" if latest.get("sample_fetch_ok", True) is True else "failed"
    if latest and latest.get("sample_fetch_ok", True) is not True:
        node.update(available=False, latest={}, alerts=["sample_fetch_failed"],
                    status={key: "unavailable" for key in ("runtime_cpu", "host_load", "memory", "storage")})
    node.setdefault("latest", {})
    return node

def render_markdown(summary: dict, history_path: str) -> list[str]:
    lines = [
        "# P2P Real Env Host Monitor Summary",
        "",
        f"- Generated at: `{summary['generated_at']}`",
        f"- History path: `{history_path}`",
        f"- Run id: `{summary.get('run_id') or 'n/a'}`",
        f"- Collection complete: `{summary['aggregate']['complete']}`; failed samples: `{summary['aggregate']['failed_sample_count']}`; failed nodes: `{', '.join(summary['aggregate']['failed_nodes']) or '(none)'}`",
        f"- Nodes with data: `{summary['aggregate']['node_count']}`",
        f"- Nodes with alerts: `{summary['aggregate']['alerted_node_count']}`",
        f"- Highest runtime CPU node: `{summary['aggregate']['highest_runtime_cpu_node'] or 'n/a'}`",
        "",
    ]
    for label in NODE_LABELS:
        node = summary["nodes"].get(label) or {"available": False}
        lines.append(f"## {label}")
        if node.get("available") is not True:
            lines.append(f"- unavailable; latest attempt: `{node.get('latest_attempt')}`; successful samples: `{node.get('success_sample_count', 0)}`")
            lines.append("")
            continue
        latest = node["latest"]
        peaks = node["peaks"]
        status = node["status"]
        storage = latest.get("storage") or {}
        lines.extend(
            [
                f"- Host: `{latest.get('hostname')}` cores=`{fmt_num(latest.get('cpu_cores'))}`",
                f"- Runtime CPU: latest `{fmt_percent(latest.get('runtime_cpu_percent'))}` core-ratio `{fmt_percent((latest.get('runtime_cpu_core_ratio') or 0) * 100) if latest.get('runtime_cpu_core_ratio') is not None else 'n/a'}`; peak `{fmt_percent(peaks.get('max_runtime_cpu_percent'))}`",
                f"- Loadavg 1m: latest `{fmt_num(latest.get('loadavg_1m'))}` ratio/core `{fmt_num(latest.get('load_per_core_ratio_1m'))}`; peak `{fmt_num(peaks.get('max_loadavg_1m'))}`",
                f"- Memory available: `{fmt_bytes(latest.get('mem_available_bytes'))}` / `{fmt_bytes(latest.get('mem_total_bytes'))}` ({fmt_percent(latest.get('mem_available_percent'))}); floor `{fmt_percent(peaks.get('min_mem_available_percent'))}`",
                f"- Storage `{storage.get('path') or 'n/a'}`: used `{fmt_percent(storage.get('used_percent'))}` (`{fmt_bytes(storage.get('used_bytes'))}` / `{fmt_bytes(storage.get('total_bytes'))}`)",
                f"- Service: active=`{(latest.get('service') or {}).get('active_state')}` sub=`{(latest.get('service') or {}).get('sub_state')}` main_pid=`{(latest.get('service') or {}).get('main_pid')}` runtime_pid=`{(latest.get('runtime') or {}).get('pid')}`",
                f"- Health status: runtime_cpu=`{status.get('runtime_cpu')}` load=`{status.get('host_load')}` memory=`{status.get('memory')}` storage=`{status.get('storage')}`",
                f"- Alerts: `{', '.join(node.get('alerts') or ['(none)'])}`",
                "",
            ]
        )
    return lines


def main():
    args = parse_args()
    history_path = Path(args.history_path)
    grouped, record_count = load_grouped_records(history_path)

    node_summaries = {label: summarize_node(grouped[label]) for label in NODE_LABELS}
    alerted_nodes = [label for label, node in node_summaries.items() if node.get("alerts")]

    highest_runtime_cpu_node = None
    highest_runtime_cpu_value = -1.0
    for label, node in node_summaries.items():
        value = node.get("latest", {}).get("runtime_cpu_percent")
        if value is None:
            continue
        if value > highest_runtime_cpu_value:
            highest_runtime_cpu_value = value
            highest_runtime_cpu_node = label

    expected_per_node = args.expected_samples_per_node or max((len(rows) for rows in grouped.values()), default=0)
    summary = {
        "schema_version": 2,
        "generated_at": datetime.now(timezone.utc).astimezone().isoformat(),
        "run_id": args.run_id,
        "run_dir": args.run_dir,
        "history_path": str(history_path),
        "history_record_count": record_count,
        "nodes": node_summaries,
        "aggregate": {
            "complete": expected_per_node > 0 and all(node.get("sample_count", 0) == expected_per_node and node.get("failed_sample_count", 0) == 0 for node in node_summaries.values()),
            "expected_sample_count": expected_per_node * len(NODE_LABELS),
            "attempted_count": record_count,
            "failed_sample_count": sum(node["failed_sample_count"] for node in node_summaries.values()),
            "failed_nodes": [label for label, node in node_summaries.items() if node["failed_sample_count"]],
            "node_count": len([node for node in node_summaries.values() if node.get("available") is True]),
            "alerted_node_count": len(alerted_nodes),
            "alerted_nodes": alerted_nodes,
            "highest_runtime_cpu_node": highest_runtime_cpu_node,
            "highest_runtime_cpu_percent": None if highest_runtime_cpu_value < 0 else highest_runtime_cpu_value,
        },
    }

    Path(args.summary_json).write_text(
        json.dumps(summary, ensure_ascii=False, indent=2) + "\n",
        encoding="utf-8",
    )
    Path(args.summary_md).write_text(
        "\n".join(render_markdown(summary, str(history_path))) + "\n",
        encoding="utf-8",
    )


if __name__ == "__main__":
    if len(sys.argv) > 1 and sys.argv[1] == "--sample-env":
        sys.exit(sample_record(sys.argv[2:]))
    main()
