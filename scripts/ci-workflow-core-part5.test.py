#!/usr/bin/env python3
"""Self-contained core workflow viewer cache and web WASM cache writer contract tests (part 5)."""
import itertools
import os
from pathlib import Path
import re
import subprocess
import tempfile
import textwrap
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


def workflow_step_script(job, name):
    step = next(step for step in re.findall(r'^      - .*?(?=^      - |\Z)', job, re.M | re.S)
                if 'name: ' + name + '\n' in step)
    if '        run: |\n' in step:
        return textwrap.dedent(step.split('        run: |\n', 1)[1])
    return step.split('        run: ', 1)[1].splitlines()[0] + '\n'


def rust_cache_steps(job):
    return [step for step in re.findall(r'^      - .*?(?=^      - |\Z)', job, re.M | re.S)
            if 'uses: Swatinem/rust-cache@' in step]


def viewer_image_digest(job):
    header = job.split('    steps:', 1)[0]
    if '    container:' not in header:
        return None
    match = re.search(
        r"(?m)^      image: \$\{\{ needs\.select\.outputs\.viewer_image_compatible == 'true' && "
        r"'ghcr\.io/nil-studio/oasis7/ci-rust-viewer@sha256:([0-9a-f]{64})' \|\| '' \}\}$", header)
    assert match, 'Viewer requires the pinned image with a host fallback'
    assert '      options: --user 1001:1001 --init\n' in header
    assert header.count('    env:\n') == 1
    assert ('    env:\n      VIEWER_IMAGE_COMPATIBLE: ${{ needs.select.outputs.viewer_image_compatible }}\n' in header)
    return match[1]


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
    if digest := viewer_image_digest(job):
        expected_key = ("${{ env.VIEWER_IMAGE_COMPATIBLE == 'true' && 'ci-rust-viewer-" + digest
                        + "' || 'release-packages-web-wasm-trusted-v3' }}")
        assert inputs.get('shared-key') == expected_key, 'Viewer image and host cache identities must stay isolated'
        inputs['shared-key'] = web['shared-key']
    assert inputs in (native, web), 'unknown or incomplete Viewer cache configuration'
    assert 'cache-mode: write' not in job
    assert ('    env:' not in job.split('    steps:', 1)[0] or digest) and '        env:' not in cache
    assert 'GH_TOKEN:' not in cache and 'OASIS7_DEPENDENCY_CACHE_' not in cache
    prepare = next(step for step in re.findall(r'^      - .*?(?=^      - |\Z)', job, re.M | re.S)
                   if 'python3 scripts/ci-rust-cache-toolchains.py' in step)
    expected = '"$RUST_TOOLCHAIN"' + (' "$OASIS7_WASM_TOOLCHAIN"' if inputs == native else '')
    assert re.search(r'^        run: python3 scripts/ci-rust-cache-toolchains.py ' +
                     re.escape(expected) + r'$', prepare, re.M)
    assert 'continue-on-error:' not in prepare
    prepare_if = re.search(r'^        if: .*?(?=^        [a-z-]+:|\Z)', prepare, re.M | re.S)
    cache_if = re.search(r'^        if: .*?(?=^        [a-z-]+:|\Z)', cache, re.M | re.S)
    if digest:
        host_only = "env.VIEWER_IMAGE_COMPATIBLE != 'true'"
        if cache_if:
            original = cache_if.group(0).removeprefix('        if: >-\n').strip()
            expected = "        if: >-\n          " + host_only + " &&\n          (" + original + ")"
        else:
            expected = '        if: ' + host_only
        assert prepare_if and prepare_if.group(0).strip() == expected.strip()
    else:
        assert (prepare_if.group(0) if prepare_if else None) == (cache_if.group(0) if cache_if else None)
    assert job.index('rustup default') < job.index(prepare) < job.index(cache)
    before_cache = job[:job.index(cache)]
    assert 'rustup target add wasm32-unknown-unknown' in before_cache
    if inputs == web:
        assert 'OASIS7_WASM' not in before_cache and 'nightly' not in before_cache
        assert re.findall(r'^          rustup toolchain install (\S+)', before_cache, re.M) == ['"$RUST_TOOLCHAIN"']
    else:
        assert 'rustup toolchain install "$OASIS7_WASM_TOOLCHAIN"' in before_cache
    return 'native' if inputs == native else 'web'


