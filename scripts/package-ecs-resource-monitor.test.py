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

SCRIPT = Path(__file__).with_name("package-ecs-resource-monitor.py")
SPEC = importlib.util.spec_from_file_location("monitor", SCRIPT)
monitor = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(monitor)


class MonitorTests(unittest.TestCase):
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
                    _, stderr = process.communicate(timeout=5)
                    self.assertEqual(process.returncode, 0, stderr)
                    report = json.loads(output.read_text())
                    self.assertTrue(report["complete"])
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


if __name__ == "__main__":
    unittest.main()
