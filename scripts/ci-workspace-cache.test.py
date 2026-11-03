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
    value = dict(PATH=os.environ.get("PATH", os.defpath), GITHUB_ACTIONS='true', GITHUB_REF='refs/heads/main', GITHUB_SHA='b' * 40,
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
            root = Path(directory).resolve()
            inputs = ('Cargo.toml', 'Cargo.lock', 'scripts/ci-workspace-cache.py',
                      'scripts/ci-cache-archive.py', 'scripts/ci-test-artifacts.py',
                      'scripts/ci-formal-process-suites.json', 'scripts/ci-rust-cache-toolchains.py',
                      '.cargo/config.toml', 'rust-toolchain.toml', 'crates/leaf/Cargo.toml')
            operational = (cache.WORKFLOW, 'scripts/ci-cache-policy.py')
            for name in (*inputs, *operational, 'src/lib.rs'):
                path = root / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(name)
            source = ['b' * 40]

            def output(command, **kwargs):
                if command[:2] == ['cargo', 'metadata']:
                    # A virtual workspace does not list its root manifest as a package.
                    return json.dumps({'packages': [{'manifest_path': str(root / 'crates/leaf/Cargo.toml')}]})
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
                for scope in ('ordinary', 'formal'):
                    baseline = cache.identity(scope)['prefix']
                    for variable in ('RUSTFLAGS', 'CARGO_PROFILE_TEST_DEBUG', 'OASIS7_WASM_TOOLCHAIN'):
                        with self.subTest(scope=scope, variable=variable), patch.dict(os.environ, {variable: 'changed'}):
                            self.assertNotEqual(cache.identity(scope)['prefix'], baseline)
                    for name in (*inputs, *operational):
                        path = root / name
                        before = path.read_text()
                        path.write_text(before + ' changed')
                        with self.subTest(scope=scope, path=name):
                            self.assertEqual(cache.identity(scope)['prefix'] == baseline, name in operational)
                        path.write_text(before)
                    with patch.dict(os.environ, CI_CACHE_REPOSITORY_BUDGET_BYTES='20000000000'):
                        self.assertEqual(cache.identity(scope)['prefix'], baseline)
                    with patch.object(cache.units, 'build_variants', return_value=[('different', ['build'], [])]):
                        self.assertNotEqual(cache.identity(scope)['prefix'], baseline)
                with patch.object(cache.platform, 'machine', return_value='different-architecture'):
                    self.assertNotEqual(cache.identity('ordinary')['prefix'], original['prefix'])
                (root / 'Cargo.toml').unlink()
                with self.assertRaises(FileNotFoundError):
                    cache.identity('ordinary')

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
            event = Path(directory).resolve() / 'event.json'
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
            directory = Path(directory).resolve()
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
            directory = Path(directory).resolve()
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
                    directory = Path(directory).resolve()
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
                directory = Path(directory).resolve()
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
            directory = Path(directory).resolve()
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
            directory = Path(directory).resolve()
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
                directory = Path(directory).resolve()
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
            directory = Path(directory).resolve()
            root, state, _ = fixture(directory, 'formal')
            args = SimpleNamespace(scope='ordinary', cache=directory / 'cache', state=state)
            with patch.object(cache, 'ROOT', root), self.assertRaises(ValueError):
                cache.measure_cache(args, {})


class BoundedWorkspace(unittest.TestCase):
    def make_state(self, directory, sizes=(600, 500, 400)):
        root = directory / 'checkout'
        root.mkdir()
        (root / 'Cargo.toml').write_text('[workspace]\n')
        target = root / 'target'
        state = cache.units.cache_begin(root, selection='libraries')
        for index, size in enumerate(sizes):
            name, unit = f'unit{index}', f'{index + 1:016x}'
            fingerprint = f'debug/.fingerprint/{name}-{unit}'
            path = target / fingerprint
            path.mkdir(parents=True)
            (path / ('lib-' + name)).write_bytes(b'fingerprint')
            for suffix in ('rlib', 'rmeta', 'd'):
                output = f'debug/deps/{"" if suffix == "d" else "lib"}{name}-{unit}.{suffix}'
                file = target / output
                file.parent.mkdir(parents=True, exist_ok=True)
                with file.open('wb') as stream:
                    stream.truncate(size if suffix == 'rlib' else 10)
                os.utime(file, ns=(1_000_000_000 + index, 1_000_000_000 + index))
                state['files'].append(output)
            os.utime(path / ('lib-' + name), ns=(1_000_000_000 + index, 1_000_000_000 + index))
            state['directories'].append(fingerprint)
            state['packages'][name] = name
        cache.units.cache_finish(state)
        return root, state

    def test_retains_complete_recent_units_without_mutating_capture(self):
        with tempfile.TemporaryDirectory() as directory:
            root, state = self.make_state(Path(directory).resolve())
            original = json.loads(json.dumps(state))
            report = {}
            chosen = cache.bounded_library_state(state, 1000, report)
            self.assertEqual(state, original)
            self.assertEqual(len(chosen['directories']), 2)
            self.assertEqual(len(chosen['files']), 6)
            self.assertFalse(any('unit0-' in name for name in chosen['files'] + chosen['directories']))
            self.assertEqual(chosen['inputs'], state['inputs'])
            self.assertEqual(chosen['input_owners'], state['input_owners'])
            self.assertEqual(report['omitted_unit_groups'], 1)
            measured = cache.units.cache_stage(chosen, Path(directory).resolve() / 'unused', 1000, measure_only=True)
            self.assertTrue(measured['eligible'], measured)
            self.assertEqual(report['selection_retained_bytes'], measured['raw_bytes'])
            reordered = dict(state, files=list(reversed(state['files'])), directories=list(reversed(state['directories'])))
            self.assertEqual(cache.bounded_library_state(reordered, 1000, {}), chosen)
            self.assertTrue(all((root / 'target' / name).exists() for name in state['files']))

    def test_public_library_alias_and_dep_info_follow_their_complete_unit(self):
        with tempfile.TemporaryDirectory() as directory:
            root, state = self.make_state(Path(directory).resolve())
            target = root / 'target'
            for index in range(3):
                unit = f'{index + 1:016x}'
                public = target / f'debug/libunit{index}.rlib'
                os.link(target / f'debug/deps/libunit{index}-{unit}.rlib', public)
                dep_info = public.with_suffix('.d')
                dep_info.write_bytes(b'dep-info')
                state['files'].extend([public.relative_to(target).as_posix(),
                                       dep_info.relative_to(target).as_posix()])
            chosen = cache.bounded_library_state(state, 1900, {})
            self.assertNotIn('debug/libunit0.d', chosen['files'])
            self.assertNotIn('debug/libunit0.rlib', chosen['files'])
            for index in (1, 2):
                self.assertIn(f'debug/libunit{index}.d', chosen['files'])
                self.assertIn(f'debug/libunit{index}.rlib', chosen['files'])
                self.assertIn(f'debug/.fingerprint/unit{index}-{index + 1:016x}', chosen['directories'])
            mystery = target / 'debug/mystery.d'
            mystery.write_bytes(b'unbound')
            state['files'].append('debug/mystery.d')
            with self.assertRaisesRegex(ValueError, 'unbound bounded Cargo output'):
                cache.bounded_library_state(state, 1900, {})

    def test_oversized_newest_unit_does_not_discard_smaller_complete_units(self):
        with tempfile.TemporaryDirectory() as directory:
            _, state = self.make_state(Path(directory).resolve(), (100, 100, 2000))
            chosen = cache.bounded_library_state(state, 300, {})
            self.assertEqual(len(chosen['directories']), 2)
            self.assertFalse(any('unit2-' in name for name in chosen['files']))

    def test_malformed_capture_cannot_be_pruned_into_success(self):
        for change in ('incomplete', 'selection', 'unsafe', 'missing', 'unbound', 'ambiguous'):
            with self.subTest(change=change), tempfile.TemporaryDirectory() as directory:
                root, state = self.make_state(Path(directory).resolve())
                if change == 'incomplete':
                    state['complete'] = False
                elif change == 'selection':
                    state['selection'] = 'all'
                elif change == 'unsafe':
                    state['files'].append('../outside')
                elif change == 'missing':
                    (root / 'target' / state['files'][0]).unlink()
                elif change == 'unbound':
                    path = root / 'target/debug/mystery.rlib'
                    path.write_bytes(b'x')
                    state['files'].append('debug/mystery.rlib')
                else:
                    state['directories'].append('debug/.fingerprint/other-0000000000000001')
                with self.assertRaises((OSError, ValueError)):
                    cache.bounded_library_state(state, 1000, {})

    def test_generator_units_and_unhashed_libraries_are_pinned(self):
        with tempfile.TemporaryDirectory() as directory:
            root, state = self.make_state(Path(directory).resolve())
            target = root / 'target'
            additions = ['debug/build/generator-1111111111111111',
                         'debug/.fingerprint/generator-1111111111111111',
                         'debug/build/generator-2222222222222222',
                         'debug/.fingerprint/generator-2222222222222222',
                         'debug/.fingerprint/mixed-3333333333333333']
            for name in additions:
                path = target / name
                path.mkdir(parents=True)
                (path / 'metadata').write_bytes(b'x')
            state['directories'].extend(additions)
            outputs = ['debug/build/generator-1111111111111111/build-script-build',
                       'debug/deps/libmixed.so', 'debug/deps/libmixed.rlib', 'debug/deps/mixed.d']
            for name in outputs:
                (target / name).write_bytes(b'output')
            state['files'].extend(outputs)
            state['unhashed_libraries'] = {'debug/mixed': dict(profile='debug', package='mixed',
                name='mixed', filenames=['debug/deps/libmixed.so', 'debug/deps/libmixed.rlib'])}
            chosen = cache.bounded_library_state(state, 500, {})
            self.assertTrue(set(additions) <= set(chosen['directories']))
            self.assertTrue(set(outputs) <= set(chosen['files']))
            with self.assertRaisesRegex(ValueError, 'mandatory'):
                cache.bounded_library_state(state, 1, {})

    def test_cross_unit_hardlinks_are_not_split(self):
        with tempfile.TemporaryDirectory() as directory:
            root, state = self.make_state(Path(directory).resolve(), (100, 100, 200))
            first, second = [root / 'target/debug/deps' / f'libunit{index}-{index + 1:016x}.rlib'
                             for index in (0, 1)]
            second.unlink()
            os.link(first, second)
            chosen = cache.bounded_library_state(state, 300, {})
            self.assertEqual(len(chosen['directories']), 2)
            self.assertFalse(any('unit2-' in name for name in chosen['files']))
            with self.assertRaisesRegex(ValueError, 'mandatory'):
                cache.bounded_library_state(state, 200, {})

    def test_real_observed_raw_size_is_bounded_without_hash_or_copy(self):
        # Sparse fixtures reproduce the 12,144,199,095-byte observed selection,
        # not its compression ratio or its actual compiled output contents.
        with tempfile.TemporaryDirectory() as directory:
            directory = Path(directory).resolve()
            observed = 12_144_199_095
            root, state = self.make_state(directory, (observed // 3 - 31,) * 3)
            state_path = directory / 'state.json'
            state_path.write_text(json.dumps(state))
            args = SimpleNamespace(scope='ordinary', state=state_path, cache=directory / 'cache')
            with patch.object(cache, 'ROOT', root), patch.object(cache.units, '_cache_file') as digest, \
                 patch.object(cache.units, '_cache_copy') as copy, patch.object(cache, 'api_client') as remote:
                report = {}
                chosen = cache.measure_cache(args, report)
            self.assertEqual(report['full_selection_raw_bytes'], observed)
            self.assertTrue(report['eligible'], report)
            self.assertLessEqual(report['raw_bytes'], 6 * 1024 ** 3)
            self.assertLess(len(chosen['files']), len(state['files']))
            digest.assert_not_called()
            copy.assert_not_called()
            remote.assert_not_called()
            self.assertFalse(args.cache.exists())

    def test_upload_miss_reselects_locally_and_rechecks_final_inventory(self):
        for inventory_growth in (False, True):
            with self.subTest(inventory_growth=inventory_growth), tempfile.TemporaryDirectory() as directory:
                directory = Path(directory).resolve()
                root, state = self.make_state(directory)
                state_path = directory / 'state.json'
                state_path.write_text(json.dumps(state))
                args = SimpleNamespace(scope='ordinary', state=state_path, cache=directory / 'cache')
                first, final = api(), api([entry(key='other', size_in_bytes=10_000_000_000)] if inventory_growth else [])
                limit = cache.policy.FAMILIES['ordinary']['max_upload_bytes']
                packed = []

                def pack(staged, payload, maximum, **kwargs):
                    manifest = json.loads((staged / 'manifest.json').read_text())
                    packed.append(set(manifest['files']))
                    payload.write_bytes(b'mock archive')
                    return dict(archive_bytes=12)

                with patch.object(cache, 'ROOT', root), patch.object(cache, 'identity', return_value={'key': key()}), \
                     patch.object(cache, 'api_client', side_effect=[first, final]), \
                     patch.object(cache.archive, 'pack_archive', side_effect=pack), \
                     patch.object(cache.archive, 'upload_bound', side_effect=[{'upload_bound_bytes': limit * 2},
                                                                            {'upload_bound_bytes': limit}]), \
                     patch.dict(os.environ, environment(), clear=True):
                    report = {}
                    cache.stage_cache(args, 10_000_000_000, report)
                self.assertEqual(len(packed), 2)
                self.assertTrue(packed[1] < packed[0])
                self.assertEqual(report['save'], not inventory_growth, report)
                self.assertEqual((args.cache / cache.ARCHIVE_NAME).exists(), not inventory_growth)
                first.list_caches.assert_called_once_with()
                final.list_caches.assert_called_once_with()
                self.assertEqual(len(report['pack_attempts']), 2)

    def test_pack_retries_are_bounded_and_do_not_publish_an_oversized_archive(self):
        with tempfile.TemporaryDirectory() as directory:
            directory = Path(directory).resolve()
            root, state = self.make_state(directory, (100,) * 20)
            state_path = directory / 'state.json'
            state_path.write_text(json.dumps(state))
            args = SimpleNamespace(scope='ordinary', state=state_path, cache=directory / 'cache')
            limit = cache.policy.FAMILIES['ordinary']['max_upload_bytes']

            def pack(staged, payload, maximum, **kwargs):
                payload.write_bytes(b'mock archive')
                return dict(archive_bytes=12)

            with patch.object(cache, 'ROOT', root), patch.object(cache, 'identity', return_value={'key': key()}), \
                 patch.object(cache, 'api_client', return_value=api()), \
                 patch.object(cache.archive, 'pack_archive', side_effect=pack) as packing, \
                 patch.object(cache.archive, 'upload_bound', return_value={'upload_bound_bytes': limit + 1}), \
                 patch.dict(os.environ, environment(), clear=True):
                report = {}
                cache.stage_cache(args, 10_000_000_000, report)
            self.assertEqual(packing.call_count, 3)
            self.assertFalse(report['save'])
            self.assertEqual(report['reason'], 'ordinary single upload budget')
            self.assertFalse((args.cache / cache.ARCHIVE_NAME).exists())

    def test_formal_budget_has_consistent_generation_and_repository_limits(self):
        policy = cache.policy
        spec = policy.FAMILIES['formal']
        self.assertEqual((spec['max_upload_bytes'], spec['max_bytes'], spec['rotation_bytes']),
                         (1024 ** 3, 2 * 1024 ** 3, 3 * 1024 ** 3))
        observed = 817_681_480
        self.assertTrue(policy.save_policy([], key('formal'), observed, 20_000_000_000)['capacity_allowed'])
        retained = [entry(index, key=key('formal', f'{index:x}'), size_in_bytes=1024 ** 3) for index in (1, 2)]
        self.assertTrue(policy.save_policy(retained, key('formal'), 1024 ** 3, 20_000_000_000)['capacity_allowed'])
        self.assertEqual(policy.save_policy([], key('formal'), 1024 ** 3 + 1, 20_000_000_000)['reason'],
                         'formal single upload budget')
        crowded = [entry(key='other-families', size_in_bytes=20_000_000_000 - observed + 1)]
        self.assertEqual(policy.save_policy(crowded, key('formal'), observed, 20_000_000_000)['reason'],
                         'repository cache capacity gap')
        occupied = [entry(key='other-families', size_in_bytes=20_000_000_000 - observed)]
        self.assertEqual(policy.save_policy(occupied, key('formal'), observed, 20_000_000_000,
                         reserved_repository_bytes=policy.DEPENDENCY_ROTATION_HEADROOM_BYTES)['reason'],
                         'dependency rotation headroom')


@unittest.skipUnless(all(shutil.which(tool) for tool in ('cargo', 'rustc', 'zstd')),
                     'compact workspace smoke requires Cargo, rustc and zstd')
class RealCargoCompact(unittest.TestCase):
    def test_compatible_restore_is_fresh_and_changed_leaf_rebuilds(self):
        for scope in ('ordinary', 'formal'):
            with self.subTest(scope=scope), tempfile.TemporaryDirectory() as directory:
                base = Path(directory).resolve()
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


    def test_bounded_subset_rebuilds_omitted_units_and_changed_retained_source(self):
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory).resolve()
            root = base / 'checkout'
            sources = {
                'Cargo.toml': '[workspace]\nmembers=["a","b","app"]\nresolver="2"\n'
                              '[profile.dev]\ndebug=0\nincremental=false\n',
                'a/Cargo.toml': '[package]\nname="trim_a"\nversion="0.1.0"\nedition="2021"\n',
                'b/Cargo.toml': '[package]\nname="trim_b"\nversion="0.1.0"\nedition="2021"\n',
                'a/src/lib.rs': 'pub fn value() -> u32 { 7 }\n',
                'b/src/lib.rs': 'pub fn value() -> u32 { 8 }\n',
                'app/Cargo.toml': '[package]\nname="trim_app"\nversion="0.1.0"\nedition="2021"\n'
                                  '[dependencies]\ntrim_a={path="../a"}\ntrim_b={path="../b"}\n',
                'app/src/main.rs': 'fn main() { println!("{}", trim_a::value()+trim_b::value()); }\n',
            }
            for name, text in sources.items():
                path = root / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(text)
            env = dict(os.environ, CARGO_INCREMENTAL='0')
            for name in ('CARGO_TARGET_DIR', 'RUSTC_WRAPPER', 'RUSTC_WORKSPACE_WRAPPER'):
                env.pop(name, None)
            subprocess.run(['cargo', 'generate-lockfile', '--offline'], cwd=root, env=env,
                           check=True, capture_output=True)

            def build():
                result = subprocess.run(['cargo', 'build', '--workspace', '--offline', '--locked',
                                         '--message-format=json'], cwd=root, env=env, text=True, capture_output=True)
                self.assertEqual(result.returncode, 0, result.stderr)
                return [json.loads(line) for line in result.stdout.splitlines() if line.startswith('{')]

            state = cache.units.cache_begin(root, selection='libraries')
            for message in build():
                cache.units.cache_record(state, message)
            cache.units.cache_finish(state)
            measured = cache.units.cache_stage(state, base / 'unused', measure_only=True)
            budget = measured['raw_bytes'] - 1
            chosen = cache.bounded_library_state(state, budget, {})
            self.assertEqual(len(chosen['directories']), 1)
            kept = Path(chosen['directories'][0]).name.rsplit('-', 1)[0]
            staged = cache.units.cache_stage(chosen, base / 'saved', budget)
            self.assertTrue(staged['save'], staged)
            for changed in (False, True):
                shutil.rmtree(root / 'target')
                for name in (*sources, 'Cargo.lock'):
                    os.utime(root / name, None)
                if changed:
                    file = root / ('a' if kept == 'trim_a' else 'b') / 'src/lib.rs'
                    old = 7 if kept == 'trim_a' else 8
                    file.write_text(f'pub fn value() -> u32 {{ {old + 1} }}\n')
                    old_stamp = state['inputs'][file.relative_to(root).as_posix()]['mtime_ns']
                    os.utime(file, ns=(old_stamp, old_stamp))
                restored = cache.units.cache_restore(root, base / 'saved', budget)
                self.assertTrue(restored['restored'], restored)
                artifacts = [item for item in build() if item.get('reason') == 'compiler-artifact']
                for item in artifacts:
                    self.assertEqual(item['fresh'], item['target']['name'] == kept and not changed, item)
                value = subprocess.check_output([str(root / 'target/debug/trim_app')], text=True).strip()
                self.assertEqual(value, '16' if changed else '15')


if __name__ == '__main__':
    unittest.main()
