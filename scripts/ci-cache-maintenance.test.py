#!/usr/bin/env python3
"""Exercise retention and API failures without credentials or remote mutations."""
import importlib.util
import io
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import Mock, patch
from urllib.error import HTTPError

spec = importlib.util.spec_from_file_location('maintenance', Path(__file__).with_name('ci-cache-maintenance.py'))
maintenance = importlib.util.module_from_spec(spec)
spec.loader.exec_module(maintenance)

PREFIX = 'formal-workspace-v2-ubuntu24-X64-1.96.0-nightly-2025-12-11-'
START = '2026-10-19T09:00:00Z'


def cache(number, *, compatibility='a', size=560_000_000, created=None, accessed=None, **overrides):
    created = created or f'2026-10-19T08:{number % 60:02d}:00Z'
    result = {'id': number, 'key': f'{PREFIX}{compatibility * 64}-{number:040x}',
              'ref': 'refs/heads/main', 'version': 'v1', 'size_in_bytes': size,
              'created_at': created, 'last_accessed_at': accessed or created}
    result.update(overrides)
    return result


def api_with(*snapshots):
    api = Mock()
    api.list_caches.side_effect = list(snapshots)
    api.list_active_runs.return_value = []
    api.delete_cache.return_value = True
    return api


def rust_cache(number, namespace, *, environment='a', dependencies='b', **overrides):
    return cache(number, key=f'v0-rust-{namespace}-Linux-x64-{environment * 8}-{dependencies * 8}',
                 **overrides)


def active_run(number=42, *, created='2026-10-19T07:00:00Z', status='in_progress', **overrides):
    result = dict(id=number, created_at=created, status=status, head_sha=f'{number:040x}',
                  path='.github/workflows/rust.yml', repository={'full_name': 'nil-studio/oasis7'})
    result.update(overrides)
    return result


class RetentionTests(unittest.TestCase):
    def plan(self, entries, **kwargs):
        return maintenance.plan_retention(entries, entries[-1]['key'], START, **kwargs)

    def test_sha_snapshots_stop_accumulating_within_one_compatibility(self):
        entries = [cache(i) for i in range(1, 7)]
        plan = self.plan(entries)
        self.assertEqual(plan['protected_ids'], [5, 6])
        self.assertEqual({item['id'] for item in plan['delete']}, {1, 2, 3, 4})
        self.assertEqual(plan['projected_bytes'], 1_120_000_000)

    def test_old_compatibility_groups_have_no_resident_allowance_below_the_budget(self):
        entries = [cache(1, compatibility='b'), cache(2, compatibility='c'), cache(3), cache(4)]
        plan = self.plan(entries)
        self.assertEqual([item['id'] for item in plan['delete']], [1, 2])
        self.assertEqual(plan['projected_bytes'], 1_120_000_000)
        self.assertLessEqual(plan['projected_bytes'], maintenance.FORMAL_MAX_BYTES)

    def test_old_active_run_protects_caches_accessed_before_the_current_run(self):
        entries = [cache(1, compatibility='b', created='2026-10-19T06:00:00Z',
                         accessed='2026-10-19T07:30:00Z'), cache(2), cache(3)]
        plan = self.plan(entries, active_runs=[active_run()])
        self.assertEqual(plan['delete'], [])
        self.assertIn(1, plan['protected_ids'])
        self.assertEqual(plan['protection_started_at'], '2026-10-19T07:00:00Z')

    def test_old_run_finishing_late_preserves_newer_run_even_with_new_compatibility(self):
        entries = [cache(1), cache(2),
                   cache(3, compatibility='b', created='2026-10-19T09:10:00Z'),
                   cache(4, created='2026-10-19T09:20:00Z')]
        plan = self.plan(entries, max_bytes=1)
        self.assertTrue({2, 3, 4}.issubset(plan['protected_ids']))
        self.assertEqual([item['id'] for item in plan['delete']], [1])
        self.assertGreater(plan['protected_over_budget_bytes'], 0)

    def test_equal_timestamp_recent_access_and_incomplete_upload_are_protected(self):
        entries = [cache(1, compatibility='b', accessed=START),
                   cache(2, compatibility='c', created=START),
                   cache(3, size=0), cache(4), cache(5)]
        plan = self.plan(entries, max_bytes=1)
        self.assertEqual(plan['delete'], [])
        self.assertEqual(plan['protected_ids'], [1, 2, 3, 4, 5])

    def test_other_refs_unknown_keys_and_other_families_are_never_deleted(self):
        outsiders = [cache(11, ref='refs/pull/4/merge'), cache(12, ref='refs/tags/v1.0.0'),
                     cache(13, key='formal-workspace-v2-ubuntu24-lookalike'),
                     cache(14, key='v0-rust-ci-full-regression-trusted-v2-Linux-x64-env-deps')]
        entries = [cache(1), *outsiders, cache(2), cache(3)]
        plan = self.plan(entries, max_bytes=1)
        self.assertEqual([item['id'] for item in plan['delete']], [1])
        summary = maintenance.inventory_summary(entries)
        self.assertEqual(summary['total_count'], len(entries))
        self.assertEqual(summary['total_bytes'], sum(item['size_in_bytes'] for item in entries))
        self.assertTrue(any(group['family'] == 'unknown' for group in summary['groups']))

    def test_current_must_be_present_complete_and_unique_across_versions(self):
        current = cache(3)
        bad_lists = [[cache(1)], [cache(1), current, dict(current, id=4, version='v2')],
                     [cache(1), dict(current, size_in_bytes=0)]]
        for entries in bad_lists:
            with self.subTest(entries=entries), self.assertRaises(maintenance.MaintenanceError):
                maintenance.plan_retention(entries, current['key'], START)

    def test_recovery_point_requires_matching_cache_version(self):
        entries = [cache(1), cache(2, version='v2'), cache(3)]
        plan = self.plan(entries, max_bytes=1)
        self.assertEqual(plan['protected_ids'], [1, 3])
        self.assertEqual([item['id'] for item in plan['delete']], [2])

    def test_invalid_api_fields_cannot_produce_a_deletion_plan(self):
        for override in ({'id': True}, {'size_in_bytes': -1}, {'created_at': 'bad'},
                         {'last_accessed_at': '2026-10-19T08:00:00'}, {'version': ''}):
            with self.subTest(override=override), self.assertRaises(maintenance.MaintenanceError):
                self.plan([cache(1, **override), cache(2), cache(3)])

    def test_dry_run_never_deletes(self):
        entries = [cache(1), cache(2), cache(3)]
        api = api_with(entries)
        report = maintenance.maintain(api, entries[-1]['key'], START)
        self.assertEqual(report['status'], 'dry-run')
        self.assertEqual(len(report['plan']['delete']), 1)
        api.delete_cache.assert_not_called()

    def test_second_inventory_can_cancel_deletion_after_recent_access(self):
        first = [cache(1), cache(2), cache(3), cache(4)]
        second = [dict(first[0], last_accessed_at=START), *first[1:]]
        api = api_with(first, second, [second[0], *second[2:]])
        report = maintenance.maintain(api, first[-1]['key'], START, apply=True)
        api.delete_cache.assert_called_once_with(2)
        self.assertEqual(report['deleted_ids'], [2])
        self.assertEqual(report['after']['total_count'], 3)

    def test_second_inventory_cannot_expand_deletions_or_reuse_changed_identity(self):
        first = [cache(1), cache(2), cache(3)]
        second = [dict(first[0], version='v2'), cache(10, compatibility='b'), *first[1:]]
        api = api_with(first, second, second)
        report = maintenance.maintain(api, first[-1]['key'], START, apply=True)
        api.delete_cache.assert_not_called()
        self.assertEqual(report['deleted_ids'], [])

    def test_later_inventory_failure_or_missing_current_causes_zero_deletes(self):
        entries = [cache(1), cache(2), cache(3)]
        changed_current = [*entries[:-1], dict(entries[-1], id=30, version='replacement')]
        for failure in (maintenance.MaintenanceError('page two failed'), entries[:-1], changed_current):
            with self.subTest(failure=failure):
                api = api_with(entries, failure, entries)
                report = maintenance.maintain(api, entries[-1]['key'], START, apply=True)
                api.delete_cache.assert_not_called()
                self.assertEqual(report['status'], 'skipped')
                self.assertTrue(report['warnings'])
                self.assertIsNone(report['after'])

    def test_delete_error_stops_remaining_ids_and_readback_is_observed(self):
        entries = [cache(i) for i in range(1, 6)]
        api = api_with(entries, entries, entries[1:])
        api.delete_cache.side_effect = [True, maintenance.MaintenanceError('HTTP 403')]
        report = maintenance.maintain(api, entries[-1]['key'], START, apply=True)
        self.assertEqual([call.args[0] for call in api.delete_cache.call_args_list], [1, 2])
        self.assertEqual(report['deleted_ids'], [1])
        self.assertEqual(report['status'], 'partial')
        self.assertEqual(report['after']['total_count'], 4)
        self.assertTrue(report['warnings'])

    def test_already_evicted_ids_and_unavailable_readback_are_not_claimed_as_reclaimed_bytes(self):
        entries = [cache(1), cache(2), cache(3)]
        api = api_with(entries, entries, maintenance.MaintenanceError('readback unavailable'))
        api.delete_cache.return_value = False
        report = maintenance.maintain(api, entries[-1]['key'], START, apply=True)
        self.assertEqual(report['already_absent_ids'], [1])
        self.assertEqual(report['deleted_ids'], [])
        self.assertIsNone(report['after'])
        self.assertIn('unknown', maintenance.render_summary(report).lower())

    def test_repeat_application_is_idempotent(self):
        remaining = [cache(2), cache(3)]
        api = api_with(remaining, remaining, remaining)
        report = maintenance.maintain(api, remaining[-1]['key'], START, apply=True)
        api.delete_cache.assert_not_called()
        self.assertEqual(report['after']['total_bytes'], 1_120_000_000)


