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
                           GITHUB_STEP_SUMMARY=str(folder / 'summary'), RUNNER_TEMP=directory,
                           EXPECTED_CHECKOUT=SHA)
                with patch.dict(os.environ, env), patch('urllib.request.urlopen', return_value=response) as request, \
                        patch('time.monotonic', side_effect=[100, 101]), patch('time.time', return_value=20000):
                    if expected is None:
                        with self.assertRaises(SystemExit):
                            exec(compile(body, 'codeql.yml:rust-budget', 'exec'), {})
                        self.assertFalse((folder / 'output').exists())
                    else:
                        exec(compile(body, 'codeql.yml:rust-budget', 'exec'), {})
                        output = dict(line.split('=', 1) for line in (folder / 'output').read_text().splitlines())
                        self.assertEqual(int(output['analysis_minutes']), expected)
                        self.assertEqual(float(output['analysis_deadline']), 20000 + expected * 60)
                        receipt = json.loads((folder / 'codeql-rust-production-budget.json').read_text())
                        self.assertEqual(receipt['source_sha'], SHA)
                        self.assertEqual(receipt['mode'], 'rust-default-production')
                        self.assertEqual(receipt['analysis_allocated_at'], 20000)
                        self.assertEqual(receipt['fixed_job_deadline'], 20000 + 5400 - age - 2)
                        self.assertIn('600', (folder / 'summary').read_text())
                        self.assertLessEqual(expected * 60 + 600, 5400 - age - 2)
                self.assertIn('/runs/23/attempts/2/jobs', request.call_args.args[0].full_url)
        analyze = WORKFLOW.split('      - id: analyze\n', 1)[1].split('      - id: upload\n', 1)[0]
        self.assertIn('fromJSON(steps.rust_prepare.outputs.analysis_minutes)', analyze)
        self.assertNotIn('time.time() +', analyze)
        self.assertIn('NODE_OPTIONS: ${{ steps.rust_prepare.outputs.node_options }}', analyze)
        self.assertIn("OASIS7_CODEQL_PRODUCTION_MODE: ${{ steps.rust_tools.outcome == 'success' && 'rust-default-production' || '' }}", analyze)
        for phase in ('rust_prepare', 'rust_production'):
            step = WORKFLOW.split('      - id: ' + phase + '\n', 1)[1].split('      - id:', 1)[0]
            self.assertIn('RUST_ANALYSIS_DEADLINE: ${{ steps.rust_budget.outputs.analysis_deadline }}', step)
            self.assertIn('--analysis-deadline "$RUST_ANALYSIS_DEADLINE"', step)

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
        local_linker = "./.github/actions/setup-linux-mold"
        self.assertTrue(all(action == local_linker or re.fullmatch(r"(?:actions/(?:checkout|upload-artifact)|github/codeql-action/(?:init|analyze|upload-sarif))@[0-9a-f]{40}", action) for action in uses))
        self.assertEqual(uses.count(local_linker), 1)
        self.assertEqual(WORKFLOW.count("persist-credentials: false"), 3)
        # The local action is bound to the executing workflow revision, even
        # when the subsequent analysis checkout is historical product source.
        steps = re.findall(r'^      - .*?(?=^      - |\Z)', WORKFLOW.split('  analyze:', 1)[1], re.M | re.S)
        setup = next(i for i, step in enumerate(steps) if 'uses: ' + local_linker in step)
        self.assertIn('ref: ${{ github.workflow_sha }}', steps[setup - 1])
        self.assertIn('sparse-checkout: .github/actions/setup-linux-mold\n', steps[setup - 1])
        self.assertNotIn('sparse-checkout-cone-mode: false', steps[setup - 1])
        for step in steps[setup - 1:setup + 1]:
            self.assertIn("if: matrix.language == 'rust'", step)
        self.assertIn('ref: ${{ needs.plan.outputs.checkout }}', steps[setup + 1])
        self.assertNotIn('sparse-checkout:', steps[setup + 1])
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
        self.assertIn("if: github.event_name != 'pull_request' || github.event.action != 'closed'", WORKFLOW)
        self.assertIn("if: always() && (github.event_name != 'pull_request' || github.event.action != 'closed')", WORKFLOW)
        self.assertIn("if: needs.plan.outputs.has_units == 'true'", WORKFLOW)
        self.assertIn("fail-fast: false", WORKFLOW)
        self.assertNotRegex(WORKFLOW, r"(?m)^  push:")

    def test_rust_native_preparation_scope_budget_and_artifact(self):
        step = WORKFLOW.split('      - id: rust_native\n', 1)[1].split('      - id:', 1)[0]
        self.assertIn("if: matrix.language == 'rust'", step)
        self.assertIn('timeout-minutes: 5', step)
        self.assertIn("CODEQL_NATIVE_SOURCE_PROFILE: ${{ needs.plan.outputs.profile == 'default' && github.event_name != 'pull_request' && 'ubuntu-archive-https' || 'runner-sources' }}", step)
        self.assertNotIn('Debug::pkgAcquire::Worker', step)
        self.assertNotIn('NativeObservation', step)
        artifact = WORKFLOW.split('      - name: Persist Rust native preparation evidence\n', 1)[1].split('\n  summary:', 1)[0]
        self.assertIn("if: always() && matrix.language == 'rust' && steps.rust_native.outcome != 'skipped'", artifact)
        self.assertIn('path: ${{ runner.temp }}/codeql-rust-native', artifact)
        self.assertNotIn('codeql-upload-evidence', artifact)
        self.assertIn('if-no-files-found: error', artifact)

    def test_rust_native_wrapper_keeps_preparation_failure_and_evidence(self):
        section = WORKFLOW.split('name: Prepare Rust native extraction prerequisites', 1)[1]
        body = section.split("python3 -I - <<'PY'\n", 1)[1].split('          PY', 1)[0]
        body = '\n'.join(line[10:] for line in body.splitlines())
        class PhaseError(RuntimeError):
            def __init__(self):
                super().__init__('private-apt-error-canary')
                self.result = {'exit':124, 'failure_kind':'child_exit'}
                self.native_source_evidence_error = {'stage':'final-source-receipt', 'type':'OSError'}
        for profile, failure in ((profile, failure) for profile in ('runner-sources', 'ubuntu-archive-https')
                                 for failure in (None, RuntimeError, PhaseError)):
            with self.subTest(profile=profile, failure=failure), tempfile.TemporaryDirectory() as directory:
                folder = Path(directory)
                calls = []
                error = None if failure is None else (PhaseError() if failure is PhaseError else RuntimeError('private-source-error-canary'))
                def prepare(source, out, end, env, **kwargs):
                    calls.append((source, out, end, env, kwargs))
                    if error is not None: raise error
                native = SimpleNamespace(runtime=SimpleNamespace(PhaseError=PhaseError), prepare_native=prepare)
                spec = SimpleNamespace(loader=SimpleNamespace(exec_module=lambda module: None))
                env = dict(GITHUB_WORKSPACE=str(ROOT), RUNNER_TEMP=directory,
                           GH_TOKEN='hidden', GITHUB_TOKEN='hidden', RUSTC_WRAPPER='wrapper',
                           CODEQL_NATIVE_SOURCE_PROFILE=profile)
                with patch.dict(os.environ, env), patch.object(sys, 'path', list(sys.path)), \
                        patch('importlib.util.spec_from_file_location', return_value=spec), \
                        patch('importlib.util.module_from_spec', return_value=native), \
                        patch('time.monotonic', side_effect=[100, 103]):
                    if error is not None:
                        with self.assertRaises(type(error)) as caught:
                            exec(compile(body, 'codeql.yml:native', 'exec'), {})
                        self.assertIs(caught.exception, error)
                    else:
                        exec(compile(body, 'codeql.yml:native', 'exec'), {})
                self.assertEqual(len(calls), 1)
                source, out, end, child_env, kwargs = calls[0]
                self.assertEqual((source, out, end), (ROOT, folder / 'codeql-rust-native', 370))
                self.assertEqual(kwargs, {'source_profile':profile})
                self.assertFalse({'GH_TOKEN', 'GITHUB_TOKEN', 'RUSTC_WRAPPER'} & child_env.keys())
                self.assertEqual((native.runtime.CAP, native.runtime.WRITE_CAP), (16 * 1024**2, 15 * 1024**2))
                terminal = json.loads((out / 'native-terminal.json').read_text())
                self.assertEqual(terminal['status'], 'FAILED' if error else 'PASS')
                self.assertEqual(terminal['elapsed_seconds'], 3)
                self.assertEqual(terminal['preparation_budget_seconds'], 180)
                self.assertEqual(terminal['source_profile'], profile)
                self.assertEqual(terminal.get('error_type'), None if failure is None else failure.__name__)
                self.assertNotIn('error', terminal)
                self.assertNotIn('canary', json.dumps(terminal))
                if failure is PhaseError:
                    self.assertEqual(terminal['exit'], 124)
                    self.assertEqual(terminal['failure_kind'], 'child_exit')
                    self.assertEqual(terminal['source_evidence_error'], error.native_source_evidence_error)

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

    def test_rust_extraction_success_requires_query_success_for_execution(self):
        section = WORKFLOW.split('      - id: execution\n', 1)
        self.assertEqual(len(section), 2, 'production query outcome must reach the upload evidence')
        body = section[1].split("python3 -I - <<'PY'\n", 1)[1].split('          PY', 1)[0]
        body = '\n'.join(line[10:] for line in body.splitlines())
        for preparation, extraction, queries, rust, expected in (
                ('success', 'success', 'success', 'true', 'success'),
                ('success', 'success', 'failure', 'true', 'failure'),
                ('success', 'success', 'cancelled', 'true', 'cancelled'),
                ('success', 'success', 'skipped', 'true', 'skipped'),
                ('success', 'failure', 'success', 'true', 'failure'),
                ('success', 'cancelled', 'failure', 'true', 'cancelled'),
                ('failure', 'skipped', 'skipped', 'true', 'failure'),
                ('cancelled', 'skipped', 'skipped', 'true', 'cancelled'),
                ('skipped', 'success', 'skipped', 'false', 'success')):
            with self.subTest(preparation=preparation, extraction=extraction, queries=queries, rust=rust), tempfile.TemporaryDirectory() as folder:
                output = Path(folder) / 'output'
                with patch.dict(os.environ, dict(RUST_PREPARATION=preparation,
                                                EXTRACTION=extraction, RUST_QUERIES=queries,
                                                PATCHED_RUST=rust, GITHUB_OUTPUT=str(output))):
                    exec(compile(body, 'codeql.yml:execution', 'exec'), {})
                self.assertEqual(output.read_text(), 'status=' + expected + '\n')
        self.assertEqual(WORKFLOW.count('EXECUTION: ${{ steps.execution.outputs.status }}'), 2)
        self.assertIn('RUST_PREPARATION: ${{ steps.rust_prepare.outcome }}', WORKFLOW)

    def test_patched_rust_is_main_default_only_and_keeps_upload_identity(self):
        section = WORKFLOW.split('      - id: rust_tools\n', 1)
        self.assertEqual(len(section), 2, 'pinned production tool preparation is missing')
        tools_step = section[1].split('      - id: init\n', 1)[0]
        self.assertIn("if: matrix.language == 'rust' && needs.plan.outputs.profile == 'default' && github.event_name != 'pull_request'", tools_step)
        self.assertIn('codeql-bundle-v2.27.4/codeql-bundle-rust-linux64.tar.zst', tools_step)
        self.assertIn('1c2f867094b80e43c9a0e511090e57e0ea3bf107600b4b3763c52b9020b4ec4b', tools_step)
        self.assertIn('sha256sum --check', tools_step)
        analyze = WORKFLOW.split('      - id: analyze\n', 1)[1].split('      - id: rust_production\n', 1)[0]
        self.assertIn("skip-queries: ${{ steps.rust_tools.outcome == 'success' }}", analyze)
        self.assertIn("upload-database: ${{ steps.rust_tools.outcome == 'success' && 'false' || 'true' }}", analyze)
        upload = WORKFLOW.split('      - id: upload\n', 1)[1].split('      - name:', 1)[0]
        self.assertIn("if: success() && steps.execution.outputs.status == 'success'", upload)
        self.assertIn('category: ${{ matrix.category }}', upload)
        self.assertIn('sha: ${{ needs.plan.outputs.checkout }}', upload)
        artifact = WORKFLOW.split('      - name: Persist Rust production evidence\n', 1)[1].split('\n  summary:', 1)[0]
        self.assertIn('path: ${{ runner.temp }}/codeql-rust-production-evidence', artifact)
        self.assertNotIn('codeql_databases', artifact)
        self.assertNotIn('codeql-upload-evidence', artifact)


