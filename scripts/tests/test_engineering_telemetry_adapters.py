import hashlib
import importlib.util
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('adapters', ROOT / 'scripts/engineering_telemetry_adapters.py')
a = importlib.util.module_from_spec(spec); spec.loader.exec_module(a)

class AdapterTests(unittest.TestCase):
    def test_actions_does_not_copy_secrets(self):
        raw = b'{"jobs":[{"id":4,"started_at":"2026-01-01T00:00:00Z","completed_at":"2026-01-01T00:01:00Z","conclusion":"failure","name":"secret /home/user"}]}'
        rows, missing, digest = a.project('actions-jobs', raw)
        self.assertEqual(rows[0]['execution_result'], 'failure')
        self.assertNotIn('secret', repr(rows)); self.assertTrue(missing)
        self.assertEqual(digest, hashlib.sha256(raw).hexdigest())

    def test_fractional_timestamp_order_and_unusual_conclusion(self):
        raw = b'{"jobs":[{"id":5,"started_at":"2026-01-01T00:00:00.5Z","completed_at":"2026-01-01T00:00:00Z","conclusion":"stale"}]}'
        rows, missing, _ = a.project("actions-jobs", raw)
        self.assertEqual(rows[0]["execution_result"], "stale")
        self.assertIn("reversed job timestamps", missing)

    def test_process_no_guessed_identity(self):
        rows, missing, _ = a.project('process-jsonl', b'{"test_name":"x", "elapsed_seconds":3, "exit_code":1, "passed":false}\n')
        self.assertEqual(rows[0]['exit_code'], 1); self.assertNotIn('job_id', rows[0]); self.assertTrue(missing)

    def test_schema_and_budget_fail_closed(self):
        with self.assertRaises(ValueError): a.project('build-manifest', b'{"schema_version":3}')
        with self.assertRaises(ValueError): a.project('compile-metrics', b'{}')
        with self.assertRaises(ValueError): a.project('actions-jobs', b' ' * (a.MAX_BYTES + 1))
        with self.assertRaises(ValueError): a.project('unknown', b'{}')
        with self.assertRaises(ValueError): a.project('actions-jobs', b"[" * 2000 + b"]" * 2000)

if __name__ == '__main__': unittest.main()
