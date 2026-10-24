#!/usr/bin/env python3
"""Compact cache regressions without a network or a repository Rust build.

Trace refs: O7-REQ-d9292810-5229-417f-9fc8-262ee04759c7.
"""
import copy
import importlib.util
import io
import json
import os
from pathlib import Path
import random
import shutil
import subprocess
import tarfile
import tempfile
import time
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('cache_archive', Path(__file__).with_name('ci-cache-archive.py'))
codec = importlib.util.module_from_spec(spec)
spec.loader.exec_module(codec)
CANONICAL = 'debug/deps/libtiny-1234567890abcdef.rlib'
ALIAS = 'debug/libtiny.rlib'
COPY = 'debug/standalone-copy.rlib'
STAMP = 1_700_000_000_000_000_000


def fixture(base, payload=None):
    staged = base / 'staged'
    target = staged / 'target'
    canonical = target / CANONICAL
    canonical.parent.mkdir(parents=True)
    canonical.write_bytes(payload if payload is not None else b'cargo unit symbols\0' * 2048)
    os.link(canonical, target / ALIAS)
    shutil.copyfile(canonical, target / COPY)
    fingerprint = target / 'debug/.fingerprint/tiny-1234567890abcdef/lib-tiny'
    fingerprint.parent.mkdir(parents=True)
    fingerprint.write_bytes(b'complete fingerprint\n')
    for path in target.rglob('*'):
        path.chmod(0o750 if path.is_dir() else 0o640)
        os.utime(path, ns=(STAMP, STAMP))
    files, directories = codec.units._cache_walk(target)
    manifest = dict(schema_version=codec.units.CACHE_SCHEMA, checkout='/fixture', target='/fixture/target',
                    inputs={}, files={name: codec.units._cache_file(target / name, info)
                                     for name, info in files.items()},
                    directories={name: dict(mode=0o750, mtime_ns=STAMP) for name in directories})
    (staged / 'manifest.json').write_text(json.dumps(manifest, sort_keys=True) + '\n')
    return staged, manifest


def zstd(data, *options):
    return subprocess.run(['zstd', '-q', '-3', '-T1', '--zstd=wlog=23', '--check', '-c', *options],
                          input=data, capture_output=True, check=True).stdout


def tar_bytes(staged, manifest, first_header=None, ending=True):
    output = io.BytesIO()
    metadata = manifest if isinstance(manifest, bytes) else json.dumps(manifest, sort_keys=True).encode()
    header = tarfile.TarInfo('manifest.json')
    header.size = len(metadata)
    output.write(header.tobuf(tarfile.USTAR_FORMAT))
    output.write(metadata + b'\0' * (-len(metadata) % tarfile.BLOCKSIZE))
    if first_header is not None:
        output.write(first_header.tobuf(tarfile.USTAR_FORMAT))
    elif isinstance(manifest, dict):
        aliases = manifest.get('archive', {}).get('aliases', {})
        for index, name in enumerate(sorted(manifest['files'])):
            if name in aliases:
                continue
            data = (staged / 'target' / name).read_bytes()
            header = tarfile.TarInfo(f'files/{index:06d}')
            header.size = len(data)
            output.write(header.tobuf(tarfile.USTAR_FORMAT))
            output.write(data + b'\0' * (-len(data) % tarfile.BLOCKSIZE))
    if ending:
        output.write(b'\0' * (2 * tarfile.BLOCKSIZE))
        output.write(b'\0' * (-output.tell() % tarfile.RECORDSIZE))
    return output.getvalue()