class WorkspaceRetentionTests(unittest.TestCase):
    @staticmethod
    def entry(number, name='ecs', compatibility='a', **changes):
        prefix = {'ecs': 'ecs-workspace-v2', 'ordinary': 'ordinary-workspace-v1',
                  'formal': 'formal-workspace-v3'}[name]
        return cache(number, key=f'{prefix}-{compatibility * 64}-{number:040x}', **changes)

    def test_confirmed_third_ecs_generation_keeps_current_and_recovery(self):
        entries = [self.entry(i) for i in (1, 2, 3)]
        report = maintenance.maintain(api_with(entries, entries, entries[1:]),
                                      entries[-1]['key'], START, apply=True)
        self.assertEqual(report['status'], 'completed')
        self.assertEqual(report['deleted_ids'], [1])
        self.assertEqual(report['plan']['protected_ids'], [2, 3])

    def test_multiple_explicit_families_preserve_every_unanchored_family(self):
        formal = [cache(1), self.entry(2, 'formal'), self.entry(3, 'formal')]
        ordinary = [self.entry(i, 'ordinary', size=100) for i in (4, 5, 6)]
        ecs = [self.entry(i) for i in (7, 8, 9)]
        entries = [*formal, *ordinary, *ecs]
        keys = [formal[-1]['key'], ordinary[-1]['key']]
        plan = maintenance.plan_retention(entries, keys, START)
        self.assertEqual({item['id'] for item in plan['delete']}, {1, 4})
        self.assertEqual(set(plan['families']), {'formal', 'ordinary'})
        self.assertTrue({2, 3, 5, 6}.issubset(plan['protected_ids']))

    def test_each_anchor_must_be_complete_unique_and_stable(self):
        formal = self.entry(3, 'formal')
        ordinary = self.entry(6, 'ordinary')
        entries = [self.entry(1, 'formal'), formal, ordinary]
        keys = [formal['key'], ordinary['key']]
        for second in (entries[:-1], [*entries[:-1], dict(ordinary, size_in_bytes=0)],
                       [*entries, dict(ordinary, id=60, version='other')],
                       [*entries[:-1], dict(ordinary, id=60)]):
            with self.subTest(second=second):
                api = api_with(entries, second)
                result = maintenance.maintain(api, keys, START, apply=True)
                self.assertEqual(result['status'], 'skipped')
                api.delete_cache.assert_not_called()

    def test_ambiguous_same_family_anchors_are_rejected(self):
        entries = [self.entry(i) for i in (1, 2, 3)]
        with self.assertRaises(maintenance.MaintenanceError):
            maintenance.plan_retention(entries, [entry['key'] for entry in entries], START)

    def test_duplicate_same_anchor_is_idempotent_and_empty_anchors_are_report_only(self):
        entries = [self.entry(i) for i in (1, 2, 3)]
        key = entries[-1]['key']
        plan = maintenance.plan_retention(entries, [key, key], START)
        self.assertEqual(plan['current_keys'], [key])
        self.assertEqual(plan['current_key'], key)
        api = api_with(entries)
        report = maintenance.maintain(api, [])
        self.assertEqual(report['status'], 'dry-run')
        self.assertIsNone(report['plan'])
        self.assertEqual(report['before']['total_bytes'], sum(entry['size_in_bytes'] for entry in entries))
        api.list_active_runs.assert_not_called()
        api.delete_cache.assert_not_called()
        self.assertEqual(maintenance.maintain(api, [], START, apply=True)['status'], 'skipped')
        api.delete_cache.assert_not_called()

    def test_legacy_ecs_migration_preserves_activity_unknown_schemas_and_other_refs(self):
        old = self.entry(1)
        old['key'] = old['key'].replace('ecs-workspace-v2-', 'ecs-workspace-v1-')
        entries = [old, dict(old, id=10, last_accessed_at=START), dict(old, id=11, size_in_bytes=0),
                   dict(old, id=12, ref='refs/pull/1/merge'), dict(old, id=13, key=old['key'] + '-extra'),
                   self.entry(2), self.entry(3)]
        plan = maintenance.plan_retention(entries, entries[-1]['key'], START)
        self.assertEqual([entry['id'] for entry in plan['delete']], [1])
        self.assertEqual(plan['protected_ids'], [2, 3, 10, 11])

    def test_each_family_retains_its_budget_and_protected_excess(self):
        entries = [self.entry(1, 'ordinary', size=900_000_000),
                   self.entry(2, 'ordinary', size=900_000_000), self.entry(3, size=1)]
        plan = maintenance.plan_retention(entries, [entries[1]['key'], entries[2]['key']], START)
        self.assertEqual(plan['families']['ordinary']['max_bytes'], 1536 * 1024 ** 2)
        self.assertEqual(plan['families']['ecs']['max_bytes'], 2 * 1024 ** 3)
        self.assertEqual(plan['protected_over_budget_bytes'], 1_800_000_000 - 1536 * 1024 ** 2)
        self.assertEqual(plan['delete'], [])

    def test_revalidation_across_families_can_only_shrink_candidates(self):
        first = [*(self.entry(i, 'formal') for i in (1, 2, 3)),
                 *(self.entry(i, 'ordinary', size=100) for i in (4, 5, 6))]
        second = [dict(first[0], last_accessed_at=START), *first[1:],
                  self.entry(7, 'ordinary', compatibility='b', size=100)]
        api = api_with(first, second, [entry for entry in second if entry['id'] != 4])
        result = maintenance.maintain(api, [first[2]['key'], first[5]['key']], START, apply=True)
        self.assertEqual(result['deleted_ids'], [4])
        api.delete_cache.assert_called_once_with(4)


