#!/usr/bin/env python3
"""Run original trusted full-regression commands in the published tools image."""
import argparse
import json
import os
from pathlib import Path
import re
import shutil
import stat
import subprocess
import tempfile
import uuid

IMAGE = ('ghcr.io/nil-studio/oasis7/ci-rust-full-regression'
         '@sha256:32f8b8cc2b07729362df6f1bceba09ff4ac1659e4932e41d88a06b3bc5d8d427')
ROOT = Path('/home/runner/work/oasis7/oasis7')
TEMP = Path('/home/runner/work/_temp')
SOCKET = Path('/var/run/docker.sock')
MOLD = Path('/opt/oasis7-mold/3.0.0-v1')
HOST_CARGO = Path('/home/runner/.cargo')
REPOSITORY = 'nil-studio/oasis7'
WORKFLOW = f'{REPOSITORY}/.github/workflows/rust.yml@refs/heads/main'
CARGO_POLICY = (
    'CARGO_INCREMENTAL', 'CARGO_PROFILE_DEV_DEBUG', 'CARGO_PROFILE_TEST_DEBUG',
    'CARGO_TERM_COLOR', 'CARGO_NET_RETRY', 'CARGO_REGISTRIES_CRATES_IO_PROTOCOL',
)


def trusted_main(env, source):
    if (not re.fullmatch('[0-9a-f]{40}', source)
            or any(env.get(k) != v for k, v in {
                'GITHUB_REPOSITORY': REPOSITORY, 'GITHUB_REF': 'refs/heads/main',
                'GITHUB_WORKFLOW_REF': WORKFLOW, 'GITHUB_SHA': source,
                'GITHUB_WORKFLOW_SHA': source, 'GITHUB_JOB': 'full-regression',
                'GITHUB_ACTIONS': 'true', 'RUNNER_ENVIRONMENT': 'github-hosted',
                'RUNNER_OS': 'Linux', 'RUNNER_ARCH': 'X64',
                'GITHUB_WORKSPACE': str(ROOT), 'RUNNER_TEMP': str(TEMP),
            }.items())
            or any(env.get(k) for k in ('DOCKER_HOST', 'DOCKER_CONTEXT', 'DOCKER_TLS_VERIFY', 'DOCKER_CERT_PATH'))
            or (os.getuid(), os.getgid()) != (1001, 1001)):
        raise ValueError('trusted full regression requires the exact hosted main source and local Docker')
    event = env.get('GITHUB_EVENT_NAME')
    if event == 'workflow_dispatch':
        path = Path(env['GITHUB_EVENT_PATH'])
        if path.resolve(strict=True) != path or not path.is_file() or path.stat().st_size > 1024 * 1024:
            raise ValueError('invalid trusted full dispatch receipt')
        if json.loads(path.read_text()).get('inputs', {}).get('run_mode') != 'full':
            raise ValueError('trusted full dispatch requires full mode')
    elif event != 'schedule':
        raise ValueError('unexpected trusted full trigger')


def canonical(path, *, directory=False):
    if path.resolve(strict=True) != path or not (path.is_dir() if directory else path.is_file()):
        raise ValueError(f'unexpected full regression bind: {path}')
    return path


def bind(argv, path, *, target=None, readonly=False):
    if any(character in str(path) for character in ',\n\r'):
        raise ValueError('invalid full regression mount path')
    argv.extend(('--mount', f'type=bind,source={path},target={target or path}' + (',readonly' if readonly else '')))


def head(root):
    return subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, text=True).strip()


def image_command(phase, source, scratch, name, env):
    root = canonical(ROOT, directory=True)
    if head(root) != source:
        raise ValueError('full regression checkout differs from the trusted source')
    identity = root.stat()
    argv = ['docker', 'run', '--rm', '--init', '--pull=never', '--platform', 'linux/amd64',
            '--user', '1001:1001', '--name', name, '--workdir', str(root)]
    bind(argv, root)
    bind(argv, scratch)
    values = {
        'HOME': str(scratch), 'RUNNER_TEMP': str(scratch), 'TMPDIR': str(scratch),
        'CARGO_HOME': str(scratch / 'cargo'), 'SOURCE_SHA': source, 'CI': 'true',
        'GITHUB_WORKSPACE': str(root), 'RUNNER_OS': 'Linux', 'RUNNER_ARCH': 'X64',
        'OASIS7_FULL_WORKSPACE_DEVICE': str(identity.st_dev), 'OASIS7_FULL_WORKSPACE_INODE': str(identity.st_ino),
    }
    for key in ('OASIS7_PRODUCT_DOC_BASE', 'OASIS7_PRODUCT_DOC_HEAD'):
        if not re.fullmatch('[0-9a-f]{40}', env.get(key, '')):
            raise ValueError(f'invalid product document revision: {key}')
        values[key] = env[key]
    if phase == 'npm':
        npm = Path('/home/runner/.npm')
        if subprocess.check_output(['npm', 'config', 'get', 'cache'], text=True).strip() != str(npm):
            raise ValueError('unexpected host npm cache root')
        npm.mkdir(exist_ok=True)
        bind(argv, canonical(npm, directory=True))
        values['npm_config_cache'] = str(npm)
    if phase in ('full', 'governance'):
        values.update({key: env[key] for key in CARGO_POLICY if key in env})
        cargo_home = scratch / 'cargo'
        cargo_home.mkdir(mode=0o700)
        for subpath in ('registry', 'git'):
            cache = HOST_CARGO / subpath
            cache.mkdir(parents=True, exist_ok=True)
            target = cargo_home / subpath
            target.mkdir(mode=0o700)
            bind(argv, canonical(cache, directory=True), target=target)
        deny = shutil.which('cargo-deny')
        if deny:
            executable = Path(deny).resolve(strict=True)
            if executable.name != 'cargo-deny' or not executable.is_relative_to(Path('/home/runner')):
                raise ValueError('unexpected cargo-deny installation path')
            subprocess.run([str(executable), '--version'], capture_output=True, text=True, check=True)
            (scratch / 'tools').mkdir()
            shutil.copy2(executable, scratch / 'tools/cargo-deny')
        elif phase == 'full':
            raise ValueError('original pinned cargo-deny is unavailable')
        bind(argv, canonical(MOLD, directory=True), readonly=True)
        values['CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER'] = str(MOLD / 'bin/oasis7-mold-gcc')
    if phase == 'full':
        socket = SOCKET.lstat()
        if not stat.S_ISSOCK(socket.st_mode):
            raise ValueError('missing original Docker socket')
        bind(argv, SOCKET)
        argv += ['--group-add', str(socket.st_gid)]
        bindgen = Path(env['WASM_BINDGEN_BIN'])
        if not re.fullmatch(r'/home/runner/\.cache/oasis7/wasm-bindgen-cli/[0-9]+\.[0-9]+\.[0-9]+/bin/wasm-bindgen', str(bindgen)):
            raise ValueError('unexpected wasm-bindgen cache root')
        bind(argv, canonical(bindgen), readonly=True)
        values['WASM_BINDGEN_BIN'] = str(bindgen)
        bind(argv, canonical(TEMP / 'ci-tools/bin/trunk'), readonly=True)
    for k, v in values.items():
        argv.extend(('--env', f'{k}={v}'))
    return [*argv, IMAGE, 'python3', 'scripts/ci-trusted-full-regression.py', '_inside', phase]


