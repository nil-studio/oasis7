#!/usr/bin/env python3
"""Browser preparation regressions: upstream coverage, cold/hot downloads and refusal."""
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import stat
import subprocess
import tempfile
import time
import unittest
from unittest.mock import patch
import zipfile


ROOT = Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location('browser_deps', ROOT / 'scripts/ci-browser-deps.py')
browser = importlib.util.module_from_spec(spec)
spec.loader.exec_module(browser)

# Independent transcription of the fixed upstream Ubuntu 24.04 contracts.
AGENT = '''libxcb-shm0 libx11-xcb1 libx11-6 libxcb1 libxext6 libxrandr2 libxcomposite1
libxcursor1 libxdamage1 libxfixes3 libxi6 libgtk-3-0t64 libpangocairo-1.0-0
libpango-1.0-0 libatk1.0-0t64 libcairo-gobject2 libcairo2 libgdk-pixbuf-2.0-0
libxrender1 libasound2t64 libfreetype6 libfontconfig1 libdbus-1-3 libnss3 libnss3-tools
libnspr4 libatk-bridge2.0-0t64 libdrm2 libxkbcommon0 libatspi2.0-0t64 libcups2t64
libxshmfence1 libgbm1 fonts-noto-color-emoji fonts-noto-cjk fonts-freefont-ttf'''.split()
PLAYWRIGHT = '''xvfb fonts-noto-color-emoji fonts-unifont libfontconfig1 libfreetype6
xfonts-cyrillic xfonts-scalable fonts-liberation fonts-ipafont-gothic fonts-wqy-zenhei
fonts-tlwg-loma-otf fonts-freefont-ttf libasound2t64 libatk-bridge2.0-0t64 libatk1.0-0t64
libatspi2.0-0t64 libcairo2 libcups2t64 libdbus-1-3 libdrm2 libgbm1 libglib2.0-0t64
libnspr4 libnss3 libpango-1.0-0 libx11-6 libxcb1 libxcomposite1 libxdamage1 libxext6
libxfixes3 libxkbcommon0 libxrandr2'''.split()
FONT_VERSIONS = {'fonts-noto-cjk': '1:20230817+repack1-3',
                 'fonts-freefont-ttf': '20211204+svn4273-2'}


def query(kind='agent', missing=(), overrides=None, code=None):
    lines = []
    for name in AGENT if kind == 'agent' else PLAYWRIGHT:
        if name in missing:
            continue
        fields = (name, 'installed', 'ok', 'all' if name.startswith('fonts-') else 'amd64',
                  FONT_VERSIONS.get(name, '1.0'))
        lines.append('\t'.join((overrides or {}).get(name, fields)))
    return subprocess.CompletedProcess([], (1 if missing else 0) if code is None else code,
                                       '\n'.join(lines) + '\n', '')


