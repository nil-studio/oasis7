#!/usr/bin/env python3
"""Exercise the real baseline script with an isolated Cargo boundary."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parent
NAMES = [
    "governance_apply_with_finality_rejects_stake_root_mismatch",
    "governance_apply_with_finality_rejects_signer_outside_epoch_snapshot",
    "governance_finality_epoch_snapshot_rotation_rejects_stale_signers_and_accepts_rotated_set",
]


class BaselineTest(unittest.TestCase):
    def run_baseline(self, mode="ok", phase="sample", extra=(), repeat_dry_run=False):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            scripts = root / "scripts"
            scripts.mkdir()
            for name in ("oasis7-runtime-finality-baseline.sh",
                         "oasis7-runtime-finality-result.py"):
                if (ROOT / name).exists():
                    shutil.copy(ROOT / name, scripts / name)
            if mode in ("wrong_package", "wrong_filter"):
                script = scripts / "oasis7-runtime-finality-baseline.sh"
                text = script.read_text()
                if mode == "wrong_package":
                    text = text.replace("-p oasis7_runtime", "-p oasis7")
                else:
                    text = text.replace('"$test_identity"\n', '"${test_identity}_missing"\n', 1)
                script.write_text(text)
            (scripts / "cargo-dev-lib.sh").write_text(
                'oasis7_cargo_dev() { python3 "$repo_root/scripts/fake.py" "$@"; }\n'
            )
            (scripts / "fake.py").write_text(FAKE)
            env = dict(os.environ, MODE=mode, PHASE=phase)
            result = subprocess.run([
                "bash", str(scripts / "oasis7-runtime-finality-baseline.sh"),
                "--out-dir", str(root / "out"), "--required-samples", "1",
                "--full-samples", "1", "--warmup-samples", "1", *extra,
            ], env=env, text=True, capture_output=True)
            if repeat_dry_run:
                first_path, = (root / "out").glob("*/summary.json")
                # Simulate the same timestamp to catch stale per-attempt records.
                timestamp = first_path.parent.name.rsplit("-", 1)[0]
                date = scripts / "date"
                date.write_text(f"#!/bin/sh\nprintf '%s\\n' '{timestamp}'\n")
                date.chmod(0o755)
                env["PATH"] = str(scripts) + os.pathsep + env["PATH"]
                result = subprocess.run([
                    "bash", str(scripts / "oasis7-runtime-finality-baseline.sh"),
                    "--out-dir", str(root / "out"), "--dry-run",
                ], env=env, text=True, capture_output=True)
                paths = set((root / "out").glob("*/summary.json")) - {first_path}
                self.assertEqual(len(paths), 1)
                summary_path, = paths
            else:
                summary_path, = (root / "out").glob("*/summary.json")
            summary = json.loads(summary_path.read_text())
            logs = {p.relative_to(summary_path.parent).as_posix(): p.read_text()
                    for p in summary_path.parent.glob("logs/*/*.log")}
            markdown = summary_path.with_suffix(".md").read_text()
            return result, summary, logs, markdown

    def test_all_three_exact_targets_execute_in_both_phases(self):
        result, summary, logs, markdown = self.run_baseline()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(summary["overall_status"], "PASS")
        self.assertEqual(len(logs), 6)
        for row, name in zip(summary["cases"], NAMES):
            self.assertEqual(row["status"], "passed")
            self.assertEqual(row["stats"]["count"], 1)
            self.assertIn("-p oasis7_runtime --lib", row["command"])
            self.assertIn("tests::governance::" + name, row["command"])
            self.assertIn("--exact", row["command"])
            self.assertIn(row["case_id"], markdown)
            self.assertEqual(row["test_package"], "oasis7_runtime")
            self.assertEqual(row["test_target"], "lib")
            self.assertEqual(row["test_identity"], "tests::governance::" + name)
            self.assertEqual(len(row["executions"]), 2)
            self.assertTrue(all(e["verified_test_count"] == 1 for e in row["executions"]))
        for log in logs.values():
            self.assertIn("verified_test_count=1", log)

    def test_invalid_execution_fails_warmup_and_sample(self):
        for phase in ("warmup", "sample"):
            for mode in ("zero", "wrong_package", "wrong_filter", "wrong_identity",
                         "ignored", "failed", "build_failed", "unknown", "duplicate",
                         "missing_result", "exit_failure", "bad_counts"):
                with self.subTest(phase=phase, mode=mode):
                    result, summary, logs, markdown = self.run_baseline(mode, phase)
                    self.assertNotEqual(result.returncode, 0)
                    self.assertEqual(summary["overall_status"], "FAIL")
                    self.assertIn("Overall: FAIL", markdown)
                    for row in summary["cases"]:
                        self.assertEqual(row["status"], "failed")
                        self.assertEqual(row["stats"]["count"], 0)
                        self.assertEqual(row["samples_ms"], [])
                        self.assertIn(phase, row["note"])
                    if phase == "warmup":
                        self.assertFalse(any("sample-" in p for p in logs))

    def test_repeated_timestamp_dry_run_cannot_reuse_execution_proof(self):
        result, summary, logs, _ = self.run_baseline(repeat_dry_run=True)
        self.assertEqual(result.returncode, 0)
        self.assertTrue(summary["dry_run"])
        self.assertEqual(logs, {})
        self.assertTrue(all(row["executions"] == [] for row in summary["cases"]))

    def test_no_execution_modes(self):
        for extra in (("--dry-run",), ("--required-samples", "0", "--skip-full")):
            result, summary, logs, _ = self.run_baseline("build_failed", extra=extra)
            self.assertEqual(result.returncode, 0)
            self.assertEqual(logs, {})
            for row in summary["cases"]:
                self.assertEqual(row["stats"]["count"], 0)
                self.assertIn(row["note"], ("dry_run", "no_samples"))
        result, summary, logs, _ = self.run_baseline(extra=("--skip-full",))
        self.assertEqual(result.returncode, 0)
        self.assertEqual(summary["cases"][2]["status"], "skipped")
        self.assertEqual(len(logs), 4)


FAKE = r'''import os
from pathlib import Path
import sys
args = sys.argv[1:]
mode = original_mode = os.environ["MODE"]
marker = Path("calls-" + args[args.index("--features") + 1] + "-" + next(
    (a for a in args if "governance_" in a), "unknown").split("::")[-1])
phase = "sample" if marker.exists() else "warmup"
marker.touch()
if phase != os.environ["PHASE"]:
    mode = "ok"
identity = next((a for a in args if a.startswith("tests::")), "missing")
if original_mode == "wrong_filter":
    identity = identity.removesuffix("_missing")
feature = "test_tier_full" if "rotation" in identity else "test_tier_required"
expected = ["test", "-p", "oasis7_runtime", "--lib", identity,
            "--features", feature, "--", "--exact", "--format", "pretty",
            "--color", "never"]
if original_mode in ("wrong_package", "wrong_filter"):
    # Restore only the selected phase to the broken command: the other phase
    # simulates the correct target so both failure propagation paths are tested.
    args = expected if phase != os.environ["PHASE"] else args
if args != expected:
    mode = "zero"
if mode == "build_failed":
    print("error: compilation failed")
    sys.exit(101)
if mode == "unknown":
    print("unrecognized output")
    sys.exit(0)
count = 0 if mode == "zero" else 1
print(f"running {count} tests")
if mode == "wrong_identity":
    identity = "tests::governance::unrelated"
status = "ignored" if mode == "ignored" else "FAILED" if mode == "failed" else "ok"
if count:
    print(f"test {identity} ... {status}")
if mode == "duplicate":
    print(f"test {identity} ... ok")
if mode != "missing_result":
    passed = 2 if mode == "bad_counts" else int(count == 1 and status == "ok")
    failed = int(mode == "failed")
    ignored = int(mode == "ignored")
    print(f"test result: {'FAILED' if failed else 'ok'}. {passed} passed; {failed} failed; {ignored} ignored; 0 measured; 123 filtered out; finished in 0.00s")
sys.exit(101 if mode in ("failed", "exit_failure") else 0)
'''

if __name__ == "__main__":
    unittest.main()
