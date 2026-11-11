"""Opt-in hosted-runner DNS recovery confined to an owned mount namespace."""
import fcntl
import ctypes
import os
from pathlib import Path
import re
import socket
import stat
import subprocess
import sys
import tempfile
import time
from urllib.parse import urlsplit

DOMAIN = 'archive.ubuntu.com'
TARGET = '/run/systemd/resolve/stub-resolv.conf'
LOCK_PATH = '/run/lock/oasis7-ci-apt-dns.lock'
RUNTIME_ROOT = '/run'
MIRROR_PATH = '/etc/apt/apt-mirrors.txt'
MIRROR_URI = 'mirror+file:/etc/apt/apt-mirrors.txt'


def direct_source_allowed(value, image):
    url = urlsplit(value)
    if (url.scheme not in ('http', 'https') or url.username is not None or
            url.password is not None or url.port is not None or url.query or url.fragment):
        return False
    if url.netloc in ('azure.archive.ubuntu.com', 'archive.ubuntu.com', 'security.ubuntu.com'):
        return url.path in ('/ubuntu', '/ubuntu/')
    version = {'ubuntu22': '22.04', 'ubuntu24': '24.04'}.get(image)
    return (url.scheme == 'https' and url.netloc == 'packages.microsoft.com' and version is not None and
            url.path in (f'/ubuntu/{version}/prod', f'/ubuntu/{version}/prod/'))


def default_mirror_allowed():
    fd = os.open(MIRROR_PATH, os.O_RDONLY | os.O_NOFOLLOW)
    with os.fdopen(fd) as stream:
        info = os.fstat(stream.fileno())
        if not stat.S_ISREG(info.st_mode) or info.st_uid != 0 or info.st_mode & 0o022:
            return False
        text = stream.read(65537)
    if len(text) > 65536:
        return False
    rows = [line.split() for line in text.splitlines() if line.strip() and not line.lstrip().startswith('#')]
    expected = [('http', 'azure.archive.ubuntu.com'), ('https', 'archive.ubuntu.com'),
                ('https', 'security.ubuntu.com')]
    if len(rows) != len(expected):
        return False
    for row, (scheme, host) in zip(rows, expected):
        url = urlsplit(row[0])
        if (url.scheme != scheme or url.netloc != host or url.path not in ('/ubuntu', '/ubuntu/') or
                url.query or url.fragment or len(row) > 2 or
                (len(row) == 2 and not re.fullmatch(r'priority:[0-9]+', row[1]))):
            return False
    return True


def hosted_allowed(enabled, env, distribution):
    return (enabled and distribution == 'ubuntu' and
            env.get('GITHUB_ACTIONS') == 'true' and
            env.get('RUNNER_ENVIRONMENT') == 'github-hosted' and
            env.get('RUNNER_OS') == 'Linux' and env.get('ImageOS') in ('ubuntu22', 'ubuntu24') and
            not any(env.get(k) for k in ('HTTP_PROXY', 'HTTPS_PROXY', 'ALL_PROXY',
                                        'http_proxy', 'https_proxy', 'all_proxy', 'NO_PROXY', 'no_proxy', 'APT_CONFIG',
                                        'RES_OPTIONS', 'LOCALDOMAIN', 'HOSTALIASES')))


def dns_failure(output):
    return any(line.startswith(('E:', 'W:')) and 'Failed to fetch' in line and
               'Temporary failure resolving' in line for line in output.splitlines())


def enter_private_namespace():
    before = os.readlink('/proc/self/ns/mnt')
    libc = ctypes.CDLL(None, use_errno=True)
    if libc.unshare(0x00020000) != 0:  # CLONE_NEWNS
        raise OSError(ctypes.get_errno(), 'private mount namespace unavailable')
    if os.readlink('/proc/self/ns/mnt') == before:
        raise RuntimeError('mount namespace identity did not change')


