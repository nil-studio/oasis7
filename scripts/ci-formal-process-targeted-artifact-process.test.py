#!/usr/bin/env python3
"""TargetedArtifactProcess regressions split from ci-formal-process.test.py."""
import copy
from contextlib import ExitStack
from concurrent.futures import ThreadPoolExecutor
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tarfile
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
from ci_process_runner import run


FULL_SYNC_CASES = (
    'formal_full_sync_pins_the_first_verified_target_cut_across_providers',
    'formal_full_sync_rejects_over_budget_material_without_publishing_then_recovers',
    'formal_full_sync_pins_only_objects_the_successful_candidate_persisted',
    'formal_full_sync_shared_budget_accumulates_prior_charges_without_publishing',
)


def fast_execution_selection(packages=('oasis7_viewer_codec',), suites=()):
    return dict(
        fast_plan_version=1,
        execution_profile='pr-fast',
        scope='targeted',
        groups=['baseline', 'oasis7_required'] + (['formal_process'] if suites else []),
        oasis7_required_packages=list(packages),
        oasis7_required_wasm_packages=[p for p in packages if p in artifacts.PORTABLE_REQUIRED_PACKAGES],
        formal_suites=list(suites),
        test_policy_changed=False,
        architecture=dict(source='source', tested='tested'),
    )


def targeted_artifact_selection():
    return fast_execution_selection(packages=('oasis7_chain_runtime',)) | dict(
        ordinary_artifact_partition_version=2,
        ordinary_partitions=['controls', 'native-0', 'native-1', 'native-2', 'native-3'],
    )


def targeted_artifact_costs(rows=()):
    return dict(
        version=1,
        fallback_ms=1000,
        evidence=dict(
            run_id=43179733413,
            run_attempt=1,
            source_sha='718dee8464b5681082d7ab4ea7d267cd838ead8c',
            tested_sha='40c170e7837b1bf0d30ab877a2a826b37555ba71',
            policy_sha='e2dbafdaf507a1f228c09c1d2d49420f278117ae',
        ),
        harnesses=list(rows),
    )


def targeted_artifact_coverage(manifest, entries):
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


def complete_artifact_selection():
    return fast_execution_selection() | dict(
        scope='full',
        ordinary_artifact_partition_version=1,
        ordinary_partitions=['controls', 'runtime', 'component-libs', 'integrations-0', 'integrations-1'],
    )


def complete_artifact_manifest(selected):
    binaries = {}
    for name, package, kind, variant in [
        ('runtime-tests', 'oasis7_chain_runtime', 'lib', 'required'),
        ('oasis7-lib-tests', 'oasis7', 'lib', 'required'),
        ('component', 'oasis7_runtime', 'lib', 'required'),
        ('new-integration', 'oasis7_runtime', 'test', 'required'),
        ('supplement', 'oasis7', 'lib', 'required-full-fast-0'),
        ('empty', 'oasis7', 'test', 'required'),
    ]:
        binaries[name] = dict(
            package=package, target=package, target_kind=[kind],
            variant=variant, ordinary=True, tests=[] if name == 'empty' else ['actual_behavior'],
            ignored_tests=[], path=name, cwd='.',
        )
    for owner, pattern in artifacts.DEFAULT_SNAPSHOT_REGRESSIONS:
        binaries[owner] = dict(
            package='oasis7', target='oasis7', target_kind=['lib'],
            variant=owner, ordinary=False, tests=[pattern + 'regression'],
            ignored_tests=[], path=owner, cwd='.',
        )
    return dict(scope='ordinary', execution_plan=selected, binaries=binaries)


def fake_receipt(suite, name, results, count, **changes):
    result = dict(
        suite=suite, test_name=name, expected_count=count, count_ok=True,
        passed=True, cleanup_result='clean', cancelled=False, timeout=False,
        exit_code=0, leaked_processes=False,
    ) | changes
    results.parent.mkdir(parents=True, exist_ok=True)
    with results.open('a') as stream:
        stream.write(json.dumps(result) + '\n')
    return result


