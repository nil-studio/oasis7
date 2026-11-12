#!/usr/bin/env python3
"""Read-only production observation through the real upload association reader."""
import copy
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import tempfile
import time
import unittest
from unittest.mock import patch


def load(name, filename):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(filename))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


observation = load("platform_observation", "codeql-platform-observation.py")
fixtures = load("association_fixtures", "codeql-upload-association.test.py")
fixtures.REPO = observation.REPOSITORY
NOW, SHA = fixtures.NOW, fixtures.SHA


class Platform(fixtures.Platform):
    def __init__(self):
        super().__init__()
        self.run.update(status="completed", conclusion="success")
        self.analysis.update(results_count=2, rules_count=37, warning="")
        self.analysis["tool"]["version"] = "2.27.4"
        self.analyses = [self.analysis]
        self.alerts = []
        self.alert_failure = False
        self.alert_pages_full = False
        self.rerun_after_analyses = False
        self.analyses_read = False

    def api(self, command, **kwargs):
        endpoint = command[2]
        route = endpoint.split("?", 1)[0]
        prefix = "repos/" + observation.REPOSITORY
        if route in (prefix + "/code-scanning/analyses", prefix + "/code-scanning/alerts"):
            self.calls.append(command)
            self.assert_read_only(command)
            if route.endswith("/analyses"):
                self.analyses_read = True
                value = self.analyses
            elif self.alert_failure:
                return subprocess.CompletedProcess(command, 1, "", "secret-token-must-not-leak")
            else:
                value = self.alerts * 100 if self.alert_pages_full else self.alerts
            return subprocess.CompletedProcess(command, 0, json.dumps(value), "")
        if (self.rerun_after_analyses and self.analyses_read
                and route == prefix + "/actions/runs/20"):
            self.run["run_attempt"] = 3
        response = super().api(command, **kwargs)
        if route == prefix + "/check-runs/30":
            check = json.loads(response.stdout)
            check["conclusion"] = self.job["conclusion"]
            response.stdout = json.dumps(check)
        return response


