#!/usr/bin/env python3
"""Coverage, transition-gate and real libtest runner regressions."""
import copy
import importlib.util
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import tarfile
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
plan = module('ci-formal-plan')
from ci_process_runner import run, owned_processes


class Transition(unittest.TestCase):
    def test_old_baseline_can_only_widen(self):
        self.assertTrue(plan.selected(dict(scope='full', groups=['baseline']), ['baseline']))
        for group in plan.CORE:
            self.assertTrue(plan.selected(dict(scope='targeted', groups=['baseline', group]), ['baseline', group]))
        for group in ['viewer_js_required', 'site_quality']:
            self.assertFalse(plan.selected(dict(scope='targeted', groups=['baseline', group]), ['baseline', group]))
        self.assertFalse(plan.selected(dict(scope='full', groups=['baseline']), ['baseline', 'formal_process']))
        self.assertTrue(plan.selected(dict(scope='targeted', groups=['formal_process']), ['formal_process']))

    def test_selected_requires_real_success_and_not_selected_requires_skip(self):
        needs = {name: dict(result='success') for name in ['select', 'formal-test-build', 'formal-process-tests']}
        needs['formal-test-build-trusted'] = dict(result='skipped')
        self.assertEqual(plan.aggregate(True, needs), 'passed')
        for job in ['select', 'formal-test-build', 'formal-process-tests']:
            for state in ['failure', 'cancelled', 'skipped', None]:
                changed = copy.deepcopy(needs)
                changed[job]['result'] = state
                with self.assertRaises(ValueError):
                    plan.aggregate(True, changed)
        for name in ('formal-test-build', 'formal-test-build-trusted'):
            needs[name]['result'] = 'skipped'
        needs['formal-process-tests']['result'] = 'skipped'
        self.assertEqual(plan.aggregate(False, needs), 'not_selected')
        del needs['formal-process-tests']
        with self.assertRaises(ValueError):
            plan.aggregate(False, needs)

    def test_formal_builder_failure_and_skip_combinations(self):
        states = ('success', 'skipped', 'failure', 'cancelled', None)
        for reader in states:
            for writer in states:
                needs = {'select': {'result': 'success'}, 'formal-process-tests': {'result': 'success'},
                         'formal-test-build': {'result': reader}, 'formal-test-build-trusted': {'result': writer}}
                if (reader, writer) in (('success', 'skipped'), ('skipped', 'success')):
                    self.assertEqual(plan.aggregate(True, needs), 'passed')
                else:
                    with self.assertRaises(ValueError):
                        plan.aggregate(True, needs)


