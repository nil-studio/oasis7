#!/usr/bin/env python3
"""Run portable-cache tools in pinned images while host cache actions retain their ABI."""
import argparse
import os
from pathlib import Path
import subprocess
import sys
import uuid

IMAGES = {
    'browser': 'ghcr.io/nil-studio/oasis7/ci-runtime@sha256:3984caa42d82963f93af752bf79e3a07838f2c94279cfbb2deeb08f715cd62c9',
    'worker': 'ghcr.io/nil-studio/oasis7/ci-rust-build@sha256:0b13d0b5ba9051617eb6ab77a5af324f066af79bb2753d664b780535c2fe13a3',
}
COMMANDS = {'browser': {'metadata', 'fetch', 'admit-save'},
            'worker': {'metadata', 'check', 'ensure', 'admit-save'}}
GITHUB_ENVIRONMENT = {
    'GITHUB_ACTIONS', 'GITHUB_API_URL', 'GITHUB_REPOSITORY', 'GITHUB_REPOSITORY_OWNER',
    'GITHUB_REF', 'GITHUB_SHA', 'GITHUB_WORKFLOW_SHA', 'GITHUB_RUN_ID',
    'GITHUB_RUN_ATTEMPT', 'GITHUB_EVENT_NAME', 'GITHUB_WORKFLOW', 'GITHUB_JOB',
    'GITHUB_OUTPUT', 'GITHUB_SERVER_URL', 'GITHUB_WORKFLOW_REF', 'GITHUB_EVENT_PATH',
}


CONTAINER_ENTRY = """import json, os, pathlib, runpy, subprocess, sys
root = pathlib.Path.cwd()
info = root.stat()
assert (os.getuid(), os.getgid()) == (1001, 1001), 'portable cache UID mismatch'
assert str(root) == os.environ['GITHUB_WORKSPACE'], 'portable cache cwd mismatch'
assert (info.st_dev, info.st_ino) == (int(os.environ['OASIS7_PORTABLE_DEVICE']), int(os.environ['OASIS7_PORTABLE_INODE'])), 'portable cache bind mismatch'
source = subprocess.check_output(['git', '-c', 'safe.directory=' + str(root), 'rev-parse', 'HEAD'], text=True).strip()
assert source == os.environ['GITHUB_SHA'], 'portable cache source mismatch'
assert sys.version_info[:2] == (3, 12), 'portable cache Python mismatch'
print('PORTABLE_CACHE_CONTAINER=' + json.dumps(dict(image=os.environ['OASIS7_PORTABLE_IMAGE'], uid=os.getuid(), gid=os.getgid(), cwd=str(root), source=source, device=info.st_dev, inode=info.st_ino)), file=sys.stderr)
sys.argv = sys.argv[1:]
sys.path.insert(0, str((root / sys.argv[0]).parent))
runpy.run_path(sys.argv[0], run_name='__main__')
"""


def directory(value):
    path = Path(value)
    if (not path.is_absolute() or '..' in path.parts
            or any(ord(c) < 32 for c in str(path))
            or any(p.is_symlink() for p in (path, *path.parents))):
        raise ValueError('unsafe portable-cache directory')
    return path


