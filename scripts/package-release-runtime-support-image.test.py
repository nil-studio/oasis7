#!/usr/bin/env python3
"""Check the finite runtime-support handoff without launching a release."""
import hashlib
import importlib.util
import os
import re
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location('support_image', ROOT / 'scripts/package-release-runtime-support-image.py')
SUPPORT = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(SUPPORT)


class RuntimeSupportImage(unittest.TestCase):
    source = '1' * 40
    workflow = '2' * 40
    parent = '3' * 40

    def context(self):
        return {
            'GITHUB_REPOSITORY': SUPPORT.REPOSITORY,
            'GITHUB_JOB': 'release_gate_runtime_support',
            'GITHUB_ACTIONS': 'true', 'RUNNER_OS': 'Linux', 'RUNNER_ARCH': 'X64',
            'RUNNER_ENVIRONMENT': 'github-hosted',
            'GITHUB_EVENT_NAME': 'workflow_dispatch', 'GITHUB_REF': 'refs/heads/main',
            'GITHUB_WORKFLOW_REF': 'nil-studio/oasis7/.github/workflows/mainnet-packages.yml@refs/heads/main',
            'GITHUB_SHA': self.workflow, 'GITHUB_WORKFLOW_SHA': self.workflow,
        }

    def test_protected_job_and_workflow_identity(self):
        env = self.context()
        self.assertTrue(SUPPORT.protected_context(env, self.source))
        for key, changed in (
                ('GITHUB_REPOSITORY', 'fork/oasis7'), ('GITHUB_JOB', 'publish-release'),
                ('GITHUB_EVENT_NAME', 'pull_request'), ('GITHUB_REF', 'refs/heads/codex/topic'),
                ('RUNNER_ENVIRONMENT', 'self-hosted'), ('GITHUB_SHA', self.source)):
            with self.subTest(key=key):
                self.assertFalse(SUPPORT.protected_context(env | {key: changed}, self.source))
        ref = 'refs/tags/2026-11-26_v1'
        tag = env | {
            'GITHUB_EVENT_NAME': 'push', 'GITHUB_REF': ref,
            'GITHUB_WORKFLOW_REF': f'nil-studio/oasis7/.github/workflows/release-packages.yml@{ref}',
            'GITHUB_SHA': self.source, 'GITHUB_WORKFLOW_SHA': self.source,
        }
        self.assertTrue(SUPPORT.protected_context(tag, self.source))
        self.assertFalse(SUPPORT.protected_context(tag, self.workflow))

    def test_wrong_source_rejected_before_host_fixture(self):
        with patch.object(SUPPORT, 'git', return_value=b'4' * 40):
            with self.assertRaisesRegex(ValueError, 'authorized source'):
                SUPPORT.run_host_fixture(self.source, Path('/unused'), self.context())

    def test_copy_closed_executable_input_and_reject_unsafe_paths(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            binary = root / 'native'
            header = b'\x7fELF\x02\x01\x01' + b'\0' * 9 + b'\x03\0\x3e\0'
            binary.write_bytes(header + b'bounded unit fixture bytes')
            binary.chmod(0o755)
            # Cargo's named binary can be a hardlink to an artifact in deps/.
            os.link(binary, root / 'cargo-deps-alias')
            staged = root / 'staged'
            self.assertEqual(SUPPORT.copy_native_elf(binary, staged, os.getuid()),
                             hashlib.sha256(binary.read_bytes()).hexdigest())
            self.assertEqual(staged.read_bytes(), binary.read_bytes())
            self.assertEqual(staged.stat().st_mode & 0o777, 0o500)
            self.assertEqual(staged.stat().st_nlink, 1)
            alias = root / 'link'
            alias.symlink_to(binary)
            with self.assertRaisesRegex(ValueError, 'runtime support mount'):
                SUPPORT.copy_native_elf(alias, root / 'second', os.getuid())
            with self.assertRaisesRegex(ValueError, 'owner-executable'):
                SUPPORT.copy_native_elf(binary, root / 'second', os.getuid() + 1)
            for mode in (0o775, 0o644, 0o4755):
                binary.chmod(mode)
                with self.subTest(mode=oct(mode)), self.assertRaisesRegex(ValueError, 'owner-executable'):
                    SUPPORT.copy_native_elf(binary, root / 'second', os.getuid())
            binary.chmod(0o755)
            binary.write_bytes(b'not a Linux executable')
            with self.assertRaisesRegex(ValueError, 'x86-64 executable'):
                SUPPORT.copy_native_elf(binary, root / 'second', os.getuid())

    def test_full_support_partitions_preserve_every_command_and_order(self):
        source = (ROOT / SUPPORT.DRIVER).read_text()
        names = ('run_full_support_tier_tests', 'run_oasis7_workspace_support_crate_tests',
                 'run_game_client_cli_contracts')
        definitions = [re.search(r'^' + name + r'\(\) \{\n.*?^}', source, re.M | re.S).group()
                       for name in names]
        dependencies = set(re.findall(r'(?:^ +|; +|=0 )(run_[a-z0-9_]+)\b', '\n'.join(definitions), re.M)) - set(names)
        fixture = '\n'.join(
            f'{name}() {{ printf "%s\\n" "{name} $*"; }}' for name in sorted(dependencies))
        script = "\n".join([
            'set -euo pipefail', 'ci_python=/usr/bin/python3',
            'export GITHUB_ACTIONS=true RUNNER_OS=Linux RUNNER_ENVIRONMENT=github-hosted',
            'run() { printf "%s\\n" "$*"; }', fixture, *definitions,
            'run_full_support_tier_tests "$1"',
        ])

        def commands(phase):
            return subprocess.check_output(['bash', '-c', script, 'bash', phase], text=True).splitlines()

        old = commands('all')
        before = commands('before-cross-uid')
        after = commands('after-cross-uid')
        fixture_commands = [command for command in old if command.startswith('sudo ')]
        self.assertEqual(len(fixture_commands), 1)
        self.assertEqual(old, before + fixture_commands + after)
        self.assertEqual(before[-1], 'run_cargo build -p oasis7_game_client --bin oasis7_game_client --locked')
        self.assertTrue(after[0].startswith('run_cargo test -p oasis7_model_provider --all-targets'))

    def test_host_fixture_uses_only_the_original_bounded_command_and_exact_elf(self):
        with tempfile.TemporaryDirectory() as directory:
            owned = Path(directory).resolve()
            fixture = owned / 'host-fixture.py'
            binary = owned / 'oasis7_game_client'
            digest = '4' * 64
            for result, failure in (
                    (subprocess.CompletedProcess([], 0, f'Linux CLI sha256={digest}\n'), None),
                    (subprocess.CompletedProcess([], 0, 'Linux CLI sha256=another\n'), ValueError),
                    (subprocess.CompletedProcess([], 124, ''), subprocess.CalledProcessError)):
                with self.subTest(result=result), patch.object(SUPPORT, 'exact_source'), \
                        patch.object(SUPPORT, 'git', return_value=b'original fixture\n'), \
                        patch.object(SUPPORT, 'copy_native_elf', return_value=digest), \
                        patch.object(SUPPORT.subprocess, 'run', return_value=result) as run:
                    if failure:
                        with self.assertRaises(failure):
                            SUPPORT.run_host_fixture(self.source, owned, {'PATH': '/usr/bin', 'GHCR_TOKEN': 'excluded'})
                    else:
                        SUPPORT.run_host_fixture(self.source, owned, {'PATH': '/usr/bin', 'GHCR_TOKEN': 'excluded'})
                    self.assertEqual(run.call_args.args[0], [
                        'sudo', '-n', 'timeout', '--signal=TERM', '--kill-after=5s', '60s',
                        '/usr/bin/python3', str(fixture), str(binary)])
                    self.assertEqual(run.call_args.kwargs['env'], {'PATH': '/usr/bin'})
                    self.assertEqual(fixture.read_bytes(), b'original fixture\n')


if __name__ == '__main__':
    unittest.main()
