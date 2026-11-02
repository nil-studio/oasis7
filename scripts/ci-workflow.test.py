#!/usr/bin/env python3
"""Regress the final DAG and execution boundaries, without Actions credentials."""
import json
import os
import subprocess
import sys
import tempfile
import textwrap
from pathlib import Path
import re
import unittest

ROOT = Path(__file__).resolve().parent.parent
WORKFLOW = (ROOT / '.github/workflows/rust.yml').read_text()
CONFIG = json.loads((ROOT / 'scripts/ci-required-scope.json').read_text())
def workflow_jobs(workflow):
    jobs = dict(re.findall(r'^  ([a-z][a-z0-9-]*):\n(.*?)(?=^  [a-z][a-z0-9-]*:|\Z)',
                          workflow.split('\njobs:\n', 1)[1], re.M | re.S))
    anchors = {}
    for job in jobs.values():
        match = re.search(r'^    steps: &([a-z0-9-]+)\n(.*)', job, re.M | re.S)
        if match:
            anchors[match[1]] = match[2]
    return {name: re.sub(r'^    steps: &[a-z0-9-]+\n', '    steps:\n',
                        re.sub(r'^    steps: \*([a-z0-9-]+)\n',
                               lambda match: '    steps:\n' + anchors[match[1]], job, flags=re.M), flags=re.M)
            for name, job in jobs.items()}


JOBS = workflow_jobs(WORKFLOW)


def without_optional_dependency_cache_install(workflow):
    # Only this exact cache setup in the trusted writer may fail into a cold
    # build. A renamed command, another step or another job is not exempt.
    full = re.search(r'^  full-regression:\n.*?(?=^  [a-z][a-z0-9-]*:|\Z)', workflow, re.M | re.S)
    if not full:
        return workflow
    installer = ('      - name: Install dependency cache admission\n'
                 '        id: dependency-cache-tools\n'
                 '        continue-on-error: true\n'
                 '        run: python3 scripts/ci-rust-cache-admission.py install\n')
    return workflow[:full.start()] + full[0].replace(installer, '', 1) + workflow[full.end():]


def validate_candidate_graph(workflow, config):
    jobs = dict(re.findall(r'^  ([a-z][a-z0-9-]*):\n(.*?)(?=^  [a-z][a-z0-9-]*:|\Z)',
                           workflow.split('\njobs:\n', 1)[1], re.M | re.S))
    gate = jobs['required-gate']
    dependencies = re.search(r'    needs: \[(.*)\]', gate).group(1).split(', ')
    expected = {'select', *(g.replace('_', '-') for g in config['groups'])}
    if set(dependencies) != expected or len(dependencies) != len(expected):
        raise ValueError('gate dependencies must exactly cover candidate groups')
    if 'always()' not in gate or 'continue-on-error:' in without_optional_dependency_cache_install(workflow):
        raise ValueError('failure propagation must remain intact')
    fleet = jobs['fleet-health']
    if ('os: [ubuntu-24.04, windows-2022, macos-15]' not in fleet
            or 'runs-on: ${{ matrix.os }}' not in fleet
            or 'fail-fast: false' not in fleet or 'include:' in fleet or 'exclude:' in fleet
            or "if: runner.os != 'Windows'" not in fleet or '--host-smoke' not in fleet):
        raise ValueError('fleet must run all three hosts and POSIX cleanup smoke')
    for group in config['groups']:
        job = jobs[group.replace('_', '-')]
        if group == 'formal_process':
            if 'always()' not in job or 'ci-formal-plan.py" --selected' not in job:
                raise ValueError('formal aggregation must execute and validate results')
            continue
        if ("needs.select.result == 'success'" not in job
                or "fromJSON(needs.select.outputs.plan).scope == 'full'" not in job):
            raise ValueError('group requires successful select and full migration fallback')
        if group != 'fleet_health' and 'strategy:' in job:
            raise ValueError('ordinary groups must not have singleton matrices')
        if group == 'command_center' and 'uses: ./.github/workflows/command-center.yml' not in job:
            raise ValueError('Command Center checks must execute through the reusable workflow')
        if 'ci-authority' in job:
            raise ValueError('execution must use candidate checkout')
    return True


