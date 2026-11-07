#!/usr/bin/env python3
"""Regress the final DAG and execution boundaries, without Actions credentials."""
import ast
import json
import itertools
import os
import subprocess
import sys
import tempfile
import textwrap
from pathlib import Path
import re
import shlex
import unittest

ROOT = Path(__file__).resolve().parent.parent
WORKFLOW = (ROOT / '.github/workflows/rust.yml').read_text()
CONFIG = json.loads((ROOT / 'scripts/ci-required-scope.json').read_text())
def workspace_support_fixture_functions(driver):
    prefix = ('ci_python=' + shlex.quote(sys.executable) + '\n'
              + 'driver_dir=' + shlex.quote(str(ROOT / 'scripts')) + '\n')
    if '# ci-driver-library: functions-v1' in driver.splitlines():
        # Load the real transitive definitions before the fixture overrides its
        # recording boundaries, without maintaining a second dependency list.
        return prefix + 'source "$driver_dir/ci-tests.sh"\n'
    # Extract the real transitive shell dependency, rather than supplying a
    # no-op helper, for historical drivers without the source interface.
    names = ('run_worker_wasm_compile_profiles',
             'run_oasis7_workspace_support_crate_tests')
    functions = [re.search(r'^' + name + r'\(\) \{\n.*?^\}',
                           driver, re.M | re.S).group() for name in names]
    return prefix + '\n'.join(functions) + '\n'


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


def rust_cache_steps(job):
    return [step for step in re.findall(r'^      - .*?(?=^      - |\Z)', job, re.M | re.S)
            if 'uses: Swatinem/rust-cache@' in step]


def web_cache_layout(cache_workflow, release_workflow, admission_source):
    """Accept only the complete old or new sole-writer wiring during migration."""
    assignments = [node for node in ast.parse(admission_source).body
                   if isinstance(node, ast.Assign)
                   and any(isinstance(target, ast.Name) and target.id == 'FAMILIES'
                           for target in node.targets)]
    assert len(assignments) == 1, 'one literal admission family registry required'
    families = ast.literal_eval(assignments[0].value)
    namespace = 'release-packages-web-wasm-trusted-v3'
    old = (namespace, '.github/workflows/release-packages.yml', 'build-web-dist')
    new = (namespace, '.github/workflows/rust-cache.yml', 'web-wasm-cache')
    registered = families.get('release-web')
    assert registered in (old, new), 'unknown Web writer identity'
    assert [name for name, identity in families.items() if identity[0] == namespace] == ['release-web']
    migrated = registered == new
    cache_jobs, release_jobs = workflow_jobs(cache_workflow), workflow_jobs(release_workflow)
    assert set(cache_jobs) == ({'shared-native-cache', 'web-wasm-cache'} if migrated
                               else {'shared-native-cache'}), 'incomplete cache producer layout'
    active = []
    for path, workflow, jobs in (('.github/workflows/rust-cache.yml', cache_workflow, cache_jobs),
                                 ('.github/workflows/release-packages.yml', release_workflow, release_jobs)):
        for name, job in jobs.items():
            for step in rust_cache_steps(job):
                if 'shared-key: ' + namespace + '\n' not in step:
                    continue
                saves = re.findall(r'^          save-if: (.+)$', step, re.M)
                assert len(saves) == 1, 'one explicit Web save policy required'
                header = job.split('    steps:', 1)[0]
                if saves[0] != 'false':
                    active.append((namespace, path, name))
                    assert 'cache-mode: read' not in header
                    assert ('cache-mode: write' in header or re.search(r'^cache-mode: write$',
                            workflow.split('\njobs:\n', 1)[0], re.M))
                    assert 'OASIS7_DEPENDENCY_CACHE_FAMILY: release-web\n' in step
                else:
                    assert 'cache-mode: write' not in header
    assert active == [registered], 'Web cache must have exactly the registered sole writer'
    if migrated:
        reader = release_jobs['build-web-dist']
        assert '    cache-mode: read\n' in reader.split('    steps:', 1)[0]
        assert len(rust_cache_steps(reader)) == 1
        assert 'save-if: false\n' in rust_cache_steps(reader)[0]
        assert 'dependency-cache-tools' not in reader and 'OASIS7_DEPENDENCY_CACHE_' not in reader
    return migrated


def viewer_cache_kind(job):
    """The two Viewer lanes may move as a complete known cache configuration."""
    steps = rust_cache_steps(job)
    assert len(steps) == 1
    cache = steps[0]
    assert 'uses: Swatinem/rust-cache@d1527c8f67ef9af357290475d4af4b0fab8f12e0 # v2\n' in cache
    entries = re.findall(r'^          ([\w-]+): (.+)$', cache, re.M)
    inputs = dict(entries)
    assert len(inputs) == len(entries), 'duplicate cache input'
    common = {'save-if': 'false', 'add-rust-environment-hash-key': 'true', 'cache-on-failure': 'false'}
    native = common | {'shared-key': 'ci-shared-native-trusted-v4',
                       'env-vars': 'CARGO CC CFLAGS CXX CMAKE RUST OASIS7_WASM'}
    web = common | {'shared-key': 'release-packages-web-wasm-trusted-v3',
                    'env-vars': 'CARGO CC CFLAGS CXX CMAKE RUST'}
    assert inputs in (native, web), 'unknown or incomplete Viewer cache configuration'
    assert 'cache-mode: write' not in job
    assert '    env:' not in job.split('    steps:', 1)[0] and '        env:' not in cache
    assert 'GH_TOKEN:' not in cache and 'OASIS7_DEPENDENCY_CACHE_' not in cache
    prepare = next(step for step in re.findall(r'^      - .*?(?=^      - |\Z)', job, re.M | re.S)
                   if 'python3 scripts/ci-rust-cache-toolchains.py' in step)
    expected = '"$RUST_TOOLCHAIN"' + (' "$OASIS7_WASM_TOOLCHAIN"' if inputs == native else '')
    assert re.search(r'^        run: python3 scripts/ci-rust-cache-toolchains.py ' +
                     re.escape(expected) + r'$', prepare, re.M)
    assert 'if:' not in prepare and 'continue-on-error:' not in prepare
    assert job.index('rustup default') < job.index(prepare) < job.index(cache)
    before_cache = job[:job.index(cache)]
    assert 'rustup target add wasm32-unknown-unknown' in before_cache
    if inputs == web:
        assert 'OASIS7_WASM' not in before_cache and 'nightly' not in before_cache
        assert re.findall(r'^          rustup toolchain install (\S+)', before_cache, re.M) == ['"$RUST_TOOLCHAIN"']
    else:
        assert 'rustup toolchain install "$OASIS7_WASM_TOOLCHAIN"' in before_cache
    return 'native' if inputs == native else 'web'


def without_optional_dependency_cache_install(workflow):
    # Only these exact optional installers and diagnostics uploads may fail.
    # Removing a writer condition, renaming the command or swallowing any build,
    # required artifact, merge or test failure is never exempt.
    for name in ('formal-debug-build', 'formal-endpoint-build'):
        job = re.search(r'^  ' + name + r':\n.*?(?=^  [a-z][a-z0-9-]*:|\Z)', workflow, re.M | re.S)
        if not job:
            continue
        condition = (
            '        if: >-\n'
            f"          github.job == '{name}-trusted' && github.ref == 'refs/heads/main' &&\n"
            "          github.workflow_sha == github.sha && needs.select.outputs.trusted_source == 'true' &&\n"
            "          (github.event_name == 'schedule' || (github.event_name == 'workflow_dispatch' && inputs.run_mode == 'full'))\n")
        installer = ('      - name: Install dependency cache admission\n'
                     '        id: dependency-cache-tools\n' + condition +
                     '        continue-on-error: true\n'
                     '        run: python3 scripts/ci-rust-cache-admission.py install\n')
        component = name.removesuffix('-build')
        diagnostics = (f'      - name: Upload {component} build diagnostics\n'
                       '        if: always()\n'
                       '        continue-on-error: true\n'
                       '        uses: actions/upload-artifact@cf430e030ddbb5b0abf93d22962f4752f3646cd9 # v7.0.2\n'
                       '        with:\n'
                       f'          name: {component}-build-diagnostics-${{{{ github.sha }}}}-${{{{ github.run_attempt }}}}\n'
                       f'          path: ${{{{ runner.temp }}}}/{component}-build-diagnostics\n'
                       '          if-no-files-found: warn\n'
                       '          retention-days: 2\n')
        workflow = (workflow[:job.start()] + job[0].replace(installer, '', 1).replace(diagnostics, '', 1)
                    + workflow[job.end():])
    return workflow


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
        if group not in ('fleet_health', 'oasis7_required') and 'strategy:' in job:
            raise ValueError('ordinary groups must not have singleton matrices')
        if group == 'oasis7_required':
            strategy = ('    strategy:\n      fail-fast: false\n      matrix:\n'
                        '        partition: ${{ fromJSON(needs.select.outputs.ordinary_partitions) }}\n')
            header = job.split('    steps:', 1)[0]
            if (strategy not in header or 'include:' in header or 'exclude:' in header
                    or "ordinary_partitions: ${{ steps.select.outputs.ordinary_partitions || '[\"all\"]' }}" not in jobs['select']
                    or 'ORDINARY_PARTITION: ${{ matrix.partition }}' not in job
                    or '--ordinary-partition "$ORDINARY_PARTITION"' not in job):
                raise ValueError('ordinary matrix must preserve the complete trusted partition inventory and legacy all fallback')
        if group == 'command_center' and 'uses: ./.github/workflows/command-center.yml' not in job:
            raise ValueError('Command Center checks must execute through the reusable workflow')
        if 'ci-authority' in job and group != 'workflow_governance':
            raise ValueError('execution must use candidate checkout')
        if group == 'workflow_governance' and (
                'python3 "$RUNNER_TEMP/ci-authority/ci-python-control-contracts.py" run' not in job
                or 'bash ./scripts/ci-tests.sh required --group "workflow_governance"' not in job):
            raise ValueError('trusted Python assertions and candidate governance must both execute')
    return True


