#!/usr/bin/env python3
"""Canonical test-only provider material, bound to its native producer identity."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import shutil
import tempfile
import shlex
import sys

MODULE = 'module.runtime.local-test-provider'
MANIFEST = 'crates/oasis7_builtin_wasm_modules/local_test_provider/Cargo.toml'
TARGET = 'wasm32-unknown-unknown'
DIRECTORY = 'preview-provider'
VARIABLES = ('OASIS7_PREVIEW_TEST_PROVIDER_WASM', 'OASIS7_PREVIEW_TEST_PROVIDER_METADATA')


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def checked(root, relative):
    if not isinstance(relative, str) or not relative or Path(relative).is_absolute() or any(p in ('', '.', '..') for p in relative.split('/')):
        raise ValueError('invalid provider artifact path')
    path = root / relative
    cursor = root
    for part in relative.split('/'):
        cursor /= part
        if cursor.is_symlink():
            raise ValueError('provider artifact symlink')
    if not path.is_file() or not path.resolve().is_relative_to(root.resolve()):
        raise ValueError('provider material missing or outside root')
    return path


def verify_metadata(root, bundle, paths):
    subprocess.run([str(paths['checker']), '--verify-provider-artifact', str(root),
                    str(paths['wasm']), str(paths['metadata']), str(paths['receipt'])],
                   cwd=root, check=True, env=dict(os.environ, LD_LIBRARY_PATH=str(bundle / DIRECTORY / 'lib') + os.pathsep + os.environ.get('LD_LIBRARY_PATH', '')))


def tree_digest(root):
    hasher = hashlib.sha256(subprocess.check_output(['git', 'diff', '--binary', 'HEAD'], cwd=root))
    hasher.update(subprocess.check_output(['git', 'rev-parse', 'HEAD^{tree}'], cwd=root))
    for name in sorted(subprocess.check_output(['git', 'ls-files', '--others', '--exclude-standard', '-z'], cwd=root).decode().split('\0')):
        if name:
            hasher.update(name.encode() + b'\0')
            hasher.update(checked(root, name).read_bytes())
    return hasher.hexdigest()


def identity(root):
    tested = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, text=True).strip()
    return {'tested_sha': tested, 'source_sha': os.environ.get('SOURCE_SHA', tested), 'checkout_tree_sha256': tree_digest(root)}


def build(root, bundle, expected):
    directory = bundle / DIRECTORY
    directory.mkdir(parents=True, exist_ok=False)
    before = identity(root)
    if any(before[key] != expected[key] for key in ('tested_sha', 'source_sha')):
        raise ValueError('provider producer source differs from native identity')
    (root / 'target').mkdir(exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='ci-preview-provider-', dir=root / 'target') as temporary:
        subprocess.run(['bash', str(root / 'scripts/build-wasm-module.sh'), '--candidate-build',
                        '--manifest-path', MANIFEST, '--out-dir', temporary,
                        '--module-id', MODULE, '--profile', 'release'], cwd=root, check=True)
        for suffix in ('.wasm', '.metadata.json', '.build-receipt.json'):
            shutil.copy2(Path(temporary) / (MODULE + suffix), directory / (MODULE + suffix))
    files = {kind: f'{DIRECTORY}/{MODULE}{suffix}' for kind, suffix in
             (('wasm', '.wasm'), ('metadata', '.metadata.json'), ('receipt', '.build-receipt.json'))}
    subprocess.run(['cargo', 'build', '--locked', '-p', 'oasis7_distfs', '--features',
                    'sync-builtin-wasm-identity', '--bin', 'sync_builtin_wasm_identity'],
                   cwd=root, check=True, env={key:value for key,value in os.environ.items() if key != 'RUSTC_WRAPPER'})
    checker = 'sync_builtin_wasm_identity' + ('.exe' if os.name == 'nt' else '')
    files['checker'] = f'{DIRECTORY}/{checker}'
    shutil.copy2(root / 'target/debug' / checker, bundle / files['checker'])
    libraries = []
    if sys.platform == 'linux':
        linked = subprocess.run(['ldd', str(bundle / files['checker'])], text=True, capture_output=True, check=True)
        for line in linked.stdout.splitlines():
            fields = line.split()
            if '=>' in fields and len(fields) > 2 and fields[2].startswith(str(root / 'target') + '/'):
                library = Path(fields[2])
                relative = f'{DIRECTORY}/lib/{library.name}'
                (directory / 'lib').mkdir(exist_ok=True)
                shutil.copy2(library, bundle / relative)
                libraries.append({'path': relative, 'sha256': digest(bundle / relative)})
    if identity(root) != before:
        raise ValueError('provider source changed during official build')
    record = dict(before, libraries=libraries, files={kind: {'path': name, 'sha256': digest(checked(bundle, name))}
                                   for kind, name in files.items()})
    validate(root, bundle, record, expected)
    return record


def validate(root, bundle, record, expected):
    if not isinstance(record, dict) or any(record.get(key) != expected[key] for key in ('tested_sha', 'source_sha')):
        raise ValueError('provider tested/source identity mismatch')
    if record.get('checkout_tree_sha256') != tree_digest(root):
        raise ValueError('provider checkout tree differs')
    if set(record.get('files', {})) != {'wasm', 'metadata', 'receipt', 'checker'}:
        raise ValueError('provider artifact pair incomplete')
    libraries = record.get('libraries')
    if not isinstance(libraries, list):
        raise ValueError('provider checker libraries missing')
    seen = set()
    for library in libraries:
        name = library.get('path', '')
        if not name.startswith(DIRECTORY + '/lib/') or name in seen or digest(checked(bundle, name)) != library.get('sha256'):
            raise ValueError('provider checker library identity mismatch')
        seen.add(name)
    paths = {}
    for kind, suffix in (('wasm', '.wasm'), ('metadata', '.metadata.json'), ('receipt', '.build-receipt.json'), ('checker', '')):
        entry = record['files'][kind]
        name = ('sync_builtin_wasm_identity' + ('.exe' if os.name == 'nt' else '')) if kind == 'checker' else MODULE + suffix
        if entry.get('path') != f'{DIRECTORY}/{name}':
            raise ValueError('provider artifact role/path mismatch')
        paths[kind] = checked(bundle, entry['path'])
        if digest(paths[kind]) != entry['sha256']:
            raise ValueError('provider artifact hash mismatch')
    if not os.access(paths['checker'], os.X_OK):
        raise ValueError('provider checker is not executable')
    verify_metadata(root, bundle, paths)
    return dict(zip(VARIABLES, (str(paths[kind].resolve()) for kind in ('wasm', 'metadata'))))


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--repo-root', type=Path, required=True)
    parser.add_argument('--bundle', type=Path, required=True)
    parser.add_argument('--env-file', type=Path, required=True)
    args = parser.parse_args()
    args.bundle.mkdir(parents=True, exist_ok=True)
    record = build(args.repo_root, args.bundle, identity(args.repo_root))
    (args.bundle / 'preview-provider.json').write_text(json.dumps(record) + '\n')
    environment = validate(args.repo_root, args.bundle, record, identity(args.repo_root))
    args.env_file.write_text(''.join('export ' + key + '=' + shlex.quote(value) + '\n' for key,value in environment.items()))
    args.env_file.chmod(0o600)


if __name__ == '__main__':
    main()