class BootstrapTests(unittest.TestCase):
    def test_one_complete_recovery_per_retired_prefix_and_cache_version(self):
        entries = [cache(1), cache(2), cache(3, compatibility='b'), cache(4, compatibility='b'),
                   cache(5, version='v2'), cache(6, version='v2')]
        plan = maintenance.plan_bootstrap(entries, START)
        self.assertEqual([item['id'] for item in plan['delete']], [1, 3, 5])
        self.assertEqual(plan['retired_schemas']['recovery_ids'], [2, 4, 6])
        self.assertEqual(plan['protected_ids'], [2, 4, 6])
        self.assertEqual(plan['current_keys'], [])
        api = api_with(entries, entries, [entries[i] for i in (1, 3, 5)])
        result = maintenance.maintain(api, run_started_at=START, bootstrap=True, apply=True)
        self.assertEqual(result['status'], 'completed')
        self.assertEqual(result['deleted_ids'], [1, 3, 5])
        self.assertEqual(result['stable_recovery_ids'], [2, 4, 6])
        self.assertIn('retired', maintenance.render_summary(result).lower())

    def test_current_schemas_unknown_hashes_and_other_refs_have_no_bootstrap_deletion(self):
        entries = [cache(1), cache(2),
                   *(WorkspaceRetentionTests.entry(i, name) for i, name in
                     ((3, 'formal'), (4, 'ordinary'), (5, 'ecs'))),
                   rust_cache(6, 'ci-shared-native-trusted-v3'),
                   rust_cache(7, 'ci-shared-native-trusted-v3', environment='c'),
                   cache(8, ref='refs/pull/1/merge'), cache(9, ref='refs/tags/v1.0.0'),
                   cache(10, key=cache(10)['key'] + '-extra')]
        plan = maintenance.plan_bootstrap(entries, START)
        self.assertEqual([item['id'] for item in plan['delete']], [1])
        self.assertEqual(plan['retired_schemas']['managed_count'], 2)

    def test_known_retired_ecs_schema_is_bounded_per_compatibility(self):
        entries = [WorkspaceRetentionTests.entry(i) for i in (1, 2)]
        entries = [dict(item, key=item['key'].replace('ecs-workspace-v2-', 'ecs-workspace-v1-'))
                   for item in entries]
        plan = maintenance.plan_bootstrap(entries, START)
        self.assertEqual([item['id'] for item in plan['delete']], [1])
        self.assertEqual(plan['retired_schemas']['recovery_ids'], [2])

    def test_activity_incomplete_and_single_recovery_are_preserved(self):
        entries = [cache(1, accessed=START), cache(2, size=0), cache(3), cache(4),
                   cache(5, compatibility='b')]
        plan = maintenance.plan_bootstrap(entries, START)
        self.assertEqual([item['id'] for item in plan['delete']], [3])
        self.assertEqual(plan['protected_ids'], [1, 2, 4, 5])
        active = maintenance.plan_bootstrap(entries, START, active_runs=[active_run()])
        self.assertEqual(active['delete'], [])
        self.assertEqual(active['protection_started_at'], '2026-10-19T07:00:00Z')
        self.assertEqual(maintenance.plan_bootstrap([cache(1, size=0)], START)['delete'], [])

    def test_timestamp_ties_and_ambiguous_exact_keys_block_the_affected_group(self):
        for ambiguous in ([cache(1, created='2026-10-19T08:03:00Z'), cache(2), cache(3)],
                          [cache(1), cache(2), dict(cache(2), id=3, version='v2')],
                          [cache(1), cache(2), dict(cache(2), id=3)]):
            with self.subTest(ambiguous=ambiguous):
                entries = [*ambiguous, cache(4, compatibility='b'), cache(5, compatibility='b')]
                plan = maintenance.plan_bootstrap(entries, START)
                self.assertEqual([item['id'] for item in plan['delete']], [4])
                self.assertTrue(plan['retired_schemas']['blocked_groups'])
                self.assertTrue({1, 2, 3}.issubset(plan['protected_ids']))

    def test_recovery_identity_must_survive_both_complete_inventories(self):
        first = [cache(1), cache(2), cache(3)]
        for second in (first[:-1], [*first[:-1], dict(first[-1], version='other')],
                       [*first[:-1], dict(first[-1], size_in_bytes=0)],
                       [*first[:-1], dict(first[-1], created_at='2026-10-19T08:04:00Z')],
                       [*first, cache(4)]):
            with self.subTest(second=second):
                api = api_with(first, second, second)
                result = maintenance.maintain(api, run_started_at=START, bootstrap=True, apply=True)
                api.delete_cache.assert_not_called()
                self.assertEqual(result['deleted_ids'], [])

    def test_revalidation_only_shrinks_candidates_and_protects_new_activity(self):
        first = [cache(1), cache(2), cache(3)]
        second = [dict(first[0], last_accessed_at=START), *first[1:],
                  cache(10, compatibility='b'), cache(11, compatibility='b')]
        api = api_with(first, second, [item for item in second if item['id'] != 2])
        result = maintenance.maintain(api, run_started_at=START, bootstrap=True, apply=True)
        self.assertEqual(result['deleted_ids'], [2])
        api.delete_cache.assert_called_once_with(2)
        api = api_with(first, [dict(first[0], version='other'), *first[1:]], first)
        result = maintenance.maintain(api, run_started_at=START, bootstrap=True, apply=True)
        self.assertEqual(result['deleted_ids'], [2])

    def test_partial_inventory_or_activity_api_failure_defers_all_deletions(self):
        entries = [cache(1), cache(2)]
        for failure_at in ('first inventory', 'second inventory', 'first active', 'second active'):
            with self.subTest(failure_at=failure_at):
                error = maintenance.MaintenanceError('incomplete API observation')
                snapshots = [entries, entries, entries]
                if 'inventory' in failure_at:
                    snapshots[0 if failure_at.startswith('first') else 1] = error
                api = api_with(*snapshots)
                if 'active' in failure_at:
                    api.list_active_runs.side_effect = [error] if failure_at.startswith('first') else [[], error]
                result = maintenance.maintain(api, run_started_at=START, bootstrap=True, apply=True)
                self.assertEqual(result['status'], 'skipped')
                api.delete_cache.assert_not_called()

    def test_bootstrap_preserves_native_namespaces_even_with_complete_replacement(self):
        old = rust_cache(1, 'ci-full-regression-trusted-v2')
        new = rust_cache(2, 'ci-shared-native-trusted-v3')
        entries = [old, new, cache(3), cache(4)]
        api = api_with(entries, entries, entries[1:])
        result = maintenance.maintain(api, run_started_at=START, bootstrap=True, apply=True)
        self.assertEqual(result['deleted_ids'], [3])
        self.assertEqual(result['stable_replacement_ids'], {})
        second = [old, dict(new, version='changed'), *entries[2:]]
        api = api_with(entries, second, second)
        result = maintenance.maintain(api, run_started_at=START, bootstrap=True, apply=True)
        self.assertEqual(result['deleted_ids'], [3])

    def test_bootstrap_dry_run_is_read_only_and_apply_requires_a_boundary(self):
        entries = [cache(1), cache(2)]
        api = api_with(entries)
        result = maintenance.maintain(api, run_started_at=START, bootstrap=True)
        self.assertEqual(result['status'], 'dry-run')
        self.assertEqual([item['id'] for item in result['plan']['delete']], [1])
        api.delete_cache.assert_not_called()
        for key, boundary in ((None, None), (entries[-1]['key'], START)):
            api = api_with(entries)
            result = maintenance.maintain(api, key, boundary, bootstrap=True, apply=True)
            self.assertEqual(result['status'], 'skipped')
            api.list_caches.assert_not_called()
            api.delete_cache.assert_not_called()


