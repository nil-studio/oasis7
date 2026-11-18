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
import json
import os
from pathlib import Path
import re
import stat
import subprocess
import sys
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
    for name in ('ci-worker-build.py', 'ci-tool-cache.py', 'ci_process_runner.py'):
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
def load_tool_cache():
    spec = importlib.util.spec_from_file_location('worker_tool_cache', ROOT / 'scripts' / 'ci-tool-cache.py')
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


@lru_cache(maxsize=1)
def load_maintenance():
    return load_tool_cache().load_maintenance(ROOT)


def shared_admission_call(name, *args, **kwargs):
    shared = load_tool_cache()
    try:
        return getattr(shared, name)(*args, **kwargs)
    except shared.ToolCacheError as error:
        raise ToolError(str(error), error.code) from None


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
    return shared_admission_call('trusted_writer', environment, api, key, root,
        repository_root=ROOT, expected_key=cache_key, writer_workflow=WRITER_WORKFLOW,
        writer_job=WRITER_JOB, label='worker-build', checkout_reader=checkout_sha)


def cache_snapshot(root):
    return shared_admission_call('cache_snapshot', cache_root(root),
        max_cache_bytes=MAX_CACHE_BYTES, max_entries=MAX_CACHE_ENTRIES, label='worker-build')


def upload_bound(root, environment, deadline):
    return shared_admission_call('upload_bound', cache_root(root), environment, deadline,
        repository_root=ROOT, max_cache_bytes=MAX_CACHE_BYTES, max_entries=MAX_CACHE_ENTRIES,
        label='worker-build', process_runner=checked_process)


def admit_save(root, key, environment, api=None):
    try:
        root = cache_root(root)
    except (ValueError, OSError) as error:
        return dict(save=False, mode='worker-build-cache-admission', saved_confirmed=None,
                    reason=str(error) if isinstance(error, ValueError) else 'tool save evidence could not be read')
    return load_tool_cache().admit_save(root, key, environment, repository_root=ROOT,
        expected_key=cache_key, writer_workflow=WRITER_WORKFLOW, writer_job=WRITER_JOB,
        check_payload=check, max_cache_bytes=MAX_CACHE_BYTES, max_entries=MAX_CACHE_ENTRIES,
        api=api, label='worker-build', timeout_seconds=SAVE_TIMEOUT_SECONDS,
        checkout_reader=checkout_sha, process_runner=checked_process)


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
