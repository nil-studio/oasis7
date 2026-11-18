#!/usr/bin/env python3
"""Pinned browser downloads and complete Ubuntu dependencies for ordinary CI.

This program runs without sudo. Only the existing bounded APT helper installs
the emitted package arguments. Cache hits are verified downloads, not proof of
installed libraries or a working browser. Candidate jobs never save caches.
"""
import argparse
import contextlib
import hashlib
import importlib.util
import json
import os
from pathlib import Path, PurePosixPath
import platform
import re
import shutil
import stat
import subprocess
import sys
import tempfile
import time
import zipfile

from ci_process_runner import run


ROOT = Path(__file__).resolve().parent.parent
TOOLS = ROOT / 'scripts/ci-browser-tools'
AGENT_VERSION = '0.37.1'
PLAYWRIGHT_VERSION = '1.62.1'
CHROME_VERSION = '158.0.8127.36'
SHELL_VERSION = '151.0.7922.34'
DOWNLOAD_BUDGET = 180
MAX_CACHE_BYTES = 256 * 1024 ** 2
ARCHIVES = {
    'chrome': dict(
        name='chrome-linux64.zip', version=CHROME_VERSION, size=208062943,
        sha256='dba9cc838e1ce269c33ea68170881127bd15d078fc3ca5891c90802819a4d7c7',
        url=f'https://storage.googleapis.com/chrome-for-testing-public/{CHROME_VERSION}/linux64/chrome-linux64.zip'),
    # Versions, lengths and hashes from packages.ubuntu.com/noble/all/<name>/download.
    'fonts-noto-cjk': dict(
        name='fonts-noto-cjk_20230817+repack1-3_all.deb', version='1:20230817+repack1-3',
        size=61229778, sha256='7d64b985f6fe128c99eae5610d5c047338e572bdcfb2bb09736be01b824a7f6c',
        url='https://archive.ubuntu.com/ubuntu/pool/main/f/fonts-noto-cjk/fonts-noto-cjk_20230817+repack1-3_all.deb'),
    'fonts-freefont-ttf': dict(
        name='fonts-freefont-ttf_20211204+svn4273-2_all.deb', version='20211204+svn4273-2',
        size=5640794, sha256='c8283ec9ca390e6ad8d2114cb0942182db62bb97f5142c2f955218fc5f2027b4',
        url='https://archive.ubuntu.com/ubuntu/pool/main/f/fonts-freefont/fonts-freefont-ttf_20211204+svn4273-2_all.deb'),
}

# Complete v0.37.1 apt_dependency_specs, resolved for Ubuntu 24.04 x64.
# https://github.com/vercel-labs/agent-browser/blob/v0.37.1/cli/src/install.rs
AGENT_PACKAGES = (
    'libxcb-shm0', 'libx11-xcb1', 'libx11-6', 'libxcb1', 'libxext6', 'libxrandr2',
    'libxcomposite1', 'libxcursor1', 'libxdamage1', 'libxfixes3', 'libxi6',
    'libgtk-3-0t64', 'libpangocairo-1.0-0', 'libpango-1.0-0', 'libatk1.0-0t64',
    'libcairo-gobject2', 'libcairo2', 'libgdk-pixbuf-2.0-0', 'libxrender1',
    'libasound2t64', 'libfreetype6', 'libfontconfig1', 'libdbus-1-3', 'libnss3',
    'libnss3-tools', 'libnspr4', 'libatk-bridge2.0-0t64', 'libdrm2', 'libxkbcommon0',
    'libatspi2.0-0t64', 'libcups2t64', 'libxshmfence1', 'libgbm1',
    'fonts-noto-color-emoji', 'fonts-noto-cjk', 'fonts-freefont-ttf',
)
# Complete tools + chromium groups for ubuntu24.04-x64 at Playwright v1.62.1.
# https://github.com/microsoft/playwright/blob/v1.62.1/packages/playwright-core/src/server/registry/nativeDeps.ts
PLAYWRIGHT_PACKAGES = (
    'xvfb', 'fonts-noto-color-emoji', 'fonts-unifont', 'libfontconfig1', 'libfreetype6',
    'xfonts-cyrillic', 'xfonts-scalable', 'fonts-liberation', 'fonts-ipafont-gothic',
    'fonts-wqy-zenhei', 'fonts-tlwg-loma-otf', 'fonts-freefont-ttf',
    'libasound2t64', 'libatk-bridge2.0-0t64', 'libatk1.0-0t64', 'libatspi2.0-0t64',
    'libcairo2', 'libcups2t64', 'libdbus-1-3', 'libdrm2', 'libgbm1', 'libglib2.0-0t64',
    'libnspr4', 'libnss3', 'libpango-1.0-0', 'libx11-6', 'libxcb1', 'libxcomposite1',
    'libxdamage1', 'libxext6', 'libxfixes3', 'libxkbcommon0', 'libxrandr2',
)