class Coverage(unittest.TestCase):
    def setUp(self):
        self.config = json.loads((HERE / 'ci-formal-process-suites.json').read_text())
        self.binaries = {name: dict(tests=['formal::logic'], ignored_tests=[]) for name in self.config['environment_formal_batches']}
        for entries in self.config['suites'].values():
            for entry in entries:
                b = self.binaries[entry['binary']]
                b['tests'].append(entry['name'])
                b['ignored_tests'].append(entry['name'])
        for binary, names in self.config['auxiliary_entries'].items():
            self.binaries[binary]['tests'].extend(names)

    def test_inventory_and_auxiliaries_are_disjoint(self):
        suites = artifacts.resolve_suites(self.config, self.binaries)
        cases = [(e['binary'], e['name']) for entries in suites.values() for e in entries if 'binary' in e]
        self.assertEqual(len(cases), len(set(cases)))
        self.assertEqual(sum(e.get('ignored', False) for entries in suites.values() for e in entries), 29)
        self.assertEqual(sum('script' in e for entries in suites.values() for e in entries), 2)

    def test_rename_duplicate_and_new_unassigned_acceptance_fail(self):
        for mutation in ['rename', 'duplicate', 'missing']:
            config, binaries = copy.deepcopy(self.config), copy.deepcopy(self.binaries)
            entry = config['suites']['world-history'][0]
            if mutation == 'rename':
                entry['name'] += '_renamed'
            elif mutation == 'duplicate':
                config['suites']['runtime-protocol'].append(entry)
            else:
                binaries['runtime-tests']['ignored_tests'].append('new::protected_endpoint_children_new_acceptance')
            with self.assertRaises(ValueError):
                artifacts.resolve_suites(config, binaries)

    def test_physical_shards_preserve_every_logical_case_once(self):
        suites = artifacts.resolve_suites(self.config, self.binaries)
        shards = artifacts.resolve_shards(self.config, suites)
        expected = [(suite, json.dumps(entry, sort_keys=True))
                    for suite, entries in suites.items() for entry in entries]
        actual = [(entry['suite'], json.dumps({k: v for k, v in entry.items() if k != 'suite'}, sort_keys=True))
                  for entries in shards.values() for entry in entries]
        self.assertCountEqual(actual, expected)
        cold = next(entry for entry in shards['storage-recovery'] if 'cold_original_lookup' in entry.get('name', ''))
        operation = next(entry for entry in shards['lifecycle-cli'] if 'operation_cli_owner' in entry.get('name', ''))
        self.assertEqual(cold['suite'], 'healthy-set')
        self.assertEqual(operation['suite'], 'healthy-set')
        self.assertEqual(len(shards['world-history']), 1)
        changed = copy.deepcopy(self.config)
        changed['case_shards']['runtime-tests']['renamed_or_missing_case'] = 'healthy-set'
        with self.assertRaises(ValueError):
            artifacts.resolve_shards(changed, suites)

    def test_trusted_formal_inventory_survives_candidate_removal_and_keeps_additions(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            authority, candidate = root / 'authority.json', root / 'candidate.json'
            authority.write_text(json.dumps(self.config))
            changed = copy.deepcopy(self.config)
            old = changed['suites']['world-history'].pop()
            for key in ('environment_formal_batches', 'shell_tests', 'binary_dependencies',
                        'script_dependencies', 'legacy_ignored_filters', 'auxiliary_entries'):
                changed[key] = {}
            candidate.write_text(json.dumps(changed))
            with patch.object(artifacts, 'CONFIG', candidate), patch.object(artifacts, 'FORMAL_AUTHORITY', authority):
                effective = artifacts.formal_configuration()
                self.assertEqual(effective, self.config)
                artifacts.resolve_suites(effective, self.binaries)
                missing = copy.deepcopy(self.binaries)
                missing[old['binary']]['ignored_tests'].remove(old['name'])
                with self.assertRaisesRegex(ValueError, 'missing ignored acceptance'):
                    artifacts.resolve_suites(effective, missing)
                added = dict(binary='runtime-tests', name='protected_endpoint_children_new_acceptance', ignored=True)
                changed['suites']['runtime-protocol'].append(added)
                candidate.write_text(json.dumps(changed))
                binaries = copy.deepcopy(self.binaries)
                binaries['runtime-tests']['tests'].append(added['name'])
                binaries['runtime-tests']['ignored_tests'].append(added['name'])
                suites = artifacts.resolve_suites(artifacts.formal_configuration(), binaries)
                self.assertIn(old, suites['world-history'])
                self.assertIn(added, suites['runtime-protocol'])
                self.assertEqual(sum(entry.get('ignored', False) for cases in suites.values() for entry in cases), 30)

    def test_candidate_cannot_relabel_formal_case_or_move_it_to_an_unplanned_shard(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            authority, candidate = root / 'authority.json', root / 'candidate.json'
            authority.write_text(json.dumps(self.config))
            changed = copy.deepcopy(self.config)
            entry = changed['suites']['world-history'].pop()
            changed['suites']['runtime-protocol'].append(entry)
            candidate.write_text(json.dumps(changed))
            with patch.object(artifacts, 'CONFIG', candidate), patch.object(artifacts, 'FORMAL_AUTHORITY', authority):
                with self.assertRaisesRegex(ValueError, 'trusted Formal case identity'):
                    artifacts.formal_configuration()
                changed = copy.deepcopy(self.config)
                changed['case_shards'].setdefault(entry['binary'], {})[entry['name']] = 'healthy-set'
                candidate.write_text(json.dumps(changed))
                selected = FastExecutionSelection().selection(suites=('world-history',))
                selected['formal_shards'] = ['world-history']
                with self.assertRaisesRegex(ValueError, 'outside the trusted plan'):
                    artifacts.formal_shards(selected)
                selected['formal_shards'].append('healthy-set')
                self.assertEqual(artifacts.formal_shards(selected), ['healthy-set'])

    def test_formal_bundle_rejects_another_authority_before_files_or_execution(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            authority, candidate = root / 'authority.json', root / 'candidate.json'
            authority.write_text(json.dumps(self.config))
            candidate.write_text(json.dumps(self.config))
            manifest = dict(tested_sha='tested', source_sha='source', producer_run_id='local',
                checkout=str(root), config_sha256=artifacts.digest(candidate), formal_authority_sha256='other',
                rust_toolchain=os.environ.get('RUST_TOOLCHAIN', 'local'),
                runner_image=os.environ.get('ImageOS', sys.platform))
            (root / 'manifest.json').write_text(json.dumps(manifest))
            with patch.object(artifacts, 'ROOT', root), patch.object(artifacts, 'CONFIG', candidate), \
                 patch.object(artifacts, 'FORMAL_AUTHORITY', authority), patch.object(artifacts, 'output', return_value='tested'), \
                 patch.dict(os.environ, SOURCE_SHA='source', GITHUB_RUN_ID='local'):
                with self.assertRaisesRegex(ValueError, 'Formal authority mismatch'):
                    artifacts.pack(root, root / 'unused.tar.gz', shard='world-history')
                with self.assertRaisesRegex(ValueError, 'formal_authority_sha256'):
                    artifacts.load(root, shard='world-history')

    def test_environment_batch_case_cannot_escape_the_trusted_physical_shards(self):
        config = copy.deepcopy(self.config)
        config['case_shards'].setdefault('runtime-tests', {})['formal::logic'] = 'healthy-set'
        selected = FastExecutionSelection().selection(suites=('runtime-protocol',))
        selected['formal_shards'] = ['runtime-protocol']
        manifest = dict(execution_plan=selected, suites=artifacts.resolve_suites(config, self.binaries))
        with self.assertRaisesRegex(ValueError, 'resolved Formal coverage'):
            artifacts.selected_entries(manifest, config, shard='runtime-protocol')


class FastExecutionSelection(unittest.TestCase):
    def selection(self, packages=('oasis7_viewer_codec',), suites=()):
        return dict(fast_plan_version=1, execution_profile='pr-fast', scope='targeted',
                    groups=['baseline', 'oasis7_required'] + (['formal_process'] if suites else []),
                    oasis7_required_packages=list(packages),
                    oasis7_required_wasm_packages=[p for p in packages if p in artifacts.PORTABLE_REQUIRED_PACKAGES],
                    formal_suites=list(suites), test_policy_changed=False,
                    architecture=dict(source='source', tested='tested'))

    def test_leaf_package_does_not_build_unrelated_harnesses_or_viewer(self):
        variants = artifacts.build_variants('ordinary', self.selection())
        self.assertEqual([name for name, _, _ in variants], ['required'])
        command = variants[0][1]
        self.assertEqual([command[i + 1] for i, arg in enumerate(command) if arg == '-p'], ['oasis7_viewer_codec'])
        self.assertIn('--tests', command)
        self.assertIn('--no-run', command)
        self.assertEqual(command[command.index('--features') + 1], 'oasis7_viewer_codec/test_tier_required')
        self.assertEqual(artifacts.required_arguments(self.selection(), 'wasm'),
                         ['-p', 'oasis7_viewer_codec', '--lib', '--target', 'wasm32-unknown-unknown', '--locked'])

    def test_process_consumers_keep_viewer_and_only_applicable_default_regressions(self):
        plan = self.selection(('oasis7_chain_runtime', 'oasis7_viewer_runtime'))
        variants = artifacts.build_variants('ordinary', plan)
        self.assertEqual({name for name, _, _ in variants}, {'required', 'viewer', 'viewer-snapshot-tests'})
        manifest = dict(execution_plan=plan)
        self.assertEqual(artifacts.snapshot_regressions(manifest), (artifacts.DEFAULT_SNAPSHOT_REGRESSIONS[0],))
        core = self.selection(('oasis7',))
        self.assertEqual(artifacts.snapshot_regressions(dict(execution_plan=core)),
                         (artifacts.DEFAULT_SNAPSHOT_REGRESSIONS[1],))

    def test_old_planner_and_uncertain_scope_cannot_narrow_old_inventory(self):
        old = dict(groups=['oasis7_required'], scope='targeted')
        self.assertEqual(artifacts.build_variants('ordinary', old), artifacts.build_variants('ordinary'))
        plan = self.selection()
        plan['scope'] = 'full'
        self.assertEqual(set(artifacts.required_packages(plan)), set(artifacts.required_packages()))
        self.assertEqual(artifacts.required_wasm_packages(plan), list(artifacts.PORTABLE_REQUIRED_PACKAGES))

    def test_changed_test_policy_includes_expanded_features_before_merge(self):
        plan = self.selection(('oasis7_runtime', 'oasis7_viewer_codec'))
        plan['test_policy_changed'] = True
        features = artifacts.package_features(plan['oasis7_required_packages'], plan)
        self.assertIn('oasis7_runtime/test_tier_full', features)
        self.assertIn('oasis7_runtime/test_tier_required', features)
        self.assertIn('oasis7_viewer_codec/test_tier_required', features)
        self.assertNotIn('oasis7_viewer_codec/test_tier_full', features)

    def test_invalid_or_missing_package_selections_fail_before_build(self):
        for key, bad in [('fast_plan_version', 2), ('fast_plan_version', True),
                         ('oasis7_required_packages', []), ('oasis7_required_packages', ['unknown']),
                         ('oasis7_required_packages', ['oasis7', 'oasis7']),
                         ('groups', ['baseline']), ('test_policy_changed', 'false')]:
            plan = self.selection()
            plan[key] = bad
            with self.subTest(key=key, bad=bad), self.assertRaises(ValueError):
                artifacts.build_variants('ordinary', plan)
        plan = self.selection()
        del plan['oasis7_required_wasm_packages']
        with self.assertRaises(ValueError):
            artifacts.required_arguments(plan, 'wasm')

    def test_case_shard_overrides_do_not_change_logical_selection(self):
        plan = self.selection(suites=('healthy-set',))
        self.assertEqual(artifacts.formal_shards(plan), ['healthy-set', 'storage-recovery', 'lifecycle-cli'])
        config = json.loads(artifacts.CONFIG.read_text())
        manifest = dict(execution_plan=plan, suites=config['suites'])
        entries = artifacts.selected_entries(manifest, config, shard='storage-recovery')
        self.assertEqual(len(entries), 1)
        self.assertEqual(entries[0]['suite'], 'healthy-set')
        self.assertIn('cold_original_lookup', entries[0]['name'])
        self.assertEqual(artifacts.selected_entries(manifest, config, shard='runtime-protocol'), [])
        with self.assertRaises(ValueError):
            artifacts.selected_entries(manifest, config, suite='storage-recovery')

    def test_source_and_tested_merge_are_bound_separately(self):
        plan = self.selection()
        with patch.object(artifacts, 'output', return_value='tested'), patch.dict(os.environ, SOURCE_SHA='source'):
            artifacts.validate_plan_identity(plan)
            for key in ('source', 'tested'):
                altered = copy.deepcopy(plan)
                altered['architecture'][key] = 'other'
                with self.subTest(key=key), self.assertRaisesRegex(ValueError, 'identity mismatch'):
                    artifacts.validate_plan_identity(altered)
                altered[key + '_sha'] = 'source' if key == 'source' else 'tested'
                with self.subTest(key=key, contradictory=True), self.assertRaisesRegex(ValueError, 'identity mismatch'):
                    artifacts.validate_plan_identity(altered)

    def test_subset_artifact_rejects_another_consumer_plan_before_files(self):
        plan = self.selection()
        other = self.selection(('oasis7_runtime',))
        with tempfile.TemporaryDirectory() as temp:
            bundle = Path(temp)
            (bundle / 'manifest.json').write_text(json.dumps(dict(execution_plan=plan)))
            with self.assertRaisesRegex(ValueError, 'execution plan mismatch'):
                artifacts.pack(bundle, bundle / 'archive.tar.gz', suite='ordinary', plan=other)

    def test_selected_package_or_behavior_cannot_disappear_from_artifact(self):
        plan = self.selection()
        binary = dict(ordinary=True, package='oasis7_viewer_codec', tests=['round_trip'], ignored_tests=[])
        manifest = dict(scope='ordinary', execution_plan=plan, binaries={'codec': binary})
        artifacts.validate_ordinary_selection(manifest)
        for mutation in ('missing', 'other', 'no_behavior'):
            changed = copy.deepcopy(manifest)
            if mutation == 'missing':
                changed['binaries'].clear()
            elif mutation == 'other':
                changed['binaries']['codec']['package'] = 'unselected'
            else:
                changed['binaries']['codec']['ignored_tests'] = ['round_trip']
            with self.subTest(mutation=mutation), self.assertRaises(ValueError):
                artifacts.validate_ordinary_selection(changed)

    def test_selected_fast_bundle_runs_behavior_and_rejects_zero_execution(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            executable = root / 'fixture'
            manifest = dict(scope='ordinary', execution_plan=self.selection(), binaries={
                'codec': dict(path='fixture', ordinary=True, test=True, cwd='.',
                              package='oasis7_viewer_codec', tests=['round_trip'], ignored_tests=[])})
            for count in (1, 0):
                executable.write_text('#!' + sys.executable + '\n'
                                      'print("test result: ok. ' + str(count) +
                                      ' passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s")\n')
                executable.chmod(0o755)
                results = root / ('results-' + str(count) + '.jsonl')
                # Process ownership has its own real-process suite below. This
                # fixture exercises dispatch and libtest count propagation.
                with patch.object(artifacts, 'ROOT', root), patch.object(artifacts, 'load', return_value=(manifest, dict(os.environ))), \
                     patch('ci_process_runner.cleanup', return_value=('clean', False)):
                    if count:
                        artifacts.execute(root, 'ordinary', results, plan=self.selection())
                    else:
                        with self.assertRaises(RuntimeError):
                            artifacts.execute(root, 'ordinary', results, plan=self.selection())
                records = [json.loads(line) for line in results.read_text().splitlines()]
                self.assertEqual(len(records), 1)
                self.assertEqual(records[0]['passed'], count == 1)
                self.assertEqual(records[0]['suite'], 'ordinary')


class FastTargetInventory(unittest.TestCase):
    def target(self, **changes):
        return dict(package='oasis7_viewer_codec', target_kind='test', target_name='pr_fast',
                    default_features=False, features=['test_tier_required'], cases=['required::round_trip']) | changes

    def selection(self, targets=None, full=False):
        selected = FastExecutionSelection().selection(('oasis7_viewer_codec', 'oasis7_runtime'))
        key = 'ordinary_full_fast_targets' if full else 'ordinary_fast_targets'
        selected['full_fast_test_inventory_version' if full else 'fast_test_inventory_version'] = 1
        selected[key] = [self.target()] if targets is None else targets
        if full:
            selected.update(execution_profile='periodic-full', scope='explicit-full')
        return selected

    def binary(self, target=None, variant='required-fast-0'):
        target = target or self.target()
        return dict(ordinary=True, test=True, package=target['package'], target=target['target_name'],
                    target_kind=[target['target_kind']], default_features=target['default_features'],
                    requested_features=target['features'], features=target['features'],
                    tests=target['cases'] + ['new_regression'], ignored_tests=[], variant=variant)

    def test_explicit_targets_select_before_compilation_and_match_clippy(self):
        selected = self.selection([self.target(), self.target(package='oasis7_runtime',
                                  target_kind='lib', target_name='oasis7_runtime')])
        commands = artifacts.required_native_commands(selected)
        self.assertEqual(commands, [
            ['-p', 'oasis7_viewer_codec', '--test', 'pr_fast', '--no-default-features', '--features', 'test_tier_required'],
            ['-p', 'oasis7_runtime', '--lib', '--no-default-features', '--features', 'test_tier_required'],
        ])
        variants = artifacts.build_variants('ordinary', selected)
        self.assertEqual([command for _, command, _ in variants], [['test', *args, '--no-run'] for args in commands])
        self.assertEqual(artifacts.snapshot_regressions(dict(execution_plan=selected)), ())
        viewer = self.selection([self.target(package='oasis7_viewer_runtime')])
        viewer['oasis7_required_packages'] = ['oasis7_viewer_runtime']
        self.assertEqual(len(artifacts.build_variants('ordinary', viewer)), 1)
        manifest = dict(execution_plan=viewer, binaries={'selected': self.binary(viewer['ordinary_fast_targets'][0])})
        self.assertEqual(artifacts.dependency_closure(manifest, {}, suite='ordinary'), {'selected'})

    def test_native_commands_cli_emits_json_for_both_fast_profiles(self):
        sha = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=artifacts.ROOT, text=True).strip()
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / 'plan.json'
            for profile in ('pr-fast', 'main-fast'):
                selected = self.selection()
                selected.update(execution_profile=profile, architecture=dict(source=sha, tested=sha))
                path.write_text(json.dumps(selected))
                command = [sys.executable, str(HERE / 'ci-test-artifacts.py'), 'required-args',
                           '--repo-root', str(artifacts.ROOT), '--plan', str(path)]
                result = subprocess.run([*command, '--phase', 'native-commands'], check=True,
                                        text=True, capture_output=True, env=dict(os.environ, SOURCE_SHA=sha))
                self.assertEqual(json.loads(result.stdout), [['-p', 'oasis7_viewer_codec', '--test', 'pr_fast',
                                 '--no-default-features', '--features', 'test_tier_required']])
                legacy = subprocess.run([*command, '--phase', 'packages'], check=True,
                                        text=True, capture_output=True, env=dict(os.environ, SOURCE_SHA=sha))
                self.assertEqual(legacy.stdout.splitlines(), ['oasis7_viewer_codec', 'oasis7_runtime'])

    def test_old_full_and_policy_plans_keep_original_inventory(self):
        ordinary = self.selection()
        for changes in ({'scope': 'full'}, {'scope': 'explicit-full'}, {'test_policy_changed': True},
                        {'execution_profile': 'periodic-full'}, {'execution_profile': 'candidate-full'}):
            selected = ordinary | changes
            # Inactive capability data cannot break or narrow the fallback.
            selected.update(fast_test_inventory_version=True, ordinary_fast_targets='invalid')
            with self.subTest(changes=changes):
                self.assertIsNone(artifacts.ordinary_fast_targets(selected))
                required = next(args for name, args, _ in artifacts.build_variants('ordinary', selected) if name == 'required')
                self.assertIn('--tests', required)
                self.assertEqual(len(artifacts.required_native_commands(selected)), 1)
        legacy = copy.deepcopy(ordinary)
        del legacy['fast_test_inventory_version']
        self.assertIsNone(artifacts.ordinary_fast_targets(legacy))
        self.assertIn('--tests', artifacts.required_native_commands(legacy)[0])
        self.assertEqual(artifacts.required_native_commands(None), [artifacts.required_arguments(None)])

    def test_invalid_target_inventory_fails_before_build(self):
        for key, value in [('fast_test_inventory_version', True), ('fast_test_inventory_version', 2),
                           ('ordinary_fast_targets', []), ('ordinary_fast_targets', [self.target(), self.target()])]:
            with self.subTest(key=key, value=value), self.assertRaises(ValueError):
                artifacts.build_variants('ordinary', self.selection() | {key: value})
        for changes in (dict(package='unselected'), dict(target_kind='bin'), dict(target_name='../escape'),
                        dict(default_features='false'), dict(features=['default']), dict(features=['x/y']),
                        dict(features=['repeated', 'repeated']), dict(cases=[]), dict(cases=['x\ny']),
                        dict(cases=['repeated', 'repeated']), dict(extra=True)):
            with self.subTest(changes=changes), self.assertRaises(ValueError):
                artifacts.build_variants('ordinary', self.selection([self.target(**changes)]))

    def test_minimum_cases_target_and_actual_features_are_verified(self):
        manifest = dict(scope='ordinary', execution_plan=self.selection(), binaries={'fast': self.binary()})
        artifacts.validate_ordinary_selection(manifest)
        for change in (dict(target='renamed'), dict(target_kind=['lib']), dict(test=False),
                       dict(requested_features=[]), dict(features=[]), dict(features=['default', 'test_tier_required']),
                       dict(default_features=True), dict(tests=['new_regression']),
                       dict(ignored_tests=['required::round_trip'])):
            altered = copy.deepcopy(manifest)
            altered['binaries']['fast'].update(change)
            with self.subTest(change=change), self.assertRaises(ValueError):
                artifacts.validate_ordinary_selection(altered)
        altered = copy.deepcopy(manifest)
        altered['binaries']['duplicate'] = self.binary()
        with self.assertRaisesRegex(ValueError, 'target inventory'):
            artifacts.validate_ordinary_selection(altered)

    def test_same_package_lib_and_test_have_distinct_artifact_names(self):
        lib = artifacts.artifact_logical_name('required-fast-0', 'oasis7_runtime',
                                            dict(name='oasis7_runtime', kind=['lib']), True)
        test = artifacts.artifact_logical_name('required-fast-1', 'oasis7_runtime',
                                             dict(name='oasis7_runtime', kind=['test']), True)
        self.assertNotEqual(lib, test)
        full = artifacts.artifact_logical_name('required-full-fast-0', 'oasis7_runtime',
                                             dict(name='oasis7_runtime', kind=['lib']), True)
        self.assertNotEqual(lib, full)

    def test_full_variants_preserve_originals_and_append_exact_fast_commands(self):
        selected = self.selection(full=True)
        selected.update(fast_test_inventory_version='invalid', ordinary_fast_targets='invalid')
        legacy = copy.deepcopy(selected)
        del legacy['full_fast_test_inventory_version']
        for scope in ('ordinary', 'all'):
            full_variants = artifacts.build_variants(scope, selected)
            original = artifacts.build_variants(scope, legacy)
            self.assertEqual(full_variants[:-1], original)
            self.assertEqual(full_variants[-1][1], ['test', '-p', 'oasis7_viewer_codec', '--test', 'pr_fast',
                                                   '--no-default-features', '--features', 'test_tier_required', '--no-run'])
        self.assertEqual(artifacts.required_native_commands(selected), [artifacts.required_arguments(selected),
                         ['-p', 'oasis7_viewer_codec', '--test', 'pr_fast', '--no-default-features', '--features', 'test_tier_required']])
        for profile in ('periodic-full', 'candidate-full'):
            for key, value in [('full_fast_test_inventory_version', True), ('full_fast_test_inventory_version', 2),
                               ('ordinary_full_fast_targets', [])]:
                with self.subTest(profile=profile, key=key), self.assertRaises(ValueError):
                    artifacts.build_variants('ordinary', selected | {'execution_profile': profile, key: value})

    def test_full_supplements_cannot_replace_original_required_inventory(self):
        selected = self.selection(full=True)
        originals = {package: dict(ordinary=True, test=True, package=package, tests=['old'], ignored_tests=[], variant='required')
                     for package in artifacts.required_packages()}
        manifest = dict(scope='ordinary', execution_plan=selected,
                        binaries=originals | {'supplement': self.binary(variant='required-full-fast-0')})
        for scope in ('ordinary', 'all'):
            manifest['scope'] = scope
            artifacts.validate_ordinary_selection(manifest)
            for missing in ('oasis7_viewer_codec', 'supplement'):
                altered = copy.deepcopy(manifest)
                del altered['binaries'][missing]
                with self.subTest(scope=scope, missing=missing), self.assertRaises(ValueError):
                    artifacts.validate_ordinary_selection(altered)


class InlineExecutionSelection(unittest.TestCase):
    def selection(self, checks=None):
        return FastTargetInventory().selection() | {
            'fast_execution_version': 1, 'ordinary_execution': 'inline',
            'rust_baseline_checks': [] if checks is None else checks,
        }

    def test_only_compatible_bounded_plans_select_baseline_extras(self):
        for profile in ('pr-fast', 'main-fast'):
            selected = self.selection() | {'execution_profile': profile}
            self.assertEqual(artifacts.required_arguments(selected, 'baseline-checks'), [])
            self.assertEqual(artifacts.required_native_commands(selected),
                             artifacts.required_native_commands(FastTargetInventory().selection()))
        selected = self.selection(['newapi_accounting'])
        self.assertEqual(artifacts.required_arguments(selected, 'baseline-checks'), ['newapi_accounting'])
        for selected in (None, {}, FastTargetInventory().selection(),
                         self.selection() | {'fast_execution_version': 2}):
            with self.subTest(legacy=selected):
                self.assertEqual(artifacts.required_arguments(selected, 'baseline-checks'),
                                 list(artifacts.RUST_BASELINE_CHECKS))

    def test_known_inline_capability_rejects_incomplete_or_expanded_plans(self):
        for version in (True, '1', 1.0, None):
            with self.subTest(legacy_version=version), self.assertRaises(ValueError):
                artifacts.required_arguments({'fast_execution_version': version}, 'baseline-checks')
        invalid = [('fast_execution_version', True), ('fast_execution_version', '1'),
                   ('ordinary_execution', 'artifact'), ('execution_profile', 'periodic-full'),
                   ('execution_profile', 'candidate-full'), ('scope', 'full'), ('scope', 'explicit-full'),
                   ('test_policy_changed', True), ('groups', ['baseline']),
                   ('fast_test_inventory_version', True), ('fast_test_inventory_version', 2),
                   ('ordinary_fast_targets', []), ('rust_baseline_checks', 'newapi_accounting'),
                   ('rust_baseline_checks', ['unknown']),
                   ('rust_baseline_checks', ['newapi_accounting', 'newapi_accounting'])]
        for key, value in invalid:
            for phase in ('baseline-checks', 'native', 'wasm', 'clippy-group'):
                with self.subTest(field=key, phase=phase), self.assertRaises(ValueError):
                    artifacts.required_arguments(self.selection() | {key: value}, phase)
        for key in ('fast_plan_version', 'fast_test_inventory_version', 'ordinary_execution',
                    'ordinary_fast_targets', 'rust_baseline_checks'):
            selected = self.selection()
            del selected[key]
            with self.subTest(missing=key), self.assertRaises(ValueError):
                artifacts.required_arguments(selected, 'baseline-checks')
        selected = self.selection()
        selected['ordinary_fast_targets'][0]['cases'] = []
        with self.assertRaises(ValueError):
            artifacts.required_arguments(selected, 'baseline-checks')

    def test_clippy_owner_requires_the_complete_known_inline_plan(self):
        selected = self.selection() | {'ordinary_clippy_group': 'rust_baseline',
                                      'groups': ['baseline', 'rust_baseline', 'oasis7_required']}
        self.assertEqual(artifacts.required_arguments(selected, 'clippy-group'), ['rust_baseline'])
        for legacy in (None, {}, self.selection(), selected | {'fast_execution_version': 2},
                       FastTargetInventory().selection(full=True)):
            with self.subTest(legacy=legacy):
                self.assertEqual(artifacts.required_arguments(legacy, 'clippy-group'), ['oasis7_required'])
        invalid = [('ordinary_clippy_group', None), ('ordinary_clippy_group', ['rust_baseline']),
                   ('ordinary_clippy_group', 'other'), ('groups', ['baseline', 'oasis7_required']),
                   ('groups', ['baseline', 'rust_baseline']), ('ordinary_execution', 'artifact'),
                   ('execution_profile', 'periodic-full'), ('scope', 'full')]
        for key, value in invalid:
            for phase in ('clippy-group', 'native', 'baseline-checks'):
                with self.subTest(field=key, value=value, phase=phase), self.assertRaises(ValueError):
                    artifacts.required_arguments(selected | {key: value}, phase)
        checker = module('ci-required-result')
        jobs = {group: group.replace('_', '-') for group in selected['groups']}
        successful = {name: {'result': 'success'} for name in ['select', *jobs.values()]}
        self.assertTrue(checker.validate(selected, successful, jobs))
        for failed in ('failure', 'cancelled', 'skipped', None):
            with self.subTest(clippy_owner_status=failed), self.assertRaisesRegex(ValueError, 'rust_baseline'):
                checker.validate(selected, successful | {'rust-baseline': {'result': failed}}, jobs)


class DriverCoverage(unittest.TestCase):
    def test_whole_cli_assigns_exact_strict_clippy_commands_once_and_preserves_failures(self):
        # Keep the actual entrypoint, trusted-plan parser and command builder.
        # Stub only unrelated baseline executables and the expensive Cargo process.
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            repo, binary = root / 'repo', root / 'bin'
            scripts = repo / 'scripts'
            scripts.mkdir(parents=True)
            binary.mkdir()
            (scripts / 'cargo-dev-architecture.py').write_text('print("architecture fixture")\n')
            for name in ('check-rustsec-ignore-baseline.test.sh', 'check-rustsec-ignore-baseline.sh',
                         'ensure-cargo-deny.sh', 'check-rust-file-size.test.sh', 'check-rust-file-size.sh',
                         'check-standalone-tool-lockfiles.test.sh', 'check-standalone-tool-lockfiles.sh'):
                path = scripts / name
                path.write_text('#!/usr/bin/env bash\nexit 0\n')
                path.chmod(0o755)
            cargo = binary / 'cargo'
            cargo.write_text('''#!/usr/bin/env python3
import json, os, sys
args = sys.argv[1:]
with open(os.environ['CARGO_LOG'], 'a') as stream:
    stream.write(json.dumps(args) + '\\n')
if args[0] == 'clippy' and args[args.index('-p') + 1] == os.environ['FAIL_CLIPPY_PACKAGE']:
    sys.exit(19)
''')
            cargo.chmod(0o755)
            subprocess.run(['git', 'init', '-q', str(repo)], check=True)
            subprocess.run(['git', '-C', str(repo), 'add', '.'], check=True)
            subprocess.run(['git', '-C', str(repo), '-c', 'user.name=CI fixture', '-c',
                            'user.email=fixture@example.invalid', 'commit', '-qm', 'fixture'], check=True)
            sha = subprocess.check_output(['git', '-C', str(repo), 'rev-parse', 'HEAD'], text=True).strip()
            selected = InlineExecutionSelection().selection() | {
                'source_sha': sha, 'tested_sha': sha, 'architecture': {'source': sha, 'tested': sha},
                'groups': ['baseline', 'rust_baseline', 'oasis7_required'],
                'ordinary_clippy_group': 'rust_baseline', 'oasis7_required_wasm_packages': [],
                'ordinary_fast_targets': [FastTargetInventory().target(),
                    FastTargetInventory().target(package='oasis7_runtime', target_kind='lib', target_name='oasis7_runtime')]}
            path = root / 'selection.json'
            log = root / 'cargo.jsonl'
            expected = [['clippy', *command, '--', '-D', 'warnings', '-D', 'clippy::correctness',
                         '-D', 'clippy::suspicious'] for command in artifacts.required_native_commands(selected)]
            for mode, failed in (('parallel', ''), ('legacy-inline', ''), ('unknown-version', ''),
                                 ('parallel', 'oasis7_viewer_codec'), ('parallel', 'oasis7_runtime')):
                current = dict(selected)
                if mode == 'legacy-inline':
                    del current['ordinary_clippy_group']
                elif mode == 'unknown-version':
                    current['fast_execution_version'] = 2
                path.write_text(json.dumps(current))
                lint_calls = {}
                for group in ('rust_baseline', 'oasis7_required'):
                    log.unlink(missing_ok=True)
                    result = subprocess.run(['bash', str(HERE / 'ci-tests.sh'), 'required', '--group', group,
                                             '--plan', str(path), '--repo-root', str(repo)], text=True, capture_output=True,
                        env=dict(os.environ, PATH=str(binary) + os.pathsep + os.environ['PATH'],
                                 SOURCE_SHA=sha, CI_VERBOSE='', CARGO_LOG=str(log), FAIL_CLIPPY_PACKAGE=failed,
                                 OASIS7_CI_TEST_BUNDLE=str(root / 'missing-native-bundle'),
                                 OASIS7_CI_RUN_PROVIDER_LIVE_GATE='false', OASIS7_CI_RUN_HOSTED_ACCOUNT_SMOKE='false'))
                    commands = [json.loads(line) for line in log.read_text().splitlines()] if log.exists() else []
                    lint_calls[group] = [args for args in commands if args[0] == 'clippy']
                    with self.subTest(mode=mode, failed=failed, group=group):
                        if group == 'rust_baseline':
                            self.assertEqual(result.returncode, 19 if failed else 0, result.stdout + result.stderr)
                        else:
                            # Reaching the real artifact reader proves lint deferral
                            # never skips the test consumer or accepts absent tests.
                            self.assertNotEqual(result.returncode, 0)
                            self.assertIn('missing-native-bundle/manifest.json', result.stderr)
                owner = 'rust_baseline' if mode == 'parallel' else 'oasis7_required'
                expected_calls = expected[:1] if failed == 'oasis7_viewer_codec' else expected
                self.assertEqual(lint_calls[owner], expected_calls, (mode, failed, lint_calls))
                self.assertEqual(lint_calls['oasis7_required' if owner == 'rust_baseline' else 'rust_baseline'], [])

    def test_whole_driver_cli_accepts_baseline_plan_and_rejects_invalid_inputs_before_cargo(self):
        sha = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=artifacts.ROOT, text=True).strip()
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            cargo = root / 'cargo'
            cargo.write_text('#!/usr/bin/env bash\nprintf "cargo-sentinel:%s\\n" "$*"\nexit 73\n')
            cargo.chmod(0o755)
            selected = InlineExecutionSelection().selection() | {
                'source_sha': sha, 'tested_sha': sha, 'architecture': {'source': sha, 'tested': sha}}
            legacy = dict(selected)
            del legacy['fast_execution_version']
            cases = [('required', 'rust_baseline', selected, 73, 'cargo-sentinel:fetch --locked'),
                     ('required', 'rust_baseline', legacy, 73, 'cargo-sentinel:fetch --locked'),
                     ('required', 'rust_baseline', selected | {'rust_baseline_checks': ['unknown']},
                      1, 'invalid execution selection: rust_baseline_checks'),
                     ('required', 'rust_baseline', selected | {'tested_sha': '0' * 40},
                      1, 'execution plan identity mismatch: tested'),
                     ('required', 'baseline', selected, 2, '--plan requires required --group'),
                     ('full', 'rust_baseline', selected, 2, '--plan requires required --group')]
            path = root / 'selection.json'
            for tier, group, plan, code, diagnostic in cases:
                path.write_text(json.dumps(plan))
                result = subprocess.run(['bash', str(HERE / 'ci-tests.sh'), tier, '--group', group,
                                         '--plan', str(path), '--repo-root', str(artifacts.ROOT)],
                                        text=True, capture_output=True,
                                        env=dict(os.environ, PATH=str(root) + os.pathsep + os.environ['PATH'],
                                                 SOURCE_SHA=sha, CI_VERBOSE='', OASIS7_CI_TEST_BUNDLE=''))
                with self.subTest(tier=tier, group=group, expected=diagnostic):
                    self.assertEqual(result.returncode, code, result.stdout + result.stderr)
                    self.assertIn(diagnostic, result.stdout + result.stderr)
                    self.assertEqual('cargo-sentinel:' in result.stdout, code == 73)

    def test_inline_driver_builds_once_runs_manifest_and_propagates_each_failure(self):
        driver = (HERE / 'ci-tests.sh').read_text()
        body = dict(re.findall(r'^(\w+)\(\) \{\n(.*?)^\}', driver, re.M | re.S))['run_oasis7_required_tier_tests']
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            helper = root / 'ci-test-artifacts.py'
            helper.write_text('''import json, os, pathlib, sys
action = sys.argv[1]
if action == 'required-args':
    sys.exit(0)
bundle = pathlib.Path(sys.argv[sys.argv.index('--bundle') + 1])
with open(os.environ['CALL_LOG'], 'a') as stream:
    stream.write(json.dumps({'args': sys.argv[1:], 'bundle': str(bundle)}) + '\\n')
if action == 'build':
    bundle.mkdir(parents=True)
if os.environ['FAILED'] == action:
    sys.exit(19)
''')
            for failed in ('', 'build', 'run'):
                log = root / 'calls.jsonl'
                log.unlink(missing_ok=True)
                script = 'set -euo pipefail\nci_python="$1"; driver_dir="$2"; repo_root="$2"; required_plan=fixture\n'
                script += 'run() { "$@"; }\nrun_cargo() { echo unexpected-Cargo; return 83; }\n'
                script += 'run_oasis7_required_tier_tests() {\n' + body + '}\nrun_oasis7_required_tier_tests\n'
                result = subprocess.run(['bash', '-c', script, 'fixture', sys.executable, str(root)],
                                        text=True, capture_output=True,
                                        env=dict(os.environ, FAILED=failed, CALL_LOG=str(log), OASIS7_CI_TEST_BUNDLE=''))
                calls = [json.loads(line) for line in log.read_text().splitlines()]
                with self.subTest(failed=failed):
                    self.assertEqual(result.returncode, 19 if failed else 0, result.stderr)
                    self.assertEqual([call['args'][0] for call in calls], ['build'] if failed == 'build' else ['build', 'run'])
                    self.assertEqual(calls[0]['args'][calls[0]['args'].index('--scope') + 1], 'ordinary')
                    self.assertEqual(len({call['bundle'] for call in calls}), 1)
                    self.assertFalse(Path(calls[0]['bundle']).exists())

    def test_baseline_executes_selected_real_extra_commands_and_propagates_failure(self):
        driver = (HERE / 'ci-tests.sh').read_text()
        bodies = dict(re.findall(r'^(\w+)\(\) \{\n(.*?)^\}', driver, re.M | re.S))
        body = bodies['run_rust_baseline']
        sha = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=artifacts.ROOT, text=True).strip()
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / 'selection.json'
            for checks, failed in (([], ''), (['newapi_accounting'], ''),
                                   (list(artifacts.RUST_BASELINE_CHECKS), ''),
                                   (list(artifacts.RUST_BASELINE_CHECKS), 'newapi_accounting'),
                                   (list(artifacts.RUST_BASELINE_CHECKS), 'standalone_tool_lockfiles'),
                                   (None, '')):
                selected = InlineExecutionSelection().selection(checks)
                selected.update(source_sha=sha, tested_sha=sha, architecture=dict(source=sha, tested=sha))
                if checks is None:
                    del selected['fast_execution_version']
                path.write_text(json.dumps(selected))
                script = 'set -euo pipefail\ntier=required; ci_python="$1"; driver_dir="$2"; repo_root="$3"; required_plan="$4"\n'
                script += 'run() { printf "command:%s\\n" "$*"; }\nrun_cargo() { printf "cargo:%s\\n" "$*"; }\n'
                script += 'run_rustsec_advisory_check() { echo advisories; }\n'
                script += 'run_newapi_bridge_service_accounting_tests() { echo newapi_accounting; [[ "$FAILED" != newapi_accounting ]] || return 19; }\n'
                script += 'run_standalone_tool_lockfiles_checks() { echo standalone_tool_lockfiles; [[ "$FAILED" != standalone_tool_lockfiles ]] || return 19; }\n'
                script += 'run_oasis7_required_clippy_for_group() {\n' + bodies['run_oasis7_required_clippy_for_group'] + '}\n'
                script += 'run_rust_baseline() {\n' + body + '}\nrun_rust_baseline\n'
                result = subprocess.run(['bash', '-c', script, 'fixture', sys.executable, str(HERE),
                                         str(artifacts.ROOT), str(path)], text=True, capture_output=True,
                                        env=dict(os.environ, FAILED=failed, SOURCE_SHA=sha))
                expected = list(artifacts.RUST_BASELINE_CHECKS) if checks is None else checks
                with self.subTest(checks=checks, failed=failed):
                    self.assertEqual(result.returncode, 19 if failed else 0, result.stderr)
                    self.assertIn('cargo:fetch --locked', result.stdout)
                    self.assertIn('cargo-dev-architecture.py', result.stdout)
                    self.assertIn('advisories', result.stdout)
                    self.assertEqual('newapi_accounting' in result.stdout.splitlines(), 'newapi_accounting' in expected)
                    self.assertEqual('standalone_tool_lockfiles' in result.stdout.splitlines(),
                                     'standalone_tool_lockfiles' in expected and failed != 'newapi_accounting')

    def test_target_clippy_executes_every_command_and_propagates_each_failure(self):
        driver = (HERE / 'ci-tests.sh').read_text()
        body = dict(re.findall(r'^(\w+)\(\) \{\n(.*?)^\}', driver, re.M | re.S))['run_oasis7_required_tier_clippy']
        commands = [['-p', 'first', '--test', 'smoke', '--no-default-features'], ['-p', 'second', '--lib']]
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / 'ci-test-artifacts.py').write_text('import json\nprint(json.dumps(' + repr(commands) + '))\n')
            for fail in ('none', 'first', 'second'):
                script = 'set -euo pipefail\nci_python="$1"; driver_dir="$2"; repo_root="$2"; required_plan="fixture"\n'
                script += 'run_cargo_clippy() { printf "%s\\n" "$*"; [[ "$2" != "$FAIL_TARGET" ]] || return 19; }\n'
                script += 'run_oasis7_required_tier_clippy() {\n' + body + '}\nrun_oasis7_required_tier_clippy\n'
                result = subprocess.run(['bash', '-c', script, 'fixture', sys.executable, str(root)],
                                        text=True, capture_output=True, env=dict(os.environ, FAIL_TARGET=fail))
                with self.subTest(fail=fail):
                    self.assertEqual(result.returncode, 0 if fail == 'none' else 19, result.stderr)
                    self.assertIn('-p first --test smoke --no-default-features', result.stdout)
                    self.assertEqual('-p second --lib' in result.stdout, fail != 'first')

    def invoke(self, commands, fail_browser=False):
        driver = (HERE / 'ci-tests.sh').read_text()
        functions = dict(re.findall(r'^(\w+)\(\) \{\n(.*?)^\}', driver, re.M | re.S))
        retained = {'run_group', 'run_full_required_superset', 'run_all_required_gate_capability_contract_tests',
                    'run_selected_workspace_support', 'run_oasis7_required_clippy_for_group'}
        definitions = []
        for name, body in functions.items():
            if name in retained:
                definitions.append(name + '() {\n' + body + '}\n')
            elif name not in ('run', 'run_cargo'):
                definitions.append(name + '() { printf "%s\\n" ' + name + '; }\n')
        script = 'set -euo pipefail\ntier=required; support_profiles=""; required_plan=""\n'
        script += '\n'.join(definitions)
        script += 'run_cargo() { printf "cargo:%s\\n" "$*"; }\n'
        script += 'run() { printf "command:%s\\n" "$*"; [[ "${FAIL_BROWSER:-}" != 1 || "$*" != *viewer-auth-browser-security-smoke.mjs* ]] || return 17; }\n'
        script += commands
        return subprocess.run(['bash', '-c', script], text=True, capture_output=True,
                              env=dict(os.environ, FAIL_BROWSER='1' if fail_browser else '0'))

    def test_full_driver_contains_every_locally_supported_required_group(self):
        groups = ['baseline', 'rust_baseline', 'oasis7_required', 'consensus', 'distfs', 'node', 'net',
                  'viewer_js_required', 'viewer_performance_report', 'pixel_world_bridge', 'launcher_web',
                  'workspace_support', 'scenario_regression', 'operational_contracts', 'packaging_contracts',
                  'workflow_governance', 'codex_agent_config_validation', 'compile_metrics', 'site_quality',
                  'doc_checker_contracts', 'cargo_tooling_contracts']
        required = self.invoke('\n'.join('run_group ' + group for group in groups))
        full = self.invoke('run_full_required_superset\nrun_rust_baseline\n')
        self.assertEqual(required.returncode, 0, required.stderr)
        self.assertEqual(full.returncode, 0, full.stderr)
        self.assertLessEqual(set(required.stdout.splitlines()), set(full.stdout.splitlines()))
        self.assertIn('run_oasis7_required_tier_clippy', full.stdout)
        self.assertIn('viewer-auth-browser-security-smoke.mjs', full.stdout)

    def test_full_browser_failure_cannot_be_hidden_by_later_success(self):
        result = self.invoke('run_full_required_superset\n', fail_browser=True)
        self.assertEqual(result.returncode, 17, result.stderr)
        self.assertIn('run_oasis7_viewer_software_safe_build', result.stdout)
        self.assertIn('viewer-auth-browser-security-smoke.mjs', result.stdout)
        self.assertNotIn('run_pixel_world_bridge_lib_tests', result.stdout)


