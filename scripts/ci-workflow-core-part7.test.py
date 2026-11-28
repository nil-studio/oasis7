#!/usr/bin/env python3
"""Self-contained core workflow formal dependency, consumer graph, and periodic admission contract tests (part 7)."""
import itertools
import json
from pathlib import Path
import re
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parent.parent
RAW_WORKFLOW = (ROOT / '.github/workflows/rust.yml').read_text()


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


class WorkflowCorePart7(unittest.TestCase):
    def test_formal_dependency_caches_match_the_real_component_profile_environment(self):
        import runpy
        build_environment = runpy.run_path(str(ROOT / 'scripts/ci-test-artifacts.py'))['build_environment']
        expected = {f'CARGO_PROFILE_{profile}_{field}': value for profile in ('DEV', 'TEST')
                    for field, value in (('OPT_LEVEL', '0'), ('DEBUG_ASSERTIONS', 'true'),
                                         ('OVERFLOW_CHECKS', 'true'))}
        for component in ('debug', 'endpoint'):
            bodies = []
            for suffix in ('', '-trusted'):
                name = f'formal-{component}-build{suffix}'
                job = JOBS[name]
                environment = re.search(r'^    env:\n((?:      .*\n)+)', job, re.M)
                self.assertIsNotNone(environment, name)
                values = {key: json.loads(value) for key, value in re.findall(
                    r'^      ([A-Z_]+): (.*)$', environment[1], re.M)}
                self.assertEqual(values, expected)
                actual = build_environment(f'formal-{component}')
                self.assertEqual({key: actual[key] for key in expected}, values)
                caches = [step for step in re.findall(r'^      - .*?(?=^      - |\Z)', job, re.M | re.S)
                          if 'uses: Swatinem/rust-cache@' in step]
                self.assertEqual(len(caches), 2, name)
                for cache in caches:
                    self.assertIn(f'shared-key: ci-formal-{component}-deps-trusted-v1', cache)
                    self.assertIn('add-rust-environment-hash-key: true', cache)
                    self.assertIn('env-vars: CARGO CC CFLAGS CXX CMAKE RUST OASIS7_WASM OASIS7_FORMAL_BUILD_INPUTS', cache)
                    self.assertIn("OASIS7_FORMAL_BUILD_INPUTS: ${{ hashFiles('scripts/ci-formal-process-suites.json') }}", cache)
                    self.assertNotIn('scripts/ci-test-artifacts.py', cache)
                    self.assertIn('cache-on-failure: false', cache)
                self.assertNotIn('OASIS7_FORMAL_BUILD_INPUTS', job.split('    steps:', 1)[0])
                self.assertNotIn("hashFiles('.github/workflows/rust.yml')", job)
                self.assertIn('save-if: false', caches[0])
                self.assertNotIn('GH_TOKEN:', caches[0])
                self.assertNotIn('OASIS7_DEPENDENCY_CACHE_FAMILY:', caches[0])
                self.assertIn('PATH: ${{ steps.dependency-cache-tools.outputs.path }}', caches[1])
                self.assertIn(f'OASIS7_DEPENDENCY_CACHE_FAMILY: formal-{component}', caches[1])
                self.assertIn('GH_TOKEN: ${{ github.token }}', caches[1])
                self.assertIn('OASIS7_DEPENDENCY_CACHE_BUDGET_BYTES: ${{ env.CI_CACHE_REPOSITORY_BUDGET_BYTES }}', caches[1])
                self.assertNotRegex(caches[1].split('          save-if:', 1)[1],
                                    r'(?i)\b(?:always|cancelled|failure|success)\s*\(')
                for step in re.findall(r'^      - .*?(?=^      - |\Z)', job, re.M | re.S):
                    if step != caches[1]:
                        self.assertNotIn('steps.dependency-cache-tools.outputs.path', step)
                        self.assertNotIn('OASIS7_DEPENDENCY_CACHE_', step)
                bodies.append(caches)
            self.assertEqual(bodies[0], bodies[1])

    def test_formal_dependency_upload_only_admits_successful_trusted_full_components(self):
        def evaluate(condition, values):
            expression = condition.strip().removeprefix('>-').strip()
            if expression.startswith('${{'):
                expression = expression[3:-2].strip()
            for token, value in sorted(values.items(), key=lambda item: -len(item[0])):
                expression = expression.replace(token, repr(value))
            expression = re.sub(r'!(?!=)', 'not ', expression.replace('&&', ' and ').replace('||', ' or '))
            return bool(eval(' '.join(expression.split()), {'__builtins__': {}}, {}))

        for component in ('debug', 'endpoint'):
            for suffix in ('', '-trusted'):
                name = f'formal-{component}-build{suffix}'
                job = JOBS[name]
                header = job.split('    if:', 1)[1].split('    runs-on:', 1)[0]
                caches = [step for step in re.findall(r'^      - .*?(?=^      - |\Z)', job, re.M | re.S)
                          if 'uses: Swatinem/rust-cache@' in step]
                self.assertEqual(len(caches), 2, name)
                reader = caches[0].split('        if:', 1)[1].split('        env:', 1)[0]
                writer = caches[1].split('        if:', 1)[1].split('        env:', 1)[0]
                save = caches[1].split('          save-if:', 1)[1].split('          cache-on-failure:', 1)[0]
                for event, ref, source, matching, profile, mode in (
                        ('pull_request', 'refs/pull/1/merge', False, True, 'pr-fast', ''),
                        ('push', 'refs/heads/main', True, True, 'main-fast', ''),
                        ('push', 'refs/heads/main', True, True, '', ''),
                        ('schedule', 'refs/heads/main', True, True, 'periodic-full', ''),
                        ('workflow_dispatch', 'refs/heads/main', True, True, 'candidate-full', 'full'),
                        ('workflow_dispatch', 'refs/heads/main', False, True, 'candidate-full', 'full'),
                        ('workflow_dispatch', 'refs/heads/topic', True, True, 'candidate-full', 'full'),
                        ('workflow_dispatch', 'refs/heads/main', True, False, 'candidate-full', 'full'),
                        ('workflow_dispatch', 'refs/heads/main', True, True, 'candidate-full', 'other')):
                    for installation, result in itertools.product(('success', 'failure', 'skipped'),
                                                                  ('success', 'failure', 'cancelled')):
                        values = {'always()': True, 'cancelled()': False, 'needs.select.result': 'success',
                            'needs.select.outputs.run_formal_process': 'true',
                            'needs.select.outputs.trusted_source': str(source).lower(),
                            'fromJSON(needs.select.outputs.plan).execution_profile': profile,
                            'fromJSON(needs.select.outputs.plan).fast_plan_version': 1 if profile else 0,
                            'fromJSON(needs.select.outputs.plan).ordinary_main_fast_cache_writer_version': 0,
                            'github.job': name, 'github.event_name': event, 'github.ref': ref,
                            'github.workflow_sha': 'a' if matching else 'b', 'github.sha': 'a',
                            'inputs.run_mode': mode, 'steps.dependency-cache-tools.outcome': installation,
                            'job.status': result}
                        active = evaluate(header, values)
                        reads, writes = evaluate(reader, values), evaluate(writer, values)
                        eligible = (suffix == '-trusted' and ref == 'refs/heads/main' and matching and source
                                    and (event == 'schedule' or event == 'workflow_dispatch' and mode == 'full'))
                        with self.subTest(job=name, event=event, ref=ref, source=source, matching=matching,
                                          installation=installation, result=result, profile=profile):
                            self.assertFalse(reads and writes)
                            self.assertEqual(reads, not eligible)
                            self.assertEqual(writes, eligible and installation == 'success')
                            self.assertEqual(active and writes and evaluate(save, values),
                                             active and eligible and installation == 'success' and result == 'success')

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
            if name == 'full-regression' and 'scripts/ci-trusted-full-regression.py full "$SOURCE_SHA"' in JOBS[name]:
                self.assertIn("['./scripts/ci-tests.sh', 'full-extended']",
                              (ROOT / 'scripts/ci-trusted-full-regression.py').read_text())
            else:
                self.assertIn('./scripts/ci-tests.sh full-extended', JOBS[name])
            self.assertIn('needs: select', JOBS[name])
            self.assertNotIn('oasis7-test-build', JOBS[name])
        self.assertNotIn('oasis7-test-build', JOBS)
        self.assertIn('compression-level: 0', JOBS['oasis7-test-build-candidate'])
        self.assertIn('overwrite: true', JOBS['oasis7-test-build-candidate'])
        self.assertIn('--scope ordinary', JOBS['oasis7-test-build-candidate'])
        self.assertNotIn('run_formal_process', JOBS['oasis7-test-build-candidate'])
        self.assertNotIn('run_formal_process', JOBS['oasis7-test-build-trusted'])
        self.assertIn('merge-formal --debug-bundle', JOBS['formal-test-build'])
        self.assertIn('--split-formal', JOBS['formal-test-build'])
        self.assertIn('formal-tests-${{ matrix.shard }}-${{ github.sha }}', matrix)
        self.assertIn('--shard "$SHARD"', matrix)
        driver = (ROOT / 'scripts/ci-tests.sh').read_text()
        required_body = driver.split('run_oasis7_required_tier_tests() {', 1)[1].split('run_scenario_regression_tests()', 1)[0]
        self.assertNotIn('formal-live-local-tests.sh', required_body)
        self.assertIn('if [[ "$tier" == full ]]; then run_group formal_process; fi', driver)

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
        consumer = JOBS['oasis7-required']
        self.assertTrue(
            'needs: [select, oasis7-test-build-candidate, oasis7-test-build-trusted]' in consumer
            or 'needs: [select, fast-preflight, oasis7-test-build-candidate, oasis7-test-build-trusted]' in consumer
        )
        condition = consumer.split('    runs-on:', 1)[0].split('    if:', 1)[1].strip().removeprefix('>-').strip()
        self.assertIn('always() && !cancelled()', condition)
        self.assertIn("needs.select.result == 'success'", condition)
        states = ('success', 'skipped', 'failure', 'cancelled', '', 'unknown')
        for candidate in states:
            for trusted in states:
                with self.subTest(candidate=candidate, trusted=trusted):
                    values = {
                        'always()': True, 'cancelled()': False,
                        'needs.select.result': 'success',
                        'needs.oasis7-test-build-candidate.result': candidate,
                        'needs.oasis7-test-build-trusted.result': trusted,
                        'needs.select.outputs.run_oasis7_required': 'true',
                        'fromJSON(needs.select.outputs.plan).scope': 'targeted',
                        'fromJSON(needs.select.outputs.plan).fast_execution_version': 1,
                        'fromJSON(needs.select.outputs.plan).ordinary_execution': 'artifact',
                    }
                    actual = effective_job_condition(condition, values, ['success', candidate, trusted])
                    expected = (candidate, trusted) in (('success', 'skipped'), ('skipped', 'success'))
                    self.assertEqual(actual, expected)

    def test_consumers_require_their_actual_dependency_and_handle_cancellation(self):
        for name in ('oasis7-required', 'full-regression', 'candidate-full-regression'):
            header = JOBS[name].split('    runs-on:', 1)[0]
            condition = header.split('    if:', 1)[1].strip()
            condition = condition.removeprefix('>-').strip()
            self.assertIn('always() && !cancelled()', condition)
            self.assertIn("needs.select.result == 'success'", condition)
            if name == 'oasis7-required':
                self.assertIn("needs.oasis7-test-build-candidate.result == 'success'", condition)
                self.assertIn("needs.oasis7-test-build-trusted.result == 'success'", condition)
            for result in ('success', 'failure', 'skipped', 'cancelled', '', 'unknown'):
                for cancelled in (False, True):
                    for event, ref, mode, scope, expected in (
                            ('pull_request', 'refs/pull/1/merge', '', 'full', 'oasis7-required'),
                            ('push', 'refs/heads/main', '', 'full', 'oasis7-required'),
                            ('workflow_dispatch', 'refs/heads/main', 'full', 'explicit-full', 'full-regression'),
                            ('schedule', 'refs/heads/main', '', 'explicit-full', 'full-regression'),
                            ('workflow_dispatch', 'refs/heads/candidate', 'full', 'explicit-full', 'candidate-full-regression'),
                            ('workflow_dispatch', 'refs/heads/main', 'other', 'explicit-full', None)):
                        values = {
                            'always()': True, 'cancelled()': cancelled,
                            'needs.fast-preflight.result': 'success',
                            'needs.oasis7-test-build-candidate.result': result,
                            'needs.oasis7-test-build-trusted.result': 'skipped',
                            'needs.select.result': 'success' if name == 'oasis7-required' else result,
                            'needs.full-regression-provider.result': result,
                            'github.workflow_sha': 'source', 'github.sha': 'source',
                            'needs.select.outputs.run_oasis7_required': 'false',
                            'fromJSON(needs.select.outputs.plan).scope': scope,
                            'needs.select.outputs.trusted_source': str(ref == 'refs/heads/main').lower(),
                            'fromJSON(needs.select.outputs.plan).execution_profile': '',
                            'fromJSON(needs.select.outputs.plan).fast_plan_version': 0,
                            'fromJSON(needs.select.outputs.plan).ordinary_main_fast_cache_writer_version': 0,
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
            for candidate, trusted in (('skipped', 'skipped'), ('success', 'skipped'),
                                       ('skipped', 'success'), ('success', 'success'),
                                       ('failure', 'skipped'), ('cancelled', 'skipped')):
                for cancelled in (False, True):
                    values = {
                        'always()': True, 'cancelled()': cancelled, 'needs.select.result': 'success',
                        'needs.fast-preflight.result': 'success',
                        'needs.oasis7-test-build-candidate.result': candidate,
                        'needs.oasis7-test-build-trusted.result': trusted,
                        'needs.select.outputs.run_oasis7_required': 'true',
                        'needs.select.outputs.trusted_source': 'false',
                        'fromJSON(needs.select.outputs.plan).scope': 'targeted',
                        'fromJSON(needs.select.outputs.plan).execution_profile': 'pr-fast',
                        'fromJSON(needs.select.outputs.plan).fast_plan_version': 1,
                        'fromJSON(needs.select.outputs.plan).ordinary_main_fast_cache_writer_version': 0,
                        'fromJSON(needs.select.outputs.plan).fast_execution_version': version,
                        'fromJSON(needs.select.outputs.plan).ordinary_execution': mode,
                        'github.event_name': 'pull_request', 'github.ref': 'refs/pull/1/merge',
                        'github.workflow_sha': 'a', 'github.sha': 'b', 'inputs.run_mode': '',
                    }
                    for name in ('oasis7-test-build-candidate', 'oasis7-test-build-trusted', 'oasis7-required'):
                        condition = JOBS[name].split('    if:', 1)[1].split('    runs-on:', 1)[0].strip().removeprefix('>-')
                        for token, value in values.items():
                            condition = condition.replace(token, repr(value))
                        condition = re.sub(r'!(?!=)', 'not ', condition.replace('&&', ' and ').replace('||', ' or '))
                        actual = eval(' '.join(condition.split()), {'__builtins__': {}}, {})
                        if name == 'oasis7-required':
                            valid_producers = ((candidate, trusted) == ('skipped', 'skipped') if inline
                                               else (candidate, trusted) in (('success', 'skipped'), ('skipped', 'success')))
                            expected = not cancelled and valid_producers
                        elif name == 'oasis7-test-build-trusted':
                            expected = False
                        else:
                            expected = not inline and not cancelled
                        with self.subTest(job=name, version=version, mode=mode,
                                          candidate=candidate, trusted=trusted, cancelled=cancelled):
                            self.assertEqual(actual, expected)

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

    def test_periodic_observation_gates_full_without_blocking_other_events(self):
        select = JOBS['select']
        self.assertIn('needs: [periodic-admission, main-fast-admission]', select)
        condition = select.split('    if: >-\n', 1)[1].split('    runs-on:', 1)[0]
        for event, ref, draft, mode, admission, full, mf_adm, mf_run, expected in (
                ('pull_request', 'refs/pull/1/merge', False, '', 'skipped', '', 'skipped', '', True),
                ('pull_request', 'refs/pull/1/merge', True, '', 'skipped', '', 'skipped', '', False),
                ('push', 'refs/heads/main', False, '', 'skipped', '', 'success', 'true', True),
                ('push', 'refs/heads/main', False, '', 'skipped', '', 'success', 'false', False),
                ('push', 'refs/heads/main', False, '', 'skipped', '', 'failure', 'true', False),
                ('push', 'refs/heads/other', False, '', 'skipped', '', 'skipped', '', True),
                ('workflow_dispatch', 'refs/heads/main', False, 'full', 'skipped', '', 'skipped', '', True),
                ('workflow_dispatch', 'refs/heads/main', False, 'other', 'skipped', '', 'skipped', '', False),
                ('schedule', 'refs/heads/main', False, '', 'success', 'true', 'skipped', '', True),
                ('schedule', 'refs/heads/main', False, '', 'success', 'false', 'skipped', '', False),
                ('schedule', 'refs/heads/main', False, '', 'failure', 'true', 'skipped', '', False),
                ('schedule', 'refs/heads/main', False, '', 'cancelled', 'true', 'skipped', '', False)):
            expr = condition
            for token, value in {'always()': True, 'cancelled()': False,
                    'github.event_name': event, 'github.ref': ref,
                    'github.event.pull_request.draft': draft,
                    'github.event.action': 'opened', 'github.event.label.name': '',
                    'github.event.pull_request.head.repo.full_name': 'nil-studio/oasis7',
                    'github.repository': 'nil-studio/oasis7',
                    'inputs.run_mode': mode,
                    'needs.periodic-admission.result': admission,
                    'needs.periodic-admission.outputs.run_full': full,
                    'needs.main-fast-admission.result': mf_adm,
                    'needs.main-fast-admission.outputs.run_main_fast': mf_run}.items():
                expr = expr.replace(token, repr(value))
            expr = re.sub(r'!(?!=)', 'not ', expr.replace('&&', ' and ').replace('||', ' or '))
            with self.subTest(event=event, ref=ref, draft=draft, admission=admission, run_full=full, mf_adm=mf_adm, mf_run=mf_run):
                self.assertEqual(eval(' '.join(expr.split()), {'__builtins__': {}}, {}), expected)

    def test_selected_jobs_survive_skipped_admission_and_keep_failure_boundaries(self):
        names = (
            'command-center', 'baseline', 'rust-baseline', 'oasis7-test-build-candidate',
            'consensus', 'distfs', 'node', 'net', 'viewer-js-required',
            'viewer-performance-report', 'pixel-world-bridge', 'launcher-web',
            'workspace-support-legacy', 'workspace-support-other', 'custody-native', 'custody-native-contracts',
            'custody-wasm-production', 'custody-node',
            'scenario-regression', 'operational-contracts', 'packaging-contracts',
            'workflow-governance', 'codex-agent-config-validation', 'compile-metrics',
            'site-quality', 'doc-checker-contracts', 'cargo-tooling-contracts',
            'windows-rollout', 'macos-package-contract', 'fleet-health',
        )
        self.assertEqual(len(names), 30)
        producers = ('custody-native', 'custody-wasm-production')
        split_jobs = {*producers, 'custody-native-contracts', 'custody-node', 'workspace-support-other'}
        conditions = {name: re.search(r'^    if: (.*?)(?=^    [a-z-]+:|\Z)',
                                      JOBS[name], re.M | re.S)[1] for name in names}

        def values_for(name, event, selected=True, select='success', scope='targeted', results=None,
                       consolidation=0, direct_support=0, offline=0):
            values = {
                'needs.select.result': select,
                'needs.select.outputs.trusted_source': 'true' if event == 'push' else 'false',
                'fromJSON(needs.select.outputs.workspace_support_partitions)': ['custody', 'other'] if selected else [],
                'fromJSON(needs.select.outputs.plan).scope': scope,
                'fromJSON(needs.select.outputs.plan).fast_plan_version': 1,
                'fromJSON(needs.select.outputs.plan).ordinary_main_fast_cache_writer_version': 0,
                'fromJSON(needs.select.outputs.plan).contract_job_consolidation_version': consolidation,
                'fromJSON(needs.select.outputs.plan).command_center_offline_version': offline,
                'fromJSON(needs.select.outputs.plan).workspace_support_direct_gate_version': direct_support,
                'fromJSON(needs.select.outputs.plan).fast_execution_version': 1,
                'fromJSON(needs.select.outputs.plan).ordinary_execution': 'artifact',
                'fromJSON(needs.select.outputs.plan).custody_execution_version': 1 if selected and name in split_jobs and not direct_support else 0,
                'fromJSON(needs.select.outputs.plan).custody_compile_proof_version': 1 if selected and not direct_support else 0,
                'fromJSON(needs.select.outputs.plan).execution_profile': {
                    'pull_request': 'pr-fast', 'push': 'main-fast',
                    'workflow_dispatch': 'candidate-full',
                }[event],
                'github.event_name': event,
                'github.ref': 'refs/pull/1/merge' if event == 'pull_request' else 'refs/heads/main',
                'github.workflow_sha': 'workflow', 'github.sha': 'workflow',
                'inputs.run_mode': 'full' if event == 'workflow_dispatch' else '',
            }
            values.update({token: str(selected).lower() for token in
                           re.findall(r'needs\.select\.outputs\.run_[a-z0-9_]+', conditions[name])})
            values.update({f'needs.{producer}.result': (results or {}).get(producer, 'success')
                           for producer in producers})
            return values

        # These events deliberately skip the schedule-only admission ancestor.
        # The effective default success() is false even after select succeeds.
        for event in ('pull_request', 'push', 'workflow_dispatch'):
            for name, condition in conditions.items():
                with self.subTest(job=name, event=event, mutation='remove status prefix'):
                    values = values_for(name, event)
                    self.assertTrue(effective_job_condition(condition, values, ['skipped', 'success']))
                    original_guard = re.sub(r'always\(\)\s*&&\s*!cancelled\(\)\s*&&\s*',
                                            '', condition, count=1)
                    self.assertNotEqual(original_guard, condition)
                    self.assertFalse(effective_job_condition(original_guard, values, ['skipped', 'success']))
                    self.assertTrue(effective_job_condition(original_guard, values, ['success', 'success']))
                for select, selected, cancelled in itertools.product(
                        ('success', 'failure', 'cancelled', 'skipped'), (False, True), (False, True)):
                    with self.subTest(job=name, event=event, select=select, selected=selected, cancelled=cancelled):
                        values = values_for(name, event, selected, select)
                        self.assertEqual(effective_job_condition(condition, values, ['skipped', select],
                                                                 cancelled=cancelled),
                                         select == 'success' and selected and not cancelled)
                if name not in split_jobs:
                    # Old trusted planners retain the complete full-scope fallback.
                    with self.subTest(job=name, event=event, scope='full'):
                        values = values_for(name, event, selected=False, scope='full')
                        self.assertTrue(effective_job_condition(condition, values, ['skipped', 'success']))
            for ver in (1, 2, 3):
                for retired in ('codex-agent-config-validation', 'compile-metrics', 'macos-package-contract'):
                    for scope in ('targeted', 'full', 'explicit-full'):
                        values = values_for(retired, event, selected=True, scope=scope, consolidation=ver)
                        self.assertFalse(effective_job_condition(conditions[retired], values, ['skipped', 'success']))
            for scope in ('targeted', 'full', 'explicit-full'):
                values = values_for('command-center', event, selected=True, scope=scope, consolidation=5, offline=1)
                self.assertFalse(effective_job_condition(conditions['command-center'], values, ['skipped', 'success']))
            absorbed_center = values_for('baseline', event, selected=False, consolidation=5, offline=1)
            absorbed_center['needs.select.outputs.run_command_center'] = 'true'
            self.assertTrue(effective_job_condition(conditions['baseline'], absorbed_center, ['skipped', 'success']))
            for retired in ('consensus', 'pixel-world-bridge', 'viewer-performance-report'):
                self.assertTrue(effective_job_condition(
                    conditions[retired], values_for(retired, event, selected=True, consolidation=1), ['skipped', 'success']))
                for ver in (2, 3):
                    for scope in ('targeted', 'full', 'explicit-full'):
                        values = values_for(retired, event, selected=True, scope=scope, consolidation=ver)
                        self.assertFalse(effective_job_condition(conditions[retired], values, ['skipped', 'success']))
            for retired in ('packaging-contracts', 'cargo-tooling-contracts', 'windows-rollout'):
                for ver in (1, 2):
                    self.assertTrue(effective_job_condition(
                        conditions[retired], values_for(retired, event, selected=True, consolidation=ver), ['skipped', 'success']))
                for scope in ('targeted', 'full', 'explicit-full'):
                    values = values_for(retired, event, selected=True, scope=scope, consolidation=3)
                    self.assertFalse(effective_job_condition(conditions[retired], values, ['skipped', 'success']))
            # Consolidated owners trigger even if only the absorbed group was selected.
            for host, flag, ver in (
                ('baseline', 'needs.select.outputs.run_macos_package_contract', 1),
                ('baseline', 'needs.select.outputs.run_macos_package_contract', 2),
                ('baseline', 'needs.select.outputs.run_macos_package_contract', 3),
                ('doc-checker-contracts', 'needs.select.outputs.run_compile_metrics', 1),
                ('doc-checker-contracts', 'needs.select.outputs.run_compile_metrics', 2),
                ('doc-checker-contracts', 'needs.select.outputs.run_compile_metrics', 3),
                ('node', 'needs.select.outputs.run_consensus', 2),
                ('node', 'needs.select.outputs.run_consensus', 3),
                ('scenario-regression', 'needs.select.outputs.run_pixel_world_bridge', 2),
                ('scenario-regression', 'needs.select.outputs.run_pixel_world_bridge', 3),
                ('viewer-js-required', 'needs.select.outputs.run_viewer_performance_report', 2),
                ('viewer-js-required', 'needs.select.outputs.run_viewer_performance_report', 3),
                ('doc-checker-contracts', 'needs.select.outputs.run_packaging_contracts', 3),
                ('scenario-regression', 'needs.select.outputs.run_cargo_tooling_contracts', 3),
                ('fleet-health', 'needs.select.outputs.run_windows_rollout', 3),
            ):
                absorbed = values_for(host, event, selected=False, consolidation=ver)
                absorbed[flag] = 'true'
                self.assertTrue(effective_job_condition(conditions[host], absorbed, ['skipped', 'success']))
            # Direct workspace-support gate skips legacy and join jobs while running workspace-support-other.
            direct_legacy = values_for('workspace-support-legacy', event, selected=True, direct_support=1)
            self.assertFalse(effective_job_condition(conditions['workspace-support-legacy'], direct_legacy, ['skipped', 'success']))
            direct_other = values_for('workspace-support-other', event, selected=True, direct_support=1)
            direct_other['fromJSON(needs.select.outputs.workspace_support_partitions)'] = ['other']
            self.assertTrue(effective_job_condition(conditions['workspace-support-other'], direct_other, ['skipped', 'success']))
            summary_if = re.search(r'^    if: (.*?)(?=^    [a-z-]+:|\Z)', JOBS['workspace-support'], re.M | re.S)[1]
            direct_summary = values_for('workspace-support-legacy', event, selected=True, direct_support=1)
            self.assertFalse(effective_job_condition(summary_if, direct_summary, ['skipped', 'success']))
            failed_select_summary = values_for('workspace-support-legacy', event, selected=False, select='failure', direct_support=1)
            self.assertTrue(effective_job_condition(summary_if, failed_select_summary, ['skipped', 'failure']))

        # An explicit status function also bypasses implicit producer success.
        # Every producer must therefore be checked by the actual Node job guard.
        states = ('success', 'failure', 'cancelled', 'skipped')
        for results in itertools.product(states, repeat=len(producers)):
            values = values_for('custody-node', 'pull_request', results=dict(zip(producers, results)))
            with self.subTest(producers=results):
                self.assertEqual(effective_job_condition(conditions['custody-node'], values,
                                                         ['skipped', 'success', *results]),
                                 all(result == 'success' for result in results))

    def test_periodic_skips_only_no_full_and_keeps_select_failure_visible(self):
        for name in ('formal-process', 'explicit-full-gate'):
            job = JOBS[name]
            self.assertIn('always()', job)
            self.assertIn("needs.select.result != 'skipped'", job)
            self.assertNotIn("needs.select.result == 'success'", job.split('    runs-on:', 1)[0])
        formal_cond = re.search(r'^    if: (.*?)(?=^    [a-z-]+:|\Z)', JOBS['formal-process'], re.M | re.S)[1]
        for select, run_formal, expected in (
            ('success', 'false', False),
            ('success', '', False),
            ('success', 'true', True),
            ('failure', '', True),
            ('cancelled', '', True),
            ('skipped', 'true', False),
        ):
            actual = effective_job_condition(formal_cond, {
                'needs.select.result': select,
                'needs.select.outputs.run_formal_process': run_formal,
                'github.event_name': 'pull_request',
                'inputs.run_mode': '',
            }, [select])
            self.assertEqual(actual, expected, (select, run_formal))
        gate = JOBS['explicit-full-gate']
        self.assertNotIn('periodic-admission', re.search(r'    needs: \[(.*)\]', gate).group(1))
        admission = JOBS['periodic-admission']
        self.assertIn('timeout-minutes: 3', admission)
        self.assertIn('group: rust-periodic-admission-main', admission)
        self.assertIn('cancel-in-progress: false', admission)
        self.assertIn('--github-output "$GITHUB_OUTPUT"', admission)
        self.assertIn('name: ci-periodic-admission-${{ github.run_id }}-${{ github.run_attempt }}', admission)
        self.assertIn('if-no-files-found: error', admission)
        self.assertNotIn('continue-on-error', admission)
        self.assertIn("needs.periodic-admission.result == 'success'", JOBS['select'])


if __name__ == '__main__':
    unittest.main()