def command(kind, arguments, env, *, uid, gid, nonce):
    if (uid, gid) != (1001, 1001):
        raise ValueError('portable cache tools require the original hosted UID')
    if not arguments or arguments[0] not in COMMANDS[kind] or any(a in ('--root', '--cache-base') or a.startswith(('--root=', '--cache-base=')) for a in arguments):
        raise ValueError('unsupported portable-cache tool command')
    if any(env.get(k) for k in ('DOCKER_HOST', 'DOCKER_CONTEXT', 'DOCKER_TLS_VERIFY', 'DOCKER_CERT_PATH')):
        raise ValueError('portable cache tools require the original local Docker daemon')
    workspace = directory(env['GITHUB_WORKSPACE'])
    temporary = directory(env['RUNNER_TEMP'])
    if str(workspace) != '/home/runner/work/oasis7/oasis7' or str(temporary) != '/home/runner/work/_temp':
        raise ValueError('portable cache actions require the original hosted paths')
    if env.get('RUNNER_OS') != 'Linux' or env.get('RUNNER_ARCH') != 'X64':
        raise ValueError('portable cache actions require the original Linux/X64 ABI')
    mounts = [(workspace, False), (temporary, False)]
    home = directory(temporary / ('portable-cache-' + kind + '-home'))
    home.mkdir(mode=0o700, exist_ok=True)
    values = {k: v for k, v in env.items()
              if k in GITHUB_ENVIRONMENT or k in {'RUNNER_OS', 'RUNNER_ARCH', 'RUNNER_TEMP',
                 'CI_CACHE_REPOSITORY_BUDGET_BYTES', 'RUST_TOOLCHAIN', 'TAR_OPTIONS', 'GZIP', 'ZSTD_CLEVEL', 'ZSTD_NBTHREADS'}
              or k.startswith(('CARGO_', 'OASIS7_WASM', 'CC_', 'CXX_', 'AR_', 'PKG_CONFIG'))
              or k in {'CC', 'CXX', 'AR', 'CFLAGS', 'CXXFLAGS', 'CPPFLAGS', 'LDFLAGS', 'LIBRARY_PATH', 'CPATH'}}
    # Cargo and tools never receive the host action runtime token or input environment.
    values.update(HOME=str(home), GITHUB_WORKSPACE=str(workspace), RUNNER_TEMP=str(temporary),
                  PYTHONDONTWRITEBYTECODE='1')
    if arguments[0] == 'admit-save':
        if not env.get('GH_TOKEN'):
            raise ValueError('cache admission requires its original read-only API token')
        values['GH_TOKEN'] = env['GH_TOKEN']
    if kind == 'worker':
        cache_parent = directory('/home/runner/.cache/oasis7/worker-build')
        cache_parent.mkdir(mode=0o700, parents=True, exist_ok=True)
        mounts.append((cache_parent, False))
        mold = directory('/opt/oasis7-mold/3.0.0-v1')
        if env.get('CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER') != str(mold / 'bin/oasis7-mold-gcc'):
            raise ValueError('the original verified mold linker is required')
        mounts.append((mold, True))
        values['CARGO_HOME'] = str(home / '.cargo')
        arguments = [arguments[0], '--root', str(cache_parent / '0.7.5'), *arguments[1:]]
    image = IMAGES[kind]
    info = workspace.stat()
    values.update(OASIS7_PORTABLE_DEVICE=str(info.st_dev), OASIS7_PORTABLE_INODE=str(info.st_ino),
                  OASIS7_PORTABLE_IMAGE=image)
    name = 'oasis7-portable-cache-' + nonce
    argv = ['docker', 'run', '--rm', '--init', '--pull=never', '--name', name,
            '--user', '1001:1001', '--workdir', str(workspace)]
    for path, readonly in mounts:
        argv += ['--mount', 'type=bind,source=' + str(path) + ',target=' + str(path) + (',readonly' if readonly else '')]
    for key in sorted(values):
        argv += ['--env', key]
    argv += [image, 'python3', '-c', CONTAINER_ENTRY, 'scripts/ci-browser-deps.py' if kind == 'browser' else 'scripts/ci-worker-build.py', *arguments]
    # Docker CLI also receives no Actions bearer credentials. Only selected container
    # variables and the ordinary CLI PATH survive; values stay out of argv/logs.
    docker_env = dict(values, PATH=env.get('PATH', os.defpath))
    return argv, docker_env, name


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--kind', choices=tuple(IMAGES), required=True)
    parser.add_argument('arguments', nargs=argparse.REMAINDER)
    args = parser.parse_args()
    arguments = args.arguments[1:] if args.arguments[:1] == ['--'] else args.arguments
    try:
        argv, env, name = command(args.kind, arguments, os.environ,
                                 uid=os.getuid(), gid=os.getgid(), nonce=uuid.uuid4().hex)
    except (ValueError, KeyError, OSError) as error:
        print('Portable cache environment rejected: ' + str(error), file=sys.stderr)
        return 2
    try:
        return subprocess.run(argv, env=env, check=False).returncode
    finally:
        # A foreground tool failure or interrupted invocation only removes its own
        # uniquely named container. Never prune the daemon or another job's cache.
        try:
            subprocess.run(['docker', 'rm', '--force', name], env=env,
                           stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=False, timeout=10)
        except (OSError, subprocess.TimeoutExpired):
            print('Portable cache container cleanup could not be confirmed', file=sys.stderr)


if __name__ == '__main__':
    raise SystemExit(main())
