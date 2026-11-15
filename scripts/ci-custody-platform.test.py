#!/usr/bin/env python3
"""Exercise custody file completeness, artifact relocation and failed execution."""
import copy
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
import zipfile

SPEC = importlib.util.spec_from_file_location('custody', Path(__file__).with_name('ci-custody-platform.py'))
C = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(C)


class Custody(unittest.TestCase):
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
        for key, value in [('custody_execution_version', True), ('custody_execution_version', 2),
                           ('custody_node_shards', [0, 1, 2]), ('custody_node_shards', [False, 1, 2, 3]),
                           ('custody_node_shards', [0, 1, 2, 2]), ('workspace_support_profiles', []),
                           ('workspace_support_partitions', ['other']), ('groups', ['baseline'])]:
            with self.subTest(key=key, value=value), self.assertRaises(ValueError):
                C.enabled(dict(self.plan, **{key: value}))

    def proof_plan(self):
        return dict(self.plan, custody_compile_proof_version=1, test_policy_changed=False)

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
                           ('workspace_support_partitions', ['other']), ('custody_node_shards', [0, 1, 2])]:
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
        receipts = []
        for shard in C.SHARDS:
            output = Path(self.temporary.name) / f'results-{shard}'
            C.execute(self.root, self.plan, shard, output, '71', 2)
            receipts.append(output / 'result.json')
        C.validate_results(self.root, self.plan, receipts, '71', 3)
        for paths in [receipts[:-1], receipts + receipts[:1]]:
            with self.assertRaises(ValueError):
                C.validate_results(self.root, self.plan, paths, '71', 3)
        value = C.load(receipts[0])
        value['files'][0]['process']['timeout'] = True
        receipts[0].write_text(json.dumps(value))
        with self.assertRaises(ValueError):
            C.validate_results(self.root, self.plan, receipts, '71', 3)

    def test_real_node_failure_and_zero_tests_cannot_publish_success(self):
        for index, source in enumerate(["import {test} from 'node:test';test('bad',()=>{throw Error('real failure')});", '']):
            path = self.worker / 'tests/0.test.mjs'
            path.write_text(source)
            shard = next(i for i, batch in enumerate(C.partition(C.inventory(self.root))) if path.name in batch)
            output = Path(self.temporary.name) / f'failure-{index}'
            with self.assertRaises(ValueError):
                C.execute(self.root, self.plan, shard, output, '71', 2)
            self.assertFalse((output / 'result.json').exists())

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
        self.assertTrue(C.validate_jobs(self.proof_plan(), needs))
        with self.assertRaisesRegex(ValueError, 'unknown or missing support dependencies'):
            C.validate_jobs(self.plan, needs)
        for state in ('failure', 'cancelled', 'skipped', None, 'missing'):
            changed = copy.deepcopy(needs)
            if state == 'missing':
                del changed['custody-wasm-contracts']
            else:
                changed['custody-wasm-contracts']['result'] = state
            with self.subTest(state=state), self.assertRaises(ValueError):
                C.validate_jobs(self.proof_plan(), changed)


if __name__ == '__main__':
    unittest.main()
