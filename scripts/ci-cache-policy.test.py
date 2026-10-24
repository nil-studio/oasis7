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
REPOSITORY_LIMIT = 10_000_000_000
OBSERVED_FIXED_BYTES = 3_960_422_714


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


def observed_inventory():
    """Replay 2026-10-23 nil-studio/oasis7 Actions cache audit byte totals.

    Source: https://github.com/nil-studio/oasis7/actions/caches, reviewed before
    the capacity-recovery change. The seven workspace sizes, compatibility
    grouping, 14-entry count and family totals are observed; IDs, keys, dates
    and the split of dependency/tool totals below are synthetic. This fixture
    does not claim that future compact ordinary/Formal archives were measured.
    """
    result = []
    for number, size, group in ((1, 697_721_846, 'b'), (2, 698_163_138, 'b'),
                                (3, 698_375_096, 'a'), (4, 698_504_379, 'a')):
        old = ('formal-workspace-v2-ubuntu24-X64-1.96.0-nightly-2025-12-11-'
               + group * 64 + f'-{number:040x}')
        stamp = f'2026-10-22T01:0{number}:00Z'
        result.append(entry('formal', number, key=old, size_in_bytes=size,
                            created_at=stamp, last_accessed_at=stamp))
    for number, size, group in ((5, 865_504_918, 'b'), (6, 865_609_559, 'b'),
                                (7, 897_049_399, 'a')):
        result.append(entry('ecs', number, key=key('ecs', number, group), size_in_bytes=size))
    fixed = ((8, 'v0-rust-ci-shared-native-trusted-v3-Linux-x64-aaaaaaaa-bbbbbbbb', 2_461_139_163),
             (9, 'v0-rust-oasis7-ecs-native-linux-ecs-v2-Linux-x64-aaaaaaaa-bbbbbbbb', 689_000_000),
             (10, 'v0-rust-oasis7-ecs-native-linux-ecs-v2-Linux-x64-cccccccc-bbbbbbbb', 689_559_258),
             (11, 'node-cache-audit-fixture', 120_724_290), (12, 'trunk-archive-audit-fixture', 1),
             (13, 'wasm-bindgen-audit-fixture', 1), (14, 'cache-trust-sentinel-audit-fixture', 1))
    result.extend(entry('ordinary', number, key=cache_key, size_in_bytes=size)
                  for number, cache_key, size in fixed)
    return result


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
    def test_raw_staging_single_upload_resident_and_rotation_are_distinct(self):
        for name, upload, raw in (('ordinary', 768 * 1024 ** 2, 6 * 1024 ** 3),
                                  ('formal', 768 * 1024 ** 2, 8 * 1024 ** 3),
                                  ('ecs', 1024 ** 3, 6 * 1024 ** 3)):
            with self.subTest(name=name):
                settings = policy.FAMILIES[name]
                self.assertEqual(settings['staging_bytes'], raw)
                self.assertEqual(settings['max_upload_bytes'], upload)
                self.assertEqual(settings['max_bytes'], 2 * upload)
                self.assertEqual(settings['rotation_bytes'], 3 * upload)
                self.assertTrue(policy.save_policy([], key(name, 1), upload, REPOSITORY_LIMIT)['capacity_allowed'])
                rejected = policy.save_policy([], key(name, 1), upload + 1, REPOSITORY_LIMIT)
                self.assertFalse(rejected['capacity_allowed'])
                self.assertEqual(rejected['reason'], name + ' single upload budget')
                pair = [entry(name, i, size_in_bytes=upload) for i in (1, 2)]
                rotation = policy.save_policy(pair, key(name, 3), upload, REPOSITORY_LIMIT)
                self.assertTrue(rotation['capacity_allowed'])
                self.assertEqual(rotation['family_limit_bytes'], 3 * upload)
                self.assertEqual(rotation['family_resident_limit_bytes'], 2 * upload)
                self.assertEqual(rotation['single_upload_limit_bytes'], upload)
        self.assertLess(4_428_645_549, policy.FAMILIES['ordinary']['staging_bytes'])

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
            limit = policy.FAMILIES[name]['rotation_bytes']
            entries = [entry(name, 1, key=legacy[name], size_in_bytes=limit - 200),
                       entry(name, 4, size_in_bytes=100),
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

    def test_ecs_migration_headroom_requires_an_absent_new_compatibility_and_full_global_capacity(self):
        entries = [entry('ecs', i, size_in_bytes=size) for i, size in
                   ((1, 865_504_918), (2, 865_609_559), (3, 897_049_399))]
        new_key = key('ecs', 4, 'c')
        first = policy.save_policy(entries, new_key, 1_020_000_000, REPOSITORY_LIMIT)
        self.assertTrue(first['capacity_allowed'])
        self.assertTrue(first['migration_admission'])
        self.assertEqual(first['admission_basis'], 'new ECS compatibility migration')
        self.assertEqual(first['family_rotation_limit_bytes'], 3 * 1024 ** 3)
        self.assertEqual(first['family_limit_bytes'], 4 * 1024 ** 3)
        self.assertEqual(first['projected_family_bytes'], 3_648_163_876)
        existing = policy.save_policy(entries, key('ecs', 4), 1_020_000_000, REPOSITORY_LIMIT)
        self.assertFalse(existing['capacity_allowed'])
        self.assertFalse(existing['migration_admission'])
        self.assertEqual(existing['family_limit_bytes'], 3 * 1024 ** 3)
        incomplete = [*entries, entry('ecs', 9, key=key('ecs', 9, 'c'), size_in_bytes=0)]
        self.assertFalse(policy.save_policy(incomplete, new_key, 1_020_000_000, REPOSITORY_LIMIT)['migration_admission'])
        full = [*entries, entry('formal', 10, key='other-caches', size_in_bytes=7_000_000_000)]
        rejected = policy.save_policy(full, new_key, 1_020_000_000, REPOSITORY_LIMIT)
        self.assertFalse(rejected['capacity_allowed'])
        self.assertEqual(rejected['reason'], 'repository cache capacity gap')
        for name in ('ordinary', 'formal'):
            report = policy.save_policy([], key(name, 4, 'c'), 100, REPOSITORY_LIMIT)
            self.assertFalse(report['migration_admission'])
            self.assertEqual(report['family_limit_bytes'], policy.FAMILIES[name]['rotation_bytes'])


class InventoryLifecycleTests(unittest.TestCase):
    @staticmethod
    def apply_plan(entries, plan):
        doomed = {item['id'] for item in plan['delete']}
        return [item for item in entries if item['id'] not in doomed]

    def test_observed_inventory_bootstrap_first_second_third_and_confirmed_retention(self):
        entries = observed_inventory()
        self.assertEqual(len(entries), 14)
        self.assertEqual(sum(item['size_in_bytes'] for item in entries), 9_381_351_049)
        bootstrap = maintenance.plan_bootstrap(entries, '2026-10-23T00:00:00Z')
        self.assertEqual({item['id'] for item in bootstrap['delete']}, {1, 3})
        self.assertEqual(sum(item['size_in_bytes'] for item in bootstrap['delete']), 1_396_096_942)
        entries = self.apply_plan(entries, bootstrap)
        self.assertEqual(sum(item['size_in_bytes'] for item in entries), 7_985_254_107)

        # Representative compact outcomes, not measured compression results:
        # actual stored ordinary/Formal 700 MB, upload upper bound 800 MB;
        # actual ECS 900 MB, upper bound 1.02 GB. All obey the public ceilings.
        # Confirmed ECS retention is essential to remove its historical third
        # snapshot; bootstrap never assumes an ECS compatibility is obsolete.
        next_id, peak = 20, 0
        for generation in range(1, 4):
            for name in ('ordinary', 'formal', 'ecs'):
                next_id += 1
                stamp = f'2026-10-{23 + generation}T00:{next_id:02d}:00Z'
                actual, bound = (900_000_000, 1_020_000_000) if name == 'ecs' else (700_000_000, 800_000_000)
                new_key = key(name, next_id, 'c')
                admission = policy.save_policy(entries, new_key, bound, REPOSITORY_LIMIT)
                self.assertTrue(admission['capacity_allowed'], (generation, name, admission))
                peak = max(peak, admission['projected_repository_bytes'])
                entries.append(entry(name, next_id, key=new_key, size_in_bytes=actual,
                                     created_at=stamp, last_accessed_at=stamp))
                plan = maintenance.plan_retention(entries, new_key, stamp)
                entries = self.apply_plan(entries, plan)
                family = [item for item in entries if policy.family_for_key(item['key']) == name]
                self.assertEqual(len(family), min(generation, 2))
                self.assertTrue(any(item['key'] == new_key for item in family))
        self.assertLessEqual(peak, REPOSITORY_LIMIT)
        self.assertEqual(sum(item['size_in_bytes'] for item in entries), OBSERVED_FIXED_BYTES + 4_600_000_000)

    def test_family_ceiling_entitlements_cannot_override_global_capacity(self):
        entries = [entry('ordinary', 100, key='unmanaged-observed-dependencies-and-tools',
                         size_in_bytes=OBSERVED_FIXED_BYTES)]
        number = 0
        for name in PREFIXES:
            for _ in range(2):
                number += 1
                entries.append(entry(name, number, size_in_bytes=policy.FAMILIES[name]['max_upload_bytes']))
        self.assertEqual(sum(item['size_in_bytes'] for item in entries), 9_329_131_834)
        before = json.dumps(entries, sort_keys=True)
        report = policy.save_policy(entries, key('ecs', 99), 1024 ** 3, REPOSITORY_LIMIT)
        self.assertEqual(report['projected_repository_bytes'], 10_402_873_658)
        self.assertFalse(report['capacity_allowed'])
        self.assertEqual(report['reason'], 'repository cache capacity gap')
        self.assertEqual(json.dumps(entries, sort_keys=True), before)

    def test_busy_bootstrap_preserves_recovery_and_current_ecs_even_if_first_save_is_blocked(self):
        entries = observed_inventory()
        run = dict(id=1, created_at='2026-10-20T00:00:00Z', status='in_progress',
                   head_sha='a' * 40, path='.github/workflows/rust.yml',
                   repository={'full_name': 'nil-studio/oasis7'})
        plan = maintenance.plan_bootstrap(entries, '2026-10-23T00:00:00Z', active_runs=[run])
        self.assertEqual(plan['delete'], [])
        report = policy.save_policy(entries, key('ordinary', 30), 800_000_000, REPOSITORY_LIMIT)
        self.assertFalse(report['capacity_allowed'])
        self.assertEqual(report['reason'], 'repository cache capacity gap')
        idle = maintenance.plan_bootstrap(entries, '2026-10-23T00:00:00Z')
        self.assertTrue({5, 6, 7}.isdisjoint(item['id'] for item in idle['delete']))


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
