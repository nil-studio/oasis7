#!/usr/bin/env python3
"""Prepare or check the isolated, fixed worker-build tool cache.

Cache readers use an exact key and never publish. Provisioning does not use
GitHub tokens. Optional save admission reads Actions metadata with a read-only
token; the trusted workflow owns saving. This helper never uploads or deletes.
"""
import argparse
import contextlib
from functools import lru_cache
import hashlib
import importlib.util
from itertools import chain
import json
import os
from pathlib import Path
import re
import stat
import subprocess
import sys
import tarfile
import tempfile
import time
import tomllib

from ci_process_runner import run


VERSION = '0.7.5'
INSTALL_TIMEOUT_SECONDS = 600
PROBE_TIMEOUT_SECONDS = 10
ROOT = Path(__file__).resolve().parent.parent
WRITER_WORKFLOW = '.github/workflows/rust-cache.yml'
WRITER_JOB = 'worker-build-cache'
MAX_CACHE_BYTES = 128 * 1024 ** 2
MAX_CACHE_ENTRIES = 128
SAVE_TIMEOUT_SECONDS = 120


class ToolError(ValueError):
    def __init__(self, message, code=1):
        super().__init__(message)
        self.code = code


def cache_root(value=None):
    if value is None:
        base = os.environ.get('XDG_CACHE_HOME') or str(Path.home() / '.cache')
        value = Path(base) / 'oasis7' / 'worker-build' / VERSION
    path = Path(value)
    if not path.is_absolute() or '..' in path.parts or any(c in str(path) for c in '\r\n'):
        raise ToolError('worker-build cache root must be a safe absolute path')
    if any(parent.is_symlink() for parent in (path, *path.parents)):
        raise ToolError('worker-build cache root must not use symlinks')
    return path


def metadata(root):
    root = cache_root(root)
    digest = hashlib.sha256()
    for name in ('ci-worker-build.py', 'ci_process_runner.py'):
        digest.update(name.encode() + b'\0' + hashlib.sha256((ROOT / 'scripts' / name).read_bytes()).digest())
    return dict(version=VERSION, root=str(root), bin=str(root / 'bin' / 'worker-build'),
                definition_sha=digest.hexdigest())


def command_environment(environment):
    return {key: value for key, value in environment.items()
            if key not in ('RUSTC_WRAPPER', 'GH_TOKEN', 'GITHUB_TOKEN', 'ACTIONS_RUNTIME_TOKEN')}


def checked_process(command, cwd, evidence, name, timeout, *, log=None):
    if timeout <= 0:
        raise ToolError('worker-build preparation deadline expired', 124)
    environment = command_environment(os.environ)
    # stdout is reserved for --print-bin / metadata. The existing process
    # runner retains deadline, cancellation and descendant-cleanup contracts.
    with contextlib.redirect_stdout(sys.stderr):
        result = run(command, cwd, environment, 'ci-tools', name,
                     evidence / 'processes.jsonl', timeout_seconds=timeout, log_stream=log)
    if result['passed']:
        return
    code = 124 if result['timeout'] else 130 if result['cancelled'] else result['exit_code']
    if type(code) is not int or not 1 <= code <= 255:
        code = 1
    raise ToolError(f'worker-build {name} failed: exit={result["exit_code"]}, '
                    f'timeout={result["timeout"]}, cancelled={result["cancelled"]}, '
                    f'cleanup={result["cleanup_result"]}', code)


def check(root, *, deadline=None):
    root = cache_root(root)
    binary = root / 'bin' / 'worker-build'
    if (not root.is_dir() or (root / 'bin').is_symlink() or binary.is_symlink()
            or not binary.exists() or not stat.S_ISREG(binary.stat().st_mode)
            or not os.access(binary, os.X_OK)):
        raise ToolError('missing or unsafe worker-build cache executable')
    timeout = PROBE_TIMEOUT_SECONDS
    if deadline is not None:
        timeout = min(timeout, deadline - time.monotonic())
    with tempfile.TemporaryDirectory(prefix='worker-build-probe-') as temporary:
        evidence = Path(temporary)
        output = evidence / 'version.txt'
        with output.open('w') as stream:
            checked_process([str(binary), '--version'], evidence, evidence,
                            'version', timeout, log=stream)
        with output.open('rb') as stream:
            version = stream.read(64)
        if version not in ((VERSION + '\n').encode(), VERSION.encode()):
            raise ToolError(f'worker-build cache version must be exactly {VERSION}')
    return binary


