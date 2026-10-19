#!/usr/bin/env python3
"""Bind intermediate native binaries to the exact package source and build tuple."""
import hashlib
import json
from pathlib import Path
import subprocess
import sys


def require(condition, message):
    if not condition:
        raise ValueError(message)


def manifest(directory, repository, profile, target, platform, ops):
    suffix = '.exe' if platform == 'windows-x64' else ''
    names = ['oasis7_game_launcher', 'oasis7_web_launcher', 'oasis7_viewer_live',
             'oasis7_chain_runtime', 'oasis7_client_launcher']
    paths = ['bin/' + name + suffix for name in names]
    if ops:
        paths += ['ops/bin/' + name + suffix for name in
                  ['oasis7_world_repair_rebuild', 'oasis7_governance_registry_import',
                   'oasis7_governance_registry_audit']]
        paths += ['ops/bin/service-readback']
    hashes = {}
    for name in paths:
        path = directory / name
        require(not path.is_symlink() and path.is_file() and path.stat().st_size, name)
        parent = path.parent
        while parent != directory.parent:
            require(not parent.is_symlink(), name)
            parent = parent.parent
        with path.open('rb') as stream:
            digest = hashlib.sha256()
            for block in iter(lambda: stream.read(1024 * 1024), b''):
                digest.update(block)
        hashes[name] = dict(sha256=digest.hexdigest(), mode=path.stat().st_mode & 0o777)
    return dict(schema=1, source=subprocess.check_output(
        ['git', '-C', repository, 'rev-parse', 'HEAD'], text=True).strip(),
        profile=profile, target=target, platform=platform, ops=ops, files=hashes)


def main():
    mode, directory, repository, profile, target, platform, ops = sys.argv[1:]
    directory = Path(directory).absolute()
    expected = manifest(directory, repository, profile, target, platform, ops == '1')
    receipt = directory / 'native-build.json'
    if mode == 'write':
        receipt.write_text(json.dumps(expected, sort_keys=True) + '\n')
    elif mode == 'verify':
        require(not receipt.is_symlink() and receipt.is_file(), 'missing native build identity')
        require(json.loads(receipt.read_text()) == expected, 'native build identity or bytes mismatch')
    else:
        raise ValueError('expected write or verify')


if __name__ == '__main__':
    main()
