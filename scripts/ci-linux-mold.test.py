#!/usr/bin/env python3
"""Offline contracts for the pinned Linux linker installer."""

import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import subprocess
import tarfile
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch


ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    'linux_mold', ROOT / '.github/actions/setup-linux-mold/setup.py')
MOLD = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MOLD)


class LinuxMoldTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.env_file = self.root / 'github-env'
        self.output_file = self.root / 'github-output'
        self.summary_file = self.root / 'summary'
        self.environment = patch.dict(os.environ, {
            'PATH': os.environ.get('PATH', '/usr/bin:/bin'),
            'GITHUB_ENV': str(self.env_file),
            'GITHUB_OUTPUT': str(self.output_file),
            'GITHUB_STEP_SUMMARY': str(self.summary_file),
        }, clear=True)
        self.environment.start()
        self.addCleanup(self.environment.stop)
        self.install_dir = self.root / 'install with spaces' / '3.0.0-v1'

    def archive(self):
        archive = self.root / 'mold.tar.gz'
        files = {
            'bin/mold': b'#!/bin/sh\nprintf "mold 3.0.0 (fixture)\\n"\n',
            'share/doc/mold/LICENSE': b'MIT fixture license\n',
        }
        with tarfile.open(archive, 'w:gz') as output:
            for relative, content in files.items():
                member = tarfile.TarInfo('mold-3.0.0-x86_64-linux/' + relative)
                member.size = len(content)
                output.addfile(member, io.BytesIO(content))
        return archive, hashlib.sha256(archive.read_bytes()).hexdigest()

    def release(self, checksum):
        return patch.object(MOLD, 'platform_release', return_value=(
            'x86_64', 'x86_64-unknown-linux-gnu', checksum))

    def test_checksum_rejection_precedes_extraction_installation_and_export(self):
        archive, _ = self.archive()
        self.env_file.write_text('EXISTING=value\n')
        with self.release('0' * 64), patch.object(MOLD, 'extract_file') as extract:
            with self.assertRaisesRegex(ValueError, 'SHA-256 mismatch'):
                MOLD.install(self.install_dir, archive)
        extract.assert_not_called()
        self.assertFalse(self.install_dir.exists())
        self.assertEqual(self.env_file.read_text(), 'EXISTING=value\n')
        self.assertFalse(self.output_file.exists())

    def test_archive_member_must_be_regular_file(self):
        buffer = io.BytesIO()
        with tarfile.open(fileobj=buffer, mode='w') as output:
            member = tarfile.TarInfo('bin/mold')
            member.type = tarfile.SYMTYPE
            member.linkname = '/unexpected/executable'
            output.addfile(member)
        buffer.seek(0)
        destination = self.root / 'extracted'
        with tarfile.open(fileobj=buffer) as archive:
            with self.assertRaisesRegex(ValueError, 'not a regular file'):
                MOLD.extract_file(archive, 'bin/mold', destination)
        self.assertFalse(destination.exists())

    def test_smoke_failure_does_not_publish_configuration(self):
        archive, checksum = self.archive()
        with self.release(checksum), patch.object(MOLD, 'smoke', side_effect=ValueError('wrong linker')):
            with self.assertRaisesRegex(ValueError, 'wrong linker'):
                MOLD.install(self.install_dir, archive)
        self.assertFalse(self.env_file.exists())
        self.assertFalse(self.output_file.exists())
        self.assertFalse(self.summary_file.exists())

    def test_smoke_rejects_an_elf_without_mold_identity(self):
        with patch.object(MOLD, 'run', return_value=SimpleNamespace(stdout='Linker: LLD')) as run:
            with self.assertRaisesRegex(ValueError, 'not linked by the pinned mold'):
                MOLD.smoke(self.root / 'driver', self.root)
        self.assertEqual(run.call_count, 2)  # Rejected before executing the binary.

    def test_success_exports_only_host_linker_and_preserves_driver_arguments(self):
        archive, checksum = self.archive()
        gcc = self.root / 'fake gcc'
        gcc.write_text('#!/usr/bin/env python3\nimport json,os,sys\n'
                       'print(json.dumps([sys.argv[1:], os.environ["PATH"].split(os.pathsep)[0]]))\n')
        gcc.chmod(0o755)
        original_which = MOLD.shutil.which
        with self.release(checksum), patch.object(MOLD, 'smoke', return_value=0.1), \
                patch.object(MOLD.shutil, 'which', side_effect=lambda name:
                             str(gcc) if name == 'gcc' else original_which(name)):
            report = MOLD.install(self.install_dir, archive)
        linker = Path(report['linker'])
        self.assertEqual(report['version'], '3.0.0')
        self.assertEqual(linker.name, 'oasis7-mold-gcc')
        key = 'CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER'
        self.assertEqual(report['environment'], {key: str(linker)})
        self.assertEqual(self.env_file.read_text(), f'{key}={linker}\n')
        self.assertEqual((linker.parent / 'ld.mold').readlink(), Path('mold'))
        actual = subprocess.check_output([str(linker), '-fuse-ld=lld', 'input with spaces.o'], text=True)
        arguments, first_path = json.loads(actual)
        self.assertEqual(arguments, ['-fuse-ld=lld', 'input with spaces.o', '-fuse-ld=mold'])
        self.assertEqual(first_path, str(linker.parent))
        self.assertIn('version=3.0.0\n', self.output_file.read_text())

    def test_conflicting_native_linker_is_not_overwritten(self):
        archive, checksum = self.archive()
        key = 'CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER'
        with self.release(checksum), patch.dict(os.environ, {key: '/custom/linker'}):
            with self.assertRaisesRegex(ValueError, 'conflicts with existing'):
                MOLD.install(self.install_dir, archive)
        self.assertFalse(self.install_dir.exists())
        self.assertFalse(self.env_file.exists())


if __name__ == '__main__':
    unittest.main()