class BuildAndBundles(unittest.TestCase):
    def test_build_timings_include_dependency_freshness_per_variant(self):
        with tempfile.TemporaryDirectory() as temp:
            base = Path(temp)
            root = base / 'checkout'
            root.mkdir()
            config = root / 'suites.json'
            config.write_text('{}')
            def artifact(local, fresh):
                return dict(reason='compiler-artifact', fresh=fresh,
                    manifest_path=str((root if local else base / 'dependency') / 'Cargo.toml'))
            messages = [
                [artifact(True, False), artifact(False, True), artifact(True, True),
                 artifact(False, False), artifact(False, False), dict(reason='build-script-executed')],
                [artifact(False, True), artifact(True, True), dict(reason='build-finished', success=True)],
            ]
            class CargoOutput:
                def __init__(self, entries):
                    self.stdout = iter(json.dumps(entry) + '\n' for entry in entries)
                def __enter__(self):
                    return self
                def __exit__(self, *args):
                    pass
                def wait(self):
                    return 0
            bundle = base / 'bundle'
            with patch.object(artifacts, 'ROOT', root), patch.object(artifacts, 'CONFIG', config), \
                 patch.object(artifacts, 'build_variants', return_value=[('first', ['build'], []), ('second', ['test'], [])]), \
                 patch.object(artifacts.subprocess, 'Popen', side_effect=[CargoOutput(entries) for entries in messages]), \
                 patch.object(artifacts, 'output', return_value='host: fixture'), patch('builtins.print') as printed:
                artifacts.build(bundle, 'ordinary')
            timings = json.loads((bundle / 'manifest.json').read_text())['build_timings']
            observed = [{key: value for key, value in timing.items() if key != 'elapsed_seconds'} for timing in timings]
            self.assertEqual(observed, [
                dict(variant='first', local_units=2, local_fresh=1, local_rebuilt=1,
                     dependency_units=3, dependency_fresh=1, dependency_rebuilt=2),
                dict(variant='second', local_units=1, local_fresh=1, local_rebuilt=0,
                     dependency_units=1, dependency_fresh=1, dependency_rebuilt=0),
            ])
            reported = [json.loads(call.args[0].removeprefix('native_build='))
                        for call in printed.call_args_list if call.args[0].startswith('native_build=')]
            self.assertEqual(reported, timings)

    def test_ordinary_cache_state_selects_libraries_and_other_scopes_keep_all(self):
        with tempfile.TemporaryDirectory() as temp:
            base = Path(temp)
            root = base / 'checkout'
            root.mkdir()
            config = root / 'suites.json'
            config.write_text(json.dumps(dict(suites={}, legacy_ignored_filters={},
                                             environment_formal_batches={}, shell_tests={})))
            for scope in ('ordinary', 'formal', 'storage', 'all'):
                with self.subTest(scope=scope), \
                     patch.object(artifacts, 'ROOT', root), patch.object(artifacts, 'CONFIG', config), \
                     patch.object(artifacts, 'build_variants', return_value=[]), \
                     patch.object(artifacts, 'output', return_value='host: fixture'), \
                     patch.object(artifacts, 'cache_begin', wraps=artifacts.cache_begin) as begin:
                    state_path = base / (scope + '-state.json')
                    artifacts.build(base / scope, scope, state_path)
                    selection = 'libraries' if scope == 'ordinary' else 'all'
                    begin.assert_called_once_with(root, selection=selection)
                    state = json.loads(state_path.read_text())
                    self.assertEqual(state['selection'], selection)
                    self.assertTrue(state['complete'])

    def test_formal_builder_does_not_wait_for_ordinary_targets(self):
        formal = artifacts.build_variants('formal')
        ordinary = artifacts.build_variants('ordinary')
        self.assertEqual({v[0] for v in formal}, {'formal-required', 'runtime', 'endpoint', 'distfs-tests'})
        self.assertEqual({v[0] for v in ordinary}, {'viewer', 'required', 'snapshot-tests', 'viewer-snapshot-tests'})
        args = next(args for name, args, _ in formal if name == 'formal-required')
        self.assertIn('--bins', args)
        self.assertIn('--lib', args)
        self.assertNotIn('oasis7_viewer_runtime', args)
        self.assertNotIn('oasis7_ops_tools', args)
        self.assertIn('ci_formal_required_features', args[args.index('--features') + 1].split(','))
        for variant, command, features in [*ordinary, *formal[1:], *artifacts.build_variants('all')]:
            self.assertNotIn('ci_formal_required_features', features, variant)
            self.assertFalse(any('ci_formal_required_features' in arg for arg in command), variant)
        for owner, package in [('snapshot-tests', 'oasis7'), ('viewer-snapshot-tests', 'oasis7_viewer_runtime')]:
            snapshot = next(args for name, args, _ in ordinary if name == owner)
            self.assertEqual(snapshot, ['test', '-p', package, '--lib', '--no-run'])
            self.assertEqual(next(features for name, _, features in ordinary if name == owner), [])
        required = next(args for name, args, _ in ordinary if name == 'required')
        required_packages = [required[index + 1] for index, value in enumerate(required) if value == '-p']
        self.assertCountEqual(required_packages, [
            'oasis7', 'oasis7_simulator', 'oasis7_runtime', 'oasis7_world_state', 'oasis7_world_projection', 'oasis7_world_service', 'scenario_test_runner', 'oasis7_chain_runtime', 'oasis7_client_cli', 'oasis7_protocol_tools', 'oasis7_launcher_backend',
            'oasis7_newapi_bridge_service', 'oasis7_ops_tools', 'oasis7_provider_local_bridge',
            'oasis7_provider_tools', 'oasis7_viewer_runtime', 'oasis7_viewer_codec', 'oasis7_observability',
            'oasis7_hosted_access', 'oasis7_letai_credit_adapter', 'oasis7_launcher_config',
            'oasis7_wasm_materializer', 'oasis7_game_client', 'oasis7_browser_game_client', 'oasis7_cas', 'oasis7_formal_protocol',
        ])
        self.assertIn('--tests', required)
        shell = (HERE / 'ci-tests.sh').read_text()
        self.assertIn('run_cargo test -p oasis7_viewer_runtime --lib snapshot_progress::;', shell)
        self.assertIn('run_cargo test -p oasis7 --lib snapshot_player_gameplay_execution_state_backfills_from_legacy_fields;', shell)

    def test_formal_alias_requires_owning_package_and_exact_target_kind(self):
        classify = artifacts.artifact_logical_name
        for variant in ('required', 'formal-required'):
            with self.subTest(variant=variant):
                self.assertEqual(classify(variant, 'oasis7_chain_runtime',
                    dict(name='oasis7_chain_runtime', kind=['lib']), True), 'runtime-tests')
                self.assertEqual(classify(variant, 'oasis7',
                    dict(name='oasis7', kind=['lib']), True), 'oasis7-lib-tests')
                for package, kind, name in [('oasis7_chain_runtime', 'bin', 'oasis7_chain_runtime'),
                                            ('other_package', 'bin', 'oasis7_chain_runtime'),
                                            ('oasis7', 'bin', 'oasis7')]:
                    logical = classify(variant, package, dict(name=name, kind=[kind]), True)
                    self.assertNotIn(logical, ('runtime-tests', 'oasis7-lib-tests'))
                    self.assertIn(package + '-' + kind + '-', logical)
                self.assertTrue(classify(variant, 'oasis7_chain_runtime',
                    dict(name='oasis7_chain_runtime', kind=['bin']), False).startswith('required-executable-'))

    def test_chain_library_and_binary_harnesses_keep_distinct_suite_owners(self):
        # Cargo emits both executable harnesses with target.name == package name.
        # Exercise the producer JSON path without compiling a Cargo workspace.
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            package = root / 'crates/oasis7_chain_runtime'
            package.mkdir(parents=True)
            manifest_path = package / 'Cargo.toml'
            manifest_path.write_text('[package]\nname = "oasis7_chain_runtime"\n')
            messages = []
            for kind in ('lib', 'bin'):
                executable = root / ('chain-' + kind)
                executable.write_bytes(kind.encode())
                messages.append(dict(reason='compiler-artifact', manifest_path=str(manifest_path),
                    package_id='path+file://' + str(package) + '#0.1.0', fresh=False,
                    target=dict(name='oasis7_chain_runtime', kind=[kind]),
                    profile=dict(test=True), features=[], executable=str(executable)))
            class CargoOutput:
                def __enter__(self):
                    self.stdout = iter(json.dumps(message) + '\n' for message in messages)
                    return self
                def __exit__(self, *args):
                    pass
                def wait(self):
                    return 0
            config_path = root / 'suites.json'
            config = dict(suites={'runtime-protocol': [dict(binary='runtime-tests', name='formal_case', ignored=True)]},
                          legacy_ignored_filters={'runtime-tests': ['formal_case']},
                          environment_formal_batches={}, shell_tests={})
            config_path.write_text(json.dumps(config))
            def inventory(path, ignored=False, env=None):
                if path.name == 'runtime-tests':
                    return ['formal_case'] if ignored else ['config::contract']
                return []
            for variant, scope in [('required', 'ordinary'), ('formal-required', 'formal')]:
                with self.subTest(variant=variant):
                    bundle = root / variant
                    with patch.object(artifacts, 'ROOT', root), patch.object(artifacts, 'CONFIG', config_path), \
                         patch.object(artifacts, 'build_variants', return_value=[(variant, ['test'], [])]), \
                         patch.object(artifacts.subprocess, 'Popen', return_value=CargoOutput()), \
                         patch.object(artifacts, 'inventory', side_effect=inventory), \
                         patch.object(artifacts, 'output', return_value='host: fixture'), \
                         patch.object(artifacts.sys, 'platform', 'fixture'):
                        artifacts.build(bundle, scope)
                    built = json.loads((bundle / 'manifest.json').read_text())
                    binaries = built['binaries']
                    library = next(name for name, binary in binaries.items() if binary.get('target_kind') == ['lib'])
                    self.assertEqual(library, 'runtime-tests')
                    self.assertEqual(binaries['runtime-tests']['target_kind'], ['lib'])
                    self.assertEqual(binaries[library]['package'], 'oasis7_chain_runtime')
                    self.assertEqual(binaries[library]['ordinary'], scope == 'ordinary')
                    if scope == 'formal':
                        entries = artifacts.selected_entries(built, config, suite='runtime-protocol')
                        self.assertEqual([entry['binary'] for entry in entries], ['runtime-tests'])
                        continue
                    for owner, pattern in artifacts.DEFAULT_SNAPSHOT_REGRESSIONS:
                        binaries[owner] = dict(path=owner, cwd='.', test=True, ordinary=False,
                            tests=[pattern + ('one' if pattern.endswith('::') else '')], ignored_tests=[])
                    self.assertIn(library, artifacts.dependency_closure(built, config, suite='ordinary'))
                    with patch.object(artifacts, 'ROOT', root), patch.object(artifacts, 'load', return_value=(built, {})), \
                         patch('ci_process_runner.run', return_value=dict(passed=True, cleanup_result='clean')) as run_test:
                        artifacts.execute(bundle, 'ordinary', root / 'results.jsonl')
                    library_runs = [call for call in run_test.call_args_list
                                    if call.args[0][0] == str(bundle / binaries[library]['path'])]
                    self.assertEqual(len(library_runs), 1)
                    self.assertEqual(library_runs[0].kwargs['expected_count'], 1)
            messages.append(messages[0])
            with patch.object(artifacts, 'ROOT', root), \
                 patch.object(artifacts, 'build_variants', return_value=[('required', ['test'], [])]), \
                 patch.object(artifacts.subprocess, 'Popen', return_value=CargoOutput()), \
                 patch.object(artifacts.sys, 'platform', 'fixture'):
                with self.assertRaisesRegex(ValueError, 'duplicate executable: runtime-tests'):
                    artifacts.build(root / 'duplicate', 'ordinary')

    def test_closure_contains_child_executables_but_not_other_suites(self):
        config = {'binary_dependencies': {'runtime-tests': ['runtime', 'endpoint'],
                                           'distfs-tests': ['endpoint']}, 'script_dependencies': {}}
        binaries = {name: {'test': name.endswith('tests')} for name in
                    ('runtime-tests', 'runtime', 'endpoint', 'distfs-tests', 'viewer', 'snapshot-tests', 'viewer-snapshot-tests')}
        manifest = {'binaries': binaries, 'suites': {'storage-recovery': [
            {'binary': 'distfs-tests', 'name': 'storage', 'ignored': True}]}}
        self.assertEqual(artifacts.dependency_closure(manifest, config, suite='storage-recovery'),
                         {'distfs-tests', 'endpoint'})
        del binaries['endpoint']
        with self.assertRaises(ValueError):
            artifacts.dependency_closure(manifest, config, suite='storage-recovery')

    def test_low_compression_package_preserves_modes_and_omits_unused_binary(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            bundle = root / 'bundle'
            (bundle / 'bin').mkdir(parents=True)
            (bundle / 'lib').mkdir()
            binaries = {}
            for name in ('viewer', 'snapshot-tests', 'viewer-snapshot-tests', 'required-test-lib-example', 'endpoint'):
                path = bundle / 'bin' / name
                path.write_bytes(b'fixture executable\n' * 2000)
                path.chmod(0o755)
                binaries[name] = dict(path=f'bin/{name}', sha256=artifacts.digest(path),
                    bytes=path.stat().st_size, test=name.endswith('tests') or name.startswith('required-test'),
                    ordinary=name.startswith('required-test'), tests=['one'], ignored_tests=[], libraries=[])
            (bundle / 'manifest.json').write_text(json.dumps(dict(schema_version=2, scope='all',
                binaries=binaries, suites={}, libraries={})))
            config = root / 'suites.json'
            config.write_text(json.dumps({}))
            with patch.object(artifacts, 'CONFIG', config):
                report = artifacts.pack(bundle, root / 'ordinary.tar.gz', suite='ordinary')
            self.assertLess(report['compressed_bytes'], report['raw_bytes'])
            with tarfile.open(root / 'ordinary.tar.gz') as archive:
                names = archive.getnames()
                self.assertNotIn('bin/endpoint', names)
                self.assertIn('bin/viewer', names)
                self.assertEqual(archive.getmember('bin/viewer').mode & 0o777, 0o755)
                manifest = json.load(archive.extractfile('manifest.json'))
                self.assertEqual(manifest['selection'], {'suite': 'ordinary'})
                self.assertNotIn('endpoint', manifest['included_binaries'])
                self.assertTrue({'snapshot-tests', 'viewer-snapshot-tests'} <= set(manifest['included_binaries']))
            for owner in ('snapshot-tests', 'viewer-snapshot-tests'):
                incomplete = dict(binaries)
                del incomplete[owner]
                with self.assertRaisesRegex(ValueError, 'missing default-feature snapshot harness: ' + owner):
                    artifacts.dependency_closure(dict(binaries=incomplete), {}, suite='ordinary')

    def test_cache_budget_is_a_skip_decision_not_a_test_failure(self):
        with tempfile.TemporaryDirectory() as temp:
            target = Path(temp)
            (target / 'artifact').write_bytes(b'x' * 101)
            self.assertFalse(artifacts.cache_budget(target, 100)['save'])
            self.assertTrue(artifacts.cache_budget(target, 101)['save'])


class WorkspaceCacheSelection(unittest.TestCase):
    """Cargo JSON fixtures cover unit selection without requiring a toolchain."""

    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.base = Path(temporary.name)
        self.root = self.base / 'checkout'
        self.root.mkdir()
        self.target = self.root / 'target'
        self.cache = self.base / 'cache'
        self.manifest = self.root / 'Cargo.toml'
        self.manifest.write_text('[package]\nname="cache_local"\nversion="0.1.0"\n')
        (self.root / 'lib.rs').write_text('pub fn value() {}')
        (self.root / 'build.rs').write_text('fn main() {}')
        self.messages, self.library_files, self.other_files = [], set(), set()
        for name, kinds, unit_hash, test, suffixes in [
                ('cache_lib', ['lib'], 'a' * 16, False, ['rlib', 'rmeta']),
                ('cache_macro', ['proc-macro'], 'b' * 16, False, ['so']),
                ('cache_lib', ['lib'], 'c' * 16, True, ['']),
                ('cache_tool', ['bin'], 'd' * 16, False, [''])]:
            selected = self.library_files if not test and 'bin' not in kinds else self.other_files
            filenames = []
            for suffix in suffixes:
                path = f'debug/deps/{"lib" if suffix else ""}{name}-{unit_hash}' + ('.' + suffix if suffix else '')
                self.write(path, selected)
                filenames.append(str(self.target / path))
            self.write(f'debug/deps/{name}-{unit_hash}.d', selected)
            self.write(f'debug/.fingerprint/cache_local-{unit_hash}/invoked.timestamp', selected)
            self.write(f'debug/.fingerprint/cache_local-{unit_hash}/unit.json', selected)
            self.messages.append(dict(reason='compiler-artifact', manifest_path=str(self.manifest),
                package_id='cache_local', filenames=filenames, target=dict(name=name, kind=kinds, src_path=str(self.root / 'lib.rs')),
                profile=dict(test=test)))
        compile_dir = 'debug/build/cache_local-' + 'e' * 16
        run_dir = 'debug/build/cache_local-' + 'f' * 16
        for path in (compile_dir + '/build-script-build', compile_dir + '/build_script_build.d',
                     'debug/.fingerprint/cache_local-' + 'e' * 16 + '/build-script-build-script-build',
                     run_dir + '/out/generated.rs', run_dir + '/output', run_dir + '/root-output',
                     run_dir + '/invoked.timestamp', run_dir + '/stderr',
                     'debug/.fingerprint/cache_local-' + 'f' * 16 + '/run-build-script-build-script-build'):
            self.write(path, self.library_files)
        self.messages += [
            dict(reason='compiler-artifact', manifest_path=str(self.manifest), package_id='cache_local',
                 filenames=[str(self.target / compile_dir / 'build-script-build')],
                 target=dict(name='build-script-build', kind=['custom-build'], src_path=str(self.root / 'build.rs')), profile=dict(test=False)),
            dict(reason='build-script-executed', package_id='cache_local', out_dir=str(self.target / run_dir / 'out')),
            dict(reason='compiler-artifact', manifest_path=str(self.base / 'external/Cargo.toml')),
            dict(reason='build-script-executed', package_id='external', out_dir=str(self.base / 'external/out')),
        ]

    def write(self, name, selection):
        path = self.target / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(name)
        selection.add(name)

    def record(self, state):
        for message in self.messages:
            artifacts.cache_record(state, message)
        artifacts.cache_finish(state)

    def test_libraries_keep_complete_build_scripts_and_omit_binaries_and_harnesses(self):
        state = artifacts.cache_begin(self.root, selection='libraries')
        self.record(state)
        report = artifacts.cache_stage(state, self.cache)
        self.assertTrue(report['save'], report)
        self.assertEqual(report['selection'], 'libraries')
        manifest = json.loads((self.cache / 'manifest.json').read_text())
        self.assertEqual(set(manifest['files']), self.library_files)
        self.assertTrue(all((self.cache / 'target' / name).is_file() for name in self.library_files))
        self.assertFalse(any((self.cache / 'target' / name).exists() for name in self.other_files))
        self.assertEqual(report['file_count'], len(self.library_files))
        self.assertLess(report['raw_bytes'], report['target_raw_bytes'])

    def test_default_and_legacy_state_keep_all_complete_units(self):
        for legacy in (False, True):
            with self.subTest(legacy=legacy):
                state = artifacts.cache_begin(self.root)
                self.assertEqual(state['selection'], 'all')
                if legacy:
                    del state['selection']
                self.record(state)
                report = artifacts.cache_stage(state, self.cache)
                self.assertTrue(report['save'], report)
                self.assertEqual(report['selection'], 'all')
                manifest = json.loads((self.cache / 'manifest.json').read_text())
                self.assertEqual(set(manifest['files']), self.library_files | self.other_files)

    def test_unknown_selection_and_oversize_library_units_do_not_stage(self):
        with self.assertRaisesRegex(ValueError, 'selection'):
            artifacts.cache_begin(self.root, selection='binaries')
        state = artifacts.cache_begin(self.root, selection='libraries')
        self.record(state)
        report = artifacts.cache_stage(state, self.cache, maximum=1)
        self.assertFalse(report['save'], report)
        self.assertFalse(report['eligible'], report)
        self.assertGreater(report['raw_bytes'], 1)
        self.assertFalse(self.cache.exists())


class WorkspaceCache(unittest.TestCase):
    """Real Cargo units, including both build-script units and a dependency cache."""

    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.base = Path(self.temporary.name)
        self.root = self.base / 'checkout'
        self.cache = self.base / 'workspace-cache'
        self.dependencies = self.base / 'dependency-cache'
        dependency = self.base / 'dependency'
        (dependency / 'src').mkdir(parents=True)
        (dependency / 'Cargo.toml').write_text('[package]\nname="cache_external"\nversion="0.1.0"\nedition="2021"\n')
        (dependency / 'src/lib.rs').write_text('pub fn value() -> &\'static str { "outside" }')
        self.sources = {
            'Cargo.toml': '[workspace]\nmembers=["support", "app"]\nresolver="2"\n[profile.dev]\ndebug=0\nincremental=false\n',
            '.gitignore': 'app/input.txt\n',
            'support/Cargo.toml': '[package]\nname="cache_support"\nversion="0.1.0"\nedition="2021"\n',
            'support/src/lib.rs': 'pub fn value() -> &\'static str { "alpha" }',
            'app/Cargo.toml': '[package]\nname="cache_app"\nversion="0.1.0"\nedition="2021"\n[dependencies]\ncache_support={path="../support"}\ncache_external={path="../../dependency"}\n',
            'app/src/main.rs': 'include!(concat!(env!("OUT_DIR"), "/generated.rs"));\nfn main() { println!("{}:{}:{}:{}", cache_support::value(), cache_external::value(), GENERATED, include_str!("../input.txt").trim()); }',
            # This file is deliberately not tracked, and .gitignore excludes it.
            'app/input.txt': 'one\n',
            'app/build.rs': 'use std::os::unix::fs::PermissionsExt; fn main() { let v=std::fs::read_to_string("input.txt").unwrap(); let mode=std::fs::metadata("input.txt").unwrap().permissions().mode(); let v=format!("{}{}", v.trim(), if mode & 0o111 != 0 { ":executable" } else { "" }); let out=std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap()); std::fs::write(out.join("generated.rs"), format!("const GENERATED: &str = {:?};", v)).unwrap(); println!("cargo:rerun-if-changed=input.txt"); println!("cargo:rerun-if-env-changed=CACHE_FIXTURE_ENV"); }',
        }
        self.checkout()
        subprocess.run(['git', 'init', '-q', str(self.root)], check=True)
        subprocess.run(['git', 'add', '.'], cwd=self.root, check=True)
        subprocess.run(['cargo', 'generate-lockfile', '--offline'], cwd=self.root,
                       check=True, capture_output=True)
        self.sources['Cargo.lock'] = (self.root / 'Cargo.lock').read_text()

    def checkout(self):
        if self.root.exists():
            shutil.rmtree(self.root)
        for name, content in self.sources.items():
            path = self.root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(content)
        if self.dependencies.exists():
            shutil.copytree(self.dependencies, self.root / 'target', copy_function=shutil.copy2)

    def cargo(self, state=None, success=True, extra_env=None, extra_args=()):
        env = dict(os.environ, CARGO_INCREMENTAL='0', CACHE_FIXTURE_ENV='original')
        env.pop('RUSTC_WRAPPER', None)
        env.update(extra_env or {})
        messages = []
        with tempfile.TemporaryFile(mode='w+') as errors:
            with subprocess.Popen(['cargo', 'build', '--workspace', '--offline', '--locked',
                                   '--message-format=json', *extra_args], cwd=self.root, env=env,
                                  stdout=subprocess.PIPE, stderr=errors, text=True) as process:
                for line in process.stdout:
                    if line.startswith('{'):
                        message = json.loads(line)
                        messages.append(message)
                        if state is not None:
                            artifacts.cache_record(state, message)
                process.wait()
            errors.seek(0)
            self.assertEqual(process.returncode == 0, success, errors.read())
        if state is not None:
            artifacts.cache_finish(state)
        units = [m for m in messages if m.get('reason') == 'compiler-artifact']
        if success:
            value = subprocess.check_output([str(self.root / 'target/debug/cache_app')], text=True).strip()
        else:
            value = None
        return units, value

    def seed(self, selection='all'):
        state = artifacts.cache_begin(self.root, selection=selection)
        units, value = self.cargo(state)
        self.assertEqual(value, 'alpha:outside:one:one')
        self.assertTrue(all(not u['fresh'] for u in units))
        report = artifacts.cache_stage(state, self.cache)
        self.assertTrue(report['save'], report)
        self.assertGreater(report['target_raw_bytes'], report['raw_bytes'])
        # Restore only the external dependency, like the existing dependency
        # cache. No local-package outputs survive without the workspace overlay.
        for subdirectory in ('deps', '.fingerprint'):
            source = self.root / 'target/debug' / subdirectory
            for path in source.iterdir():
                if not path.name.startswith(('cache_external-', 'libcache_external-')):
                    continue
                destination = self.dependencies / 'debug' / subdirectory / path.name
                destination.parent.mkdir(parents=True, exist_ok=True)
                if path.is_dir():
                    shutil.copytree(path, destination, copy_function=shutil.copy2)
                else:
                    shutil.copy2(path, destination)
        # An ordinary archive can round file mtimes to seconds. The manifest
        # must restore exact Cargo timestamps instead of trusting tar headers.
        archive_path = self.base / 'cache.tar'
        with tarfile.open(archive_path, 'w', format=tarfile.USTAR_FORMAT) as archive:
            archive.add(self.cache, arcname='cache')
        shutil.rmtree(self.cache)
        with tarfile.open(archive_path) as archive:
            archive.extractall(self.base / 'restored', filter='data')
        shutil.move(self.base / 'restored/cache', self.cache)
        return state

    def test_library_overlay_reuses_libraries_and_build_scripts_but_rebuilds_binary(self):
        self.seed(selection='libraries')
        manifest = json.loads((self.cache / 'manifest.json').read_text())
        self.assertNotIn('debug/cache_app', manifest['files'])
        self.assertFalse(any(name.startswith('debug/deps/cache_app-') for name in manifest['files']))
        self.checkout()
        restored = artifacts.cache_restore(self.root, self.cache)
        self.assertTrue(restored['restored'], restored)
        self.assertFalse((self.root / 'target/debug/cache_app').exists())
        units, value = self.cargo()
        self.assertEqual(value, 'alpha:outside:one:one')
        for unit in units:
            self.assertEqual(unit['fresh'], 'bin' not in unit['target']['kind'], unit)
        self.checkout()
        changed = self.root / 'support/src/lib.rs'
        changed.write_text(self.sources['support/src/lib.rs'].replace('alpha', 'bravo'))
        original_mtime = manifest['inputs']['support/src/lib.rs']['mtime_ns']
        os.utime(changed, ns=(original_mtime, original_mtime))
        self.assertTrue(artifacts.cache_restore(self.root, self.cache)['restored'])
        units, value = self.cargo()
        self.assertEqual(value, 'bravo:outside:one:one')
        self.assertTrue(any(not unit['fresh'] and unit['target']['name'] == 'cache_support' for unit in units))

    def test_real_overlay_is_fresh_and_changed_inputs_rebuild(self):
        self.seed()
        self.checkout()
        restored = artifacts.cache_restore(self.root, self.cache)
        self.assertTrue(restored['restored'], restored)
        units, value = self.cargo()
        self.assertTrue(all(u['fresh'] for u in units), units)
        self.assertEqual(value, 'alpha:outside:one:one')
        manifest = json.loads((self.cache / 'manifest.json').read_text())
        cases = [
            ('support/src/lib.rs', self.sources['support/src/lib.rs'].replace('alpha', 'bravo'), None,
             'bravo:outside:one:one'),
            ('app/input.txt', 'two\n', None, 'alpha:outside:two:two'),
            ('app/input.txt', 'one\n', 0o755, 'alpha:outside:one:executable:one'),
        ]
        for name, text, mode, expected in cases:
            with self.subTest(input=name, mode=mode):
                self.checkout()
                path = self.root / name
                path.write_text(text)
                if mode is not None:
                    path.chmod(mode)
                old_mtime = manifest['inputs'][name]['mtime_ns']
                os.utime(path, ns=(old_mtime, old_mtime))
                restored = artifacts.cache_restore(self.root, self.cache)
                self.assertTrue(restored['restored'], restored)
                self.assertEqual(restored['changed_inputs'], 1)
                units, value = self.cargo()
                self.assertTrue(any(not u['fresh'] and u['target']['name'] == 'cache_app' for u in units), units)
                self.assertEqual(value, expected)
        self.checkout()
        self.assertTrue(artifacts.cache_restore(self.root, self.cache)['restored'])
        units, _ = self.cargo(extra_env={'CACHE_FIXTURE_ENV': 'changed'})
        self.assertTrue(any(not u['fresh'] and u['target']['name'] == 'cache_app' for u in units), units)

    def test_path_changes_rebuild_and_unsafe_cache_inputs_are_misses(self):
        # Also retain Cargo's default package-directory observation when a
        # build script does not narrow its inputs with rerun-if directives.
        self.sources['app/build.rs'] = self.sources['app/build.rs'].replace(
            'println!("cargo:rerun-if-changed=input.txt"); println!("cargo:rerun-if-env-changed=CACHE_FIXTURE_ENV");', '')
        (self.root / 'app/build.rs').write_text(self.sources['app/build.rs'])
        self.seed()
        for change in ('new', 'deleted', 'renamed', 'source-symlink'):
            with self.subTest(change=change):
                self.checkout()
                path = self.root / 'app/input.txt'
                if change == 'new':
                    (self.root / 'app/new.txt').write_text('new ignored input')
                elif change == 'renamed':
                    path.rename(self.root / 'app/renamed-input.txt')
                else:
                    path.unlink()
                    if change == 'source-symlink':
                        path.symlink_to(self.base / 'dependency/src/lib.rs')
                report = artifacts.cache_restore(self.root, self.cache)
                if change == 'source-symlink':
                    self.assertFalse(report['restored'], report)
                    self.assertFalse((self.root / 'target/debug/cache_app').exists())
                else:
                    self.assertTrue(report['restored'], report)
                    units, _ = self.cargo(success=change not in ('deleted', 'renamed'))
                    self.assertTrue(any(not u['fresh'] for u in units))
        original = json.loads((self.cache / 'manifest.json').read_text())
        for damage in ('digest', 'missing', 'payload-symlink', 'traversal', 'future', 'checkout'):
            with self.subTest(damage=damage):
                self.checkout()
                broken = self.base / ('broken-' + damage)
                shutil.copytree(self.cache, broken, copy_function=shutil.copy2)
                manifest = copy.deepcopy(original)
                name = next(iter(manifest['files']))
                path = broken / 'target' / name
                if damage == 'digest':
                    path.write_bytes(b'broken')
                elif damage == 'missing':
                    path.unlink()
                elif damage == 'payload-symlink':
                    path.unlink()
                    path.symlink_to(self.base / 'dependency/src/lib.rs')
                elif damage == 'traversal':
                    manifest['files']['../escape'] = manifest['files'].pop(name)
                elif damage == 'future':
                    manifest['files'][name]['mtime_ns'] = 2 ** 63 - 1
                else:
                    manifest['checkout'] = str(self.base / 'different-checkout')
                (broken / 'manifest.json').write_text(json.dumps(manifest))
                report = artifacts.cache_restore(self.root, broken)
                self.assertFalse(report['restored'], report)
                self.assertFalse((self.root / 'target/debug/cache_app').exists())

    def test_document_addition_reuses_unaffected_library(self):
        self.sources['doc/index.md'] = 'existing documentation'
        self.checkout()
        self.seed()
        self.checkout()
        (self.root / 'doc/added.md').write_text('new documentation')
        restored = artifacts.cache_restore(self.root, self.cache)
        self.assertTrue(restored['restored'], restored)
        self.assertEqual(restored['input_path_changes']['added']['paths'], ['doc/added.md'])
        units, value = self.cargo()
        support = [u for u in units if u['target']['name'] == 'cache_support']
        self.assertTrue(support)
        self.assertTrue(all(u['fresh'] for u in support), support)
        self.assertTrue(any(not u['fresh'] and 'custom-build' in u['target']['kind'] for u in units))
        self.assertEqual(value, 'alpha:outside:one:one')

    def test_added_ambiguous_module_cannot_reuse_old_successful_binary(self):
        self.sources['support/src/lib.rs'] = 'mod piece; pub fn value() -> &\'static str { piece::value() }'
        self.sources['support/src/piece.rs'] = 'pub fn value() -> &\'static str { "alpha" }'
        self.checkout()
        self.seed()
        self.checkout()
        added = self.root / 'support/src/piece/mod.rs'
        added.parent.mkdir()
        added.write_text('pub fn value() -> &\'static str { "bravo" }')
        os.utime(added, ns=(1, 1))
        restored = artifacts.cache_restore(self.root, self.cache)
        self.assertTrue(restored['restored'], restored)
        # Rust must report the now-ambiguous module, not return a stale success.
        self.cargo(success=False)

    def test_outside_package_include_is_not_ignored(self):
        self.sources['assets/value.txt'] = 'alpha'
        self.sources['support/src/lib.rs'] = 'pub fn value() -> &\'static str { include_str!("../../assets/value.txt") }'
        self.checkout()
        self.seed()
        self.checkout()
        (self.root / 'assets/added.txt').write_text('new sibling')
        restored = artifacts.cache_restore(self.root, self.cache)
        self.assertTrue(restored['restored'], restored)
        units, value = self.cargo()
        support = [u for u in units if u['target']['name'] == 'cache_support']
        self.assertTrue(support)
        self.assertTrue(any(not u['fresh'] for u in support), support)
        self.assertEqual(value, 'alpha:outside:one:one')

    def test_directory_observer_rebuilds_on_add_remove_and_rename(self):
        self.sources['assets/one.txt'] = 'one'
        self.sources['app/build.rs'] = '''fn main() {
            let count = std::fs::read_dir("../assets").unwrap().count();
            let value = match count { 1 => "one", 2 => "two", _ => "zero" };
            let out = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
            std::fs::write(out.join("generated.rs"), format!("const GENERATED: &str = {:?};", value)).unwrap();
            println!("cargo:rerun-if-changed=../assets");
        }'''
        self.checkout()
        self.seed()
        for change, expected in [('add', 'two'), ('remove', 'zero'), ('rename', 'one')]:
            with self.subTest(change=change):
                self.checkout()
                path = self.root / 'assets/one.txt'
                if change == 'add':
                    (self.root / 'assets/two.txt').write_text('two')
                elif change == 'remove':
                    path.unlink()
                else:
                    path.rename(path.with_name('renamed.txt'))
                restored = artifacts.cache_restore(self.root, self.cache)
                self.assertTrue(restored['restored'], restored)
                units, value = self.cargo()
                self.assertEqual(value, f'alpha:outside:{expected}:one')
                self.assertTrue(any(not u['fresh'] and 'custom-build' in u['target']['kind'] for u in units))

    def test_budget_and_source_changes_do_not_publish_partial_units(self):
        state = artifacts.cache_begin(self.root)
        self.cargo(state)
        before = artifacts._cache_inputs(self.root)
        with patch.object(artifacts, 'digest', side_effect=AssertionError('measurement must not hash files')):
            measurement = artifacts.cache_stage(state, self.cache, measure_only=True)
        self.assertTrue(measurement['eligible'], measurement)
        self.assertFalse(measurement['save'])
        self.assertGreater(measurement['file_count'], 0)
        self.assertGreater(measurement['target_raw_bytes'], measurement['raw_bytes'])
        self.assertGreater(measurement['unique_bytes'], 0)
        self.assertTrue(measurement['bytes_by_category'])
        self.assertFalse(self.cache.exists())
        self.assertEqual(before, artifacts._cache_inputs(self.root))
        report = artifacts.cache_stage(state, self.cache, maximum=1)
        self.assertFalse(report['save'], report)
        self.assertGreater(report['raw_bytes'], 1)
        self.assertFalse((self.cache / 'manifest.json').exists())
        (self.root / 'app/input.txt').write_text('changed during build\n')
        with self.assertRaisesRegex(ValueError, 'source inputs changed'):
            artifacts.cache_finish(state)
        report = artifacts.cache_stage(state, self.cache)
        self.assertFalse(report['save'], report)
        self.assertFalse((self.cache / 'manifest.json').exists())


class UnhashedLibraryCache(unittest.TestCase):
    """Model Cargo's path-library layout, including delayed fingerprints."""

    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.base = Path(temporary.name)
        self.root = self.base / 'checkout'
        self.root.mkdir()
        self.target = self.root / 'target'
        self.cache = self.base / 'cache'
        self.manifest = self.root / 'Cargo.toml'
        self.manifest.write_text('[package]\nname="mixed-lib"\nversion="0.1.0"\n')
        self.fingerprint = 'debug/.fingerprint/mixed-lib-' + 'a' * 16
        self.filenames = ['debug/deps/libmixed_lib.so', 'debug/deps/libmixed_lib.rlib']
        for name in [*self.filenames, 'debug/deps/mixed_lib.d']:
            self.write(name)
        self.message = dict(reason='compiler-artifact', manifest_path=str(self.manifest),
            package_id='mixed-lib', filenames=[str(self.target / name) for name in self.filenames],
            target=dict(name='mixed_lib', kind=['cdylib', 'lib']),
            profile=dict(test=False), features=[], fresh=False)

    def write(self, name, text=None):
        path = self.target / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(name if text is None else text)
        return path

    def fingerprint_files(self, directory=None, features=()):
        directory = directory or self.fingerprint
        for name in ('lib-mixed_lib', 'dep-lib-mixed_lib', 'invoked.timestamp'):
            self.write(directory + '/' + name)
        self.write(directory + '/lib-mixed_lib.json', json.dumps(dict(features=json.dumps(list(features)))))

    def finish(self, state):
        artifacts.cache_finish(state)
        self.assertTrue(state['complete'])
        return artifacts.cache_stage(state, self.cache)

    def test_deferred_complete_units_roundtrip_for_both_selections(self):
        for selection in ('libraries', 'all'):
            with self.subTest(selection=selection):
                if (self.target / 'debug/.fingerprint').exists():
                    shutil.rmtree(self.target / 'debug/.fingerprint')
                state = artifacts.cache_begin(self.root, selection=selection)
                artifacts.cache_record(state, self.message)
                self.assertFalse(state['complete'])
                self.assertFalse(artifacts.cache_stage(state, self.cache)['save'])
                self.fingerprint_files()
                # A valid timestamp marker need not contain payload bytes.
                self.write(self.fingerprint + '/invoked.timestamp', '')
                report = self.finish(state)
                self.assertTrue(report['save'], report)
                manifest = json.loads((self.cache / 'manifest.json').read_text())
                self.assertTrue(set(self.filenames) <= set(manifest['files']))
                self.assertIn('debug/deps/mixed_lib.d', manifest['files'])
                self.assertIn(self.fingerprint, manifest['directories'])
                shutil.rmtree(self.target)
                restored = artifacts.cache_restore(self.root, self.cache)
                self.assertTrue(restored['restored'], restored)
                for name in self.filenames:
                    self.assertEqual((self.target / name).read_text(), name)

    def test_uplifted_library_preserves_proven_concrete_outputs(self):
        self.fingerprint_files()
        message = copy.deepcopy(self.message)
        public_names = []
        for name in self.filenames:
            public = self.target / 'debug' / Path(name).name
            os.link(self.target / name, public)
            public_names.append(str(public))
        message['filenames'] = public_names
        self.write('debug/libmixed_lib.d')
        state = artifacts.cache_begin(self.root)
        artifacts.cache_record(state, message)
        self.assertTrue(self.finish(state)['save'])
        manifest = json.loads((self.cache / 'manifest.json').read_text())
        for name in self.filenames:
            public = Path('debug') / Path(name).name
            self.assertIn(public.as_posix(), manifest['files'])
            self.assertTrue(os.path.samefile(self.cache / 'target' / name, self.cache / 'target' / public))
        self.assertIn('debug/libmixed_lib.d', manifest['files'])

    def test_hashed_check_and_test_units_do_not_own_unhashed_library(self):
        self.fingerprint_files()
        other = 'debug/.fingerprint/mixed-lib-' + 'b' * 16
        self.fingerprint_files(other)
        self.write('debug/deps/libmixed_lib-' + 'b' * 16 + '.rmeta')
        self.write('debug/.fingerprint/mixed-lib-' + 'c' * 16 + '/test-lib-mixed_lib')
        state = artifacts.cache_begin(self.root)
        artifacts.cache_record(state, self.message)
        self.assertTrue(self.finish(state)['save'])
        self.assertEqual(state['directories'], [self.fingerprint])
        self.assertFalse(any('b' * 16 in name for name in state['files']))

    def test_last_feature_variant_owns_the_shared_output_names(self):
        state = artifacts.cache_begin(self.root)
        first = dict(self.message, features=['alternate'])
        artifacts.cache_record(state, first)
        artifacts.cache_record(state, self.message)
        self.fingerprint_files()
        self.assertTrue(self.finish(state)['save'])
        self.assertEqual(len(state['unhashed_libraries']), 1)
        self.assertEqual(next(iter(state['unhashed_libraries'].values()))['features'], [])

    def test_ambiguous_fingerprint_is_not_selected_by_timestamp(self):
        self.fingerprint_files()
        self.fingerprint_files('debug/.fingerprint/mixed-lib-' + 'b' * 16)
        state = artifacts.cache_begin(self.root)
        artifacts.cache_record(state, self.message)
        with self.assertRaisesRegex(ValueError, 'unique unhashed.*mixed-lib/mixed_lib'):
            artifacts.cache_finish(state)
        self.assertFalse(state['complete'])
        self.assertEqual(state['files'], [])
        self.assertFalse(artifacts.cache_stage(state, self.cache)['save'])

    def test_missing_fingerprint_members_are_rejected(self):
        for name in ('lib-mixed_lib', 'lib-mixed_lib.json', 'dep-lib-mixed_lib', 'invoked.timestamp'):
            with self.subTest(missing=name):
                self.fingerprint_files()
                (self.target / self.fingerprint / name).unlink()
                state = artifacts.cache_begin(self.root)
                artifacts.cache_record(state, self.message)
                with self.assertRaises(ValueError):
                    artifacts.cache_finish(state)
                self.assertFalse(state['complete'])
                self.assertFalse(artifacts.cache_stage(state, self.cache)['save'])

    def test_missing_outputs_and_dep_info_are_rejected(self):
        self.fingerprint_files()
        for name in [*self.filenames, 'debug/deps/mixed_lib.d']:
            with self.subTest(missing=name):
                state = artifacts.cache_begin(self.root)
                artifacts.cache_record(state, self.message)
                (self.target / name).unlink()
                with self.assertRaisesRegex(ValueError, 'missing unhashed'):
                    artifacts.cache_finish(state)
                self.assertFalse(state['complete'])
                self.write(name)

    def test_fingerprint_features_and_json_must_match(self):
        for text in ('{', '{}', '[]', json.dumps(dict(features='["other"]'))):
            with self.subTest(metadata=text):
                self.fingerprint_files()
                self.write(self.fingerprint + '/lib-mixed_lib.json', text)
                state = artifacts.cache_begin(self.root)
                artifacts.cache_record(state, self.message)
                with self.assertRaises(ValueError):
                    artifacts.cache_finish(state)
                self.assertFalse(state['complete'])

    def test_symlinked_fingerprint_is_rejected(self):
        self.fingerprint_files()
        path = self.target / self.fingerprint
        moved = self.base / 'elsewhere'
        path.rename(moved)
        path.symlink_to(moved, target_is_directory=True)
        state = artifacts.cache_begin(self.root)
        artifacts.cache_record(state, self.message)
        with self.assertRaisesRegex(ValueError, 'symlink'):
            artifacts.cache_finish(state)
        self.assertFalse(state['complete'])

    def test_unbound_public_copy_is_not_treated_as_a_cargo_hardlink(self):
        self.fingerprint_files()
        public = self.write('debug/libmixed_lib.so', self.filenames[0])
        message = copy.deepcopy(self.message)
        message['filenames'][0] = str(public)
        state = artifacts.cache_begin(self.root)
        artifacts.cache_record(state, message)
        with self.assertRaisesRegex(ValueError, 'unbound unhashed'):
            artifacts.cache_finish(state)
        self.assertFalse(state['complete'])

    def test_different_package_cannot_claim_the_same_output_namespace(self):
        other = self.root / 'other/Cargo.toml'
        other.parent.mkdir()
        other.write_text('[package]\nname="another-package"\nversion="0.1.0"\n')
        state = artifacts.cache_begin(self.root)
        artifacts.cache_record(state, self.message)
        message = dict(self.message, manifest_path=str(other), package_id='another-package')
        with self.assertRaisesRegex(ValueError, 'output collision'):
            artifacts.cache_record(state, message)

    def test_unknown_unhashed_binary_still_fails(self):
        message = copy.deepcopy(self.message)
        message['target']['kind'] = ['bin']
        state = artifacts.cache_begin(self.root)
        with self.assertRaisesRegex(ValueError, 'unique local Cargo output unit'):
            artifacts.cache_record(state, message)


class MixedLibraryWorkspaceCache(WorkspaceCache):
    """Run the existing real cold/restore/change/corruption cases with cdylib+lib."""

    def setUp(self):
        super().setUp()
        self.sources['support/Cargo.toml'] += '\n[lib]\ncrate-type=["cdylib", "lib"]\n[features]\nalternate=[]\n'
        self.checkout()

    def test_dependency_only_mixed_library_is_fresh_after_restore(self):
        self.sources['Cargo.toml'] = self.sources['Cargo.toml'].replace(
            'members=["support", "app"]', 'members=["app"]\nexclude=["support"]')
        self.checkout()
        state = self.seed()
        self.assertTrue(state['unhashed_libraries'])
        self.checkout()
        self.assertTrue(artifacts.cache_restore(self.root, self.cache)['restored'])
        units, value = self.cargo()
        self.assertTrue(all(unit['fresh'] for unit in units), units)
        self.assertEqual(value, 'alpha:outside:one:one')

    def test_repeated_mixed_library_variants_restore_the_final_features(self):
        self.seed()
        state = artifacts.cache_begin(self.root)
        self.cargo(state, extra_args=['--features', 'cache_support/alternate'])
        self.cargo(state)
        self.assertTrue(artifacts.cache_stage(state, self.cache)['save'])
        self.checkout()
        self.assertTrue(artifacts.cache_restore(self.root, self.cache)['restored'])
        units, value = self.cargo()
        local = [unit for unit in units if Path(unit['manifest_path']).is_relative_to(self.root)]
        self.assertTrue(all(unit['fresh'] for unit in local), local)
        self.assertEqual(value, 'alpha:outside:one:one')


class WorkspaceInputPaths(unittest.TestCase):
    """Exercise input retiming through the real stage/restore boundary."""

    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.base = Path(temporary.name).resolve()
        self.root = self.base / 'checkout'
        self.cache = self.base / 'cache'
        self.target = self.root / 'target'
        sources = {
            'Cargo.toml': '[workspace]\nmembers=["a", "b"]\n',
            'a/Cargo.toml': '[package]\nname="a"\nversion="0.1.0"\n',
            'a/src/lib.rs': 'pub fn value() {}\n',
            'a/build.rs': 'fn main() {}\n',
            'b/Cargo.toml': '[package]\nname="b"\nversion="0.1.0"\n',
            'b/src/lib.rs': 'pub fn other() {}\n',
            'doc/index.md': 'source input, not an ignored path\n',
            'assets/input.txt': 'original\n',
        }
        for name, text in sources.items():
            self.write(name, text)
        directory = 'debug/.fingerprint/a-' + 'a' * 16
        self.payload = ['debug/deps/liba-' + 'a' * 16 + '.rlib', directory + '/lib-a']
        for name in self.payload:
            self.write('target/' + name, 'complete fixture unit')
        state = artifacts.cache_begin(self.root)
        state['input_owners'] = {'a/Cargo.toml': ['a/build.rs'], 'b/Cargo.toml': []}
        state['files'] = self.payload
        state['directories'] = [directory]
        artifacts.cache_finish(state)
        self.assertTrue(artifacts.cache_stage(state, self.cache)['save'])
        self.manifest_path = self.cache / 'manifest.json'
        self.manifest = json.loads(self.manifest_path.read_text())
        shutil.rmtree(self.target)

    def write(self, name, text='new input'):
        path = self.root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text)
        return path

    def stamp(self, name):
        return (self.root / name).stat().st_mtime_ns

    def assert_restored(self):
        report = artifacts.cache_restore(self.root, self.cache)
        self.assertTrue(report['restored'], report)
        self.assertTrue(all((self.target / name).is_file() for name in self.payload))
        return report

    def assert_miss(self, pattern):
        before = artifacts._cache_inputs(self.root)
        report = artifacts.cache_restore(self.root, self.cache)
        self.assertFalse(report['restored'], report)
        self.assertRegex(report['reason'], pattern)
        self.assertFalse(self.target.exists())
        self.assertEqual(artifacts._cache_inputs(self.root), before)
        return report

    def test_unchanged_input_paths_preserve_exact_mtimes(self):
        report = self.assert_restored()
        self.assertEqual(report['refreshed_inputs'], 0)
        for name, metadata in self.manifest['inputs'].items():
            self.assertEqual(self.stamp(name), metadata['mtime_ns'])

    def test_added_source_promotes_owner_not_unrelated_package(self):
        added = self.write('a/src/new/mod.rs')
        os.utime(added, ns=(1, 1))
        report = self.assert_restored()
        self.assertEqual(report['input_path_changes']['added']['paths'], ['a/src/new/mod.rs'])
        self.assertEqual(report['refreshed_input_scopes'], ['a'])
        self.assertGreater(self.stamp('a/src/lib.rs'), self.manifest['inputs']['a/src/lib.rs']['mtime_ns'])
        self.assertGreater(self.stamp('a/src/new/mod.rs'), 1)
        self.assertEqual(self.stamp('b/src/lib.rs'), self.manifest['inputs']['b/src/lib.rs']['mtime_ns'])

    def test_removed_and_renamed_inputs_stay_absent(self):
        old = self.root / 'a/src/lib.rs'
        old.rename(old.with_name('renamed.rs'))
        report = self.assert_restored()
        self.assertFalse(old.exists())
        self.assertEqual(report['input_path_changes']['removed']['paths'], ['a/src/lib.rs'])
        self.assertEqual(report['input_path_changes']['added']['paths'], ['a/src/renamed.rs'])
        self.assertGreater(self.stamp('a/src/renamed.rs'), self.manifest['inputs']['a/src/lib.rs']['mtime_ns'])

    def test_document_addition_keeps_libraries_but_refreshes_observers(self):
        self.write('doc/new/index.md')
        report = self.assert_restored()
        self.assertEqual(report['refreshed_input_scopes'], ['doc'])
        for name in ('a/src/lib.rs', 'b/src/lib.rs'):
            self.assertEqual(self.stamp(name), self.manifest['inputs'][name]['mtime_ns'])
        for name in ('a/build.rs', 'doc/index.md'):
            self.assertGreater(self.stamp(name), self.manifest['inputs'][name]['mtime_ns'])
        self.assertEqual(report['refreshed_generator_inputs'], 1)

    def test_unowned_asset_removal_promotes_surviving_parent(self):
        (self.root / 'assets/input.txt').unlink()
        self.write('assets/replacement.txt')
        report = self.assert_restored()
        self.assertEqual(report['refreshed_input_scopes'], ['assets'])
        self.assertFalse((self.root / 'assets/input.txt').exists())
        self.assertEqual(self.stamp('b/src/lib.rs'), self.manifest['inputs']['b/src/lib.rs']['mtime_ns'])

    def test_new_top_level_tree_conservatively_promotes_checkout(self):
        self.write('unknown/first.txt')
        report = self.assert_restored()
        self.assertEqual(report['refreshed_input_scopes'], ['.'])
        self.assertGreater(self.stamp('b/src/lib.rs'), self.manifest['inputs']['b/src/lib.rs']['mtime_ns'])

    def test_legacy_snapshot_keeps_exact_path_support(self):
        del self.manifest['input_owners']
        self.manifest_path.write_text(json.dumps(self.manifest))
        self.assert_restored()
        shutil.rmtree(self.target)
        self.write('a/new.txt')
        report = self.assert_miss('ownership metadata')
        self.assertEqual(report['input_path_changes']['added']['paths'], ['a/new.txt'])

    def test_configuration_path_change_is_a_diagnosed_miss(self):
        self.write('.cargo/config.toml', '[build]\n')
        report = self.assert_miss('configuration paths changed')
        self.assertEqual(report['input_path_changes']['added']['paths'], ['.cargo/config.toml'])

    def test_removed_input_metadata_is_still_validated(self):
        (self.root / 'doc/index.md').unlink()
        self.manifest['inputs']['doc/index.md']['mtime_ns'] = 2 ** 63 - 1
        self.manifest_path.write_text(json.dumps(self.manifest))
        self.assert_miss('invalid cache timestamp')

    def test_removed_manifest_path_cannot_escape_source_root(self):
        self.manifest['inputs']['../escape'] = self.manifest['inputs'].pop('doc/index.md')
        self.manifest_path.write_text(json.dumps(self.manifest))
        self.assert_miss('invalid cache path')

    def test_git_and_target_cannot_be_forged_as_removed_inputs(self):
        for name in ('.git/config', 'target/source.rs'):
            with self.subTest(name=name):
                manifest = copy.deepcopy(self.manifest)
                manifest['inputs'][name] = manifest['inputs']['doc/index.md']
                self.manifest_path.write_text(json.dumps(manifest))
                self.assert_miss('invalid cache source input')

    def test_unrecorded_generator_and_bad_ownership_are_rejected(self):
        self.write('doc/new.md')
        for owners in ([], {}, {'a/Cargo.toml': ['not-observed.rs']}, {'a/Cargo.toml': ['../escape']}):
            with self.subTest(owners=owners):
                self.manifest['input_owners'] = owners
                self.manifest_path.write_text(json.dumps(self.manifest))
                self.assert_miss('metadata|unrecorded|invalid cache path')

    def test_input_diagnostics_are_bounded(self):
        for index in range(60):
            self.write(f'doc/{index:03d}.md')
        report = self.assert_restored()
        added = report['input_path_changes']['added']
        self.assertEqual(added['count'], 60)
        self.assertEqual(len(added['paths']), 50)
        self.assertTrue(added['truncated'])

    def test_retiming_failure_discards_restored_fingerprints(self):
        self.write('a/src/new.rs')
        original = os.utime
        def fail(path, *args, **kwargs):
            if Path(path) == self.root / 'a/src/lib.rs':
                raise OSError('retiming unavailable')
            return original(path, *args, **kwargs)
        with patch.object(artifacts.os, 'utime', side_effect=fail):
            report = artifacts.cache_restore(self.root, self.cache)
        self.assertFalse(report['restored'], report)
        self.assertIn('retiming unavailable', report['reason'])
        self.assertFalse(any((self.target / name).exists() for name in self.payload))

    def test_record_uses_cargo_manifest_and_generator_source(self):
        source = self.root / 'a/build.rs'
        executable = self.write('target/debug/build/a-' + 'a' * 16 + '/build-script-build')
        state = artifacts.cache_begin(self.root)
        artifacts.cache_record(state, dict(reason='compiler-artifact',
            manifest_path=str(self.root / 'a/Cargo.toml'), package_id='a',
            target=dict(name='build-script-build', kind=['custom-build'], src_path=str(source)),
            profile=dict(test=False), filenames=[str(executable)]))
        self.assertEqual(state['input_owners'], {'a/Cargo.toml': ['a/build.rs']})


