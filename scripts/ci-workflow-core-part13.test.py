#!/usr/bin/env python3
"""Self-contained core workflow selection, projection, shared-cache, and platform-gate contract tests (part 13)."""
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


def project_selection(directory, plan=None):
    root = Path(directory)
    selection = root / 'selection.json'
    if plan is not None:
        selection.write_text(json.dumps(plan, ensure_ascii=False, indent=2) + '\n', encoding='utf-8')
    output = root / 'projection-output'
    output.unlink(missing_ok=True)
    result = subprocess.run(['bash', '-euo', 'pipefail', '-c',
        workflow_step_script(JOBS['select'], 'Publish workflow scheduling projection')],
        env=dict(os.environ, RUNNER_TEMP=str(root), GITHUB_OUTPUT=str(output)), text=True, capture_output=True)
    if result.returncode:
        raise AssertionError(result.stdout + result.stderr)
    return json.loads(dict(line.split('=', 1) for line in output.read_text().splitlines())['plan'])


def large_transport_plan():
    return dict(scope='targeted', execution_profile='pr-fast', fast_plan_version=1,
        fast_execution_version=1, ordinary_execution='artifact', ordinary_artifact_delivery_version=1,
        ordinary_artifact_partition_version=1, oasis7_required_packages=['oasis7_runtime'],
        custody_execution_version=1, custody_node_shards=[0, 1, 2, 3], python_control_contract_version=1,
        groups=['baseline', 'workspace_support'], source_sha='a' * 40, tested_sha='b' * 40,
        policy_sha='c' * 40, check_inventory_version=1, fast_plan_exceptions=['preserve every exception'],
        architecture=dict(version=1, mode='full', packages=[], base='c' * 40, source='a' * 40, tested='b' * 40),
        reasons=["quoted ' \" \\\n$(touch transport-must-not-execute)\nPY\n" * 4000])


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


