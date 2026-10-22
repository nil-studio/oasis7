#!/usr/bin/env python3
"""Bounded workspace-cache archives shared by ordinary, Formal and ECS.

Trace refs: O7-REQ-d9292810-5229-417f-9fc8-262ee04759c7.
The archive format and wrapper bound are shared; each caller supplies its own
logical staging limit before the normal Cargo unit/source validation.
"""
import gzip
import importlib.util
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tarfile
import tempfile
import time

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parent.parent
STAGING_BYTES = 6 * 1024 ** 3
ARCHIVE_NAME = 'workspace-units.tar.gz'
MANIFEST_BYTES = 64 * 1024 ** 2
MAX_FILES = 20_000
_spec = importlib.util.spec_from_file_location('workspace_archive_units', ROOT / 'scripts/ci-test-artifacts.py')
units = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(units)


def archive_manifest(metadata, max_bytes=STAGING_BYTES):
    def unique_object(pairs):
        result = {}
        for key, value in pairs:
            if key in result:
                raise ValueError('duplicate cache manifest field')
            result[key] = value
        return result
    if len(metadata) > MANIFEST_BYTES:
        raise ValueError('cache manifest exceeds its byte limit')
    try:
        manifest = json.loads(metadata, object_pairs_hook=unique_object)
    except RecursionError:
        raise ValueError('cache manifest nesting exceeds the JSON parser limit') from None
    if not isinstance(manifest, dict):
        raise ValueError('cache manifest must be an object')
    files, directories = manifest.get('files'), manifest.get('directories')
    if (not isinstance(files, dict) or not 0 < len(files) <= MAX_FILES
            or not isinstance(directories, dict) or len(directories) > MAX_FILES):
        raise ValueError('cache manifest has invalid file or directory counts')
    size = 0
    for name, metadata in [*files.items(), *directories.items()]:
        units._cache_path(Path('/cache-path-validation'), name)
        if (not isinstance(metadata, dict) or type(metadata.get('mode')) is not int
                or metadata['mode'] & ~0o777):
            raise ValueError('cache manifest has an invalid file mode')
    for metadata in files.values():
        if type(metadata.get('bytes')) is not int or metadata['bytes'] < 0:
            raise ValueError('cache manifest has an invalid file size')
        size += metadata['bytes']
    if not 0 < size <= max_bytes:
        raise ValueError('cache logical payload exceeds the staging budget or is empty')
    return manifest


def pack_archive(staged, destination, max_bytes=STAGING_BYTES):
    """Use short flat USTAR names: the unit manifest retains the original paths.

    No links, sparse records or PAX extensions are necessary. Duplicate Cargo
    hardlinks are regular payloads and count against the logical-byte bound.
    """
    started = time.monotonic()
    if (staged / 'manifest.json').stat().st_size > MANIFEST_BYTES:
        raise ValueError('cache manifest exceeds its byte limit')
    metadata = (staged / 'manifest.json').read_bytes()
    manifest = archive_manifest(metadata, max_bytes)
    # A failed compression never leaves a partial file at the action's path.
    with tempfile.TemporaryDirectory(prefix='workspace-pack-', dir=destination.parent.parent) as directory:
        pending = Path(directory) / ARCHIVE_NAME
        with pending.open('wb') as output, gzip.GzipFile(fileobj=output, mode='wb', compresslevel=1,
                                                       mtime=0, filename='') as compressed:
            with tarfile.open(fileobj=compressed, mode='w|', format=tarfile.USTAR_FORMAT) as archive:
                header = tarfile.TarInfo('manifest.json')
                header.size, header.mode = len(metadata), 0o600
                archive.addfile(header, io.BytesIO(metadata))
                for index, name in enumerate(sorted(manifest['files'])):
                    source = units._cache_path(staged / 'target', name)
                    header = tarfile.TarInfo(f'files/{index:06d}')
                    header.size, header.mode = manifest['files'][name]['bytes'], 0o600
                    with source.open('rb') as stream:
                        archive.addfile(header, stream)
        pending.replace(destination)
    return dict(archive_bytes=destination.stat().st_size,
                pack_seconds=round(time.monotonic() - started, 3))


def read_exact(stream, length):
    value = stream.read(length)
    if len(value) != length:
        raise ValueError('truncated workspace cache archive')
    return value


def read_header(stream, expected):
    # Read the raw header before tarfile can follow GNU/PAX/sparse extensions or
    # allocate an attacker-controlled extension buffer.
    header = tarfile.TarInfo.frombuf(read_exact(stream, tarfile.BLOCKSIZE), 'utf-8', 'strict')
    if (header.name != expected or not header.isreg() or header.type != tarfile.REGTYPE
            or header.linkname or header.pax_headers or header.sparse is not None):
        raise ValueError('unexpected, duplicate or unsafe workspace archive member')
    return header


def read_padding(stream, size):
    padding = read_exact(stream, -size % tarfile.BLOCKSIZE)
    if any(padding):
        raise ValueError('invalid workspace archive padding')


