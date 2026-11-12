#!/usr/bin/env python3
import datetime as dt
import importlib.util
from pathlib import Path
import unittest
import subprocess
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("health", Path(__file__).with_name("codeql-health.py"))
health = importlib.util.module_from_spec(spec)
spec.loader.exec_module(health)
SHA = "a" * 40
NOW = dt.datetime(2026, 10, 1, tzinfo=dt.timezone.utc)


def fixture():
    return {"identity": {"sha": SHA, "ref": "refs/heads/main"}, "analyses": [], "jobs": [], "alerts": []}


def add_analysis(data, unit="rust-repo", profile="default", **extra):
    row = {"category": f"oasis7/{unit}/{profile}", "commit_sha": SHA, "ref": "refs/heads/main",
           "tool": {"name": "CodeQL", "version": "fixture"}, "created_at": "2026-09-30T23:00:00Z", "id": 1}
    row.update(extra)
    data["analyses"].append(row)
    return row


def associated_job(data, **extra):
    data["analyses"][-1]["sarif_id"] = "fixture-sarif"
    row = {"name": "CodeQL / rust-repo / default", "head_sha": SHA, "ref": "refs/heads/main",
           "run_id": 20, "run_attempt": 2, "upload_sarif_id": "fixture-sarif",
           "status": "completed", "conclusion": "success", "started_at": "2026-09-30T22:00:00Z",
           "steps": [{"name": "CodeQL extraction and queries", "conclusion": "success"},
                     {"name": "CodeQL SARIF upload", "conclusion": "success"}]}
    row.update(extra)
    data["jobs"].append(row)
    return row