def ensure(root):
    root = cache_root(root)
    if root.exists():
        # A present but invalid exact cache is an error, not a cache miss. Do not
        # hide corruption/version drift by replacing it or by consulting PATH.
        return check(root)
    deadline = time.monotonic() + INSTALL_TIMEOUT_SECONDS
    root.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='.worker-build-install-', dir=root.parent) as temporary:
        evidence = Path(temporary)
        stage = evidence / 'root'
        # Keep the repository's pinned rust-toolchain.toml in effect, just as
        # the original verify.sh cargo install did. Only the install destination
        # is isolated; no Cargo target directory or source configuration changes.
        checked_process(['cargo', 'install', 'worker-build', '--version', VERSION,
                         '--locked', '--root', str(stage)], ROOT, evidence,
                        'install', deadline - time.monotonic())
        check(stage, deadline=deadline)
        if root.exists() or root.is_symlink():
            raise ToolError('worker-build cache appeared during installation')
        # Failed/partial installs remain in the private temporary directory and
        # are never exposed under the cache key or accepted by another stage.
        stage.rename(root)
    return root / 'bin' / 'worker-build'


@lru_cache(maxsize=1)
def load_maintenance():
    spec = importlib.util.spec_from_file_location('worker_cache_maintenance', ROOT / 'scripts' / 'ci-cache-maintenance.py')
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def cache_key(environment, root):
    toolchain = environment.get('RUST_TOOLCHAIN', '')
    pinned = tomllib.loads((ROOT / 'rust-toolchain.toml').read_text())['toolchain']['channel']
    if not re.fullmatch(r'[0-9]+\.[0-9]+\.[0-9]+', toolchain) or toolchain != pinned:
        raise ToolError('worker-build cache requires the fixed Rust toolchain version')
    if (environment.get('RUNNER_OS'), environment.get('RUNNER_ARCH')) != ('Linux', 'X64'):
        raise ToolError('worker-build cache writer requires Linux X64')
    return f'worker-build-cli-v1-Linux-X64-{toolchain}-{VERSION}-{metadata(root)["definition_sha"]}'


def checkout_sha(environment):
    return subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT,
        env=command_environment(environment), text=True, stderr=subprocess.DEVNULL, timeout=10).strip()


def trusted_writer(environment, api, key, root):
    sha, event, run_id = (environment.get(name, '') for name in
                         ('GITHUB_SHA', 'GITHUB_EVENT_NAME', 'GITHUB_RUN_ID'))
    if (environment.get('GITHUB_ACTIONS') != 'true'
            or environment.get('GITHUB_REPOSITORY') != api.repository
            or environment.get('GITHUB_REF') != 'refs/heads/main'
            or environment.get('GITHUB_WORKFLOW_SHA') != sha
            or environment.get('GITHUB_WORKFLOW_REF') != f'{api.repository}/{WRITER_WORKFLOW}@refs/heads/main'
            or environment.get('GITHUB_JOB') != WRITER_JOB
            or event not in ('schedule', 'workflow_dispatch')
            or not re.fullmatch(r'[0-9a-f]{40}', sha)
            or not re.fullmatch(r'[1-9][0-9]*', run_id)
            or not Path(environment.get('GITHUB_WORKSPACE', '')).is_absolute()
            or Path(environment.get('GITHUB_WORKSPACE', '')).resolve() != ROOT
            or key != cache_key(environment, root)):
        raise ToolError('worker-build cache save requires the exact trusted main writer')
    if event == 'workflow_dispatch':
        path = Path(environment.get('GITHUB_EVENT_PATH', ''))
        if path.stat().st_size > 1024 * 1024:
            raise ToolError('worker-build manual event exceeds its bounded contract')
        payload = json.loads(path.read_text())
        if (not isinstance(payload, dict) or not isinstance(payload.get('inputs'), dict)
                or payload['inputs'].get('run_mode') != 'full'):
            raise ToolError('worker-build manual cache save requires run_mode=full')
    if checkout_sha(environment) != sha:
        raise ToolError('worker-build checkout does not match the trusted main source')
    run = api.request('GET', f'/actions/runs/{run_id}')
    if (not isinstance(run, dict) or type(run.get('id')) is not int
            or run['id'] != int(run_id) or run.get('head_sha') != sha
            or run.get('head_branch') != 'main' or run.get('event') != event
            or run.get('path') != WRITER_WORKFLOW or not isinstance(run.get('repository'), dict)
            or run['repository'].get('full_name') != api.repository):
        raise ToolError('REST run identity does not match the worker-build writer')


