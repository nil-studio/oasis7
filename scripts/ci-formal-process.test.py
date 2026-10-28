#!/usr/bin/env python3
"""Coverage, transition-gate and real libtest runner regressions."""
import copy
import importlib.util
import json
import os
from pathlib import Path
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
        manifest = json.loads((self.cache / 'manifest.json').read_text())['inputs']
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
            config = dict(suites={'healthy-set': [dict(binary='runtime-tests', name='formal_acceptance', ignored=True)]},
                environment_formal_batches={}, legacy_ignored_filters={'runtime-tests': ['formal_acceptance']},
                shell_tests={}, auxiliary_entries={'runtime-tests': ['crash_child']},
                binary_dependencies={'runtime-tests': ['runtime']},
                case_shards={'runtime-tests': {'formal_acceptance': 'storage-recovery'}})
            config_path.write_text(json.dumps(config))
            manifest = dict(schema_version=2, scope='formal', tested_sha='tested', source_sha='source',
                producer_run_id='run', checkout=str(root), config_sha256=artifacts.digest(config_path),
                rust_toolchain=os.environ.get('RUST_TOOLCHAIN', 'local'), runner_image=os.environ.get('ImageOS', sys.platform),
                binaries=binaries, suites=artifacts.resolve_suites(config, binaries), libraries={})
            (bundle / 'manifest.json').write_text(json.dumps(manifest))
            archive_path, consumer = root / 'storage-recovery.tar.gz', root / 'consumer'
            with patch.object(artifacts, 'ROOT', root), patch.object(artifacts, 'CONFIG', config_path), patch.object(artifacts, 'output', return_value='tested'), patch.dict(os.environ, SOURCE_SHA='source', GITHUB_RUN_ID='run'):
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
