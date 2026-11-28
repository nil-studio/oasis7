#!/usr/bin/env python3
"""Read-only disk accounting for the first trusted mixed full-regression source."""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import stat
import subprocess
import sys

sys.dont_write_bytecode = True

SOURCE = '233925bb0499d8d4ad18e5eb6779371ef311ef46'
IMAGE = ('ghcr.io/nil-studio/oasis7/ci-rust-full-regression'
         '@sha256:32f8b8cc2b07729362df6f1bceba09ff4ac1659e4932e41d88a06b3bc5d8d427')
HOME = Path('/home/runner')
WORKSPACE = HOME / 'work/oasis7/oasis7'
TEMP = HOME / 'work/_temp'
STAGES = ('pre-tools', 'pre-pull', 'post-pull', 'post-restore', 'post-provider')
AREAS = {
    'repository-target': WORKSPACE / 'target',
    'cargo-registry': HOME / '.cargo/registry',
    'cargo-git': HOME / '.cargo/git',
    'host-stable': HOME / '.rustup/toolchains/1.96.0-x86_64-unknown-linux-gnu',
    'host-nightly': HOME / '.rustup/toolchains/nightly-2025-12-11-x86_64-unknown-linux-gnu',
    'npm-cache': HOME / '.npm',
    'trunk-archive': TEMP / 'ci-tools/trunk-x86_64-unknown-linux-gnu.tar.gz',
    'bindgen-cache': HOME / '.cache/oasis7/wasm-bindgen-cli/0.2.127',
    'sentinel': TEMP / 'cache-permission-sentinel',
}


def local_command(*command):
    return subprocess.run(command, env={'PATH': '/usr/bin:/bin', 'LC_ALL': 'C', 'HOME': '/nonexistent'},
                          stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
                          text=True, check=False, timeout=20)


def require_source():
    operating_system = Path('/etc/os-release').read_text().splitlines() if sys.platform == 'linux' else []
    if (sys.platform != 'linux' or (os.getuid(), os.getgid()) != (1001, 1001)
            or os.environ.get('GITHUB_ACTIONS') != 'true'
            or os.environ.get('GITHUB_REPOSITORY') != 'nil-studio/oasis7'
            or os.environ.get('RUNNER_ENVIRONMENT') != 'github-hosted'
            or os.environ.get('RUNNER_OS') != 'Linux'
            or 'ID=ubuntu' not in operating_system or 'VERSION_ID="24.04"' not in operating_system
            or os.environ.get('RUNNER_ARCH') != 'X64'
            or Path.cwd() != WORKSPACE or WORKSPACE.resolve(strict=True) != WORKSPACE
            or os.environ.get('GITHUB_WORKSPACE') != str(WORKSPACE)
            or os.environ.get('RUNNER_TEMP') != str(TEMP)
            or TEMP.resolve(strict=True) != TEMP):
        raise ValueError('capacity observation requires the original hosted Ubuntu workspace')
    head = local_command('/usr/bin/git', '-c', f'safe.directory={WORKSPACE}',
                         '-C', str(WORKSPACE), 'rev-parse', 'HEAD')
    if head.returncode or head.stdout.strip() != SOURCE:
        raise ValueError('capacity observation requires the exact historical failure source')


def disk(path):
    filesystem = path.stat()
    counts = os.statvfs(path)
    return {'device': filesystem.st_dev, 'free_bytes': counts.f_bavail * counts.f_frsize,
            'free_inodes': counts.f_favail}


def allocated(path):
    try:
        metadata = path.lstat()
        if not (stat.S_ISREG(metadata.st_mode) or stat.S_ISDIR(metadata.st_mode)) or (
                path.resolve(strict=True) != path):
            return {'state': 'unsupported-path'}
        result = local_command('/usr/bin/du', '-skx', '--', str(path))
        if result.returncode != 0:
            return {'state': 'unavailable'}
        return {'state': 'measured', 'allocated_bytes': int(result.stdout.split()[0]) * 1024}
    except FileNotFoundError:
        return {'state': 'absent'}
    except (OSError, ValueError, IndexError, subprocess.TimeoutExpired):
        return {'state': 'unavailable'}


def docker_receipt():
    socket = Path('/var/run/docker.sock')
    if not socket.is_socket():
        return {'state': 'no-local-socket'}
    try:
        cli = ('/usr/bin/docker', '--host', f'unix://{socket}')
        usage = local_command(*cli, 'system', 'df', '--format', '{{json .}}')
        image = local_command(*cli, 'image', 'inspect', '--format', '{{.Size}}', IMAGE)
        store = local_command(*cli, 'info', '--format', '{{.DockerRootDir}}')
        if usage.returncode != 0 or store.returncode != 0:
            return {'state': 'unavailable'}
        docker_root = Path(store.stdout.strip())
        if not docker_root.is_absolute() or not docker_root.is_dir():
            return {'state': 'unavailable'}
        storage = [
            {key: row[key] for key in ('Type', 'TotalCount', 'Size', 'Reclaimable')}
            for row in map(json.loads, usage.stdout.splitlines())
        ]
        return {'state': 'measured', 'root_fs': disk(docker_root), 'storage': storage,
                'fixed_image_bytes': int(image.stdout) if image.returncode == 0 else None}
    except (OSError, ValueError, KeyError, subprocess.TimeoutExpired):
        return {'state': 'unavailable'}


def sdk_allowlist():
    # Reuse the existing audited runner-owned inventory. This observer never
    # calls reclaim or the candidate environment gate.
    candidate = Path(__file__).with_name('ci-candidate-capacity.py')
    spec = importlib.util.spec_from_file_location('candidate_capacity', candidate)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return [Path(item) for item in module.ALLOWLIST]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('stage', choices=STAGES)
    args = parser.parse_args()
    require_source()
    print(json.dumps({
        'schema': 'oasis7-trusted-full-capacity-proof/v1',
        'historical_source': SOURCE,
        'image': IMAGE,
        'stage': args.stage,
        'filesystem': {label: disk(path) for label, path in
                       {'host': Path('/'), 'workspace': WORKSPACE, 'temp': TEMP}.items()},
        'areas': {label: allocated(path) for label, path in AREAS.items()},
        'runner_sdk': {str(path): allocated(path) for path in sdk_allowlist()},
        'docker': docker_receipt(),
    }, sort_keys=True), flush=True)


if __name__ == '__main__':
    main()