class ObservationTests(unittest.TestCase):
    def observe(self, platform):
        with patch("subprocess.run", side_effect=platform.api):
            return observation.observe(observation.target("20", "2", SHA), now=NOW)

    @staticmethod
    def alert(sha=SHA, category=observation.CATEGORY):
        return {"number": 4, "state": "open", "html_url": "https://github.com/alert/4",
                "rule": {"id": "rust/test"}, "tool": {"name": "CodeQL"},
                "most_recent_instance": {"commit_sha": sha, "ref": observation.REF,
                                         "category": category, "state": "open"}}

    def test_exact_completed_attempt_uses_real_reader_and_narrow_sarif_query(self):
        platform = Platform()
        result = self.observe(platform)
        self.assertEqual(result["status"], "observed")
        self.assertEqual(result["target_health"]["upload_status"], "accepted")
        self.assertEqual(result["target_health"]["analysis_id"], 80)
        self.assertEqual(result["analysis_results_count"], 2)
        self.assertEqual(result["analyses"], platform.analyses)
        self.assertEqual(result["findings"]["status"], "unknown")
        self.assertIsNone(result["target_health"]["open_findings"])
        endpoints = [command[2] for command in platform.calls]
        self.assertTrue(any("/check-runs/30" in item for item in endpoints))
        self.assertTrue(any("/actions/artifacts/70/zip" in item for item in endpoints))
        analyses = [item for item in endpoints if "/code-scanning/analyses?" in item]
        self.assertEqual(len(analyses), 1)
        self.assertIn("sarif_id=official-upload-id", analyses[0])
        self.assertIn("ref=refs%2Fheads%2Fmain", analyses[0])
        self.assertIn("tool_name=CodeQL", analyses[0])
        self.assertNotIn("secret-token", json.dumps(result))

    def test_producer_run_and_job_must_be_exact_completed_identity(self):
        cases = (("run", "run_attempt", 3), ("run", "head_sha", "b" * 40),
                 ("run", "head_branch", "other"), ("run", "status", "in_progress"),
                 ("job", "run_attempt", 1), ("job", "head_sha", "b" * 40),
                 ("job", "status", "queued"))
        for owner, field, value in cases:
            with self.subTest(owner=owner, field=field):
                platform = Platform()
                getattr(platform, owner)[field] = value
                result = self.observe(platform)
                self.assertEqual(result["status"], "unknown")
                self.assertTrue(result["errors"])
                self.assertFalse(platform.analyses_read)

    def test_artifact_context_failure_does_not_read_or_borrow_analyses(self):
        platform = Platform()
        platform.record["run_attempt"] = 1
        result = self.observe(platform)
        self.assertEqual(result["status"], "unknown")
        self.assertFalse(platform.analyses_read)
        self.assertTrue(any("artifact record context" in item for item in result["errors"]))

    def test_failed_execution_remains_failed_and_has_no_analysis_association(self):
        platform = Platform()
        platform.job["steps"][0]["conclusion"] = "failure"
        platform.job["steps"][1]["conclusion"] = "skipped"
        result = self.observe(platform)
        self.assertEqual(result["target_health"]["execution_status"], "failure")
        self.assertEqual(result["target_health"]["upload_status"], "skipped")
        self.assertFalse(platform.analyses_read)
        self.assertNotEqual(result["status"], "observed")

    def test_preparation_failure_with_skipped_extraction_and_queries_remains_failed(self):
        platform = Platform()
        platform.run["conclusion"] = platform.job["conclusion"] = "failure"
        platform.job["steps"] = [
            {"name": "Verify the pinned Rust default production bundle", "conclusion": "success"},
            {"name": "Prepare the fixed Rust libraries and current source inventory", "conclusion": "failure"},
            {"name": "CodeQL extraction and queries", "conclusion": "skipped"},
            {"name": "Run and verify all default Rust queries with the fixed libraries", "conclusion": "skipped"},
            {"name": "CodeQL SARIF upload", "conclusion": "skipped"},
        ]
        result = self.observe(platform)
        self.assertEqual(result["status"], "failed")
        self.assertEqual(result["target_health"]["execution_status"], "failure")
        self.assertEqual(result["target_health"]["upload_status"], "skipped")
        self.assertEqual(result["target_health"]["analysis_association"], "unknown")
        self.assertFalse(platform.analyses_read)
        self.assertTrue(any("job execution/upload not successful" in item for item in result["errors"]))

    def test_analysis_missing_duplicate_wrong_sha_and_missing_error_remain_unknown(self):
        for kind in ("missing", "duplicate", "wrong_sha", "missing_error"):
            with self.subTest(kind=kind):
                platform = Platform()
                if kind == "missing":
                    platform.analyses = []
                elif kind == "duplicate":
                    platform.analyses.append(copy.deepcopy(platform.analysis))
                elif kind == "wrong_sha":
                    platform.analysis["commit_sha"] = "b" * 40
                else:
                    del platform.analysis["error"]
                result = self.observe(platform)
                self.assertEqual(result["status"], "unknown")
                self.assertNotEqual(result["target_health"]["upload_status"], "accepted")
                self.assertTrue(result["errors"])

    def test_analysis_errors_and_all_warnings_are_preserved(self):
        platform = Platform()
        platform.analysis.update(error="database processing failed", warning="partial extraction",
                                 errors=["additional processing error"], warnings=["additional warning"])
        result = self.observe(platform)
        self.assertEqual(result["status"], "failed")
        self.assertEqual(result["target_health"]["upload_status"], "failed")
        self.assertEqual(result["analyses"], platform.analyses)
        self.assertIn("partial extraction", json.dumps(result["warnings"]))
        self.assertIn("additional warning", json.dumps(result["warnings"]))
        self.assertIn("additional processing error", json.dumps(result["analysis_errors"]))

    def test_final_attempt_change_revokes_association(self):
        platform = Platform()
        platform.rerun_after_analyses = True
        result = self.observe(platform)
        self.assertEqual(result["status"], "unknown")
        self.assertEqual(result["target_health"]["analysis_association"], "unknown")
        self.assertEqual(result["target_health"]["upload_status"], "unknown")
        self.assertTrue(any("identity" in error for error in result["errors"]))

    def test_findings_are_current_matching_observations_not_new_scan_results(self):
        platform = Platform()
        platform.alerts = [self.alert(), self.alert("b" * 40),
                           self.alert(category="oasis7/rust-repo/extended")]
        platform.alerts[1]["number"] = 5
        platform.alerts[2]["number"] = 6
        result = self.observe(platform)
        self.assertEqual(result["status"], "observed")
        self.assertEqual(result["findings"]["status"], "open")
        self.assertEqual(result["findings"]["alerts_read"], 3)
        self.assertEqual(len(result["findings"]["observed_open_findings"]), 1)
        self.assertFalse(result["findings"]["alert_instances_complete"])
        self.assertEqual(result["analysis_results_count"], 2)

    def test_old_alerts_do_not_become_target_findings_or_zero_findings(self):
        platform = Platform()
        platform.alerts = [self.alert("b" * 40)]
        result = self.observe(platform)
        self.assertEqual(result["findings"]["status"], "unknown")
        self.assertEqual(result["findings"]["observed_open_findings"], [])
        self.assertIsNone(result["target_health"]["open_findings"])

    def test_fixed_instance_is_not_an_open_target_finding_and_missing_state_is_unknown(self):
        for state in ("fixed", None):
            with self.subTest(state=state):
                platform = Platform()
                platform.alerts = [self.alert()]
                platform.alerts[0]["most_recent_instance"]["state"] = state
                result = self.observe(platform)
                self.assertEqual(result["findings"]["status"], "unknown")
                if state is None:
                    self.assertEqual(result["status"], "unknown")
                    self.assertTrue(result["errors"])
                else:
                    self.assertEqual(result["status"], "observed")
                    self.assertEqual(result["findings"]["observed_open_findings"], [])

    def test_unexpected_failure_before_final_identity_read_revokes_association(self):
        platform = Platform()
        original = platform.api

        def unexpected(command, **kwargs):
            if "/code-scanning/alerts?" in command[2]:
                raise AttributeError("unavailable API shape")
            return original(command, **kwargs)

        with patch("subprocess.run", side_effect=unexpected):
            result = observation.observe(observation.target("20", "2", SHA), now=NOW)
        self.assertEqual(result["status"], "unknown")
        self.assertEqual(result["target_health"]["analysis_association"], "unknown")
        self.assertEqual(result["target_health"]["upload_status"], "unknown")

    def test_findings_permission_or_page_limit_failure_is_unknown_not_zero(self):
        for field in ("alert_failure", "alert_pages_full"):
            with self.subTest(field=field):
                platform = Platform()
                platform.alerts = [self.alert()]
                setattr(platform, field, True)
                result = self.observe(platform)
                self.assertEqual(result["status"], "unknown")
                self.assertEqual(result["target_health"]["upload_status"], "accepted")
                self.assertEqual(result["findings"]["status"], "unknown")
                self.assertIsNone(result["findings"]["observed_open_findings"])
                self.assertNotIn("secret-token-must-not-leak", json.dumps(result))
                self.assertTrue(result["errors"])

    def test_actual_read_budget_terminates_owned_child_and_returns_partial_unknown(self):
        def stalled(data):
            data["warnings"].append("observation before timeout")
            subprocess.run(["python3", "-c", "import time; time.sleep(10)"], check=True)

        start = time.monotonic()
        with patch.object(observation, "gather", side_effect=stalled):
            result = observation.observe(observation.target("20", "2", SHA), now=NOW, budget_seconds=0.1)
        self.assertLess(time.monotonic() - start, 2)
        self.assertEqual(result["status"], "unknown")
        self.assertIn("observation before timeout", result["warnings"])
        self.assertTrue(any("budget" in error for error in result["errors"]))


