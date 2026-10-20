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


class TrustTests(unittest.TestCase):
    def environment(self):
        return {'GITHUB_ACTIONS': 'true', 'GITHUB_REPOSITORY': 'nil-studio/oasis7',
                'GITHUB_REF': 'refs/heads/main', 'GITHUB_SHA': f'{3:040x}',
                'GITHUB_EVENT_NAME': 'push', 'GITHUB_RUN_ID': '42',
                'GITHUB_WORKFLOW_REF': 'nil-studio/oasis7/.github/workflows/rust.yml@refs/heads/main'}

    def run_metadata(self):
        return {'id': 42, 'head_sha': f'{3:040x}', 'head_branch': 'main',
                'event': 'push', 'path': '.github/workflows/rust.yml', 'created_at': START,
                'repository': {'full_name': 'nil-studio/oasis7'}}

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
                         {'GITHUB_SHA': f'{4:040x}'}, {'GITHUB_WORKFLOW_REF': 'other/repo/f.yml@main'}):
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
