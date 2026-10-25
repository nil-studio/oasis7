#!/usr/bin/env python3
"""Exercise ECS policy independently from network and expensive native builds."""
import importlib.util
import gzip
import io
import json
import os
from pathlib import Path
import shutil
import subprocess
import tarfile
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import MagicMock, patch
from urllib.error import HTTPError

spec = importlib.util.spec_from_file_location('ecs', Path(__file__).with_name('ci-ecs-cache.py'))
ecs = importlib.util.module_from_spec(spec)
spec.loader.exec_module(ecs)
maintenance = ecs.load('ecs_test_maintenance', 'ci-cache-maintenance.py')
KEY = 'ecs-workspace-v2-' + 'a' * 64 + '-' + 'b' * 40


def entry(number=1, **changes):
    value = dict(id=number, ref='refs/heads/main', key=KEY[:-40] + f'{number:040x}',
                 version='v1', size_in_bytes=100, created_at='2026-10-21T01:00:00Z',
                 last_accessed_at='2026-10-21T01:00:00Z')
    value.update(changes)
    return value


def api_error(status=403, message='Resource not accessible by integration', headers=None):
    return maintenance.MaintenanceApiError('GET', HTTPError('secret-url', status, 'secret', headers or {},
        io.BytesIO(json.dumps({'message': message}).encode())))


def capacity_api(quota=None, entries=None):
    api = MagicMock()
    api.list_caches.return_value = entries if entries is not None else [entry()]
    if isinstance(quota, Exception):
        api.request.side_effect = quota
    else:
        api.request.return_value = quota if quota is not None else {'max_cache_size_gb': 10}
    return api


def writer_environment():
    return dict(GITHUB_ACTIONS='true', GITHUB_REF='refs/heads/main', GITHUB_JOB='native-trusted',
                GITHUB_EVENT_NAME='workflow_dispatch', GITHUB_SHA='b' * 40, GITHUB_WORKFLOW_SHA='b' * 40,
                GITHUB_REPOSITORY='nil-studio/oasis7', ACTIONS_CACHE_MODE='write',
                GITHUB_WORKFLOW_REF='nil-studio/oasis7/.github/workflows/ecs-packages.yml@refs/heads/main')


def fixture(root):
    checkout = root / 'checkout'
    (checkout / 'src').mkdir(parents=True)
    (checkout / 'Cargo.toml').write_text('[package]\nname="tiny"\nversion="0.1.0"\n')
    (checkout / 'src' / 'lib.rs').write_text('pub fn tiny() {}\n')
    target = checkout / 'target'
    unit = target / 'ecs' / '.fingerprint' / 'tiny-1234567890abcdef'
    unit.mkdir(parents=True)
    (unit / 'lib-tiny').write_text('fingerprint')
    library = target / 'ecs' / 'deps' / 'libtiny-1234567890abcdef.rlib'
    library.parent.mkdir(parents=True)
    library.write_bytes(b'complete unit bytes\n' * 100_000)
    state = ecs.units.cache_begin(checkout)
    state.update(packages={'tiny': str(checkout / 'Cargo.toml')},
                 files=[library.relative_to(target).as_posix()],
                 directories=[unit.relative_to(target).as_posix()])
    ecs.units.cache_finish(state)
    state_path = root / 'state.json'
    state_path.write_text(json.dumps(state))
    staged = root / 'staged'
    result = ecs.units.cache_stage(state, staged, ecs.STAGING_BYTES)
    assert result['save'], result
    cache = root / 'cache'
    cache.mkdir()
    ecs.pack_archive(staged, cache / ecs.ARCHIVE_NAME)
    return checkout, state_path, staged, cache


