#!/usr/bin/env python3
"""Self-contained delivery, apt, and capacity workflow contract tests."""
import itertools
import json
import os
from pathlib import Path
import re
import subprocess
import sys
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


def workspace_support_image_matrix(job):
    pair = "matrix.execution == 'signer-context-image'" in job
    protocol = "matrix.execution == 'signer-protocol-image'" in job
    assert not protocol or pair
    container_job = job
    if pair:
        image_condition = "matrix.execution == 'signer-context-image'"
        if protocol:
            image_condition = "(" + image_condition + " || matrix.execution == 'signer-protocol-image')"
        pair_image = (
            image_condition + " && "
            "'ghcr.io/nil-studio/oasis7/ci-rust-build@sha256:"
            "f121df903f97527a7097134d39f25822c46a3775152768a9cbd591fe65ee987c' || "
        )
        assert pair_image in job
        container_job = job.replace(pair_image, '', 1)
    container = re.search(
        r"^    container:\n      image: \$\{\{ (?:matrix\.execution == 'signer-image' && "
        r"'ghcr\.io/nil-studio/oasis7/ci-rust-build@sha256:([0-9a-f]{64})' \|\| )?matrix\.execution == 'image' && "
        r"'ghcr\.io/nil-studio/oasis7/ci-rust-build@sha256:([0-9a-f]{64})' \|\| '' \}\}$",
        container_job, re.M)
    if not container:
        return None
    expected_matrix = (
        "        execution: ${{ fromJSON(needs.select.outputs.workspace_support_host != 'false' "
        "&& '[\"host\",\"image\"]' || '[\"image\"]') }}\n")
    if container[1]:
        expected_matrix = expected_matrix.replace(
            'fromJSON(', 'fromJSON(needs.select.outputs.workspace_support_signer_only == \'true\' '
            "&& '[\"signer-image\"]' || ")
    if pair:
        assert container[1]
        expected_matrix = expected_matrix.replace(
            'fromJSON(', "fromJSON(needs.select.outputs.workspace_support_signer_context == 'true' "
            "&& '[\"signer-context-image\"]' || ",
        )
    if protocol:
        expected_matrix = expected_matrix.replace(
            'fromJSON(', "fromJSON(needs.select.outputs.workspace_support_signer_protocol == 'true' "
            "&& '[\"signer-protocol-image\"]' || ",
        )
    assert expected_matrix in job and '      fail-fast: false\n' in job
    assert '--support-execution "$SUPPORT_EXECUTION"' in job
    assert '          SUPPORT_EXECUTION: ${{ matrix.execution }}\n' in job
    steps = re.findall(r'^      - .*?(?=^      - |\Z)', job, re.M | re.S)
    apt = [step for step in steps if 'scripts/ci-apt-install.py' in step]
    assert len(apt) == 1 and "        if: matrix.execution == 'host'\n" in apt[0]
    caches = [step for step in steps if 'Swatinem/rust-cache@' in step]
    keys = {'host': 'ci-shared-native-trusted-v4',
            'image': 'ci-rust-build-' + container[2]}
    if container[1]:
        keys['signer-image'] = 'ci-rust-build-' + container[1]
    assert len(caches) == len(keys)
    for mode, key in keys.items():
        matching = [step for step in caches if f"        if: matrix.execution == '{mode}'\n" in step]
        assert len(matching) == 1
        assert f'          shared-key: {key}\n' in matching[0]
        assert '          save-if: false\n' in matching[0]
        assert '          add-rust-environment-hash-key: true\n' in matching[0]
    return keys['image']


