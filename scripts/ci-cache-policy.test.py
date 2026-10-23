#!/usr/bin/env python3
"""Exercise shared workspace capacity decisions without remote mutations."""
import importlib.util
import io
import json
from pathlib import Path
import unittest
from unittest.mock import Mock
from urllib.error import HTTPError


def load(name, filename):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(filename))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


policy = load('workspace_policy', 'ci-cache-policy.py')
maintenance = load('workspace_maintenance', 'ci-cache-maintenance.py')
PREFIXES = {'formal': 'formal-workspace-v3', 'ordinary': 'ordinary-workspace-v1', 'ecs': 'ecs-workspace-v2'}


def key(name, number, compatibility='a'):
    return f'{PREFIXES[name]}-{compatibility * 64}-{number:040x}'


def entry(name, number, **changes):
    result = dict(id=number, ref='refs/heads/main', key=key(name, number), version='v1',
                  size_in_bytes=100, created_at='2026-10-21T01:00:00Z', last_accessed_at='2026-10-21T01:00:00Z')
    result.update(changes)
    return result


def capacity_api(quota, entries):
    api = Mock()
    api.list_caches.return_value = entries
    if isinstance(quota, Exception):
        api.request.side_effect = quota
    else:
        api.request.return_value = quota
    return api


def denial(message='Resource not accessible by integration', status=403, headers=None):
    error = HTTPError('https://api.github.com', status, 'failure', headers or {},
                      io.BytesIO(json.dumps({'message': message}).encode()))
    return maintenance.MaintenanceApiError('GET', error)


class SchemaTests(unittest.TestCase):
    def test_current_and_managed_historical_schemas_are_distinct(self):
        legacy = {
            'formal': 'formal-workspace-v2-ubuntu24-X64-1.96.0-nightly-2025-12-11-' + 'a' * 64 + '-' + 'b' * 40,
            'ecs': key('ecs', 1).replace('-v2-', '-v1-'),
        }
        for name in PREFIXES:
            with self.subTest(name=name):
                self.assertEqual(policy.family_for_key(key(name, 1)), name)
                self.assertTrue(policy.is_current_key(key(name, 1)))
        for name, old_key in legacy.items():
            with self.subTest(name=name):
                self.assertEqual(policy.family_for_key(old_key), name)
                self.assertFalse(policy.is_current_key(old_key))
                with self.assertRaises(ValueError):
                    policy.save_policy([], old_key, 100, 10_000_000_000)
        for invalid in (None, [], '', key('formal', 1) + '-extra', key('ecs', 1) + '\n',
                        key('ordinary', 1).replace('a' * 64, 'a' * 63),
                        'formal-workspace-v2-lookalike-' + 'a' * 64 + '-' + 'b' * 40):
            with self.subTest(invalid=invalid):
                self.assertIsNone(policy.family_for_key(invalid))
                self.assertFalse(policy.is_current_key(invalid))

    def test_configured_decimal_bytes_and_live_decimal_gigabytes_remain_distinct(self):
        self.assertEqual(policy.configured_budget('10000000000'), 10_000_000_000)
        self.assertEqual(policy.quota_bytes({'max_cache_size_gb': 10}), 10_000_000_000)
        self.assertIsNone(policy.configured_budget(None))
        self.assertIsNone(policy.configured_budget(''))
        for value in ('0', '-1', '10GB', '10.0', ' 10', '10\n', True, 10):
            with self.subTest(value=value), self.assertRaises(ValueError):
                policy.configured_budget(value)
        for value in (None, {}, {'max_cache_size_gb': True}, {'max_cache_size_gb': '10'}, {'max_cache_size_gb': 0}):
            with self.subTest(value=value), self.assertRaises(ValueError):
                policy.quota_bytes(value)