class DependencyRetentionTests(unittest.TestCase):
    NAMESPACES = ('ci-shared-native-trusted-v3', 'oasis7-ecs-native-linux-ecs-v2')

    @staticmethod
    def dependency(number, namespace='ci-shared-native-trusted-v3', *, environment='a', **changes):
        return cache(number, key=f'v0-rust-{namespace}-Linux-x64-{environment * 8}-{number:08x}', **changes)

    @staticmethod
    def plan(entries, bootstrap, **kwargs):
        if bootstrap:
            return maintenance.plan_bootstrap(entries, START, **kwargs)
        anchor = WorkspaceRetentionTests.entry(90, 'ordinary')
        return maintenance.plan_retention([*entries, anchor], anchor['key'], START, **kwargs)

    @staticmethod
    def apply(first, second, after, bootstrap, active=None):
        if not bootstrap:
            anchor = WorkspaceRetentionTests.entry(90, 'ordinary')
            first, second, after = [*first, anchor], [*second, anchor], [*after, anchor]
        api = api_with(first, second, after)
        if active is not None:
            api.list_active_runs.side_effect = active
        report = maintenance.maintain(api, None if bootstrap else anchor['key'], START,
                                      bootstrap=bootstrap, apply=True)
        return api, report

    def test_each_namespace_environment_and_version_keeps_its_own_latest_complete_snapshot(self):
        for bootstrap in (True, False):
            for namespace in self.NAMESPACES:
                with self.subTest(bootstrap=bootstrap, namespace=namespace):
                    entries = [self.dependency(1, namespace), self.dependency(2, namespace),
                               self.dependency(3, namespace, environment='b'),
                               self.dependency(4, namespace, environment='b'),
                               self.dependency(5, namespace, version='other'),
                               self.dependency(6, namespace, version='other')]
                    plan = self.plan(entries, bootstrap)
                    self.assertEqual([entry['id'] for entry in plan['delete']], [1, 3, 5])
                    self.assertEqual(plan['dependencies']['recovery_ids'], [2, 4, 6])
                    self.assertEqual(plan['dependencies']['protected_ids'], [2, 4, 6])
                    after = [entries[i] for i in (1, 3, 5)]
                    api, report = self.apply(entries, entries, after, bootstrap)
                    self.assertEqual(report['deleted_ids'], [1, 3, 5])
                    self.assertEqual(report['deleted_dependency_ids'], [1, 3, 5])
                    self.assertEqual(report['deleted_workspace_ids'], [])
                    self.assertEqual(report['stable_dependency_recovery_ids'], [2, 4, 6])
                    self.assertEqual([call.args[0] for call in api.delete_cache.call_args_list], [1, 3, 5])

    def test_allowlist_requires_exact_main_linux_x64_and_two_eight_hex_hashes(self):
        valid = [self.dependency(1), self.dependency(2)]
        outsider = self.dependency(10)
        variants = [dict(outsider, ref='refs/pull/4/merge'), dict(outsider, ref='refs/tags/v1'),
                    self.dependency(10, 'unregistered-trusted-v3'),
                    dict(outsider, key=outsider['key'].replace('Linux-x64', 'Linux-arm64')),
                    dict(outsider, key=outsider['key'].replace('Linux-x64', 'Windows-x64')),
                    dict(outsider, key=outsider['key'].replace('aaaaaaaa', 'aaaaaaa')),
                    dict(outsider, key=outsider['key'].replace('0000000a', '000000a')),
                    dict(outsider, key=outsider['key'].replace('0000000a', '0000000g')),
                    dict(outsider, key=outsider['key'] + '-extra'),
                    WorkspaceRetentionTests.entry(10, 'ordinary'),
                    WorkspaceRetentionTests.entry(10, 'formal'), WorkspaceRetentionTests.entry(10, 'ecs')]
        entries = [*valid, *(dict(entry, id=20 + i) for i, entry in enumerate(variants))]
        for bootstrap in (True, False):
            with self.subTest(bootstrap=bootstrap):
                plan = self.plan(entries, bootstrap)
                self.assertEqual([entry['id'] for entry in plan['dependencies']['delete']], [1])
                self.assertEqual(plan['dependencies']['managed_count'], 2)
                if bootstrap:
                    self.assertEqual([entry['id'] for entry in plan['delete']], [1])

    def test_activity_zero_bytes_and_only_complete_recovery_are_never_deleted(self):
        entries = [self.dependency(1, accessed=START), self.dependency(2, size=0),
                   self.dependency(3), self.dependency(4), self.dependency(5, environment='b')]
        for bootstrap in (True, False):
            with self.subTest(bootstrap=bootstrap):
                plan = self.plan(entries, bootstrap)
                self.assertEqual([entry['id'] for entry in plan['dependencies']['delete']], [3])
                self.assertEqual(plan['dependencies']['protected_ids'], [1, 2, 4, 5])
                active = self.plan(entries, bootstrap, active_runs=[active_run()])
                self.assertEqual(active['dependencies']['delete'], [])
                self.assertEqual(active['protection_started_at'], '2026-10-19T07:00:00Z')
                empty = self.plan([self.dependency(1, size=0)], bootstrap)
                self.assertEqual(empty['dependencies']['delete'], [])
                self.assertEqual(empty['dependencies']['recovery_ids'], [])

    def test_timestamp_ties_and_exact_key_ambiguity_hold_only_the_affected_groups(self):
        for first in ([self.dependency(1, created='2026-10-19T08:03:00Z'), self.dependency(2), self.dependency(3)],
                      [self.dependency(1), self.dependency(2), dict(self.dependency(2), id=3, version='v2')],
                      [self.dependency(1), self.dependency(2), dict(self.dependency(2), id=3)]):
            entries = [*first, self.dependency(4, environment='b'), self.dependency(5, environment='b')]
            for bootstrap in (True, False):
                with self.subTest(first=first, bootstrap=bootstrap):
                    plan = self.plan(entries, bootstrap)
                    self.assertEqual([entry['id'] for entry in plan['delete']], [4])
                    self.assertTrue(plan['dependencies']['blocked_groups'])
                    self.assertTrue({1, 2, 3}.issubset(plan['dependencies']['protected_ids']))

    def test_missing_changed_or_new_recovery_anchor_prevents_group_deletion(self):
        first = [self.dependency(1), self.dependency(2), self.dependency(3)]
        variations = [first[:-1], [*first[:-1], dict(first[-1], version='other')],
                      [*first[:-1], dict(first[-1], id=30)],
                      [*first[:-1], dict(first[-1], size_in_bytes=0)],
                      [*first[:-1], dict(first[-1], size_in_bytes=1)],
                      [*first[:-1], dict(first[-1], created_at='2026-10-19T08:04:00Z')],
                      [*first, self.dependency(4)], [*first, dict(first[-1], id=30, version='other')]]
        for second in variations:
            for bootstrap in (True, False):
                with self.subTest(second=second, bootstrap=bootstrap):
                    api, report = self.apply(first, second, second, bootstrap)
                    api.delete_cache.assert_not_called()
                    self.assertEqual(report['deleted_dependency_ids'], [])

    def test_candidate_intersection_identity_and_new_activity_only_shrink_mutations(self):
        first = [self.dependency(1), self.dependency(2), self.dependency(3)]
        second = [dict(first[0], last_accessed_at=START), *first[1:],
                  self.dependency(10, environment='b'), self.dependency(11, environment='b')]
        for bootstrap in (True, False):
            with self.subTest(bootstrap=bootstrap):
                api, report = self.apply(first, second, [item for item in second if item['id'] != 2], bootstrap)
                self.assertEqual(report['deleted_dependency_ids'], [2])
                api.delete_cache.assert_called_once_with(2)
                changed = [dict(first[0], size_in_bytes=1), *first[1:]]
                api, report = self.apply(first, changed, [changed[0], changed[2]], bootstrap)
                self.assertEqual(report['deleted_dependency_ids'], [2])
                api, report = self.apply(first, first, first, bootstrap, active=[[], [active_run()]])
                api.delete_cache.assert_not_called()
                self.assertEqual(report['deleted_dependency_ids'], [])

    def test_either_inventory_or_active_run_failure_prevents_all_dependency_deletes(self):
        entries = [self.dependency(1), self.dependency(2)]
        for bootstrap in (True, False):
            for failure_at in ('first inventory', 'second inventory', 'first active', 'second active'):
                with self.subTest(bootstrap=bootstrap, failure_at=failure_at):
                    anchor = WorkspaceRetentionTests.entry(90, 'ordinary')
                    observed = entries if bootstrap else [*entries, anchor]
                    snapshots = [observed, observed, observed]
                    error = maintenance.MaintenanceError('incomplete API observation')
                    if 'inventory' in failure_at:
                        snapshots[0 if failure_at.startswith('first') else 1] = error
                    api = api_with(*snapshots)
                    if 'active' in failure_at:
                        api.list_active_runs.side_effect = [error] if failure_at.startswith('first') else [[], error]
                    result = maintenance.maintain(api, None if bootstrap else anchor['key'], START,
                                                  bootstrap=bootstrap, apply=True)
                    self.assertEqual(result['status'], 'skipped')
                    api.delete_cache.assert_not_called()

    def test_namespace_retirement_and_dependency_redundancy_preserve_the_recovery_anchor(self):
        old = rust_cache(1, 'ci-full-regression-trusted-v2')
        entries = [old, self.dependency(2), self.dependency(3)]
        for bootstrap in (True, False):
            with self.subTest(bootstrap=bootstrap):
                expected = [2] if bootstrap else [1, 2]
                after = [entry for entry in entries if entry['id'] not in expected]
                api, report = self.apply(entries, entries, after, bootstrap)
                self.assertEqual(report['deleted_ids'], expected)
                self.assertEqual(report['deleted_dependency_ids'], [2])
                self.assertEqual(report['deleted_namespace_ids'], [] if bootstrap else [1])
                self.assertIn(3, report['plan']['dependencies']['protected_ids'])
                self.assertNotIn(3, [entry['id'] for entry in report['plan']['retired_namespaces']['delete']])
                self.assertEqual(report['stable_dependency_recovery_ids'], [3])

    def test_summary_separates_plans_from_successful_dependency_workspace_and_namespace_deletes(self):
        entries = [self.dependency(1), self.dependency(2), cache(3), cache(4), cache(5),
                   rust_cache(6, 'ci-full-regression-trusted-v2')]
        api = api_with(entries, entries, [entries[i] for i in (1, 3, 4, 5)])
        api.delete_cache.side_effect = [True, False, maintenance.MaintenanceError('HTTP 403')]
        report = maintenance.maintain(api, entries[4]['key'], START, apply=True)
        self.assertEqual(report['status'], 'partial')
        self.assertEqual(report['deleted_ids'], [1])
        self.assertEqual(report['already_absent_ids'], [3])
        self.assertEqual(report['deleted_dependency_ids'], [1])
        self.assertEqual(report['deleted_workspace_ids'], [])
        self.assertEqual(report['deleted_namespace_ids'], [])
        summary = maintenance.render_summary(report)
        self.assertIn('Dependency snapshots', summary)
        self.assertIn('dependency: [1]; workspace: []; retired namespaces: []', summary)
        self.assertIn('Planned bytes are not measured reclamation', summary)


