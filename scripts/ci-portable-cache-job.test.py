#!/usr/bin/env python3
"""Portable job bind/source/UID guards use real filesystem aliases, no cache API."""
import importlib.util
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('portable_job', Path(__file__).with_name('ci-portable-cache-job.py'))
job = importlib.util.module_from_spec(spec)
spec.loader.exec_module(job)
SHA = 'a' * 40


class PortableJob(unittest.TestCase):
    def test_alias_rejects_different_directory_and_symlink(self):
        with tempfile.TemporaryDirectory() as name:
            root = Path(name).resolve(); other = root / 'other'; other.mkdir()
            job.alias(root, root)
            with self.assertRaises(ValueError):
                job.alias(root, other)
            link = root / 'link'; link.symlink_to(other)
            with self.assertRaises(ValueError):
                job.alias(link, other)

    def context(self, root, kind='browser'):
        return dict(GITHUB_ACTIONS='true', GITHUB_REPOSITORY='nil-studio/oasis7',
                    GITHUB_REF='refs/heads/main', GITHUB_SHA=SHA, GITHUB_WORKFLOW_SHA=SHA,
                    RUNNER_OS='Linux', RUNNER_ARCH='X64', RUNNER_ENVIRONMENT='github-hosted', GITHUB_JOB=kind + '-archive-cache' if kind == 'browser' else 'worker-build-cache',
                    GITHUB_WORKFLOW_REF='nil-studio/oasis7/.github/workflows/rust-cache.yml@refs/heads/main',
                    GITHUB_WORKSPACE=str(root), RUNNER_TEMP=str(root / 'temp'),
                    HOME=str(root / 'temp/portable-worker-home'), CARGO_HOME=str(root / 'temp/portable-worker-home/.cargo'))

    def test_source_and_role_failure_precede_tool_commands(self):
        with tempfile.TemporaryDirectory() as name:
            root = Path(name).resolve(); temporary = root / 'temp'; temporary.mkdir()
            with patch.multiple(job, ROOT=root, TEMP=temporary, WORKER=temporary, SOCKET=root / 'absent'), \
                    patch.object(job.os, 'getuid', return_value=1001), patch.object(job.os, 'getgid', return_value=1001), \
                    patch.object(job.sys, 'platform', 'linux'), \
                    patch.object(job.Path, 'cwd', return_value=root), patch.object(job.subprocess, 'run') as run, \
                    patch.object(job.subprocess, 'check_output', return_value='b' * 40):
                with self.assertRaisesRegex(ValueError, 'source mismatch'):
                    job.verify('browser', self.context(root))
                run.assert_not_called()
                for delta in ({'GITHUB_JOB': 'worker-build-cache'}, {'GITHUB_REF': 'refs/pull/1/merge'},
                              {'GITHUB_WORKFLOW_SHA': 'b' * 40}, {'RUNNER_ARCH': 'ARM64'}):
                    with self.subTest(delta=delta), self.assertRaises(ValueError):
                        job.verify('browser', self.context(root) | delta)
                run.assert_not_called()

    def test_original_bind_and_daemon_denial_guard(self):
        with tempfile.TemporaryDirectory() as name:
            root = Path(name).resolve(); temporary = root / 'temp'; temporary.mkdir()
            socket = root / 'socket'; socket.touch()
            with patch.multiple(job, ROOT=root, TEMP=temporary, SOCKET=socket), \
                    patch.object(job.os, 'getuid', return_value=1001), patch.object(job.os, 'getgid', return_value=1001), \
                    patch.object(job.sys, 'platform', 'linux'), \
                    patch.object(job.Path, 'cwd', return_value=root), patch.object(job.subprocess, 'run'), \
                    patch.object(job.subprocess, 'check_output', side_effect=lambda command, **_: SHA if command[0] == 'git' else 'tar (GNU tar) 1.35'), \
                    patch.object(job.socket, 'socket') as factory, patch('builtins.print'):
                factory.return_value.__enter__.return_value.connect.side_effect = PermissionError()
                job.verify('browser', self.context(root))
                factory.return_value.__enter__.return_value.connect.side_effect = None
                with self.assertRaisesRegex(ValueError, 'Docker daemon'):
                    job.verify('browser', self.context(root))
                with patch.object(job.os, 'getuid', return_value=0), self.assertRaises(ValueError):
                    job.verify('browser', self.context(root))


if __name__ == '__main__':
    unittest.main()