class EntryBoundaryTests(unittest.TestCase):
    def test_target_requires_positive_ids_and_full_lowercase_sha(self):
        for run, attempt, sha in (("0", "1", SHA), ("20;cmd", "1", SHA),
                                  ("20", "-1", SHA), ("20", "2", SHA[:12]),
                                  ("20", "2", SHA.upper())):
            with self.subTest(run=run, attempt=attempt, sha=sha):
                with self.assertRaises(ValueError):
                    observation.target(run, attempt, sha)

    def context(self, directory):
        event = Path(directory) / "event.json"
        event.write_text(json.dumps({"repository": {"full_name": observation.REPOSITORY,
                                                     "default_branch": "main"}}))
        return {"GITHUB_EVENT_NAME": "workflow_dispatch", "GITHUB_REPOSITORY": observation.REPOSITORY,
                "GITHUB_REF": observation.REF, "GITHUB_SHA": SHA,
                "GITHUB_WORKFLOW_REF": observation.REPOSITORY + "/" + observation.WORKFLOW + "@" + observation.REF,
                "GITHUB_WORKFLOW_SHA": SHA, "GITHUB_EVENT_PATH": str(event),
                "GITHUB_RUN_ID": "90", "GITHUB_RUN_ATTEMPT": "1"}

    def test_context_is_own_main_dispatch_and_exact_workflow_checkout(self):
        with tempfile.TemporaryDirectory() as directory:
            environment = self.context(directory)
            with patch("subprocess.run", return_value=subprocess.CompletedProcess([], 0, SHA + "\n", "")):
                result = observation.collector_context(Path(directory), environment)
                self.assertEqual(result["sha"], SHA)
                for key, value in (("GITHUB_EVENT_NAME", "pull_request"),
                                   ("GITHUB_REPOSITORY", "fork/oasis7"),
                                   ("GITHUB_REF", "refs/heads/review"),
                                   ("GITHUB_WORKFLOW_SHA", "b" * 40)):
                    with self.subTest(key=key):
                        with self.assertRaises(ValueError):
                            observation.collector_context(Path(directory), {**environment, key: value})
            with patch("subprocess.run", return_value=subprocess.CompletedProcess([], 0, "b" * 40 + "\n", "")):
                with self.assertRaises(ValueError):
                    observation.collector_context(Path(directory), environment)

    def test_cli_writes_unknown_receipt_on_untrusted_context(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "platform-observation.json"
            with patch.dict(os.environ, {}, clear=True):
                code = observation.main(["--producer-run-id", "20", "--producer-run-attempt", "2",
                                         "--producer-sha", SHA, "--output", str(output)])
            self.assertNotEqual(code, 0)
            result = json.loads(output.read_text())
            self.assertEqual(result["status"], "unknown")
            self.assertTrue(result["errors"])

    def test_workflow_is_read_only_main_dispatch_with_failure_artifact(self):
        root = Path(__file__).resolve().parents[2]
        workflow = (root / observation.WORKFLOW).read_text()
        self.assertIn("workflow_dispatch:", workflow)
        self.assertNotIn("pull_request:", workflow)
        self.assertNotIn("schedule:", workflow)
        self.assertIn("github.ref == 'refs/heads/main'", workflow)
        self.assertIn("github.repository == 'nil-studio/oasis7'", workflow)
        self.assertIn("github.event_name == 'workflow_dispatch'", workflow)
        self.assertIn("timeout-minutes: 7", workflow)
        self.assertIn("checks: read", workflow)
        self.assertIn("security-events: read", workflow)
        self.assertNotIn(": write", workflow)
        self.assertNotIn("upload-sarif", workflow)
        self.assertNotIn("codeql-action", workflow)
        self.assertIn("persist-credentials: false", workflow)
        self.assertIn("ref: ${{ github.sha }}", workflow)
        self.assertIn("if: ${{ always() }}", workflow)
        self.assertIn("run python3 ./scripts/security/codeql-platform-observation.test.py",
                      (root / "scripts/ci-tests.sh").read_text())


if __name__ == "__main__":
    unittest.main()
