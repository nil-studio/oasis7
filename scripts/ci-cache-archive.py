#!/usr/bin/env python3
"""Bounded workspace-cache archives shared by ordinary, Formal and ECS.

Trace refs: O7-REQ-d9292810-5229-417f-9fc8-262ee04759c7.
The archive format and wrapper bound are shared; each caller supplies its own
logical staging limit before the normal Cargo unit/source validation.
"""
from contextlib import contextmanager
import gzip
import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import re
import stat
import subprocess
import sys
import tarfile
import tempfile
import time

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parent.parent
STAGING_BYTES = 6 * 1024 ** 3
ARCHIVE_NAME = 'workspace-units.tar.gz'
COMPACT_ARCHIVE_NAME = 'workspace-units.tar.zst'
MANIFEST_BYTES = 64 * 1024 ** 2
MAX_FILES = 20_000
ZSTD_WINDOW_BYTES = 64 * 1024 ** 2
MAX_ZSTD_BLOCKS = 1_000_000
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
    if 'archive' in manifest:
        archive_aliases(manifest)
    return manifest


def archive_aliases(manifest):
    envelope = manifest['archive']
    if (not isinstance(envelope, dict) or set(envelope) != {'version', 'aliases'}
            or type(envelope['version']) is not int or envelope['version'] != 1
            or not isinstance(envelope['aliases'], dict)):
        raise ValueError('invalid compact workspace archive envelope')
    files, aliases = manifest['files'], envelope['aliases']
    now = time.time_ns()
    for metadata in [*files.values(), *manifest['directories'].values()]:
        if type(metadata.get('mtime_ns')) is not int or not 0 < metadata['mtime_ns'] < now:
            raise ValueError('invalid compact workspace archive timestamp')
    for metadata in files.values():
        if not isinstance(metadata.get('sha256'), str) or not re.fullmatch('[0-9a-f]{64}', metadata['sha256']):
            raise ValueError('invalid compact workspace archive checksum')
    for name, canonical in aliases.items():
        if (name not in files or not isinstance(canonical, str) or canonical not in files
                or canonical >= name or canonical in aliases):
            raise ValueError('compact archive aliases require an earlier canonical file')
        if any(files[name][field] != files[canonical][field]
               for field in ('bytes', 'mode', 'mtime_ns', 'sha256')):
            raise ValueError('compact archive alias metadata differs from its canonical file')
    return aliases


def archive_statistics(manifest, compact):
    aliases = archive_aliases(manifest) if compact else {}
    logical = sum(metadata['bytes'] for metadata in manifest['files'].values())
    deduplicated = sum(manifest['files'][name]['bytes'] for name in aliases)
    return dict(archive_codec='zstd' if compact else 'gzip', archive_logical_bytes=logical,
                archive_payload_bytes=logical - deduplicated, archive_deduplicated_bytes=deduplicated,
                archive_alias_count=len(aliases))


def zstd_environment():
    # Pin the writer contract even if a runner carries user compression defaults.
    return {key: value for key, value in os.environ.items() if key not in ('ZSTD_CLEVEL', 'ZSTD_NBTHREADS')}


@contextmanager
def compression_stream(output, compact):
    if not compact:
        with gzip.GzipFile(fileobj=output, mode='wb', compresslevel=1, mtime=0, filename='') as stream:
            yield stream
        return
    with tempfile.TemporaryFile() as errors:
        process = subprocess.Popen(['zstd', '-q', '-3', '-T1', '--zstd=wlog=23', '--check', '-c'],
                                   stdin=subprocess.PIPE, stdout=output, stderr=errors, env=zstd_environment())
        try:
            with process.stdin as stream:
                yield stream
            if process.wait():
                raise ValueError('compact workspace archive compression failed')
        finally:
            if process.poll() is None:
                process.kill()
            process.wait()


class DigestingReader:
    def __init__(self, stream):
        self.stream, self.digest = stream, hashlib.sha256()

    def read(self, size):
        value = self.stream.read(size)
        self.digest.update(value)
        return value


