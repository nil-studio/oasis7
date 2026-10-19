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
import re
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

    def test_only_capacity_long_overrides_logical_budget(self):
        workflow = (HERE.parent / '.github/workflows/storage-acceptance.yml').read_text()
        global_env = workflow.split('\nenv:\n', 1)[1].split('\njobs:\n', 1)[0]
        variable = 'OASIS7_STORAGE_MAX_LOGICAL_BYTES'
        self.assertIn(f"  {variable}: '4294967296'", global_env)
        jobs = dict(re.findall(
            r'^  ([a-z][a-z0-9-]*):\n(.*?)(?=^  [a-z][a-z0-9-]*:\n|\Z)',
            workflow.split('\njobs:\n', 1)[1], re.MULTILINE | re.DOTALL))
        self.assertIn(f"    env:\n      {variable}: '17179869184'", jobs['capacity-long'])
        self.assertIn("if: inputs.capacity_scale != '10'", jobs['capacity-long'])
        for name, body in jobs.items():
            if name != 'capacity-long':
                self.assertNotIn(variable, body, name)
        self.assertIn('timeout-minutes: 360', jobs['capacity-long'])
        self.assertIn('steps: *acceptance-consumer', jobs['capacity-long'])

    def test_execution_timing_is_enabled_only_for_full_node_consumers(self):
        workflow = (HERE.parent / '.github/workflows/storage-acceptance.yml').read_text()
        variable = 'OASIS7_FORMAL_EXECUTION_TIMING'
        self.assertEqual(workflow.count(variable), 1)
        step = workflow.split('- name: Run acceptance with exact inventory and process cleanup\n', 1)[1].split('      - name:', 1)[0]
        self.assertIn(variable + ": ${{ matrix.group == 'full-node' && '1' || '' }}", step)
        self.assertIn('python3 scripts/ci-storage-acceptance.py run', step)

    def test_storage_build_optimizes_native_tests_without_disabling_checks(self):
        with patch.dict(os.environ, {'RUSTC_WRAPPER': 'untrusted-wrapper',
                                   'CARGO_PROFILE_TEST_DEBUG_ASSERTIONS': 'false'}):
            ordinary = acceptance.artifacts.build_environment('formal')
            storage = acceptance.artifacts.build_environment('storage')
        self.assertNotIn('RUSTC_WRAPPER', storage)
        self.assertEqual(ordinary['CARGO_PROFILE_TEST_DEBUG_ASSERTIONS'], 'false')
        for profile in ('DEV', 'TEST'):
            self.assertEqual(storage[f'CARGO_PROFILE_{profile}_OPT_LEVEL'], '2')
            self.assertEqual(storage[f'CARGO_PROFILE_{profile}_DEBUG_ASSERTIONS'], 'true')
            self.assertEqual(storage[f'CARGO_PROFILE_{profile}_OVERFLOW_CHECKS'], 'true')
        self.assertEqual(storage.get('CARGO_PROFILE_RELEASE_OPT_LEVEL'),
                         ordinary.get('CARGO_PROFILE_RELEASE_OPT_LEVEL'))

    def test_storage_artifact_profile_is_verified_at_consumption(self):
        binary = dict(profile=dict(opt_level='2', debug_assertions=True, overflow_checks=True))
        acceptance.artifacts.validate_profile('runtime-tests', binary, 'storage')
        for key, value in [('opt_level', '0'), ('debug_assertions', False), ('overflow_checks', False)]:
            changed = copy.deepcopy(binary)
            changed['profile'][key] = value
            with self.assertRaises(ValueError):
                acceptance.artifacts.validate_profile('runtime-tests', changed, 'storage')
        # Ordinary formal consumers retain their existing profile contract.
        acceptance.artifacts.validate_profile('runtime-tests', dict(profile={}), 'formal')

    def test_storage_consumer_rejects_unoptimized_manifest(self):
        artifacts = acceptance.artifacts
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            binary = root / 'runtime'
            binary.write_text('#!/bin/sh\nexit 0\n')
            binary.chmod(0o755)
            config = root / 'suites.json'
            config.write_text(json.dumps(dict(suites={}, legacy_ignored_filters={},
                                             environment_formal_batches={}, shell_tests={})))
            entry = dict(path='runtime', sha256=artifacts.digest(binary),
                         original_executable=str(binary), test=False,
                         profile=dict(opt_level='2', debug_assertions=True, overflow_checks=True))
            manifest = dict(schema_version=2, scope='storage', tested_sha='tested', source_sha='source',
                            producer_run_id='run', checkout=str(root),
                            config_sha256=artifacts.digest(config),
                            rust_toolchain=os.environ.get('RUST_TOOLCHAIN', 'local'),
                            runner_image=os.environ.get('ImageOS', sys.platform),
                            binaries={'runtime': entry}, suites={}, libraries={})
            with patch.object(artifacts, 'ROOT', root), patch.object(artifacts, 'CONFIG', config), \
                 patch.object(artifacts, 'output', return_value='tested'), \
                 patch.dict(os.environ, SOURCE_SHA='source', GITHUB_RUN_ID='run'):
                (root / 'manifest.json').write_text(json.dumps(manifest))
                artifacts.load(root)
                entry['profile']['opt_level'] = '0'
                (root / 'manifest.json').write_text(json.dumps(manifest))
                with self.assertRaisesRegex(ValueError, 'optimized storage artifact'):
                    artifacts.load(root)
                for schema in (None, 1, 99):
                    if schema is None:
                        manifest.pop('schema_version', None)
                    else:
                        manifest['schema_version'] = schema
                    (root / 'manifest.json').write_text(json.dumps(manifest))
                    with self.assertRaisesRegex(ValueError, 'schema 2 storage artifact'):
                        artifacts.load(root)

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
            needs['full-node-long'] = {'result': 'skipped'}
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

    def test_attempt_release_requires_exact_nonempty_ignored_inventory(self):
        manifest = self.manifest()
        with self.assertRaises(ValueError):
            acceptance.select(manifest, 'attempt-release')
        name = 'fixture::formal_attempt_release_crash_and_transport_failure_recover_live_remote_pins'
        manifest['binaries']['runtime-tests']['ignored_tests'].append(name)
        self.assertEqual(acceptance.select(manifest, 'attempt-release'), [dict(binary='runtime-tests', name=name, ignored=True)])
        manifest['binaries']['runtime-tests']['ignored_tests'].append(name)
        with self.assertRaises(ValueError):
            acceptance.select(manifest, 'attempt-release')

    def test_full_node_inventory_and_partial_reports_fail_closed(self):
        manifest = self.manifest()
        with self.assertRaises(ValueError):
            acceptance.select(manifest, 'full-node')
        name = 'fixture::formal_full_node_storage_maintenance_preserves_history'
        manifest['binaries']['runtime-tests']['ignored_tests'].append(name)
        self.assertEqual(acceptance.select(manifest, 'full-node'), [dict(binary='runtime-tests', name=name, ignored=True)])
        report = dict(status='passed', executed_commits=1000,
                      fixed_head_unchanged=True, logical_history_unchanged=True,
                      typed_history_unchanged=True, original_result_unchanged=True,
                      original_effect_unchanged=True, continuation_completed=True,
                      fixed_head='a'*64, authority_digest='b'*64, cold_history_evicted=True,
                      published_history_unchanged=True, accounting_quiescent=True,
                      fixed_head_position=1002, original_commit_position=2,
                      growth_before_maintenance_allocated_bytes=100,
                      maintenance_net_allocated_change_bytes=-10,
                      cold_restore_net_allocated_change_bytes=0)
        for field in ('initial', 'before', 'after_maintenance', 'after_cold_restore'):
            report[field] = dict(complete=True, total=dict(complete=True, allocated_bytes=100))
        report['published_before'] = dict(complete=True, files=1002, bytes=64000, digest='d'*64,
            original_position=2, original_digest='e'*64, fixed_head_position=1002, fixed_head_digest='f'*64)
        report['published_after_maintenance'] = copy.deepcopy(report['published_before'])
        report['published_after_cold_restore'] = copy.deepcopy(report['published_before'])
        report['logical_before'] = {role: dict(retention_records_including_duplicate_digests=1, digest='a'*64)
                                    for role in ('primary', 'replica')}
        report['logical_after'] = copy.deepcopy(report['logical_before'])
        report['typed_history_before'] = dict(records=1000, digest='c'*64)
        report['typed_history_after'] = copy.deepcopy(report['typed_history_before'])
        self.assertTrue(acceptance.full_node_passed(report, 1000))
        self.assertFalse(acceptance.full_node_passed(report, 10000))
        for field in ('fixed_head_unchanged', 'logical_history_unchanged',
                      'typed_history_unchanged', 'original_result_unchanged',
                      'original_effect_unchanged', 'continuation_completed', 'published_history_unchanged', 'accounting_quiescent'):
            changed = copy.deepcopy(report)
            changed[field] = False
            self.assertFalse(acceptance.full_node_passed(changed, 1000))
        for field in ('before', 'after_maintenance', 'after_cold_restore'):
            changed = copy.deepcopy(report)
            changed[field]['complete'] = False
            self.assertFalse(acceptance.full_node_passed(changed, 1000))
        for field in ('published_before', 'published_after_maintenance', 'published_after_cold_restore'):
            changed = copy.deepcopy(report)
            changed[field]['digest'] = 'x'*64
            self.assertFalse(acceptance.full_node_passed(changed, 1000))
        report['status'] = 'partial'
        self.assertFalse(acceptance.full_node_passed(report, 1000))

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
