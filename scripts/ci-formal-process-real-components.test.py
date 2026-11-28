#!/usr/bin/env python3
"""RealFormalComponents regressions split from ci-formal-process.test.py."""
import copy
from contextlib import ExitStack
import hashlib
import importlib.util
import itertools
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import sys
import tarfile
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
            '[workspace]\nmembers=["crates/oasis7","runtime","distfs"]\nresolver="2"\n')
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
        for directory, package in (('crates/oasis7', 'oasis7'), ('runtime', 'oasis7_chain_runtime'),
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

    def test_real_native_owners_without_umbrella_keep_both_formal_component_roles(self):
        checkout = self.case / 'native-checkout'
        shutil.copytree(self.root, checkout, ignore=shutil.ignore_patterns('target'))
        manifest_path = checkout / 'Cargo.toml'
        manifest_path.write_text(manifest_path.read_text().replace('"crates/oasis7",', ''))
        config_path = checkout / 'scripts/ci-formal-process-suites.json'
        config = json.loads(config_path.read_text())
        authority_path = self.case / 'old-authority.json'
        authority_path.write_text(json.dumps(config))
        alias = 'oasis7-lib-tests'
        for key in ('binary_dependencies', 'environment_formal_batches', 'legacy_ignored_filters'):
            del config[key][alias]
        config['suites']['runtime-protocol'] = [
            entry for entry in config['suites']['runtime-protocol'] if entry['binary'] != alias
        ]
        config_path.write_text(json.dumps(config))
        subprocess.run(['cargo', 'generate-lockfile', '--offline'], cwd=checkout, check=True)
        debug, endpoint, merged = (self.case / name for name in ('native-debug', 'native-endpoint', 'native-merged'))
        with patch.object(artifacts, 'ROOT', checkout), patch.object(artifacts, 'CONFIG', config_path):
            artifacts.build(debug, 'formal-debug')
            artifacts.build(endpoint, 'formal-endpoint')
            artifacts.merge_formal(debug, endpoint, merged)
            manifest = json.loads((merged / 'manifest.json').read_text())
            self.assertNotIn(alias, manifest['binaries'])
            self.assertEqual({key for key in manifest['binaries'] if key in ('runtime', 'endpoint')},
                             {'runtime', 'endpoint'})
            self.assertEqual(manifest['suites'], artifacts.resolve_suites(config, manifest['binaries']))
            stale = json.loads(authority_path.read_text())
            stale['suites'] = copy.deepcopy(config['suites'])
            stale['legacy_ignored_filters'] = copy.deepcopy(config['legacy_ignored_filters'])
            for binary in (alias, 'unknown-formal-batch'):
                with self.subTest(binary=binary):
                    stale['environment_formal_batches'] = config['environment_formal_batches'] | {binary: 'runtime-protocol'}
                    authority_path.write_text(json.dumps(stale))
                    with patch.object(artifacts, 'FORMAL_AUTHORITY', authority_path):
                        with self.assertRaisesRegex(ValueError, f'required Formal batch binary is missing: {binary}'):
                            artifacts.resolve_suites(artifacts.formal_configuration(), manifest['binaries'])

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
        (checkout / 'crates/oasis7/src/lib.rs').write_text('compile_error!("intentional build failure");\n')
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
            'host-linker': lambda m: m['compiler_environment'].update(
                CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER='/changed/linker'),
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


if __name__ == '__main__':
    unittest.main()
