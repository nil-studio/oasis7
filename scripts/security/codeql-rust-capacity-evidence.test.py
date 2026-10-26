#!/usr/bin/env python3
"""Small-boundary evidence regressions; no CodeQL, extractor, or capacity run."""
import contextlib
import gzip
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile
import threading
import time
import unittest
from unittest.mock import patch

import codeql_rust_capacity_evidence as evidence


class EvidenceTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.database = self.root / 'database'
        self.logs = self.database / 'log'
        self.logs.mkdir(parents=True)
        self.out = self.root / 'evidence'
        self.out.mkdir()
        self.source = self.root / 'source'
        self.source.mkdir()
        self.expected = 'a' * 40
        self.deadline = time.monotonic() + 60
        self.stack = contextlib.ExitStack()
        self.addCleanup(self.stack.close)
        self.stack.enter_context(patch.object(evidence.runtime, 'CAP', 128 * 1024))
        self.stack.enter_context(patch.object(evidence.runtime, 'WRITE_CAP', 64 * 1024))

    def capture(self):
        return evidence.collect_logs(self.database, self.out, self.deadline)

    def receipt(self, name='database-log-capture.json'):
        return json.loads((self.out / name).read_text())

    def fake_git(self, responses=None):
        responses = responses or {}
        self.git_calls = []

        def run(argv, cwd, output, end, timeout, root, env, file_limit):
            self.git_calls.append({'argv': argv, 'end': end, 'timeout': timeout,
                                   'env': env, 'cwd': cwd, 'output': output})
            command = ('status' if 'status' in argv else
                       'top_level' if '--show-toplevel' in argv else 'head')
            default = {'head': self.expected.encode() + b'\n', 'status': b'',
                       'top_level': os.fsencode(self.source) + b'\n'}
            response = responses.get(command, {})
            output.write_bytes(response.get('stdout', default[command]))
            output.with_name(output.name + '.stderr').write_bytes(response.get('stderr', b''))
            result = {'exit': response.get('exit', 0), 'failure_kind': response.get('failure_kind'),
                      'error': response.get('error'), 'cleanup_errors': response.get('cleanup_errors', []),
                      'evidence_errors': response.get('evidence_errors', []), 'pid': 12345,
                      'pgid': 12345, 'elapsed': 0.01}
            evidence.runtime.save(output.with_name(output.name + '.result.json'), result, root,
                                  terminal=True)
            if result['error'] or result['cleanup_errors']:
                raise evidence.runtime.PhaseError(result)
            return result
        self.stack.enter_context(patch.object(evidence.runtime, 'run', side_effect=run))

    def test_complete_nested_logs_preserve_exact_raw_bytes(self):
        nested = self.logs / 'rust'
        nested.mkdir()
        original = b'extractor error\x00\xff\r\n'
        (nested / 'extract.log').write_bytes(original)
        (self.logs / 'not-a-log.txt').write_text('unselected')
        report = self.capture()
        self.assertEqual(report['status'], 'COMPLETE')
        self.assertEqual(len(report['files']), 1)
        record = report['files'][0]
        self.assertEqual(record['destination'], 'database-logs/rust/extract.log')
        self.assertEqual((self.out / record['destination']).read_bytes(), original)
        self.assertEqual(record['sha256'], hashlib.sha256(original).hexdigest())
        self.assertEqual(report, self.receipt())

    def test_large_log_is_saved_completely_without_expanding_artifact_budget(self):
        chunk = b'original evaluator log line\n' * 40000
        path = self.logs / 'execute.log'
        raw_hash = hashlib.sha256()
        with path.open('wb') as stream:
            for _ in range(20):
                stream.write(chunk)
                raw_hash.update(chunk)
        with patch.object(evidence.runtime, 'CAP', 8 * 1024**2), \
                patch.object(evidence.runtime, 'WRITE_CAP', 4 * 1024**2):
            report = self.capture()
        self.assertEqual(report['status'], 'COMPLETE')
        record = report['files'][0]
        self.assertEqual(record['encoding'], 'gzip')
        self.assertEqual(record['raw_bytes'], path.stat().st_size)
        self.assertEqual(record['raw_sha256'], raw_hash.hexdigest())
        self.assertLess(record['copied_bytes'], 4 * 1024**2)
        stored = self.out / record['destination']
        self.assertEqual(record['sha256'], hashlib.sha256(stored.read_bytes()).hexdigest())
        decoded = hashlib.sha256()
        with gzip.open(stored, 'rb') as stream:
            while data := stream.read(65536):
                decoded.update(data)
        self.assertEqual(decoded.hexdigest(), raw_hash.hexdigest())
        self.assertEqual(record['omitted_ranges'], [])

    def gzip_artifact(self, raw):
        stored = gzip.compress(raw, mtime=0)
        path = self.out / 'complete.log.gz'
        path.write_bytes(stored)
        record = {'status': 'COMPLETE', 'encoding': 'gzip', 'raw_bytes': len(raw),
                  'raw_sha256': hashlib.sha256(raw).hexdigest(), 'copied_bytes': len(stored),
                  'sha256': hashlib.sha256(stored).hexdigest()}
        return path, record

    def test_gzip_reader_is_streamed_reiterable_and_validates_original_hash(self):
        path, record = self.gzip_artifact(b'first\nsecond\nunterminated')
        text = evidence.CompressedLogText(path, record, self.deadline)
        with patch.object(Path, 'read_bytes', side_effect=AssertionError('whole file read')):
            self.assertEqual(list(text), ['first', 'second'])
            self.assertEqual(list(text), ['first', 'second'])
        record['raw_sha256'] = '0' * 64
        with self.assertRaisesRegex(RuntimeError, 'original length/hash'):
            list(text)

    def test_gzip_reader_rejects_truncation_bad_crc_expansion_and_long_lines(self):
        for problem in ('truncated', 'crc', 'expanded', 'line', 'partial', 'deadline'):
            with self.subTest(problem=problem):
                path, record = self.gzip_artifact(b'first\nsecond\n')
                stored = path.read_bytes()
                if problem in ('truncated', 'crc'):
                    stored = stored[:-2] if problem == 'truncated' else stored[:-8] + bytes([stored[-8] ^ 1]) + stored[-7:]
                    path.write_bytes(stored)
                    record.update(copied_bytes=len(stored), sha256=hashlib.sha256(stored).hexdigest())
                if problem == 'expanded': record['raw_bytes'] = 2
                if problem == 'partial': record['status'] = 'PARTIAL'
                with patch.object(evidence, 'MAX_LOG_LINE_BYTES', 2 if problem == 'line' else 1024):
                    with self.assertRaises((RuntimeError, OSError, EOFError, TimeoutError)):
                        list(evidence.CompressedLogText(path, record, 0 if problem == 'deadline' else self.deadline))

    def test_gzip_stored_limit_includes_header_and_partial_payload(self):
        (self.logs / 'large.log').write_bytes(b'large original bytes\n' * 100)
        with patch.object(evidence, 'COMPRESS_MIN_BYTES', 1), patch.object(evidence, 'MAX_COMPRESSED_FILE_BYTES', 8):
            report = self.capture()
        record = report['files'][0]
        self.assertEqual(report['status'], 'PARTIAL')
        self.assertEqual(record['status'], 'PARTIAL')
        self.assertEqual(record['copied_bytes'], 8)
        self.assertEqual((self.out / record['destination']).stat().st_size, 8)
        self.assertIn('COMPRESSED_SIZE_LIMIT', record['reasons'])
        self.assertEqual(report['copied_bytes'], 8)

    def test_gzip_short_writes_preserve_exact_bytes(self):
        raw = b'bounded log line\n' * 100
        (self.logs / 'large.log').write_bytes(raw)
        write = os.write
        with patch.object(evidence, 'COMPRESS_MIN_BYTES', 1), \
                patch.object(evidence.os, 'write', side_effect=lambda fd, data: write(fd, data[:3])):
            report = self.capture()
        self.assertEqual(report['status'], 'COMPLETE')
        record = report['files'][0]
        self.assertEqual(gzip.decompress((self.out / record['destination']).read_bytes()), raw)

    def test_gzip_reader_rejects_symlink_ancestors(self):
        path, record = self.gzip_artifact(b'original\n')
        alias = self.root / 'alias'; alias.symlink_to(self.out, target_is_directory=True)
        with self.assertRaises(OSError):
            list(evidence.CompressedLogText(alias / path.name, record, self.deadline))

    def test_gzip_copy_does_not_certify_a_replaced_source_directory(self):
        nested = self.logs / 'rust'; nested.mkdir()
        (nested / 'large.log').write_bytes(b'original data\n' * 100)
        read = os.read
        changed = False
        def replace(fd, count):
            nonlocal changed
            data = read(fd, count)
            if not changed:
                changed = True
                nested.rename(self.logs / 'old-rust')
                nested.mkdir()
                (nested / 'large.log').write_bytes(b'replaced')
            return data
        with patch.object(evidence, 'COMPRESS_MIN_BYTES', 1), patch.object(evidence.os, 'read', side_effect=replace):
            report = self.capture()
        self.assertEqual(report['status'], 'PARTIAL')
        record = next(item for item in report['files'] if item['source'] == 'rust/large.log')
        self.assertEqual(record['status'], 'PARTIAL')
        self.assertIn('source log parent changed', record['error'])

    def test_raw_read_budget_is_separate_from_stored_byte_budget(self):
        (self.logs / 'first.log').write_bytes(b'12345678')
        with patch.object(evidence, 'MAX_RAW_CAPTURE_BYTES', 4):
            report = self.capture()
        self.assertEqual(report['status'], 'PARTIAL')
        self.assertEqual(report['raw_bytes_read'], 4)
        self.assertIn('RAW_CAPTURE_BYTE_LIMIT', report['reasons'])

    def test_lock_inventory_observes_absence_and_ignored_files_without_git_or_cargo(self):
        vendor = self.source / 'vendor'; vendor.mkdir()
        (self.source / '.gitignore').write_text('Cargo.lock\n')
        (self.source / 'Cargo.lock').write_bytes(b'root lock')
        initial = evidence.lock_inventory(self.source, ['Cargo.toml', 'vendor/Cargo.toml'], self.out,
                                          self.deadline, self.expected, stage='initial', fixed_deadline=123)
        self.assertEqual(initial['status'], 'COMPLETE')
        self.assertEqual([x['status'] for x in initial['locks']], ['CAPTURED', 'ABSENT'])
        (vendor / 'Cargo.lock').write_bytes(b'new ignored lock')
        later = evidence.lock_inventory(self.source, ['Cargo.toml', 'vendor/Cargo.toml'], self.out,
                                        self.deadline, self.expected, stage='later', fixed_deadline=123)
        self.assertEqual(later['locks'][1]['status'], 'CAPTURED')
        self.assertEqual(later['dependency_resolution_impact'], 'UNKNOWN')
        self.fake_git({'status': {'stdout': b'?? vendor/Cargo.lock\n'}})
        report = evidence.capture_identity(self.source, self.out, self.deadline, self.expected, initial_locks=initial)
        self.assertEqual(report['untracked_locks'][0]['initial_presence'], 'ABSENT')
        self.assertEqual(report['clean_gate'], 'FAIL')
        self.assertEqual(report['untracked_locks'][0]['origin'], 'UNKNOWN')

    def test_lock_inventory_never_calls_unsafe_or_missing_parent_absent(self):
        (self.source / 'Cargo.lock').symlink_to(self.root / 'foreign')
        for manifests, stage in [(['Cargo.toml'], 'symlink'), (['missing/Cargo.toml'], 'missing-parent')]:
            report = evidence.lock_inventory(self.source, manifests, self.out, self.deadline,
                                             self.expected, stage=stage, fixed_deadline=123)
            self.assertEqual(report['status'], 'PARTIAL')
            self.assertEqual(report['locks'][0]['status'], 'UNKNOWN')

    def test_oversize_log_keeps_head_tail_and_later_small_log(self):
        (self.logs / 'large.log').write_bytes(b'abcdefgh')
        (self.logs / 'small.log').write_bytes(b'abc')
        with patch.object(evidence, 'MAX_FILE_BYTES', 4):
            report = self.capture()
        self.assertEqual(report['status'], 'PARTIAL')
        records = {item['source']: item for item in report['files']}
        self.assertIn('FILE_SIZE_LIMIT', records['large.log']['reasons'])
        self.assertEqual((self.out / 'database-logs/large.log').read_bytes(), b'abgh')
        self.assertEqual(records['large.log']['omitted_ranges'], [{'offset': 2, 'length': 4}])
        self.assertEqual([x['source_offset'] for x in records['large.log']['source_ranges']], [0, 6])
        self.assertEqual(records['small.log']['status'], 'COMPLETE')
        for segment in records['large.log']['source_ranges']:
            value=(self.out / 'database-logs/large.log').read_bytes()[segment['destination_offset']:segment['destination_offset']+segment['length']]
            self.assertEqual(segment['sha256'],hashlib.sha256(value).hexdigest())
        self.assertEqual(report, self.receipt())

    def test_untracked_lock_is_described_but_not_admitted(self):
        lock = self.source / 'vendor/Cargo.lock'
        lock.parent.mkdir()
        lock.write_bytes(b'version = 4\n')
        self.fake_git({'status': {'stdout': b'?? vendor/Cargo.lock\n'}})
        report = evidence.capture_identity(self.source, self.out, self.deadline, self.expected)
        self.assertEqual(report['clean_gate'], 'FAIL')
        self.assertEqual(report['tracked_status_gate'], 'PASS')
        self.assertEqual(report['untracked_locks'][0]['sha256'], hashlib.sha256(lock.read_bytes()).hexdigest())
        self.assertEqual(report['untracked_locks'][0]['dependency_resolution_impact'], 'UNKNOWN')

    def test_lock_symlink_and_tracked_change_still_fail(self):
        external=self.root/'external';external.write_text('private')
        vendor=self.source/'vendor';vendor.mkdir()
        (vendor/'Cargo.lock').symlink_to(external)
        self.fake_git({'status': {'stdout': b' M tracked.rs\n?? vendor/Cargo.lock\n'}})
        report=evidence.capture_identity(self.source,self.out,self.deadline,self.expected)
        self.assertEqual(report['tracked_status_gate'],'FAIL')
        self.assertEqual(report['clean_gate'],'FAIL')
        self.assertEqual(report['untracked_locks'][0]['status'],'UNKNOWN')
        self.assertNotIn('sha256',report['untracked_locks'][0])

    def settled_capture(self, change=None, deadline=60):
        clock=[0.0]
        def sleep(seconds):
            clock[0]+=seconds
            if change:change(clock[0])
        with patch.object(evidence.time,'monotonic',side_effect=lambda:clock[0]), \
                patch.object(evidence.time,'sleep',side_effect=sleep):
            return evidence.collect_logs(self.database,self.out,deadline,settle=True)

    def test_settlement_includes_delayed_tail_before_copy(self):
        path=self.logs/'execute.log';path.write_bytes(b'first')
        def append(now):
            if now<0.3:path.write_bytes(b'first tail')
        report=self.settled_capture(append)
        self.assertEqual(report['settlement']['status'],'STABLE_WINDOW')
        self.assertEqual(report['status'],'COMPLETE')
        self.assertEqual((self.out/'database-logs/execute.log').read_bytes(),b'first tail')
        self.assertEqual(report['producer_cleanup'],'NOT_OBSERVED_BY_THIS_HELPER')

    def test_real_delayed_writer_tail_is_captured(self):
        path=self.logs/'execute.log';path.write_bytes(b'first')
        def write_tail():
            time.sleep(0.1)
            with path.open('ab') as stream:stream.write(b' tail')
        writer=threading.Thread(target=write_tail)
        writer.start()
        try:
            report=evidence.collect_logs(self.database,self.out,self.deadline,settle=True)
        finally:
            writer.join(timeout=2)
        self.assertFalse(writer.is_alive())
        self.assertEqual(report['status'],'COMPLETE')
        self.assertEqual((self.out/'database-logs/execute.log').read_bytes(),b'first tail')

    def test_continuing_writer_remains_partial_but_keeps_bytes(self):
        path=self.logs/'execute.log';path.write_bytes(b'first')
        def append(now):
            with path.open('ab') as stream:stream.write(b'x')
        report=self.settled_capture(append)
        self.assertEqual(report['status'],'PARTIAL')
        self.assertIn('LOG_STABILITY_NOT_OBSERVED',report['reasons'])
        self.assertTrue((self.out/'database-logs/execute.log').read_bytes().startswith(b'first'))
        self.assertLessEqual(report['settlement']['elapsed_seconds'],5.001)

    def test_settlement_replacement_is_partial_without_following_old_inode(self):
        path=self.logs/'execute.log';path.write_bytes(b'old')
        replaced=[]
        def replace(now):
            if not replaced:
                path.rename(self.logs/'old.txt')
                path.write_bytes(b'new')
                replaced.append(True)
        report=self.settled_capture(replace)
        self.assertEqual(report['status'],'PARTIAL')
        self.assertIn('path replaced',report['settlement']['reason'])
        self.assertEqual((self.out/'database-logs/execute.log').read_bytes(),b'new')

    def test_insufficient_settlement_budget_preserves_available_snapshot(self):
        (self.logs/'execute.log').write_bytes(b'available')
        report=self.settled_capture(deadline=0.5)
        self.assertEqual(report['status'],'PARTIAL')
        self.assertIn('insufficient settlement budget',report['settlement']['reason'])
        self.assertEqual((self.out/'database-logs/execute.log').read_bytes(),b'available')

    def test_empty_final_sample_cannot_admit_after_settlement_deadline(self):
        clock=[0.0];calls=[];real_open=evidence._open_directory
        def late_open(path):
            calls.append(path)
            if len(calls)==2:clock[0]=6.0
            return real_open(path)
        def sleep(seconds):clock[0]+=seconds
        with patch.object(evidence.time,'monotonic',side_effect=lambda:clock[0]), \
                patch.object(evidence.time,'sleep',side_effect=sleep), \
                patch.object(evidence,'_open_directory',side_effect=late_open):
            report=evidence._settle_logs(self.database,60)
        self.assertEqual(report['status'],'PARTIAL')
        self.assertIn('deadline',report['reason'])

    def test_settlement_scan_limit_still_collects_bounded_bytes(self):
        (self.logs/'execute.log').write_bytes(b'available')
        with patch.object(evidence,'MAX_SCAN_ENTRIES',2):
            report=self.settled_capture()
        self.assertEqual(report['status'],'PARTIAL')
        self.assertIn('settlement scan limit',report['settlement']['reason'])
        self.assertEqual((self.out/'database-logs/execute.log').read_bytes(),b'available')

    def test_quiet_window_does_not_hide_growth_during_copy(self):
        source=self.logs/'execute.log';source.write_bytes(b'initial')
        real_read=os.read
        def grow(fd,size):
            data=real_read(fd,size)
            with source.open('ab') as writer:writer.write(b'tail')
            return data
        with patch.object(evidence.os,'read',side_effect=grow):
            report=self.settled_capture()
        self.assertEqual(report['settlement']['status'],'STABLE_WINDOW')
        self.assertEqual(report['status'],'PARTIAL')
        self.assertIn('SOURCE_CHANGED_DURING_CAPTURE',report['files'][0]['reasons'])

    def test_growing_file_captures_only_initial_length_and_marks_change(self):
        source = self.logs / 'live.log'
        source.write_bytes(b'original')
        real_read = os.read
        appended = False

        def append_after_read(fd, size):
            nonlocal appended
            chunk = real_read(fd, size)
            if not appended:
                appended = True
                with source.open('ab') as writer:
                    writer.write(b' later failure')
            return chunk
        with patch.object(evidence.os, 'read', side_effect=append_after_read):
            report = self.capture()
        record = report['files'][0]
        self.assertEqual(report['status'], 'PARTIAL')
        self.assertIn('SOURCE_CHANGED_DURING_CAPTURE', record['reasons'])
        self.assertEqual(record['initial']['bytes'], 8)
        self.assertEqual(record['copied_bytes'], 8)
        self.assertEqual((self.out / record['destination']).read_bytes(), b'original')

    def test_read_error_keeps_preceding_original_bytes(self):
        (self.logs / 'broken.log').write_bytes(b'abcdefgh')
        real_read = os.read
        reads = 0

        def fail_second_read(fd, size):
            nonlocal reads
            reads += 1
            if reads == 2:
                raise OSError('injected read failure')
            return real_read(fd, size)
        with patch.object(evidence, 'CHUNK_BYTES', 4), patch.object(evidence.os, 'read', side_effect=fail_second_read):
            report = self.capture()
        record = report['files'][0]
        self.assertEqual(record['copied_bytes'], 4)
        self.assertIn('injected read failure', record['error'])
        self.assertEqual((self.out / record['destination']).read_bytes(), b'abcd')
        self.assertEqual(record['sha256'], hashlib.sha256(b'abcd').hexdigest())
        self.assertEqual(report, self.receipt())

    def test_partial_write_error_keeps_exact_written_prefix(self):
        (self.logs / 'broken.log').write_bytes(b'abcdefgh')
        real_write = os.write
        writes = 0

        def fail_after_short_write(fd, data):
            nonlocal writes
            writes += 1
            if writes == 1:
                return real_write(fd, data[:2])
            if writes == 2:
                raise OSError('injected partial write')
            return real_write(fd, data)
        with patch.object(evidence.os, 'write', side_effect=fail_after_short_write):
            report = self.capture()
        record = report['files'][0]
        self.assertEqual(record['status'], 'PARTIAL')
        self.assertEqual(record['copied_bytes'], 2)
        self.assertIn('injected partial write', record['error'])
        self.assertEqual((self.out / record['destination']).read_bytes(), b'ab')
        self.assertEqual(record['sha256'], hashlib.sha256(b'ab').hexdigest())
        self.assertEqual(report, self.receipt())

    def test_symlink_directories_files_and_special_logs_are_never_followed(self):
        external = self.root / 'external'
        external.mkdir()
        (external / 'private.log').write_bytes(b'not evidence')
        (self.logs / 'linked-directory').symlink_to(external, target_is_directory=True)
        (self.logs / 'linked.log').symlink_to(external / 'private.log')
        os.mkfifo(self.logs / 'pipe.log')
        (self.logs / 'real.log').write_bytes(b'actual failure')
        report = self.capture()
        self.assertEqual(report['status'], 'PARTIAL')
        self.assertIn('SYMLINK_SKIPPED', report['reasons'])
        self.assertIn('NON_REGULAR_LOG_SKIPPED', report['reasons'])
        self.assertEqual([item['source'] for item in report['files']], ['real.log'])
        self.assertFalse((self.out / 'database-logs/linked-directory').exists())

    def test_symlink_ancestor_rejects_capture(self):
        linked_database = self.root / 'linked-db'
        linked_database.symlink_to(self.database, target_is_directory=True)
        (self.logs / 'private.log').write_bytes(b'not evidence')
        report = evidence.collect_logs(linked_database, self.out, self.deadline)
        self.assertEqual(report['status'], 'PARTIAL')
        self.assertEqual(report['copied_bytes'], 0)
        self.assertEqual(report['files'], [])
        self.assertEqual(report, self.receipt())

    def test_existing_destination_is_not_overwritten_or_deleted(self):
        destination = self.out / 'database-logs'
        destination.mkdir()
        (destination / 'analysis.log').write_bytes(b'previous raw bytes')
        (self.logs / 'analysis.log').write_bytes(b'new raw bytes')
        report = self.capture()
        self.assertEqual(report['status'], 'PARTIAL')
        self.assertIn('LOG_CAPTURE_ERROR', report['files'][0]['reasons'])
        self.assertFalse(report['files'][0]['artifact_created'])
        self.assertEqual((destination / 'analysis.log').read_bytes(), b'previous raw bytes')

    def test_unsafe_output_tree_does_not_write_through_symlink(self):
        external = self.root / 'external'
        external.mkdir()
        (self.out / 'database-logs').symlink_to(external, target_is_directory=True)
        (self.logs / 'failure.log').write_bytes(b'raw failure')
        report = self.capture()
        self.assertEqual(report['status'], 'PARTIAL')
        self.assertEqual(report['copied_bytes'], 0)
        self.assertEqual(list(external.iterdir()), [])
        self.assertIn('receipt_error', report)

    def test_deadline_exhausted_starts_no_log_read_but_writes_terminal(self):
        (self.logs / 'late.log').write_bytes(b'raw failure')
        with patch.object(evidence.os, 'read', side_effect=AssertionError('unexpected read')):
            report = evidence.collect_logs(self.database, self.out, time.monotonic() - 1)
        self.assertEqual(report['status'], 'NOT_REACHED')
        self.assertIn('DEADLINE_EXHAUSTED', report['reasons'])
        self.assertEqual(report['copied_bytes'], 0)
        self.assertEqual(report, self.receipt())

    def test_deadline_during_copy_preserves_prefix_and_stops(self):
        (self.logs / 'partial.log').write_bytes(b'abcdefgh')
        real_read = os.read
        now = [0.0]

        def expire_after_read(fd, size):
            data = real_read(fd, size)
            now[0] = 2.0
            return data
        with patch.object(evidence, 'CHUNK_BYTES', 4), patch.object(evidence.time, 'monotonic', side_effect=lambda: now[0]), patch.object(evidence.os, 'read', side_effect=expire_after_read):
            report = evidence.collect_logs(self.database, self.out, 1.0)
        self.assertEqual(report['status'], 'PARTIAL')
        self.assertIn('DEADLINE_EXHAUSTED', report['reasons'])
        self.assertEqual((self.out / 'database-logs/partial.log').read_bytes(), b'abcd')
        self.assertEqual(report, self.receipt())

    def test_full_normal_cap_still_has_terminal_reserve(self):
        (self.out / 'already-captured.raw').write_bytes(b'x' * 32)
        (self.logs / 'failure.log').write_bytes(b'raw failure')
        with patch.object(evidence.runtime, 'WRITE_CAP', 32):
            report = self.capture()
        self.assertEqual(report['status'], 'NOT_REACHED')
        self.assertIn('CAPTURE_BYTE_LIMIT', report['reasons'])
        self.assertEqual((self.out / 'already-captured.raw').read_bytes(), b'x' * 32)
        self.assertEqual(report, self.receipt())

    def test_capture_fills_normal_cap_without_losing_terminal_or_prefix(self):
        (self.logs / 'failure.log').write_bytes(b'abcdefgh')
        with patch.object(evidence.runtime, 'WRITE_CAP', 5):
            report = self.capture()
        self.assertEqual(report['status'], 'PARTIAL')
        self.assertEqual(report['copied_bytes'], 5)
        self.assertEqual((self.out / 'database-logs/failure.log').read_bytes(), b'abcgh')
        self.assertEqual(report, self.receipt())

    def test_file_and_scan_count_limits_are_reported(self):
        for name in ('one.log', 'two.log', 'three.log'):
            (self.logs / name).write_bytes(b'raw')
        with patch.object(evidence, 'MAX_LOG_FILES', 1):
            report = self.capture()
        self.assertEqual(len(report['files']), 1)
        self.assertIn('LOG_FILE_LIMIT', report['reasons'])
        other = self.root / 'other-evidence'
        other.mkdir()
        with patch.object(evidence, 'MAX_SCAN_ENTRIES', 1):
            report = evidence.collect_logs(self.database, other, self.deadline)
        self.assertIn('SCAN_ENTRY_LIMIT', report['reasons'])
        self.assertTrue((other / 'database-log-capture.json').is_file())

    def test_terminal_write_failure_is_reported_and_raw_bytes_remain(self):
        (self.logs / 'failure.log').write_bytes(b'original failure')
        with patch.object(evidence.runtime, 'CAP', 16):
            report = self.capture()
        self.assertEqual(report['status'], 'PARTIAL')
        self.assertIn('TERMINAL_RECEIPT_INCOMPLETE', report['reasons'])
        self.assertEqual((self.out / 'database-logs/failure.log').read_bytes(), b'original failure')

    def test_identity_pass_has_independent_raw_outputs_and_no_optional_locks(self):
        self.fake_git()
        report = evidence.capture_identity(self.source, self.out, self.deadline, self.expected)
        self.assertEqual(report['status'], 'PASS')
        self.assertEqual([report[key] for key in ('identity_gate', 'clean_gate', 'repository_gate')], ['PASS'] * 3)
        self.assertEqual(len(self.git_calls), 3)
        for call in self.git_calls:
            self.assertIn('--no-optional-locks', call['argv'])
            self.assertEqual(call['env']['GIT_OPTIONAL_LOCKS'], '0')
            self.assertEqual(call['end'], self.deadline)
            self.assertLessEqual(call['timeout'], evidence.GIT_TIMEOUT_SECONDS)
        self.assertEqual(self.receipt('checkout-identity.json'), report)
        self.assertEqual((self.out / report['commands']['head']['stdout']['path']).read_bytes(), self.expected.encode() + b'\n')

    def test_wrong_head_does_not_short_circuit_status_or_top_level(self):
        self.fake_git({'head': {'stdout': b'b' * 40 + b'\n'},
                       'status': {'stdout': b'?? unexpected-file\n'}})
        report = evidence.capture_identity(self.source, self.out, self.deadline, self.expected)
        self.assertEqual(report['status'], 'FAIL')
        self.assertEqual(report['identity_gate'], 'FAIL')
        self.assertEqual(report['clean_gate'], 'FAIL')
        self.assertEqual(report['repository_gate'], 'PASS')
        self.assertEqual(len(self.git_calls), 3)
        self.assertEqual((self.out / report['commands']['status']['stdout']['path']).read_bytes(), b'?? unexpected-file\n')

    def test_head_and_status_errors_preserve_both_original_failures(self):
        self.fake_git({'head': {'exit': 128, 'stdout': b'HEAD\n', 'stderr': b'fatal: bad HEAD\n',
                                'failure_kind': 'child_exit', 'error': 'head failed'},
                       'status': {'exit': -15, 'stderr': b'status was interrupted\n',
                                  'failure_kind': 'timeout', 'error': 'deadline exhausted'}})
        report = evidence.capture_identity(self.source, self.out, self.deadline, self.expected)
        self.assertEqual(report['status'], 'PARTIAL')
        self.assertEqual(report['identity_gate'], 'UNKNOWN')
        self.assertEqual(report['clean_gate'], 'UNKNOWN')
        self.assertEqual(report['repository_gate'], 'PASS')
        self.assertEqual(len(self.git_calls), 3)
        head, status = report['commands']['head'], report['commands']['status']
        self.assertEqual(head['exit'], 128)
        self.assertEqual(status['exit'], -15)
        self.assertEqual(head['error'], 'head failed')
        self.assertEqual(status['failure_kind'], 'timeout')
        self.assertEqual((self.out / head['stderr']['path']).read_bytes(), b'fatal: bad HEAD\n')
        self.assertEqual((self.out / status['stderr']['path']).read_bytes(), b'status was interrupted\n')

    def test_repository_subdirectory_is_not_a_valid_root(self):
        self.fake_git({'top_level': {'stdout': os.fsencode(self.root) + b'\n'}})
        report = evidence.capture_identity(self.source, self.out, self.deadline, self.expected)
        self.assertEqual(report['status'], 'FAIL')
        self.assertEqual(report['identity_gate'], 'PASS')
        self.assertEqual(report['clean_gate'], 'PASS')
        self.assertEqual(report['repository_gate'], 'FAIL')

    def test_identity_deadline_exhausted_does_not_spawn_any_command(self):
        with patch.object(evidence.runtime, 'run', side_effect=AssertionError('unexpected child')) as run:
            report = evidence.capture_identity(self.source, self.out, time.monotonic() - 1, self.expected)
        run.assert_not_called()
        self.assertEqual(report['status'], 'NOT_REACHED')
        self.assertEqual(len(report['commands']), 3)
        self.assertTrue(all(item['status'] == 'NOT_REACHED' for item in report['commands'].values()))
        self.assertEqual(report, self.receipt('checkout-identity.json'))

    def test_identity_full_normal_cap_preserves_original_and_receipt(self):
        (self.out / 'original.raw').write_bytes(b'x' * 32)
        with patch.object(evidence.runtime, 'WRITE_CAP', 32), patch.object(evidence.runtime, 'run') as run:
            report = evidence.capture_identity(self.source, self.out, self.deadline, self.expected)
        run.assert_not_called()
        self.assertEqual(report['status'], 'NOT_REACHED')
        self.assertEqual((self.out / 'original.raw').read_bytes(), b'x' * 32)
        self.assertEqual(report, self.receipt('checkout-identity.json'))

    def test_identity_prefixes_keep_preflight_and_failure_evidence_separate(self):
        self.fake_git()
        first = evidence.capture_identity(self.source, self.out, self.deadline, self.expected, prefix='isolation')
        second = evidence.capture_identity(self.source, self.out, self.deadline, self.expected, prefix='capture')
        self.assertEqual(first['status'], 'PASS')
        self.assertEqual(second['status'], 'PASS')
        self.assertEqual(self.receipt('isolation-identity.json'), first)
        self.assertEqual(self.receipt('capture-identity.json'), second)
        self.assertTrue(all(item['stdout']['path'].startswith('capture-') for item in second['commands'].values()))

    def test_identity_existing_artifacts_are_preserved_without_new_children(self):
        self.fake_git()
        first = evidence.capture_identity(self.source, self.out, self.deadline, self.expected)
        before = {path.name: path.read_bytes() for path in self.out.iterdir()}
        second = evidence.capture_identity(self.source, self.out, self.deadline, self.expected)
        self.assertEqual(first['status'], 'PASS')
        self.assertEqual(second['status'], 'NOT_REACHED')
        self.assertEqual(len(self.git_calls), 3)
        self.assertIn('receipt_error', second)
        self.assertEqual(before, {path.name: path.read_bytes() for path in self.out.iterdir()})

    def test_real_git_failed_head_still_captures_status_and_repository(self):
        subprocess.run(['git', 'init', '--quiet', '--template=', str(self.source)], check=True,
                       stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        (self.source / 'untracked.txt').write_bytes(b'changed checkout')
        report = evidence.capture_identity(self.source, self.out, self.deadline, self.expected)
        self.assertEqual(report['status'], 'FAIL')
        self.assertEqual(report['identity_gate'], 'UNKNOWN')
        self.assertEqual(report['clean_gate'], 'FAIL')
        self.assertEqual(report['repository_gate'], 'PASS')
        self.assertNotEqual(report['commands']['head']['exit'], 0)
        self.assertEqual(report['commands']['status']['exit'], 0)
        self.assertEqual(report['commands']['top_level']['exit'], 0)
        self.assertEqual((self.out / report['commands']['status']['stdout']['path']).read_bytes(),
                         b'?? untracked.txt\n')
        self.assertTrue((self.out / report['commands']['head']['stderr']['path']).read_bytes())
        self.assertEqual(report, self.receipt('checkout-identity.json'))

    def test_invalid_prefix_cannot_choose_artifact_destination(self):
        self.fake_git()
        report = evidence.capture_identity(self.source, self.out, self.deadline, self.expected, prefix='../escape')
        self.assertEqual(report['status'], 'NOT_REACHED')
        self.assertIn('INVALID_ARTIFACT_PREFIX', report['reasons'])
        self.assertEqual(self.git_calls, [])
        self.assertEqual(list(self.out.iterdir()), [])


if __name__ == '__main__':
    unittest.main()