class Workflow(unittest.TestCase):
    def shared_cache_workflow(self):
        path = ROOT / '.github/workflows/rust-cache.yml'
        self.assertTrue(path.is_file(), 'dependency warming must have an independent workflow')
        workflow = path.read_text()
        jobs = workflow_jobs(workflow)
        web_cache_layout(workflow, (ROOT / '.github/workflows/release-packages.yml').read_text(),
                         (ROOT / 'scripts/ci-rust-cache-admission.py').read_text())
        return workflow, jobs['shared-native-cache']

    def test_web_cache_migration_rejects_partial_or_multiple_writers(self):
        namespace = 'release-packages-web-wasm-trusted-v3'
        old_identity = (namespace, '.github/workflows/release-packages.yml', 'build-web-dist')
        new_identity = (namespace, '.github/workflows/rust-cache.yml', 'web-wasm-cache')
        # Small wiring fixtures exercise the layout decision; the actual
        # producer's trust, environment and complete build are checked below.
        native = 'name: fixture\ncache-mode: read\njobs:\n  shared-native-cache:\n    steps: []\n'
        web_step = ('      - uses: Swatinem/rust-cache@pinned\n'
                    '        env:\n          OASIS7_DEPENDENCY_CACHE_FAMILY: release-web\n'
                    '        with:\n          shared-key: ' + namespace + '\n'
                    "          save-if: ${{ job.status == 'success' }}\n")
        old_release = ('name: fixture\ncache-mode: write\njobs:\n  build-web-dist:\n'
                       '    steps:\n' + web_step)
        new_cache = native + '  web-wasm-cache:\n    cache-mode: write\n    steps:\n' + web_step
        new_release = ('name: fixture\ncache-mode: write\njobs:\n  build-web-dist:\n'
                       '    cache-mode: read\n    steps:\n' + web_step.replace(
                           '        env:\n          OASIS7_DEPENDENCY_CACHE_FAMILY: release-web\n', '').replace(
                           "save-if: ${{ job.status == 'success' }}", 'save-if: false'))
        def source(identity):
            return 'FAMILIES = ' + repr({'release-web': identity})
        self.assertFalse(web_cache_layout(native, old_release, source(old_identity)))
        self.assertTrue(web_cache_layout(new_cache, new_release, source(new_identity)))
        for cache, release, registry in (
            (new_cache, old_release, source(new_identity)),
            (native, new_release, source(old_identity)),
            (new_cache, new_release, source(old_identity)),
            (native, old_release, source(new_identity)),
            (new_cache, new_release.replace('    cache-mode: read\n', ''), source(new_identity)),
            (new_cache.replace('    cache-mode: write\n', '    cache-mode: read\n'), new_release, source(new_identity)),
            (new_cache, new_release, source((namespace, '.github/workflows/unknown.yml', 'web-wasm-cache'))),
            (new_cache, new_release, 'FAMILIES = ' + repr({'release-web': new_identity, 'extra': new_identity})),
        ):
            with self.subTest(cache=cache, release=release, registry=registry):
                with self.assertRaises(AssertionError):
                    web_cache_layout(cache, release, registry)

    def test_viewer_cache_transition_requires_the_entire_known_configuration(self):
        for name in ('viewer-js-required', 'viewer-performance-report'):
            actual = JOBS[name]
            kind = viewer_cache_kind(actual)
            native = actual if kind == 'native' else actual.replace(
                'shared-key: release-packages-web-wasm-trusted-v3', 'shared-key: ci-shared-native-trusted-v4').replace(
                'env-vars: CARGO CC CFLAGS CXX CMAKE RUST\n', 'env-vars: CARGO CC CFLAGS CXX CMAKE RUST OASIS7_WASM\n').replace(
                'python3 scripts/ci-rust-cache-toolchains.py "$RUST_TOOLCHAIN"\n',
                'python3 scripts/ci-rust-cache-toolchains.py "$RUST_TOOLCHAIN" "$OASIS7_WASM_TOOLCHAIN"\n').replace(
                '          rustup default "$RUST_TOOLCHAIN"\n',
                '          rustup default "$RUST_TOOLCHAIN"\n'
                '          rustup toolchain install "$OASIS7_WASM_TOOLCHAIN" --profile minimal --component rust-src\n'
                '          rustup target add "$OASIS7_WASM_TARGET" --toolchain "$OASIS7_WASM_TOOLCHAIN"\n')
            web = native.replace('shared-key: ci-shared-native-trusted-v4',
                                 'shared-key: release-packages-web-wasm-trusted-v3').replace(
                'env-vars: CARGO CC CFLAGS CXX CMAKE RUST OASIS7_WASM',
                'env-vars: CARGO CC CFLAGS CXX CMAKE RUST').replace(
                ' "$OASIS7_WASM_TOOLCHAIN"\n', '\n')
            web = re.sub(r'^          rustup (?:toolchain install|target add) [^\n]*OASIS7_WASM[^\n]*\n', '', web, flags=re.M)
            self.assertEqual(viewer_cache_kind(native), 'native', name)
            self.assertEqual(viewer_cache_kind(web), 'web', name)
            for mutation in (
                native.replace('shared-key: ci-shared-native-trusted-v4', 'shared-key: release-packages-web-wasm-trusted-v3'),
                web.replace('save-if: false', 'save-if: true'),
                web.replace('add-rust-environment-hash-key: true', 'add-rust-environment-hash-key: false'),
                web.replace('env-vars: CARGO CC CFLAGS CXX CMAKE RUST\n',
                            'env-vars: CARGO CC CFLAGS CXX CMAKE RUST OASIS7_WASM\n'),
                web.replace('python3 scripts/ci-rust-cache-toolchains.py "$RUST_TOOLCHAIN"',
                            'python3 scripts/ci-rust-cache-toolchains.py "$RUST_TOOLCHAIN" "$OASIS7_WASM_TOOLCHAIN"'),
                web.replace('          rustup default', '          rustup toolchain install nightly\n          rustup default'),
                web.replace('          cache-on-failure: false', '          cache-on-failure: false\n          workspaces: . -> other-target'),
            ):
                with self.subTest(job=name, mutation=mutation):
                    with self.assertRaises(AssertionError):
                        viewer_cache_kind(mutation)

    def test_web_cache_writer_preserves_trust_and_complete_build(self):
        workflow, native = self.shared_cache_workflow()
        jobs = workflow_jobs(workflow)
        release_workflow = (ROOT / '.github/workflows/release-packages.yml').read_text()
        if 'web-wasm-cache' not in jobs:
            release = workflow_jobs(release_workflow)['build-web-dist']
            self.assertIn('./scripts/build-viewer-software-safe.sh --optional-payload-dir', release)
            self.assertIn('env -u NO_COLOR trunk build --release --dist', release)
            return
        self.assert_web_cache_writer(workflow, release_workflow)

    def assert_web_cache_writer(self, workflow, release_workflow):
        jobs = workflow_jobs(workflow)
        native = jobs['shared-native-cache']
        writer = jobs['web-wasm-cache']
        header = writer.split('    steps:', 1)[0]
        for required in ('cache-mode: write', 'runs-on: ubuntu-24.04',
                         'permissions:\n      contents: read\n      actions: read'):
            self.assertIn(required, header)
        self.assertNotIn('needs:', header)
        self.assertNotIn('env:', header)
        self.assertNotIn('web-wasm-cache', WORKFLOW)
        self.assertEqual(re.search(r'    if: >-\n(.*?)    runs-on:', writer, re.S)[1],
                         re.search(r'    if: >-\n(.*?)    runs-on:', native, re.S)[1])
        steps = re.findall(r'^      - .*?(?=^      - |\Z)', writer, re.M | re.S)
        source = next(step for step in steps if 'name: Verify current main cache source\n' in step)
        native_source = next(step for step in re.findall(r'^      - .*?(?=^      - |\Z)', native, re.M | re.S)
                             if 'name: Verify current main cache source\n' in step)
        self.assertEqual(source, native_source)
        self.assertEqual(writer.count('uses: actions/checkout@'), 1)
        self.assertIn('ref: ${{ github.sha }}\n          persist-credentials: false', writer)
        self.assertIn('npm ci --prefix crates/oasis7_viewer', writer)
        self.assertNotIn('OASIS7_WASM', writer)
        self.assertNotIn('nightly', writer)
        self.assertIn('run: python3 scripts/ci-rust-cache-toolchains.py "$RUST_TOOLCHAIN"\n', writer)
        self.assertEqual(len(rust_cache_steps(writer)), 1)
        cache = rust_cache_steps(writer)[0]
        self.assertNotRegex(cache, re.compile(r'^        if:', re.M))
        self.assertIn('uses: Swatinem/rust-cache@d1527c8f67ef9af357290475d4af4b0fab8f12e0 # v2\n', cache)
        before_cache = writer[:writer.index(cache)]
        self.assertEqual(re.findall(r'^          rustup toolchain install (\S+)', before_cache, re.M),
                         ['"${RUST_TOOLCHAIN}"'])
        self.assertIn('rustup default "${RUST_TOOLCHAIN}"', before_cache)
        self.assertIn('rustup target add wasm32-unknown-unknown --toolchain "${RUST_TOOLCHAIN}"', before_cache)
        install = next(step for step in steps if 'id: dependency-cache-tools\n' in step)
        self.assertIn('run: python3 scripts/ci-rust-cache-admission.py install-release\n', install)
        self.assertIn('continue-on-error: true', install)
        for required in ('continue-on-error: true', 'cache-provider: github', 'cache-on-failure: false',
                         'NODE_OPTIONS: ${{ steps.dependency-cache-tools.outputs.node_options }}',
                         'OASIS7_DEPENDENCY_CACHE_PYTHON: ${{ steps.dependency-cache-tools.outputs.python }}',
                         'OASIS7_DEPENDENCY_CACHE_SOURCE_SHA: ${{ github.sha }}',
                         'OASIS7_DEPENDENCY_CACHE_BUDGET_BYTES: ${{ env.CI_CACHE_REPOSITORY_BUDGET_BYTES }}'):
            self.assertIn(required, cache)
        self.assertNotIn('env-vars:', cache)
        save = re.search(r'^          save-if: \$\{\{ (.*) \}\}$', cache, re.M)[1]
        with_body = cache.split('        with:\n', 1)[1]
        inputs = re.findall(r'^          ([\w-]+): (.+)$', with_body, re.M)
        self.assertEqual(len(inputs), len(with_body.strip().splitlines()))
        expected_inputs = {'shared-key': 'release-packages-web-wasm-trusted-v3',
                           'cache-provider': 'github', 'cache-on-failure': 'false',
                           'save-if': '${{ ' + save + ' }}'}
        if ('add-rust-environment-hash-key', 'true') in inputs:
            expected_inputs['add-rust-environment-hash-key'] = 'true'
        self.assertEqual(len(inputs), len(dict(inputs)))
        self.assertEqual(dict(inputs), expected_inputs)
        environment = re.search(r'^        env:\n(.*?)(?=^        \S|\Z)', cache, re.M | re.S)[1]
        entries = re.findall(r'^          ([\w-]+): (.+)$', environment, re.M)
        self.assertEqual(len(entries), len(environment.strip().splitlines()))
        self.assertEqual(len(entries), len(dict(entries)))
        self.assertEqual(dict(entries), {
            'NODE_OPTIONS': '${{ steps.dependency-cache-tools.outputs.node_options }}',
            'OASIS7_DEPENDENCY_CACHE_PYTHON': '${{ steps.dependency-cache-tools.outputs.python }}',
            'OASIS7_DEPENDENCY_CACHE_FAMILY': 'release-web',
            'OASIS7_DEPENDENCY_CACHE_SOURCE_SHA': '${{ github.sha }}',
            'OASIS7_DEPENDENCY_CACHE_BUDGET_BYTES': '${{ env.CI_CACHE_REPOSITORY_BUDGET_BYTES }}',
            'GH_TOKEN': '${{ github.token }}',
        })
        self.assertNotRegex(save, r'(?i)\b(?:always|cancelled|failure|success)\s*\(')
        for status, guard, event, mode, ref, matching in itertools.product(
                ('success', 'failure', 'cancelled'), ('true', 'false', ''),
                ('schedule', 'workflow_dispatch', 'push', 'pull_request'), ('full', '', 'pr-fast'),
                ('refs/heads/main', 'refs/heads/topic'), (False, True)):
            values = {'job.status': status, 'steps.dependency-cache-tools.outputs.guard_ready': guard,
                      'github.event_name': event, 'inputs.run_mode': mode, 'github.ref': ref,
                      'github.workflow_sha': 'a' if matching else 'b', 'github.sha': 'a'}
            predicate = save
            for key in sorted(values, key=len, reverse=True):
                predicate = predicate.replace(key, repr(values[key]))
            expected = (status == 'success' and guard == 'true' and ref == 'refs/heads/main'
                        and matching and (event == 'schedule' or event == 'workflow_dispatch' and mode == 'full'))
            self.assertEqual(eval(predicate.replace('&&', ' and ').replace('||', ' or '),
                                  {'__builtins__': {}}, {}), expected, values)
        self.assertNotIn('NODE_OPTIONS:', workflow.split('\njobs:\n', 1)[0])
        self.assertLess(writer.index(source), writer.index(cache))
        normalizer = next(step for step in steps if 'python3 scripts/ci-rust-cache-toolchains.py' in step)
        self.assertNotRegex(normalizer, re.compile(r'^        if:', re.M))
        self.assertLess(writer.index('rustup default'), writer.index('python3 scripts/ci-rust-cache-toolchains.py'))
        self.assertLess(writer.index('python3 scripts/ci-rust-cache-toolchains.py'), writer.index(cache))
        self.assertLess(steps.index(install), steps.index(cache))
        completion = next(step for step in steps if 'name: Complete Web WASM cache\n' in step)
        self.assertIn('id: web-cache\n', cache)
        self.assertIn('set -euo pipefail', completion)
        self.assertIn('./scripts/build-viewer-software-safe.sh --optional-payload-dir output/release/viewer-optional-payload', completion)
        self.assertIn('cd crates/oasis7_client_launcher', completion)
        self.assertIn('env -u NO_COLOR trunk build --release --dist ../../output/release/web-launcher-dist', completion)
        self.assertLess(steps.index(cache), steps.index(completion))
        release_steps = re.findall(r'^      - .*?(?=^      - |\Z)',
                                   workflow_jobs(release_workflow)['build-web-dist'], re.M | re.S)
        self.assertEqual(re.search(r'^  TRUNK_VERSION: (.+)$', workflow, re.M)[1],
                         re.search(r'^  TRUNK_VERSION: (.+)$', release_workflow, re.M)[1])
        for name in ('Restore pinned Trunk release archive', 'Install verified pinned Trunk', 'Verify pinned trunk'):
            step = next(step for step in steps if 'name: ' + name + '\n' in step)
            self.assertEqual(step, next(step for step in release_steps if 'name: ' + name + '\n' in step))
            self.assertLess(steps.index(step), steps.index(completion))
        self.assertNotIn('uses: actions/cache@', writer)
        self.assertNotIn('uses: actions/cache/save@', writer)
        condition = re.search(r'^        if: (.*)$', completion, re.M)[1]
        for hit in ('true', 'false', '', 'unknown'):
            self.assertEqual(eval(condition.replace('steps.web-cache.outputs.cache-hit', repr(hit)),
                                  {'__builtins__': {}}, {}), hit != 'true')
        for step in steps:
            if step != cache:
                self.assertNotIn('NODE_OPTIONS:', step)
                self.assertNotIn('OASIS7_DEPENDENCY_CACHE_', step)
                self.assertNotIn('        env:', step)
            if step not in (install, cache):
                self.assertNotIn('continue-on-error:', step)
        self.assertNotRegex(writer, r'\b(?:CARGO|RUST|CC|CFLAGS|CXX|CMAKE)[A-Z0-9_]*=')

    def test_web_writer_complete_fixture_rejects_cache_context_drift(self):
        workflow, native = self.shared_cache_workflow()
        release = (ROOT / '.github/workflows/release-packages.yml').read_text()
        release_web = workflow_jobs(release)['build-web-dist']
        def step(job, marker):
            return next(part for part in re.findall(r'^      - .*?(?=^      - |\Z)', job, re.M | re.S)
                        if marker in part)
        # Compose the future lane from the independently maintained native
        # source guard and Release's canonical stable/Trunk preparation.
        header = native.split('    steps:', 1)[0]
        preparation = ''.join((step(native, 'uses: actions/checkout@'),
            step(native, 'name: Verify current main cache source\n'),
            step(release_web, 'uses: actions/setup-node@'),
            step(release_web, 'name: Install viewer web dependencies\n'),
            step(release_web, 'name: Install pinned Rust toolchain\n')))
        cache = '''      - name: Normalize Rust cache toolchains
        shell: bash
        run: python3 scripts/ci-rust-cache-toolchains.py "$RUST_TOOLCHAIN"
      - name: Install Web cache admission
        id: dependency-cache-tools
        continue-on-error: true
        run: python3 scripts/ci-rust-cache-admission.py install-release
      - uses: Swatinem/rust-cache@d1527c8f67ef9af357290475d4af4b0fab8f12e0 # v2
        id: web-cache
        continue-on-error: true
        env:
          NODE_OPTIONS: ${{ steps.dependency-cache-tools.outputs.node_options }}
          OASIS7_DEPENDENCY_CACHE_PYTHON: ${{ steps.dependency-cache-tools.outputs.python }}
          OASIS7_DEPENDENCY_CACHE_FAMILY: release-web
          OASIS7_DEPENDENCY_CACHE_SOURCE_SHA: ${{ github.sha }}
          OASIS7_DEPENDENCY_CACHE_BUDGET_BYTES: ${{ env.CI_CACHE_REPOSITORY_BUDGET_BYTES }}
          GH_TOKEN: ${{ github.token }}
        with:
          shared-key: release-packages-web-wasm-trusted-v3
          save-if: ${{ job.status == 'success' && steps.dependency-cache-tools.outputs.guard_ready == 'true' && github.ref == 'refs/heads/main' && github.workflow_sha == github.sha && (github.event_name == 'schedule' || (github.event_name == 'workflow_dispatch' && inputs.run_mode == 'full')) }}
          cache-provider: github
          cache-on-failure: false
'''
        trunk = ''.join(step(release_web, 'name: ' + name + '\n') for name in (
            'Restore pinned Trunk release archive', 'Install verified pinned Trunk', 'Verify pinned trunk'))
        completion = '''      - name: Complete Web WASM cache
        if: steps.web-cache.outputs.cache-hit != 'true'
        shell: bash
        run: |
          set -euo pipefail
          mkdir -p output/release/viewer-optional-payload output/release/web-launcher-dist
          OASIS7_FORCE_VIEWER_SOFTWARE_SAFE_BUILD=1 ./scripts/build-viewer-software-safe.sh --optional-payload-dir output/release/viewer-optional-payload
          (
            cd crates/oasis7_client_launcher
            env -u NO_COLOR trunk build --release --dist ../../output/release/web-launcher-dist
          )
'''
        writer = header + '    steps:\n' + preparation + cache + trunk + completion
        prefix = workflow.split('\njobs:\n', 1)[0]
        if '  TRUNK_VERSION:' not in prefix:
            prefix += '\n  TRUNK_VERSION: ' + re.search(r'^  TRUNK_VERSION: (.+)$', release, re.M)[1] + '\n'
        def assembled(candidate):
            return prefix + '\njobs:\n  shared-native-cache:\n' + native + '\n  web-wasm-cache:\n' + candidate
        self.assert_web_cache_writer(assembled(writer), release)
        for mutation in (
            writer.replace('          cache-provider: github', '          cache-provider: github\n          workspaces: . -> different-target'),
            writer.replace('          cache-provider: github', '          cache-provider: github\n          add-rust-environment-hash-key: false'),
            writer.replace('          cache-provider: github', '          cache-provider: github\n          cache-targets: false'),
            writer.replace('    steps:\n', '    env:\n      CARGO_PROFILE_RELEASE_OPT_LEVEL: "0"\n    steps:\n', 1),
            writer.replace('          GH_TOKEN:', '          CARGO_TARGET_DIR: other-target\n          GH_TOKEN:'),
            writer.replace('          rustup default', '          rustup toolchain install 1.95.0\n          rustup default'),
        ):
            with self.subTest(mutation=mutation):
                with self.assertRaises(AssertionError):
                    self.assert_web_cache_writer(assembled(mutation), release)

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
        if not legacy:
            (repo / 'scripts/ci-python-control-contracts.py').write_bytes(
                (ROOT / 'scripts/ci-python-control-contracts.py').read_bytes())
        if real_receipt:
            import runpy
            policy_files = set(runpy.run_path(str(ROOT / 'scripts/ci-full-result.py'))['POLICY_FILES'])
            policy_files.update(test for entry in CONFIG.get('python_control_contracts', {}).values()
                                for test in entry['tests'])
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
        (repo / 'scripts/ci-python-control-contracts.py').write_text("raise SystemExit('untrusted candidate control authority executed')\n")
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
                self.assertIn('ref: ${{ needs.select.outputs.tested_sha }}', job)
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

    def test_formal_build_diagnostics_are_optional_without_hiding_required_failures(self):
        for component, suffix in itertools.product(('debug', 'endpoint'), ('', '-trusted')):
            job = JOBS[f'formal-{component}-build{suffix}']
            build = job.split(f'      - name: Build and archive formal-{component} component\n', 1)[1].split('      - ', 1)[0]
            self.assertIn(f'--build-diagnostics "$RUNNER_TEMP/formal-{component}-build-diagnostics"', build)
            self.assertNotIn('continue-on-error:', build)
            self.assertNotIn('|| true', build)
            upload = job.split(f'      - name: Upload formal-{component} build diagnostics\n', 1)[1].split('      - ', 1)[0]
            self.assertIn('        if: always()\n', upload)
            self.assertIn('        continue-on-error: true\n', upload)
            self.assertIn(f'          path: ${{{{ runner.temp }}}}/formal-{component}-build-diagnostics\n', upload)
            self.assertIn(f'          name: formal-{component}-build-diagnostics-${{{{ github.sha }}}}-${{{{ github.run_attempt }}}}\n', upload)
            self.assertIn('          if-no-files-found: warn\n', upload)
        self.assertTrue(validate_candidate_graph(WORKFLOW, CONFIG))
        mutations = (
            WORKFLOW.replace('      - name: Build and archive formal-debug component\n',
                             '      - name: Build and archive formal-debug component\n        continue-on-error: true\n', 1),
            WORKFLOW.replace('      - name: Merge and pack formal process dependencies\n',
                             '      - name: Merge and pack formal process dependencies\n        continue-on-error: true\n', 1),
            WORKFLOW.replace('      - name: Upload formal-debug build diagnostics\n        if: always()\n',
                             '      - name: Upload formal-debug build diagnostics\n        if: success()\n', 1),
            WORKFLOW.replace('          path: ${{ runner.temp }}/formal-debug-build-diagnostics\n',
                             '          path: ${{ runner.temp }}/formal-debug-tests.tar.gz\n', 1),
        )
        for workflow in mutations:
            self.assertNotEqual(workflow, WORKFLOW, 'failure-propagation mutation did not change the workflow')
            with self.assertRaisesRegex(ValueError, 'failure propagation'):
                validate_candidate_graph(workflow, CONFIG)

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
                    self.assertIn("OASIS7_FORMAL_BUILD_INPUTS: ${{ hashFiles('scripts/ci-test-artifacts.py', 'scripts/ci-formal-process-suites.json') }}", cache)
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
                        ('workflow_dispatch', 'refs/heads/main', True, True, 'candidate-full', 'newapi_bridge_package')):
                    for installation, result in itertools.product(('success', 'failure', 'skipped'),
                                                                  ('success', 'failure', 'cancelled')):
                        values = {'always()': True, 'cancelled()': False, 'needs.select.result': 'success',
                            'needs.select.outputs.run_formal_process': 'true',
                            'needs.select.outputs.trusted_source': str(source).lower(),
                            'fromJSON(needs.select.outputs.plan).execution_profile': profile,
                            'fromJSON(needs.select.outputs.plan).fast_plan_version': 1 if profile else 0,
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
            self.assertIn('./scripts/ci-tests.sh full-extended', JOBS[name])
            self.assertIn('needs: select', JOBS[name])
            self.assertNotIn('oasis7-test-build', JOBS[name])
        self.assertIn('compression-level: 0', JOBS['oasis7-test-build-candidate'])
        self.assertIn('overwrite: true', JOBS['oasis7-test-build-candidate'])
        self.assertIn('--scope ordinary', JOBS['oasis7-test-build-candidate'])
        self.assertNotIn('run_formal_process', JOBS['oasis7-test-build'])
        self.assertIn('merge-formal --debug-bundle', JOBS['formal-test-build'])
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
                ('formal-debug', 'formal-debug-build', 'formal-debug-build-trusted'),
                ('formal-endpoint', 'formal-endpoint-build', 'formal-endpoint-build-trusted')):
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
        self.assertIn('ci-fast-preview.py plan-matrix --base-ref "$BASE_SHA" --head-ref "$TESTED_SHA"', planner)
        self.assertIn('SOURCE_SHA: ${{ github.event.pull_request.head.sha }}', planner)
        self.assertIn('--source-ref "$SOURCE_SHA"', planner)
        self.assertIn('artifact: pr-fast-preview-plan-${{ github.run_id }}-${{ github.run_attempt }}', planner)
        self.assertIn('name: ${{ needs.pr-fast-preview-plan.outputs.artifact }}', preview)
        self.assertIn('matrix: ${{ steps.preview.outputs.matrix }}', planner)
        self.assertIn('matrix: ${{ fromJSON(needs.pr-fast-preview-plan.outputs.matrix) }}', preview)
        self.assertIn('PREVIEW_SAMPLE: ${{ matrix.sample }}', preview)
        self.assertIn('PREVIEW_PARTITION: ${{ matrix.partition }}', preview)
        self.assertEqual(len(re.findall(r'^    name:', preview, re.M)), 1)
        self.assertIn('name: pr-fast-preview (${{ matrix.sample }}, ${{ matrix.group }}, ${{ matrix.partition }})', preview)
        self.assertIn('--group "$PREVIEW_GROUP" --partition "$PREVIEW_PARTITION"', preview)
        self.assertIn('OASIS7_PRODUCT_DOC_BASE: ${{ matrix.synthetic_base_sha }}', preview)
        self.assertIn('name: pr-fast-preview-${{ matrix.sample }}-${{ matrix.group }}-${{ matrix.partition }}-', preview)
        self.assertIn("'partition': os.environ['PREVIEW_PARTITION']", preview)
        self.assertIn('fail-fast: false', preview)
        self.assertIn('ref: ${{ needs.pr-fast-preview-plan.outputs.tested_sha }}', preview)
        self.assertIn('ci-fast-preview.py restore --sample "$PREVIEW_SAMPLE"', preview)
        self.assertIn('--metadata "$RUNNER_TEMP/previews/$PREVIEW_SAMPLE/selection.json.metadata.json"', preview)
        self.assertIn('--plan "$RUNNER_TEMP/previews/$PREVIEW_SAMPLE/selection.json"', preview)
        compiler = next(step for step in re.findall(r'^      - .*?(?=^      - |\Z)', preview, re.M | re.S)
                        if 'name: Install WASM C compiler' in step)
        self.assertIn("if: matrix.group == 'net'", compiler)
        for command in ('sudo -n /usr/bin/python3 scripts/ci-apt-install.py --budget-seconds 600 -- clang llvm',
                        'clang --print-targets | grep -w wasm32', 'llvm-ar --version'):
            self.assertIn(command, compiler)
            self.assertIn(command, JOBS['net'])
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
                '#!/usr/bin/env bash\nprintf "%s\\0" "$@" > "$ARGV_LOG"\n'
                + 'printf "%s" "$OASIS7_CI_ARCHITECTURE_PLAN" > "$PLAN_LOG"\nexit "$DRIVER_EXIT"\n')
            for sample, groups in (
                ('runtime-world', ('baseline', 'rust_baseline', 'oasis7_required', 'consensus', 'distfs', 'net', 'node')),
                ('game-client-private-unix', ('baseline', 'rust_baseline', 'oasis7_required')),
            ):
                selection = root / 'previews' / sample / 'selection.json'
                selection.parent.mkdir(parents=True)
                selection.write_text(json.dumps({'sample': sample}))
                for group in groups:
                    partitions = ('oasis7_runtime', 'oasis7_world_state') if (
                        sample == 'runtime-world' and group == 'oasis7_required') else ('all',)
                    for partition in partitions:
                        for code in (0, 19):
                            result = subprocess.run(['bash', '-c', script], cwd=root, text=True, capture_output=True,
                                env=dict(os.environ, PREVIEW_GROUP=group, PREVIEW_SAMPLE=sample,
                                         PREVIEW_PARTITION=partition, RUNNER_TEMP=str(root),
                                         GITHUB_WORKSPACE=str(root), ARGV_LOG=str(root / 'args'),
                                         PLAN_LOG=str(root / 'plan-log'), DRIVER_EXIT=str(code)))
                            args = (root / 'args').read_text().strip('\0').split('\0')
                            expected = ['required', '--group', group]
                            if group in ('rust_baseline', 'oasis7_required', 'distfs'):
                                expected.extend(['--plan', str(selection)])
                            if group == 'oasis7_required':
                                expected.extend(['--ordinary-partition', partition])
                            expected.extend(['--repo-root', str(root)])
                            with self.subTest(sample=sample, group=group, partition=partition, driver_exit=code):
                                self.assertEqual(args, expected)
                                self.assertEqual(json.loads((root / 'plan-log').read_text()), {'sample': sample})
                                self.assertEqual(result.returncode, code, result.stdout + result.stderr)

    def test_distfs_workflow_binds_new_handoffs_and_preserves_old_source_ref_cli(self):
        job = JOBS['distfs']
        self.assertIn('ref: ${{ needs.select.outputs.tested_sha }}', job)
        self.assertIn('name: ${{ needs.select.outputs.selection_artifact }}', job)
        self.assertIn('SOURCE_SHA: ${{ needs.select.outputs.source_sha }}', job)
        step = next(step for step in re.findall(r'^      - .*?(?=^      - |\Z)', job, re.M | re.S)
                    if 'name: Execute selected group' in step)
        script = textwrap.dedent(step.split('        run: |\n', 1)[1])
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / 'scripts').mkdir()
            (root / 'scripts/ci-tests.sh').write_text(
                '#!/usr/bin/env bash\nprintf "%s\\0" "$@" > "$ARGV_LOG"\n'
                'for arg in "$@"; do if [[ "$OLD_DRIVER" == 1 && "$arg" == --plan ]]; then exit 2; fi; done\n'
                'exit "$DRIVER_EXIT"\n')
            plans = [{}, {'execution_profile': 'candidate-full'},
                     {'distfs_clippy_contract_version': 1, 'distfs_clippy_group': 'rust_baseline'},
                     {'distfs_clippy_contract_version': 2}, {'distfs_clippy_group': 'other'}]
            for plan in plans:
                handoff = bool({'distfs_clippy_contract_version', 'distfs_clippy_group'}.intersection(plan))
                (root / 'selection.json').write_text(json.dumps(plan))
                for code in (0, 19):
                    result = subprocess.run(['bash', '-c', script], cwd=root, text=True, capture_output=True,
                        env=dict(os.environ, RUNNER_TEMP=str(root), GITHUB_WORKSPACE=str(root),
                                 ARGV_LOG=str(root / 'args'), OLD_DRIVER='0' if handoff else '1', DRIVER_EXIT=str(code)))
                    args = (root / 'args').read_text().strip('\0').split('\0')
                    expected = ['required', '--group', 'distfs']
                    if handoff:
                        expected += ['--plan', str(root / 'selection.json')]
                    expected += ['--repo-root', str(root)]
                    with self.subTest(plan=plan, code=code):
                        self.assertEqual(args, expected)
                        self.assertEqual(result.returncode, code, result.stdout + result.stderr)
            for malformed in ('[', '[]'):
                (root / 'selection.json').write_text(malformed)
                (root / 'args').unlink(missing_ok=True)
                result = subprocess.run(['bash', '-c', script], cwd=root, text=True, capture_output=True,
                    env=dict(os.environ, RUNNER_TEMP=str(root), GITHUB_WORKSPACE=str(root),
                             ARGV_LOG=str(root / 'args'), OLD_DRIVER='0', DRIVER_EXIT='0'))
                self.assertNotEqual(result.returncode, 0)
                self.assertFalse((root / 'args').exists())

    def test_ordinary_matrix_keeps_the_trusted_complete_partition_list_and_failure_boundary(self):
        job = JOBS['oasis7-required']
        self.assertTrue(validate_candidate_graph(WORKFLOW, CONFIG))
        self.assertIn('needs: [select, oasis7-test-build]', job)
        self.assertIn('oasis7-required', JOBS['required-gate'])
        for damaged in (
            job.replace('fail-fast: false', 'fail-fast: true'),
            job.replace('${{ fromJSON(needs.select.outputs.ordinary_partitions) }}', '[oasis7_runtime]'),
            job.replace('      matrix:\n', '      matrix:\n        exclude: [{partition: oasis7_world_state}]\n'),
            job.replace('--ordinary-partition "$ORDINARY_PARTITION"', ''),
        ):
            with self.subTest(damaged=damaged[:120]), self.assertRaises(ValueError):
                validate_candidate_graph(WORKFLOW.replace(job, damaged), CONFIG)
        fallback = "ordinary_partitions: ${{ steps.select.outputs.ordinary_partitions || '[\"all\"]' }}"
        with self.assertRaises(ValueError):
            validate_candidate_graph(WORKFLOW.replace(fallback, "ordinary_partitions: '[\"oasis7_runtime\"]'"), CONFIG)

    def test_workspace_maintenance_recovers_without_a_new_confirmed_save(self):
        job = JOBS.get('cache-maintenance', '')
        self.assertIn('needs: [select, oasis7-test-build-trusted, formal-debug-build-trusted, formal-endpoint-build-trusted]', job)
        self.assertNotIn('full-regression', job)
        self.assertNotIn('formal-test-build-trusted', job)
        self.assertIn('always() && !cancelled()', job.split('    steps:', 1)[0])
        for producer in ('oasis7-test-build-trusted', 'formal-debug-build-trusted', 'formal-endpoint-build-trusted'):
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
        for ordinary, debug, endpoint in itertools.product((False, True), repeat=3):
            values = dict(ORDINARY_RESULT='success' if ordinary else 'failure',
                          FORMAL_DEBUG_RESULT='success', FORMAL_ENDPOINT_RESULT='success',
                          ORDINARY_CONFIRMED='true', FORMAL_DEBUG_CONFIRMED='true' if debug else 'false',
                          FORMAL_ENDPOINT_CONFIRMED='true' if endpoint else 'false',
                          ORDINARY_KEY='ordinary-key', FORMAL_DEBUG_KEY='formal-debug-key',
                          FORMAL_ENDPOINT_KEY='formal-endpoint-key')
            result = subprocess.run(['bash', '-c', stub + script], text=True, capture_output=True,
                                    env=dict(os.environ, **values))
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual('<ordinary-key>' in result.stdout, ordinary)
            self.assertEqual('<formal-debug-key>' in result.stdout, debug)
            self.assertEqual('<formal-endpoint-key>' in result.stdout, endpoint)
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
                                    'needs.formal-debug-build-trusted.result': formal,
                                    'needs.formal-endpoint-build-trusted.result': formal,
                                    'needs.native-trusted.result': ordinary,
                                    'needs.oasis7-test-build-trusted.outputs.cache_confirmed': confirmed,
                                    'needs.formal-debug-build-trusted.outputs.cache_confirmed': confirmed,
                                    'needs.formal-endpoint-build-trusted.outputs.cache_confirmed': confirmed,
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

    def test_formal_component_cache_recovery_does_not_wait_for_merge_or_other_success(self):
        job = JOBS['cache-maintenance']
        condition = job.split('    if:', 1)[1].split('    cache-mode:', 1)[0].strip().removeprefix('>-')
        states = ('success', 'failure', 'cancelled', 'skipped', '', 'unknown')
        for debug, endpoint, cancelled in itertools.product(states, states, (False, True)):
            values = {
                'always()': True, 'cancelled()': cancelled,
                'needs.oasis7-test-build-trusted.result': 'failure',
                'needs.formal-debug-build-trusted.result': debug,
                'needs.formal-endpoint-build-trusted.result': endpoint,
                'needs.select.outputs.trusted_source': 'true',
                'fromJSON(needs.select.outputs.plan).execution_profile': 'periodic-full',
                'fromJSON(needs.select.outputs.plan).fast_plan_version': 1,
                'github.event_name': 'schedule', 'github.ref': 'refs/heads/main',
                'inputs.run_mode': '', 'github.workflow_sha': 'a', 'github.sha': 'a',
            }
            expression = condition
            for token, value in sorted(values.items(), key=lambda item: -len(item[0])):
                expression = expression.replace(token, repr(value))
            expression = re.sub(r'!(?!=)', 'not ', expression.replace('&&', ' and ').replace('||', ' or '))
            with self.subTest(debug=debug, endpoint=endpoint, cancelled=cancelled):
                self.assertEqual(bool(eval(' '.join(expression.split()), {'__builtins__': {}}, {})),
                                 not cancelled and 'success' in (debug, endpoint))

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
        for name in ('oasis7-test-build-candidate', 'formal-debug-build', 'formal-endpoint-build',
                     'formal-test-build', 'formal-test-build-trusted', 'required-gate', 'explicit-full-gate'):
            self.assertNotIn('cache-bootstrap-maintenance', JOBS[name])
        for name in ('oasis7-test-build-trusted', 'formal-debug-build-trusted', 'formal-endpoint-build-trusted'):
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
                                                   ('formal-debug-build-trusted', formal),
                                                   ('formal-endpoint-build-trusted', formal)):
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
        # Both lanes run the same full validation. Only the trusted main lane
        # additionally saves its existing tool caches and sentinel.
        writer = JOBS['full-regression'].split('    steps:\n', 1)[1]
        candidate = JOBS['candidate-full-regression'].split('    steps:\n', 1)[1]
        for job in (writer, candidate):
            self.assertNotIn('cache-native', job)
            self.assertNotIn('dependency-cache-tools', job)
        caches = []
        for job in (writer, candidate):
            matches = [step for step in re.findall(r'^      - .*?(?=^      - |\Z)', job, re.M | re.S)
                       if 'uses: Swatinem/rust-cache@' in step]
            self.assertEqual(len(matches), 1)
            caches.append(matches[0])
        # Action input order is immaterial; compare every setting before
        # comparing the remaining compiler setup, tests and diagnostics.
        self.assertEqual(sorted(caches[0].splitlines()), sorted(caches[1].splitlines()))
        writer = writer.replace(caches[0], '', 1)
        candidate = candidate.replace(caches[1], '', 1)
        candidate = re.sub(r'      - name: Record exact candidate identity\n.*?(?=      - name: Resolve)', '', candidate, flags=re.S)
        writer = re.sub(r'      - name: Prepare trusted cache sentinel\n.*?(?=      - (?:name: Run full|uses: actions/download-artifact))', '', writer, flags=re.S)
        writer = re.sub(r'      - name: Save prepared wasm-bindgen tool cache\n.*?(?=      - name:)', '', writer, flags=re.S)
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

    def test_engineering_telemetry_entry_requires_complete_nonempty_suites(self):
        driver = (ROOT / 'scripts/ci-tests.sh').read_text()
        function = re.search(r'^run_engineering_telemetry_contract_tests\(\) \{\n.*?^\}', driver, re.M | re.S)
        self.assertIsNotNone(function)
        governance = re.search(r'^run_workflow_governance_contract_tests\(\) \{\n.*?^\}', driver, re.M | re.S)
        self.assertIsNotNone(governance)
        self.assertEqual(governance.group().count('  run_engineering_telemetry_contract_tests'), 1)
        paths = (
            'scripts/engineering_telemetry.py',
            'scripts/engineering_telemetry_adapters.py',
            'scripts/tests/fixtures/engineering-telemetry/input.json',
            'scripts/tests/test_engineering_telemetry.py',
            'scripts/tests/test_engineering_telemetry_adapters.py',
        )
        cases = [('absent', None), ('complete', None)]
        cases += [(kind, path) for kind in ('missing', 'directory', 'symlink') for path in paths]
        cases += [(kind, path) for kind in ('empty', 'failing') for path in paths[-2:]]
        for kind, changed in cases:
            with self.subTest(kind=kind, path=changed), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                if kind != 'absent':
                    for path in paths:
                        target = root / path
                        target.parent.mkdir(parents=True, exist_ok=True)
                        if path == changed and kind == 'missing':
                            continue
                        if path == changed and kind == 'directory':
                            target.mkdir()
                            continue
                        if path == changed and kind == 'symlink':
                            target.symlink_to(root / 'missing-target')
                            continue
                        content = '# synthetic telemetry input\n'
                        if path in paths[-2:] and not (path == changed and kind == 'empty'):
                            content = ("import unittest\nfrom pathlib import Path\n"
                                       "class Contract(unittest.TestCase):\n"
                                       "    def test_runs(self):\n"
                                       "        Path(%r).write_text('once')\n"
                                       "        self.assertTrue(%r)\n" %
                                       (target.name + '.ran', not (path == changed and kind == 'failing')))
                        target.write_text(content)
                shell = ('set -euo pipefail\nrun() { "$@"; }\n'
                         'run_workflow_governance_baseline_contract_tests() { echo governance-baseline; }\n'
                         'run_workflow_governance_operational_contract_tests() { echo governance-operational; }\n'
                         + function.group() + '\n' + governance.group()
                         + '\nrun_workflow_governance_contract_tests\n')
                result = subprocess.run(['bash', '-c', shell], cwd=root, capture_output=True, text=True)
                if kind in ('absent', 'complete'):
                    self.assertEqual(result.returncode, 0, result.stderr)
                    self.assertEqual(result.stdout.splitlines()[-2:],
                                     ['governance-baseline', 'governance-operational'])
                else:
                    self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
                    self.assertNotIn('governance-baseline', result.stdout)
                    self.assertNotIn('governance-operational', result.stdout)
                if kind == 'absent':
                    self.assertIn('not introduced', result.stdout)
                if kind in ('complete', 'failing'):
                    for path in paths[-2:]:
                        self.assertTrue((root / (Path(path).name + '.ran')).is_file(), result.stderr)
                    self.assertIn('Ran 2 tests', result.stderr)
                if kind == 'empty':
                    self.assertIn('no tests in', result.stderr)

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
                if name in ('formal-test-build', 'formal-test-build-trusted'):
                    # Merge needs the real compiler identity but cannot run
                    # build/lint commands. Existing jobs retain their checks.
                    self.assertIn('rustup toolchain install "$RUST_TOOLCHAIN" --profile minimal\n', job)
                    self.assertIn('rustup default "$RUST_TOOLCHAIN"', job)
                    self.assertIn('ci-rust-cache-toolchains.py "$RUST_TOOLCHAIN"\n', job)
                    self.assertIn('ci-test-artifacts.py merge-formal ', job)
                    self.assertNotRegex(job, r'cargo (?:build|test|check|clippy|fmt)\b')
                    self.assertNotIn('ci-test-artifacts.py build ', job)
                else:
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
        workflow, writer = self.shared_cache_workflow()
        on = re.search(r'^on:\n(.*?)(?=^[^\s#]|\Z)', workflow, re.M | re.S)[1]
        self.assertEqual(set(re.findall(r'^  ([a-z_]+):', on, re.M)), {'schedule', 'workflow_dispatch'})
        self.assertIn('options: [full]', on)
        self.assertNotIn('source_ref', on)
        self.assertNotIn('workflow_run', on)
        self.assertIn('cache-mode: write', writer)
        self.assertIn('test "$GITHUB_REF" = refs/heads/main', writer)
        self.assertNotIn('test "$ACTIONS_CACHE_MODE"', writer)
        header = writer.split('    runs-on:', 1)[0]
        expression = ' '.join(header.split('    if:', 1)[1].strip().removeprefix('>-').split())
        for event in ('schedule', 'workflow_dispatch', 'push', 'pull_request', 'workflow_run'):
            for ref in ('refs/heads/main', 'refs/heads/topic', 'refs/pull/1/merge', 'refs/tags/v1'):
                for matching in (False, True):
                    for mode in ('full', '', 'newapi_bridge_package'):
                        values = {'github.event_name': event, 'github.ref': ref,
                                  'github.workflow_sha': 'a' if matching else 'b',
                                  'github.sha': 'a', 'inputs.run_mode': mode}
                        predicate = expression
                        for token, value in values.items():
                            predicate = predicate.replace(token, repr(value))
                        predicate = predicate.replace('&&', ' and ').replace('||', ' or ')
                        with self.subTest(event=event, ref=ref, matching=matching, mode=mode):
                            self.assertEqual(eval(predicate, {'__builtins__': {}}, {}),
                                ref == 'refs/heads/main' and matching
                                and (event == 'schedule' or event == 'workflow_dispatch' and mode == 'full'))

    def test_shared_cache_workflow_is_independent_of_full_validation(self):
        workflow, writer = self.shared_cache_workflow()
        self.assertNotIn('shared-native-cache', JOBS)
        self.assertNotIn('needs:', writer.split('    steps:', 1)[0])
        self.assertIn('group: ci-shared-native-cache-main', workflow)
        self.assertIn('cancel-in-progress: false', workflow)
        self.assertNotIn('ci-shared-native-cache-main', WORKFLOW)
        self.assertIn('timeout-minutes: 210', writer)
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
            self.assertIn('./scripts/ci-tests.sh full-extended', job)
            self.assertNotIn('cache-native', job)
            self.assertNotIn('dependency-cache-tools', job)
            cache = next(step for step in re.findall(r'^      - .*?(?=^      - |\Z)', job, re.M | re.S)
                         if 'uses: Swatinem/rust-cache@' in step)
            self.assertIn('save-if: false', cache)
            self.assertNotIn('GH_TOKEN:', cache)
            full = next(step for step in re.findall(r'^      - .*?(?=^      - |\Z)', job, re.M | re.S)
                        if './scripts/ci-tests.sh full-extended' in step)
            self.assertNotIn('if:', full)
            self.assertNotIn('continue-on-error:', full)

    def test_shared_cache_checkout_verifies_actual_current_main_source(self):
        _, writer = self.shared_cache_workflow()
        self.assertIn('ref: ${{ github.sha }}', writer)
        self.assertEqual(len(re.findall(r'uses: actions/checkout@[0-9a-f]{40}\b', writer)), 1)
        self.assertIn('persist-credentials: false', writer)
        step = next(step for step in re.findall(r'^      - .*?(?=^      - |\Z)', writer, re.M | re.S)
                    if 'name: Verify current main cache source\n' in step)
        self.assertNotIn('if:', step)
        self.assertNotIn('continue-on-error:', step)
        self.assertLess(writer.index(step), writer.index('uses: Swatinem/rust-cache@'))
        script = textwrap.dedent(step.split('        run: |\n', 1)[1])
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            subprocess.run(['git', 'init', '-q', '-b', 'main', str(root)], check=True)
            subprocess.run(['git', '-C', str(root), '-c', 'user.name=CI fixture',
                            '-c', 'user.email=ci-fixture@example.invalid', 'commit', '-q',
                            '--allow-empty', '-m', 'current cache source'], check=True)
            sha = subprocess.check_output(['git', '-C', str(root), 'rev-parse', 'HEAD'], text=True).strip()
            for ref in ('refs/heads/main', 'refs/heads/topic', 'refs/pull/1/merge'):
                for event_matches in (False, True):
                    for workflow_matches in (False, True):
                        environment = dict(os.environ, GITHUB_REF=ref,
                            GITHUB_SHA=sha if event_matches else 'b' * 40,
                            GITHUB_WORKFLOW_SHA=sha if workflow_matches else 'b' * 40)
                        result = subprocess.run(['bash', '-c', script], cwd=root,
                                                env=environment, capture_output=True, text=True)
                        with self.subTest(ref=ref, event_matches=event_matches, workflow_matches=workflow_matches):
                            self.assertEqual(result.returncode == 0,
                                ref == 'refs/heads/main' and event_matches and workflow_matches,
                                result.stdout + result.stderr)

    def test_readers_share_compatible_trusted_writer_identity(self):
        cache_workflow, writer = self.shared_cache_workflow()
        self.assertIn('cache-mode: write', writer)
        self.assertIn('shared-key: ci-shared-native-trusted-v4', writer)
        self.assertIn('env-vars: CARGO CC CFLAGS CXX CMAKE RUST OASIS7_WASM', writer)
        environments = []
        for text in (WORKFLOW, cache_workflow):
            block = re.search(r'^env:\n(.*?)(?=^[^\s#]|\Z)', text, re.M | re.S)[1]
            environments.append(dict(re.findall(
                r'^  ((?:CARGO|RUST|OASIS7_WASM)[A-Z0-9_]*): (.+)$', block, re.M)))
        self.assertEqual(environments[0], environments[1],
                         'shared dependency contexts require the same pinned compiler and Cargo/WASM environment')
        for name, job in JOBS.items():
            if 'save-if: false' in job and 'Swatinem/rust-cache@' in job:
                if name in ('viewer-js-required', 'viewer-performance-report'):
                    viewer_cache_kind(job)
                    continue
                component = re.fullmatch(r'formal-(debug|endpoint)-build(?:-trusted)?', name)
                namespace = f'ci-formal-{component[1]}-deps-trusted-v1' if component else 'ci-shared-native-trusted-v4'
                self.assertIn('shared-key: ' + namespace, job, name)
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
        cache_workflow, shared_writer = self.shared_cache_workflow()
        lanes = ((shared_writer, 'shared-native', 'CI_CACHE_REPOSITORY_BUDGET_BYTES'),
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
        self.assertEqual(governance.count('run python3 ./scripts/ci-rust-cache-admission.test.py'), 1)

    def test_dependency_save_inputs_reject_step_only_status_functions(self):
        # GitHub permits status functions in step `if`, not action inputs.
        # The `job` context is available in `with`, so save-if uses job.status.
        ecs_jobs = workflow_jobs((ROOT / '.github/workflows/ecs-packages.yml').read_text())
        _, shared_writer = self.shared_cache_workflow()
        for name, job in (('shared-native-cache', shared_writer),
                          ('native', ecs_jobs['native']), ('native-trusted', ecs_jobs['native-trusted'])):
            cache = next(step for step in re.findall(r'^      - .*?(?=^      - |\Z)', job, re.M | re.S)
                         if 'uses: Swatinem/rust-cache@' in step)
            save = re.search(r'          save-if: \$\{\{ (.*?) \}\}', cache)
            self.assertIsNotNone(save, name)
            with self.subTest(job=name):
                self.assertNotRegex(save.group(1), r'(?i)\b(?:always|cancelled|failure|success)\s*\(')

    def test_dependency_save_requires_success_and_current_trusted_source(self):
        ecs_jobs = workflow_jobs((ROOT / '.github/workflows/ecs-packages.yml').read_text())
        _, shared_writer = self.shared_cache_workflow()
        for name, job in (('shared-native-cache', shared_writer),
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

    def test_download_caches_follow_actual_worksets(self):
        node_jobs={name for name,job in JOBS.items() if 'uses: actions/setup-node@' in job}
        self.assertEqual(node_jobs,{'viewer-js-required','viewer-performance-report','launcher-web','workspace-support','full-regression','candidate-full-regression'})
        for name in node_jobs - {'workspace-support'}:
            self.assertIn('cache: npm',JOBS[name])
            self.assertIn('cache-dependency-path: crates/oasis7_viewer/package-lock.json',JOBS[name])
            self.assertIn('npm ci --prefix crates/oasis7_viewer',JOBS[name])
        for name,job in JOBS.items():
            if 'Swatinem/rust-cache@' in job and name in {g.replace('_', '-') for g in CONFIG['groups']}:
                if name in ('viewer-js-required', 'viewer-performance-report'):
                    viewer_cache_kind(job)
                else:
                    self.assertIn('shared-key: ci-shared-native-trusted-v4',job)
                    self.assertIn('env-vars: CARGO CC CFLAGS CXX CMAKE RUST OASIS7_WASM',job)
                self.assertIn('add-rust-environment-hash-key: true',job)
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
            if group not in ('fleet_health', 'oasis7_required'):
                self.assertNotIn('strategy:', job)
            if group != 'workflow_governance':
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
        authority_consumers = {name for name, job in JOBS.items()
                               if 'name: ${{ needs.select.outputs.authority_artifact }}' in job}
        self.assertEqual(authority_consumers, {'formal-debug-build', 'formal-debug-build-trusted',
            'formal-endpoint-build', 'formal-endpoint-build-trusted', 'formal-test-build',
            'formal-test-build-trusted', 'formal-process-tests', 'formal-process',
            'workflow-governance', 'required-gate', 'explicit-full-gate'})
        for name, job in JOBS.items():
            if name != 'pr-fast-preview':
                self.assertNotIn('matrix.group', job, name)
        self.assertEqual(len(re.findall(r'uses: actions/checkout@[0-9a-f]{40}\b', WORKFLOW)), WORKFLOW.count('persist-credentials: false'))
        self.assertNotIn('secrets.', WORKFLOW)
        without_admitted_uploads = WORKFLOW
        for name, family in (('formal-debug-build', 'formal-debug'), ('formal-endpoint-build', 'formal-endpoint')):
            dependency_cache = next(step for step in re.findall(r'^      - .*?(?=^      - |\Z)',
                                                                 JOBS[name], re.M | re.S)
                                    if 'uses: Swatinem/rust-cache@' in step
                                    and 'OASIS7_DEPENDENCY_CACHE_FAMILY: ' + family in step)
            self.assertEqual(dependency_cache.count('GH_TOKEN: ${{ github.token }}'), 1)
            without_admitted_uploads = without_admitted_uploads.replace(dependency_cache, '', 1)
        self.assertNotIn('GH_TOKEN:', without_admitted_uploads)
        cache_workflow, _ = self.shared_cache_workflow()
        without_admitted_uploads = cache_workflow
        for name, writer in workflow_jobs(cache_workflow).items():
            dependency_caches = rust_cache_steps(writer)
            self.assertEqual(len(dependency_caches), 1, name)
            dependency_cache = dependency_caches[0]
            family = 'shared-native' if name == 'shared-native-cache' else 'release-web'
            self.assertIn('OASIS7_DEPENDENCY_CACHE_FAMILY: ' + family + '\n', dependency_cache)
            self.assertEqual(dependency_cache.count('GH_TOKEN: ${{ github.token }}'), 1)
            without_admitted_uploads = without_admitted_uploads.replace(dependency_cache, '', 1)
        self.assertNotIn('GH_TOKEN:', without_admitted_uploads)
        self.assertNotIn('secrets.', cache_workflow)
        self.assertNotIn('scripts/pm/', WORKFLOW)
        self.assertNotIn('task_uid', WORKFLOW)
        self.assertIn('permissions:\n  contents: read', WORKFLOW)

    def test_python_control_assertions_use_fixed_baseline_and_pinned_offline_tools(self):
        job = JOBS['workflow-governance']
        trusted = 'python3 "$RUNNER_TEMP/ci-authority/ci-python-control-contracts.py" run'
        self.assertIn(trusted, job)
        self.assertIn('--policy-sha "$POLICY_SHA"', job)
        self.assertIn('--plan "$RUNNER_TEMP/ci-control-selection/selection.json"', job)
        self.assertIn('python_control_contract_version == 1', job)
        self.assertIn('rustup toolchain install "$RUST_TOOLCHAIN" --profile minimal', job)
        self.assertIn('rustup default "$RUST_TOOLCHAIN"', job)
        self.assertIn('command -v zstdmt', job)
        self.assertIn('pip install --disable-pip-version-check --requirement scripts/doc-governance-requirements.txt', job)
        self.assertLess(job.index(trusted), job.index('bash ./scripts/ci-tests.sh required'))
        self.assertNotIn('cargo test -p', job)
        self.assertNotIn('Swatinem/rust-cache', job)
        driver = (ROOT / 'scripts/ci-tests.sh').read_text()
        self.assertIn('run python3 ./scripts/ci-python-control-contracts.test.py', driver)
        for legacy in (False, True):
            with self.subTest(legacy=legacy), tempfile.TemporaryDirectory() as directory:
                result, _, plan, _, _ = self.run_selection(
                    directory, event='pull_request', legacy=legacy, candidate=True)
                self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                runner = Path(directory) / 'ci-authority/ci-python-control-contracts.py'
                if legacy:
                    self.assertFalse(runner.exists())
                    self.assertEqual(plan['scope'], 'full')
                    self.assertNotIn('python_control_contract_version', plan)
                else:
                    self.assertEqual(runner.read_bytes(), (ROOT / 'scripts/ci-python-control-contracts.py').read_bytes())

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
        function = workspace_support_fixture_functions(driver)
        script = "set -euo pipefail\n" + function + """run_signed_read_protocol_contract_tests() { :; }
run() { :; }
run_cargo() { printf '%s|%s\\n' "$OASIS7_WASM_BUILD_STD" "$*"; }
run_cargo_clippy() { printf '%s|clippy %s\\n' "$OASIS7_WASM_BUILD_STD" "$*"; }
""" + "\nrun_oasis7_workspace_support_crate_tests\nprintf 'after|%s\\n' \"$OASIS7_WASM_BUILD_STD\"\n"
        result = subprocess.run(['bash', '-c', script], text=True, capture_output=True,
                                env=dict(os.environ, OASIS7_WASM_BUILD_STD='1'))
        self.assertEqual(result.returncode, 0, result.stderr)
        calls = result.stdout.splitlines()
        self.assertEqual(calls[-2], '0|test -p wasm_build_suite -p wasm_module_observe')
        self.assertTrue(all(call.startswith('1|') for call in calls[:-2]))
        self.assertEqual(calls[-1], 'after|1')

    def test_agent_contract_and_harness_checks_are_in_workspace_support(self):
        driver = (ROOT / 'scripts/ci-tests.sh').read_text()
        function = workspace_support_fixture_functions(driver)
        script = "set -euo pipefail\n" + function + """run_signed_read_protocol_contract_tests() { :; }
run() { :; }
run_cargo() { printf 'test|%s\\n' "$*"; }
run_cargo_clippy() { printf 'clippy|%s\\n' "$*"; }
""" + "\nrun_oasis7_workspace_support_crate_tests\n"
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
        function = workspace_support_fixture_functions(driver)
        script = "set -euo pipefail\n" + function + "run_signed_read_protocol_contract_tests() { :; }\nrun() { :; }\nrun_cargo() { printf '%s\\n' \"$*\"; }\nrun_cargo_clippy() { printf 'clippy %s\\n' \"$*\"; }\n\nrun_oasis7_workspace_support_crate_tests\n"
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

    def test_workspace_support_runs_real_worker_compile_helper_and_propagates_failure(self):
        driver = (ROOT / 'scripts/ci-tests.sh').read_text()
        functions = workspace_support_fixture_functions(driver)
        script = "set -euo pipefail\n" + functions + """run_signed_read_protocol_contract_tests() { :; }
run_cargo() { :; }
run_cargo_clippy() { :; }
run() { if [[ "$1" == "$ci_python" && "${2:-}" == - ]]; then "$@"; fi; }
""" + "\nrun_oasis7_workspace_support_crate_tests\n"
        features = ('identity', 'owner', 'executor', 'recovery-fence',
                    'owner,approval-harness', 'identity,native-runtime-harness')
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            manifest = root / 'crates/oasis7_custody_worker/Cargo.toml'
            manifest.parent.mkdir(parents=True)
            # A closed manifest fixture keeps this shell-consumer proof separate
            # from the conditional future-profile contracts in the Python suite.
            names = sorted({name for profile in features for name in profile.split(',')})
            manifest.write_text('[features]\ndefault = []\n'
                                + ''.join(name + ' = []\n' for name in names))
            binary = root / 'bin'
            binary.mkdir()
            capture = root / 'cargo-command-capture'
            cargo = binary / 'cargo'
            # This executable captures argv only; no real Cargo or compiler runs.
            cargo.write_text('#!/bin/sh\nprintf \'%s\\n\' "$*" >> "$WORKER_COMPILE_CAPTURE"\nexit "$WORKER_COMPILE_RESULT"\n')
            cargo.chmod(0o755)
            environment = dict(os.environ, PATH=str(binary) + os.pathsep + os.environ['PATH'],
                               WORKER_COMPILE_CAPTURE=str(capture), WORKER_COMPILE_RESULT='0')
            result = subprocess.run(['bash', '-c', script], cwd=root, env=environment,
                                    text=True, capture_output=True)
            self.assertEqual(result.returncode, 0, result.stderr)
            expected = ['check -p oasis7_custody_worker --target wasm32-unknown-unknown '
                        '--no-default-features --features ' + feature + ' --locked'
                        for feature in features]
            self.assertEqual(capture.read_text().splitlines(), expected)
            capture.unlink()
            environment['WORKER_COMPILE_RESULT'] = '42'
            failed = subprocess.run(['bash', '-c', script], cwd=root, env=environment,
                                    text=True, capture_output=True)
            self.assertNotEqual(failed.returncode, 0)
            self.assertIn('exit status 42', failed.stderr)
            self.assertEqual(capture.read_text().splitlines(), expected[:1])

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
                     WORKFLOW.replace("          github.job == 'formal-debug-build-trusted' && github.ref",
                                      "          github.job != 'formal-debug-build-trusted' && github.ref", 1),
                     WORKFLOW.replace('      - name: Run full test tier\n',
                                      '      - name: Run full test tier\n        continue-on-error: true\n', 1),
                     WORKFLOW.replace(' --host-smoke', ''),
                     WORKFLOW.replace('    if: always()', '    if: success()'),
                     WORKFLOW.replace("fromJSON(needs.select.outputs.plan).scope == 'full'", 'false', 1)]
        for workflow in mutations:
            self.assertNotEqual(workflow, WORKFLOW, 'mutation must exercise the current workflow')
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
                                   ('formal-debug-build', 'formal-debug-build-trusted'),
                                   ('formal-endpoint-build', 'formal-endpoint-build-trusted')):
                for name, expected in ((reader, not writer_expected), (writer, writer_expected)):
                    condition = JOBS[name].split('    if:', 1)[1].split('    runs-on:', 1)[0].strip().removeprefix('>-')
                    for token, value in values.items():
                        condition = condition.replace(token, repr(value))
                    condition = re.sub(r'!(?!=)', 'not ', condition.replace('&&', ' and ').replace('||', ' or '))
                    with self.subTest(job=name, profile=profile, trusted=trusted_source):
                        self.assertEqual(eval(' '.join(condition.split()), {'__builtins__': {}}, {}), expected)

    def test_selected_jobs_consume_fixed_source_and_same_plan(self):
        for name in ('baseline', 'oasis7-test-build-candidate', 'formal-test-build', 'formal-test-build-trusted',
                     'formal-debug-build', 'formal-debug-build-trusted',
                     'formal-endpoint-build', 'formal-endpoint-build-trusted', 'oasis7-required',
                     'formal-process-tests', 'workspace-support', 'full-regression', 'candidate-full-regression',
                     'windows-rollout', 'fleet-health'):
            with self.subTest(job=name):
                self.assertIn('ref: ${{ needs.select.outputs.tested_sha }}', JOBS[name])
        for name in ('oasis7-test-build-candidate', 'formal-test-build', 'formal-test-build-trusted',
                     'formal-debug-build', 'formal-debug-build-trusted',
                     'formal-endpoint-build', 'formal-endpoint-build-trusted', 'oasis7-required', 'formal-process-tests'):
            with self.subTest(plan_consumer=name):
                self.assertIn('name: ${{ needs.select.outputs.selection_artifact }}', JOBS[name])
                self.assertIn('--plan "$RUNNER_TEMP/selection.json"', JOBS[name])
        for name in ('formal-test-build', 'formal-test-build-trusted',
                     'formal-debug-build', 'formal-debug-build-trusted',
                     'formal-endpoint-build', 'formal-endpoint-build-trusted', 'formal-process-tests'):
            with self.subTest(formal_policy_consumer=name):
                self.assertIn('name: ${{ needs.select.outputs.authority_artifact }}', JOBS[name])
                commands = [line for line in JOBS[name].splitlines()
                            if re.search(r'ci-test-artifacts.py (?:build|pack|run|merge-formal) ', line)]
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


