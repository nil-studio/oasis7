#!/usr/bin/env python3
"""Seal and restore the original canonical Provider bundle within a trusted run."""
import argparse
import hashlib
from io import BytesIO
import json
import os
from pathlib import Path, PurePosixPath
import re
import shlex
import shutil
import stat
import subprocess
import tarfile
from importlib import import_module

ProviderArtifact = import_module('ci-test-artifacts').ProviderArtifact

RECORD = 'preview-provider.json'
SEAL = 'provider-handoff.json'
MAX_ARCHIVE_BYTES = 256 * 1024 * 1024
MAX_RECORD_BYTES = 256 * 1024
MAX_MEMBERS = 32
REPOSITORY = 'nil-studio/oasis7'
WORKFLOW = f'{REPOSITORY}/.github/workflows/rust.yml@refs/heads/main'


def positive(value):
    if not isinstance(value, str) or not re.fullmatch(r'[1-9][0-9]{0,18}', value):
        raise ValueError('expected positive numeric Provider identity')
    return int(value)


def digest(value):
    if not isinstance(value, str) or not re.fullmatch(r'[a-f0-9]{64}', value):
        raise ValueError('invalid Provider archive SHA-256')
    return value


def identity(root, source):
    if not isinstance(source, str) or not re.fullmatch(r'[a-f0-9]{40}', source):
        raise ValueError('invalid Provider source SHA')
    actual = ProviderArtifact.identity(root)
    if actual['source_sha'] != source or actual['tested_sha'] != source:
        raise ValueError('Provider source differs from the trusted checkout')
    return actual


def trusted_consumer(env, source):
    required = {
        'GITHUB_ACTIONS': 'true', 'GITHUB_REPOSITORY': REPOSITORY,
        'GITHUB_REF': 'refs/heads/main', 'GITHUB_WORKFLOW_REF': WORKFLOW,
        'GITHUB_SHA': source, 'GITHUB_WORKFLOW_SHA': source, 'GITHUB_JOB': 'full-regression',
        'RUNNER_ENVIRONMENT': 'github-hosted', 'RUNNER_OS': 'Linux', 'RUNNER_ARCH': 'X64',
    }
    if any(env.get(name) != value for name, value in required.items()):
        raise ValueError('Provider handoff requires the exact trusted full regression job')
    event = env.get('GITHUB_EVENT_NAME')
    if event == 'workflow_dispatch':
        with verified_file(Path(env['GITHUB_EVENT_PATH']), MAX_RECORD_BYTES) as file:
            inputs = json.load(file).get('inputs') or {}
        if inputs.get('run_mode') != 'full' or inputs.get('source_ref') not in ('', None):
            raise ValueError('Provider handoff requires the current main full run')
    elif event != 'schedule':
        raise ValueError('Provider handoff requires schedule or full dispatch')


def verified_file(path, bound):
    if any(parent.is_symlink() for parent in path.parents):
        raise ValueError('Provider path has a symlink ancestor')
    fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
    file = os.fdopen(fd, 'rb')
    info = os.fstat(file.fileno())
    if not stat.S_ISREG(info.st_mode) or info.st_nlink != 1 or not 0 < info.st_size <= bound:
        file.close()
        raise ValueError('Provider file must be a bounded regular file')
    return file


def load_json(path):
    with verified_file(path, MAX_RECORD_BYTES) as file:
        return json.load(file)


def archive_names(record):
    if not isinstance(record, dict):
        raise ValueError('Provider record must be an object')
    files = record.get('files')
    libraries = record.get('libraries')
    if not isinstance(files, dict) or set(files) != {'wasm', 'metadata', 'receipt', 'checker'} or not isinstance(libraries, list):
        raise ValueError('Provider record needs the original four materials and libraries')
    names = {RECORD: 0o644}
    for entry, mode in [(value, 0o755 if kind == 'checker' else 0o644) for kind, value in files.items()] + [
            (value, None) for value in libraries]:
        name = entry['path']
        if not isinstance(name, str) or any(part in ('', '.', '..') for part in name.split('/')) or name in names:
            raise ValueError('invalid or duplicate Provider record path')
        names[name] = mode
    if len(names) + 1 > MAX_MEMBERS:
        raise ValueError('Provider record exceeds its inventory bound')
    return names


def seal_for(expected, run, attempt, modes):
    return {'schema': 1, 'run_id': positive(run), 'producer_attempt': positive(attempt), 'modes': modes, **expected}


