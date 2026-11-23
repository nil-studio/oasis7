#!/usr/bin/env python3
"""Exercise custody file completeness, artifact relocation and failed execution."""
import copy
from collections import Counter
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest import mock
import zipfile

SPEC = importlib.util.spec_from_file_location('custody', Path(__file__).with_name('ci-custody-platform.py'))
C = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(C)


class Custody(unittest.TestCase):
    def test_isolated_wasm_contracts_exports_real_fixture_and_propagates_failure(self):
        source = Path(__file__).resolve().parents[1] / C.WORKER / 'scripts/verify.sh'
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            script = root / C.WORKER / 'scripts/verify.sh'
            script.parent.mkdir(parents=True)
            shutil.copyfile(source, script)
            binary = root / 'bin'
            binary.mkdir()
            cargo = binary / 'cargo'
            cargo.write_text("#!/usr/bin/env python3\nimport os,sys\nfrom pathlib import Path\na=sys.argv[1:]\nwith open(os.environ['CAPTURE'],'a') as f: f.write(' '.join(a)+'\\n')\nif 'export_developer_admission_fixture' in a:\n    if os.environ['EXPORT_RESULT'] != '0': sys.exit(int(os.environ['EXPORT_RESULT']))\n    Path(os.environ['OASIS7_CUSTODY_ADMISSION_FIXTURE_OUTPUT']).write_text('exported-by-test-double')\n    sys.exit(0)\nif 'oasis7_custody_worker' in a:\n    feature=a[a.index('--features')+1] if '--features' in a else ''\n    if not feature or (',' in feature and 'harness' not in feature):\n        print('requires exactly one role feature'); sys.exit(1)\n    if feature == 'identity,approval-harness':\n        print('approval-harness requires owner'); sys.exit(1)\n    if 'native-runtime-harness' in feature and not feature.startswith('identity,'):\n        print('native-runtime-harness requires identity'); sys.exit(1)\n")
            cargo.chmod(0o755)
            for status in ('0', '71'):
                with self.subTest(export_result=status):
                    capture = root / 'commands'
                    capture.write_text('')
                    environment = dict(os.environ, PATH=str(binary) + os.pathsep + os.environ['PATH'],
                                       CAPTURE=str(capture), EXPORT_RESULT=status, OASIS7_SUPPORT_CAPACITY='0')
                    result = subprocess.run(['bash', str(script), '--stage', 'wasm-contracts'],
                                            env=environment, text=True, capture_output=True)
                    self.assertEqual(result.returncode, int(status), result.stdout + result.stderr)
                    lines = capture.read_text().splitlines()
                    self.assertEqual(lines[-1], 'test -p oasis7_custody_core --test approval_admission export_developer_admission_fixture --locked')
                    self.assertEqual(sum('export_developer_admission_fixture' in line for line in lines), 1)
                    if status == '0':
                        self.assertEqual((root / C.WORKER / 'artifacts/approval-fixture.json').read_text(),
                                         'exported-by-test-double')

    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name) / 'producer'
        self.worker = self.root / C.WORKER
        (self.worker / 'tests').mkdir(parents=True)
        (self.root / 'target').mkdir()
        (self.root / 'rust-toolchain.toml').write_text('[toolchain]\nchannel="1.96.0"\n')
        for index in range(8):
            (self.worker / 'tests' / f'{index}.test.mjs').write_text(
                "import {test} from 'node:test';\nimport assert from 'node:assert/strict';\n"
                "test('first',()=>assert.equal(2+2,4));\ntest('second',()=>assert.equal(3+3,6));\n")
        self.plan = dict(source_sha='a' * 40, tested_sha='b' * 40, policy_sha='c' * 40,
            scope='targeted', groups=['baseline', 'workspace_support'], workspace_support_profile_version=1,
            workspace_support_profiles=['oasis7_custody_worker'], workspace_support_partition_version=1,
            workspace_support_partitions=['custody'], custody_execution_version=1, custody_node_shards=C.SHARDS)
        self.artifacts = self.worker / 'artifacts'
        self.artifacts.mkdir()
        for name in ('native', 'world'):
            binary = self.root / 'target' / name
            binary.write_text('#!/bin/sh\necho actual-fixture\n')
            binary.chmod(0o755)
            (self.artifacts / f'{name}-test-binary.txt').write_text(str(binary) + '\n')
        for component in C.COMPONENTS:
            for name in C.minimum_files(component) - {'bin/native', 'bin/world'}:
                path = self.artifacts / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text('{}\n')
        self.bundles = {component: Path(self.temporary.name) / component for component in C.COMPONENTS}

    def packed(self):
        for component in C.COMPONENTS:
            C.pack(self.root, self.plan, component, self.bundles[component], '71', 2)
        return self.bundles

    def test_old_capability_falls_back_and_incomplete_new_plan_fails(self):
        legacy = dict(self.plan)
        del legacy['custody_execution_version']
        self.assertFalse(C.enabled(legacy))
        self.assertTrue(C.enabled(dict(self.plan, custody_node_shards=[0, 1, 2])))
        self.assertTrue(C.enabled(dict(self.plan, custody_node_shards=[0, 1, 2, 3])))
        for key, value in [('custody_execution_version', True), ('custody_execution_version', 2),
                           ('custody_node_shards', [0, 1]), ('custody_node_shards', [False, 1, 2, 3]),
                           ('custody_node_shards', [0, 1, 2, 2]), ('workspace_support_profiles', []),
                           ('workspace_support_partitions', ['other']), ('groups', ['baseline'])]:
            with self.subTest(key=key, value=value), self.assertRaises(ValueError):
                C.enabled(dict(self.plan, **{key: value}))

    def proof_plan(self):
        return dict(self.plan, custody_compile_proof_version=1, test_policy_changed=False)

    def policy_plan(self):
        return dict(self.proof_plan(), fast_plan_version=1, execution_profile='pr-fast',
                    test_policy_changed=True, custody_policy_tests_version=1)

    def test_policy_tests_require_complete_trusted_capabilities(self):
        plan = self.policy_plan()
        self.assertTrue(C.enabled(plan))
        self.assertTrue(C.separate_proof(plan))
        for key in ('custody_policy_tests_version', 'custody_compile_proof_version',
                    'custody_execution_version', 'fast_plan_version',
                    'workspace_support_profile_version', 'workspace_support_partition_version'):
            for value in (None, False, True, 0, 2, 1.0, '1', 'missing'):
                changed = plan | {key: value}
                if value == 'missing':
                    del changed[key]
                with self.subTest(key=key, value=value), self.assertRaises(ValueError):
                    C.enabled(changed)
        for key, value in [('test_policy_changed', False), ('test_policy_changed', 1),
                           ('execution_profile', 'periodic-full'), ('execution_profile', None)]:
            with self.subTest(key=key, value=value), self.assertRaises(ValueError):
                C.enabled(plan | {key: value})

    @unittest.skipUnless(shutil.which('cargo') and shutil.which('rustc'), 'requires a Rust toolchain')
    def test_real_policy_tail_runs_full_features_and_new_targets_and_propagates_failure(self):
        (self.root / '.gitignore').write_text('target/\n')
        (self.root / 'Cargo.toml').write_text('[workspace]\nresolver="2"\nmembers=["crates/oasis7_custody_worker"]\n')
        (self.worker / 'src').mkdir()
        manifest = self.worker / 'Cargo.toml'
        manifest.write_text('[package]\nname="oasis7_custody_worker"\nversion="0.1.0"\nedition="2021"\n'
                            '[features]\ntest_tier_required=[]\ntest_tier_full=[]\n')
        (self.worker / 'src/lib.rs').write_text('#[test] fn original_library_case() {}\n')
        case = self.worker / 'tests/policy_case.rs'
        case.write_text('#[cfg(feature="test_tier_required")] #[test] fn required_case() {}\n'
                        '#[cfg(feature="test_tier_full")] #[test] fn full_case() { '
                        'assert!(std::env::var_os("FAIL_POLICY_CASE").is_none()); }\n')
        def git(*arguments):
            return subprocess.check_output(['git', '-C', str(self.root), *arguments], text=True).strip()
        git('init', '-q')
        git('add', '.')
        git('-c', 'user.name=CI fixture', '-c', 'user.email=fixture@example.invalid',
            'commit', '-qm', 'real policy tail fixture')
        plan = self.policy_plan() | {'tested_sha': git('rev-parse', 'HEAD')}
        selection = Path(self.temporary.name) / 'policy-selection.json'
        selection.write_text(json.dumps(plan))
        environment = dict(os.environ, GITHUB_RUN_ID='71', GITHUB_RUN_ATTEMPT='2',
            RUSTC_WRAPPER='/does-not-exist-policy-cargo-wrapper',
            CUSTODY_SELECTION_SHA256=hashlib.sha256(selection.read_bytes()).hexdigest(),
            **{'OASIS7_SUPPORT_' + key.upper(): plan[key] for key in ('source_sha', 'tested_sha', 'policy_sha')})
        def invoke(extra=None):
            return subprocess.run([sys.executable, str(Path(C.__file__)), 'policy-tests',
                '--repo-root', str(self.root), '--plan', str(selection)],
                env=environment | (extra or {}), text=True, capture_output=True)
        result = invoke()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn('test required_case ... ok', result.stdout)
        self.assertIn('test full_case ... ok', result.stdout)
        self.assertEqual(result.stdout.count('test original_library_case ... ok'), 1)
        failed = invoke({'FAIL_POLICY_CASE': '1'})
        self.assertNotEqual(failed.returncode, 0)
        self.assertIn('test full_case ... FAILED', failed.stdout)
        # A new target must run even when this package owns no tier features.
        manifest.write_text(manifest.read_text().split('[features]')[0])
        case.write_text('#[test] fn new_no_feature_target() {}\n')
        git('add', '.')
        git('-c', 'user.name=CI fixture', '-c', 'user.email=fixture@example.invalid',
            'commit', '-qm', 'new no-feature target')
        plan['tested_sha'] = git('rev-parse', 'HEAD')
        selection.write_text(json.dumps(plan))
        environment.update(CUSTODY_SELECTION_SHA256=hashlib.sha256(selection.read_bytes()).hexdigest(),
                           OASIS7_SUPPORT_TESTED_SHA=plan['tested_sha'])
        result = invoke()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn('test new_no_feature_target ... ok', result.stdout)
        for changed in ({'CUSTODY_SELECTION_SHA256': '0' * 64},
                        {'OASIS7_SUPPORT_POLICY_SHA': 'd' * 40},
                        {'GITHUB_RUN_ID': '0'}, {'GITHUB_RUN_ATTEMPT': '0'}):
            failed = invoke(changed)
            self.assertNotEqual(failed.returncode, 0)
            self.assertNotIn('test new_no_feature_target', failed.stdout)

    def test_policy_stages_preserve_the_complete_legacy_command_multiset(self):
        # This records the actual verifier/driver dispatch; it does not claim
        # real product WASM compilation. The expansion and Node execution have
        # separate real Cargo/Node fixtures above and below.
        source = Path(__file__).resolve().parents[1]
        scripts = self.worker / 'scripts'
        scripts.mkdir()
        shutil.copyfile(source / C.WORKER / 'scripts/verify.sh', scripts / 'verify.sh')
        (self.root / 'scripts').mkdir()
        (self.root / 'scripts/ci-worker-build.py').write_text(
            'import os\nprint(os.environ["WORKER_TOOL"])\n')
        binary = self.root / 'bin'
        binary.mkdir()
        recorder = binary / 'record'
        recorder.write_text('''#!/usr/bin/env python3
import json, os, sys
from pathlib import Path
name = Path(sys.argv[0]).name
a = sys.argv[1:]
def record(command):
    with open(os.environ['CAPTURE'], 'a') as f: f.write(json.dumps(command) + '\\n')
if name == 'cargo':
    record([name, *a])
    if a[0] == 'check' and 'oasis7_custody_worker' in a:
        feature = a[a.index('--features') + 1] if '--features' in a else ''
        if not feature or (',' in feature and 'harness' not in feature):
            print('requires exactly one role feature'); sys.exit(1)
        if feature == 'identity,approval-harness':
            print('approval-harness requires owner'); sys.exit(1)
        if 'native-runtime-harness' in feature and not feature.startswith('identity,'):
            print('native-runtime-harness requires identity'); sys.exit(1)
elif name == 'npm':
    if a[0] == 'test':
        record(['typescript'])
        for path in sorted(Path('crates/oasis7_custody_worker/tests').glob('*.test.mjs')):
            record(['node-test', path.name])
    elif a[:2] == ['run', 'test:types']: record(['typescript'])
elif name == 'world':
    if a == ['--list']:
        print('fixture::custody_happy_submit_barrier_preserves_unknown_and_settles_exact_originals: test')
    else:
        record(['world-regression', *a]); print('test result: ok. 1 passed; 0 failed; 0 ignored;')
elif name == 'node': record([name, *a])
else: record([name, *a])
''')
        recorder.chmod(0o755)
        for name in ('cargo', 'npm', 'npx', 'node', 'world'):
            shutil.copyfile(recorder, binary / name)
            (binary / name).chmod(0o755)
        (self.artifacts / 'world-test-binary.txt').write_text(str(binary / 'world') + '\n')
        for name in ('build-role.sh', 'build-approval-harness.sh', 'build-native-runtime-harness.sh',
                     'build-native-fixture.sh', 'build-world-fixture.sh', 'export-context-body-fixture.sh'):
            (scripts / name).write_text('#!/bin/sh\nexec "$RECORD_SCRIPT" "' + name + '" "$@"\n')
        (self.root / 'Cargo.toml').write_text('[workspace]\nmembers=["crates/oasis7_custody_worker"]\n')
        (self.worker / 'Cargo.toml').write_text('[package]\nname="oasis7_custody_worker"\nversion="0.1.0"\n'
            '[features]\ndefault=[]\nidentity=[]\nowner=[]\nexecutor=[]\nrecovery-fence=[]\n'
            'approval-harness=[]\nnative-runtime-harness=[]\ntest_tier_required=[]\ntest_tier_full=[]\n'
            'wasmtime=[]\nviewer_live_integration=[]\n')
        trace = self.root / 'commands.jsonl'
        environment = dict(os.environ, PATH=str(binary) + os.pathsep + os.environ['PATH'],
            CAPTURE=str(trace), WORKER_TOOL=str(binary / 'worker-build'), RECORD_SCRIPT=str(recorder),
            OASIS7_SUPPORT_CAPACITY='0')
        common = 'set -euo pipefail\nci_python="$1"; driver_dir="$2"\nsource "$driver_dir/ci-tests.sh"\n'
        def shell(script, *arguments):
            result = subprocess.run(['bash', '-c', common + script, 'contract', sys.executable,
                str(source / 'scripts'), *arguments], cwd=self.root, env=environment,
                text=True, capture_output=True)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        before = self.policy_plan()
        for field in ('custody_execution_version', 'custody_node_shards',
                      'custody_compile_proof_version', 'custody_policy_tests_version'):
            before.pop(field)
        for verbose in ('0', '1'):
            with self.subTest(CI_VERBOSE=verbose):
                environment['CI_VERBOSE'] = verbose
                trace.unlink(missing_ok=True)
                shell('support_profiles="$3"\nrun_selected_workspace_support\n', 'plan:' + json.dumps(before))
                legacy = Counter(tuple(json.loads(line)) for line in trace.read_text().splitlines())
                trace.unlink()
                for stage in ('native-contracts', 'native-fixtures', 'wasm-production',
                              'wasm-approval-artifacts', 'wasm-contracts'):
                    shell('bash crates/oasis7_custody_worker/scripts/verify.sh --stage "$3"\n', stage)
                shell('run_worker_wasm_compile_profiles\n')
                # Command construction reads the current process environment;
                # use the same verbosity as the legacy driver and child Cargo.
                with mock.patch.dict(os.environ, {'CI_VERBOSE': verbose}):
                    command = C.policy_test_command(self.root, self.policy_plan())
                result = subprocess.run(command, cwd=self.root, env=environment,
                                        text=True, capture_output=True)
                self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                shell('npm run test:types --prefix crates/oasis7_custody_worker\n')
                staged = Counter(tuple(json.loads(line)) for line in trace.read_text().splitlines())
                # File placement is the existing shard protocol, exercised with real Node
                # files by test_real_node_files_complete_all_shards_... .
                for batch in C.partition(C.inventory(self.root)):
                    staged.update(('node-test', filename) for filename in batch)
                # The existing separate WASM proof prepares its own real fixture before
                # checking approval-harness; it cannot reuse another runner's local file.
                legacy.update([('cargo', 'test', '-p', 'oasis7_custody_core', '--test',
                                'approval_admission', 'export_developer_admission_fixture', '--locked')])
                self.assertEqual(staged, legacy,
                                 f'extra={staged - legacy}; missing={legacy - staged}')

    def test_compile_proof_requires_strict_capability_and_complete_parent_plan(self):
        self.assertFalse(C.separate_proof(self.plan))
        plan = self.proof_plan()
        self.assertTrue(C.separate_proof(plan))
        for key in ('custody_compile_proof_version', 'custody_execution_version',
                    'workspace_support_partition_version', 'workspace_support_profile_version'):
            for value in (None, False, True, 0, 2, '1'):
                with self.subTest(key=key, value=value), self.assertRaises(ValueError):
                    C.enabled(dict(plan, **{key: value}))
        for key in ('custody_execution_version', 'workspace_support_partition_version',
                    'test_policy_changed'):
            changed = dict(plan)
            del changed[key]
            with self.subTest(missing=key), self.assertRaises(ValueError):
                C.enabled(changed)
        for key, value in [('test_policy_changed', True), ('test_policy_changed', 0),
                           ('groups', []), ('workspace_support_profiles', []),
                           ('workspace_support_partitions', ['other']), ('custody_node_shards', [0, 1])]:
            with self.subTest(key=key, value=value), self.assertRaises(ValueError):
                C.enabled(dict(plan, **{key: value}))

    def test_build_stage_cli_binds_selected_file_checkout_and_independent_run_identity(self):
        def git(*arguments):
            return subprocess.check_output(['git', '-C', str(self.root), *arguments], text=True).strip()
        git('init', '-q')
        git('add', '.')
        git('-c', 'user.name=CI fixture', '-c', 'user.email=fixture@example.invalid',
            'commit', '-qm', 'custody stage fixture')
        plan = self.proof_plan() | {'tested_sha': git('rev-parse', 'HEAD')}
        path = Path(self.temporary.name) / 'selection.json'
        path.write_text(json.dumps(plan))
        environment = dict(os.environ, GITHUB_RUN_ID='71', GITHUB_RUN_ATTEMPT='2',
            CUSTODY_SELECTION_SHA256=hashlib.sha256(path.read_bytes()).hexdigest(),
            **{'OASIS7_SUPPORT_' + key.upper(): plan[key] for key in ('source_sha', 'tested_sha', 'policy_sha')})
        def stage(component, extra_environment=None):
            return subprocess.run([sys.executable, str(Path(C.__file__)), 'stage', '--repo-root', str(self.root),
                '--plan', str(path), '--component', component], env=environment | (extra_environment or {}),
                text=True, capture_output=True)
        for component, expected in [('native', 'native-fixtures'), ('wasm-production', 'wasm-production'),
                                    ('wasm-approval', 'wasm-approval-artifacts'), ('wasm-contracts', 'wasm-contracts')]:
            result = stage(component)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(result.stdout.strip(), expected)
        for field, value in [('CUSTODY_SELECTION_SHA256', '0' * 64), ('GITHUB_RUN_ID', '0'),
                             ('GITHUB_RUN_ATTEMPT', '0'), ('OASIS7_SUPPORT_SOURCE_SHA', 'd' * 40),
                             ('OASIS7_SUPPORT_TESTED_SHA', 'd' * 40), ('OASIS7_SUPPORT_POLICY_SHA', 'd' * 40)]:
            with self.subTest(field=field):
                result = stage('wasm-contracts', {field: value})
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(result.stdout, '')
        path.write_text(json.dumps(plan | {'tested_sha': 'd' * 40}))
        result = stage('wasm-contracts', {'CUSTODY_SELECTION_SHA256': hashlib.sha256(path.read_bytes()).hexdigest(),
                                         'OASIS7_SUPPORT_TESTED_SHA': 'd' * 40})
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('checkout is not the selected tested commit', result.stderr)
        del plan['custody_compile_proof_version']
        path.write_text(json.dumps(plan))
        environment['CUSTODY_SELECTION_SHA256'] = hashlib.sha256(path.read_bytes()).hexdigest()
        self.assertEqual(stage('wasm-approval').stdout.strip(), 'wasm-approval')
        result = stage('wasm-contracts')
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('separate custody proof is not selected', result.stderr)

    def test_all_files_are_assigned_once_including_new_files(self):
        first = C.partition(C.inventory(self.root))
        self.assertEqual(sorted(sum(first, [])), [f'{i}.test.mjs' for i in range(8)])
        (self.worker / 'tests/new.test.mjs').write_text("import {test} from 'node:test';test('new',()=>{});\n")
        second = C.partition(C.inventory(self.root))
        self.assertEqual(sorted(sum(second, [])), sorted([f'{i}.test.mjs' for i in range(8)] + ['new.test.mjs']))
        self.assertEqual(second, C.partition(list(reversed(C.inventory(self.root)))))

    def test_complete_outputs_restore_to_new_paths_and_real_executables_run(self):
        self.packed()
        consumer = Path(self.temporary.name) / 'consumer'
        shutil.copytree(self.root, consumer, ignore=shutil.ignore_patterns('artifacts', 'target'))
        C.restore(consumer, self.plan, self.bundles, '71', dict.fromkeys(C.COMPONENTS, 2), 3)
        for name in ('native', 'world'):
            pointer = consumer / C.WORKER / 'artifacts' / f'{name}-test-binary.txt'
            binary = Path(pointer.read_text().strip())
            self.assertTrue(binary.is_relative_to(consumer))
            self.assertEqual(subprocess.check_output([str(binary)], text=True).strip(), 'actual-fixture')
        with self.assertRaisesRegex(ValueError, 'empty custody artifacts'):
            C.restore(consumer, self.plan, self.bundles, '71', dict.fromkeys(C.COMPONENTS, 2), 3)

    def test_hidden_build_files_survive_transport_and_missing_payload_fails_before_restore(self):
        hidden = {'executor/.gitignore': b'generated build output\n',
                  'approval-harness/.metadata/provenance.json': b'{"fixture":true}\n'}
        for name, content in hidden.items():
            path = self.artifacts / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(content)
        # The upload root is the freshly packed bundle, never the checkout.
        (self.root / '.env').write_text('outside-the-generated-bundle\n')
        self.packed()
        transported = {}
        for component, bundle in self.bundles.items():
            archive = Path(self.temporary.name) / f'{component}.zip'
            with zipfile.ZipFile(archive, 'w') as writer:
                for path in bundle.rglob('*'):
                    if path.is_file():
                        writer.write(path, path.relative_to(bundle).as_posix())
            transported[component] = Path(self.temporary.name) / f'download-{component}'
            with zipfile.ZipFile(archive) as reader:
                reader.extractall(transported[component])
            C.check_bundle(self.root, self.plan, component, transported[component], '71', 2)
            self.assertNotIn('.env', [row['path'] for row in C.load(bundle / 'manifest.json')['files']])
        missing = transported['wasm-production'] / 'files/executor/.gitignore'
        missing.unlink()
        consumer = Path(self.temporary.name) / 'consumer'
        shutil.copytree(self.root, consumer, ignore=shutil.ignore_patterns('artifacts', 'target'))
        with self.assertRaises(FileNotFoundError):
            C.restore(consumer, self.plan, transported, '71', dict.fromkeys(C.COMPONENTS, 2), 3)
        self.assertFalse((consumer / C.WORKER / 'artifacts').exists())
        missing.write_bytes(hidden['executor/.gitignore'])
        C.restore(consumer, self.plan, transported, '71', dict.fromkeys(C.COMPONENTS, 2), 3)
        for name, content in hidden.items():
            restored = consumer / C.WORKER / 'artifacts' / name
            self.assertEqual(restored.read_bytes(), content)
            self.assertEqual(restored.stat().st_mode & 0o777, 0o644)

    def test_source_policy_run_attempt_test_inventory_and_build_contract_are_bound(self):
        self.packed()
        for key in ('source_sha', 'tested_sha', 'policy_sha'):
            with self.subTest(key=key), self.assertRaises(ValueError):
                C.check_bundle(self.root, dict(self.plan, **{key: 'd' * 40}), 'native', self.bundles['native'], '71', 2)
        for run_id, attempt in [('72', 2), ('71', 1), ('71', True)]:
            with self.assertRaises(ValueError):
                C.check_bundle(self.root, self.plan, 'native', self.bundles['native'], run_id, attempt)
        (self.worker / 'tests/0.test.mjs').write_text('changed')
        with self.assertRaises(ValueError):
            C.check_bundle(self.root, self.plan, 'native', self.bundles['native'], '71', 2)

    def test_compile_proof_ownership_is_bound_to_approval_before_restore(self):
        self.assertEqual(C.BUILD_CONTRACTS['wasm-production'], list(C.ROLES))
        self.assertEqual(C.BUILD_CONTRACTS['wasm-approval'], [
            'owner,approval-harness', 'approval-admission:export',
            'role-feature-negative-checks', 'worker-wasm-six-profiles'])
        self.packed()
        consumer = Path(self.temporary.name) / 'consumer'
        shutil.copytree(self.root, consumer, ignore=shutil.ignore_patterns('artifacts', 'target'))
        for component in ('wasm-production', 'wasm-approval'):
            path = self.bundles[component] / 'manifest.json'
            original = C.load(path)
            changed = copy.deepcopy(original)
            if component == 'wasm-production':
                changed['build_contract'].append('role-feature-negative-checks')
            else:
                changed['build_contract'].remove('role-feature-negative-checks')
            path.write_text(json.dumps(changed))
            with self.subTest(component=component), self.assertRaisesRegex(ValueError, 'identity mismatch'):
                C.restore(consumer, self.plan, self.bundles, '71', dict.fromkeys(C.COMPONENTS, 2), 3)
            self.assertFalse((consumer / C.WORKER / 'artifacts').exists())
            path.write_text(json.dumps(original))
        C.restore(consumer, self.plan, self.bundles, '71', dict.fromkeys(C.COMPONENTS, 2), 3)

    def test_new_approval_claims_only_its_artifacts_and_cross_protocol_restore_is_rejected(self):
        legacy = dict(self.plan)
        self.plan = self.proof_plan()
        self.packed()
        self.assertEqual(C.load(self.bundles['wasm-approval'] / 'manifest.json')['build_contract'],
                         ['owner,approval-harness', 'approval-admission:export'])
        consumer = Path(self.temporary.name) / 'consumer'
        shutil.copytree(self.root, consumer, ignore=shutil.ignore_patterns('artifacts', 'target'))
        with self.assertRaisesRegex(ValueError, 'identity mismatch'):
            C.restore(consumer, legacy, self.bundles, '71', dict.fromkeys(C.COMPONENTS, 2), 3)
        self.assertFalse((consumer / C.WORKER / 'artifacts').exists())
        path = self.bundles['wasm-approval'] / 'manifest.json'
        original = C.load(path)
        path.write_text(json.dumps(original | {'build_contract': C.BUILD_CONTRACTS['wasm-approval']}))
        with self.assertRaisesRegex(ValueError, 'identity mismatch'):
            C.restore(consumer, self.plan, self.bundles, '71', dict.fromkeys(C.COMPONENTS, 2), 3)
        self.assertFalse((consumer / C.WORKER / 'artifacts').exists())
        path.write_text(json.dumps(original))
        C.restore(consumer, self.plan, self.bundles, '71', dict.fromkeys(C.COMPONENTS, 2), 3)

    def test_bad_payloads_duplicate_paths_modes_and_traversal_are_rejected(self):
        self.packed()
        path = self.bundles['native'] / 'manifest.json'
        original = C.load(path)
        mutations = [lambda value: value['files'].append(value['files'][0]),
                     lambda value: value['files'].pop(),
                     lambda value: value['files'][0].update(path='../outside'),
                     lambda value: value['files'][0].update(mode=0o777),
                     lambda value: value['files'][0].update(sha256='0' * 64),
                     lambda value: value.update(build_contract=[])]
        for mutate in mutations:
            changed = copy.deepcopy(original)
            mutate(changed)
            path.write_text(json.dumps(changed))
            with self.assertRaises((ValueError, FileNotFoundError)):
                C.check_bundle(self.root, self.plan, 'native', self.bundles['native'], '71', 2)
        path.write_text(json.dumps(original))
        extra = self.bundles['native'] / 'extra'
        extra.write_text('unlisted')
        with self.assertRaises(ValueError):
            C.check_bundle(self.root, self.plan, 'native', self.bundles['native'], '71', 2)
        extra.unlink()
        target = self.bundles['native'] / 'files/bin/native'
        target.unlink()
        target.symlink_to(self.root / 'target/native')
        with self.assertRaises(ValueError):
            C.check_bundle(self.root, self.plan, 'native', self.bundles['native'], '71', 2)

    def test_missing_producer_future_attempt_and_external_binary_fail_before_restore(self):
        self.packed()
        consumer = Path(self.temporary.name) / 'consumer'
        shutil.copytree(self.root, consumer, ignore=shutil.ignore_patterns('artifacts', 'target'))
        for bundles, attempts, current in [(dict(native=self.bundles['native']), dict.fromkeys(C.COMPONENTS, 2), 2),
                                           (self.bundles, dict.fromkeys(C.COMPONENTS, 2), 1)]:
            with self.assertRaises(ValueError):
                C.restore(consumer, self.plan, bundles, '71', attempts, current)
            self.assertFalse((consumer / C.WORKER / 'artifacts').exists())
        (self.root / 'outside').write_text('not a target fixture')
        (self.artifacts / 'native-test-binary.txt').write_text(str(self.root / 'target/../outside') + '\n')
        with self.assertRaises(ValueError):
            C.pack(self.root, self.plan, 'native', Path(self.temporary.name) / 'traversal', '71', 2)
        (self.artifacts / 'native-test-binary.txt').write_text('/bin/true\n')
        with self.assertRaises(ValueError):
            C.pack(self.root, self.plan, 'native', Path(self.temporary.name) / 'unsafe', '71', 2)

    def test_node_summary_preserves_only_existing_opt_in_skip(self):
        summary = '# tests 2\n# pass 2\n# fail 0\n# cancelled 0\n# skipped 0\n# todo 0\n'
        self.assertEqual(C.tap_summary(summary, 'real.test.mjs')['pass'], 2)
        for bad in [summary.replace('# tests 2', '# tests 0'), summary.replace('# fail 0', '# fail 1'),
                    summary.replace('# cancelled 0', '# cancelled 1'), summary.replace('# todo 0', '# todo 1'),
                    summary.replace('# pass 2', '# pass 1').replace('# skipped 0', '# skipped 1'),
                    summary + '# tests 2\n', '']:
            with self.assertRaises(ValueError):
                C.tap_summary(bad, 'real.test.mjs')
        skip = '# tests 1\n# pass 0\n# fail 0\n# cancelled 0\n# skipped 1\n# todo 0\n'
        self.assertEqual(C.tap_summary(skip, 'host-control-https.integration.test.mjs')['skipped'], 1)

    def test_real_node_files_complete_all_shards_and_missing_or_duplicate_receipts_fail(self):
        for index, plan in enumerate((self.plan, self.policy_plan())):
            receipts = []
            for shard in C.SHARDS:
                output = Path(self.temporary.name) / f'results-{index}-{shard}'
                C.execute(self.root, plan, shard, output, '71', 2)
                receipts.append(output / 'result.json')
            C.validate_results(self.root, plan, receipts, '71', 3)
            for paths in [receipts[:-1], receipts + receipts[:1]]:
                with self.assertRaises(ValueError):
                    C.validate_results(self.root, plan, paths, '71', 3)
            value = C.load(receipts[0])
            value['files'][0]['process']['timeout'] = True
            receipts[0].write_text(json.dumps(value))
            with self.assertRaises(ValueError):
                C.validate_results(self.root, plan, receipts, '71', 3)

    def test_real_node_failure_and_zero_tests_cannot_publish_success(self):
        for version, plan in enumerate((self.plan, self.policy_plan())):
            for index, source in enumerate(["import {test} from 'node:test';test('bad',()=>{throw Error('real failure')});", '']):
                path = self.worker / 'tests/0.test.mjs'
                path.write_text(source)
                shard = next(i for i, batch in enumerate(C.partition(C.inventory(self.root))) if path.name in batch)
                output = Path(self.temporary.name) / f'failure-{version}-{index}'
                with self.assertRaises(ValueError):
                    C.execute(self.root, plan, shard, output, '71', 2)
                self.assertFalse((output / 'result.json').exists())

    def client_fixture(self):
        path = self.worker / 'tests' / C.workerd_capture.TEST_FILE
        (self.worker / 'tests/0.test.mjs').rename(path)
        shard = next(i for i, batch in enumerate(C.partition(C.inventory(self.root))) if path.name in batch)
        return path, shard

    def test_workerd_capture_is_explicit_and_only_wraps_the_original_client_command(self):
        unused, shard = self.client_fixture()
        output = Path(self.temporary.name) / 'capture-default-off'
        with mock.patch.object(C.workerd_capture, 'prepare', side_effect=AssertionError('unexpected debugger')):
            C.execute(self.root, self.plan, shard, output, '71', 2)
        output = Path(self.temporary.name) / 'capture-enabled'
        def prepare(root, results, environment, context, attempt, command):
            self.assertEqual(command, ['node', '--test', '--test-concurrency=1', '--test-reporter=tap',
                                       'tests/' + C.workerd_capture.TEST_FILE])
            self.assertEqual(attempt, 2)
            self.assertEqual(context, C.identity(self.root, self.plan, '71'))
            return results / 'workerd-fault', command, environment
        with mock.patch.object(C.workerd_capture, 'prepare', side_effect=prepare) as observer, \
             mock.patch.object(C.workerd_capture, 'finish', return_value={'passed': True}) as finish:
            C.execute(self.root, self.plan, shard, output, '71', 2, True)
        observer.assert_called_once()
        self.assertTrue(finish.call_args.args[-1])
        self.assertEqual([row['file'] for row in C.load(output / 'result.json')['files']],
                         C.partition(C.inventory(self.root))[shard])

    def test_capture_failure_cannot_publish_success_or_replace_original_node_failure(self):
        path, shard = self.client_fixture()
        original = path.read_text()
        for node_fails in (False, True):
            for write_fails in (False, True):
                path.write_text("import {test} from 'node:test';test('original',()=>{throw Error('first failure')});"
                                if node_fails else original)
                output = Path(self.temporary.name) / f'capture-failure-{node_fails}-{write_fails}'
                def prepare(root, results, environment, context, attempt, command):
                    directory = results / 'workerd-fault'
                    directory.mkdir()
                    (directory / 'records.jsonl').write_text('null\n')
                    return directory, command, environment
                with mock.patch.object(C.workerd_capture, 'prepare', side_effect=prepare):
                    with mock.patch.object(C.workerd_capture, 'finish', side_effect=OSError('unwritable')) \
                            if write_fails else mock.patch.object(C.workerd_capture, 'finish', wraps=C.workerd_capture.finish):
                        expected = 'custody Node file failed' if node_fails else 'workerd capture failed'
                        with self.assertRaisesRegex(ValueError, expected):
                            C.execute(self.root, self.plan, shard, output, '71', 2, True)
                self.assertFalse((output / 'result.json').exists())

    def test_known_workerd_fault_retries_once_and_preserves_attempt_one_evidence(self):
        path, shard = self.client_fixture()
        passing_source = path.read_text()
        failing_source = "import {test} from 'node:test';test('fault',()=>{throw Error('fetch failed')});"
        output = Path(self.temporary.name) / 'known-fault-retry-pass'
        calls = []

        def prepare(root, results, environment, context, attempt, command):
            call_index = len(calls) + 1
            calls.append(call_index)
            path.write_text(failing_source if call_index == 1 else passing_source)
            directory = results / 'workerd-fault'
            directory.mkdir()
            (directory / 'records.jsonl').write_text(f'attempt-{call_index}\n')
            return directory, command, environment

        def finish(directory, context, attempt, node_passed):
            report = dict(
                schema=C.workerd_capture.SCHEMA,
                engine_sha256=C.workerd_capture.ENGINE_SHA256,
                complete=True,
                passed=bool(node_passed),
                summary=dict(known_fault=not node_passed, signal_count=0 if node_passed else 1,
                             runtime_count=1, passed=bool(node_passed)),
            )
            (directory / 'report.json').write_text(json.dumps(report) + '\n')
            return report

        with mock.patch.object(C.workerd_capture, 'prepare', side_effect=prepare), \
             mock.patch.object(C.workerd_capture, 'finish', side_effect=finish):
            C.execute(self.root, self.plan, shard, output, '71', 2, True)

        self.assertEqual(calls, [1, 2])
        self.assertEqual((output / 'workerd-fault-attempt-1/records.jsonl').read_text(), 'attempt-1\n')
        self.assertEqual((output / 'workerd-fault/records.jsonl').read_text(), 'attempt-2\n')
        self.assertTrue((output / (C.workerd_capture.TEST_FILE + '.attempt-1.log')).exists())
        receipt = C.load(output / 'result.json')
        row = next(item for item in receipt['files'] if item['file'] == C.workerd_capture.TEST_FILE)
        self.assertEqual(row['known_fault_retry'], dict(
            attempts=2,
            engine_sha256=C.workerd_capture.ENGINE_SHA256,
            fault_offset=C.workerd_capture.KNOWN_FAULT_OFFSET,
            issue='#5900',
            upstream_issue=C.workerd_capture.UPSTREAM_ISSUE_URL,
            preserved_evidence='workerd-fault-attempt-1',
            preserved_log=C.workerd_capture.TEST_FILE + '.attempt-1.log',
        ))

    def test_known_workerd_fault_retry_is_bounded_and_never_retries_other_failures(self):
        path, shard = self.client_fixture()
        path.write_text("import {test} from 'node:test';test('fault',()=>{throw Error('boom')});")

        for known_fault, expected_calls in ((True, 2), (False, 1)):
            output = Path(self.temporary.name) / f'known-fault-bounded-{known_fault}'
            calls = []

            def prepare(root, results, environment, context, attempt, command):
                calls.append(len(calls) + 1)
                directory = results / 'workerd-fault'
                directory.mkdir()
                (directory / 'records.jsonl').write_text(f'attempt-{len(calls)}\n')
                return directory, command, environment

            def finish(directory, context, attempt, node_passed):
                report = dict(
                    schema=C.workerd_capture.SCHEMA,
                    engine_sha256=C.workerd_capture.ENGINE_SHA256,
                    complete=True,
                    passed=False,
                    summary=dict(known_fault=known_fault, signal_count=1 if known_fault else 0,
                                 runtime_count=1, passed=False),
                )
                (directory / 'report.json').write_text(json.dumps(report) + '\n')
                return report

            with mock.patch.object(C.workerd_capture, 'prepare', side_effect=prepare), \
                 mock.patch.object(C.workerd_capture, 'finish', side_effect=finish), \
                 self.assertRaisesRegex(ValueError, 'custody Node file failed'):
                C.execute(self.root, self.plan, shard, output, '71', 2, True)

            self.assertEqual(len(calls), expected_calls)
            self.assertFalse((output / 'result.json').exists())
            self.assertEqual((output / 'workerd-fault-attempt-1').exists(), known_fault)

    def test_final_consumer_requires_capture_and_binds_the_producer_attempt(self):
        self.client_fixture()
        receipts = []
        for shard in C.SHARDS:
            output = Path(self.temporary.name) / f'capture-consumer-{shard}'
            C.execute(self.root, self.plan, shard, output, '71', 2)
            receipts.append(output / 'result.json')
        with self.assertRaises(FileNotFoundError):
            C.validate_results(self.root, self.plan, receipts, '71', 3, True)
        with mock.patch.object(C.workerd_capture, 'validate') as validate:
            C.validate_results(self.root, self.plan, receipts, '71', 3, True)
        validate.assert_called_once()
        self.assertEqual(validate.call_args.args[2], 2)

    def test_job_summary_requires_every_selected_success_and_unselected_skip(self):
        needs = {'select': dict(result='success'), 'workspace-support-legacy': dict(result='skipped'),
                 'workspace-support-other': dict(result='skipped'), 'custody-node': dict(result='success'),
                 'custody-native-contracts': dict(result='success'),
                 **{f'custody-{part}': dict(result='success') for part in C.COMPONENTS}}
        self.assertTrue(C.validate_jobs(self.plan, needs))
        for job in needs:
            for state in ('failure', 'cancelled', 'skipped', None):
                if state == 'skipped' and needs[job]['result'] == 'skipped':
                    continue
                changed = copy.deepcopy(needs)
                changed[job]['result'] = state
                with self.subTest(job=job, state=state), self.assertRaises(ValueError):
                    C.validate_jobs(self.plan, changed)
        consolidated_wasm = {k: v for k, v in needs.items() if k != 'custody-wasm-approval'}
        self.assertTrue(C.validate_jobs(self.plan, consolidated_wasm))
        for bad in ('failure', 'cancelled', 'skipped', None):
            failed_wasm = copy.deepcopy(consolidated_wasm)
            failed_wasm['custody-wasm-production'] = dict(result=bad)
            with self.subTest(consolidated_wasm=bad), self.assertRaises(ValueError):
                C.validate_jobs(self.plan, failed_wasm)
        legacy = dict(self.plan)
        del legacy['custody_execution_version']
        for job in needs:
            needs[job]['result'] = 'success' if job in ('select', 'workspace-support-legacy') else 'skipped'
        self.assertFalse(C.validate_jobs(legacy, needs))

    def test_separate_wasm_proof_is_required_only_under_the_selected_capability(self):
        needs = {'select': dict(result='success'), 'workspace-support-legacy': dict(result='skipped'),
                 'workspace-support-other': dict(result='skipped'), 'custody-node': dict(result='success'),
                 'custody-native-contracts': dict(result='success'), 'custody-wasm-contracts': dict(result='success'),
                 **{f'custody-{part}': dict(result='success') for part in C.COMPONENTS}}
        with self.assertRaisesRegex(ValueError, 'unknown or missing support dependencies'):
            C.validate_jobs(self.plan, needs)
        for plan in (self.proof_plan(), self.policy_plan()):
            self.assertTrue(C.validate_jobs(plan, needs))
            consolidated = {key: value for key, value in needs.items() if key != 'custody-wasm-contracts'}
            self.assertTrue(C.validate_jobs(plan, consolidated))
            for state in ('failure', 'cancelled', 'skipped', None):
                failed = copy.deepcopy(consolidated)
                failed['custody-native-contracts'] = dict(result=state)
                with self.subTest(consolidated_state=state), self.assertRaises(ValueError):
                    C.validate_jobs(plan, failed)
            for job in ('custody-native-contracts', 'custody-wasm-contracts', 'custody-node'):
                for state in ('failure', 'cancelled', 'skipped', None, 'missing'):
                    if job == 'custody-wasm-contracts' and state == 'missing':
                        continue
                    changed = copy.deepcopy(needs)
                    if state == 'missing':
                        del changed[job]
                    else:
                        changed[job]['result'] = state
                    with self.subTest(job=job, state=state), self.assertRaises(ValueError):
                        C.validate_jobs(plan, changed)


if __name__ == '__main__':
    unittest.main()