class SupportCapacity(unittest.TestCase):
    def probe(self, root, *, enabled='1', extra='', env=None):
        script = 'set -euo pipefail\nsource "$1/scripts/ci-tests.sh"\nsupport_capacity_snapshot fixture\n' + extra
        return subprocess.run(['bash', '-c', script, 'fixture', str(ROOT)], cwd=root,
                              env=dict(os.environ, OASIS7_SUPPORT_CAPACITY=enabled,
                                       RUNNER_TEMP=str(root / 'runner-temp'),
                                       CARGO_HOME=str(root / 'cargo'),
                                       RUSTUP_HOME=str(root / 'rustup'), **(env or {})),
                              text=True, capture_output=True, timeout=35)

    def records(self, result):
        return [json.loads(line.removeprefix('SUPPORT_CAPACITY '))
                for line in result.stdout.splitlines() if line.startswith('SUPPORT_CAPACITY ')]

    def test_opt_in_snapshot_reports_bytes_inodes_without_contents_or_mutation(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            deps = root / 'target/debug/deps'; deps.mkdir(parents=True)
            (root / 'runner-temp').mkdir()
            payload = deps / 'secret-name'; payload.write_bytes(b'PRIVATE-CONTENT' * 100)
            os.link(payload, deps / 'hardlink')
            result = self.probe(root)
            self.assertEqual(result.returncode, 0, result.stderr)
            records = self.records(result)
            self.assertTrue(records, result.stdout)
            identity = next(r for r in records if r.get('kind') == 'identity')
            cache_family = re.search(r'^\s+shared-key: (\S+)$', JOBS['workspace-support'], re.M).group(1)
            self.assertEqual(identity['cache_family'], cache_family)
            fs = next(r for r in records if r.get('kind') == 'filesystem' and r['label'] == 'workspace')
            self.assertGreater(fs['available_bytes'], 0)
            self.assertGreater(fs['available_inodes'], 0)
            usage = next(r for r in records if r.get('kind') == 'directory' and r['label'] == 'target/debug/deps')
            self.assertGreaterEqual(usage['apparent_bytes'], payload.stat().st_size)
            self.assertLessEqual(usage['apparent_bytes'], payload.stat().st_size + deps.stat().st_size)
            self.assertEqual(usage['inodes'], 2)
            self.assertNotIn('PRIVATE-CONTENT', result.stdout)
            self.assertNotIn('secret-name', result.stdout)
            self.assertEqual(payload.read_bytes(), b'PRIVATE-CONTENT' * 100)

    def test_disabled_probe_is_silent_and_observer_failure_preserves_test_failure(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            disabled = self.probe(root, enabled='0')
            self.assertEqual((disabled.returncode, disabled.stdout, disabled.stderr), (0, '', ''))
            result = self.probe(root, extra='exit 37\n', env={'PATH': '/bin', 'ci_python': '/missing-python'})
            self.assertEqual(result.returncode, 37, result.stderr)
            self.assertIn('SUPPORT_CAPACITY_WARNING', result.stdout)

    def test_directory_symlinks_are_not_followed_and_missing_paths_are_explicit(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            outside = root / 'outside'; outside.mkdir()
            (outside / 'secret').write_text('secret-value')
            (root / 'target').symlink_to(outside, target_is_directory=True)
            result = self.probe(root)
            self.assertEqual(result.returncode, 0, result.stderr)
            records = self.records(result)
            for label in ('target', 'target/debug/deps'):
                record = next(r for r in records if r.get('kind') == 'directory' and r['label'] == label)
                self.assertEqual(record['status'], 'symlink')
            missing = next(r for r in records if r.get('kind') == 'directory' and r['label'] == 'custody/artifacts')
            self.assertEqual(missing['status'], 'missing')

    def test_failed_directory_measurement_is_explicit_and_does_not_replace_exit_code(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory); (root / 'target').mkdir()
            binary = root / 'bin'; binary.mkdir()
            du = binary / 'du'; du.write_text('#!/bin/sh\necho PRIVATE-DU-ERROR >&2\nexit 13\n'); du.chmod(0o755)
            result = self.probe(root, extra='exit 41\n', env={'PATH': str(binary) + os.pathsep + os.environ['PATH']})
            self.assertEqual(result.returncode, 41, result.stderr)
            record = next(r for r in self.records(result) if r.get('kind') == 'directory' and r['label'] == 'target')
            self.assertEqual(record['status'], 'error')
            self.assertNotIn('PRIVATE-DU-ERROR', result.stdout + result.stderr)

    def test_outer_timeout_keeps_partial_evidence_and_original_failure(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory); (root / 'target').mkdir()
            binary = root / 'bin'; binary.mkdir()
            wrapper = binary / 'timeout'
            wrapper.write_text('#!/bin/sh\nshift\nexec /usr/bin/timeout 0.2s "$@"\n'); wrapper.chmod(0o755)
            du = binary / 'du'; du.write_text('#!/bin/sh\nexec /bin/sleep 5\n'); du.chmod(0o755)
            result = self.probe(root, extra='exit 43\n', env={'PATH': str(binary) + os.pathsep + os.environ['PATH']})
            self.assertEqual(result.returncode, 43, result.stderr)
            self.assertTrue(any(r['kind'] == 'filesystem' for r in self.records(result)))
            self.assertIn('observer_status=124', result.stdout)

    def test_enabled_observer_does_not_disable_profile_errexit(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            script = r'''set -euo pipefail
source "$1/scripts/ci-tests.sh"
ci_python="$2"; driver_dir="$1/scripts"; support_profiles='["oasis7_context"]'
run_cargo() { return 47; }
run_selected_workspace_support
printf 'unexpected-after-tests
'
'''
            result = subprocess.run(['bash', '-c', script, 'fixture', str(ROOT), sys.executable],
                                    cwd=root, env=dict(os.environ, OASIS7_SUPPORT_CAPACITY='1'),
                                    text=True, capture_output=True, timeout=35)
            self.assertEqual(result.returncode, 47, result.stderr)
            self.assertIn('profile-before:oasis7_context', result.stdout)
            self.assertNotIn('profile-after:oasis7_context', result.stdout)
            self.assertNotIn('support profile completed:', result.stdout)
            self.assertNotIn('unexpected-after-tests', result.stdout)

    def test_read_only_probes_surround_cache_profiles_and_custody_native_boundary(self):
        job = JOBS['workspace-support']
        self.assertLess(job.index('support_capacity_snapshot before-cache'), job.index('Swatinem/rust-cache@'))
        self.assertGreater(job.index('support_capacity_snapshot after-cache'), job.index('Swatinem/rust-cache@'))
        self.assertIn('if: always()', job)
        self.assertIn('support_capacity_snapshot job-exit', job)
        self.assertIn('OASIS7_SUPPORT_CAPACITY: "1"', job)
        self.assertIn('save-if: false', job)
        self.assertIn('runs-on: ubuntu-24.04', job)
        self.assertNotIn('continue-on-error:', job)
        driver = (ROOT / 'scripts/ci-tests.sh').read_text()
        self.assertIn('support_capacity_snapshot "profile-before:$profile"', driver)
        self.assertIn('support_capacity_snapshot "profile-after:$profile"', driver)
        custody = (ROOT / 'crates/oasis7_custody_worker/scripts/verify.sh').read_text()
        native = 'env -u RUSTC_WRAPPER cargo test -p oasis7_launcher_backend'
        self.assertLess(custody.index('support_capacity_snapshot custody-wasm-complete'), custody.index(native))
        self.assertIn('support_capacity_snapshot custody-native-complete', custody)
        self.assertIn('support_capacity_snapshot custody-platform-complete', custody)
class AptWorkflow(unittest.TestCase):
    NATIVE = ('pkg-config libwayland-dev wayland-protocols libxkbcommon-dev '
              'libx11-dev libxi-dev libxcursor-dev libxrandr-dev libxinerama-dev '
              'libasound2-dev libudev-dev').split()
    NATIVE_JOBS = {'pr-fast-preview', 'rust-baseline', 'oasis7-test-build-candidate',
                   'oasis7-test-build-trusted', 'formal-debug-build',
                   'formal-debug-build-trusted', 'formal-endpoint-build',
                   'formal-endpoint-build-trusted', 'formal-test-build',
                   'formal-test-build-trusted', 'oasis7-required', 'formal-process-tests',
                   'viewer-js-required', 'viewer-performance-report', 'pixel-world-bridge',
                   'launcher-web', 'workspace-support', 'scenario-regression'}

    def apt_steps(self):
        workflows = {'rust': WORKFLOW,
                     'cache': (ROOT / '.github/workflows/rust-cache.yml').read_text()}
        found = {}
        for workflow, source in workflows.items():
            self.assertNotIn('sudo apt-get ', source)
            for name, job in workflow_jobs(source).items():
                for step in re.findall(r'^      - .*?(?=^      - |\Z)', job, re.M | re.S):
                    if 'scripts/ci-apt-install.py' in step:
                        title = step.splitlines()[0].split('name: ', 1)[1]
                        found[workflow, name, title] = step
        return found

    def test_all_rust_apt_consumers_keep_packages_and_bounded_failure(self):
        expected = {('rust', name, 'Install native libraries'): self.NATIVE
                    for name in self.NATIVE_JOBS}
        expected.update({('rust', name, 'Install WASM C compiler'): ['clang', 'llvm']
                         for name in ('pr-fast-preview', 'net')})
        expected.update({('rust', name, 'Install system deps'): ['clang', 'llvm', *self.NATIVE]
                         for name in ('full-regression', 'candidate-full-regression')})
        expected['rust', 'newapi-bridge-package', 'Install system deps'] = self.NATIVE
        expected['rust', 'fleet-health', 'Ensure Linux process inspection dependency'] = ['lsof']
        expected['cache', 'shared-native-cache', 'Install system deps'] = ['clang', 'llvm', *self.NATIVE]
        actual = self.apt_steps()
        self.assertEqual(set(actual), set(expected))
        for key, step in actual.items():
            with self.subTest(consumer=key):
                self.assertIn('        shell: bash\n', step)
                self.assertIn('        timeout-minutes: 11\n', step)
                self.assertNotIn('continue-on-error', step)
                script = textwrap.dedent(step.split('        run: |\n', 1)[1])
                with tempfile.TemporaryDirectory() as directory:
                    capture = Path(directory) / 'argv'
                    stub = ('sudo() { printf "%s\\n" "$@" > "$CAPTURE"; return "$CODE"; };\n'
                            'clang() { echo wasm32; }; llvm-ar() { :; };\n'
                            'command() { if [[ \"$*\" == \"-v lsof\" ]]; then return 1; else builtin command \"$@\"; fi; };\n')
                    for code in (0, 42):
                        result = subprocess.run(['bash', '-euo', 'pipefail', '-c',
                                                 stub + script + '\nprintf reached-tests'],
                                                env=dict(os.environ, CAPTURE=str(capture), CODE=str(code)),
                                                text=True, capture_output=True)
                        self.assertEqual(result.returncode, code, result.stderr)
                        self.assertEqual('reached-tests' in result.stdout, code == 0)
                        self.assertEqual(capture.read_text().splitlines(),
                                         ['-n', '/usr/bin/python3', 'scripts/ci-apt-install.py',
                                          '--budget-seconds', '600', '--', *expected[key]])

    def test_existing_lsof_does_not_invoke_package_manager(self):
        step = self.apt_steps()['rust', 'fleet-health', 'Ensure Linux process inspection dependency']
        self.assertIn("if: runner.os == 'Linux'", step)
        script = textwrap.dedent(step.split('        run: |\n', 1)[1])
        stub = ('command() { return 0; };\n'
                'sudo() { echo unexpected-install >&2; return 99; };\n')
        result = subprocess.run(['bash', '-euo', 'pipefail', '-c', stub + script],
                                text=True, capture_output=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertNotIn('unexpected-install', result.stderr)

    def test_apt_helper_has_candidate_and_future_baseline_contracts(self):
        helper, test = 'scripts/ci-apt-install.py', 'scripts/ci-apt-install.test.py'
        for path in (helper, test):
            self.assertEqual(CONFIG['scoped_control_paths'][path], ['workflow_governance'])
            self.assertEqual(CONFIG['python_control_contracts'][path]['group'], 'workflow_governance')
            self.assertIn(test, CONFIG['python_control_contracts'][path]['tests'])
        self.assertIn('scripts/ci-workflow.test.py', CONFIG['python_control_contracts'][helper]['tests'])
        self.assertIn('run python3 ./scripts/ci-apt-install.test.py',
                      (ROOT / 'scripts/ci-tests.sh').read_text())


if __name__ == '__main__':
    unittest.main()
