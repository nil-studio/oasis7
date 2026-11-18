#!/usr/bin/env python3
"""Check shared cache admission with non-executable archive payloads."""
import hashlib
import importlib.util
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time
import unittest


ROOT = Path(__file__).resolve().parent.parent
SPEC = importlib.util.spec_from_file_location('tool_cache', ROOT / 'scripts' / 'ci-tool-cache.py')
C = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(C)


class CacheApi:
    repository = 'nil-studio/oasis7'

    def __init__(self, sha, *, quota=None, mutate=None):
        self.run = dict(id=71, head_sha=sha, head_branch='main', event='schedule',
                        path='.github/workflows/rust-cache.yml',
                        repository={'full_name': self.repository})
        self.quota = {'max_cache_size_gb': 20} if quota is None else quota
        self.entries = []
        self.calls = []
        self.mutate = mutate

    def request(self, method, path):
        self.calls.append((method, path))
        if method != 'GET':
            raise AssertionError('archive admission must not write')
        if path == '/actions/runs/71':
            return self.run
        if path == '/actions/cache/storage-limit':
            if self.mutate:
                self.mutate()
            if isinstance(self.quota, BaseException):
                raise self.quota
            return self.quota
        raise AssertionError('unexpected request: ' + path)

    def list_caches(self):
        return self.entries


