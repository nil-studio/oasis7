#!/usr/bin/env python3
"""Exercise custody file completeness, artifact relocation and failed execution."""
import copy
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
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
                 **{f'custody-{part}': dict(result='success') for part in C.COMPONENTS}}
        self.assertTrue(C.validate_jobs(self.plan, needs))
        for job in needs:
            for state in ('failure', 'cancelled', None):
                changed = copy.deepcopy(needs)
                changed[job]['result'] = state
                with self.subTest(job=job, state=state), self.assertRaises(ValueError):
                    C.validate_jobs(self.plan, changed)
        legacy = dict(self.plan)
        del legacy['custody_execution_version']
        for job in needs:
            needs[job]['result'] = 'success' if job in ('select', 'workspace-support-legacy') else 'skipped'
        self.assertFalse(C.validate_jobs(legacy, needs))


if __name__ == '__main__':
    unittest.main()