class BrowserError(ValueError):
    def __init__(self, message, code=1):
        super().__init__(message)
        self.code = code


def safe_path(value):
    path = Path(value)
    if (not path.is_absolute() or '..' in path.parts
            or any(ord(char) < 32 for char in str(path))
            or any(parent.is_symlink() for parent in (path, *path.parents))):
        raise BrowserError('browser preparation requires a safe absolute path without symlinks')
    return path


def definition(component):
    # The input definition changes on a version/digest/schema change, not on
    # every source commit. Restore still verifies against the current pins.
    data = dict(schema=1, platform='ubuntu-24.04-Linux-X64', component=component,
                archive=ARCHIVES[component])
    return hashlib.sha256(json.dumps(data, sort_keys=True).encode()).hexdigest()


def metadata(component, cache_base=None):
    base = safe_path(cache_base or Path(os.environ.get('RUNNER_TEMP', tempfile.gettempdir())) / 'oasis7-browser-archives')
    identity = definition(component)
    return dict(root=str(base / f'{component}-{identity}'),
                key=f'ci-browser-archive-v1-Linux-X64-ubuntu24.04-{component}-{identity}')


def components(browser):
    return ('fonts-noto-cjk', 'fonts-freefont-ttf', 'chrome') if browser == 'agent' else ('fonts-freefont-ttf',)


def packages(browser):
    return AGENT_PACKAGES if browser == 'agent' else PLAYWRIGHT_PACKAGES


def environment(extra=None):
    result = {key: value for key, value in os.environ.items()
              if key not in ('GH_TOKEN', 'GITHUB_TOKEN', 'ACTIONS_RUNTIME_TOKEN')}
    result.update(extra or {})
    return result


def checked_command(command, work, name, timeout, extra=None):
    if timeout <= 0:
        raise BrowserError('browser preparation deadline expired', 124)
    output = work / f'{name}.log'
    with output.open('w') as stream, contextlib.redirect_stdout(sys.stderr):
        result = run(command, ROOT, environment(extra), 'ci-browser', name,
                     work / 'processes.jsonl', timeout_seconds=timeout, log_stream=stream)
    if not result['passed']:
        with output.open('rb') as stream:
            stream.seek(max(0, output.stat().st_size - 4096))
            print(stream.read().decode('utf-8', errors='replace'), file=sys.stderr)
        code = 124 if result['timeout'] else 130 if result['cancelled'] else result['exit_code']
        if type(code) is not int or not 1 <= code <= 255:
            code = 1
        raise BrowserError(f'{name} failed: exit={result["exit_code"]}, timeout={result["timeout"]}, '
                           f'cancelled={result["cancelled"]}, cleanup={result["cleanup_result"]}; log={output}', code)
    with output.open('rb') as stream:
        text = stream.read(1024 * 1024 + 1)
    if len(text) > 1024 * 1024:
        raise BrowserError(f'{name} exceeded the bounded output contract')
    return text.decode('utf-8', errors='replace')


def verify_file(path, spec):
    fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
    with os.fdopen(fd, 'rb') as stream:
        before = os.fstat(stream.fileno())
        if not stat.S_ISREG(before.st_mode) or before.st_nlink != 1 or before.st_size != spec['size']:
            raise BrowserError(f'invalid archive type, links or length: {path.name}')
        digest = hashlib.file_digest(stream, 'sha256').hexdigest()
        after = os.fstat(stream.fileno())
    identity = lambda item: (item.st_dev, item.st_ino, item.st_size, item.st_mtime_ns, item.st_ctime_ns)
    if identity(before) != identity(after) or digest != spec['sha256']:
        raise BrowserError(f'archive changed or SHA256 mismatch: {path.name}')