class WorkflowCorePart13(unittest.TestCase):
    def shared_cache_workflow(self):
        path = ROOT / '.github/workflows/rust-cache.yml'
        self.assertTrue(path.is_file(), 'dependency warming must have an independent workflow')
        workflow = path.read_text()
        return workflow, workflow_jobs(workflow)['shared-native-cache']

    def run_selection(self, directory, *, event='schedule', candidate=False, source_ref='',
                      ref=None, policy_config=None, candidate_config=None):
        """Execute the real workflow shell against immutable local Git fixtures."""
        repo = Path(directory) / 'repo'
        repo.mkdir()
        subprocess.run(['git', 'init', '-q', '-b', 'main', str(repo)], check=True)
        def git(*args):
            return subprocess.check_output(['git', '-C', str(repo), *args], text=True).strip()
        git('config', 'user.name', 'CI fixture')
        git('config', 'user.email', 'fixture@example.invalid')
        (repo / 'scripts').mkdir()
        for name in ('plan-rust-required-scope.py', 'ci-required-scope.json', 'ci-required-result.py',
                     'ci-formal-plan.py', 'ci-formal-process-suites.json', 'ci-python-control-contracts.py'):
            (repo / 'scripts' / name).write_bytes((ROOT / 'scripts' / name).read_bytes())
        if policy_config is not None:
            (repo / 'scripts/ci-required-scope.json').write_text(json.dumps(policy_config))
        (repo / 'scripts/ci-full-result.py').write_text('# producer is not executed by selection\n')
        git('add', '.')
        git('commit', '-qm', 'trusted policy')
        policy = git('rev-parse', 'HEAD')
        git('remote', 'add', 'origin', str(repo))
        git('checkout', '-qb', 'candidate')
        (repo / 'README.md').write_text('candidate behavior\n')
        (repo / 'scripts/plan-rust-required-scope.py').write_text("raise SystemExit('untrusted candidate planner executed')\n")
        (repo / 'scripts/ci-python-control-contracts.py').write_text("raise SystemExit('untrusted candidate control authority executed')\n")
        if candidate_config is not None:
            (repo / 'scripts/ci-required-scope.json').write_text(json.dumps(candidate_config))
        git('add', '.')
        git('commit', '-qm', 'candidate')
        source = git('rev-parse', 'HEAD')
        if not candidate and not source_ref:
            git('checkout', '-q', 'main')
            source = policy
        requested = source if source_ref == 'candidate' else source_ref
        script = workflow_step_script(JOBS['select'], 'Select using trusted baseline Git data')
        output = Path(directory) / 'github-output'
        (Path(directory) / 'selection-outputs.txt').write_text('stale_output=must-not-survive\n')
        environment = dict(os.environ, EVENT_NAME=event, SOURCE_REF=requested,
            SOURCE_SHA=source if candidate else policy, BASE_SHA=policy, WORKFLOW_SHA=source if candidate else policy,
            GITHUB_SHA=source if candidate else policy,
            GITHUB_REF=ref or ('refs/heads/candidate' if candidate and event != 'pull_request' else
                              'refs/pull/1/merge' if event == 'pull_request' else 'refs/heads/main'),
            RUNNER_TEMP=str(directory), GITHUB_OUTPUT=str(output),
            GITHUB_STEP_SUMMARY=str(Path(directory) / 'summary'))
        result = subprocess.run(['bash', '-c', script], cwd=repo, env=environment,
                                text=True, capture_output=True)
        values = dict(line.split('=', 1) for line in output.read_text().splitlines()) if output.exists() else {}
        self.assertNotIn('plan', values, 'the trusted planner must not expose the complete plan as an Actions output')
        self.assertNotIn('stale_output', values)
        plan = None
        if result.returncode == 0:
            plan = json.loads((Path(directory) / 'selection.json').read_text())
            values['plan'] = json.dumps(project_selection(directory), separators=(',', ':'))
        return result, values, plan, policy, source

    def test_component_image_writer_runs_only_for_current_main_cache_events(self):
        workflow, _ = self.shared_cache_workflow()
        job = workflow_jobs(workflow)['rust-image-component-cache']
        supports_image_support = ('plan-image-component-cache' in workflow_jobs(workflow)
                                  or 'image-support' in workflow.split('jobs:', 1)[0])
        expression = ' '.join(job.split('    if:', 1)[1].split('    strategy:', 1)[0]
                              .strip().removeprefix('>-').split())
        for event, ref, matching, mode, repository in itertools.product(
                ('schedule', 'workflow_dispatch', 'pull_request', 'push', 'workflow_run'),
                ('refs/heads/main', 'refs/heads/topic', 'refs/pull/1/merge', 'refs/tags/v1'),
                (True, False), ('full', 'image-native', 'image-support', 'pr-fast', ''), ('nil-studio/oasis7', 'fork/oasis7')):
            values = {'github.event_name': event, 'github.ref': ref, 'github.workflow_sha': 'a',
                      'github.sha': 'a' if matching else 'b', 'inputs.run_mode': mode, 'github.repository': repository}
            predicate = expression
            for token, value in values.items():
                predicate = predicate.replace(token, repr(value))
            with self.subTest(event=event, ref=ref, matching=matching, mode=mode):
                self.assertEqual(eval(predicate.replace('&&', ' and ').replace('||', ' or '), {'__builtins__': {}}, {}),
                    ref == 'refs/heads/main' and matching and (event == 'schedule'
                    or event == 'workflow_dispatch' and (mode in ('full', 'image-native')
                        or mode == 'image-support' and repository == 'nil-studio/oasis7' and supports_image_support)))

    def test_component_cache_planner_runs_the_complete_or_support_only_matrix(self):
        workflow, _ = self.shared_cache_workflow()
        jobs = workflow_jobs(workflow)
        planner = jobs.get('plan-image-component-cache')
        writer = jobs['rust-image-component-cache']
        if planner is None:
            if 'fromJSON(' in writer:
                self.assertIn('image-support', workflow.split('jobs:', 1)[0])
                include_expr = writer.split('        include:', 1)[1].split('\n    runs-on:', 1)[0]
                include_expr = ' '.join(include_expr.strip().removeprefix('>-').split())
                self.assertTrue(include_expr.startswith('${{') and include_expr.endswith('}}'))
                inner = include_expr[3:-2].strip()
                full_rows = None
                for mode in ('', 'full', 'image-native', 'image-support'):
                    expr = inner.replace('inputs.run_mode', repr(mode)).replace('&&', ' and ').replace('||', ' or ')
                    rows = eval(expr, {'__builtins__': {}}, {'fromJSON': json.loads})
                    profiles = [row['profile'] for row in rows]
                    if mode == 'image-support':
                        self.assertEqual(profiles, ['support'])
                        self.assertEqual(rows, [row for row in full_rows if row['profile'] == 'support'])
                    else:
                        self.assertEqual(profiles, ['native', 'node', 'net', 'launcher', 'tooling', 'support'])
                        full_rows = rows
                return
            self.assertNotIn('image-support', workflow.split('jobs:', 1)[0])
            self.assertIn('matrix:\n        include:\n', writer)
            self.assertEqual(re.findall(r'^          - profile: ([a-z]+)$', writer, re.M),
                             ['native', 'node', 'net', 'launcher', 'tooling', 'support'])
            return
        predicate = lambda job: ' '.join(job.split('    if: >-\n', 1)[1].split('\n    ', 1)[0].split())
        self.assertEqual(predicate(planner), predicate(writer))
        script = textwrap.dedent(planner.split('        run: |\n', 1)[1]).strip()
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / 'output'
            for mode in ('', 'full', 'image-native', 'image-support'):
                output.write_text('')
                subprocess.run(['bash', '-e', '-c', script], check=True,
                               env=os.environ | {'MODE': mode, 'GITHUB_OUTPUT': str(output)})
                rows = json.loads(output.read_text().removeprefix('rows='))
                profiles = [row['profile'] for row in rows]
                if mode == 'image-support':
                    self.assertEqual(profiles, ['support'])
                    self.assertEqual(rows, [row for row in full_rows if row['profile'] == 'support'])
                else:
                    self.assertEqual(profiles, ['native', 'node', 'net', 'launcher', 'tooling', 'support'])
                    full_rows = rows

    def test_large_workflow_projection_preserves_fields_and_binds_complete_file_and_identity(self):
        fields = {'scope', 'execution_profile', 'fast_plan_version', 'fast_execution_version',
                  'ordinary_execution', 'ordinary_artifact_delivery_version', 'ordinary_artifact_partition_version',
                  'oasis7_required_packages', 'custody_execution_version', 'custody_node_shards',
                  'python_control_contract_version', 'groups', 'source_sha', 'tested_sha', 'policy_sha',
                  'check_inventory_version', 'fast_plan_exceptions'}
        consumers = set(re.findall(r'fromJSON\(needs\.select\.outputs\.plan\)\.([A-Za-z0-9_]+)', RAW_WORKFLOW))
        fleet_subcontract_fields = {'fleet_health_subcontracts_version', 'fleet_health_oses', 'fleet_health_checks'}
        self.assertLessEqual(consumers, fields | fleet_subcontract_fields | {
            'ordinary_dynamic_partition_version', 'ordinary_controls_in_rust_baseline_version',
            'ordinary_main_fast_cache_writer_version',
            'contract_job_consolidation_version', 'command_center_offline_version',
            'workspace_support_direct_gate_version',
            'custody_compile_proof_version', 'custody_policy_tests_version', 'selection_sha256'})
        self.assertIn('plan: ${{ steps.workflow-plan.outputs.plan }}', JOBS['select'])
        guard = workflow_step_script(JOBS['required-gate'], 'Validate complete selection handoff')
        self.assertEqual(guard, workflow_step_script(JOBS['explicit-full-gate'], 'Validate complete selection handoff'))
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            plan = large_transport_plan()
            projected = project_selection(root, plan)
            raw = (root / 'selection.json').read_bytes()
            self.assertGreater(len(raw), 128 * 1024)
            self.assertLess(len(json.dumps(projected).encode()), 16 * 1024)
            self.assertEqual(set(projected), fields | {'workflow_projection_version', 'selection_sha256'})
            self.assertEqual({key: projected[key] for key in fields}, {key: plan[key] for key in fields})
            self.assertEqual(project_selection(root), projected)
            self.assertEqual((root / 'selection.json').read_bytes(), raw)
            environment = dict(os.environ, RUNNER_TEMP=directory, SOURCE_SHA=plan['source_sha'],
                               TESTED_SHA=plan['tested_sha'], POLICY_SHA=plan['policy_sha'])
            def invoke(value):
                return subprocess.run(['bash', '-euo', 'pipefail', '-c', guard], cwd=root,
                    env=dict(environment, WORKFLOW_PLAN=json.dumps(value)), text=True, capture_output=True)
            result = invoke(projected)
            self.assertEqual(result.returncode, 0, result.stderr)
            if fleet_subcontract_fields.intersection(consumers):
                fleet_extra = {
                    'fleet_health_subcontracts_version': 1,
                    'fleet_health_checks': ['macos_signer'],
                    'fleet_health_oses': ['macos-15'],
                }
                fleet_projected = project_selection(root, plan | fleet_extra)
                for key, expected_val in fleet_extra.items():
                    self.assertEqual(fleet_projected.get(key), expected_val)
                fleet_result = invoke(fleet_projected)
                self.assertEqual(fleet_result.returncode, 0, fleet_result.stderr)
                self.assertNotEqual(invoke(fleet_projected | {'fleet_health_oses': ['windows-2022']}).returncode, 0)
                project_selection(root, plan)
            for key in fields:
                for broken in ({k: v for k, v in projected.items() if k != key},
                               projected | {key: {'changed': True}}):
                    with self.subTest(field=key, missing=key not in broken):
                        self.assertNotEqual(invoke(broken).returncode, 0)
            for change in ({'workflow_projection_version': True}, {'selection_sha256': '0' * 64},
                           {'unknown_projection_field': 1}, {'fast_plan_version': True}):
                self.assertNotEqual(invoke(projected | change).returncode, 0)
            (root / 'selection.json').write_bytes(raw + b' ')
            self.assertNotEqual(invoke(projected).returncode, 0)
            for key in ('source_sha', 'tested_sha', 'policy_sha'):
                with self.subTest(identity=key):
                    changed = project_selection(root, plan | {key: '0' * 40})
                    self.assertNotEqual(invoke(changed).returncode, 0)
            legacy = {key: plan[key] for key in ('scope', 'groups', 'architecture', 'reasons')}
            old = project_selection(root, legacy)
            result = invoke(old)
            self.assertEqual(result.returncode, 0, result.stderr)
            for change in ({'base': '0' * 40}, {'source': None}, {'tested': '0' * 40}):
                changed = project_selection(root, legacy | {'architecture': legacy['architecture'] | change})
                self.assertNotEqual(invoke(changed).returncode, 0)
            changed = project_selection(root, {key: value for key, value in legacy.items() if key != 'architecture'})
            self.assertNotEqual(invoke(changed).returncode, 0)
            (root / 'selection.json').unlink()
            self.assertNotEqual(invoke(projected).returncode, 0)
            self.assertFalse((root / 'transport-must-not-execute').exists())

    def test_trusted_selection_projects_custody_proof_only_for_the_capable_baseline(self):
        job_condition = re.search(r'^    if: (.*?)(?=^    [a-z-]+:|\Z)',
                                  JOBS['custody-native-contracts'], re.M | re.S)[1]
        proof_step = next(step for step in re.findall(r'^      - .*?(?=^      - |\Z)',
                                                      JOBS['custody-native-contracts'], re.M | re.S)
                          if 'name: Execute complete custody WASM compile proofs\n' in step)
        step_condition = re.search(r'^        if: (.+)$', proof_step, re.M)[1]
        legacy_capable = dict(CONFIG, custody_execution_version=1, custody_compile_proof_version=1,
                              custody_policy_tests_version=1, custody_node_shards_version=2)
        for version in ('new', 'old-custody', 'legacy'):
            policy = dict(legacy_capable)
            if version != 'new':
                del policy['custody_compile_proof_version']
                del policy['custody_policy_tests_version']
            if version == 'legacy':
                del policy['custody_execution_version']
            with self.subTest(version=version), tempfile.TemporaryDirectory() as directory:
                result, outputs, plan, _, _ = self.run_selection(directory, policy_config=policy)
                self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                projected = json.loads(outputs['plan'])
                self.assertEqual('custody_compile_proof_version' in plan, version == 'new')
                self.assertEqual('custody_compile_proof_version' in projected, version == 'new')
                values = {'needs.select.result': 'success',
                          'fromJSON(needs.select.outputs.plan).custody_execution_version': projected.get('custody_execution_version', 0),
                          'fromJSON(needs.select.outputs.plan).custody_compile_proof_version': projected.get('custody_compile_proof_version', 0)}
                job_runs = effective_job_condition(job_condition, values, ['skipped', 'success'])
                step_runs = job_runs and effective_job_condition(step_condition, values, [])
                self.assertEqual(job_runs, version in ('new', 'old-custody'))
                self.assertEqual(step_runs, version == 'new')

    def test_current_workflow_preserves_the_fixed_historical_custody_source_protocol(self):
        legacy_capable = dict(CONFIG, custody_execution_version=1, custody_compile_proof_version=1,
                              custody_policy_tests_version=1, custody_node_shards_version=2)
        legacy_capable.pop('workspace_support_direct_gate_version', None)
        old = {key: value for key, value in legacy_capable.items()
               if key not in ('custody_compile_proof_version', 'custody_policy_tests_version')}
        for event in ('workflow_dispatch', 'pull_request', 'push'):
            with self.subTest(event=event), tempfile.TemporaryDirectory() as directory:
                result, outputs, plan, policy, source = self.run_selection(
                    directory, event=event, source_ref='candidate' if event == 'workflow_dispatch' else '',
                    candidate=event != 'workflow_dispatch', ref='refs/heads/main' if event == 'push' else None,
                    policy_config=legacy_capable, candidate_config=old)
                self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                self.assertNotEqual(source, policy)
                self.assertEqual(plan['custody_execution_version'], 1)
                self.assertNotIn('custody_compile_proof_version', plan)
                projected = json.loads(outputs['plan'])
                self.assertNotIn('custody_compile_proof_version', projected)
                self.assertEqual(plan['tested_sha'], source)
                self.assertIn('oasis7_custody_worker', plan['workspace_support_profiles'])

    def test_manual_and_scheduled_selection_expands_complete_fixed_inventory(self):
        for event in ('workflow_dispatch', 'schedule'):
            with self.subTest(event=event), tempfile.TemporaryDirectory() as directory:
                result, values, plan, policy, source = self.run_selection(directory, event=event)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(set(plan['groups']), set(CONFIG['groups']))
                self.assertEqual(plan['scope'], 'explicit-full')
                self.assertEqual(plan['source_sha'], source)
                self.assertEqual(plan['tested_sha'], source)
                self.assertEqual(plan['policy_sha'], policy)
                self.assertEqual(values['trusted_source'], 'true')
                for group in CONFIG['groups']:
                    self.assertEqual(values['run_' + group], 'true')
        with tempfile.TemporaryDirectory() as directory:
            result, values, plan, policy, source = self.run_selection(directory, event='workflow_dispatch', source_ref='candidate')
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertNotEqual(source, policy)
            self.assertEqual(plan['source_sha'], source)
            self.assertEqual(plan['policy_sha'], policy)
            self.assertEqual(values['trusted_source'], 'false')

    def test_selection_artifacts_preserve_the_select_attempt_on_failed_job_reruns(self):
        job = JOBS['select']
        step = job.split('      - id: selection-artifacts\n', 1)[1].split('      - uses:', 1)[0]
        script = textwrap.dedent(step.split('        run: |\n', 1)[1])
        attempts = []
        with tempfile.TemporaryDirectory() as directory:
            for attempt in (1, 2):
                output = Path(directory) / str(attempt)
                result = subprocess.run(['bash', '-euo', 'pipefail', '-c', script],
                    env=dict(os.environ, GITHUB_RUN_ID='123', GITHUB_RUN_ATTEMPT=str(attempt),
                             GITHUB_OUTPUT=str(output)), text=True, capture_output=True)
                self.assertEqual(result.returncode, 0, result.stderr)
                names = dict(line.split('=', 1) for line in output.read_text().splitlines())
                self.assertEqual(names, {'selection_artifact': f'ci-selection-123-{attempt}',
                                         'authority_artifact': f'ci-authority-123-{attempt}'})
                attempts.append(names)
        self.assertNotEqual(attempts[0], attempts[1])
        for kind in ('selection', 'authority'):
            output = kind + '_artifact'
            self.assertIn(f'{output}: ${{{{ steps.selection-artifacts.outputs.{output} }}}}', job)
            self.assertIn(f'name: ${{{{ steps.selection-artifacts.outputs.{output} }}}}', job)
            consumers = [body for name, body in JOBS.items() if name != 'select'
                         and f'needs.select.outputs.{output}' in body]
            self.assertTrue(consumers)
            for body in consumers:
                self.assertIn(f'name: ${{{{ needs.select.outputs.{output} }}}}', body)
                self.assertNotIn(f'name: ci-{kind}-${{{{ github.run_id }}}}', body)

    def test_shared_cache_workflow_is_independent_of_full_validation(self):
        workflow, writer = self.shared_cache_workflow()
        self.assertNotIn('shared-native-cache', JOBS)
        self.assertNotIn('needs:', writer.split('    steps:', 1)[0])
        self.assertIn('group: ci-shared-native-cache-main', workflow)
        self.assertIn('cancel-in-progress: false', workflow)
        self.assertNotIn('ci-shared-native-cache-main', WORKFLOW)
        writer_timeout = re.search(r'^    timeout-minutes:\s*(\d+)\s*$', writer.split('    steps:', 1)[0], re.M)
        self.assertIsNotNone(writer_timeout)
        self.assertGreater(int(writer_timeout.group(1)), 0)
        self.assertLessEqual(int(writer_timeout.group(1)), 150)
        self.assertIn('permissions:\n      contents: read\n      actions: read', writer)
        self.assertNotIn('actions: write', workflow)
        for forbidden in ('ci-full-result.py', 'ci-required-result.py',
                          'actions/upload-artifact@', 'actions/download-artifact@',
                          'full-extended', 'OASIS7_CI_TEST_BUNDLE'):
            self.assertNotIn(forbidden, workflow)
        for name in ('required-gate', 'explicit-full-gate'):
            self.assertNotIn('shared-native-cache', JOBS[name])
            self.assertNotIn('rust-cache.yml', JOBS[name])
        for name in ('full-regression', 'candidate-full-regression'):
            job = JOBS[name]
            self.assertIn('timeout-minutes: 210', job)
            entrypoint = './scripts/ci-tests.sh full-extended'
            if name == 'full-regression' and 'scripts/ci-trusted-full-regression.py full "$SOURCE_SHA"' in job:
                self.assertIn("['./scripts/ci-tests.sh', 'full-extended']",
                              (ROOT / 'scripts/ci-trusted-full-regression.py').read_text())
                entrypoint = 'scripts/ci-trusted-full-regression.py full "$SOURCE_SHA"'
            self.assertIn(entrypoint, job)
            self.assertNotIn('cache-native', job)
            self.assertNotIn('dependency-cache-tools', job)
            cache = next(step for step in re.findall(r'^      - .*?(?=^      - |\Z)', job, re.M | re.S)
                         if 'uses: Swatinem/rust-cache@' in step)
            self.assertIn('save-if: false', cache)
            self.assertNotIn('GH_TOKEN:', cache)
            full = next(step for step in re.findall(r'^      - .*?(?=^      - |\Z)', job, re.M | re.S)
                        if entrypoint in step)
            self.assertNotIn('if:', full)
            self.assertNotIn('continue-on-error:', full)

    def test_dependency_admission_is_local_to_the_two_cache_actions(self):
        admission = (ROOT / 'scripts/ci-rust-cache-admission.py').read_text()
        revision = re.search(r"^ACTION_REVISION = '([0-9a-f]{40})'$", admission, re.M)
        self.assertIsNotNone(revision)
        # Tar-before-reserve behavior was verified on this exact upstream
        # package. A pin upgrade must review/update the guard and this contract.
        self.assertEqual(revision[1], 'd1527c8f67ef9af357290475d4af4b0fab8f12e0')
        ecs_text = (ROOT / '.github/workflows/ecs-packages.yml').read_text()
        ecs_jobs = workflow_jobs(ecs_text)
        cache_workflow, shared_writer = self.shared_cache_workflow()
        dual_ecs = 'OASIS7_DEPENDENCY_CACHE_FAMILY: ecs-image\n' in ecs_jobs['native-trusted']
        lanes = [
            (shared_writer, 'shared-native', 'CI_CACHE_REPOSITORY_BUDGET_BYTES'),
            (ecs_jobs['native'], 'ecs', 'ECS_CACHE_REPOSITORY_BUDGET_BYTES'),
        ]
        if not dual_ecs:
            lanes.append((ecs_jobs['native-trusted'], 'ecs', 'ECS_CACHE_REPOSITORY_BUDGET_BYTES'))
        else:
            steps = re.findall(r'^      - .*?(?=^      - |\Z)', ecs_jobs['native-trusted'], re.M | re.S)
            install, = [step for step in steps if 'id: image-dependency-cache-tools\n' in step]
            _, image = [step for step in steps if 'uses: Swatinem/rust-cache@' in step]
            self.assertIn("if: github.job == 'native-trusted'\n", install)
            self.assertIn('continue-on-error: true', install)
            self.assertIn('run: /usr/bin/python3 scripts/ci-rust-cache-admission.py install-release\n', install)
            self.assertNotIn('GITHUB_PATH', install)
            self.assertIn('uses: Swatinem/rust-cache@' + revision[1] + ' # v2', image)
            self.assertLess(steps.index(install), steps.index(image))
            self.assertIn('OASIS7_DEPENDENCY_CACHE_BUDGET_BYTES: ${{ env.ECS_CACHE_REPOSITORY_BUDGET_BYTES }}', image)
            self.assertIn('GH_TOKEN: ${{ github.token }}', image)
            self.assertIn('cache-on-failure: false', image)
            self.assertNotIn('lookup-only:', image)
            for step in steps:
                if step != image:
                    self.assertNotIn('steps.image-dependency-cache-tools.outputs.node_options', step)
                    self.assertNotIn('steps.image-dependency-cache-tools.outputs.python', step)
        for job, family, budget in lanes:
            with self.subTest(family=family, trusted='cache-mode: write' in job):
                steps = re.findall(r'^      - .*?(?=^      - |\Z)', job, re.M | re.S)
                installs = [step for step in steps if 'id: dependency-cache-tools\n' in step]
                self.assertEqual(len(installs), 1)
                install = installs[0]
                self.assertIn('continue-on-error: true', install)
                self.assertIn('run: python3 scripts/ci-rust-cache-admission.py install', install)
                self.assertNotIn('GITHUB_PATH', install)
                cache = next(step for step in steps if 'uses: Swatinem/rust-cache@' in step)
                self.assertIn('uses: Swatinem/rust-cache@' + revision[1] + ' # v2', cache)
                self.assertLess(steps.index(install), steps.index(cache))
                self.assertIn("steps.dependency-cache-tools.outcome == 'success'", cache)
                self.assertIn('PATH: ${{ steps.dependency-cache-tools.outputs.path }}', cache)
                self.assertIn('OASIS7_DEPENDENCY_CACHE_FAMILY: ' + family, cache)
                self.assertIn('OASIS7_DEPENDENCY_CACHE_BUDGET_BYTES: ${{ env.' + budget + ' }}', cache)
                self.assertIn('GH_TOKEN: ${{ github.token }}', cache)
                self.assertIn('cache-on-failure: false', cache)
                self.assertNotIn('lookup-only:', cache)
                condition = re.search(r'^        if: (.*)$', cache, re.M).group(1)
                for outcome in ('success', 'failure', 'skipped', 'cancelled'):
                    for has_toolchains in (False, True):
                        predicate = condition.replace('steps.dependency-cache-tools.outcome', repr(outcome))
                        if dual_ecs and family == 'ecs':
                            predicate = predicate.replace('github.job', repr('native'))
                        predicate = predicate.replace("hashFiles('scripts/ci-rust-cache-toolchains.py')",
                                                      repr('hash' if has_toolchains else ''))
                        allowed = eval(predicate.replace('&&', ' and '), {'__builtins__': {}}, {})
                        self.assertEqual(allowed, outcome == 'success' and (family != 'ecs' or has_toolchains))
                for step in steps:
                    if step != cache:
                        self.assertNotIn('steps.dependency-cache-tools.outputs.path', step)
                        if not (dual_ecs and family == 'ecs' and 'OASIS7_DEPENDENCY_CACHE_FAMILY: ecs-image\n' in step):
                            self.assertNotIn('OASIS7_DEPENDENCY_CACHE_', step)
                self.assertNotIn('steps.dependency-cache-tools.outputs.path', job.split('    steps:', 1)[0])
        self.assertIn('permissions:\n      contents: read\n      actions: read', shared_writer)
        # Native uses one optional installer. The migrated Web lane also has
        # its optional Node installer and restore/save action, checked below.
        migrated = 'web-wasm-cache' in workflow_jobs(cache_workflow)
        self.assertEqual(cache_workflow.count('continue-on-error:'), 3 if migrated else 1)
        self.assertNotIn('dependency-cache-tools', JOBS['full-regression'])
        self.assertNotIn('dependency-cache-tools', JOBS['candidate-full-regression'])
        self.assertNotIn('dependency-cache-tools', ecs_jobs['web'])
        governance = (ROOT / 'scripts/ci-tests.sh').read_text().split(
            'run_workflow_governance_baseline_contract_tests() {', 1)[1].split('\n}', 1)[0]
        self.assertEqual(CONFIG['workflow_governance_python_tests'].count('scripts/ci-rust-cache-admission.test.py'), 1)

    def test_dependency_save_inputs_reject_step_only_status_functions(self):
        # GitHub permits status functions in step `if`, not action inputs.
        # The `job` context is available in `with`, so save-if uses job.status.
        ecs_jobs = workflow_jobs((ROOT / '.github/workflows/ecs-packages.yml').read_text())
        _, shared_writer = self.shared_cache_workflow()
        for name, job in (('shared-native-cache', shared_writer),
                          ('native', ecs_jobs['native']), ('native-trusted', ecs_jobs['native-trusted'])):
            caches = [step for step in re.findall(r'^      - .*?(?=^      - |\Z)', job, re.M | re.S)
                      if 'uses: Swatinem/rust-cache@' in step]
            if name in ('native', 'native-trusted') and len(caches) == 2:
                host, image = caches
                self.assertIn('save-if: false\n', host)
                caches = [image]
            self.assertEqual(len(caches), 1, name)
            save = re.search(r'          save-if: \$\{\{ (.*?) \}\}', caches[0])
            self.assertIsNotNone(save, name)
            with self.subTest(job=name):
                self.assertNotRegex(save.group(1), r'(?i)\b(?:always|cancelled|failure|success)\s*\(')

    def test_shared_cache_is_populated_completely_before_successful_save(self):
        _, writer = self.shared_cache_workflow()
        steps = re.findall(r'^      - .*?(?=^      - |\Z)', writer, re.M | re.S)
        cache = next(step for step in steps if 'uses: Swatinem/rust-cache@' in step)
        self.assertIn('id: native-cache', cache)
        self.assertIn('cache-on-failure: false', cache)
        self.assertNotIn('save-if: false', cache)
        completion = next(step for step in steps if 'name: Complete shared native cache\n' in step)
        self.assertIn('./scripts/ci-tests.sh cache-native', completion)
        self.assertNotIn('continue-on-error', completion)
        self.assertLess(steps.index(cache), steps.index(completion))
        condition = re.search(r'^        if: (.*)$', completion, re.M)[1]
        for hit in ('true', 'false', '', 'unknown'):
            with self.subTest(cache_hit=hit):
                expression = condition.replace('steps.native-cache.outputs.cache-hit', repr(hit))
                self.assertEqual(eval(expression, {'__builtins__': {}}, {}), hit != 'true')
        self.assertNotIn('full-extended', writer)
        for name, job in JOBS.items():
            if 'shared-key: ci-shared-native-trusted-v4' in job:
                self.assertIn('save-if: false', job, name)
                # Full validation still writes tool caches; the ordinary producer
                # writes its workspace snapshot, not shared dependencies.
                if name not in ('full-regression', 'oasis7-test-build-trusted'):
                    self.assertNotIn('cache-mode: write', job, name)

    def test_shared_cache_normalization_runs_after_install_before_restore(self):
        _, shared_writer = self.shared_cache_workflow()
        for name, job in (*JOBS.items(), ('shared-native-cache', shared_writer)):
            if re.search(r'shared-key: (?:ci-shared-native-trusted-v4|ci-formal-(?:debug|endpoint)-deps-trusted-v1)\n', job):
                prepare = job.index('python3 scripts/ci-rust-cache-toolchains.py')
                self.assertIn('"$RUST_TOOLCHAIN" "$OASIS7_WASM_TOOLCHAIN"', job, name)
                self.assertLess(job.index('rustup default'), prepare, name)
                self.assertLess(prepare, job.index('uses: Swatinem/rust-cache@'), name)

    def test_platform_product_jobs_still_gate(self):
        self.assertIn('runs-on: windows-2022', JOBS['windows-rollout'])
        self.assertIn('p2p-public-testnet-package-rollout.test.sh', JOBS['windows-rollout'])
        self.assertIn('testnet-packages-macos-arm64-contract.test.sh', JOBS['baseline'])
        self.assertIn('testnet-packages-macos-arm64-contract.test.sh', JOBS['macos-package-contract'])
        self.assertIn('p2p-public-testnet-fleet-health.test.py', JOBS['fleet-health'])
        self.assertIn('p2p-public-testnet-package-rollout.test.sh', JOBS['fleet-health'])
        self.assertIn('fromJSON(\'["ubuntu-24.04","windows-2022","macos-15"]\')', JOBS['fleet-health'])
        self.assertIn('--host-smoke', JOBS['fleet-health'])
        self.assertEqual(WORKFLOW.count('testnet-packages-macos-arm64-contract.test.sh'), 2)
        if 'scripts/ci-trusted-full-regression.py full "$SOURCE_SHA"' in JOBS['full-regression']:
            self.assertIn("['./scripts/ci-tests.sh', 'full-extended']",
                          (ROOT / 'scripts/ci-trusted-full-regression.py').read_text())
        else:
            self.assertIn('./scripts/ci-tests.sh full', JOBS['full-regression'])
        self.assertIn('package-newapi-bridge-service.sh', JOBS['newapi-bridge-package'])
        report = JOBS['viewer-performance-report']
        self.assertIn('rustup default', report)
        self.assertIn('rustup target add wasm32-unknown-unknown', report)
        self.assertIn('path: output/ci/viewer-performance/**', report)
        driver = (ROOT / 'scripts/ci-tests.sh').read_text()
        self.assertIn('viewer-performance-report-only-contract.test.sh', driver)
        full = driver.split('run_full_required_superset() {', 1)[1].split('\n}', 1)[0]
        self.assertIn('testnet-packages-macos-arm64-contract.test.sh', full)

    def test_main_push_cache_capability_flows_from_trusted_selection_to_the_dag(self):
        field = 'ordinary_main_fast_cache_writer_version'
        jobs = ('oasis7-test-build-candidate', 'oasis7-test-build-trusted',
                'cache-bootstrap-maintenance', 'cache-maintenance')
        conditions = {name: re.search(r'^    if: (.*?)(?=^    [a-z-]+:|\Z)',
                                      JOBS[name], re.M | re.S)[1] for name in jobs}
        for enabled in (False, True):
            config = {key: value for key, value in CONFIG.items() if key != field}
            if enabled:
                config[field] = 1
            with self.subTest(enabled=enabled), tempfile.TemporaryDirectory() as directory:
                result, outputs, plan, policy, source = self.run_selection(
                    directory, event='push', candidate=True, ref='refs/heads/main', policy_config=config)
                self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                self.assertEqual((plan['scope'], plan['execution_profile']), ('full', 'main-fast'))
                self.assertEqual((plan['policy_sha'], plan['source_sha'], plan['tested_sha']),
                                 (policy, source, source))
                self.assertNotEqual(policy, source)
                self.assertEqual(plan.get(field), 1 if enabled else None)
                self.assertEqual(outputs['trusted_source'], 'true')
                projected = json.loads(outputs['plan'])
                self.assertEqual(projected.get(field), plan.get(field))
                values = {'needs.select.result': 'success', 'github.event_name': 'push',
                          'github.ref': 'refs/heads/main', 'github.sha': source,
                          'github.workflow_sha': source, 'inputs.run_mode': '',
                          'needs.oasis7-test-build-trusted.result': 'success' if enabled else 'skipped',
                          'needs.formal-debug-build-trusted.result': 'skipped',
                          'needs.formal-endpoint-build-trusted.result': 'skipped'}
                values.update({f'needs.select.outputs.{key}': value for key, value in outputs.items()})
                for name in re.findall(r'fromJSON\(needs\.select\.outputs\.plan\)\.([A-Za-z0-9_]+)',
                                       '\n'.join(conditions.values())):
                    values[f'fromJSON(needs.select.outputs.plan).{name}'] = projected.get(name)
                for name, condition in conditions.items():
                    self.assertEqual(effective_job_condition(condition, values, ['skipped', 'success']),
                                     not enabled if name == 'oasis7-test-build-candidate' else enabled, name)
                for maintenance in ('success', 'failure', 'skipped'):
                    self.assertEqual(effective_job_condition(conditions['oasis7-test-build-trusted'], values,
                                                            ['success', maintenance]), enabled)


if __name__ == '__main__':
    unittest.main()
