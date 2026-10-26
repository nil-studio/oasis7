#!/usr/bin/env python3
"""Workspace cache boundaries without GitHub credentials or a native Rust build."""
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import MagicMock, patch

spec = importlib.util.spec_from_file_location('workspace_cache', Path(__file__).with_name('ci-workspace-cache.py'))
cache = importlib.util.module_from_spec(spec)
spec.loader.exec_module(cache)


def key(scope='ordinary', source='b'):
    version = 'v1' if scope == 'ordinary' else 'v3'
    return f'{scope}-workspace-{version}-' + 'a' * 64 + '-' + source * 40


def environment(scope='ordinary', **changes):
    value = dict(GITHUB_ACTIONS='true', GITHUB_REF='refs/heads/main', GITHUB_SHA='b' * 40,
        GITHUB_WORKFLOW_SHA='b' * 40, GITHUB_EVENT_NAME='push', GITHUB_REPOSITORY='nil-studio/oasis7',
        GITHUB_JOB='oasis7-test-build-trusted' if scope == 'ordinary' else 'formal-test-build-trusted',
        ACTIONS_CACHE_MODE='write',
        GITHUB_WORKFLOW_REF='nil-studio/oasis7/.github/workflows/rust.yml@refs/heads/main')
    return dict(value, **changes)


def entry(number=1, **changes):
    return dict(dict(id=number, key=key(source=f'{number:x}'), ref='refs/heads/main',
        version='v1', size_in_bytes=100, created_at='2026-10-22T00:00:00Z',
        last_accessed_at='2026-10-22T00:00:00Z'), **changes)


def api(entries=()):
    client = MagicMock()
    client.list_caches.return_value = list(entries)
    client.request.return_value = {'max_cache_size_gb': 10}
    return client


def fixture(directory, scope='ordinary'):
    root = directory / 'checkout'
    (root / 'src').mkdir(parents=True)
    (root / 'Cargo.toml').write_text('[package]\nname="tiny"\nversion="0.1.0"\n')
    (root / 'src/lib.rs').write_text('pub fn tiny() {}\n')
    target = root / 'target'
    fingerprint = target / 'debug/.fingerprint/tiny-1234567890abcdef'
    fingerprint.mkdir(parents=True)
    (fingerprint / 'lib-tiny').write_bytes(b'fingerprint')
    library = target / 'debug/deps/libtiny-1234567890abcdef.rlib'
    library.parent.mkdir(parents=True)
    library.write_bytes(b'complete library unit\n' * 100)
    state = cache.units.cache_begin(root, selection='libraries' if scope == 'ordinary' else 'all')
    state.update(packages={'tiny': str(root / 'Cargo.toml')},
        files=[library.relative_to(target).as_posix()],
        directories=[fingerprint.relative_to(target).as_posix()])
    cache.units.cache_finish(state)
    state_path = directory / 'state.json'
    state_path.write_text(json.dumps(state))
    return root, state_path, library