def cache_snapshot(root):
    root = cache_root(root)
    rows, raw_bytes = [], 0
    for path in chain((root,), root.rglob('*')):
        value = path.lstat()
        relative = path.relative_to(root).as_posix()
        if (path.is_symlink() or not (stat.S_ISDIR(value.st_mode) or stat.S_ISREG(value.st_mode))
                or any(ord(char) < 32 for char in str(path)) or len(os.fsencode(path)) > 4096
                or path.name in ('cache.tzst', 'cache.tgz')):
            raise ToolError('unsafe worker-build cache payload')
        if stat.S_ISREG(value.st_mode):
            if value.st_nlink != 1:
                raise ToolError('worker-build cache payload must not use hardlinks')
            raw_bytes += value.st_size
        rows.append((relative, value.st_dev, value.st_ino, value.st_mode, value.st_size,
                     value.st_mtime_ns, value.st_ctime_ns))
        if len(rows) > MAX_CACHE_ENTRIES or raw_bytes > MAX_CACHE_BYTES:
            raise ToolError('worker-build cache exceeds the small-tool payload budget')
    return tuple(sorted(rows)), raw_bytes


def upload_bound(root, environment, deadline):
    """Measure the pinned Linux cache action's exact one-directory tar input.

    actions/cache@55cc8345 resolves this literal root with implicitDescendants
    disabled. The GNU tar and compression contract is the same one audited by
    ci-cache-archive.py:upload_bound. Each entry gets an extra complete tar
    record for possible timestamp/PAX growth before save; no compression ratio
    is assumed. Only the isolated root is an action input, never the checkout.
    """
    if (environment.get('TAR_OPTIONS') or environment.get('GZIP')
            or any(char in str(root) for char in '*?[]!{}')):
        raise ToolError('custom archive options or glob paths cannot authorize a tool cache save')
    before, raw_bytes = cache_snapshot(root)
    for command, marker in ((['tar', '--version'], 'GNU tar'), (['gzip', '--version'], 'Free Software Foundation')):
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            raise ToolError('worker-build save admission deadline expired')
        output = subprocess.check_output(command, env=command_environment(environment),
                                         text=True, stderr=subprocess.DEVNULL, timeout=min(5, remaining))
        if marker not in output:
            raise ToolError('worker-build upload sizing requires GNU tar and GNU gzip')
    relative = os.path.relpath(root, ROOT)
    if relative.startswith('-'):
        raise ToolError('worker-build upload path is outside the fixed archive contract')
    with tempfile.TemporaryDirectory(prefix='worker-build-size-') as temporary:
        evidence = Path(temporary)
        manifest = evidence / 'manifest.txt'
        manifest.write_text(relative + '\n')
        archive = evidence / 'cache.tar'
        checked_process(['tar', '--posix', '-cf', str(archive), '--exclude', 'cache.tzst',
                         '-P', '-C', str(ROOT), '--files-from', str(manifest)],
                        evidence, evidence, 'cache-size', min(30, deadline - time.monotonic()))
        size = archive.stat().st_size
    if cache_snapshot(root)[0] != before:
        raise ToolError('worker-build payload changed during upload sizing')
    bounded_tar = size + tarfile.RECORDSIZE * (len(before) + 1)
    zstd = bounded_tar + (bounded_tar >> 8) + max(0, ((128 << 10) - bounded_tar) >> 11)
    gzip_bound = bounded_tar + 5 * ((bounded_tar + 16_383) // 16_384 + 1) + 18
    return dict(raw_bytes=raw_bytes, payload_entries=len(before), measured_tar_bytes=size,
                upload_bound_bytes=max(zstd, gzip_bound)), before


def admit_save(root, key, environment, api=None):
    report = dict(save=False, mode='worker-build-cache-admission', saved_confirmed=None,
                  reason='tool cache save not authorized')
    deadline = time.monotonic() + SAVE_TIMEOUT_SECONDS
    try:
        root = cache_root(root)
        maintenance = load_maintenance()
        policy = maintenance.policy
        if api is None:
            if not environment.get('GH_TOKEN'):
                raise ToolError('read-only Actions token missing for worker-build cache admission')
            class ReadOnlyApi(maintenance.GitHubApi):
                def request(self, method, path):
                    if method != 'GET':
                        raise ToolError('worker-build cache admission is read-only')
                    return super().request(method, path)
            api = ReadOnlyApi(environment.get('GITHUB_REPOSITORY'), environment['GH_TOKEN'])
        if hasattr(api, 'deadline'):
            api.deadline = min(api.deadline, deadline)
        trusted_writer(environment, api, key, root)
        check(root, deadline=deadline)
        measurement, before = upload_bound(root, environment, deadline)
        report.update(measurement)
        budget = policy.configured_budget(environment.get('CI_CACHE_REPOSITORY_BUDGET_BYTES'))
        if budget is None:
            raise ToolError('verified worker-build repository cache budget is missing')
        entries = policy.capacity_context(maintenance, api, budget, report)
        if cache_snapshot(root)[0] != before:
            raise ToolError('worker-build payload changed during capacity verification')
        if time.monotonic() >= deadline:
            raise ToolError('worker-build save admission deadline expired')
        projected = report['observed_repository_bytes'] + report['upload_bound_bytes']
        reserve = policy.DEPENDENCY_ROTATION_HEADROOM_BYTES
        report.update(projected_repository_bytes=projected, reserved_repository_bytes=reserve,
                      projected_repository_bytes_with_reserve=projected + reserve)
        if any(entry['key'] == key and entry['ref'] == 'refs/heads/main' for entry in entries):
            report['reason'] = 'existing immutable key'
        elif projected + reserve > report['effective_repository_budget_bytes']:
            report['reason'] = 'repository cache capacity or dependency rotation gap'
        else:
            report.update(save=True, reason='measured tool upload fits repository budget with rotation headroom')
    except (ValueError, OSError, subprocess.SubprocessError, KeyError, TypeError, AttributeError) as error:
        report['reason'] = str(error) if isinstance(error, ValueError) else 'tool save evidence could not be read'
    return report


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('command', choices=('metadata', 'check', 'ensure', 'admit-save'))
    parser.add_argument('--root', type=Path)
    parser.add_argument('--print-bin', action='store_true')
    parser.add_argument('--github-output', type=Path)
    parser.add_argument('--key')
    args = parser.parse_args(argv)
    if (args.command in ('metadata', 'admit-save') and args.print_bin
            or args.command in ('check', 'ensure') and args.github_output
            or args.command == 'admit-save' and not args.key
            or args.command != 'admit-save' and args.key):
        parser.error('--github-output is for metadata/admit-save; --print-bin for check/ensure; --key for admit-save')
    if args.command == 'admit-save':
        report = admit_save(args.root, args.key, os.environ)
        if args.github_output:
            try:
                with args.github_output.open('a') as stream:
                    stream.write('save=' + str(report['save']).lower() + '\n')
                    if 'upload_bound_bytes' in report:
                        stream.write(f'upload_bound_bytes={report["upload_bound_bytes"]}\n')
            except OSError:
                report.update(save=False, reason='tool save admission output could not be written')
                print(json.dumps(report, sort_keys=True))
                return 1
        print(json.dumps(report, sort_keys=True))
        return 0
    try:
        root = cache_root(args.root)
        if args.command == 'metadata':
            values = metadata(root)
            if args.github_output:
                with args.github_output.open('a') as stream:
                    stream.writelines(f'{key}={value}\n' for key, value in values.items())
            else:
                print(json.dumps(values, sort_keys=True))
        else:
            binary = check(root) if args.command == 'check' else ensure(root)
            if args.print_bin:
                print(binary)
    except (ToolError, OSError) as error:
        print(f'error: {error}', file=sys.stderr)
        return getattr(error, 'code', 1)
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