class WorkflowCorePart5(unittest.TestCase):
    def shared_cache_workflow(self):
        path = ROOT / '.github/workflows/rust-cache.yml'
        self.assertTrue(path.is_file(), 'dependency warming must have an independent workflow')
        workflow = path.read_text()
        jobs = workflow_jobs(workflow)
        return workflow, jobs['shared-native-cache']

    def test_viewer_cache_transition_requires_the_entire_known_configuration(self):
        for name in ('viewer-js-required', 'viewer-performance-report'):
            if name not in JOBS:
                continue
            actual = JOBS[name]
            kind = viewer_cache_kind(actual)
            if viewer_image_digest(actual):
                self.assertEqual(kind, 'web')
                for before, after in (
                    ('ci-rust-viewer-', 'ci-rust-build-'),
                    ("env.VIEWER_IMAGE_COMPATIBLE == 'true' && 'ci-rust-viewer-",
                     "env.VIEWER_IMAGE_COMPATIBLE != 'true' && 'ci-rust-viewer-"),
                    ('release-packages-web-wasm-trusted-v3', 'ci-shared-native-trusted-v4'),
                    ('save-if: false', 'save-if: true'),
                    ('add-rust-environment-hash-key: true', 'add-rust-environment-hash-key: false'),
                    ('env-vars: CARGO CC CFLAGS CXX CMAKE RUST\n',
                     'env-vars: CARGO CC CFLAGS CXX CMAKE RUST OASIS7_WASM\n'),
                    ('cache-on-failure: false', 'cache-on-failure: false\n          workspaces: . -> other-target'),
                ):
                    self.assertIn(before, actual)
                    with self.subTest(job=name, mutation=before), self.assertRaises(AssertionError):
                        viewer_cache_kind(actual.replace(before, after))
                preparation = re.search(r'(?ms)^      - name: Stabilize Rust cache toolchains\n.*?(?=^      - )', actual)[0]
                false_guard = preparation.replace("env.VIEWER_IMAGE_COMPATIBLE != 'true'",
                                                  "env.VIEWER_IMAGE_COMPATIBLE == 'true'")
                self.assertNotEqual(preparation, false_guard)
                with self.subTest(job=name, mutation='mutable image toolchain'), self.assertRaises(AssertionError):
                    viewer_cache_kind(actual.replace(preparation, false_guard))
                continue
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

    def test_viewer_image_eligibility_uses_tested_git_inputs_and_preserves_host_fallback(self):
        selection = JOBS['select']
        if '      viewer_image_compatible:' not in selection:
            return
        step = workflow_step_script(selection, 'Select the compatible Viewer image environment')
        inputs = ('rust-toolchain.toml', 'scripts/ci-browser-deps.py', 'scripts/ci_process_runner.py',
                  'scripts/ci-browser-tools/package.json', 'scripts/ci-browser-tools/package-lock.json',
                  'scripts/ensure-wasm-bindgen-cli.sh', 'scripts/wasm-bindgen-cli-common.sh')
        with tempfile.TemporaryDirectory() as temporary:
            fixture = Path(temporary).resolve()

            def git(*args):
                return subprocess.check_output(['git', '-C', str(fixture), '-c', 'user.email=ci@example.invalid',
                    '-c', 'user.name=CI Fixture', '-c', 'commit.gpgsign=false', *args], text=True).strip()

            git('init', '-q')
            for name in inputs:
                path = fixture / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(name)
            git('add', '.')
            git('commit', '-qm', 'Frozen Viewer image inputs')
            source = git('rev-parse', 'HEAD')
            script, count = re.subn(r'(?m)^image_source=[0-9a-f]{40}$', 'image_source=' + source, step)
            self.assertEqual(count, 1)

            def check(tested_sha, compatible):
                output = fixture / 'github-output'
                output.unlink(missing_ok=True)
                result = subprocess.run(['bash', '-euo', 'pipefail', '-c', script], cwd=fixture,
                    env=dict(os.environ, TESTED_SHA=tested_sha, GITHUB_OUTPUT=str(output)), text=True, capture_output=True)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(output.read_text(), 'compatible=' + str(compatible).lower() + '\n')

            (fixture / 'unrelated').write_text('new payload')
            git('add', 'unrelated')
            git('commit', '-qm', 'Unrelated new source')
            check(git('rev-parse', 'HEAD'), True)
            for name in inputs:
                with self.subTest(changed=name):
                    path = fixture / name
                    path.unlink()
                    git('add', name)
                    git('commit', '-qm', 'Incompatible historical input')
                    check(git('rev-parse', 'HEAD'), False)
                    git('reset', '--hard', '-q', source)
            check('f' * 40, False)
            check('not-a-commit', False)

        for name, browser in (('viewer-js-required', 'security smoke'),
                              ('viewer-performance-report', 'performance')):
            if name not in JOBS:
                continue
            job = JOBS[name]
            self.assertIsNotNone(viewer_image_digest(job))
            self.assertIn("Install pinned " + browser + " browser\n        if: env.VIEWER_IMAGE_COMPATIBLE != 'true'", job)
            self.assertIn("if: env.VIEWER_IMAGE_COMPATIBLE == 'true'", job)
            self.assertIn('activate --prebuilt --browser agent', job)
            self.assertLess(job.index('activate --prebuilt --browser agent'), job.index('Execute selected group'))
            self.assertIn('export XDG_CACHE_HOME=/home/runner/.cache', job)
            self.assertIn('export XDG_CACHE_HOME=/.cache', job)

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
        image = '    container:' in header
        image_ref = 'ghcr.io/nil-studio/oasis7/ci-rust-release-web@sha256:f47cf4a0ce572de9b871475aed4e38c35fc7e6e128dda93a7d96e50480e03c59'
        if image:
            self.assertIn('      image: ' + image_ref + '\n', header)
            self.assertIn('      options: --user 1001:1001 --init\n', header)
            self.assertNotIn('volumes:', header)
            self.assertNotIn('privileged', header)
            self.assertEqual(header.split('    env:\n', 1)[1],
                             '      RUST_RELEASE_WEB_IMAGE: ' + image_ref + '\n')
        else:
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
        self.assertIn('ref: ${{ github.sha }}\n' + ('          fetch-depth: 0\n' if image else '') + '          persist-credentials: false', writer)
        self.assertIn('npm ci --prefix crates/oasis7_viewer', writer)
        self.assertNotIn('OASIS7_WASM', writer)
        self.assertNotIn('nightly', writer)
        if not image:
            self.assertIn('run: python3 scripts/ci-rust-cache-toolchains.py "$RUST_TOOLCHAIN"\n', writer)
        self.assertEqual(len(rust_cache_steps(writer)), 1)
        cache = rust_cache_steps(writer)[0]
        self.assertNotRegex(cache, re.compile(r'^        if:', re.M))
        self.assertIn('uses: Swatinem/rust-cache@d1527c8f67ef9af357290475d4af4b0fab8f12e0 # v2\n', cache)
        before_cache = writer[:writer.index(cache)]
        if image:
            preflight = next(step for step in steps if 'name: Verify the fixed Release Web image and main source\n' in step)
            self.assertEqual(preflight, '      - name: Verify the fixed Release Web image and main source\n'
                '        run: python3 scripts/ci-release-web-image.py --preflight --image-source '
                'd5319297d754a9c7d4de347a2ce8e1a20ba43591 --source "$GITHUB_SHA"\n')
            self.assertEqual(writer.count(preflight), 1)
            self.assertLess(steps.index(preflight), steps.index(source))
            self.assertLess(writer.index(preflight), writer.index('npm ci'))
            self.assertNotIn('rustup toolchain install', writer)
            self.assertNotIn('ci-rust-cache-toolchains.py', writer)
        else:
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
        if not image:
            self.assertNotIn('env-vars:', cache)
        save = re.search(r'^          save-if: \$\{\{ (.*) \}\}$', cache, re.M)[1]
        with_body = cache.split('        with:\n', 1)[1]
        inputs = re.findall(r'^          ([\w-]+): (.+)$', with_body, re.M)
        self.assertEqual(len(inputs), len(with_body.strip().splitlines()))
        expected_inputs = {'shared-key': ('ci-rust-release-web-' + image_ref.split('@sha256:')[1] if image else 'release-packages-web-wasm-trusted-v3'),
                           'cache-provider': 'github', 'cache-on-failure': 'false',
                           'save-if': '${{ ' + save + ' }}'}
        if image:
            expected_inputs.update({'add-rust-environment-hash-key': 'true', 'env-vars': 'CARGO CC CFLAGS CXX CMAKE RUST'})
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
            'OASIS7_DEPENDENCY_CACHE_FAMILY': 'release-web-image' if image else 'release-web',
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
        if not image:
            normalizer = next(step for step in steps if 'python3 scripts/ci-rust-cache-toolchains.py' in step)
            self.assertNotRegex(normalizer, re.compile(r'^        if:', re.M))
            self.assertLess(writer.index('rustup default'), writer.index('python3 scripts/ci-rust-cache-toolchains.py'))
            self.assertLess(writer.index('python3 scripts/ci-rust-cache-toolchains.py'), writer.index(cache))
        self.assertLess(steps.index(install), steps.index(cache))
        completion = next(step for step in steps if 'name: Complete Web WASM cache\n' in step)
        self.assertIn('id: web-cache\n', cache)
        self.assertIn('set -euo pipefail', completion)
        self.assertIn('./scripts/build-viewer-software-safe.sh --optional-payload-dir output/release/viewer-optional-payload', completion)
        self.assertLess(steps.index(cache), steps.index(completion))
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
        # The host writer owns toolchain installation after the Release reader
        # moves to a preinstalled compiler. Keep the older layout as a fixture.
        host_web = workflow_jobs(workflow).get('web-wasm-cache', release_web)
        host_install = ('      - name: Install pinned Rust toolchain\n'
                        '        run: |\n'
                        '          rustup toolchain install "${RUST_TOOLCHAIN}" --profile default\n'
                        '          rustup target add wasm32-unknown-unknown --toolchain "${RUST_TOOLCHAIN}"\n'
                        '          rustup default "${RUST_TOOLCHAIN}"\n')
        header = native.split('    steps:', 1)[0]
        preparation = ''.join((step(native, 'uses: actions/checkout@'),
            step(native, 'name: Verify current main cache source\n'),
            step(release_web, 'uses: actions/setup-node@'),
            step(release_web, 'name: Install viewer web dependencies\n'),
            host_install))
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

        actual = workflow_jobs(workflow)['web-wasm-cache']
        if '    container:' in actual:
            self.assert_web_cache_writer(workflow, release)
            guard = step(actual, 'name: Verify the fixed Release Web image and main source\n')
            for broken in (
                    actual.replace('ci-rust-release-web@sha256:', 'ci-rust-build@sha256:'),
                    actual.replace('shared-key: ci-rust-release-web-', 'shared-key: unrelated-'),
                    actual.replace('FAMILY: release-web-image', 'FAMILY: release-web'),
                    actual.replace('env-vars: CARGO CC CFLAGS CXX CMAKE RUST', 'env-vars: CARGO'),
                    actual.replace('          cache-provider: github', '          cache-provider: github\n          cache-targets: false'),
                    actual.replace(guard, '') + guard,
                    actual.replace(guard, guard.replace('        run:', '        continue-on-error: true\n        run:')),
                    actual.replace('RUST_RELEASE_WEB_IMAGE:', 'UNRELATED_IMAGE:')):
                with self.subTest(image_mutation=broken), self.assertRaises(AssertionError):
                    self.assert_web_cache_writer(assembled(broken), release)

    def test_web_cache_build_runs_both_contexts_and_propagates_each_failure(self):
        workflow, _ = self.shared_cache_workflow()
        writer = workflow_jobs(workflow)['web-wasm-cache']
        completion = next(step for step in re.findall(r'^      - .*?(?=^      - |\Z)', writer, re.M | re.S)
                          if 'name: Complete Web WASM cache\n' in step)
        script = textwrap.dedent(completion.split('        run: |\n', 1)[1])
        self.assertNotIn('continue-on-error:', completion)
        with tempfile.TemporaryDirectory(prefix='web-cache-build-') as directory:
            root = Path(directory)
            (root / 'scripts').mkdir()
            (root / 'crates/oasis7_client_launcher').mkdir(parents=True)
            (root / 'bin').mkdir()
            viewer = root / 'scripts/build-viewer-software-safe.sh'
            viewer.write_text('''#!/usr/bin/env bash
set -euo pipefail
test "$OASIS7_FORCE_VIEWER_SOFTWARE_SAFE_BUILD" = 1
test "$*" = '--optional-payload-dir output/release/viewer-optional-payload'
printf 'viewer\\n' >> "$WEB_CACHE_FIXTURE_TRACE"
exit "$WEB_CACHE_FIXTURE_VIEWER_EXIT"
''')
            trunk = root / 'bin/trunk'
            trunk.write_text('''#!/usr/bin/env bash
set -euo pipefail
test -z "${NO_COLOR+x}"
test "$PWD" = "$WEB_CACHE_FIXTURE_ROOT/crates/oasis7_client_launcher"
test "$*" = 'build --release --dist ../../output/release/web-launcher-dist'
printf 'trunk\\n' >> "$WEB_CACHE_FIXTURE_TRACE"
exit "$WEB_CACHE_FIXTURE_TRUNK_EXIT"
''')
            for path in (viewer, trunk):
                path.chmod(0o755)
            trace = root / 'calls'
            for viewer_exit, expected_calls in ((0, ['viewer']), (17, ['viewer'])):
                trace.write_text('')
                environment = dict(os.environ, PATH=str(root / 'bin') + os.pathsep + os.environ['PATH'],
                    NO_COLOR='fixture-value', WEB_CACHE_FIXTURE_ROOT=str(root),
                    WEB_CACHE_FIXTURE_TRACE=str(trace), WEB_CACHE_FIXTURE_VIEWER_EXIT=str(viewer_exit))
                result = subprocess.run(['bash', '--noprofile', '--norc', '-e', '-o', 'pipefail', '-c', script],
                                        cwd=root, env=environment, capture_output=True, text=True, timeout=10)
                with self.subTest(viewer=viewer_exit):
                    self.assertEqual(result.returncode, viewer_exit, result.stdout + result.stderr)
                    self.assertEqual(trace.read_text().splitlines(), expected_calls)


if __name__ == '__main__':
    unittest.main()
