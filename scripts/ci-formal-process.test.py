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
        self.assertEqual(sum(e.get('ignored', False) for entries in suites.values() for e in entries), 24)
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
    def test_formal_builder_does_not_wait_for_ordinary_targets(self):
        formal = artifacts.build_variants('formal')
        ordinary = artifacts.build_variants('ordinary')
        self.assertEqual({v[0] for v in formal}, {'formal-required', 'runtime', 'endpoint', 'distfs-tests'})
        self.assertEqual({v[0] for v in ordinary}, {'viewer', 'required', 'snapshot-tests'})
        args = next(args for name, args, _ in formal if name == 'formal-required')
        self.assertIn('--bins', args)
        self.assertIn('--lib', args)
        self.assertNotIn('oasis7_viewer_runtime', args)
        self.assertNotIn('oasis7_ops_tools', args)
        self.assertIn('ci_formal_required_features', args[args.index('--features') + 1].split(','))
        for variant, command, features in [*ordinary, *formal[1:], *artifacts.build_variants('all')]:
            self.assertNotIn('ci_formal_required_features', features, variant)
            self.assertFalse(any('ci_formal_required_features' in arg for arg in command), variant)
        snapshot = next(args for name, args, _ in ordinary if name == 'snapshot-tests')
        self.assertNotIn('--features', snapshot)
        required = next(args for name, args, _ in ordinary if name == 'required')
        required_packages = [required[index + 1] for index, value in enumerate(required) if value == '-p']
        self.assertCountEqual(required_packages, [
            'oasis7', 'oasis7_simulator', 'oasis7_runtime', 'oasis7_world_service', 'scenario_test_runner', 'oasis7_chain_runtime', 'oasis7_client_cli', 'oasis7_launcher_backend',
            'oasis7_newapi_bridge_service', 'oasis7_ops_tools', 'oasis7_provider_local_bridge',
            'oasis7_provider_tools', 'oasis7_viewer_runtime', 'oasis7_observability',
            'oasis7_hosted_access', 'oasis7_letai_credit_adapter', 'oasis7_launcher_config',
            'oasis7_wasm_materializer', 'oasis7_game_client', 'oasis7_cas', 'oasis7_formal_protocol',
        ])
        self.assertIn('--tests', required)

    def test_closure_contains_child_executables_but_not_other_suites(self):
        config = {'binary_dependencies': {'runtime-tests': ['runtime', 'endpoint'],
                                           'distfs-tests': ['endpoint']}, 'script_dependencies': {}}
        binaries = {name: {'test': name.endswith('tests')} for name in
                    ('runtime-tests', 'runtime', 'endpoint', 'distfs-tests', 'viewer', 'snapshot-tests')}
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
            for name in ('viewer', 'snapshot-tests', 'required-test-lib-example', 'endpoint'):
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

    def test_cache_budget_is_a_skip_decision_not_a_test_failure(self):
        with tempfile.TemporaryDirectory() as temp:
            target = Path(temp)
            (target / 'artifact').write_bytes(b'x' * 101)
            self.assertFalse(artifacts.cache_budget(target, 100)['save'])
            self.assertTrue(artifacts.cache_budget(target, 101)['save'])


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

    def cargo(self, state=None, success=True, extra_env=None):
        env = dict(os.environ, CARGO_INCREMENTAL='0', CACHE_FIXTURE_ENV='original')
        env.pop('RUSTC_WRAPPER', None)
        env.update(extra_env or {})
        messages = []
        with tempfile.TemporaryFile(mode='w+') as errors:
            with subprocess.Popen(['cargo', 'build', '--workspace', '--offline', '--locked',
                                   '--message-format=json'], cwd=self.root, env=env,
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

    def seed(self):
        state = artifacts.cache_begin(self.root)
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

    def test_new_deleted_and_unsafe_cache_inputs_are_misses(self):
        # Also retain Cargo's default package-directory observation when a
        # build script does not narrow its inputs with rerun-if directives.
        self.sources['app/build.rs'] = self.sources['app/build.rs'].replace(
            'println!("cargo:rerun-if-changed=input.txt"); println!("cargo:rerun-if-env-changed=CACHE_FIXTURE_ENV");', '')
        (self.root / 'app/build.rs').write_text(self.sources['app/build.rs'])
        self.seed()
        for change in ('new', 'deleted', 'source-symlink'):
            with self.subTest(change=change):
                self.checkout()
                path = self.root / 'app/input.txt'
                if change == 'new':
                    (self.root / 'app/new.txt').write_text('new ignored input')
                else:
                    path.unlink()
                    if change == 'source-symlink':
                        path.symlink_to(self.base / 'dependency/src/lib.rs')
                report = artifacts.cache_restore(self.root, self.cache)
                self.assertFalse(report['restored'], report)
                self.assertFalse((self.root / 'target/debug/cache_app').exists())

                if change != 'source-symlink':
                    units, _ = self.cargo(success=change != 'deleted')
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
                'snapshot-tests': '#[cfg(test)] mod snapshot_progress { #[test] fn one() {} } #[test] fn snapshot_player_gameplay_execution_state_backfills_from_legacy_fields() {}',
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
