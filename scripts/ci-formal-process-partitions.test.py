#!/usr/bin/env python3
"""Risk target inventory, complete artifact partitions, and targeted artifact partitions regressions split from ci-formal-process.test.py."""
from contextlib import redirect_stdout
import copy
import importlib.util
import io
import json
from pathlib import Path
import sys
import tempfile
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


def fast_execution_selection(packages=('oasis7_viewer_codec',), suites=()):
    return dict(fast_plan_version=1, execution_profile='pr-fast', scope='targeted',
                groups=['baseline', 'oasis7_required'] + (['formal_process'] if suites else []),
                oasis7_required_packages=list(packages),
                oasis7_required_wasm_packages=[p for p in packages if p in artifacts.PORTABLE_REQUIRED_PACKAGES],
                formal_suites=list(suites), test_policy_changed=False,
                architecture=dict(source='source', tested='tested'))


def fast_target_binary(target, variant='required-fast-0'):
    return dict(ordinary=True, test=True, package=target['package'], target=target['target_name'],
                target_kind=[target['target_kind']], default_features=target['default_features'],
                requested_features=target['features'], features=target['features'],
                tests=target['cases'] + ['new_regression'], ignored_tests=[], variant=variant)


def parallel_native_receipt(suite, name, results, count, **changes):
    result = dict(suite=suite, test_name=name, expected_count=count, count_ok=True,
                  passed=True, cleanup_result='clean', cancelled=False, timeout=False,
                  exit_code=0, leaked_processes=False) | changes
    results.parent.mkdir(parents=True, exist_ok=True)
    with results.open('a') as stream:
        stream.write(json.dumps(result) + '\n')
    return result


def coverage_fixture():
    config = json.loads((HERE / 'ci-formal-process-suites.json').read_text())
    binaries = {name: dict(tests=['formal::logic'], ignored_tests=[]) for name in config['environment_formal_batches']}
    for entries in config['suites'].values():
        for entry in entries:
            b = binaries[entry['binary']]
            b['tests'].append(entry['name'])
            b['ignored_tests'].append(entry['name'])
    for binary, names in config['auxiliary_entries'].items():
        binaries[binary]['tests'].extend(names)
    for binary, overrides in config.get('case_shards', {}).items():
        binaries[binary]['tests'].extend(name for name in overrides if name not in binaries[binary]['tests'])
    return config, binaries


