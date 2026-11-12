#!/usr/bin/env python3
"""Resolver eligibility and namespace ownership, without sudo/network."""
import importlib.util
from pathlib import Path
import unittest
import os
import json
import subprocess
import sys
import tempfile
import time
from unittest.mock import Mock, patch

spec = importlib.util.spec_from_file_location('ci_apt_dns', Path(__file__).with_name('ci-apt-dns.py'))
dns = importlib.util.module_from_spec(spec)
spec.loader.exec_module(dns)

DEFAULT_SOURCE_CONFIG = ('Dir "/";\nDir::Etc "etc/apt";\n'
                         'Dir::Etc::sourcelist "sources.list";\n'
                         'Dir::Etc::sourceparts "sources.list.d";\n')

class RecoveryTests(unittest.TestCase):
    def test_unknown_resolvectl_shape_is_diagnosed_without_disclosing_values(self):
        logs = []
        class Installer:
            def log(self, message): logs.append(message)
            def run_command(self, _stage, command, **_options):
                outputs = {'dns': 'Link 2 (eth0): 168.63.129.16\n', 'domain': 'Global: private.internal\n',
                           'default-route': '  Link 2 (private-interface): yes\n'}
                return 0, outputs[command[-1]], True
        recovery = dns.Recovery(Installer(), True)
        self.assertFalse(recovery.default_routes())
        self.assertEqual(recovery.reason, 'dynamic_resolver_shape_unknown')
        self.assertEqual(len(logs), 1)
        shape = json.loads(logs[0].split('=', 1)[1])
        self.assertEqual(shape['option'], 'default-route')
        self.assertEqual(shape['header_count'], 0)
        self.assertEqual(shape['stripped_header_count'], 1)
        self.assertEqual(shape['blank_count'], 0)
        self.assertNotIn('private', logs[0])
        self.assertNotIn('168.63', logs[0])

    def test_resolver_diagnostic_classifies_ansi_and_warning_tokens_only(self):
        output = ('\x1b[1mLink 2 (private-interface)\x1b[0m: yes\n'
                  'Warning: private.internal 192.0.2.1 secret\n'
                  '  Hint: private-secret\nunknown-private\n')
        shape = dns.resolver_output_shape('default-route', output)
        self.assertTrue(shape['ansi_present'])
        self.assertEqual(shape['ansi_stripped_header_count'], 1)
        self.assertEqual(shape['yes_no_suffix_count'], 1)
        self.assertEqual(shape['colon_line_count'], 3)
        self.assertEqual(shape['known_warning_prefix_counts']['Warning:'], 1)
        self.assertEqual(shape['known_warning_prefix_counts']['Hint:'], 1)
        encoded = json.dumps(shape)
        for secret in ('private', '192.0.2.1', 'secret'):
            self.assertNotIn(secret, encoded)

    def test_effective_proxy_tree_not_comments_controls_eligibility(self):
        for dump in ('APT "";\nAcquire "";\n', 'Acquire::http::Proxy "";\n',
                     'Acquire::http::Proxy::archive.ubuntu.com "DIRECT";\n',
                     'APT::Comment "ProxyAutoDetect is only documentation";\n'):
            self.assertTrue(dns.effective_proxy_clear(dump))
        for dump in ('Acquire::http::Proxy "http://private:secret@proxy.internal";\n',
                     'Acquire::https::ProxyAutoDetect "/private/detect";\n',
                     'Binary::apt-get::Acquire::http::Proxy-Auto-Detect "auto";\n', '', 'malformed'):
            self.assertFalse(dns.effective_proxy_clear(dump))

    def test_effective_source_paths_allow_only_complete_default_tree(self):
        for dump in (DEFAULT_SOURCE_CONFIG, DEFAULT_SOURCE_CONFIG.lower(),
                     DEFAULT_SOURCE_CONFIG + 'RootDir "";\n',
                     DEFAULT_SOURCE_CONFIG.replace('"etc/apt"', '"/etc/apt/"')
                     .replace('"sources.list"', '"/etc/apt/sources.list"')
                     .replace('"sources.list.d"', '"/etc/apt/sources.list.d/"')):
            self.assertTrue(dns.effective_source_paths_default(dump), dump)
        for line in DEFAULT_SOURCE_CONFIG.splitlines(keepends=True):
            for value in ('', '/private/source', '../sources.list', r'sources\.list'):
                with self.subTest(line=line, value=value):
                    key = line.split()[0]
                    dump = DEFAULT_SOURCE_CONFIG.replace(line, key + ' "' + value + '";\n')
                    self.assertFalse(dns.effective_source_paths_default(dump))
            self.assertFalse(dns.effective_source_paths_default(DEFAULT_SOURCE_CONFIG.replace(line, '')))
            self.assertFalse(dns.effective_source_paths_default(DEFAULT_SOURCE_CONFIG + line))
        for extra in ('RootDir "/private/root";\n', 'RootDir "/";\n', 'malformed',
                      'Dir::Etc "private \"quoted\" source";\n'):
            self.assertFalse(dns.effective_source_paths_default(DEFAULT_SOURCE_CONFIG + extra))
        for binary in ('apt-get', 'apt-config', 'apt'):
            for name in ('Dir', 'Dir::Etc', 'Dir::Etc::sourcelist', 'Dir::Etc::sourceparts', 'RootDir'):
                self.assertFalse(dns.effective_source_paths_default(
                    DEFAULT_SOURCE_CONFIG + f'Binary::{binary}::{name} "/private/source";\n'))
        self.assertFalse(dns.effective_source_paths_default(''))
        self.assertFalse(dns.effective_source_paths_default('APT "";\n'))

    @unittest.skipUnless(Path('/usr/bin/apt-config').is_file() and Path('/usr/bin/apt-get').is_file(),
                         'real APT configuration tests require apt-config and apt-get')
    def test_real_apt_effective_source_overrides_are_rejected_without_network(self):
        with tempfile.TemporaryDirectory() as root:
            root = Path(root)
            empty = root / 'empty'; empty.mkdir()
            config = root / 'apt.conf'
            # Read only this fixture's configuration, never the machine's apt.conf.d.
            base = f'Dir::Etc::parts "{empty}";\nDir::Etc::main "{empty / "absent"}";\n'
            env = dict(os.environ, APT_CONFIG=str(config), LC_ALL='C')
            def run(command):
                result = subprocess.run(command, env=env, capture_output=True, text=True, timeout=5)
                self.assertEqual(result.returncode, 0, result.stderr)
                return result.stdout
            config.write_text(base)
            self.assertTrue(dns.effective_source_paths_default(run(['/usr/bin/apt-config', 'dump'])))
            etc = root / 'etc/apt'; etc.mkdir(parents=True)
            private = 'deb http://private.invalid/ubuntu noble main\n'
            (etc / 'sources.list').write_text(private)
            parts = etc / 'sources.list.d'; parts.mkdir()
            (parts / 'private.list').write_text(private)
            for override in (f'Dir::Etc::sourcelist "{etc / "sources.list"}";',
                             f'Dir::Etc::sourceparts "{parts}";', f'Dir::Etc "{etc}";',
                             f'Dir "{root}";',
                             f'Binary::apt-get::Dir::Etc::sourcelist "{etc / "sources.list"}";'):
                with self.subTest(override=override):
                    config.write_text(base + override + '\n')
                    dump = run(['/usr/bin/apt-config', 'dump'])
                    self.assertTrue(dns.effective_proxy_clear(dump))
                    self.assertFalse(dns.effective_source_paths_default(dump))
                    # --print-uris enumerates the actual sources without downloads or DNS.
                    self.assertIn('http://private.invalid/ubuntu/dists/noble/InRelease',
                                  run(['/usr/bin/apt-get', '--print-uris', 'update']))

    def test_real_cli_shape_entry_and_file_read_boundaries_are_json(self):
        script = str(Path(dns.__file__).resolve())
        direct = subprocess.run([sys.executable, script, '--shape'], capture_output=True, text=True, timeout=5)
        self.assertEqual(direct.returncode, 0, direct.stderr)
        self.assertIsInstance(json.loads(direct.stdout)['available'], bool)
        child = r'''
import runpy, sys
from pathlib import Path
from unittest.mock import patch
script, mode = sys.argv[1:]
original = Path.read_text
def read(path, *args, **kwargs):
    if str(path) == '/run/systemd/resolve/stub-resolv.conf':
        if mode == 'missing': raise FileNotFoundError()
        return 'nameserver 127.0.0.53\noptions edns0 trust-ad\nsearch private.internal\n'
    return original(path, *args, **kwargs)
sys.argv = [script, '--shape']
with patch.object(Path, 'read_text', read):
    runpy.run_path(script, run_name='__main__')
'''
        for mode in ('normal', 'missing'):
            result = subprocess.run([sys.executable, '-c', child, script, mode], capture_output=True, text=True, timeout=5)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(json.loads(result.stdout)['available'], mode == 'normal')
            self.assertNotIn('private.internal', result.stdout + result.stderr)

    def test_shape_never_discloses_unknown_domain_or_option_values(self):
        shape = dns.stub_shape('nameserver 127.0.0.53\noptions edns0 private-option\nsearch private.internal\n')
        self.assertEqual(shape['search_suffix_count'], 1)
        self.assertFalse(shape['search_azure_dhcp_default'])
        self.assertFalse(shape['options_known'])
        self.assertNotIn('private', str(shape))
        self.assertTrue(dns.stub_shape('search reddog.microsoft.com\n')['search_azure_dhcp_default'])
        self.assertTrue(dns.stub_shape('search private.internal.cloudapp.net\n')['search_azure_cloudapp_suffix'])
        self.assertNotIn('private', str(dns.stub_shape('search private.internal.cloudapp.net\n')))

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

    def test_dynamic_dhcp_domain_is_ignored_without_relaxing_default_dns(self):
        class Installer:
            def run_command(self, _stage, command, **options):
                self_options.append(options)
                value = {'dns': '168.63.129.16', 'domain': 'private.internal.cloudapp.net', 'default-route': 'yes'}[command[-1]]
                if command[-1] == 'default-route':
                    assert command == ['/usr/bin/resolvectl', '--interface=2', 'default-route']
                return 0, 'Link 2 (eth0): ' + value + '\n', True
        self_options = []
        self.assertTrue(dns.Recovery(Installer(), True).default_routes())
        self.assertTrue(all(options == {'report_output': False} for options in self_options))

    def test_default_route_refuses_global_table_or_different_interface(self):
        for result in ('Link 2 (eth0):\n', 'Link 2 (eth0): yes\nLink 2 (eth0): yes\n',
                       'Global\n Protocols: other\nresolv.conf mode: stub\nLink 2 (eth0): yes\n',
                       'Link 3 (eth1): yes\n'):
            class Installer:
                def log(self, _message): pass
                def run_command(self, _stage, command, **_options):
                    if command[-1] == 'dns': return 0, 'Link 2 (eth0): 168.63.129.16\n', True
                    if command[-1] == 'domain': return 0, 'Global: private.internal\n', True
                    return 0, result, True
            self.assertFalse(dns.Recovery(Installer(), True).default_routes())

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
            self.assertEqual(resolver.read_text(), 'nameserver 1.1.1.1\nsearch .\n')
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
            (mapping['/etc/apt/apt.conf.d'] / 'comment-only').write_text('// Proxy and ProxyAutoDetect examples are not active settings\n')
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
                installer = Mock()
                installer.work_end = time.monotonic() + 5
                installer.run_command.return_value = (0, DEFAULT_SOURCE_CONFIG, True)
                recovery = dns.Recovery(installer, True)
                self.assertTrue(recovery.eligible(), recovery.reason)
                mirror.assert_called_once()
                mapping[dns.TARGET].write_text('nameserver 127.0.0.53\noptions edns0 trust-ad\nsearch private.internal\n')
                self.assertTrue(recovery.eligible())
                with patch.object(dns, 'RUNTIME_ROOT', str(root)):
                    recovery.namespace_prefix()
                    self.assertEqual((Path(recovery.directory.name) / 'resolv.conf').read_text(), 'nameserver 1.1.1.1\nsearch .\n')
                    recovery.close()
                mapping[dns.TARGET].write_text(files[dns.TARGET])
                for key, value in (('Dir::Etc::sourcelist', '/private/sources.list'),
                                   ('Dir::Etc::sourceparts', '/private/sources.list.d'),
                                   ('Dir::Etc', '/private/apt'), ('Dir', '/private/root')):
                    with self.subTest(key=key):
                        original = next(line for line in DEFAULT_SOURCE_CONFIG.splitlines()
                                        if line.startswith(key + ' '))
                        dump = DEFAULT_SOURCE_CONFIG.replace(original, key + ' "' + value + '";')
                        installer.run_command.return_value = (0, dump, True)
                        installer.run_command.reset_mock()
                        with patch.object(recovery, 'claim_lock') as claim:
                            self.assertFalse(recovery.try_enable('E: Failed to fetch x Temporary failure resolving x'))
                            claim.assert_not_called()
                        self.assertEqual(recovery.reason, 'effective_apt_source_paths_custom_or_unknown')
                        installer.run_command.assert_called_once_with(
                            'apt-proxy-config', ['/usr/bin/apt-config', 'dump'], report_output=False)
                        self.assertEqual(recovery.prefix, [])
                        self.assertIsNone(recovery.directory)
                installer.run_command.return_value = (0, DEFAULT_SOURCE_CONFIG, True)
                for uri in ('mirror+file:/etc/apt/private-mirrors.txt', 'custom://internal.example/repo',
                            'custom:internal.example/repo', 'ssh:/internal.example/repo',
                            'https://archive.ubuntu.com/ubuntu#private', 'https://archive.ubuntu.com/ubuntu?token=x'):
                    source.write_text('URIs: ' + uri + '\n')
                    self.assertFalse(recovery.eligible(), uri)

if __name__ == '__main__': unittest.main()