class RetiredNamespaceTests(unittest.TestCase):
    migrations = {
        'ci-full-regression-trusted-v2': 'ci-shared-native-trusted-v3',
        **{f'release-packages-release-gate-{shard}-trusted-v2': 'ci-shared-native-trusted-v3'
           for shard in ('runtime-core', 'runtime-support', 'runtime-sync', 'soak')},
        'release-packages-web-wasm-trusted-v2': 'release-packages-web-wasm-trusted-v3',
        'oasis7-ecs-native-linux-packaging-v1': 'oasis7-ecs-native-linux-ecs-v2',
        'oasis7-ecs-native-linux-ecs-v1': 'oasis7-ecs-native-linux-ecs-v2',
    }

    def test_only_enumerated_namespaces_retire_after_their_replacement_exists(self):
        for old, new in self.migrations.items():
            with self.subTest(old=old):
                entries = [rust_cache(1, old), rust_cache(2, new), cache(3)]
                plan = maintenance.plan_retention(entries, entries[-1]['key'], START)
                self.assertEqual([entry['id'] for entry in plan['delete']], [1])
                self.assertEqual(plan['delete'][0]['replacement_family'], new)

    def test_missing_incomplete_other_ref_and_lookalike_replacements_do_not_authorize_retirement(self):
        old = rust_cache(1, 'oasis7-ecs-native-linux-packaging-v1')
        new = rust_cache(2, 'oasis7-ecs-native-linux-ecs-v2')
        invalid = [[], [dict(new, size_in_bytes=0)], [dict(new, ref='refs/pull/1/merge')],
                   [dict(new, key=new['key'] + '-extra')],
                   [dict(new, key=new['key'].replace('aaaaaaaa', 'aaaaaaa'))],
                   [dict(new, key=new['key'].replace('Linux-x64', 'Linux-arm64'))]]
        for replacements in invalid:
            with self.subTest(replacements=replacements):
                entries = [old, *replacements, cache(3)]
                plan = maintenance.plan_retention(entries, entries[-1]['key'], START)
                self.assertEqual(plan['delete'], [])

    def test_other_refs_unknown_namespaces_and_all_new_environment_keys_are_preserved(self):
        old = rust_cache(1, 'ci-full-regression-trusted-v2')
        new = rust_cache(2, 'ci-shared-native-trusted-v3')
        entries = [dict(old, ref='refs/tags/v1.0.0'), new,
                   rust_cache(3, 'ci-shared-native-trusted-v3', environment='c'),
                   rust_cache(4, 'unregistered-trusted-v2'),
                   dict(old, id=5, key=old['key'] + '-suffix'), cache(6)]
        plan = maintenance.plan_retention(entries, entries[-1]['key'], START)
        self.assertEqual(plan['delete'], [])
        families = {entry['family'] for entry in maintenance.inventory_summary(entries)['groups']}
        self.assertIn('rust/ci-shared-native-trusted-v3', families)
        self.assertEqual(maintenance.family(rust_cache(7, 'oasis7-ecs-native-linux-ecs-v2')['key']),
                         'rust/oasis7-ecs-native-linux-ecs-v2')

    def test_active_access_and_incomplete_old_cache_are_protected(self):
        entries = [rust_cache(1, 'ci-full-regression-trusted-v2',
                              created='2026-10-19T06:00:00Z', accessed='2026-10-19T07:30:00Z'),
                   rust_cache(2, 'ci-full-regression-trusted-v2', size=0),
                   rust_cache(3, 'ci-shared-native-trusted-v3'), cache(4)]
        plan = maintenance.plan_retention(entries, entries[-1]['key'], START,
                                          active_runs=[active_run()])
        self.assertEqual(plan['delete'], [])

    def test_replacement_identity_must_survive_both_inventories(self):
        old = rust_cache(1, 'oasis7-ecs-native-linux-packaging-v1')
        new = rust_cache(2, 'oasis7-ecs-native-linux-ecs-v2')
        first = [old, new, cache(3)]
        replacements = [[], [dict(new, version='v2')], [dict(new, size_in_bytes=0)],
                        [dict(new, size_in_bytes=new['size_in_bytes'] + 1)],
                        [rust_cache(4, 'oasis7-ecs-native-linux-ecs-v2', environment='c')]]
        for replacement in replacements:
            with self.subTest(replacement=replacement):
                second = [old, *replacement, first[-1]]
                api = api_with(first, second, second)
                report = maintenance.maintain(api, first[-1]['key'], START, apply=True)
                api.delete_cache.assert_not_called()
                self.assertEqual(report['deleted_ids'], [])

    def test_one_stable_replacement_is_enough_without_picking_a_newest_environment(self):
        old = rust_cache(1, 'release-packages-web-wasm-trusted-v2')
        replacement_a = rust_cache(2, 'release-packages-web-wasm-trusted-v3')
        replacement_b = rust_cache(3, 'release-packages-web-wasm-trusted-v3', environment='c')
        current = cache(4)
        first = [old, replacement_a, replacement_b, current]
        second = [old, replacement_a, dict(replacement_b, size_in_bytes=0), current]
        api = api_with(first, second, second[1:])
        report = maintenance.maintain(api, current['key'], START, apply=True)
        api.delete_cache.assert_called_once_with(1)
        self.assertEqual(report['deleted_ids'], [1])

    def test_later_replacement_and_later_active_run_cannot_expand_the_deletion_set(self):
        old = rust_cache(1, 'ci-full-regression-trusted-v2')
        new = rust_cache(2, 'ci-shared-native-trusted-v3')
        current = cache(3)
        first, second = [old, current], [old, new, current]
        api = api_with(first, second, second)
        report = maintenance.maintain(api, current['key'], START, apply=True)
        api.delete_cache.assert_not_called()
        self.assertEqual(report['deleted_ids'], [])

        api = api_with(second, second, second)
        api.list_active_runs.side_effect = [[], [active_run()]]
        report = maintenance.maintain(api, current['key'], START, apply=True)
        api.delete_cache.assert_not_called()
        self.assertEqual(report['deleted_ids'], [])

    def test_either_active_run_inventory_failure_prevents_all_deletes(self):
        entries = [rust_cache(1, 'ci-full-regression-trusted-v2'),
                   rust_cache(2, 'ci-shared-native-trusted-v3'), cache(3)]
        for snapshots in ([maintenance.MaintenanceError('active page two failed')],
                          [[], maintenance.MaintenanceError('active page two failed')]):
            with self.subTest(snapshots=snapshots):
                api = api_with(entries, entries, entries)
                api.list_active_runs.side_effect = snapshots
                report = maintenance.maintain(api, entries[-1]['key'], START, apply=True)
                api.delete_cache.assert_not_called()
                self.assertEqual(report['status'], 'skipped')
                self.assertTrue(report['warnings'])