class Workspace(unittest.TestCase):
    def test_identity_is_scoped_and_tracks_configuration_but_not_source_contents(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            inputs = ('Cargo.toml', 'Cargo.lock', cache.WORKFLOW, 'scripts/ci-workspace-cache.py',
                      'scripts/ci-cache-policy.py', 'scripts/ci-cache-archive.py', 'scripts/ci-test-artifacts.py',
                      'scripts/ci-formal-process-suites.json', 'scripts/ci-rust-cache-toolchains.py')
            for name in (*inputs, 'src/lib.rs'):
                path = root / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(name)
            source = ['b' * 40]

            def output(command, **kwargs):
                if command[:2] == ['cargo', 'metadata']:
                    return json.dumps({'packages': [{'manifest_path': str(root / 'Cargo.toml')}]})
                if command[:2] == ['git', 'rev-parse']:
                    return source[0]
                return ' '.join(command) + ' pinned-version'

            with patch.object(cache, 'ROOT', root), patch.object(cache.subprocess, 'check_output', side_effect=output), \
                 patch.dict(os.environ, {'OASIS7_WASM_TOOLCHAIN': 'nightly-2025-12-11'}, clear=True):
                original = cache.identity('ordinary')
                self.assertTrue(cache.current_key('ordinary', original['key']))
                self.assertNotEqual(cache.identity('formal')['prefix'], original['prefix'])
                source[0] = 'c' * 40
                (root / 'src/lib.rs').write_text('modified source')
                self.assertEqual(cache.identity('ordinary')['prefix'], original['prefix'])
                self.assertTrue(cache.identity('ordinary')['key'].endswith(source[0]))
                with patch.dict(os.environ, CARGO_BUILD_JOBS='1', CARGO_TERM_COLOR='never'):
                    self.assertEqual(cache.identity('ordinary')['prefix'], original['prefix'])
                for variable in ('RUSTFLAGS', 'CARGO_PROFILE_TEST_DEBUG', 'OASIS7_WASM_TOOLCHAIN'):
                    with patch.dict(os.environ, {variable: 'changed'}):
                        self.assertNotEqual(cache.identity('ordinary')['prefix'], original['prefix'])
                for name in inputs:
                    path = root / name
                    before = path.read_text()
                    path.write_text(before + ' changed')
                    self.assertNotEqual(cache.identity('ordinary')['prefix'], original['prefix'], name)
                    path.write_text(before)
                with patch.object(cache.platform, 'machine', return_value='different-architecture'):
                    self.assertNotEqual(cache.identity('ordinary')['prefix'], original['prefix'])

    def test_only_matching_trusted_main_lane_can_stage(self):
        for scope in ('ordinary', 'formal'):
            for event in ('push', 'schedule'):
                with patch.dict(os.environ, environment(scope, GITHUB_EVENT_NAME=event), clear=True):
                    cache.trusted_writer(scope, key(scope))
            for changes in ({'GITHUB_JOB': 'oasis7-test-build-candidate'},
                            {'GITHUB_REF': 'refs/pull/1/merge'}, {'GITHUB_EVENT_NAME': 'pull_request'},
                            {'GITHUB_WORKFLOW_SHA': 'c' * 40}, {'ACTIONS_CACHE_MODE': 'read'},
                            {'GITHUB_WORKFLOW_REF': 'nil-studio/oasis7/.github/workflows/other.yml@refs/heads/main'}):
                with self.subTest(scope=scope, changes=changes), \
                     patch.dict(os.environ, environment(scope, **changes), clear=True), self.assertRaises(ValueError):
                    cache.trusted_writer(scope, key(scope))
            with patch.dict(os.environ, environment(scope), clear=True), self.assertRaises(ValueError):
                cache.trusted_writer(scope, key(scope, source='c'))

    def test_manual_writer_requires_full_in_event_payload(self):
        with tempfile.TemporaryDirectory() as directory:
            event = Path(directory) / 'event.json'
            for mode in ('full', 'newapi_bridge_package', None):
                event.write_text(json.dumps({'inputs': {'run_mode': mode}}))
                with patch.dict(os.environ, environment(GITHUB_EVENT_NAME='workflow_dispatch',
                                GITHUB_EVENT_PATH=str(event)), clear=True):
                    if mode == 'full':
                        cache.trusted_writer('ordinary', key())
                    else:
                        with self.assertRaises(ValueError):
                            cache.trusted_writer('ordinary', key())

    def test_measurement_does_not_query_capacity_or_prepare_payload(self):
        with tempfile.TemporaryDirectory() as directory:
            directory = Path(directory)
            root, state, _ = fixture(directory)
            args = SimpleNamespace(scope='ordinary', cache=directory / 'cache', state=state)
            with patch.object(cache, 'ROOT', root), patch.object(cache, 'api_client') as remote, \
                 patch.object(cache.archive, 'pack_archive') as pack:
                report = {}
                cache.measure_cache(args, report)
            self.assertTrue(report['eligible'])
            self.assertFalse(report['save'])
            self.assertEqual(report['selection'], 'libraries')
            remote.assert_not_called()
            pack.assert_not_called()
            self.assertFalse(args.cache.exists())

    def test_stage_rechecks_capacity_and_never_saves_when_inventory_grows(self):
        with tempfile.TemporaryDirectory() as directory:
            directory = Path(directory)
            root, state, _ = fixture(directory)
            args = SimpleNamespace(scope='ordinary', cache=directory / 'cache', state=state)
            first, second = api(), api([entry(key='unrelated', size_in_bytes=10_000_000_000)])
            with patch.object(cache, 'ROOT', root), patch.object(cache, 'identity', return_value={'key': key()}), \
                 patch.object(cache, 'api_client', side_effect=[first, second]), \
                 patch.dict(os.environ, environment(), clear=True):
                report = {}
                cache.stage_cache(args, 10_000_000_000, report)
            self.assertFalse(report['save'])
            self.assertEqual(report['reason'], 'repository cache capacity gap')
            self.assertGreater(report['upload_bound_bytes'], report['archive_bytes'])
            self.assertFalse((args.cache / cache.ARCHIVE_NAME).exists())

    def test_full_stage_preserves_dependency_space_after_final_inventory_read(self):
        reserve = 5 * 1024 ** 3 // 2
        limit = 10_000_000_000
        for scope in ('ordinary', 'formal'):
            for event in ('push', 'schedule', 'workflow_dispatch'):
                with self.subTest(scope=scope, event=event), tempfile.TemporaryDirectory() as directory:
                    directory = Path(directory)
                    root, state, _ = fixture(directory, scope)
                    event_path = directory / 'event.json'
                    event_path.write_text(json.dumps({'inputs': {'run_mode': 'full'}}))
                    args = SimpleNamespace(scope=scope, cache=directory / 'cache', state=state)
                    first = api()
                    final = api([entry(key='other-families', size_in_bytes=limit - reserve)])
                    with patch.object(cache, 'ROOT', root), \
                         patch.object(cache, 'identity', return_value={'key': key(scope)}), \
                         patch.object(cache, 'api_client', side_effect=[first, final]), \
                         patch.dict(os.environ, environment(scope, GITHUB_EVENT_NAME=event,
                                        GITHUB_EVENT_PATH=str(event_path)), clear=True):
                        report = {}
                        cache.stage_cache(args, limit, report)
                    first.list_caches.assert_called_once_with()
                    final.list_caches.assert_called_once_with()
                    self.assertEqual(report['observed_repository_bytes'], limit - reserve)
                    self.assertGreater(report['upload_bound_bytes'], report['archive_bytes'])
                    self.assertEqual(report['projected_repository_bytes'],
                                     limit - reserve + report['upload_bound_bytes'])
                    self.assertEqual(report['save'], event == 'push', report)
                    self.assertEqual(report['reserved_repository_bytes'], 0 if event == 'push' else reserve)
                    self.assertEqual((args.cache / cache.ARCHIVE_NAME).exists(), event == 'push')
                    if event != 'push':
                        self.assertEqual(report['reason'], 'dependency rotation headroom')

    def test_full_stage_can_save_when_actual_upload_and_rotation_space_both_fit(self):
        for scope in ('ordinary', 'formal'):
            with self.subTest(scope=scope), tempfile.TemporaryDirectory() as directory:
                directory = Path(directory)
                root, state, _ = fixture(directory, scope)
                args = SimpleNamespace(scope=scope, cache=directory / 'cache', state=state)
                with patch.object(cache, 'ROOT', root), \
                     patch.object(cache, 'identity', return_value={'key': key(scope)}), \
                     patch.object(cache, 'api_client', return_value=api()), \
                     patch.dict(os.environ, environment(scope, GITHUB_EVENT_NAME='schedule'), clear=True):
                    report = {}
                    cache.stage_cache(args, 10_000_000_000, report)
                self.assertTrue(report['save'], report)
                self.assertEqual(report['reserved_repository_bytes'], 5 * 1024 ** 3 // 2)
                self.assertGreaterEqual(report['projected_available_repository_bytes'],
                                        report['reserved_repository_bytes'])

    def test_stage_rejects_untrusted_or_nonfull_manual_runs_before_preparation(self):
        with tempfile.TemporaryDirectory() as directory:
            directory = Path(directory)
            event_path = directory / 'event.json'
            event_path.write_text(json.dumps({'inputs': {'run_mode': 'required'}}))
            args = SimpleNamespace(scope='ordinary', cache=directory / 'cache', state=directory / 'state.json')
            for changes in ({'GITHUB_JOB': 'oasis7-test-build-candidate'},
                            {'GITHUB_REF': 'refs/pull/1/merge'},
                            {'GITHUB_WORKFLOW_SHA': 'c' * 40},
                            {'GITHUB_EVENT_NAME': 'workflow_dispatch', 'GITHUB_EVENT_PATH': str(event_path)}):
                with self.subTest(changes=changes), \
                     patch.object(cache, 'identity', return_value={'key': key()}), \
                     patch.object(cache, 'api_client') as client, \
                     patch.object(cache, 'measure_cache') as measure, \
                     patch.dict(os.environ, environment(**changes), clear=True):
                    with self.assertRaises(ValueError):
                        cache.stage_cache(args, 10_000_000_000, {})
                    client.assert_not_called()
                    measure.assert_not_called()
                    self.assertFalse(args.cache.exists())

    def test_measured_ordinary_selection_fits_staging_without_hashing_or_copying(self):
        # Ordinary producer 120263591919 measured this logical library-only
        # byte count. A sparse file reproduces admission without allocating or
        # pretending to compress the hosted payload.
        observed_raw_bytes = 4_428_645_549
        with tempfile.TemporaryDirectory() as directory:
            directory = Path(directory)
            root, state, library = fixture(directory)
            with library.open('wb') as stream:
                stream.truncate(observed_raw_bytes - len(b'fingerprint'))
            args = SimpleNamespace(scope='ordinary', cache=directory / 'cache', state=state)
            with patch.object(cache, 'ROOT', root), patch.object(cache, 'api_client') as remote, \
                 patch.object(cache.units, '_cache_file') as hash_file, \
                 patch.object(cache.units, '_cache_copy') as copy_files:
                report = {}
                cache.measure_cache(args, report)
            self.assertEqual(report['raw_bytes'], observed_raw_bytes)
            self.assertTrue(report['eligible'], report)
            self.assertFalse(report['save'])
            remote.assert_not_called()
            hash_file.assert_not_called()
            copy_files.assert_not_called()

    def test_stage_round_trip_and_restore_requires_compatible_key(self):
        for scope in ('ordinary', 'formal'):
            with self.subTest(scope=scope), tempfile.TemporaryDirectory() as directory:
                directory = Path(directory)
                root, state, library = fixture(directory, scope)
                args = SimpleNamespace(scope=scope, cache=directory / 'cache', state=state)
                with patch.object(cache, 'ROOT', root), patch.object(cache, 'identity', return_value={'key': key(scope)}), \
                     patch.object(cache, 'api_client', return_value=api()), \
                     patch.dict(os.environ, environment(scope), clear=True):
                    report = {}
                    cache.stage_cache(args, 10_000_000_000, report)
                    self.assertTrue(report['save'], report)
                    payload = args.cache / 'workspace-units.tar.zst'
                    self.assertTrue(payload.is_file(), sorted(args.cache.iterdir()))
                    self.assertEqual(payload.read_bytes()[:4], b'\x28\xb5\x2f\xfd')
                    expected = library.read_bytes()
                    shutil.rmtree(root / 'target')
                    args.requested_key, args.matched_key, args.cache_hit = key(scope, 'c'), key(scope), 'false'
                    restored = {}
                    cache.restore_cache(args, restored)
                    self.assertTrue(restored['restored'], restored)
                    self.assertEqual(library.read_bytes(), expected)
                    args.matched_key = key(scope).replace('a' * 64, 'd' * 64)
                    with self.assertRaises(ValueError):
                        cache.restore_cache(args, {})

    def test_observation_requires_unique_complete_main_entry(self):
        for entries, confirmed in (([], False), ([entry(key=key())], True),
                                   ([entry(key=key(), size_in_bytes=0)], False),
                                   ([entry(key=key()), entry(2, key=key(), version='other')], False),
                                   ([entry(key=key(), ref='refs/pull/1/merge')], False)):
            with self.subTest(entries=entries), patch.object(cache, 'api_client', return_value=api(entries)):
                report = {}
                cache.observe_save('ordinary', key(), report)
                self.assertEqual(report['saved_confirmed'], confirmed)

    def test_wrong_selection_cannot_be_uploaded_as_ordinary(self):
        with tempfile.TemporaryDirectory() as directory:
            directory = Path(directory)
            root, state, _ = fixture(directory, 'formal')
            args = SimpleNamespace(scope='ordinary', cache=directory / 'cache', state=state)
            with patch.object(cache, 'ROOT', root), self.assertRaises(ValueError):
                cache.measure_cache(args, {})


@unittest.skipUnless(all(shutil.which(tool) for tool in ('cargo', 'rustc', 'zstd')),
                     'compact workspace smoke requires Cargo, rustc and zstd')
class RealCargoCompact(unittest.TestCase):
    def test_compatible_restore_is_fresh_and_changed_leaf_rebuilds(self):
        for scope in ('ordinary', 'formal'):
            with self.subTest(scope=scope), tempfile.TemporaryDirectory() as directory:
                base = Path(directory)
                root = base / 'checkout'
                sources = {
                    'Cargo.toml': '[workspace]\nmembers=["base", "leaf"]\nresolver="2"\n'
                                  '[profile.dev]\ndebug=0\nincremental=false\n',
                    'base/Cargo.toml': '[package]\nname="compact_base"\nversion="0.1.0"\nedition="2021"\n',
                    'base/src/lib.rs': 'pub fn value() -> u32 { 7 }\n',
                    'base/build.rs': 'fn main() { println!("cargo:rerun-if-changed=src/lib.rs"); }\n',
                    'leaf/Cargo.toml': '[package]\nname="compact_leaf"\nversion="0.1.0"\nedition="2021"\n'
                                       '[dependencies]\ncompact_base={path="../base"}\n',
                    'leaf/src/lib.rs': 'pub fn value() -> u32 { compact_base::value() }\n',
                    'leaf/src/main.rs': 'fn main() { println!("{}", compact_leaf::value()); }\n',
                }
                for name, content in sources.items():
                    path = root / name
                    path.parent.mkdir(parents=True, exist_ok=True)
                    path.write_text(content)
                env = dict(os.environ, CARGO_INCREMENTAL='0')
                for name in ('CARGO_TARGET_DIR', 'RUSTC_WRAPPER', 'RUSTC_WORKSPACE_WRAPPER'):
                    env.pop(name, None)
                subprocess.run(['cargo', 'generate-lockfile', '--offline'], cwd=root, env=env,
                               check=True, capture_output=True)

                def build(state=None):
                    result = subprocess.run(['cargo', 'build', '--workspace', '--offline', '--locked',
                                             '--message-format=json'], cwd=root, env=env,
                                            text=True, capture_output=True)
                    self.assertEqual(result.returncode, 0, result.stderr)
                    messages = [json.loads(line) for line in result.stdout.splitlines() if line.startswith('{')]
                    if state is not None:
                        for message in messages:
                            cache.units.cache_record(state, message)
                        cache.units.cache_finish(state)
                    value = subprocess.check_output([str(root / 'target/debug/compact_leaf')], text=True).strip()
                    return [m for m in messages if m.get('reason') == 'compiler-artifact'], value

                state = cache.units.cache_begin(root, selection='libraries' if scope == 'ordinary' else 'all')
                cold_units, value = build(state)
                self.assertEqual(value, '7')
                self.assertTrue(all(not unit['fresh'] for unit in cold_units))
                state_path = base / 'state.json'
                state_path.write_text(json.dumps(state))
                args = SimpleNamespace(scope=scope, state=state_path, cache=base / 'cache',
                                       requested_key=key(scope, 'c'), matched_key=key(scope), cache_hit='false')
                with patch.object(cache, 'ROOT', root), patch.object(cache, 'identity', return_value={'key': key(scope)}), \
                     patch.object(cache, 'api_client', return_value=api()), \
                     patch.dict(os.environ, environment(scope)):
                    report = {}
                    cache.stage_cache(args, 10_000_000_000, report)
                    self.assertTrue(report['save'], report)
                    self.assertEqual((args.cache / cache.ARCHIVE_NAME).read_bytes()[:4], b'\x28\xb5\x2f\xfd')
                    for changed in (False, True):
                        shutil.rmtree(root / 'target')
                        for name in (*sources, 'Cargo.lock'):
                            os.utime(root / name, None)
                        if changed:
                            leaf = root / 'leaf/src/lib.rs'
                            leaf.write_text('pub fn value() -> u32 { compact_base::value() + 2 }\n')
                            previous = state['inputs']['leaf/src/lib.rs']['mtime_ns']
                            os.utime(leaf, ns=(previous, previous))
                        restored = {}
                        cache.restore_cache(args, restored)
                        self.assertTrue(restored['restored'], restored)
                        self.assertEqual(restored['changed_inputs'], int(changed))
                        if scope == 'ordinary':
                            self.assertFalse((root / 'target/debug/compact_leaf').exists())
                        warm_units, value = build()
                        self.assertEqual(value, '9' if changed else '7')
                        for unit in warm_units:
                            expected = not ((changed and unit['target']['name'] == 'compact_leaf')
                                            or (scope == 'ordinary' and 'bin' in unit['target']['kind']))
                            self.assertEqual(unit['fresh'], expected, unit)


if __name__ == '__main__':
    unittest.main()