class EcsCacheTests(unittest.TestCase):
    def test_shared_policy_and_archive_changes_invalidate_restore_identity(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            files = ('Cargo.toml', 'Cargo.lock', '.github/workflows/ecs-packages.yml',
                     'scripts/ci-ecs-cache.py', 'scripts/package-ecs.py', 'scripts/ci-test-artifacts.py',
                     'scripts/ci-cache-policy.py', 'scripts/ci-cache-archive.py')
            for name in files:
                path = root / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text('original')

            def command_output(command, **kwargs):
                if command[:2] == ['cargo', 'metadata']:
                    return json.dumps({'packages': [{'manifest_path': str(root / 'Cargo.toml')}]})
                if command[0] in ('rustc', 'cargo'):
                    return command[0] + ' fixed version'
                if command[:2] == ['git', 'rev-parse']:
                    return 'b' * 40
                raise AssertionError(command)

            with patch.object(ecs, 'ROOT', root), patch.dict(os.environ, {}, clear=True), \
                    patch.object(ecs.subprocess, 'check_output', side_effect=command_output):
                baseline = ecs.identity()
                for helper in ('scripts/ci-cache-policy.py', 'scripts/ci-cache-archive.py'):
                    with self.subTest(helper=helper):
                        (root / helper).write_text('changed')
                        changed = ecs.identity()
                        self.assertNotEqual(changed['prefix'], baseline['prefix'])
                        self.assertTrue(changed['key'].endswith('-' + 'b' * 40))
                        (root / helper).write_text('original')

    def test_budget_generation_and_main_scope(self):
        self.assertTrue(ecs.save_allowed([entry()], KEY, 100, repository_limit_bytes=100 * 1024 ** 3))
        policy = ecs.save_policy([entry(1), entry(2)], KEY, 100, 100 * 1024 ** 3)
        self.assertTrue(policy['capacity_allowed'])
        self.assertTrue(policy['rotation_required'])
        self.assertEqual(policy['compatible_cache_ids'], [1, 2])
        # Rotation is post-save maintenance, never a reason to delete a recovery
        # point before the replacement has been uploaded and confirmed.
        self.assertFalse(ecs.save_allowed([entry(1), entry(2)], KEY, 101,
                                         repository_limit_bytes=300))
        self.assertTrue(ecs.save_allowed([entry(size_in_bytes=ecs.FAMILY_BYTES)], KEY, 1,
                                        repository_limit_bytes=100 * 1024 ** 3))
        self.assertFalse(ecs.save_allowed([entry(size_in_bytes=ecs.policy.FAMILIES['ecs']['rotation_bytes'])],
                                         KEY, 1, repository_limit_bytes=100 * 1024 ** 3))
        self.assertFalse(ecs.save_allowed([entry(key=KEY)], KEY, 1, repository_limit_bytes=100 * 1024 ** 3))
        self.assertTrue(ecs.save_allowed([entry(ref='refs/heads/topic', size_in_bytes=ecs.FAMILY_BYTES),
            entry(2, key='formal-workspace-unrelated', size_in_bytes=ecs.FAMILY_BYTES)], KEY, 1,
            repository_limit_bytes=100 * 1024 ** 3))
        # The old ECS format still occupies the same bounded remote family.
        self.assertTrue(ecs.save_allowed([entry(key=KEY.replace('v2-', 'v1-'), size_in_bytes=ecs.FAMILY_BYTES)],
                                        KEY, 1, repository_limit_bytes=100 * 1024 ** 3))
        self.assertFalse(ecs.save_allowed([entry(key=KEY.replace('v2-', 'v1-'),
            size_in_bytes=ecs.policy.FAMILIES['ecs']['migration_bytes'])],
            KEY, 1, repository_limit_bytes=100 * 1024 ** 3))

    def test_unrelated_caches_exhaust_repository_quota(self):
        unrelated = entry(ref='refs/heads/topic', key='formal-workspace-unrelated', size_in_bytes=999)
        self.assertFalse(ecs.save_allowed([unrelated], KEY, 2, repository_limit_bytes=1000))
        policy = ecs.save_policy([unrelated], KEY, 2, 1000)
        self.assertEqual(policy['reason'], 'repository cache capacity gap')
        self.assertEqual(policy['observed_repository_bytes'], 999)
        self.assertEqual(policy['projected_repository_bytes'], 1001)
        self.assertTrue(ecs.save_allowed([unrelated], KEY, 1, repository_limit_bytes=1000))

    def test_live_quota_is_positive_integer_decimal_gb(self):
        self.assertEqual(ecs.quota_bytes({'max_cache_size_gb': 10}), 10_000_000_000)
        for value in (None, {}, [], {'max_cache_size_gb': True}, {'max_cache_size_gb': 0},
                      {'max_cache_size_gb': -1}, {'max_cache_size_gb': '10'}, {'max_cache_size_gb': 1.5}):
            with self.subTest(value=value), self.assertRaises(ValueError):
                ecs.quota_bytes(value)
        self.assertFalse(ecs.save_allowed([], KEY, 1))

    def test_verified_budget_fallback_is_explicit_and_live_lower_limit_wins(self):
        report = {}
        ecs.capacity_context(maintenance, capacity_api(api_error()), 10_000_000_000, report)
        self.assertEqual(report['capacity_basis'], 'verified_configured_budget')
        self.assertIsNone(report['live_repository_limit_bytes'])
        self.assertEqual(report['effective_repository_budget_bytes'], 10_000_000_000)
        self.assertEqual(report['quota_api_diagnostics']['category'], 'integration_access')
        for live, budget, expected in [(1, 10_000_000_000, 1_000_000_000),
                                       (100, 10_000_000_000, 10_000_000_000),
                                       (10, None, 10_000_000_000)]:
            ecs.capacity_context(maintenance, capacity_api({'max_cache_size_gb': live}), budget, report)
            self.assertEqual(report['effective_repository_budget_bytes'], expected)
            self.assertNotIn('quota_api_diagnostics', report)

    def test_missing_budget_invalid_quota_or_rate_limits_never_fallback(self):
        errors = [api_error(), api_error(404), api_error(429), api_error(500),
                  api_error(message='unknown denial'), api_error(headers={'Retry-After': '0'}),
                  api_error(headers={'X-RateLimit-Remaining': '0'}),
                  api_error(message='You have exceeded a secondary rate limit'),
                  {}, {'max_cache_size_gb': True}, ValueError('timeout')]
        for error in errors:
            budget = None if error is errors[0] else 10_000_000_000
            with self.subTest(error=error), self.assertRaises(ValueError):
                ecs.capacity_context(maintenance, capacity_api(error), budget, {})
        for value in ('0', '-1', '10.0', '10GB', ' 10', '1\n', True):
            with self.subTest(value=value), self.assertRaises(ValueError):
                ecs.configured_budget(value)

    def test_inventory_failure_prevents_quota_fallback(self):
        for entries in ([entry(), entry()], [entry(size_in_bytes=-1)]):
            api = capacity_api(api_error(), entries)
            with self.subTest(entries=entries), self.assertRaises(ValueError):
                ecs.capacity_context(maintenance, api, 10_000_000_000, {})
            api.request.assert_not_called()

    def test_preflight_failure_keeps_inventory_and_diagnostics(self):
        with tempfile.TemporaryDirectory() as directory:
            report = Path(directory) / 'report.json'
            with patch.object(ecs, 'api_client', return_value=(maintenance, capacity_api(api_error()))), \
                 patch.object(ecs.sys, 'argv', ['cache', 'preflight', '--repository-budget-bytes', '', '--report', str(report)]):
                self.assertEqual(ecs.main(), 0)
            result = json.loads(report.read_text())
            self.assertFalse(result['save'])
            self.assertTrue(result['inventory_complete'])
            self.assertIsNone(result['effective_repository_budget_bytes'])
            self.assertEqual(result['api_diagnostics']['http_status'], 403)

    def test_only_current_trusted_main_writer_can_stage(self):
        with patch.dict(os.environ, writer_environment(), clear=True):
            ecs.trusted_writer(KEY)
        for changes in ({'GITHUB_REF': 'refs/pull/1/merge'}, {'GITHUB_WORKFLOW_SHA': 'c' * 40},
                        {'GITHUB_EVENT_NAME': 'pull_request'}, {'GITHUB_JOB': 'native'},
                        {'ACTIONS_CACHE_MODE': 'read'}, {'GITHUB_ACTIONS': 'false'}):
            with self.subTest(changes=changes), patch.dict(os.environ, dict(writer_environment(), **changes), clear=True), \
                 self.assertRaises(ValueError):
                ecs.trusted_writer(KEY)

    def test_observe_save_requires_one_nonempty_exact_main_entry(self):
        for entries, confirmed in [([], False), ([entry(key=KEY, size_in_bytes=0)], False),
                ([entry(key=KEY), entry(2, key=KEY, version='other')], False),
                ([entry(key=KEY), entry(2, key=KEY, ref='refs/pull/2/merge')], True)]:
            with self.subTest(entries=entries), patch.object(ecs, 'api_client',
                    return_value=(maintenance, capacity_api(entries=entries))):
                report = {}
                ecs.observe_save(KEY, report)
                self.assertEqual(report['saved_confirmed'], confirmed)
                if confirmed:
                    self.assertEqual(report['cache_id'], 1)
                    self.assertEqual(report['remote_compressed_bytes'], 100)

    def test_corrupt_cache_falls_back(self):
        with tempfile.TemporaryDirectory() as checkout, tempfile.TemporaryDirectory() as cache:
            root = Path(checkout)
            (root / 'Cargo.toml').write_text('not a real crate')
            (Path(cache) / 'manifest.json').write_text('{invalid')
            report = ecs.units.cache_restore(root, Path(cache), ecs.STAGING_BYTES)
            self.assertFalse(report['restored'])
            self.assertFalse((root / 'target').exists())

    def test_record_failure_still_runs_cargo(self):
        with tempfile.TemporaryDirectory() as tmp:
            state = Path(tmp) / 'state.json'
            with patch.object(ecs.units, 'cache_begin', side_effect=ValueError('unsafe input')):
                ecs.build(['python3', '-c', 'print(\'{"reason":"build-finished","success":true}\')'], {}, state)
            self.assertEqual(json.loads(state.read_text()), {})
            self.assertEqual(json.loads(state.with_name('ecs-units.json').read_text())['rebuilt'], 0)

    def test_build_failure_is_never_hidden(self):
        with tempfile.TemporaryDirectory() as tmp:
            with patch.object(ecs.units, 'cache_begin', side_effect=ValueError('unsafe input')):
                with self.assertRaises(subprocess.CalledProcessError):
                    ecs.build(['python3', '-c', 'raise SystemExit(7)'], {}, Path(tmp) / 'state.json')

    def test_reports_cannot_be_source_inputs(self):
        with self.assertRaises(ValueError):
            ecs.temporary(ecs.ROOT / 'output' / 'cache-state.json')

    def test_untracked_inputs_and_path_changes_invalidate_collection(self):
        for change in ('add', 'remove', 'rename', 'content'):
            with self.subTest(change=change), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                (root / '.gitignore').write_text('ignored.txt\n')
                original = root / 'ignored.txt'
                original.write_text('before')
                state = ecs.units.cache_begin(root)
                self.assertIn('ignored.txt', state['inputs'])
                if change == 'add':
                    (root / 'untracked.txt').write_text('new')
                elif change == 'remove':
                    original.unlink()
                elif change == 'rename':
                    original.rename(root / 'renamed.txt')
                else:
                    original.write_text('after')
                with self.assertRaisesRegex(ValueError, 'source inputs changed'):
                    ecs.units.cache_finish(state)


class EcsArchiveTests(unittest.TestCase):
    def test_complete_unit_roundtrip_preserves_freshness_inputs_and_records_matched_key(self):
        with tempfile.TemporaryDirectory() as directory:
            checkout, _, staged, cache = fixture(Path(directory))
            inputs = ecs.units._cache_inputs(checkout)
            shutil.rmtree(checkout / 'target')
            source = checkout / 'src' / 'lib.rs'
            source.write_text('pub fn changed() {}\n')
            args = SimpleNamespace(cache=cache, requested_key=KEY,
                                   matched_key=KEY[:-40] + 'c' * 40, cache_hit='false')
            with patch.object(ecs, 'ROOT', checkout):
                report = {}
                ecs.restore_cache(args, report)
            self.assertTrue(report['restored'])
            self.assertFalse(report['action_cache_hit'])
            self.assertEqual(report['changed_inputs'], 1)
            self.assertEqual(report['matched_key'], args.matched_key)
            self.assertEqual((checkout / 'Cargo.toml').stat().st_mtime_ns, inputs['Cargo.toml']['mtime_ns'])
            self.assertGreater(source.stat().st_mtime_ns, inputs['src/lib.rs']['mtime_ns'])
            self.assertEqual(ecs.units.digest(checkout / 'target/ecs/deps/libtiny-1234567890abcdef.rlib'),
                             ecs.units.digest(staged / 'target/ecs/deps/libtiny-1234567890abcdef.rlib'))

    def test_trusted_stage_uses_packed_bound_and_rechecks_capacity(self):
        for remaining, allowed in [(10_000_000_000, True), (1, False)]:
            with self.subTest(remaining=remaining), tempfile.TemporaryDirectory() as directory:
                checkout, state, _, cache = fixture(Path(directory))
                first = capacity_api(api_error())
                second = capacity_api(api_error(), [entry(size_in_bytes=10_000_000_000 - remaining)])
                args = SimpleNamespace(cache=cache, state=state)
                with patch.object(ecs, 'ROOT', checkout), patch.object(ecs, 'identity', return_value={'key': KEY}), \
                     patch.object(ecs, 'api_client', side_effect=[(maintenance, first), (maintenance, second)]), \
                     patch.dict(os.environ, writer_environment()):
                    report = {'save': False}
                    ecs.stage_cache(args, 10_000_000_000, report)
                self.assertEqual(report['save'], allowed)
                first.list_caches.assert_called_once()
                second.list_caches.assert_called_once()
                self.assertLess(report['upload_bound_bytes'], report['raw_bytes'])
                self.assertGreater(report['upload_bound_bytes'], report['archive_bytes'])
                self.assertEqual((cache / ecs.ARCHIVE_NAME).exists(), allowed)

    def test_candidate_preview_measures_archive_but_never_enables_save(self):
        with tempfile.TemporaryDirectory() as directory:
            checkout, state, _, cache = fixture(Path(directory))
            args = SimpleNamespace(cache=cache, state=state, preview=True)
            api = capacity_api(api_error())
            with patch.object(ecs, 'ROOT', checkout), patch.object(ecs, 'identity', return_value={'key': KEY}), \
                 patch.object(ecs, 'api_client', return_value=(maintenance, api)), \
                 patch.dict(os.environ, dict(writer_environment(), GITHUB_REF='refs/pull/1/merge',
                                            GITHUB_JOB='native', ACTIONS_CACHE_MODE='read')):
                report = {'save': False}
                ecs.stage_cache(args, 10_000_000_000, report)
            self.assertTrue(report['preview'])
            self.assertTrue(report['would_save'])
            self.assertFalse(report['save'])
            self.assertTrue((cache / ecs.ARCHIVE_NAME).is_file())
            api.list_caches.assert_called_once()
            api.delete_cache.assert_not_called()

    def test_packing_failure_does_not_publish_a_partial_archive(self):
        with tempfile.TemporaryDirectory() as directory:
            _, _, staged, cache = fixture(Path(directory))
            archive = cache / ecs.ARCHIVE_NAME
            archive.unlink()
            payload = staged / 'target/ecs/deps/libtiny-1234567890abcdef.rlib'
            payload.write_bytes(b'truncated')
            with self.assertRaises(OSError):
                ecs.pack_archive(staged, archive)
            self.assertEqual(list(cache.iterdir()), [])
            self.assertEqual(list(Path(directory).glob('ecs-pack-*')), [])

    @unittest.skipUnless(shutil.which('zstdmt'), 'actual action wrapper comparison requires zstdmt')
    def test_upload_bound_covers_actual_zstdmt_wrapper_with_incompressible_data_and_long_path(self):
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory)
            workspace = base / 'checkout'
            workspace.mkdir()
            archive = base / ('long-' + 'a' * 100) / ('long-' + 'b' * 100) / ecs.ARCHIVE_NAME
            archive.parent.mkdir(parents=True)
            archive.write_bytes(gzip.compress(os.urandom(256 * 1024), compresslevel=1, mtime=0))
            with patch.object(ecs, 'ROOT', workspace), \
                 patch.dict(os.environ, GITHUB_WORKSPACE=str(workspace)):
                bound = ecs.upload_bound(archive)
            manifest, compressed = base / 'manifest.txt', base / 'cache.tzst'
            manifest.write_text(os.path.relpath(archive, workspace) + '\n')
            subprocess.run(['tar', '--posix', '-cf', str(compressed), '--exclude', 'cache.tzst', '-P',
                            '-C', str(workspace), '--files-from', str(manifest),
                            '--use-compress-program', 'zstdmt'], check=True, capture_output=True)
            self.assertGreater(compressed.stat().st_size, 0)
            self.assertLessEqual(compressed.stat().st_size, bound['upload_bound_bytes'])

    def test_unsafe_headers_are_rejected_before_payload_or_target_write(self):
        for kind, name in [(tarfile.SYMTYPE, 'files/000000'), (tarfile.LNKTYPE, 'files/000000'),
                           (tarfile.CHRTYPE, 'files/000000'), (tarfile.GNUTYPE_SPARSE, 'files/000000'),
                           (tarfile.XHDTYPE, 'files/000000'), (tarfile.GNUTYPE_LONGNAME, 'files/000000'),
                           (tarfile.REGTYPE, '../escape'), (tarfile.REGTYPE, '/absolute'),
                           (tarfile.REGTYPE, 'manifest.json')]:
            with self.subTest(kind=kind, name=name), tempfile.TemporaryDirectory() as directory:
                checkout, _, staged, cache = fixture(Path(directory))
                metadata = (staged / 'manifest.json').read_bytes()
                with gzip.open(cache / ecs.ARCHIVE_NAME, 'wb') as stream:
                    manifest_header = tarfile.TarInfo('manifest.json')
                    manifest_header.size = len(metadata)
                    stream.write(manifest_header.tobuf(tarfile.USTAR_FORMAT))
                    stream.write(metadata + b'\0' * (-len(metadata) % 512))
                    header = tarfile.TarInfo(name)
                    header.type, header.size = kind, 1024 ** 3
                    stream.write(header.tobuf(tarfile.USTAR_FORMAT))
                shutil.rmtree(checkout / 'target')
                output = Path(directory) / 'unpacked'
                output.mkdir()
                with self.assertRaises(ValueError):
                    ecs.unpack_archive(cache / ecs.ARCHIVE_NAME, output)
                self.assertFalse((checkout / 'target').exists())
                self.assertFalse((Path(directory) / 'escape').exists())

    def test_manifest_size_path_and_logical_bombs_are_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            _, _, staged, _ = fixture(Path(directory))
            manifest = json.loads((staged / 'manifest.json').read_text())
            sample = next(iter(manifest['files'].values()))
            for files in ({'../escape': sample}, {'/absolute': sample},
                          {'safe': dict(sample, bytes=ecs.STAGING_BYTES + 1)},
                          {'safe': dict(sample, mode=0o4777)}):
                with self.subTest(files=files), self.assertRaises(ValueError):
                    ecs.archive_manifest(json.dumps(dict(manifest, files=files)).encode())
            with self.assertRaises(ValueError):
                ecs.archive_manifest(b'{"files":{},"files":{}}')
            with self.assertRaises(ValueError):
                ecs.archive_manifest(b'[' * 2000 + b']' * 2000)

    def test_truncation_extra_members_and_corruption_do_not_restore_target(self):
        for damage in ('truncated', 'extra', 'corrupt'):
            with self.subTest(damage=damage), tempfile.TemporaryDirectory() as directory:
                checkout, _, _, cache = fixture(Path(directory))
                archive = cache / ecs.ARCHIVE_NAME
                value = archive.read_bytes()
                if damage == 'truncated':
                    value = value[:-8]
                elif damage == 'extra':
                    value += gzip.compress(b'not part of the bounded archive')
                else:
                    value = value[:-8] + b'bad-crc!'
                archive.write_bytes(value)
                shutil.rmtree(checkout / 'target')
                args = SimpleNamespace(cache=cache, requested_key=KEY, matched_key=KEY, cache_hit='true')
                with patch.object(ecs, 'ROOT', checkout), self.assertRaises((ValueError, OSError, EOFError)):
                    ecs.restore_cache(args, {})
                self.assertFalse((checkout / 'target').exists())

    def test_source_path_changes_and_payload_hash_mismatch_are_cache_misses(self):
        for change in ('new-source', 'payload'):
            with self.subTest(change=change), tempfile.TemporaryDirectory() as directory:
                checkout, _, staged, cache = fixture(Path(directory))
                if change == 'new-source':
                    (checkout / 'src' / 'new.rs').write_text('// newly tracked input')
                else:
                    payload = staged / 'target/ecs/deps/libtiny-1234567890abcdef.rlib'
                    data = payload.read_bytes()
                    payload.write_bytes(b'X' + data[1:])
                    ecs.pack_archive(staged, cache / ecs.ARCHIVE_NAME)
                shutil.rmtree(checkout / 'target')
                args = SimpleNamespace(cache=cache, requested_key=KEY, matched_key=KEY, cache_hit='true')
                with patch.object(ecs, 'ROOT', checkout):
                    report = {}
                    ecs.restore_cache(args, report)
                self.assertFalse(report['restored'])
                self.assertFalse((checkout / 'target').exists())