class Recovery:
    def __init__(self, installer, enabled=False):
        self.installer = installer
        self.enabled = enabled
        self.prefix = []
        self.directory = None
        self.lock = None
        self.reason = 'not_checked'

    def decline(self, reason):
        self.reason = reason
        return False

    def eligible(self):
        try:
            return self._eligible()
        except (OSError, UnicodeError, ValueError):
            return self.decline('unsupported_configuration')

    def _eligible(self):
        if not self.enabled:
            return self.decline('opt_in_required')
        release = dict(re.findall(r'^(\w+)="?([^"\n]+)"?$', Path('/etc/os-release').read_text(), re.M))
        if not hosted_allowed(self.enabled, os.environ, release.get('ID')):
            return self.decline('hosted_ubuntu_or_proxy_gate')
        if not Path('/etc/resolv.conf').is_symlink() or os.path.realpath('/etc/resolv.conf') != TARGET:
            return self.decline('custom_resolver_path')
        lines = [line.strip() for line in Path(TARGET).read_text().splitlines()
                 if line.strip() and not line.lstrip().startswith('#')]
        if any(line not in ('nameserver 127.0.0.53', 'options edns0 trust-ad', 'search .') for line in lines):
            return self.decline('custom_stub_configuration')
        if lines.count('nameserver 127.0.0.53') != 1:
            return self.decline('custom_stub_nameserver')
        upstream = Path('/run/systemd/resolve/resolv.conf').read_text()
        if re.findall(r'^nameserver\s+(\S+)\s*$', upstream, re.M) != ['168.63.129.16']:
            return self.decline('custom_upstream_configuration')
        resolved = [Path('/etc/systemd/resolved.conf'), *Path('/etc/systemd/resolved.conf.d').glob('*')]
        for path in resolved:
            if not path.is_file():
                continue
            if path.stat().st_size > 65536:
                return self.decline('oversized_resolved_configuration')
            if re.search(r'^\s*(?:DNS|FallbackDNS|Domains)\s*=\s*\S', path.read_text(), re.M):
                return self.decline('explicit_resolved_override')
        hosts = re.findall(r'^hosts:\s*(.*)$', Path('/etc/nsswitch.conf').read_text(), re.M)
        if hosts != ['files dns']:
            return self.decline('custom_nss_configuration')
        configs = [Path('/etc/apt/apt.conf'), *Path('/etc/apt/apt.conf.d').glob('*')]
        if len(configs) > 128:
            return self.decline('too_many_apt_configurations')
        for path in configs:
            if path.is_file() and (path.stat().st_size > 65536 or 'proxy' in path.read_text().lower()):
                return self.decline('custom_apt_proxy_or_oversized_configuration')
        sources = [Path('/etc/apt/sources.list'), *Path('/etc/apt/sources.list.d').glob('*')]
        if len(sources) > 128:
            return self.decline('too_many_apt_sources')
        for path in sources:
            if not path.is_file():
                continue
            if path.stat().st_size > 65536:
                return self.decline('oversized_apt_source')
            source_text = '\n'.join(re.split(r'\s+#', line, maxsplit=1)[0] for line in path.read_text().splitlines()
                                    if not line.lstrip().startswith('#'))
            for value in source_text.split():
                # Deb822 field names end in ':' with no value. Every actual
                # URI, including non-// schemes, must pass this closed gate.
                if not re.match(r'^[A-Za-z][A-Za-z0-9+.-]*:.+', value):
                    continue
                if value == MIRROR_URI:
                    if not default_mirror_allowed():
                        return self.decline('custom_mirror_file')
                elif not direct_source_allowed(value, os.environ.get('ImageOS')):
                    return self.decline('custom_apt_source_url')
        self.reason = 'eligible' if all(Path(p).is_file() for p in ('/usr/bin/mount', '/usr/bin/resolvectl')) else 'resolver_tool_unavailable'
        return self.reason == 'eligible'

    def default_routes(self):
        for option in ('dns', 'domain', 'default-route'):
            status, output, valid = self.installer.run_command('dns-config-' + option, ['/usr/bin/resolvectl', option], report_output=False)
            if not valid or status != 0:
                return self.decline('dynamic_resolver_query_failed')
            if not output.strip():
                return self.decline('dynamic_resolver_query_empty')
            has_default_dns = False
            for line in output.splitlines():
                if not re.fullmatch(r'(Global|Link \d+ \([\w.-]+\)):.*', line):
                    return self.decline('dynamic_resolver_shape_unknown')
                value = line.split(':', 1)[1].strip()
                allowed = {'dns': ('', '168.63.129.16'), 'domain': ('', '~.'),
                           'default-route': ('', 'yes', 'no')}[option]
                if value not in allowed:
                    return self.decline('dynamic_resolver_configuration_custom')
                has_default_dns |= value == '168.63.129.16'
            if option == 'dns' and not has_default_dns:
                return self.decline('dynamic_default_dns_missing')
        return True

    def try_enable(self, output):
        if self.prefix or not dns_failure(output) or not self.eligible():
            return False
        self.lock = os.open(LOCK_PATH, os.O_CREAT | os.O_RDWR | os.O_NOFOLLOW, 0o600)
        info = os.fstat(self.lock)
        if not stat.S_ISREG(info.st_mode) or info.st_uid != 0 or stat.S_IMODE(info.st_mode) != 0o600 or info.st_nlink != 1:
            self.close()
            return False
        try:
            fcntl.flock(self.lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            self.close()
            return False
        if not self.default_routes():
            self.close()
            return False
        probe = [sys.executable, str(Path(__file__).resolve()), '--probe']
        status, _, valid = self.installer.run_command('dns-default', probe)
        if not valid or status != 75 or self.installer.cancelled:
            self.close()
            return False
        self.directory = tempfile.TemporaryDirectory(prefix='oasis7-ci-apt-dns-', dir=RUNTIME_ROOT)
        resolver = Path(self.directory.name) / 'resolv.conf'
        fd = os.open(resolver, os.O_CREAT | os.O_EXCL | os.O_WRONLY, 0o644)
        os.fchmod(fd, 0o644)
        with os.fdopen(fd, 'w') as stream:
            stream.write('nameserver 1.1.1.1\n')
            stream.flush()
            os.fsync(stream.fileno())
        prefix = [sys.executable, str(Path(__file__).resolve()), '--namespace', str(resolver),
                  str(self.installer.work_end), '--']
        status, _, valid = self.installer.run_command('dns-alternate', prefix + probe)
        if not valid or status != 0 or self.installer.cancelled:
            self.close()
            return False
        self.prefix = prefix
        self.installer.log('dns_recovery=private_namespace alternate_probe=passed host_resolver_modified=false')
        return True

    def close(self):
        self.prefix = []
        if self.directory is not None:
            self.directory.cleanup()
            self.directory = None
        if self.lock is not None:
            os.close(self.lock)
            self.lock = None


def namespace_main(argv):
    if len(argv) < 6 or argv[0] != '--namespace' or argv[3] != '--':
        raise RuntimeError('invalid namespace entry')
    enter_private_namespace()
    resolver, deadline = Path(argv[1]), float(argv[2])
    info = resolver.stat()
    parent = resolver.parent.stat()
    if (os.geteuid() != 0 or info.st_uid != 0 or not stat.S_ISREG(info.st_mode) or
            stat.S_IMODE(info.st_mode) != 0o644 or resolver.is_symlink() or
            parent.st_uid != 0 or stat.S_IMODE(parent.st_mode) != 0o700):
        raise RuntimeError('resolver must be an owned private regular file')
    if resolver.read_text() != 'nameserver 1.1.1.1\n' or os.path.realpath('/etc/resolv.conf') != TARGET:
        raise RuntimeError('unexpected resolver configuration')
    remaining = deadline - time.monotonic()
    if remaining <= 0:
        raise RuntimeError('original apt deadline exhausted')
    subprocess.run(['/usr/bin/mount', '--make-rprivate', '/'], check=True,
                   timeout=remaining, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    remaining = deadline - time.monotonic()
    if remaining <= 0:
        raise RuntimeError('original apt deadline exhausted')
    subprocess.run(['/usr/bin/mount', '--bind', str(resolver), TARGET], check=True,
                   timeout=remaining, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    if time.monotonic() >= deadline:
        raise RuntimeError('original apt deadline exhausted')
    os.execv(argv[4], argv[4:])


if __name__ == '__main__':
    if sys.argv[1:] == ['--probe']:
        try:
            addresses = socket.getaddrinfo(DOMAIN, 80, socket.AF_INET, socket.SOCK_STREAM)
            sys.exit(0 if addresses else 2)
        except socket.gaierror as error:
            sys.exit(75 if error.errno == socket.EAI_AGAIN else 2)
    namespace_main(sys.argv[1:])
