#!/usr/bin/env python3
"""Exercise fixed tool provisioning with real bounded fixture subprocesses."""
import importlib.util
import io
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import time
import unittest
from unittest import mock
from urllib.error import HTTPError


HELPER = Path(__file__).with_name('ci-worker-build.py')
SPEC = importlib.util.spec_from_file_location('worker_build', HELPER)
C = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(C)

FAKE_CARGO = r'''#!/usr/bin/env python3
import json, os, pathlib, subprocess, sys, time
arguments = sys.argv[1:]
with open(os.environ['WORKER_BUILD_TEST_LOG'], 'a') as stream:
    stream.write(json.dumps({'argv': arguments, 'wrapper': os.environ.get('RUSTC_WRAPPER'),
                             'cwd': os.getcwd(), 'has_token': bool(os.environ.get('GH_TOKEN'))}) + '\n')
mode = os.environ.get('WORKER_BUILD_TEST_MODE', 'success')
if mode == 'failure':
    sys.exit(17)
if mode == 'wait':
    child = subprocess.Popen([sys.executable, '-c', 'import time; time.sleep(30)'])
    pathlib.Path(os.environ['WORKER_BUILD_TEST_CHILD']).write_text(str(child.pid))
    time.sleep(30)
root = pathlib.Path(arguments[arguments.index('--root') + 1])
binary = root / 'bin' / 'worker-build'
binary.parent.mkdir(parents=True)
version = '0.7.4' if mode == 'wrong-version' else '0.7.5'
binary.write_text('#!/bin/sh\nprintf "' + version + '\\n"\n')
if mode == 'slow-version':
    time.sleep(0.15)
    binary.write_text('#!/usr/bin/env python3\nimport os, pathlib, time\n'
        'pathlib.Path(os.environ["WORKER_BUILD_TEST_PROBE"]).write_text("started")\n'
        'time.sleep(1)\nprint("0.7.5")\n')
binary.chmod(0o755)
if mode == 'partial':
    sys.exit(19)
'''


class CacheApi:
    repository = 'nil-studio/oasis7'
    list_caches = C.load_maintenance().GitHubApi.list_caches

    def __init__(self, entries=(), *, quota=None, pagination=None, mutate=None):
        self.entries = list(entries)
        self.quota = {'max_cache_size_gb': 20} if quota is None else quota
        self.pagination = pagination
        self.mutate = mutate
        self.calls = []
        self.run = dict(id=71, head_sha='a' * 40, head_branch='main', event='schedule',
                        path=C.WRITER_WORKFLOW, repository={'full_name': self.repository})

    def request(self, method, path):
        self.calls.append((method, path))
        if method != 'GET':
            raise AssertionError('tool cache admission attempted a write')
        if path == '/actions/runs/71':
            return self.run
        if path.startswith('/actions/caches?'):
            page = int(path.split('&page=', 1)[1].split('&', 1)[0])
            result = dict(total_count=len(self.entries), actions_caches=self.entries[(page - 1) * 100:page * 100])
            return self.pagination(page, result) if self.pagination else result
        if path == '/actions/cache/storage-limit':
            if self.mutate:
                self.mutate()
            if isinstance(self.quota, BaseException):
                raise self.quota
            return self.quota
        raise AssertionError('unexpected read: ' + path)


