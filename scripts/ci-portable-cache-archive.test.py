#!/usr/bin/env python3
"""Real compressed archives; validation must finish before any extraction."""
import importlib.util
import hashlib
import json
import io
import os
from pathlib import Path
import subprocess
import sys
import tarfile
import tempfile
import unittest
from unittest import mock

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location('portable_inspect', HERE / 'ci-portable-cache-archive.py')
inspector = importlib.util.module_from_spec(spec)
spec.loader.exec_module(inspector)


class ArchiveTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.base = Path(self.temporary.name)
        self.layout = dict(member='../../../.cache/oasis7/worker-build/0.7.5',
                           root=str(self.base / 'cache'), max_bytes=1024, max_files=4)

    def archive(self, entries, pax=None):
        buffer = io.BytesIO()
        with tarfile.open(fileobj=buffer, mode='w') as writer:
            for name, kind, data in entries:
                entry = tarfile.TarInfo(name)
                entry.pax_headers = pax or {}
                entry.type = kind
                if kind == tarfile.REGTYPE:
                    entry.size = len(data)
                if kind in (tarfile.SYMTYPE, tarfile.LNKTYPE):
                    entry.linkname = '/outside'
                writer.addfile(entry, io.BytesIO(data) if kind == tarfile.REGTYPE else None)
        result = self.base / 'cache.tzst'
        result.write_bytes(subprocess.run(['zstd', '-q', '-c'], input=buffer.getvalue(), check=True, stdout=subprocess.PIPE).stdout)
        return result

    def check(self, entries, **layout):
        return inspector.inspect_archive(self.archive(entries), 'zstd-without-long', self.layout | layout)

    def test_legacy_relative_prefix_is_accepted_without_extracting(self):
        prefix = self.layout['member']
        result = self.check([(prefix, tarfile.DIRTYPE, b''), (prefix + '/bin', tarfile.DIRTYPE, b''),
                             (prefix + '/bin/worker-build', tarfile.REGTYPE, b'fixed')])
        self.assertEqual(result, dict(files=1, payload_bytes=5, entries=3))
        self.assertFalse(Path(self.layout['root']).exists())

    def test_outside_absolute_traversal_controls_and_duplicate_rejected(self):
        prefix = self.layout['member']
        for name in ('/etc/passwd', '../outside', prefix + '/../../escape', prefix + '/./alias', prefix + '//alias', prefix + '/bad\nname', prefix + '/bad\\name'):
            with self.subTest(name=name), self.assertRaises(ValueError):
                self.check([(name, tarfile.REGTYPE, b'fixed')])
        with self.assertRaises(ValueError):
            self.check([(prefix + '/x', tarfile.REGTYPE, b'a')] * 2)

    def test_links_devices_and_file_directory_conflicts_rejected(self):
        prefix = self.layout['member']
        for kind in (tarfile.SYMTYPE, tarfile.LNKTYPE, tarfile.CHRTYPE, tarfile.FIFOTYPE):
            with self.subTest(kind=kind), self.assertRaises(ValueError):
                self.check([(prefix + '/bad', kind, b'')])
        for entries in (
            [(prefix, tarfile.REGTYPE, b'a')],
            [(prefix + '/a', tarfile.REGTYPE, b'a'), (prefix + '/a/b', tarfile.REGTYPE, b'b')],
            [(prefix + '/a/b', tarfile.REGTYPE, b'b'), (prefix + '/a', tarfile.REGTYPE, b'a')],
        ):
            with self.assertRaises(ValueError):
                self.check(entries)

    def test_payload_count_and_decompressed_extension_limits(self):
        prefix = self.layout['member']
        with self.assertRaises(ValueError):
            self.check([(prefix + '/big', tarfile.REGTYPE, b'a' * 1025)])
        with self.assertRaises(ValueError):
            self.check([(prefix + '/x', tarfile.REGTYPE, b'a')], max_files=0)
        # Huge PAX name is consumed by tarfile before it returns a member:
        # the underlying stream bound still rejects it before extraction.
        with self.assertRaises(ValueError):
            self.check([(prefix + '/' + 'a' * (2 * 1024 * 1024), tarfile.REGTYPE, b'a')])

    def test_destination_symlink_and_archive_fifo_rejected(self):
        root = Path(self.layout['root'])
        root.symlink_to(self.base / 'elsewhere')
        with self.assertRaises(ValueError):
            self.check([(self.layout['member'] + '/x', tarfile.REGTYPE, b'a')])
        root.unlink()
        fifo = self.base / 'fifo'
        os.mkfifo(fifo)
        with self.assertRaises(ValueError):
            inspector.inspect_archive(fifo, 'zstd-without-long', self.layout)
        archive = self.archive([(self.layout['member'] + '/x', tarfile.REGTYPE, b'a')])
        link = self.base / 'link'
        link.symlink_to(archive)
        with self.assertRaises((OSError, ValueError)):
            inspector.inspect_archive(link, 'zstd-without-long', self.layout)

    def test_pax_unknown_and_sparse_extensions_rejected(self):
        entries = [(self.layout['member'] + '/x', tarfile.REGTYPE, b'a')]
        for headers in ({'OASIS.unknown': 'x'}, {'GNU.sparse.map': '0,1'}, {'linkpath': '/outside'}):
            with self.subTest(headers=headers), self.assertRaises(ValueError):
                inspector.inspect_archive(self.archive(entries, headers), 'zstd-without-long', self.layout)
        result = inspector.inspect_archive(self.archive(entries, {'mtime': '1.5'}), 'zstd-without-long', self.layout)
        self.assertEqual(result['files'], 1)

    def test_nested_destination_symlink_and_directory_count_rejected(self):
        root = Path(self.layout['root'])
        root.mkdir()
        (root / 'nested').symlink_to(self.base / 'outside')
        with self.assertRaises(ValueError):
            self.check([(self.layout['member'] + '/nested/x', tarfile.REGTYPE, b'a')])
        (root / 'nested').unlink()
        with self.assertRaises(ValueError):
            self.check([(self.layout['member'] + '/' + str(i), tarfile.DIRTYPE, b'') for i in range(258)], max_files=1)

    def test_runtime_action_imports_fixed_sdk_without_build_or_install(self):
        action = HERE.parent / '.github/actions/portable-cache-transport'
        self.assertIn('main: run.mjs', (action / 'action.yml').read_text())
        self.assertIn("../../../vendor/actions-cache-explicit-context/index.mjs", (action / 'run.mjs').read_text())
        self.assertFalse((action / 'node_modules').exists())
        self.assertFalse((action / 'package.json').exists())
        result = subprocess.run(['node', '--test', str(action / 'transport.test.cjs')],
                                cwd=HERE.parent, capture_output=True, text=True, timeout=30)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_supervised_unknown_contract_is_fatal_and_reaped(self):
        result = subprocess.run([sys.executable, str(HERE / 'ci-portable-cache-archive.py'),
                                 '--supervise', '--contract', 'unknown', '--component', 'x'],
                                capture_output=True, text=True, timeout=15)
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        receipt = json.loads(next(line for line in result.stdout.splitlines() if line.startswith('{')))
        self.assertFalse(receipt['passed'])
        self.assertEqual(receipt['cleanup_result'], 'clean')
        self.assertFalse(receipt['leaked_processes'])

    def test_compression_and_unknown_contract_rejected(self):
        with self.assertRaises(ValueError):
            inspector.inspect_archive('/missing', 'gzip', self.layout)
        for contract, component in (('other', 'worker-build'), ('worker-build', '../worker-build'), ('browser-archive', 'other')):
            with self.assertRaises(ValueError):
                inspector.describe(contract, component)

    @unittest.skipUnless(sys.platform == 'linux', 'Canonical /home has a system symlink on Darwin')
    def test_layout_comes_from_current_original_metadata(self):
        browser = inspector.load('ci-browser-deps')
        for component in ('fonts-noto-cjk', 'fonts-freefont-ttf', 'chrome'):
            result = inspector.describe('browser-archive', component)
            original = browser.metadata(component, inspector.TEMP / 'oasis7-browser-archives')
            self.assertEqual((result['key'], result['root']), (original['key'], original['root']))
        worker = inspector.load('ci-worker-build')
        import tomllib
        pinned = tomllib.loads((inspector.ROOT / 'rust-toolchain.toml').read_text())['toolchain']['channel']
        with mock.patch.dict(os.environ, RUST_TOOLCHAIN=pinned, RUNNER_OS='Linux', RUNNER_ARCH='X64'):
            result = inspector.describe('worker-build', 'worker-build')
            self.assertEqual(result['key'], worker.cache_key(os.environ, Path(result['root'])))


if __name__ == '__main__':
    unittest.main()