@unittest.skipUnless(shutil.which('cargo') and shutil.which('rustc'), 'real Cargo cache smoke requires cargo and rustc')
class RealCargoArchiveTests(unittest.TestCase):
    def test_archive_roundtrip_is_fresh_and_leaf_edit_rebuilds_only_its_dependency_cone(self):
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory)
            checkout, target, cache = base / 'checkout', base / 'checkout/target', base / 'cache'
            sources = {
                'Cargo.toml': '[workspace]\nmembers=["base", "leaf"]\nresolver="2"\n'
                              '[profile.ecs]\ninherits="dev"\ndebug=0\nincremental=false\n',
                'base/Cargo.toml': '[package]\nname="ecs_cache_base"\nversion="0.1.0"\nedition="2021"\n',
                'base/src/lib.rs': 'pub fn value() -> u32 { 7 }\n',
                'leaf/Cargo.toml': '[package]\nname="ecs_cache_leaf"\nversion="0.1.0"\nedition="2021"\n'
                                   '[dependencies]\necs_cache_base={path="../base"}\n'
                                   '[[bin]]\nname="ecs_cache_app"\npath="src/main.rs"\n',
                'leaf/src/lib.rs': 'pub fn value() -> u32 { ecs_cache_base::value() }\n',
                'leaf/src/main.rs': 'fn main() { println!("{}", ecs_cache_leaf::value()); }\n',
            }
            for name, content in sources.items():
                path = checkout / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(content)
            env = dict(os.environ, CARGO_INCREMENTAL='0')
            for name in ('CARGO_TARGET_DIR', 'RUSTC_WRAPPER', 'RUSTC_WORKSPACE_WRAPPER'):
                env.pop(name, None)
            subprocess.run(['cargo', 'generate-lockfile', '--offline'], cwd=checkout,
                           env=env, check=True, capture_output=True)
            command = ['cargo', 'build', '--workspace', '--offline', '--locked', '--profile', 'ecs',
                       '--target-dir', str(target)]
            state = base / 'producer/state.json'
            with patch.object(ecs, 'ROOT', checkout):
                ecs.build(command, env, state)
                producer = json.loads(state.with_name('ecs-units.json').read_text())
                self.assertEqual((producer['fresh'], producer['rebuilt']), (0, 3))
                with patch.object(ecs, 'identity', return_value={'key': KEY}), \
                     patch.object(ecs, 'api_client', return_value=(maintenance, capacity_api())):
                    staged = {'save': False}
                    ecs.stage_cache(SimpleNamespace(cache=cache, state=state, preview=True),
                                    10_000_000_000, staged)
                self.assertTrue(staged['would_save'], staged)
                self.assertFalse(staged['save'])
                self.assertTrue((cache / ecs.ARCHIVE_NAME).is_file())
                for changed in (False, True):
                    with self.subTest(leaf_source_changed=changed):
                        shutil.rmtree(target)
                        # A clean consumer checkout has new mtimes. The overlay
                        # must restore originals only for byte-identical inputs.
                        for name in [*sources, 'Cargo.lock']:
                            os.utime(checkout / name, None)
                        if changed:
                            leaf = checkout / 'leaf/src/lib.rs'
                            leaf.write_text(leaf.read_text() + '// touched source\n')
                        restored = {}
                        ecs.restore_cache(SimpleNamespace(cache=cache, requested_key=KEY,
                            matched_key=KEY, cache_hit='true'), restored)
                        self.assertTrue(restored['restored'], restored)
                        self.assertEqual(restored['changed_inputs'], int(changed))
                        consumer_state = base / ('edited' if changed else 'consumer') / 'state.json'
                        ecs.build(command, env, consumer_state)
                        consumer = json.loads(consumer_state.with_name('ecs-units.json').read_text())
                        fresh = {unit['target']['name']: unit['fresh'] for unit in consumer['units']}
                        self.assertEqual(fresh, {'ecs_cache_base': True, 'ecs_cache_leaf': not changed,
                                                'ecs_cache_app': not changed})
                        self.assertTrue(json.loads(consumer_state.read_text())['complete'])
                        if not changed:
                            # Cargo recreates the public/deps executable hardlink
                            # on a Fresh build, so the consumer can be a producer.
                            with patch.object(ecs, 'identity', return_value={'key': KEY}), \
                                 patch.object(ecs, 'api_client', return_value=(maintenance, capacity_api())):
                                restaged = {'save': False}
                                ecs.stage_cache(SimpleNamespace(cache=base / 'consumer-cache',
                                    state=consumer_state, preview=True), 10_000_000_000, restaged)
                            self.assertTrue(restaged['would_save'], restaged)


if __name__ == '__main__':
    unittest.main()