def check_archive(component, root, *, deadline=None):
    root = safe_path(root)
    spec = ARCHIVES[component]
    if not root.is_dir() or sorted(item.name for item in root.iterdir()) != [spec['name']]:
        raise BrowserError(f'{component} cache is incomplete or has unexpected entries')
    archive = root / spec['name']
    verify_file(archive, spec)
    if deadline is not None and time.monotonic() >= deadline:
        raise BrowserError('browser archive verification deadline expired', 124)
    return archive


def ensure_archive(component, root, *, deadline=None):
    root = safe_path(root)
    if root.exists():
        archive = check_archive(component, root, deadline=deadline)
        print(f'ci-browser component={component} cache=verified', file=sys.stderr)
        return archive
    deadline = deadline if deadline is not None else time.monotonic() + DOWNLOAD_BUDGET
    root.parent.mkdir(parents=True, exist_ok=True)
    spec = ARCHIVES[component]
    with tempfile.TemporaryDirectory(prefix='.browser-download-', dir=root.parent) as temporary:
        work = Path(temporary)
        stage = work / 'archive'
        stage.mkdir()
        target = stage / spec['name']
        remaining = deadline - time.monotonic()
        checked_command(['curl', '--fail', '--location', '--silent', '--show-error',
                         '--proto', '=https', '--proto-redir', '=https', '--connect-timeout', '15',
                         '--retry', '2', '--retry-max-time', str(max(1, int(remaining))),
                         '--max-time', str(max(1, int(remaining))), '--max-filesize', str(spec['size']),
                         '--output', str(target), spec['url']], work, 'download', remaining)
        check_archive(component, stage, deadline=deadline)
        if root.exists() or root.is_symlink():
            raise BrowserError('browser archive destination appeared during download')
        stage.rename(root)
    print(f'ci-browser component={component} cache=cold_download_verified', file=sys.stderr)
    return root / spec['name']


def extract_chrome(archive, work):
    destination = work / 'chrome'
    if destination.exists() or destination.is_symlink():
        raise BrowserError('browser extraction destination must be new')
    with zipfile.ZipFile(archive) as source:
        entries = source.infolist()
        if len(entries) > 1000 or sum(item.file_size for item in entries) > 600 * 1024 ** 2:
            raise BrowserError('Chrome archive exceeds its extraction budget')
        seen = set()
        for item in entries:
            name = PurePosixPath(item.filename)
            mode = item.external_attr >> 16
            if (name.is_absolute() or '..' in name.parts or '\\' in item.filename
                    or not name.parts or name.parts[0] != 'chrome-linux64'
                    or any(ord(char) < 32 for char in item.filename)
                    or name in seen or mode & 0o7000
                    or stat.S_IFMT(mode) not in (0, stat.S_IFDIR, stat.S_IFREG)):
                raise BrowserError('unsafe Chrome archive member')
            seen.add(name)
        destination.mkdir()
        for item in entries:
            target = destination / item.filename
            if item.is_dir():
                target.mkdir(parents=True, exist_ok=True)
                continue
            target.parent.mkdir(parents=True, exist_ok=True)
            with source.open(item) as incoming, target.open('xb') as outgoing:
                shutil.copyfileobj(incoming, outgoing, 1024 * 1024)
            target.chmod(0o755 if (item.external_attr >> 16) & 0o111 else 0o644)
    binary = destination / 'chrome-linux64/chrome'
    if not binary.is_file() or not os.access(binary, os.X_OK):
        raise BrowserError('Chrome archive has no executable browser')
    return binary


def require_platform():
    release = platform.freedesktop_os_release()
    if (platform.system(), platform.machine(), release.get('ID'), release.get('VERSION_ID')) != (
            'Linux', 'x86_64', 'ubuntu', '24.04'):
        raise BrowserError('browser system dependency preparation supports Ubuntu 24.04 x64 only')


