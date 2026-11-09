#!/usr/bin/env python3
"""Offline scanning-chain contracts; these tests do not start hosted scans."""
import ast
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile
from types import SimpleNamespace
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
             batches=None, forged_rows=None, schedule=None):
        with tempfile.TemporaryDirectory() as directory:
            folder = Path(directory)
            event = {"repository": {"default_branch": "main"}}
            if kind == "pull_request":
                event.update(action=action, pull_request={"number": 9, "draft": draft,
                    "base": {"sha": BASE}, "head": {"sha": HEAD}})
            elif kind == "schedule":
                event["schedule"] = schedule or ("43 18 * * *" if profile == "extended" else "17 */6 * * *")
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

    def test_maintenance_schedules_are_disjoint_and_manual_keeps_all(self):
        for schedule, expected in [('17 */6 * * *', ['actions-repo', 'python-repo', 'javascript-repo']),
                ('29 3 * * *', ['rust-repo']),
                ('43 18 * * *', ['actions-repo', 'python-repo', 'javascript-repo', 'rust-repo'])]:
            with self.subTest(schedule=schedule):
                outputs, _ = self.plan(kind='schedule', schedule=schedule)
                rows = json.loads(outputs['matrix'])['include']
                self.assertEqual([row['unit'] for row in rows], expected)
                self.assertEqual(outputs['profile'], 'extended' if schedule.startswith('43') else 'default')
                for row in rows:
                    expected_budget = 45 if schedule.startswith('43') else (90 if row['language'] == 'rust' else 20)
                    self.assertEqual(row['timeout_minutes'], expected_budget)
        outputs, _ = self.plan(kind='workflow_dispatch')
        self.assertEqual(len(json.loads(outputs['matrix'])['include']), 4)
        with self.assertRaises(SystemExit):
            self.plan(kind='schedule', schedule='0 0 * * *')

    def test_new_scope_argument_does_not_reach_old_pr_base(self):
        _, calls = self.plan(trusted=True)
        self.assertNotIn('--maintenance-scope', calls[-1][0])
        for schedule, expected in [('17 */6 * * *', 'non-rust'), ('29 3 * * *', 'rust'), ('43 18 * * *', 'all')]:
            _, calls = self.plan(trusted=True, kind='schedule', schedule=schedule,
                                 profile='extended' if schedule.startswith('43') else 'default')
            command = calls[-1][0]
            self.assertEqual(command[command.index('--maintenance-scope') + 1], expected)

    def test_planner_cannot_forge_schedule_or_budget(self):
        row = dict(unit='rust-repo', language='rust', slot=1, category='oasis7/rust-repo/default',
                   build_mode='none', queries='', timeout_minutes=90)
        with self.assertRaisesRegex(SystemExit, 'matrix contract'):
            self.plan(trusted=True, kind='schedule', forged_rows=[row])
        row['timeout_minutes'] = 45
        with self.assertRaisesRegex(SystemExit, 'matrix contract'):
            self.plan(trusted=True, kind='schedule', schedule='29 3 * * *', forged_rows=[row])
        row['timeout_minutes'] = 90
        with self.assertRaisesRegex(SystemExit, 'matrix contract'):
            self.plan(trusted=True, forged_rows=[row])

    def test_rust_analysis_budget_reserves_upload_time_from_actual_job_start(self):
        section = WORKFLOW.split('      - id: rust_budget\n', 1)[1].split('      - id: analyze\n', 1)[0]
        self.assertIn("matrix.language == 'rust' && needs.plan.outputs.profile == 'default' && github.event_name != 'pull_request'", section)
        body = section.split("python3 -I - <<'PY'\n", 1)[1].split('          PY', 1)[0]
        body = '\n'.join(line[10:] for line in body.splitlines())
        import datetime
        import email.utils
        from io import BytesIO
        started = datetime.datetime(2026, 11, 9, tzinfo=datetime.timezone.utc)
        for age, expected in [(0, 75), (1200, 59), (4738, 1), (4800, None), (-60, None)]:
            with self.subTest(age=age), tempfile.TemporaryDirectory() as directory:
                folder = Path(directory)
                job = dict(name='CodeQL / rust-repo / default', status='in_progress', id=42,
                           started_at=started.isoformat())
                response = BytesIO(json.dumps({'jobs': [job], 'total_count': 1}).encode())
                response.headers = {'Date': email.utils.format_datetime(started + datetime.timedelta(seconds=age))}
                env = dict(GITHUB_REPOSITORY='example/repo', GITHUB_RUN_ID='23', GITHUB_RUN_ATTEMPT='2',
                           GH_TOKEN='fixture', GITHUB_OUTPUT=str(folder / 'output'),
                           GITHUB_STEP_SUMMARY=str(folder / 'summary'))
                with patch.dict(os.environ, env), patch('urllib.request.urlopen', return_value=response) as request, \
                        patch('time.monotonic', side_effect=[100, 101]):
                    if expected is None:
                        with self.assertRaises(SystemExit):
                            exec(compile(body, 'codeql.yml:rust-budget', 'exec'), {})
                        self.assertFalse((folder / 'output').exists())
                    else:
                        exec(compile(body, 'codeql.yml:rust-budget', 'exec'), {})
                        self.assertEqual((folder / 'output').read_text(), f'analysis_minutes={expected}\n')
                        self.assertIn('600', (folder / 'summary').read_text())
                        self.assertLessEqual(expected * 60 + 600, 5400 - age - 2)
                self.assertIn('/runs/23/attempts/2/jobs', request.call_args.args[0].full_url)
        analyze = WORKFLOW.split('      - id: analyze\n', 1)[1].split('      - id: upload\n', 1)[0]
        self.assertIn('fromJSON(steps.rust_budget.outputs.analysis_minutes)', analyze)

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

    def test_javascript_minified_source_opt_in_is_language_scoped(self):
        scan_env = WORKFLOW.split("    env:\n      RUSTC_WRAPPER:", 1)[1].split("    steps:", 1)[0]
        self.assertIn("CODEQL_EXTRACTOR_JAVASCRIPT_ALLOW_MINIFIED_FILES: ${{ matrix.language == 'javascript-typescript' && 'true' || 'false' }}", scan_env)
        self.assertEqual(WORKFLOW.count("CODEQL_EXTRACTOR_JAVASCRIPT_ALLOW_MINIFIED_FILES:"), 1)
        # Both profiles use this shared scan job; no per-profile omission or new path exclusion.
        self.assertNotIn("needs.plan.outputs.profile", scan_env)
        self.assertNotIn("software_safe_src/**", WORKFLOW)

    def test_shared_queue_empty_guard_and_cancel(self):
        self.assertIn("group: oasis7-codeql-scan-slot-${{ matrix.slot }}\n      queue: max\n      cancel-in-progress: false", WORKFLOW)
        self.assertIn("cancel-in-progress: ${{ github.event_name == 'pull_request' }}", WORKFLOW)
        self.assertIn("github.event.pull_request.number || github.run_id", WORKFLOW)
        self.assertIn("if: needs.plan.outputs.has_units == 'true'", WORKFLOW)
        self.assertIn("fail-fast: false", WORKFLOW)
        self.assertNotRegex(WORKFLOW, r"(?m)^  push:")

    def test_rust_native_preparation_scope_budget_and_artifact(self):
        step = WORKFLOW.split('      - id: rust_native\n', 1)[1].split('      - id: init\n', 1)[0]
        self.assertIn("if: matrix.language == 'rust'", step)
        self.assertIn('timeout-minutes: 5', step)
        self.assertNotIn('needs.plan.outputs.profile', step)
        artifact = WORKFLOW.split('      - name: Persist Rust native preparation evidence\n', 1)[1].split('\n  summary:', 1)[0]
        self.assertIn("if: always() && matrix.language == 'rust' && steps.rust_native.outcome != 'skipped'", artifact)
        self.assertIn('path: ${{ runner.temp }}/codeql-rust-native', artifact)
        self.assertNotIn('codeql-upload-evidence', artifact)
        self.assertIn('if-no-files-found: error', artifact)

    def test_rust_native_wrapper_keeps_preparation_failure_and_evidence(self):
        section = WORKFLOW.split('name: Prepare Rust native extraction prerequisites', 1)[1]
        body = section.split("python3 -I - <<'PY'\n", 1)[1].split('          PY', 1)[0]
        body = '\n'.join(line[10:] for line in body.splitlines())
        for fail in (False, True):
            with self.subTest(fail=fail), tempfile.TemporaryDirectory() as directory:
                folder = Path(directory)
                calls = []
                def prepare(source, out, end, env):
                    calls.append((source, out, end, env))
                    if fail:
                        raise RuntimeError('missing native module')
                native = SimpleNamespace(runtime=SimpleNamespace(), prepare_native=prepare)
                spec = SimpleNamespace(loader=SimpleNamespace(exec_module=lambda module: None))
                env = dict(GITHUB_WORKSPACE=str(ROOT), RUNNER_TEMP=directory,
                           GH_TOKEN='hidden', GITHUB_TOKEN='hidden', RUSTC_WRAPPER='wrapper')
                with patch.dict(os.environ, env), patch.object(sys, 'path', list(sys.path)), \
                        patch('importlib.util.spec_from_file_location', return_value=spec), \
                        patch('importlib.util.module_from_spec', return_value=native), \
                        patch('time.monotonic', side_effect=[100, 103]):
                    if fail:
                        with self.assertRaisesRegex(RuntimeError, 'missing native module'):
                            exec(compile(body, 'codeql.yml:native', 'exec'), {})
                    else:
                        exec(compile(body, 'codeql.yml:native', 'exec'), {})
                self.assertEqual(len(calls), 1)
                source, out, end, child_env = calls[0]
                self.assertEqual((source, out, end), (ROOT, folder / 'codeql-rust-native', 370))
                self.assertFalse({'GH_TOKEN', 'GITHUB_TOKEN', 'RUSTC_WRAPPER'} & child_env.keys())
                self.assertEqual((native.runtime.CAP, native.runtime.WRITE_CAP), (16 * 1024**2, 15 * 1024**2))
                terminal = json.loads((out / 'native-terminal.json').read_text())
                self.assertEqual(terminal['status'], 'FAILED' if fail else 'PASS')
                self.assertEqual(terminal['elapsed_seconds'], 3)
                self.assertEqual(terminal['preparation_budget_seconds'], 180)
                self.assertEqual(terminal.get('error'), 'missing native module' if fail else None)

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
