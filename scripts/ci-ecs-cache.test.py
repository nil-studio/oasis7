#!/usr/bin/env python3
"""Exercise ECS policy independently from network and expensive native builds."""
import importlib.util
import json
from pathlib import Path
import subprocess
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import MagicMock, patch

spec = importlib.util.spec_from_file_location('ecs', Path(__file__).with_name('ci-ecs-cache.py'))
ecs = importlib.util.module_from_spec(spec)
spec.loader.exec_module(ecs)


class EcsCacheTests(unittest.TestCase):
    def test_budget_generation_and_main_scope(self):
        key = 'ecs-workspace-v1-' + 'a' * 64 + '-' + 'b' * 40
        entry = dict(ref='refs/heads/main', key=key[:-40] + 'c' * 40, size_in_bytes=100)
        self.assertTrue(ecs.save_allowed([entry], key, 100, repository_limit_bytes=100 * 1024 ** 3))
        self.assertFalse(ecs.save_allowed([entry, dict(entry, key=key[:-40] + 'd' * 40)], key, 100, repository_limit_bytes=100 * 1024 ** 3))
        self.assertFalse(ecs.save_allowed([dict(entry, size_in_bytes=ecs.FAMILY_BYTES)], key, 1, repository_limit_bytes=100 * 1024 ** 3))
        self.assertFalse(ecs.save_allowed([dict(entry, key=key)], key, 1, repository_limit_bytes=100 * 1024 ** 3))
        self.assertTrue(ecs.save_allowed([dict(entry, ref='refs/heads/topic', size_in_bytes=ecs.FAMILY_BYTES),
                                                dict(entry, key='formal-workspace-unrelated', size_in_bytes=ecs.FAMILY_BYTES)], key, 1, repository_limit_bytes=100 * 1024 ** 3))

    def test_unrelated_caches_exhaust_repository_quota(self):
        key = 'ecs-workspace-v1-' + 'a' * 64 + '-' + 'b' * 40
        unrelated = dict(ref='refs/heads/topic', key='formal-workspace-unrelated', size_in_bytes=999)
        self.assertFalse(ecs.save_allowed([unrelated], key, 2, repository_limit_bytes=1000))
        policy = ecs.save_policy([unrelated], key, 2, 1000)
        self.assertEqual(policy['reason'], 'repository cache capacity gap')
        self.assertEqual(policy['observed_repository_bytes'], 999)
        self.assertEqual(policy['projected_repository_bytes'], 1001)
        self.assertTrue(ecs.save_allowed([unrelated], key, 1, repository_limit_bytes=1000))

    def test_live_quota_is_positive_integer_decimal_gb(self):
        self.assertEqual(ecs.quota_bytes({'max_cache_size_gb': 10}), 10_000_000_000)
        for value in (None, {}, [], {'max_cache_size_gb': True}, {'max_cache_size_gb': 0},
                      {'max_cache_size_gb': -1}, {'max_cache_size_gb': '10'}, {'max_cache_size_gb': 1.5}):
            with self.subTest(value=value), self.assertRaises(ValueError):
                ecs.quota_bytes(value)
        self.assertFalse(ecs.save_allowed([], 'key', 1))

    def test_stage_reads_live_quota_and_preserves_capacity_report(self):
        for quota in ({}, {'max_cache_size_gb': 1}, ValueError('cache quota service unavailable')):
            with self.subTest(quota=quota), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                state, report = root / 'state.json', root / 'report.json'
                state.write_text('{}')
                api = MagicMock()
                api.list_caches.return_value = [dict(ref='refs/heads/topic', key='unrelated',
                                                   size_in_bytes=999_999_999)]
                if isinstance(quota, Exception):
                    api.request.side_effect = quota
                else:
                    api.request.return_value = quota
                maintenance = SimpleNamespace(GitHubApi=MagicMock(return_value=api), validate_inventory=lambda entries: entries)
                with patch.object(ecs, 'load', return_value=maintenance), patch.object(ecs, 'identity', return_value={'key': 'key'}), \
                     patch.object(ecs.units, 'cache_stage', return_value={'eligible': True, 'raw_bytes': 2}) as stage, \
                     patch.object(ecs.sys, 'argv', ['cache', 'stage', '--cache', str(root / 'cache'),
                                                   '--state', str(state), '--report', str(report)]):
                    self.assertEqual(ecs.main(), 0)
                api.request.assert_called_once_with('GET', '/actions/cache/storage-limit')
                self.assertEqual(stage.call_count, 1)  # measure-only; no payload staging
                result = json.loads(report.read_text())
                self.assertFalse(result['save'])
                self.assertEqual(result['observed_repository_bytes'], 999_999_999)
                self.assertEqual(result['projected_repository_bytes'], 1_000_000_001)
                self.assertEqual(result['repository_limit_bytes'],
                                 1_000_000_000 if isinstance(quota, dict) and quota else None)

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


if __name__ == '__main__':
    unittest.main()