@unittest.skipUnless(shutil.which('zstd'), 'compact cache archives require the runner zstd CLI')
class CompactArchiveTests(unittest.TestCase):
    def test_compact_roundtrip_restores_only_original_hardlinks_and_uses_regular_members(self):
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory)
            staged, manifest = fixture(base)
            archive = base / codec.COMPACT_ARCHIVE_NAME
            packed = codec.pack_archive(staged, archive, compact=True)
            destination = base / 'unpacked'
            destination.mkdir()
            unpacked = codec.unpack_archive(archive, destination)
            self.assertTrue((destination / 'target' / ALIAS).samefile(destination / 'target' / CANONICAL))
            self.assertFalse((destination / 'target' / COPY).samefile(destination / 'target' / CANONICAL))
            for name, expected in manifest['files'].items():
                path = destination / 'target' / name
                self.assertEqual(codec.units._cache_file(path), expected)
            raw = subprocess.check_output(['zstd', '-q', '-d', '-c', str(archive)])
            with tarfile.open(fileobj=io.BytesIO(raw), mode='r:') as stream:
                members = stream.getmembers()
                self.assertTrue(all(member.type == tarfile.REGTYPE and not member.linkname for member in members))
                self.assertEqual(len(members), len(manifest['files']))
            logical = sum(item['bytes'] for item in manifest['files'].values())
            self.assertEqual(packed['archive_codec'], 'zstd')
            self.assertEqual(packed['archive_alias_count'], 1)
            self.assertEqual(packed['archive_deduplicated_bytes'], manifest['files'][ALIAS]['bytes'])
            self.assertEqual(packed['archive_payload_bytes'], logical - manifest['files'][ALIAS]['bytes'])
            self.assertEqual(unpacked['archive_payload_bytes'], packed['archive_payload_bytes'])

    def test_same_staging_comparison_reverses_flat_gzip_size_regression(self):
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory)
            staged, manifest = fixture(base, random.Random(20261023).randbytes(2 * 1024 ** 2))
            legacy = codec.pack_archive(staged, base / codec.ARCHIVE_NAME)
            compact = codec.pack_archive(staged, base / codec.COMPACT_ARCHIVE_NAME, compact=True)
            self.assertLess(compact['archive_bytes'], legacy['archive_bytes'] * 0.60)
            self.assertEqual(compact['archive_deduplicated_bytes'], manifest['files'][ALIAS]['bytes'])
            self.assertEqual(compact['archive_payload_bytes'] + compact['archive_deduplicated_bytes'],
                             legacy['archive_payload_bytes'])

    def test_legacy_gzip_default_remains_readable_without_aliases(self):
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory)
            staged, manifest = fixture(base)
            archive = base / codec.ARCHIVE_NAME
            report = codec.pack_archive(staged, archive)
            self.assertEqual(archive.read_bytes()[:2], b'\x1f\x8b')
            self.assertEqual(report['archive_codec'], 'gzip')
            self.assertEqual(report['archive_alias_count'], 0)
            destination = base / 'unpacked'
            destination.mkdir()
            codec.unpack_archive(archive, destination)
            self.assertFalse((destination / 'target' / ALIAS).samefile(destination / 'target' / CANONICAL))
            self.assertEqual(json.loads((destination / 'manifest.json').read_text()), manifest)

    def test_payload_at_last_record_block_accepts_full_ustar_termination(self):
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory)
            staged, manifest = fixture(base, b'X' * 5120)
            path = staged / 'target' / COPY
            path.write_bytes(b'C')
            os.utime(path, ns=(STAMP, STAMP))
            manifest['files'][COPY] = codec.units._cache_file(path)
            (staged / 'manifest.json').write_text(json.dumps(manifest, sort_keys=True) + '\n')
            archive = base / codec.COMPACT_ARCHIVE_NAME
            codec.pack_archive(staged, archive, compact=True)
            raw = subprocess.check_output(['zstd', '-q', '-d', '-c', str(archive)])
            with tarfile.open(fileobj=io.BytesIO(raw), mode='r:') as stream:
                last = stream.getmembers()[-1]
                payload_end = last.offset_data + last.size + (-last.size % tarfile.BLOCKSIZE)
            # The two required zero blocks straddle a record boundary, followed
            # by nineteen padding blocks. This layout occurred in real Cargo.
            self.assertEqual(len(raw) - payload_end, tarfile.RECORDSIZE + tarfile.BLOCKSIZE)
            destination = base / 'unpacked'
            destination.mkdir()
            codec.unpack_archive(archive, destination)

    def test_known_small_content_size_and_rle_block_frames_remain_readable(self):
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory)
            staged, manifest = fixture(base, b'small')
            manifest['archive'] = {'version': 1, 'aliases': {ALIAS: CANONICAL}}
            raw = tar_bytes(staged, manifest)
            sized = zstd(raw, '--stream-size=' + str(len(raw)))
            self.assertEqual(sized[4] >> 6, 1)  # Two-byte FCS stores the size minus 256.
            prefix = raw[:-1024]
            # A legal RLE block stores one byte even though its expanded size is
            # 1024. Reuse the encoder's XXH64 checksum for the identical stream.
            rle = (b'\x28\xb5\x2f\xfd\x04\x58' + (len(prefix) << 3).to_bytes(3, 'little')
                   + prefix + ((1024 << 3) | 3).to_bytes(3, 'little') + b'\0' + sized[-4:])
            for index, value in enumerate((sized, rle)):
                archive = base / codec.COMPACT_ARCHIVE_NAME
                archive.write_bytes(value)
                destination = base / str(index)
                destination.mkdir()
                codec.unpack_archive(archive, destination)
                self.assertTrue((destination / 'target' / ALIAS).samefile(destination / 'target' / CANONICAL))

    def test_aliases_count_against_full_logical_staging_bound(self):
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory)
            staged, manifest = fixture(base)
            logical = sum(item['bytes'] for item in manifest['files'].values())
            archive = base / codec.COMPACT_ARCHIVE_NAME
            with self.assertRaises(ValueError):
                codec.pack_archive(staged, archive, max_bytes=logical - 1, compact=True)
            self.assertFalse(archive.exists())
            codec.pack_archive(staged, archive, max_bytes=logical, compact=True)
            destination = base / 'unpacked'
            destination.mkdir()
            with self.assertRaises(ValueError):
                codec.unpack_archive(archive, destination, max_bytes=logical - 1)
            self.assertFalse((destination / 'target').exists())

    def test_malformed_aliases_are_rejected_before_any_payload_is_written(self):
        bad_envelopes = [
            {'version': True, 'aliases': {}}, {'version': 2, 'aliases': {}},
            {'version': 1, 'aliases': {}, 'unknown': 1}, {'version': 1, 'aliases': []},
            {'version': 1, 'aliases': {ALIAS: ALIAS}},
            {'version': 1, 'aliases': {ALIAS: 'missing'}},
            {'version': 1, 'aliases': {'../escape': CANONICAL}},
            {'version': 1, 'aliases': {ALIAS: '../escape'}},
            {'version': 1, 'aliases': {ALIAS: '/absolute'}},
            {'version': 1, 'aliases': {ALIAS: COPY}},
            {'version': 1, 'aliases': {ALIAS: CANONICAL, COPY: ALIAS}},
            {'version': 1, 'aliases': {ALIAS: COPY, COPY: ALIAS}},
        ]
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory)
            staged, original = fixture(base)
            for index, envelope in enumerate(bad_envelopes):
                with self.subTest(envelope=envelope):
                    manifest = dict(original, archive=envelope)
                    archive = base / codec.COMPACT_ARCHIVE_NAME
                    archive.write_bytes(zstd(tar_bytes(staged, json.dumps(manifest).encode())))
                    destination = base / str(index)
                    destination.mkdir()
                    with self.assertRaises(ValueError):
                        codec.unpack_archive(archive, destination)
                    self.assertFalse((destination / 'target').exists())

    def test_alias_metadata_must_match_size_mode_timestamp_and_digest(self):
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory)
            staged, original = fixture(base)
            for field, value in [('bytes', 1), ('mode', 0o600), ('mtime_ns', STAMP - 1), ('sha256', '0' * 64)]:
                with self.subTest(field=field):
                    manifest = copy.deepcopy(original)
                    manifest['archive'] = {'version': 1, 'aliases': {ALIAS: CANONICAL}}
                    manifest['files'][ALIAS][field] = value
                    archive = base / codec.COMPACT_ARCHIVE_NAME
                    archive.write_bytes(zstd(tar_bytes(staged, json.dumps(manifest).encode())))
                    destination = base / field
                    destination.mkdir()
                    with self.assertRaises(ValueError):
                        codec.unpack_archive(archive, destination)
                    self.assertFalse((destination / 'target').exists())

    def test_out_of_range_and_future_file_or_directory_timestamps_fail_before_extraction(self):
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory)
            staged, original = fixture(base)
            for category in ('files', 'directories'):
                for index, value in enumerate((10 ** 100, time.time_ns() + 10 ** 12)):
                    with self.subTest(category=category, value=value):
                        manifest = copy.deepcopy(original)
                        manifest['archive'] = {'version': 1, 'aliases': {ALIAS: CANONICAL}}
                        if category == 'files':
                            for name in (CANONICAL, ALIAS):
                                manifest[category][name]['mtime_ns'] = value
                        else:
                            next(iter(manifest[category].values()))['mtime_ns'] = value
                        archive = base / codec.COMPACT_ARCHIVE_NAME
                        archive.write_bytes(zstd(tar_bytes(staged, manifest)))
                        destination = base / (category + str(index))
                        destination.mkdir()
                        with self.assertRaises(ValueError):
                            codec.unpack_archive(archive, destination)
                        self.assertFalse((destination / 'target').exists())

    def test_unsafe_headers_and_unbounded_member_sizes_are_rejected(self):
        kinds = [tarfile.SYMTYPE, tarfile.LNKTYPE, tarfile.CHRTYPE, tarfile.GNUTYPE_SPARSE,
                 tarfile.XHDTYPE, tarfile.GNUTYPE_LONGNAME]
        cases = [(kind, 'files/000000') for kind in kinds]
        cases += [(tarfile.REGTYPE, name) for name in ('../escape', '/absolute', 'manifest.json', 'files/000000')]
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory)
            staged, original = fixture(base)
            manifest = dict(original, archive={'version': 1, 'aliases': {ALIAS: CANONICAL}})
            for index, (kind, name) in enumerate(cases):
                with self.subTest(kind=kind, name=name):
                    header = tarfile.TarInfo(name)
                    header.type, header.size = kind, 1024 ** 3
                    archive = base / codec.COMPACT_ARCHIVE_NAME
                    archive.write_bytes(zstd(tar_bytes(staged, manifest, first_header=header)))
                    destination = base / str(index)
                    destination.mkdir()
                    with self.assertRaises(ValueError):
                        codec.unpack_archive(archive, destination)
            self.assertFalse((base / 'escape').exists())

    def test_duplicate_manifest_fields_invalid_digest_and_file_count_are_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory)
            staged, original = fixture(base)
            metadata_cases = [b'{"files":{},"files":{}}', b'[' * 2000 + b']' * 2000]
            for field, value in [('sha256', 'not-a-hash'), ('mtime_ns', True), ('mode', 0o4777)]:
                manifest = copy.deepcopy(original)
                manifest['files'][CANONICAL][field] = value
                manifest['archive'] = {'version': 1, 'aliases': {}}
                metadata_cases.append(json.dumps(manifest).encode())
            for index, metadata in enumerate(metadata_cases):
                with self.subTest(index=index):
                    archive = base / codec.COMPACT_ARCHIVE_NAME
                    archive.write_bytes(zstd(tar_bytes(staged, metadata)))
                    destination = base / str(index)
                    destination.mkdir()
                    with self.assertRaises(ValueError):
                        codec.unpack_archive(archive, destination)
            with patch.object(codec, 'MAX_FILES', 1), self.assertRaises(ValueError):
                codec.archive_manifest(json.dumps(original).encode())

    def test_compact_checksum_mismatch_never_creates_alias(self):
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory)
            staged, original = fixture(base)
            manifest = dict(original, archive={'version': 1, 'aliases': {ALIAS: CANONICAL}})
            path = staged / 'target' / CANONICAL
            data = path.read_bytes()
            path.write_bytes(b'X' + data[1:])
            archive = base / codec.COMPACT_ARCHIVE_NAME
            archive.write_bytes(zstd(tar_bytes(staged, manifest)))
            destination = base / 'unpacked'
            destination.mkdir()
            with self.assertRaisesRegex(ValueError, 'checksum'):
                codec.unpack_archive(archive, destination)
            self.assertFalse((destination / 'target' / ALIAS).exists())

    def test_truncation_checksum_and_even_empty_trailing_frames_are_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory)
            staged, _ = fixture(base)
            archive = base / codec.COMPACT_ARCHIVE_NAME
            codec.pack_archive(staged, archive, compact=True)
            original = archive.read_bytes()
            damaged = [original[:-1], original[:-4] + bytes([original[-4] ^ 1]) + original[-3:],
                       original + zstd(b''), original + b'\x50\x2a\x4d\x18\0\0\0\0', original + b'junk']
            for index, value in enumerate(damaged):
                with self.subTest(index=index):
                    archive.write_bytes(value)
                    destination = base / str(index)
                    destination.mkdir()
                    with self.assertRaises((ValueError, OSError, EOFError)):
                        codec.unpack_archive(archive, destination)

    def test_frame_window_content_size_and_header_work_are_bounded(self):
        magic = b'\x28\xb5\x2f\xfd'
        frames = [magic + b'\x04\xff', magic + b'\xe4' + (64 * 1024 ** 2 + 1).to_bytes(8, 'little'),
                  magic + b'\xc4\x58' + (10 * 1024 ** 3).to_bytes(8, 'little'),
                  magic + b'\x04\x58' + b'\0\0\0' * 5 + b'\x01\0\0' + b'\0' * 4,
                  magic + b'\x0c\x58', magic + b'\x05\x58\x01']
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory)
            for index, value in enumerate(frames):
                with self.subTest(index=index):
                    archive = base / codec.COMPACT_ARCHIVE_NAME
                    archive.write_bytes(value)
                    destination = base / str(index)
                    destination.mkdir()
                    with patch.object(codec, 'MAX_ZSTD_BLOCKS', 4), self.assertRaises(ValueError):
                        codec.unpack_archive(archive, destination)
                    self.assertEqual(list(destination.iterdir()), [])

    def test_no_checksum_and_missing_compact_envelope_are_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory)
            staged, manifest = fixture(base)
            values = [zstd(tar_bytes(staged, manifest)), zstd(tar_bytes(staged, manifest), '--no-check')]
            for index, value in enumerate(values):
                archive = base / codec.COMPACT_ARCHIVE_NAME
                archive.write_bytes(value)
                destination = base / str(index)
                destination.mkdir()
                with self.assertRaises(ValueError):
                    codec.unpack_archive(archive, destination)

    def test_parse_failure_reaps_decoder_with_unread_expanded_output(self):
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory)
            staged, manifest = fixture(base)
            manifest['archive'] = {'version': 1, 'aliases': {ALIAS: CANONICAL}}
            archive = base / codec.COMPACT_ARCHIVE_NAME
            archive.write_bytes(zstd(tar_bytes(staged, manifest) + b'\0' * (2 * 1024 ** 2)))
            destination = base / 'unpacked'
            destination.mkdir()
            processes = []
            original = subprocess.Popen

            def spawn(*args, **kwargs):
                process = original(*args, **kwargs)
                processes.append(process)
                return process

            with patch.object(codec.subprocess, 'Popen', side_effect=spawn), self.assertRaises(ValueError):
                codec.unpack_archive(archive, destination)
            self.assertEqual(len(processes), 1)
            self.assertIsNotNone(processes[0].poll())
            self.assertTrue(processes[0].stdout.closed)

    def test_pack_failure_preserves_previous_complete_archive(self):
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory)
            staged, _ = fixture(base)
            archive = base / codec.COMPACT_ARCHIVE_NAME
            codec.pack_archive(staged, archive, compact=True)
            previous = archive.read_bytes()
            (staged / 'target' / CANONICAL).write_bytes(b'truncated')
            with self.assertRaises((OSError, ValueError)):
                codec.pack_archive(staged, archive, compact=True)
            self.assertEqual(archive.read_bytes(), previous)
            self.assertEqual(list(base.glob('workspace-pack-*')), [])

    def test_unpack_requires_an_empty_real_directory(self):
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory)
            staged, _ = fixture(base)
            archive = base / codec.COMPACT_ARCHIVE_NAME
            codec.pack_archive(staged, archive, compact=True)
            outside = base / 'outside'
            outside.mkdir()
            destination = base / 'unpacked'
            destination.mkdir()
            (destination / 'target').symlink_to(outside, target_is_directory=True)
            with self.assertRaises(ValueError):
                codec.unpack_archive(archive, destination)
            self.assertEqual(list(outside.iterdir()), [])


if __name__ == '__main__':
    unittest.main()