def pack(root, bundle, source, run, attempt, archive):
    expected = identity(root, source)
    record = load_json(bundle / RECORD)
    names = archive_names(record)
    ProviderArtifact.validate(root, bundle, record, expected)
    modes = {}
    for name, required in names.items():
        mode = stat.S_IMODE(ProviderArtifact.checked(bundle, name).stat().st_mode)
        if mode not in (0o644, 0o755) or (required is not None and mode != required):
            raise ValueError('Provider material has unexpected file mode')
        modes[name] = mode
    payload = (json.dumps(seal_for(expected, run, attempt, modes), sort_keys=True) + '\n').encode()
    with archive.open('xb') as out, tarfile.open(fileobj=out, mode='w', format=tarfile.USTAR_FORMAT) as writer:
        info = tarfile.TarInfo(SEAL)
        info.size = len(payload)
        info.mode = 0o644
        writer.addfile(info, BytesIO(payload))
        remaining = MAX_ARCHIVE_BYTES - 1024 * 1024
        for name, mode in modes.items():
            path = ProviderArtifact.checked(bundle, name)
            with verified_file(path, remaining) as file:
                info = os.fstat(file.fileno())
                if stat.S_IMODE(info.st_mode) != mode:
                    raise ValueError('Provider material has unexpected file mode')
                member = tarfile.TarInfo(name)
                member.size = info.st_size
                member.mode = mode
                writer.addfile(member, file)
                remaining -= member.size
    with verified_file(archive, MAX_ARCHIVE_BYTES) as file:
        return hashlib.file_digest(file, 'sha256').hexdigest()


def extract(archive, target, expected_digest):
    with verified_file(archive, MAX_ARCHIVE_BYTES) as file:
        if hashlib.file_digest(file, 'sha256').hexdigest() != digest(expected_digest):
            raise ValueError('Provider artifact SHA-256 mismatch')
        file.seek(0)
        modes = {}
        with tarfile.open(fileobj=file, mode='r:') as reader:
            for member in reader:
                name = member.name
                parts = name.split('/')
                if (not member.isfile() or member.sparse is not None or member.pax_headers
                        or not all(parts) or any(part in ('.', '..') for part in parts)
                        or '\\' in name or name in modes
                        or len(modes) >= MAX_MEMBERS
                        or not 0 < member.size <= MAX_ARCHIVE_BYTES
                        or not all(32 <= ord(c) < 127 for c in name)
                        or member.mode not in (0o644, 0o755)):
                    raise ValueError('invalid Provider archive member')
                destination = target.joinpath(*PurePosixPath(name).parts)
                destination.parent.mkdir(parents=True, exist_ok=True)
                with destination.open('xb') as out, reader.extractfile(member) as source:
                    shutil.copyfileobj(source, out, length=64 * 1024)
                destination.chmod(member.mode)
                modes[name] = member.mode
            end = reader.offset
        file.seek(end)
        while chunk := file.read(64 * 1024):
            if any(chunk):
                raise ValueError('Provider archive has trailing content')
        return modes


def consume(root, archive, target, source, run, current_attempt, producer_attempt, artifact_id, expected_digest):
    positive(artifact_id)
    if positive(producer_attempt) > positive(current_attempt):
        raise ValueError('Provider artifact came from a future attempt')
    expected = identity(root, source)
    if any(path.is_symlink() for path in target.parents):
        raise ValueError('Provider import has a symlink ancestor')
    target.mkdir(mode=0o700)
    modes = extract(archive, target, expected_digest)
    seal = load_json(target / SEAL)
    record = load_json(target / RECORD)
    names = archive_names(record)
    if modes.pop(SEAL, None) != 0o644 or set(modes) != set(names) or any(
            required is not None and modes[name] != required for name, required in names.items()):
        raise ValueError('Provider archive inventory or mode mismatch')
    if seal != seal_for(expected, run, producer_attempt, modes):
        raise ValueError('Provider run, origin attempt, source, tree or mode mismatch')
    return ProviderArtifact.validate(root, target, record, expected)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('operation', choices=['pack', 'consume'])
    parser.add_argument('--root', type=Path, required=True)
    parser.add_argument('--bundle', type=Path, required=True)
    parser.add_argument('--archive', type=Path, required=True)
    parser.add_argument('--source', required=True)
    parser.add_argument('--run-id', required=True)
    parser.add_argument('--attempt', required=True)
    parser.add_argument('--producer-attempt')
    parser.add_argument('--artifact-id')
    parser.add_argument('--sha256')
    parser.add_argument('--output', type=Path)
    args = parser.parse_args()
    if args.operation == 'pack':
        print(pack(args.root, args.bundle, args.source, args.run_id, args.attempt, args.archive))
        return
    trusted_consumer(os.environ, args.source)
    if (os.getuid(), os.getgid()) != (1001, 1001):
        raise ValueError('Provider consumer requires the original Linux runner UID and GID')
    if args.run_id != os.environ.get('GITHUB_RUN_ID') or args.attempt != os.environ.get('GITHUB_RUN_ATTEMPT'):
        raise ValueError('Provider consumer run identity differs from the current run')
    if args.output != args.bundle.parent / 'environment.sh':
        parser.error('--output must be the environment file adjacent to the fresh import')
    values = consume(args.root, args.archive, args.bundle, args.source, args.run_id, args.attempt,
                     args.producer_attempt, args.artifact_id, args.sha256)
    with args.output.open('x', encoding='utf-8') as output:
        output.write(''.join('export ' + key + '=' + shlex.quote(value) + '\n' for key, value in values.items()))
    args.output.chmod(0o600)


if __name__ == '__main__':
    try:
        main()
    except (ValueError, OSError, subprocess.SubprocessError, tarfile.TarError, KeyError, TypeError) as error:
        raise SystemExit(f'Provider handoff rejected: {error}') from error