def rust_image_cache_namespace(name, job):
    if name not in ('node', 'consensus', 'distfs', 'rust-baseline', 'pixel-world-bridge', 'net', 'scenario-regression', 'launcher-web', 'custody-native-contracts', 'custody-native', 'custody-wasm-production', 'workspace-support-other', 'cargo-tooling-contracts') or '    container:' not in job:
        return None
    if name == 'workspace-support-other' and (matrix := workspace_support_image_matrix(job)):
        return matrix
    target = {'net': 'rust-wasm-build', 'custody-wasm-production': 'rust-custody-wasm'}.get(name, 'rust-build')
    image = re.search(r'^    container:\n      image: ghcr\.io/nil-studio/oasis7/ci-' + target
                      + r'@sha256:([0-9a-f]{64})$', job, re.M)
    assert image, f'Rust cache readers require the pinned {target} image'
    assert '          save-if: false\n' in job, 'Rust image consumers may only read cache'
    return f'ci-{target}-{image.group(1)}'


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


def rust_cache_steps(job):
    return [step for step in re.findall(r'^      - .*?(?=^      - |\Z)', job, re.M | re.S)
            if 'uses: Swatinem/rust-cache@' in step]


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
            cache_family = re.search(r'^\s+shared-key: (\S+)$', JOBS['workspace-support-legacy'], re.M).group(1)
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
        job = JOBS['workspace-support-legacy']
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
                   'launcher-web', 'workspace-support-legacy', 'workspace-support-other', 'custody-native',
                   'custody-wasm-production', 'custody-node', 'scenario-regression'}

    def apt_steps(self):
        workflows = {'rust': WORKFLOW,
                     'cache': (ROOT / '.github/workflows/rust-cache.yml').read_text()}
        found = {}
        for workflow, source in workflows.items():
            self.assertNotIn('sudo apt-get ', source)
            for name, job in workflow_jobs(source).items():
                for step in re.findall(r'^      - .*?(?=^      - |\Z)', job, re.M | re.S):
                    if 'scripts/ci-apt-install.py' in step and '--prepare-sources-only' not in step:
                        title = step.splitlines()[0].split('name: ', 1)[1]
                        found[workflow, name, title] = step
        return found

    def test_all_rust_apt_consumers_keep_packages_and_bounded_failure(self):
        expected = {('rust', name, 'Install native libraries'): self.NATIVE
                    for name in self.NATIVE_JOBS}
        for name in ('rust-baseline', 'pixel-world-bridge', 'scenario-regression', 'launcher-web',
                     'custody-native', 'custody-wasm-production', 'workspace-support-other'):
            if rust_image_cache_namespace(name, JOBS[name]):
                if name != 'workspace-support-other' or not workspace_support_image_matrix(JOBS[name]):
                    del expected['rust', name, 'Install native libraries']
                packages = (ROOT / 'docker/ci-base/build-packages.txt').read_text().splitlines()
                self.assertLessEqual(set(self.NATIVE), set(packages))
        ordinary = JOBS['oasis7-test-build-trusted']
        if '    container:' in ordinary:
            self.assertIn('name: Verify trusted ordinary image and source', ordinary)
            self.assertIn('pkg-config --exists', ordinary)
            self.assertLessEqual(set(self.NATIVE), set((ROOT / 'docker/ci-base/build-packages.txt').read_text().splitlines()))
            del expected['rust', 'oasis7-test-build-trusted', 'Install native libraries']
        custody = ('rust', 'custody-node', 'Install native libraries')
        if re.search(r'^    container:', JOBS['custody-node'], re.M):
            del expected[custody]
        else:
            expected[custody] = ['gdb', 'gcc', *self.NATIVE]
        preview = JOBS['pr-fast-preview']
        if '    container:' in preview:
            self.assertIn("matrix.group == 'net' && 'ghcr.io/nil-studio/oasis7/ci-rust-wasm-build@", preview)
            self.assertIn('name: Verify the preinstalled WASM C compiler', preview)
            self.assertIn('pkg-config --exists', preview)
            self.assertLessEqual(set(self.NATIVE),
                                 set((ROOT / 'docker/ci-base/build-packages.txt').read_text().splitlines()))
            del expected['rust', 'pr-fast-preview', 'Install native libraries']
        else:
            expected['rust', 'pr-fast-preview', 'Install WASM C compiler'] = ['clang', 'llvm']
        if not rust_image_cache_namespace('net', JOBS['net']):
            expected['rust', 'net', 'Install WASM C compiler'] = ['clang', 'llvm']
        expected['rust', 'candidate-full-regression', 'Install system deps'] = ['clang', 'llvm', *self.NATIVE]
        if 'scripts/ci-trusted-full-regression.py full "$SOURCE_SHA"' in JOBS['full-regression']:
            dockerfile = (ROOT / 'docker/ci-base/Dockerfile').read_text()
            self.assertIn('FROM rust-custody-wasm AS rust-full-regression\n', dockerfile)
            self.assertIn('apt-get install -y --no-install-recommends clang llvm', dockerfile)
            self.assertLessEqual(set(self.NATIVE), set((ROOT / 'docker/ci-base/build-packages.txt').read_text().splitlines()))
        else:
            expected['rust', 'full-regression', 'Install system deps'] = ['clang', 'llvm', *self.NATIVE]
        newapi = JOBS['newapi-bridge-package']
        if '    container:' in newapi:
            image = re.search(r'^    container:\n      image: ghcr\.io/nil-studio/oasis7/ci-rust-build@sha256:([0-9a-f]{64})$',
                              newapi, re.M)
            self.assertIsNotNone(image)
            self.assertIn('shared-key: newapi-bridge-service-linux-x86-64-ci-rust-build-' + image.group(1), newapi)
            packages = (ROOT / 'docker/ci-base/build-packages.txt').read_text().splitlines()
            self.assertLessEqual(set(self.NATIVE), set(packages))
        else:
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
                        arguments = ['-n']
                        if key[0] == 'rust':
                            arguments.append('--preserve-env=GITHUB_ACTIONS,RUNNER_ENVIRONMENT,RUNNER_OS,ImageOS,HTTP_PROXY,HTTPS_PROXY,ALL_PROXY,http_proxy,https_proxy,all_proxy,NO_PROXY,no_proxy,APT_CONFIG,RES_OPTIONS,LOCALDOMAIN,HOSTALIASES')
                        arguments += ['/usr/bin/python3', 'scripts/ci-apt-install.py', '--budget-seconds', '600']
                        if key[0] == 'rust':
                            arguments.append('--hosted-dns-recovery')
                        self.assertEqual(capture.read_text().splitlines(), arguments + ['--', *expected[key]])

    def test_performance_browser_apt_transport_is_bounded_without_skipping_dependencies(self):
        step = next(step for step in re.findall(r'^      - .*?(?=^      - |\Z)', JOBS['viewer-performance-report'], re.M | re.S)
                    if 'name: Install pinned performance browser' in step)
        step_timeout = re.search(r'^        timeout-minutes:\s*(\d+)\s*$', step, re.M)
        self.assertIsNotNone(step_timeout)
        self.assertGreater(int(step_timeout.group(1)), 0)
        self.assertLessEqual(int(step_timeout.group(1)), 11)
        self.assertIn('uses: ./.github/actions/setup-ci-browser', step)
        self.assertIn('browser: agent', step)
        action = (ROOT / '.github/actions/setup-ci-browser/action.yml').read_text()
        self.assertIn('scripts/ci-apt-install.py --budget-seconds 300 --no-remove', action)
        self.assertLess(action.index('ci-browser-deps.py prepare'), action.index('scripts/ci-apt-install.py'))
        self.assertLess(action.index('scripts/ci-apt-install.py'), action.index('ci-browser-deps.py activate'))
        self.assertNotIn('continue-on-error', step)

    def test_existing_lsof_does_not_invoke_package_manager(self):
        step = self.apt_steps()['rust', 'fleet-health', 'Ensure Linux process inspection dependency']
        self.assertRegex(step, r"if:\s*(?:>-\s*)?runner\.os == 'Linux'")
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
        self.assertIn('scripts/ci-apt-install.test.py', CONFIG['workflow_governance_python_tests'])
        for path in ('scripts/ci-apt-dns.py', 'scripts/ci-apt-dns.test.py'):
            self.assertEqual(CONFIG['scoped_control_paths'][path], ['workflow_governance'])
            self.assertIn('scripts/ci-apt-dns.test.py', CONFIG['python_control_contracts'][path]['tests'])
        self.assertIn('scripts/ci-apt-dns.test.py', CONFIG['workflow_governance_python_tests'])

    def test_browser_install_prepares_the_shared_mirrors_and_blocks_on_failure(self):
        expected = {'viewer-js-required': ('agent', 11),
                    'viewer-performance-report': ('agent', 11), 'site-quality': ('playwright', 4)}
        for name, (browser, limit) in expected.items():
            job = JOBS[name]
            job_timeout = int(re.search(r'^    timeout-minutes:\s*(\d+)\s*$', job, re.M).group(1))
            step = next(step for step in re.findall(r'^      - .*?(?=^      - |\Z)', job, re.M | re.S)
                        if 'uses: ./.github/actions/setup-ci-browser' in step)
            self.assertIn('browser: ' + browser, step)
            step_timeout = re.search(r'^        timeout-minutes:\s*(\d+)\s*$', step, re.M)
            self.assertIsNotNone(step_timeout)
            actual_step = int(step_timeout.group(1))
            self.assertGreater(actual_step, 0)
            self.assertLessEqual(actual_step, limit)
            self.assertLess(actual_step, job_timeout)
            self.assertNotIn('continue-on-error', step)
            self.assertLess(job.index('uses: ./.github/actions/setup-ci-browser'), job.index('name: Execute selected group'))
        action = (ROOT / '.github/actions/setup-ci-browser/action.yml').read_text()
        step = next(step for step in re.findall(r'^    - .*?(?=^    - |\Z)', action, re.M | re.S)
                    if 'name: Install missing browser system dependencies' in step)
        script = textwrap.dedent(step.split('      run: |\n', 1)[1])
        self.assertIn('ci-apt-install.py --budget-seconds 300 --no-remove', script)
        self.assertNotIn('continue-on-error', step)
        for missing, code in ((True, 0), (True, 42), (False, 0)):
            with self.subTest(missing=missing, code=code), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                work = root / 'oasis7-ci-browser-agent'
                work.mkdir()
                (work / 'apt-arguments.txt').write_text('--\nlibnss3\n' if missing else '')
                capture = root / 'called'
                result = subprocess.run(['bash', '-euo', 'pipefail', '-c',
                    'sudo() { printf "%s\\n" "$@" > "$CAPTURE"; return "$CODE"; };\n' + script + '\nprintf browser-activation'],
                    env=dict(os.environ, RUNNER_TEMP=str(root), BROWSER_KIND='agent',
                             CAPTURE=str(capture), CODE=str(code)), text=True, capture_output=True)
                self.assertEqual(result.returncode, code if missing else 0, result.stderr)
                self.assertEqual('browser-activation' in result.stdout, not missing or code == 0)
                self.assertEqual(capture.exists(), missing)
                if missing:
                    self.assertEqual(capture.read_text().splitlines(), ['-n', '/usr/bin/python3',
                        'scripts/ci-apt-install.py', '--budget-seconds', '300', '--no-remove', '--', 'libnss3'])