class HealthTests(unittest.TestCase):
    def rust(self, data):
        return next(u for u in health.report(data, NOW)["units"] if u["unit"] == "rust-repo" and u["profile"] == "default")

    def test_old_accepted_analysis_cannot_mask_latest_failed_upload(self):
        data = fixture()
        for profile in ("default", "extended"):
            for unit in health.UNITS:
                add_analysis(data, unit, profile, created_at="2026-09-30T20:00:00Z")
                data["jobs"].append({"name": f"CodeQL / {unit} / {profile}", "head_sha": SHA,
                    "ref": "refs/heads/main", "run_id": 20, "run_attempt": 2,
                    "started_at": "2026-09-30T23:00:00Z", "status": "completed", "conclusion": "failure",
                    "steps": [{"name": "CodeQL extraction and queries", "conclusion": "success"},
                              {"name": "CodeQL SARIF upload", "conclusion": "failure"}]})
        result = health.report(data, NOW)
        self.assertNotEqual(result["status"], "healthy")
        self.assertTrue(all(u["upload_status"] == "failed" for u in result["units"]))

    def test_unknown_api_errors_are_not_no_findings(self):
        data = fixture()
        data.update(errors=["403"], analyses=None, alerts=None)
        result = health.report(data, NOW)
        self.assertEqual(result["status"], "unknown")
        self.assertTrue(all(u["finding_status"] == "unknown" for u in result["units"]))

    def test_upload_execution_findings_independent(self):
        data = fixture()
        add_analysis(data)
        row = self.rust(data)
        self.assertEqual((row["execution_status"], row["upload_status"], row["finding_status"]), ("unknown", "unknown", "unknown"))
        data["jobs"] = [{"name": "CodeQL / rust-repo / default", "head_sha": SHA, "ref": "refs/heads/main", "status": "completed", "conclusion": "failure",
                         "steps": [{"name": "CodeQL extraction and queries", "conclusion": "failure"}]}]
        self.assertEqual(self.rust(data)["execution_status"], "failure")
        self.assertEqual(self.rust(data)["upload_status"], "unknown")

    def test_production_query_outcome_follows_successful_extraction(self):
        for query, expected in (("success", "success"), ("failure", "failure"),
                                ("cancelled", "cancelled"), ("timed_out", "timed_out"),
                                ("skipped", "skipped"), (None, "unknown")):
            with self.subTest(query=query):
                data = fixture()
                add_analysis(data)
                job = associated_job(data)
                job["steps"].extend([
                    {"name": "Verify the pinned Rust default production bundle", "conclusion": "success"},
                    {"name": "Run and verify all default Rust queries with the fixed libraries", "conclusion": query}])
                self.assertEqual(self.rust(data)["execution_status"], expected)

    def test_production_query_step_missing_or_duplicate_is_unknown(self):
        for count in (0, 2):
            with self.subTest(count=count):
                data = fixture()
                add_analysis(data)
                job = associated_job(data)
                job["steps"].append({"name": "Verify the pinned Rust default production bundle", "conclusion": "success"})
                job["steps"].extend([
                    {"name": "Run and verify all default Rust queries with the fixed libraries", "conclusion": "success"}
                    for _ in range(count)])
                self.assertEqual(self.rust(data)["execution_status"], "unknown")

    def test_skipped_production_steps_preserve_other_scan_paths(self):
        for unit, profile, ref in (("rust-repo", "default", "refs/pull/7/merge"),
                                    ("rust-repo", "extended", "refs/heads/main"),
                                    ("python-repo", "default", "refs/heads/main")):
            with self.subTest(unit=unit, profile=profile, ref=ref):
                data = fixture()
                data["identity"]["ref"] = ref
                add_analysis(data, unit, profile, ref=ref)
                job = associated_job(data, name=f"CodeQL / {unit} / {profile}", ref=ref)
                job["steps"].extend([
                    {"name": "Verify the pinned Rust default production bundle", "conclusion": "skipped"},
                    {"name": "Run and verify all default Rust queries with the fixed libraries", "conclusion": "skipped"}])
                row = next(row for row in health.report(data, NOW)["units"]
                           if row["unit"] == unit and row["profile"] == profile)
                self.assertEqual(row["execution_status"], "success")

    def test_production_extraction_failure_remains_primary(self):
        data = fixture()
        add_analysis(data)
        job = associated_job(data)
        job["steps"][0]["conclusion"] = "failure"
        job["steps"].extend([
            {"name": "Verify the pinned Rust default production bundle", "conclusion": "success"},
            {"name": "Run and verify all default Rust queries with the fixed libraries", "conclusion": "skipped"}])
        self.assertEqual(self.rust(data)["execution_status"], "failure")

    def test_complete_fixture_zero_is_explicit(self):
        data = fixture()
        add_analysis(data)
        associated_job(data)
        data["alert_instances_complete"] = True
        self.assertEqual(self.rust(data)["finding_status"], "no_open_findings")
        data["alerts"] = [{"instances": [{"category": "oasis7/rust-repo/default", "commit_sha": SHA, "ref": "refs/heads/main"}]}]
        self.assertEqual(self.rust(data)["finding_status"], "open")

    def test_exact_identity_and_profile(self):
        for change in ({"commit_sha": "b" * 40}, {"ref": "refs/heads/other"}, {"tool": {"name": "Fake"}}, {"category": "oasis7/rust-repo/extended"}):
            data = fixture()
            add_analysis(data, **change)
            self.assertEqual(self.rust(data)["upload_status"], "unknown")

    def test_age_future_and_coverage_gap(self):
        for date in ("2026-09-28T00:00:00Z", "2026-10-02T00:00:00Z", "bad"):
            data = fixture()
            add_analysis(data, created_at=date)
            self.assertFalse(self.rust(data)["fresh"])
        data = fixture()
        add_analysis(data)
        self.assertEqual(self.rust(data)["coverage_status"], "unknown")
        self.assertIsNone(self.rust(data)["missing_manifests"])

    def test_cancelled_queued_timeout_not_complete(self):
        for state, conclusion, expected in (("queued", None, "queued"), ("completed", "cancelled", "cancelled"), ("completed", "timed_out", "timed_out")):
            data = fixture()
            data["jobs"] = [{"name": "CodeQL / rust-repo / default", "head_sha": SHA, "ref": "refs/heads/main", "status": state, "conclusion": conclusion}]
            row = self.rust(data)
            self.assertEqual(row["execution_status"], expected)
            self.assertEqual(row["upload_status"], expected)

    def test_invalid_identity_no_health(self):
        data = fixture()
        data["identity"]["sha"] = "short"
        self.assertEqual(health.report(data, NOW)["status"], "unknown")

    def test_upload_failure_does_not_relabel_execution(self):
        data = fixture()
        data["jobs"] = [{"name": "CodeQL / rust-repo / default", "head_sha": SHA, "ref": "refs/heads/main", "status": "completed", "conclusion": "failure",
                         "steps": [{"name": "CodeQL extraction and queries", "conclusion": "success"}, {"name": "CodeQL SARIF upload", "conclusion": "failure"}]}]
        self.assertEqual(self.rust(data)["execution_status"], "success")
        self.assertEqual(self.rust(data)["upload_status"], "failed")

    def test_read_pages_fail_closed_on_schema_permission_and_budget(self):
        for output, code in (("{}", 0), ("permission denied", 1), ("broken", 0)):
            with patch("subprocess.run", return_value=subprocess.CompletedProcess([], code, output, "")):
                with self.assertRaises((RuntimeError, health.json.JSONDecodeError)):
                    health.read_pages("fixture/repo", "code-scanning/analyses", limit=1)
        with patch("subprocess.run", return_value=subprocess.CompletedProcess([], 0, health.json.dumps([{}] * 100), "")):
            with self.assertRaisesRegex(RuntimeError, "page budget exceeded"):
                health.read_pages("fixture/repo", "code-scanning/analyses", limit=1)

    def test_live_api_unavailable_is_unknown_without_writes(self):
        with patch("subprocess.run", return_value=subprocess.CompletedProcess([], 1, "", "403")) as command:
            data = health.live("fixture/repo", "refs/heads/main", SHA)
        self.assertEqual(health.report(data, NOW)["status"], "unknown")
        self.assertTrue(all(call.args[0][:2] == ["gh", "api"] and "--method" not in call.args[0] for call in command.call_args_list))

    def test_success_same_identity_timestamp_without_sarif_link_is_unknown(self):
        data = fixture()
        add_analysis(data)
        job = associated_job(data)
        del job["upload_sarif_id"]
        row = self.rust(data)
        self.assertEqual(row["upload_step_status"], "success")
        self.assertEqual(row["upload_status"], "unknown")
        self.assertFalse(row["fresh"])
        self.assertEqual(row["previous_analysis_id"], 1)

    def test_exact_sarif_link_needs_run_and_attempt(self):
        for field in ("run_id", "run_attempt", "upload_sarif_id"):
            data = fixture()
            add_analysis(data)
            job = associated_job(data)
            row = self.rust(data)
            self.assertEqual(row["upload_status"], "accepted")
            self.assertEqual(row["analysis_association"], "sarif_id")
            del job[field]
            self.assertEqual(self.rust(data)["upload_status"], "unknown")

    def test_queued_cancelled_latest_attempt_preserves_old_analysis_separately(self):
        for state, conclusion, expected in (("queued", None, "queued"), ("completed", "cancelled", "cancelled"), ("completed", "timed_out", "timed_out")):
            data = fixture()
            add_analysis(data)
            associated_job(data)
            data["jobs"].append({"name": "CodeQL / rust-repo / default", "head_sha": SHA, "ref": "refs/heads/main",
                "run_id": 20, "run_attempt": 3, "run_started_at": "2026-09-30T23:00:00Z",
                "status": state, "conclusion": conclusion})
            row = self.rust(data)
            self.assertEqual(row["upload_status"], expected)
            self.assertEqual(row["run_attempt"], 3)
            self.assertIsNone(row["analysis_id"])
            self.assertEqual(row["previous_analysis_id"], 1)

    def test_live_reads_exact_run_attempt_endpoint_without_inventing_sarif_link(self):
        run = {"id": 20, "run_attempt": 3, "head_sha": SHA, "head_branch": "main",
               "path": ".github/workflows/codeql.yml", "run_started_at": "2026-09-30T23:00:00Z"}
        job = {"name": "CodeQL / rust-repo / default", "status": "completed", "conclusion": "success",
               "steps": [{"name": "CodeQL SARIF upload", "conclusion": "success"}]}
        with patch.object(health, "read_pages", side_effect=[[], [run], [], [job]]) as pages:
            data = health.live("fixture/repo", "refs/heads/main", SHA)
        self.assertEqual(pages.call_args_list[-1].args[1], "actions/runs/20/attempts/3/jobs")
        self.assertEqual(data["jobs"][0]["run_attempt"], 3)
        self.assertNotIn("upload_sarif_id", data["jobs"][0])


if __name__ == "__main__":
    unittest.main()