class RiskTargetInventory(unittest.TestCase):
    def target(self):
        return dict(package='oasis7_viewer_codec', target_kind='test', target_name='scale',
                    default_features=True, features=['test_tier_full'], cases=['full_scale'])

    def selection(self):
        return fast_execution_selection(('oasis7_viewer_codec',)) | dict(
            risk_test_inventory_version=1, ordinary_risk_targets=[self.target()])

    def test_risk_targets_are_real_builds_and_clippy_without_expanding_every_package(self):
        selected = self.selection()
        variants = artifacts.build_variants('ordinary', selected)
        self.assertEqual([(name, args) for name, args, _ in variants if name.startswith('required-risk-')],
                         [('required-risk-0', ['test', '-p', 'oasis7_viewer_codec', '--test', 'scale',
                                               '--features', 'test_tier_full', '--no-run'])])
        original = next(args for name, args, _ in variants if name == 'required')
        self.assertNotIn('oasis7_viewer_codec/test_tier_full', original[original.index('--features') + 1])
        self.assertIn(['-p', 'oasis7_viewer_codec', '--test', 'scale', '--features', 'test_tier_full'],
                      artifacts.required_native_commands(selected))
        expanded = selected | {'test_policy_changed': True}
        self.assertFalse(any(name.startswith('required-risk-') for name, _, _ in
                             artifacts.build_variants('ordinary', expanded)))

    def test_risk_case_and_full_feature_cannot_disappear_or_replace_required(self):
        selected = self.selection()
        full = fast_target_binary(self.target(), 'required-risk-0')
        original = dict(full, target='ordinary', variant='required', features=['test_tier_required'],
                        requested_features=['oasis7_viewer_codec/test_tier_required'], tests=['required_case'])
        manifest = dict(scope='ordinary', execution_plan=selected, binaries={'ordinary': original, 'risk': full})
        artifacts.validate_ordinary_selection(manifest)
        for change in ('missing', 'ignored', 'feature', 'required'):
            broken = copy.deepcopy(manifest)
            if change == 'missing':
                del broken['binaries']['risk']
            elif change == 'required':
                del broken['binaries']['ordinary']
            elif change == 'ignored':
                broken['binaries']['risk']['ignored_tests'] = ['full_scale']
            else:
                broken['binaries']['risk']['features'] = ['test_tier_required']
            with self.subTest(change=change), self.assertRaises(ValueError):
                artifacts.validate_ordinary_selection(broken)
        expanded = dict(manifest, execution_plan=selected | {'test_policy_changed': True},
                        binaries={'full-required': dict(full, variant='required',
                            requested_features=['oasis7_viewer_codec/test_tier_required',
                                                'oasis7_viewer_codec/test_tier_full'])})
        artifacts.validate_ordinary_selection(expanded)
        expanded['binaries']['full-required']['tests'] = ['unrelated_case']
        with self.assertRaises(ValueError):
            artifacts.validate_ordinary_selection(expanded)

    def test_all_scope_cannot_silently_drop_risk_contracts(self):
        for expanded in (False, True):
            selected = self.selection() | {'test_policy_changed': expanded}
            with self.subTest(expanded=expanded):
                with self.assertRaisesRegex(ValueError, 'ordinary risk contracts'):
                    artifacts.build_variants('all', selected)
                with self.assertRaisesRegex(ValueError, 'ordinary risk contracts'):
                    artifacts.validate_ordinary_selection(dict(
                        scope='all', execution_plan=selected, binaries={}))
                # Formal components use the same parent plan but do not own the
                # ordinary inventory; they must still build independently.
                self.assertTrue(artifacts.build_variants('formal-debug', selected))
                artifacts.validate_ordinary_selection(dict(
                    scope='formal-debug', execution_plan=selected, binaries={}))

    def test_weighted_owners_keep_full_risk_variant_complete_with_its_own_cost_identity(self):
        fixture = TargetedArtifactPartitions()
        selected = fixture.selection() | dict(
            oasis7_required_packages=['oasis7_chain_runtime', 'oasis7_viewer_codec'],
            risk_test_inventory_version=1, ordinary_risk_targets=[self.target()])
        selected['ordinary_harness_costs'] = fixture.costs([dict(
            package='oasis7_viewer_codec', target_kind='test', target_name='scale',
            variant='required', mode='batch', selector='', elapsed_ms=100000)])
        manifest = fixture.manifest(selected)
        full = fast_target_binary(self.target(), 'required-risk-0')
        manifest['binaries']['full-scale'] = full | dict(path='full-scale', cwd='.')
        manifest['binaries']['required-scale'] = full | dict(
            path='required-scale', cwd='.', variant='required', tests=['required_chunk'],
            features=['test_tier_required'], requested_features=['oasis7_viewer_codec/test_tier_required'])
        artifacts.validate_ordinary_selection(manifest)
        actual, receipts, full_owners = [], [], []
        for owner in selected['ordinary_partitions'][1:]:
            captured = io.StringIO()
            with redirect_stdout(captured):
                entries = artifacts.ordinary_artifact_entries(manifest, selected, owner)
            receipt = json.loads(captured.getvalue().split('=', 1)[1])
            covered = fixture.coverage(manifest, entries)
            self.assertEqual(receipt['expected_count'], len(covered))
            actual.extend(covered)
            receipts.append(receipt)
            full_owners.extend(owner for entry in entries if entry['binary'] == 'full-scale')
        self.assertEqual(len(full_owners), 1)
        self.assertIn(('full-scale', 'full_scale'), actual)
        self.assertCountEqual(actual, fixture.coverage(manifest, artifacts.ordinary_entries(manifest)))
        self.assertEqual(len(actual), len(set(actual)))
        # The old required timing is not reused for the distinct full feature
        # variant; an unknown full harness still executes with the fallback.
        self.assertEqual(sum(item['cost_matched'] for item in receipts), 1)
        fallback_count = sum(item['cost_fallback'] for item in receipts)
        self.assertGreaterEqual(fallback_count, 1)
        self.assertEqual(sum(item['harness_estimated_ms'] for item in receipts),
                         100000 + fallback_count * selected['ordinary_harness_costs']['fallback_ms'])