def missing_packages(browser, query=None):
    wanted = packages(browser)
    if query is None:
        query = subprocess.run(['/usr/bin/dpkg-query', '-W',
                                '-f=${Package}\t${db:Status-Status}\t${db:Status-Eflag}\t${Architecture}\t${Version}\n', '--', *wanted],
                               text=True, capture_output=True, timeout=10, env=environment())
    if query.returncode not in (0, 1):
        raise BrowserError('could not inspect installed browser dependencies')
    installed, seen = {}, set()
    for line in query.stdout.splitlines():
        fields = line.split('\t')
        if len(fields) != 5 or fields[0] not in wanted or fields[0] in seen:
            raise BrowserError('unexpected dpkg dependency state')
        name, status, error_flag, architecture, version = fields
        seen.add(name)
        if error_flag != 'ok':
            raise BrowserError(f'broken browser dependency: {name}')
        if status == 'installed':
            if architecture not in ('amd64', 'all'):
                raise BrowserError(f'foreign-architecture browser dependency: {name}')
            if name in ARCHIVES and version != ARCHIVES[name]['version']:
                raise BrowserError(f'installed {name} conflicts with the fixed font version')
            installed[name] = version
        elif status not in ('not-installed', 'config-files'):
            raise BrowserError(f'partially configured browser dependency: {name}')
    return [name for name in wanted if name not in installed]


def prepare(browser, work, cache_base=None):
    require_platform()
    work = safe_path(work)
    work.mkdir(parents=True, exist_ok=False)
    deadline = time.monotonic() + DOWNLOAD_BUDGET
    archives = {component: ensure_archive(component, metadata(component, cache_base)['root'], deadline=deadline)
                for component in components(browser)}
    if browser == 'agent':
        extract_chrome(archives['chrome'], work)
    missing = missing_packages(browser)
    arguments = [str(archives[name]) if name in archives else name for name in packages(browser)] if missing else []
    (work / 'apt-arguments.txt').write_text(''.join(value + '\n' for value in arguments))
    print(json.dumps(dict(browser=browser, missing=missing, apt_required=bool(missing)), sort_keys=True))


def check_tools(browser):
    node = subprocess.check_output(['node', '--version'], text=True, timeout=10).strip()
    if not re.fullmatch(r'v24\.[0-9]+\.[0-9]+', node):
        raise BrowserError('CI browser tools require Node 24')
    names = {'agent-browser': AGENT_VERSION, 'playwright': PLAYWRIGHT_VERSION,
             'playwright-core': PLAYWRIGHT_VERSION}
    for name, version in names.items():
        manifest = TOOLS / 'node_modules' / name / 'package.json'
        if json.loads(manifest.read_text()).get('version') != version:
            raise BrowserError(f'installed browser tool version mismatch: {name}')
    # The npm integrity covers this bundled binary. Never invoke the package's
    # postinstall fallback, which may download an unverified release or relink
    # an unrelated global installation.
    binary = TOOLS / 'node_modules/agent-browser/bin/agent-browser-linux-x64'
    if browser == 'agent' and (binary.is_symlink() or not binary.is_file()):
        raise BrowserError('pinned npm package is missing its bundled agent-browser binary')
    return binary


def install_tools():
    with tempfile.TemporaryDirectory(prefix='ci-browser-npm-') as temporary:
        work = Path(temporary)
        checked_command(['npm', 'ci', '--prefix', str(TOOLS), '--ignore-scripts',
                         '--no-audit', '--no-fund', '--fetch-retries=2', '--fetch-timeout=20000'],
                        work, 'npm-ci', 120)
        check_tools('agent')