def unpack_archive(archive, destination, max_bytes=STAGING_BYTES):
    started = time.monotonic()
    if (archive.is_symlink() or not archive.is_file() or not 0 < archive.stat().st_size
            <= max_bytes + MANIFEST_BYTES + MAX_FILES * 1024):
        raise ValueError('workspace archive is missing, unsafe or too large')
    with gzip.open(archive, 'rb') as stream:
        header = read_header(stream, 'manifest.json')
        if not 0 < header.size <= MANIFEST_BYTES:
            raise ValueError('cache manifest exceeds its byte limit')
        metadata = read_exact(stream, header.size)
        manifest = archive_manifest(metadata, max_bytes)
        read_padding(stream, header.size)
        (destination / 'manifest.json').write_bytes(metadata)
        for index, name in enumerate(sorted(manifest['files'])):
            header = read_header(stream, f'files/{index:06d}')
            expected = manifest['files'][name]
            if header.size != expected['bytes']:
                raise ValueError('workspace archive member differs from the bounded manifest size')
            path = units._cache_path(destination / 'target', name)
            path.parent.mkdir(parents=True, exist_ok=True)
            remaining = header.size
            with path.open('xb') as output:
                while remaining:
                    value = read_exact(stream, min(remaining, 1024 ** 2))
                    output.write(value)
                    remaining -= len(value)
            path.chmod(expected['mode'])
            read_padding(stream, header.size)
        # USTAR termination is two zero blocks, padded to one 10 KiB record.
        # Also consume the gzip trailer; truncated or concatenated payloads
        # cannot reach Cargo validation with a partial archive.
        ending = stream.read(tarfile.RECORDSIZE + 1)
        if not 2 * tarfile.BLOCKSIZE <= len(ending) <= tarfile.RECORDSIZE or any(ending):
            raise ValueError('invalid or oversized workspace archive termination')
        for name in sorted(manifest['directories'], key=lambda name: len(Path(name).parts)):
            units._cache_path(destination / 'target', name).mkdir(parents=True, exist_ok=True)
        for name in sorted(manifest['directories'], key=lambda name: len(Path(name).parts), reverse=True):
            units._cache_path(destination / 'target', name).chmod(manifest['directories'][name]['mode'])
    return dict(archive_bytes=archive.stat().st_size, unpack_seconds=round(time.monotonic() - started, 3))


def upload_bound(archive, root=None):
    """Bound the pinned Linux actions/cache wrapper around exactly one file.

    The pinned v4 ECS and v6.1.0 Rust actions use the same relative-path,
    --posix/-P/-C/--files-from tar and zstdmt-without-long/gzip contracts:
    v6 resolvePaths L43315, compression L43381, tar L94065 and L94196:
    https://github.com/actions/cache/blob/55cc8345863c7cc4c66a329aec7e433d2d1c52a9/dist/save-only/index.js
    https://github.com/actions/cache/blob/5a3ec84eff668545956fd18022155c47e93e2684/dist/save-only/index.js
    GNU tar is measured rather than assuming path/PAX/header overhead. Only
    atime/ctime can change before save; their bounded timestamp strings can add
    at most one PAX data block, hence one additional 20-block tar record.
    ZSTD_COMPRESSBOUND alone only promises a one-pass bound. This contract uses
    the audited standard zstdmt CLI (v1.5.5, no intermediate flushes, no --long),
    which fills streaming blocks; it does not apply to arbitrary flush callers.
    The final ceiling also takes the larger zlib generic deflateBound, covering
    the gzip fallback (fixed/stored blocks plus the 18-byte gzip wrapper).
    """
    if os.environ.get('TAR_OPTIONS') or os.environ.get('GZIP'):
        raise ValueError('custom tar/gzip options are outside the cache upload size contract')
    version = subprocess.check_output(['tar', '--version'], text=True)
    if 'GNU tar' not in version.splitlines()[0]:
        raise ValueError('workspace cache upload sizing requires GNU tar')
    workspace = Path(os.environ.get('GITHUB_WORKSPACE', root or ROOT)).resolve()
    relative = os.path.relpath(archive, workspace)
    if len(os.fsencode(relative)) > 4096 or relative.startswith('-') or any(ord(c) < 32 for c in relative):
        raise ValueError('workspace cache path is outside the upload size contract')
    with tempfile.TemporaryDirectory(prefix='workspace-wrapper-', dir=archive.parent.parent) as directory:
        manifest = Path(directory) / 'manifest.txt'
        manifest.write_text(relative + '\n')
        with tempfile.TemporaryFile() as errors:
            with subprocess.Popen(['tar', '--posix', '-cf', '-', '--exclude', 'cache.tzst', '-P',
                                   '-C', str(workspace), '--files-from', str(manifest)],
                                  stdout=subprocess.PIPE, stderr=errors) as process:
                total = 0
                while data := process.stdout.read(1024 ** 2):
                    total += len(data)
                if process.wait():
                    raise ValueError('GNU tar cache wrapper sizing failed')
    bounded_tar = total + tarfile.RECORDSIZE
    zstd = bounded_tar + (bounded_tar >> 8) + max(0, ((128 << 10) - bounded_tar) >> 11)
    fixed = bounded_tar + (bounded_tar >> 3) + (bounded_tar >> 8) + (bounded_tar >> 9) + 4
    stored = bounded_tar + (bounded_tar >> 5) + (bounded_tar >> 7) + (bounded_tar >> 11) + 7
    return dict(outer_tar_bytes=total, outer_tar_bound_bytes=bounded_tar,
                upload_bound_bytes=max(zstd, fixed + 18, stored + 18))