class TargetedOrdinaryArtifactWorkflow(unittest.TestCase):
    def test_prebuilt_native_consumer_is_limited_to_v2_artifact_partitions(self):
        job = JOBS['oasis7-required']
        image = re.search(r"^      image: >-\n(.*?)^      options:", job, re.M | re.S)[1].strip()
        condition, image_pin = re.fullmatch(
            r"\$\{\{ (.*) &&\s+'(ghcr\.io/nil-studio/oasis7/ci-rust-build@sha256:[0-9a-f]{64})' \|\| '' \}\}",
            image, re.S).groups()
        owner_list = re.search(r"fromJSON\('([^']+)'\)", condition)[1]
        for execution, partition_version, delivery_version, owner in itertools.product(
                (None, 'inline'), (None, 1, 2), (None, 1, 2),
                ('controls', 'all', 'runtime', 'component-libs', 'integrations-0',
                 'native-0', 'native-1', 'native-2', 'native-3', 'native-4')):
            values = {
                'fromJSON(needs.select.outputs.plan).ordinary_execution': execution,
                'fromJSON(needs.select.outputs.plan).ordinary_artifact_partition_version': partition_version,
                'fromJSON(needs.select.outputs.plan).ordinary_artifact_delivery_version': delivery_version,
                f"fromJSON('{owner_list}')": json.loads(owner_list),
                'matrix.partition': owner,
            }
            expected = (execution != 'inline' and partition_version == delivery_version == 2
                        and owner in ('native-0', 'native-1', 'native-2', 'native-3'))
            with self.subTest(execution=execution, partition_version=partition_version,
                              delivery_version=delivery_version, owner=owner):
                self.assertEqual(effective_job_condition(condition, values, []), expected)
        self.assertIn(image_pin, job)
        self.assertIn('options: --user 1001:1001 --init', job)
        self.assertIn('\ncache-mode: read\n', WORKFLOW)
        self.assertNotIn('uses: actions/cache/save@', job)
        self.assertEqual(len(rust_cache_steps(job)), 1)
        self.assertIn('          save-if: false\n', rust_cache_steps(job)[0])
        for step in ('Install pinned Rust toolchains', 'Install native libraries'):
            script = workflow_step_script(job, step)
            image = subprocess.run(['/bin/bash', '-e', '-c', script], capture_output=True,
                                   env={'NATIVE_CONTAINER': 'true', 'PATH': '/no-tools'})
            self.assertEqual(image.returncode, 0, image.stderr)

    def test_native_path_hook_does_not_change_child_shell_working_directories(self):
        job = JOBS['oasis7-required']
        steps = re.findall(r'^      - .*?(?=^      - |\Z)', job, re.M | re.S)
        hooks = [step.splitlines()[0] for step in steps if '          BASH_ENV:' in step]
        self.assertEqual(hooks, ['      - name: Unpack exact ordinary consumption closure',
                                 '      - name: Execute selected group'])
        preflight = workflow_step_script(job, 'Verify prebuilt native artifact consumer environment')
        hook = preflight.split("<<'NATIVE_ENV'\n", 1)[1].split('\nNATIVE_ENV', 1)[0]
        canonical = '/home/runner/work/oasis7/oasis7'
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            (root / 'child').mkdir()
            path = root / 'hook'
            path.write_text(hook.replace(canonical, str(root)))
            result = subprocess.run(['bash', '-e', '-c', """
                test "$PWD" = "$GITHUB_WORKSPACE"
                test "$ImageOS" = ubuntu24
                test -z "${BASH_ENV+x}"
                cd child
                bash -c 'test "$PWD" = "$GITHUB_WORKSPACE/child"'
                """], cwd=root / 'child', env=dict(os.environ, BASH_ENV=str(path)), capture_output=True)
            self.assertEqual(result.returncode, 0, result.stderr)

    def test_only_chain_runtime_native_artifact_owners_skip_compiler_preparation(self):
        job = JOBS['oasis7-required']
        steps = re.findall(r'^      - .*?(?=^      - |\Z)', job, re.M | re.S)
        prepared = [step for step in steps if any(marker in step for marker in (
            'name: Install pinned Rust toolchains', 'name: Stabilize Rust cache toolchains',
            'uses: Swatinem/rust-cache@'))]
        self.assertEqual(len(prepared), 3)
        named_bundle_owners = ('runtime', 'component-libs', 'integrations-0', 'integrations-1',
                               'native-0', 'native-1', 'native-2', 'native-3')
        for step in prepared:
            is_toolchain = 'name: Install pinned Rust toolchains' in step
            condition = re.search(r'^        if: >-\n(.*?)(?=^        \w|\Z)', step, re.M | re.S)[1]
            for execution in (None, 'inline'):
                for version in (None, 1, 2):
                    if execution == 'inline' and version == 2:
                        continue
                    for owner in ('controls', 'native-0', 'native-1', 'native-2', 'native-3',
                                  'all', 'runtime', 'component-libs', 'integrations-0', 'integrations-1',
                                  'oasis7_runtime', 'unknown'):
                        for packages in (['oasis7_chain_runtime'], ['oasis7_chain_runtime', 'oasis7_game_client'],
                                         ['oasis7_wasm_materializer'], ['oasis7_game_client'], []):
                            values = {
                                'fromJSON(needs.select.outputs.plan).ordinary_execution': execution,
                                'fromJSON(needs.select.outputs.plan).ordinary_artifact_partition_version': version,
                                'matrix.partition': owner,
                                'fromJSON(needs.select.outputs.plan).oasis7_required_packages': packages,
                                "join(fromJSON(needs.select.outputs.plan).oasis7_required_packages, ',')": ','.join(packages),
                            }
                            must_run = (
                                execution == 'inline'
                                or owner not in named_bundle_owners
                                or (is_toolchain and owner != 'runtime' and (
                                    version != 2
                                    or not packages
                                    or 'oasis7_game_client' in packages
                                    or 'oasis7_wasm_materializer' in packages
                                ))
                            )
                            actual = effective_job_condition(condition, values, [])
                            with self.subTest(step=is_toolchain, execution=execution, version=version, owner=owner, packages=packages):
                                if must_run:
                                    self.assertTrue(actual)
                                self.assertFalse(effective_job_condition(condition, values, ['failure']))
        native = next(step for step in steps if 'name: Install native libraries' in step)
        self.assertNotIn('        if:', native)
        self.assertIn('scripts/ci-apt-install.py --budget-seconds 600', native)

    def test_actual_native_consumer_forwards_every_trusted_owner_and_failure(self):
        job = JOBS['oasis7-required']
        header = job.split('    steps:', 1)[0]
        self.assertIn('partition: ${{ fromJSON(needs.select.outputs.ordinary_partitions) }}', header)
        self.assertIn('max-parallel: 5', header)
        self.assertIn('fail-fast: false', header)
        self.assertNotIn('include:', header)
        self.assertNotIn('exclude:', header)
        self.assertTrue(
            'needs: [select, oasis7-test-build-candidate, oasis7-test-build-trusted]' in header
            or 'needs: [select, fast-preflight, oasis7-test-build-candidate, oasis7-test-build-trusted]' in header
        )
        self.assertIn('oasis7-required', JOBS['required-gate'])
        step = job.split('      - name: Execute selected group\n', 1)[1]
        script = textwrap.dedent(step.split('        run: ', 1)[1])
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / 'scripts').mkdir()
            (root / 'scripts/ci-tests.sh').write_text(
                '#!/usr/bin/env bash\nprintf "%s\\0" "$@" > "$ARGV_LOG"\nexit "$DRIVER_EXIT"\n')
            for owner in ('controls', 'native-0', 'native-1', 'native-2', 'native-3',
                          'all', 'runtime', 'component-libs', 'integrations-0', 'integrations-1',
                          'oasis7_runtime', 'oasis7_world_state'):
                for code in (0, 19):
                    result = subprocess.run(['bash', '-c', script], cwd=root,
                        text=True, capture_output=True, env=dict(os.environ,
                        RUNNER_TEMP=str(root), GITHUB_WORKSPACE=str(root), ORDINARY_PARTITION=owner,
                        ARGV_LOG=str(root / 'args'), DRIVER_EXIT=str(code)))
                    with self.subTest(owner=owner, code=code):
                        self.assertEqual(result.returncode, code, result.stdout + result.stderr)
                        self.assertEqual((root / 'args').read_text().strip('\0').split('\0'), [
                            'required', '--group', 'oasis7_required', '--plan', str(root / 'selection.json'),
                            '--ordinary-partition', owner, '--repo-root', str(root)])


if __name__ == '__main__':
    unittest.main()