class CompleteArtifactPartitions(unittest.TestCase):
    def selection(self):
        return fast_execution_selection() | dict(
            scope='full', ordinary_artifact_partition_version=1,
            ordinary_partitions=['controls', 'runtime', 'component-libs', 'integrations-0', 'integrations-1'])

    def policy_selection(self):
        return self.selection() | dict(scope='targeted', test_policy_changed=True,
            oasis7_required_packages=['oasis7', 'oasis7_chain_runtime', 'oasis7_viewer_runtime'],
            oasis7_required_wasm_packages=['oasis7_viewer_runtime'])

    def manifest(self, selected):
        binaries = {}
        for name, package, kind, variant in [
            ('runtime-tests', 'oasis7_chain_runtime', 'lib', 'required'),
            ('oasis7-lib-tests', 'oasis7', 'lib', 'required'),
            ('component', 'oasis7_runtime', 'lib', 'required'),
            ('new-integration', 'oasis7_runtime', 'test', 'required'),
            ('supplement', 'oasis7', 'lib', 'required-full-fast-0'),
            ('empty', 'oasis7', 'test', 'required')]:
            binaries[name] = dict(package=package, target=package, target_kind=[kind],
                                  variant=variant, ordinary=True, tests=[] if name == 'empty' else ['actual_behavior'],
                                  ignored_tests=[], path=name, cwd='.')
        for owner, pattern in artifacts.DEFAULT_SNAPSHOT_REGRESSIONS:
            binaries[owner] = dict(package='oasis7', target='oasis7', target_kind=['lib'],
                                   variant=owner, ordinary=False, tests=[pattern + 'regression'],
                                   ignored_tests=[], path=owner, cwd='.')
        return dict(scope='ordinary', execution_plan=selected, binaries=binaries)

    def test_complete_partition_union_keeps_variants_snapshots_and_new_harnesses(self):
        selected = self.selection()
        manifest = self.manifest(selected)
        groups = {partition: artifacts.ordinary_artifact_entries(manifest, selected, partition)
                  for partition in selected['ordinary_partitions']}
        self.assertEqual(groups['controls'], [])
        self.assertEqual([entry['binary'] for entry in groups['runtime']], ['runtime-tests'])
        self.assertEqual([entry['binary'] for entry in groups['component-libs']], ['component', 'oasis7-lib-tests', 'supplement'])
        self.assertEqual(len(groups['integrations-0']), 1)
        self.assertEqual(len(groups['integrations-1']), 1)
        self.assertCountEqual([entry for values in groups.values() for entry in values],
                              artifacts.ordinary_entries(manifest))
        reordered = copy.deepcopy(manifest)
        reordered['binaries'] = dict(reversed(list(reordered['binaries'].items())))
        reordered['binaries']['new-integration']['tests'].append('new_case_same_complete_harness')
        for partition in selected['ordinary_partitions']:
            self.assertEqual(artifacts.ordinary_artifact_entries(reordered, selected, partition), groups[partition])
        original = {key: value for key, value in selected.items()
                    if key not in ('ordinary_artifact_partition_version', 'ordinary_partitions')}
        self.assertEqual(artifacts.build_variants('ordinary', selected), artifacts.build_variants('ordinary', original))
        with self.assertRaises(ValueError):
            artifacts.build_variants('ordinary', selected, ordinary_partition='runtime')

    def test_complete_partition_rejects_partial_unknown_and_inline_capabilities(self):
        for key, value in [('ordinary_artifact_partition_version', True),
                           ('ordinary_artifact_partition_version', 2),
                           ('ordinary_partitions', ['runtime']), ('ordinary_partitions', ['all']),
                           ('ordinary_partitions', ['runtime'] * 5), ('scope', 'targeted'),
                           ('fast_execution_version', 1), ('ordinary_partition_version', 1)]:
            with self.subTest(key=key, value=value), self.assertRaises(ValueError):
                artifacts.ordinary_partitions(self.selection() | {key: value})
        for field in ('ordinary_artifact_partition_version', 'ordinary_partitions', 'fast_plan_version'):
            selected = self.selection()
            del selected[field]
            with self.subTest(missing=field), self.assertRaises(ValueError):
                artifacts.ordinary_partitions(selected)

    def test_targeted_policy_partitions_preserve_build_features_and_complete_inventory(self):
        for profile in ('pr-fast', 'main-fast'):
            selected = self.policy_selection() | dict(execution_profile=profile, ordinary_artifact_delivery_version=1)
            manifest = self.manifest(selected)
            manifest['binaries']['supplement'].update(package='oasis7_viewer_runtime',
                target='oasis7_viewer_runtime', variant='required', tests=['required_behavior', 'new_full_behavior'])
            with self.subTest(profile=profile):
                partitions = artifacts.ordinary_partitions(selected)
                groups = {owner: artifacts.ordinary_artifact_entries(manifest, selected, owner) for owner in partitions}
                self.assertEqual(groups['controls'], [])
                self.assertTrue(all(groups[owner] for owner in partitions[1:]))
                self.assertCountEqual([entry for entries in groups.values() for entry in entries],
                                      artifacts.ordinary_entries(manifest))
                original = {key: value for key, value in selected.items() if key not in (
                    'ordinary_artifact_partition_version', 'ordinary_partitions', 'ordinary_artifact_delivery_version')}
                self.assertEqual(artifacts.build_variants('ordinary', selected),
                                 artifacts.build_variants('ordinary', original))
                self.assertEqual(artifacts.required_native_commands(selected), artifacts.required_native_commands(original))
                self.assertEqual(artifacts.snapshot_regressions(manifest), artifacts.DEFAULT_SNAPSHOT_REGRESSIONS)
                required = next(args for name, args, _ in artifacts.build_variants('ordinary', selected) if name == 'required')
                self.assertIn('oasis7_chain_runtime/test_tier_full', required[required.index('--features') + 1].split(','))

    def test_targeted_policy_partition_consumer_rejects_incomplete_or_mixed_plans(self):
        original = self.policy_selection()
        changes = [
            ('oasis7_required_packages', [package for package in original['oasis7_required_packages'] if package != missing])
            for missing in original['oasis7_required_packages']
        ] + [
            ('oasis7_required_packages', None), ('oasis7_required_packages', 'oasis7'),
            ('test_policy_changed', False), ('test_policy_changed', 1),
            ('execution_profile', 'candidate-full'), ('execution_profile', 'periodic-full'),
            ('ordinary_fast_targets', []), ('ordinary_full_fast_targets', []),
            ('ordinary_execution', 'inline'), ('fast_execution_version', 1), ('ordinary_partition_version', 1),
            ('ordinary_artifact_partition_version', 2), ('ordinary_artifact_delivery_version', 2),
            ('ordinary_partitions', ['all']), ('ordinary_partitions', ['controls', 'runtime']),
            ('ordinary_partitions', ['controls', 'runtime', 'component-libs', 'integrations-0', 'integrations-0']),
            ('groups', ['baseline']),
        ]
        for key, value in changes:
            with self.subTest(key=key, value=value), self.assertRaises(ValueError):
                artifacts.ordinary_partitions(original | {key: value})
        for key in ('test_policy_changed', 'oasis7_required_packages'):
            selected = dict(original)
            selected.pop(key)
            with self.subTest(missing=key), self.assertRaises(ValueError):
                artifacts.ordinary_partitions(selected)

    def test_full_parent_still_executes_formal_and_native_failure_is_not_hidden(self):
        selected = self.selection()
        manifest = self.manifest(selected)
        formal = dict(scope='formal', formal_execution_version=1, binaries=manifest['binaries'])
        with tempfile.TemporaryDirectory() as temporary, \
             patch.object(artifacts, 'load', return_value=(formal, {})) as load, \
             patch.object(artifacts, 'selected_entries', return_value=[dict(binary='runtime-tests', name='case', ignored=False, suite='runtime-protocol')]), \
             patch.object(artifacts, 'execute_inline_jobs') as pool:
            artifacts.execute(Path(temporary), None, Path(temporary) / 'results', shard='runtime-protocol', plan=selected)
            pool.assert_called_once()
            self.assertEqual(pool.call_args.args[0][0]['suite'], 'runtime-protocol')
        with tempfile.TemporaryDirectory() as temporary, \
             patch.object(artifacts, 'load', return_value=(manifest, {'OASIS7_FORMAL_TEST': 'must-be-removed'})) as load, \
             patch('ci_process_runner.run', side_effect=lambda command, cwd, env, suite, name, results, expected_count=None, **kwargs:
                   parallel_native_receipt(suite, name, results, expected_count, passed=False, exit_code=101, count_ok=False)) as runner:
            with self.assertRaises(RuntimeError):
                artifacts.execute(Path(temporary), 'ordinary', Path(temporary) / 'results', plan=selected, ordinary_partition='runtime')
            self.assertEqual(runner.call_count, 1)
            self.assertNotIn('OASIS7_FORMAL_TEST', runner.call_args.args[2])
            self.assertNotIn('ordinary_partition', load.call_args.kwargs)

    def test_unpartitioned_complete_execution_runs_every_harness_and_propagates_failure(self):
        selected = self.selection()
        manifest = self.manifest(selected)
        for passed in (True, False):
            with self.subTest(passed=passed), tempfile.TemporaryDirectory() as temporary, \
                 patch.object(artifacts, 'load', return_value=(manifest, {})) as load, \
                 patch('ci_process_runner.run', return_value=dict(passed=passed, cleanup_result='clean')) as runner:
                def execute():
                    artifacts.execute(Path(temporary), 'ordinary', Path(temporary) / 'results', plan=selected)
                if passed:
                    execute()
                else:
                    with self.assertRaises(RuntimeError):
                        execute()
                self.assertCountEqual([call.args[4] for call in runner.call_args_list],
                    ['runtime-tests', 'oasis7-lib-tests', 'component', 'new-integration', 'supplement',
                     *[pattern for _, pattern in artifacts.DEFAULT_SNAPSHOT_REGRESSIONS]])
                self.assertNotIn('ordinary_partition', load.call_args.kwargs)
        for partition in ('controls', 'all', 'unknown'):
            with self.subTest(partition=partition), patch.object(artifacts, 'load') as load:
                with self.assertRaises(ValueError):
                    artifacts.execute(Path('.'), 'ordinary', Path('results'), plan=selected,
                                      ordinary_partition=partition)
                load.assert_not_called()

    def test_environment_batch_destinations_are_exact_and_nonempty(self):
        config, binaries = coverage_fixture()
        selected = fast_execution_selection(suites=('runtime-protocol',))
        self.assertEqual(set(artifacts.formal_shards(selected)), {'runtime-protocol', 'healthy-set', 'lifecycle-cli'})
        selected['formal_shards'] = artifacts.formal_shards(selected)
        manifest = dict(execution_plan=selected, suites=artifacts.resolve_suites(config, binaries))
        combined = []
        for shard in selected['formal_shards']:
            entries = artifacts.selected_entries(manifest, config, shard=shard)
            self.assertTrue(entries, shard)
            self.assertTrue(all(entry['suite'] == 'runtime-protocol' for entry in entries))
            combined.extend(entries)
        self.assertEqual(len(combined), len(manifest['suites']['runtime-protocol']))
        self.assertEqual(len({(entry.get('binary'), entry.get('name'), entry.get('script')) for entry in combined}), len(combined))


