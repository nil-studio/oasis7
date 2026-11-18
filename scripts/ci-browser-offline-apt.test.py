#!/usr/bin/env python3
"""Real APT archive selection, using tiny fixtures and download-only isolation."""
import hashlib
import importlib.util
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
from unittest.mock import patch


ROOT = Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location('browser_offline_deps', ROOT / 'scripts/ci-browser-deps.py')
browser = importlib.util.module_from_spec(spec)
spec.loader.exec_module(browser)


class OfflineFontArchives(unittest.TestCase):
    def setUp(self):
        self.assertIsNotNone(shutil.which('apt-get'), 'this contract requires the real APT executable')
        self.assertIsNotNone(shutil.which('dpkg-deb'), 'this contract requires the real Debian archive builder')
        temporary = tempfile.TemporaryDirectory(prefix='ci-browser-offline-apt-')
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.archives = self.root / 'var/cache/apt/archives'
        for directory in ('etc/apt/apt.conf.d', 'etc/apt/sources.list.d', 'etc/apt/preferences.d',
                          'var/lib/apt/lists/partial', 'var/cache/apt/archives/partial',
                          'var/lib/dpkg', 'var/log/apt', 'package/DEBIAN', 'package/usr/share/fonts'):
            (self.root / directory).mkdir(parents=True, exist_ok=True)
        self.status = self.root / 'var/lib/dpkg/status'
        self.status.write_text('')
        (self.root / 'etc/apt/empty.conf').write_text('')
        self.sources = self.root / 'etc/apt/sources.list'
        self.sources.write_text('deb [trusted=yes] https://archive.ubuntu.com/ubuntu noble main\n')
        control = ('Package: fonts-noto-cjk\nVersion: 1:1.0+fixture-1\nArchitecture: all\n'
                   'Maintainer: oasis7 CI <ci@example.invalid>\n'
                   'Description: tiny offline archive selection fixture\n')
        (self.root / 'package/DEBIAN/control').write_text(control)
        (self.root / 'package/usr/share/fonts/fixture.txt').write_text('offline font fixture\n')
        self.deb = self.root / 'fonts-noto-cjk_1.0+fixture-1_all.deb'
        built = subprocess.run(['dpkg-deb', '--build', '--root-owner-group',
                                str(self.root / 'package'), str(self.deb)],
                               capture_output=True, text=True, timeout=20)
        self.assertEqual(built.returncode, 0, built.stdout + built.stderr)
        payload = self.deb.read_bytes()
        self.pin = dict(name=self.deb.name, version='1:1.0+fixture-1', size=len(payload),
                        sha256=hashlib.sha256(payload).hexdigest(),
                        url='https://archive.ubuntu.com/ubuntu/pool/main/f/fonts-noto-cjk/' + self.deb.name)
        self.index = self.root / 'var/lib/apt/lists/archive.ubuntu.com_ubuntu_dists_noble_main_binary-amd64_Packages'
        self.index.write_text(control + 'Filename: pool/main/f/fonts-noto-cjk/' + self.deb.name + '\n'
                              + f'Size: {len(payload)}\nSHA256: {self.pin["sha256"]}\n\n')
        self.config = self.root / 'apt.conf'
        self.config.write_text(
            f'Dir "{self.root}";\n'
            f'Dir::State::status "{self.status}";\n'
            f'Dir::Cache::archives "{self.archives}";\n'
            f'Dir::Etc::main "{self.root / "etc/apt/empty.conf"}";\n'
            f'Dir::Etc::parts "{self.root / "etc/apt/apt.conf.d"}";\n'
            'APT::Architecture "amd64";\n'
            'Debug::NoLocking "true";\n')
        self.env = dict(os.environ, APT_CONFIG=str(self.config), LC_ALL='C')

    def apt(self, *, offline=True, print_uris=False):
        # Every invocation is download-only; status, lists, config, logs and
        # archives are private. No apt update or dpkg installation is executed.
        command = ['apt-get', '--download-only', '--no-remove']
        if offline:
            command.append('--no-download')
        else:
            self.assertTrue(print_uris, 'online acquisition is forbidden in this fixture')
        if print_uris:
            command.append('--print-uris')
        result = subprocess.run([*command, 'install', '-y', '--', str(self.deb)],
                                env=self.env, capture_output=True, text=True, timeout=20)
        self.assertEqual(self.status.read_text(), '', 'download-only must not modify dpkg status')
        return result.returncode, result.stdout + result.stderr

    def assert_no_fetch(self, output):
        self.assertNotRegex(output, r'(?m)^(?:Get|Ign|Err):', output)

    def stage_exact_archive(self):
        with patch.dict(browser.ARCHIVES, {'fonts-noto-cjk': self.pin}):
            name = browser.font_archive_name('fonts-noto-cjk')
        self.assertEqual(name, 'fonts-noto-cjk_1%3a1.0+fixture-1_all.deb')
        destination = self.archives / name
        shutil.copyfile(self.deb, destination)
        browser.verify_file(destination, self.pin)
        # Production passes an exact directory of debs; APT must also work
        # without a pre-created partial download directory.
        (self.archives / 'partial').rmdir()
        return destination

    def test_local_deb_still_selects_same_version_repository_without_archive_cache(self):
        code, output = self.apt(offline=False, print_uris=True)
        self.assertEqual(code, 0, output)
        self.assertIn("'https://archive.ubuntu.com/ubuntu/pool/main/f/fonts-noto-cjk/", output)
        self.assertIn('fonts-noto-cjk_1%3a1.0+fixture-1_all.deb', output)
        self.assert_no_fetch(output)
        self.assertFalse(list(self.archives.glob('*.deb')))

    def test_empty_offline_cache_fails_instead_of_downloading(self):
        code, output = self.apt()
        self.assertEqual(code, 100, output)
        self.assert_no_fetch(output)
        self.assertFalse(list(self.archives.glob('*.deb')))

    def test_missing_epoch_in_cache_filename_is_not_an_offline_hit(self):
        shutil.copyfile(self.deb, self.archives / self.deb.name)
        code, output = self.apt()
        self.assertEqual(code, 100, output)
        self.assert_no_fetch(output)

    def test_exact_archive_filename_needs_no_download_with_same_version_repository(self):
        cached = self.stage_exact_archive()
        code, output = self.apt(offline=False, print_uris=True)
        self.assertEqual(code, 0, output)
        self.assertIn('Need to get 0 B/', output)
        self.assertNotIn("'https://", output)
        code, output = self.apt()
        self.assertEqual(code, 0, output)
        self.assert_no_fetch(output)
        self.assertEqual(cached.read_bytes(), self.deb.read_bytes())

    def test_local_font_archives_do_not_require_repository_index_update(self):
        self.stage_exact_archive()
        self.sources.write_text('')
        self.index.unlink()
        code, output = self.apt()
        self.assertEqual(code, 0, output)
        self.assert_no_fetch(output)


if __name__ == '__main__':
    unittest.main()
