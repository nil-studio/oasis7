#!/usr/bin/env python3
"""Workspace cache boundaries without GitHub credentials or a native Rust build."""
import importlib.util
import json
import os
from pathlib import Path
import shutil
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
            self.assertFalse((args.cache / cache.archive.ARCHIVE_NAME).exists())

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


if __name__ == '__main__':
    unittest.main()
