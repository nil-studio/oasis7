#!/usr/bin/env python3
"""Check whether the verified release source can use the fixed soak image."""
import os
import re
import subprocess
import sys
import tomllib

REPOSITORY = 'nil-studio/oasis7'
SHA = re.compile(r'[0-9a-f]{40}')
SOAK_PATHS = (
    'scripts/release-gate.sh',
    'scripts/p2p-longrun-soak.sh',
    'scripts/s10-five-node-game-soak.sh',
)


def protected_event(source, env):
    event_sha = env.get('GITHUB_SHA', '')
    if (env.get('GITHUB_REPOSITORY') != REPOSITORY or env.get('GITHUB_JOB') != 'plan'
            or not SHA.fullmatch(source) or not SHA.fullmatch(event_sha)
            or event_sha != env.get('GITHUB_WORKFLOW_SHA')):
        return False
    ref = env.get('GITHUB_REF', '')
    workflow = env.get('GITHUB_WORKFLOW_REF')
    if env.get('GITHUB_EVENT_NAME') == 'workflow_dispatch':
        return ref == 'refs/heads/main' and workflow in {
            f'{REPOSITORY}/.github/workflows/{name}-packages.yml@refs/heads/main'
            for name in ('release', 'mainnet')
        }
    return (env.get('GITHUB_EVENT_NAME') == 'push'
            and ref.startswith('refs/tags/') and event_sha == source
            and workflow == f'{REPOSITORY}/.github/workflows/release-packages.yml@{ref}')


def compatible_source(source):
    tree = subprocess.check_output(
        ['git', 'ls-tree', source, '--', 'rust-toolchain', 'rust-toolchain.toml', *SOAK_PATHS], text=True)
    entries = {path: metadata.split()[0] for metadata, path in
               (line.split('\t', 1) for line in tree.splitlines())}
    if entries != {'rust-toolchain.toml': '100644', **dict.fromkeys(SOAK_PATHS, '100755')}:
        return False
    result = subprocess.run(['git', 'show', f'{source}:rust-toolchain.toml'], capture_output=True, check=False)
    if result.returncode:
        return False
    try:
        toolchain = tomllib.loads(result.stdout.decode()).get('toolchain')
    except (UnicodeError, tomllib.TOMLDecodeError):
        return False
    return toolchain == {
        'channel': '1.96.0', 'components': ['rustfmt', 'clippy'], 'targets': ['wasm32-unknown-unknown'],
    }


def main():
    if len(sys.argv) != 2:
        raise SystemExit('usage: package-release-soak-image.py <verified-source-sha>')
    source = sys.argv[1]
    compatible = protected_event(source, os.environ) and compatible_source(source)
    with open(os.environ['GITHUB_OUTPUT'], 'a', encoding='utf-8') as output:
        print(f'compatible={str(compatible).lower()}', file=output)


if __name__ == '__main__':
    main()
