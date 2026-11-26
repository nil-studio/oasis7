#!/usr/bin/env python3
"""NativePackagePartitions and ParallelNativeExecution regressions split from ci-formal-process.test.py."""
import importlib.util
import json
from pathlib import Path
import signal
import sys
import tempfile
import threading
import time
import unittest
from unittest.mock import patch

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))


def module(name):
    spec = importlib.util.spec_from_file_location(name.replace('-', '_'), HERE / (name + '.py'))
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


artifacts = module('ci-test-artifacts')


def fast_target(**changes):
    return dict(package='oasis7_viewer_codec', target_kind='test', target_name='pr_fast',
                default_features=False, features=['test_tier_required'], cases=['required::round_trip']) | changes


def fast_binary(target=None, variant='required-fast-0'):
    target = target or fast_target()
    return dict(ordinary=True, test=True, package=target['package'], target=target['target_name'],
                target_kind=[target['target_kind']], default_features=target['default_features'],
                requested_features=target['features'], features=target['features'],
                tests=target['cases'] + ['new_regression'], ignored_tests=[], variant=variant)


def inline_selection(checks=None):
    packages = ('oasis7_viewer_codec', 'oasis7_runtime')
    return dict(
        fast_plan_version=1, execution_profile='pr-fast', scope='targeted',
        groups=['baseline', 'oasis7_required'],
        oasis7_required_packages=list(packages),
        oasis7_required_wasm_packages=[p for p in packages if p in artifacts.PORTABLE_REQUIRED_PACKAGES],
        formal_suites=[], test_policy_changed=False,
        architecture=dict(source='source', tested='tested'),
        fast_test_inventory_version=1,
        ordinary_fast_targets=[fast_target()],
        fast_execution_version=1, ordinary_execution='inline',
        rust_baseline_checks=[] if checks is None else checks,
    )


