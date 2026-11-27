#!/usr/bin/env python3
"""Self-contained core workflow ordinary delivery, governance report, heavy PR timeout, draft PR fail-closed gate, and fast-preflight/main-fast admission contract tests (part 11)."""
import importlib.util
import itertools
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
import textwrap
import unittest

ROOT = Path(__file__).resolve().parent.parent
RAW_WORKFLOW = (ROOT / '.github/workflows/rust.yml').read_text()
CONFIG = json.loads((ROOT / 'scripts/ci-required-scope.json').read_text())


def workflow_jobs(workflow):
    jobs = dict(re.findall(r'^  ([a-z][a-z0-9_-]*):\n(.*?)(?=^  [a-z][a-z0-9_-]*:|\Z)',
                          workflow.split('\njobs:\n', 1)[1], re.M | re.S))
    anchors = {}
    for job in jobs.values():
        match = re.search(r'^    steps: &([a-z0-9-]+)\n(.*)', job, re.M | re.S)
        if match:
            assert match[1] not in anchors, 'duplicate step sequence anchor'
            anchors[match[1]] = match[2]
    jobs = {name: re.sub(r'^    steps: &[a-z0-9-]+\n', '    steps:\n',
                        re.sub(r'^    steps: \*([a-z0-9-]+)\n',
                               lambda match: '    steps:\n' + anchors[match[1]], job, flags=re.M), flags=re.M)
            for name, job in jobs.items()}
    # Resolve individual step aliases as well as complete step sequences. Keep
    # the old observation alias opaque: contracts inspect that step separately.
    definitions = re.findall(
        r'^      - &([a-z0-9-]+)\n(.*?)(?=^      - |^\S|^  [a-z]|\Z)',
        workflow, re.M | re.S)
    step_anchors = dict(definitions)
    assert len(step_anchors) == len(definitions), 'duplicate step anchor'
    assert not set(step_anchors).intersection(anchors), 'ambiguous step anchor'
    known_steps = set(step_anchors)
    step_anchors.pop('engineering-job-observation', None)
    for name, body in step_anchors.items():
        assert body.startswith('        '), f'invalid step anchor {name}'
        step_anchors[name] = '      - ' + body.removeprefix('        ')
    for name, job in jobs.items():
        for alias in re.findall(r'^      - \*([a-z0-9-]+)\n', job, re.M):
            assert alias in known_steps, f'undefined step alias {alias}'
        job = re.sub(r'^      - &([a-z0-9-]+)\n(.*?)(?=^      - |\Z)',
                     lambda match: step_anchors.get(match[1], match[0]), job, flags=re.M | re.S)
        jobs[name] = re.sub(r'^      - \*([a-z0-9-]+)\n',
                           lambda match: step_anchors.get(match[1], match[0]), job, flags=re.M)
    return jobs


def workflow_contract_view(workflow):
    """Use each job's actual steps for both positive contracts and mutations."""
    header = workflow.split('\njobs:\n', 1)[0]
    return header + '\njobs:\n' + ''.join(
        '  ' + name + ':\n' + body.rstrip('\n') + '\n\n'
        for name, body in workflow_jobs(workflow).items())


WORKFLOW = workflow_contract_view(RAW_WORKFLOW)
JOBS = workflow_jobs(WORKFLOW)


def workflow_step_script(job, name):
    step = next(step for step in re.findall(r'^      - .*?(?=^      - |\Z)', job, re.M | re.S)
                if 'name: ' + name + '\n' in step)
    if '        run: |\n' in step:
        return textwrap.dedent(step.split('        run: |\n', 1)[1])
    return step.split('        run: ', 1)[1].splitlines()[0] + '\n'


def effective_job_condition(condition, values, ancestor_results, *, cancelled=False):
    """Include Actions' implicit success() instead of evaluating only written YAML."""
    expression = condition.strip().removeprefix('>-').strip()
    if not re.search(r'\b(?:always|success|failure|cancelled)\s*\(', expression):
        expression = 'success() && (' + expression + ')'
    values = {
        'needs.fast-preflight.result': 'success',
        **values,
        'always()': True,
        'success()': not cancelled and all(result == 'success' for result in ancestor_results),
        'failure()': 'failure' in ancestor_results,
        'cancelled()': cancelled,
    }
    for token, value in sorted(values.items(), key=lambda item: -len(item[0])):
        expression = expression.replace(token, repr(value))
    expression = re.sub(r'!(?!=)', 'not ', expression.replace('&&', ' and ').replace('||', ' or '))
    return bool(eval(' '.join(expression.split()), {'__builtins__': {}},
                     {'contains': lambda collection, item: item in collection}))