class TargetedArtifactPartitions(unittest.TestCase):
    full_sync_cases = (
        'formal_full_sync_pins_the_first_verified_target_cut_across_providers',
        'formal_full_sync_rejects_over_budget_material_without_publishing_then_recovers',
        'formal_full_sync_pins_only_objects_the_successful_candidate_persisted',
        'formal_full_sync_shared_budget_accumulates_prior_charges_without_publishing',
    )

    def selection(self):
        return fast_execution_selection(packages=('oasis7_chain_runtime',)) | dict(
            ordinary_artifact_partition_version=2,
            ordinary_partitions=['controls', 'native-0', 'native-1', 'native-2', 'native-3'])

    def manifest(self, selected=None):
        cases = [f'case_{index:04d}' for index in range(1288)] + list(self.full_sync_cases)
        cases += sorted(artifacts.ORDINARY_EXCLUSIVE_RUNTIME_CASES)
        return dict(scope='ordinary', execution_plan=selected or self.selection(), binaries={
            'runtime-tests': dict(package='oasis7_chain_runtime', target='oasis7_chain_runtime',
                target_kind=['lib'], variant='required', ordinary=True, test=True, path='runtime', cwd='.',
                tests=cases + ['ignored_child'], ignored_tests=['ignored_child']),
            'integration': dict(package='oasis7_chain_runtime', target='real_http',
                target_kind=['test'], variant='required', ordinary=True, test=True, path='integration', cwd='.',
                tests=['real_http_success', 'real_http_refusal', 'real_http_recovery'], ignored_tests=[]),
            'new-integration': dict(package='oasis7_chain_runtime', target='new_fixture',
                target_kind=['test'], variant='required', ordinary=True, test=True, path='new', cwd='.',
                tests=['new_case'], ignored_tests=[]),
        })

    @staticmethod
    def costs(rows=()):
        return dict(version=1, fallback_ms=1000, evidence=dict(
            run_id=43179733413, run_attempt=1,
            source_sha='718dee8464b5681082d7ab4ea7d267cd838ead8c',
            tested_sha='40c170e7837b1bf0d30ab877a2a826b37555ba71',
            policy_sha='e2dbafdaf507a1f228c09c1d2d49420f278117ae'), harnesses=list(rows))

    def test_measured_costs_balance_whole_harnesses_without_changing_any_case(self):
        selected = self.selection()
        manifest = self.manifest(selected)
        rows = []
        for index, milliseconds in enumerate((100000, 90000, 80000, 70000, 1000)):
            name = f'cost_{index}'
            manifest['binaries'][name] = dict(package='oasis7_chain_runtime', target=name,
                target_kind=['test'], variant='required', ordinary=True, test=True, path=name, cwd='.',
                tests=[f'case_{i}' for i in range(1000 if index == 4 else 1)], ignored_tests=[])
            rows.append(dict(package='oasis7_chain_runtime', target_kind='test', target_name=name,
                             variant='required', mode='batch', selector='', elapsed_ms=milliseconds))
        selected['ordinary_harness_costs'] = self.costs(rows)
        groups = {owner: artifacts.ordinary_artifact_entries(manifest, selected, owner)
                  for owner in selected['ordinary_partitions'][1:]}
        ownership = {entry['binary']: owner for owner, entries in groups.items() for entry in entries
                     if entry['binary'].startswith('cost_')}
        self.assertEqual(ownership, dict(cost_0='native-0', cost_1='native-1', cost_2='native-2',
                                         cost_3='native-3', cost_4='native-3'))
        actual = [case for entries in groups.values() for case in self.coverage(manifest, entries)]
        self.assertCountEqual(actual, self.coverage(manifest, artifacts.ordinary_entries(manifest)))
        self.assertEqual(len(actual), len(set(actual)))
        reordered = copy.deepcopy(manifest)
        reordered['binaries'] = dict(reversed(list(reordered['binaries'].items())))
        reordered['execution_plan']['ordinary_harness_costs']['harnesses'].reverse()
        for owner, entries in groups.items():
            self.assertEqual(artifacts.ordinary_artifact_entries(
                reordered, reordered['execution_plan'], owner), entries)
        legacy = {key: value for key, value in selected.items() if key != 'ordinary_harness_costs'}
        self.assertEqual(artifacts.build_variants('ordinary', selected), artifacts.build_variants('ordinary', legacy))
        self.assertEqual(artifacts.required_native_commands(selected), artifacts.required_native_commands(legacy))

    def test_cost_profile_rejects_invalid_data_and_incompatible_plans(self):
        row = dict(package='oasis7_chain_runtime', target_kind='test', target_name='real_http',
                   variant='required', mode='batch', selector='', elapsed_ms=100)
        good = self.costs([row])
        invalid = [good | {'version': value} for value in (True, 0, 2, None, '1')]
        invalid += [good | {'fallback_ms': value} for value in (True, 0, -1, 3600001, 1.5, float('nan'))]
        invalid += [good | {'harnesses': [row, row]}, good | {'evidence': {}}]
        for field, values in dict(elapsed_ms=[True, 0, -1, 3600001, 1.5, float('inf')],
                                  mode=['unknown'], selector=['unexpected'], target_kind=['unknown'],
                                  target_name=['../target'], variant=['']).items():
            invalid.extend(good | {'harnesses': [row | {field: value}]} for value in values)
        for profile in invalid:
            with self.subTest(profile=profile), self.assertRaises(ValueError):
                artifacts.ordinary_partitions(self.selection() | {'ordinary_harness_costs': profile})
        for base in (CompleteArtifactPartitions().selection(), {'ordinary_harness_costs': good},
                     self.selection() | {'ordinary_execution': 'inline'}):
            with self.subTest(base=base), self.assertRaises(ValueError):
                artifacts.ordinary_partitions(base | {'ordinary_harness_costs': good})

    def coverage(self, manifest, entries):
        selected = []
        for entry in entries:
            binary = manifest['binaries'][entry['binary']]
            if entry.get('batch'):
                names = set(binary['tests']) - set(binary['ignored_tests'])
                names -= set(entry.get('skip', ())) | set(entry.get('skip_exact', ()))
            elif 'filter' in entry:
                names = {name for name in binary['tests'] if entry['filter'] in name
                         and name not in binary['ignored_tests']}
            else:
                names = {entry['name']}
            selected.extend((entry['binary'], name) for name in names)
        return selected

    def test_complete_runtime_and_other_harness_inventory_has_one_nonempty_owner(self):
        selected = self.selection()
        manifest = self.manifest(selected)
        groups = {owner: artifacts.ordinary_artifact_entries(manifest, selected, owner)
                  for owner in selected['ordinary_partitions']}
        self.assertEqual(groups.pop('controls'), [])
        actual = [item for entries in groups.values() for item in self.coverage(manifest, entries)]
        expected = [('runtime-tests', name) for name in manifest['binaries']['runtime-tests']['tests']
                    if name != 'ignored_child']
        expected += [('integration', name) for name in
                     ('real_http_success', 'real_http_refusal', 'real_http_recovery')]
        expected.append(('new-integration', 'new_case'))
        self.assertCountEqual(actual, expected)
        self.assertEqual(len(actual), len(set(actual)))
        self.assertTrue(all(self.coverage(manifest, entries) for entries in groups.values()))
        self.assertEqual(sum(owner == 'runtime-tests' for owner, _ in actual), 1294)
        self.assertTrue(all(('runtime-tests', name) in actual for name in self.full_sync_cases))
        exclusive = [entry for entries in groups.values() for entry in entries if entry.get('exclusive')]
        self.assertCountEqual([entry['name'] for entry in exclusive], artifacts.ORDINARY_EXCLUSIVE_RUNTIME_CASES)
        self.assertTrue(all(entry.get('batch') is not True for entry in exclusive))
        reordered = copy.deepcopy(manifest)
        reordered['binaries'] = dict(reversed(list(reordered['binaries'].items())))
        for binary in reordered['binaries'].values():
            binary['tests'].reverse()
        for owner, entries in groups.items():
            self.assertEqual(artifacts.ordinary_artifact_entries(reordered, selected, owner), entries)
        manifest['binaries']['runtime-tests']['tests'].append('new_runtime_case')
        updated = [item for owner in groups for item in self.coverage(manifest,
                   artifacts.ordinary_artifact_entries(manifest, selected, owner))]
        self.assertCountEqual(updated, expected + [('runtime-tests', 'new_runtime_case')])

    def test_same_original_producer_features_and_complete_targets_are_required(self):
        selected = self.selection()
        original = {key: value for key, value in selected.items()
                    if key not in ('ordinary_artifact_partition_version', 'ordinary_partitions')}
        self.assertEqual(artifacts.build_variants('ordinary', selected),
                         artifacts.build_variants('ordinary', original))
        self.assertEqual(artifacts.required_native_commands(selected),
                         artifacts.required_native_commands(original))
        command = next(args for name, args, _ in artifacts.build_variants('ordinary', selected)
                       if name == 'required')
        self.assertIn('--tests', command)
        self.assertNotIn('--no-default-features', command)
        self.assertIn('oasis7_chain_runtime/test_tier_required', command)
        for owner in selected['ordinary_partitions']:
            with self.subTest(owner=owner), self.assertRaisesRegex(ValueError, 'cannot rebuild'):
                artifacts.build_variants('ordinary', selected, ordinary_partition=owner)
        self.assertEqual(artifacts.ordinary_partitions(original), ['all'])
        self.assertEqual(artifacts.ordinary_partitions(CompleteArtifactPartitions().selection()),
                         list(artifacts.ORDINARY_ARTIFACT_PARTITIONS))

    def test_capability_rejects_missing_unknown_duplicate_and_incompatible_owners(self):
        changes = [
            ('ordinary_artifact_partition_version', value) for value in (True, None, 0, 3, '2')
        ] + [
            ('ordinary_partitions', []), ('ordinary_partitions', ['all']),
            ('ordinary_partitions', ['controls', 'native-0', 'native-1', 'native-2']),
            ('ordinary_partitions', ['controls', 'native-0', 'native-1', 'native-2', 'native-2']),
            ('ordinary_partitions', ['controls', 'native-0', 'native-1', 'native-2', 'unknown']),
            ('scope', 'full'), ('execution_profile', 'candidate-full'),
            ('test_policy_changed', True), ('test_policy_changed', 0),
            ('oasis7_required_packages', []), ('oasis7_required_packages', None),
            ('groups', ['baseline']), ('fast_execution_version', 1),
            ('ordinary_partition_version', 1), ('ordinary_execution', 'inline'),
            ('ordinary_fast_targets', []), ('ordinary_full_fast_targets', []),
        ]
        for key, value in changes:
            with self.subTest(key=key, value=value), self.assertRaises(ValueError):
                artifacts.ordinary_partitions(self.selection() | {key: value})
        for field in ('ordinary_artifact_partition_version', 'ordinary_partitions', 'fast_plan_version',
                      'test_policy_changed', 'oasis7_required_packages'):
            selected = self.selection()
            del selected[field]
            with self.subTest(missing=field), self.assertRaises(ValueError):
                artifacts.ordinary_partitions(selected)
        for owner in ('all', 'controls', 'runtime', 'unknown', '../native-0'):
            with self.subTest(owner=owner), patch.object(artifacts, 'load') as load:
                with self.assertRaises(ValueError):
                    artifacts.execute(Path('.'), 'ordinary', Path('unused'), plan=self.selection(),
                                      ordinary_partition=owner)
                load.assert_not_called()

    def test_runtime_identity_empty_and_duplicate_inventory_fail_closed(self):
        for change in ('missing', 'package', 'target', 'kind', 'variant', 'duplicate-target',
                       'duplicate-case', 'empty', 'too-small'):
            manifest = self.manifest()
            binary = manifest['binaries']['runtime-tests']
            if change == 'missing':
                del manifest['binaries']['runtime-tests']
            elif change in ('package', 'target', 'variant'):
                binary[change] = 'unrelated'
            elif change == 'kind':
                binary['target_kind'] = ['test']
            elif change == 'duplicate-target':
                manifest['binaries']['second-runtime'] = dict(binary)
            elif change == 'duplicate-case':
                binary['tests'].append(binary['tests'][0])
            else:
                binary['tests'] = [] if change == 'empty' else ['one', 'two', 'three']
                binary['ignored_tests'] = []
            with self.subTest(change=change), self.assertRaises(ValueError):
                artifacts.ordinary_artifact_entries(manifest, self.selection(), 'native-0')

    def test_deferred_cases_stay_deferred_and_unpartitioned_fallback_stays_complete(self):
        selected = self.selection() | dict(formal_full_profiles_only_version=1, formal_shards=[],
            formal_deferred_ordinary_cases={'runtime-tests': sorted(artifacts.ORDINARY_EXCLUSIVE_RUNTIME_CASES)})
        manifest = self.manifest(selected)
        actual = [item for owner in selected['ordinary_partitions'][1:] for item in self.coverage(
                  manifest, artifacts.ordinary_artifact_entries(manifest, selected, owner))]
        expected = self.coverage(manifest, artifacts.ordinary_entries(manifest))
        self.assertCountEqual(actual, expected)
        self.assertFalse(set(name for _, name in actual) & artifacts.ORDINARY_EXCLUSIVE_RUNTIME_CASES)
        calls = []
        with tempfile.TemporaryDirectory() as temporary, \
             patch.object(artifacts, 'load', return_value=(manifest, {})), \
             patch('ci_process_runner.run', side_effect=lambda command, cwd, env, suite, name, *args, **kwargs:
                   calls.append(name) or dict(passed=True, cleanup_result='clean')):
            artifacts.execute(Path(temporary), 'ordinary', Path(temporary) / 'results', plan=selected)
        self.assertCountEqual(calls, ['runtime-tests', 'integration', 'new-integration'])


if __name__ == '__main__':
    unittest.main()
