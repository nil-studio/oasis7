#!/usr/bin/env python3
"""Exercise the live health reader through authentic API-shaped ZIP responses."""
import base64
import copy
import datetime as dt
import hashlib
import importlib.util
import io
import json
from pathlib import Path
import subprocess
import unittest
from unittest.mock import patch
import zipfile

spec = importlib.util.spec_from_file_location("health", Path(__file__).with_name("codeql-health.py"))
health = importlib.util.module_from_spec(spec)
spec.loader.exec_module(health)
REPO = "fixture/repo"
SHA = "a" * 40
REF = "refs/heads/main"
WORKFLOW = b"trusted reviewed workflow with official SARIF producer\n"
NOW = dt.datetime(2026, 10, 1, tzinfo=dt.timezone.utc)


class Platform:
    def __init__(self):
        self.calls = []
        self.run = {"id": 20, "run_attempt": 2, "head_sha": SHA, "head_branch": "main",
                    "path": ".github/workflows/codeql.yml", "workflow_id": 50,
                    "event": "workflow_dispatch", "check_suite_id": 60,
                    "repository": {"id": 10, "full_name": REPO},
                    "head_repository": {"id": 10, "full_name": REPO},
                    "run_started_at": "2026-09-30T23:00:00Z"}
        self.job = {"id": 30, "run_id": 20, "run_attempt": 2, "head_sha": SHA,
                    "name": "CodeQL / rust-repo / default", "status": "completed",
                    "conclusion": "success", "check_run_url": f"https://api.github.com/repos/{REPO}/check-runs/30",
                    "steps": [{"name": "CodeQL extraction and queries", "conclusion": "success"},
                              {"name": "CodeQL SARIF upload", "conclusion": "success"}]}
        self.record = {"schema": "oasis7-codeql-upload-evidence/v1", "repository": REPO,
                       "workflow_path": ".github/workflows/codeql.yml",
                       "workflow_ref": REPO + "/.github/workflows/codeql.yml@" + REF,
                       "workflow_sha": SHA, "run_id": 20, "run_attempt": 2,
                       "job_key": "analyze", "job_name": self.job["name"],
                       "checkout_sha": SHA, "ref": REF, "unit": "rust-repo",
                       "profile": "default", "category": "oasis7/rust-repo/default",
                       "execution_status": "success", "upload_status": "success",
                       "upload_sarif_id": "official-upload-id"}
        self.analysis = {"id": 80, "sarif_id": "official-upload-id", "ref": REF,
                         "commit_sha": SHA, "category": "oasis7/rust-repo/default",
                         "tool": {"name": "CodeQL"}, "error": "",
                         "created_at": "2026-09-30T23:10:00Z"}
        self.artifact = {"id": 70, "name": "oasis7-codeql-upload-20-2-rust-repo-default",
                         "expired": False, "workflow_run": {"id": 20, "repository_id": 10,
                         "head_repository_id": 10, "head_branch": "main", "head_sha": SHA}}
        self.workflow_drift = False
        self.duplicate_artifact = False
        self.archive_override = None

    def archive(self):
        if self.archive_override is not None:
            return self.archive_override
        output = io.BytesIO()
        with zipfile.ZipFile(output, "w") as archive:
            # One API artifact has stable bytes even across ZIP's two-second clock ticks.
            archive.writestr(zipfile.ZipInfo("evidence.json", NOW.timetuple()[:6]), json.dumps(self.record))
        return output.getvalue()

    def api(self, command, **kwargs):
        self.calls.append(command)
        self.assert_read_only(command)
        endpoint = command[2]
        route = endpoint.split("?", 1)[0]
        prefix = "repos/" + REPO
        archive = self.archive()
        self.artifact.update(size_in_bytes=len(archive), digest="sha256:" + hashlib.sha256(archive).hexdigest())
        if route == prefix + "/code-scanning/analyses":
            value = [self.analysis]
        elif route == prefix + "/code-scanning/alerts":
            value = []
        elif route == prefix + "/actions/workflows/codeql.yml/runs":
            value = {"total_count": 1, "workflow_runs": [self.run]}
        elif route == prefix + "/actions/runs/20/attempts/2/jobs":
            value = {"total_count": 1, "jobs": [self.job]}
        elif route == prefix + "/actions/runs/20/artifacts":
            rows = [self.artifact] * (2 if self.duplicate_artifact else 1)
            value = {"total_count": len(rows), "artifacts": rows}
        elif route == prefix + "/actions/artifacts/70/zip":
            if kwargs.get("stdout") is not None:
                kwargs["stdout"].write(archive)
                return subprocess.CompletedProcess(command, 0, None, None)
            return subprocess.CompletedProcess(command, 0, archive, b"")
        elif route == prefix + "/actions/artifacts/70":
            value = self.artifact
        elif route == prefix + "/actions/runs/20":
            value = self.run
        elif route == prefix + "/actions/workflows/codeql.yml":
            value = {"id": 50, "path": ".github/workflows/codeql.yml", "state": "active"}
        elif route == prefix:
            value = {"id": 10, "full_name": REPO, "default_branch": "main"}
        elif route == "apps/github-actions":
            value = {"id": 40, "slug": "github-actions", "owner": {"login": "github", "type": "Organization"}}
        elif route == prefix + "/check-runs/30":
            value = {"id": 30, "name": self.job["name"], "head_sha": SHA,
                     "app": {"id": 40}, "check_suite": {"id": 60}, "conclusion": "success"}
        elif route == prefix + "/contents/.github/workflows/codeql.yml":
            content = WORKFLOW
            if self.workflow_drift and ("ref=main" in endpoint or "ref=refs%2Fheads%2Fmain" in endpoint):
                content += b"candidate change"
            value = {"type": "file", "encoding": "base64", "content": base64.b64encode(content).decode()}
        else:
            raise AssertionError("unexpected API route: " + endpoint)
        payload = json.dumps(value)
        return subprocess.CompletedProcess(command, 0, payload if kwargs.get("text") else payload.encode(), "" if kwargs.get("text") else b"")

    @staticmethod
    def assert_read_only(command):
        if command[:2] != ["gh", "api"] or "--method" in command or "-X" in command:
            raise AssertionError("unexpected mutation command")


