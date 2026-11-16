#!/usr/bin/env python3
"""Check sampling units, unavailable resources, and graceful observer shutdown."""
import importlib.util
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import time
import unittest
from unittest.mock import patch

SCRIPT = Path(__file__).with_name("package-ecs-resource-monitor.py")
SPEC = importlib.util.spec_from_file_location("monitor", SCRIPT)
monitor = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(monitor)


class MonitorTests(unittest.TestCase):
    def test_heartbeat_cgroup_fields_are_bounded_and_preserve_missing_counters(self):
        current = dict(cgroup=dict(memory_current_bytes=200, memory_max_bytes="max",
                                   memory_peak_since_creation_bytes=900, unrelated="not logged"))
        events = dict(start=dict(low=0, high=1, max=2, oom=3, oom_kill=4, oom_group_kill=5,
                                 unrelated="not logged"),
                      end=dict(low=0, high=2, max=4, oom=3, oom_kill=4, oom_group_kill=5),
                      delta=dict(low=0, high=1, max=2, oom=0, oom_kill=0, oom_group_kill=0))
        observed = monitor.cgroup_heartbeat(current, events)
        self.assertEqual(observed, dict(memory_current_bytes=200, memory_max_bytes="max",
            memory_peak_since_creation_bytes=900,
            events={phase: {key: snapshot[key] for key in
                    ("low", "high", "max", "oom", "oom_kill", "oom_group_kill")}
                    for phase, snapshot in events.items()}))
        self.assertNotIn("unrelated", json.dumps(observed))
        for value in current["cgroup"]:
            current["cgroup"][value] = None
        missing = monitor.cgroup_heartbeat(current, dict(start=None, end={"oom_kill": 1}, delta=None))
        self.assertIsNone(missing["memory_current_bytes"])
        self.assertIsNone(missing["events"]["start"])
        self.assertIsNone(missing["events"]["delta"])
        self.assertEqual(missing["events"]["end"]["oom_kill"], 1)
        self.assertIsNone(missing["events"]["end"]["oom"])

    def test_units_missing_fields_and_counter_resets(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "meminfo").write_text("MemTotal: 8192 kB\nMemAvailable: 4096 kB\nSwapFree: invalid kB\n")
            (root / "vmstat").write_text("pswpin 4\npswpout 8\n")
            initial = monitor.sample(root, None)
            self.assertEqual(initial["host"], {"mem_total_bytes": 8388608, "mem_available_bytes": 4194304, "swap_free_bytes": None})
            self.assertEqual(initial["swap_pages"], {"pswpin": 4, "pswpout": 8})
            self.assertIsNone(initial["events"])
            self.assertTrue(all(value is None for value in initial["cgroup"].values()))
            report = {"sample_count": 0}
            monitor.update(report, initial, initial)
            (root / "meminfo").write_text("MemAvailable: 2048 kB\n")
            (root / "vmstat").write_text("pswpin 7\npswpout 1\n")
            monitor.update(report, initial, monitor.sample(root, None))
            self.assertEqual(report["host"]["mem_available_bytes"], {"start": 4194304, "end": 2097152, "min": 2097152, "max": 4194304})
            self.assertEqual(report["swap_pages"]["delta"], {"pswpin": 3, "pswpout": None})
            self.assertIsNone(report["cgroup"]["events"]["delta"])
            self.assertEqual(monitor.read_fields(root / "missing"), {})
            self.assertTrue(all(value is None for value in monitor.sample(root / "missing", None)["host"].values()))

    def test_cgroup_mount_mapping_and_historical_peak(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            proc, mount = root / "proc", root / "cgroup space"
            (proc / "self").mkdir(parents=True)
            cgroup = mount / "build"
            cgroup.mkdir(parents=True)
            (proc / "self/cgroup").write_text("0::/runner/build\n")
            escaped = str(mount).replace(" ", "\\040")
            (proc / "self/mountinfo").write_text(f"29 23 0:26 /runner {escaped} rw - cgroup2 cgroup rw\n")
            self.assertEqual(monitor.cgroup_path(proc), cgroup)
            for filename, value in {"memory.current": "100", "memory.max": "max", "memory.peak": "900", "memory.events": "oom 0\noom_kill 0\n"}.items():
                (cgroup / filename).write_text(value)
            initial = monitor.sample(proc, cgroup)
            report = {"sample_count": 0}
            monitor.update(report, initial, initial)
            (cgroup / "memory.current").write_text("200")
            (cgroup / "memory.events").write_text("oom 1\noom_kill 1\n")
            monitor.update(report, initial, monitor.sample(proc, cgroup))
            self.assertEqual(report["cgroup"]["sampled"]["memory_current_bytes"]["max"], 200)
            self.assertEqual(report["cgroup"]["memory_peak_since_creation_bytes"], {"start": 900, "end": 900})
            self.assertEqual(report["cgroup"]["memory_max_bytes"], {"start": "max", "end": "max"})
            self.assertEqual(report["cgroup"]["events"]["delta"], {"oom": 1, "oom_kill": 1})
            (proc / "self/cgroup").write_text("0::/unrelated/build\n")
            self.assertIsNone(monitor.cgroup_path(proc))
            (proc / "self/cgroup").write_text("0::/runner/../../outside\n")
            self.assertIsNone(monitor.cgroup_path(proc))
            (proc / "self/cgroup").unlink()
            self.assertIsNone(monitor.cgroup_path(proc))

    def test_real_sampling_and_signal_shutdown(self):
        for signum in (signal.SIGTERM, signal.SIGINT):
            with self.subTest(signal=signum), tempfile.TemporaryDirectory() as directory:
                output = Path(directory) / "resources.json"
                process = subprocess.Popen([sys.executable, str(SCRIPT), "--out", str(output)], env=dict(os.environ, CARGO_BUILD_JOBS="2"), stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
                try:
                    deadline = time.monotonic() + 5
                    expected = 2 if signum == signal.SIGTERM else 1
                    observed = None
                    while process.poll() is None and time.monotonic() < deadline:
                        if output.exists():
                            observed = json.loads(output.read_text())
                            if observed["sample_count"] >= expected:
                                break
                        time.sleep(0.02)
                    self.assertIsNotNone(observed)
                    self.assertGreaterEqual(observed["sample_count"], expected)
                    first = observed
                    self.assertFalse(first["complete"])
                    self.assertEqual(first["jobs"], 2)
                    process.send_signal(signum)
                    stdout, stderr = process.communicate(timeout=5)
                    self.assertEqual(process.returncode, 0, stderr)
                    self.assertEqual(stdout, "")
                    report = json.loads(output.read_text())
                    self.assertTrue(report["complete"])
                    self.assertNotIn("process_tree", report)
                    self.assertNotIn("label", report)
                    self.assertGreaterEqual(report["sample_count"], 2)
                    self.assertEqual(report["host"]["mem_total_bytes"]["start"], first["host"]["mem_total_bytes"]["start"])
                    self.assertEqual(list(output.parent.glob("*.tmp")), [])
                finally:
                    if process.poll() is None:
                        process.kill()
                    process.communicate(timeout=5)

    def test_unwritable_output_is_observer_failure(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "directory"
            output.mkdir()
            result = subprocess.run([sys.executable, str(SCRIPT), "--out", str(output), "--jobs", "1"], capture_output=True, text=True, timeout=5)
            self.assertEqual(result.returncode, 1)
            self.assertIn("warning: ECS resource monitor", result.stderr)


class ProcessMonitorTests(unittest.TestCase):
    def process(self, proc, pid, *, parent=1, start=10, user=0, system=0, pages=1,
                name="cargo", state="S"):
        directory = proc / str(pid)
        directory.mkdir(parents=True, exist_ok=True)
        fields = ["0"] * 22
        for index, value in {0: state, 1: parent, 11: user, 12: system,
                             19: start, 21: pages}.items():
            fields[index] = str(value)
        (directory / "stat").write_text(f"{pid} ({name}) " + " ".join(fields) + "\n")
        (directory / "cmdline").write_text("private command argument")
        (directory / "environ").write_text("SECRET=private environment")

    def observer(self, proc, pid=100):
        return monitor.ProcessTree(pid, proc=proc, clock_ticks=100, page_size=4096)

    def test_descendants_stat_units_parentheses_and_no_private_process_inputs(self):
        with tempfile.TemporaryDirectory() as directory:
            proc = Path(directory)
            self.process(proc, 100, user=100, system=50, pages=2)
            self.process(proc, 101, parent=100, start=20, user=30, system=20,
                         pages=3, name="rustc (phase) )")
            self.process(proc, 102, parent=101, start=30, user=50, pages=4, state="R")
            self.process(proc, 999, pages=1000, name="unrelated")
            read_text = Path.read_text
            def stat_only(path, *args, **kwargs):
                self.assertEqual(path.name, "stat")
                return read_text(path, *args, **kwargs)
            with patch.object(Path, "read_text", stat_only), \
                 patch.object(Path, "read_bytes", side_effect=AssertionError("no private bytes")):
                observed = self.observer(proc).sample()
            latest = observed["latest"]
            self.assertEqual([process["pid"] for process in latest["processes"]], [100, 101, 102])
            self.assertEqual(latest["processes"][1], dict(pid=101, ppid=100, comm="rustc (phase) )",
                state="S", start_ticks=20, cpu_seconds=0.5, rss_bytes=12288))
            self.assertEqual(latest["cpu_seconds"], 2.5)
            self.assertEqual(latest["rss_bytes"], 9 * 4096)
            self.assertEqual(observed["sampled_peak_rss_bytes"], 9 * 4096)
            self.assertFalse(observed["target_exited"])

    def test_reused_parent_cannot_adopt_older_descendants(self):
        with tempfile.TemporaryDirectory() as directory:
            proc = Path(directory)
            self.process(proc, 100)
            self.process(proc, 101, parent=100, start=20)
            self.process(proc, 102, parent=101, start=30, pages=50)
            observer = self.observer(proc)
            first = observer.sample()
            self.process(proc, 101, parent=100, start=40)
            self.process(proc, 103, parent=101, start=41)
            current = observer.sample()
            self.assertEqual([process["pid"] for process in current["latest"]["processes"]], [100, 101, 103])
            self.assertEqual(current["latest"]["rss_bytes"], 3 * 4096)
            self.assertEqual(current["sampled_peak_rss_bytes"], first["sampled_peak_rss_bytes"])

    def test_root_identity_is_never_rebound_after_exit_or_pid_reuse(self):
        with tempfile.TemporaryDirectory() as directory:
            proc = Path(directory)
            self.process(proc, 100, start=10, pages=2)
            observer = self.observer(proc)
            first = observer.sample()
            self.process(proc, 100, start=99, pages=1000, name="replacement")
            self.process(proc, 101, parent=100, start=100, pages=1000)
            current = observer.sample()
            self.assertTrue(current["target_exited"])
            self.assertEqual(current["root_start_ticks"], 10)
            self.assertEqual(current["latest"], first["latest"])
            self.assertEqual(current["sampled_peak_rss_bytes"], 2 * 4096)
            missing = self.observer(proc, 200)
            self.process(proc, 200, start=200, pages=1000)
            self.assertIsNone(missing.sample()["latest"])
            self.assertTrue(missing.sample()["target_exited"])

    def test_unavailable_or_malformed_stat_is_not_zero_usage_or_target_success(self):
        with tempfile.TemporaryDirectory() as directory:
            proc = Path(directory)
            for root in (proc / "unavailable", proc):
                if root == proc:
                    self.process(proc, 100)
                    (proc / "100/stat").write_text("malformed stat")
                observed = self.observer(root).sample()
                self.assertIsNone(observed["latest"])
                self.assertIsNone(observed["sampled_peak_rss_bytes"])
                self.assertIsNone(observed["target_exited"])

    @unittest.skipUnless(sys.platform == "linux", "Linux process observations")
    def test_real_heartbeats_and_observer_shutdown_never_stop_target(self):
        for target_exits in (False, True):
            with self.subTest(target_exits=target_exits), tempfile.TemporaryDirectory() as directory:
                report_path = Path(directory) / "resources.json"
                child = subprocess.Popen([sys.executable, "-c", "import time; time.sleep(30)"],
                                         stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
                observer = subprocess.Popen([sys.executable, str(SCRIPT), "--out", str(report_path),
                    "--pid", str(child.pid), "--label", "formal-required", "--heartbeat-seconds", "0.05"],
                    stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
                try:
                    deadline = time.monotonic() + 5
                    observed = None
                    while observer.poll() is None and time.monotonic() < deadline:
                        if report_path.exists():
                            observed = json.loads(report_path.read_text())
                            if observed["sample_count"] >= 2:
                                break
                        time.sleep(0.02)
                    self.assertIsNotNone(observed)
                    self.assertGreaterEqual(observed["sample_count"], 2)
                    self.assertIsNone(child.poll())
                    if target_exits:
                        child.terminate()
                        child.wait(timeout=5)
                    else:
                        observer.terminate()
                    stdout, stderr = observer.communicate(timeout=5)
                    self.assertEqual(observer.returncode, 0, stderr)
                    if not target_exits:
                        self.assertIsNone(child.poll())
                    report = json.loads(report_path.read_text())
                    self.assertTrue(report["complete"])
                    self.assertEqual(report["process_tree"]["target_exited"], target_exits)
                    self.assertGreater(report["process_tree"]["sampled_peak_rss_bytes"], 0)
                    heartbeats = [json.loads(line.removeprefix("native_build_heartbeat="))
                                  for line in stdout.splitlines()]
                    self.assertGreaterEqual(len(heartbeats), 2)
                    self.assertTrue(all(beat["label"] == "formal-required" for beat in heartbeats))
                    self.assertEqual(heartbeats[-1]["complete"], True)
                    self.assertIn("host", heartbeats[-1])
                    self.assertIn("swap_pages", heartbeats[-1])
                    self.assertEqual(heartbeats[-1]["cgroup"], monitor.cgroup_heartbeat(
                        {"cgroup": {"memory_current_bytes": report["cgroup"]["sampled"]["memory_current_bytes"]["end"],
                                    "memory_max_bytes": report["cgroup"]["memory_max_bytes"]["end"],
                                    "memory_peak_since_creation_bytes": report["cgroup"]["memory_peak_since_creation_bytes"]["end"]}},
                        report["cgroup"]["events"]))
                    self.assertNotIn("success", report["process_tree"])
                finally:
                    for process in (observer, child):
                        if process.poll() is None:
                            process.kill()
                        process.communicate(timeout=5)


if __name__ == "__main__":
    unittest.main()
