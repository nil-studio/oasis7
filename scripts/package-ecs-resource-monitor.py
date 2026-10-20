#!/usr/bin/env python3
"""Observe runner/cgroup memory without controlling the ECS build process."""
import argparse
from datetime import datetime, timezone
import json
import os
from pathlib import Path
import re
import signal
import sys
import time


def read_text(path):
    try:
        return path.read_text()
    except (OSError, UnicodeError):
        return ""


def read_fields(path):
    values = {}
    for line in read_text(path).splitlines():
        fields = line.split()
        if len(fields) >= 2:
            try:
                values[fields[0].rstrip(":")] = int(fields[1]) * (1024 if fields[2:] == ["kB"] else 1)
            except ValueError:
                pass
    return values


def cgroup_path(proc=Path("/proc")):
    memberships = [line[3:] for line in read_text(proc / "self/cgroup").splitlines() if line.startswith("0::")]
    if not memberships or not memberships[0].startswith("/") or ".." in Path(memberships[0]).parts:
        return None
    for line in read_text(proc / "self/mountinfo").splitlines():
        mount, separator, filesystem = line.partition(" - ")
        fields = mount.split()
        if not separator or filesystem.split()[:1] != ["cgroup2"] or len(fields) < 5:
            continue
        root, destination = [Path(re.sub(r"\\([0-7]{3})", lambda match: chr(int(match[1], 8)), value)) for value in fields[3:5]]
        try:
            return destination / Path(memberships[0]).relative_to(root)
        except ValueError:
            continue
    return None


def read_number(path):
    value = read_text(path).strip() if path is not None else ""
    try:
        return int(value)
    except ValueError:
        return "max" if value == "max" else None


def sample(proc, cgroup):
    memory, vmstat = read_fields(proc / "meminfo"), read_fields(proc / "vmstat")
    return {
        "host": {name: memory.get(key) for name, key in (
            ("mem_total_bytes", "MemTotal"), ("mem_available_bytes", "MemAvailable"),
            ("swap_free_bytes", "SwapFree"))},
        "swap_pages": {key: vmstat.get(key) for key in ("pswpin", "pswpout")},
        "cgroup": {key: read_number(cgroup / filename if cgroup else None) for key, filename in (
            ("memory_current_bytes", "memory.current"), ("memory_max_bytes", "memory.max"),
            ("memory_peak_since_creation_bytes", "memory.peak"))},
        "events": (read_fields(cgroup / "memory.events") or None) if cgroup else None,
    }


def update_bounds(metrics, values):
    for key, value in values.items():
        bounds = metrics.setdefault(key, {"start": value, "end": value, "min": None, "max": None})
        bounds["end"] = value
        if isinstance(value, int):
            bounds["min"] = value if bounds["min"] is None else min(bounds["min"], value)
            bounds["max"] = value if bounds["max"] is None else max(bounds["max"], value)


def counter_delta(before, after):
    if before is None or after is None:
        return None
    return {key: after[key] - before[key]
            if isinstance(before.get(key), int) and isinstance(after.get(key), int)
            and after[key] >= before[key] else None
            for key in before.keys() | after.keys()}


def update(report, initial, current):
    update_bounds(report.setdefault("host", {}), current["host"])
    group = report.setdefault("cgroup", {})
    update_bounds(group.setdefault("sampled", {}), {"memory_current_bytes": current["cgroup"]["memory_current_bytes"]})
    for key in ("memory_max_bytes", "memory_peak_since_creation_bytes"):
        group[key] = {"start": initial["cgroup"][key], "end": current["cgroup"][key]}
    group["events"] = {"start": initial["events"], "end": current["events"], "delta": counter_delta(initial["events"], current["events"])}
    report["swap_pages"] = {"start": initial["swap_pages"], "end": current["swap_pages"], "delta": counter_delta(initial["swap_pages"], current["swap_pages"])}
    report["sample_count"] += 1


def write_report(path, report):
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name(f".{path.name}.{os.getpid()}.tmp")
    try:
        temporary.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
        temporary.replace(path)
    finally:
        temporary.unlink(missing_ok=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--jobs", type=int, choices=(1, 2), default=os.environ.get("CARGO_BUILD_JOBS"))
    args = parser.parse_args()
    stopping = False
    def stop(_signal, _frame):
        nonlocal stopping
        stopping = True
    for signum in (signal.SIGTERM, signal.SIGINT):
        signal.signal(signum, stop)
    started = time.monotonic()
    cgroup = cgroup_path()
    initial = sample(Path("/proc"), cgroup)
    report = {
        "schema": 1, "jobs": args.jobs,
        "started_at": datetime.now(timezone.utc).isoformat(), "sample_interval_seconds": 1,
        "sample_count": 0, "complete": False,
        "cgroup": {"path": str(cgroup) if cgroup else None},
        "scope": "Host and cgroup measurements include other runner processes. Min/max are sampled; "
                 "memory_peak_since_creation_bytes is the existing cgroup historical peak, not this build's peak. "
                 "Missing counters or counter resets produce null deltas. "
                 "complete means the observer stopped cleanly, not that the build succeeded.",
    }
    current = initial
    while True:
        update(report, initial, current)
        report.update(complete=stopping, elapsed_seconds=round(time.monotonic() - started, 3), ended_at=datetime.now(timezone.utc).isoformat())
        write_report(args.out, report)
        if report["complete"]:
            return
        time.sleep(1)
        current = sample(Path("/proc"), cgroup)


if __name__ == "__main__":
    try:
        main()
    except OSError as error:
        print(f"warning: ECS resource monitor could not write its report: {error}", file=sys.stderr)
        sys.exit(1)
