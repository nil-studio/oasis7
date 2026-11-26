#!/usr/bin/env python3
"""Process part-1 real libtest and Cargo build regressions split from ci-formal-process.test.py."""
import copy
import importlib.util
import json
import os
from pathlib import Path
import re
import shutil
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
from ci_process_runner import run


def compile_rust_fixture(source, executable, *, test=False, env=None):
    environment = os.environ if env is None else env
    command = ['rustc', *(['--test'] if test else []), str(source), '-o', str(executable)]
    linker = None
    if sys.platform == 'linux':
        version = subprocess.check_output(['rustc', '-vV'], text=True, env=environment)
        host = next(line[6:] for line in version.splitlines() if line.startswith('host: '))
        linker = environment.get('CARGO_TARGET_' + host.upper().replace('-', '_') + '_LINKER')
        if linker:
            command += ['-C', 'linker=' + linker]
    subprocess.run(command, check=True, env=environment)
    if linker and Path(linker).name == 'oasis7-mold-gcc':
        comment = subprocess.check_output(
            ['readelf', '-p', '.comment', str(executable)], text=True, env=environment)
        if not re.search(r'\bmold 3\.0\.0(?:\s|$)', comment):
            raise AssertionError('native Rust fixture was not linked by the pinned mold')


def fast_selection(packages=('oasis7_viewer_codec',), suites=()):
    return dict(fast_plan_version=1, execution_profile='pr-fast', scope='targeted',
                groups=['baseline', 'oasis7_required'] + (['formal_process'] if suites else []),
                oasis7_required_packages=list(packages),
                oasis7_required_wasm_packages=[p for p in packages if p in artifacts.PORTABLE_REQUIRED_PACKAGES],
                formal_suites=list(suites), test_policy_changed=False,
                architecture=dict(source='source', tested='tested'))


def risk_selection():
    target = dict(package='oasis7_viewer_codec', target_kind='test', target_name='scale',
                  default_features=True, features=['test_tier_full'], cases=['full_scale'])
    return fast_selection(('oasis7_viewer_codec',)) | dict(
        risk_test_inventory_version=1, ordinary_risk_targets=[target])


def fast_target(**changes):
    return dict(package='oasis7_viewer_codec', target_kind='test', target_name='pr_fast',
                default_features=False, features=['test_tier_required'], cases=['required::round_trip']) | changes


def fast_target_selection(targets=None):
    selected = fast_selection(('oasis7_viewer_codec', 'oasis7_runtime'))
    selected['fast_test_inventory_version'] = 1
    selected['ordinary_fast_targets'] = [fast_target()] if targets is None else targets
    return selected


def inline_selection(checks=None):
    return fast_target_selection() | {
        'fast_execution_version': 1, 'ordinary_execution': 'inline',
        'rust_baseline_checks': [] if checks is None else checks,
    }


def native_package_partition_selection():
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


