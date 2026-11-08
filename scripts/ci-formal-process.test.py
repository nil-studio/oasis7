#!/usr/bin/env python3
"""Coverage, transition-gate and real libtest runner regressions."""
import copy
from contextlib import ExitStack
from concurrent.futures import ThreadPoolExecutor
import hashlib
import importlib.util
import itertools
import json
import os
from pathlib import Path
import re
import shutil
import signal
import subprocess
import sys
import tempfile
import tarfile
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
plan = module('ci-formal-plan')
from ci_process_runner import run, owned_processes, _Capture, READ_BYTES, SUMMARY_LINE_BYTES



class ProviderArtifactContracts(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.bundle = self.root / 'bundle'
        (self.bundle / artifacts.ProviderArtifact.DIRECTORY).mkdir(parents=True)
        self.expected = dict(tested_sha='a' * 40, source_sha='b' * 40)
        files = {}
        for kind, name in [('wasm',artifacts.ProviderArtifact.MODULE+'.wasm'), ('metadata',artifacts.ProviderArtifact.MODULE+'.metadata.json'),
                           ('receipt',artifacts.ProviderArtifact.MODULE+'.build-receipt.json'), ('checker','sync_builtin_wasm_identity'+('.exe' if os.name=='nt' else ''))]:
            name = artifacts.ProviderArtifact.DIRECTORY + '/' + name
            path = self.bundle / name
            path.write_bytes(b'synthetic contract data; not a valid canonical artifact')
            path.chmod(0o700 if kind == 'checker' else 0o600)
            files[kind] = dict(path=name, sha256=artifacts.ProviderArtifact.digest(path))
        self.record = dict(self.expected, checkout_tree_sha256='tree',files=files,libraries=[])
        self.tree = patch.object(artifacts.ProviderArtifact,'tree_digest',return_value='tree')
        self.tree.start()
        self.addCleanup(self.tree.stop)
        # An explicit mock of the official verifier, never synthetic provenance
        # accepted in production. This contract checks invocation and fail-closed
        # orchestration; real source/receipt validity is a separate Rust/build test.
        self.invoke = patch.object(artifacts.ProviderArtifact.subprocess,'run')
        self.native = self.invoke.start()
        self.addCleanup(self.invoke.stop)

    def validate(self, record=None):
        return artifacts.ProviderArtifact.validate(self.root,self.bundle,self.record if record is None else record,self.expected)

    def test_expanded_native_provider_material_follows_actual_full_consumer(self):
        manifest = dict(selection={"suite": "ordinary"},
                        execution_plan=dict(test_policy_changed=True),
                        binaries={"runtime-tests": dict(package="oasis7_chain_runtime", test=True, features=["test_tier_full"])})
        self.assertTrue(artifacts.preview_provider_required(manifest))
        for key, value in [("package", "oasis7_viewer_codec"), ("test", False), ("features", ["test_tier_required"])]:
            changed = copy.deepcopy(manifest)
            changed["binaries"]["runtime-tests"][key] = value
            self.assertFalse(artifacts.preview_provider_required(changed))
        manifest["execution_plan"] = dict(scope="targeted", execution_profile="pr-fast")
        self.assertFalse(artifacts.preview_provider_required(manifest))

    def test_merge_dual_identity_and_exact_official_checker(self):
        environment = self.validate()
        self.assertEqual(set(environment),set(artifacts.ProviderArtifact.VARIABLES))
        command = self.native.call_args.args[0]
        self.assertEqual(command[1:3], ['--verify-provider-artifact',str(self.root)])
        self.assertEqual(len(command),6)
        self.assertNotIn('cargo', command)
        self.assertNotEqual(self.record['tested_sha'],self.record['source_sha'])

    def test_missing_pair_hash_and_source_fail_closed_before_checker(self):
        mutations = [lambda r:r['files'].pop('wasm'),
                     lambda r:r.update(source_sha='c'*40),
                     lambda r:r.update(tested_sha='c'*40),
                     lambda r:r.update(checkout_tree_sha256='other-tree'),
                     lambda r:r['files']['metadata'].update(sha256='0'*64)]
        for mutate in mutations:
            record=copy.deepcopy(self.record)
            mutate(record)
            with self.subTest(record=record), self.assertRaises(ValueError): self.validate(record)
        self.native.assert_not_called()

    def test_bad_path_and_symlink_fail_closed(self):
        record=copy.deepcopy(self.record)
        record['files']['wasm']['path']='../foreign.wasm'
        with self.assertRaises(ValueError): self.validate(record)
        wasm=self.bundle/self.record['files']['wasm']['path']
        foreign=self.root/'foreign'
        foreign.write_bytes(wasm.read_bytes())
        wasm.unlink()
        wasm.symlink_to(foreign)
        with self.assertRaises(ValueError): self.validate()
        self.native.assert_not_called()

    def test_official_checker_failure_cannot_inject_environment(self):
        self.native.side_effect=subprocess.CalledProcessError(2,['official-checker'])
        with self.assertRaises(subprocess.CalledProcessError): self.validate()


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
        for binary, overrides in self.config.get('case_shards', {}).items():
            self.binaries[binary]['tests'].extend(name for name in overrides
                                                if name not in self.binaries[binary]['tests'])

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

    def test_initial_lost_reply_runs_unconditionally_under_trusted_parent(self):
        name = ('formal_live_runtime::tests::protected_process_tests::'
                'protected_endpoint_children_submit_qualify_restart_and_read_from_external_pin')
        suites = artifacts.resolve_suites(self.config, self.binaries)
        shards = artifacts.resolve_shards(self.config, suites)
        selected = [(shard, entry) for shard, entries in shards.items()
                    for entry in entries if entry.get('name') == name]
        self.assertEqual(len(selected), 1)
        shard, entry = selected[0]
        self.assertEqual(shard, 'runtime-protocol')
        self.assertEqual(entry['binary'], 'runtime-tests')
        self.assertTrue(entry['ignored'])
        self.assertIn('endpoint', self.config['binary_dependencies'][entry['binary']])
        source = (HERE.parent / 'crates/oasis7_chain_runtime/src/host/formal_live_protected_process_tests.rs').read_text()
        parent = re.search(r'fn protected_endpoint_children_submit_qualify_restart_and_read_from_external_pin\(\) \{(.*?)\n\}', source, re.S).group(1)
        self.assertRegex(parent, r'run_on_fixture_stack\(false\)\.unwrap\(\);\s*//[^\n]*\n\s*protected_initial_closure_lost_reply_recovers_original_with_new_coordinator_session\(\);')
        helper = 'protected_initial_closure_lost_reply_recovers_original_with_new_coordinator_session'
        self.assertNotIn(helper, [e.get('name', '').split('::')[-1] for values in suites.values() for e in values])
        self.assertIn('FixtureCase::InitialLostResponse', source)

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
        fixture = Coverage()
        fixture.setUp()
        manifest = dict(execution_plan=plan, suites=artifacts.resolve_suites(config, fixture.binaries))
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
        expected = [['test', *args, '--no-run'] for args in commands]
        if sys.platform == 'linux' and hasattr(artifacts, 'RUNTIME_WORKER_VARIANT'):
            expected.append(['build', '-p', 'oasis7_wasm_executor', '--no-default-features',
                             '--features', 'wasmtime', '--bin', 'oasis7-wasm-worker'])
        self.assertEqual([command for _, command, _ in variants], expected)
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
            if sys.platform == 'linux' and hasattr(artifacts, 'RUNTIME_WORKER_VARIANT'):
                worker = (artifacts.RUNTIME_WORKER_VARIANT,
                          ['build', '-p', 'oasis7_wasm_executor', '--no-default-features',
                           '--features', 'wasmtime', '--bin', 'oasis7-wasm-worker'], ['wasmtime'])
                self.assertEqual(full_variants[-1], worker)
                self.assertEqual(original[-1], worker)
                full_variants, original = full_variants[:-1], original[:-1]
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


class SupportHandoffSelection(unittest.TestCase):
    def selection(self):
        owner = 'oasis7_game_client'
        return InlineExecutionSelection().selection() | {
            'groups': ['baseline', 'rust_baseline', 'oasis7_required'],
            'ordinary_clippy_group': 'rust_baseline',
            'ordinary_support_contract_version': 1,
            'ordinary_support_handoffs': [owner],
            'ordinary_support_deferred': ['oasis7_agent_harness', 'oasis7_model_provider'],
            'oasis7_required_packages': [owner],
            'workspace_support_profiles': [],
            'ordinary_fast_targets': [FastTargetInventory().target(
                package=owner, target_kind='lib', target_name=owner, default_features=True, features=[]),
                FastTargetInventory().target(package=owner, target_name='dev_test_service_cli',
                                             default_features=True, features=[])]}

    def test_every_original_support_clippy_survives_without_duplicate_owner_lint(self):
        selected = self.selection()
        self.assertEqual(artifacts.required_native_commands(selected), [
            ['-p', 'oasis7_agent_harness', '--all-targets'],
            ['-p', 'oasis7_game_client', '--all-targets'],
            ['-p', 'oasis7_model_provider', '--all-targets']])
        # Tests retain precise targets; all-targets is a lint obligation only.
        variants = artifacts.build_variants('ordinary', selected)
        self.assertEqual(len(variants), 1)
        self.assertIn('--lib', variants[0][1])
        self.assertIn('dev_test_service_cli', variants[0][1])
        self.assertNotIn('--all-targets', variants[0][1])

    def test_handoff_requires_complete_same_default_library_and_unique_baseline_owner(self):
        for field, value in (
            ('ordinary_support_contract_version', True),
            ('ordinary_support_contract_version', 2),
            ('ordinary_support_handoffs', []),
            ('ordinary_support_handoffs', ['oasis7_model_provider']),
            ('ordinary_support_deferred', ['oasis7_game_client']),
            ('ordinary_clippy_group', 'oasis7_required'),
            ('workspace_support_profiles', ['oasis7_model_provider']),
            ('ordinary_execution', 'artifact'),
        ):
            with self.subTest(field=field, value=value), self.assertRaises(ValueError):
                artifacts.required_native_commands(self.selection() | {field: value})
        for changes in ({'default_features': False}, {'features': ['new-feature']},
                        {'target_kind': 'test'}):
            selected = self.selection()
            selected['ordinary_fast_targets'][0].update(changes)
            with self.subTest(changes=changes), self.assertRaises(ValueError):
                artifacts.required_native_commands(selected)
        for field in ('ordinary_support_contract_version', 'ordinary_support_deferred'):
            selected = self.selection()
            del selected[field]
            with self.subTest(missing=field), self.assertRaises(ValueError):
                artifacts.required_native_commands(selected)


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

    def test_inline_build_combines_only_identical_package_feature_selections(self):
        target = FastTargetInventory().target
        targets = [
            target(target_name='first'),
            target(target_kind='lib', target_name='oasis7_viewer_codec'),
            target(target_name='different_features', features=[]),
            target(target_name='different_defaults', default_features=True),
            target(package='oasis7_runtime', target_name='runtime'),
        ]
        selected = self.selection() | {'ordinary_fast_targets': targets}
        variants = artifacts.build_variants('ordinary', selected)
        self.assertEqual(len(variants), 4)
        self.assertEqual(variants[0], ('required-fast-0',
            ['test', '-p', 'oasis7_viewer_codec', '--test', 'first', '--lib',
             '--no-default-features', '--features', 'test_tier_required', '--no-run'],
            ['test_tier_required']))
        self.assertEqual([name for name, _, _ in variants],
                         ['required-fast-0', 'required-fast-2', 'required-fast-3', 'required-fast-4'])
        # Clippy still checks every exact original target and feature contract.
        self.assertEqual(len(artifacts.required_native_commands(selected)), len(targets))
        for capability in (None, 2):
            legacy = dict(selected)
            if capability is None:
                del legacy['fast_execution_version']
            else:
                legacy['fast_execution_version'] = capability
            self.assertEqual(len(artifacts.build_variants('ordinary', legacy)), len(targets))
        full = FastTargetInventory().selection(targets, full=True)
        supplements = [v for v in artifacts.build_variants('ordinary', full)
                       if v[0].startswith('required-full-fast-')]
        self.assertEqual(len(supplements), len(targets))


class DistfsClippyHandoff(unittest.TestCase):
    def selection(self):
        selected = NativePackagePartitions().selection()
        for target in selected['ordinary_fast_targets']:
            if target['target_kind'] == 'test':
                target['target_name'] = ('pr_fast_incremental' if target['package'] == 'oasis7_runtime'
                                         else 'pr_fast_reducers')
        return selected | {
            'distfs_clippy_contract_version': 1, 'distfs_clippy_group': 'rust_baseline',
            'groups': ['baseline', 'rust_baseline', 'oasis7_required', 'distfs'],
            'formal_suites': [], 'fast_plan_exceptions': [],
            'architecture': dict(mode='targeted', build_policy_version=1,
                build_policy='ordinary-targets', consumer_groups={'distfs': ['oasis7_distfs']}),
        }

    def test_original_lint_inventory_has_one_owner_and_keeps_both_required_groups(self):
        selected = self.selection()
        expected = [['-p', 'oasis7_cas', '-p', 'oasis7_formal_protocol', '--lib'],
                    ['-p', 'oasis7_distfs', '--lib', '--bin', 'oasis7_durability_endpoint']]
        self.assertEqual(artifacts.required_native_commands(selected)[-2:], expected)
        self.assertEqual(artifacts.required_arguments(selected, 'distfs-clippy-group'), ['rust_baseline'])
        legacy = {key: value for key, value in selected.items()
                  if key not in ('distfs_clippy_contract_version', 'distfs_clippy_group')}
        self.assertEqual(len(artifacts.required_native_commands(legacy)), 4)
        for plan in (None, {}, legacy, FastTargetInventory().selection(full=True)):
            self.assertEqual(artifacts.required_arguments(plan, 'distfs-clippy-group'), ['distfs'])
        checker = module('ci-required-result')
        jobs = {group: group.replace('_', '-') for group in selected['groups']}
        successful = {name: {'result': 'success'} for name in ['select', *jobs.values()]}
        self.assertTrue(checker.validate(selected, successful, jobs))
        for owner in ('rust-baseline', 'distfs'):
            for failure in ('failure', 'cancelled', 'skipped', None):
                with self.subTest(owner=owner, status=failure), self.assertRaises(ValueError):
                    checker.validate(selected, successful | {owner: {'result': failure}}, jobs)

    def test_partial_untrusted_or_incomplete_handoff_cannot_skip_lint(self):
        selected = self.selection()
        invalid = [('distfs_clippy_contract_version', value) for value in (True, None, '1', 2)]
        invalid += [('distfs_clippy_group', value) for value in ('distfs', None, 'other')]
        invalid += [('ordinary_clippy_group', 'oasis7_required'), ('ordinary_execution', 'artifact'),
                    ('fast_execution_version', 2), ('scope', 'full'), ('execution_profile', 'candidate-full'),
                    ('test_policy_changed', True), ('fast_plan_exceptions', ['extra']),
                    ('formal_suites', ['world-history']), ('oasis7_required_wasm_packages', ['oasis7_runtime']),
                    ('ordinary_partitions', ['all'])]
        invalid += [('groups', [group for group in selected['groups'] if group != missing])
                    for missing in ('distfs', 'rust_baseline', 'oasis7_required')]
        candidates = [selected | {key: value} for key, value in invalid]
        candidates += [{key: value for key, value in selected.items() if key != missing}
                       for missing in ('distfs_clippy_contract_version', 'distfs_clippy_group', 'fast_plan_version')]
        candidates += [selected | {'ordinary_fast_targets': selected['ordinary_fast_targets'][:-1]},
                       selected | {'ordinary_fast_targets': selected['ordinary_fast_targets'][:1] * 4}]
        for changes in ({'features': []}, {'default_features': True}, {'target_name': 'unknown'},
                        {'target_kind': []}, {'cases': []}):
            candidate = copy.deepcopy(selected)
            candidate['ordinary_fast_targets'][0].update(changes)
            candidates.append(candidate)
        for candidate in candidates:
            with self.subTest(plan=candidate):
                with self.assertRaises(ValueError):
                    artifacts.required_arguments(candidate, 'distfs-clippy-group')
                with self.assertRaises(ValueError):
                    artifacts.required_native_commands(candidate)

    def test_whole_driver_moves_exact_lints_preserves_tests_and_propagates_failures(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            repo, binary = root / 'repo', root / 'bin'
            scripts = repo / 'scripts'
            scripts.mkdir(parents=True)
            binary.mkdir()
            (scripts / 'cargo-dev-architecture.py').write_text('print("architecture fixture")\n')
            for name in ('check-rustsec-ignore-baseline.test.sh', 'check-rustsec-ignore-baseline.sh',
                         'ensure-cargo-deny.sh', 'check-rust-file-size.test.sh', 'check-rust-file-size.sh'):
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
            selected = self.selection() | {'source_sha': sha, 'tested_sha': sha}
            selected['architecture'].update(source=sha, tested=sha)
            selection, log = root / 'selection.json', root / 'cargo.jsonl'
            original_lints = [
                ['clippy', '-p', 'oasis7_cas', '-p', 'oasis7_formal_protocol', '--lib'],
                ['clippy', '-p', 'oasis7_distfs', '--lib', '--bin', 'oasis7_durability_endpoint']]
            original_lints = [args + ['--', '-D', 'warnings', '-D', 'clippy::correctness',
                                      '-D', 'clippy::suspicious'] for args in original_lints]
            original_tests = [
                ['test', '-p', 'oasis7_cas', '-p', 'oasis7_formal_protocol', '--lib', '--', '--test-threads=1'],
                ['test', '-p', 'oasis7_distfs', '--lib', '--bin', 'oasis7_durability_endpoint', '--', '--test-threads=1']]
            for mode, failed in (('handoff', ''), ('legacy', ''), ('handoff', 'oasis7_cas'),
                                 ('handoff', 'oasis7_distfs'), ('manual', '')):
                current = copy.deepcopy(selected)
                if mode == 'legacy':
                    del current['distfs_clippy_contract_version'], current['distfs_clippy_group']
                selection.write_text(json.dumps(current))
                observed = {}
                for group in (('distfs',) if mode == 'manual' else ('rust_baseline', 'distfs')):
                    log.unlink(missing_ok=True)
                    plan_args = [] if mode == 'manual' else ['--plan', str(selection)]
                    result = subprocess.run(['bash', str(HERE / 'ci-tests.sh'), 'required', '--group', group,
                        *plan_args, '--repo-root', str(repo)], text=True, capture_output=True,
                        env=dict(os.environ, PATH=str(binary) + os.pathsep + os.environ['PATH'], SOURCE_SHA=sha,
                                 CI_VERBOSE='', CARGO_LOG=str(log), FAIL_CLIPPY_PACKAGE=failed,
                                 OASIS7_CI_RUN_PROVIDER_LIVE_GATE='false', OASIS7_CI_RUN_HOSTED_ACCOUNT_SMOKE='false'))
                    commands = [json.loads(line) for line in log.read_text().splitlines()]
                    observed[group] = [args for args in commands if args[0] == 'clippy' and args[2] in ('oasis7_cas', 'oasis7_distfs')]
                    with self.subTest(mode=mode, failed=failed, group=group):
                        self.assertEqual(result.returncode, 19 if group == 'rust_baseline' and failed else 0,
                                         result.stdout + result.stderr)
                        if group == 'distfs':
                            self.assertEqual([args for args in commands if args[0] == 'test'], original_tests)
                owner = 'rust_baseline' if mode == 'handoff' else 'distfs'
                self.assertEqual(observed[owner], original_lints[:1] if failed == 'oasis7_cas' else original_lints)
                self.assertEqual(observed.get('distfs' if owner == 'rust_baseline' else 'rust_baseline', []), [])
            for field in ('source_sha', 'tested_sha'):
                selection.write_text(json.dumps(selected | {field: '0' * 40}))
                log.unlink(missing_ok=True)
                result = subprocess.run(['bash', str(HERE / 'ci-tests.sh'), 'required', '--group', 'distfs',
                    '--plan', str(selection), '--repo-root', str(repo)], text=True, capture_output=True,
                    env=dict(os.environ, PATH=str(binary) + os.pathsep + os.environ['PATH'], SOURCE_SHA=sha,
                             CI_VERBOSE='', CARGO_LOG=str(log), FAIL_CLIPPY_PACKAGE=''))
                self.assertNotEqual(result.returncode, 0)
                self.assertIn('execution plan identity mismatch', result.stderr)
                self.assertFalse(log.exists())


class CompleteArtifactPartitions(unittest.TestCase):
    def selection(self):
        return FastExecutionSelection().selection() | dict(
            scope='full', ordinary_artifact_partition_version=1,
            ordinary_partitions=['controls', 'runtime', 'component-libs', 'integrations-0', 'integrations-1'])

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
        self.assertEqual(len(groups['integrations-0']), 2)
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
             patch('ci_process_runner.run', return_value=dict(passed=False, cleanup_result='clean')) as runner:
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
        fixture = Coverage()
        fixture.setUp()
        selected = FastExecutionSelection().selection(suites=('runtime-protocol',))
        self.assertEqual(set(artifacts.formal_shards(selected)), {'runtime-protocol', 'healthy-set', 'lifecycle-cli'})
        selected['formal_shards'] = artifacts.formal_shards(selected)
        manifest = dict(execution_plan=selected, suites=artifacts.resolve_suites(fixture.config, fixture.binaries))
        combined = []
        for shard in selected['formal_shards']:
            entries = artifacts.selected_entries(manifest, fixture.config, shard=shard)
            self.assertTrue(entries, shard)
            self.assertTrue(all(entry['suite'] == 'runtime-protocol' for entry in entries))
            combined.extend(entries)
        self.assertEqual(len(combined), len(manifest['suites']['runtime-protocol']))
        self.assertEqual(len({(entry.get('binary'), entry.get('name'), entry.get('script')) for entry in combined}), len(combined))


class NativePackagePartitions(unittest.TestCase):
    def selection(self):
        targets = [FastTargetInventory().target(package=package, target_kind=kind, target_name=name)
                   for package in ('oasis7_runtime', 'oasis7_world_state')
                   for kind, name in (('lib', package), ('test', 'pr_fast'))]
        return InlineExecutionSelection().selection() | {
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
        for plan in (None, {}, InlineExecutionSelection().selection()):
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
                damaged['binaries']['harness-2'] = FastTargetInventory().binary(plan['ordinary_fast_targets'][2])
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
        return InlineExecutionSelection().selection() | {
            'ordinary_fast_targets': [FastTargetInventory().target(target_name='case_' + str(index))
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
            'harness-' + str(index): FastTargetInventory().binary(target, 'required-fast-' + str(index)) |
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


class DriverCoverage(unittest.TestCase):
    def test_complete_artifact_cli_controls_run_once_and_consumers_never_call_cargo(self):
        root = HERE.parent
        sha = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, text=True).strip()
        selected = CompleteArtifactPartitions().selection()
        selected['architecture'] = dict(source=sha, tested=sha)
        with tempfile.TemporaryDirectory() as temporary:
            temporary = Path(temporary)
            executable = temporary / 'cargo'
            executable.write_text('#!' + sys.executable + '\nimport json,os,sys\n'
                'args = [arg for arg in sys.argv[1:] if arg != "--verbose"]\n'
                'with open(os.environ["CARGO_CALLS"],"a") as stream: stream.write(json.dumps(args)+"\\n")\n'
                'raise SystemExit(19 if os.environ.get("FAIL_CARGO") == args[0] else 0)\n')
            executable.chmod(0o755)
            selection = temporary / 'selection.json'
            selection.write_text(json.dumps(selected))
            log = temporary / 'calls'
            environment = dict(os.environ, PATH=str(temporary) + os.pathsep + os.environ['PATH'],
                               CARGO_CALLS=str(log), SOURCE_SHA=sha,
                               OASIS7_CI_TEST_BUNDLE=str(temporary / 'missing-bundle'))
            def invoke(partition, failure=''):
                log.unlink(missing_ok=True)
                result = subprocess.run(['bash', str(HERE / 'ci-tests.sh'), 'required',
                    '--group', 'oasis7_required', '--plan', str(selection), '--ordinary-partition', partition,
                    '--repo-root', str(root)], cwd=root, env=environment | {'FAIL_CARGO': failure},
                    capture_output=True, text=True)
                calls = [json.loads(line) for line in log.read_text().splitlines()] if log.exists() else []
                return result, calls
            # Hosted governance enables verbosity. Cargo accepts it before the
            # subcommand as well as after clippy; neither form changes dispatch.
            for verbose in ('', '1'):
                environment['CI_VERBOSE'] = verbose
                with self.subTest(verbose=verbose):
                    result, calls = invoke('controls')
                    self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                    self.assertEqual([call[0] for call in calls], ['clippy', 'check'])
                    self.assertIn('-D', calls[0])
                    self.assertIn('clippy::correctness', calls[0])
                    self.assertIn('wasm32-unknown-unknown', calls[1])
                    for failure, expected in [('clippy', ['clippy']), ('check', ['clippy', 'check'])]:
                        result, calls = invoke('controls', failure)
                        self.assertEqual(result.returncode, 19)
                        self.assertEqual([call[0] for call in calls], expected)
                        self.assertNotIn('manifest.json', result.stderr)
                    for partition in artifacts.ORDINARY_ARTIFACT_PARTITIONS[1:]:
                        result, calls = invoke(partition)
                        self.assertNotEqual(result.returncode, 0)
                        self.assertEqual(calls, [])
                        self.assertIn('manifest.json', result.stderr)

    def test_partition_driver_rejects_explicit_all_for_a_split_parent_before_consuming_artifacts(self):
        sha = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=artifacts.ROOT, text=True).strip()
        plan = NativePackagePartitions().selection() | {
            'source_sha': sha, 'tested_sha': sha, 'policy_sha': sha,
            'architecture': dict(source=sha, tested=sha)}
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            selection = root / 'selection.json'
            selection.write_text(json.dumps(plan))
            for partition in ('all', 'unknown'):
                result = subprocess.run(['bash', str(HERE / 'ci-tests.sh'), 'required',
                    '--group', 'oasis7_required', '--plan', str(selection),
                    '--ordinary-partition', partition, '--repo-root', str(artifacts.ROOT)],
                    text=True, capture_output=True,
                    env=dict(os.environ, SOURCE_SHA=sha, OASIS7_CI_TEST_BUNDLE=str(root / 'missing-bundle')))
                with self.subTest(partition=partition):
                    self.assertNotEqual(result.returncode, 0)
                    self.assertIn('unknown ordinary partition', result.stderr)
                    self.assertNotIn('missing-bundle/manifest.json', result.stderr)

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
            for partition, failed in ((partition, failed) for partition in ('', 'all', 'oasis7_runtime')
                                      for failed in ('', 'build', 'run')):
                log = root / 'calls.jsonl'
                log.unlink(missing_ok=True)
                script = 'set -euo pipefail\nci_python="$1"; driver_dir="$2"; repo_root="$2"; required_plan=fixture; ordinary_partition="$3"\n'
                script += 'run() { "$@"; }\nrun_cargo() { echo unexpected-Cargo; return 83; }\n'
                script += 'run_oasis7_required_tier_tests() {\n' + body + '}\nrun_oasis7_required_tier_tests\n'
                result = subprocess.run(['bash', '-c', script, 'fixture', sys.executable, str(root), partition],
                                        text=True, capture_output=True,
                                        env=dict(os.environ, FAILED=failed, CALL_LOG=str(log), OASIS7_CI_TEST_BUNDLE=''))
                calls = [json.loads(line) for line in log.read_text().splitlines()]
                with self.subTest(partition=partition, failed=failed):
                    self.assertEqual(result.returncode, 19 if failed else 0, result.stderr)
                    self.assertEqual([call['args'][0] for call in calls], ['build'] if failed == 'build' else ['build', 'run'])
                    self.assertEqual(calls[0]['args'][calls[0]['args'].index('--scope') + 1], 'ordinary')
                    self.assertEqual(len({call['bundle'] for call in calls}), 1)
                    self.assertFalse(Path(calls[0]['bundle']).exists())
                    for call in calls:
                        if partition and partition != 'all':
                            self.assertEqual(call['args'][call['args'].index('--ordinary-partition') + 1], partition)
                        else:
                            self.assertNotIn('--ordinary-partition', call['args'])

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


class FormalBuildComponents(unittest.TestCase):
    def test_components_preserve_each_original_compile_context_once(self):
        # Frozen command oracle from b3658f94, independent of the new splitter.
        required = ['test_tier_required']
        formal = ['test_tier_required', 'ci_formal_required_features']
        original = [
            ('formal-required', ['test', '-p', 'oasis7', '-p', 'oasis7_chain_runtime',
                '--lib', '--bins', '--features', ','.join(formal), '--no-run'], formal),
            ('runtime', ['build', '-p', 'oasis7_chain_runtime', '--bin', 'oasis7_chain_runtime',
                '--features', 'test_tier_required'], required),
            ('endpoint', ['build', '-p', 'oasis7_distfs', '--bin', 'oasis7_durability_endpoint', '--release'], []),
            ('distfs-tests', ['test', '-p', 'oasis7_distfs', '--lib', '--no-run'], []),
        ]
        self.assertEqual(artifacts.build_variants('formal'), original)
        debug = artifacts.build_variants('formal-debug')
        endpoint = artifacts.build_variants('formal-endpoint')
        self.assertEqual(debug, [original[0], original[1], original[3]])
        self.assertEqual(endpoint, [original[2]])
        self.assertCountEqual(debug + endpoint, original)
        self.assertEqual(len({name for name, _, _ in debug + endpoint}), 4)

    def test_formal_debug_profile_keeps_assertions_and_endpoint_release_contract(self):
        with patch.dict(os.environ, RUSTC_WRAPPER='must-not-wrap',
                        CARGO_PROFILE_DEV_OPT_LEVEL='2', CARGO_PROFILE_TEST_OPT_LEVEL='2',
                        CARGO_PROFILE_DEV_DEBUG_ASSERTIONS='false', CARGO_PROFILE_TEST_DEBUG_ASSERTIONS='false',
                        CARGO_PROFILE_DEV_OVERFLOW_CHECKS='false', CARGO_PROFILE_TEST_OVERFLOW_CHECKS='false'):
            for scope, level in (('formal', '0'), ('formal-debug', '0'),
                                 ('formal-endpoint', '0'), ('storage', '2')):
                with self.subTest(scope=scope):
                    environment = artifacts.build_environment(scope)
                    self.assertNotIn('RUSTC_WRAPPER', environment)
                    for profile in ('DEV', 'TEST'):
                        self.assertEqual(environment['CARGO_PROFILE_' + profile + '_OPT_LEVEL'], level)
                        self.assertEqual(environment['CARGO_PROFILE_' + profile + '_DEBUG_ASSERTIONS'], 'true')
                        self.assertEqual(environment['CARGO_PROFILE_' + profile + '_OVERFLOW_CHECKS'], 'true')
            self.assertEqual(artifacts.build_environment('ordinary')['CARGO_PROFILE_DEV_OPT_LEVEL'], '2')
        for scope, level in (('formal', '0'), ('formal-debug', '0'), ('storage', '2')):
            debug = dict(profile=dict(opt_level=level, debug_assertions=True, overflow_checks=True))
            artifacts.validate_profile('runtime-tests', debug, scope)
            for field, wrong in (('opt_level', '2' if level == '0' else '0'),
                                 ('debug_assertions', False), ('overflow_checks', False)):
                altered = copy.deepcopy(debug)
                altered['profile'][field] = wrong
                with self.subTest(scope=scope, field=field), self.assertRaises(ValueError):
                    artifacts.validate_profile('runtime-tests', altered, scope)
        endpoint = dict(target='oasis7_durability_endpoint', test=False,
                        original_executable='/checkout/target/release/oasis7_durability_endpoint',
                        profile=dict(opt_level='3', debug_assertions=False))
        artifacts.validate_profile('endpoint', endpoint, 'formal-endpoint')
        endpoint['profile']['opt_level'] = '2'
        with self.assertRaises(ValueError):
            artifacts.validate_profile('endpoint', endpoint, 'formal-endpoint')


class RealFormalComponents(unittest.TestCase):
    """The actual four Cargo contexts, reduced only to tiny standalone crates."""

    @classmethod
    def context(cls):
        stack = ExitStack()
        stack.enter_context(patch.object(artifacts, 'ROOT', cls.root))
        stack.enter_context(patch.object(artifacts, 'CONFIG', cls.configuration))
        stack.enter_context(patch.object(artifacts, 'FORMAL_AUTHORITY', None))
        stack.enter_context(patch.dict(os.environ, SOURCE_SHA=cls.sha, GITHUB_RUN_ID='42',
            GITHUB_RUN_ATTEMPT='2', CARGO_INCREMENTAL='0', CARGO_NET_OFFLINE='true',
            CARGO_PROFILE_DEV_DEBUG='0', CARGO_PROFILE_TEST_DEBUG='0',
            CARGO_PROFILE_DEV_OPT_LEVEL='2', CARGO_PROFILE_TEST_OPT_LEVEL='2',
            CARGO_PROFILE_DEV_DEBUG_ASSERTIONS='false', CARGO_PROFILE_TEST_DEBUG_ASSERTIONS='false',
            CARGO_PROFILE_DEV_OVERFLOW_CHECKS='false', CARGO_PROFILE_TEST_OVERFLOW_CHECKS='false'))
        return stack

    @classmethod
    def setUpClass(cls):
        cls.temporary = tempfile.TemporaryDirectory(prefix='oasis7-formal-components-fixture-')
        cls.addClassCleanup(cls.temporary.cleanup)
        cls.base = Path(cls.temporary.name)
        cls.root = cls.base / 'checkout'
        cls.root.mkdir()
        (cls.root / 'Cargo.toml').write_text(
            '[workspace]\nmembers=["world","runtime","distfs"]\nresolver="2"\n')
        common = '''
#[cfg(all(test, not(debug_assertions)))] compile_error!("test assertions were disabled");
pub fn value() -> u8 { 7 }
#[test] fn formal_contract() {
    assert_eq!(value(), 7);
    let value = std::hint::black_box(u8::MAX);
    assert!(std::panic::catch_unwind(|| value + std::hint::black_box(1)).is_err());
}
#[test] #[ignore] fn acceptance() { assert_eq!(value(), 7); }
'''
        paired = '''
fn rendezvous(name: &str, peer: &str) {
    let Some(directory) = std::env::var_os("OASIS7_FORMAL_FIXTURE_RENDEZVOUS") else { return; };
    let directory = std::path::PathBuf::from(directory);
    std::fs::write(directory.join(name), b"started").unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !directory.join(peer).exists() {
        assert!(std::time::Instant::now() < deadline, "complete Formal cases did not overlap");
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    if std::env::var_os("OASIS7_FORMAL_FIXTURE_FAIL").is_some() {
        assert_ne!(name, "a", "real case failure");
        std::thread::sleep(std::time::Duration::from_secs(30));
    }
}
#[test] fn formal_parallel_a() { rendezvous("a", "b"); }
#[test] fn formal_parallel_b() { rendezvous("b", "a"); }
'''
        for directory, package in (('world', 'oasis7'), ('runtime', 'oasis7_chain_runtime'),
                                   ('distfs', 'oasis7_distfs')):
            member = cls.root / directory
            (member / 'src').mkdir(parents=True)
            features = '[features]\ndefault=[]\ntest_tier_required=[]\nci_formal_required_features=[]\n'
            (member / 'Cargo.toml').write_text('[package]\nname="' + package +
                '"\nversion="0.1.0"\nedition="2021"\n' + features)
            (member / 'src/lib.rs').write_text(common + (paired if directory == 'runtime' else ''))
            if directory == 'runtime':
                (member / 'src/main.rs').write_text('fn main() { println!("fixture runtime"); }\n')
            if directory == 'distfs':
                (member / 'src/bin').mkdir()
                (member / 'src/bin/oasis7_durability_endpoint.rs').write_text(
                    '#[cfg(debug_assertions)] compile_error!("endpoint is not release");\nfn main() {}\n')
                (member / 'build.rs').write_text(
                    'fn main() { println!("cargo:rerun-if-changed=src/lib.rs"); }\n')
        config = dict(schema_version=2, suites={suite: [] for suite in artifacts.SHARDS},
            legacy_ignored_filters={}, environment_formal_batches={}, shell_tests={},
            binary_dependencies={'runtime-tests': ['runtime', 'endpoint'],
                                 'oasis7-lib-tests': ['runtime', 'endpoint'],
                                 'distfs-tests': ['endpoint']},
            parallel_cases={'runtime-tests': {'formal_parallel_a': 1, 'formal_parallel_b': 1}})
        for name, suite in (('runtime-tests', 'runtime-protocol'), ('oasis7-lib-tests', 'runtime-protocol'),
                            ('distfs-tests', 'storage-recovery')):
            config['suites'][suite].append(dict(binary=name, name='acceptance', ignored=True))
            config['legacy_ignored_filters'][name] = ['acceptance']
            config['environment_formal_batches'][name] = suite
        cls.configuration = cls.root / 'scripts/ci-formal-process-suites.json'
        cls.configuration.parent.mkdir()
        cls.configuration.write_text(json.dumps(config))
        subprocess.run(['cargo', 'generate-lockfile', '--offline'], cwd=cls.root,
                       check=True, capture_output=True)
        # This temporary Git fixture is independent of the project worktree.
        for command in (['git', 'init', '-q'], ['git', 'add', '.'],
                        ['git', '-c', 'user.name=Fixture', '-c', 'user.email=fixture@example.invalid',
                         'commit', '-qm', 'fixture']):
            subprocess.run(command, cwd=cls.root, check=True, capture_output=True)
        cls.sha = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=cls.root, text=True).strip()
        cls.debug, cls.endpoint = cls.base / 'debug', cls.base / 'endpoint'
        with cls.context():
            with patch.dict(os.environ, GITHUB_RUN_ATTEMPT='1'):
                artifacts.build(cls.debug, 'formal-debug', cls.base / 'debug-state.json')
            artifacts.build(cls.endpoint, 'formal-endpoint', cls.base / 'endpoint-state.json')
            artifacts.build(cls.base / 'single', 'formal')

    def setUp(self):
        self.stack = self.context()
        self.addCleanup(self.stack.close)
        temporary = tempfile.TemporaryDirectory(dir=self.base, prefix='case-')
        self.addCleanup(temporary.cleanup)
        self.case = Path(temporary.name)

    def copy_components(self):
        debug, endpoint = self.case / 'debug', self.case / 'endpoint'
        shutil.copytree(self.debug, debug)
        shutil.copytree(self.endpoint, endpoint)
        return debug, endpoint

    @staticmethod
    def failed_observer(mode):
        original = artifacts.subprocess.Popen
        observed = []

        def launch(command, *args, **kwargs):
            if len(command) > 1 and Path(command[1]).name == 'package-ecs-resource-monitor.py':
                observed.append(command)
                if mode == 'spawn-error':
                    raise OSError('fixture observer unavailable')
                process = original([sys.executable, '-c', 'raise SystemExit(19)'], *args, **kwargs)
                process.wait(timeout=5)
                return process
            return original(command, *args, **kwargs)

        return patch.object(artifacts.subprocess, 'Popen', side_effect=launch), observed

    def test_real_split_merge_matches_original_full_inventory_and_executes_every_case(self):
        merged = self.case / 'merged'
        artifacts.merge_formal(self.debug, self.endpoint, merged)
        manifest = json.loads((merged / 'manifest.json').read_text())
        original = json.loads((self.base / 'single/manifest.json').read_text())
        self.assertEqual(set(manifest['binaries']), set(original['binaries']))
        for logical, binary in manifest['binaries'].items():
            for field in ('package', 'variant', 'target', 'target_kind', 'requested_features',
                          'features', 'profile', 'tests', 'ignored_tests', 'cwd'):
                self.assertEqual(binary.get(field), original['binaries'][logical].get(field), (logical, field))
            self.assertEqual(binary['profile']['opt_level'], '3' if logical == 'endpoint' else '0')
            if logical != 'endpoint':
                self.assertIs(binary['profile']['debug_assertions'], True)
                self.assertIs(binary['profile']['overflow_checks'], True)
        self.assertEqual(manifest['suites'], original['suites'])
        self.assertEqual(manifest['formal_execution_version'], 1)
        self.assertEqual({role: value['producer_attempt'] for role, value in manifest['components'].items()},
                         {'formal-debug': '1', 'formal-endpoint': '2'})
        rendezvous = self.case / 'rendezvous'
        rendezvous.mkdir()
        count = 0
        for shard in ('runtime-protocol', 'storage-recovery'):
            archive = self.case / (shard + '.tar.gz')
            artifacts.pack(merged, archive, shard=shard)
            consumer = self.case / shard
            with tarfile.open(archive) as package:
                package.extractall(consumer, filter='data')
            results = self.case / (shard + '.jsonl')
            with patch.object(artifacts, 'native_cpu_budget', return_value=4), \
                 patch.dict(os.environ, OASIS7_FORMAL_FIXTURE_RENDEZVOUS=str(rendezvous)):
                artifacts.execute(consumer, 'ordinary', results, shard=shard)
            records = [json.loads(line) for line in results.read_text().splitlines()]
            expected = artifacts.selected_entries(manifest, artifacts.formal_configuration(), shard=shard)
            self.assertCountEqual([(record['suite'], record['test_name']) for record in records],
                                  [(entry['suite'], entry['name']) for entry in expected])
            self.assertTrue(all(record['passed'] and record['count_ok'] and record['cleanup_result'] == 'clean'
                                and record['shard'] == shard and record['expected_count'] == 1 for record in records))
            count += len(records)
        self.assertEqual(count, 8)
        self.assertTrue((rendezvous / 'a').exists() and (rendezvous / 'b').exists())

    def test_each_component_cache_restores_complete_real_units_without_rebuild(self):
        for scope, component in (('formal-debug', 'debug'), ('formal-endpoint', 'endpoint')):
            with self.subTest(scope=scope):
                state = json.loads((self.base / (component + '-state.json')).read_text())
                self.assertEqual(state['selection'], 'all')
                self.assertTrue(state['complete'], state)
                cache = self.case / (component + '-cache')
                staged = artifacts.cache_stage(state, cache)
                self.assertTrue(staged['save'], staged)
                shutil.rmtree(self.root / 'target')
                restored = artifacts.cache_restore(self.root, cache)
                self.assertTrue(restored['restored'], restored)
                bundle = self.case / (component + '-rebuilt')
                artifacts.build(bundle, scope)
                manifest = json.loads((bundle / 'manifest.json').read_text())
                self.assertTrue(all(timing['local_rebuilt'] == 0 for timing in manifest['build_timings']), manifest['build_timings'])
                # Restore the other profile before staging its original state.
                if component == 'debug':
                    artifacts.build(self.case / 'endpoint-for-cache', 'formal-endpoint')

    def test_real_failed_case_cancels_running_peer_and_keeps_prior_completed_receipts(self):
        merged = self.case / 'merged'
        artifacts.merge_formal(self.debug, self.endpoint, merged)
        rendezvous = self.case / 'rendezvous'
        rendezvous.mkdir()
        results = self.case / 'failed.jsonl'
        with patch.object(artifacts, 'native_cpu_budget', return_value=4), \
             patch.dict(os.environ, OASIS7_FORMAL_FIXTURE_RENDEZVOUS=str(rendezvous),
                        OASIS7_FORMAL_FIXTURE_FAIL='1'):
            with self.assertRaisesRegex(RuntimeError, 'formal failed'):
                artifacts.execute(merged, 'ordinary', results, shard='runtime-protocol')
        records = [json.loads(line) for line in results.read_text().splitlines()]
        self.assertEqual(len(records), 5)
        failed = next(record for record in records if record['test_name'] == 'formal_parallel_a')
        peer = next(record for record in records if record['test_name'] == 'formal_parallel_b')
        self.assertFalse(failed['passed'])
        self.assertEqual(failed['exit_code'], 101)
        self.assertFalse(peer['passed'])
        self.assertTrue(peer['cancelled'])
        self.assertEqual(peer['exit_code'], -signal.SIGTERM)
        self.assertTrue(all(record['cleanup_result'] == 'clean' for record in records))
        # The existing runner records a live owner requiring termination as
        # leaked, even when cancellation successfully reaps it. It cannot pass.
        self.assertTrue(peer['leaked_processes'])
        self.assertTrue(all(not record['leaked_processes'] for record in records if record is not peer))
        self.assertTrue(all(record['passed'] for record in records
                            if record['test_name'] not in ('formal_parallel_a', 'formal_parallel_b')))

    def test_real_cli_components_merge_pack_and_consumer_use_the_identical_trusted_plan(self):
        authority = self.case / 'authority.json'
        authority.write_text(self.configuration.read_text())
        selected = dict(fast_plan_version=1, execution_profile='pr-fast', scope='targeted',
            groups=['formal_process'], formal_suites=['runtime-protocol', 'storage-recovery'],
            formal_shards=['runtime-protocol', 'storage-recovery'],
            architecture=dict(source=self.sha, tested=self.sha))
        plan_path = self.case / 'selection.json'
        plan_path.write_text(json.dumps(selected))
        common = [sys.executable, str(HERE / 'ci-test-artifacts.py'), '--repo-root', str(self.root),
                  '--plan', str(plan_path), '--formal-config', str(authority)]
        def call(*args):
            result = subprocess.run([*common, *map(str, args)], text=True, capture_output=True)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            return result
        debug, endpoint, merged = (self.case / name for name in ('cli-debug', 'cli-endpoint', 'cli-merged'))
        for scope, bundle, variants in (
                ('formal-debug', debug, ('formal-required', 'runtime', 'distfs-tests')),
                ('formal-endpoint', endpoint, ('endpoint',))):
            diagnostics = self.case / (scope + '-diagnostics')
            call('build', '--scope', scope, '--bundle', bundle, '--build-diagnostics', diagnostics)
            self.assertEqual({path.name for path in diagnostics.iterdir()}, set(variants))
            for variant in variants:
                report = json.loads((diagnostics / variant / 'build.json').read_text())
                self.assertEqual(report['variant'], variant)
                self.assertEqual(report['status'], 'success')
                self.assertEqual(report['cargo_exit_code'], 0)
                self.assertGreaterEqual(report['elapsed_seconds'], 0)
                self.assertTrue(report['timings'], report)
                for name in report['timings']:
                    self.assertTrue(name.startswith('cargo-timing') and name.endswith('.html'))
                    self.assertIn('<html', (diagnostics / variant / name).read_text().lower())
        call('merge-formal', '--debug-bundle', debug, '--endpoint-bundle', endpoint, '--bundle', merged)
        archives = self.case / 'archives'
        call('pack', '--bundle', merged, '--split-formal', '--output', archives)
        self.assertEqual(sorted(path.name for path in archives.iterdir()),
                         ['runtime-protocol.tar.gz', 'runtime-protocol.tar.size.json',
                          'storage-recovery.tar.gz', 'storage-recovery.tar.size.json'])
        consumer = self.case / 'cli-consumer'
        with tarfile.open(archives / 'runtime-protocol.tar.gz') as archive:
            archive.extractall(consumer, filter='data')
        results = self.case / 'cli-results.jsonl'
        call('run', '--bundle', consumer, '--shard', 'runtime-protocol', '--results', results)
        records = [json.loads(line) for line in results.read_text().splitlines()]
        self.assertEqual(len(records), 6)
        self.assertTrue(all(record['passed'] for record in records))
        selected['architecture']['source'] = 'f' * 40
        plan_path.write_text(json.dumps(selected))
        rejected = subprocess.run([*common, 'merge-formal', '--debug-bundle', str(debug),
            '--endpoint-bundle', str(endpoint), '--bundle', str(self.case / 'wrong-plan')],
            text=True, capture_output=True)
        self.assertNotEqual(rejected.returncode, 0)
        self.assertFalse((self.case / 'wrong-plan').exists())

    def test_observer_failure_cannot_turn_a_successful_real_build_into_failure(self):
        for mode in ('spawn-error', 'nonzero-exit'):
            observer, observed = self.failed_observer(mode)
            diagnostics = self.case / (mode + '-diagnostics')
            bundle = self.case / (mode + '-bundle')
            with self.subTest(observer=mode), observer:
                artifacts.build(bundle, 'formal-endpoint', diagnostics_dir=diagnostics)
            self.assertEqual(len(observed), 1)
            self.assertEqual(observed[0][-4:], ['--label', 'endpoint', '--heartbeat-seconds', '20'])
            self.assertIn('--pid', observed[0])
            report = json.loads((diagnostics / 'endpoint/build.json').read_text())
            self.assertEqual((report['status'], report['cargo_exit_code']), ('success', 0))
            self.assertTrue((bundle / 'manifest.json').is_file())

    def test_failed_real_cargo_records_its_exit_and_never_starts_later_variants(self):
        checkout = self.case / 'broken-checkout'
        shutil.copytree(self.root, checkout, ignore=shutil.ignore_patterns('target', '.git'))
        (checkout / 'world/src/lib.rs').write_text('compile_error!("intentional build failure");\n')
        diagnostics, bundle = self.case / 'failed-diagnostics', self.case / 'failed-bundle'
        observer, observed = self.failed_observer('nonzero-exit')
        with patch.object(artifacts, 'ROOT', checkout), \
             patch.object(artifacts, 'CONFIG', checkout / 'scripts/ci-formal-process-suites.json'), observer:
            with self.assertRaisesRegex(RuntimeError, 'build failed: formal-required'):
                artifacts.build(bundle, 'formal-debug', diagnostics_dir=diagnostics)
        self.assertEqual(len(observed), 1)
        self.assertEqual({path.name for path in diagnostics.iterdir()}, {'formal-required'})
        report = json.loads((diagnostics / 'formal-required/build.json').read_text())
        self.assertEqual(report['cargo_exit_code'], 101)
        self.assertEqual(report['status'], 'failure')
        self.assertEqual(report['error_type'], 'RuntimeError')
        self.assertFalse((bundle / 'manifest.json').exists())

    def test_real_opt2_artifacts_are_rejected_before_merge_or_case_execution(self):
        environment, validate = artifacts.build_environment, artifacts.validate_profile

        def previous_environment(scope):
            result = environment(scope)
            for profile in ('DEV', 'TEST'):
                result[f'CARGO_PROFILE_{profile}_OPT_LEVEL'] = '2'
            return result

        def previous_profile(logical, binary, scope=None):
            # Reproduce the former opt2 contract through the unchanged storage
            # profile. Cargo, executable bytes, metadata and hashes remain real.
            return validate(logical, binary, 'storage' if scope in
                            ('formal', 'formal-debug', 'formal-endpoint') else scope)

        debug, endpoint, complete = (self.case / name for name in
                                     ('old-opt2-debug', 'old-opt2-endpoint', 'old-opt2-formal'))
        with patch.object(artifacts, 'build_environment', side_effect=previous_environment), \
             patch.object(artifacts, 'validate_profile', side_effect=previous_profile):
            artifacts.build(debug, 'formal-debug')
            artifacts.build(endpoint, 'formal-endpoint')
            artifacts.build(complete, 'formal')
        old = json.loads((complete / 'manifest.json').read_text())
        self.assertEqual({binary['profile']['opt_level'] for logical, binary in old['binaries'].items()
                          if logical != 'endpoint'}, {'2'})
        self.assertEqual(old['binaries']['endpoint']['profile']['opt_level'], '3')

        for label, left, right in (('debug-environment', debug, self.endpoint),
                                    ('endpoint-environment', self.debug, endpoint)):
            with self.subTest(component=label):
                destination = self.case / label
                with self.assertRaisesRegex(ValueError, 'compiler_environment'):
                    artifacts.merge_formal(left, right, destination)
                self.assertFalse(destination.exists())
        # A forged current environment cannot make actual opt2 target profiles
        # acceptable. The merger must reject before executing any --list call.
        manifest = json.loads((debug / 'manifest.json').read_text())
        manifest['compiler_environment'] = artifacts.compiler_environment(environment('formal'))
        (debug / 'manifest.json').write_text(json.dumps(manifest))
        with self.subTest(component='target-profile'), \
             patch.object(artifacts, 'inventory', wraps=artifacts.inventory) as listing:
            with self.assertRaisesRegex(ValueError, 'artifact'):
                artifacts.merge_formal(debug, self.endpoint, self.case / 'profile-forgery')
            listing.assert_not_called()
            self.assertFalse((self.case / 'profile-forgery').exists())

        archive, consumer = self.case / 'old-opt2.tar.gz', self.case / 'old-opt2-consumer'
        artifacts.pack(complete, archive, shard='runtime-protocol')
        with tarfile.open(archive) as payload:
            payload.extractall(consumer, filter='data')
        downloaded = json.loads((consumer / 'manifest.json').read_text())
        for version, schema in itertools.product((1, None, 999), (2, None, 1, 99, 2.0)):
            altered = copy.deepcopy(downloaded)
            if version is None:
                altered.pop('formal_execution_version')
            else:
                altered['formal_execution_version'] = version
            if schema is None:
                altered.pop('schema_version')
            else:
                altered['schema_version'] = schema
            (consumer / 'manifest.json').write_text(json.dumps(altered))
            results = self.case / ('old-opt2-results-' + str(version) + '-' + str(schema) + '.jsonl')
            with self.subTest(execution_version=version, schema=schema), \
                 patch('ci_process_runner.run', side_effect=AssertionError('old opt2 case started')) as run_case, \
                 patch.object(artifacts, 'inventory', wraps=artifacts.inventory) as listing:
                with self.assertRaisesRegex(ValueError, 'artifact'):
                    artifacts.execute(consumer, 'ordinary', results, shard='runtime-protocol')
                run_case.assert_not_called()
                listing.assert_not_called()
            self.assertFalse(results.exists())

    def test_named_formal_bundle_requires_current_schema_even_with_valid_opt0_profiles(self):
        bundle = self.case / 'invalid-schema'
        shutil.copytree(self.base / 'single', bundle)
        original = json.loads((bundle / 'manifest.json').read_text())
        for schema in (None, 1, 99, 2.0):
            manifest = copy.deepcopy(original)
            if schema is None:
                manifest.pop('schema_version')
            else:
                manifest['schema_version'] = schema
            (bundle / 'manifest.json').write_text(json.dumps(manifest))
            with self.subTest(schema=schema), patch.object(artifacts, 'inventory') as listing:
                with self.assertRaisesRegex(ValueError, 'schema 2'):
                    artifacts.load(bundle, shard='runtime-protocol')
                listing.assert_not_called()

    def test_merge_rejects_identity_profile_target_and_inventory_damage_before_output(self):
        debug, endpoint = self.copy_components()
        original = (debug / 'manifest.json').read_text()
        mutations = {
            'source': lambda m: m.update(source_sha='f' * 40),
            'tested': lambda m: m.update(tested_sha='f' * 40),
            'run': lambda m: m.update(producer_run_id='other'),
            'future-attempt': lambda m: m.update(producer_attempt='3'),
            'bad-attempt': lambda m: m.update(producer_attempt='0'),
            'authority': lambda m: m.update(formal_authority_sha256='wrong'),
            'compiler': lambda m: m.update(rustc_version='wrong compiler'),
            'target': lambda m: m.update(target_triple='wrong-target'),
            'compiler-flags': lambda m: m['compiler_environment'].update(RUSTFLAGS='--cfg other_behavior'),
            'scope': lambda m: m.update(scope='formal-endpoint'),
            'missing-bin': lambda m: m['binaries'].pop('required-test-oasis7_chain_runtime-bin-oasis7_chain_runtime'),
            'features': lambda m: m['binaries']['runtime-tests'].update(requested_features=[]),
            'profile': lambda m: m['binaries']['runtime-tests']['profile'].update(overflow_checks=False),
            'alias': lambda m: m['binaries'].update(alias=copy.deepcopy(m['binaries']['runtime-tests'])),
            'variant': lambda m: m['binaries']['runtime-tests'].update(variant='runtime'),
            'inventory': lambda m: m['binaries']['runtime-tests']['tests'].append('absent'),
        }
        for name, change in mutations.items():
            with self.subTest(damage=name):
                manifest = json.loads(original)
                change(manifest)
                (debug / 'manifest.json').write_text(json.dumps(manifest))
                output = self.case / ('rejected-' + name)
                with self.assertRaises((ValueError, KeyError)):
                    artifacts.merge_formal(debug, endpoint, output)
                self.assertFalse(output.exists())
        (debug / 'manifest.json').write_text(original)

    def test_consistent_forged_inventory_is_rejected_by_the_real_binary_listing(self):
        debug, endpoint = self.copy_components()
        manifest = json.loads((debug / 'manifest.json').read_text())
        binary = manifest['binaries']['runtime-tests']
        binary['tests'] = sorted([*binary['tests'], 'formal_absent_from_compiled_binary'])
        binary['test_list_sha256'] = hashlib.sha256(json.dumps(binary['tests']).encode()).hexdigest()
        (debug / 'manifest.json').write_text(json.dumps(manifest))
        output = self.case / 'forged-inventory'
        with patch.object(artifacts, 'inventory', wraps=artifacts.inventory) as listing:
            with self.assertRaisesRegex(ValueError, 'actual test inventory mismatch'):
                artifacts.merge_formal(debug, endpoint, output)
        self.assertTrue(any(call.args[0] == debug / binary['path']
                            for call in listing.call_args_list))
        self.assertFalse(output.exists())

    def test_merge_rejects_corrupt_nonexecutable_and_symlinked_files_before_listing(self):
        debug, endpoint = self.copy_components()
        file = endpoint / 'bin/endpoint'
        original, mode = file.read_bytes(), file.stat().st_mode
        for damage in ('bytes', 'mode', 'symlink'):
            with self.subTest(damage=damage):
                if damage == 'bytes':
                    file.write_bytes(b'corrupted')
                elif damage == 'mode':
                    file.chmod(0o644)
                else:
                    file.unlink()
                    file.symlink_to(self.endpoint / 'bin/endpoint')
                with patch.object(artifacts, 'inventory') as listing:
                    with self.assertRaises(ValueError):
                        artifacts.merge_formal(debug, endpoint, self.case / ('rejected-' + damage))
                    listing.assert_not_called()
                file.unlink()
                file.write_bytes(original)
                file.chmod(mode)

    def test_partial_and_unknown_scope_cannot_be_consumed_or_packed(self):
        for component in (self.debug, self.endpoint):
            with self.subTest(component=component.name), patch('ci_process_runner.run') as execute:
                with self.assertRaisesRegex(ValueError, 'component'):
                    artifacts.execute(component, 'ordinary', self.case / 'partial.jsonl', shard='runtime-protocol')
                with self.assertRaisesRegex(ValueError, 'component'):
                    artifacts.pack(component, self.case / 'partial.tar.gz', shard='runtime-protocol')
                execute.assert_not_called()
        unknown = self.case / 'unknown'
        shutil.copytree(self.base / 'single', unknown)
        manifest = json.loads((unknown / 'manifest.json').read_text())
        manifest['scope'] = 'future-unchecked-scope'
        (unknown / 'manifest.json').write_text(json.dumps(manifest))
        for operation in (lambda: artifacts.load(unknown),
                          lambda: artifacts.pack(unknown, self.case / 'unknown.tar.gz', shard='runtime-protocol')):
            with self.assertRaisesRegex(ValueError, 'scope'):
                operation()

    def test_merge_binds_effective_compile_inputs_but_not_network_output_or_worker_count(self):
        with patch.dict(os.environ, CARGO_BUILD_JOBS='1', CARGO_TERM_COLOR='never', CARGO_NET_RETRY='1',
                        CARGO_REGISTRY_TOKEN='must-not-be-part-of-manifest'):
            artifacts.merge_formal(self.debug, self.endpoint, self.case / 'execution-knobs')
        for key, value in (('RUSTFLAGS', '--cfg changed'), ('CARGO_ENCODED_RUSTFLAGS', '--cfg\x1fchanged'),
                           ('CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUSTFLAGS', '--cfg changed'),
                           ('CARGO_BUILD_TARGET', 'aarch64-unknown-linux-gnu'), ('CFLAGS', '-DCHANGED')):
            with self.subTest(key=key), patch.dict(os.environ, {key: value}):
                with self.assertRaisesRegex(ValueError, 'compiler_environment'):
                    artifacts.merge_formal(self.debug, self.endpoint, self.case / ('changed-' + key))


class BuildDiagnostics(unittest.TestCase):
    def test_only_current_timing_files_are_published_with_start_and_end_state(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            timings, diagnostics = root / 'timings', root / 'diagnostics'
            timings.mkdir()
            diagnostics.mkdir()
            stale = timings / 'cargo-timing-previous.html'
            stale.write_text('<html>stale previous command</html>')
            alias = timings / 'cargo-timing.html'
            alias.write_text('<html>old alias</html>')
            with artifacts.BuildObservation('current', diagnostics, timings) as observation:
                start = json.loads((diagnostics / 'current/build.json').read_text())
                self.assertEqual((start['status'], start['cargo_exit_code']), ('running', None))
                self.assertEqual(start['timings'], [])
                alias.write_text('<html>updated current command alias</html>')
                new = timings / 'cargo-timing-current.html'
                new.write_text('<html>current command</html>')
                # The real Cargo exit object is tested by the standalone Rust
                # fixtures; this case isolates changed-file collection.
                observation.process = type('CompletedCargo', (), {'returncode': 0})()
            report = json.loads((diagnostics / 'current/build.json').read_text())
            self.assertEqual((report['status'], report['cargo_exit_code']), ('success', 0))
            self.assertCountEqual(report['timings'], [alias.name, new.name])
            self.assertEqual((diagnostics / 'current' / alias.name).read_text(), alias.read_text())
            self.assertEqual((diagnostics / 'current' / new.name).read_text(), new.read_text())
            self.assertFalse((diagnostics / 'current' / stale.name).exists())


class ParallelFormalExecution(unittest.TestCase):
    def execute(self, root, entries, invoke, weights=None, cpus=4, version=1, shard='runtime-protocol', plan=None):
        config = dict(parallel_cases={'runtime-tests': weights or {}})
        manifest = dict(scope='formal', formal_execution_version=version, binaries={
            'runtime-tests': dict(path='bin/runtime-tests', cwd='.', test=True, ordinary=False)})
        with patch.object(artifacts, 'load', return_value=(manifest, {})), \
             patch.object(artifacts, 'formal_configuration', return_value=config), \
             patch.object(artifacts, 'selected_entries', return_value=entries), \
             patch.object(artifacts, 'native_cpu_budget', return_value=cpus), \
             patch('ci_process_runner.run', side_effect=invoke):
            artifacts.execute(root, 'ordinary', root / 'results.jsonl', shard=shard, plan=plan)

    @staticmethod
    def entry(name, ignored=False):
        return dict(binary='runtime-tests', name=name, ignored=ignored, suite='runtime-protocol')

    def test_weighted_exact_cases_overlap_and_unknown_case_runs_alone(self):
        entries = [self.entry(name) for name in ('light-a', 'light-b', 'unknown', 'light-c')]
        barrier = threading.Barrier(2)
        lock = threading.Lock()
        active, maximum, calls = {}, [0], []
        def invoke(command, cwd, env, suite, name, results, expected_count=None, **kwargs):
            weight = {'light-a': 2, 'light-b': 2, 'light-c': 1}.get(name, 4)
            self.assertEqual(command[1:], ['--exact', name, '--test-threads=1', '--nocapture'])
            self.assertEqual(expected_count, 1)
            self.assertEqual(env['RUST_TEST_THREADS'], '1')
            with lock:
                active[name] = weight
                self.assertLessEqual(sum(active.values()), 4)
                if name == 'unknown':
                    self.assertEqual(list(active), ['unknown'])
                maximum[0] = max(maximum[0], len(active))
                calls.append(name)
            try:
                if name in ('light-a', 'light-b'):
                    barrier.wait(timeout=2)
                return ParallelNativeExecution.receipt(suite, name, results, expected_count)
            finally:
                with lock:
                    del active[name]
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            self.execute(root, entries, invoke, {'light-a': 2, 'light-b': 2, 'light-c': 1})
            records = [json.loads(line) for line in (root / 'results.jsonl').read_text().splitlines()]
        self.assertEqual(maximum[0], 2)
        self.assertCountEqual(calls, [entry['name'] for entry in entries])
        self.assertEqual(len(records), 4)
        self.assertTrue(all(record['passed'] and record['shard'] == 'runtime-protocol' for record in records))

    def test_parallel_case_allowance_cannot_exceed_four_workers_or_cpu(self):
        for cpus, expected in ((1, 1), (2, 2), (8, 4)):
            entries = [self.entry('case-' + str(index)) for index in range(8)]
            barrier = threading.Barrier(expected)
            active, maximum, calls = [0], [0], []
            lock = threading.Lock()
            def invoke(command, cwd, env, suite, name, results, expected_count=None, **kwargs):
                with lock:
                    active[0] += 1
                    maximum[0] = max(maximum[0], active[0])
                    calls.append(name)
                try:
                    barrier.wait(timeout=2)
                    return ParallelNativeExecution.receipt(suite, name, results, expected_count)
                finally:
                    with lock:
                        active[0] -= 1
            with self.subTest(cpus=cpus), tempfile.TemporaryDirectory() as temporary:
                self.execute(Path(temporary), entries, invoke, {entry['name']: 1 for entry in entries}, cpus)
            self.assertEqual(maximum[0], expected)
            self.assertCountEqual(calls, [entry['name'] for entry in entries])

    def test_formal_first_failure_cancels_active_and_never_launches_pending_case(self):
        entries = [self.entry(name) for name in ('failure', 'active', 'pending')]
        barrier = threading.Barrier(2)
        calls = []
        def invoke(command, cwd, env, suite, name, results, expected_count=None, **kwargs):
            calls.append(name)
            barrier.wait(timeout=2)
            if name == 'failure':
                return ParallelNativeExecution.receipt(suite, name, results, expected_count,
                    passed=False, exit_code=7)
            self.assertTrue(kwargs['cancellation_event'].wait(timeout=2))
            return ParallelNativeExecution.receipt(suite, name, results, expected_count,
                passed=False, cancelled=True, exit_code=-signal.SIGTERM)
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            with self.assertRaisesRegex(RuntimeError, 'formal failed'):
                self.execute(root, entries, invoke, {entry['name']: 1 for entry in entries}, cpus=2)
            records = [json.loads(line) for line in (root / 'results.jsonl').read_text().splitlines()]
        self.assertCountEqual(calls, ['failure', 'active'])
        self.assertEqual(len(records), 2)
        self.assertEqual(next(record for record in records if record['test_name'] == 'failure')['exit_code'], 7)
        self.assertTrue(all(record['shard'] == 'runtime-protocol' for record in records))

    def test_old_formal_bundle_keeps_original_serial_order_and_environment(self):
        entries = [self.entry('first', True), self.entry('second')]
        calls = []
        def invoke(command, cwd, env, suite, name, results, expected_count=None, **kwargs):
            self.assertNotIn('cancellation_event', kwargs)
            self.assertNotIn('RUST_TEST_THREADS', env)
            calls.append(name)
            return ParallelNativeExecution.receipt(suite, name, results, expected_count)
        with tempfile.TemporaryDirectory() as temporary:
            self.execute(Path(temporary), entries, invoke, {'first': 1, 'second': 1}, version=None)
        self.assertEqual(calls, ['first', 'second'])

    def test_formal_shard_with_fast_plan_keeps_case_order_and_receipt_identity(self):
        # The CLI's default suite is ordinary even for a Formal shard. A fast
        # plan must not apply library priority or ordinary receipt semantics.
        calls = []
        def invoke(command, cwd, env, suite, name, results, expected_count=None, **kwargs):
            calls.append(name)
            self.assertEqual(env['RUST_TEST_THREADS'], '1')
            self.assertIn('cancellation_event', kwargs)
            return ParallelNativeExecution.receipt(suite, name, results, expected_count)
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            self.execute(root, [self.entry('first', True), self.entry('second')], invoke,
                         {'first': 1, 'second': 1}, cpus=1,
                         plan=ParallelNativeExecution().mixed_selection())
            records = [json.loads(line) for line in (root / 'results.jsonl').read_text().splitlines()]
        self.assertEqual(calls, ['first', 'second'])
        self.assertTrue(all(record['binary'] == 'runtime-tests'
                            and record['shard'] == 'runtime-protocol'
                            and record['suite'] == 'runtime-protocol'
                            and record['passed'] for record in records))

    def test_malformed_weight_fails_before_any_case_starts(self):
        for value in (0, -1, 3, True, '1', None):
            calls = []
            with self.subTest(weight=value), tempfile.TemporaryDirectory() as temporary:
                with self.assertRaises(ValueError):
                    self.execute(Path(temporary), [self.entry('first')], lambda *a, **k: calls.append(a), {'first': value})
            self.assertEqual(calls, [])

    def test_candidate_cannot_expand_or_reduce_a_trusted_case_weight(self):
        base = dict(schema_version=2, suites={}, environment_formal_batches={}, shell_tests={},
                    parallel_cases={'runtime-tests': {'known': 2}})
        candidate = copy.deepcopy(base)
        candidate['parallel_cases']['runtime-tests'] = {'known': 1, 'unreviewed': 1}
        with tempfile.TemporaryDirectory() as temporary:
            authority, source = Path(temporary) / 'base.json', Path(temporary) / 'candidate.json'
            authority.write_text(json.dumps(base))
            source.write_text(json.dumps(candidate))
            with patch.object(artifacts, 'CONFIG', source), patch.object(artifacts, 'FORMAL_AUTHORITY', authority):
                self.assertEqual(artifacts.formal_parallel_cases(artifacts.formal_configuration()),
                                 {('runtime-tests', 'known'): 2})
            with patch.object(artifacts, 'CONFIG', source), patch.object(artifacts, 'FORMAL_AUTHORITY', None):
                self.assertEqual(artifacts.formal_parallel_cases(artifacts.formal_configuration()),
                                 {('runtime-tests', 'known'): 1, ('runtime-tests', 'unreviewed'): 1})

    def test_missing_duplicate_wrong_or_unclean_receipt_stops_dispatch(self):
        for damage in ('missing', 'duplicate', 'identity', 'count', 'read-error', 'leak', 'late-exit-zero'):
            calls = []
            def invoke(command, cwd, env, suite, name, results, expected_count=None, **kwargs):
                calls.append(name)
                if damage == 'late-exit-zero':
                    raise SystemExit(0)
                result = ParallelNativeExecution.receipt(suite, name, results, expected_count)
                if damage == 'missing':
                    results.unlink()
                elif damage == 'duplicate':
                    with results.open('a') as stream:
                        stream.write(json.dumps(result) + '\n')
                else:
                    field, value = {'identity': ('test_name', 'other'), 'count': ('expected_count', 0),
                                    'read-error': ('output_read_error', 'OSError'),
                                    'leak': ('leaked_processes', True)}[damage]
                    result[field] = value
                    results.write_text(json.dumps(result) + '\n')
                return result
            with self.subTest(damage=damage), tempfile.TemporaryDirectory() as temporary:
                with self.assertRaisesRegex(RuntimeError, 'formal failed'):
                    self.execute(Path(temporary), [self.entry('first'), self.entry('pending')], invoke, cpus=1)
            self.assertEqual(calls, ['first'])

    def test_later_failure_does_not_rewrite_a_completed_success(self):
        completed = threading.Event()
        def invoke(command, cwd, env, suite, name, results, expected_count=None, **kwargs):
            if name == 'success':
                result = ParallelNativeExecution.receipt(suite, name, results, expected_count)
                completed.set()
                return result
            self.assertTrue(completed.wait(timeout=2))
            time.sleep(0.02)
            return ParallelNativeExecution.receipt(suite, name, results, expected_count, passed=False, exit_code=7)
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            with self.assertRaisesRegex(RuntimeError, 'formal failed'):
                self.execute(root, [self.entry('success'), self.entry('failure')], invoke,
                             {'success': 1, 'failure': 1}, cpus=2)
            records = [json.loads(line) for line in (root / 'results.jsonl').read_text().splitlines()]
        self.assertTrue(next(record for record in records if record['test_name'] == 'success')['passed'])
        self.assertFalse(next(record for record in records if record['test_name'] == 'failure')['passed'])


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
        expected_ordinary = {'viewer', 'required', 'snapshot-tests', 'viewer-snapshot-tests'}
        if sys.platform == 'linux' and hasattr(artifacts, 'RUNTIME_WORKER_VARIANT'):
            expected_ordinary.add(artifacts.RUNTIME_WORKER_VARIANT)
        self.assertEqual({v[0] for v in ordinary}, expected_ordinary)
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
        self.assertIn('run_cargo test -p oasis7_viewer_runtime --lib snapshot_progress::', shell)
        self.assertIn('run_cargo test -p oasis7 --lib snapshot_player_gameplay_execution_state_backfills_from_legacy_fields', shell)

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
                    profile=dict(test=True, opt_level='0', debug_assertions=True, overflow_checks=True),
                    features=[], executable=str(executable)))
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


class OutputBackpressure(unittest.TestCase):
    def test_queued_packets_keep_original_descriptor_after_caller_closes_it(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            program = ('import contextlib,os,stat,sys,time\nfrom pathlib import Path\n'
                f'sys.path.insert(0,{str(HERE)!r})\nfrom ci_process_runner import run,live_output\n'
                'read_fd,write_fd=os.pipe()\n'
                'with os.fdopen(write_fd,"w") as destination, contextlib.redirect_stdout(destination):\n'
                ' run([sys.executable,"-c",\'import os; [os.write(1,b"x"*8191+b"\\\\n") for _ in range(512)]; '
                'print("test result: ok. 1 passed; 0 failed; 0 ignored;")\'],'
                f'Path({str(root)!r}),os.environ,"probe","lifetime",Path({str(root / "results")!r}),'
                'expected_count=1,timeout_seconds=3)\n'
                f'with open({str(root / "sentinel")!r},"w") as sentinel:\n'
                ' sentinel.write("untouched")\n'
                ' relay=live_output()\n'
                ' with relay.queue.mutex:\n'
                '  assert relay.queue.queue\n'
                '  assert all(stat.S_ISFIFO(os.fstat(packet[3]).st_mode) for packet in relay.queue.queue)\n'
                ' os.close(read_fd)\n'
                ' deadline=time.monotonic()+2\n'
                ' while relay.queue.unfinished_tasks and time.monotonic()<deadline: time.sleep(0.01)\n'
                ' assert not relay.queue.unfinished_tasks\n')
            process = subprocess.run([sys.executable, '-c', program], capture_output=True, timeout=6)
            self.assertEqual(process.returncode, 0, process.stderr)
            self.assertEqual((root / 'sentinel').read_text(), 'untouched')
            self.assertTrue(json.loads((root / 'results').read_text())['passed'])

    def test_explicit_log_write_failure_cannot_pass(self):
        import contextlib
        import ci_process_runner as runner
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            with (root / 'log').open('w') as log, (root / 'console').open('w') as console:
                identity = os.fstat(log.fileno())
                original = runner.write_fd
                def fail_log(fd, data):
                    current = os.fstat(fd)
                    if (current.st_dev, current.st_ino) == (identity.st_dev, identity.st_ino):
                        raise OSError('injected persistent log failure')
                    return original(fd, data)
                with contextlib.redirect_stdout(console), patch.object(runner, 'write_fd', side_effect=fail_log):
                    result = runner.run([sys.executable, '-c',
                        'print("test result: ok. 1 passed; 0 failed; 0 ignored;")'],
                        root, os.environ, 'probe', 'log-failure', root / 'results',
                        expected_count=1, timeout_seconds=3, log_stream=log)
            self.assertFalse(result['passed'])
            self.assertFalse(result['log_complete'])
            self.assertFalse(result['count_ok'])
            self.assertEqual(result['output_read_error'], 'OSError')
            self.assertEqual(result['output_bytes'], 48)
            self.assertEqual(result['cleanup_result'], 'clean')

    def test_each_run_uses_current_redirected_file(self):
        import contextlib
        from ci_process_runner import run
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for index in range(3):
                destination = root / f'output-{index}'
                with destination.open('w') as stream, contextlib.redirect_stdout(stream):
                    result = run([sys.executable, '-c',
                        f'print("destination-{index}"); print("test result: ok. 1 passed; 0 failed; 0 ignored;")'],
                        root, os.environ, 'probe', str(index), root / 'results',
                        expected_count=1, timeout_seconds=3)
                self.assertTrue(result['passed'])
                content = destination.read_text()
                self.assertIn(f'destination-{index}', content)
                self.assertIn('test result: ok.', content)
                for other in range(index):
                    self.assertNotIn(f'destination-{other}', content)

    def test_explicit_log_is_complete_with_undrained_console(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            log, results = root / 'durable.log', root / 'results'
            child = ('import os; [os.write(1,b"x"*8191+b"\\n") for _ in range(512)]; '
                     'print(\'STORAGE_CAPACITY_REPORT={"status":"passed","executed_heights":10}\'); '
                     'print(\'STORAGE_FULL_NODE_REPORT={"status":"passed"}\'); '
                     'print("test result: ok. 1 passed; 0 failed; 0 ignored;")')
            program = ('import os,sys\nfrom pathlib import Path\n'
                f'sys.path.insert(0,{str(HERE)!r})\nfrom ci_process_runner import run\n'
                f'with open({str(log)!r},"w") as stream:\n'
                f' run([sys.executable,"-c",{child!r}],Path({str(root)!r}),os.environ,'
                f'"probe","durable",Path({str(results)!r}),expected_count=1,timeout_seconds=3,log_stream=stream)\n')
            process = subprocess.Popen([sys.executable, '-c', program], stdout=subprocess.PIPE,
                                       stderr=subprocess.STDOUT)
            try:
                process.wait(timeout=5)
                timely = True
            except subprocess.TimeoutExpired:
                timely = False
            finally:
                output, _ = process.communicate(timeout=5)
            self.assertTrue(timely)
            self.assertEqual(process.returncode, 0, output[-4000:])
            result = json.loads(results.read_text())
            self.assertTrue(result['passed'] and result['log_complete'] and result['count_ok'])
            self.assertGreater(result['output_suppressed_bytes'], 0)
            content = log.read_bytes()
            self.assertEqual(len(content), result['output_bytes'])
            self.assertIn(b'STORAGE_CAPACITY_REPORT={"status":"passed","executed_heights":10}', content)
            self.assertIn(b'STORAGE_FULL_NODE_REPORT={"status":"passed"}', content)
            self.assertTrue(content.endswith(b'test result: ok. 1 passed; 0 failed; 0 ignored;\n'))

    def test_drained_live_output_preserves_heartbeat_and_summary(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            results = root / 'results.jsonl'
            program = (
                'import os,sys\nfrom pathlib import Path\n'
                f'sys.path.insert(0,{str(HERE)!r})\nfrom ci_process_runner import run\n'
                'run([sys.executable,"-c",\'import time; time.sleep(0.15); '
                'print("test result: ok. 1 passed; 0 failed; 0 ignored;")\'],'
                f'Path({str(root)!r}),os.environ,"probe","healthy",Path({str(results)!r}),'
                'expected_count=1,timeout_seconds=2,heartbeat_seconds=0.05)\n'
            )
            process = subprocess.run([sys.executable, '-c', program], capture_output=True, timeout=4)
            self.assertEqual(process.returncode, 0, process.stderr)
            self.assertIn(b'suite=probe test=healthy', process.stdout)
            self.assertIn(b'test result: ok. 1 passed;', process.stdout)
            record = json.loads(results.read_text())
            self.assertTrue(record['passed'])
            self.assertEqual(record['output_suppressed_bytes'], 0)
            self.assertEqual(record['output_pending_bytes'], 0)

    def test_summary_parser_handles_chunk_boundaries_without_retaining_long_lines(self):
        capture = _Capture()
        for piece in [b'test result: o', b'k. 4 passed; 0 fail', b'ed; 2 ignored;\n']:
            capture.feed(piece)
        self.assertEqual(capture.summary, (4, 0, 2))
        for _ in range(1024):
            capture.feed(b'x' * READ_BYTES)
            self.assertLessEqual(len(capture.line), SUMMARY_LINE_BYTES)
        capture.feed(b'\ntest result: ok. 5 passed; 0 failed; 0 ignored;\n')
        self.assertEqual(capture.summary, (5, 0, 0))
        capture.feed(b'test result: ok. ' + b'9' * READ_BYTES)
        self.assertIsNone(capture.summary, 'an oversized later summary cannot reuse an old count')

    def test_repeated_runs_share_one_bounded_relay_when_stdout_is_not_drained(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            results, observations = root / 'results.jsonl', root / 'observations.json'
            child = ('import os; print("test result: ok. 1 passed; 0 failed; 0 ignored;",flush=True); '
                     '[os.write(1,b"x"*8191+b"\\n") for _ in range(512)]')
            program = (
                'import json,os,sys,threading\nfrom pathlib import Path\n'
                f'sys.path.insert(0,{str(HERE)!r})\n'
                'from ci_process_runner import run,live_output,READ_BYTES,LIVE_CHUNKS\n'
                'before=threading.active_count()\n'
                'for i in range(3):\n'
                f' run([sys.executable,"-c",{child!r}],Path({str(root)!r}),os.environ,'
                f'"probe",str(i),Path({str(results)!r}),expected_count=1,timeout_seconds=2,heartbeat_seconds=0.05)\n'
                'relay=live_output()\n'
                f'Path({str(observations)!r}).write_text(json.dumps(dict('
                'threads=threading.active_count()-before,queued=relay.queue.qsize(),'
                'max_chunks=LIVE_CHUNKS,max_packet=max(len(p[0]) for p in relay.queue.queue),'
                'chunk_bytes=READ_BYTES)))\n'
            )
            process = subprocess.Popen([sys.executable, '-c', program], stdout=subprocess.PIPE,
                                       stderr=subprocess.STDOUT)
            try:
                process.wait(timeout=6)
                timely = True
            except subprocess.TimeoutExpired:
                timely = False
            finally:
                output, _ = process.communicate(timeout=10)
            self.assertTrue(timely, 'repeated runs must not wait for blocked relay threads')
            self.assertEqual(process.returncode, 0, output[-4000:])
            records = [json.loads(line) for line in results.read_text().splitlines()]
            self.assertEqual(len(records), 3)
            self.assertTrue(all(record['passed'] and record['count_ok'] for record in records))
            self.assertTrue(all(record['output_suppressed_bytes'] > 0 for record in records))
            self.assertTrue(all(record['output_bytes'] == 512 * 8192 + 48 for record in records))
            self.assertTrue(all(record['output_suppressed_bytes'] + record['output_pending_bytes']
                                <= record['output_bytes'] for record in records))
            stats = json.loads(observations.read_text())
            self.assertEqual(stats['threads'], 1, 'only one relay survives across all runs')
            self.assertLessEqual(stats['queued'], stats['max_chunks'])
            self.assertLessEqual(stats['max_packet'], stats['chunk_bytes'])

    def test_undrained_stdout_cannot_block_deadline_result_or_owned_cleanup(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            results, pids = root / 'results.jsonl', root / 'pids.json'
            child = (
                'import json,os,subprocess,sys,time; '
                'other=subprocess.Popen([sys.executable,"-c","import time; time.sleep(30)"]); '
                f'open({str(pids)!r},"w").write(json.dumps([os.getpid(),other.pid])); '
                'print("test result: ok. 1 passed; 0 failed; 0 ignored;",flush=True); '
                '[os.write(1,b"x"*8191+b"\\n") for _ in range(1024)]; time.sleep(30)'
            )
            program = (
                'import os,sys\nfrom pathlib import Path\n'
                f'sys.path.insert(0,{str(HERE)!r})\n'
                'from ci_process_runner import run\n'
                f'run([sys.executable,"-c",{child!r}],Path({str(root)!r}),os.environ,'
                f'"probe","blocked-output",Path({str(results)!r}),expected_count=1,'
                'timeout_seconds=0.25,heartbeat_seconds=0.05)\n'
            )
            process = subprocess.Popen([sys.executable, '-c', program], stdout=subprocess.PIPE,
                                       stderr=subprocess.STDOUT)
            try:
                # Deliberately never drain the live output until the runner has
                # exited. Its deadline/result/cleanup must not need our reader.
                process.wait(timeout=4)
                timely = True
            except subprocess.TimeoutExpired:
                timely = False
            finally:
                # Also release an unfixed runner, so the red regression leaves
                # no sleeping child or blocked pipe behind.
                output, _ = process.communicate(timeout=10)
            self.assertTrue(timely, 'live stdout prevented the deadline and runner exit')
            self.assertEqual(process.returncode, 0, output[-4000:])
            record = json.loads(results.read_text().splitlines()[-1])
            self.assertTrue(record['timeout'])
            self.assertFalse(record['passed'])
            self.assertTrue(record['count_ok'], 'summary parsing must continue independently')
            self.assertEqual(record['cleanup_result'], 'clean')
            self.assertLess(record['elapsed_seconds'], 3)
            self.assertGreater(record['output_suppressed_bytes'], 0)
            for pid in json.loads(pids.read_text()):
                state = subprocess.run(['ps', '-p', str(pid), '-o', 'stat='],
                                       capture_output=True, text=True).stdout.strip()
                self.assertTrue(not state or state.startswith('Z'), (pid, state))

    def test_child_exit_zero_observed_after_deadline_cannot_pass(self):
        original_popen = subprocess.Popen
        def delayed_observation(*args, **kwargs):
            child = original_popen(*args, **kwargs)
            # A real child completes successfully after the deadline while the
            # supervisor cannot observe it yet (as with a stalled output call).
            child.wait(timeout=2)
            return child
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            with patch('ci_process_runner.subprocess.Popen', side_effect=delayed_observation):
                result = run([sys.executable, '-c',
                              'import time; time.sleep(0.15); print("test result: ok. 1 passed; 0 failed; 0 ignored;")'],
                             root, os.environ, 'probe', 'late-success', root / 'results.jsonl',
                             expected_count=1, timeout_seconds=0.05, heartbeat_seconds=0.01)
            self.assertEqual(result['exit_code'], 0)
            self.assertTrue(result['count_ok'])
            self.assertTrue(result['timeout'])
            self.assertFalse(result['passed'])


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

    def test_real_native_partitions_preserve_feature_isolation_and_complete_harnesses(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            (root / 'Cargo.toml').write_text('[workspace]\nmembers=["runtime","world"]\nresolver="2"\n')
            for directory, package in (('runtime', 'oasis7_runtime'), ('world', 'oasis7_world_state')):
                member = root / directory
                (member / 'src').mkdir(parents=True)
                (member / 'tests').mkdir()
                manifest = ('[package]\nname="' + package + '"\nversion="0.1.0"\nedition="2021"\n'
                            '[features]\ndefault=["bad-default"]\nbad-default=[]\ntest_tier_required=[]\ntest-support=[]\n')
                if directory == 'runtime':
                    manifest += ('[dev-dependencies]\noasis7_world_state={path="../world",'
                                 'default-features=false,features=["test-support"]}\n')
                (member / 'Cargo.toml').write_text(manifest)
                source = ('pub fn test_support_enabled() -> bool { cfg!(feature="test-support") }\n'
                          '#[test] fn required_case() { assert!(cfg!(feature="test_tier_required")); '
                          'assert!(!cfg!(feature="bad-default")); CHECK }\n'
                          '#[test] fn added_regression() { assert_eq!(2 + 2, 4); }\n'
                          '#[test] #[ignore] fn ignored_case() { panic!("ignored inventory changed"); }\n')
                if directory == 'runtime':
                    source = source.replace('CHECK', 'assert!(oasis7_world_state::test_support_enabled());')
                else:
                    source = ('#[cfg(all(test, feature="test-support"))]\n'
                              'compile_error!("cross-package test-support leaked into WorldState root");\n'
                              + source.replace('CHECK', 'assert!(!test_support_enabled());'))
                (member / 'src/lib.rs').write_text(source)
                (member / 'tests/pr_fast.rs').write_text(
                    '#[test] fn required_case() { assert!(cfg!(feature="test_tier_required")); }\n'
                    '#[test] fn added_integration() { assert_eq!(3 + 3, 6); }\n')
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
            plan = NativePackagePartitions().selection() | {
                'source_sha': sha, 'tested_sha': sha, 'policy_sha': sha,
                'architecture': dict(source=sha, tested=sha)}
            for target in plan['ordinary_fast_targets']:
                target['cases'] = ['required_case']
            # The negative control proves this real fixture detects the feature
            # union which independent package builds must continue to avoid.
            merged = subprocess.run(['cargo', 'test', '--locked', '-p', 'oasis7_runtime', '-p', 'oasis7_world_state',
                '--lib', '--no-default-features', '--features', 'test_tier_required', '--no-run'],
                cwd=root, text=True, capture_output=True)
            self.assertNotEqual(merged.returncode, 0)
            self.assertIn('cross-package test-support leaked', merged.stderr)
            observed = []
            with patch.object(artifacts, 'ROOT', root), patch.object(artifacts, 'CONFIG', config), \
                    patch.object(artifacts, 'native_cpu_budget', return_value=4), \
                    patch.dict(os.environ, SOURCE_SHA=sha):
                for partition in plan['ordinary_partitions']:
                    bundle = root / ('bundle-' + partition)
                    artifacts.build(bundle, 'ordinary', plan=plan, ordinary_partition=partition)
                    manifest = json.loads((bundle / 'manifest.json').read_text())
                    self.assertEqual(manifest['execution_plan'], plan)
                    self.assertEqual(manifest['ordinary_partition'], partition)
                    binaries = [binary for binary in manifest['binaries'].values() if binary['ordinary']]
                    self.assertEqual(len(binaries), 2)
                    self.assertTrue(all(binary['package'] == partition for binary in binaries))
                    self.assertEqual(sorted(len(binary['ignored_tests']) for binary in binaries), [0, 1])
                    observed.extend(binary['variant'] for binary in binaries)
                    archive = root / ('partial-' + partition + '.tar.gz')
                    with self.assertRaisesRegex(ValueError, 'complete bundles'):
                        artifacts.pack(bundle, archive, suite='ordinary', plan=plan)
                    self.assertFalse(archive.exists())
                    results = root / (partition + '.jsonl')
                    artifacts.execute(bundle, 'ordinary', results, plan=plan, ordinary_partition=partition)
                    records = [json.loads(line) for line in results.read_text().splitlines()]
                    self.assertEqual(len(records), 2)
                    self.assertTrue(all(record['passed'] and record['expected_count'] == 2 for record in records))
            self.assertEqual(sorted(observed), ['required-fast-' + str(index) for index in range(4)])

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

    def prepare_target_workspace(self, root, source, integration=None, unrelated_source=None, worker=False):
        (root / 'Cargo.toml').write_text('[workspace]\nmembers=["codec","unrelated"]\nresolver="2"\n')
        for directory, package, text in (
            ('codec', 'oasis7_viewer_codec', source),
            ('unrelated', 'oasis7_runtime', unrelated_source if unrelated_source is not None
             else 'compile_error!("unselected package was compiled");'),
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
        if worker and sys.platform == 'linux' and hasattr(artifacts, 'RUNTIME_WORKER_VARIANT'):
            # Full Linux bundles must deliver a genuine Cargo-built companion,
            # even though this fixture's executed harnesses belong to codec.
            manifest = root / 'Cargo.toml'
            manifest.write_text(manifest.read_text().replace('"unrelated"]', '"unrelated","worker"]'))
            (root / 'worker/src').mkdir(parents=True)
            (root / 'worker/Cargo.toml').write_text(
                '[package]\nname="oasis7_wasm_executor"\nversion="0.1.0"\nedition="2021"\n'
                '[features]\nwasmtime=[]\n[[bin]]\nname="oasis7-wasm-worker"\npath="src/main.rs"\n')
            (root / 'worker/src/main.rs').write_text('fn main() {}\n')
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

    def test_real_inline_combined_build_keeps_each_original_target_and_features(self):
        source = '''
#[cfg(feature="default")] compile_error!("default feature was incorrectly enabled");
#[cfg(all(test,feature="test_tier_required"))] mod required {
    #[test] fn round_trip() { assert_eq!(1 + 1, 2); }
    #[test] fn newly_added() { assert_eq!(2 + 2, 4); }
    #[test] #[ignore] fn expensive() { panic!("ignored test ran"); }
}
'''
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            config, sha = self.prepare_target_workspace(root, source, source)
            target = FastTargetInventory().target
            selected = InlineExecutionSelection().selection() | {
                'architecture': dict(source=sha, tested=sha),
                'ordinary_fast_targets': [target(), target(target_kind='lib', target_name='oasis7_viewer_codec')],
            }
            with patch.object(artifacts, 'ROOT', root), patch.object(artifacts, 'CONFIG', config), \
                 patch.object(artifacts, 'native_cpu_budget', return_value=2), \
                 patch.dict(os.environ, SOURCE_SHA=sha):
                bundle = root / 'bundle'
                artifacts.build(bundle, 'ordinary', plan=selected)
                manifest = json.loads((bundle / 'manifest.json').read_text())
                self.assertEqual(len(manifest['build_timings']), 1)
                binaries = list(manifest['binaries'].values())
                self.assertEqual({binary['variant'] for binary in binaries}, {'required-fast-0', 'required-fast-1'})
                self.assertEqual({tuple(binary['target_kind']) for binary in binaries}, {('lib',), ('test',)})
                self.assertTrue(all(binary['features'] == ['test_tier_required'] for binary in binaries))
                self.assertTrue(all(binary['default_features'] is False for binary in binaries))
                results = root / 'results.jsonl'
                artifacts.execute(bundle, 'ordinary', results, plan=selected)
                records = [json.loads(line) for line in results.read_text().splitlines()]
                self.assertEqual(len(records), 2)
                self.assertTrue(all(record['passed'] and record['expected_count'] == 2 for record in records))
                altered = copy.deepcopy(manifest)
                del altered['binaries'][next(iter(altered['binaries']))]
                (bundle / 'manifest.json').write_text(json.dumps(altered))
                with patch('ci_process_runner.run') as execute_test:
                    with self.assertRaisesRegex(ValueError, 'target inventory'):
                        artifacts.execute(bundle, 'ordinary', root / 'bad-results.jsonl', plan=selected)
                    execute_test.assert_not_called()

    def test_real_inline_harnesses_overlap_without_dropping_new_cases(self):
        def source(name, other):
            return '''
#[cfg(feature="default")] compile_error!("default feature was incorrectly enabled");
#[cfg(all(test,feature="test_tier_required"))] mod required {
    #[test] fn round_trip() {
        let root = std::path::PathBuf::from(std::env::var("OASIS7_CI_NATIVE_PARALLEL_FIXTURE").unwrap());
        std::fs::write(root.join("SELF.started"), b"started").unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while !root.join("OTHER.started").is_file() {
            assert!(std::time::Instant::now() < deadline, "other full harness never started");
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }
    #[test] fn newly_added() {
        let root = std::path::PathBuf::from(std::env::var("OASIS7_CI_NATIVE_PARALLEL_FIXTURE").unwrap());
        assert_eq!(std::env::var("RUST_TEST_THREADS").unwrap(), "1");
        std::fs::write(root.join("SELF.new"), b"executed").unwrap();
    }
    #[test] #[ignore] fn expensive() { panic!("ignored test ran"); }
}
'''.replace('SELF', name).replace('OTHER', other)
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            config, sha = self.prepare_target_workspace(root, source('lib', 'integration'),
                                                       source('integration', 'lib'))
            target = FastTargetInventory().target
            selected = InlineExecutionSelection().selection() | {
                'architecture': dict(source=sha, tested=sha),
                'ordinary_fast_targets': [target(), target(target_kind='lib', target_name='oasis7_viewer_codec')],
            }
            with patch.object(artifacts, 'ROOT', root), patch.object(artifacts, 'CONFIG', config), \
                 patch.object(artifacts, 'native_cpu_budget', return_value=2), \
                 patch.dict(os.environ, SOURCE_SHA=sha, OASIS7_CI_NATIVE_PARALLEL_FIXTURE=str(root)):
                bundle = root / 'bundle'
                artifacts.build(bundle, 'ordinary', plan=selected)
                results = root / 'results.jsonl'
                artifacts.execute(bundle, 'ordinary', results, plan=selected)
                records = [json.loads(line) for line in results.read_text().splitlines()]
                self.assertEqual(len(records), 2)
                self.assertTrue(all(record['passed'] and record['expected_count'] == 2 for record in records))
                self.assertTrue(all((root / (name + suffix)).is_file()
                                    for name in ('lib', 'integration') for suffix in ('.started', '.new')))

    def test_real_inline_libraries_start_together_before_validated_integrations(self):
        def source(name, other=None):
            barrier = ('''
        std::fs::write(root.join("SELF.started"), b"started").unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while !root.join("OTHER.started").is_file() {
            assert!(std::time::Instant::now() < deadline, "other library never started");
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
''' if other else '''
        assert!(root.join("codec.started").is_file(), "codec library was delayed");
        assert!(root.join("runtime.started").is_file(), "runtime library was delayed");
''')
            return ('''
#[cfg(feature="default")] compile_error!("default feature was incorrectly enabled");
#[cfg(all(test,feature="test_tier_required"))] mod required {
    #[test] fn round_trip() {
        let root = std::path::PathBuf::from(std::env::var("OASIS7_CI_NATIVE_PRIORITY_FIXTURE").unwrap());
        BARRIER
    }
    #[test] fn newly_added() {
        let root = std::path::PathBuf::from(std::env::var("OASIS7_CI_NATIVE_PRIORITY_FIXTURE").unwrap());
        assert_eq!(std::env::var("RUST_TEST_THREADS").unwrap(), "1");
        std::fs::write(root.join("SELF.new"), b"executed").unwrap();
    }
    #[test] #[ignore] fn expensive() { panic!("ignored test ran"); }
}
'''.replace('BARRIER', barrier).replace('SELF', name).replace('OTHER', other or 'unused'))
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            config, sha = self.prepare_target_workspace(root, source('codec', 'runtime'),
                source('integration'), unrelated_source=source('runtime', 'codec'), worker=True)
            target = FastTargetInventory().target
            selected = InlineExecutionSelection().selection() | {
                'architecture': dict(source=sha, tested=sha),
                'ordinary_fast_targets': [target(),
                    target(target_kind='lib', target_name='oasis7_viewer_codec'),
                    target(package='oasis7_runtime', target_kind='lib', target_name='oasis7_runtime')],
            }
            with patch.object(artifacts, 'ROOT', root), patch.object(artifacts, 'CONFIG', config), \
                 patch.object(artifacts, 'native_cpu_budget', return_value=2), \
                 patch.dict(os.environ, SOURCE_SHA=sha, OASIS7_CI_NATIVE_PRIORITY_FIXTURE=str(root)):
                bundle = root / 'bundle'
                artifacts.build(bundle, 'ordinary', plan=selected)
                manifest_path = bundle / 'manifest.json'
                manifest = json.loads(manifest_path.read_text())
                # Compiler completion order is not the desired dispatch order.
                # Reproduce the hosted integration-before-library inventory.
                manifest['binaries'] = dict(sorted(manifest['binaries'].items(),
                    key=lambda item: item[1]['target_kind'] == ['lib']))
                manifest_path.write_text(json.dumps(manifest))
                results = root / 'results.jsonl'
                artifacts.execute(bundle, 'ordinary', results, plan=selected)
                records = [json.loads(line) for line in results.read_text().splitlines()]
                self.assertEqual(len(records), 3)
                self.assertTrue(all(record['passed'] and record['expected_count'] == 2 for record in records))
                self.assertTrue(all((root / (name + '.new')).is_file()
                                    for name in ('codec', 'runtime', 'integration')))
                integration = next(name for name, binary in manifest['binaries'].items()
                                   if binary['target_kind'] == ['test'])
                # Deferred integration validation must still finish before any
                # priority library can launch, including its actual --list data.
                for damage in ('inventory', 'features', 'source'):
                    altered = copy.deepcopy(manifest)
                    if damage == 'inventory':
                        altered['binaries'][integration]['tests'].append('invented_case')
                    elif damage == 'features':
                        altered['binaries'][integration]['features'].append('default')
                    else:
                        altered['source_sha'] = '0' * 40
                    manifest_path.write_text(json.dumps(altered))
                    with self.subTest(damage=damage), patch('ci_process_runner.run') as execute_test:
                        with self.assertRaises(ValueError):
                            artifacts.execute(bundle, 'ordinary', root / 'bad-results.jsonl', plan=selected)
                        execute_test.assert_not_called()

    def test_real_full_bundle_runs_default_and_no_default_variants_of_same_lib(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            config, sha = self.prepare_target_workspace(root,
                '#[test] fn common() {}\n'
                '#[cfg(feature="default")] #[test] fn original_default() {}\n'
                '#[cfg(not(feature="default"))] #[test] fn fast_only() {}\n'
                '#[cfg(not(feature="default"))] #[test] fn newly_added() {}\n', worker=True)
            target = FastTargetInventory().target(target_kind='lib', target_name='oasis7_viewer_codec', cases=['fast_only'])
            selected = FastTargetInventory().selection([target], full=True)
            selected.update(ordinary_artifact_partition_version=1,
                            ordinary_partitions=list(artifacts.ORDINARY_ARTIFACT_PARTITIONS))
            selected['architecture'] = dict(source=sha, tested=sha)
            # Restrict only the old package inventory to this real tiny workspace;
            # build, Cargo feature resolution, pack, download checks and run stay real.
            with patch.object(artifacts, 'ROOT', root), patch.object(artifacts, 'CONFIG', config), \
                 patch.object(artifacts, 'required_packages', return_value=['oasis7_viewer_codec']), \
                 patch.dict(os.environ, SOURCE_SHA=sha):
                bundle, consumer = root / 'producer', root / 'consumer'
                artifacts.build(bundle, 'ordinary', plan=selected)
                manifest = json.loads((bundle / 'manifest.json').read_text())
                if sys.platform == 'linux' and hasattr(artifacts, 'RUNTIME_WORKER_VARIANT'):
                    companion = manifest['worker_companion']
                    self.assertEqual(companion['package'], 'oasis7_wasm_executor')
                    self.assertEqual(companion['target'], 'oasis7-wasm-worker')
                    self.assertEqual(companion['requested_features'], ['wasmtime'])
                    self.assertTrue((bundle / companion['path']).is_file())
                self.assertEqual(len(manifest['binaries']), 2)
                self.assertEqual({entry['target'] for entry in manifest['binaries'].values()}, {'oasis7_viewer_codec'})
                self.assertEqual(sorted('default' in entry['features'] for entry in manifest['binaries'].values()), [False, True])
                archive_path = root / 'ordinary.tar.gz'
                artifacts.pack(bundle, archive_path, suite='ordinary', plan=selected)
                with tarfile.open(archive_path) as archive:
                    archive.extractall(consumer, filter='data')
                for partition in ('component-libs', None):
                    results = root / f'results-{partition}.jsonl'
                    artifacts.execute(consumer, 'ordinary', results, plan=selected, ordinary_partition=partition)
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

    def test_pre_cancelled_worker_never_starts_a_child(self):
        cancelled = threading.Event()
        cancelled.set()
        with patch('ci_process_runner.subprocess.Popen') as child:
            with self.assertRaisesRegex(InterruptedError, 'cancelled before'):
                self.execute([sys.executable, '-c', 'raise SystemExit(0)'], cancellation_event=cancelled)
            child.assert_not_called()

    def test_shared_cancellation_cannot_be_overridden_by_a_late_zero_exit(self):
        cancelled = threading.Event()
        real_popen = subprocess.Popen
        def start(*args, **kwargs):
            process = real_popen(*args, **kwargs)
            real_poll = process.poll
            def poll():
                code = real_poll()
                if code == 0:
                    cancelled.set()
                return code
            process.poll = poll
            return process
        with patch('ci_process_runner.subprocess.Popen', side_effect=start):
            result = self.execute([sys.executable, '-c',
                'print("test result: ok. 1 passed; 0 failed; 0 ignored;")'], cancellation_event=cancelled)
        self.assertTrue(result['cancelled'])
        self.assertFalse(result['passed'])
        self.assertEqual(result['cleanup_result'], 'clean')

    def test_cancelling_one_parallel_owner_preserves_other_and_unrelated_processes(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            cancelled, other_cancelled = threading.Event(), threading.Event()
            observing, resume_observation = threading.Event(), threading.Event()
            children = []
            owners = {}
            real_popen = subprocess.Popen
            foreign = real_popen([sys.executable, '-c', 'import time; time.sleep(20)'])
            def start(*args, **kwargs):
                process = real_popen(*args, **kwargs)
                children.append(process)
                role = kwargs['env']['OASIS7_NATIVE_FIXTURE_ROLE']
                owners[role] = process
                if role == 'first':
                    real_poll = process.poll
                    def poll():
                        if not observing.is_set():
                            observing.set()
                            if not resume_observation.wait(timeout=5):
                                raise RuntimeError('cancellation observation was not released')
                        return real_poll()
                    process.poll = poll
                return process
            def execute(name, cancellation):
                program = (
                    'from pathlib import Path; import os,signal,subprocess,sys,time\n'
                    f'root=Path({str(root)!r}); name={name!r}\n'
                    'if name == "first":\n'
                    '    child=subprocess.Popen([sys.executable,"-c","import time; time.sleep(.05)"])\n'
                    '    def finish(signum,frame):\n'
                    '        child.wait(timeout=2)\n'
                    '        (root/"first.reaped").write_text(str(child.returncode))\n'
                    '        print("test result: ok. 1 passed; 0 failed; 0 ignored;",flush=True)\n'
                    '        raise SystemExit(0)\n'
                    '    signal.signal(signal.SIGTERM,finish)\n'
                    '(root/(name+".ready")).write_text(str(os.getpid()))\n'
                    'while not (root/(name+".release")).exists(): time.sleep(0.01)\n'
                    'print("test result: ok. 1 passed; 0 failed; 0 ignored;")\n'
                )
                return run([sys.executable, '-c', program], root,
                           dict(os.environ, OASIS7_NATIVE_FIXTURE_ROLE=name), 'fixture', name,
                           root / (name + '.jsonl'), expected_count=1,
                           timeout_seconds=10, cancellation_event=cancellation)
            try:
                with patch('ci_process_runner.subprocess.Popen', side_effect=start):
                    with ThreadPoolExecutor(max_workers=2) as pool:
                        first = pool.submit(execute, 'first', cancelled)
                        other = pool.submit(execute, 'other', other_cancelled)
                        try:
                            deadline = time.monotonic() + 3
                            while not observing.is_set() or not all(
                                    (root / (name + '.ready')).exists() for name in ('first', 'other')):
                                self.assertLess(time.monotonic(), deadline, 'both children must really start')
                                time.sleep(0.01)
                            cancelled.set()
                            # Deliver TERM through the original Popen handle in
                            # this PID namespace. The fixture reaps its real
                            # descendant before supervision observes its zero
                            # exit; cancellation must still win. /proc may be
                            # mounted from the host namespace in local sandboxes.
                            owners['first'].terminate()
                            self.assertEqual(owners['first'].wait(timeout=3), 0)
                            self.assertEqual((root / 'first.reaped').read_text(), '0')
                            resume_observation.set()
                            first_result = first.result(timeout=6)
                            self.assertIsNone(foreign.poll(), 'unrelated same-user process was killed')
                            self.assertFalse(other.done(), 'other owner was cancelled or killed')
                            os.kill(int((root / 'other.ready').read_text()), 0)
                            (root / 'other.release').touch()
                            other_result = other.result(timeout=3)
                            self.assertTrue(first_result['cancelled'])
                            self.assertFalse(first_result['passed'])
                            self.assertEqual(first_result['exit_code'], 0)
                            self.assertTrue(first_result['count_ok'])
                            self.assertEqual(first_result['cleanup_result'], 'clean')
                            self.assertTrue(other_result['passed'])
                        finally:
                            resume_observation.set()
                            for name in ('first', 'other'):
                                (root / (name + '.release')).touch()
                            cancelled.set()
                            other_cancelled.set()
            finally:
                # These are the fixture's original Popen handles, never PIDs
                # inferred from another process or another owner's /proc view.
                for process in [*children, foreign]:
                    if process.poll() is None:
                        process.terminate()
                    process.wait(timeout=3)

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
                    profile=dict(opt_level='0', debug_assertions=True, overflow_checks=True),
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