class ArchiveAdmission(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix='ci-tool-cache-test-')
        self.addCleanup(self.temporary.cleanup)
        self.directory = Path(self.temporary.name)
        self.root = self.directory / 'downloads'
        self.root.mkdir()
        self.archive = self.root / 'fixed-archive.deb'
        # Deliberately not an executable: admission must not run cached files.
        self.archive.write_bytes(b'fixed fixture archive\x00\xff\n')
        self.digest = hashlib.sha256(self.archive.read_bytes()).hexdigest()
        self.key = 'browser-downloads-v1-Linux-X64-ubuntu24-fonts-' + self.digest
        self.sha = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip()
        self.environment = dict(os.environ, GITHUB_ACTIONS='true',
            GITHUB_REPOSITORY='nil-studio/oasis7', GITHUB_REF='refs/heads/main',
            GITHUB_SHA=self.sha, GITHUB_WORKFLOW_SHA=self.sha,
            GITHUB_WORKFLOW_REF='nil-studio/oasis7/.github/workflows/rust-cache.yml@refs/heads/main',
            GITHUB_JOB='browser-archive-cache', GITHUB_EVENT_NAME='schedule', GITHUB_RUN_ID='71',
            GITHUB_WORKSPACE=str(ROOT), RUNNER_OS='Linux', RUNNER_ARCH='X64',
            CI_CACHE_REPOSITORY_BUDGET_BYTES='20000000000')
        self.checked = []

    def check_payload(self, root, *, deadline):
        self.checked.append(root)
        self.assertGreater(deadline, time.monotonic())
        if sorted(path.name for path in root.iterdir()) != ['fixed-archive.deb']:
            raise ValueError('unexpected archive payload')
        if hashlib.sha256((root / 'fixed-archive.deb').read_bytes()).hexdigest() != self.digest:
            raise ValueError('archive digest mismatch')

    def admit(self, *, api=None, changes=None, key=None, **options):
        return C.admit_save(self.root, self.key if key is None else key,
            {**self.environment, **(changes or {})}, repository_root=ROOT,
            expected_key=self.key, writer_workflow='.github/workflows/rust-cache.yml',
            writer_job='browser-archive-cache', check_payload=self.check_payload,
            max_cache_bytes=options.pop('max_cache_bytes', 256 * 1024 ** 2),
            max_entries=options.pop('max_entries', 4), label='browser-archive',
            api=api or CacheApi(self.sha), **options)

    def test_non_executable_exact_archive_is_validated_measured_and_not_modified(self):
        original = self.archive.read_bytes()
        api = CacheApi(self.sha)
        report = self.admit(api=api)
        self.assertTrue(report['save'], report)
        self.assertEqual(self.checked, [self.root])
        self.assertEqual(report['raw_bytes'], len(original))
        self.assertEqual(report['payload_entries'], 2)
        self.assertGreater(report['upload_bound_bytes'], report['measured_tar_bytes'])
        self.assertEqual(report['reserved_repository_bytes'], 5 * 1024 ** 3 // 2)
        self.assertIsNone(report['saved_confirmed'])
        self.assertEqual(self.archive.read_bytes(), original)
        self.assertEqual({method for method, _ in api.calls}, {'GET'})

    def test_exact_writer_platform_component_key_and_real_checkout_are_required(self):
        for changes in ({'GITHUB_REF': 'refs/pull/42/merge'},
                        {'GITHUB_JOB': 'worker-build-cache'},
                        {'GITHUB_WORKFLOW_SHA': '0' * 40},
                        {'RUNNER_ARCH': 'ARM64'}, {'RUNNER_OS': 'Windows'},
                        {'GITHUB_EVENT_NAME': 'push'}):
            with self.subTest(changes=changes):
                self.assertFalse(self.admit(changes=changes)['save'])
        self.assertFalse(self.admit(key=self.key.replace('-fonts-', '-chrome-'))['save'])
        other_sha = 'b' * 40 if self.sha != 'b' * 40 else 'a' * 40
        report = self.admit(api=CacheApi(other_sha),
                            changes={'GITHUB_SHA': other_sha, 'GITHUB_WORKFLOW_SHA': other_sha})
        self.assertFalse(report['save'])
        self.assertIn('checkout does not match', report['reason'])
        self.assertEqual(self.checked, [])

    def test_corruption_symlinks_hardlinks_and_oversized_payload_cannot_save(self):
        original = self.archive.read_bytes()
        self.archive.write_bytes(b'corrupt')
        self.assertIn('digest mismatch', self.admit()['reason'])
        self.archive.write_bytes(original)
        self.assertFalse(self.admit(max_cache_bytes=len(original) - 1)['save'])
        self.assertFalse(self.admit(max_entries=1)['save'])
        self.assertFalse(self.admit(max_cache_bytes=0)['save'])
        outside = self.directory / 'outside'
        outside.write_bytes(original)
        self.archive.unlink()
        self.archive.symlink_to(outside)
        self.assertIn('unsafe', self.admit()['reason'])
        self.archive.unlink()
        os.link(outside, self.archive)
        self.assertIn('hardlinks', self.admit()['reason'])

    def test_mutation_during_capacity_and_unknown_quota_skip_optional_save(self):
        report = self.admit(api=CacheApi(self.sha, mutate=lambda: self.archive.write_bytes(b'changed')))
        self.assertFalse(report['save'])
        self.assertIn('changed during capacity', report['reason'])
        self.archive.write_bytes(b'fixed fixture archive\x00\xff\n')
        for quota in ({}, OSError('unavailable')):
            with self.subTest(quota=quota):
                report = self.admit(api=CacheApi(self.sha, quota=quota))
                self.assertFalse(report['save'])
                self.assertTrue(self.archive.is_file())

    def test_archive_options_and_glob_roots_never_authorize_save(self):
        for changes in ({'TAR_OPTIONS': '--ignore-failed-read'}, {'GZIP': '-9'}):
            with self.subTest(changes=changes):
                self.assertFalse(self.admit(changes=changes)['save'])
        for name in ('cache*', 'cache?', 'cache[0]'):
            directory = self.directory / name
            directory.mkdir()
            with self.subTest(name=name), self.assertRaisesRegex(C.ToolCacheError, 'glob paths'):
                C.upload_bound(directory, self.environment, time.monotonic() + 10,
                    repository_root=ROOT, max_cache_bytes=1024, max_entries=4)

    def test_shared_process_runner_preserves_failure_timeout_and_token_removal(self):
        environment = dict(self.environment, GH_TOKEN='fixture-secret',
                           GITHUB_TOKEN='fixture-secret', ACTIONS_RUNTIME_TOKEN='fixture-secret')
        code = 'import os, sys; assert not any(k in os.environ for k in ' \
               '("GH_TOKEN", "GITHUB_TOKEN", "ACTIONS_RUNTIME_TOKEN")); sys.exit(17)'
        with self.assertRaises(C.ToolCacheError) as failure:
            C.checked_process([sys.executable, '-c', code], self.directory, self.directory,
                              'failed-command', 5, environment=environment, label='fixture')
        self.assertEqual(failure.exception.code, 17)
        started = time.monotonic()
        with self.assertRaises(C.ToolCacheError) as timeout:
            C.checked_process([sys.executable, '-c', 'import time; time.sleep(30)'],
                              self.directory, self.directory, 'timed-command', 0.2,
                              environment=environment, label='fixture')
        self.assertEqual(timeout.exception.code, 124)
        self.assertLess(time.monotonic() - started, 4)


if __name__ == '__main__':
    unittest.main()