class LiveAssociationTests(unittest.TestCase):
    def test_fixture_artifact_bytes_do_not_change_with_wall_clock(self):
        platform = Platform()
        with patch("zipfile.time.localtime", side_effect=[(2026, 10, 9, 7, 4, 22, 4, 282, 0),
                                                        (2026, 10, 9, 7, 4, 24, 4, 282, 0)]):
            self.assertEqual(platform.archive(), platform.archive())

    def test_malformed_step_entry_is_unknown_diagnostic(self):
        platform = Platform()
        platform.job["steps"] = [None]
        with patch("subprocess.run", side_effect=platform.api):
            data = health.live(REPO, REF, SHA)
        result = health.report(data, NOW)
        self.assertEqual(result["status"], "unknown")
        self.assertTrue(any("step" in error for error in data["errors"]))
        self.assertTrue(all(row["upload_status"] != "accepted" for row in result["units"]))

    def test_malformed_repository_metadata_is_unknown_diagnostic(self):
        platform = Platform()
        platform.run["repository"] = "invalid"
        with patch("subprocess.run", side_effect=platform.api):
            data = health.live(REPO, REF, SHA)
        result = health.report(data, NOW)
        self.assertEqual(result["status"], "unknown")
        self.assertTrue(any("repository" in error for error in data["errors"]))
        self.assertTrue(all(row["upload_status"] != "accepted" for row in result["units"]))

    def observe(self, platform):
        with patch("subprocess.run", side_effect=platform.api):
            data = health.live(REPO, REF, SHA)
        return next(row for row in health.report(data, NOW)["units"]
                    if row["unit"] == "rust-repo" and row["profile"] == "default")

    def test_live_verified_archive_associates_official_upload(self):
        platform = Platform()
        row = self.observe(platform)
        self.assertEqual(row["upload_status"], "accepted")
        self.assertEqual(row["analysis_id"], 80)
        self.assertTrue(any("/actions/artifacts/70/zip" in call[2] for call in platform.calls))

    def test_production_queries_must_succeed_before_association(self):
        for query, expected in (("success", "accepted"), ("failure", "unknown"),
                                ("cancelled", "unknown"), ("skipped", "unknown")):
            with self.subTest(query=query):
                platform = Platform()
                platform.job["steps"].extend([
                    {"name": "Verify the pinned Rust default production bundle", "conclusion": "success"},
                    {"name": "Run and verify all default Rust queries with the fixed libraries", "conclusion": query}])
                # Even a matching success artifact and upload cannot substitute
                # for the independently observed production query outcome.
                row = self.observe(platform)
                self.assertEqual(row["upload_status"], expected)
                if query != "success":
                    self.assertIsNone(row["analysis_id"])

    def test_missing_or_duplicate_production_queries_reject_association(self):
        for count in (0, 2):
            with self.subTest(count=count):
                platform = Platform()
                platform.job["steps"].append({"name": "Verify the pinned Rust default production bundle", "conclusion": "success"})
                platform.job["steps"].extend([
                    {"name": "Run and verify all default Rust queries with the fixed libraries", "conclusion": "success"}
                    for _ in range(count)])
                row = self.observe(platform)
                self.assertEqual(row["upload_status"], "unknown")
                self.assertIsNone(row["analysis_id"])

    def test_failed_production_preparation_cannot_accept_a_success_archive(self):
        platform = Platform()
        platform.job['steps'][0]['conclusion'] = 'skipped'
        platform.job['conclusion'] = 'failure'
        platform.job['steps'].extend([
            {'name': 'Verify the pinned Rust default production bundle', 'conclusion': 'success'},
            {'name': 'Prepare the fixed Rust libraries and current source inventory', 'conclusion': 'failure'},
            {'name': 'Run and verify all default Rust queries with the fixed libraries', 'conclusion': 'skipped'}])
        row = self.observe(platform)
        self.assertEqual(row['execution_status'], 'failure')
        self.assertEqual(row['upload_status'], 'unknown')
        self.assertIsNone(row['analysis_id'])

    def test_old_attempt_archive_never_borrows_current_analysis(self):
        platform = Platform()
        platform.record["run_attempt"] = 1
        self.assertEqual(self.observe(platform)["upload_status"], "unknown")

    def test_reused_old_job_attempt_is_not_rewritten_to_current(self):
        platform = Platform()
        platform.job["run_attempt"] = 1
        self.assertEqual(self.observe(platform)["upload_status"], "unknown")

    def test_old_job_success_is_not_current_attempt_execution(self):
        platform = Platform()
        platform.job["run_attempt"] = 1
        with patch("subprocess.run", side_effect=platform.api):
            data = health.live(REPO, REF, SHA)
        row = next(r for r in health.report(data, NOW)["units"]
                   if r["unit"] == "rust-repo" and r["profile"] == "default")
        self.assertEqual(row["execution_status"], "unknown")
        self.assertEqual(row["upload_step_status"], "unknown")
        self.assertEqual(row["run_attempt"], 2)
        self.assertEqual(row["observed_job_identity"]["run_attempt"], 1)
        self.assertTrue(any("diagnostic job identity" in error for error in data["errors"]))

    def test_matching_candidate_payload_cannot_replace_trusted_workflow(self):
        platform = Platform()
        platform.workflow_drift = True
        self.assertEqual(self.observe(platform)["upload_status"], "unknown")

    def test_duplicate_artifacts_not_first_match(self):
        platform = Platform()
        platform.duplicate_artifact = True
        self.assertEqual(self.observe(platform)["upload_status"], "unknown")

    def test_failed_upload_does_not_borrow_successful_archive(self):
        platform = Platform()
        platform.job["steps"][1]["conclusion"] = "failure"
        self.assertEqual(self.observe(platform)["upload_status"], "failed")

    def test_archive_extra_member_is_rejected(self):
        platform = Platform()
        output = io.BytesIO()
        with zipfile.ZipFile(output, "w") as archive:
            archive.writestr("evidence.json", json.dumps(platform.record))
            archive.writestr("../foreign.json", "{}")
        platform.archive_override = output.getvalue()
        self.assertEqual(self.observe(platform)["upload_status"], "unknown")

    def test_record_identity_fields_are_not_authority(self):
        for key, value in (("repository", "foreign/repo"), ("workflow_sha", "b" * 40),
                           ("workflow_ref", REPO + "/.github/workflows/codeql.yml@refs/heads/other"),
                           ("run_id", True), ("checkout_sha", "b" * 40), ("ref", "refs/heads/other"),
                           ("category", "oasis7/rust-repo/extended"), ("job_name", "CodeQL / plan"),
                           ("upload_status", "failure"), ("execution_status", "failure")):
            with self.subTest(key=key):
                platform = Platform()
                platform.record[key] = value
                self.assertEqual(self.observe(platform)["upload_status"], "unknown")

    def test_pr_or_fork_producer_is_unknown_even_with_matching_record(self):
        for change in ({"event": "pull_request"}, {"head_repository": {"id": 99, "full_name": "foreign/repo"}}):
            platform = Platform()
            platform.run.update(change)
            self.assertEqual(self.observe(platform)["upload_status"], "unknown")

    def test_expired_or_wrong_run_artifact_unknown(self):
        for change in ({"expired": True}, {"workflow_run": {"id": 19}}):
            platform = Platform()
            platform.artifact.update(change)
            self.assertEqual(self.observe(platform)["upload_status"], "unknown")

    def test_strict_archive_json_no_duplicate_keys_or_unknown_fields(self):
        for payload in (json.dumps(Platform().record)[:-1] + ', "run_attempt": 2}',
                        json.dumps(dict(Platform().record, untrusted="extra")), "\ufffd", "{}"):
            platform = Platform()
            output = io.BytesIO()
            with zipfile.ZipFile(output, "w") as archive:
                archive.writestr("evidence.json", payload)
            platform.archive_override = output.getvalue()
            self.assertEqual(self.observe(platform)["upload_status"], "unknown")

    def test_record_over_limit_unknown(self):
        platform = Platform()
        output = io.BytesIO()
        with zipfile.ZipFile(output, "w", compression=zipfile.ZIP_DEFLATED) as archive:
            archive.writestr("evidence.json", b" " * (64 * 1024 + 1))
        platform.archive_override = output.getvalue()
        self.assertEqual(self.observe(platform)["upload_status"], "unknown")

    def test_archive_download_limit_applies_before_decode(self):
        platform = Platform()
        platform.archive_override = b"x" * (1024 * 1024 + 1)
        self.assertEqual(self.observe(platform)["upload_status"], "unknown")

    def test_wrong_provider_or_suite_unknown(self):
        for field, value in (("app", {"id": 99}), ("check_suite", {"id": 99})):
            platform = Platform()
            original = platform.api
            def api(command, **kwargs):
                response = original(command, **kwargs)
                if command[2].split("?", 1)[0].endswith("/check-runs/30"):
                    row = json.loads(response.stdout)
                    row[field] = value
                    response.stdout = json.dumps(row)
                return response
            platform.api = api
            self.assertEqual(self.observe(platform)["upload_status"], "unknown")

    def test_digest_mismatch_unknown(self):
        platform = Platform()
        original = platform.api
        def api(command, **kwargs):
            response = original(command, **kwargs)
            if "/actions/runs/20/artifacts" in command[2]:
                row = json.loads(response.stdout)
                row["artifacts"][0]["digest"] = "sha256:" + "0" * 64
                response.stdout = json.dumps(row)
            return response
        platform.api = api
        self.assertEqual(self.observe(platform)["upload_status"], "unknown")


if __name__ == "__main__":
    unittest.main()