def activate(browser, work, github_env=None):
    require_platform()
    work = safe_path(work)
    missing = missing_packages(browser)
    if missing:
        raise BrowserError('browser dependencies remain missing: ' + ', '.join(missing))
    agent = check_tools(browser)
    if browser == 'agent':
        agent.chmod(0o755)
        actual = checked_command([str(agent), '--version'], work, 'agent-version', 10).strip()
        if actual != f'agent-browser {AGENT_VERSION}':
            raise BrowserError('agent-browser executable version mismatch')
        chrome = work / 'chrome/chrome-linux64/chrome'
        actual = checked_command([str(chrome), '--version'], work, 'chrome-version', 10).strip()
        if actual != f'Google Chrome for Testing {CHROME_VERSION}':
            raise BrowserError('Chrome executable version mismatch')
        marker = 'oasis7-ci-browser-ready'
        dom = checked_command([str(chrome), '--headless', '--no-sandbox', '--disable-dev-shm-usage',
                               '--user-data-dir=' + str(work / 'probe-profile'), '--dump-dom',
                               f'data:text/html,<body>{marker}</body>'], work, 'chrome-launch', 30)
        if f'<body>{marker}</body>' not in dom:
            raise BrowserError('Chrome renderer probe did not return the expected document')
        values = dict(AGENT_BROWSER_BIN=str(agent), AGENT_BROWSER_EXECUTABLE_PATH=str(chrome))
    else:
        browsers = work / 'playwright-browsers'
        extra = dict(PLAYWRIGHT_BROWSERS_PATH=str(browsers))
        checked_command(['node', str(TOOLS / 'node_modules/playwright/cli.js'),
                         'install', 'chromium', '--only-shell'], work, 'playwright-install', 180, extra)
        probe = """const {chromium}=require(process.argv[1]);
(async()=>{const browser=await chromium.launch({headless:true});
try {if(browser.version()!==process.argv[2]) throw Error('browser version mismatch');
const page=await browser.newPage(); await page.setContent('<body>oasis7-ci-browser-ready</body>');
if(await page.textContent('body')!=='oasis7-ci-browser-ready') throw Error('renderer probe failed');
console.log('browser-ready '+browser.version());} finally {await browser.close();}
})().catch(error=>{console.error(error);process.exitCode=1;});"""
        checked_command(['node', '-e', probe, str(TOOLS / 'node_modules/playwright'), SHELL_VERSION],
                        work, 'playwright-launch', 30, extra)
        values = dict(OASIS7_PLAYWRIGHT_MODULE=str(TOOLS / 'node_modules/playwright'), **extra)
    if github_env:
        with Path(github_env).open('a') as stream:
            stream.writelines(f'{key}={value}\n' for key, value in values.items())
    print(json.dumps(dict(browser=browser, ready=True, environment=values), sort_keys=True))
    return values


def admission(component, cache_base, key, environment_values):
    spec = importlib.util.spec_from_file_location('ci_tool_cache', ROOT / 'scripts/ci-tool-cache.py')
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    values = metadata(component, cache_base)
    return module.admit_save(
        Path(values['root']), key, environment_values, repository_root=ROOT,
        expected_key=values['key'], writer_workflow='.github/workflows/rust-cache.yml',
        writer_job='browser-archive-cache',
        check_payload=lambda root, deadline=None: check_archive(component, root, deadline=deadline),
        max_cache_bytes=MAX_CACHE_BYTES, max_entries=4, label='browser-archive')


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('command', choices=('metadata', 'fetch', 'tools', 'prepare', 'activate', 'admit-save'))
    parser.add_argument('--component', choices=tuple(ARCHIVES))
    parser.add_argument('--browser', choices=('agent', 'playwright'))
    parser.add_argument('--cache-base', type=Path)
    parser.add_argument('--work-dir', type=Path)
    parser.add_argument('--github-output', type=Path)
    parser.add_argument('--github-env', type=Path)
    parser.add_argument('--key')
    args = parser.parse_args(argv)
    if args.command in ('fetch', 'admit-save') and not args.component:
        parser.error('fetch/admit-save require --component')
    if args.command in ('prepare', 'activate') and (not args.browser or not args.work_dir):
        parser.error('prepare/activate require --browser and --work-dir')
    if args.command == 'metadata' and not (args.component or args.browser):
        parser.error('metadata requires --component or --browser')
    if args.command == 'admit-save' and not args.key:
        parser.error('admit-save requires --key')
    try:
        if args.command == 'metadata':
            selected = (args.component,) if args.component else components(args.browser)
            values = {}
            for component in selected:
                prefix = '' if args.component else component.replace('-', '_') + '_'
                values.update({prefix + key: value for key, value in metadata(component, args.cache_base).items()})
            if args.github_output:
                with args.github_output.open('a') as stream:
                    stream.writelines(f'{key}={value}\n' for key, value in values.items())
            print(json.dumps(values, sort_keys=True))
        elif args.command == 'fetch':
            ensure_archive(args.component, metadata(args.component, args.cache_base)['root'])
        elif args.command == 'tools':
            install_tools()
        elif args.command == 'prepare':
            prepare(args.browser, args.work_dir, args.cache_base)
        elif args.command == 'activate':
            activate(args.browser, args.work_dir, args.github_env)
        else:
            report = admission(args.component, args.cache_base, args.key, os.environ)
            if args.github_output:
                with args.github_output.open('a') as stream:
                    stream.write('save=' + str(report['save']).lower() + '\n')
            print(json.dumps(report, sort_keys=True))
    except (OSError, ValueError, subprocess.SubprocessError, zipfile.BadZipFile) as error:
        print(f'error: {error}', file=sys.stderr)
        return getattr(error, 'code', 1)
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
