#!/usr/bin/env python3
"""Resolver eligibility and namespace ownership, without sudo/network."""
import importlib.util
from pathlib import Path
import unittest
import os
import tempfile
import time
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('ci_apt_dns', Path(__file__).with_name('ci_apt_dns.py'))
dns = importlib.util.module_from_spec(spec)
spec.loader.exec_module(dns)

class RecoveryTests(unittest.TestCase):
    def test_direct_sources_have_only_public_exact_roots(self):
        for url in ('http://azure.archive.ubuntu.com/ubuntu', 'https://archive.ubuntu.com/ubuntu/',
                    'https://security.ubuntu.com/ubuntu', 'https://packages.microsoft.com/ubuntu/24.04/prod'):
            self.assertTrue(dns.direct_source_allowed(url, 'ubuntu24'))
        for url in ('https://archive.ubuntu.com/ubuntu?token=x', 'https://archive.ubuntu.com/ubuntu#x',
                    'https://archive.ubuntu.com/private/token#x', 'https://user:secret@archive.ubuntu.com/ubuntu',
                    'https://archive.ubuntu.com:443/ubuntu', 'https://packages.microsoft.com/private',
                    'https://packages.microsoft.com/ubuntu/22.04/prod'):
            self.assertFalse(dns.direct_source_allowed(url, 'ubuntu24'))

    def test_dynamic_route_refusal_has_safe_fixed_reason(self):
        class Installer:
            def run_command(self, *_args, **_kwargs): return 0, 'Global: private.internal\n', True
        recovery = dns.Recovery(Installer(), True)
        recovery.reason = 'eligible'
        self.assertFalse(recovery.default_routes())
        self.assertEqual(recovery.reason, 'dynamic_resolver_configuration_custom')
        self.assertNotIn('private', recovery.reason)

    def test_only_explicit_hosted_ubuntu_without_proxy(self):
        env = dict(GITHUB_ACTIONS='true', RUNNER_ENVIRONMENT='github-hosted', RUNNER_OS='Linux', ImageOS='ubuntu24')
        self.assertFalse(dns.hosted_allowed(False, env, 'ubuntu'))
        self.assertTrue(dns.hosted_allowed(True, env, 'ubuntu'))
        for key, value in [('RUNNER_ENVIRONMENT','self-hosted'),('GITHUB_ACTIONS','false'),('HTTP_PROXY','http://private')]:
            self.assertFalse(dns.hosted_allowed(True, dict(env, **{key:value}), 'ubuntu'))
        self.assertFalse(dns.hosted_allowed(True, env, 'debian'))

    def test_only_explicit_dns_error(self):
        self.assertTrue(dns.dns_failure("E: Failed to fetch http://archive.ubuntu.com/x Temporary failure resolving 'archive.ubuntu.com'"))
        self.assertFalse(dns.dns_failure('E: Failed to fetch http://archive.ubuntu.com/x Connection timed out'))

    def test_namespace_entry_refuses_same_host_mount_namespace(self):
        with patch.object(dns.os, 'readlink', return_value='mnt:[same]'), patch.object(dns.ctypes, 'CDLL') as libc:
            libc.return_value.unshare.return_value = 0
            with self.assertRaises(RuntimeError): dns.enter_private_namespace()

    def exercise(self, replies, callback):
        class Installer:
            work_end = time.monotonic() + 5
            cancelled = 0
            def __init__(self): self.calls = []
            def log(self, _message): pass
            def run_command(self, stage, command, **_options):
                self.calls.append((stage, command))
                return replies.pop(0)
        with tempfile.TemporaryDirectory() as root:
            installer = Installer()
            recovery = dns.Recovery(installer, True)
            fstat = os.fstat
            def root_owned(fd):
                values = list(fstat(fd)); values[4] = 0
                return os.stat_result(values)
            with patch.object(dns.Recovery, 'eligible', return_value=True), \
                 patch.object(dns.Recovery, 'default_routes', return_value=True), \
                 patch.object(dns, 'LOCK_PATH', str(Path(root) / 'lock')), \
                 patch.object(dns, 'RUNTIME_ROOT', root), \
                 patch.object(dns.os, 'fstat', side_effect=root_owned):
                try: callback(recovery, installer)
                finally: recovery.close()
            self.assertIsNone(recovery.lock)
            self.assertIsNone(recovery.directory)
            self.assertEqual(recovery.prefix, [])

    def test_temporary_failure_then_real_nss_alternate_enables_only_namespace(self):
        def check(recovery, installer):
            self.assertTrue(recovery.try_enable("E: Failed to fetch http://archive.ubuntu.com/x Temporary failure resolving 'archive.ubuntu.com'"))
            self.assertEqual([x[0] for x in installer.calls], ['dns-default', 'dns-alternate'])
            self.assertIn('--namespace', recovery.prefix)
            self.assertEqual(float(recovery.prefix[-2]), installer.work_end)
            resolver = Path(recovery.directory.name) / 'resolv.conf'
            self.assertEqual(resolver.read_text(), 'nameserver 1.1.1.1\n')
            self.assertEqual(resolver.stat().st_mode & 0o777, 0o644)
            self.assertEqual(resolver.parent.stat().st_mode & 0o777, 0o700)
        self.exercise([(75, '', True), (0, '', True)], check)

    def test_healthy_nxdomain_and_invalid_probe_never_try_alternate(self):
        for result in [(0, '', True), (2, '', True), (None, '', False)]:
            def check(recovery, installer):
                self.assertFalse(recovery.try_enable('E: Failed to fetch x Temporary failure resolving x'))
                self.assertEqual(len(installer.calls), 1)
                self.assertEqual(recovery.prefix, [])
            self.exercise([result], check)

    def test_alternate_failure_does_not_change_command_or_leave_directory(self):
        def check(recovery, _installer):
            self.assertFalse(recovery.try_enable('E: Failed to fetch x Temporary failure resolving x'))
            self.assertEqual(recovery.prefix, [])
            self.assertIsNone(recovery.directory)
        self.exercise([(75, '', True), (2, '', True)], check)

    def test_other_transient_never_probes(self):
        def check(recovery, installer):
            self.assertFalse(recovery.try_enable('E: Failed to fetch x Connection timed out'))
            self.assertEqual(installer.calls, [])
        self.exercise([], check)

    def test_exact_hosted_mirror_file_accepts_official_rows_only(self):
        default = ('http://azure.archive.ubuntu.com/ubuntu/ priority:1\n'
                   'https://archive.ubuntu.com/ubuntu/ priority:2\n'
                   'https://security.ubuntu.com/ubuntu/ priority:3\n')
        with tempfile.TemporaryDirectory() as root:
            path = Path(root) / 'apt-mirrors.txt'
            path.write_text(default); path.chmod(0o644)
            fstat = os.fstat
            def root_owned(fd):
                values = list(fstat(fd)); values[4] = 0
                return os.stat_result(values)
            with patch.object(dns, 'MIRROR_PATH', str(path)), patch.object(dns.os, 'fstat', side_effect=root_owned):
                self.assertTrue(dns.default_mirror_allowed())
                for text in (default.replace('azure.archive.ubuntu.com', 'private.example'),
                             default.replace('/ubuntu/', '/ubuntu/?secret=x', 1),
                             default.replace('https://archive.', 'https://user:password@archive.'),
                             default + 'http://internal.example/ubuntu/\n'):
                    path.write_text(text)
                    self.assertFalse(dns.default_mirror_allowed())

    def test_mock_default_hosted_source_is_eligible_comments_are_not_sources(self):
        with tempfile.TemporaryDirectory() as root:
            root = Path(root)
            files = {'/etc/os-release': 'ID=ubuntu\n', dns.TARGET: 'nameserver 127.0.0.53\noptions edns0 trust-ad\nsearch .\n',
                     '/run/systemd/resolve/resolv.conf': 'nameserver 168.63.129.16\n',
                     '/etc/nsswitch.conf': 'hosts: files dns\n', '/usr/bin/mount': '', '/usr/bin/resolvectl': ''}
            mapping = {}
            for index, (name, text) in enumerate(files.items()):
                path = root / str(index); path.write_text(text); mapping[name] = path
            for name in ('/etc/apt/apt.conf.d', '/etc/apt/sources.list.d', '/etc/systemd/resolved.conf.d'):
                path = root / name.lstrip('/'); path.mkdir(parents=True); mapping[name] = path
            for name in ('/etc/apt/apt.conf', '/etc/apt/sources.list', '/etc/systemd/resolved.conf'):
                mapping[name] = root / (name.replace('/', '-') + '-absent')
            resolver = root / 'resolv.conf'; resolver.symlink_to(mapping[dns.TARGET]); mapping['/etc/resolv.conf'] = resolver
            source = mapping['/etc/apt/sources.list.d'] / 'ubuntu.sources'
            source.write_text('# https://private.example is just a comment\nTypes: deb\nURIs: ' + dns.MIRROR_URI + '\n')
            env = dict(GITHUB_ACTIONS='true', RUNNER_ENVIRONMENT='github-hosted', RUNNER_OS='Linux', ImageOS='ubuntu24')
            with patch.object(dns, 'Path', side_effect=lambda value: Path(mapping.get(str(value), value))), \
                 patch.object(dns.os.path, 'realpath', return_value=dns.TARGET), \
                 patch.dict(dns.os.environ, env, clear=True), \
                 patch.object(dns, 'default_mirror_allowed', return_value=True) as mirror:
                recovery = dns.Recovery(None, True)
                self.assertTrue(recovery.eligible(), recovery.reason)
                mirror.assert_called_once()
                for uri in ('mirror+file:/etc/apt/private-mirrors.txt', 'custom://internal.example/repo',
                            'custom:internal.example/repo', 'ssh:/internal.example/repo',
                            'https://archive.ubuntu.com/ubuntu#private', 'https://archive.ubuntu.com/ubuntu?token=x'):
                    source.write_text('URIs: ' + uri + '\n')
                    self.assertFalse(recovery.eligible(), uri)

if __name__ == '__main__': unittest.main()
