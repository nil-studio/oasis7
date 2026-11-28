#!/usr/bin/env python3
"""Process part-2 timeout, cancellation, proc scan, consumer, and bundle regressions split from ci-formal-process.test.py."""
from concurrent.futures import ThreadPoolExecutor
import copy
import importlib.util
import json
import os
from pathlib import Path
import re
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
from ci_process_runner import owned_processes, run


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


def complete_artifact_partition_selection(packages=('oasis7_viewer_codec',), suites=()):
    return dict(
        fast_plan_version=1,
        execution_profile='pr-fast',
        scope='full',
        groups=['baseline', 'oasis7_required'] + (['formal_process'] if suites else []),
        oasis7_required_packages=list(packages),
        oasis7_required_wasm_packages=[p for p in packages if p in artifacts.PORTABLE_REQUIRED_PACKAGES],
        formal_suites=list(suites),
        test_policy_changed=False,
        architecture=dict(source='source', tested='tested'),
        ordinary_artifact_partition_version=1,
        ordinary_partitions=['controls', 'runtime', 'component-libs', 'integrations-0', 'integrations-1'],
    )


class ProcessPart2(unittest.TestCase):
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
            compile_rust_fixture(source, original)
            harness = root / 'harness'
            source.write_text('#[test] fn calls_tool() { assert!(std::process::Command::new(env!("CARGO_BIN_EXE_tool")).status().unwrap().success()); }')
            compile_rust_fixture(source, harness, test=True,
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
                compile_rust_fixture(source, destination, test=True)
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
            compile_rust_fixture(source, original)
            bundle = root / 'bundle'
            bundle.mkdir()
            source.write_text('#[test] #[ignore] fn formal_acceptance() { assert!(std::process::Command::new(env!("CARGO_BIN_EXE_tool")).status().unwrap().success()); }\n#[test] fn ordinary() {}\n#[test] #[ignore] fn crash_child() {}\n')
            compile_rust_fixture(source, bundle / 'runtime-tests', test=True,
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

    def test_coalesce_build_invocations_merges_only_both_default_snapshot_libs(self):
        variants = [*artifacts.build_variants('ordinary'), ('snapshot-tests', ['test', '-p', 'oasis7', '--lib', '--no-run'], [])]
        invocations = artifacts.coalesce_build_invocations(variants)
        self.assertEqual([name for name, _, _, _ in invocations], ['viewer', 'required', 'snapshot-tests'])
        _, args, features, pkg_map = invocations[2]
        self.assertEqual(args, ['test', '-p', 'oasis7', '-p', 'oasis7_viewer_runtime', '--lib', '--no-run'])
        self.assertEqual(features, [])
        self.assertEqual(pkg_map, {'oasis7': 'snapshot-tests', 'oasis7_viewer_runtime': 'viewer-snapshot-tests'})
        single = [v for v in variants if v[0] != 'viewer-snapshot-tests']
        self.assertEqual(
            artifacts.coalesce_build_invocations(single),
            [(name, a, f, None) for name, a, f in single],
        )

    def test_snapshot_profiles_reject_leaked_test_tier_features(self):
        for logical in ('snapshot-tests', 'viewer-snapshot-tests'):
            artifacts.validate_profile(logical, {'features': ['default']}, 'ordinary')
            for leaked in ('test_tier_required', 'test_tier_full'):
                with self.subTest(logical=logical, leaked=leaked):
                    with self.assertRaisesRegex(ValueError, 'default-only features'):
                        artifacts.validate_profile(logical, {'features': ['default', leaked]}, 'ordinary')

    def test_ordinary_controls_in_rust_baseline_omits_controls_partition_and_runs_clippy_and_wasm_in_baseline(self):
        v1_plan = dict(
            fast_plan_version=1,
            execution_profile='periodic-full',
            scope='explicit-full',
            groups=['baseline', 'rust_baseline', 'oasis7_required'],
            test_policy_changed=False,
            ordinary_artifact_partition_version=1,
            ordinary_artifact_delivery_version=1,
            ordinary_controls_in_rust_baseline_version=1,
            ordinary_clippy_group='rust_baseline',
            ordinary_wasm_group='rust_baseline',
            ordinary_partitions=['runtime', 'component-libs', 'integrations-0', 'integrations-1'],
            oasis7_required_packages=['oasis7', 'oasis7_chain_runtime', 'oasis7_viewer_runtime'],
            oasis7_required_wasm_packages=['oasis7_agent_api', 'oasis7_simulator'],
        )
        self.assertEqual(artifacts.ordinary_partitions(v1_plan),
                         ['runtime', 'component-libs', 'integrations-0', 'integrations-1'])
        self.assertEqual(artifacts.native_ordinary_partitions(v1_plan),
                         ['runtime', 'component-libs', 'integrations-0', 'integrations-1'])
        self.assertEqual(artifacts.required_arguments(v1_plan, 'clippy-group'), ['rust_baseline'])
        self.assertEqual(artifacts.required_arguments(v1_plan, 'wasm-group'), ['rust_baseline'])

        v2_plan = dict(
            fast_plan_version=1,
            execution_profile='pr-fast',
            scope='targeted',
            groups=['baseline', 'rust_baseline', 'oasis7_required'],
            test_policy_changed=False,
            ordinary_artifact_partition_version=2,
            ordinary_artifact_delivery_version=2,
            ordinary_controls_in_rust_baseline_version=1,
            ordinary_clippy_group='rust_baseline',
            ordinary_wasm_group='rust_baseline',
            ordinary_partitions=['native-0', 'native-1', 'native-2', 'native-3'],
            oasis7_required_packages=['oasis7_chain_runtime'],
            oasis7_required_wasm_packages=['oasis7_simulator'],
        )
        self.assertEqual(artifacts.ordinary_partitions(v2_plan),
                         ['native-0', 'native-1', 'native-2', 'native-3'])
        self.assertEqual(artifacts.native_ordinary_partitions(v2_plan),
                         ['native-0', 'native-1', 'native-2', 'native-3'])
        self.assertEqual(artifacts.required_arguments(v2_plan, 'clippy-group'), ['rust_baseline'])
        self.assertEqual(artifacts.required_arguments(v2_plan, 'wasm-group'), ['rust_baseline'])

        # Reject retaining controls or malformed handoff fields fail-closed
        for bad in (
            v2_plan | {'ordinary_partitions': ['controls', 'native-0', 'native-1', 'native-2', 'native-3']},
            v2_plan | {'ordinary_clippy_group': 'oasis7_required'},
            v2_plan | {'ordinary_wasm_group': 'oasis7_required'},
            v2_plan | {'groups': ['baseline', 'oasis7_required']},
            v2_plan | {'ordinary_controls_in_rust_baseline_version': 99},
            v2_plan | {'ordinary_controls_in_rust_baseline_version': True},
            {k: v for k, v in v2_plan.items() if k != 'ordinary_controls_in_rust_baseline_version'},
        ):
            with self.subTest(bad=bad), self.assertRaises(ValueError):
                artifacts.fast_plan(bad)

        # Verify shell driver executes Clippy + WASM in rust_baseline and skips WASM in oasis7_required
        driver = (HERE / 'ci-tests.sh').read_text()
        bodies = dict(re.findall(r'^(\w+)\(\) \{\n(.*?)^\}', driver, re.M | re.S))
        sha = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=artifacts.ROOT, text=True).strip()
        plan_file = self.root / 'baseline-controls-plan.json'
        plan_file.write_text(json.dumps(v2_plan | {'source_sha': sha, 'tested_sha': sha}))
        bundle_dir = self.root / 'dummy-bundle'
        bundle_dir.mkdir(exist_ok=True)
        script = (
            'set -euo pipefail\n'
            'tier=required; ci_python="$1"; driver_dir="$2"; repo_root="$3"; required_plan="$4"\n'
            'ordinary_partition="native-0"; export OASIS7_CI_TEST_BUNDLE="$5"\n'
            'run_oasis7_required_tier_clippy() { echo "CLIPPY_EXECUTED"; }\n'
            'run_cargo() { printf "CARGO:%s\\n" "$*"; }\n'
            'run() { printf "RUN:%s\\n" "$*"; }\n'
            'run_oasis7_required_clippy_for_group() {\n' + bodies['run_oasis7_required_clippy_for_group'] + '}\n'
            'run_oasis7_required_tier_tests() {\n' + bodies['run_oasis7_required_tier_tests'] + '}\n'
            'echo "---BASELINE---"\n'
            'run_oasis7_required_clippy_for_group rust_baseline\n'
            'echo "---ORDINARY---"\n'
            'run_oasis7_required_clippy_for_group oasis7_required\n'
            'run_oasis7_required_tier_tests\n'
        )
        res = subprocess.run(
            ['bash', '-c', script, 'fixture', sys.executable, str(HERE), str(artifacts.ROOT),
             str(plan_file), str(bundle_dir)],
            text=True, capture_output=True, env=dict(os.environ, SOURCE_SHA=sha),
        )
        self.assertEqual(res.returncode, 0, res.stderr)
        baseline_out, ordinary_out = res.stdout.split('---ORDINARY---\n', 1)
        self.assertIn('CLIPPY_EXECUTED', baseline_out)
        self.assertIn('CARGO:check -p oasis7_simulator --lib --target wasm32-unknown-unknown --locked', baseline_out)
        self.assertNotIn('CLIPPY_EXECUTED', ordinary_out)
        self.assertNotIn('CARGO:check', ordinary_out)

    def test_parallel_ordinary_bundle_consumer_uses_bounded_pool_and_honors_local_check_serial(self):
        selected = complete_artifact_partition_selection()
        selected.pop('ordinary_artifact_partition_version', None)
        selected.pop('ordinary_partitions', None)
        manifest = dict(
            execution_plan=selected,
            binaries={
                'short-test': dict(path='bin/short', cwd='crates/oasis7', package='oasis7', target='short',
                                   target_kind=['test'], variant='required', ordinary=True,
                                   tests=['a', 'b', 'c'], ignored_tests=[]),
                'heavy-test': dict(path='bin/heavy', cwd='crates/oasis7_runtime', package='oasis7_runtime',
                                   target='incremental_full_scales', target_kind=['test'],
                                   variant='required-risk-0', ordinary=True, tests=['x'], ignored_tests=[]),
            },
        )
        with tempfile.TemporaryDirectory() as temporary:
            bundle = Path(temporary)
            results = bundle / 'results.jsonl'
            with patch.object(artifacts, 'load', return_value=(manifest, {})), \
                 patch.object(artifacts, 'snapshot_regressions', return_value=()), \
                 patch.object(artifacts, 'execute_inline_jobs') as pool, \
                 patch.dict(os.environ, {}, clear=False):
                os.environ.pop('OASIS7_LOCAL_CHECK', None)
                artifacts.execute(bundle, 'ordinary', results, plan=selected, parallel_ordinary=True)
                pool.assert_called_once()
                self.assertTrue(pool.call_args.kwargs['artifact_partition'])
                # Heavy calibrated cost (incremental_full_scales) starts before higher-case-count short target
                self.assertEqual([job['name'] for job in pool.call_args.args[0]], ['heavy-test', 'short-test'])
            with patch.object(artifacts, 'load', return_value=(manifest, {})), \
                 patch.object(artifacts, 'snapshot_regressions', return_value=()), \
                 patch.object(artifacts, 'execute_inline_jobs') as pool, \
                 patch('ci_process_runner.run', return_value=dict(passed=True, cleanup_result='clean')) as serial, \
                 patch.dict(os.environ, OASIS7_LOCAL_CHECK='1'):
                artifacts.execute(bundle, 'ordinary', results, plan=selected, parallel_ordinary=True)
                pool.assert_not_called()
                self.assertEqual(serial.call_count, 2)
        for bad_args in (['build', '--bundle', 'b', '--parallel-ordinary'],
                         ['run', '--bundle', 'b', '--shard', 'storage-recovery', '--parallel-ordinary']):
            proc = subprocess.run([sys.executable, str(HERE / 'ci-test-artifacts.py'), *bad_args],
                                  text=True, capture_output=True)
            self.assertNotEqual(proc.returncode, 0)


if __name__ == '__main__':
    unittest.main()