class NativePackagePartitions(unittest.TestCase):
    def selection(self):
        targets = [fast_target(package=package, target_kind=kind, target_name=name)
                   for package in ('oasis7_runtime', 'oasis7_world_state')
                   for kind, name in (('lib', package), ('test', 'pr_fast'))]
        return inline_selection() | {
            'ordinary_fast_targets': targets, 'ordinary_partition_version': 1,
            'ordinary_partitions': ['oasis7_runtime', 'oasis7_world_state'],
            'groups': ['baseline', 'rust_baseline', 'oasis7_required'],
            'oasis7_required_packages': ['oasis7_runtime', 'oasis7_world_state'],
            'ordinary_clippy_group': 'rust_baseline', 'oasis7_required_wasm_packages': [],
        }

    def manifest(self, plan, partition):
        manifest = ParallelNativeExecution().manifest(plan)
        manifest['ordinary_partition'] = partition
        manifest['binaries'] = {name: binary for name, binary in manifest['binaries'].items()
                                if binary['package'] == partition}
        return manifest

    def test_package_partitions_cover_the_complete_parent_and_keep_original_variants(self):
        plan = self.selection()
        original = json.dumps(plan, sort_keys=True)
        self.assertEqual(artifacts.ordinary_partitions(plan), ['oasis7_runtime', 'oasis7_world_state'])
        selected = []
        for package, variant in (('oasis7_runtime', 'required-fast-0'),
                                 ('oasis7_world_state', 'required-fast-2')):
            targets = artifacts.ordinary_partition_targets(plan, package)
            self.assertEqual(len(targets), 2)
            self.assertTrue(all(target['package'] == package for target in targets))
            selected.extend(targets)
            commands = artifacts.build_variants('ordinary', plan, ordinary_partition=package)
            self.assertEqual(len(commands), 1)
            self.assertEqual(commands[0][0], variant)
            self.assertEqual(commands[0][1], ['test', '-p', package, '--lib', '--test', 'pr_fast',
                                             '--no-default-features', '--features', 'test_tier_required', '--no-run'])
        self.assertEqual(selected, plan['ordinary_fast_targets'])
        self.assertEqual(len(artifacts.required_native_commands(plan)), 4)
        self.assertEqual(json.dumps(plan, sort_keys=True), original)

    def test_partition_contract_rejects_missing_duplicate_unknown_and_incompatible_owners(self):
        invalid = [('ordinary_partition_version', True), ('ordinary_partition_version', 2),
                   ('ordinary_partitions', []), ('ordinary_partitions', ['all']),
                   ('ordinary_partitions', ['oasis7_runtime']),
                   ('ordinary_partitions', ['oasis7_runtime', 'oasis7_runtime']),
                   ('ordinary_partitions', ['oasis7_runtime', 'unknown']),
                   ('ordinary_clippy_group', 'oasis7_required'),
                   ('oasis7_required_wasm_packages', ['oasis7_world_state']),
                   ('fast_execution_version', 2), ('scope', 'full')]
        for key, value in invalid:
            with self.subTest(key=key, value=value), self.assertRaises(ValueError):
                artifacts.ordinary_partitions(self.selection() | {key: value})
        for key in ('ordinary_partition_version', 'ordinary_partitions', 'fast_plan_version'):
            plan = self.selection()
            del plan[key]
            with self.subTest(missing=key), self.assertRaises(ValueError):
                artifacts.ordinary_partitions(plan)
        for partition in ('all', 'unknown', '', '../oasis7_runtime'):
            with self.subTest(partition=partition), self.assertRaises(ValueError):
                artifacts.ordinary_partition_targets(self.selection(), partition)

    def test_legacy_and_single_package_keep_complete_all_execution(self):
        for plan in (None, {}, inline_selection()):
            self.assertEqual(artifacts.ordinary_partitions(plan), ['all'])
        plan = self.selection()
        plan['ordinary_fast_targets'] = plan['ordinary_fast_targets'][:2]
        plan['ordinary_partitions'] = ['all']
        self.assertEqual(artifacts.ordinary_partition_targets(plan, 'all'), plan['ordinary_fast_targets'])
        self.assertEqual(artifacts.ordinary_partition_targets(self.selection()),
                         self.selection()['ordinary_fast_targets'])
        with self.assertRaises(ValueError):
            artifacts.build_variants('formal', self.selection(), ordinary_partition='oasis7_runtime')

    def test_partition_inventory_cannot_accept_missing_cross_package_or_relabelled_targets(self):
        plan = self.selection()
        manifest = self.manifest(plan, 'oasis7_runtime')
        artifacts.validate_ordinary_selection(manifest)
        for change in ('missing', 'cross-package', 'variant', 'features'):
            damaged = json.loads(json.dumps(manifest))
            if change == 'missing':
                damaged['binaries'].pop('harness-1')
            elif change == 'cross-package':
                damaged['binaries']['harness-2'] = fast_binary(plan['ordinary_fast_targets'][2])
            elif change == 'variant':
                damaged['binaries']['harness-0']['variant'] = 'required-fast-2'
            else:
                damaged['binaries']['harness-0']['requested_features'] = []
            with self.subTest(change=change), self.assertRaises(ValueError):
                artifacts.validate_ordinary_selection(damaged)

    def test_load_cannot_consume_a_different_partition_or_present_it_as_the_whole_group(self):
        plan = self.selection()
        with tempfile.TemporaryDirectory() as temporary, patch.object(artifacts, 'validate_plan_identity'):
            root = Path(temporary)
            (root / 'manifest.json').write_text(json.dumps(self.manifest(plan, 'oasis7_runtime')))
            for partition in (None, 'all', 'oasis7_world_state'):
                with self.subTest(partition=partition), self.assertRaisesRegex(ValueError, 'partition'):
                    artifacts.load(root, 'ordinary', plan=plan, ordinary_partition=partition)

    def test_validated_partition_runs_complete_harnesses_serially_with_the_cpu_budget(self):
        plan = self.selection()
        for limit, expected_threads in ((None, '4'), ('1', '1')):
            second_started = threading.Event()
            calls = []
            def invoke(command, cwd, env, suite, name, results, expected_count=None, **kwargs):
                self.assertEqual(len(command), 1)
                self.assertEqual(expected_count, 2)
                self.assertEqual(env['RUST_TEST_THREADS'], expected_threads)
                calls.append(name)
                if name == 'harness-0':
                    self.assertFalse(second_started.wait(timeout=0.1))
                else:
                    second_started.set()
                return ParallelNativeExecution.receipt(suite, name, results, expected_count)
            environment = {} if limit is None else {'RUST_TEST_THREADS': limit}
            with tempfile.TemporaryDirectory() as temporary, \
                    patch.object(artifacts, 'load', return_value=(self.manifest(plan, 'oasis7_runtime'), environment)), \
                    patch.object(artifacts, 'native_cpu_budget', return_value=4), \
                    patch('ci_process_runner.run', side_effect=invoke):
                results = Path(temporary) / 'results.jsonl'
                artifacts.execute(Path(temporary), 'ordinary', results, plan=plan,
                                  ordinary_partition='oasis7_runtime')
                records = [json.loads(line) for line in results.read_text().splitlines()]
            self.assertEqual(calls, ['harness-0', 'harness-1'])
            self.assertEqual(len(records), 2)
            self.assertTrue(all(record['passed'] and record['expected_count'] == 2 for record in records))


