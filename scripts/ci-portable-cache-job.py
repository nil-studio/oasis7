#!/usr/bin/env python3
"""Check the finite aliases of the two trusted portable-cache containers."""
import json
import os
from pathlib import Path
import re
import socket
import subprocess
import sys
import tomllib

ROOT = Path('/home/runner/work/oasis7/oasis7')
TEMP = Path('/home/runner/work/_temp')
WORKER = Path('/home/runner/.cache/oasis7/worker-build')
SOCKET = Path('/var/run/docker.sock')


def alias(actual, canonical):
    for value in (Path(actual), canonical):
        if (not value.is_absolute() or value.resolve(strict=True) != value
                or not value.is_dir()):
            raise ValueError('portable cache alias must be a real directory')
    if not os.path.samefile(actual, canonical):
        raise ValueError('portable cache alias differs from the original runner bind')


def verify(kind, env):
    expected = {'browser': 'browser-archive-cache', 'worker': 'worker-build-cache'}
    if (kind not in expected or (os.getuid(), os.getgid()) != (1001, 1001)
            or any(env.get(k) != v for k, v in {
                'GITHUB_ACTIONS': 'true', 'GITHUB_REPOSITORY': 'nil-studio/oasis7',
                'GITHUB_REF': 'refs/heads/main', 'RUNNER_OS': 'Linux', 'RUNNER_ARCH': 'X64',
                'RUNNER_ENVIRONMENT': 'github-hosted',
                'GITHUB_JOB': expected[kind],
                'GITHUB_WORKFLOW_REF': 'nil-studio/oasis7/.github/workflows/rust-cache.yml@refs/heads/main',
            }.items()) or not re.fullmatch('[a-f0-9]{40}', env.get('GITHUB_SHA', ''))
            or env.get('GITHUB_WORKFLOW_SHA') != env['GITHUB_SHA']):
        raise ValueError('portable cache container requires its exact trusted main job')
    check_bindings(kind, env, env['GITHUB_SHA'])


def check_bindings(kind, env, expected_source):
    if kind not in ('browser', 'worker') or sys.platform != 'linux' or (os.getuid(), os.getgid()) != (1001, 1001):
        raise ValueError('portable bindings require Linux UID/GID1001')
    alias(env['GITHUB_WORKSPACE'], ROOT)
    alias(env['RUNNER_TEMP'], TEMP)
    head = subprocess.check_output(['git', '-C', str(ROOT), 'rev-parse', 'HEAD'], text=True).strip()
    alias(Path.cwd(), ROOT)
    if head != expected_source:
        raise ValueError('portable cache source mismatch')
    if sys.version_info[:2] != (3, 12):
        raise ValueError('portable cache requires fixed Python3.12')
    tar_version = subprocess.check_output(['tar', '--version'], text=True)
    if not tar_version.startswith('tar (GNU tar)'):
        raise ValueError('portable cache requires the original GNU tar format')
    print(tar_version.splitlines()[0])
    subprocess.run(['zstd', '--version'], check=True)
    if SOCKET.exists():
        with socket.socket(socket.AF_UNIX) as connection:
            connection.settimeout(1)
            try:
                connection.connect(str(SOCKET))
            except PermissionError:
                pass
            else:
                raise ValueError('portable cache containers must not access the Docker daemon')
    if kind == 'worker':
        pin = tomllib.loads((ROOT / 'rust-toolchain.toml').read_text())['toolchain']['channel']
        if env.get('RUST_TOOLCHAIN') != pin or not re.fullmatch(r'[0-9]+\.[0-9]+\.[0-9]+', pin):
            raise ValueError('Worker requires the original fixed Rust toolchain')
        installed = subprocess.check_output(['rustup', 'toolchain', 'list'], text=True).splitlines()
        if not any(line.split()[0] == pin + '-x86_64-unknown-linux-gnu' for line in installed):
            raise ValueError('Worker fixed toolchain is absent from the image')
        subprocess.run(['rustc', '+' + pin, '-vV'], check=True)
        alias(TEMP, WORKER)
        home = TEMP / 'portable-worker-home'
        homes = {str(home), str(Path(env['RUNNER_TEMP']) / 'portable-worker-home')}
        if env.get('HOME') not in homes or env.get('CARGO_HOME') != str(Path(env['HOME']) / '.cargo'):
            raise ValueError('Worker Cargo home differs from its private runner temporary directory')
        for directory in (home, home / '.cargo'):
            directory.mkdir(mode=0o700, exist_ok=True)
            if directory.is_symlink() or directory.stat().st_uid != os.getuid():
                raise ValueError('Worker temporary home ownership mismatch')
        alias(env['HOME'], home)
        alias(env['CARGO_HOME'], home / '.cargo')
    print(json.dumps({'kind': kind, 'source': head, 'canonical_workspace': str(ROOT),
                      'canonical_temp': str(TEMP), 'observed_cwd': str(Path.cwd()),
                      'uid': os.getuid(), 'gid': os.getgid()}))


if __name__ == '__main__':
    if len(sys.argv) != 2:
        raise SystemExit('one finite portable-cache kind is required')
    verify(sys.argv[1], os.environ)
