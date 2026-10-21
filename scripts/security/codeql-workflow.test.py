#!/usr/bin/env python3
"""Offline scanning-chain contracts; these tests do not start hosted scans."""
import ast
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
WORKFLOW = (ROOT / ".github/workflows/codeql.yml").read_text()
PLAN = WORKFLOW.split("python3 -I - <<'PY'\n", 1)[1].split("          PY", 1)[0]
PLAN = "\n".join(line[10:] for line in PLAN.splitlines())
SHA, BASE, HEAD = "a" * 40, "b" * 40, "c" * 40


class WorkflowTests(unittest.TestCase):
    def plan(self, mode="observe", kind="pull_request", action="synchronize", draft=False,
             profile="default", slots="2", ref=None, trusted=False, planner_failure=False,
             batches=None, forged_rows=None):
        with tempfile.TemporaryDirectory() as directory:
            folder = Path(directory)
            event = {"repository": {"default_branch": "main"}}
            if kind == "pull_request":
                event.update(action=action, pull_request={"number": 9, "draft": draft,
                    "base": {"sha": BASE}, "head": {"sha": HEAD}})
            elif kind == "schedule":
                event["schedule"] = "43 18 * * *" if profile == "extended" else "17 */6 * * *"
            (folder / "event").write_text(json.dumps(event))
            env = {"GITHUB_EVENT_PATH": str(folder / "event"), "GITHUB_EVENT_NAME": kind,
                   "GITHUB_SHA": SHA, "GITHUB_REF": ref or ("refs/pull/9/merge" if kind == "pull_request" else "refs/heads/main"),
                   "RUNNER_TEMP": directory, "GITHUB_OUTPUT": str(folder / "output"),
                   "GITHUB_STEP_SUMMARY": str(folder / "summary"), "CODEQL_MODE": mode,
                   "CODEQL_PROFILE": profile, "CODEQL_SLOTS": slots}
            env["CODEQL_BATCH_MODES"] = batches if batches is not None else json.dumps({
                u + "/" + p: "observe" if p == "default" else "baseline"
                for u in ("actions-repo", "python-repo", "javascript-repo", "rust-repo")
                for p in ("default", "extended")})
            calls = []
            def run(command, **kwargs):
                calls.append((command, kwargs))
                if command[:2] == ["git", "cat-file"]:
                    return subprocess.CompletedProcess(command, 0)
                if command[:2] == ["git", "show"]:
                    return subprocess.CompletedProcess(command, 0 if trusted else 1, b"fixture only", b"")
                if planner_failure:
                    raise subprocess.CalledProcessError(1, command)
                output = Path(command[command.index("--output") + 1])
                output.write_text(json.dumps({"profile": profile, "identity": {"checkout": SHA},
                    "batch_modes": json.loads(env["CODEQL_BATCH_MODES"] or "{}"),
                    "status": "selected" if forged_rows else "not_applicable",
                    "matrix": {"include": forged_rows or []}}))
                return subprocess.CompletedProcess(command, 0)
            with patch.dict(os.environ, env), patch("subprocess.check_output", return_value=SHA + "\n"), patch("subprocess.run", side_effect=run):
                exec(compile(PLAN, "codeql.yml:plan", "exec"), {})
            outputs = dict(line.split("=", 1) for line in (folder / "output").read_text().splitlines())
            return outputs, calls

    def test_embedded_python_compiles(self):
        ast.parse(PLAN)

    def test_first_introduction_conservative_all_four(self):
        outputs, calls = self.plan()
        rows = json.loads(outputs["matrix"])["include"]
        self.assertEqual({r["unit"] for r in rows}, {"actions-repo", "python-repo", "javascript-repo", "rust-repo"})
        self.assertEqual({r["slot"] for r in rows}, {0, 1})
        self.assertEqual(outputs["checkout"], SHA)
        self.assertTrue(all(command[:2] in (["git", "show"], ["git", "cat-file"]) for command, _ in calls))
        self.assertTrue(all(command[2].startswith(BASE + ":") for command, _ in calls if command[:2] == ["git", "show"]))

    def test_disabled_modes_and_lifecycle(self):
        for kwargs in ({"mode": "off"}, {"mode": "baseline"}, {"draft": True}, {"action": "closed"}, {"action": "converted_to_draft"}):
            outputs, calls = self.plan(**kwargs)
            self.assertEqual(outputs["status"], "disabled")
            self.assertEqual(outputs["has_units"], "false")
            self.assertFalse(calls)

    def test_baseline_maintenance_extended_single_slot(self):
        outputs, _ = self.plan(mode="baseline", kind="schedule", profile="extended", slots="1")
        rows = json.loads(outputs["matrix"])["include"]
        self.assertTrue(all(r["slot"] == 0 and r["queries"] == "security-extended" and r["category"].endswith("/extended") for r in rows))

    def test_illegal_configuration_fails(self):
        for kwargs in ({"mode": "invalid"}, {"slots": "3"}, {"profile": "injected", "kind": "workflow_dispatch"}, {"kind": "workflow_dispatch", "ref": "refs/heads/candidate"}):
            with self.assertRaises(SystemExit):
                self.plan(**kwargs)

    def test_trusted_planner_isolated_and_failure_preserved(self):
        outputs, calls = self.plan(trusted=True)
        self.assertEqual(outputs["status"], "not_applicable")
        command, kwargs = calls[-1]
        self.assertEqual(command[:2], ["python3", "-I"])
        self.assertEqual(command[command.index("--checkout") + 1], SHA)
        self.assertEqual(command[command.index("--head") + 1], HEAD)
        self.assertNotEqual(kwargs["cwd"], str(ROOT))
        with self.assertRaises(subprocess.CalledProcessError):
            self.plan(trusted=True, planner_failure=True)

    def test_empty_batches_never_bootstrap_scans(self):
        for kind in ("pull_request", "workflow_dispatch", "schedule"):
            outputs, _ = self.plan(kind=kind, batches="{}")
            self.assertEqual(outputs["status"], "disabled")
            self.assertEqual(json.loads(outputs["matrix"])["include"], [])
        self.assertIn("CODEQL_BATCH_MODES: ${{ vars.OASIS7_CODEQL_BATCH_MODES || '{}' }}", WORKFLOW)

    def test_single_baseline_trial_cannot_enable_pr(self):
        config = json.dumps({"actions-repo/default": "baseline"})
        outputs, _ = self.plan(batches=config)
        self.assertEqual(outputs["has_units"], "false")
        outputs, _ = self.plan(mode="baseline", kind="workflow_dispatch", batches=config)
        self.assertEqual([r["unit"] for r in json.loads(outputs["matrix"])["include"]], ["actions-repo"])
        outputs, _ = self.plan(mode="off", kind="workflow_dispatch", batches=config)
        self.assertEqual(outputs["has_units"], "false")

    def test_invalid_batches_and_forged_rows_rejected(self):
        for config in ("[]", "null", "{", '{"actions-repo/default":"observe","actions-repo/default":"baseline"}',
                       '{"actions-repo/extended":"observe"}', '{"missing/default":"baseline"}',
                       '{"actions-repo/default":true}'):
            with self.subTest(config=config), self.assertRaises((SystemExit, ValueError)):
                self.plan(batches=config)
        forged = [dict(unit="rust-repo", language="rust", slot=0, category="oasis7/rust-repo/default",
                       build_mode="none", queries="")]
        with self.assertRaises(SystemExit):
            self.plan(trusted=True, batches="{}", forged_rows=forged)
        with self.assertRaises(SystemExit):
            self.plan(trusted=True, batches='{"rust-repo/default":"baseline"}', forged_rows=forged)

    def test_permissions_pins_and_no_business_chain(self):
        uses = re.findall(r"uses: ([^\s]+)", WORKFLOW)
        self.assertTrue(uses)
        self.assertTrue(all(re.fullmatch(r"(?:actions/(?:checkout|upload-artifact)|github/codeql-action/(?:init|analyze|upload-sarif))@[0-9a-f]{40}", action) for action in uses))
        self.assertEqual(WORKFLOW.count("persist-credentials: false"), 2)
        for forbidden in ("pull_request_target", "continue-on-error", "secrets.", "secrets: inherit", "cargo build", "cargo test", "npm install", "npm build", "workflow_run", "environment:"):
            self.assertNotIn(forbidden, WORKFLOW)
        self.assertIn("security-events: write", WORKFLOW)
        self.assertIn("upload: never", WORKFLOW)
        self.assertIn("wait-for-processing: true", WORKFLOW)

    def test_shared_queue_empty_guard_and_cancel(self):
        self.assertIn("group: oasis7-codeql-scan-slot-${{ matrix.slot }}\n      queue: max\n      cancel-in-progress: false", WORKFLOW)
        self.assertIn("cancel-in-progress: ${{ github.event_name == 'pull_request' }}", WORKFLOW)
        self.assertIn("github.event.pull_request.number || github.run_id", WORKFLOW)
        self.assertIn("if: needs.plan.outputs.has_units == 'true'", WORKFLOW)
        self.assertIn("fail-fast: false", WORKFLOW)
        self.assertNotRegex(WORKFLOW, r"(?m)^  push:")

    def test_upload_evidence_actual_producer_and_failure_identity(self):
        # Execute the actual workflow producer, rather than a mirror serializer.
        section = WORKFLOW.split("name: Record upload association evidence", 1)
        self.assertEqual(len(section), 2, "official upload output is not persisted")
        body = section[1].split("python3 -I - <<'PY'\n", 1)[1].split("          PY", 1)[0]
        body = "\n".join(line[10:] for line in body.splitlines())
        expected = {"schema", "repository", "workflow_path", "workflow_ref", "workflow_sha",
                    "run_id", "run_attempt", "job_key", "job_name", "checkout_sha", "ref",
                    "unit", "profile", "category", "execution_status", "upload_status", "upload_sarif_id"}
        with tempfile.TemporaryDirectory() as folder:
            env = {"RUNNER_TEMP": folder, "GITHUB_REPOSITORY": "example/repo",
                   "GITHUB_RUN_ID": "23", "GITHUB_RUN_ATTEMPT": "2", "GITHUB_JOB": "analyze",
                   "GITHUB_WORKFLOW_REF": "example/repo/.github/workflows/codeql.yml@refs/heads/main",
                   "GITHUB_WORKFLOW_SHA": SHA, "GITHUB_REF": "refs/heads/main",
                   "UNIT": "python-repo", "PROFILE": "extended", "CATEGORY": "oasis7/python-repo/extended",
                   "CHECKOUT": SHA, "EXECUTION": "success", "UPLOAD": "success", "SARIF_ID": "official-upload-id"}
            for status in ("success", "failure", "skipped", "cancelled"):
                env["UPLOAD"] = status
                with patch.dict(os.environ, env):
                    exec(compile(body, "codeql.yml:evidence", "exec"), {})
                value = json.loads((Path(folder) / "codeql-upload-evidence/evidence.json").read_text())
                self.assertEqual(set(value), expected)
                self.assertEqual(value["schema"], "oasis7-codeql-upload-evidence/v1")
                self.assertEqual(value["run_id"], 23)
                self.assertEqual(value["run_attempt"], 2)
                self.assertEqual(value["job_key"], "analyze")
                self.assertEqual(value["job_name"], "CodeQL / python-repo / extended")
                self.assertEqual(value["workflow_sha"], SHA)
                self.assertEqual(value["checkout_sha"], SHA)
                self.assertEqual(value["upload_status"], status)
                self.assertEqual(value["upload_sarif_id"], "official-upload-id" if status == "success" else "")
        self.assertIn("SARIF_ID: ${{ steps.upload.outputs.sarif-id }}", section[1])
        self.assertRegex(section[1], r"uses: actions/upload-artifact@[0-9a-f]{40}(?:\s|$)")
        self.assertIn("name: oasis7-codeql-upload-${{ github.run_id }}-${{ github.run_attempt }}-${{ matrix.unit }}-${{ needs.plan.outputs.profile }}", section[1])
        self.assertIn("path: ${{ runner.temp }}/codeql-upload-evidence/evidence.json", section[1])
        self.assertIn("if-no-files-found: error", section[1])
        self.assertGreaterEqual(section[1].count("if: always()"), 2)


if __name__ == "__main__":
    unittest.main()