def pack_archive(staged, destination, max_bytes=STAGING_BYTES, *, compact=False):
    """Write regular flat USTAR entries; only compact metadata describes aliases.

    ECS retains its gzip format by default. Compact callers use zstd plus an
    explicit inode-alias map, while every logical path still counts in full
    against max_bytes. Equal contents alone never introduce a new hardlink.
    """
    started = time.monotonic()
    if (staged / 'manifest.json').stat().st_size > MANIFEST_BYTES:
        raise ValueError('cache manifest exceeds its byte limit')
    metadata = (staged / 'manifest.json').read_bytes()
    manifest = archive_manifest(metadata, max_bytes)
    if 'archive' in manifest:
        raise ValueError('staging metadata already contains an archive envelope')
    aliases = {}
    if compact:
        manifest['archive'] = dict(version=1, aliases=aliases)
        archive_aliases(manifest)
        inodes = {}
        for name in sorted(manifest['files']):
            info = units._cache_path(staged / 'target', name).stat()
            expected = manifest['files'][name]
            if (not stat.S_ISREG(info.st_mode) or info.st_size != expected['bytes']
                    or stat.S_IMODE(info.st_mode) != expected['mode'] or info.st_mtime_ns != expected['mtime_ns']):
                raise ValueError('compact archive source differs from its staged metadata')
            inode = (info.st_dev, info.st_ino)
            if inode in inodes:
                aliases[name] = inodes[inode]
            else:
                inodes[inode] = name
        metadata = (json.dumps(manifest, sort_keys=True) + '\n').encode()
        archive_manifest(metadata, max_bytes)
    # A failed compression never leaves a partial file at the action's path.
    with tempfile.TemporaryDirectory(prefix='workspace-pack-', dir=destination.parent.parent) as directory:
        pending = Path(directory) / ARCHIVE_NAME
        with pending.open('wb') as output, compression_stream(output, compact) as compressed:
            with tarfile.open(fileobj=compressed, mode='w|', format=tarfile.USTAR_FORMAT) as archive:
                header = tarfile.TarInfo('manifest.json')
                header.size, header.mode = len(metadata), 0o600
                archive.addfile(header, io.BytesIO(metadata))
                for index, name in enumerate(sorted(manifest['files'])):
                    if name in aliases:
                        continue
                    source = units._cache_path(staged / 'target', name)
                    header = tarfile.TarInfo(f'files/{index:06d}')
                    header.size, header.mode = manifest['files'][name]['bytes'], 0o600
                    with source.open('rb') as stream:
                        reader = DigestingReader(stream) if compact else stream
                        archive.addfile(header, reader)
                        if compact and (reader.digest.hexdigest() != manifest['files'][name]['sha256']
                                        or stream.read(1)):
                            raise ValueError('compact archive source checksum or size changed while packing')
        pending.replace(destination)
    return dict(archive_statistics(manifest, compact), archive_bytes=destination.stat().st_size,
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


def validate_zstd_frame(archive, max_expanded):
    """Check one bounded frame; the zstd CLI owns decompression and checksums.

    RFC 8878 sections 3.1.1.1-3.1.1.2 describe these length/header fields:
    https://www.rfc-editor.org/rfc/rfc8878.html#section-3.1.1
    Empty or skippable trailing frames otherwise disappear during CLI decode.
    """
    with archive.open('rb') as stream:
        if read_exact(stream, 4) != b'\x28\xb5\x2f\xfd':
            raise ValueError('compact archive must contain one normal zstd frame')
        descriptor = read_exact(stream, 1)[0]
        # This writer uses checksums, no dictionaries and no future extensions.
        if descriptor & 0x1b or not descriptor & 4:
            raise ValueError('unsupported compact archive frame descriptor')
        single = bool(descriptor & 0x20)
        window = None
        if not single:
            value = read_exact(stream, 1)[0]
            base = 1 << (10 + (value >> 3))
            window = base + (base >> 3) * (value & 7)
        size_bytes = ((1 if single else 0), 2, 4, 8)[descriptor >> 6]
        content_size = None
        if size_bytes:
            content_size = int.from_bytes(read_exact(stream, size_bytes), 'little')
            if size_bytes == 2:
                content_size += 256
        if single:
            window = content_size
        if window > ZSTD_WINDOW_BYTES or content_size is not None and content_size > max_expanded:
            raise ValueError('compact archive frame exceeds memory or expanded byte bounds')
        for _ in range(MAX_ZSTD_BLOCKS):
            block = int.from_bytes(read_exact(stream, 3), 'little')
            kind, size = (block >> 1) & 3, block >> 3
            if kind == 3 or size > min(window, 128 * 1024):
                raise ValueError('invalid compact archive block header')
            stream.seek(1 if kind == 1 else size, os.SEEK_CUR)
            if block & 1:
                read_exact(stream, 4)  # Required content checksum; validated by zstd.
                if stream.read(1):
                    raise ValueError('compact archive contains trailing bytes or frames')
                return
        raise ValueError('compact archive exceeds the bounded frame block count')


@contextmanager
def decompression_stream(archive, compact):
    if not compact:
        with gzip.open(archive, 'rb') as stream:
            yield stream
        return
    with archive.open('rb') as source, tempfile.TemporaryFile() as errors:
        process = subprocess.Popen(['zstd', '-q', '-d', '-c', '-M64MiB', '--check', '--no-pass-through'],
                                   stdin=source, stdout=subprocess.PIPE, stderr=errors, env=zstd_environment())
        try:
            with process.stdout as stream:
                yield stream
            if process.wait():
                raise ValueError('compact workspace archive decompression or checksum failed')
        finally:
            # A rejected manifest/header can leave arbitrarily much unread
            # output. Never wait for a decoder blocked on its full stdout pipe.
            if process.poll() is None:
                process.kill()
            process.wait()


def unpack_archive(archive, destination, max_bytes=STAGING_BYTES):
    started = time.monotonic()
    maximum = max_bytes + MANIFEST_BYTES + MAX_FILES * 1024
    if (archive.is_symlink() or not archive.is_file() or not 0 < archive.stat().st_size <= maximum):
        raise ValueError('workspace archive is missing, unsafe or too large')
    if destination.is_symlink() or not destination.is_dir() or any(destination.iterdir()):
        raise ValueError('workspace archive requires an empty real extraction directory')
    with archive.open('rb') as stream:
        magic = stream.read(4)
    if magic == b'\x28\xb5\x2f\xfd':
        compact = True
        validate_zstd_frame(archive, maximum + tarfile.RECORDSIZE)
    elif magic[:2] == b'\x1f\x8b':
        compact = False
    else:
        raise ValueError('unsupported workspace archive compression')
    with decompression_stream(archive, compact) as stream:
        header = read_header(stream, 'manifest.json')
        if not 0 < header.size <= MANIFEST_BYTES:
            raise ValueError('cache manifest exceeds its byte limit')
        metadata = read_exact(stream, header.size)
        manifest = archive_manifest(metadata, max_bytes)
        if compact != ('archive' in manifest):
            raise ValueError('workspace archive compression and manifest format differ')
        aliases = archive_aliases(manifest) if compact else {}
        read_padding(stream, header.size)
        consumed = tarfile.BLOCKSIZE + header.size + (-header.size % tarfile.BLOCKSIZE)
        (destination / 'manifest.json').write_bytes(metadata)
        for index, name in enumerate(sorted(manifest['files'])):
            expected = manifest['files'][name]
            path = units._cache_path(destination / 'target', name)
            path.parent.mkdir(parents=True, exist_ok=True)
            if name in aliases:
                # Canonical paths precede aliases and were already checksummed.
                canonical = units._cache_path(destination / 'target', aliases[name])
                os.link(canonical, path, follow_symlinks=False)
                continue
            header = read_header(stream, f'files/{index:06d}')
            if header.size != expected['bytes']:
                raise ValueError('workspace archive member differs from the bounded manifest size')
            remaining = header.size
            digest = hashlib.sha256() if compact else None
            with path.open('xb') as output:
                while remaining:
                    value = read_exact(stream, min(remaining, 1024 ** 2))
                    output.write(value)
                    if digest is not None:
                        digest.update(value)
                    remaining -= len(value)
            if digest is not None and digest.hexdigest() != expected['sha256']:
                raise ValueError('compact archive payload checksum mismatch')
            path.chmod(expected['mode'])
            if compact:
                os.utime(path, ns=(expected['mtime_ns'], expected['mtime_ns']))
            read_padding(stream, header.size)
            consumed += tarfile.BLOCKSIZE + header.size + (-header.size % tarfile.BLOCKSIZE)
        # USTAR termination is two zero blocks, padded to one 10 KiB record.
        # Those two blocks can straddle a record boundary: the exact ending
        # then occupies 21 blocks (10,752 bytes), not at most one record.
        # Also consume the codec trailer; truncated or concatenated payloads
        # cannot reach Cargo validation with a partial archive.
        zero_blocks = 2 * tarfile.BLOCKSIZE
        ending = read_exact(stream, zero_blocks + (-(consumed + zero_blocks) % tarfile.RECORDSIZE))
        if any(ending) or stream.read(1):
            raise ValueError('invalid or oversized workspace archive termination')
        for name in sorted(manifest['directories'], key=lambda name: len(Path(name).parts)):
            units._cache_path(destination / 'target', name).mkdir(parents=True, exist_ok=True)
        for name in sorted(manifest['directories'], key=lambda name: len(Path(name).parts), reverse=True):
            path, expected = units._cache_path(destination / 'target', name), manifest['directories'][name]
            path.chmod(expected['mode'])
            if compact:
                os.utime(path, ns=(expected['mtime_ns'], expected['mtime_ns']))
    return dict(archive_statistics(manifest, compact), archive_bytes=archive.stat().st_size,
                unpack_seconds=round(time.monotonic() - started, 3))


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
