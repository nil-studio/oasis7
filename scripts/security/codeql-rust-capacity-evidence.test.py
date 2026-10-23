#!/usr/bin/env python3
"""Small-boundary evidence regressions; no CodeQL, extractor, or capacity run."""
import contextlib
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile
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

    def test_oversize_log_keeps_prefix_and_later_small_log(self):
        (self.logs / 'large.log').write_bytes(b'abcdefgh')
        (self.logs / 'small.log').write_bytes(b'abc')
        with patch.object(evidence, 'MAX_FILE_BYTES', 4):
            report = self.capture()
        self.assertEqual(report['status'], 'PARTIAL')
        records = {item['source']: item for item in report['files']}
        self.assertIn('FILE_SIZE_LIMIT', records['large.log']['reasons'])
        self.assertEqual((self.out / 'database-logs/large.log').read_bytes(), b'abcd')
        self.assertEqual(records['small.log']['status'], 'COMPLETE')
        self.assertEqual(report, self.receipt())

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
        self.assertEqual((self.out / 'database-logs/failure.log').read_bytes(), b'abcde')
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
