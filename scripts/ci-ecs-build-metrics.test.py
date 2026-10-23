#!/usr/bin/env python3
"""Contract tests plus a real, offline Rust workspace measurement fixture."""
import importlib.util
import json
import os
from pathlib import Path
import platform
import shutil
import signal
import subprocess
import sys
import tempfile
import time
import unittest
from unittest import mock

sys.dont_write_bytecode = True
SCRIPT = Path(__file__).with_name('ci-ecs-build-metrics.py')
SPEC = importlib.util.spec_from_file_location('ecs_build_metrics', SCRIPT)
METRICS = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(METRICS)
RUST_AVAILABLE = bool(shutil.which('cargo') and shutil.which('rustc')
                      and sys.platform == 'linux' and platform.machine() == 'x86_64')


class Contracts(unittest.TestCase):
    def test_external_outputs_reject_checkout_and_ancestors(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            repo = root / 'repo'
            repo.mkdir()
            for path in (repo, repo / 'target' / 'report', root):
                with self.assertRaises(METRICS.MeasurementError):
                    METRICS.external(path, [repo])
            self.assertEqual(METRICS.external(root, [repo], parent=True), root)
            self.assertEqual(METRICS.external(root / 'output', [repo]), root / 'output')
            (root / 'alias').symlink_to(repo, target_is_directory=True)
            with self.assertRaises(METRICS.MeasurementError):
                METRICS.external(root / 'alias' / 'report', [repo])

    def test_normalization_disables_wrappers_incremental_and_profile_overrides(self):
        overrides = {'RUSTC_WRAPPER': 'sccache', 'RUSTC_WORKSPACE_WRAPPER': 'wrapper',
                     'CARGO_BUILD_RUSTC_WRAPPER': 'other-wrapper',
                     'CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER': 'another-wrapper',
                     'CARGO_BUILD_TARGET': 'wasm32-unknown-unknown',
                     'CARGO_PROFILE_ECS_OPT_LEVEL': '0', 'CARGO_PROFILE_RELEASE_LTO': 'true',
                     'CARGO_INCREMENTAL': '1', 'CARGO_BUILD_JOBS': '99'}
        with mock.patch.dict(os.environ, overrides):
            env = METRICS.normalized_env(Path('/external/target'), 1)
        for name in overrides.keys() - {'CARGO_INCREMENTAL', 'CARGO_BUILD_JOBS'}:
            self.assertNotIn(name, env)
        self.assertEqual(env['CARGO_INCREMENTAL'], '0')
        self.assertEqual(env['CARGO_BUILD_JOBS'], '1')
        self.assertEqual(env['CARGO_TARGET_DIR'], '/external/target')

    def test_cargo_exit_success_alone_is_not_measurement_success(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            log = root / 'cargo.jsonl'
            log.write_text('{"reason":"build-finished","success":true}\n')
            with self.assertRaisesRegex(METRICS.MeasurementError, 'all three actual native binaries'):
                METRICS.read_artifacts(log, {'checkout': root, 'target': root, 'packages': {}})

    def test_fresh_must_be_boolean_and_stdout_must_be_json(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            log = root / 'cargo.jsonl'
            fixture = {'checkout': root, 'target': root, 'packages': {}}
            for text, message in (('{"reason":"compiler-artifact","fresh":"true"}\n', 'boolean'),
                                  ('build succeeded\n', 'non-JSON')):
                log.write_text(text)
                with self.assertRaisesRegex(METRICS.MeasurementError, message):
                    METRICS.read_artifacts(log, fixture)

    def test_reverse_pair_is_selected_only_for_three_sample_range_overlap(self):
        def record(values):
            return {'phases': {('viewer_touch' if i == 0 else f'viewer_touch_{i+1}'):
                               {'wall_seconds': value} for i, value in enumerate(values)}}
        for current, baseline, expected in (
                ([100, 101, 102], [101, 102, 103], True),
                ([90, 91, 92], [100, 101, 102], False),
                ([100], [100], False),
                ([100, 101], [100, 101], False)):
            self.assertEqual(METRICS.viewer_order_control_needed(
                {'current': record(current), 'baseline': record(baseline)}), expected)
        # An order control remains separate from the original three samples.
        repeated = record([100, 101, 102])
        repeated['phases']['viewer_touch_order_control'] = {'wall_seconds': 999}
        self.assertEqual(METRICS.viewer_touch_samples(repeated), [100, 101, 102])


@unittest.skipUnless(RUST_AVAILABLE, 'x86_64 Linux Cargo/rustc required for offline Rust integration fixture')
class RustFixture(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix='oasis7-ecs-metrics-test-')
        self.root = Path(self.temporary.name).resolve()
        self.repo = self.root / 'repo'
        self.repo.mkdir()
        self.out = self.root / 'report'
        self.work = self.root / 'measurement-work'
        self.env = METRICS.normalized_env(self.root / 'setup-target', 1)
        self.git('init', '-q')
        self.git('config', 'user.name', 'ECS measurement fixture')
        self.git('config', 'user.email', 'ecs-measurement-fixture@example.invalid')
        self.git('config', 'commit.gpgsign', 'false')
        self.write('.gitignore', 'generated-unexpected.rs\n')
        self.write('Cargo.toml', '''[workspace]
members = ["crates/*"]
resolver = "2"

[profile.ecs]
inherits = "release"
opt-level = 3
strip = "symbols"
lto = false
codegen-units = 16
''')
        self.package('oasis7_world_state', [], 'pub fn value() -> u32 { 1 }\n')
        self.package('oasis7_world_projection', ['oasis7_world_state'],
                     'pub fn value() -> u32 { oasis7_world_state::value() }\n')
        self.package('oasis7_runtime', ['oasis7_world_state'],
                     'pub fn value() -> u32 { oasis7_world_state::value() + 1 }\n')
        self.package('oasis7_world_service', ['oasis7_world_projection', 'oasis7_runtime'],
                     'pub fn value() -> u32 { oasis7_world_projection::value() + oasis7_runtime::value() }\n')
        self.package('oasis7_game_client', [], 'pub fn value() -> u32 { 2 }\n')
        self.package('oasis7_chain_runtime', ['oasis7_world_service'], None,
                     'fn main() { println!("{}", oasis7_world_service::value()); }\n')
        self.package('oasis7_launcher_backend', ['oasis7_game_client'], None,
                     'fn main() { println!("{}", oasis7_game_client::value()); }\n')
        self.package('oasis7_viewer_runtime', ['oasis7_world_service', 'oasis7_game_client'],
                     '#[path = "viewer/runtime_live/world_view_source.rs"]\n'
                     'mod world_view_source;\npub fn value() -> u32 { world_view_source::value() }\n',
                     'fn main() { println!("{}", oasis7_viewer_runtime::value()); }\n')
        self.write(METRICS.VIEWER_PATH,
                   'pub fn value() -> u32 { oasis7_world_service::value() + oasis7_game_client::value() }\n')
        subprocess.run(['cargo', 'generate-lockfile', '--offline'], cwd=self.repo,
                       env=self.env, check=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        self.baseline = self.commit('fixture baseline')
        self.write('crates/oasis7_world_state/src/lib.rs', 'pub fn value() -> u32 { 3 }\n')
        self.current = self.commit('fixture current')

    def test_codec_is_fresh_for_orchestration_and_invalidated_for_own_input(self):
        self.package('oasis7_viewer_codec', ['oasis7_world_service'],
                     'pub fn value() -> u32 { oasis7_world_service::value() }\n')
        manifest = self.repo / 'crates/oasis7_viewer_runtime/Cargo.toml'
        manifest.write_text(manifest.read_text().replace(
            '[dependencies]\n',
            '[dependencies]\noasis7_viewer_codec = { path = "../oasis7_viewer_codec" }\n'))
        self.write(METRICS.VIEWER_PATH,
                   'pub fn value() -> u32 { oasis7_viewer_codec::value() }\n')
        subprocess.run(['cargo', 'generate-lockfile', '--offline'], cwd=self.repo,
                       env=self.env, check=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        self.current = self.commit('extract codec')
        self.run_measurement(samples=3)
        report = json.loads((self.out / 'report.json').read_text())
        current = report['results']['current']['phases']
        self.assertTrue(current['viewer_touch']['lower_libraries']['oasis7_viewer_codec']['all_fresh'])
        self.assertGreater(current['codec_touch']['local_crates']['oasis7_viewer_codec']['rebuilt'], 0)
        self.assertGreater(current['codec_touch']['local_crates']['oasis7_viewer_runtime']['rebuilt'], 0)
        for label in ('current', 'baseline'):
            record = report['results'][label]
            order = [record['phases'][name]['wall_seconds']
                     for name in ('viewer_touch', 'viewer_touch_2', 'viewer_touch_3')]
            summary = record['viewer_touch_summary']
            self.assertEqual(summary['samples'], 3)
            self.assertEqual(summary['execution_order_seconds'], order)
            self.assertEqual(summary['median_seconds'], sorted(order)[1])
            self.assertEqual(summary['min_seconds'], min(order))
            self.assertEqual(summary['max_seconds'], max(order))
        for name in ('viewer_touch_2', 'viewer_touch_3'):
            self.assertTrue(current[name]['lower_libraries']['oasis7_viewer_codec']['all_fresh'])
        self.assert_isolated_cleanup()

    def test_runtime_event_codec_owner_invalidates_real_consumers(self):
        self.package('oasis7_viewer_codec', ['oasis7_world_service'],
                     'pub fn value() -> u32 { oasis7_world_service::value() }\n')
        manifest = self.repo / 'crates/oasis7_viewer_runtime/Cargo.toml'
        manifest.write_text(manifest.read_text().replace(
            '[dependencies]\n',
            '[dependencies]\noasis7_viewer_codec = { path = "../oasis7_viewer_codec" }\n'))
        # Match production: the isolated Launcher/GameClient do not consume WorldState.
        self.write('crates/oasis7_world_state/src/lib.rs',
                   'mod codec; pub fn value() -> u32 { codec::value() }\n')
        self.write(METRICS.RUNTIME_EVENT_CODEC_PATH, 'pub fn value() -> u32 { 3 }\n')
        subprocess.run(['cargo', 'generate-lockfile', '--offline'], cwd=self.repo,
                       env=self.env, check=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        self.current = self.commit('reuse Runtime Event codec owner')
        self.run_measurement(event_codec=True, baseline=self.current)
        report = json.loads((self.out / 'report.json').read_text())
        for record in report['results'].values():
            self.assertTrue(record['source_restored'])
            self.assertEqual(record['runtime_event_codec_mutation']['path'],
                             METRICS.RUNTIME_EVENT_CODEC_PATH)
            self.assertTrue(record['phases']['viewer_touch']['lower_libraries']
                            ['oasis7_world_state']['all_fresh'])
            touched = record['phases']['runtime_event_codec_touch']
            for name in ('oasis7_world_state', 'oasis7_runtime',
                         'oasis7_viewer_codec', 'oasis7_viewer_runtime'):
                self.assertGreater(touched['local_crates'][name]['rebuilt'], 0)
            for name in ('oasis7_chain_runtime', 'oasis7_viewer_live'):
                self.assertFalse(touched['binaries'][name]['fresh'])
            self.assertTrue(touched['binaries']['oasis7_game_launcher']['fresh'])
            self.assertIn('codec_touch', record['phases'])
            for phase in ('seed', 'viewer_touch'):
                abi = record['phases'][phase]['runtime_event_codec_abi']
                self.assertIn('nm', abi['tool'])
                for name in ('oasis7_world_state', 'oasis7_viewer_runtime'):
                    self.assertEqual(len(abi['libraries'][name]['sha256']), 64)
        self.assert_isolated_cleanup()

    def tearDown(self):
        self.temporary.cleanup()

    def git(self, *args):
        return subprocess.check_output(['git', *args], cwd=self.repo, text=True).strip()

    def write(self, path, content):
        destination = self.repo / path
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_text(content)

    def package(self, name, dependencies, library, binary=None):
        manifest = '[package]\n' + f'name = "{name}"\nversion = "0.1.0"\nedition = "2021"\n'
        if dependencies:
            manifest += '\n[dependencies]\n' + ''.join(
                f'{dependency} = {{ path = "../{dependency}" }}\n' for dependency in dependencies)
        if binary is not None:
            manifest += f'\n[[bin]]\nname = "{METRICS.ROOTS[name]}"\npath = "src/main.rs"\n'
            self.write(f'crates/{name}/src/main.rs', binary)
        self.write(f'crates/{name}/Cargo.toml', manifest)
        if library is not None:
            self.write(f'crates/{name}/src/lib.rs', library)

    def commit(self, message):
        self.git('add', '--all')
        self.git('commit', '-qm', message)
        return self.git('rev-parse', 'HEAD')

    def measurement_command(self, baseline=None, out=None):
        return [sys.executable, str(SCRIPT), '--repo', str(self.repo),
                '--baseline-ref', baseline or self.baseline, '--current-ref', self.current,
                '--out-dir', str(out or self.out), '--work-dir', str(self.work), '--jobs', '1']

    def run_measurement(self, *, expected=0, baseline=None, out=None, samples=1, event_codec=False):
        command = self.measurement_command(baseline, out) + ['--viewer-touch-samples', str(samples)]
        if event_codec:
            command.append('--runtime-event-codec-touch')
        result = subprocess.run(command, env=self.env, text=True, stdout=subprocess.PIPE,
                                stderr=subprocess.PIPE, timeout=120)
        self.assertEqual(result.returncode, expected, result.stdout + '\n' + result.stderr)
        return result

    def assert_isolated_cleanup(self):
        self.assertEqual(self.git('status', '--porcelain', '--untracked-files=all'), '')
        self.assertEqual(self.git('rev-parse', 'HEAD'), self.current)
        worktrees = [line for line in self.git('worktree', 'list', '--porcelain').splitlines()
                     if line.startswith('worktree ')]
        self.assertEqual(worktrees, ['worktree ' + str(self.repo)])
        self.assertFalse(self.work.exists() and list(self.work.iterdir()))
        self.assertFalse((self.repo / 'target').exists())

    def test_real_three_bin_seed_noop_and_compiled_viewer_mutation(self):
        before = METRICS.source_fingerprint(self.repo)
        self.run_measurement()
        report = json.loads((self.out / 'report.json').read_text())
        self.assertTrue(report['success'])
        self.assertEqual(report['revisions'], {'baseline': self.baseline, 'current': self.current})
        self.assertEqual(len(report['comparison']), 3)
        self.assertGreater(report['runner']['logical_cpu_count'], 0)
        self.assertGreater(report['runner']['host_memory_bytes']['MemTotal'], 0)
        self.assertTrue(json.loads((self.out / 'comparison.json').read_text())['success'])
        for label in ('current', 'baseline'):
            record = report['results'][label]
            self.assertTrue(record['source_restored'])
            self.assertEqual(set(record['phases']), set(METRICS.PHASES))
            self.assertEqual(record['phases']['no_op']['rebuilt'], 0)
            self.assertGreater(record['phases']['seed']['rebuilt'], 0)
            touch = record['phases']['viewer_touch']
            self.assertGreater(touch['local_crates']['oasis7_viewer_runtime']['rebuilt'], 0)
            for name in METRICS.LOWER_LIBRARIES:
                self.assertTrue(touch['lower_libraries'][name]['all_fresh'], name)
            for phase, metrics in record['phases'].items():
                self.assertTrue(metrics['success'])
                self.assertEqual(set(metrics['binaries']), set(METRICS.ROOTS.values()))
                self.assertIn('--offline', metrics['command'])
                self.assertIn('--locked', metrics['command'])
                self.assertIn('--timings', metrics['command'])
                self.assertGreaterEqual(metrics['wall_seconds'], 0)
                self.assertGreaterEqual(metrics['resources']['user_cpu_seconds'], 0)
                self.assertGreaterEqual(metrics['resources']['system_cpu_seconds'], 0)
                self.assertGreater(metrics['resources']['max_rss_kib'], 0)
                self.assertIn('wait4', metrics['resources']['method'])
                for artifact in metrics['binaries'].values():
                    self.assertEqual(len(artifact['sha256']), 64)
                    self.assertEqual(artifact['profile']['opt_level'], '3')
                    self.assertGreater(artifact['bytes'], 0)
                for filename in ('cargo.jsonl', 'cargo.stderr.log', 'diagnostics.log', 'metrics.json',
                                 'timings/cargo-timing.html'):
                    self.assertTrue((self.out / label / phase / filename).is_file(), filename)
            mutation = json.loads((self.out / label / 'mutation.json').read_text())
            self.assertNotEqual(mutation['before_sha256'], mutation['after_sha256'])
            patch = self.out / label / 'viewer.patch'
            self.assertEqual(mutation['patch_sha256'], METRICS.digest(patch))
            self.assertIn(METRICS.PATCH_BYTES.strip().decode(), patch.read_text())
        self.assertEqual(METRICS.source_fingerprint(self.repo), before)
        self.assert_isolated_cleanup()

    def test_failed_touched_compile_restores_source_and_cannot_publish_comparison(self):
        self.write('crates/oasis7_viewer_runtime/build.rs', '''fn main() {
    println!("cargo::rerun-if-changed=src/viewer/runtime_live/world_view_source.rs");
    let source = std::fs::read_to_string("src/viewer/runtime_live/world_view_source.rs").unwrap();
    assert!(!source.contains("ECS build measurement"), "deliberate touched-source failure");
}
''')
        self.current = self.commit('fail only the actual Viewer mutation')
        before = METRICS.source_fingerprint(self.repo)
        self.run_measurement(expected=1)
        report = json.loads((self.out / 'report.json').read_text())
        self.assertFalse(report['success'])
        self.assertTrue(report['results']['current']['source_restored'])
        metrics = json.loads((self.out / 'current/viewer_touch/metrics.json').read_text())
        self.assertFalse(metrics['success'])
        self.assertNotEqual(metrics['exit_code'], 0)
        self.assertGreater(metrics['resources']['max_rss_kib'], 0)
        self.assertIn('deliberate touched-source failure',
                      (self.out / 'current/viewer_touch/cargo.stderr.log').read_text())
        self.assertFalse((self.out / 'comparison.json').exists())
        self.assertEqual(METRICS.source_fingerprint(self.repo), before)
        self.assert_isolated_cleanup()

    def test_build_generated_ignored_source_is_rejected(self):
        self.write('crates/oasis7_viewer_runtime/build.rs', '''fn main() {
    std::fs::write("../../generated-unexpected.rs", "unexpected source input").unwrap();
}
''')
        self.current = self.commit('detect ignored source generation')
        self.run_measurement(expected=1)
        report = json.loads((self.out / 'report.json').read_text())
        self.assertFalse(report['success'])
        self.assertIn('build modified measurement source', report['error'])
        metrics = json.loads((self.out / 'current/seed/metrics.json').read_text())
        self.assertFalse(metrics['success'])
        self.assertEqual(metrics['exit_code'], 0)
        self.assertFalse((self.out / 'comparison.json').exists())
        self.assert_isolated_cleanup()

    def test_interrupt_touched_build_stops_children_and_restores_source(self):
        pid_file = self.root / 'touched-build-pid'
        self.env['ECS_MEASUREMENT_TEST_PID_FILE'] = str(pid_file)
        self.write('crates/oasis7_viewer_runtime/build.rs', '''fn main() {
    println!("cargo::rerun-if-changed=src/viewer/runtime_live/world_view_source.rs");
    let source = std::fs::read_to_string("src/viewer/runtime_live/world_view_source.rs").unwrap();
    if source.contains("ECS build measurement") {
        std::fs::write(std::env::var("ECS_MEASUREMENT_TEST_PID_FILE").unwrap(),
                       std::process::id().to_string()).unwrap();
        std::thread::sleep(std::time::Duration::from_secs(30));
    }
}
''')
        self.current = self.commit('interrupt an actual touched-source compiler child')
        before = METRICS.source_fingerprint(self.repo)
        process = subprocess.Popen(self.measurement_command(), env=self.env, text=True,
                                   stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        try:
            deadline = time.monotonic() + 30
            while not pid_file.exists() and process.poll() is None and time.monotonic() < deadline:
                time.sleep(0.01)
            self.assertTrue(pid_file.exists(), 'measurement did not reach touched build child')
            child_pid = int(pid_file.read_text())
            process.send_signal(signal.SIGTERM)
            stdout, stderr = process.communicate(timeout=20)
            self.assertEqual(process.returncode, 1, stdout + '\n' + stderr)
        finally:
            if process.poll() is None:
                process.send_signal(signal.SIGTERM)
                process.communicate(timeout=20)
        status = Path(f'/proc/{child_pid}/status')
        if status.exists():
            self.assertIn('Z (zombie)', next(line for line in status.read_text().splitlines()
                                            if line.startswith('State:')))
        report = json.loads((self.out / 'report.json').read_text())
        self.assertFalse(report['success'])
        self.assertIn('interrupted by signal', report['error'])
        self.assertTrue(report['results']['current']['source_restored'])
        metrics = json.loads((self.out / 'current/viewer_touch/metrics.json').read_text())
        self.assertFalse(metrics['success'])
        self.assertFalse((self.out / 'comparison.json').exists())
        self.assertEqual(METRICS.source_fingerprint(self.repo), before)
        self.assert_isolated_cleanup()

    def test_uncompiled_viewer_path_cannot_claim_invalidation(self):
        self.write('crates/oasis7_viewer_runtime/src/lib.rs',
                   'pub fn value() -> u32 { oasis7_world_service::value() + oasis7_game_client::value() }\n')
        self.current = self.commit('Viewer fixture path is tracked but not compiled')
        self.run_measurement(expected=1)
        report = json.loads((self.out / 'report.json').read_text())
        self.assertFalse(report['success'])
        self.assertIn('did not rebuild the Viewer library', report['error'])
        self.assertTrue(report['results']['current']['source_restored'])
        self.assertFalse((self.out / 'comparison.json').exists())
        self.assert_isolated_cleanup()

    def test_noop_rebuild_is_rejected(self):
        self.write('crates/oasis7_viewer_runtime/build.rs', '''fn main() {
    println!("cargo::rerun-if-changed=deliberately-missing-input");
}
''')
        self.current = self.commit('non-repeatable Cargo fixture')
        self.run_measurement(expected=1)
        report = json.loads((self.out / 'report.json').read_text())
        self.assertFalse(report['success'])
        self.assertIn('same-source no-op unexpectedly rebuilt', report['error'])
        metrics = json.loads((self.out / 'current/no_op/metrics.json').read_text())
        self.assertGreater(metrics['rebuilt'], 0)
        self.assertFalse((self.out / 'comparison.json').exists())
        self.assert_isolated_cleanup()

    def test_dirty_source_and_reports_inside_checkout_are_rejected_before_build(self):
        path = self.repo / METRICS.VIEWER_PATH
        original = path.read_bytes()
        path.write_bytes(original + b'\n// developer edit\n')
        result = self.run_measurement(expected=1)
        self.assertIn('source checkout must be clean', result.stderr)
        self.assertFalse(self.out.exists())
        self.assertEqual(path.read_bytes(), original + b'\n// developer edit\n')
        path.write_bytes(original)
        result = self.run_measurement(expected=1, out=self.repo / 'report')
        self.assertIn('outside every checkout', result.stderr)
        self.assertFalse((self.repo / 'report').exists())
        self.assert_isolated_cleanup()

    def test_mutation_context_restores_bytes_mode_and_mtime_on_exception(self):
        path = self.repo / METRICS.VIEWER_PATH
        original, info = path.read_bytes(), path.stat()
        self.out.mkdir()
        with self.assertRaisesRegex(RuntimeError, 'fixture interruption'):
            with METRICS.viewer_mutation(self.repo, self.out):
                self.assertEqual(path.read_bytes(), original + METRICS.PATCH_BYTES)
                raise RuntimeError('fixture interruption')
        self.assertEqual(path.read_bytes(), original)
        self.assertEqual(path.stat().st_mode, info.st_mode)
        self.assertEqual(path.stat().st_mtime_ns, info.st_mtime_ns)
        self.assert_isolated_cleanup()


if __name__ == '__main__':
    unittest.main()