class FixedSourceWorkflowTests(unittest.TestCase):
    def setUp(self):
        self.workflow = (ROOT / '.github/workflows/codeql-rust-capacity.yml').read_text()
        self.text = '  observe:\n' + self.workflow.split('  observe:\n', 1)[1]

    def section(self, text, start, end):
        return text.split(start, 1)[1].split(end, 1)[0]

    def python_body(self, section):
        body = section.split("python3 -I - <<'PY'\n", 1)[1].split('          PY', 1)[0]
        return '\n'.join(line[10:] for line in body.splitlines())

    def test_closed_manual_workflow_keeps_source_tool_and_upload_boundaries(self):
        text = self.text
        dispatch = self.section(self.workflow, '  workflow_dispatch:\n', '\npermissions:')
        self.assertEqual(re.findall(r'^      (\w+):', dispatch, re.M), ['execute', 'compare_qualifier_patch', 'mode'])
        self.assertIn('type: boolean\n        default: false', dispatch)
        self.assertIn('default: capacity', dispatch)
        self.assertIn('          - rust-default-fixed-source-observation', dispatch)
        self.assertIn('          - rust-default-negative-projection-observation', dispatch)
        self.assertIn("if: github.event_name == 'workflow_dispatch' && github.ref == 'refs/heads/main' && inputs.execute == true && (inputs.mode == 'rust-default-fixed-source-observation' || inputs.mode == 'rust-default-negative-projection-observation')", text)
        self.assertIn("if: github.event_name == 'workflow_dispatch' && inputs.execute == true && inputs.mode != 'rust-default-fixed-source-observation' && inputs.mode != 'rust-default-negative-projection-observation'",
                      self.workflow.split('  observe:\n', 1)[0])
        self.assertIn('OASIS7_CODEQL_PRODUCTION_MODE: ${{ inputs.mode }}', text)
        self.assertIn('if [ "$COMPARE_QUALIFIER_PATCH" != false ]; then', text)
        self.assertIn('timeout-minutes: 90', text)
        self.assertIn('group: oasis7-codeql-scan-slot-1\n      queue: max\n      cancel-in-progress: false', text)
        self.assertIn('ref: ${{ github.sha }}', text)
        self.assertIn('ref: ${{ env.OASIS7_CODEQL_PRODUCTION_SOURCE }}', text)
        self.assertIn('OASIS7_CODEQL_PRODUCTION_SOURCE: 2bcc4edafdc615f395c9138a0b39ede366726d34', text)
        self.assertNotRegex(text, r'(?m)^\s+GITHUB_SHA:', 'the real event SHA must not be overwritten')
        self.assertEqual(text.count('persist-credentials: false'), 2)
        self.assertEqual(text.count('--production-default'), 2)
        self.assertEqual(text.count('--mode "$OASIS7_CODEQL_PRODUCTION_MODE"'), 2)
        self.assertEqual(text.count('$RUNNER_TEMP/codeql-rust-fixed-source-helper/codeql-rust-production.py'), 2)
        self.assertIn('NODE_OPTIONS: ${{ steps.rust_prepare.outputs.node_options }}', text)
        self.assertIn("if: always() && steps.rust_prepare.outcome == 'success'", text)
        for forbidden in ('security-events: write', 'upload-sarif@', 'codeql-platform-observation',
                          'CODEQL_BATCH_MODES', 'repository-dispatch', 'candidate-workload-research'):
            self.assertNotIn(forbidden, text)
        self.assertIn('upload: never\n          skip-queries: true\n          upload-database: false', text)
        artifact = text.split('      - name: Persist bounded observation evidence\n', 1)[1]
        for forbidden in ('codeql_databases', 'codeql-results', 'fixed-source-helper', 'github.workspace', '*.sarif'):
            self.assertNotIn(forbidden, artifact)
        self.assertIn('codeql-rust-production-evidence', artifact)
        scope = json.loads((ROOT / 'scripts/ci-required-scope.json').read_text())
        self.assertEqual(scope['scoped_control_paths']['.github/workflows/codeql-rust-capacity.yml'], ['workflow_governance'])

    def test_original_init_selection_resources_and_budget_allocation_are_preserved(self):
        text = self.text
        init = self.section(text, '      - id: init\n', '      - id: rust_budget\n')
        self.assertIn('ram: 14570\n          threads: 4', init)
        self.assertIn('dependency-caching: false\n          trap-caching: false', init)
        self.assertIn('CODEQL_OVERLAY_DATABASE_MODE: none', init)
        self.assertIn('codeql-bundle-v2.27.4/codeql-bundle-rust-linux64.tar.zst', text)
        self.assertIn('1c2f867094b80e43c9a0e511090e57e0ea3bf107600b4b3763c52b9020b4ec4b', text)
        self.assertEqual(re.findall(r'uses: github/codeql-action/(?:init|analyze)@([0-9a-f]+)', text),
                         ['33d7b85da1a04171e211befe2c739303b9a4c066'] * 2)
        original_init = self.section(WORKFLOW, '      - id: init\n', '      - name: Verify Actions query')
        self.assertEqual(re.findall(r'^              - (.+)$', init, re.M),
                         re.findall(r'^              - (.+)$', original_init, re.M))
        original = self.python_body(self.section(WORKFLOW, '      - id: rust_budget\n', '      - id: rust_prepare\n'))
        observed = self.python_body(self.section(text, '      - id: rust_budget\n', '      - id: rust_prepare\n'))
        normalized = observed.replace('CodeQL Rust fixed source observation', 'CodeQL / rust-repo / default')
        normalized = normalized.replace("mode=os.environ['OASIS7_CODEQL_PRODUCTION_MODE']", "mode='rust-default-production'")
        normalized = normalized.replace("source_sha=os.environ['OASIS7_CODEQL_PRODUCTION_SOURCE'], tool_sha=os.environ['GITHUB_SHA'],",
                                        "source_sha=os.environ['EXPECTED_CHECKOUT'],")
        # The existing real-clock fixture tests the same AST, including late/expired starts.
        self.assertEqual(ast.dump(ast.parse(normalized)), ast.dump(ast.parse(original)))
        native = self.python_body(self.section(text, '      - id: rust_native\n', '      - id: rust_tools\n'))
        original_native = self.python_body(self.section(WORKFLOW, '      - id: rust_native\n', '      - id: rust_tools\n'))
        native = native.replace("pathlib.Path(os.environ['RUNNER_TEMP']) / 'codeql-rust-fixed-source-helper'", "source / 'scripts/security'")
        # Fixed-source research retains runner sources and its historical error text.
        # Only the production profile selection and safe error projection differ.
        original_native = original_native.replace("profile = os.environ['CODEQL_NATIVE_SOURCE_PROFILE']\n", '')
        original_native = original_native.replace(", 'source_profile': profile", '')
        original_native = original_native.replace(', source_profile=profile)', ')')
        projected = ("terminal.update(error_type=type(error).__name__)\n"
                     "    if isinstance(error, native.runtime.PhaseError):\n"
                     "        terminal.update(exit=error.result.get('exit'), failure_kind=error.result.get('failure_kind'))\n"
                     "    if hasattr(error, 'native_source_evidence_error'):\n"
                     "        terminal['source_evidence_error'] = error.native_source_evidence_error")
        self.assertEqual(original_native.count(projected), 1)
        original_native = original_native.replace(projected,
            "terminal.update(error_type=type(error).__name__, error=str(error))")
        self.assertEqual(ast.dump(ast.parse(native)), ast.dump(ast.parse(original_native)))


if __name__ == "__main__":
    unittest.main()