def inside(phase):
    source = os.environ['SOURCE_SHA']
    identity = ROOT.stat()
    if ((os.getuid(), os.getgid()) != (1001, 1001) or Path.cwd() != ROOT or head(ROOT) != source
            or (identity.st_dev, identity.st_ino) != (int(os.environ['OASIS7_FULL_WORKSPACE_DEVICE']),
                                                     int(os.environ['OASIS7_FULL_WORKSPACE_INODE']))):
        raise ValueError('image source or workspace bind identity mismatch')
    env = os.environ.copy()
    env['PATH'] = f'/opt/oasis7-node22/bin:{Path(env["RUNNER_TEMP"]) / "tools"}:{TEMP / "ci-tools/bin"}:' + env['PATH']
    if phase == 'npm':
        return subprocess.run(['npm', 'ci', '--prefix', 'crates/oasis7_viewer'], env=env, check=False).returncode
    if phase == 'full':
        subprocess.run(['bash', '-euo', 'pipefail', '-c',
                        'test "$(cargo-deny --version)" = "cargo-deny 0.19.9"; '
                        'node --version; npm --version; rustc --version; '
                        'test "$(trunk --version)" = "trunk 0.21.14"; '
                        'clang --print-targets | grep -w wasm32; llvm-ar --version; '
                        'docker info --format "{{.ServerVersion}}"; docker buildx version; '
                        'pkg-config --exists wayland-client xkbcommon x11 xi xcursor xrandr xinerama alsa libudev'],
                       env=env, check=True)
        venv = Path(env['RUNNER_TEMP']) / 'doc-venv'
        subprocess.run(['python3', '-m', 'venv', str(venv)], env=env, check=True)
        subprocess.run([str(venv / 'bin/python'), '-m', 'pip', 'install', '--disable-pip-version-check',
                        '--requirement', 'scripts/doc-governance-requirements.txt'], env=env, check=True)
        env['PATH'] = str(venv / 'bin') + ':' + env['PATH']
        env['CI_VERBOSE'] = '1'
        return subprocess.run(['./scripts/ci-tests.sh', 'full-extended'], env=env, check=False).returncode
    return subprocess.run(['./scripts/ci-rust-governance-report.sh', '--out-dir', 'output/rust-governance'],
                          env=env, check=False).returncode


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('phase', choices=('npm', 'full', 'governance', '_inside'))
    parser.add_argument('argument')
    args = parser.parse_args()
    if args.phase == '_inside':
        if args.argument not in ('npm', 'full', 'governance'):
            parser.error('unknown image command')
        return inside(args.argument)
    trusted_main(os.environ, args.argument)
    if args.phase in ('full', 'governance') and any(not os.environ.get(key) for key in CARGO_POLICY):
        raise ValueError('trusted full regression is missing the original Cargo workflow policy')
    canonical(TEMP, directory=True)
    name = 'oasis7-trusted-full-' + uuid.uuid4().hex
    with tempfile.TemporaryDirectory(dir=TEMP, prefix='trusted-full-image-') as temporary:
        env = {'PATH': os.environ['PATH']}
        argv = image_command(args.phase, args.argument, Path(temporary), name, os.environ)
        try:
            return subprocess.run(argv, env=env, check=False).returncode
        finally:
            try:
                subprocess.run(['docker', 'rm', '--force', name], env=env, check=False, timeout=10,
                               stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            except (OSError, subprocess.TimeoutExpired):
                print('Could not confirm removal of the owned regression container', flush=True)


if __name__ == '__main__':
    raise SystemExit(main())