class ApiTests(unittest.TestCase):
    def test_active_run_statuses_are_completely_paginated_and_cross_status_moves_are_deduplicated(self):
        api = maintenance.GitHubApi('nil-studio/oasis7', '')
        runs = [active_run(i, status='requested') for i in range(1, 102)]
        responses = [{'total_count': 101, 'workflow_runs': runs[:100]},
                     {'total_count': 101, 'workflow_runs': runs[100:]},
                     {'total_count': 0, 'workflow_runs': []},
                     {'total_count': 0, 'workflow_runs': []},
                     {'total_count': 0, 'workflow_runs': []},
                     {'total_count': 1, 'workflow_runs': [dict(runs[0], status='in_progress')]}]
        api.request = Mock(side_effect=responses)
        actual = api.list_active_runs()
        self.assertEqual({run['id'] for run in actual}, set(range(1, 102)))
        paths = [call.args[1] for call in api.request.call_args_list]
        self.assertTrue(any('page=2' in path for path in paths))
        for status in ('requested', 'pending', 'waiting', 'queued', 'in_progress'):
            self.assertTrue(any(f'status={status}' in path for path in paths), status)

    def test_active_run_incomplete_or_inconsistent_pages_fail_closed(self):
        runs = [active_run(i, status='requested') for i in range(1, 101)]
        failures = [{'total_count': 101, 'workflow_runs': []},
                    {'total_count': 101, 'workflow_runs': [runs[0]]},
                    {'total_count': 102, 'workflow_runs': [active_run(101)]},
                    maintenance.MaintenanceError('HTTP 500')]
        for second in failures:
            with self.subTest(second=second):
                api = maintenance.GitHubApi('nil-studio/oasis7', '')
                api.request = Mock(side_effect=[{'total_count': 101, 'workflow_runs': runs}, second])
                with self.assertRaises(maintenance.MaintenanceError):
                    api.list_active_runs()

    def test_active_run_unknown_identity_status_and_api_filter_limit_fail_closed(self):
        invalid = [active_run(id=True), active_run(created='bad'), active_run(status='unknown'),
                   active_run(head_sha='bad'), active_run(repository={'full_name': 'other/repo'})]
        for run in invalid:
            with self.subTest(run=run):
                api = maintenance.GitHubApi('nil-studio/oasis7', '')
                api.request = Mock(return_value={'total_count': 1, 'workflow_runs': [run]})
                with self.assertRaises(maintenance.MaintenanceError):
                    api.list_active_runs()
        api = maintenance.GitHubApi('nil-studio/oasis7', '')
        api.request = Mock(return_value={'total_count': 1001, 'workflow_runs': []})
        with self.assertRaises(maintenance.MaintenanceError):
            api.list_active_runs()

    def test_complete_pagination_and_deterministic_created_sort(self):
        api = maintenance.GitHubApi('nil-studio/oasis7', '')
        entries = [cache(i) for i in range(1, 102)]
        api.request = Mock(side_effect=[{'total_count': 101, 'actions_caches': entries[:100]},
                                       {'total_count': 101, 'actions_caches': entries[100:]}])
        self.assertEqual(api.list_caches(), entries)
        paths = [call.args[1] for call in api.request.call_args_list]
        self.assertIn('page=2', paths[1])
        self.assertIn('sort=created_at', paths[0])
        self.assertIn('direction=asc', paths[0])

    def test_truncated_duplicate_changed_count_and_failed_page_are_rejected(self):
        entries = [cache(i) for i in range(1, 101)]
        failures = [{'total_count': 101, 'actions_caches': []},
                    {'total_count': 101, 'actions_caches': [entries[0]]},
                    {'total_count': 102, 'actions_caches': [cache(101)]},
                    maintenance.MaintenanceError('HTTP 500')]
        for second in failures:
            with self.subTest(second=second):
                api = maintenance.GitHubApi('nil-studio/oasis7', '')
                api.request = Mock(side_effect=[{'total_count': 101, 'actions_caches': entries}, second])
                with self.assertRaises(maintenance.MaintenanceError):
                    api.list_caches()

    def test_api_origin_requests_and_runtime_are_bounded(self):
        for repo in ('../x', 'owner/../other', 'https://example.com/repo'):
            with self.subTest(repo=repo), self.assertRaises(maintenance.MaintenanceError):
                maintenance.GitHubApi(repo, 'secret')
        api = maintenance.GitHubApi('nil-studio/oasis7', 'secret')
        with self.assertRaises(maintenance.MaintenanceError):
            api.request('GET', 'https://example.com/caches')
        api.requests = maintenance.MAX_REQUESTS
        with self.assertRaisesRegex(maintenance.MaintenanceError, 'budget'):
            api.request('GET', '/actions/caches')
        api.requests = 0
        api.deadline = 0
        with self.assertRaisesRegex(maintenance.MaintenanceError, 'budget'):
            api.request('GET', '/actions/caches')

    def test_http_diagnostics_expose_only_validated_fields(self):
        api = maintenance.GitHubApi('nil-studio/oasis7', 'secret-token')
        error = HTTPError('https://example.com/secret-url', 403, 'secret-reason', {
            'X-Accepted-GitHub-Permissions': 'actions=read',
            'X-GitHub-Request-Id': 'ABCD:1234:5678',
            'X-RateLimit-Remaining': '123',
            'Authorization': 'secret-token',
            'Set-Cookie': 'secret-cookie',
        }, io.BytesIO(b'{"message":"Resource not accessible by integration","secret":"secret-body"}'))
        api.opener.open = Mock(side_effect=error)
        with self.assertRaises(maintenance.MaintenanceApiError) as caught:
            api.request('GET', '/actions/cache/storage-limit')
        failure = caught.exception
        self.assertEqual(failure.status, 403)
        self.assertFalse(failure.rate_limited)
        self.assertEqual(failure.diagnostics, dict(http_status=403, accepted_permissions='actions=read',
                         request_id='ABCD:1234:5678', rate_limit_remaining=123, category='integration_access'))
        self.assertNotIn('secret', str(failure) + json.dumps(failure.diagnostics))

    def test_http_diagnostics_distinguish_rate_limits_and_reject_injected_headers(self):
        for headers, body in [({'X-RateLimit-Remaining': '0'}, b'{}'),
                              ({'Retry-After': '30'}, b'{}'),
                              ({'Retry-After': '0'}, b'{}'),
                              ({'Retry-After': 'Wed, 21 Oct 2026 10:00:00 GMT'}, b'{}'),
                              ({}, b'{"message":"API rate limit exceeded for secret-user"}')]:
            with self.subTest(headers=headers, body=body):
                error = HTTPError('secret-url', 403, 'secret', dict(headers,
                    **{'X-Accepted-GitHub-Permissions': 'actions=read\nsecret',
                       'X-GitHub-Request-Id': 'secret-token'}), io.BytesIO(body))
                failure = maintenance.MaintenanceApiError('GET', error)
                self.assertTrue(failure.rate_limited)
                self.assertNotIn('accepted_permissions', failure.diagnostics)
                self.assertNotIn('request_id', failure.diagnostics)
                self.assertNotIn('secret', str(failure) + json.dumps(failure.diagnostics))
