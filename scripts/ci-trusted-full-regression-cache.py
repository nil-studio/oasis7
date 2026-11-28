#!/usr/bin/env python3
"""Archive inspection and read-only save admission for the finite full cache roles."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess

from importlib import import_module

archive = import_module('ci-portable-cache-archive')
tool_cache = import_module('ci-tool-cache')

REPO = Path('/home/runner/work/oasis7/oasis7')
HOME = Path('/home/runner')
TEMP = HOME / 'work/_temp'
MIB = 1024 * 1024
ROLE_LIMITS = {
    'npm': (HOME / '.npm', 512 * MIB, 16384),
    'trunk': (TEMP / 'ci-tools/trunk-x86_64-unknown-linux-gnu.tar.gz', 64 * MIB, 1),
    'wasm-bindgen': (HOME / '.cache/oasis7/wasm-bindgen-cli', 128 * MIB, 256),
    'sentinel': (TEMP / 'cache-permission-sentinel', 1024, 2),
}


def role_layout(role, bindgen_version):
    root, max_bytes, max_files = ROLE_LIMITS[role]
    if role == 'wasm-bindgen':
        if not re.fullmatch(r'\d+\.\d+\.\d+', bindgen_version):
            raise ValueError('invalid wasm-bindgen lock version')
        root = root / bindgen_version
    return dict(root=str(root), member=os.path.relpath(root, REPO), max_bytes=max_bytes, max_files=max_files)


def check_payload(role, root, bindgen_version, deadline):
    if role == 'npm':
        return
    if role == 'trunk':
        source = (REPO / 'scripts/install-ci-trunk.sh').read_text(encoding='utf-8')
        checksums = re.findall(r'^checksum=([a-f0-9]{64})$', source, re.M)
        if len(checksums) != 1 or root.stat().st_size > ROLE_LIMITS['trunk'][1]:
            raise ValueError('Trunk cache payload has no valid pinned checksum or exceeds its bound')
        with root.open('rb') as stream:
            if hashlib.file_digest(stream, 'sha256').hexdigest() != checksums[0]:
                raise ValueError('Trunk cache archive checksum mismatch')
    elif role == 'wasm-bindgen':
        binary = tool_cache.cache_root(root / 'bin/wasm-bindgen', label=role)
        output = subprocess.check_output([str(binary), '--version'], text=True, timeout=min(5, deadline),
                                         env={'PATH': os.defpath})
        if output.strip() != f'wasm-bindgen {bindgen_version}':
            raise ValueError('wrong prepared wasm-bindgen version')
    elif role == 'sentinel':
        child = tool_cache.cache_root(root / 'sentinel', label=role)
        if child.stat().st_size != 11 or child.read_bytes() != b'trusted-v2\n':
            raise ValueError('wrong main sentinel payload')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('operation', choices=['inspect', 'admit'])
    parser.add_argument('role', choices=ROLE_LIMITS)
    parser.add_argument('--bindgen-version', default='')
    parser.add_argument('--archive', type=Path)
    parser.add_argument('--key')
    args = parser.parse_args()
    layout = role_layout(args.role, args.bindgen_version)
    if args.operation == 'inspect':
        if args.role == 'sentinel' or args.archive is None:
            parser.error('archive is required and sentinel cannot be restored')
        result = archive.inspect_archive(args.archive, 'zstd-without-long', layout, exact_file=args.role == 'trunk')
    else:
        if not args.key:
            parser.error('validated layout key is required')
        result = tool_cache.admit_save(
            Path(layout['root']), args.key, os.environ, repository_root=REPO, expected_key=args.key,
            writer_workflow='.github/workflows/rust.yml', writer_job='full-regression',
            check_payload=lambda root, *, deadline: check_payload(args.role, root, args.bindgen_version, deadline),
            max_cache_bytes=layout['max_bytes'], max_entries=layout['max_files'], label=args.role,
            exact_file=args.role == 'trunk', allow_physical_workspace_alias=True,
        )
    print(json.dumps(result, sort_keys=True))


if __name__ == '__main__':
    main()