class Workflow(unittest.TestCase):
    def run_selection(self, directory, *, event='schedule', legacy=False, candidate=False, source_ref='', real_receipt=False):
        """Execute the real workflow shell against immutable local Git fixtures."""
        repo = Path(directory) / 'repo'
        repo.mkdir()
        subprocess.run(['git', 'init', '-q', '-b', 'main', str(repo)], check=True)
        def git(*args):
            return subprocess.check_output(['git', '-C', str(repo), *args], text=True).strip()
        git('config', 'user.name', 'CI fixture')
        git('config', 'user.email', 'fixture@example.invalid')
        (repo / 'scripts').mkdir()
        files = ('plan-rust-required-scope.py', 'ci-required-scope.json', 'ci-required-result.py',
                 'ci-formal-plan.py', 'ci-formal-process-suites.json')
        for name in files:
            (repo / 'scripts' / name).write_bytes((ROOT / 'scripts' / name).read_bytes())
        if real_receipt:
            import runpy
            policy_files = runpy.run_path(str(ROOT / 'scripts/ci-full-result.py'))['POLICY_FILES']
            for path in policy_files:
                destination = repo / path
                destination.parent.mkdir(parents=True, exist_ok=True)
                destination.write_bytes((ROOT / path).read_bytes())
        else:
            (repo / 'scripts/ci-full-result.py').write_text('# producer is not executed by selection\n')
        if legacy:
            (repo / 'scripts/plan-rust-required-scope.py').write_text('''import argparse,json,pathlib
p=argparse.ArgumentParser()
for key in ('base-ref','head-ref','test-ref','github-output','output'): p.add_argument('--'+key)
a=p.parse_args()
plan={'scope':'full','groups':json.loads(pathlib.Path(__file__).with_name('ci-required-scope.json').read_text())['groups']}
pathlib.Path(a.output).write_text(json.dumps(plan))
with pathlib.Path(a.github_output).open('a') as f: f.write('plan='+json.dumps(plan)+'\\n')
''')
        git('add', '.')
        git('commit', '-qm', 'trusted policy')
        policy = git('rev-parse', 'HEAD')
        git('remote', 'add', 'origin', str(repo))
        git('checkout', '-qb', 'candidate')
        (repo / 'README.md').write_text('candidate behavior\n')
        # If the shell accidentally runs candidate policy instead of BASE, fail.
        (repo / 'scripts/plan-rust-required-scope.py').write_text("raise SystemExit('untrusted candidate planner executed')\n")
        git('add', '.')
        git('commit', '-qm', 'candidate')
        source = git('rev-parse', 'HEAD')
        if not candidate and not source_ref:
            git('checkout', '-q', 'main')
            source = policy
        requested = source if source_ref == 'candidate' else source_ref
        step = JOBS['select'].split('      - id: select\n', 1)[1].split('      - id: formal-select\n', 1)[0]
        script = textwrap.dedent(step.split('        run: |\n', 1)[1])
        output = Path(directory) / 'github-output'
        environment = dict(os.environ, EVENT_NAME=event, SOURCE_REF=requested,
            SOURCE_SHA=source if candidate else policy, BASE_SHA=policy, WORKFLOW_SHA=source if candidate else policy,
            GITHUB_SHA=source if candidate else policy,
            GITHUB_REF='refs/heads/candidate' if candidate and event != 'pull_request' else
                       'refs/pull/1/merge' if event == 'pull_request' else 'refs/heads/main',
            RUNNER_TEMP=str(directory), GITHUB_OUTPUT=str(output),
            GITHUB_STEP_SUMMARY=str(Path(directory) / 'summary'))
        result = subprocess.run(['bash', '-c', script], cwd=repo, env=environment,
                                text=True, capture_output=True)
        values = dict(line.split('=', 1) for line in output.read_text().splitlines()) if output.exists() else {}
        plan = json.loads(values['plan']) if 'plan' in values else None
        return result, values, plan, policy, source

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

    def test_old_policy_cannot_be_replaced_by_candidate_fast_policy(self):
        with tempfile.TemporaryDirectory() as directory:
            result, values, plan, policy, source = self.run_selection(directory, event='pull_request', legacy=True, candidate=True)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertNotIn('fast_plan_version', plan)
            self.assertEqual(set(plan['groups']), set(CONFIG['groups']))
            self.assertEqual(values['policy_sha'], policy)
        with tempfile.TemporaryDirectory() as directory:
            result, values, plan, policy, source = self.run_selection(directory, legacy=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertIsNone(plan)
        with tempfile.TemporaryDirectory() as directory:
            result, values, plan, policy, source = self.run_selection(directory, event='workflow_dispatch', legacy=True, candidate=True)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertTrue(plan['bootstrap_full'])
            self.assertNotIn('fast_plan_version', plan)
            self.assertEqual(set(plan['groups']), set(CONFIG['groups']))
            self.assertEqual(values['trusted_source'], 'false')

    def test_candidate_source_input_requires_trusted_main_dispatch(self):
        for event, candidate in (('schedule', False), ('workflow_dispatch', True)):
            with self.subTest(event=event), tempfile.TemporaryDirectory() as directory:
                result, values, plan, policy, source = self.run_selection(
                    directory, event=event, candidate=candidate, source_ref='candidate')
                self.assertNotEqual(result.returncode, 0)
                self.assertIsNone(plan)

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

    def test_explicit_full_gate_covers_every_group_and_records_only_success(self):
        gate = JOBS['explicit-full-gate']
        dependencies = re.search(r'    needs: \[(.*)\]', gate).group(1).split(', ')
        self.assertEqual(set(dependencies), {'select', 'oasis7-test-build', 'full-regression',
            'candidate-full-regression', *(group.replace('_', '-') for group in CONFIG['groups'])})
        self.assertEqual(len(dependencies), len(set(dependencies)))
        self.assertIn('always()', gate)
        self.assertIn('ci-required-result.py" --full', gate)
        self.assertIn('ci-full-result.py" produce', gate)
        self.assertIn('name: ci-full-result-${{ github.run_id }}-${{ github.run_attempt }}', gate)
        self.assertNotIn('if: always()', gate.split('ci-full-result.py" produce', 1)[1])
        self.assertIn('if-no-files-found: error', gate)

    def test_full_gate_produces_verifiable_policy_bound_receipt_from_real_authority(self):
        import runpy
        helper = runpy.run_path(str(ROOT / 'scripts/ci-full-result.py'))
        gate = JOBS['explicit-full-gate']
        step = gate.split('      - name: Require complete full inventory and exact source\n', 1)[1]
        script = textwrap.dedent(step.split('        run: |\n', 1)[1].split('      - uses:', 1)[0])
        for event, requested in (('schedule', ''), ('workflow_dispatch', 'candidate')):
            with self.subTest(event=event), tempfile.TemporaryDirectory() as directory:
                result, outputs, plan, policy, source = self.run_selection(
                    directory, event=event, source_ref=requested, real_receipt=True)
                self.assertEqual(result.returncode, 0, result.stderr)
                repo = Path(directory) / 'repo'
                subprocess.run(['git', '-C', str(repo), 'checkout', '-q', policy], check=True)
                needs = {group.replace('_', '-'): {'result': 'success'} for group in CONFIG['groups']}
                needs.update(select={'result': 'success'}, **{'oasis7-test-build': {'result': 'success'}})
                trusted = source == policy
                needs['full-regression'] = {'result': 'success' if trusted else 'skipped'}
                needs['candidate-full-regression'] = {'result': 'skipped' if trusted else 'success'}
                environment = dict(os.environ, RUNNER_TEMP=directory, GITHUB_WORKSPACE=str(repo),
                    GITHUB_REPOSITORY='fixture/oasis7', GITHUB_EVENT_NAME=event,
                    GITHUB_REF='refs/heads/main', GITHUB_SHA=policy, GITHUB_WORKFLOW_SHA=policy,
                    GITHUB_WORKFLOW_REF='fixture/oasis7/.github/workflows/rust.yml@refs/heads/main',
                    GITHUB_RUN_ID='123', GITHUB_RUN_ATTEMPT='2',
                    EVENT_NAME=event, REF='refs/heads/main', WORKFLOW_SHA=policy, SOURCE_SHA=source,
                    RUN_MODE='full' if event == 'workflow_dispatch' else '',
                    PLAN=json.dumps(plan), NEEDS=json.dumps(needs))
                result = subprocess.run(['bash', '-euo', 'pipefail', '-c', script],
                    cwd=repo, env=environment, text=True, capture_output=True)
                self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                receipt_path = Path(directory) / 'full-result.json'
                receipt = json.loads(receipt_path.read_text())
                self.assertEqual(receipt['context']['run_attempt'], 2)
                self.assertEqual(receipt['plan']['source_sha'], source)
                self.assertEqual(receipt['plan']['policy_sha'], policy)
                digest = helper['definition_digest'](repo, policy)
                self.assertEqual(receipt['definition_sha256'], digest)
                run = dict(id=123, run_attempt=2, workflow_id=10, path='.github/workflows/rust.yml',
                           head_branch='main', head_sha=policy, event=event, status='completed', conclusion='success')
                jobs = [dict(id=1, name='explicit-full-gate', run_attempt=2, status='completed', conclusion='success')]
                verified = helper['verify_receipt'](receipt, run, jobs, source=source,
                    repository='fixture/oasis7', workflow_id=10, config=CONFIG, digest=digest)
                self.assertEqual(verified, receipt)
                # The same exact gate shell may never publish success after a
                # selected native platform group fails, skips or is cancelled.
                for result_name in ('failure', 'skipped', 'cancelled'):
                    receipt_path.unlink(missing_ok=True)
                    failed = dict(needs, **{'fleet-health': {'result': result_name}})
                    result = subprocess.run(['bash', '-euo', 'pipefail', '-c', script],
                        cwd=repo, env=dict(environment, NEEDS=json.dumps(failed)), text=True, capture_output=True)
                    self.assertNotEqual(result.returncode, 0)
                    self.assertFalse(receipt_path.exists())

    def test_command_center_is_reusable_and_required(self):
        workflow = (ROOT / '.github/workflows/command-center.yml').read_text()
        self.assertIn('  workflow_call:', workflow)
        self.assertNotIn('  pull_request:', workflow)
        self.assertNotIn('  push:', workflow)
        self.assertIn('uses: ./.github/workflows/command-center.yml', JOBS['command-center'])
        self.assertIn('run_command_center: ${{ steps.select.outputs.run_command_center }}', JOBS['select'])
        for command in ('cargo test --locked', '--example export_types', 'npm test',
                        'npm run build', 'cargo clippy --locked', 'desktop:build -- --debug'):
            self.assertIn(command, workflow)
        self.assertNotIn('continue-on-error:', workflow)
        for package in ('oasis7_context', 'oasis7_local_signer', 'oasis7_secure_envelope'):
            self.assertIn('command_center', CONFIG['package_groups'][package])

    def test_corpus_data_uses_baseline_and_implementation_uses_platforms(self):
        workflow = (ROOT / '.github/workflows/document-corpus-platform.yml').read_text()
        for path in ('doc/.governance/document-corpus-inventory.json',
                     'doc/.governance/document-corpus/**', 'doc/testing/evidence/inventory.json'):
            self.assertNotIn('      - ' + path + '\n', workflow)
        for path in ('scripts/document_corpus.py', 'scripts/document_evidence_policy.py',
                     'scripts/document-corpus-inventory*', 'scripts/doc-evidence-inventory-check*',
                     'scripts/fixtures/document-corpus-v3/**', '.gitattributes', '.gitignore'):
            self.assertEqual(workflow.count('      - ' + path + '\n'), 2)
        for os in ('ubuntu-24.04', 'macos-latest', 'windows-latest'):
            self.assertIn('- ' + os, workflow)
        driver = (ROOT / 'scripts/ci-tests.sh').read_text()
        self.assertIn('baseline) run_required_gate_checks', driver)
        baseline = driver.split('run_required_gate_checks() {', 1)[1].split('\n}', 1)[0]
        self.assertIn('run_product_doc_governance_check', baseline)
        governance = (ROOT / 'scripts/doc-governance-check.sh').read_text()
        self.assertIn('scripts/document-corpus-inventory-check.py', governance)
        checker = (ROOT / 'scripts/document-corpus-inventory-check.py').read_text()
        self.assertIn('dc.validate_evidence(view, model)', checker)
        self.assertIn('evidence_policy.validate_evidence_policy(view, model, baseline)', checker)

    def test_baseline_document_range_binds_ordinary_event_and_fixed_full_source(self):
        baseline = JOBS['baseline']
        self.assertIn('ref: ${{ needs.select.outputs.tested_sha }}', baseline)
        self.assertIn('TESTED_SHA: ${{ needs.select.outputs.tested_sha }}', baseline)
        self.assertIn('CI_PROFILE: ${{ fromJSON(needs.select.outputs.plan).execution_profile }}', baseline)
        self.assertIn('OASIS7_CI_PROFILE: ${{ fromJSON(needs.select.outputs.plan).execution_profile }}', baseline)
        step = baseline.split('      - name: Resolve tested product-document range\n', 1)[1]
        step = step.split('      - name: Execute selected group\n', 1)[0]
        script = textwrap.dedent(step.split('        run: |\n', 1)[1])
        self.assertLess(script.index('test "$TESTED_SHA" = "$GITHUB_SHA"'),
                        script.index('python3 scripts/resolve-product-doc-ci-range.py'))
        self.assertIn('--tested-oid "$GITHUB_SHA" >>"$GITHUB_ENV"', script)
        self.assertNotRegex(script, r'\bGITHUB_SHA\s*=')
        with tempfile.TemporaryDirectory() as directory:
            repo = Path(directory) / 'repo'
            repo.mkdir()
            def git(*args):
                return subprocess.check_output(['git', '-C', str(repo), *args],
                                               text=True, stderr=subprocess.PIPE).strip()
            def commit(message):
                git('add', '.')
                git('commit', '-qm', message)
                return git('rev-parse', 'HEAD')
            git('init', '-q', '-b', 'main')
            git('config', 'user.name', 'Range fixture')
            git('config', 'user.email', 'fixture@example.invalid')
            (repo / 'scripts').mkdir()
            helper = 'scripts/resolve-product-doc-ci-range.py'
            (repo / helper).write_bytes((ROOT / helper).read_bytes())
            (repo / 'README.md').write_text('# Base\n')
            base = commit('event base')
            git('checkout', '-qb', 'feature')
            (repo / 'candidate.md').write_text('# Candidate\n')
            source = commit('candidate')
            git('checkout', '-q', 'main')
            (repo / 'main-only.md').write_text('# Main advanced\n')
            target = commit('main advanced after event base')
            git('merge', '--no-ff', '--no-commit', 'feature')
            (repo / 'merge-only.md').write_text('# Tested merge\n')
            merged = commit('synthetic merge')
            pr = {'pull_request': {'base': {'sha': base}, 'head': {'sha': source}}}
            event_path = Path(directory) / 'event.json'
            exports = Path(directory) / 'github-env'
            cases = (
                ('pr-fast', 'pull_request', pr, merged, merged, target),
                ('', 'pull_request', pr, merged, merged, target),
                ('main-fast', 'push', {'before': base, 'after': merged}, merged, merged, base),
                ('periodic-full', 'schedule', {}, merged, target, target),
                ('candidate-full', 'workflow_dispatch', {}, source, merged, base),
            )
            for profile, event_name, event, tested, event_sha, expected_base in cases:
                with self.subTest(profile=profile, event_name=event_name):
                    git('checkout', '-q', '--detach', tested)
                    event_path.write_text(json.dumps(event))
                    exports.write_text('')
                    environment = dict(os.environ, CI_PROFILE=profile, TESTED_SHA=tested,
                                       GITHUB_SHA=event_sha, GITHUB_EVENT_NAME=event_name,
                                       GITHUB_EVENT_PATH=str(event_path), GITHUB_ENV=str(exports),
                                       GITHUB_WORKSPACE=str(repo))
                    result = subprocess.run(['bash', '--noprofile', '--norc', '-eo', 'pipefail', '-c', script],
                                            cwd=repo, env=environment, text=True, capture_output=True)
                    self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                    self.assertEqual(exports.read_text(),
                                     f'OASIS7_PRODUCT_DOC_BASE={expected_base}\nOASIS7_PRODUCT_DOC_HEAD={tested}\n')
                    self.assertEqual(git('rev-parse', 'HEAD'), tested)
                    if event_name in ('pull_request', 'push'):
                        # The old helper alone would accept this event/checkout.
                        # A conflicting selected SHA must fail before any export.
                        exports.write_text('')
                        environment['TESTED_SHA'] = source
                        result = subprocess.run(['bash', '--noprofile', '--norc', '-eo', 'pipefail', '-c', script],
                                                cwd=repo, env=environment, text=True, capture_output=True)
                        self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
                        self.assertEqual(exports.read_text(), '')

    def test_formal_suites_share_build_and_propagate_all_failures(self):
        matrix = JOBS['formal-process-tests']
        self.assertIn('needs: [select, formal-test-build, formal-test-build-trusted]', matrix)
        self.assertNotIn('needs: oasis7-required', matrix)
        self.assertIn('fail-fast: false', matrix)
        self.assertIn('max-parallel: 5', matrix)
        self.assertIn('shard: ${{ fromJSON(needs.select.outputs.formal_shards) }}', matrix)
        self.assertIn('actions/download-artifact@', matrix)
        self.assertNotIn('cargo build', matrix)
        self.assertNotIn('cargo test', matrix)
        self.assertNotIn('rust-cache@', matrix)
        self.assertIn('--results output/formal-process/results.jsonl', matrix)
        self.assertIn('needs: [select, formal-test-build, formal-test-build-trusted, formal-process-tests]', JOBS['formal-process'])
        for name in ['full-regression', 'candidate-full-regression']:
            self.assertIn('./scripts/ci-tests.sh full-extended', JOBS[name])
            self.assertIn('needs: select', JOBS[name])
            self.assertNotIn('oasis7-test-build', JOBS[name])
        self.assertIn('compression-level: 0', JOBS['oasis7-test-build-candidate'])
        self.assertIn('overwrite: true', JOBS['oasis7-test-build-candidate'])
        self.assertIn('--scope ordinary', JOBS['oasis7-test-build-candidate'])
        self.assertNotIn('run_formal_process', JOBS['oasis7-test-build'])
        self.assertIn('--scope formal', JOBS['formal-test-build'])
        self.assertIn('--split-formal', JOBS['formal-test-build'])
        self.assertIn('formal-tests-${{ matrix.shard }}-${{ github.sha }}', matrix)
        self.assertIn('--shard "$SHARD"', matrix)
        driver = (ROOT / 'scripts/ci-tests.sh').read_text()
        required_body = driver.split('run_oasis7_required_tier_tests() {', 1)[1].split('run_scenario_regression_tests()', 1)[0]
        self.assertNotIn('formal-live-local-tests.sh', required_body)
        self.assertIn('if [[ "$tier" == full ]]; then run_group formal_process; fi', driver)

    def test_workspace_caches_have_exclusive_trust_and_shared_admission(self):
        for scope, reader_name, writer_name in (
                ('ordinary', 'oasis7-test-build-candidate', 'oasis7-test-build-trusted'),
                ('formal', 'formal-test-build', 'formal-test-build-trusted')):
            reader, writer = JOBS[reader_name], JOBS[writer_name]
            self.assertIn('cache-mode: read', reader)
            self.assertNotIn('cache-mode: write', reader)
            self.assertIn('cache-mode: write', writer)
            for text in ('refs/heads/main', "github.workflow_sha == github.sha", "github.event_name == 'push'",
                         "github.event_name == 'schedule'", "github.event_name == 'workflow_dispatch'",
                         "inputs.run_mode == 'full'"):
                self.assertIn(text, writer.split('    steps:', 1)[0])
                self.assertIn(text, reader.split('    steps:', 1)[0])
            self.assertIn('!(', reader.split('    steps:', 1)[0])
            self.assertEqual(reader.split('    steps:', 1)[1].strip(), writer.split('    steps:', 1)[1].strip())
            for job in (reader, writer):
                self.assertIn('actions: read', job)
                self.assertNotIn('actions: write', job)
                for mode in ('identity', 'preflight', 'measure', 'restore', 'stage', 'observe-save'):
                    self.assertIn(f'ci-workspace-cache.py {mode} --scope {scope}', job)
                self.assertIn('actions/cache/restore@55cc8345863c7cc4c66a329aec7e433d2d1c52a9', job)
                self.assertIn('actions/cache/save@55cc8345863c7cc4c66a329aec7e433d2d1c52a9', job)
                self.assertIn(f"steps.{scope}-cache-stage.outputs.save == 'true'", job)
                self.assertIn(f'path: ${{{{ runner.temp }}}}/{scope}-workspace-cache/workspace-units.tar.zst', job)
                self.assertIn(f'--cache-state "$RUNNER_TEMP/{scope}-cache-state.json"', job)
                self.assertLess(job.index('Swatinem/rust-cache@'), job.index(f'ci-workspace-cache.py restore --scope {scope}'))
                staging = job.split(f'- name: Stage bounded {scope} workspace units', 1)[1].split('- name:', 1)[0]
                self.assertIn(f"github.job == '{writer_name}'", staging)
                self.assertIn(f"steps.{scope}-workspace-cache.outputs.cache-hit != 'true'", staging)
                observation = job.split(f'- name: Confirm {scope} workspace cache', 1)[1].split('- name:', 1)[0]
                self.assertIn(f"steps.{scope}-cache-save.outcome == 'success'", observation)
                self.assertIn(f"steps.{scope}-workspace-cache.outputs.cache-hit == 'true'", observation)
                self.assertIn('CACHE_INVENTORY_TOKEN: ${{ github.token }}', observation)
            self.assertIn(f'cache_key: ${{{{ steps.{scope}-cache-identity.outputs.key }}}}', writer)
            self.assertIn(f'cache_confirmed: ${{{{ steps.{scope}-cache-observe.outputs.saved_confirmed }}}}', writer)
        matrix = JOBS['formal-process-tests']
        self.assertIn('always()', matrix)
        for name in ('formal-test-build', 'formal-test-build-trusted'):
            self.assertIn(f"needs.{name}.result == 'success'", matrix)
            self.assertIn(f"needs.{name}.result == 'skipped'", matrix)

    def test_formal_consumers_stop_when_cancelled_and_require_one_successful_builder(self):
        header = JOBS['formal-process-tests'].split('    timeout-minutes:', 1)[0]
        condition = header.split('    if:', 1)[1].strip().removeprefix('>-').strip()
        # One producer is deliberately skipped; retain the explicit status check.
        self.assertIn('always()', condition)
        states = ('success', 'skipped', 'failure', 'cancelled', '', 'unknown')
        for candidate in states:
            for trusted in states:
                for select in states:
                    for selected in (False, True):
                        for cancelled in (False, True):
                            values = {
                                'always()': True, 'cancelled()': cancelled,
                                'needs.select.result': select,
                                'needs.select.outputs.run_formal_process': str(selected).lower(),
                                'needs.formal-test-build.result': candidate,
                                'needs.formal-test-build-trusted.result': trusted,
                            }
                            expression = condition
                            for token, value in values.items():
                                expression = expression.replace(token, repr(value))
                            expression = re.sub(r'!(?!=)', 'not ', expression.replace('&&', ' and ').replace('||', ' or '))
                            expected = (not cancelled and select == 'success' and selected
                                        and (candidate, trusted) in (('success', 'skipped'), ('skipped', 'success')))
                            with self.subTest(candidate=candidate, trusted=trusted, select=select,
                                              selected=selected, cancelled=cancelled):
                                actual = eval(' '.join(expression.split()), {'__builtins__': {}}, {})
                                self.assertEqual(actual, expected)

    def test_ordinary_build_aggregation_rejects_every_incomplete_lane_combination(self):
        gate = JOBS['oasis7-test-build']
        self.assertIn('needs: [select, oasis7-test-build-candidate, oasis7-test-build-trusted]', gate)
        self.assertIn('always()', gate.split('    runs-on:', 1)[0])
        self.assertIn("needs.select.result == 'success'", gate)
        script = textwrap.dedent(gate.split('        run: |\n', 1)[1])
        states = ('success', 'skipped', 'failure', 'cancelled', '', 'unknown')
        for candidate in states:
            for trusted in states:
                with self.subTest(candidate=candidate, trusted=trusted):
                    result = subprocess.run(['bash', '-c', script], text=True, capture_output=True,
                        env=dict(os.environ, CANDIDATE_RESULT=candidate, TRUSTED_RESULT=trusted))
                    expected = (candidate, trusted) in (('success', 'skipped'), ('skipped', 'success'))
                    self.assertEqual(result.returncode == 0, expected, result.stdout + result.stderr)

    def test_consumers_require_their_actual_dependency_and_handle_cancellation(self):
        for name in ('oasis7-required', 'full-regression', 'candidate-full-regression'):
            header = JOBS[name].split('    runs-on:', 1)[0]
            condition = header.split('    if:', 1)[1].strip()
            condition = condition.removeprefix('>-').strip()
            self.assertIn('always() && !cancelled()', condition)
            predecessor = 'oasis7-test-build' if name == 'oasis7-required' else 'select'
            self.assertIn(f"needs.{predecessor}.result == 'success'", condition)
            for result in ('success', 'failure', 'skipped', 'cancelled', '', 'unknown'):
                for cancelled in (False, True):
                    for event, ref, mode, scope, expected in (
                            ('pull_request', 'refs/pull/1/merge', '', 'full', 'oasis7-required'),
                            ('push', 'refs/heads/main', '', 'full', 'oasis7-required'),
                            ('workflow_dispatch', 'refs/heads/main', 'full', 'explicit-full', 'full-regression'),
                            ('schedule', 'refs/heads/main', '', 'explicit-full', 'full-regression'),
                            ('workflow_dispatch', 'refs/heads/candidate', 'full', 'explicit-full', 'candidate-full-regression'),
                            ('workflow_dispatch', 'refs/heads/main', 'newapi_bridge_package', 'explicit-full', None)):
                        values = {
                            'always()': True, 'cancelled()': cancelled,
                            'needs.oasis7-test-build.result': result,
                            'needs.select.result': 'success' if name == 'oasis7-required' else result,
                            'needs.select.outputs.run_oasis7_required': 'false',
                            'fromJSON(needs.select.outputs.plan).scope': scope,
                            'needs.select.outputs.trusted_source': str(ref == 'refs/heads/main').lower(),
                            'fromJSON(needs.select.outputs.plan).execution_profile': '',
                            'fromJSON(needs.select.outputs.plan).fast_plan_version': 0,
                            'fromJSON(needs.select.outputs.plan).fast_execution_version': 0,
                            'fromJSON(needs.select.outputs.plan).ordinary_execution': '',
                            'github.event_name': event, 'github.ref': ref, 'inputs.run_mode': mode,
                        }
                        expression = condition
                        for token, value in values.items():
                            expression = expression.replace(token, repr(value))
                        expression = re.sub(r'!(?!=)', 'not ', expression.replace('&&', ' and ').replace('||', ' or '))
                        expression = ' '.join(expression.split())
                        with self.subTest(job=name, result=result, cancelled=cancelled, event=event, ref=ref, mode=mode):
                            # Evaluate only this checked-in boolean condition with fixture literals.
                            actual = eval(expression, {'__builtins__': {}}, {})
                            self.assertEqual(actual, result == 'success' and not cancelled and name == expected)

    def test_inline_execution_skips_only_compatible_producers_and_rejects_bad_dependency_states(self):
        for version, mode in ((0, ''), (2, 'inline'), (1, 'artifact'), (1, 'inline')):
            inline = version == 1 and mode == 'inline'
            for producer in ('success', 'skipped', 'failure', 'cancelled', '', 'unknown'):
                for cancelled in (False, True):
                    values = {
                        'always()': True, 'cancelled()': cancelled, 'needs.select.result': 'success',
                        'needs.oasis7-test-build.result': producer,
                        'needs.select.outputs.run_oasis7_required': 'true',
                        'needs.select.outputs.trusted_source': 'false',
                        'fromJSON(needs.select.outputs.plan).scope': 'targeted',
                        'fromJSON(needs.select.outputs.plan).execution_profile': 'pr-fast',
                        'fromJSON(needs.select.outputs.plan).fast_plan_version': 1,
                        'fromJSON(needs.select.outputs.plan).fast_execution_version': version,
                        'fromJSON(needs.select.outputs.plan).ordinary_execution': mode,
                        'github.event_name': 'pull_request', 'github.ref': 'refs/pull/1/merge',
                        'github.workflow_sha': 'a', 'github.sha': 'b', 'inputs.run_mode': '',
                    }
                    for name in ('oasis7-test-build-candidate', 'oasis7-test-build-trusted',
                                 'oasis7-test-build', 'oasis7-required'):
                        condition = JOBS[name].split('    if:', 1)[1].split('    runs-on:', 1)[0].strip().removeprefix('>-')
                        for token, value in values.items():
                            condition = condition.replace(token, repr(value))
                        condition = re.sub(r'!(?!=)', 'not ', condition.replace('&&', ' and ').replace('||', ' or '))
                        actual = eval(' '.join(condition.split()), {'__builtins__': {}}, {})
                        if name == 'oasis7-required':
                            expected = not cancelled and producer == ('skipped' if inline else 'success')
                        elif name == 'oasis7-test-build-trusted':
                            expected = False
                        else:
                            expected = not inline
                        with self.subTest(job=name, version=version, mode=mode, producer=producer, cancelled=cancelled):
                            self.assertEqual(actual, expected)

    def test_preview_is_cold_read_only_and_independent_of_required_authority(self):
        planner, preview = JOBS['pr-fast-preview-plan'], JOBS['pr-fast-preview']
        self.assertIn("if: github.event_name == 'pull_request'", planner)
        self.assertNotIn('needs: select', planner)
        self.assertIn('ci-fast-preview.py plan --base-ref "$BASE_SHA" --head-ref "$TESTED_SHA"', planner)
        self.assertIn('artifact: pr-fast-preview-plan-${{ github.run_id }}-${{ github.run_attempt }}', planner)
        self.assertIn('name: ${{ needs.pr-fast-preview-plan.outputs.artifact }}', preview)
        self.assertIn('group: [baseline, rust_baseline, oasis7_required]', preview)
        self.assertIn('fail-fast: false', preview)
        self.assertIn('ref: ${{ needs.pr-fast-preview-plan.outputs.tested_sha }}', preview)
        self.assertIn('ci-fast-preview.py restore --metadata', preview)
        self.assertIn('scripts/ci-tests.sh required --group "$PREVIEW_GROUP" "${plan_args[@]}"', preview)
        self.assertIn('--requirement scripts/doc-governance-requirements.txt', preview)
        self.assertIn('OASIS7_CI_PROFILE: pr-fast', preview)
        self.assertIn('set -euo pipefail', preview)
        self.assertIn('diagnostic_only', preview)
        for name, job in (('plan', planner), ('preview', preview)):
            with self.subTest(job=name):
                self.assertIn('contents: read', job)
                self.assertIn('persist-credentials: false', job)
                for forbidden in ('contents: write', 'actions: write', 'cache-mode: write', 'continue-on-error:',
                                  'rust-cache@', 'actions/cache/', 'ci-full-result.py', 'ci-required-result.py'):
                    self.assertNotIn(forbidden, job)
        for gate in ('required-gate', 'explicit-full-gate'):
            self.assertNotIn('pr-fast-preview', JOBS[gate])
        baseline = JOBS['rust-baseline']
        self.assertIn('name: ${{ needs.select.outputs.selection_artifact }}', baseline)
        self.assertIn('--plan "$RUNNER_TEMP/selection.json"', baseline)
        self.assertIn('SOURCE_SHA: ${{ needs.select.outputs.source_sha }}', baseline)

    def test_preview_invocation_passes_plan_only_to_its_actual_consumers(self):
        step = next(step for step in re.findall(r'^      - .*?(?=^      - |\Z)',
                                                JOBS['pr-fast-preview'], re.M | re.S)
                    if 'name: Execute actual preview group' in step)
        script = textwrap.dedent(step.split('        run: |\n', 1)[1])
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / 'scripts').mkdir()
            (root / 'scripts/ci-tests.sh').write_text(
                '#!/usr/bin/env bash\nprintf "%s\\0" "$@" > "$ARGV_LOG"\nexit "$DRIVER_EXIT"\n')
            (root / 'selection.json').write_text('{}')
            for group in ('baseline', 'rust_baseline', 'oasis7_required'):
                for code in (0, 19):
                    result = subprocess.run(['bash', '-c', script], cwd=root, text=True, capture_output=True,
                        env=dict(os.environ, PREVIEW_GROUP=group, RUNNER_TEMP=str(root),
                                 GITHUB_WORKSPACE=str(root), ARGV_LOG=str(root / 'args'), DRIVER_EXIT=str(code)))
                    args = (root / 'args').read_text().strip('\0').split('\0')
                    expected = ['required', '--group', group]
                    if group != 'baseline':
                        expected.extend(['--plan', str(root / 'selection.json')])
                    expected.extend(['--repo-root', str(root)])
                    with self.subTest(group=group, driver_exit=code):
                        self.assertEqual(args, expected)
                        self.assertEqual(result.returncode, code, result.stdout + result.stderr)

    def test_workspace_maintenance_recovers_without_a_new_confirmed_save(self):
        job = JOBS.get('cache-maintenance', '')
        self.assertIn('needs: [select, oasis7-test-build-trusted, formal-test-build-trusted, full-regression]', job)
        self.assertIn('always() && !cancelled()', job.split('    steps:', 1)[0])
        for producer in ('oasis7-test-build-trusted', 'formal-test-build-trusted'):
            self.assertIn(f"needs.{producer}.result == 'success'", job)
            self.assertNotIn(f"needs.{producer}.outputs.cache_confirmed", job.split('    steps:', 1)[0])
        for condition in ("github.ref == 'refs/heads/main'", "github.workflow_sha == github.sha",
                          "github.event_name == 'push'", "github.event_name == 'schedule'", "inputs.run_mode == 'full'"):
            self.assertIn(condition, job.split('    steps:', 1)[0])
        self.assertIn('cache-mode: none', job)
        self.assertIn('actions: write', job)
        self.assertIn('cancel-in-progress: false', job)
        self.assertIn('group: ci-cache-maintenance-main', job)
        self.assertIn('ref: ${{ needs.select.outputs.tested_sha }}', job)
        self.assertIn('CACHE_MAINTENANCE_TOKEN: ${{ github.token }}', job)
        self.assertIn('ci-cache-maintenance.py --recover --apply', job)
        self.assertNotIn('Swatinem/rust-cache', job)
        self.assertNotIn('actions/cache/', job)
        self.assertNotIn('cache-maintenance', JOBS['required-gate'])
        self.assertNotIn('cache-maintenance', JOBS['explicit-full-gate'])
        for name, body in JOBS.items():
            if name not in ('cache-maintenance', 'cache-bootstrap-maintenance'):
                self.assertNotIn('actions: write', body, name)
        # Execute the exact selection shell, substituting only the API boundary.
        script = textwrap.dedent(job.split('        run: |\n', 1)[1])
        stub = 'python3() { printf "<%s>\\n" "$@"; };\n'
        for ordinary, formal in ((True, False), (False, True), (True, True), (False, False)):
            values = dict(ORDINARY_RESULT='success' if ordinary else 'failure',
                          FORMAL_RESULT='success', ORDINARY_CONFIRMED='true',
                          FORMAL_CONFIRMED='true' if formal else 'false',
                          ORDINARY_KEY='ordinary-key', FORMAL_KEY='formal-key')
            result = subprocess.run(['bash', '-c', stub + script], text=True, capture_output=True,
                                    env=dict(os.environ, **values))
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual('<ordinary-key>' in result.stdout, ordinary)
            self.assertEqual('<formal-key>' in result.stdout, formal)
            self.assertIn('<--recover>', result.stdout)
            self.assertIn('<--apply>', result.stdout)

    def test_recovery_selection_preserves_success_cancellation_and_trust_boundaries(self):
        ecs = workflow_jobs((ROOT / '.github/workflows/ecs-packages.yml').read_text())
        for scope, job in (('rust', JOBS['cache-maintenance']), ('ecs', ecs['cache-maintenance'])):
            header = job.split('    steps:', 1)[0]
            condition = header.split('    if:', 1)[1].split('    cache-mode:', 1)[0].strip().removeprefix('>-')
            for ordinary in ('success', 'failure', 'cancelled', 'skipped'):
                for formal in ('success', 'failure', 'cancelled', 'skipped'):
                    for confirmed in ('true', 'false', ''):
                        for cancelled in (False, True):
                            for event, ref, mode, matching_workflow, trusted in (
                                    ('push', 'refs/heads/main', '', True, scope == 'rust'),
                                    ('schedule', 'refs/heads/main', '', True, scope == 'rust'),
                                    ('workflow_dispatch', 'refs/heads/main', 'full', True, True),
                                    ('workflow_dispatch', 'refs/heads/main', 'newapi_bridge_package', True, scope == 'ecs'),
                                    ('pull_request', 'refs/pull/1/merge', '', True, False),
                                    ('push', 'refs/heads/topic', '', True, False),
                                    ('workflow_dispatch', 'refs/heads/main', 'full', False, False)):
                                values = {
                                    'always()': True, 'cancelled()': cancelled,
                                    'needs.oasis7-test-build-trusted.result': ordinary,
                                    'needs.formal-test-build-trusted.result': formal,
                                    'needs.native-trusted.result': ordinary,
                                    'needs.oasis7-test-build-trusted.outputs.cache_confirmed': confirmed,
                                    'needs.formal-test-build-trusted.outputs.cache_confirmed': confirmed,
                                    'needs.native-trusted.outputs.cache_confirmed': confirmed,
                                    'needs.select.outputs.trusted_source': str(ref == 'refs/heads/main').lower(),
                                    'fromJSON(needs.select.outputs.plan).execution_profile': '',
                                    'fromJSON(needs.select.outputs.plan).fast_plan_version': 0,
                                    'fromJSON(needs.select.outputs.plan).fast_execution_version': 0,
                                    'fromJSON(needs.select.outputs.plan).ordinary_execution': '',
                                    'github.event_name': event, 'github.ref': ref, 'inputs.run_mode': mode,
                                    'github.workflow_sha': 'a' if matching_workflow else 'b', 'github.sha': 'a',
                                }
                                expression = condition
                                for token, value in values.items():
                                    expression = expression.replace(token, repr(value))
                                expression = re.sub(r'!(?!=)', 'not ', expression.replace('&&', ' and ').replace('||', ' or '))
                                success = ordinary == 'success' or (scope == 'rust' and formal == 'success')
                                with self.subTest(scope=scope, ordinary=ordinary, formal=formal,
                                                  confirmed=confirmed, cancelled=cancelled, event=event, ref=ref):
                                    actual = eval(' '.join(expression.split()), {'__builtins__': {}}, {})
                                    self.assertEqual(actual, success and trusted and not cancelled)

    def test_bootstrap_is_trusted_and_cannot_block_or_expand_build_selection(self):
        bootstrap = JOBS.get('cache-bootstrap-maintenance', '')
        self.assertIn('needs: select', bootstrap)
        self.assertIn('cache-mode: none', bootstrap)
        self.assertIn('actions: write', bootstrap)
        self.assertIn('group: ci-cache-maintenance-main', bootstrap)
        self.assertIn('cancel-in-progress: false', bootstrap)
        self.assertIn('ref: ${{ needs.select.outputs.tested_sha }}', bootstrap)
        self.assertIn('persist-credentials: false', bootstrap)
        self.assertIn('ci-cache-maintenance.py --recover --apply', bootstrap)
        for forbidden in ('--current-key', 'actions/cache/', 'Swatinem/rust-cache', 'cargo '):
            self.assertNotIn(forbidden, bootstrap)
        for name in ('oasis7-test-build-candidate', 'formal-test-build', 'required-gate', 'explicit-full-gate'):
            self.assertNotIn('cache-bootstrap-maintenance', JOBS[name])
        for name in ('oasis7-test-build-trusted', 'formal-test-build-trusted'):
            self.assertIn('needs: [select, cache-bootstrap-maintenance]', JOBS[name])
            self.assertIn('always() && !cancelled()', JOBS[name])
        for select in ('success', 'failure', 'skipped'):
            for cancelled in (False, True):
                for maintenance_result in ('success', 'failure', 'skipped'):
                    for ordinary, formal, scope in ((True, False, 'affected'), (False, True, 'affected'),
                                                   (False, False, 'affected'), (False, False, 'explicit-full'),
                                                   (False, False, 'full')):
                        for event, ref, mode, matching_workflow, trusted in (
                                ('push', 'refs/heads/main', '', True, True),
                                ('schedule', 'refs/heads/main', '', True, True),
                                ('workflow_dispatch', 'refs/heads/main', 'full', True, True),
                                ('workflow_dispatch', 'refs/heads/main', 'newapi_bridge_package', True, False),
                                ('pull_request', 'refs/pull/1/merge', '', True, False),
                                ('push', 'refs/heads/topic', '', True, False),
                                ('workflow_dispatch', 'refs/heads/main', 'full', False, False)):
                            expected_ordinary = ordinary or scope in ('full', 'explicit-full')
                            values = {
                                'always()': True, 'cancelled()': cancelled,
                                'needs.select.result': select,
                                'needs.cache-bootstrap-maintenance.result': maintenance_result,
                                'needs.select.outputs.run_oasis7_required': str(ordinary).lower(),
                                'needs.select.outputs.run_formal_process': str(formal).lower(),
                                'fromJSON(needs.select.outputs.plan).scope': scope,
                            'needs.select.outputs.trusted_source': str(ref == 'refs/heads/main').lower(),
                            'fromJSON(needs.select.outputs.plan).execution_profile': '',
                            'fromJSON(needs.select.outputs.plan).fast_plan_version': 0,
                            'fromJSON(needs.select.outputs.plan).fast_execution_version': 0,
                            'fromJSON(needs.select.outputs.plan).ordinary_execution': '',
                                'github.event_name': event, 'github.ref': ref, 'inputs.run_mode': mode,
                                'github.workflow_sha': 'a' if matching_workflow else 'b', 'github.sha': 'a',
                            }
                            for name, selected in (('cache-bootstrap-maintenance', expected_ordinary or formal),
                                                   ('oasis7-test-build-trusted', expected_ordinary),
                                                   ('formal-test-build-trusted', formal)):
                                condition = JOBS[name].split('    if:', 1)[1].split('    runs-on:', 1)[0]
                                condition = condition.split('\n    cache-mode:', 1)[0].strip().removeprefix('>-')
                                for token, value in values.items():
                                    condition = condition.replace(token, repr(value))
                                condition = re.sub(r'!(?!=)', 'not ', condition.replace('&&', ' and ').replace('||', ' or '))
                                with self.subTest(job=name, select=select, cancelled=cancelled,
                                                  maintenance=maintenance_result, event=event, scope=scope):
                                    actual = eval(' '.join(condition.split()), {'__builtins__': {}}, {})
                                    self.assertEqual(actual, select == 'success' and not cancelled and trusted and selected)

    def test_ecs_maintenance_recovers_after_native_success_without_a_new_save(self):
        ecs = workflow_jobs((ROOT / '.github/workflows/ecs-packages.yml').read_text())
        writer, job = ecs['native-trusted'], ecs['cache-maintenance']
        self.assertIn('needs: native-trusted', job)
        self.assertIn("needs.native-trusted.result == 'success'", job)
        self.assertIn('always() && !cancelled()', job.split('    steps:', 1)[0])
        self.assertNotIn('needs.native-trusted.outputs.cache_confirmed', job.split('    steps:', 1)[0])
        self.assertIn('group: ci-cache-maintenance-main', job)
        self.assertIn('cache-mode: none', job)
        self.assertIn('actions: write', job)
        self.assertIn('ci-cache-maintenance.py --recover --apply "${args[@]}"', job)
        self.assertIn('CI_CACHE_REPOSITORY_BUDGET_BYTES: ${{ env.ECS_CACHE_REPOSITORY_BUDGET_BYTES }}', job)
        self.assertIn("steps.ecs-workspace-cache.outputs.cache-hit == 'true'", writer)
        self.assertIn('cache_confirmed: ${{ steps.ecs-cache-observe.outputs.saved_confirmed }}', writer)
        self.assertNotIn('cache-maintenance', ecs['package'].split('    steps:', 1)[0])
        for name, body in ecs.items():
            if name != 'cache-maintenance':
                self.assertNotIn('actions: write', body, name)
        script = textwrap.dedent(job.split('        run: |\n', 1)[1])
        stub = 'python3() { printf "<%s>\\n" "$@"; };\n'
        for result in ('success', 'failure'):
            for confirmed in ('true', 'false', ''):
                output = subprocess.run(['bash', '-c', stub + script], text=True, capture_output=True,
                                        env=dict(os.environ, NATIVE_RESULT=result,
                                                 CURRENT_CACHE_CONFIRMED=confirmed, CURRENT_CACHE_KEY='ecs-key'))
                self.assertEqual(output.returncode, 0, output.stderr)
                self.assertIn('<--recover>', output.stdout)
                self.assertIn('<--apply>', output.stdout)
                self.assertEqual('<ecs-key>' in output.stdout, result == 'success' and confirmed == 'true')

    def test_manual_candidate_full_is_read_only_and_not_scheduled(self):
        candidate = JOBS['candidate-full-regression']
        self.assertIn("needs.select.outputs.trusted_source != 'true' && github.event_name == 'workflow_dispatch' && inputs.run_mode == 'full'", candidate)
        self.assertIn('cache-mode: read', candidate)
        self.assertNotIn('cache-mode: write', candidate)
        self.assertNotIn('trusted cache sentinel', candidate)
        self.assertNotIn('actions/cache/save@', candidate)
        self.assertIn('save-if: false', candidate)
        self.assertIn('actions/cache/restore@', candidate)
        self.assertIn('git rev-parse --verify', candidate)
        self.assertIn('SOURCE_SHA', candidate)
        self.assertIn('GITHUB_STEP_SUMMARY', candidate)
        self.assertIn('CI_VERBOSE=1 ./scripts/ci-tests.sh full', candidate)

    def test_candidate_full_preserves_full_setup_and_execution(self):
        # Candidate tests preserve the execution body; only the trusted writer
        # additionally compiles cache variants and saves caches/sentinels.
        writer = JOBS['full-regression'].split('    steps:\n', 1)[1]
        candidate = JOBS['candidate-full-regression'].split('    steps:\n', 1)[1]
        candidate = re.sub(r'      - name: Record exact candidate identity\n.*?(?=      - name: Resolve)', '', candidate, flags=re.S)
        writer = re.sub(r'      - name: Prepare trusted cache sentinel\n.*?(?=      - (?:name: Run full|uses: actions/download-artifact))', '', writer, flags=re.S)
        writer = re.sub(r'      - name: Complete shared native cache\n.*?(?=      - name:)', '\n', writer, flags=re.S)
        writer = re.sub(r'      - name: Save prepared wasm-bindgen tool cache\n.*?(?=      - name:)', '', writer, flags=re.S)
        writer = re.sub(r'^      - name: Install dependency cache admission\n.*?(?=^      - |\Z)', '', writer, flags=re.M | re.S)
        # Cache identity and writer admission have separate contracts; retain
        # equality for the compiler setup, build, test and artifact steps.
        writer = re.sub(r'^      - uses: Swatinem/rust-cache@.*?(?=^      - |\Z)', '', writer, flags=re.M | re.S)
        candidate = re.sub(r'^      - uses: Swatinem/rust-cache@.*?(?=^      - |\Z)', '', candidate, flags=re.M | re.S)
        candidate = candidate.replace('          save-if: false\n', '')
        candidate = candidate.replace('actions/cache/restore@55cc8345863c7cc4c66a329aec7e433d2d1c52a9 # v6.1.0',
                                      'actions/cache@55cc8345863c7cc4c66a329aec7e433d2d1c52a9 # v6.1.0')
        self.assertEqual(candidate.strip(), writer.strip())

    def test_pinned_trunk_helper_failure_and_success_paths(self):
        # The protected baseline already executes this suite during migration.
        subprocess.run(['bash',str(ROOT/'scripts/install-ci-trunk.test.sh')],cwd=ROOT,check=True)

    def test_viewer_security_smoke_uses_installed_browser_after_formal_build(self):
        job = JOBS['viewer-js-required']
        self.assertIn('agent-browser@0.37.1', job)
        self.assertIn('--prefix "$RUNNER_TEMP/viewer-security-browser"', job)
        self.assertIn('"$browser_bin" install --with-deps', job)
        self.assertIn("printf 'AGENT_BROWSER_BIN=%s\\n'", job)
        self.assertNotIn('cache-mode: write', job)
        group = re.search(r'^    viewer_js_required\) (.*?) ;;$',
                          (ROOT / 'scripts/ci-tests.sh').read_text(), re.M).group(1)
        self.assertLess(group.index('run_oasis7_viewer_software_safe_build'),
                        group.index('viewer-auth-browser-security-smoke.mjs'))

    def test_signed_read_codegen_runs_in_support_and_full(self):
        job = JOBS['workspace-support']
        self.assertIn('actions/setup-node@949feb2413d6458794dcd2491c4babbbce0c15c1', job)
        self.assertIn("node-version: '22'", job)
        driver = (ROOT / 'scripts/ci-tests.sh').read_text()
        function = driver.split('run_signed_read_protocol_contract_tests() {', 1)[1].split('\n}', 1)[0]
        self.assertIn('protocol_codegen/generate.py check', function)
        self.assertNotIn('generate.py write', function)
        self.assertIn('--ignore-scripts', function)
        self.assertIn('npm test --prefix crates/oasis7_agent_api/protocol_conformance', function)
        self.assertIn('PYTHON="$ci_python"', function)
        support = driver.split('run_oasis7_workspace_support_crate_tests() {', 1)[1].split('\n}', 1)[0]
        self.assertIn('run_signed_read_protocol_contract_tests', support)
        self.assertIn('oasis7_agent_api) run_signed_read_protocol_contract_tests;', driver)
        for full in ('run_full_support_tier_tests', 'run_full_required_superset'):
            self.assertIn('run_oasis7_workspace_support_crate_tests',
                          driver.split(full + '() {', 1)[1].split('\n}', 1)[0])

    def test_additive_security_steps_preserve_candidate_groups(self):
        for name in ('net', 'viewer-js-required', 'viewer-performance-report', 'workflow-governance'):
            self.assertEqual(JOBS[name].count('name: Execute selected group'), 1)
            self.assertIn(f'bash ./scripts/ci-tests.sh required --group "{name.replace(chr(45), chr(95))}"', JOBS[name])
        performance = JOBS['viewer-performance-report']
        self.assertIn('agent-browser@0.37.1', performance)
        self.assertIn('--prefix "$RUNNER_TEMP/viewer-performance-browser"', performance)
        self.assertIn('"$browser_bin" install --with-deps', performance)
        self.assertIn("printf 'AGENT_BROWSER_BIN=%s\\n'", performance)
        self.assertLess(performance.index('Install pinned performance browser'), performance.index('Execute selected group'))
        driver = (ROOT / 'scripts/ci-tests.sh').read_text()
        for job, forbidden in [('net', 'Verify pinned network source'),
                               ('viewer-js-required', 'Verify browser authentication security'),
                               ('viewer-performance-report', 'Build performance test artifact')]:
            self.assertNotIn(forbidden, JOBS[job])
        net = JOBS['net']
        self.assertLess(net.index('Install WASM C compiler'), net.index('Execute selected group'))
        for check in ('scripts/libp2p-security-source.test.py', 'scripts/libp2p-compat.test.py',
                      'CC_wasm32_unknown_unknown=clang', 'AR_wasm32_unknown_unknown=llvm-ar',
                      'cargo check -p oasis7_net --no-default-features --target wasm32-unknown-unknown --locked',
                      'cargo check -p oasis7_node --features libp2p --target wasm32-unknown-unknown --locked'):
            self.assertIn(check, driver)
        governance = JOBS['workflow-governance']
        self.assertGreater(governance.index('Verify new source archive'), governance.index('Execute selected group'))
        for test in ('package-source-plan.test.py', 'safe-git-archive.test.py', 'cache-permission-probe.test.cjs'):
            self.assertIn(test, governance)
        self.assertNotIn('run: python3 scripts/ci-workflow.test.py', governance) # already in candidate dispatcher

    def test_bindgen_cache_has_exact_identity_and_single_trusted_writer(self):
        identities = []
        for name in ('viewer-js-required', 'viewer-performance-report', 'full-regression'):
            job = JOBS[name]
            self.assertIn('source scripts/wasm-bindgen-cli-common.sh', job)
            self.assertIn('uses: actions/cache/restore@caa296126883cff596d87d8935842f9db880ef25', job)
            self.assertIn('--ensure-cache --print-bin', job)
            self.assertIn('test -x "$bindgen_bin"', job)
            self.assertIn('test "$("$bindgen_bin" --version)" = "wasm-bindgen $WASM_BINDGEN_LOCK_VERSION"', job)
            self.assertLess(job.index('Swatinem/rust-cache@'), job.index('id: bindgen-meta'))
            prepare = job.split('id: prepare-bindgen', 1)[1].split('      - ', 1)[0]
            self.assertIn('set -euo pipefail', prepare)
            hot, cold = prepare.split('          else\n', 1)
            self.assertNotIn('--ensure-cache', hot)
            self.assertIn('--ensure-cache', cold)
            self.assertIn('WASM_BINDGEN_BIN=%s', prepare)
            identity = re.search(r'          key: (wasm-bindgen-cli-v1-.*)', job).group(1)
            identities.append(identity)
            self.assertIn("hashFiles('scripts/ensure-wasm-bindgen-cli.sh', 'scripts/wasm-bindgen-cli-common.sh')", identity)
            self.assertIn('path: ${{ steps.bindgen-meta.outputs.root }}', job)
            if name != 'full-regression':
                self.assertNotIn('actions/cache/save@', job)
            else:
                save = job.split('name: Save prepared wasm-bindgen tool cache', 1)[1].split('      - ', 1)[0]
                for condition in ('success()', "github.ref == 'refs/heads/main'", "steps.bindgen-cache.outputs.cache-hit != 'true'", "steps.prepare-bindgen.outcome == 'success'"):
                    self.assertIn(condition, save)
                self.assertIn('key: ' + identity, save)
                self.assertLess(job.index('Save prepared wasm-bindgen'), job.index('Run full test tier'))
        self.assertEqual(len(set(identities)), 1)
        self.assertNotIn('bindgen-meta', JOBS['launcher-web'])

    def test_local_cargo_contract_regressions_execute_and_propagate_failures(self):
        driver = (ROOT / 'scripts/ci-tests.sh').read_text()
        names = ('run_packaging_artifact_contract_tests', 'run_packaging_contract_tests',
                 'run_operational_identity_contract_tests', 'run_operational_node_contract_tests',
                 'run_operational_contract_tests')
        functions = '\n'.join(re.search(r'^' + name + r'\(\) \{\n.*?^\}', driver, re.M | re.S).group()
                              for name in names)
        setup = 'set -euo pipefail\nrun_provider_remote_https_smoke() { :; }\n'
        setup += 'run() { echo "$*"; if [[ -n "$FAIL_TEST" && "$*" == *"$FAIL_TEST"* ]]; then return 37; fi; }\n'
        calls = '\nrun_operational_contract_tests\nrun_packaging_contract_tests\necho contracts-complete\n'
        tests = ('local-letai-provider-bridge.test.sh', 'build-wasm-module.test.sh')
        for failing in ('', *tests):
            result = subprocess.run(['bash', '-c', setup + functions + calls], text=True,
                                    capture_output=True, env=dict(os.environ, FAIL_TEST=failing))
            if failing:
                self.assertEqual(result.returncode, 37, result.stdout + result.stderr)
                self.assertNotIn('contracts-complete', result.stdout)
            else:
                self.assertEqual(result.returncode, 0, result.stderr)
                for test in tests:
                    self.assertEqual(result.stdout.count('./scripts/' + test), 1, result.stdout)

    def test_local_public_testnet_build_uses_selected_repo_in_dev_and_ci(self):
        with tempfile.TemporaryDirectory() as directory:
            temporary = Path(directory)
            repo = (temporary / 'build repo').resolve()
            (repo / 'scripts').mkdir(parents=True)
            bin_dir = temporary / 'build-bin'
            bin_dir.mkdir()
            for path, mode in ((repo / 'scripts/cargo-dev.sh', 'wrapper'), (bin_dir / 'cargo', 'raw')):
                path.write_text('#!/usr/bin/env bash\n{ printf "' + mode + '\\n"; printf "%s\\n" "$PWD" "$@"; } > "$BUILD_TRACE"\nexit "${BUILD_EXIT:-0}"\n')
                path.chmod(0o755)
            source = (ROOT / 'scripts/run-local-public-testnet-letai-test-environment.sh').read_text()
            function = re.search(r'^build_bins\(\) \{\n.*?^\}', source, re.M | re.S).group()
            script = 'set -euo pipefail\nROOT_DIR="$1"; source "$2"; BUILD=1; log() { :; }\n' + function + '\nbuild_bins\n'
            for ci, raw in (('', '0'), ('1', '0'), ('true', '0'), ('', '1')):
                trace = temporary / ('build-trace-' + (ci or 'dev') + raw)
                env = dict(os.environ, CI=ci, OASIS7_FORCE_RAW_CARGO=raw,
                           OASIS7_CARGO_DEV_REPO_ROOT=str(temporary / 'unrelated-repo'), BUILD_TRACE=str(trace),
                           PATH=str(bin_dir) + os.pathsep + os.environ['PATH'])
                for code in (0, 37):
                    result = subprocess.run(['bash', '-c', script, '_', str(repo), str(ROOT / 'scripts/cargo-dev-lib.sh')],
                                            cwd=temporary, env=dict(env, BUILD_EXIT=str(code)), capture_output=True, text=True)
                    assert result.returncode == code, result.stderr
                    records = trace.read_text().splitlines()
                    assert records[:2] == [('raw' if ci or raw == '1' else 'wrapper'), str(repo)], records
                    assert 'oasis7_viewer_live' in records, records

    def test_dot_todo_entry_requires_pair_and_propagates_failure(self):
        driver = (ROOT / 'scripts/ci-tests.sh').read_text()
        function = re.search(r'^run_dot_todo_contract_tests\(\) \{\n.*?^\}', driver, re.M | re.S)
        self.assertIsNotNone(function)
        governance = driver.split('run_workflow_governance_baseline_contract_tests() {', 1)[1].split('\n}', 1)[0]
        self.assertEqual(governance.count('  run_dot_todo_contract_tests'), 1)
        for present, test_exit, expected in (((), 0, 0), (('source',), 0, 1),
                                              (('test',), 0, 1), (('source', 'test'), 0, 0),
                                              (('source', 'test'), 37, 37)):
            with self.subTest(present=present, test_exit=test_exit), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                (root / 'scripts').mkdir()
                if 'source' in present:
                    (root / 'scripts/dot-todo.py').write_text('# synthetic source')
                if 'test' in present:
                    (root / 'scripts/dot-todo.test.py').write_text(
                        "from pathlib import Path\nPath('ran').write_text('once')\nraise SystemExit(%d)\n" % test_exit)
                shell = 'set -euo pipefail\nrun() { "$@"; }\n' + function.group() + '\nrun_dot_todo_contract_tests\n'
                result = subprocess.run(['bash', '-c', shell], cwd=root, capture_output=True, text=True)
                self.assertEqual(result.returncode, expected, result.stderr)
                self.assertEqual((root / 'ran').exists(), len(present) == 2)

    def test_local_signer_and_new_contracts_are_connected_once(self):
        fleet = JOBS['fleet-health']
        self.assertIn("if: runner.os == 'macOS'", fleet)
        self.assertIn('TMPDIR: /private/tmp', fleet)
        self.assertIn('python3 scripts/ci-local-signer-tests.py --require-darwin', fleet)
        driver = (ROOT / 'scripts/ci-tests.sh').read_text()
        packaging = driver.split('run_packaging_contract_tests() {', 1)[1].split('\n}', 1)[0]
        self.assertEqual(packaging.count('run python3 ./scripts/ci-local-signer-tests.py'), 1)
        governance = driver.split('run_workflow_governance_baseline_contract_tests() {', 1)[1].split('\n}', 1)[0]
        for test in ('plan-wasm-determinism-scope.test.sh', 'ensure-wasm-bindgen-cli.test.sh', 'ci-local-signer-tests.test.py'):
            self.assertEqual(governance.count(test), 1)

    def test_stable_install_is_minimal_with_required_components(self):
        self.assertNotIn('--profile default', WORKFLOW)
        for name, job in JOBS.items():
            if 'rustup toolchain install' in job and name != 'newapi-bridge-package':
                self.assertRegex(job, r'rustup toolchain install .*RUST_TOOLCHAIN.* --profile minimal --component rustfmt --component clippy')

    def test_wasm_dynamic_matrix_and_job_failure_boundaries(self):
        workflow = (ROOT / '.github/workflows/wasm-determinism-gate.yml').read_text()
        jobs = dict(re.findall(r'^  ([a-z][a-z0-9-]*):\n(.*?)(?=^  [a-z][a-z0-9-]*:|\Z)', workflow.split('\njobs:\n', 1)[1], re.M | re.S))
        expression = '${{ fromJSON(needs.plan-wasm-determinism-scope.outputs.selected_module_sets_json) }}'
        self.assertIn('selected_module_sets_json: ${{ steps.scope.outputs.selected_module_sets_json }}', jobs['plan-wasm-determinism-scope'])
        for name in ('collect-wasm-summaries', 'verify-wasm-determinism'):
            job = jobs[name]
            self.assertIn('module_set: ' + expression, job)
            guard = job.split('    if: >-', 1)[1].split('    strategy:', 1)[0]
            self.assertIn("needs.plan-wasm-determinism-scope.result == 'success'", guard)
            self.assertIn("outputs.scope == 'partial'", guard)
            self.assertIn("outputs.scope == 'all'", guard)
            self.assertIn('fail-fast: false', job)
            self.assertNotIn('Scope note', job)
            self.assertNotIn('outputs.run_m', job)
            execution = re.sub(r'^      - name: Preserve failed candidate build evidence\n.*?(?=^      - |\Z)',
                               '', job, flags=re.M | re.S)
            self.assertNotIn('        if:', execution)
            self.assertIn('if-no-files-found: error', job)
        evidence = re.search(r'^      - name: Preserve failed candidate build evidence\n.*?(?=^      - |\Z)',
                             jobs['collect-wasm-summaries'], re.M | re.S).group()
        self.assertIn('        if: failure()\n', evidence)
        self.assertIn('uses: actions/upload-artifact@', evidence)
        self.assertIn('include-hidden-files: true', evidence)
        self.assertIn('if-no-files-found: warn', evidence)
        self.assertNotIn('        run:', evidence)
        self.assertEqual(re.findall(r'^            (.+)$', evidence, re.M), [
            '.tmp/builtin-wasm-sync-modules/*.wasm',
            '.tmp/builtin-wasm-sync-modules/*.metadata.json',
            '.tmp/builtin-wasm-sync-modules/*.build-receipt.json',
        ])
        self.assertIn("needs.collect-wasm-summaries.result == 'success'", jobs['verify-wasm-determinism'])
        self.assertNotIn('continue-on-error:', workflow)
        for filename in ('wasm-determinism-gate.yml', 'document-corpus-platform.yml'):
            workflow = (ROOT / '.github/workflows' / filename).read_text()
            self.assertIn("group: ${{ github.workflow }}-${{ github.event_name == 'pull_request' && format('pr-{0}', github.event.pull_request.number) || format('run-{0}', github.run_id) }}", workflow)
            self.assertIn("cancel-in-progress: ${{ github.event_name == 'pull_request' }}", workflow)

    def test_group_commands_execute_once_and_propagate_failure(self):
        driver = (ROOT / 'scripts/ci-tests.sh').read_text()
        network = re.search(r'^run_oasis7_net_libp2p_tests\(\) \{\n.*?^\}', driver, re.M | re.S).group()
        script = "set -euo pipefail\nrun() { printf '%s\\n' \"$*\"; }\nrun_cargo() { run cargo \"$@\"; }\n" + network + "\nrun_oasis7_net_libp2p_tests\n"
        result = subprocess.run(['bash', '-c', script], text=True, capture_output=True, env=dict(os.environ, GITHUB_ACTIONS='true', RUNNER_OS='Linux'))
        self.assertEqual(result.returncode, 0, result.stderr)
        for command in ('libp2p-security-source.test.py', 'libp2p-compat.test.py',
                        'test --locked --manifest-path ./vendor-libp2p-rendezvous-0.18.1/Cargo.toml --target-dir ./target --lib -- --test-threads=1',
                        'check -p oasis7_net', 'check -p oasis7_node'):
            self.assertEqual(result.stdout.count(command), 1)
        failure = subprocess.run(['bash', '-c', script.replace("printf '%s\\n' \"$*\"", 'return 42')], text=True, capture_output=True, env=dict(os.environ, GITHUB_ACTIONS='true', RUNNER_OS='Linux'))
        self.assertEqual(failure.returncode, 42)
        for group, command in [('viewer_js_required', 'viewer-auth-browser-security-smoke.mjs'), ('viewer_performance_report', 'build:viewer:visual-test')]:
            case = re.search(r'^    ' + group + r'\) (.*?) ;;$', driver, re.M).group(1)
            definitions = "set -euo pipefail\nrun() { printf '%s\\n' \"$*\"; }\n"
            names = re.findall(r'run_oasis7_[a-z_]+', case)
            for name in names:
                if name == 'run_oasis7_viewer_visual_test_build':
                    definitions += name + "() { run npm run build:viewer:visual-test; }\n"
                else:
                    definitions += name + "() { run " + name + "; }\n"
            result = subprocess.run(['bash', '-c', definitions + case], text=True, capture_output=True)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(result.stdout.count(command), 1)
            failed = subprocess.run(['bash', '-c', definitions.replace("printf '%s\\n' \"$*\"", 'return 42') + case], text=True, capture_output=True)
            self.assertEqual(failed.returncode, 42)

    def test_required_and_full_tier_runners_share_feature_matched_viewer_artifact(self):
        driver = (ROOT / 'scripts/ci-tests.sh').read_text()
        names = ('run_oasis7_package_tests', 'build_oasis7_viewer_binary', 'run_oasis7_test_tier_with_viewer',
                 'run_oasis7_required_tier_tests', 'run_oasis7_full_tier_tests')
        functions = [re.search(r'^' + name + r'\(\) \{\n.*?^\}', driver, re.M | re.S).group()
                     for name in names]
        with tempfile.TemporaryDirectory() as temporary:
            viewer = Path(temporary) / 'oasis7_viewer_live'
            viewer.write_text('test viewer executable')
            viewer.chmod(0o755)
            log = Path(temporary) / 'commands.log'
            script = """set -euo pipefail
ci_python=python3
run_cargo() {
  if [[ "$1" == build ]]; then
    printf 'BUILD|%s\\n' "$*" >> "$CI_LOG"
    [[ "${FAIL_PHASE:-}" != build ]] || return 47
    printf '{"reason":"compiler-artifact","target":{"name":"oasis7_viewer_live"},"executable":"%s"}\\n' "$VIEWER_BIN"
  elif [[ "$1" == check ]]; then
    printf 'CHECK|%s\\n' "$*" >> "$CI_LOG"
    [[ "${FAIL_PHASE:-}" != check ]] || return 49
  else
    printf 'TEST|%s|VIEWER=%s\\n' "$*" "${PRE2_VIEWER_BINARY:-}" >> "$CI_LOG"
    [[ "${FAIL_PHASE:-}" != test ]] || return 48
  fi
}
""" + "\n".join(functions) + "\nrun_oasis7_required_tier_tests\nrun_oasis7_full_tier_tests\n"
            environment = dict(os.environ, CI_LOG=str(log), VIEWER_BIN=str(viewer))
            environment.pop('PRE2_VIEWER_BINARY', None)
            # Exercise the Cargo fallback even when the outer CI uses a bundle.
            environment.pop('OASIS7_CI_TEST_BUNDLE', None)
            result = subprocess.run(['bash', '-c', script], text=True, capture_output=True, env=environment)
            self.assertEqual(result.returncode, 0, result.stderr)
            calls = log.read_text().splitlines()
            self.assertEqual(sum(call.startswith('BUILD|') for call in calls), 2)
            self.assertEqual(sum(call.startswith('TEST|') for call in calls), 2)
            self.assertIn('CHECK|check -p oasis7_agent_api -p oasis7_simulator -p oasis7_viewer_codec -p oasis7_viewer_runtime --lib --target wasm32-unknown-unknown --locked', calls)
            self.assertTrue(all('-p oasis7_simulator' in call for call in calls if call.startswith('TEST|')))
            for feature in ('test_tier_required', 'test_tier_full,wasmtime,viewer_live_integration'):
                build_index = next((index for index, call in enumerate(calls)
                                    if call.startswith('BUILD|') and f'--features {feature} ' in call), None)
                test_index = next((index for index, call in enumerate(calls)
                                   if call.startswith('TEST|') and f'--features {feature}|' in call
                                   and f'VIEWER={viewer}' in call), None)
                self.assertIsNotNone(build_index, feature)
                self.assertIsNotNone(test_index, feature)
                self.assertLess(build_index, test_index, feature)

            full_only_script = script.replace('run_oasis7_required_tier_tests\nrun_oasis7_full_tier_tests\n',
                                              'run_oasis7_full_tier_tests\n')
            self.assertNotEqual(full_only_script, script)
            for phase, expected in (('build', 47), ('test', 48)):
                failure_log = Path(temporary) / f'{phase}.log'
                failure_environment = dict(environment, CI_LOG=str(failure_log), FAIL_PHASE=phase)
                failed = subprocess.run(['bash', '-c', full_only_script], text=True, capture_output=True,
                                        env=failure_environment)
                self.assertEqual(failed.returncode, expected, failed.stderr)
                failure_calls = failure_log.read_text().splitlines()
                if phase == 'build':
                    self.assertTrue(any(call.startswith('BUILD|') and '--features test_tier_full,wasmtime,viewer_live_integration' in call
                                        for call in failure_calls))
                    self.assertFalse(any(call.startswith('TEST|') for call in failure_calls))
                else:
                    self.assertTrue(any(call.startswith('TEST|') and '--features test_tier_full,wasmtime,viewer_live_integration' in call
                                        for call in failure_calls))

    def test_full_support_supplies_feature_matched_viewer_and_stops_on_build_failure(self):
        driver = (ROOT / 'scripts/ci-tests.sh').read_text()
        names = ('run_full_support_tier_tests', 'run_oasis7_package_tests',
                 'build_oasis7_viewer_binary', 'run_oasis7_test_tier_with_viewer')
        functions = [re.search(r'^' + name + r'\(\) \{\n.*?^\}', driver, re.M | re.S).group()
                     for name in names]
        support = functions[0]
        dependencies = set(re.findall(r'^  (run_[a-z0-9_]+)', support, re.M)) - set(names)
        with tempfile.TemporaryDirectory() as temporary:
            viewer = Path(temporary) / 'oasis7_viewer_live'
            viewer.write_text('controlled artifact for runner wiring only')
            viewer.chmod(0o755)
            script = "set -euo pipefail\nci_python=python3\n"
            script += "\n".join(name + '() { :; }' for name in dependencies) + "\n"
            script += r"""
run_cargo() {
  if [[ "$1" == build ]]; then
    [[ "${FAIL_BUILD:-}" != 1 ]] || return 47
    printf '{"reason":"compiler-artifact","target":{"name":"oasis7_viewer_live"},"executable":"%s"}\n' "$VIEWER_BIN"
  else
    [[ -n "${PRE2_VIEWER_BINARY:-}" && "$PRE2_VIEWER_BINARY" == "$VIEWER_BIN" ]] || return 48
    [[ "$*" == *"--lib --bins --features wasmtime" ]] || return 49
  fi
}
"""
            script += "\n".join(functions) + "\nrun_full_support_tier_tests\n"
            for fail_build, expected in (('0', 0), ('1', 47)):
                result = subprocess.run(['bash', '-c', script], text=True, capture_output=True,
                    env=dict(os.environ, PRE2_VIEWER_BINARY='', VIEWER_BIN=str(viewer), FAIL_BUILD=fail_build))
                self.assertEqual(result.returncode, expected, result.stderr)

    def test_primary_rejects_non_loopback_test_login_listener(self):
        result = subprocess.run(['bash', str(ROOT / 'scripts/viewer-primary-web-entry-regression.sh'),
            '--viewer-host', '0.0.0.0'], capture_output=True, text=True)
        self.assertEqual(result.returncode, 2)
        self.assertIn('requires a loopback viewer host', result.stderr)

    def test_primary_login_requires_guest_gate_and_visible_backend_issued_session(self):
        source = (ROOT / 'scripts/viewer-primary-web-entry-regression.sh').read_text()
        function = 'enter_primary_player_surface() {' + source.split(
            'enter_primary_player_surface() {', 1)[1].split('\n}', 1)[0] + '\n}'
        script = r"""set -euo pipefail
out_dir=/unused
assert_eval_true() {
  if [[ "$2" == *'authSource'* ]]; then
    echo issued >> "$TRACE_LOG"
    [[ "$FAIL_PHASE" != issued ]]
  else
    echo guest >> "$TRACE_LOG"
    [[ "$FAIL_PHASE" != guest ]]
  fi
}
capture_screenshot_best_effort() { :; }
ab_cmd() {
  [[ "$2" == click && "$3" == '[data-auth-action="test-login"]' ]] || return 42
  echo click >> "$TRACE_LOG"
  [[ "$FAIL_PHASE" != click ]]
}
""" + function.replace('>>"$out_dir/test-login.log" 2>&1', '') + '\nenter_primary_player_surface owned default'
        with tempfile.TemporaryDirectory() as directory:
            for phase, calls in (('', ['guest', 'click', 'issued']), ('guest', ['guest']),
                                 ('click', ['guest', 'click']), ('issued', ['guest', 'click', 'issued'])):
                trace = Path(directory) / (phase + 'trace')
                result = subprocess.run(['bash', '-c', script], capture_output=True, text=True,
                    env=dict(os.environ, TRACE_LOG=str(trace), FAIL_PHASE=phase))
                self.assertEqual(result.returncode == 0, phase == '', result.stderr)
                self.assertEqual(trace.read_text().splitlines(), calls)

    def test_primary_flow_requires_visible_unique_action_context_and_receipt(self):
        source = (ROOT / 'scripts/viewer-primary-web-entry-regression.sh').read_text()
        self.assertTrue('primary_entry_surface_predicate()' in source, 'primary flow predicate missing')
        function = source.split('primary_entry_surface_predicate() {', 1)[1]
        expression = function.split("<<'JS'\n", 1)[1].split('\nJS', 1)[0]
        valid = dict(root=True, actions=1, context=True, receipt=True,
                     visible=True, href='#command', text='Inspect current blocker')
        variants = [(valid, True)]
        for field, value in (('root', False), ('actions', 0), ('actions', 2),
                             ('context', False), ('receipt', False), ('visible', False),
                             ('href', ''), ('text', '')):
            variants.append((dict(valid, **{field: value}), False))
        for candidate, expected in variants:
            script = 'const state=' + json.dumps(candidate) + ';\n' + r"""
const node = () => ({textContent:state.text, getClientRects:()=>state.visible?[{}]:[],
  getAttribute:()=>state.href});
const root = node();
root.querySelectorAll = () => Array.from({length:state.actions}, node);
root.querySelector = selector => selector.includes('supporting-context')
  ? (state.context ? node() : null) : (state.receipt ? node() : null);
global.document = {querySelector:()=>state.root ? root : null};
global.getComputedStyle = () => ({visibility:'visible', display:'block'});
"""
            script += 'if ((' + expression + ') !== ' + str(expected).lower() + ') process.exit(1);'
            result = subprocess.run(['node', '-e', script], capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, str(candidate) + result.stderr)

    def test_release_web_fixture_starts_only_after_complete_stack_build(self):
        source = (ROOT / 'scripts/viewer-software-safe-step-regression.sh').read_text()
        match = re.search(r'^prepare_release_web_stack\(\) \{\n.*?^\}', source, re.M | re.S)
        self.assertIsNotNone(match, 'release Web startup must finish builds before starting fixture')
        self.assertIn('    prepare_release_web_stack\n', source)
        script = r"""set -euo pipefail
out_dir=/unused
ab_log=/dev/null
BOOTSTRAP_USES_BUNDLE="${CASE_BUNDLE:-0}"
unset OASIS7_RUN_LAUNCHER_STACK_SKIP_SOURCE_BUILD
log_note() { :; }
record() { echo "$1" >> "$TRACE_LOG"; [[ "$FAIL_PHASE" != "$1" ]]; }
release_web_fixture_build_viewer() { record viewer; }
release_web_fixture_build() { record fixture; }
oasis7_cargo_dev() {
  for binary in oasis7_llm_provider_probe oasis7_game_launcher oasis7_viewer_live; do
    [[ "$*" == *"--bin $binary"* ]] || return 49
  done
  record stack
}
release_web_fixture_start() { record start; }
""" + match.group() + '\nprepare_release_web_stack\nif [[ "$BOOTSTRAP_USES_BUNDLE" == 1 ]]; then [[ -z "${OASIS7_RUN_LAUNCHER_STACK_SKIP_SOURCE_BUILD+x}" ]]; else [[ "$OASIS7_RUN_LAUNCHER_STACK_SKIP_SOURCE_BUILD" == 1 ]]; fi'
        with tempfile.TemporaryDirectory() as directory:
            for phase, calls in (('', ['viewer', 'fixture', 'stack', 'start']),
                                 ('viewer', ['viewer']), ('fixture', ['viewer', 'fixture']),
                                 ('stack', ['viewer', 'fixture', 'stack'])):
                trace = Path(directory) / (phase + 'trace')
                result = subprocess.run(['bash', '-c', script], capture_output=True, text=True,
                    env=dict(os.environ, TRACE_LOG=str(trace), FAIL_PHASE=phase))
                self.assertEqual(result.returncode == 0, phase == '', result.stderr)
                self.assertEqual(trace.read_text().splitlines(), calls)
            trace = Path(directory) / 'bundle-trace'
            result = subprocess.run(['bash', '-c', script], capture_output=True, text=True,
                env=dict(os.environ, TRACE_LOG=str(trace), FAIL_PHASE='', CASE_BUNDLE='1'))
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(trace.read_text().splitlines(), ['viewer', 'fixture', 'start'])

    def test_full_tail_keeps_strict_wasmtime_profile_and_fixture_viewer(self):
        driver = (ROOT / 'scripts/ci-tests.sh').read_text()
        functions = [re.search(r'^' + name + r'\(\) \{\n.*?^\}', driver, re.M | re.S).group()
                     for name in ('run_oasis7_package_tests', 'build_oasis7_viewer_binary', 'run_oasis7_test_tier_with_viewer', 'run_full_extended_tier_tests')]
        full = re.search(r'^    full\|full-standard\) (.*?) ;;$', driver, re.M).group(1)
        with tempfile.TemporaryDirectory() as temporary:
            viewer = Path(temporary) / 'oasis7_viewer_live'
            viewer.write_text('test viewer executable')
            viewer.chmod(0o755)
            script = """set -euo pipefail
tier=full-standard
ci_python=python3
run_full_required_superset() { :; }
run_rust_baseline() { :; }
run_oasis7_full_tier_tests() { :; }
run_workspace_support_full_feature_tests() { :; }
run_oasis7_llm_baseline_fixture_smoke() { :; }
run_group() { [[ "$1" == formal_process ]] || return 50; printf 'FORMAL\\n' >> "$CI_LOG"; }
run_oasis7_production_mock_guard() { printf 'GUARD\\n' >> "$CI_LOG"; [[ "${FAIL_PHASE:-}" != guard ]] || return 49; }
run_cargo() {
  if [[ "$1" == build ]]; then
    printf 'BUILD|%s\\n' "$*" >> "$CI_LOG"
    [[ "${FAIL_PHASE:-}" != build ]] || return 47
    printf '{"reason":"compiler-artifact","target":{"name":"oasis7_viewer_live"},"executable":"%s"}\\n' "$VIEWER_BIN"
  else
    printf 'TEST|%s|VIEWER=%s\\n' "$*" "${PRE2_VIEWER_BINARY:-}" >> "$CI_LOG"
    [[ "${FAIL_PHASE:-}" != test ]] || return 48
  fi
}
""" + "\n".join(functions) + "\n" + full + "\n"
            environment = dict(os.environ, VIEWER_BIN=str(viewer))
            environment.pop('PRE2_VIEWER_BINARY', None)
            for phase, expected in (('', 0), ('guard', 49), ('build', 47), ('test', 48)):
                log = Path(temporary) / (phase + 'calls.log')
                result = subprocess.run(['bash', '-c', script], text=True, capture_output=True,
                                        env=dict(environment, CI_LOG=str(log), FAIL_PHASE=phase))
                self.assertEqual(result.returncode, expected, result.stderr)
                calls = log.read_text().splitlines()
                self.assertEqual(calls.pop(0), 'GUARD')
                if phase == 'guard':
                    self.assertEqual(calls, [])
                    continue
                self.assertTrue(calls[0].startswith('BUILD|'), calls)
                self.assertIn('--features test_tier_required,wasmtime ', calls[0])
                self.assertNotIn('test_tier_full', '\n'.join(calls))
                self.assertNotIn('viewer_live_integration', '\n'.join(calls))
                tests = [call for call in calls if call.startswith('TEST|')]
                self.assertEqual(len(tests), 0 if phase == 'build' else 1)
                if tests:
                    self.assertIn('--lib --bins', tests[0])
                    self.assertIn('--features test_tier_required,wasmtime|', tests[0])
                    self.assertIn(f'VIEWER={viewer}', tests[0])

            formal_log = Path(temporary) / 'formal-calls.log'
            formal_script = script.replace('tier=full-standard', 'tier=full')
            result = subprocess.run(['bash', '-c', formal_script], text=True, capture_output=True,
                                    env=dict(environment, CI_LOG=str(formal_log)))
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(formal_log.read_text().splitlines()[-1], 'FORMAL')
            failed_log = Path(temporary) / 'formal-failed-calls.log'
            failed = subprocess.run(['bash', '-c', formal_script], text=True, capture_output=True,
                                    env=dict(environment, CI_LOG=str(failed_log), FAIL_PHASE='test'))
            self.assertEqual(failed.returncode, 48, failed.stderr)
            self.assertNotIn('FORMAL', failed_log.read_text())

    def test_production_mock_guard_checks_actual_exit_and_rejection(self):
        import runpy
        guard = runpy.run_path(str(ROOT / 'scripts/hosted-local-mock-production-guard.py'))
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            manifest = directory / 'artifacts.jsonl'
            calls = directory / 'calls.log'
            executables = {}
            artifacts = {}
            packages = {
                'oasis7_chain_runtime': ('oasis7_chain_runtime', ['default', 'node-libp2p', 'wasmtime']),
                'oasis7_game_launcher': ('oasis7_launcher_backend', ['default']),
            }

            def write_executable(name, output, code):
                executables[name].write_text(
                    f'#!{sys.executable}\nfrom pathlib import Path\n'
                    f'with Path({str(calls)!r}).open("a") as log: log.write({name!r} + "\\n")\n'
                    f'print({output!r})\nraise SystemExit({code})\n'
                )
                executables[name].chmod(0o755)

            for name, (package, features) in packages.items():
                executable = directory / name
                executables[name] = executable
                write_executable(name, guard['REJECTION'], 1)
                artifacts[name] = {
                    'reason': 'compiler-artifact', 'target': {'name': name, 'kind': ['bin']},
                    'manifest_path': str(ROOT / 'crates' / package / 'Cargo.toml'),
                    'profile': {'test': False},
                    'executable': str(executable), 'features': features,
                }

            def write_manifest(records):
                # Cargo's JSON stream can also contain driver status lines.
                manifest.write_text('+ cargo build\n' + '\n'.join(json.dumps(record) for record in records))
                calls.unlink(missing_ok=True)

            # Match the split packages: only Chain declares wasmtime.
            write_manifest(artifacts.values())
            guard['verify'](manifest)
            self.assertEqual(calls.read_text().splitlines(), list(packages))

            for name, artifact in artifacts.items():
                invalid = [
                    {'features': [*artifact['features'], feature]}
                    for feature in ('test_tier_required', 'test_tier_full')
                ]
                invalid.extend([
                    {'target': {'name': name, 'kind': ['test']}},
                    {'profile': {'test': True}},
                    {'manifest_path': str(ROOT / 'crates/oasis7/Cargo.toml')},
                    {'manifest_path': None},
                ])
                if name == 'oasis7_chain_runtime':
                    invalid.append({'features': ['default', 'node-libp2p']})
                for changes in invalid:
                    with self.subTest(target=name, changes=changes):
                        write_manifest(dict(record, **changes) if target == name else record
                                       for target, record in artifacts.items())
                        with self.assertRaises(AssertionError):
                            guard['verify'](manifest)
                        self.assertFalse(calls.exists(), 'invalid artifacts must fail before execution')
                for records in ([record for target, record in artifacts.items() if target != name],
                                [*artifacts.values(), artifact]):
                    with self.subTest(target=name, count=len(records)):
                        write_manifest(records)
                        with self.assertRaises(AssertionError):
                            guard['verify'](manifest)
                        self.assertFalse(calls.exists())

            for name in executables:
                for output, code in ((guard['REJECTION'], 0), ('unrelated failure', 1)):
                    with self.subTest(target=name, output=output, code=code):
                        write_manifest(artifacts.values())
                        write_executable(name, output, code)
                        with self.assertRaises(AssertionError):
                            guard['verify'](manifest)
                write_executable(name, guard['REJECTION'], 1)

    def test_network_wasm_tools_precede_group_execution(self):
        net = JOBS['net']
        self.assertLess(net.index('Install WASM C compiler'), net.index('Execute selected group'))
        self.assertLess(net.index('llvm-ar --version'), net.index('Execute selected group'))
        writer = JOBS['full-regression']
        self.assertIn('            clang \\', writer)
        self.assertIn('            llvm \\', writer)
        self.assertIn('rustup target add wasm32-unknown-unknown', writer)
        for tool in ('clang --print-targets | grep -w wasm32', 'llvm-ar --version'):
            self.assertLess(writer.index(tool), writer.index('Run full test tier'))

    def test_trusted_writer_only_runs_on_protected_main(self):
        writer = JOBS['full-regression']
        self.assertIn("github.ref == 'refs/heads/main' && (github.event_name == 'schedule' || (github.event_name == 'workflow_dispatch' && inputs.run_mode == 'full'))", writer)
        self.assertIn('cache-mode: write', writer)
        self.assertIn('test "$GITHUB_REF" = refs/heads/main', writer)
        self.assertNotIn('test "$ACTIONS_CACHE_MODE"', writer)

    def test_readers_share_compatible_trusted_writer_identity(self):
        writer = JOBS['full-regression']
        self.assertIn('cache-mode: write', writer)
        self.assertIn('shared-key: ci-shared-native-trusted-v3', writer)
        self.assertIn('env-vars: CARGO CC CFLAGS CXX CMAKE RUST OASIS7_WASM', writer)
        for name, job in JOBS.items():
            if 'save-if: false' in job and 'Swatinem/rust-cache@' in job:
                self.assertIn('shared-key: ci-shared-native-trusted-v3', job, name)
                self.assertIn('env-vars: CARGO CC CFLAGS CXX CMAKE RUST OASIS7_WASM', job, name)
        self.assertNotIn('ordinary-required-v2', WORKFLOW)

    def test_dependency_admission_is_local_to_the_two_cache_actions(self):
        admission = (ROOT / 'scripts/ci-rust-cache-admission.py').read_text()
        revision = re.search(r"^ACTION_REVISION = '([0-9a-f]{40})'$", admission, re.M)
        self.assertIsNotNone(revision)
        # Tar-before-reserve behavior was verified on this exact upstream
        # package. A pin upgrade must review/update the guard and this contract.
        self.assertEqual(revision[1], 'd1527c8f67ef9af357290475d4af4b0fab8f12e0')
        ecs_text = (ROOT / '.github/workflows/ecs-packages.yml').read_text()
        ecs_jobs = workflow_jobs(ecs_text)
        lanes = ((JOBS['full-regression'], 'shared-native', 'CI_CACHE_REPOSITORY_BUDGET_BYTES'),
                 (ecs_jobs['native'], 'ecs', 'ECS_CACHE_REPOSITORY_BUDGET_BYTES'),
                 (ecs_jobs['native-trusted'], 'ecs', 'ECS_CACHE_REPOSITORY_BUDGET_BYTES'))
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
                        predicate = predicate.replace("hashFiles('scripts/ci-rust-cache-toolchains.py')",
                                                      repr('hash' if has_toolchains else ''))
                        allowed = eval(predicate.replace('&&', ' and '), {'__builtins__': {}}, {})
                        self.assertEqual(allowed, outcome == 'success' and (family != 'ecs' or has_toolchains))
                for step in steps:
                    if step != cache:
                        self.assertNotIn('steps.dependency-cache-tools.outputs.path', step)
                        self.assertNotIn('OASIS7_DEPENDENCY_CACHE_', step)
                self.assertNotIn('steps.dependency-cache-tools.outputs.path', job.split('    steps:', 1)[0])
        self.assertIn('permissions:\n      contents: read\n      actions: read', JOBS['full-regression'])
        self.assertNotIn('dependency-cache-tools', JOBS['candidate-full-regression'])
        self.assertNotIn('dependency-cache-tools', ecs_jobs['web'])
        governance = (ROOT / 'scripts/ci-tests.sh').read_text().split(
            'run_workflow_governance_baseline_contract_tests() {', 1)[1].split('\n}', 1)[0]
        self.assertEqual(governance.count('run python3 ./scripts/ci-rust-cache-admission.test.py'), 1)

    def test_dependency_save_inputs_reject_step_only_status_functions(self):
        # GitHub permits status functions in step `if`, not action inputs.
        # The `job` context is available in `with`, so save-if uses job.status.
        ecs_jobs = workflow_jobs((ROOT / '.github/workflows/ecs-packages.yml').read_text())
        for name, job in (('full-regression', JOBS['full-regression']),
                          ('native', ecs_jobs['native']), ('native-trusted', ecs_jobs['native-trusted'])):
            cache = next(step for step in re.findall(r'^      - .*?(?=^      - |\Z)', job, re.M | re.S)
                         if 'uses: Swatinem/rust-cache@' in step)
            save = re.search(r'          save-if: \$\{\{ (.*?) \}\}', cache)
            self.assertIsNotNone(save, name)
            with self.subTest(job=name):
                self.assertNotRegex(save.group(1), r'(?i)\b(?:always|cancelled|failure|success)\s*\(')

    def test_dependency_save_requires_success_and_current_trusted_source(self):
        ecs_jobs = workflow_jobs((ROOT / '.github/workflows/ecs-packages.yml').read_text())
        for name, job in (('full-regression', JOBS['full-regression']),
                          ('native', ecs_jobs['native']), ('native-trusted', ecs_jobs['native-trusted'])):
            cache = next(step for step in re.findall(r'^      - .*?(?=^      - |\Z)', job, re.M | re.S)
                         if 'uses: Swatinem/rust-cache@' in step)
            save = re.search(r'          save-if: \$\{\{ (.*?) \}\}', cache)
            self.assertIsNotNone(save, name)
            expression = save.group(1)
            for status in ('success', 'failure', 'cancelled'):
                for ref in ('refs/heads/main', 'refs/heads/topic', 'refs/pull/1/merge'):
                    for matching_source in (False, True):
                        values = {'job.status': status, 'github.ref': ref, 'github.job': name,
                                  'github.workflow_sha': 'a' if matching_source else 'b', 'github.sha': 'a'}
                        predicate = expression
                        for token, value in values.items():
                            predicate = predicate.replace(token, repr(value))
                        predicate = predicate.replace('&&', ' and ').replace('||', ' or ')
                        with self.subTest(job=name, status=status, ref=ref, matching=matching_source):
                            actual = eval(predicate, {'__builtins__': {}}, {})
                            self.assertEqual(actual, status == 'success' and ref == 'refs/heads/main'
                                             and matching_source and name != 'native')

    def test_shared_cache_is_populated_completely_before_successful_save(self):
        writer = JOBS['full-regression']
        cache = re.search(r'      - uses: Swatinem/rust-cache@.*?(?=      - name:)', writer, re.S)[0]
        self.assertIn('cache-on-failure: false', cache)
        self.assertNotIn('save-if: false', cache)
        completion = re.search(r'      - name: Complete shared native cache\n.*?(?=      - name:)', writer, re.S)[0]
        self.assertIn('./scripts/ci-tests.sh cache-native', completion)
        self.assertNotIn('continue-on-error', completion)
        self.assertNotIn('if:', completion)
        self.assertLess(writer.index('./scripts/ci-tests.sh full-extended'), writer.index(completion))
        for name, job in JOBS.items():
            if name != 'full-regression' and 'shared-key: ci-shared-native-trusted-v3' in job:
                self.assertIn('save-if: false', job, name)
                # The trusted Formal producer writes only its separate curated
                # workspace snapshot while reading this dependency archive.
                if name not in ('formal-test-build-trusted', 'oasis7-test-build-trusted'):
                    self.assertNotIn('cache-mode: write', job, name)

    def test_shared_cache_normalization_runs_after_install_before_restore(self):
        for name, job in JOBS.items():
            if 'shared-key: ci-shared-native-trusted-v3' in job:
                prepare = job.index('python3 scripts/ci-rust-cache-toolchains.py')
                self.assertIn('"$RUST_TOOLCHAIN" "$OASIS7_WASM_TOOLCHAIN"', job, name)
                self.assertLess(job.index('rustup default'), prepare, name)
                self.assertLess(prepare, job.index('uses: Swatinem/rust-cache@'), name)

    def test_download_caches_follow_actual_worksets(self):
        node_jobs={name for name,job in JOBS.items() if 'uses: actions/setup-node@' in job}
        self.assertEqual(node_jobs,{'viewer-js-required','viewer-performance-report','launcher-web','workspace-support','full-regression','candidate-full-regression'})
        for name in node_jobs - {'workspace-support'}:
            self.assertIn('cache: npm',JOBS[name])
            self.assertIn('cache-dependency-path: crates/oasis7_viewer/package-lock.json',JOBS[name])
            self.assertIn('npm ci --prefix crates/oasis7_viewer',JOBS[name])
        for name,job in JOBS.items():
            if 'Swatinem/rust-cache@' in job and name in {g.replace('_', '-') for g in CONFIG['groups']}:
                self.assertIn('shared-key: ci-shared-native-trusted-v3',job)
                self.assertIn('add-rust-environment-hash-key: true',job)
                self.assertIn('env-vars: CARGO CC CFLAGS CXX CMAKE RUST OASIS7_WASM',job)
                self.assertLess(job.index('rustup default'),job.index('uses: Swatinem/rust-cache@d1527c8f67ef9af357290475d4af4b0fab8f12e0 # v2'))
        for name in ('launcher-web','full-regression'):
            job=JOBS[name]
            self.assertIn('uses: actions/cache@55cc8345863c7cc4c66a329aec7e433d2d1c52a9',job)
            self.assertIn("hashFiles('scripts/install-ci-trunk.sh')",job)
            self.assertNotIn('cargo install trunk',job)
            self.assertNotIn('restore-keys:',job)
            self.assertLess(job.index('Cache pinned Trunk release archive'),job.index('bash scripts/install-ci-trunk.sh'))
            invocation='Execute selected group' if name=='launcher-web' else 'Run full test tier'
            self.assertLess(job.index('bash scripts/install-ci-trunk.sh'),job.index(invocation))

    def test_failed_scope_planning_reports_gate_failure_before_artifact_download(self):
        gate = JOBS['required-gate']
        self.assertIn('always()', gate)
        self.assertLess(gate.index('Require successful scope planning'),
                        gate.index('actions/download-artifact@'))
        script = gate.split('        run: |\n', 1)[1].split('      - uses:', 1)[0]
        for state in ('failure', 'cancelled', 'skipped', '', 'success'):
            with self.subTest(state=state):
                result = subprocess.run(['bash', '-c', script], text=True, capture_output=True,
                                        env=dict(os.environ, SELECT_RESULT=state))
                self.assertEqual(result.returncode, 0 if state == 'success' else 1)
                if state != 'success':
                    self.assertIn('CI scope planning did not succeed', result.stdout)
        for name in ('oasis7-test-build', 'formal-test-build', 'formal-test-build-trusted', 'formal-process-tests', 'command-center'):
            self.assertIn("needs.select.result == 'success'", JOBS[name])

    def test_gate_is_last_and_all_groups_are_dependencies(self):
        gate = JOBS['required-gate']
        dependencies = re.search(r'    needs: \[(.*)\]', gate).group(1).split(', ')
        self.assertEqual(set(dependencies), {'select', *(g.replace('_', '-') for g in CONFIG['groups'])})
        self.assertIn('always()', gate)
        self.assertIn('ci-required-result.py', gate)
        for group in CONFIG['groups']:
            job = JOBS[group.replace('_', '-')]
            if group == 'formal_process':
                self.assertIn('always()', job)
                self.assertIn('needs: [select, formal-test-build, formal-test-build-trusted, formal-process-tests]', job)
                continue
            dependency = 'needs: [select, oasis7-test-build]' if group == 'oasis7_required' else 'needs: select'
            self.assertIn(dependency, job)
            self.assertIn(f"needs.select.outputs.run_{group} == 'true'", job)
            self.assertIn("needs.select.result == 'success'", job)
            self.assertIn("fromJSON(needs.select.outputs.plan).scope == 'full'", job)
            if group != 'fleet_health':
                self.assertNotIn('strategy:', job)
            self.assertNotIn('ci-authority', job)
        self.assertNotIn('needs: required-gate', WORKFLOW)
        self.assertNotIn('continue-on-error:', without_optional_dependency_cache_install(WORKFLOW))
        self.assertNotIn('paths-ignore:', WORKFLOW)

    def test_platform_product_jobs_still_gate(self):
        self.assertIn('runs-on: windows-2022', JOBS['windows-rollout'])
        self.assertIn('p2p-public-testnet-package-rollout.test.sh', JOBS['windows-rollout'])
        self.assertIn('testnet-packages-macos-arm64-contract.test.sh', JOBS['macos-package-contract'])
        self.assertIn('p2p-public-testnet-fleet-health.test.py', JOBS['fleet-health'])
        self.assertIn('os: [ubuntu-24.04, windows-2022, macos-15]', JOBS['fleet-health'])
        self.assertIn('--host-smoke', JOBS['fleet-health'])
        self.assertEqual(WORKFLOW.count('run: bash ./scripts/testnet-packages-macos-arm64-contract.test.sh'), 1)
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

    def test_trusted_closure_and_safe_checkout(self):
        select = JOBS['select']
        self.assertIn('policy_sha="$BASE_SHA"', select)
        self.assertIn('git show "${policy_sha}:scripts/${file}"', select)
        self.assertIn('--test-ref "$tested_sha"', select)
        self.assertIn('for file in plan-rust-required-scope.py ci-required-scope.json ci-required-result.py ci-formal-plan.py ci-formal-process-suites.json;', select)
        self.assertIn('bootstrap_full=true', select)
        self.assertIn('&& "$GITHUB_REF" != refs/heads/main', select)
        self.assertEqual(WORKFLOW.count('name: ${{ needs.select.outputs.authority_artifact }}'), 5)
        for name, job in JOBS.items():
            if name != 'pr-fast-preview':
                self.assertNotIn('matrix.group', job, name)
        self.assertEqual(len(re.findall(r'uses: actions/checkout@[0-9a-f]{40}\b', WORKFLOW)), WORKFLOW.count('persist-credentials: false'))
        self.assertNotIn('secrets.', WORKFLOW)
        dependency_cache = next(step for step in re.findall(r'^      - .*?(?=^      - |\Z)',
                                                             JOBS['full-regression'], re.M | re.S)
                                if 'uses: Swatinem/rust-cache@' in step)
        self.assertEqual(dependency_cache.count('GH_TOKEN: ${{ github.token }}'), 1)
        self.assertNotIn('GH_TOKEN:', WORKFLOW.replace(dependency_cache, '', 1))
        self.assertNotIn('scripts/pm/', WORKFLOW)
        self.assertNotIn('task_uid', WORKFLOW)
        self.assertIn('permissions:\n  contents: read', WORKFLOW)

    def test_runner_discovers_supported_python_when_default_is_unsupported(self):
        with tempfile.TemporaryDirectory() as temporary:
            for name in ('python', 'python3'):
                path = Path(temporary) / name
                path.write_text('#!/usr/bin/env bash\nexit 91\n')
                path.chmod(0o755)
            environment = dict(os.environ, PATH=temporary + os.pathsep + os.environ['PATH'])
            result = subprocess.run(['bash', str(ROOT / 'scripts/ci-tests.sh'), 'required',
                                     '--group', 'codex_agent_config_validation'],
                                    cwd=ROOT, env=environment, text=True, capture_output=True)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertIn('validated agent configurations:', result.stdout)

    def test_native_tool_suite_does_not_inherit_nightly_build_std(self):
        driver = (ROOT / 'scripts/ci-tests.sh').read_text()
        function = re.search(r'^run_oasis7_workspace_support_crate_tests\(\) \{\n.*?^\}', driver, re.M | re.S).group()
        script = """set -euo pipefail
run_signed_read_protocol_contract_tests() { :; }
run() { :; }
run_cargo() { printf '%s|%s\\n' "$OASIS7_WASM_BUILD_STD" "$*"; }
run_cargo_clippy() { printf '%s|clippy %s\\n' "$OASIS7_WASM_BUILD_STD" "$*"; }
""" + function + "\nrun_oasis7_workspace_support_crate_tests\nprintf 'after|%s\\n' \"$OASIS7_WASM_BUILD_STD\"\n"
        result = subprocess.run(['bash', '-c', script], text=True, capture_output=True,
                                env=dict(os.environ, OASIS7_WASM_BUILD_STD='1'))
        self.assertEqual(result.returncode, 0, result.stderr)
        calls = result.stdout.splitlines()
        self.assertEqual(calls[-2], '0|test -p wasm_build_suite -p wasm_module_observe')
        self.assertTrue(all(call.startswith('1|') for call in calls[:-2]))
        self.assertEqual(calls[-1], 'after|1')

    def test_agent_contract_and_harness_checks_are_in_workspace_support(self):
        driver = (ROOT / 'scripts/ci-tests.sh').read_text()
        function = re.search(r'^run_oasis7_workspace_support_crate_tests\(\) \{\n.*?^\}', driver, re.M | re.S).group()
        script = """set -euo pipefail
run_signed_read_protocol_contract_tests() { :; }
run() { :; }
run_cargo() { printf 'test|%s\\n' "$*"; }
run_cargo_clippy() { printf 'clippy|%s\\n' "$*"; }
""" + function + "\nrun_oasis7_workspace_support_crate_tests\n"
        result = subprocess.run(['bash', '-c', script], text=True, capture_output=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        commands = result.stdout.splitlines()
        self.assertIn('clippy|-p oasis7_agent_api --all-targets', commands)
        self.assertIn('clippy|-p oasis7_agent_harness --all-targets', commands)
        self.assertIn('test|test -p oasis7_agent_api', commands)
        self.assertIn('test|test -p oasis7_agent_harness --lib', commands)
        self.assertLess(commands.index('clippy|-p oasis7_agent_harness --all-targets'),
                        commands.index('test|test -p oasis7_agent_harness --lib'))

    def test_private_context_all_targets_and_caller_are_executed(self):
        driver = (ROOT / 'scripts/ci-tests.sh').read_text()
        function = re.search(r'^run_oasis7_workspace_support_crate_tests\(\) \{\n.*?^\}', driver, re.M | re.S).group()
        script = "set -euo pipefail\nrun_signed_read_protocol_contract_tests() { :; }\nrun() { :; }\nrun_cargo() { printf '%s\\n' \"$*\"; }\nrun_cargo_clippy() { printf 'clippy %s\\n' \"$*\"; }\n" + function + "\nrun_oasis7_workspace_support_crate_tests\n"
        result = subprocess.run(['bash', '-c', script], text=True, capture_output=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        calls = [line.split() for line in result.stdout.splitlines()]
        self.assertIn(['clippy', '-p', 'oasis7_agent_api', '--all-targets'], calls)
        self.assertIn(['clippy', '-p', 'oasis7_agent_harness', '--all-targets'], calls)
        for package in ('oasis7_context', 'oasis7_secure_envelope'):
            selected = [call for call in calls if package in call]
            self.assertTrue(selected, package)
            self.assertTrue(any('--all-targets' in call and '--locked' in call for call in selected))
            self.assertEqual(CONFIG['package_groups'][package], ['workspace_support', 'command_center'])
        self.assertTrue(any('oasis7_local_signer' in call and '--test' in call and 'context_cli' in call for call in calls))
        for path in ('crates/oasis7_context/**', 'crates/oasis7_secure_envelope/**'):
            selected_groups = {group for rule in CONFIG['rules'] if path in rule['match'] for group in rule['groups']}
            self.assertIn('fleet_health', selected_groups)

    def test_private_context_macos_uses_fixed_toolchain_and_declared_resource(self):
        fleet = JOBS['fleet-health']
        self.assertIn('rust_toolchain', CONFIG['resources']['fleet_health'])
        prepare = fleet.index('name: Prepare native filesystem contract Rust toolchain')
        verify = fleet.index('name: Verify private context filesystem and caller on macOS')
        self.assertLess(prepare, verify)
        hosted = fleet.index('name: Verify hosted access cross-process ownership on native host')
        self.assertLess(prepare, hosted)
        self.assertLess(hosted, verify)
        preparation = fleet[prepare:hosted]
        self.assertNotIn('if:', preparation)
        hosted_validation = fleet[hosted:verify]
        self.assertNotIn('if:', hosted_validation)
        self.assertIn('cargo test --locked -p oasis7_hosted_access', hosted_validation)
        self.assertIn('fleet_health', CONFIG['package_groups']['oasis7_hosted_access'])
        self.assertIn('rustup toolchain install "$RUST_TOOLCHAIN"', preparation)
        self.assertIn('rustup default "$RUST_TOOLCHAIN"', preparation)
        self.assertTrue((ROOT / 'rust-toolchain.toml').is_file())
        validation = fleet[verify:fleet.index('name: Ensure Linux process inspection dependency')]
        self.assertIn("if: runner.os == 'macOS'", validation)
        self.assertIn('cargo test --locked -p oasis7_context -p oasis7_secure_envelope --all-targets', validation)
        self.assertIn('cargo test --locked -p oasis7_local_signer --test context_cli', validation)

    def test_invalid_selected_group_cannot_silently_skip_command(self):
        driver = (ROOT / 'scripts/ci-tests.sh').read_text()
        self.assertIn('Unknown CI group:', driver)
        self.assertNotIn('disabled_by_scope_planner', driver)
        self.assertNotIn('required_gate_execution_contract', driver)
        self.assertNotIn('scripts/pm/', driver)

    def test_candidate_graph_rejects_missing_dependencies_platforms_and_failure_swallowing(self):
        self.assertTrue(validate_candidate_graph(WORKFLOW, CONFIG))
        mutations = [WORKFLOW.replace(', fleet-health]', ']'),
                     WORKFLOW.replace('windows-2022, macos-15]', 'windows-2022]'),
                     WORKFLOW.replace('    runs-on: ${{ matrix.os }}', '    runs-on: windows-2022'),
                     WORKFLOW.replace('      fail-fast: false', '      continue-on-error: true'),
                     WORKFLOW.replace('scripts/ci-rust-cache-admission.py install', 'scripts/ci-tests.sh full'),
                     WORKFLOW.replace(' --host-smoke', ''),
                     WORKFLOW.replace('    if: always()', '    if: success()'),
                     WORKFLOW.replace("fromJSON(needs.select.outputs.plan).scope == 'full'", 'false', 1)]
        for workflow in mutations:
            with self.subTest(workflow=workflow[-100:]), self.assertRaises(ValueError):
                validate_candidate_graph(workflow, CONFIG)
        new_config = dict(CONFIG, groups=[*CONFIG['groups'], 'new_group'])
        with self.assertRaises(ValueError):
            validate_candidate_graph(WORKFLOW, new_config)

    def test_candidate_driver_executes_atomic_rename_and_new_failure(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            scripts = root / 'scripts'
            scripts.mkdir()
            for filename in ('ci-tests.sh', 'find-python-with-module.sh', 'viewer-dependency-preflight.sh'):
                (scripts / filename).write_bytes((ROOT / 'scripts' / filename).read_bytes())
            driver = scripts / 'ci-tests.sh'
            driver.write_text(driver.read_text().replace('validate-codex-agent-config.py', 'renamed-validator.py'))
            command = ['bash', str(driver), 'required', '--group', 'codex_agent_config_validation', '--repo-root', str(root)]
            entry = scripts / 'renamed-validator.py'
            entry.write_text('print("candidate entry executed")\n')
            passed = subprocess.run(command, text=True, capture_output=True)
            self.assertEqual(passed.returncode, 0, passed.stderr)
            self.assertIn('candidate entry executed', passed.stdout)
            entry.write_text('raise SystemExit(42)\n')
            failed = subprocess.run(command, text=True, capture_output=True)
            self.assertEqual(failed.returncode, 42, failed.stderr)
            self.assertFalse((scripts / 'validate-codex-agent-config.py').exists())

    def test_concurrency_is_per_pr_and_per_non_pr_run(self):
        self.assertIn("format('pr-{0}', github.event.pull_request.number)", WORKFLOW)
        self.assertIn("format('run-{0}', github.run_id)", WORKFLOW)
        self.assertIn("cancel-in-progress: ${{ github.event_name == 'pull_request' }}", WORKFLOW)
        concurrency = WORKFLOW.split('concurrency:', 1)[1].split('permissions:', 1)[0]
        self.assertIn("'periodic-full-main'", concurrency)
        self.assertIn("format('candidate-full-{0}-{1}'", concurrency)
        self.assertIn('cron: "17 */4 * * *"', WORKFLOW)
        self.assertIn('github.workflow', concurrency)

    def test_partial_fast_builds_and_main_dispatched_candidates_never_write_cache(self):
        cases = (
            ('main-fast', 1, 'push', 'refs/heads/main', True, False),
            ('pr-fast', 1, 'pull_request', 'refs/pull/1/merge', False, False),
            ('periodic-full', 1, 'schedule', 'refs/heads/main', True, True),
            ('candidate-full', 1, 'workflow_dispatch', 'refs/heads/main', True, True),
            ('candidate-full', 1, 'workflow_dispatch', 'refs/heads/main', False, False),
            ('candidate-full', 1, 'workflow_dispatch', 'refs/heads/candidate', False, False),
            ('', 0, 'push', 'refs/heads/main', True, True),
        )
        for profile, version, event, ref, trusted_source, writer_expected in cases:
            values = {
                'always()': True, 'cancelled()': False, 'needs.select.result': 'success',
                'needs.select.outputs.run_oasis7_required': 'true',
                'needs.select.outputs.run_formal_process': 'true',
                'needs.select.outputs.trusted_source': str(trusted_source).lower(),
                'fromJSON(needs.select.outputs.plan).scope': 'full',
                'fromJSON(needs.select.outputs.plan).execution_profile': profile,
                'fromJSON(needs.select.outputs.plan).fast_plan_version': version,
                'fromJSON(needs.select.outputs.plan).fast_execution_version': 0,
                'fromJSON(needs.select.outputs.plan).ordinary_execution': '',
                'github.event_name': event, 'github.ref': ref, 'inputs.run_mode': 'full',
                'github.workflow_sha': 'a', 'github.sha': 'a',
            }
            for reader, writer in (('oasis7-test-build-candidate', 'oasis7-test-build-trusted'),
                                   ('formal-test-build', 'formal-test-build-trusted')):
                for name, expected in ((reader, not writer_expected), (writer, writer_expected)):
                    condition = JOBS[name].split('    if:', 1)[1].split('    runs-on:', 1)[0].strip().removeprefix('>-')
                    for token, value in values.items():
                        condition = condition.replace(token, repr(value))
                    condition = re.sub(r'!(?!=)', 'not ', condition.replace('&&', ' and ').replace('||', ' or '))
                    with self.subTest(job=name, profile=profile, trusted=trusted_source):
                        self.assertEqual(eval(' '.join(condition.split()), {'__builtins__': {}}, {}), expected)

    def test_selected_jobs_consume_fixed_source_and_same_plan(self):
        for name in ('baseline', 'oasis7-test-build-candidate', 'formal-test-build', 'oasis7-required',
                     'formal-process-tests', 'workspace-support', 'full-regression', 'candidate-full-regression',
                     'windows-rollout', 'fleet-health'):
            with self.subTest(job=name):
                self.assertIn('ref: ${{ needs.select.outputs.tested_sha }}', JOBS[name])
        for name in ('oasis7-test-build-candidate', 'formal-test-build', 'oasis7-required', 'formal-process-tests'):
            with self.subTest(plan_consumer=name):
                self.assertIn('name: ${{ needs.select.outputs.selection_artifact }}', JOBS[name])
                self.assertIn('--plan "$RUNNER_TEMP/selection.json"', JOBS[name])
        for name in ('formal-test-build', 'formal-process-tests'):
            with self.subTest(formal_policy_consumer=name):
                self.assertIn('name: ${{ needs.select.outputs.authority_artifact }}', JOBS[name])
                commands = [line for line in JOBS[name].splitlines()
                            if re.search(r'ci-test-artifacts.py (?:build|pack|run) ', line)]
                self.assertTrue(commands)
                for command in commands:
                    self.assertIn('--formal-config "$RUNNER_TEMP/ci-authority/ci-formal-process-suites.json"', command)
        for name in ('full-regression', 'candidate-full-regression'):
            self.assertIn('./scripts/ci-tests.sh full-extended', JOBS[name])
            self.assertNotIn('full-extended --plan', JOBS[name])
            self.assertNotIn('OASIS7_CI_TEST_BUNDLE', JOBS[name])
            self.assertNotIn('name: native-tests-', JOBS[name])
        center = (ROOT / '.github/workflows/command-center.yml').read_text()
        self.assertIn('ref: ${{ inputs.source_sha }}', center)
        self.assertIn('package_app: ${{ needs.select.outputs.command_center_package', JOBS['command-center'])
        self.assertIn('- name: Package development App\n        if: inputs.package_app', center)
        host = center.split('      - name: Check desktop host\n', 1)[1].split('      - name:', 1)[0]
        self.assertNotIn('if:', host)
        self.assertIn('--workspace --all-targets', host)
        self.assertIn('cancel-in-progress: false', center)

    def test_clippy_precedes_heavy_group_tests(self):
        driver = (ROOT / 'scripts/ci-tests.sh').read_text()
        ordinary = re.search(r'^    oasis7_required\).*?;;', driver, re.M).group()
        self.assertLess(ordinary.index('run_oasis7_required_clippy_for_group oasis7_required'),
                        ordinary.index('run_oasis7_required_tier_tests'))
        baseline = driver.split('run_rust_baseline() {', 1)[1].split('\n}', 1)[0]
        self.assertIn('run_oasis7_required_clippy_for_group rust_baseline', baseline)
        for group, prefix in [('consensus', 'consensus'), ('distfs', 'distfs'),
                              ('node', 'node'), ('net', 'net')]:
            case = re.search(r'^    ' + group + r'\).*?;;', driver, re.M).group()
            self.assertLess(case.index('run_oasis7_' + prefix + '_clippy'),
                            case.index('run_oasis7_' + prefix + '_tests'))


if __name__ == '__main__':
    unittest.main()