class AdmissionTests(unittest.TestCase):
    def test_all_three_families_allow_temporary_third_only_with_full_capacity(self):
        for name in PREFIXES:
            entries = [entry(name, number) for number in (1, 2)]
            with self.subTest(name=name):
                result = policy.save_policy(entries, key(name, 3), 101, 301)
                self.assertTrue(result['capacity_allowed'])
                self.assertTrue(result['rotation_required'])
                self.assertEqual(result['compatible_cache_ids'], [1, 2])
                self.assertEqual(result['projected_repository_bytes'], 301)
                self.assertFalse(policy.save_policy(entries, key(name, 3), 101, 300)['capacity_allowed'])
                # Neither admission nor rejection evicts an existing recovery point.
                self.assertEqual(entries, [entry(name, number) for number in (1, 2)])

    def test_every_repository_ref_and_cache_family_counts_before_upload(self):
        entries = [entry('formal', 1, size_in_bytes=1000),
                   entry('ecs', 2, size_in_bytes=2000, ref='refs/pull/9/merge'),
                   entry('ordinary', 3, key='unknown-tools-cache', size_in_bytes=3000)]
        result = policy.save_policy(entries, key('ordinary', 4), 100, 6099)
        self.assertFalse(result['capacity_allowed'])
        self.assertEqual(result['observed_repository_bytes'], 6000)
        self.assertEqual(result['projected_family_bytes'], 100)
        self.assertTrue(policy.save_policy(entries, key('ordinary', 4), 100, 6100)['capacity_allowed'])

    def test_family_limits_include_historical_formats_but_not_other_refs(self):
        legacy = {
            'formal': 'formal-workspace-v2-ubuntu24-ARM64-1.96.0-nightly-2025-12-11-' + 'a' * 64 + '-' + 'b' * 40,
            'ecs': key('ecs', 1).replace('-v2-', '-v1-'),
            'ordinary': key('ordinary', 1),
        }
        for name in PREFIXES:
            limit = policy.FAMILIES[name]['max_bytes']
            entries = [entry(name, 1, key=legacy[name], size_in_bytes=limit - 100),
                       entry(name, 2, size_in_bytes=1000, ref='refs/pull/9/merge')]
            with self.subTest(name=name):
                self.assertFalse(policy.save_policy(entries, key(name, 3), 101, 100_000_000_000)['capacity_allowed'])
                result = policy.save_policy(entries, key(name, 3), 100, 100_000_000_000)
                self.assertTrue(result['capacity_allowed'])
                self.assertEqual(result['projected_family_bytes'], limit)
                self.assertEqual(result['family_limit_bytes'], limit)

    def test_exact_immutable_key_and_unknown_quota_are_never_saved(self):
        for name in PREFIXES:
            with self.subTest(name=name):
                result = policy.save_policy([entry(name, 1)], key(name, 1), 100, 10_000_000_000)
                self.assertFalse(result['capacity_allowed'])
                self.assertEqual(result['reason'], 'existing immutable key')
                for quota in (None, True, 0, -1, '100'):
                    self.assertFalse(policy.save_policy([], key(name, 2), 100, quota)['capacity_allowed'])
                for size in (True, 0, -1, 1.5):
                    with self.assertRaises(ValueError):
                        policy.save_policy([], key(name, 2), size, 10_000_000_000)


class CapacityEvidenceTests(unittest.TestCase):
    def test_complete_inventory_and_lower_live_quota_bound_configured_capacity(self):
        entries = [entry('formal', 1), entry('ecs', 2, ref='refs/pull/9/merge')]
        report = {}
        api = capacity_api({'max_cache_size_gb': 1}, entries)
        self.assertEqual(policy.capacity_context(maintenance, api, 10_000_000_000, report), entries)
        self.assertEqual(report['capacity_basis'], 'live_quota_and_configured_budget')
        self.assertEqual(report['effective_repository_budget_bytes'], 1_000_000_000)
        self.assertEqual(report['available_repository_bytes'], 1_000_000_000 - 200)
        self.assertTrue(report['inventory_complete'])
        self.assertEqual(report['observed_cache_count'], 2)
        api.request.assert_called_once_with('GET', '/actions/cache/storage-limit')

    def test_only_recognized_quota_access_denial_allows_explicit_configured_fallback(self):
        for message in ('Resource not accessible by integration', 'Resource not accessible by personal access token'):
            report = {}
            policy.capacity_context(maintenance, capacity_api(denial(message), []), 10_000_000_000, report)
            self.assertEqual(report['capacity_basis'], 'verified_configured_budget')
            self.assertIsNone(report['live_repository_limit_bytes'])
            self.assertEqual(report['effective_repository_budget_bytes'], 10_000_000_000)
        for failure in (denial('unknown denial'), denial(status=429), denial(status=404),
                        denial(headers={'Retry-After': '1'}), denial(headers={'X-RateLimit-Remaining': '0'}),
                        denial('API rate limit exceeded'), {'max_cache_size_gb': False}, ValueError('timeout')):
            with self.subTest(failure=failure), self.assertRaises(ValueError):
                policy.capacity_context(maintenance, capacity_api(failure, []), 10_000_000_000, {})
        with self.assertRaises(ValueError):
            policy.capacity_context(maintenance, capacity_api(denial(), []), None, {})

    def test_incomplete_or_invalid_inventory_cannot_fall_back_or_use_quota(self):
        for entries in ([entry('ecs', 1), entry('ecs', 1)], [entry('formal', 1, size_in_bytes=-1)],
                        [entry('ordinary', 1, version='')], maintenance.MaintenanceError('incomplete pagination')):
            api = capacity_api(denial(), entries)
            if isinstance(entries, Exception):
                api.list_caches.side_effect = entries
            report = {}
            with self.subTest(entries=entries), self.assertRaises(ValueError):
                policy.capacity_context(maintenance, api, 10_000_000_000, report)
            self.assertFalse(report['inventory_complete'])
            self.assertIsNone(report['effective_repository_budget_bytes'])
            api.request.assert_not_called()


if __name__ == '__main__':
    unittest.main()
