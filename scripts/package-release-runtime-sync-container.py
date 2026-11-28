#!/usr/bin/env python3
"""Route verified package sources to the fixed canonical WASM job environment."""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import platform
import re
import subprocess
import tomllib

IMAGE = ('ghcr.io/nil-studio/oasis7/ci-rust-ordinary-builder@sha256:'
         '714b3f428097659c56639b37083269f04a67f1dcbde56cdcef501a7b18d3c7a8')
CHECKOUT = Path('/home/runner/work/oasis7/oasis7')
SHA = re.compile(r'[0-9a-f]{40}')
TOOLCHAIN = dict(channel='1.96.0', components=['rustfmt', 'clippy'],
                 targets=['wasm32-unknown-unknown'])


def git(*args):
    return subprocess.check_output(['git', '--no-pager', *args], timeout=15, stderr=subprocess.DEVNULL)


def regular_blob(source, path):
    rows = git('ls-tree', source, '--', path).decode().splitlines()
    if len(rows) != 1 or rows[0].split('\t')[-1] != path or not rows[0].startswith(('100644 blob ', '100755 blob ')):
        raise ValueError('missing or nonregular package source input: ' + path)
    return git('show', f'{source}:{path}')


def eligible(env):
    if any(env.get(key) != value for key, value in {
        'GITHUB_REPOSITORY': 'nil-studio/oasis7', 'RUNNER_ENVIRONMENT': 'github-hosted',
        'RUNNER_OS': 'Linux', 'RUNNER_ARCH': 'X64',
    }.items()):
        return False
    sha = env.get('GITHUB_SHA', '')
    if not SHA.fullmatch(sha) or env.get('GITHUB_WORKFLOW_SHA') != sha:
        return False
    workflow = env.get('GITHUB_WORKFLOW_REF', '')
    if env.get('GITHUB_EVENT_NAME') == 'workflow_dispatch':
        return env.get('GITHUB_REF') == 'refs/heads/main' and workflow in {
            f'nil-studio/oasis7/.github/workflows/{name}-packages.yml@refs/heads/main'
            for name in ('release', 'mainnet')}
    ref = env.get('GITHUB_REF', '')
    return (env.get('GITHUB_EVENT_NAME') == 'push' and ref.startswith('refs/tags/')
            and workflow == 'nil-studio/oasis7/.github/workflows/release-packages.yml@' + ref)


def compatible(source):
    if not SHA.fullmatch(source):
        return False
    try:
        if git('ls-tree', source, '--', 'rust-toolchain').strip():
            return False
        if tomllib.loads(regular_blob(source, 'rust-toolchain.toml').decode()).get('toolchain') != TOOLCHAIN:
            return False
        for module in (1, 4, 5):
            regular_blob(source, f'scripts/sync-m{module}-builtin-wasm-artifacts.sh')
        return True
    except (OSError, ValueError, UnicodeError, subprocess.SubprocessError):
        return False


def selected(source, env):
    return (eligible(env) and compatible(source)
            and (env.get('GITHUB_EVENT_NAME') != 'push' or source == env.get('GITHUB_SHA')))


def preflight(source):
    if (not selected(source, os.environ)
            or os.environ.get('GITHUB_JOB') != 'release_gate_runtime_sync'):
        raise ValueError('runtime sync requires the trusted hosted package job and fixed image')
    environment_preflight(source)


def environment_preflight(source):
    """Check capabilities; this component check does not grant package authority."""
    if (not compatible(source) or 'OASIS7_WASM_BUILD_IN_CONTAINER' in os.environ
            or (os.getuid(), os.getgid()) != (0, 0)):
        raise ValueError('runtime sync requires the fixed trusted-root compiler environment')
    workspace = Path(os.environ['GITHUB_WORKSPACE'])
    if not workspace.samefile(CHECKOUT) or git('rev-parse', 'HEAD').decode().strip() != source:
        raise ValueError('runtime sync requires the exact physical planned checkout')
    release = platform.freedesktop_os_release()
    if release.get('ID') != 'ubuntu' or release.get('VERSION_ID') != '24.04':
        raise ValueError('runtime sync requires Ubuntu 24.04')
    # Import only the executing workflow's saved helper, never historical product glue.
    helper = Path(__file__).with_name('ci-test-artifacts.py')
    workflow = os.environ['GITHUB_WORKFLOW_SHA']
    for path in (Path(__file__), helper):
        if path.is_symlink() or path.read_bytes() != regular_blob(workflow, 'scripts/' + path.name):
            raise ValueError('runtime sync tools differ from the executing workflow')
    spec = importlib.util.spec_from_file_location('runtime_sync_tools', helper)
    tools = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(tools)
    docker = tools.ordinary_docker_preflight()
    if (docker['client_version'] != '28.5.2' or docker['buildx_version'] != 'v0.29.1'
            or tuple(map(int, docker['server_api'].split('.'))) < (1, 48)):
        raise ValueError('runtime sync requires the pinned Docker clients and compatible server API')
    for command, expected in (
        (['rustc', '--version'], 'rustc 1.96.0 '),
        (['rustup', 'target', 'list', '--toolchain', '1.96.0', '--installed'], 'wasm32-unknown-unknown'),
        (['rustup', 'target', 'list', '--toolchain', 'nightly-2025-12-11', '--installed'], 'wasm32-unknown-unknown'),
        (['rustup', 'component', 'list', '--toolchain', 'nightly-2025-12-11', '--installed'], 'rust-src'),
    ):
        result = subprocess.check_output(command, text=True, timeout=15)
        if expected not in result:
            raise ValueError('runtime sync fixed toolchain is incomplete')
    if any(os.environ.get(key) != value for key, value in {
        'RUST_TOOLCHAIN': '1.96.0', 'OASIS7_WASM_TOOLCHAIN': 'nightly-2025-12-11',
        'OASIS7_WASM_TARGET': 'wasm32-unknown-unknown', 'OASIS7_WASM_BUILD_STD': '1',
        'OASIS7_WASM_BUILD_STD_COMPONENTS': 'std,panic_abort',
    }.items()):
        raise ValueError('runtime sync compiler environment differs from its fixed image')
    startup = Path(os.environ['RUNNER_TEMP']) / 'runtime-sync-cwd.bash'
    with startup.open('x') as handle:
        handle.write('cd -- /home/runner/work/oasis7/oasis7 || exit 1\n')
    startup.chmod(0o600)
    with open(os.environ['GITHUB_ENV'], 'a') as output:
        output.write(f'BASH_ENV={startup}\nRUSTUP_AUTO_INSTALL=0\n')
    print(json.dumps(dict(image=IMAGE, source=source, canonical_checkout=str(CHECKOUT),
                          same_directory=True, docker=docker)))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('operation', choices=('select', 'preflight'))
    parser.add_argument('source')
    args = parser.parse_args()
    if args.operation == 'select':
        enabled = selected(args.source, os.environ)
        with open(os.environ['GITHUB_OUTPUT'], 'a') as output:
            output.write('compatible=' + str(enabled).lower() + '\n')
    else:
        preflight(args.source)


if __name__ == '__main__':
    main()