class TrustTests(unittest.TestCase):
    def environment(self):
        return {'GITHUB_ACTIONS': 'true', 'GITHUB_REPOSITORY': 'nil-studio/oasis7',
                'GITHUB_REF': 'refs/heads/main', 'GITHUB_SHA': f'{3:040x}',
                'GITHUB_WORKFLOW_SHA': f'{3:040x}',
                'GITHUB_EVENT_NAME': 'push', 'GITHUB_RUN_ID': '42',
                'GITHUB_WORKFLOW_REF': 'nil-studio/oasis7/.github/workflows/rust.yml@refs/heads/main'}

    def run_metadata(self):
        return {'id': 42, 'head_sha': f'{3:040x}', 'head_branch': 'main',
                'event': 'push', 'path': '.github/workflows/rust.yml', 'created_at': START,
                'repository': {'full_name': 'nil-studio/oasis7'}}

    def test_bootstrap_requires_the_same_trusted_main_run_without_a_source_key(self):
        api = Mock(repository='nil-studio/oasis7')
        api.request.return_value = self.run_metadata()
        self.assertEqual(maintenance.trusted_run_start(api, self.environment(), bootstrap=True), START)
        api.request.assert_called_once_with('GET', '/actions/runs/42')
        for override in ({'GITHUB_REF': 'refs/pull/1/merge'}, {'GITHUB_REF': 'refs/tags/v1.0.0'},
                         {'GITHUB_ACTIONS': 'false'}, {'GITHUB_EVENT_NAME': 'pull_request'},
                         {'GITHUB_WORKFLOW_SHA': f'{4:040x}'}, {'GITHUB_REPOSITORY': 'other/repo'},
                         {'GITHUB_WORKFLOW_REF': 'nil-studio/oasis7/.github/workflows/rust.yml@feature'}):
            with self.subTest(override=override):
                api.reset_mock()
                with self.assertRaises(maintenance.MaintenanceError):
                    maintenance.trusted_run_start(api, dict(self.environment(), **override), bootstrap=True)
                api.request.assert_not_called()
        api.request.return_value = dict(self.run_metadata(), head_branch='feature')
        with self.assertRaises(maintenance.MaintenanceError):
            maintenance.trusted_run_start(api, self.environment(), bootstrap=True)
        with self.assertRaises(maintenance.MaintenanceError):
            maintenance.trusted_run_start(api, self.environment(), cache(3)['key'], bootstrap=True)

    def test_bootstrap_manual_execution_must_still_be_full(self):
        with tempfile.TemporaryDirectory() as temporary:
            event = Path(temporary) / 'event.json'
            env = dict(self.environment(), GITHUB_EVENT_NAME='workflow_dispatch', GITHUB_EVENT_PATH=str(event))
            api = Mock(repository='nil-studio/oasis7')
            api.request.return_value = dict(self.run_metadata(), event='workflow_dispatch')
            for mode in ('', 'newapi_bridge_package', 'full'):
                event.write_text(json.dumps({'inputs': {'run_mode': mode}}))
                if mode == 'full':
                    self.assertEqual(maintenance.trusted_run_start(api, env, bootstrap=True), START)
                else:
                    with self.assertRaises(maintenance.MaintenanceError):
                        maintenance.trusted_run_start(api, env, bootstrap=True)

    def test_bootstrap_rejects_ecs_dispatch_before_any_api_call(self):
        api = Mock(repository='nil-studio/oasis7')
        env = dict(self.environment(), GITHUB_EVENT_NAME='workflow_dispatch',
                   GITHUB_WORKFLOW_REF='nil-studio/oasis7/.github/workflows/ecs-packages.yml@refs/heads/main')
        with self.assertRaises(maintenance.MaintenanceError):
            maintenance.trusted_run_start(api, env, bootstrap=True)
        api.request.assert_not_called()

    def test_bootstrap_cli_without_a_saved_key_revalidates_and_requires_scoped_token(self):
        entries = [cache(1), cache(2)]
        api = api_with(entries, entries, entries[1:])
        api.repository = 'nil-studio/oasis7'
        api.request.return_value = self.run_metadata()
        env = dict(self.environment(), CACHE_MAINTENANCE_TOKEN='scoped-token')
        with patch.dict(os.environ, env, clear=True), \
                patch.object(maintenance, 'GitHubApi', return_value=api), \
                patch('sys.stdout', new_callable=io.StringIO) as output:
            self.assertEqual(maintenance.main(['--bootstrap', '--apply']), 0)
        report = json.loads(output.getvalue())
        self.assertEqual(report['deleted_ids'], [1])
        self.assertTrue(report['bootstrap'])
        api.request.assert_called_once_with('GET', '/actions/runs/42')
        api.delete_cache.assert_called_once_with(1)
        for flags in (['--bootstrap', '--apply'],
                      ['--bootstrap', '--apply', '--current-key', entries[-1]['key']]):
            with self.subTest(flags=flags), patch.dict(os.environ, self.environment(), clear=True), \
                    patch.object(maintenance, 'GitHubApi') as api_class, \
                    patch('sys.stdout', new_callable=io.StringIO) as output:
                self.assertEqual(maintenance.main(flags), 0)
                self.assertIn('::warning::', output.getvalue())
                api_class.assert_not_called()

    def test_real_run_identity_is_required_and_old_run_timestamp_is_used(self):
        api = Mock(repository='nil-studio/oasis7')
        api.request.return_value = self.run_metadata()
        self.assertEqual(maintenance.trusted_run_start(api, self.environment(), cache(3)['key']), START)
        api.request.assert_called_once_with('GET', '/actions/runs/42')
        for override in ({'head_branch': 'feature'}, {'head_sha': f'{4:040x}'},
                         {'id': 43}, {'path': '.github/workflows/other.yml'}, {'event': 'pull_request'}):
            with self.subTest(override=override), self.assertRaises(maintenance.MaintenanceError):
                api.request.return_value = dict(self.run_metadata(), **override)
                maintenance.trusted_run_start(api, self.environment(), cache(3)['key'])

    def test_pr_tag_local_and_wrong_sha_are_rejected_before_any_api_call(self):
        for override in ({'GITHUB_REF': 'refs/pull/1/merge'}, {'GITHUB_REF': 'refs/tags/v1.0.0'},
                         {'GITHUB_ACTIONS': 'false'}, {'GITHUB_EVENT_NAME': 'pull_request'},
                         {'GITHUB_SHA': f'{4:040x}'}, {'GITHUB_WORKFLOW_SHA': ''},
                         {'GITHUB_WORKFLOW_SHA': f'{4:040x}'}, {'GITHUB_WORKFLOW_REF': 'other/repo/f.yml@main'}):
            with self.subTest(override=override):
                api = Mock(repository='nil-studio/oasis7')
                with self.assertRaises(maintenance.MaintenanceError):
                    maintenance.trusted_run_start(api, dict(self.environment(), **override), cache(3)['key'])
                api.request.assert_not_called()

    def test_manual_event_payload_must_request_full(self):
        with tempfile.TemporaryDirectory() as temporary:
            event = Path(temporary) / 'event.json'
            env = dict(self.environment(), GITHUB_EVENT_NAME='workflow_dispatch', GITHUB_EVENT_PATH=str(event))
            api = Mock(repository='nil-studio/oasis7')
            api.request.return_value = dict(self.run_metadata(), event='workflow_dispatch')
            for mode in ('', 'newapi_bridge_package', 'full'):
                event.write_text(json.dumps({'inputs': {'run_mode': mode}}))
                if mode == 'full':
                    self.assertEqual(maintenance.trusted_run_start(api, env, cache(3)['key']), START)
                else:
                    with self.assertRaises(maintenance.MaintenanceError):
                        maintenance.trusted_run_start(api, env, cache(3)['key'])

    def test_rust_accepts_explicit_formal_and_ordinary_but_not_ecs(self):
        api = Mock(repository='nil-studio/oasis7')
        keys = [WorkspaceRetentionTests.entry(3, name)['key'] for name in ('formal', 'ordinary')]
        for event in ('push', 'schedule'):
            api.request.return_value = dict(self.run_metadata(), event=event)
            env = dict(self.environment(), GITHUB_EVENT_NAME=event)
            self.assertEqual(maintenance.trusted_run_start(api, env, keys), START)
        for invalid in ([*keys, WorkspaceRetentionTests.entry(3)['key']],
                        [keys[0], WorkspaceRetentionTests.entry(4, 'ordinary')['key']]):
            api.reset_mock()
            with self.assertRaises(maintenance.MaintenanceError):
                maintenance.trusted_run_start(api, self.environment(), invalid)
            api.request.assert_not_called()

    def test_ecs_dispatch_is_scoped_to_ecs_family_and_rest_workflow_identity(self):
        api = Mock(repository='nil-studio/oasis7')
        env = dict(self.environment(), GITHUB_EVENT_NAME='workflow_dispatch',
                   GITHUB_WORKFLOW_REF='nil-studio/oasis7/.github/workflows/ecs-packages.yml@refs/heads/main')
        api.request.return_value = dict(self.run_metadata(), event='workflow_dispatch',
                                        path='.github/workflows/ecs-packages.yml')
        key = WorkspaceRetentionTests.entry(3)['key']
        self.assertEqual(maintenance.trusted_run_start(api, env, key), START)
        for keys, overrides in (([cache(3)['key']], {}), ([key], {'GITHUB_EVENT_NAME': 'push'}),
                                ([key], {'GITHUB_EVENT_NAME': 'schedule'})):
            api.reset_mock()
            with self.assertRaises(maintenance.MaintenanceError):
                maintenance.trusted_run_start(api, dict(env, **overrides), keys)
            api.request.assert_not_called()
        api.request.return_value = dict(self.run_metadata(), event='workflow_dispatch')
        with self.assertRaises(maintenance.MaintenanceError):
            maintenance.trusted_run_start(api, env, key)

    def test_cli_passes_repeated_anchors_without_choosing_a_latest_key(self):
        entries = [WorkspaceRetentionTests.entry(3, name) for name in ('formal', 'ordinary')]
        entries[1]['id'] = 4
        api = api_with(entries)
        api.repository = 'nil-studio/oasis7'
        api.request.return_value = self.run_metadata()
        with patch.dict(os.environ, self.environment(), clear=True), \
                patch.object(maintenance, 'GitHubApi', return_value=api), \
                patch('sys.stdout', new_callable=io.StringIO) as output:
            self.assertEqual(maintenance.main(['--current-key', entries[0]['key'],
                                                '--current-key', entries[1]['key']]), 0)
        result = json.loads(output.getvalue())
        self.assertEqual(set(result['plan']['families']), {'formal', 'ordinary'})
        self.assertEqual(result['plan']['unanchored_families'], ['ecs'])
        api.delete_cache.assert_not_called()

    def test_api_failure_is_advisory_and_writes_a_summary(self):
        with tempfile.TemporaryDirectory() as temporary:
            summary = Path(temporary) / 'summary.md'
            with patch.dict(os.environ, self.environment(), clear=True), \
                    patch.object(maintenance.GitHubApi, 'list_caches', side_effect=maintenance.MaintenanceError('HTTP 403')), \
                    patch('sys.stdout', new_callable=io.StringIO) as output:
                result = maintenance.main(['--summary', str(summary)])
            self.assertEqual(result, 0)
            self.assertIn('HTTP 403', summary.read_text())
            self.assertIn('::warning::', output.getvalue())


if __name__ == '__main__':
    unittest.main()