class WorkerBuild(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix='ci-worker-build-test-')
        self.addCleanup(self.temporary.cleanup)
        self.directory = Path(self.temporary.name)
        self.root = self.directory / 'cache' / C.VERSION
        self.commands = self.directory / 'commands'
        self.commands.mkdir()
        cargo = self.commands / 'cargo'
        cargo.write_text(FAKE_CARGO)
        cargo.chmod(0o755)
        # A matching-looking external PATH binary must never replace a miss or
        # validate a broken exact cache directory.
        external = self.commands / 'worker-build'
        external.write_text('#!/bin/sh\nprintf "0.7.5\\n"\n')
        external.chmod(0o755)
        self.log = self.directory / 'cargo.jsonl'
        self.child = self.directory / 'child.pid'
        self.environment = dict(os.environ, PATH=str(self.commands) + os.pathsep + os.environ['PATH'],
            WORKER_BUILD_TEST_LOG=str(self.log), WORKER_BUILD_TEST_CHILD=str(self.child),
            WORKER_BUILD_TEST_PROBE=str(self.directory / 'probe-started'),
            RUSTC_WRAPPER='must-not-reach-cargo', PYTHONDONTWRITEBYTECODE='1')

    def cli(self, command, *arguments, environment=None):
        return subprocess.run([sys.executable, str(HELPER), command, '--root', str(self.root), *arguments],
                              env={**self.environment, **(environment or {})},
                              capture_output=True, text=True, timeout=15, check=False)

    def calls(self):
        return [json.loads(line) for line in self.log.read_text().splitlines()] if self.log.exists() else []

    def binary(self, output='0.7.5\n'):
        binary = self.root / 'bin' / 'worker-build'
        binary.parent.mkdir(parents=True, exist_ok=True)
        binary.write_text('#!/usr/bin/env python3\nimport sys\nsys.stdout.write(' + repr(output) + ')\n')
        binary.chmod(0o755)
        return binary

    def assert_child_stopped(self):
        if not self.child.exists():
            return
        pid = int(self.child.read_text())
        if Path('/proc').is_dir():
            status = Path('/proc') / str(pid) / 'stat'
            self.assertTrue(not status.exists() or status.read_text().rsplit(')', 1)[1].split()[0] == 'Z')
        else:
            completed = subprocess.run(['ps', '-o', 'stat=', '-p', str(pid)], capture_output=True, text=True)
            self.assertTrue(not completed.stdout.strip() or completed.stdout.strip().startswith('Z'))

    def test_metadata_is_read_only_and_exports_exact_paths(self):
        result = self.cli('metadata')
        self.assertEqual(result.returncode, 0, result.stderr)
        values = json.loads(result.stdout)
        self.assertRegex(values.pop('definition_sha'), r'^[0-9a-f]{64}$')
        self.assertEqual(values, {
            'version': '0.7.5', 'root': str(self.root), 'bin': str(self.root / 'bin' / 'worker-build'),
        })
        output = self.directory / 'github-output'
        result = self.cli('metadata', '--github-output', str(output))
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(dict(line.split('=', 1) for line in output.read_text().splitlines()),
                         json.loads(self.cli('metadata').stdout))
        self.assertFalse(self.root.exists())
        self.assertEqual(self.calls(), [])

    def test_cache_miss_installs_fixed_version_once_and_reuses_it(self):
        result = self.cli('ensure', '--print-bin')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout, str(self.root / 'bin' / 'worker-build') + '\n')
        calls = self.calls()
        self.assertEqual(len(calls), 1)
        self.assertEqual(calls[0]['argv'][:6], ['install', 'worker-build', '--version', '0.7.5', '--locked', '--root'])
        self.assertEqual(Path(calls[0]['argv'][6]).parent.parent, self.root.parent)
        self.assertIsNone(calls[0]['wrapper'])
        self.assertFalse(calls[0]['has_token'])
        self.assertEqual(calls[0]['cwd'], str(HELPER.resolve().parent.parent))
        before = (self.root / 'bin' / 'worker-build').read_bytes()
        for command in ('check', 'ensure'):
            with self.subTest(command=command):
                result = self.cli(command, '--print-bin')
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual((self.root / 'bin' / 'worker-build').read_bytes(), before)
        self.assertEqual(self.calls(), calls)

    def test_exact_hit_missing_or_incomplete_payload_never_installs(self):
        for command in ('check', 'ensure'):
            with self.subTest(command=command):
                self.root.mkdir(parents=True, exist_ok=True)
                result = self.cli(command)
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(self.calls(), [])
        self.root.rmdir()
        result = self.cli('check')
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse(self.root.exists())

    def test_wrong_version_or_extra_output_is_not_repaired(self):
        for version in ('0.7.4\n', 'worker-build 0.7.5\n', '0.7.5\nextra\n', '0.7.5\n' + 'x' * 100):
            binary = self.binary(version)
            before = binary.read_bytes()
            for command in ('check', 'ensure'):
                with self.subTest(version=version, command=command):
                    result = self.cli(command)
                    self.assertNotEqual(result.returncode, 0)
                    self.assertIn('version must be exactly 0.7.5', result.stderr)
                    self.assertEqual(binary.read_bytes(), before)
                    self.assertEqual(self.calls(), [])

    def test_install_failure_partial_payload_and_wrong_version_are_not_published(self):
        for mode, code in (('failure', 17), ('partial', 19), ('wrong-version', 1)):
            with self.subTest(mode=mode):
                self.log.unlink(missing_ok=True)
                result = self.cli('ensure', environment={'WORKER_BUILD_TEST_MODE': mode})
                self.assertEqual(result.returncode, code, result.stderr)
                self.assertFalse(self.root.exists())
                self.assertEqual(len(self.calls()), 1)
                self.assertEqual(list(self.root.parent.iterdir()), [])

    def test_invalid_paths_symlinks_and_nonexecutables_fail_closed(self):
        for path in (Path('relative'), self.directory / '..' / 'escape', self.directory / 'bad\noutput'):
            with self.subTest(path=path), self.assertRaises(C.ToolError):
                C.cache_root(path)
        binary = self.binary()
        binary.chmod(0o644)
        self.assertNotEqual(self.cli('check').returncode, 0)
        binary.unlink()
        binary.symlink_to(self.commands / 'worker-build')
        self.assertNotEqual(self.cli('ensure').returncode, 0)
        self.assertEqual(self.calls(), [])
        link = self.directory / 'linked-root'
        link.symlink_to(self.root, target_is_directory=True)
        with self.assertRaises(C.ToolError):
            C.cache_root(link)

    def test_install_deadline_stops_children_without_retry_or_publishing(self):
        with mock.patch.dict(os.environ, {**self.environment, 'WORKER_BUILD_TEST_MODE': 'wait'}), \
                mock.patch.object(C, 'INSTALL_TIMEOUT_SECONDS', 0.4):
            started = time.monotonic()
            with self.assertRaises(C.ToolError) as failure:
                C.ensure(self.root)
        self.assertEqual(failure.exception.code, 124)
        self.assertLess(time.monotonic() - started, 4)
        self.assert_child_stopped()
        self.assertFalse(self.root.exists())
        self.assertEqual(len(self.calls()), 1)

    def test_version_probe_deadline_is_enforced_without_reinstall(self):
        binary = self.binary()
        binary.write_text('#!/usr/bin/env python3\nimport time\ntime.sleep(30)\n')
        with mock.patch.dict(os.environ, self.environment), mock.patch.object(C, 'PROBE_TIMEOUT_SECONDS', 0.2):
            with self.assertRaises(C.ToolError) as failure:
                C.check(self.root)
        self.assertEqual(failure.exception.code, 124)
        self.assertTrue(binary.exists())
        self.assertEqual(self.calls(), [])

    def test_install_and_final_probe_share_one_deadline(self):
        with mock.patch.dict(os.environ, {**self.environment, 'WORKER_BUILD_TEST_MODE': 'slow-version'}), \
                mock.patch.object(C, 'INSTALL_TIMEOUT_SECONDS', 0.65):
            with self.assertRaises(C.ToolError) as failure:
                C.ensure(self.root)
        self.assertEqual(failure.exception.code, 124)
        self.assertTrue(Path(self.environment['WORKER_BUILD_TEST_PROBE']).exists())
        self.assertFalse(self.root.exists())

    def test_cancelled_install_cleans_children_and_leaves_no_cache(self):
        process = subprocess.Popen([sys.executable, str(HELPER), 'ensure', '--root', str(self.root)],
            env={**self.environment, 'WORKER_BUILD_TEST_MODE': 'wait'},
            stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        try:
            deadline = time.monotonic() + 5
            while not self.child.exists() and process.poll() is None and time.monotonic() < deadline:
                time.sleep(0.02)
            self.assertTrue(self.child.exists())
            process.send_signal(signal.SIGTERM)
            stdout, stderr = process.communicate(timeout=5)
            self.assertEqual(process.returncode, 130, stdout + stderr)
            self.assert_child_stopped()
            self.assertFalse(self.root.exists())
            self.assertEqual(len(self.calls()), 1)
        finally:
            if process.poll() is None:
                process.kill()
                process.communicate(timeout=5)

    def admission_environment(self):
        return dict(self.environment, GITHUB_ACTIONS='true', GITHUB_REPOSITORY='nil-studio/oasis7',
                    GITHUB_REF='refs/heads/main', GITHUB_WORKFLOW_SHA='a' * 40, GITHUB_SHA='a' * 40,
                    GITHUB_WORKFLOW_REF=f'nil-studio/oasis7/{C.WRITER_WORKFLOW}@refs/heads/main',
                    GITHUB_JOB=C.WRITER_JOB, GITHUB_EVENT_NAME='schedule', GITHUB_RUN_ID='71',
                    GITHUB_WORKSPACE=str(C.ROOT), RUNNER_OS='Linux', RUNNER_ARCH='X64',
                    RUST_TOOLCHAIN='1.96.0', CI_CACHE_REPOSITORY_BUDGET_BYTES='20000000000')

    def admission(self, api, *, changes=None, key=None):
        environment = self.admission_environment()
        expected_key = C.cache_key(environment, self.root)
        environment.update(changes or {})
        with mock.patch.object(C, 'checkout_sha', return_value='a' * 40):
            return C.admit_save(self.root, key if key is not None else expected_key, environment, api)

    def cache_entry(self, identifier, size=1, **changes):
        return dict(id=identifier, key=f'other-{identifier}', ref='refs/heads/main', version='fixture-v1',
                    size_in_bytes=size, created_at='2026-11-10T10:00:00Z',
                    last_accessed_at='2026-11-10T10:00:00Z', **changes)

    def test_admission_reads_all_pages_measures_hidden_files_and_preserves_headroom(self):
        self.binary()
        (self.root / '.crates.toml').write_text('fixture metadata\n')
        api = CacheApi([self.cache_entry(identifier) for identifier in range(1, 102)])
        report = self.admission(api)
        self.assertTrue(report['save'], report)
        self.assertTrue(report['inventory_complete'])
        self.assertEqual(report['observed_cache_count'], 101)
        self.assertEqual(len([path for _, path in api.calls if path.startswith('/actions/caches?')]), 2)
        self.assertEqual(report['raw_bytes'], sum(p.stat().st_size for p in self.root.rglob('*') if p.is_file()))
        self.assertGreater(report['upload_bound_bytes'], report['raw_bytes'])
        self.assertGreater(report['upload_bound_bytes'], report['measured_tar_bytes'])
        self.assertEqual(report['reserved_repository_bytes'], C.load_maintenance().policy.DEPENDENCY_ROTATION_HEADROOM_BYTES)
        self.assertIsNone(report['saved_confirmed'])
        self.assertTrue(all(method == 'GET' for method, _ in api.calls))

    def test_capacity_boundary_and_lower_live_quota_can_only_skip_save(self):
        self.binary()
        initial = self.admission(CacheApi())
        budget = 20_000_000_000
        available_for_existing = budget - initial['upload_bound_bytes'] - initial['reserved_repository_bytes']
        self.assertTrue(self.admission(CacheApi([self.cache_entry(1, available_for_existing)]))['save'])
        self.assertFalse(self.admission(CacheApi([self.cache_entry(1, available_for_existing + 1)]))['save'])
        report = self.admission(CacheApi([self.cache_entry(1, 8_000_000_000)], quota={'max_cache_size_gb': 10}))
        self.assertFalse(report['save'])
        self.assertEqual(report['effective_repository_budget_bytes'], 10_000_000_000)
        self.assertTrue((self.root / 'bin' / 'worker-build').exists())

    def test_foreign_writer_source_event_platform_key_or_manual_mode_cannot_save(self):
        self.binary()
        cases = [
            {'GITHUB_REF': 'refs/pull/1/merge'}, {'GITHUB_WORKFLOW_SHA': 'b' * 40},
            {'GITHUB_JOB': 'custody-wasm-production'}, {'GITHUB_EVENT_NAME': 'pull_request'},
            {'GITHUB_WORKFLOW_REF': 'nil-studio/oasis7/.github/workflows/rust.yml@refs/heads/main'},
            {'GITHUB_WORKSPACE': ''}, {'RUNNER_ARCH': 'ARM64'}, {'RUST_TOOLCHAIN': '1.99.0'},
        ]
        for changes in cases:
            with self.subTest(changes=changes):
                self.assertFalse(self.admission(CacheApi(), changes=changes)['save'])
        self.assertFalse(self.admission(CacheApi(), key='worker-build-cli-v1-Linux-X64-1.96.0-0.7.5-' + '0' * 64)['save'])
        api = CacheApi()
        api.run['head_sha'] = 'b' * 40
        self.assertFalse(self.admission(api)['save'])
        event = self.directory / 'event.json'
        event.write_text('{"inputs":{"run_mode":"full"}}')
        api = CacheApi()
        api.run['event'] = 'workflow_dispatch'
        changes = {'GITHUB_EVENT_NAME': 'workflow_dispatch', 'GITHUB_EVENT_PATH': str(event)}
        self.assertTrue(self.admission(api, changes=changes)['save'])
        for payload in ({'inputs': {'run_mode': 'other'}}, {'inputs': []}, {}):
            event.write_text(json.dumps(payload))
            self.assertFalse(self.admission(api, changes=changes)['save'])

    def test_incomplete_changing_or_invalid_inventory_and_budget_never_authorize_save(self):
        self.binary()
        entries = [self.cache_entry(identifier) for identifier in range(1, 102)]
        for pagination in (
            lambda page, result: dict(result, total_count=result['total_count'] + (1 if page == 2 else 0)),
            lambda page, result: dict(result, actions_caches=result['actions_caches'][:-1]) if page == 1 else result,
        ):
            with self.subTest(pagination=pagination):
                report = self.admission(CacheApi(entries, pagination=pagination))
                self.assertFalse(report['save'])
                self.assertFalse(report['inventory_complete'])
        for budget in ('', '-1', 'unknown'):
            self.assertFalse(self.admission(CacheApi(), changes={'CI_CACHE_REPOSITORY_BUDGET_BYTES': budget})['save'])
        for quota in ({}, {'max_cache_size_gb': 0}, OSError('fixture API failure')):
            self.assertFalse(self.admission(CacheApi(quota=quota))['save'])

    def test_quota_fallback_requires_recognized_non_rate_limited_permission_failure(self):
        self.binary()
        maintenance = C.load_maintenance()
        def error(message, headers=None, status=403):
            response = HTTPError('https://example.invalid', status, 'fixture', headers or {},
                                 io.BytesIO(json.dumps({'message': message}).encode()))
            return maintenance.MaintenanceApiError('GET', response)
        report = self.admission(CacheApi(quota=error('Resource not accessible by integration')))
        self.assertTrue(report['save'], report)
        self.assertEqual(report['capacity_basis'], 'verified_configured_budget')
        for failure in (error('unknown'), error('API rate limit exceeded'),
                        error('Resource not accessible by integration', {'X-RateLimit-Remaining': '0'})):
            self.assertFalse(self.admission(CacheApi(quota=failure))['save'])

    def test_existing_key_mutation_and_oversized_or_unsafe_payload_skip_save(self):
        binary = self.binary()
        key = C.cache_key(self.admission_environment(), self.root)
        entry = self.cache_entry(1)
        entry['key'] = key
        report = self.admission(CacheApi([entry]))
        self.assertFalse(report['save'])
        self.assertEqual(report['reason'], 'existing immutable key')
        with mock.patch.object(C, 'MAX_CACHE_BYTES', 1):
            self.assertFalse(self.admission(CacheApi())['save'])
        with mock.patch.object(C, 'MAX_CACHE_ENTRIES', 2):
            self.assertFalse(self.admission(CacheApi())['save'])
        report = self.admission(CacheApi(mutate=lambda: binary.write_text(binary.read_text() + '# changed\n')))
        self.assertFalse(report['save'])
        self.assertIn('changed during capacity', report['reason'])
        (self.root / 'outside-link').symlink_to(self.directory / 'outside')
        self.assertFalse(self.admission(CacheApi())['save'])

    def test_admission_failure_is_optional_and_outputs_false(self):
        output = self.directory / 'github-output'
        result = self.cli('admit-save', '--key', 'invalid', '--github-output', str(output),
                          environment={'GH_TOKEN': ''})
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertFalse(json.loads(result.stdout)['save'])
        self.assertIn('save=false\n', output.read_text())
        self.assertFalse(self.root.exists())

    def test_exhausted_real_api_client_budget_never_requests_or_authorizes_save(self):
        self.binary()
        maintenance = C.load_maintenance()
        for exhausted in ('requests', 'deadline'):
            api = maintenance.GitHubApi('nil-studio/oasis7', 'fixture-read-only-token')
            if exhausted == 'requests':
                api.requests = maintenance.MAX_REQUESTS
            else:
                api.deadline = time.monotonic() - 1
            with self.subTest(exhausted=exhausted), mock.patch.object(
                    api.opener, 'open', side_effect=AssertionError('network must not be attempted')):
                report = self.admission(api)
            self.assertFalse(report['save'])
            self.assertIn('request/time budget exhausted', report['reason'])
            self.assertTrue((self.root / 'bin' / 'worker-build').exists())


if __name__ == '__main__':
    unittest.main()
