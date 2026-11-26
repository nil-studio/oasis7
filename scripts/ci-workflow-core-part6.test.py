#!/usr/bin/env python3
"""Self-contained core workflow baseline, document range, and formal component build/merge contract tests (part 6)."""
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

TRUSTED_SOURCE_WRITERS = (
    'oasis7-test-build-trusted', 'formal-debug-build-trusted',
    'formal-endpoint-build-trusted', 'full-regression',
)


def validate_mold_checkout_reuse(tools, setup, source):
    def inputs(step):
        fields = re.findall(r'^          ([a-z][a-z-]*): (.*)$', step, re.M)
        assert len(fields) == len(dict(fields)), 'duplicate checkout input'
        return dict(fields)

    def guards(step):
        return re.findall(r'^        if: (.+)$', step, re.M)

    first, selected = inputs(tools), inputs(source)
    assert set(selected) <= {'ref', 'fetch-depth', 'persist-credentials'}
    assert selected.get('persist-credentials') == 'false'
    ref = selected['ref'].removeprefix('${{').removesuffix('}}').strip()
    assert ref in ('needs.select.outputs.tested_sha', 'github.sha')
    depth = selected.get('fetch-depth', '1')
    assert depth in ('0', '1')
    different = f"{ref} == '' || github.workflow_sha != {ref}"
    sparse = "${{ (" + different + ") && '.github/actions/setup-linux-mold' || '' }}"
    first_depth = (depth if depth == '1' else "${{ " +
                   f"{ref} != '' && github.workflow_sha == {ref} && '0' || '1'" + " }}")
    assert first == {'ref': '${{ github.workflow_sha }}', 'persist-credentials': 'false',
                     'fetch-depth': first_depth, 'sparse-checkout': sparse}
    platform = guards(tools)
    assert platform == guards(setup) and platform in ([], ["runner.os == 'Linux'"])
    fallback = ("runner.os != 'Linux' || " if platform else '') + different
    assert guards(source) == ["${{ " + fallback + " }}"], 'only the identical nonempty source may skip checkout'


def selected_source_steps(name):
    """Normalize only the equivalent first checkout of admitted main writers."""
    steps = JOBS[name].split('    steps:\n', 1)[1]
    checkouts = [step for step in re.findall(r'^      - .*?(?=^      - |\Z)', steps, re.M | re.S)
                 if re.search(r'^\s+(?:- )?uses: actions/checkout@', step, re.M)]
    if 'uses: ./.github/actions/setup-linux-mold' in steps:
        assert len(checkouts) == 2, 'workflow tools and one selected checkout are required'
        assert '          ref: ${{ github.workflow_sha }}\n' in checkouts[0], 'linker tools must use the workflow revision'
        if '          sparse-checkout: .github/actions/setup-linux-mold\n' not in checkouts[0]:
            setup = next(step for step in re.findall(r'^      - .*?(?=^      - |\Z)', steps, re.M | re.S)
                         if 'uses: ./.github/actions/setup-linux-mold' in step)
            validate_mold_checkout_reuse(checkouts[0], setup, checkouts[-1])
        else:
            assert not re.findall(r'^        if: .*workflow_sha', checkouts[-1], re.M), (
                'sparse mold checkout cannot replace the selected source')
    else:
        assert len(checkouts) == 1, 'one selected checkout is required'
    if name in TRUSTED_SOURCE_WRITERS:
        assert steps.startswith(checkouts[0]), 'writer checkout must be first'
    checkout = checkouts[-1]
    refs = re.findall(r'^          ref: (.*)$', checkout, re.M)
    tested = '${{ needs.select.outputs.tested_sha }}'
    allowed = (tested, '${{ github.sha }}') if name in TRUSTED_SOURCE_WRITERS else (tested,)
    assert len(refs) == 1 and refs[0] in allowed, 'checkout must use the selected or proven identical source'
    # Do not normalize any other checkout setting, later step, or candidate ref.
    if refs[0] == '${{ github.sha }}':
        for step in checkouts:
            steps = steps.replace(step, step.replace('github.sha', 'needs.select.outputs.tested_sha'), 1)
    return steps