class ParallelNativeExecution(unittest.TestCase):
    def selection(self, count=3):
        return inline_selection() | {
            'ordinary_fast_targets': [fast_target(target_name='case_' + str(index))
                                      for index in range(count)],
        }

    def mixed_selection(self):
        selected = self.selection(5)
        for index, package in ((1, 'oasis7_viewer_codec'), (3, 'oasis7_runtime')):
            selected['ordinary_fast_targets'][index].update(
                package=package, target_kind='lib', target_name=package)
        return selected

    def manifest(self, selected):
        return dict(scope='ordinary', execution_plan=selected, binaries={
            'harness-' + str(index): fast_binary(target, 'required-fast-' + str(index)) |
                dict(path='harness-' + str(index), cwd='.')
            for index, target in enumerate(selected['ordinary_fast_targets'])
        })

    @staticmethod
    def receipt(suite, name, results, count, **changes):
        result = dict(suite=suite, test_name=name, expected_count=count, count_ok=True,
                      passed=True, cleanup_result='clean', cancelled=False, timeout=False,
                      exit_code=0, leaked_processes=False) | changes
        results.parent.mkdir(parents=True, exist_ok=True)
        with results.open('a') as stream:
            stream.write(json.dumps(result) + '\n')
        return result

    def test_inline_runs_every_complete_harness_once_with_bounded_cpu_budget(self):
        selected = self.selection()
        entered = threading.Barrier(2)
        lock = threading.Lock()
        calls, active = [], [0, 0]
        def invoke(command, cwd, env, suite, name, results, expected_count=None, **kwargs):
            with lock:
                active[0] += 1
                active[1] = max(active)
                calls.append((name, expected_count, env.get('RUST_TEST_THREADS')))
            try:
                if name in ('harness-0', 'harness-1'):
                    entered.wait(timeout=2)
                return self.receipt(suite, name, results, expected_count)
            finally:
                with lock:
                    active[0] -= 1
        with tempfile.TemporaryDirectory() as temporary, \
             patch.object(artifacts, 'load', return_value=(self.manifest(selected), {})), \
             patch.object(artifacts, 'native_cpu_budget', return_value=4), \
             patch('ci_process_runner.run', side_effect=invoke):
            results = Path(temporary) / 'results.jsonl'
            artifacts.execute(Path(temporary), 'ordinary', results, plan=selected)
            records = [json.loads(line) for line in results.read_text().splitlines()]
        self.assertEqual(active[1], 2)
        self.assertEqual(sorted(calls), [('harness-' + str(i), 2, '2') for i in range(3)])
        self.assertEqual(sorted(record['test_name'] for record in records),
                         ['harness-0', 'harness-1', 'harness-2'])
        self.assertTrue(all(record['passed'] for record in records))

    def test_inline_starts_complete_libraries_together_before_integrations(self):
        selected = self.mixed_selection()
        entered = threading.Barrier(2)
        lock = threading.Lock()
        calls, active = [], [0, 0]
        libraries = {'harness-1', 'harness-3'}
        def invoke(command, cwd, env, suite, name, results, expected_count=None, **kwargs):
            self.assertEqual(len(command), 1)
            self.assertEqual(expected_count, 2)
            self.assertEqual(env['RUST_TEST_THREADS'], '2')
            with lock:
                active[0] += 1
                active[1] = max(active)
                calls.append(name)
            try:
                if name in libraries:
                    entered.wait(timeout=2)
                return self.receipt(suite, name, results, expected_count)
            finally:
                with lock:
                    active[0] -= 1
        with tempfile.TemporaryDirectory() as temporary, \
             patch.object(artifacts, 'load', return_value=(self.manifest(selected), {})), \
             patch.object(artifacts, 'native_cpu_budget', return_value=4), \
             patch('ci_process_runner.run', side_effect=invoke):
            results = Path(temporary) / 'results.jsonl'
            artifacts.execute(Path(temporary), 'ordinary', results, plan=selected)
            records = [json.loads(line) for line in results.read_text().splitlines()]
        self.assertEqual(set(calls[:2]), libraries)
        self.assertEqual(active[1], 2)
        self.assertEqual(sorted(calls), ['harness-' + str(i) for i in range(5)])
        self.assertEqual(sorted(record['test_name'] for record in records), sorted(calls))
        self.assertTrue(all(record['passed'] and record['expected_count'] == 2 for record in records))

    def test_library_priority_keeps_stable_order_and_complete_cases_with_one_cpu(self):
        selected = self.mixed_selection()
        calls = []
        def invoke(command, cwd, env, suite, name, results, expected_count=None, **kwargs):
            self.assertEqual(len(command), 1)
            self.assertEqual(expected_count, 2)
            self.assertEqual(env['RUST_TEST_THREADS'], '1')
            calls.append(name)
            return self.receipt(suite, name, results, expected_count)
        with tempfile.TemporaryDirectory() as temporary, \
             patch.object(artifacts, 'load', return_value=(self.manifest(selected), {})), \
             patch.object(artifacts, 'native_cpu_budget', return_value=1), \
             patch('ci_process_runner.run', side_effect=invoke):
            artifacts.execute(Path(temporary), 'ordinary', Path(temporary) / 'results.jsonl', plan=selected)
        self.assertEqual(calls, ['harness-' + str(i) for i in (1, 3, 0, 2, 4)])

    def test_failed_priority_library_cancels_other_owner_without_starting_integrations(self):
        selected = self.mixed_selection()
        entered = threading.Barrier(2)
        calls = []
        def invoke(command, cwd, env, suite, name, results, expected_count=None, **kwargs):
            calls.append(name)
            if name not in ('harness-1', 'harness-3'):
                return self.receipt(suite, name, results, expected_count)
            entered.wait(timeout=2)
            if name == 'harness-1':
                return self.receipt(suite, name, results, expected_count, passed=False, exit_code=7)
            self.assertTrue(kwargs['cancellation_event'].wait(timeout=2))
            return self.receipt(suite, name, results, expected_count,
                                passed=False, cancelled=True, exit_code=-signal.SIGTERM)
        with tempfile.TemporaryDirectory() as temporary, \
             patch.object(artifacts, 'load', return_value=(self.manifest(selected), {})), \
             patch.object(artifacts, 'native_cpu_budget', return_value=4), \
             patch('ci_process_runner.run', side_effect=invoke):
            results = Path(temporary) / 'results.jsonl'
            with self.assertRaisesRegex(RuntimeError, 'ordinary failed'):
                artifacts.execute(Path(temporary), 'ordinary', results, plan=selected)
            records = [json.loads(line) for line in results.read_text().splitlines()]
        self.assertEqual(sorted(calls), ['harness-1', 'harness-3'])
        self.assertEqual(len(records), 2)
        failed = next(record for record in records if record['test_name'] == 'harness-1')
        self.assertEqual(failed['exit_code'], 7)
        self.assertFalse(failed['passed'])
        self.assertTrue(next(record for record in records if record['test_name'] == 'harness-3')['cancelled'])

    def test_first_failure_stops_pending_and_cancels_active_without_retry(self):
        selected = self.selection()
        entered = threading.Barrier(2)
        calls = []
        def invoke(command, cwd, env, suite, name, results, expected_count=None, **kwargs):
            calls.append(name)
            entered.wait(timeout=2)
            if name == 'harness-0':
                return self.receipt(suite, name, results, expected_count, passed=False, exit_code=7)
            self.assertTrue(kwargs['cancellation_event'].wait(timeout=2))
            return self.receipt(suite, name, results, expected_count,
                                passed=False, cancelled=True, exit_code=-signal.SIGTERM)
        with tempfile.TemporaryDirectory() as temporary, \
             patch.object(artifacts, 'load', return_value=(self.manifest(selected), {})), \
             patch.object(artifacts, 'native_cpu_budget', return_value=4), \
             patch('ci_process_runner.run', side_effect=invoke):
            results = Path(temporary) / 'results.jsonl'
            with self.assertRaisesRegex(RuntimeError, 'ordinary failed'):
                artifacts.execute(Path(temporary), 'ordinary', results, plan=selected)
            records = [json.loads(line) for line in results.read_text().splitlines()]
        self.assertEqual(sorted(calls), ['harness-0', 'harness-1'])
        self.assertEqual(len(records), 2)
        first = next(record for record in records if record['test_name'] == 'harness-0')
        self.assertEqual(first['exit_code'], 7)
        self.assertFalse(first['passed'])
        self.assertTrue(next(record for record in records if record['test_name'] == 'harness-1')['cancelled'])

    def test_missing_duplicate_or_wrong_worker_receipt_cannot_pass(self):
        selected = self.selection(1)
        for damage in ('missing', 'duplicate', 'identity', 'count', 'exit-zero'):
            def invoke(command, cwd, env, suite, name, results, expected_count=None, **kwargs):
                if damage == 'exit-zero':
                    raise SystemExit(0)
                result = self.receipt(suite, name, results, expected_count)
                if damage == 'missing':
                    results.unlink()
                elif damage == 'duplicate':
                    with results.open('a') as stream:
                        stream.write(json.dumps(result) + '\n')
                else:
                    result['test_name' if damage == 'identity' else 'expected_count'] = 'other'
                    results.write_text(json.dumps(result) + '\n')
                return result
            with self.subTest(damage=damage), tempfile.TemporaryDirectory() as temporary, \
                 patch.object(artifacts, 'load', return_value=(self.manifest(selected), {})), \
                 patch('ci_process_runner.run', side_effect=invoke):
                with self.assertRaisesRegex(RuntimeError, 'ordinary failed'):
                    artifacts.execute(Path(temporary), 'ordinary', Path(temporary) / 'results.jsonl', plan=selected)

    def test_legacy_targets_keep_sequential_execution_and_environment(self):
        for version in (None, 2):
            selected = self.mixed_selection()
            if version is None:
                del selected['fast_execution_version']
            else:
                selected['fast_execution_version'] = version
            calls = []
            def invoke(command, cwd, env, suite, name, results, expected_count=None, **kwargs):
                self.assertNotIn('cancellation_event', kwargs)
                self.assertEqual(env.get('RUST_TEST_THREADS'), '3')
                calls.append(name)
                return self.receipt(suite, name, results, expected_count)
            with self.subTest(version=version), tempfile.TemporaryDirectory() as temporary, \
                 patch.object(artifacts, 'load', return_value=(self.manifest(selected), {'RUST_TEST_THREADS': '3'})), \
                 patch('ci_process_runner.run', side_effect=invoke):
                artifacts.execute(Path(temporary), 'ordinary', Path(temporary) / 'results.jsonl', plan=selected)
            self.assertEqual(calls, ['harness-' + str(i) for i in range(5)])

    def test_inline_respects_one_cpu_and_existing_lower_thread_limit(self):
        selected = self.selection()
        for cpus, configured, expected in ((1, '9', '1'), (8, '1', '1')):
            active, maximum, calls = [0], [0], []
            lock = threading.Lock()
            def invoke(command, cwd, env, suite, name, results, expected_count=None, **kwargs):
                with lock:
                    active[0] += 1
                    maximum[0] = max(maximum[0], active[0])
                    calls.append(env['RUST_TEST_THREADS'])
                time.sleep(0.01)
                with lock:
                    active[0] -= 1
                return self.receipt(suite, name, results, expected_count)
            with self.subTest(cpus=cpus), tempfile.TemporaryDirectory() as temporary, \
                 patch.object(artifacts, 'load', return_value=(self.manifest(selected), {'RUST_TEST_THREADS': configured})), \
                 patch.object(artifacts, 'native_cpu_budget', return_value=cpus), \
                 patch('ci_process_runner.run', side_effect=invoke):
                artifacts.execute(Path(temporary), 'ordinary', Path(temporary) / 'results.jsonl', plan=selected)
            self.assertEqual(calls, [expected] * 3)
            self.assertLessEqual(maximum[0], min(2, cpus))

    def test_invalid_thread_budget_fails_before_starting_harnesses(self):
        selected = self.selection()
        for configured in ('0', '-1', 'unbounded'):
            with self.subTest(configured=configured), tempfile.TemporaryDirectory() as temporary, \
                 patch.object(artifacts, 'load', return_value=(self.manifest(selected), {'RUST_TEST_THREADS': configured})), \
                 patch('ci_process_runner.run') as execute_test:
                with self.assertRaisesRegex(ValueError, 'RUST_TEST_THREADS'):
                    artifacts.execute(Path(temporary), 'ordinary', Path(temporary) / 'results.jsonl', plan=selected)
                execute_test.assert_not_called()

    def test_read_error_leak_or_inconsistent_success_cannot_start_pending_harnesses(self):
        selected = self.selection()
        for changes in (dict(passed=False, count_ok=False, output_read_error='OSError'),
                        dict(passed=False, cleanup_result='unresolved', leaked_processes=True),
                        dict(passed=True, cleanup_result='unresolved'),
                        dict(passed=True, count_ok=False)):
            calls = []
            def invoke(command, cwd, env, suite, name, results, expected_count=None, **kwargs):
                calls.append(name)
                return self.receipt(suite, name, results, expected_count, **changes)
            with self.subTest(changes=changes), tempfile.TemporaryDirectory() as temporary, \
                 patch.object(artifacts, 'load', return_value=(self.manifest(selected), {})), \
                 patch.object(artifacts, 'native_cpu_budget', return_value=1), \
                 patch('ci_process_runner.run', side_effect=invoke):
                with self.assertRaisesRegex(RuntimeError, 'ordinary failed'):
                    artifacts.execute(Path(temporary), 'ordinary', Path(temporary) / 'results.jsonl', plan=selected)
            self.assertEqual(calls, ['harness-0'])

    def test_signal_stops_dispatch_and_cancels_each_active_owner(self):
        selected = self.selection()
        entered = threading.Barrier(2)
        handlers = {}
        calls = []
        def register(signum, handler):
            previous = handlers.get(signum, 'prior-handler')
            handlers[signum] = handler
            return previous
        def invoke(command, cwd, env, suite, name, results, expected_count=None, **kwargs):
            calls.append(name)
            entered.wait(timeout=2)
            if name == 'harness-0':
                handlers[signal.SIGTERM](signal.SIGTERM, None)
            self.assertTrue(kwargs['cancellation_event'].wait(timeout=2))
            return self.receipt(suite, name, results, expected_count,
                                passed=False, cancelled=True, exit_code=-signal.SIGTERM)
        with tempfile.TemporaryDirectory() as temporary, \
             patch.object(artifacts, 'load', return_value=(self.manifest(selected), {})), \
             patch.object(artifacts, 'native_cpu_budget', return_value=2), \
             patch.object(artifacts.signal, 'signal', side_effect=register), \
             patch('ci_process_runner.run', side_effect=invoke):
            results = Path(temporary) / 'results.jsonl'
            with self.assertRaisesRegex(RuntimeError, 'ordinary failed'):
                artifacts.execute(Path(temporary), 'ordinary', results, plan=selected)
            records = [json.loads(line) for line in results.read_text().splitlines()]
        self.assertEqual(sorted(calls), ['harness-0', 'harness-1'])
        self.assertEqual(len(records), 2)
        self.assertTrue(all(record['cancelled'] and not record['passed'] for record in records))
        self.assertEqual(handlers, {signal.SIGTERM: 'prior-handler', signal.SIGINT: 'prior-handler'})

    def test_cpu_budget_obeys_visible_quota_and_affinity(self):
        root = Path('/sys/fs/cgroup')
        for files, expected in (
            ({root / 'cpu.max': '100000 100000'}, 1),
            ({root / 'cpu.max': '250000 100000'}, 2),
            ({root / 'cpu.max': '50000 100000'}, 1),
            ({root / 'cpu.max': 'max 100000'}, 8),
            ({root / 'cpu.max': '1600000 100000'}, 8),
            ({root / 'cpu/cpu.cfs_quota_us': '200000',
              root / 'cpu/cpu.cfs_period_us': '100000'}, 2),
            ({root / 'cpu/cpu.cfs_quota_us': '-1',
              root / 'cpu/cpu.cfs_period_us': '100000'}, 8),
            ({}, 8),
        ):
            def read(path, *args, **kwargs):
                if path not in files:
                    raise FileNotFoundError(path)
                return files[path]
            with self.subTest(files=files), \
                 patch.object(artifacts.os, 'sched_getaffinity', return_value=set(range(8)), create=True), \
                 patch.object(Path, 'read_text', read):
                self.assertEqual(artifacts.native_cpu_budget(), expected)
        for value in ('0 100000', '-1 100000', '100000 0', 'max 0', 'invalid'):
            with self.subTest(value=value), \
                 patch.object(artifacts.os, 'sched_getaffinity', return_value=set(range(8)), create=True), \
                 patch.object(Path, 'read_text', return_value=value):
                with self.assertRaisesRegex(ValueError, 'CPU quota'):
                    artifacts.native_cpu_budget()


if __name__ == '__main__':
    unittest.main()
