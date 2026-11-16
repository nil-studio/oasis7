#!/usr/bin/env python3
"""Exercise the actual dependency archive checkpoint without remote writes."""
import importlib.util
import io
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import Mock, patch
from urllib.error import HTTPError


spec = importlib.util.spec_from_file_location('admission', Path(__file__).with_name('ci-rust-cache-admission.py'))
admission = importlib.util.module_from_spec(spec)
spec.loader.exec_module(admission)


class AdmissionTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.workspace = self.root / 'workspace'
        self.workspace.mkdir()
        subprocess.run(['git', 'init', '-q', '-b', 'main', str(self.workspace)], check=True)
        subprocess.run(['git', '-C', str(self.workspace), '-c', 'user.name=CI fixture',
                        '-c', 'user.email=ci-fixture@example.invalid', 'commit', '-q',
                        '--allow-empty', '-m', 'trusted cache source'], check=True)
        self.source_sha = subprocess.check_output(
            ['git', '-C', str(self.workspace), 'rev-parse', 'HEAD'], text=True).strip()
        self.archive_dir = self.root / 'cache-run'
        self.archive_dir.mkdir()
        self.archive = self.archive_dir / 'cache.tgz'
        self.archive.write_bytes(b'x' * 100)
        (self.archive_dir / 'manifest.txt').write_text('payload\n')
        self.environment = dict(os.environ, GITHUB_ACTIONS='true', RUNNER_OS='Linux', RUNNER_ARCH='X64',
            GITHUB_REPOSITORY='nil-studio/oasis7', GITHUB_REF='refs/heads/main', GITHUB_SHA=self.source_sha,
            GITHUB_WORKFLOW_SHA=self.source_sha, GITHUB_RUN_ID='42', GITHUB_EVENT_NAME='schedule',
            GITHUB_JOB='shared-native-cache', GITHUB_WORKFLOW_REF='nil-studio/oasis7/.github/workflows/rust-cache.yml@refs/heads/main',
            GITHUB_WORKSPACE=str(self.workspace), RUNNER_TEMP=str(self.root), GH_TOKEN='fixture-secret',
            OASIS7_DEPENDENCY_CACHE_FAMILY='shared-native', OASIS7_DEPENDENCY_CACHE_BUDGET_BYTES='1000')
        self.environment['INPUT_SHARED-KEY'] = 'ci-shared-native-trusted-v4'
        self.api = Mock()
        self.api.repository = 'nil-studio/oasis7'
        self.run = dict(id=42, head_sha=self.source_sha, head_branch='main', event='schedule',
                        path='.github/workflows/rust-cache.yml', repository={'full_name': 'nil-studio/oasis7'})
        self.api.request.side_effect = lambda method, path: self.run if path == '/actions/runs/42' else {'max_cache_size_gb': 10}
        self.api.list_caches.return_value = []

    def entry(self, size=900, **overrides):
        result = dict(id=1, key='other-family', version='v1', ref='refs/pull/1/merge',
                      size_in_bytes=size, created_at='2026-10-24T00:00:00Z', last_accessed_at='2026-10-24T00:00:00Z')
        return result | overrides

    def args(self):
        return ['--posix', '-cf', 'cache.tgz', '--exclude', 'cache.tgz', '-P', '-C',
                str(self.workspace), '--files-from', 'manifest.txt', '-z']

    def test_report_family_preserves_only_registered_public_labels(self):
        for family in admission.FAMILIES:
            with self.subTest(family=family):
                environment = self.environment | {'OASIS7_DEPENDENCY_CACHE_FAMILY': family}
                self.assertEqual(admission.empty_report(environment)['family'], family)
        for family in (None, '', 'unknown', 'SHARED-NATIVE', 'shared-native ',
                       'fixture-only-secret\n::error::untrusted-family'):
            with self.subTest(family=family):
                environment = self.environment | {'OASIS7_DEPENDENCY_CACHE_FAMILY': family}
                self.assertIsNone(admission.empty_report(environment)['family'])

    def test_unknown_family_is_not_echoed_at_any_denied_checkpoint(self):
        sentinel = 'fixture-only-secret\n::error::untrusted-family'
        environment = self.environment | {'OASIS7_DEPENDENCY_CACHE_FAMILY': sentinel,
                                         'GITHUB_STEP_SUMMARY': str(self.root / 'summary.md')}
        cases = (
            ('writer', lambda: admission.check_archive(self.archive, environment, self.api),
             'unknown dependency cache writer family'),
            ('archive', lambda: admission.check_archive(self.root / 'missing', environment, self.api),
             'cache archive or event could not be read'),
        )
        for name, check, reason in cases:
            with self.subTest(checkpoint=name), patch('sys.stdout', new_callable=io.StringIO) as output:
                report = check()
                admission.emit_report(report, environment)
                self.assertFalse(report['allowed'])
                self.assertEqual(report['reason'], reason)
                self.assertIsNone(json.loads(output.getvalue().splitlines()[0])['family'])
                self.assertNotIn('fixture-only-secret', output.getvalue())
        with patch('sys.stdout', new_callable=io.StringIO) as output, \
                patch.object(admission.subprocess, 'run') as tar:
            result = admission.run_tar(['-cf', 'cache.tgz'], environment, self.archive_dir, api=self.api)
        self.assertEqual(result, admission.DENIED)
        self.assertIsNone(json.loads(output.getvalue().splitlines()[0])['family'])
        self.assertNotIn('fixture-only-secret', output.getvalue())
        tar.assert_not_called()
        with patch.dict(os.environ, environment, clear=True), \
                patch('sys.stdout', new_callable=io.StringIO) as output:
            result = admission.main(['check', '--archive', str(self.root / 'missing')])
        self.assertEqual(result, admission.DENIED)
        self.assertIsNone(json.loads(output.getvalue().splitlines()[0])['family'])
        self.assertNotIn('fixture-only-secret', output.getvalue())
        self.assertNotIn('fixture-only-secret', (self.root / 'summary.md').read_text())
        self.api.request.assert_not_called()
        self.api.list_caches.assert_not_called()

    def test_final_compressed_bytes_count_every_family_and_ref(self):
        self.api.list_caches.return_value = [self.entry()]
        result = admission.check_archive(self.archive, self.environment, self.api)
        self.assertTrue(result['allowed'])
        self.assertEqual(result['archive_bytes'], 100)
        self.assertEqual(result['projected_repository_bytes'], 1000)
        self.api.list_caches.return_value = [self.entry(901)]
        result = admission.check_archive(self.archive, self.environment, self.api)
        self.assertFalse(result['allowed'])
        self.assertEqual(result['reason'], 'repository cache capacity gap')
        self.api.delete_cache.assert_not_called()

    def test_formal_dependency_writers_bind_profile_namespace_and_main_full_events(self):
        for component in ('debug', 'endpoint'):
            family = 'formal-' + component
            namespace = 'ci-' + family + '-deps-trusted-v1'
            environment = self.environment | dict(OASIS7_DEPENDENCY_CACHE_FAMILY=family,
                                                  GITHUB_JOB=family + '-build-trusted',
                                                  GITHUB_WORKFLOW_REF='nil-studio/oasis7/.github/workflows/rust.yml@refs/heads/main')
            environment['INPUT_SHARED-KEY'] = namespace
            self.run['event'] = 'schedule'
            self.run['path'] = '.github/workflows/rust.yml'
            with self.subTest(component=component):
                self.assertTrue(admission.check_archive(self.archive, environment, self.api)['allowed'])
                for change in ({'GITHUB_JOB': family + '-build'}, {'GITHUB_JOB': 'formal-test-build-trusted'},
                               {'INPUT_SHARED-KEY': 'ci-shared-native-trusted-v3'},
                               {'INPUT_SHARED-KEY': 'ci-shared-native-trusted-v4'},
                               {'GITHUB_WORKFLOW_REF': 'nil-studio/oasis7/.github/workflows/rust-cache.yml@refs/heads/main'},
                               {'INPUT_SHARED-KEY': 'ci-formal-' + ('endpoint' if component == 'debug' else 'debug') + '-deps-trusted-v1'},
                               {'GITHUB_EVENT_NAME': 'push'}, {'GITHUB_EVENT_NAME': 'pull_request'},
                               {'GITHUB_REF': 'refs/pull/1/merge'}, {'RUNNER_ARCH': 'ARM64'},
                               {'GITHUB_WORKFLOW_SHA': 'b' * 40}):
                    with self.subTest(change=change):
                        self.assertFalse(admission.check_archive(self.archive, environment | change, self.api)['allowed'])
                self.run['path'] = '.github/workflows/rust-cache.yml'
                self.assertFalse(admission.check_archive(self.archive, environment, self.api)['allowed'])
                self.run['path'] = '.github/workflows/rust.yml'
                event = self.root / (component + '-event.json')
                environment.update(GITHUB_EVENT_NAME='workflow_dispatch', GITHUB_EVENT_PATH=str(event))
                self.run['event'] = 'workflow_dispatch'
                for mode in ('full', 'pr-fast', None):
                    event.write_text(json.dumps({'inputs': {'run_mode': mode}}))
                    self.assertEqual(admission.check_archive(self.archive, environment, self.api)['allowed'], mode == 'full')
                event.write_text(json.dumps({'inputs': {'run_mode': 'full'}}))
                self.api.list_caches.return_value = [self.entry(901)]
                self.assertFalse(admission.check_archive(self.archive, environment, self.api)['allowed'])
                self.api.list_caches.return_value = []
        self.api.delete_cache.assert_not_called()

    def test_archive_growth_during_inventory_cannot_reuse_admission(self):
        def changed():
            self.archive.write_bytes(b'x' * 101)
            return []
        self.api.list_caches.side_effect = changed
        result = admission.check_archive(self.archive, self.environment, self.api)
        self.assertFalse(result['allowed'])
        self.assertIn('changed', result['reason'])

    def test_only_recognized_quota_denial_uses_configured_budget(self):
        def denial(code, message):
            return admission.maintenance.MaintenanceApiError('GET', HTTPError(
                'https://api.github.com', code, 'denied', {}, io.BytesIO(json.dumps({'message': message}).encode())))
        for error, allowed in ((denial(403, 'Resource not accessible by integration'), True),
                               (denial(403, 'API rate limit exceeded'), False),
                               (denial(500, 'unknown'), False)):
            with self.subTest(error=error, allowed=allowed):
                self.api.request.side_effect = lambda method, path: self.run if path == '/actions/runs/42' else (_ for _ in ()).throw(error)
                result = admission.check_archive(self.archive, self.environment, self.api)
                self.assertEqual(result['allowed'], allowed)
                if allowed:
                    self.assertIsNone(result['live_repository_limit_bytes'])
                    self.assertEqual(result['capacity_basis'], 'verified_configured_budget')

    def test_missing_inventory_invalid_budget_and_bad_archive_fail_closed(self):
        self.api.list_caches.side_effect = admission.maintenance.MaintenanceError('page two failed')
        self.assertFalse(admission.check_archive(self.archive, self.environment, self.api)['allowed'])
        self.api.list_caches.side_effect = None
        for budget in ('', '0', 'unknown'):
            with self.subTest(budget=budget):
                self.assertFalse(admission.check_archive(self.archive, self.environment |
                    {'OASIS7_DEPENDENCY_CACHE_BUDGET_BYTES': budget}, self.api)['allowed'])
        self.archive.unlink()
        self.assertFalse(admission.check_archive(self.archive, self.environment, self.api)['allowed'])
        self.archive.symlink_to(self.archive_dir / 'manifest.txt')
        self.assertFalse(admission.check_archive(self.archive, self.environment, self.api)['allowed'])

    def test_candidate_stale_workflow_wrong_job_and_wrong_family_cannot_upload(self):
        for change in ({'GITHUB_REF': 'refs/pull/5/merge'}, {'GITHUB_WORKFLOW_SHA': 'b' * 40},
                       {'GITHUB_JOB': 'candidate-full-regression'}, {'GITHUB_JOB': 'full-regression'},
                       {'GITHUB_EVENT_NAME': 'push'}, {'GITHUB_EVENT_NAME': 'workflow_run'},
                       {'GITHUB_ACTIONS': 'false'}, {'INPUT_SHARED-KEY': 'another-cache'},
                       {'INPUT_SHARED-KEY': 'ci-shared-native-trusted-v3'},
                       {'GITHUB_WORKFLOW_REF': 'nil-studio/oasis7/.github/workflows/rust.yml@refs/heads/main'},
                       {'GITHUB_WORKFLOW_REF': 'nil-studio/oasis7/.github/workflows/rust-cache.yml@refs/heads/topic'},
                       {'OASIS7_DEPENDENCY_CACHE_FAMILY': ''}, {'RUNNER_OS': 'Windows'}):
            with self.subTest(change=change):
                self.assertFalse(admission.check_archive(self.archive, self.environment | change, self.api)['allowed'])
        self.run['head_sha'] = 'b' * 40
        self.assertFalse(admission.check_archive(self.archive, self.environment, self.api)['allowed'])

    def test_shared_native_writer_verifies_real_checkout_before_upload(self):
        # The event and workflow can agree while the workspace contains another
        # source. Check the actual local Git boundary, not a forged HEAD string.
        self.assertTrue(admission.check_archive(self.archive, self.environment, self.api)['allowed'])
        subprocess.run(['git', '-C', str(self.workspace), '-c', 'user.name=CI fixture',
                        '-c', 'user.email=ci-fixture@example.invalid', 'commit', '-q',
                        '--allow-empty', '-m', 'different unchecked source'], check=True)
        self.api.list_caches.reset_mock()
        rejected = admission.check_archive(self.archive, self.environment, self.api)
        self.assertFalse(rejected['allowed'])
        self.api.list_caches.assert_not_called()
        (self.workspace / '.git').rename(self.workspace / 'unavailable-git')
        self.assertFalse(admission.check_archive(self.archive, self.environment, self.api)['allowed'])

    def test_shared_native_writer_requires_exact_rest_run_identity(self):
        original = self.run.copy()
        self.assertTrue(admission.check_archive(self.archive, self.environment, self.api)['allowed'])
        for changes in ({'path': '.github/workflows/rust.yml'}, {'id': 43},
                        {'head_branch': 'topic'}, {'event': 'workflow_run'},
                        {'head_sha': 'b' * 40}, {'repository': {'full_name': 'another/oasis7'}}):
            with self.subTest(changes=changes):
                self.run = original | changes
                self.api.list_caches.reset_mock()
                self.assertFalse(admission.check_archive(self.archive, self.environment, self.api)['allowed'])
                self.api.list_caches.assert_not_called()

    def test_manual_full_event_and_ecs_native_are_verified(self):
        event = self.root / 'event.json'
        event.write_text(json.dumps({'inputs': {'run_mode': 'full'}}))
        self.environment.update(GITHUB_EVENT_NAME='workflow_dispatch', GITHUB_EVENT_PATH=str(event))
        self.run['event'] = 'workflow_dispatch'
        self.assertTrue(admission.check_archive(self.archive, self.environment, self.api)['allowed'])
        for payload in ({'inputs': {'run_mode': 'newapi_bridge_package'}},
                        {'inputs': {'run_mode': ''}}, {'inputs': {}}, {}, {'inputs': None}):
            event.write_text(json.dumps(payload))
            with self.subTest(payload=payload):
                self.assertFalse(admission.check_archive(self.archive, self.environment, self.api)['allowed'])
        self.environment.update(OASIS7_DEPENDENCY_CACHE_FAMILY='ecs', GITHUB_JOB='native-trusted',
            GITHUB_WORKFLOW_REF='nil-studio/oasis7/.github/workflows/ecs-packages.yml@refs/heads/main')
        self.environment['INPUT_SHARED-KEY'] = 'oasis7-ecs-native-linux-ecs-v2'
        self.run['path'] = '.github/workflows/ecs-packages.yml'
        self.assertTrue(admission.check_archive(self.archive, self.environment, self.api)['allowed'])

    def test_release_writers_require_matching_platform_source_and_workflow(self):
        web = admission.FAMILIES['release-web']
        self.assertIn(web, (
            ('release-packages-web-wasm-trusted-v3', '.github/workflows/release-packages.yml', 'build-web-dist'),
            ('release-packages-web-wasm-trusted-v3', '.github/workflows/rust-cache.yml', 'web-wasm-cache'),
        ))
        cases = (
            ('release-web', web[2], 'Linux', 'X64', web[0]),
            ('release-linux', 'package-native', 'Linux', 'X64', 'oasis7-package-native-Linux-native-packaging-trusted-v2'),
            ('release-windows', 'package-native', 'Windows', 'X64', 'release-packages-package-native-Windows-native-trusted-v2'),
            ('release-macos', 'package-native', 'macOS', 'ARM64', 'release-packages-package-native-macOS-x86_64-apple-darwin-trusted-v2'),
        )
        event = self.root / 'release-event.json'
        event.write_text(json.dumps({'inputs': {'run_mode': 'full'}}))
        for family, job, runner, arch, namespace in cases:
            workflow = web[1] if family == 'release-web' else '.github/workflows/release-packages.yml'
            self.run.update(event='workflow_dispatch', path=workflow)
            environment = self.environment | dict(OASIS7_DEPENDENCY_CACHE_FAMILY=family,
                GITHUB_JOB=job, RUNNER_OS=runner, RUNNER_ARCH=arch, GITHUB_EVENT_NAME='workflow_dispatch',
                GITHUB_WORKFLOW_REF=f'nil-studio/oasis7/{workflow}@refs/heads/main',
                GITHUB_EVENT_PATH=str(event),
                OASIS7_DEPENDENCY_CACHE_SOURCE_SHA=self.source_sha)
            environment['INPUT_SHARED-KEY'] = namespace
            environment['INPUT_CACHE-PROVIDER'] = 'github'
            with self.subTest(family=family), patch.object(admission.subprocess, 'check_output', return_value=self.source_sha + '\n'):
                self.assertTrue(admission.check_archive(self.archive, environment, self.api)['allowed'])
                for bad in ({'OASIS7_DEPENDENCY_CACHE_SOURCE_SHA': 'b' * 40},
                            {'GITHUB_WORKFLOW_SHA': 'b' * 40}, {'GITHUB_REF': 'refs/tags/v1'},
                            {'INPUT_CACHE-PROVIDER': 'warpbuild'}, {'GITHUB_EVENT_NAME': 'push'},
                            {'RUNNER_ARCH': 'unknown'}, {'GITHUB_JOB': 'publish'}):
                    self.assertFalse(admission.check_archive(self.archive, environment | bad, self.api)['allowed'])
            with patch.object(admission.subprocess, 'check_output', return_value='b' * 40 + '\n'):
                self.assertFalse(admission.check_archive(self.archive, environment, self.api)['allowed'])

    def test_web_writer_migration_retains_one_identity_and_event_boundary(self):
        namespace = 'release-packages-web-wasm-trusted-v3'
        identities = ((namespace, '.github/workflows/release-packages.yml', 'build-web-dist'),
                      (namespace, '.github/workflows/rust-cache.yml', 'web-wasm-cache'))
        registered = admission.FAMILIES['release-web']
        self.assertIn(registered, identities)
        self.assertEqual([name for name, value in admission.FAMILIES.items() if value[0] == namespace],
                         ['release-web'])
        payload = self.root / 'web-event.json'
        for identity in identities:
            for event in ('schedule', 'workflow_dispatch', 'push', 'pull_request', 'workflow_run'):
                for mode in ('full', 'pr-fast', '', None):
                    payload.write_text(json.dumps({'inputs': {'run_mode': mode}}))
                    self.run.update(path=identity[1], event=event)
                    environment = self.environment | dict(
                        OASIS7_DEPENDENCY_CACHE_FAMILY='release-web', GITHUB_JOB=identity[2],
                        GITHUB_WORKFLOW_REF=f'nil-studio/oasis7/{identity[1]}@refs/heads/main',
                        GITHUB_EVENT_NAME=event, GITHUB_EVENT_PATH=str(payload),
                        OASIS7_DEPENDENCY_CACHE_SOURCE_SHA=self.source_sha)
                    environment['INPUT_SHARED-KEY'] = namespace
                    environment['INPUT_CACHE-PROVIDER'] = 'github'
                    active = identity == registered
                    allowed = active and (event == 'workflow_dispatch' if identity == identities[0]
                                          else event == 'schedule' or event == 'workflow_dispatch' and mode == 'full')
                    with self.subTest(identity=identity, event=event, mode=mode):
                        self.assertEqual(admission.check_archive(self.archive, environment, self.api)['allowed'], allowed)
        payload.write_text(json.dumps({'inputs': {'run_mode': 'full'}}))
        self.run.update(path=registered[1], event='workflow_dispatch')
        environment.update(GITHUB_JOB=registered[2], GITHUB_EVENT_NAME='workflow_dispatch',
                           GITHUB_WORKFLOW_REF=f'nil-studio/oasis7/{registered[1]}@refs/heads/main')
        self.assertTrue(admission.check_archive(self.archive, environment, self.api)['allowed'])
        for change in ({'GITHUB_JOB': 'unregistered-writer'}, {'RUNNER_OS': 'Windows'},
                       {'RUNNER_ARCH': 'ARM64'}, {'INPUT_SHARED-KEY': namespace + '-other'},
                       {'GITHUB_REF': 'refs/pull/1/merge'}, {'GITHUB_WORKFLOW_SHA': 'b' * 40},
                       {'OASIS7_DEPENDENCY_CACHE_SOURCE_SHA': 'b' * 40},
                       {'INPUT_CACHE-PROVIDER': 'warpbuild'}):
            with self.subTest(change=change):
                self.assertFalse(admission.check_archive(self.archive, environment | change, self.api)['allowed'])
        original = self.run.copy()
        for changes in ({'id': 43}, {'path': '.github/workflows/unknown.yml'},
                        {'head_branch': 'topic'}, {'event': 'push'}, {'head_sha': 'b' * 40},
                        {'repository': {'full_name': 'another/oasis7'}}):
            self.run = original | changes
            self.assertFalse(admission.check_archive(self.archive, environment, self.api)['allowed'])
        self.run = original
        self.api.list_caches.return_value = [self.entry(901)]
        self.assertFalse(admission.check_archive(self.archive, environment, self.api)['allowed'])
        self.api.delete_cache.assert_not_called()

    def test_release_guard_installer_refuses_inherited_node_options(self):
        environment = self.environment | {'NODE_OPTIONS': '--require unknown.cjs'}
        with self.assertRaises(admission.AdmissionError):
            admission.install_release_guard(environment)
        ready = admission.install_release_guard(self.environment | {'NODE_OPTIONS': ''})
        self.assertEqual(ready['guard_ready'], 'true')
        self.assertTrue(Path(ready['python']).is_absolute())
        self.assertEqual(ready['node_options'], '--require "' + str(
            Path(admission.__file__).with_suffix('.cjs')).replace('\\', '/') + '"')

    def test_testnet_writer_is_limited_to_the_existing_trusted_packaging_lane(self):
        event = self.root / 'event.json'
        self.run.update(event='workflow_dispatch', path='.github/workflows/testnet-packages.yml')
        environment = self.environment | dict(OASIS7_DEPENDENCY_CACHE_FAMILY='testnet-linux',
            GITHUB_JOB='package-native-trusted', GITHUB_EVENT_NAME='workflow_dispatch',
            GITHUB_EVENT_PATH=str(event), OASIS7_DEPENDENCY_CACHE_SOURCE_SHA=self.source_sha,
            GITHUB_WORKFLOW_REF='nil-studio/oasis7/.github/workflows/testnet-packages.yml@refs/heads/main')
        environment['INPUT_SHARED-KEY'] = 'oasis7-package-native-Linux-native-packaging-trusted-v2'
        environment['INPUT_CACHE-PROVIDER'] = 'github'
        with patch.object(admission.subprocess, 'check_output', return_value=self.source_sha + '\n') as git:
            for scope, profile, allowed in (('linux_only', 'packaging', True),
                    ('all_existing', 'packaging', False), ('linux_only', 'dev', False)):
                event.write_text(json.dumps({'inputs': {'package_scope': scope, 'build_profile': profile}}))
                self.assertEqual(admission.check_archive(self.archive, environment, self.api)['allowed'], allowed)
            event.write_text(json.dumps({'inputs': {'package_scope': 'linux_only', 'build_profile': 'packaging'}}))
            self.assertFalse(admission.check_archive(self.archive,
                environment | {'GITHUB_JOB': 'package-native'}, self.api)['allowed'])
            self.assertFalse(admission.check_archive(self.archive,
                environment | {'GITHUB_WORKFLOW_SHA': 'b' * 40}, self.api)['allowed'])
            self.assertNotIn('GH_TOKEN', git.call_args.kwargs['env'])
            self.assertNotIn('NODE_OPTIONS', git.call_args.kwargs['env'])

    def test_node_preload_uses_the_reviewed_upstream_revision(self):
        hook = Path(__file__).with_name('ci-rust-cache-admission-hook.cjs')
        result = subprocess.run(['node', '-e',
                                 "process.stdout.write(require(process.argv[1]).ACTION_REVISION || '')",
                                 str(hook)], capture_output=True, text=True, timeout=30)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout, admission.ACTION_REVISION,
                         'upstream pin and reviewed Node bundle manifest must be upgraded together')

    def test_node_preload_checkpoint_contract(self):
        result = subprocess.run(['node', str(Path(__file__).with_suffix('.cjs'))],
                                capture_output=True, text=True, timeout=30)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_completed_release_archive_requires_private_directory_and_manifest(self):
        self.assertEqual(admission.completed_archive(self.archive, self.environment), self.archive)
        with self.assertRaises(admission.AdmissionError):
            admission.completed_archive(self.archive,
                self.environment | {'GITHUB_WORKSPACE': str(self.archive_dir)})
        (self.archive_dir / 'manifest.txt').unlink()
        with self.assertRaises((admission.AdmissionError, OSError)):
            admission.completed_archive(self.archive, self.environment)

    def test_real_tar_creates_archive_before_inventory_and_denial_stops_save(self):
        self.archive.unlink()
        (self.workspace / 'payload').write_bytes(b'compressible data' * 20000)
        observed = []
        def inventory():
            observed.append(self.archive.stat().st_size)
            return [self.entry(1000)]
        self.api.list_caches.side_effect = inventory
        with patch('sys.stdout', new_callable=io.StringIO) as output:
            result = admission.run_tar(self.args(), self.environment, self.archive_dir, api=self.api)
        self.assertEqual(result, admission.DENIED)
        self.assertTrue(observed and observed[0] > 0)
        self.assertIn('repository cache capacity gap', output.getvalue())
        self.assertNotIn('fixture-secret', output.getvalue())
        subprocess.run(['/usr/bin/tar', '-tzf', str(self.archive)], check=True, capture_output=True)

    def test_known_create_shape_and_private_archive_directory_are_required(self):
        self.archive.unlink()
        self.assertEqual(admission.archive_for_create(self.args(), self.environment, self.archive_dir), self.archive)
        for args in (['-cf', 'cache.tgz'], self.args() + ['--dereference'],
                     [item.replace('cache.tgz', '../cache.tgz') for item in self.args()]):
            with self.subTest(args=args), patch.object(admission.subprocess, 'run') as tar:
                self.assertEqual(admission.run_tar(args, self.environment, self.archive_dir, api=self.api), admission.DENIED)
                tar.assert_not_called()
        with self.assertRaises(admission.AdmissionError):
            admission.archive_for_create(self.args(), self.environment, self.workspace)

    def test_restore_and_version_passthrough_never_request_inventory(self):
        for args in (['--version'], ['-tf', 'cache.tgz', '-P', '-z'],
                     ['-xf', 'cache.tgz', '-P', '-C', str(self.workspace), '-z']):
            with self.subTest(args=args), patch.object(admission.subprocess, 'run', return_value=Mock(returncode=0)) as tar:
                self.assertEqual(admission.run_tar(args, {}, self.workspace, api=self.api), 0)
                tar.assert_called_once()
        self.api.request.assert_not_called()
        self.api.list_caches.assert_not_called()

    def test_real_tar_failure_never_authorizes_upload(self):
        self.archive.unlink()
        with patch.object(admission.subprocess, 'run', return_value=Mock(returncode=2)):
            self.assertEqual(admission.run_tar(self.args(), self.environment, self.archive_dir, api=self.api), 2)
        self.api.request.assert_not_called()


if __name__ == '__main__':
    unittest.main()
