#!/usr/bin/env python3
"""Observe runner/cgroup memory without controlling the ECS build process."""
import argparse
from datetime import datetime, timezone
import json
import math
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


class ProcessTree:
    """Observe one fixed process identity; never signal or inspect its arguments."""
    def __init__(self, pid, proc=Path("/proc"), clock_ticks=None, page_size=None):
        self.pid, self.proc = pid, proc
        try:
            self.clock_ticks = clock_ticks or os.sysconf("SC_CLK_TCK")
            self.page_size = page_size or os.sysconf("SC_PAGE_SIZE")
        except (AttributeError, OSError, ValueError):
            self.clock_ticks = self.page_size = None
        initial = self.read_process(pid)
        self.start_ticks = initial["start_ticks"] if initial else None
        # Do not attach later if the target was already gone or unreadable.
        self.exited = False if initial else (True if self.missing() else None)
        self.latest = self.peak = None
        self.samples = 0

    def missing(self):
        return self.proc.is_dir() and not (self.proc / str(self.pid)).exists()

    def read_process(self, pid):
        if not self.clock_ticks or not self.page_size:
            return None
        try:
            raw = (self.proc / str(pid) / "stat").read_text()
            opening, closing = raw.index("("), raw.rindex(")")
            fields = raw[closing + 1:].split()
            if int(raw[:opening]) != pid or len(fields) < 22:
                return None
            parent, user, system, start, rss = (int(fields[index]) for index in (1, 11, 12, 19, 21))
            if min(parent, user, system, start, rss) < 0:
                return None
            return dict(pid=pid, ppid=parent, comm=raw[opening + 1:closing], state=fields[0],
                        start_ticks=start, cpu_seconds=(user + system) / self.clock_ticks,
                        rss_bytes=rss * self.page_size)
        except (OSError, UnicodeError, ValueError):
            return None

    def sample(self):
        available = False
        if self.start_ticks is not None and not self.exited:
            root = self.read_process(self.pid)
            if root is None or root["start_ticks"] != self.start_ticks:
                self.exited = True if root is not None or self.missing() else None
            else:
                processes = {self.pid: root}
                try:
                    for path in self.proc.iterdir():
                        if path.name.isdecimal() and int(path.name) != self.pid:
                            process = self.read_process(int(path.name))
                            if process is not None:
                                processes[process["pid"]] = process
                except OSError:
                    processes = {}
                # A reused parent PID cannot adopt a process older than itself.
                selected = {self.pid: root}
                while True:
                    added = {pid: process for pid, process in processes.items()
                             if pid not in selected and process["ppid"] in selected
                             and process["start_ticks"] >= selected[process["ppid"]]["start_ticks"]}
                    if not added:
                        break
                    selected.update(added)
                final = self.read_process(self.pid)
                if final is None or final["start_ticks"] != self.start_ticks:
                    self.exited = True if final is not None or self.missing() else None
                elif processes:
                    available = True
                    self.exited = False
                    self.latest = dict(observed_at=datetime.now(timezone.utc).isoformat(),
                        processes=[selected[pid] for pid in sorted(selected)],
                        cpu_seconds=round(sum(process["cpu_seconds"] for process in selected.values()), 6),
                        rss_bytes=sum(process["rss_bytes"] for process in selected.values()))
                    self.peak = max(self.peak or 0, self.latest["rss_bytes"])
                    self.samples += 1
        return dict(root_pid=self.pid, root_start_ticks=self.start_ticks, available=available,
                    target_exited=self.exited, latest=self.latest, sample_count=self.samples,
                    sampled_peak_rss_bytes=self.peak,
                    scope="Read-only /proc/stat snapshots of the fixed root and current descendants. "
                          "RSS sums may count shared pages more than once and miss short peaks. "
                          "CPU seconds sum currently observed processes, not cumulative build CPU; "
                          "the sum may fall when workers exit. Latest retains the last valid sample. "
                          "Target disappearance and observer completion do not establish build success.")


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
    parser.add_argument("--pid", type=int, help="observe this fixed process and its current descendants")
    parser.add_argument("--label", help="variant label for optional process heartbeats")
    parser.add_argument("--heartbeat-seconds", type=float, default=20)
    args = parser.parse_args()
    if args.pid is not None and args.pid < 1:
        parser.error("--pid must be positive")
    if not math.isfinite(args.heartbeat_seconds) or args.heartbeat_seconds <= 0:
        parser.error("--heartbeat-seconds must be finite and positive")
    stopping = False
    def stop(_signal, _frame):
        nonlocal stopping
        stopping = True
    for signum in (signal.SIGTERM, signal.SIGINT):
        signal.signal(signum, stop)
    started = time.monotonic()
    cgroup = cgroup_path()
    initial = sample(Path("/proc"), cgroup)
    process_tree = ProcessTree(args.pid) if args.pid is not None else None
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
    if process_tree is not None:
        report.update(label=args.label, heartbeat_interval_seconds=args.heartbeat_seconds)
    next_heartbeat = started
    current = initial
    while True:
        update(report, initial, current)
        target_exited = False
        if process_tree is not None:
            report["process_tree"] = process_tree.sample()
            target_exited = report["process_tree"]["target_exited"] is True
        now = time.monotonic()
        report.update(complete=stopping or target_exited, elapsed_seconds=round(now - started, 3), ended_at=datetime.now(timezone.utc).isoformat())
        write_report(args.out, report)
        if process_tree is not None and (now >= next_heartbeat or report["complete"]):
            print("native_build_heartbeat=" + json.dumps(dict(label=args.label,
                elapsed_seconds=report["elapsed_seconds"], complete=report["complete"],
                host=current["host"], swap_pages=current["swap_pages"],
                process_tree=report["process_tree"]), sort_keys=True), flush=True)
            next_heartbeat = now + args.heartbeat_seconds
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