class BrowserDownloads(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.base = Path(self.temporary.name)
        self.payload = b'fixed official download fixture\n'
        self.pin = dict(name='pinned.deb', version='1', size=len(self.payload),
                        sha256=hashlib.sha256(self.payload).hexdigest(), url='https://archive.ubuntu.com/pinned.deb')
        self.patch = patch.dict(browser.ARCHIVES, {'fixture': self.pin})
        self.patch.start()
        self.addCleanup(self.patch.stop)

    def cache(self):
        root = self.base / 'archive'
        root.mkdir()
        (root / 'pinned.deb').write_bytes(self.payload)
        return root

    def test_complete_upstream_dependencies_and_deck_scope(self):
        self.assertEqual(list(browser.AGENT_PACKAGES), AGENT)
        self.assertEqual(list(browser.PLAYWRIGHT_PACKAGES), PLAYWRIGHT)
        self.assertEqual(len(AGENT), 36)
        self.assertEqual(len(PLAYWRIGHT), 33)
        self.assertEqual(browser.components('playwright'), ('fonts-freefont-ttf',))
        self.assertNotIn('fonts-noto-cjk', browser.packages('playwright'))

    def test_fixed_keys_change_with_download_identity_not_source_run(self):
        before = browser.metadata('fixture', self.base)
        with patch.dict(os.environ, {'GITHUB_SHA': 'a' * 40, 'GITHUB_RUN_ID': '123'}):
            self.assertEqual(browser.metadata('fixture', self.base), before)
        with patch.dict(browser.ARCHIVES, {'fixture': dict(self.pin, sha256='1' * 64)}):
            self.assertNotEqual(browser.metadata('fixture', self.base)['key'], before['key'])
        self.assertNotEqual(browser.metadata('chrome', self.base)['key'], before['key'])

    def test_cold_download_then_verified_warm_cache_and_corruption(self):
        root = self.base / 'cold'
        def download(command, work, name, timeout, extra=None):
            self.assertIn('--proto-redir', command)
            self.assertIn('=https', command)
            self.assertEqual(command[-1], self.pin['url'])
            Path(command[command.index('--output') + 1]).write_bytes(self.payload)
        with patch.object(browser, 'checked_command', side_effect=download) as command:
            first = browser.ensure_archive('fixture', root)
            self.assertEqual(first.read_bytes(), self.payload)
            self.assertEqual(browser.ensure_archive('fixture', root), first)
            self.assertEqual(command.call_count, 1)
            first.write_bytes(b'X' * len(self.payload))
            with self.assertRaisesRegex(browser.BrowserError, 'SHA256'):
                browser.ensure_archive('fixture', root)
            self.assertEqual(command.call_count, 1, 'bad cache must not silently become a miss')

    def test_download_failure_or_wrong_hash_never_publishes_cache(self):
        for mode in ('failure', 'wrong-hash'):
            root = self.base / mode
            def download(command, *_args, **_kwargs):
                if mode == 'failure':
                    raise browser.BrowserError('network failure', 22)
                Path(command[command.index('--output') + 1]).write_bytes(b'X' * len(self.payload))
            with self.subTest(mode=mode), patch.object(browser, 'checked_command', side_effect=download):
                with self.assertRaises(browser.BrowserError):
                    browser.ensure_archive('fixture', root)
                self.assertFalse(root.exists())

    def test_cache_rejects_partial_extra_links_and_length(self):
        for mode in ('empty', 'extra', 'symlink', 'hardlink', 'length'):
            with self.subTest(mode=mode), tempfile.TemporaryDirectory(dir=self.base) as temporary:
                root = Path(temporary) / 'cache'
                root.mkdir()
                path = root / self.pin['name']
                if mode != 'empty':
                    path.write_bytes(self.payload)
                if mode == 'extra':
                    (root / 'unexpected').write_text('extra')
                elif mode == 'symlink':
                    target = Path(temporary) / 'original'
                    path.rename(target)
                    path.symlink_to(target)
                elif mode == 'hardlink':
                    os.link(path, Path(temporary) / 'alias')
                elif mode == 'length':
                    path.write_bytes(self.payload + b'X')
                with self.assertRaises((browser.BrowserError, OSError)):
                    browser.check_archive('fixture', root)

    def test_safe_extract_rejects_escape_symlink_duplicate_and_expansion(self):
        for name, mode in (('../escape', stat.S_IFREG | 0o755),
                           ('chrome-linux64/link', stat.S_IFLNK | 0o777),
                           ('chrome-linux64/chrome', stat.S_IFREG | 0o4755)):
            with self.subTest(name=name, mode=mode), tempfile.TemporaryDirectory(dir=self.base) as temporary:
                work = Path(temporary)
                archive = work / 'chrome.zip'
                with zipfile.ZipFile(archive, 'w') as stream:
                    info = zipfile.ZipInfo(name)
                    info.external_attr = mode << 16
                    stream.writestr(info, b'fixture')
                with self.assertRaises(browser.BrowserError):
                    browser.extract_chrome(archive, work)
                self.assertFalse((self.base / 'escape').exists())

    def test_safe_extract_preserves_executable_and_refuses_stale_destination(self):
        archive = self.base / 'chrome.zip'
        with zipfile.ZipFile(archive, 'w') as stream:
            info = zipfile.ZipInfo('chrome-linux64/chrome')
            info.external_attr = (stat.S_IFREG | 0o755) << 16
            stream.writestr(info, b'fixture')
        binary = browser.extract_chrome(archive, self.base)
        self.assertTrue(os.access(binary, os.X_OK))
        with self.assertRaisesRegex(browser.BrowserError, 'must be new'):
            browser.extract_chrome(archive, self.base)

    def test_platform_guard_rejects_different_distro_and_arch(self):
        for release, arch in (({'ID': 'debian', 'VERSION_ID': '13'}, 'x86_64'),
                              ({'ID': 'ubuntu', 'VERSION_ID': '24.04'}, 'aarch64')):
            with patch.object(browser.platform, 'freedesktop_os_release', return_value=release), \
                    patch.object(browser.platform, 'system', return_value='Linux'), \
                    patch.object(browser.platform, 'machine', return_value=arch):
                with self.assertRaises(browser.BrowserError):
                    browser.require_platform()


class BrowserDependencies(unittest.TestCase):
    def test_ready_checks_full_set_and_exact_font_versions(self):
        self.assertEqual(browser.missing_packages('agent', query()), [])
        self.assertEqual(browser.missing_packages('playwright', query('playwright')), [])
        self.assertEqual(browser.missing_packages('agent', query(missing=('libnss3',))), ['libnss3'])
        self.assertEqual(browser.missing_packages('agent', query(missing=('fonts-noto-cjk',))), ['fonts-noto-cjk'])
        with self.assertRaisesRegex(browser.BrowserError, 'fixed font version'):
            browser.missing_packages('agent', query(overrides={
                'fonts-noto-cjk': ('fonts-noto-cjk', 'installed', 'ok', 'all', '2:newer')}))

    def test_broken_foreign_or_unknown_dpkg_state_is_not_ready(self):
        for status, error, arch in (('half-configured', 'ok', 'amd64'),
                                    ('installed', 'reinstreq', 'amd64'),
                                    ('installed', 'ok', 'arm64')):
            with self.subTest(status=status, error=error, arch=arch), self.assertRaises(browser.BrowserError):
                browser.missing_packages('agent', query(overrides={
                    'libnss3': ('libnss3', status, error, arch, '1')}))
        with self.assertRaises(browser.BrowserError):
            browser.missing_packages('agent', query(code=2))
        repeated = query()
        repeated.stdout += repeated.stdout.splitlines()[0] + '\n'
        with self.assertRaises(browser.BrowserError):
            browser.missing_packages('agent', repeated)

    def test_not_installed_record_without_architecture_requests_cold_install(self):
        state = query(overrides={'fonts-freefont-ttf': ('fonts-freefont-ttf', 'not-installed', 'ok', '', '')})
        self.assertEqual(browser.missing_packages('agent', state), ['fonts-freefont-ttf'])

    def test_prepare_emits_local_verified_fonts_and_complete_missing_dependency_plan(self):
        with tempfile.TemporaryDirectory() as temporary:
            base = Path(temporary)
            work = base / 'work'
            with patch.object(browser, 'require_platform'), \
                    patch.object(browser, 'ensure_archive', side_effect=lambda part, *_args, **_kwargs: base / (part + '.archive')), \
                    patch.object(browser, 'extract_chrome'), \
                    patch.object(browser, 'missing_packages', return_value=['libnss3']):
                browser.prepare('agent', work, base / 'cache')
            arguments = (work / 'apt-arguments.txt').read_text().splitlines()
            self.assertEqual(len(arguments), 36)
            self.assertIn('libnss3', arguments)
            self.assertIn(str(base / 'fonts-noto-cjk.archive'), arguments)
            self.assertIn(str(base / 'fonts-freefont-ttf.archive'), arguments)
            self.assertNotIn('fonts-noto-cjk', arguments)

    def test_fully_installed_deck_does_not_request_apt_or_cjk(self):
        with tempfile.TemporaryDirectory() as temporary:
            base = Path(temporary)
            with patch.object(browser, 'require_platform'), \
                    patch.object(browser, 'ensure_archive', return_value=base / 'freefont.deb') as fetch, \
                    patch.object(browser, 'missing_packages', return_value=[]):
                browser.prepare('playwright', base / 'work', base / 'cache')
            self.assertEqual(fetch.call_count, 1)
            self.assertEqual(fetch.call_args.args[0], 'fonts-freefont-ttf')
            self.assertEqual((base / 'work/apt-arguments.txt').read_text(), '')

    def test_missing_dependency_stops_before_browser_execution_and_env_export(self):
        with tempfile.TemporaryDirectory() as temporary:
            work = Path(temporary)
            output = work / 'env'
            with patch.object(browser, 'require_platform'), \
                    patch.object(browser, 'missing_packages', return_value=['libnss3']), \
                    patch.object(browser, 'checked_command') as execute, self.assertRaises(browser.BrowserError):
                browser.activate('agent', work, output)
            execute.assert_not_called()
            self.assertFalse(output.exists())

    def test_agent_version_or_launch_failure_cannot_export_runtime_environment(self):
        for outputs in (['agent-browser wrong'],
                        ['agent-browser 0.37.1', 'Google Chrome for Testing wrong'],
                        ['agent-browser 0.37.1', 'Google Chrome for Testing 158.0.8127.36', '<body>wrong</body>']):
            with self.subTest(outputs=outputs), tempfile.TemporaryDirectory() as temporary:
                work = Path(temporary)
                agent = work / 'agent'
                agent.write_text('fixture')
                output = work / 'env'
                with patch.object(browser, 'require_platform'), patch.object(browser, 'missing_packages', return_value=[]), \
                        patch.object(browser, 'check_tools', return_value=agent), \
                        patch.object(browser, 'checked_command', side_effect=outputs), self.assertRaises(browser.BrowserError):
                    browser.activate('agent', work, output)
                self.assertFalse(output.exists())

    def test_playwright_preserves_shell_installer_and_private_runtime_path(self):
        with tempfile.TemporaryDirectory() as temporary:
            work = Path(temporary)
            with patch.object(browser, 'require_platform'), patch.object(browser, 'missing_packages', return_value=[]), \
                    patch.object(browser, 'check_tools'), patch.object(browser, 'checked_command', return_value='') as execute:
                values = browser.activate('playwright', work, work / 'env')
            install, probe = execute.call_args_list
            self.assertEqual(install.args[0][-3:], ['install', 'chromium', '--only-shell'])
            self.assertNotIn('--with-deps', install.args[0])
            self.assertEqual(install.args[-1], probe.args[-1])
            self.assertEqual(values['PLAYWRIGHT_BROWSERS_PATH'], str(work / 'playwright-browsers'))
            self.assertEqual(probe.args[0][-1], '151.0.7922.34')


if __name__ == '__main__':
    unittest.main()