class TargetedArtifactProcess(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.temporary = tempfile.TemporaryDirectory()
        cls.addClassCleanup(cls.temporary.cleanup)
        cls.root = Path(cls.temporary.name).resolve()
        (cls.root / 'Cargo.toml').write_text('[workspace]\nmembers=["runtime","viewer","risk","core"]\nresolver="2"\n')
        (cls.root / '.gitignore').write_text('target/\nbundle/\nconsumer/\n*.tar.gz\n')
        runtime, viewer, risk, core = (cls.root / name for name in ('runtime', 'viewer', 'risk', 'core'))
        for member in (runtime, viewer, risk, core):
            (member / 'src').mkdir(parents=True)
        (runtime / 'Cargo.toml').write_text(
            '[package]\nname="oasis7_chain_runtime"\nversion="0.1.0"\nedition="2021"\n'
            '[features]\ndefault=["wasmtime","node-libp2p"]\nwasmtime=[]\nnode-libp2p=[]\n'
            'test_tier_required=[]\n')
        source = r'''
#[cfg(not(all(feature="wasmtime", feature="node-libp2p", feature="test_tier_required")))]
compile_error!("the original runtime feature combination is required");
pub fn mark(name: &str) {
    let path = std::path::PathBuf::from(std::env::var("FIXTURE_RECORDS").unwrap()).join(name);
    std::fs::OpenOptions::new().create_new(true).write(true).open(path).unwrap();
}
#[test] fn case() { mark("case"); }
#[test] fn case_suffix() { mark("case_suffix"); }
#[test] fn prefix_case() { mark("prefix_case"); }
#[test] fn exclusive_first() { mark("exclusive_first"); }
#[test] fn exclusive_second() { mark("exclusive_second"); }
#[test] fn parent() {
    let result = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "ignored_child", "--ignored"])
        .env("FIXTURE_CHILD", "1").output().unwrap();
    assert!(result.status.success(), "{:?}", result);
    mark("parent");
}
#[test] #[ignore] fn ignored_child() {
    assert_eq!(std::env::var("FIXTURE_CHILD").unwrap(), "1");
    mark("child-called");
}
#[test] fn fault() {
    let mode = std::env::var("FIXTURE_MODE").unwrap_or_default();
    if mode == "panic" { panic!("real shard failure"); }
    if mode == "timeout" || mode == "cancel" {
        mark("fault-started");
        std::thread::sleep(std::time::Duration::from_secs(30));
    }
    if mode == "leak" {
        let child = std::process::Command::new(std::env::var("FIXTURE_PYTHON").unwrap())
            .args(["-c", "import time; time.sleep(30)"])
            .stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null()).spawn().unwrap();
        std::fs::write(std::path::PathBuf::from(std::env::var("FIXTURE_RECORDS").unwrap())
            .join("leaked-pid"), child.id().to_string()).unwrap();
        std::mem::forget(child);
    }
    mark("fault");
}
'''
        source += ''.join(f'#[test] fn generated_{index}() {{ mark("generated_{index}"); }}\n'
                          for index in range(12))
        source += ''.join(f'#[test] fn {name}() {{ mark("{name}"); }}\n'
                          for name in FULL_SYNC_CASES)
        (runtime / 'src/lib.rs').write_text(source)
        (runtime / 'src/main.rs').write_text('fn main() {}\n')
        (runtime / 'tests').mkdir()
        (runtime / 'tests/process.rs').write_text(
            '#[test] fn complete_integration() {\n'
            ' assert!(std::process::Command::new(env!("CARGO_BIN_EXE_oasis7_chain_runtime")).status().unwrap().success());\n'
            ' assert!(std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml")).unwrap().contains("oasis7_chain_runtime"));\n'
            ' oasis7_chain_runtime::mark("complete_integration");\n}\n'
            '#[test] fn new_integration_case() { oasis7_chain_runtime::mark("new_integration_case"); }\n')
        (risk / 'Cargo.toml').write_text(
            '[package]\nname="oasis7_viewer_codec"\nversion="0.1.0"\nedition="2021"\n'
            '[features]\ntest_tier_required=[]\ntest_tier_full=["test_tier_required"]\n'
            '[[test]]\nname="full_scales"\npath="tests/full_scales.rs"\nrequired-features=["test_tier_full"]\n')
        (risk / 'src/lib.rs').write_text(
            'pub fn mark(name: &str) {\n'
            ' let path = std::path::PathBuf::from(std::env::var("FIXTURE_RECORDS").unwrap()).join(name);\n'
            ' std::fs::OpenOptions::new().create_new(true).write(true).open(path).unwrap();\n}\n'
            '#[test] fn risk_required() { assert!(cfg!(feature="test_tier_required")); mark("risk_required"); }\n'
            '#[cfg(feature="test_tier_full")] #[test] fn newly_added_full_case() {\n'
            ' assert_ne!(std::env::var("FIXTURE_MODE").unwrap_or_default(), "policy-panic");\n'
            ' mark("newly_added_full_case");\n}\n')
        (risk / 'tests').mkdir()
        (risk / 'tests/full_scales.rs').write_text(
            '#[cfg(not(feature="test_tier_full"))] compile_error!("full risk tier is required");\n' +
            ''.join(f'#[test] fn full_scale_{index}() {{ oasis7_viewer_codec::mark("full_scale_{index}"); }}\n'
                    for index in range(5)))
        (viewer / 'Cargo.toml').write_text(
            '[package]\nname="oasis7_viewer_runtime"\nversion="0.1.0"\nedition="2021"\n'
            '[features]\ntest_tier_required=[]\n[[bin]]\nname="oasis7_viewer_live"\npath="src/main.rs"\n')
        (viewer / 'src/main.rs').write_text('fn main() {}\n')
        (core / 'Cargo.toml').write_text(
            '[package]\nname="oasis7"\nversion="0.1.0"\nedition="2021"\n'
            '[features]\ntest_tier_required=[]\n')
        marker = (
            'pub fn mark(name: &str) {\n'
            ' let path = std::path::PathBuf::from(std::env::var("FIXTURE_RECORDS").unwrap()).join(name);\n'
            ' std::fs::OpenOptions::new().create_new(true).write(true).open(path).unwrap();\n}\n')
        (core / 'src/lib.rs').write_text(marker +
            '#[test] fn ordinary_core() { mark("ordinary_core"); }\n'
            '#[test] fn snapshot_player_gameplay_execution_state_backfills_from_legacy_fields() {\n'
            ' mark(if cfg!(feature="test_tier_required") { "core_snapshot_required" } else { "core_snapshot_default" });\n}\n')
        (viewer / 'src/lib.rs').write_text(marker +
            '#[test] fn ordinary_viewer() { mark("ordinary_viewer"); }\n'
            '#[cfg(test)] mod snapshot_progress {\n'
            ' #[test] fn restored() { super::mark(if cfg!(feature="test_tier_required")\n'
            ' { "viewer_snapshot_required" } else { "viewer_snapshot_default" }); }\n}\n')
        cls.config = cls.root / 'scripts/ci-formal-process-suites.json'
        cls.config.parent.mkdir()
        cls.config.write_text('{}')
        subprocess.run(['cargo', 'generate-lockfile', '--offline'], cwd=cls.root, check=True,
                       stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        for command in (['git', 'init', '-q'], ['git', 'add', '.'],
                        ['git', '-c', 'user.name=Fixture', '-c', 'user.email=fixture@example.invalid',
                         'commit', '-qm', 'fixture']):
            subprocess.run(command, cwd=cls.root, check=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        cls.sha = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=cls.root, text=True).strip()
        cls.selected = targeted_artifact_selection() | dict(
            source_sha=cls.sha, tested_sha=cls.sha, policy_sha=cls.sha,
            architecture=dict(source=cls.sha, tested=cls.sha),
            ordinary_harness_costs=targeted_artifact_costs([
                dict(package='oasis7_chain_runtime', target_kind='test', target_name='process',
                     variant='required', mode='batch', selector='', elapsed_ms=5000)]))
        cls.image = os.environ.get('ORDINARY_BUILD_IMAGE',
                                   'ghcr.io/nil-studio/oasis7/ci-rust-build@sha256:' + 'a' * 64)
        cls.bundle, cls.consumer = cls.root / 'bundle', cls.root / 'consumer'
        with patch.object(artifacts, 'ROOT', cls.root), patch.object(artifacts, 'CONFIG', cls.config), \
             patch.dict(os.environ, SOURCE_SHA=cls.sha):
            with patch.dict(os.environ, ORDINARY_BUILD_IMAGE=cls.image):
                artifacts.build(cls.bundle, 'ordinary', plan=cls.selected)
            archive = cls.root / 'ordinary.tar.gz'
            artifacts.pack(cls.bundle, archive, suite='ordinary', plan=cls.selected)
        cls.consumer.mkdir()
        with tarfile.open(archive) as compressed:
            compressed.extractall(cls.consumer, filter='data')
        cls.manifest = json.loads((cls.consumer / 'manifest.json').read_text())

    def context(self, records, **environment):
        stack = ExitStack()
        stack.enter_context(patch.object(artifacts, 'ROOT', self.root))
        stack.enter_context(patch.object(artifacts, 'CONFIG', self.config))
        stack.enter_context(patch.object(artifacts, 'ORDINARY_EXCLUSIVE_RUNTIME_CASES',
                                        frozenset(('exclusive_first', 'exclusive_second'))))
        stack.enter_context(patch.dict(os.environ, SOURCE_SHA=self.sha, FIXTURE_RECORDS=str(records),
                                        FIXTURE_PYTHON=sys.executable, **environment))
        return stack

    def test_real_build_pack_load_and_four_owners_equal_the_original_complete_execution(self):
        self.assertEqual(json.loads((self.bundle / 'manifest.json').read_text())['image'], self.image)
        self.assertEqual(json.loads((self.consumer / 'manifest.json').read_text())['image'], self.image)
        self.assertNotIn('ORDINARY_BUILD_IMAGE', os.environ)
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            baseline, partitioned = root / 'baseline', root / 'partitioned'
            baseline.mkdir()
            partitioned.mkdir()
            with self.context(baseline):
                artifacts.execute(self.consumer, 'ordinary', root / 'baseline.jsonl', plan=self.selected)
            tools = root / 'tools'
            tools.mkdir()
            calls = root / 'unexpected-compiler'
            for name in ('cargo', 'rustc', 'rustup'):
                executable = tools / name
                executable.write_text('#!' + sys.executable + '\nfrom pathlib import Path\n'
                                      f'Path({str(calls)!r}).write_text("unexpected compiler")\nraise SystemExit(91)\n')
                executable.chmod(0o755)
            with self.context(partitioned, PATH=str(tools) + os.pathsep + os.environ['PATH']):
                with ThreadPoolExecutor(max_workers=4) as pool:
                    futures = [pool.submit(artifacts.execute, self.consumer, 'ordinary',
                               root / (owner + '.jsonl'), plan=self.selected, ordinary_partition=owner)
                               for owner in self.selected['ordinary_partitions'][1:]]
                    for future in futures:
                        future.result()
            self.assertFalse(calls.exists(), 'compiled consumers cannot require Cargo, rustc or rustup')
            self.assertEqual(sorted(path.name for path in baseline.iterdir()),
                             sorted(path.name for path in partitioned.iterdir()))
            self.assertTrue((partitioned / 'child-called').is_file())
            self.assertTrue(all((partitioned / name).is_file()
                                for name in FULL_SYNC_CASES))
            baseline_records = [json.loads(line) for line in (root / 'baseline.jsonl').read_text().splitlines()]
            records = [json.loads(line) for owner in self.selected['ordinary_partitions'][1:]
                       for line in (root / (owner + '.jsonl')).read_text().splitlines()]
            self.assertEqual(sum(record['expected_count'] for record in records),
                             sum(record['expected_count'] for record in baseline_records))
            self.assertTrue(all(record['passed'] and record['count_ok'] for record in records))

    def test_ordinary_runner_image_identity_remains_exact(self):
        manifest = json.loads((self.consumer / 'manifest.json').read_text())
        with tempfile.TemporaryDirectory() as temporary, self.context(Path(temporary)), \
             patch.dict(os.environ, ImageOS='ubuntu24'):
            generic = dict(manifest, runner_image='linux')
            with self.assertRaisesRegex(ValueError, 'artifact identity mismatch: runner_image'):
                artifacts.validate_artifact_manifest(generic, suite='ordinary', plan=self.selected)
            verified = dict(manifest, runner_image='ubuntu24')
            self.assertEqual(artifacts.validate_artifact_manifest(verified, suite='ordinary', plan=self.selected),
                             set(manifest['included_binaries']))

    def test_real_ordinary_artifact_still_rejects_another_consumer_checkout(self):
        manifest = json.loads((self.bundle / 'manifest.json').read_text())
        with tempfile.TemporaryDirectory() as temporary, self.context(Path(temporary)):
            with patch.object(artifacts, 'ROOT', self.root / 'different-consumer'), \
                 patch.object(artifacts, 'output', return_value=self.sha), \
                 self.assertRaisesRegex(ValueError, 'artifact identity mismatch: checkout'):
                artifacts.validate_artifact_manifest(manifest, suite='ordinary', plan=self.selected)

    def test_real_v1_delivery_preserves_behavior_and_restores_cargo_binary_paths_without_compiling(self):
        selected = self.selected | dict(scope='full', ordinary_artifact_partition_version=1,
            ordinary_artifact_delivery_version=1, ordinary_partitions=list(artifacts.ORDINARY_ARTIFACT_PARTITIONS))
        selected.pop('ordinary_harness_costs')
        self.assert_delivery_roundtrip(selected)

    def test_real_targeted_policy_delivery_keeps_full_cases_risks_and_both_default_snapshots(self):
        for profile in ('pr-fast', 'main-fast'):
            selected = self.selected | dict(execution_profile=profile, test_policy_changed=True,
                ordinary_artifact_partition_version=1, ordinary_artifact_delivery_version=1,
                ordinary_partitions=list(artifacts.ORDINARY_ARTIFACT_PARTITIONS),
                oasis7_required_packages=['oasis7', 'oasis7_chain_runtime', 'oasis7_viewer_codec', 'oasis7_viewer_runtime'],
                oasis7_required_wasm_packages=['oasis7_viewer_codec', 'oasis7_viewer_runtime'],
                risk_test_inventory_version=1,
                ordinary_risk_targets=[dict(package='oasis7_viewer_codec', target_kind='test',
                    target_name='full_scales', default_features=True, features=['test_tier_full'],
                    cases=[f'full_scale_{index}' for index in range(5)])])
            selected.pop('ordinary_harness_costs')
            with self.subTest(profile=profile):
                self.assert_delivery_roundtrip(selected)

    def test_real_full_fast_v2_delivery_preserves_full_risks_exact_deferrals_and_failures(self):
        for profile in ('pr-fast', 'main-fast'):
            selected = self.selected | dict(scope='full', execution_profile=profile,
                ordinary_artifact_delivery_version=2, risk_test_inventory_version=1,
                oasis7_required_packages=['oasis7_chain_runtime', 'oasis7_viewer_codec'],
                ordinary_risk_targets=[dict(package='oasis7_viewer_codec', target_kind='test',
                    target_name='full_scales', default_features=True, features=['test_tier_full'],
                    cases=[f'full_scale_{index}' for index in range(5)])],
                formal_full_profiles_only_version=1, formal_shards=[],
                formal_deferred_ordinary_cases={'runtime-tests': ['case']})
            with self.subTest(profile=profile):
                self.assert_delivery_roundtrip(selected)

    def assert_delivery_roundtrip(self, selected):
        owners = selected['ordinary_partitions'][1:]
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            baseline, records = root / 'baseline', root / 'records'
            baseline.mkdir()
            records.mkdir()
            with self.context(baseline) as context:
                if selected['scope'] in ('full', 'explicit-full'):
                    context.enter_context(patch.object(artifacts, 'required_packages',
                                                      return_value=selected['oasis7_required_packages']))
                bundle, archives = root / 'bundle', root / 'archives'
                artifacts.build(bundle, 'ordinary', plan=selected)
                artifacts.pack_ordinary_delivery(bundle, archives, selected)
                artifacts.execute(bundle, 'ordinary', root / 'baseline.jsonl', plan=selected)
                manifest = json.loads((bundle / 'manifest.json').read_bytes())
                helpers = [Path(entry['original_executable']) for entry in manifest['binaries'].values()
                           if not entry['test']]
                tools = root / 'tools'
                tools.mkdir()
                compiler_called = root / 'unexpected-compiler'
                for name in ('cargo', 'rustc', 'rustup'):
                    executable = tools / name
                    executable.write_text('#!' + sys.executable + '\nfrom pathlib import Path\n'
                        f'Path({str(compiler_called)!r}).touch()\nraise SystemExit(91)\n')
                    executable.chmod(0o755)
                with patch.dict(os.environ, FIXTURE_RECORDS=str(records), PATH=str(tools) + os.pathsep + os.environ['PATH']):
                    for owner in owners:
                        for helper in helpers:
                            helper.unlink(missing_ok=True)
                        destination = root / owner
                        artifacts.unpack_ordinary_delivery(archives / 'common.tar.gz', archives / (owner + '.tar.gz'),
                                                           destination, selected, owner)
                        parent, _ = artifacts.load(destination, 'ordinary', plan=selected, ordinary_artifact_partition=owner)
                        self.assertTrue(all(helper.is_file() for helper in helpers))
                        entries = artifacts.ordinary_artifact_entries(parent, selected, owner)
                        if selected.get('test_policy_changed') is True:
                            self.assertTrue(entries, 'each admitted policy partition must actually execute')
                        if selected['ordinary_artifact_partition_version'] == 2:
                            self.assertTrue(entries)
                        elif entries:
                            artifacts.execute(destination, 'ordinary', root / (owner + '.jsonl'),
                                              plan=selected, ordinary_partition=owner)
                    if selected['ordinary_artifact_partition_version'] == 2:
                        with ThreadPoolExecutor(max_workers=4) as pool:
                            futures = [pool.submit(artifacts.execute, root / owner, 'ordinary',
                                       root / (owner + '.jsonl'), plan=selected, ordinary_partition=owner)
                                       for owner in owners]
                            for future in futures:
                                future.result()
                        self.assertFalse((records / 'case').exists())
                        self.assertTrue(all((records / name).is_file() for name in
                            ('case_suffix', 'prefix_case', 'exclusive_first', 'exclusive_second', 'child-called',
                             *(f'full_scale_{index}' for index in range(5)))))
                        risk = [entry for entry in manifest['binaries'].values()
                                if entry.get('variant', '').startswith('required-risk-')]
                        self.assertEqual(len(risk), 1)
                        self.assertEqual(risk[0]['requested_features'], ['test_tier_full'])
                        self.assertEqual(len(risk[0]['tests']), 5)
                        self.assertNotIn('test_tier_full', manifest['binaries']['runtime-tests']['features'])
                        self.assertTrue((records / 'risk_required').is_file())
                        if selected['execution_profile'] == 'pr-fast':
                            self.assert_failed_owners({owner: root / owner for owner in owners}, selected, manifest)
                    else:
                        if selected.get('test_policy_changed') is True:
                            self.assertTrue(all((records / name).is_file() for name in (
                                'ordinary_core', 'ordinary_viewer', 'core_snapshot_required', 'core_snapshot_default',
                                'viewer_snapshot_required', 'viewer_snapshot_default', 'newly_added_full_case',
                                *(f'full_scale_{index}' for index in range(5)))))
                            risk = [entry for entry in manifest['binaries'].values()
                                    if entry.get('package') == 'oasis7_viewer_codec' and entry.get('target') == 'full_scales']
                            self.assertEqual(len(risk), 1)
                            self.assertEqual(risk[0]['variant'], 'required')
                            self.assertIn('test_tier_full', risk[0]['features'])
                            self.assertFalse(any(entry.get('variant', '').startswith('required-risk-')
                                                 for entry in manifest['binaries'].values()))
                            full_failure = root / 'full-failure'
                            full_failure.mkdir()
                            with patch.dict(os.environ, FIXTURE_MODE='policy-panic', FIXTURE_RECORDS=str(full_failure)), \
                                 self.assertRaises(RuntimeError):
                                artifacts.execute(root / 'component-libs', 'ordinary', root / 'full-failure.jsonl',
                                                  plan=selected, ordinary_partition='component-libs')
                            failures = [json.loads(line) for line in (root / 'full-failure.jsonl').read_text().splitlines()]
                            self.assertTrue(any(not row['passed'] for row in failures))
                        fault_records = root / 'fault'
                        fault_records.mkdir()
                        with patch.dict(os.environ, FIXTURE_MODE='panic', FIXTURE_RECORDS=str(fault_records)), self.assertRaises(RuntimeError):
                            artifacts.execute(root / 'runtime', 'ordinary', root / 'failure.jsonl',
                                              plan=selected, ordinary_partition='runtime')
                self.assertFalse(compiler_called.exists())
                self.assertEqual(sorted(path.name for path in baseline.iterdir()), sorted(path.name for path in records.iterdir()))
                expected = [json.loads(line) for line in (root / 'baseline.jsonl').read_text().splitlines()]
                actual = [json.loads(line) for owner in owners
                          if (root / (owner + '.jsonl')).exists()
                          for line in (root / (owner + '.jsonl')).read_text().splitlines()]
                self.assertEqual(sum(row['expected_count'] for row in actual), sum(row['expected_count'] for row in expected))
                self.assertTrue(all(row['passed'] and row['count_ok'] and row['cleanup_result'] == 'clean' for row in actual))

    def test_real_panic_partial_count_timeout_cancel_and_leaked_child_fail_the_owner(self):
        self.assert_failed_owners(self.consumer, self.selected, self.manifest)

    def assert_failed_owners(self, bundles, selected, manifest):
        for mode in ('panic', 'count', 'timeout', 'cancel', 'leak'):
            with self.subTest(mode=mode), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                records = root / 'records'
                records.mkdir()
                receipts = root / 'receipts.jsonl'
                with self.context(records, FIXTURE_MODE=mode):
                    owner = next(owner for owner in selected['ordinary_partitions'][1:] if
                        ('runtime-tests', 'fault') in targeted_artifact_coverage(manifest,
                         artifacts.ordinary_artifact_entries(manifest, selected, owner)))
                    def invoke(command, cwd, env, suite, name, results, expected_count=None, **kwargs):
                        if mode == 'count' and name == 'runtime-tests':
                            command = command + ['--skip', 'fault']
                        stop, waiter = threading.Event(), None
                        if mode == 'cancel' and name == 'runtime-tests':
                            def cancel_after_start():
                                deadline = time.monotonic() + 5
                                while not (records / 'fault-started').exists() and time.monotonic() < deadline:
                                    time.sleep(0.01)
                                stop.set()
                            waiter = threading.Thread(target=cancel_after_start)
                            waiter.start()
                        try:
                            return run(command, cwd, env, suite, name, results, expected_count=expected_count,
                                       timeout_seconds=0.3 if mode == 'timeout' else 6,
                                       cancellation_event=stop)
                        finally:
                            if waiter:
                                waiter.join(timeout=6)
                    with patch('ci_process_runner.run', side_effect=invoke):
                        with self.assertRaises(RuntimeError):
                            artifacts.execute(bundles[owner] if isinstance(bundles, dict) else bundles,
                                              'ordinary', receipts, plan=selected, ordinary_partition=owner)
                failed = next(record for record in map(json.loads, receipts.read_text().splitlines())
                              if record['test_name'] == 'runtime-tests')
                self.assertFalse(failed['passed'])
                if mode == 'panic':
                    self.assertNotEqual(failed['exit_code'], 0)
                elif mode == 'count':
                    self.assertEqual(failed['exit_code'], 0)
                    self.assertFalse(failed['count_ok'])
                else:
                    self.assertTrue(failed[{'timeout': 'timeout', 'cancel': 'cancelled',
                                            'leak': 'leaked_processes'}[mode]])
                    self.assertEqual(failed['cleanup_result'], 'clean')

    def test_actual_downloaded_inventory_and_source_tested_policy_identity_remain_required(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            with self.context(root):
                for field in ('source_sha', 'tested_sha', 'policy_sha'):
                    with self.subTest(field=field), self.assertRaisesRegex(ValueError, 'identity|execution plan'):
                        artifacts.load(self.consumer, 'ordinary', plan=self.selected | {field: 'f' * 40})
                damaged = root / 'damaged'
                shutil.copytree(self.consumer, damaged)
                manifest = copy.deepcopy(self.manifest)
                manifest['binaries']['runtime-tests']['tests'].remove('case_suffix')
                (damaged / 'manifest.json').write_text(json.dumps(manifest))
                with self.assertRaisesRegex(ValueError, 'downloaded test inventory'):
                    artifacts.execute(damaged, 'ordinary', root / 'never-created', plan=self.selected,
                                      ordinary_partition='native-0')
                self.assertFalse((root / 'never-created').exists())

    def test_native_artifact_partition_runs_complete_harnesses_with_bounded_concurrency(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            with self.context(root):
                entered = threading.Barrier(2)
                lock = threading.Lock()
                calls, active = [], [0, 0]

                def invoke(command, cwd, env, suite, name, results, expected_count=None, **kwargs):
                    with lock:
                        active[0] += 1
                        active[1] = max(active)
                        calls.append((name, env.get('RUST_TEST_THREADS')))
                    try:
                        if active[1] <= 2 and len(calls) <= 2:
                            entered.wait(timeout=2)
                        return fake_receipt(suite, name, results, expected_count)
                    finally:
                        with lock:
                            active[0] -= 1

                receipts = root / 'results.jsonl'
                with patch.object(artifacts, 'native_cpu_budget', return_value=4), \
                        patch('ci_process_runner.run', side_effect=invoke):
                    artifacts.execute(self.consumer, 'ordinary', receipts, plan=self.selected,
                                      ordinary_partition='native-0')
                records = [json.loads(line) for line in receipts.read_text().splitlines()]
                self.assertEqual(active[1], 2)
                self.assertTrue(all(threads == '2' for _, threads in calls))
                self.assertEqual(len(records), len(calls))
                self.assertTrue(all(record['passed'] for record in records))

    def test_v1_component_libs_partition_runs_largest_first_with_bounded_concurrency(self):
        selected = complete_artifact_selection()
        manifest = complete_artifact_manifest(selected)
        manifest['binaries'].update({
            'alpha-small-lib': dict(
                package='oasis7_cas', target='oasis7_cas', target_kind=['lib'],
                variant='required', ordinary=True, path='alpha-small-lib', cwd='.',
                tests=['s_0'], ignored_tests=[],
            ),
            'omega-large-lib': dict(
                package='oasis7_world_state', target='oasis7_world_state', target_kind=['lib'],
                variant='required', ordinary=True, path='omega-large-lib', cwd='.',
                tests=[f'w_{index}' for index in range(20)], ignored_tests=[],
            ),
            'zeta-medium-lib': dict(
                package='oasis7_runtime', target='oasis7_runtime', target_kind=['lib'],
                variant='required', ordinary=True, path='zeta-medium-lib', cwd='.',
                tests=[f'r_{index}' for index in range(15)], ignored_tests=[],
            ),
        })
        entered = threading.Barrier(2)
        lock = threading.Lock()
        calls, active = [], [0, 0]

        def invoke(command, cwd, env, suite, name, results, expected_count=None, **kwargs):
            with lock:
                active[0] += 1
                active[1] = max(active)
                calls.append((name, expected_count, env.get('RUST_TEST_THREADS')))
            try:
                if len(calls) <= 2:
                    entered.wait(timeout=2)
                return fake_receipt(suite, name, results, expected_count)
            finally:
                with lock:
                    active[0] -= 1

        with tempfile.TemporaryDirectory() as temporary, \
                patch.object(artifacts, 'load', return_value=(manifest, {})), \
                patch.object(artifacts, 'native_cpu_budget', return_value=4), \
                patch('ci_process_runner.run', side_effect=invoke):
            receipts = Path(temporary) / 'results.jsonl'
            artifacts.execute(Path(temporary), 'ordinary', receipts, plan=selected,
                              ordinary_partition='component-libs')
            records = [json.loads(line) for line in receipts.read_text().splitlines()]
        self.assertEqual(active[1], 2)
        self.assertEqual({name for name, _, _ in calls[:2]}, {'omega-large-lib', 'zeta-medium-lib'})
        self.assertTrue(all(threads == '2' for _, _, threads in calls))
        self.assertEqual(len(records), len(calls))


if __name__ == '__main__':
    unittest.main()
