#!/usr/bin/env python3
"""Verify the trusted full-regression image against its original Provider entry."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import stat
import subprocess
import tempfile

IMAGE = ('ghcr.io/nil-studio/oasis7/ci-rust-full-regression'
         '@sha256:32f8b8cc2b07729362df6f1bceba09ff4ac1659e4932e41d88a06b3bc5d8d427')
WORKSPACE = Path('/home/runner/work/oasis7/oasis7')
RUNNER_TEMP = Path('/home/runner/work/_temp')
SOCKET = Path('/var/run/docker.sock')
REPOSITORY = 'nil-studio/oasis7'
PROBE_BASE = 'codex/engineering-full-regression-provider-base'
PROBE_HEAD = 'codex/engineering-full-regression-provider-proof'


def validate_context(source, controller, environment):
    if (environment.get('GITHUB_REPOSITORY') != REPOSITORY
            or environment.get('GITHUB_ACTIONS') != 'true'
            or environment.get('RUNNER_ENVIRONMENT') != 'github-hosted'
            or environment.get('RUNNER_OS') != 'Linux'
            or environment.get('RUNNER_ARCH') != 'X64'
            or environment.get('GITHUB_WORKSPACE') != str(WORKSPACE)
            or environment.get('RUNNER_TEMP') != str(RUNNER_TEMP)
            or not re.fullmatch('[0-9a-f]{40}', source)
            or not re.fullmatch('[0-9a-f]{40}', controller)
            or environment.get('GITHUB_SHA') != controller
            or environment.get('GITHUB_EVENT_NAME') != 'pull_request'
            or environment.get('GITHUB_BASE_REF') != PROBE_BASE
            or environment.get('GITHUB_HEAD_REF') != PROBE_HEAD):
        raise ValueError('unexpected hosted source or runner context')
    if any(environment.get(name) for name in
           ('DOCKER_HOST', 'DOCKER_CONTEXT', 'DOCKER_TLS_VERIFY', 'DOCKER_CERT_PATH')):
        raise ValueError('full regression requires the original local Docker daemon')
    if (os.getuid(), os.getgid()) != (1001, 1001):
        raise ValueError('unexpected hosted runner user')
    for directory in (WORKSPACE, RUNNER_TEMP):
        if directory.resolve(strict=True) != directory or not directory.is_dir():
            raise ValueError('unexpected canonical directory')
    if subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=WORKSPACE, text=True).strip() != source:
        raise ValueError('source checkout differs from the frozen source')
    if subprocess.check_output(['git', 'status', '--porcelain'], cwd=WORKSPACE, text=True).strip():
        raise ValueError('the Provider witness requires a clean source tree')
    event_path = Path(environment['GITHUB_EVENT_PATH'])
    if event_path.stat().st_size > 1024 * 1024:
        raise ValueError('oversized Provider probe event')
    request = json.loads(event_path.read_text())
    pull = request['pull_request']
    if (request['action'] != 'opened' or pull['draft'] is not True
            or pull['head']['repo']['full_name'] != REPOSITORY
            or pull['base']['repo']['full_name'] != REPOSITORY
            or pull['head']['ref'] != PROBE_HEAD or pull['base']['ref'] != PROBE_BASE):
        raise ValueError('Provider probe must use the owned diagnostic pull request')
    socket = SOCKET.lstat()
    if not stat.S_ISSOCK(socket.st_mode):
        raise ValueError('the original Docker socket is unavailable')
    return socket.st_gid


def provider_command(source, workspace, scratch, socket_group, launcher):
    args = ['docker', 'run', '--rm', '--init', '--pull=never', '--platform', 'linux/amd64',
            '--user', '1001:1001', '--group-add', str(socket_group), '--workdir', str(workspace)]
    for path, readonly in ((workspace, False), (scratch, False), (SOCKET, False), (launcher, True)):
        args += ['--mount', f'type=bind,source={path},target={path}' + (',readonly' if readonly else '')]
    identity = workspace.stat()
    for variable, value in {'PROBE_WORKSPACE_DEV': str(identity.st_dev),
                            'PROBE_WORKSPACE_INODE': str(identity.st_ino),
                            'SOURCE_SHA': source, 'HOME': str(scratch),
                            'RUNNER_TEMP': str(scratch), 'CARGO_HOME': str(scratch / 'cargo')}.items():
        args += ['--env', f'{variable}={value}']
    args += [IMAGE, 'python3', str(launcher), '_provider']
    return args


def provider():
    source = os.environ['SOURCE_SHA']
    scratch = Path(os.environ['RUNNER_TEMP'])
    if (os.getuid(), os.getgid()) != (1001, 1001) or Path.cwd() != WORKSPACE:
        raise ValueError('unexpected image user or build directory')
    identity = WORKSPACE.stat()
    if (identity.st_dev, identity.st_ino) != (int(os.environ['PROBE_WORKSPACE_DEV']),
                                             int(os.environ['PROBE_WORKSPACE_INODE'])):
        raise ValueError('unexpected workspace bind identity')
    if subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip() != source:
        raise ValueError('image checkout differs from the frozen source')
    subprocess.run(['python3', '.github/actions/setup-linux-mold/setup.py',
                    '--install-dir', str(scratch / 'mold')], check=True)
    os.environ['CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER'] = str(scratch / 'mold/bin/oasis7-mold-gcc')
    os.environ['PATH'] = '/opt/oasis7-node22/bin:' + os.environ['PATH']
    subprocess.run(['bash', '-euo', 'pipefail', '-c',
                    'node --version; npm --version; rustc --version; '
                    'clang --print-targets | grep -w wasm32; llvm-ar --version; '
                    'docker buildx version; docker info --format "{{.ServerVersion}}"; '
                    'pkg-config --exists wayland-client xkbcommon x11 xi xcursor xrandr xinerama alsa libudev'],
                   check=True)
    (WORKSPACE / 'target').mkdir(exist_ok=True)
    with tempfile.TemporaryDirectory(dir=WORKSPACE / 'target', prefix='full-provider-') as owned:
        bundle = Path(owned)
        subprocess.run(['python3', 'scripts/ci-test-artifacts.py', 'prepare-provider',
                        '--repo-root', str(WORKSPACE), '--bundle', str(bundle),
                        '--output', str(bundle / 'environment.sh')], check=True)
        record = json.loads((bundle / 'preview-provider.json').read_text())
        if record.get('source_sha') != source or record.get('tested_sha') != source:
            raise ValueError('Provider receipt source differs from the frozen source')
        print(json.dumps({'source': source, 'image': IMAGE, 'provider': record, 'probe_only': True},
                         sort_keys=True), flush=True)


def cache_geometry(npm_root, bindgen_root):
    if npm_root != '/home/runner/.npm':
        raise ValueError('npm reports an unexpected legacy host cache root')
    if not re.fullmatch(r'/home/runner/\.cache/oasis7/wasm-bindgen-cli/[0-9]+\.[0-9]+\.[0-9]+', bindgen_root):
        raise ValueError('wasm-bindgen reports an unexpected legacy host cache root')
    cases = {
        'npm': (Path(npm_root), True),
        'wasm-bindgen': (Path(bindgen_root), True),
        'trunk': (RUNNER_TEMP / 'ci-tools/trunk-x86_64-unknown-linux-gnu.tar.gz', False),
        'sentinel': (RUNNER_TEMP / 'cache-permission-sentinel', True),
    }
    with tempfile.TemporaryDirectory(dir=RUNNER_TEMP, prefix='full-cache-geometry-') as directory:
        temporary = Path(directory)
        for name, (original, is_directory) in cases.items():
            member = os.path.relpath(original, WORKSPACE)
            writer, reader = temporary / 'writer', temporary / 'reader'
            writer_workspace = writer / WORKSPACE.relative_to('/')
            reader_workspace = reader / WORKSPACE.relative_to('/')
            writer_workspace.mkdir(parents=True, exist_ok=True)
            reader_workspace.mkdir(parents=True, exist_ok=True)
            relative_payload = original.relative_to('/') / 'fixture' if is_directory else original.relative_to('/')
            payload = writer / relative_payload
            payload.parent.mkdir(parents=True, exist_ok=True)
            payload.write_bytes(name.encode())
            archive = temporary / (name + '.tar.zst')
            members = temporary / (name + '.members')
            members.write_text(member + '\n')
            subprocess.run(['tar', '--posix', '--zstd', '-cf', str(archive), '-P',
                            '-C', str(writer_workspace), '--files-from', str(members)], check=True)
            subprocess.run(['tar', '--zstd', '-xf', str(archive), '-P', '-C', str(reader_workspace)], check=True)
            if (reader / relative_payload).read_bytes() != payload.read_bytes():
                raise ValueError('host archive fixture bytes handoff failed')
            # Original SDK path/version inputs. This diagnostic does not call
            # the GitHub cache API and does not perform a shared-cache save.
            version_inputs = [str(original), 'zstd-without-long', '1.0']
            print(json.dumps({'probe_only': True, 'role': name, 'path': str(original), 'member': member,
                              'version_inputs': version_inputs,
                              'version': hashlib.sha256('|'.join(version_inputs).encode()).hexdigest()},
                             sort_keys=True), flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('phase', choices=('verify-provider', 'verify-cache-geometry', '_provider'))
    parser.add_argument('--source')
    parser.add_argument('--controller')
    parser.add_argument('--npm-root')
    parser.add_argument('--bindgen-root')
    arguments = parser.parse_args()
    if arguments.phase == '_provider':
        provider()
        return 0
    if not arguments.source or not arguments.controller:
        parser.error('verify-provider requires source and controller hashes')
    group = validate_context(arguments.source, arguments.controller, os.environ)
    if arguments.phase == 'verify-cache-geometry':
        if not arguments.npm_root or not arguments.bindgen_root:
            parser.error('verify-cache-geometry requires original npm and bindgen roots')
        cache_geometry(arguments.npm_root, arguments.bindgen_root)
        return 0
    launcher = Path(__file__).resolve(strict=True)
    if launcher.parent != RUNNER_TEMP:
        raise ValueError('probe launcher must be staged in the runner temporary directory')
    with tempfile.TemporaryDirectory(dir=RUNNER_TEMP, prefix='full-image-probe-') as temporary:
        # The host has completed its registry pull. The client subprocess
        # receives no job credentials, GitHub cache token, or proxy state.
        return subprocess.run(provider_command(arguments.source, WORKSPACE, Path(temporary), group, launcher),
                              env={'PATH': os.environ['PATH']}, check=False).returncode


if __name__ == '__main__':
    raise SystemExit(main())
