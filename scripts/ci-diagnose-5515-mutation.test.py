#!/usr/bin/env python3
"""Runner mechanics only; these tests do not claim the Rust experiment passed."""
import importlib.util
from pathlib import Path
import sys
import tempfile
import time
import unittest
from unittest import mock
import hashlib
import json
import os
import signal
import subprocess

spec = importlib.util.spec_from_file_location("mutation", Path(__file__).with_name("ci-diagnose-5515-mutation.py"))
m = importlib.util.module_from_spec(spec)
spec.loader.exec_module(m)


class Contracts(unittest.TestCase):
    def test_mutation_requires_exact_original(self):
        with self.assertRaises(ValueError):
            m.mutate(m.OLD)
        with self.assertRaises(ValueError):
            m.mutate(m.OLD * 2)

    def test_green_requires_one_exact_test(self):
        log = f"running 1 test\ntest {m.TEST} ... ok\ntest result: ok. 1 passed; 0 failed; 0 ignored;".encode()
        self.assertTrue(m.classify(0, log, False))
        self.assertFalse(m.classify(0, log.replace(b"running 1", b"running 0"), False))
        self.assertFalse(m.classify(101, log, False))
        self.assertFalse(m.classify(0, log.replace(m.TEST.encode(), b"other"), False))

    def test_red_rejects_compile_failure_and_wrong_panic(self):
        log = f"running 1 test\ntest {m.TEST} ... FAILED\nfailed-provider charge must consume this call's budget\ntest result: FAILED. 0 passed; 1 failed; 0 ignored;".encode()
        self.assertTrue(m.classify(101, log, True))
        self.assertFalse(m.classify(101, b"could not compile", True))
        self.assertFalse(m.classify(101, log.replace(b"failed-provider charge", b"unrelated"), True))
        self.assertFalse(m.classify(-9, log, True))

    def test_subprocess_deadline(self):
        with tempfile.TemporaryDirectory() as root:
            with self.assertRaises(TimeoutError):
                m.run([sys.executable, "-c", "import time; time.sleep(5)"], root, time.monotonic()+0.15)

    def test_subprocess_output_cap(self):
        with tempfile.TemporaryDirectory() as root:
            with self.assertRaises(ValueError):
                m.run([sys.executable, "-c", f"import sys; sys.stdout.write('x'*{m.CAP+1})"], root, time.monotonic()+5)

    def test_fixed_canonical_target(self):
        self.assertEqual(len(m.SOURCE), 40)
        self.assertTrue(m.TEST.endswith("formal_full_sync_real_provider_charges_survive_failover_and_fresh_call_recovers"))

    def test_failed_phase_restores_and_records_incomplete_experiment(self):
        for failure in [TimeoutError("deadline"), m.Cancelled("cancelled"), (101, b"compile failure")]:
            with self.subTest(failure=type(failure).__name__), tempfile.TemporaryDirectory() as root:
                path = Path(root) / "experiment-source" / m.FILE
                path.parent.mkdir(parents=True)
                original = m.OLD
                path.write_bytes(original)
                green = f"running 1 test\ntest {m.TEST} ... ok\ntest result: ok. 1 passed; 0 failed; 0 ignored;".encode()
                responses = [(0, (m.SOURCE+"\n").encode()), (0, b""),
                             (0, green), failure, (0, b"")]
                with mock.patch.dict(m.os.environ, {"GITHUB_WORKSPACE": root}), \
                     mock.patch.object(m, "FILE_SHA", hashlib.sha256(original).hexdigest()), \
                     mock.patch.object(m, "run", side_effect=responses):
                    with self.assertRaises((m.Cancelled, ValueError, TimeoutError)):
                        m.entrypoint()
                self.assertEqual(path.read_bytes(), original)
                report = json.loads((Path(root)/"mutation-5515-result.json").read_text())
                self.assertTrue(report["restored"])
                self.assertEqual([p["accepted"] for p in report["phases"]], [True, False])

    def test_dirty_checkout_rejected_before_mutation(self):
        with tempfile.TemporaryDirectory() as root:
            (Path(root)/"experiment-source").mkdir()
            with mock.patch.dict(m.os.environ, {"GITHUB_WORKSPACE": root}), \
                 mock.patch.object(m, "run", side_effect=[(0, m.SOURCE.encode()), (0, b" M source")]):
                with self.assertRaisesRegex(ValueError, "dirty"):
                    m.main()

    def test_real_sigterm_reaps_child_and_runs_finalizer(self):
        with tempfile.TemporaryDirectory() as root:
            runner = str(Path(m.__file__).resolve())
            program = '''
import importlib.util, pathlib, sys, time
s=importlib.util.spec_from_file_location("runner", sys.argv[1])
m=importlib.util.module_from_spec(s); s.loader.exec_module(m)
root=pathlib.Path(sys.argv[2])
def main():
    try:
        m.run([sys.executable, "-c", "import os,pathlib,time; pathlib.Path('child.pid').write_text(str(os.getpid())); time.sleep(20)"], root, time.monotonic()+10)
    finally:
        (root/"finalized").write_text("yes")
m.main=main
try:
    m.entrypoint()
except m.Cancelled:
    (root/"cancelled").write_text("yes")
'''
            process = subprocess.Popen([sys.executable, "-c", program, runner, root])
            try:
                ready = Path(root)/"child.pid"
                deadline = time.monotonic()+5
                while not ready.exists() and time.monotonic()<deadline:
                    time.sleep(0.01)
                self.assertTrue(ready.exists())
                child = int(ready.read_text())
                process.send_signal(signal.SIGTERM)
                self.assertEqual(process.wait(timeout=5), 0)
                self.assertEqual((Path(root)/"finalized").read_text(), "yes")
                self.assertEqual((Path(root)/"cancelled").read_text(), "yes")
                with self.assertRaises(ProcessLookupError):
                    os.kill(child, 0)
            finally:
                if process.poll() is None:
                    process.kill()
                process.wait()


if __name__ == "__main__":
    unittest.main()