class Process(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.temp = tempfile.TemporaryDirectory()
        cls.root = Path(cls.temp.name)
        source = cls.root / 'fixture.rs'
        source.write_text('#[test] fn ordinary() {}\n#[test] #[ignore] fn acceptance() {}\n#[test] #[ignore] fn crash_child() {}\n')
        cls.binary = cls.root / 'fixture'
        subprocess.run(['rustc', '--test', str(source), '-o', str(cls.binary)], check=True)

    @classmethod
    def tearDownClass(cls):
        cls.temp.cleanup()

    def execute(self, args, count=1, **kwargs):
        return run(args, self.root, os.environ, 'fixture', 'test', self.root / 'results.jsonl', expected_count=count, **kwargs)

    def test_real_libtest_lists_and_exact_ignored_execution(self):
        self.assertEqual(artifacts.inventory(self.binary, True), ['acceptance', 'crash_child'])
        self.assertTrue(self.execute([str(self.binary), '--exact', 'acceptance', '--ignored', '--test-threads=1'])['passed'])
        self.assertTrue(self.execute([str(self.binary)], count=1)['passed'])

    def test_zero_match_cannot_pass(self):
        result = self.execute([str(self.binary), '--exact', 'renamed', '--ignored'])
        self.assertEqual(result['exit_code'], 0)
        self.assertFalse(result['passed'])

    def test_real_selected_build_keeps_required_features_and_excludes_unrelated_package(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            (root / 'Cargo.toml').write_text('[workspace]\nmembers=["codec","unrelated"]\nresolver="2"\n')
            for directory, package, source in (
                ('codec', 'oasis7_viewer_codec', '#[cfg(feature="test_tier_required")] #[test] fn round_trip() { assert_eq!(1 + 1, 2); }\n'
                 '#[cfg(feature="test_tier_full")] #[test] fn extended_round_trip() { assert_eq!(2 + 2, 4); }'),
                ('unrelated', 'oasis7_runtime', 'compile_error!("unrelated target was incorrectly selected");'),
            ):
                member = root / directory
                (member / 'src').mkdir(parents=True)
                (member / 'Cargo.toml').write_text('[package]\nname="' + package +
                    '"\nversion="0.1.0"\nedition="2021"\n[features]\ntest_tier_required=[]\ntest_tier_full=[]\n')
                (member / 'src/lib.rs').write_text(source)
            config = root / 'scripts/ci-formal-process-suites.json'
            config.parent.mkdir()
            config.write_text('{}')
            subprocess.run(['cargo', 'generate-lockfile', '--offline'], cwd=root, check=True,
                           stdout=subprocess.PIPE, stderr=subprocess.PIPE)
            for command in (['git', 'init', '-q'], ['git', 'add', '.'],
                            ['git', '-c', 'user.name=Fixture', '-c', 'user.email=fixture@example.invalid',
                             'commit', '-qm', 'fixture']):
                subprocess.run(command, cwd=root, check=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
            sha = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, text=True).strip()
            plan = FastExecutionSelection().selection()
            plan['architecture'] = dict(source=sha, tested=sha)
            with patch.object(artifacts, 'ROOT', root), patch.object(artifacts, 'CONFIG', config), \
                 patch.dict(os.environ, SOURCE_SHA=sha):
                for expanded, expected_count in ((False, 1), (True, 2)):
                    plan['test_policy_changed'] = expanded
                    bundle, consumer = root / ('producer-' + str(expanded)), root / ('consumer-' + str(expanded))
                    artifacts.build(bundle, 'ordinary', plan=plan)
                    manifest = json.loads((bundle / 'manifest.json').read_text())
                    self.assertEqual({entry['package'] for entry in manifest['binaries'].values()}, {'oasis7_viewer_codec'})
                    archive_path = root / ('ordinary-' + str(expanded) + '.tar.gz')
                    artifacts.pack(bundle, archive_path, suite='ordinary', plan=plan)
                    with tarfile.open(archive_path) as archive:
                        archive.extractall(consumer, filter='data')
                    results = root / ('results-' + str(expanded) + '.jsonl')
                    artifacts.execute(consumer, 'ordinary', results, plan=plan)
                    records = [json.loads(line) for line in results.read_text().splitlines()]
                    self.assertEqual(len(records), 1)
                    self.assertTrue(records[0]['passed'])
                    self.assertEqual(records[0]['expected_count'], expected_count)
                plan['oasis7_required_packages'].append('oasis7_runtime')
                with self.assertRaisesRegex(RuntimeError, 'build failed'):
                    artifacts.build(root / 'expanded', 'ordinary', plan=plan)

    def prepare_target_workspace(self, root, source, integration=None):
        (root / 'Cargo.toml').write_text('[workspace]\nmembers=["codec","unrelated"]\nresolver="2"\n')
        for directory, package, text in (
            ('codec', 'oasis7_viewer_codec', source),
            ('unrelated', 'oasis7_runtime', 'compile_error!("unselected package was compiled");'),
        ):
            member = root / directory
            (member / 'src').mkdir(parents=True)
            (member / 'Cargo.toml').write_text('[package]\nname="' + package +
                '"\nversion="0.1.0"\nedition="2021"\n[features]\ndefault=[]\ntest_tier_required=[]\n')
            (member / 'src/lib.rs').write_text(text)
        if integration is not None:
            (root / 'codec/tests').mkdir()
            (root / 'codec/tests/pr_fast.rs').write_text(integration)
            (root / 'codec/tests/heavy.rs').write_text('compile_error!("unselected integration was compiled");')
        config = root / 'scripts/ci-formal-process-suites.json'
        config.parent.mkdir()
        config.write_text('{}')
        subprocess.run(['cargo', 'generate-lockfile', '--offline'], cwd=root, check=True,
                       stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        for command in (['git', 'init', '-q'], ['git', 'add', '.'],
                        ['git', '-c', 'user.name=Fixture', '-c', 'user.email=fixture@example.invalid',
                         'commit', '-qm', 'fixture']):
            subprocess.run(command, cwd=root, check=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        sha = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, text=True).strip()
        return config, sha

    def test_real_target_build_excludes_heavy_harnesses_and_runs_new_nonignored_cases(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            integration = ('#[cfg(feature="test_tier_required")] mod required {\n'
                           '  #[test] fn round_trip() { assert_eq!(1 + 1, 2); }\n'
                           '  #[test] fn newly_added() { assert_eq!(2 + 2, 4); }\n'
                           '  #[test] #[ignore] fn expensive() { panic!("ignored test ran"); }\n}\n')
            config, sha = self.prepare_target_workspace(root,
                '#[cfg(test)] compile_error!("unselected lib harness was compiled");\n'
                '#[cfg(feature="default")] compile_error!("default feature was incorrectly enabled");\n', integration)
            selected = FastTargetInventory().selection()
            selected['architecture'] = dict(source=sha, tested=sha)
            with patch.object(artifacts, 'ROOT', root), patch.object(artifacts, 'CONFIG', config), \
                 patch.dict(os.environ, SOURCE_SHA=sha):
                bundle, consumer = root / 'producer', root / 'consumer'
                artifacts.build(bundle, 'ordinary', plan=selected)
                manifest = json.loads((bundle / 'manifest.json').read_text())
                self.assertEqual(len(manifest['binaries']), 1)
                binary = next(iter(manifest['binaries'].values()))
                self.assertEqual(binary['target_kind'], ['test'])
                self.assertEqual(binary['target'], 'pr_fast')
                self.assertEqual(binary['features'], ['test_tier_required'])
                archive_path = root / 'ordinary.tar.gz'
                artifacts.pack(bundle, archive_path, suite='ordinary', plan=selected)
                with tarfile.open(archive_path) as archive:
                    archive.extractall(consumer, filter='data')
                results = root / 'results.jsonl'
                artifacts.execute(consumer, 'ordinary', results, plan=selected)
                records = [json.loads(line) for line in results.read_text().splitlines()]
                self.assertEqual(len(records), 1)
                self.assertTrue(records[0]['passed'])
                self.assertEqual(records[0]['expected_count'], 2)
                # Minimum case identities are checked against the actual binary,
                # including when the recorded manifest claims a case still exists.
                altered = copy.deepcopy(manifest)
                next(iter(altered['binaries'].values()))['tests'].append('invented_case')
                (consumer / 'manifest.json').write_text(json.dumps(altered))
                with self.assertRaisesRegex(ValueError, 'downloaded test inventory'):
                    artifacts.load(consumer, suite='ordinary', plan=selected)
                for label, rewritten in (
                    ('renamed', integration.replace('fn round_trip()', 'fn renamed()')),
                    ('ignored', integration.replace('#[test] fn round_trip()', '#[test] #[ignore] fn round_trip()')),
                ):
                    (root / 'codec/tests/pr_fast.rs').write_text(rewritten)
                    with self.subTest(label=label), self.assertRaisesRegex(ValueError, 'minimum case inventory'):
                        artifacts.build(root / label, 'ordinary', plan=selected)

    def test_real_full_bundle_runs_default_and_no_default_variants_of_same_lib(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            config, sha = self.prepare_target_workspace(root,
                '#[test] fn common() {}\n'
                '#[cfg(feature="default")] #[test] fn original_default() {}\n'
                '#[cfg(not(feature="default"))] #[test] fn fast_only() {}\n'
                '#[cfg(not(feature="default"))] #[test] fn newly_added() {}\n')
            target = FastTargetInventory().target(target_kind='lib', target_name='oasis7_viewer_codec', cases=['fast_only'])
            selected = FastTargetInventory().selection([target], full=True)
            selected['architecture'] = dict(source=sha, tested=sha)
            # Restrict only the old package inventory to this real tiny workspace;
            # build, Cargo feature resolution, pack, download checks and run stay real.
            with patch.object(artifacts, 'ROOT', root), patch.object(artifacts, 'CONFIG', config), \
                 patch.object(artifacts, 'required_packages', return_value=['oasis7_viewer_codec']), \
                 patch.dict(os.environ, SOURCE_SHA=sha):
                bundle, consumer = root / 'producer', root / 'consumer'
                artifacts.build(bundle, 'ordinary', plan=selected)
                manifest = json.loads((bundle / 'manifest.json').read_text())
                self.assertEqual(len(manifest['binaries']), 2)
                self.assertEqual({entry['target'] for entry in manifest['binaries'].values()}, {'oasis7_viewer_codec'})
                self.assertEqual(sorted('default' in entry['features'] for entry in manifest['binaries'].values()), [False, True])
                archive_path = root / 'ordinary.tar.gz'
                artifacts.pack(bundle, archive_path, suite='ordinary', plan=selected)
                with tarfile.open(archive_path) as archive:
                    archive.extractall(consumer, filter='data')
                results = root / 'results.jsonl'
                artifacts.execute(consumer, 'ordinary', results, plan=selected)
                records = [json.loads(line) for line in results.read_text().splitlines()]
                self.assertEqual(sorted(record['expected_count'] for record in records), [2, 3])
                self.assertTrue(all(record['passed'] for record in records))
                original = json.loads((consumer / 'manifest.json').read_text())
                for logical in manifest['binaries']:
                    altered = copy.deepcopy(original)
                    altered['included_binaries'].remove(logical)
                    (consumer / 'manifest.json').write_text(json.dumps(altered))
                    with self.subTest(missing=logical), self.assertRaisesRegex(ValueError, 'executable closure'):
                        artifacts.load(consumer, suite='ordinary', plan=selected)
                (consumer / 'manifest.json').write_text(json.dumps(original))
                for logical, entry in manifest['binaries'].items():
                    path = consumer / entry['path']
                    data = path.read_bytes()
                    path.unlink()
                    try:
                        with self.subTest(missing_file=logical), self.assertRaisesRegex(ValueError, 'corrupt executable'):
                            artifacts.load(consumer, suite='ordinary', plan=selected)
                    finally:
                        path.write_bytes(data)
                        path.chmod(0o755)

    def test_absolute_timeout_is_not_reset_by_heartbeats(self):
        result = self.execute([sys.executable, '-c', 'import time; time.sleep(30)'], count=None, timeout_seconds=0.2, heartbeat_seconds=0.05)
        self.assertTrue(result['timeout'])
        self.assertFalse(result['passed'])
        self.assertEqual(result['cleanup_result'], 'clean')
        self.assertLess(result['elapsed_seconds'], 3)

    def test_fresh_consumer_restores_compiled_cargo_binary_path_and_prior_attempt(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp).resolve()
            original = root / 'target/debug/tool'
            original.parent.mkdir(parents=True)
            source = root / 'tool.rs'
            source.write_text('fn main() {}')
            subprocess.run(['rustc', str(source), '-o', str(original)], check=True)
            harness = root / 'harness'
            source.write_text('#[test] fn calls_tool() { assert!(std::process::Command::new(env!("CARGO_BIN_EXE_tool")).status().unwrap().success()); }')
            subprocess.run(['rustc', '--test', str(source), '-o', str(harness)], check=True,
                           env=dict(os.environ, CARGO_BIN_EXE_tool=str(original)))
            bundle = root / 'bundle'
            bundle.mkdir()
            config_path = root / 'suites.json'
            config = dict(suites={'fixture': []}, environment_formal_batches={}, legacy_ignored_filters={}, shell_tests={})
            config_path.write_text(json.dumps(config))
            binaries = {}
            for logical in ['viewer', 'runtime', 'endpoint', 'runtime-tests']:
                destination = bundle / logical
                destination.write_bytes((harness if logical == 'runtime-tests' else original).read_bytes())
                destination.chmod(0o755)
                binaries[logical] = dict(path=logical, sha256=artifacts.digest(destination),
                    original_executable=str(harness if logical == 'runtime-tests' else original),
                    test=logical == 'runtime-tests', ordinary=logical == 'runtime-tests', cwd='.',
                    tests=['calls_tool'], ignored_tests=[])
            # Include a legitimate empty Cargo harness and the two original
            # default-feature regressions. Ordinary execution must skip only
            # the empty batch, while retaining both snapshot combinations.
            for logical, rust_source in {
                'empty-tests': '',
                'viewer-snapshot-tests': '#[cfg(test)] mod snapshot_progress { #[test] fn one() {} }',
                'snapshot-tests': '#[test] fn snapshot_player_gameplay_execution_state_backfills_from_legacy_fields() {}',
            }.items():
                source.write_text(rust_source)
                destination = bundle / logical
                subprocess.run(['rustc', '--test', str(source), '-o', str(destination)], check=True)
                binaries[logical] = dict(path=logical, sha256=artifacts.digest(destination),
                    original_executable=str(destination), test=True, ordinary=logical == 'empty-tests', cwd='.',
                    tests=artifacts.inventory(destination), ignored_tests=[])
            original.unlink()
            manifest = dict(tested_sha='tested', source_sha='source', producer_run_id='run', producer_attempt='1',
                checkout=str(root), config_sha256=artifacts.digest(config_path),
                rust_toolchain=os.environ.get('RUST_TOOLCHAIN', 'local'), runner_image=os.environ.get('ImageOS', sys.platform),
                binaries=binaries, suites={'fixture': []}, libraries={})
            (bundle / 'manifest.json').write_text(json.dumps(manifest))
            with patch.object(artifacts, 'ROOT', root), patch.object(artifacts, 'CONFIG', config_path), patch.object(artifacts, 'output', return_value='tested'), patch.dict(os.environ, SOURCE_SHA='source', GITHUB_RUN_ID='run', GITHUB_RUN_ATTEMPT='2'):
                restored, env = artifacts.load(bundle)
                self.assertEqual(restored['producer_attempt'], '1')
                self.assertTrue(original.is_file())
                with patch('ci_process_runner.run', wraps=run) as child_runner:
                    artifacts.execute(bundle, 'ordinary', root / 'results.jsonl')
                    self.assertTrue(all('--test-threads=1' not in call.args[0] for call in child_runner.call_args_list))
                    self.assertTrue(all('--nocapture' not in call.args[0] for call in child_runner.call_args_list))
                    commands = [call.args[0] for call in child_runner.call_args_list]
                    self.assertIn([str(bundle / 'viewer-snapshot-tests'), 'snapshot_progress::'], commands)
                    self.assertIn([str(bundle / 'snapshot-tests'), 'snapshot_player_gameplay_execution_state_backfills_from_legacy_fields'], commands)
                results = [json.loads(line) for line in (root / 'results.jsonl').read_text().splitlines()]
                self.assertEqual(len(results), 3)
                self.assertTrue(all(result['passed'] for result in results))

    def test_proc_scan_never_reads_other_users_environment(self):
        class OtherUser:
            def stat(self):
                return type('Stat', (), {'st_uid': os.getuid()+1})()
            def __truediv__(self, name):
                raise AssertionError('must not read another user process')
        with patch.object(Path, 'glob', return_value=[OtherUser()]):
            self.assertEqual(owned_processes('owned-test'), {})

    def test_packed_moved_case_preserves_logical_suite_and_restores_child(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp).resolve()
            original = root / 'target/debug/tool'
            original.parent.mkdir(parents=True)
            source = root / 'fixture.rs'
            source.write_text('fn main() {}')
            subprocess.run(['rustc', str(source), '-o', str(original)], check=True)
            bundle = root / 'bundle'
            bundle.mkdir()
            source.write_text('#[test] #[ignore] fn formal_acceptance() { assert!(std::process::Command::new(env!("CARGO_BIN_EXE_tool")).status().unwrap().success()); }\n#[test] fn ordinary() {}\n#[test] #[ignore] fn crash_child() {}\n')
            subprocess.run(['rustc', '--test', str(source), '-o', str(bundle / 'runtime-tests')], check=True,
                           env=dict(os.environ, CARGO_BIN_EXE_tool=str(original)))
            binaries = {}
            for name in ('runtime', 'runtime-tests', 'viewer'):
                destination = bundle / name
                if name != 'runtime-tests':
                    destination.write_bytes(original.read_bytes())
                    destination.chmod(0o755)
                binaries[name] = dict(path=name, sha256=artifacts.digest(destination),
                    original_executable=str(original), test=name == 'runtime-tests', ordinary=False,
                    cwd='.', tests=artifacts.inventory(destination) if name == 'runtime-tests' else [],
                    ignored_tests=artifacts.inventory(destination, True) if name == 'runtime-tests' else [],
                    libraries=[])
            config_path = root / 'suites.json'
            config = dict(schema_version=2, suites={'healthy-set': [dict(binary='runtime-tests', name='formal_acceptance', ignored=True)]},
                environment_formal_batches={}, legacy_ignored_filters={'runtime-tests': ['formal_acceptance']},
                shell_tests={}, auxiliary_entries={'runtime-tests': ['crash_child']},
                binary_dependencies={'runtime-tests': ['runtime']},
                case_shards={'runtime-tests': {'formal_acceptance': 'storage-recovery'}})
            authority_path = root / 'trusted-suites.json'
            authority_path.write_text(json.dumps(config))
            candidate = copy.deepcopy(config)
            candidate['suites']['healthy-set'] = []
            candidate['legacy_ignored_filters'] = {}
            config_path.write_text(json.dumps(candidate))
            manifest = dict(schema_version=2, scope='formal', tested_sha='tested', source_sha='source',
                producer_run_id='run', checkout=str(root), config_sha256=artifacts.digest(config_path),
                formal_authority_sha256=artifacts.digest(authority_path),
                rust_toolchain=os.environ.get('RUST_TOOLCHAIN', 'local'), runner_image=os.environ.get('ImageOS', sys.platform),
                binaries=binaries, suites=artifacts.resolve_suites(config, binaries), libraries={})
            (bundle / 'manifest.json').write_text(json.dumps(manifest))
            archive_path, consumer = root / 'storage-recovery.tar.gz', root / 'consumer'
            with patch.object(artifacts, 'ROOT', root), patch.object(artifacts, 'CONFIG', config_path), \
                 patch.object(artifacts, 'FORMAL_AUTHORITY', authority_path), patch.object(artifacts, 'output', return_value='tested'), \
                 patch.dict(os.environ, SOURCE_SHA='source', GITHUB_RUN_ID='run'):
                artifacts.pack(bundle, archive_path, shard='storage-recovery')
                with tarfile.open(archive_path) as archive:
                    archive.extractall(consumer, filter='data')
                self.assertFalse((consumer / 'viewer').exists())
                original.unlink()
                results = root / 'results.jsonl'
                artifacts.execute(consumer, 'ordinary', results, shard='storage-recovery')
                self.assertTrue(original.is_file())
                records = [json.loads(line) for line in results.read_text().splitlines()]
                self.assertEqual(len(records), 1)
                self.assertTrue(records[0]['passed'])
                self.assertEqual(records[0]['suite'], 'healthy-set')
                self.assertEqual(records[0]['shard'], 'storage-recovery')
                self.assertEqual(records[0]['test_name'], 'formal_acceptance')
                previous = results.read_text()
                original_open = Path.open
                def reject_final_append(path, *args, **kwargs):
                    if path == results and args == ('a',):
                        raise OSError('final journal is not writable')
                    return original_open(path, *args, **kwargs)
                with patch.object(Path, 'open', reject_final_append):
                    with self.assertRaisesRegex(OSError, 'final journal is not writable'):
                        artifacts.execute(consumer, 'ordinary', results, shard='storage-recovery')
                self.assertEqual(results.read_text(), previous)
                with self.assertRaisesRegex(ValueError, 'selection mismatch'):
                    artifacts.load(consumer, shard='healthy-set')
                manifest_path = consumer / 'manifest.json'
                packaged = json.loads(manifest_path.read_text())
                changed = copy.deepcopy(packaged)
                changed['included_binaries'].remove('runtime')
                manifest_path.write_text(json.dumps(changed))
                with self.assertRaisesRegex(ValueError, 'closure mismatch'):
                    artifacts.load(consumer, shard='storage-recovery')
                changed = copy.deepcopy(packaged)
                changed['binaries']['runtime-tests']['tests'].append('invented_case')
                manifest_path.write_text(json.dumps(changed))
                with self.assertRaisesRegex(ValueError, 'downloaded test inventory mismatch'):
                    artifacts.load(consumer, shard='storage-recovery')

    def test_proc_scan_ignores_inaccessible_environment_of_unowned_same_uid_process(self):
        class DeniedProcess:
            name = '123456'
            def stat(self):
                return type('Stat', (), {'st_uid': os.getuid()})()
            def __truediv__(self, name):
                if name == 'stat':
                    return type('StatFile', (), {'read_text': lambda self: '123456 (unrelated) S 1 42 42 ' + '0 '*15 + '99'})()
                return type('Denied', (), {'read_bytes': lambda self: (_ for _ in ()).throw(PermissionError('denied'))})()
        with patch.object(Path, 'glob', return_value=[DeniedProcess()]):
            self.assertEqual(owned_processes('owned-test'), {})
            self.assertEqual(owned_processes('owned-test', group=42), {123456: '99'})

    def test_explicit_checkout_also_owns_default_results(self):
        with patch.object(artifacts, 'ROOT', artifacts.ROOT), patch.object(artifacts, 'CONFIG', artifacts.CONFIG), patch.object(artifacts, 'execute') as execute:
            with patch.object(sys, 'argv', ['ci-test-artifacts.py', 'run', '--repo-root', str(self.root), '--bundle', str(self.root / 'bundle')]):
                artifacts.main()
            self.assertEqual(execute.call_args.args[2], self.root.resolve() / 'output/formal-process/results.jsonl')

    def test_corrupt_executable_and_identity_fail_before_execution(self):
        bundle = self.root / 'bundle'
        bundle.mkdir(exist_ok=True)
        entry = dict(path='fixture', sha256='wrong')
        expected = dict(tested_sha='tested', source_sha='source', producer_run_id='run', checkout=str(artifacts.ROOT), config_sha256=artifacts.digest(artifacts.CONFIG), rust_toolchain=os.environ.get('RUST_TOOLCHAIN', 'local'), runner_image=os.environ.get('ImageOS', sys.platform), binaries={'test':entry})
        (bundle / 'fixture').write_bytes(self.binary.read_bytes())
        (bundle / 'fixture').chmod(0o755)
        (bundle / 'manifest.json').write_text(json.dumps(expected))
        with patch.object(artifacts, 'output', return_value='tested'), patch.dict(os.environ, SOURCE_SHA='source', GITHUB_RUN_ID='run'):
            with self.assertRaisesRegex(ValueError, 'corrupt executable'):
                artifacts.load(bundle)
            expected['tested_sha'] = 'another-commit'
            (bundle / 'manifest.json').write_text(json.dumps(expected))
            with self.assertRaisesRegex(ValueError, 'tested_sha'):
                artifacts.load(bundle)


if __name__ == '__main__':
    unittest.main()