class WorkflowCorePart11(unittest.TestCase):
    def steps(self, name):
        return re.findall(r'^      - .*?(?=^      - |\Z)', JOBS[name], re.M | re.S)

    def condition(self, step):
        return re.search(r'^        if: (.*?)(?=^        \w|\Z)', step, re.M | re.S)[1]

    def test_native_consumers_download_exactly_their_closure_and_controls_download_none(self):
        steps = [step for step in self.steps('oasis7-required')
                 if 'name: native-tests-' in step or 'name: Unpack ' in step]
        self.assertEqual(len(steps), 5)
        for delivery, inline, owner in itertools.product((None, 1, 2), (False, True),
                ('controls', 'runtime', 'component-libs', 'integrations-0', 'integrations-1', 'all', 'native-0', 'native-1', 'native-2', 'native-3')):
            if delivery is not None and (inline or owner == 'all'
                    or delivery == 1 and owner.startswith('native-')
                    or delivery == 2 and owner not in ('controls', 'native-0', 'native-1', 'native-2', 'native-3')):
                continue  # Independently rejected by the plan consumer.
            values = {'fromJSON(needs.select.outputs.plan).ordinary_artifact_delivery_version': delivery,
                      'fromJSON(needs.select.outputs.plan).fast_execution_version': 1 if inline else None,
                      'fromJSON(needs.select.outputs.plan).ordinary_execution': 'inline' if inline else None,
                      'matrix.partition': owner}
            active = [step for step in steps if effective_job_condition(self.condition(step), values, [])]
            with self.subTest(delivery=delivery, inline=inline, owner=owner):
                self.assertEqual(len(active), 0 if inline or delivery is not None and owner == 'controls'
                                 else 3 if delivery is not None else 2)
                if delivery is not None and owner != 'controls':
                    self.assertIn('name: native-tests-common-${{ github.sha }}', ''.join(active))
                    self.assertIn('name: native-tests-${{ matrix.partition }}-${{ github.sha }}', ''.join(active))
                    self.assertIn('ci-test-artifacts.py unpack-ordinary --plan "$RUNNER_TEMP/selection.json"', ''.join(active))
                for step in steps:
                    self.assertFalse(effective_job_condition(self.condition(step), values, ['failure']))

    def test_producer_keeps_one_build_fails_closed_and_publishes_all_five_packages(self):
        steps = self.steps('oasis7-test-build-candidate')
        build = next(step for step in steps if 'name: Build ordinary test artifacts' in step)
        script = textwrap.dedent(build.split('        run: |\n', 1)[1])
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            stub = ('python3() { printf "%s\\n" "$*" >> "$CALLS"; '
                    'if [[ "$2" == "$FAIL_ACTION" ]]; then return 17; fi; };\n')
            for version, failed in itertools.product(('0', '1', '2'), ('', 'build', 'pack')):
                calls = root / 'calls'
                calls.unlink(missing_ok=True)
                result = subprocess.run(['bash', '-c', stub + script], text=True, capture_output=True,
                    env=dict(os.environ, RUNNER_TEMP=str(root), ORDINARY_DELIVERY_VERSION=version,
                             CALLS=str(calls), FAIL_ACTION=failed))
                with self.subTest(version=version, failed=failed):
                    self.assertEqual(result.returncode, 17 if failed else 0, result.stderr)
                    lines = calls.read_text().splitlines()
                    self.assertEqual(len(lines), 1 if failed == 'build' else 2)
                    self.assertIn('build --scope ordinary --plan', lines[0])
                    if len(lines) == 2:
                        self.assertEqual('--split-ordinary' in lines[1], version in ('1', '2'))
        uploads = [step for step in steps if 'uses: actions/upload-artifact@' in step and 'name: native-tests-' in step]
        self.assertEqual(len(uploads), 6)
        for version, compact, baseline_controls in (
                (None, False, None), (1, False, None), (1, False, 1),
                (2, False, None), (2, False, 1), (2, True, None), (2, True, 1)):
            native_owners = (
                [] if version is None
                else ['runtime', 'component-libs', 'integrations-0', 'integrations-1'] if version == 1
                else ['native-0', 'native-1'] if compact
                else ['native-0', 'native-1', 'native-2', 'native-3']
            )
            owners = (['all'] if version is None
                      else native_owners if baseline_controls == 1
                      else ['controls', *native_owners])
            partition_values = {
                'fromJSON(needs.select.outputs.plan).ordinary_artifact_delivery_version': version,
                'fromJSON(needs.select.outputs.plan).ordinary_controls_in_rust_baseline_version': baseline_controls,
                'fromJSON(needs.select.outputs.ordinary_partitions)[0]': owners[0] if len(owners) > 0 else None,
                'fromJSON(needs.select.outputs.ordinary_partitions)[1]': owners[1] if len(owners) > 1 else None,
                'fromJSON(needs.select.outputs.ordinary_partitions)[2]': owners[2] if len(owners) > 2 else None,
                'fromJSON(needs.select.outputs.ordinary_partitions)[3]': owners[3] if len(owners) > 3 else None,
                'fromJSON(needs.select.outputs.ordinary_partitions)[4]': owners[4] if len(owners) > 4 else None,
            }
            selected = [step for step in uploads if effective_job_condition(self.condition(step), partition_values, [])]
            self.assertEqual(len(selected), 1 if version is None else 1 + len(native_owners))
            for step in selected:
                for required in ('compression-level: 0', 'overwrite: true', 'if-no-files-found: error'):
                    self.assertIn(required, step)
            if version is not None:
                resolved = '\n'.join(selected)
                for index, owner in enumerate(native_owners):
                    expression = (
                        '${{ fromJSON(needs.select.outputs.plan).ordinary_controls_in_rust_baseline_version == 1 && '
                        f'fromJSON(needs.select.outputs.ordinary_partitions)[{index}] || '
                        f'fromJSON(needs.select.outputs.ordinary_partitions)[{index + 1}] }}}}'
                    )
                    self.assertEqual(resolved.count(expression), 2)
                    resolved = resolved.replace(expression, owner)
                self.assertEqual(set(re.findall(r'/ordinary-archives/(.*)\.tar\.gz', resolved)),
                                 {'common', *native_owners})
                self.assertEqual(set(re.findall(r'name: native-tests-(.*)-\$\{\{ github.sha \}\}', resolved)),
                                 {'common', *native_owners})

    def test_retired_ordinary_delivery_preview_is_absent(self):
        self.assertNotIn('pr-ordinary-delivery-preview', JOBS)
        self.assertNotIn('pr-ordinary-delivery-preview', WORKFLOW)

    def test_governance_report_uses_real_entry_point(self):
        result = subprocess.run(
            ['bash', str(ROOT / 'scripts/ci-rust-governance-report.test.sh')],
            cwd=ROOT, capture_output=True, text=True, timeout=30,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_ci_tests_warning_zero_contract(self):
        result = subprocess.run(
            ['bash', str(ROOT / 'scripts/ci-tests-warning-zero-contract.test.sh')],
            cwd=ROOT, capture_output=True, text=True, timeout=30,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_heavy_pr_jobs_declare_bounded_fallback_timeout_minutes(self):
        expected_limits = {
            '.github/workflows/rust.yml': {
                'oasis7-test-build-candidate': 40,
                'oasis7-test-build-trusted': 45,
                'formal-debug-build': 35,
                'formal-debug-build-trusted': 35,
                'formal-endpoint-build': 35,
                'formal-endpoint-build-trusted': 35,
                'formal-test-build': 20,
                'formal-test-build-trusted': 20,
                'formal-process-tests': 45,
                'workspace-support-legacy': 60,
                'workspace-support-other': 35,
                'operational-contracts': 12,
            },
            '.github/workflows/wasm-determinism-gate.yml': {
                'collect-wasm-summaries': 22,
            },
        }
        for rel_path, limits in expected_limits.items():
            jobs = workflow_jobs((ROOT / rel_path).read_text())
            for job_name, max_minutes in limits.items():
                with self.subTest(workflow=rel_path, job=job_name):
                    self.assertIn(job_name, jobs)
                    match = re.search(r'^    timeout-minutes:\s*(\d+)\s*$', jobs[job_name], re.M)
                    self.assertIsNotNone(match, f'{rel_path}:{job_name} must declare explicit timeout-minutes')
                    actual = int(match.group(1))
                    self.assertGreater(actual, 0)
                    self.assertLessEqual(actual, max_minutes)

        required_job = workflow_jobs((ROOT / '.github/workflows/rust.yml').read_text())['oasis7-required']
        expr_match = re.search(r'^    timeout-minutes:\s*(\$\{\{.+?\}\})\s*$', required_job, re.M)
        self.assertIsNotNone(expr_match, '.github/workflows/rust.yml:oasis7-required must declare partition-bounded timeout-minutes')
        condition, heavy_raw, fast_raw = re.fullmatch(
            r'\$\{\{\s*(.+?)\s*&&\s*(\d+)\s*\|\|\s*(\d+)\s*\}\}',
            expr_match.group(1),
        ).groups()
        heavy_minutes, fast_minutes = int(heavy_raw), int(fast_raw)
        self.assertGreater(fast_minutes, 0)
        self.assertLessEqual(fast_minutes, 25)
        self.assertGreater(heavy_minutes, fast_minutes)
        self.assertLessEqual(heavy_minutes, 50)
        heavy_list = re.search(r"fromJSON\('([^']+)'\)", condition)[1]
        for execution, owner in itertools.product(
            (None, 'inline'),
            ('controls', 'all', 'runtime', 'component-libs', 'integrations-0', 'integrations-1',
             'native-0', 'native-1', 'native-2', 'native-3'),
        ):
            values = {
                'fromJSON(needs.select.outputs.plan).ordinary_execution': execution,
                f"fromJSON('{heavy_list}')": json.loads(heavy_list),
                'matrix.partition': owner,
            }
            expect_heavy = execution == 'inline' or owner in ('all', 'component-libs', 'runtime')
            with self.subTest(execution=execution, owner=owner):
                self.assertEqual(effective_job_condition(condition, values, []), expect_heavy)

    def test_draft_pr_fail_closed_gate_and_main_fast_cancelled_push_range_recovery(self):
        pr_header = RAW_WORKFLOW.split('  pull_request:\n', 1)[1].split('  schedule:\n', 1)[0]
        self.assertIn('types: [opened, synchronize, reopened, ready_for_review, labeled]', pr_header)
        concurrency = RAW_WORKFLOW.split('concurrency:\n', 1)[1].split('permissions:\n', 1)[0]
        self.assertIn(
            "(github.event_name == 'pull_request' && github.event.action == 'labeled' && "
            "(github.event.label.name != 'ci-priority' || "
            "github.event.pull_request.head.repo.full_name != github.repository || "
            "github.event.pull_request.draft)) && format('pr-label-noop-{0}', github.run_id)",
            concurrency,
        )
        self.assertEqual(CONFIG.get('main_fast_range_recovery_version'), 1)
        self.assertEqual(CONFIG.get('main_fast_coalesce_concurrency_version'), 1)
        self.assertEqual(CONFIG.get('draft_pr_heavy_ci_skip_version'), 1)
        self.assertEqual(CONFIG.get('auxiliary_workflow_consolidation_version'), 1)
        self.assertEqual(CONFIG.get('workspace_member_cargo_manifest_scope_version'), 2)
        self.assertEqual(CONFIG.get('ordinary_controls_in_rust_baseline_version'), 1)
        self.assertEqual(CONFIG.get('contract_job_consolidation_version'), 5)
        self.assertEqual(CONFIG.get('workspace_support_direct_gate_version'), 2)
        self.assertEqual(CONFIG.get('ordinary_scope_config_slice_version'), 2)
        self.assertEqual(CONFIG.get('scoped_production_cfg_version'), 1)

        gate = JOBS['required-gate']
        self.assertIn(
            "if: >-\n"
            "      always() && ((github.event_name == 'push' &&\n"
            "        (github.ref != 'refs/heads/main' || needs.select.result != 'skipped')) ||\n"
            "      (github.event_name == 'pull_request' &&\n"
            "       (github.event.action != 'labeled' ||\n"
            "        (github.event.label.name == 'ci-priority' &&\n"
            "         github.event.pull_request.head.repo.full_name == github.repository &&\n"
            "         !github.event.pull_request.draft))))",
            gate,
        )
        self.assertIn(
            "(github.event.action != 'labeled' ||\n"
            "         (github.event.label.name == 'ci-priority' &&\n"
            "          github.event.pull_request.head.repo.full_name == github.repository))",
            JOBS['select'],
        )
        self.assertIn(
            "IS_DRAFT_PR: ${{ github.event_name == 'pull_request' && github.event.pull_request.draft && 'true' || 'false' }}",
            gate,
        )
        gate_step = workflow_step_script(gate, 'Require successful scope planning')
        for select_result in ('skipped', 'success', 'failure'):
            draft_res = subprocess.run(
                ['bash', '-euo', 'pipefail', '-c', gate_step],
                env=dict(os.environ, IS_DRAFT_PR='true', SELECT_RESULT=select_result),
                text=True, capture_output=True,
            )
            self.assertNotEqual(draft_res.returncode, 0)
            self.assertIn('Draft PR skipped main CI jobs', draft_res.stdout + draft_res.stderr)
        ready_ok = subprocess.run(
            ['bash', '-euo', 'pipefail', '-c', gate_step],
            env=dict(os.environ, IS_DRAFT_PR='false', SELECT_RESULT='success'),
            text=True, capture_output=True,
        )
        self.assertEqual(ready_ok.returncode, 0, ready_ok.stdout + ready_ok.stderr)

        select_job = JOBS['select']
        self.assertIn('permissions:\n      contents: read\n      actions: read', select_job)
        self.assertIn(
            "MAIN_FAST_BASE_TOKEN: ${{ github.event_name == 'push' && github.ref == 'refs/heads/main' && github.token || '' }}",
            select_job,
        )
        with tempfile.TemporaryDirectory() as directory:
            repo = Path(directory) / 'repo'
            repo.mkdir()
            def git(*args):
                return subprocess.check_output(['git', '-C', str(repo), *args], text=True).strip()
            git('init', '-q', '-b', 'main')
            git('config', 'user.name', 'Fixture')
            git('config', 'user.email', 'fixture@example.invalid')
            (repo / 'scripts').mkdir()
            for name in ('ci-required-scope.json', 'ci-required-result.py', 'ci-formal-plan.py',
                         'ci-formal-process-suites.json'):
                (repo / 'scripts' / name).write_bytes((ROOT / 'scripts' / name).read_bytes())
            (repo / 'scripts/plan-rust-required-scope.py').write_text('''import argparse, json, pathlib, subprocess
p = argparse.ArgumentParser()
for key in ('profile', 'base-ref', 'head-ref', 'test-ref', 'policy-ref', 'formal-config', 'github-output', 'output'):
    p.add_argument('--' + key)
a = p.parse_args()
diff_files = subprocess.check_output(['git', 'diff', '--name-only', a.base_ref, a.head_ref], text=True).splitlines()
groups = ['baseline', 'oasis7_required'] if 'rust_touch.txt' in diff_files else ['baseline']
plan = {'scope': 'targeted', 'execution_profile': a.profile, 'fast_plan_version': 1,
        'check_inventory_version': 1, 'groups': groups,
        'source_sha': a.head_ref, 'tested_sha': a.test_ref, 'policy_sha': a.policy_ref}
pathlib.Path(a.output).write_text(json.dumps(plan))
with pathlib.Path(a.github_output).open('a') as f:
    for g in groups:
        f.write(f'run_{g}=true\\n')
''')
            (repo / 'README.md').write_text('m0\n')
            git('add', '.')
            git('commit', '-qm', 'm0')
            m0 = git('rev-parse', 'HEAD')
            # m1 changes rust_touch.txt AND installs a deterministic resolve-main-fast-base stub in trusted policy.
            (repo / 'rust_touch.txt').write_text('changed in cancelled m1\n')
            (repo / 'scripts/ci-full-result.py').write_text(f'''import argparse, os, sys
p = argparse.ArgumentParser()
sub = p.add_subparsers(dest='command', required=True)
r = sub.add_parser('resolve-main-fast-base')
for k in ('repo-root', 'before', 'head'):
    r.add_argument('--' + k, required=True)
a = p.parse_args()
assert os.environ.get('MAIN_FAST_BASE_TOKEN') == 'push-read-token'
print({m0!r})
''')
            git('add', '.')
            git('commit', '-qm', 'm1')
            m1 = git('rev-parse', 'HEAD')
            # m2 only touches README.md; without range recovery m1..m2 would miss rust_touch.txt!
            (repo / 'README.md').write_text('m2 doc-only\n')
            git('add', '.')
            git('commit', '-qm', 'm2')
            m2 = git('rev-parse', 'HEAD')

            script = workflow_step_script(select_job, 'Select using trusted baseline Git data')
            out_file = Path(directory) / 'github-output'
            env = dict(
                os.environ, EVENT_NAME='push', SOURCE_REF='', SOURCE_SHA=m2, BASE_SHA=m1,
                WORKFLOW_SHA=m2, GITHUB_SHA=m2, GITHUB_REF='refs/heads/main',
                GITHUB_WORKSPACE=str(repo), GITHUB_REPOSITORY='nil-studio/oasis7',
                MAIN_FAST_BASE_TOKEN='push-read-token',
                RUNNER_TEMP=str(directory), GITHUB_OUTPUT=str(out_file),
                GITHUB_STEP_SUMMARY=str(Path(directory) / 'summary'),
            )
            res = subprocess.run(['bash', '-c', script], cwd=repo, env=env, text=True, capture_output=True)
            self.assertEqual(res.returncode, 0, res.stdout + res.stderr)
            outputs = dict(line.split('=', 1) for line in out_file.read_text().splitlines())
            self.assertEqual(outputs['policy_sha'], m1)
            self.assertEqual(outputs['range_base'], m0)
            self.assertEqual(outputs['run_oasis7_required'], 'true')
            plan = json.loads((Path(directory) / 'selection.json').read_text())
            self.assertEqual((plan['policy_sha'], plan['source_sha'], plan['tested_sha']), (m1, m2, m2))
            self.assertIn('oasis7_required', plan['groups'])

    def test_fast_preflight_and_main_fast_admission_contracts(self):
        admission = JOBS['main-fast-admission']
        self.assertIn("if: github.event_name == 'push' && github.ref == 'refs/heads/main'", admission)
        self.assertIn('fetch-depth: 0', admission)
        self.assertIn('scripts/ci-full-result.py admit-main-fast --head "$GITHUB_SHA"', admission)
        self.assertIn('ci-main-fast-deferred-${{ github.run_id }}-${{ github.run_attempt }}', admission)
        self.assertIn('ci-main-fast-admission-${{ github.run_id }}-${{ github.run_attempt }}', admission)

        preflight = JOBS['fast-preflight']
        self.assertIn('needs: select', preflight)
        self.assertIn("github.event_name == 'pull_request'", preflight)
        self.assertIn('cargo +"$RUST_TOOLCHAIN" fmt --all -- --check', preflight)
        self.assertNotIn('fast-preflight', JOBS['required-gate'].split('    steps:', 1)[0])
        self.assertEqual(
            re.search(r'^\s+image:\s*(\S+)\s*$', preflight, re.M).group(1),
            re.search(r'^\s+image:\s*(\S+)\s*$', JOBS['rust-baseline'], re.M).group(1),
            'fast-preflight and rust-baseline must share the same container image when skipping duplicate rustfmt',
        )

        for heavy in ('rust-baseline', 'oasis7-test-build-candidate', 'workspace-support-other',
                      'custody-native-contracts', 'custody-native', 'custody-wasm-production',
                      'distfs', 'node', 'net', 'scenario-regression'):
            job = JOBS[heavy]
            self.assertIn('needs: [select, fast-preflight]', job)
            self.assertIn("(needs.fast-preflight.result == 'success' || needs.fast-preflight.result == 'skipped')", job)
            cond = re.search(r'^    if: (.*?)(?=^    [a-z-]+:|\Z)', job, re.M | re.S).group(1)
            base_vals = {
                'needs.select.result': 'success',
                'needs.select.outputs.run_rust_baseline': 'true',
                'needs.select.outputs.run_oasis7_required': 'true',
                'needs.select.outputs.run_distfs': 'true',
                'needs.select.outputs.run_node': 'true',
                'needs.select.outputs.run_consensus': 'false',
                'needs.select.outputs.run_net': 'true',
                'needs.select.outputs.run_launcher_web': 'true',
                'needs.select.outputs.run_scenario_regression': 'true',
                'needs.select.outputs.run_pixel_world_bridge': 'false',
                'needs.select.outputs.run_cargo_tooling_contracts': 'false',
                'needs.select.outputs.trusted_source': 'false',
                'needs.select.outputs.workspace_support_partitions': ['other'],
                'fromJSON(needs.select.outputs.workspace_support_partitions)': ['other'],
                'fromJSON(needs.select.outputs.plan).scope': 'targeted',
                'fromJSON(needs.select.outputs.plan).execution_profile': 'pr-fast',
                'fromJSON(needs.select.outputs.plan).fast_plan_version': 1,
                'fromJSON(needs.select.outputs.plan).fast_execution_version': 0,
                'fromJSON(needs.select.outputs.plan).ordinary_execution': '',
                'fromJSON(needs.select.outputs.plan).ordinary_main_fast_cache_writer_version': 0,
                'fromJSON(needs.select.outputs.plan).custody_execution_version': 1,
                'fromJSON(needs.select.outputs.plan).workspace_support_direct_gate_version': 2,
                'fromJSON(needs.select.outputs.plan).contract_job_consolidation_version': 5,
                'github.event_name': 'pull_request',
                'github.ref': 'refs/pull/1/merge',
                'github.workflow_sha': 'a',
                'github.sha': 'b',
                'inputs.run_mode': '',
            }
            self.assertFalse(effective_job_condition(
                cond, dict(base_vals, **{'needs.fast-preflight.result': 'failure'}), ('success', 'failure'),
            ), f'{heavy} must skip when fast-preflight fails')
            self.assertTrue(effective_job_condition(
                cond, dict(base_vals, **{'needs.fast-preflight.result': 'success'}), ('success', 'success'),
            ), f'{heavy} must run when fast-preflight succeeds')
            self.assertTrue(effective_job_condition(
                cond, dict(base_vals, **{'needs.fast-preflight.result': 'skipped'}), ('success', 'skipped'),
            ), f'{heavy} must run when fast-preflight is skipped')

        for host_job, overrides in (
            ('node', {'needs.select.outputs.run_node': 'false', 'needs.select.outputs.run_consensus': 'true'}),
            ('scenario-regression', {
                'needs.select.outputs.run_scenario_regression': 'false',
                'needs.select.outputs.run_pixel_world_bridge': 'true',
            }),
            ('scenario-regression', {
                'needs.select.outputs.run_scenario_regression': 'false',
                'needs.select.outputs.run_cargo_tooling_contracts': 'true',
            }),
        ):
            cond = re.search(r'^    if: (.*?)(?=^    [a-z-]+:|\Z)', JOBS[host_job], re.M | re.S).group(1)
            vals = dict(base_vals, **overrides)
            self.assertFalse(effective_job_condition(
                cond, dict(vals, **{'needs.fast-preflight.result': 'failure'}), ('success', 'failure'),
            ), f'{host_job} consolidated trigger must skip when fast-preflight fails')
            self.assertTrue(effective_job_condition(
                cond, dict(vals, **{'needs.fast-preflight.result': 'success'}), ('success', 'success'),
            ), f'{host_job} consolidated trigger must run when fast-preflight succeeds')
            self.assertTrue(effective_job_condition(
                cond, dict(vals, **{'needs.fast-preflight.result': 'skipped'}), ('success', 'skipped'),
            ), f'{host_job} consolidated trigger must run when fast-preflight is skipped')

        spec = importlib.util.spec_from_file_location('ci_required_result', ROOT / 'scripts/ci-required-result.py')
        required_result = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(required_result)
        plan = {'scope': 'targeted', 'contract_job_consolidation_version': 5, 'groups': ['baseline']}
        group_jobs = required_result.configured_group_jobs(CONFIG, plan)
        retired_jobs = required_result.retired_group_jobs(CONFIG, plan)
        for selected_group in ('rust_baseline', 'distfs', 'node', 'consensus', 'net',
                               'scenario_regression', 'pixel_world_bridge',
                               'cargo_tooling_contracts'):
            plan_with_group = dict(plan, groups=['baseline', selected_group])
            needs = {'select': {'result': 'success'}}
            for grp, job_name in group_jobs.items():
                needs[job_name] = {'result': 'skipped'}
            needs['baseline'] = {'result': 'success'}
            for rj in retired_jobs:
                needs[rj] = {'result': 'skipped'}
            with self.subTest(skipped_group=selected_group), self.assertRaisesRegex(
                ValueError, rf'{selected_group}: unexpected result \'skipped\'',
            ):
                required_result.validate(plan_with_group, needs, group_jobs, retired_jobs=retired_jobs)

        inline_job = JOBS['oasis7-required']
        self.assertIn('needs: [select, fast-preflight, oasis7-test-build-candidate, oasis7-test-build-trusted]', inline_job)
        inline_cond = re.search(r'^    if: (.*?)(?=^    [a-z-]+:|\Z)', inline_job, re.M | re.S).group(1)
        inline_vals = {
            'needs.select.result': 'success',
            'needs.select.outputs.run_oasis7_required': 'true',
            'needs.oasis7-test-build-candidate.result': 'skipped',
            'needs.oasis7-test-build-trusted.result': 'skipped',
            'fromJSON(needs.select.outputs.plan).scope': 'targeted',
            'fromJSON(needs.select.outputs.plan).fast_execution_version': 1,
            'fromJSON(needs.select.outputs.plan).ordinary_execution': 'inline',
        }
        self.assertFalse(effective_job_condition(
            inline_cond, dict(inline_vals, **{'needs.fast-preflight.result': 'failure'}),
            ('success', 'failure', 'skipped', 'skipped'),
        ), 'inline oasis7-required must skip when fast-preflight fails')
        self.assertTrue(effective_job_condition(
            inline_cond, dict(inline_vals, **{'needs.fast-preflight.result': 'success'}),
            ('success', 'success', 'skipped', 'skipped'),
        ), 'inline oasis7-required must run when fast-preflight succeeds')
        self.assertTrue(effective_job_condition(
            inline_cond, dict(inline_vals, **{'needs.fast-preflight.result': 'skipped'}),
            ('success', 'skipped', 'skipped', 'skipped'),
        ), 'inline oasis7-required must run when fast-preflight is skipped')

        skip_fmt_match = re.search(
            r'OASIS7_SKIP_RUSTFMT:\s*\$\{\{\s*(.+?)\s*\}\}',
            JOBS['rust-baseline'],
        )
        self.assertIsNotNone(skip_fmt_match, 'rust-baseline must wire OASIS7_SKIP_RUSTFMT')
        skip_expr_raw = skip_fmt_match.group(1)
        for event_name, preflight_res, scope, expected_flag in (
            ('pull_request', 'success', 'targeted', '1'),
            ('pull_request', 'skipped', 'targeted', '0'),
            ('pull_request', 'success', 'full', '0'),
            ('push', 'skipped', 'targeted', '0'),
            ('schedule', 'skipped', 'explicit-full', '0'),
            ('workflow_dispatch', 'skipped', 'explicit-full', '0'),
        ):
            py_expr = skip_expr_raw
            for token, val in {
                'github.event_name': repr(event_name),
                'needs.fast-preflight.result': repr(preflight_res),
                'fromJSON(needs.select.outputs.plan).scope': repr(scope),
            }.items():
                py_expr = py_expr.replace(token, val)
            py_expr = py_expr.replace('&&', ' and ').replace('||', ' or ').replace('!=', ' != ').replace('==', ' == ')
            with self.subTest(event_name=event_name, preflight=preflight_res, scope=scope):
                self.assertEqual(eval(py_expr, {'__builtins__': {}}, {}), expected_flag)

        driver = (ROOT / 'scripts/ci-tests.sh').read_text()
        body = dict(re.findall(r'^(\w+)\(\) \{\n(.*?)^\}', driver, re.M | re.S))['run_rust_baseline']
        script = (
            'set -euo pipefail\ntier=required; required_plan=""\n'
            'run() { printf "run:%s\\n" "$*"; }\n'
            'run_cargo() { printf "cargo:%s\\n" "$*"; }\n'
            'run_rustsec_advisory_check() { echo advisories; }\n'
            'run_oasis7_required_clippy_for_group() { echo clippy; }\n'
            'run_newapi_bridge_service_accounting_tests() { echo newapi; }\n'
            'run_standalone_tool_lockfiles_checks() { echo lockfiles; }\n'
            'run_rust_baseline() {\n' + body + '}\nrun_rust_baseline\n'
        )
        for env_override, expect_fmt in (
            ({}, True),
            ({'OASIS7_SKIP_RUSTFMT': '0'}, True),
            ({'OASIS7_SKIP_RUSTFMT': '1'}, False),
        ):
            env = dict(os.environ)
            env.pop('OASIS7_SKIP_RUSTFMT', None)
            env.update(env_override)
            res = subprocess.run(['bash', '-c', script], text=True, capture_output=True, env=env, check=True)
            lines = res.stdout.splitlines()
            self.assertEqual(
                'run:env -u RUSTC_WRAPPER cargo fmt --all -- --check' in lines,
                expect_fmt,
                (env_override, lines),
            )
            self.assertIn('cargo:fetch --locked', lines)
            self.assertIn('advisories', lines)
            self.assertIn('clippy', lines)


if __name__ == '__main__':
    unittest.main()
