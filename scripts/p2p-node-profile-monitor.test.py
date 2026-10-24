#!/usr/bin/env python3
"""Actual entrypoint tests; executable fixtures are explicit test doubles."""
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

SCRIPTS = Path(__file__).resolve().parent
PYTHON = sys.executable


class Collector(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="profile monitor ")
        self.root = Path(self.temp.name)
        self.data = self.root / "data with spaces;literal"
        self.data.mkdir()
        self.out = self.root / "out"
        self.binary = self.root / "fake node;literal"

    def tearDown(self):
        self.temp.cleanup()

    def fake(self, body):
        self.binary.write_text("#!" + PYTHON + "\nimport json, os, sys, time\n" + body + "\n")
        self.binary.chmod(0o755)

    def command(self, *extra):
        return [PYTHON, str(SCRIPTS / "p2p-node-profile-monitor.py"), "--node-binary", str(self.binary),
                "--data-dir", str(self.data), "--out-dir", str(self.out), *extra]

    def run_cli(self, body, *extra):
        self.fake(body)
        return subprocess.run(self.command(*extra), capture_output=True, timeout=10)

    def state(self):
        return json.loads((self.out / "attempt_state.json").read_text())

    def source(self):
        run = self.out / self.state()["latest_attempt"]["run_directory"]
        return json.loads((run / "report/latest_summary.json").read_text())

    def wait_pending(self):
        until = time.monotonic() + 5
        while time.monotonic() < until:
            if (self.out / "attempt_state.json").exists():
                try:
                    if self.state()["latest_attempt"]["result"] == "pending":
                        return
                except (ValueError, KeyError, TypeError):
                    pass
            time.sleep(0.01)
        self.fail("no pending state")

    def test_argument_validation_before_write(self):
        self.fake("print('{}')")
        for args in (("--timeout-ms", "0"), ("--timeout-ms", "1.0"), ("--unknown", "x"), ("--out-dir", str(self.data)), ("--out-dir", str(self.root))):
            self.assertEqual(subprocess.run(self.command(*args), capture_output=True).returncode, 2)
            self.assertFalse(self.out.exists())

    def test_blocked_diagnostic_success_and_exact_argv(self):
        result = self.run_cli("assert sys.argv[1:] == ['node', 'status', '--data-dir', " + repr(str(self.data.resolve())) + "]\nprint(json.dumps({'api_version':'oasis7.local-node-status/v1','runtime_profile':'unsupported'}))")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.state()["latest_attempt"]["result"], "success")
        source = self.source()
        self.assertTrue(source["collection"]["complete"])
        self.assertEqual(source["overall"]["status"], "blocked")
        self.assertNotIn(str(self.data), json.dumps(source))
        self.assertEqual(result.stdout, b"")

    def test_invalid_status_and_exit_failures(self):
        for body, expected in (("print('not-json')", "invalid_status"), ("print('[1]')", "invalid_status"),
                               ("print('{\"api_version\":1,\"api_version\":2}')", "invalid_status"),
                               ("print('{\"api_version\": NaN}')", "invalid_status"),
                               ("os.write(1,b'\\xff')", "invalid_status"),
                               ("print('secret raw'); sys.exit(7)", "command_failed")):
            result = self.run_cli(body)
            self.assertEqual(result.returncode, 1, result.stderr)
            self.assertEqual(self.source()["collection"]["result"], expected)
            self.assertNotIn(b"secret raw", result.stdout + result.stderr)
            self.assertEqual(self.source()["profile_readiness"]["status"], "unknown")

    def test_timeout_and_output_limit(self):
        result = self.run_cli("time.sleep(5)", "--timeout-ms", "100")
        self.assertEqual(result.returncode, 1)
        self.assertEqual(self.source()["collection"]["result"], "timeout")
        result = self.run_cli("os.write(1,b'x'*(4*1024*1024+1))")
        self.assertEqual(result.returncode, 1)
        self.assertEqual(self.source()["collection"]["result"], "output_limit")

    def test_pipe_inheritor_is_bounded_and_killed(self):
        pidfile = self.root / "child.pid"
        body = "pid=os.fork()\nif pid == 0:\n open(" + repr(str(pidfile)) + ", 'w').write(str(os.getpid()))\n time.sleep(5)\nelse:\n sys.exit(0)"
        result = self.run_cli(body, "--timeout-ms", "200")
        self.assertEqual(result.returncode, 1)
        self.assertEqual(self.source()["collection"]["result"], "timeout")
        if pidfile.exists():
            pid = int(pidfile.read_text())
            # A terminated orphan may remain a zombie briefly; it must not be running.
            status = subprocess.run(["ps", "-o", "stat=", "-p", str(pid)], capture_output=True, text=True).stdout.strip()
            self.assertTrue(not status or status.startswith("Z"), status)

    def test_pending_before_probe_and_sigterm_failure(self):
        seen = self.root / "seen"
        self.fake("state=json.load(open(" + repr(str(self.out / 'attempt_state.json')) + "))\nassert state['latest_attempt']['result']=='pending'\nopen(" + repr(str(seen)) + ",'w').write('yes')\ntime.sleep(5)")
        child = subprocess.Popen(self.command(), stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        self.wait_pending()
        until = time.monotonic() + 3
        while not seen.exists() and time.monotonic() < until:
            time.sleep(.01)
        self.assertTrue(seen.exists())
        child.send_signal(signal.SIGTERM)
        child.communicate(timeout=5)
        self.assertEqual(child.returncode, 1)
        self.assertEqual(self.source()["collection"]["result"], "interrupted")

    def test_sigkill_preserves_pending(self):
        pidfile = self.root / "owned.pid"
        self.fake("open(" + repr(str(pidfile)) + ",'w').write(str(os.getpid()))\ntime.sleep(5)")
        child = subprocess.Popen(self.command(), stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        try:
            self.wait_pending()
            until = time.monotonic() + 3
            while not pidfile.exists() and time.monotonic() < until:
                time.sleep(.01)
            child.kill(); child.wait(timeout=5)
            self.assertEqual(self.state()["latest_attempt"]["result"], "pending")
        finally:
            if child.poll() is None:
                child.kill(); child.wait()
            if pidfile.exists():
                try:
                    os.killpg(int(pidfile.read_text()), signal.SIGKILL)
                except ProcessLookupError:
                    pass

    def test_superseded_completion(self):
        marker = self.root / "marker"
        self.fake("first=not os.path.exists(" + repr(str(marker)) + ")\nopen(" + repr(str(marker)) + ",'w').write('x')\nif first: time.sleep(0.8)\nprint(json.dumps({'api_version':'oasis7.local-node-status/v1'}))")
        first = subprocess.Popen(self.command(), stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        self.wait_pending()
        until = time.monotonic() + 3
        while not marker.exists() and time.monotonic() < until:
            time.sleep(.01)
        second = subprocess.run(self.command(), capture_output=True, timeout=5)
        first.communicate(timeout=5)
        self.assertEqual(second.returncode, 0, second.stderr)
        self.assertEqual(first.returncode, 3)
        self.assertEqual(self.state()["latest_attempt"]["sequence"], 2)

    def test_failure_recovery_same_scope(self):
        mode = self.root / "mode"
        self.fake("mode=open(" + repr(str(mode)) + ").read()\nif mode=='fail': sys.exit(1)\nprint(json.dumps({'api_version':'oasis7.local-node-status/v1'}))")
        mode.write_text("ok")
        self.assertEqual(subprocess.run(self.command(), capture_output=True).returncode, 0)
        first_success = self.state()["last_success"]["sequence"]
        mode.write_text("fail")
        self.assertEqual(subprocess.run(self.command(), capture_output=True).returncode, 1)
        self.assertEqual(self.state()["last_success"]["sequence"], first_success)
        mode.write_text("ok")
        self.assertEqual(subprocess.run(self.command(), capture_output=True).returncode, 0)
        self.assertEqual(self.state()["last_success"]["sequence"], 3)

    def test_spawn_failure(self):
        self.binary.write_bytes(b"invalid executable format")
        self.binary.chmod(0o755)
        result = subprocess.run(self.command(), capture_output=True)
        self.assertEqual(result.returncode, 1)
        self.assertEqual(self.source()["collection"]["result"], "spawn_failed")

    def test_corrupt_protocol_rejects_probe(self):
        self.assertEqual(self.run_cli("print(json.dumps({'api_version':'oasis7.local-node-status/v1'}))").returncode, 0)
        (self.out / "attempt_state.json").write_text("broken")
        marker = self.root / "probe"
        self.fake("open(" + repr(str(marker)) + ",'w').write('bad')")
        result = subprocess.run(self.command(), capture_output=True)
        self.assertEqual(result.returncode, 1)
        self.assertFalse(marker.exists())


if __name__ == "__main__":
    unittest.main()