class ProcessPart1(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.temp = tempfile.TemporaryDirectory()
        cls.root = Path(cls.temp.name)
        source = cls.root / 'fixture.rs'
        source.write_text('#[test] fn ordinary() {}\n#[test] #[ignore] fn acceptance() {}\n#[test] #[ignore] fn crash_child() {}\n')
        cls.binary = cls.root / 'fixture'
        compile_rust_fixture(source, cls.binary, test=True)

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

    def test_real_fast_formal_deferral_is_exact_and_full_preserves_failure(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source, executable = root / 'tests.rs', root / 'runtime-tests'
            source.write_text('''
#[test] fn ordinary() { std::fs::write("ordinary-ran", "yes").unwrap(); }
#[test] fn formal_heavy() { std::fs::write("heavy-ran", "yes").unwrap(); panic!("full failure"); }
#[test] fn formal_heavy_neighbor() {
    std::fs::write("neighbor-ran", "yes").unwrap();
    assert!(std::env::var_os("FAIL_ORDINARY_NEIGHBOR").is_none());
}
#[test] #[ignore] fn ignored_case() { panic!("ignored"); }
''')
            compile_rust_fixture(source, executable, test=True)
            selected = fast_selection(('oasis7_chain_runtime',)) | dict(
                formal_full_profiles_only_version=1, formal_shards=[],
                formal_deferred_ordinary_cases={'runtime-tests': ['formal_heavy']})
            binary = dict(package='oasis7_chain_runtime', target='oasis7_chain_runtime',
                          target_kind=['lib'], variant='required', ordinary=True,
                          tests=artifacts.inventory(executable), ignored_tests=artifacts.inventory(executable, True),
                          path=executable.name, cwd='.')
            manifest = dict(scope='ordinary', binaries={'runtime-tests': binary}, execution_plan=selected)
            environment = dict(os.environ)
            environment.pop('FAIL_ORDINARY_NEIGHBOR', None)
            def execute(selection, name, fail_neighbor=False):
                manifest['execution_plan'] = selection
                env = dict(environment)
                if fail_neighbor:
                    env['FAIL_ORDINARY_NEIGHBOR'] = '1'
                with patch.object(artifacts, 'ROOT', root), \
                     patch.object(artifacts, 'load', return_value=(manifest, env)), \
                     patch.object(artifacts, 'snapshot_regressions', return_value=()):
                    artifacts.execute(root, 'ordinary', root / (name + '.jsonl'), plan=selection)
            execute(selected, 'fast')
            self.assertTrue((root / 'ordinary-ran').is_file())
            self.assertTrue((root / 'neighbor-ran').is_file())
            self.assertFalse((root / 'heavy-ran').exists())
            receipt = json.loads((root / 'fast.jsonl').read_text())
            self.assertTrue(receipt['passed'])
            self.assertEqual(receipt['expected_count'], 2)
            with self.assertRaises(RuntimeError):
                execute(selected, 'neighbor-failure', fail_neighbor=True)
            full = selected | dict(execution_profile='periodic-full', scope='explicit-full',
                                   groups=['baseline', 'oasis7_required', 'formal_process'],
                                   formal_suites=list(artifacts.SHARDS), formal_shards=list(artifacts.SHARDS),
                                   formal_deferred_ordinary_cases={})
            legacy = {key: value for key, value in selected.items()
                      if key not in ('formal_full_profiles_only_version', 'formal_deferred_ordinary_cases')}
            for name, selection in [('full', full), ('old-base', legacy)]:
                with self.subTest(profile=name), self.assertRaises(RuntimeError):
                    execute(selection, name)
                self.assertTrue((root / 'heavy-ran').is_file())
                receipt = json.loads((root / (name + '.jsonl')).read_text())
                self.assertFalse(receipt['passed'])
                self.assertEqual(receipt['expected_count'], 3)

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
            plan = native_package_partition_selection() | {
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
            plan = fast_selection()
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

    def test_real_scale_risk_build_executes_full_once_and_propagates_failure(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            config, _ = self.prepare_target_workspace(root, '#[test] fn required_lib() {}\n')
            manifest_path = root / 'codec/Cargo.toml'
            manifest_path.write_text(manifest_path.read_text() + 'test_tier_full=["test_tier_required"]\n'
                '\n[[test]]\nname="scale"\npath="tests/scale.rs"\nrequired-features=["test_tier_full"]\n')
            (root / 'codec/tests').mkdir()
            (root / 'codec/tests/required.rs').write_text(
                '#[test] fn required_chunk() { assert_eq!(2 * 65536 + 17, 131089); }\n')
            (root / 'codec/tests/scale.rs').write_text('''
#[cfg(not(feature="test_tier_full"))]
compile_error!("full scale target must not be compiled by the required tuple");
#[test] fn full_scale() {
    std::fs::write(std::env::var("SCALE_WITNESS").unwrap(), "full executed").unwrap();
    assert!(std::env::var_os("FAIL_SCALE").is_none(), "real full scale failure");
}
''')
            subprocess.run(['git', 'add', 'codec'], cwd=root, check=True)
            subprocess.run(['git', '-c', 'user.name=Fixture', '-c', 'user.email=fixture@example.invalid',
                            'commit', '-qm', 'scale contract'], cwd=root, check=True, stdout=subprocess.PIPE)
            sha = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, text=True).strip()
            for risk, expanded, total in ((False, False, 2), (True, False, 3), (True, True, 3)):
                selected = risk_selection() if risk else fast_selection()
                selected.update(source_sha=sha, tested_sha=sha, policy_sha=sha,
                                architecture=dict(source=sha, tested=sha), test_policy_changed=expanded)
                name = f'{risk}-{expanded}'
                witness = root / ('witness-' + name)
                with patch.object(artifacts, 'ROOT', root), patch.object(artifacts, 'CONFIG', config), \
                     patch.dict(os.environ, SOURCE_SHA=sha, SCALE_WITNESS=str(witness)):
                    bundle, consumer = root / ('build-' + name), root / ('consumer-' + name)
                    artifacts.build(bundle, 'ordinary', plan=selected)
                    archive = root / (name + '.tar.gz')
                    artifacts.pack(bundle, archive, suite='ordinary', plan=selected)
                    with tarfile.open(archive) as compressed:
                        compressed.extractall(consumer, filter='data')
                    results = root / (name + '.jsonl')
                    artifacts.execute(consumer, 'ordinary', results, plan=selected)
                    receipts = [json.loads(line) for line in results.read_text().splitlines()]
                    self.assertTrue(all(record['passed'] and record['count_ok'] for record in receipts))
                    self.assertEqual(sum(record['expected_count'] for record in receipts), total)
                    self.assertEqual(witness.exists(), risk)
                    manifest = json.loads((bundle / 'manifest.json').read_text())
                    self.assertEqual(sum('full_scale' in binary.get('tests', [])
                                         for binary in manifest['binaries'].values()), int(risk))
                    original = [binary for binary in manifest['binaries'].values()
                                if binary.get('variant') == 'required' and binary.get('target') == 'scale']
                    self.assertEqual(len(original), int(expanded))
                    if expanded:
                        self.assertEqual(original[0]['tests'], ['full_scale'])
                    self.assertEqual(sum('required_chunk' in binary.get('tests', [])
                                         for binary in manifest['binaries'].values()), 1)
                    if risk and not expanded:
                        with patch.dict(os.environ, FAIL_SCALE='1'), self.assertRaises(RuntimeError):
                            artifacts.execute(consumer, 'ordinary', root / 'failed.jsonl', plan=selected)

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
            selected = fast_target_selection()
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
            selected = inline_selection() | {
                'architecture': dict(source=sha, tested=sha),
                'ordinary_fast_targets': [fast_target(), fast_target(target_kind='lib', target_name='oasis7_viewer_codec')],
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

    def test_ordinary_build_strips_staged_elf_and_keeps_original_target_intact(self):
        if sys.platform != 'linux' or shutil.which('strip') is None:
            return
        source = '''
#[cfg(feature="default")] compile_error!("default feature was incorrectly enabled");
#[cfg(all(test,feature="test_tier_required"))] mod required {
    #[test] fn round_trip() { assert_eq!(1 + 1, 2); }
    #[test] fn newly_added() { assert_eq!(2 + 2, 4); }
}
'''
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            config, sha = self.prepare_target_workspace(root, source, source)
            selected = inline_selection() | {
                'architecture': dict(source=sha, tested=sha),
                'ordinary_fast_targets': [fast_target()],
            }
            with patch.object(artifacts, 'ROOT', root), patch.object(artifacts, 'CONFIG', config), \
                 patch.object(artifacts, 'native_cpu_budget', return_value=2), \
                 patch.dict(os.environ, SOURCE_SHA=sha):
                bundle = root / 'bundle'
                artifacts.build(bundle, 'ordinary', plan=selected)
                manifest = json.loads((bundle / 'manifest.json').read_text())
                entry = next(iter(manifest['binaries'].values()))
                staged = bundle / entry['path']
                original = Path(entry['original_executable'])
                self.assertEqual(entry['sha256'], artifacts.digest(staged))
                self.assertEqual(entry['bytes'], staged.stat().st_size)
                self.assertLess(staged.stat().st_size, original.stat().st_size)
                self.assertNotEqual(artifacts.digest(staged), artifacts.digest(original))
                readelf = shutil.which('readelf')
                if readelf is not None:
                    original_sections = subprocess.check_output([readelf, '-SW', str(original)], text=True)
                    staged_sections = subprocess.check_output([readelf, '-SW', str(staged)], text=True)
                    self.assertIn('.symtab', original_sections)
                    self.assertNotIn('.symtab', staged_sections)
                results = root / 'results.jsonl'
                artifacts.execute(bundle, 'ordinary', results, plan=selected)
                records = [json.loads(line) for line in results.read_text().splitlines()]
                self.assertEqual(len(records), 1)
                self.assertTrue(records[0]['passed'] and records[0]['expected_count'] == 2)

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
            selected = inline_selection() | {
                'architecture': dict(source=sha, tested=sha),
                'ordinary_fast_targets': [fast_target(), fast_target(target_kind='lib', target_name='oasis7_viewer_codec')],
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
            selected = inline_selection() | {
                'architecture': dict(source=sha, tested=sha),
                'ordinary_fast_targets': [fast_target(),
                    fast_target(target_kind='lib', target_name='oasis7_viewer_codec'),
                    fast_target(package='oasis7_runtime', target_kind='lib', target_name='oasis7_runtime')],
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


if __name__ == '__main__':
    unittest.main()