class WorkflowCorePart6(unittest.TestCase):
    def test_command_center_is_reusable_and_required(self):
        workflow = (ROOT / '.github/workflows/command-center.yml').read_text()
        self.assertIn('  workflow_call:', workflow)
        self.assertIn('\ncache-mode: read\n', workflow)
        self.assertIn('ref: ${{ inputs.source_sha }}', workflow)
        self.assertIn('persist-credentials: false', workflow)
        self.assertNotIn('  pull_request:', workflow)
        self.assertNotIn('  push:', workflow)
        self.assertIn('uses: ./.github/workflows/command-center.yml', JOBS['command-center'])
        self.assertIn('fromJSON(needs.select.outputs.plan).command_center_offline_version != 1', JOBS['command-center'])
        self.assertIn('run_command_center: ${{ steps.select.outputs.run_command_center }}', JOBS['select'])
        self.assertEqual(CONFIG.get('command_center_offline_version'), 1)
        self.assertEqual(CONFIG.get('group_jobs', {}).get('command_center'), 'baseline')
        for command in ('cargo test --locked', '--example export_types', 'npm test',
                        'npm run build', 'cargo clippy --locked', 'desktop:build -- --debug'):
            self.assertIn(command, workflow)
        self.assertNotIn('continue-on-error:', workflow)
        for package in ('oasis7_signer_protocol', 'oasis7_context', 'oasis7_local_signer', 'oasis7_secure_envelope'):
            self.assertNotIn('command_center', CONFIG['package_groups'][package])

    def test_corpus_data_uses_baseline_and_implementation_uses_platforms(self):
        for path in (
            '.github/workflows/document-corpus-platform.yml',
            'doc/.governance/document-corpus-inventory.json',
            'doc/.governance/document-corpus',
            'doc/testing/evidence/inventory.json',
            'scripts/document_corpus.py',
            'scripts/document-corpus-inventory.py',
            'scripts/document-corpus-inventory-check.py',
            'scripts/doc-evidence-inventory-check.py',
        ):
            self.assertFalse((ROOT / path).exists(), path)
        driver = (ROOT / 'scripts/ci-tests.sh').read_text()
        self.assertIn('baseline) run_required_gate_checks', driver)
        baseline = driver.split('run_required_gate_checks() {', 1)[1].split('\n}', 1)[0]
        self.assertIn('run_product_doc_governance_check', baseline)
        governance = (ROOT / 'scripts/doc-governance-check.sh').read_text()
        self.assertNotIn('scripts/document-corpus-inventory-check.py', governance)

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

    def test_formal_components_build_in_parallel_before_the_stable_mergers(self):
        producers = ('formal-debug-build', 'formal-debug-build-trusted',
                     'formal-endpoint-build', 'formal-endpoint-build-trusted')
        for name in producers:
            with self.subTest(producer=name):
                self.assertIn(name, JOBS)
                job = JOBS[name]
                dependencies = re.search(r'    needs: (.*)', job).group(1)
                expected = '[select, cache-bootstrap-maintenance]' if name.endswith('-trusted') else 'select'
                self.assertEqual(dependencies, expected)
                self.assertIn('ref: ${{ needs.select.outputs.tested_sha }}', selected_source_steps(name))
                self.assertIn('name: ${{ needs.select.outputs.selection_artifact }}', job)
                self.assertIn('name: ${{ needs.select.outputs.authority_artifact }}', job)
                component = 'formal-debug' if 'debug' in name else 'formal-endpoint'
                self.assertIn(f'build --scope {component}', job)
                self.assertIn('--plan "$RUNNER_TEMP/selection.json"', job)
                self.assertIn('--formal-config "$RUNNER_TEMP/ci-authority/ci-formal-process-suites.json"', job)
                self.assertNotIn('merge-formal', job)
                self.assertNotIn('--split-formal', job)
        for name in ('formal-test-build', 'formal-test-build-trusted'):
            job = JOBS[name]
            dependencies = re.search(r'    needs: \[(.*)\]', job).group(1).split(', ')
            self.assertEqual(set(dependencies), {'select', *producers})
            self.assertEqual(len(dependencies), len(set(dependencies)))
            self.assertIn('cache-mode: read', job)
            self.assertNotIn('actions/cache/', job)
            self.assertNotIn('Swatinem/rust-cache', job)
            self.assertNotIn('ci-workspace-cache.py', job)
            self.assertNotIn('ci-test-artifacts.py build ', job)
            self.assertIn('ci-test-artifacts.py merge-formal ', job)
            self.assertIn('--split-formal', job)

    def test_formal_mergers_reject_every_incomplete_producer_combination(self):
        import runpy
        aggregate = runpy.run_path(str(ROOT / 'scripts/ci-formal-plan.py'))['aggregate']
        producers = ('formal-debug-build', 'formal-debug-build-trusted',
                     'formal-endpoint-build', 'formal-endpoint-build-trusted')
        # These states include absent/unknown API results, not just ordinary
        # failure. An unselected alternative is the only permissible skip.
        states = ('success', 'skipped', 'failure', 'cancelled', '', 'unknown')
        for trusted, cancelled in itertools.product((False, True), repeat=2):
            values = {
                'always()': True, 'cancelled()': cancelled,
                'needs.select.result': 'success',
                'needs.select.outputs.run_formal_process': 'true',
                'needs.select.outputs.trusted_source': str(trusted).lower(),
                'fromJSON(needs.select.outputs.plan).execution_profile': 'periodic-full' if trusted else 'pr-fast',
                'fromJSON(needs.select.outputs.plan).fast_plan_version': 1,
                'fromJSON(needs.select.outputs.plan).ordinary_main_fast_cache_writer_version': 0,
                'github.event_name': 'schedule' if trusted else 'pull_request',
                'github.ref': 'refs/heads/main' if trusted else 'refs/pull/1/merge',
                'github.workflow_sha': 'a', 'github.sha': 'a', 'inputs.run_mode': '',
            }
            expected_success = ('skipped', 'success', 'skipped', 'success') if trusted else (
                'success', 'skipped', 'success', 'skipped')
            for results in itertools.product(states, repeat=4):
                values.update({f'needs.{name}.result': result for name, result in zip(producers, results)})
                merger_results = {}
                for name in ('formal-test-build', 'formal-test-build-trusted'):
                    condition = JOBS[name].split('    if:', 1)[1].split('    runs-on:', 1)[0].strip().removeprefix('>-')
                    for token, value in sorted(values.items(), key=lambda item: -len(item[0])):
                        condition = condition.replace(token, repr(value))
                    condition = re.sub(r'!(?!=)', 'not ', condition.replace('&&', ' and ').replace('||', ' or '))
                    actual = bool(eval(' '.join(condition.split()), {'__builtins__': {}}, {}))
                    expected = not cancelled and results == expected_success and name.endswith('-trusted') == trusted
                    with self.subTest(job=name, trusted=trusted, cancelled=cancelled, producers=results):
                        self.assertEqual(actual, expected)
                    merger_results[name] = {'result': 'success' if actual else 'skipped'}
                needs = dict(merger_results, select={'result': 'success'},
                             **{'formal-process-tests': {'result': 'success'}})
                if not cancelled and results == expected_success:
                    self.assertEqual(aggregate(True, needs), 'passed')
                else:
                    with self.assertRaises(ValueError):
                        aggregate(True, needs)

    def test_formal_component_artifacts_preserve_successful_prior_attempt_outputs(self):
        for component in ('debug', 'endpoint'):
            name = f'formal-{component}-build'
            self.assertIn(name, JOBS)
            job = JOBS[name]
            self.assertIn('artifact: ${{ steps.component-artifact.outputs.artifact }}', job)
            step = job.split('      - id: component-artifact\n', 1)[1].split('      - ', 1)[0]
            script = textwrap.dedent(step.split('        run: |\n', 1)[1])
            with tempfile.TemporaryDirectory() as directory:
                outputs = []
                for attempt in (1, 2):
                    output = Path(directory) / str(attempt)
                    result = subprocess.run(['bash', '-euo', 'pipefail', '-c', script],
                        env=dict(os.environ, GITHUB_RUN_ID='123', GITHUB_RUN_ATTEMPT=str(attempt),
                                 GITHUB_OUTPUT=str(output)), text=True, capture_output=True)
                    self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                    values = dict(line.split('=', 1) for line in output.read_text().splitlines())
                    self.assertEqual(values, {'artifact': f'formal-{component}-123-{attempt}'})
                    outputs.append(values['artifact'])
                self.assertNotEqual(*outputs)
            self.assertIn('name: ${{ steps.component-artifact.outputs.artifact }}', job)
            for merger in ('formal-test-build', 'formal-test-build-trusted'):
                body = JOBS[merger]
                self.assertIn(f'needs.formal-{component}-build.outputs.artifact', body)
                self.assertIn(f'needs.formal-{component}-build-trusted.outputs.artifact', body)
                self.assertNotIn(f'name: formal-{component}-${{{{ github.run_id }}}}', body)

    def test_formal_merge_stops_before_pack_when_component_validation_fails(self):
        job = JOBS['formal-test-build']
        self.assertIn('      - name: Merge and pack formal process dependencies\n', job)
        step = job.split('      - name: Merge and pack formal process dependencies\n', 1)[1].split('      - ', 1)[0]
        script = textwrap.dedent(step.split('        run: |\n', 1)[1])
        stub = '''python3() {
          printf '%s\\n' "$*" >> "$CALLS";
          if [[ "$2" == merge-formal ]]; then return "$MERGE_EXIT"; fi
          return 0;
        }
        '''
        with tempfile.TemporaryDirectory() as directory:
            calls = Path(directory) / 'calls'
            for code in (0, 17):
                calls.unlink(missing_ok=True)
                result = subprocess.run(['bash', '-euo', 'pipefail', '-c', stub + script],
                    env=dict(os.environ, RUNNER_TEMP=directory, SOURCE_SHA='a' * 40,
                             CALLS=str(calls), MERGE_EXIT=str(code)), text=True, capture_output=True)
                self.assertEqual(result.returncode, code, result.stdout + result.stderr)
                commands = calls.read_text().splitlines()
                self.assertEqual(len(commands), 1 if code else 2)
                self.assertIn(' merge-formal ', commands[0])
                self.assertIn('--debug-bundle ', commands[0])
                self.assertIn('--endpoint-bundle ', commands[0])
                for command in commands:
                    self.assertIn('--plan ', command)
                    self.assertIn('--formal-config ', command)
                if not code:
                    self.assertIn(' pack ', commands[1])
                    self.assertIn('--split-formal', commands[1])

    def test_formal_component_transport_preserves_modes_and_stops_after_build_failure(self):
        import tarfile
        with tempfile.TemporaryDirectory(prefix='oasis7-formal-workflow-') as directory:
            root = Path(directory)
            (root / 'scripts').mkdir()
            # Substitute only the producer CLI. The production Bash, tar and
            # unpack commands run unchanged against real executable fixtures.
            (root / 'scripts/ci-test-artifacts.py').write_text(textwrap.dedent('''
                import json, os, pathlib, sys
                arguments = sys.argv[1:]
                pathlib.Path(os.environ['CALLS']).write_text(json.dumps(arguments))
                if int(os.environ['BUILD_EXIT']):
                    raise SystemExit(int(os.environ['BUILD_EXIT']))
                bundle = pathlib.Path(arguments[arguments.index('--bundle') + 1])
                (bundle / 'bin').mkdir(parents=True)
                (bundle / 'lib').mkdir()
                (bundle / 'manifest.json').write_text('{}')
                executable = bundle / 'bin/fixture'
                executable.write_text('#!/bin/sh\\nexit 0\\n')
                executable.chmod(0o751)
                (bundle / 'lib/fixture.so').write_bytes(b'fixture-library')
            '''))
            calls = root / 'calls'
            for component in ('debug', 'endpoint'):
                job = JOBS[f'formal-{component}-build']
                step = job.split(f'      - name: Build and archive formal-{component} component\n', 1)[1].split('      - ', 1)[0]
                script = textwrap.dedent(step.split('        run: |\n', 1)[1])
                archive = root / f'formal-{component}-tests.tar.gz'
                for code in (23, 0):
                    result = subprocess.run(['bash', '-euo', 'pipefail', '-c', script], cwd=root,
                        env=dict(os.environ, RUNNER_TEMP=directory, SOURCE_SHA='a' * 40,
                                 CALLS=str(calls), BUILD_EXIT=str(code)), text=True, capture_output=True)
                    self.assertEqual(result.returncode, code, result.stdout + result.stderr)
                    arguments = json.loads(calls.read_text())
                    self.assertEqual(arguments[:3], ['build', '--scope', f'formal-{component}'])
                    for option, relative in (('--plan', 'selection.json'),
                            ('--formal-config', 'ci-authority/ci-formal-process-suites.json'),
                            ('--cache-state', f'formal-{component}-cache-state.json')):
                        self.assertEqual(arguments[arguments.index(option) + 1], str(root / relative))
                    self.assertEqual(archive.exists(), code == 0)
                with tarfile.open(archive) as payload:
                    self.assertEqual(payload.getmember('./bin/fixture').mode, 0o751)
                    self.assertEqual(payload.extractfile('./lib/fixture.so').read(), b'fixture-library')
                destination = root / 'formal-components' / component
                destination.mkdir(parents=True)
                archive.rename(destination / archive.name)
                (root / f'formal-{component}-tests').rename(root / f'original-{component}')
            step = JOBS['formal-test-build'].split('      - name: Unpack Formal components preserving executable modes\n', 1)[1].split('      - ', 1)[0]
            script = textwrap.dedent(step.split('        run: |\n', 1)[1])
            result = subprocess.run(['bash', '-euo', 'pipefail', '-c', script], cwd=root,
                env=dict(os.environ, RUNNER_TEMP=directory), text=True, capture_output=True)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            for component in ('debug', 'endpoint'):
                executable = root / f'formal-{component}-tests/bin/fixture'
                self.assertEqual(executable.stat().st_mode & 0o777, 0o751)
                self.assertEqual(subprocess.run([str(executable)]).returncode, 0)
            # A missing selected component must stop the actual unpack shell.
            (root / 'formal-components/endpoint/formal-endpoint-tests.tar.gz').unlink()
            result = subprocess.run(['bash', '-euo', 'pipefail', '-c', script + '\nprintf unreachable'],
                cwd=root, env=dict(os.environ, RUNNER_TEMP=directory), text=True, capture_output=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertNotIn('unreachable', result.stdout)

    def test_formal_merge_artifact_names_reject_missing_cross_run_and_cross_role_inputs(self):
        job = JOBS['formal-test-build']
        step = job.split('      - name: Require exact successful Formal component artifacts\n', 1)[1].split('      - ', 1)[0]
        script = textwrap.dedent(step.split('        run: |\n', 1)[1])
        cases = [('formal-debug-123-1', 'formal-endpoint-123-2', True),
                 ('formal-debug-123-2', 'formal-endpoint-123-1', True),
                 ('formal-debug-123-2', 'formal-endpoint-123-2', True),
                 ('', 'formal-endpoint-123-2', False),
                 ('formal-debug-123-2', '', False),
                 ('formal-debug-124-2', 'formal-endpoint-123-2', False),
                 ('formal-debug-123-2', 'formal-endpoint-124-2', False),
                 ('formal-endpoint-123-2', 'formal-debug-123-2', False),
                 ('formal-debug-123-0', 'formal-endpoint-123-2', False),
                 ('formal-debug-123-2', 'formal-endpoint-123-unknown', False)]
        for debug, endpoint, expected in cases:
            result = subprocess.run(['bash', '-euo', 'pipefail', '-c', script], text=True, capture_output=True,
                env=dict(os.environ, GITHUB_RUN_ID='123', GITHUB_RUN_ATTEMPT='2',
                         DEBUG_ARTIFACT=debug, ENDPOINT_ARTIFACT=endpoint))
            with self.subTest(debug=debug, endpoint=endpoint):
                self.assertEqual(result.returncode == 0, expected, result.stdout + result.stderr)


if __name__ == '__main__':
    unittest.main()
