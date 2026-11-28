#!/usr/bin/env python3
"""Recover only the existing runner-owned SDK roots for protected full-regression."""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import re
import stat
import subprocess
import sys

sys.dont_write_bytecode = True

ROOT = Path('/home/runner/work/oasis7/oasis7')
TEMP = Path('/home/runner/work/_temp')
WORKFLOW = 'nil-studio/oasis7/.github/workflows/rust.yml@refs/heads/main'
IDENTITY = {
    'GITHUB_ACTIONS': 'true',
    'GITHUB_REPOSITORY': 'nil-studio/oasis7',
    'GITHUB_REF': 'refs/heads/main',
    'GITHUB_WORKFLOW_REF': WORKFLOW,
    'GITHUB_JOB': 'full-regression',
    'RUNNER_ENVIRONMENT': 'github-hosted',
    'RUNNER_OS': 'Linux',
    'RUNNER_ARCH': 'X64',
    'GITHUB_WORKSPACE': str(ROOT),
    'RUNNER_TEMP': str(TEMP),
    'SUDO_UID': '1001',
    'SUDO_GID': '1001',
}
EVENT_LIMIT = 1024 * 1024


def load_capacity():
    source = Path(__file__).with_name('ci-candidate-capacity.py')
    spec = importlib.util.spec_from_file_location('candidate_capacity', source)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def read_dispatch(path):
    if path.resolve(strict=True) != path:
        raise ValueError('unexpected trusted full dispatch receipt path')
    descriptor = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
    with os.fdopen(descriptor, 'rb') as receipt:
        info = os.fstat(receipt.fileno())
        if not stat.S_ISREG(info.st_mode) or info.st_size > EVENT_LIMIT:
            raise ValueError('invalid trusted full dispatch receipt')
        raw = receipt.read(EVENT_LIMIT + 1)
    if len(raw) > EVENT_LIMIT:
        raise ValueError('invalid trusted full dispatch receipt')
    return json.loads(raw).get('inputs', {})


def check_authority(env, actual_head, release, *, platform, uid):
    source = env.get('GITHUB_SHA', '')
    if (platform != 'linux' or uid != 0 or
            not {'ID=ubuntu', 'VERSION_ID="24.04"'}.issubset(release) or
            any(env.get(key) != value for key, value in IDENTITY.items()) or
            not re.fullmatch('[0-9a-f]{40}', source) or
            source != actual_head or source != env.get('GITHUB_WORKFLOW_SHA')):
        raise ValueError('SDK reclamation requires the exact protected hosted full-regression')
    event = env.get('GITHUB_EVENT_NAME')
    if event == 'workflow_dispatch':
        inputs = read_dispatch(Path(env['GITHUB_EVENT_PATH']))
        if inputs.get('run_mode') != 'full' or inputs.get('source_ref', '') != '':
            raise ValueError('trusted full reclamation rejects a candidate source or mode')
    elif event != 'schedule':
        raise ValueError('trusted full reclamation requires schedule or explicit trusted full mode')


def owned_sdk_paths(capacity):
    paths = [Path(value) for value in capacity.ALLOWLIST]
    mountpoints = capacity.mounts(Path('/proc/self/mountinfo').read_text())
    capacity.validate(paths, mountpoints)
    root_device = Path('/').stat().st_dev
    for path in paths:
        if path.exists() and path.stat().st_dev != root_device:
            raise ValueError(f'SDK reclamation cannot free the trusted full filesystem: {path}')
        for component in (*path.parents, path):
            if component.exists():
                info = component.stat()
                if info.st_uid != 0 or info.st_mode & 0o022:
                    raise ValueError(f'SDK reclamation requires a root-owned runner directory: {component}')
    return paths, mountpoints


def positive(value):
    parsed = int(value)
    if parsed <= 0:
        raise argparse.ArgumentTypeError('expected a positive capacity threshold')
    return parsed


def reclaim_owned_sdk(*, apply, min_free_bytes, min_free_inodes):
    if min_free_bytes <= 0 or min_free_inodes <= 0:
        raise ValueError('trusted full SDK reclamation requires positive capacity thresholds')
    capacity = load_capacity()
    capacity.capacity_domains(os.environ)
    paths, mountpoints = owned_sdk_paths(capacity)
    capacity.reclaim(paths, apply=apply, mountpoints=mountpoints, measure_sizes=True)
    after = capacity.snapshot(paths)
    if after['free_bytes'] < min_free_bytes or after['free_inodes'] < min_free_inodes:
        raise ValueError('trusted full regression lacks measured bytes or inodes; full tests cannot start')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--apply', action='store_true')
    parser.add_argument('--min-free-bytes', type=positive, required=True)
    parser.add_argument('--min-free-inodes', type=positive, required=True)
    args = parser.parse_args()
    if not (ROOT.resolve(strict=True) == ROOT and TEMP.resolve(strict=True) == TEMP
            and ROOT.is_dir() and TEMP.is_dir() and Path.cwd() == ROOT):
        raise ValueError('noncanonical trusted full workspace')
    head = subprocess.check_output(
        ['/usr/bin/git', '-c', f'safe.directory={ROOT}', '-C', str(ROOT), 'rev-parse', 'HEAD'],
        env={'PATH': '/usr/bin:/bin', 'HOME': '/nonexistent'}, text=True, timeout=10).strip()
    release = Path('/etc/os-release').read_text().splitlines()
    check_authority(os.environ, head, release, platform=sys.platform, uid=os.geteuid())
    reclaim_owned_sdk(apply=args.apply, min_free_bytes=args.min_free_bytes,
                      min_free_inodes=args.min_free_inodes)


if __name__ == '__main__':
    main()
