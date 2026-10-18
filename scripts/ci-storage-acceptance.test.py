#!/usr/bin/env python3
"""Fail-closed contracts for manually dispatched storage acceptance."""
import copy
import importlib.util
import json
from pathlib import Path
import tempfile
import tarfile
import argparse
import os
import sys
from unittest.mock import patch
import unittest

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location('storage_acceptance', HERE / 'ci-storage-acceptance.py')
acceptance = importlib.util.module_from_spec(spec)
spec.loader.exec_module(acceptance)


class Contracts(unittest.TestCase):
    def manifest(self):
        binaries = {name: dict(tests=['one'], ignored_tests=[]) for name in acceptance.LIBRARIES}
        binaries['runtime-tests'] = dict(tests=['runtime::storage_test', 'runtime::other',
            'bench::actual_formal_storage_capacity_smoke_10_heights',
            'bench::actual_formal_storage_capacity_1000_or_10000_heights'],
            ignored_tests=['bench::actual_formal_storage_capacity_1000_or_10000_heights'])
        return dict(binaries=binaries, suites={'runtime-protocol': [
            dict(binary='runtime-tests', name='test::' + name, ignored=True)
            for name in acceptance.NETWORK_SUFFIXES]})

    def report(self, label='units'):
        return dict(label=label, group='units', tested_sha='abc', source_sha='abc',
                    producer_run_id='1', producer_attempt='1', passed=True,
                    expected_cases=1, selected_inventory=[{'binary': 'test'}], results=[dict(
                        passed=True, exit_code=0, cleanup_result='clean', timeout=False, cancelled=False, count_ok=True, leaked_processes=False)])

    def test_storage_scope_adds_libraries_without_changing_formal(self):
        formal = acceptance.artifacts.build_variants('formal')
        storage = acceptance.artifacts.build_variants('storage')
        self.assertEqual(storage[:-1], formal)
        self.assertEqual(storage[-1][0], 'storage-libs')
        for package in ('oasis7_cas', 'oasis7_formal_protocol', 'oasis7_runtime', 'oasis7_world_service'):
            self.assertIn(package, storage[-1][1])

    def test_units_are_nonempty_actual_tests_and_exclude_ignored(self):
        manifest = self.manifest()
        entries = acceptance.select(manifest, 'units')
        self.assertEqual(len(entries), len(acceptance.LIBRARIES) + 1)
        self.assertFalse(any('1000_or_10000' in entry.get('name', '') for entry in entries))
        manifest['binaries'][acceptance.LIBRARIES[0]]['tests'] = []
        with self.assertRaises(ValueError):
            acceptance.select(manifest, 'units')

    def test_capacity_inventory_exact_and_ignored_mode(self):
        manifest = self.manifest()
        self.assertFalse(acceptance.select(manifest, 'capacity', 10)[0]['ignored'])
        self.assertTrue(acceptance.select(manifest, 'capacity', 10000)[0]['ignored'])
        manifest['binaries']['runtime-tests']['tests'] = []
        with self.assertRaises(ValueError):
            acceptance.select(manifest, 'capacity')

    def test_network_missing_or_duplicate_is_not_pass(self):
        manifest = self.manifest()
        self.assertEqual(len(acceptance.select(manifest, 'network')), 3)
        manifest['suites']['runtime-protocol'].pop()
        with self.assertRaises(ValueError):
            acceptance.select(manifest, 'network')

        manifest = self.manifest()
        manifest['suites']['runtime-protocol'].append(manifest['suites']['runtime-protocol'][0])
        with self.assertRaises(ValueError):
            acceptance.select(manifest, 'network')

    def test_disk_requires_the_actual_ignored_enospc_case(self):
        manifest = self.manifest()
        with self.assertRaises(ValueError):
            acceptance.select(manifest, 'disk')
        name = 'physical::physical_real_filesystem_enospc_preserves_published_bytes_and_retry'
        binary = manifest['binaries']['distfs-tests']
        binary['tests'].append(name)
        binary['ignored_tests'].append(name)
        self.assertEqual(acceptance.select(manifest, 'disk'),
                         [dict(binary='distfs-tests', name=name, ignored=True)])
        binary['ignored_tests'].append(name)
        with self.assertRaises(ValueError):
            acceptance.select(manifest, 'disk')

    def test_summary_rejects_missing_duplicate_mixed_identity_and_failed_jobs(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            path = root / 'summary.json'
            path.write_text(json.dumps(self.report()))
            self.assertTrue(acceptance.summarize(root, ['units'])['passed'])
            for expected in ([], ['units', 'capacity'], ['units', 'units']):
                with self.assertRaises(ValueError):
                    acceptance.summarize(root, expected)
            for state in ('failure', 'cancelled', 'skipped', None):
                with self.assertRaises(ValueError):
                    acceptance.summarize(root, ['units'], {'consumer': {'result': state}})
            nested = root / 'nested'
            nested.mkdir()
            second = self.report('network')
            second['producer_run_id'] = '2'
            (nested / 'summary.json').write_text(json.dumps(second))
            with self.assertRaises(ValueError):
                acceptance.summarize(root, ['units', 'network'])

    def test_real_harness_execution_preserves_count_log_and_summary(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            bundle = root / 'bundle'
            bundle.mkdir()
            binary = bundle / 'test'
            binary.write_text('#!' + sys.executable + '\nprint("test result: ok. 1 passed; 0 failed; 0 ignored;")\n')
            binary.chmod(0o755)
            manifest = dict(scope='storage', selection=None, tested_sha='abc', source_sha='abc',
                producer_run_id='1', producer_attempt='1', runner_image='test', rustc_version='test',
                binaries={'test': dict(path='test', cwd='.')})
            args = argparse.Namespace(bundle=bundle, results_dir=root / 'results', group='units',
                capacity_scale=10, repeat_faults=1, label='units', timeout_seconds=5)
            with patch.object(acceptance.artifacts, 'load', return_value=(manifest, dict(os.environ))), \
                 patch.object(acceptance, 'select', return_value=[dict(binary='test', batch=True, count=1)]), \
                 patch.object(acceptance.shutil, 'disk_usage', return_value=argparse.Namespace(total=20*1024**3, used=1*1024**3, free=10 * 1024**3)):
                acceptance.execute(args)
            report = json.loads((args.results_dir / 'summary.json').read_text())
            self.assertTrue(report['passed'])
            self.assertEqual(report['results'][0]['expected_count'], 1)
            self.assertIn('test result: ok.', (args.results_dir / report['results'][0]['log']).read_text())
            self.assertTrue(acceptance.summarize(args.results_dir, ['units'])['passed'])

    def test_optional_job_skip_is_exact(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / 'summary.json').write_text(json.dumps(self.report()))
            needs = {name: {'result': 'success'} for name in ('build', 'verify')}
            needs['capacity-long'] = {'result': 'skipped'}
            self.assertTrue(acceptance.summarize(root, ['units'], needs)['passed'])
            needs['capacity-long']['result'] = 'success'
            with self.assertRaises(ValueError):
                acceptance.summarize(root, ['units'], needs)

    def test_full_bundle_pack_checks_every_file_and_scope(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            bundle = root / 'bundle'
            (bundle / 'bin').mkdir(parents=True)
            binary = bundle / 'bin' / 'test'
            binary.write_bytes(b'original')
            manifest = dict(scope='storage', selection=None, libraries={}, binaries={
                'test': dict(path='bin/test', sha256=acceptance.artifacts.digest(binary))})
            (bundle / 'manifest.json').write_text(json.dumps(manifest))
            with patch.object(acceptance.artifacts, 'load', return_value=(manifest, {})):
                archive = root / 'bundle.tar.gz'
                acceptance.pack(bundle, archive)
                with tarfile.open(archive) as packed:
                    self.assertEqual(set(packed.getnames()), {'manifest.json', 'bin/test'})
                    self.assertEqual(packed.extractfile('bin/test').read(), b'original')
                binary.write_bytes(b'corrupt')
                with self.assertRaises(ValueError):
                    acceptance.pack(bundle, archive)
                manifest['scope'] = 'formal'
                with self.assertRaises(ValueError):
                    acceptance.pack(bundle, archive)

    def test_summary_rejects_partial_nonzero_timeout_cancelled_or_unclean(self):
        for change in ({'exit_code': 1}, {'timeout': True}, {'cancelled': True},
                       {'cleanup_result': 'unresolved'}, {'passed': False}):
            with tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                report = self.report()
                report['results'][0].update(change)
                (root / 'summary.json').write_text(json.dumps(report))
                with self.assertRaises(ValueError):
                    acceptance.summarize(root, ['units'])
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            report = self.report('capacity-10')
            report['group'] = 'capacity'
            (root / 'summary.json').write_text(json.dumps(report))
            with self.assertRaises(ValueError):
                acceptance.summarize(root, ['capacity-10'])


if __name__ == '__main__':
    unittest.main()
