#!/usr/bin/env python3
"""Candidate observation contracts with real pipes, seals and owned children."""
import hashlib
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import time
import unittest
from unittest.mock import patch

import codeql_rust_candidate_profile as profile
import codeql_rust_diagnostic_runtime as runtime

HERE = Path(__file__).resolve().parent
# Public synthetic input: fake producer source may contain it; observed evidence must not.
SYNTHETIC_CANARY = 'RAW_CANARY_NEVER_PERSIST_937542'
HEADER = {'type': 'LOG_HEADER', 'logVersion': '0.5.0', 'codeqlVersion': '2.27.4'}
FOOTER = {'type': 'LOG_FOOTER'}
RAW = (json.dumps(HEADER) + '\n' + json.dumps({'type': 'PREDICATE_STARTED', 'predicateName': SYNTHETIC_CANARY})
       + '\n' + json.dumps(FOOTER)).encode()
QUERY = dict(path='/frozen/Query.ql', relative='Query.ql', pack='fixture/profile', sha256='a'*64)


class CaptureTests(unittest.TestCase):
    def capture(self, *args):
        item = profile.Capture(*args)
        self.addCleanup(item.close)
        return item

    def test_real_memfd_seals_identity_and_last_object_without_newline(self):
        capture = self.capture()
        capture.feed(RAW[:23]); capture.feed(RAW[23:]); capture.finish(True, True)
        report = capture.summary()
        self.assertEqual(report['status'], 'CAPTURED')
        self.assertEqual(report['log_boundary_status'], 'COMPLETE')
        self.assertEqual(report['retained_bytes'], len(RAW))
        self.assertEqual(report['retained_sha256'], hashlib.sha256(RAW).hexdigest())
        self.assertEqual(Path(capture.path).read_bytes(), RAW)
        with self.assertRaises(OSError): os.write(capture.fd, b'x')
        with self.assertRaises(OSError): os.ftruncate(capture.fd, 0)
        self.assertFalse(os.get_inheritable(capture.fd))

    def test_memory_overflow_keeps_exact_prefix_and_observed_drain_hash(self):
        capture = self.capture(4)
        capture.feed(b'abcd')
        capture.feed(b'ef')
        capture.feed(b'gh'); capture.finish(True, True)
        report = capture.summary()
        self.assertEqual(report['status'], 'INCOMPLETE')
        self.assertEqual(report['error'], 'MEMORY_LIMIT')
        self.assertEqual(report['retained_bytes'], 4)
        self.assertEqual(report['observed_bytes'], 8)
        self.assertEqual(report['discarded_bytes'], 4)
        self.assertEqual(report['retained_sha256'], hashlib.sha256(b'abcd').hexdigest())
        self.assertEqual(report['observed_sha256'], hashlib.sha256(b'abcdefgh').hexdigest())
        self.assertEqual(Path(capture.path).read_bytes(), b'abcd')

    def test_crossing_memory_limit_never_resumes_retention_for_smaller_chunks(self):
        capture = self.capture(4)
        capture.feed(b'abc'); capture.feed(b'de'); capture.feed(b'f')
        capture.finish(True, True)
        report = capture.summary()
        self.assertEqual(report['error'], 'MEMORY_LIMIT')
        self.assertEqual(report['retained_bytes'], 3)
        self.assertEqual(report['discarded_bytes'], 3)
        self.assertEqual(report['observed_sha256'], hashlib.sha256(b'abcdef').hexdigest())
        self.assertEqual(Path(capture.path).read_bytes(), b'abc')
        self.assertEqual(report['log_boundary_status'], 'PARTIAL')

    def test_positive_short_write_reports_the_actual_retained_identity(self):
        capture = self.capture()
        real_write = os.write
        with patch.object(os, 'write', side_effect=lambda fd, data: real_write(fd, data[:5])):
            with self.assertRaises(profile.ObservationError): capture.feed(RAW)
        capture.finish(True, True)
        report = capture.summary()
        self.assertEqual(report['retained_bytes'], 5)
        self.assertEqual(report['retained_sha256'], hashlib.sha256(RAW[:5]).hexdigest())
        self.assertEqual(Path(capture.path).read_bytes(), RAW[:5])
        self.assertEqual(report['error'], 'SHORT_MEMORY_WRITE')

    def test_missing_footer_truncation_duplicate_header_and_bad_version_are_partial(self):
        for raw in (json.dumps(HEADER).encode(), RAW[:-1], json.dumps(HEADER).encode()+b'\n'+RAW,
                    RAW.replace(b'0.5.0', b'0.4.0')):
            with self.subTest(raw_sha256=hashlib.sha256(raw).hexdigest()):
                capture = self.capture(); capture.feed(raw); capture.finish(True, True)
                self.assertEqual(capture.summary()['log_boundary_status'], 'PARTIAL')
                capture.close()

    def test_false_eof_or_cleanup_cannot_be_reported_as_complete(self):
        for eof, cleanup in ((False, True), (True, False)):
            capture = self.capture(); capture.feed(RAW); capture.finish(eof, cleanup)
            self.assertEqual(capture.summary()['status'], 'INCOMPLETE')
            capture.close()


def numeric_event(kind, event, **fields):
    return dict(type=kind, eventId=event, nanoTime=event*100, time='2026-01-01T00:00:00Z', **fields)


def numeric_events(aliases=None):
    aliases = aliases if aliases is not None else {'TypeInference::inferType/2#fixture': [1]}
    return [numeric_event('LOG_HEADER', 1, codeqlVersion='2.27.4', logVersion='0.5.0'),
            numeric_event('PREDICATE_STARTED', 2, appearsAs=aliases, predicateType='RECURSIVE_INTENSIONAL', layerId=7),
            numeric_event('PIPELINE_STARTED', 3, predicateStartEvent=2, generationNumber=4),
            numeric_event('PIPELINE_COMPLETED', 4, startEvent=3, resultSize=0),
            numeric_event('PIPELINE_STARTED', 5, predicateStartEvent=2, generationNumber=5),
            numeric_event('PIPELINE_COMPLETED', 6, startEvent=5, resultSize=81),
            numeric_event('PIPELINE_STARTED', 7, predicateStartEvent=2, generationNumber=1539),
            numeric_event('PIPELINE_COMPLETED', 8, startEvent=7, resultSize=2),
            numeric_event('PREDICATE_COMPLETED', 9, startEvent=2, resultSize=201596),
            numeric_event('LOG_FOOTER', 10)]


def numeric_bytes(events):
    return b'\n'.join(json.dumps(event, separators=(',', ':')).encode() for event in events)


class FeedCostTests(unittest.TestCase):
    def capture(self, limit=4):
        value = profile.Capture(limit, projection=profile.NumericProjection())
        self.addCleanup(value.close)
        return value

    def test_whole_callback_buckets_keep_original_retention_and_hash(self):
        for chunks, retained in (((b'abc', b'de', b'f'), b'abc'),
                                 ((b'abcd', b'e', b'f'), b'abcd')):
            with self.subTest(chunks=chunks):
                capture = self.capture()
                with patch.object(time, 'perf_counter_ns', side_effect=[10, 14, 20, 27, 30, 39]), \
                     patch.object(time, 'thread_time_ns', side_effect=[100, 102, 200, 203, 300, 305]):
                    for chunk in chunks:
                        capture.feed(chunk)
                capture.finish(True, True)
                report = capture.summary()
                buckets = capture.feed_cost_summary()['buckets']
                for key, chunk, wall, cpu in zip(('BEFORE_LIMIT', 'CAP_CROSSING', 'AFTER_LIMIT'),
                                                chunks, (4, 7, 9), (2, 3, 5)):
                    self.assertEqual(buckets[key]['calls'], 1)
                    self.assertEqual(buckets[key]['input_bytes'], len(chunk))
                    self.assertEqual(buckets[key]['wall']['total_ns'], wall)
                    self.assertEqual(buckets[key]['thread_cpu']['total_ns'], cpu)
                self.assertEqual(report['error'], 'MEMORY_LIMIT')
                self.assertEqual(report['retained_sha256'], hashlib.sha256(retained).hexdigest())
                self.assertEqual(report['observed_sha256'], hashlib.sha256(b''.join(chunks)).hexdigest())
                self.assertEqual(Path(capture.path).read_bytes(), retained)

    def test_clock_failures_are_independent_unknown_and_do_not_change_capture(self):
        capture = self.capture(1024)
        with patch.object(time, 'perf_counter_ns', side_effect=[10, 17, 20, 30]), \
             patch.object(time, 'thread_time_ns', side_effect=[NotImplementedError(SYNTHETIC_CANARY), 2, 9, 3]):
            capture.feed(RAW[:23]); capture.feed(RAW[23:])
        capture.finish(True, True)
        report = capture.summary()
        self.assertEqual(report['status'], 'CAPTURED')
        self.assertEqual(report['log_boundary_status'], 'COMPLETE')
        buckets = capture.feed_cost_summary()['buckets']
        self.assertEqual(buckets['BEFORE_LIMIT']['wall']['total_ns'], 17)
        self.assertIsNone(buckets['BEFORE_LIMIT']['thread_cpu']['total_ns'])
        self.assertEqual(buckets['BEFORE_LIMIT']['thread_cpu']['unmeasured_intervals'], 2)
        self.assertEqual(buckets['AFTER_LIMIT']['wall']['status'], 'NOT_OBSERVED')
        self.assertIsNone(buckets['AFTER_LIMIT']['wall']['total_ns'])
        self.assertNotIn(SYNTHETIC_CANARY, json.dumps(capture.feed_cost_summary()))

    def test_processing_failure_keeps_original_error_and_records_cost_in_finally(self):
        capture = self.capture(1024)
        with patch.object(time, 'perf_counter_ns', side_effect=[5, 8]), \
             patch.object(time, 'thread_time_ns', side_effect=[10, OSError(SYNTHETIC_CANARY)]), \
             patch.object(os, 'write', side_effect=OSError(SYNTHETIC_CANARY)):
            with self.assertRaisesRegex(profile.ObservationError, '^MEMORY_WRITE_FAILED$'):
                capture.feed(RAW)
        report = capture.summary()
        self.assertEqual(report['error'], 'MEMORY_WRITE_FAILED')
        cost = capture.feed_cost_summary()['buckets']['BEFORE_LIMIT']
        self.assertEqual(cost['calls'], 1)
        self.assertEqual(cost['wall']['total_ns'], 3)
        self.assertIsNone(cost['thread_cpu']['total_ns'])
        self.assertNotIn(SYNTHETIC_CANARY, json.dumps(capture.feed_cost_summary()))

    def test_invalid_clock_values_keep_measured_subtotal_and_unknown_total(self):
        for invalid in (None, -1, True, 1.5, float('inf'), 2**63):
            with self.subTest(invalid=invalid):
                capture = self.capture(1024)
                with patch.object(time, 'perf_counter_ns', side_effect=[10, 14, invalid, 30]), \
                     patch.object(time, 'thread_time_ns', side_effect=[1, 2, 3, 5]):
                    capture.feed(RAW[:23]); capture.feed(RAW[23:])
                cost = capture.feed_cost_summary()['buckets']['BEFORE_LIMIT']
                self.assertEqual(cost['wall']['measured_ns'], 4)
                self.assertEqual(cost['wall']['unmeasured_intervals'], 1)
                self.assertIsNone(cost['wall']['total_ns'])
                self.assertEqual(cost['thread_cpu']['total_ns'], 3)
        capture = self.capture(1024)
        with patch.object(time, 'thread_time_ns', None):
            capture.feed(RAW)
        self.assertIsNone(capture.feed_cost_summary()['buckets']['BEFORE_LIMIT']['thread_cpu']['total_ns'])

    def test_raw_only_capture_does_not_read_measurement_clocks(self):
        capture = profile.Capture(); self.addCleanup(capture.close)
        with patch.object(time, 'perf_counter_ns', side_effect=AssertionError('unexpected wall clock')) as wall, \
             patch.object(time, 'thread_time_ns', side_effect=AssertionError('unexpected CPU clock')) as cpu:
            capture.feed(RAW)
        wall.assert_not_called()
        cpu.assert_not_called()
        self.assertIsNone(capture.feed_cost_summary())
        self.assertNotIn('feed_cost', capture.summary())


class NumericProjectionTests(unittest.TestCase):
    def observe(self, events=None, *, limit=profile.RAW_CAP, chunks=None):
        projection = profile.NumericProjection()
        capture = profile.Capture(limit, projection=projection)
        self.addCleanup(capture.close)
        data = numeric_bytes(events if events is not None else numeric_events())
        for chunk in chunks if chunks is not None else (data,):
            capture.feed(chunk)
        capture.finish(True, True)
        return capture, projection.report()

    def test_actual_integer_nanos_links_sparse_generations_and_zero_then_positive(self):
        capture, report = self.observe()
        self.assertEqual(report['status'], 'OBSERVED')
        self.assertEqual(report['predicate_starts'], 1)
        self.assertEqual(report['pipeline_starts'], 3)
        self.assertEqual([(row['generation'], row['pipeline_result_size']) for row in report['pipeline_tail']],
                         [(4, 0), (5, 81), (1539, 2)])
        self.assertTrue(all(row['elapsed_ns'] == 100 for row in report['pipeline_tail']))
        self.assertEqual(report['predicate_tail'][0]['predicate_result_size'], 201596)
        self.assertEqual(capture.summary()['status'], 'CAPTURED')
        encoded = json.dumps(report)
        for forbidden in ('TypeInference', 'fixture', 'queryCausingWork', 'predicateName', 'delta', 'cpu_rank'):
            self.assertNotIn(forbidden, encoded)

    def test_family_uses_namespace_and_exact_terminal_member_not_similar_or_nested_names(self):
        positive = ('TypeInference::inferType/2#opaque',
                    'Root::TypeInference::M3<Other::X>::inferType/2#opaque')
        negative = ('TypeInferenceX::inferType/2#opaque', 'TypeInference::inferTypeCertain/2#opaque',
                    'TypeInference::inferType/20#opaque', 'TypeInference::inferType/2::Extra',
                    'Other<TypeInference::X>::inferType/2#opaque', 'Other::inferType/2#opaque')
        for alias in (*positive, *negative):
            with self.subTest(alias=alias):
                _, report = self.observe(numeric_events({alias: [1]}))
                self.assertEqual(report['status'], 'OBSERVED' if alias in positive else 'NOT_OBSERVED')
                self.assertNotIn(alias, json.dumps(report))

    def test_multiple_aliases_and_separate_predicate_events_are_not_summed_or_deduplicated(self):
        events = numeric_events({'TypeInference::inferType/2#one': [1, 2],
                                 'TypeInference::M3::inferType/2#two': [2, 3]})
        _, report = self.observe(events)
        self.assertEqual(report['predicate_starts'], 1)
        self.assertEqual(report['pipeline_starts'], 3)
        self.assertEqual(len(report['aliases']), 2)
        self.assertEqual(len(report['predicate_tail']), 1)
        extra = [numeric_event('PREDICATE_STARTED', 11, appearsAs=events[1]['appearsAs'], predicateType='STAGE_RESULT'),
                 numeric_event('PREDICATE_COMPLETED', 12, startEvent=11, resultSize=201596)]
        _, report = self.observe(events[:-1]+extra+[numeric_event('LOG_FOOTER', 13)])
        self.assertEqual(report['predicate_starts'], 2)
        self.assertEqual([row['predicate_event'] for row in report['predicate_tail']], [2, 11])

    def test_cap_crossing_preserves_default_prefix_receipt_but_projects_full_tail(self):
        data = numeric_bytes(numeric_events())
        chunks = (data[:177], data[177:423], data[423:])
        original = profile.Capture(200); self.addCleanup(original.close)
        for chunk in chunks: original.feed(chunk)
        original.finish(True, True)
        capture, report = self.observe(limit=200, chunks=chunks)
        self.assertEqual({key:value for key,value in capture.summary().items() if key != 'limits'},
                         {key:value for key,value in original.summary().items() if key != 'limits'})
        self.assertEqual(capture.summary()['error'], 'MEMORY_LIMIT')
        self.assertEqual(capture.summary()['status'], 'INCOMPLETE')
        self.assertEqual(report['status'], 'OBSERVED')
        self.assertTrue(report['raw_memory_limit_seen'])
        self.assertGreater(report['post_limit_records'], 0)
        self.assertEqual(report['pipeline_tail'][-1]['generation'], 1539)

    def test_split_utf8_and_last_object_without_lf_keep_optional_values_unknown(self):
        events = numeric_events()
        events[1]['predicateName'] = SYNTHETIC_CANARY+'é'
        for event in events:
            event.pop('generationNumber', None)
            event.pop('resultSize', None)
        data = numeric_bytes(events)
        _, report = self.observe(events, chunks=[data[i:i+7] for i in range(0, len(data), 7)])
        self.assertEqual(report['status'], 'OBSERVED')
        self.assertTrue(all(row['generation'] is None and row['pipeline_result_size'] is None
                            for row in report['pipeline_tail']))
        self.assertIsNone(report['predicate_tail'][0]['predicate_result_size'])

    def test_unknown_schema_bool_ids_and_string_nanos_are_partial_without_raw_fallback(self):
        for changes in ({'eventId': True}, {'nanoTime': '300'}, {'unknown': SYNTHETIC_CANARY},
                        {'completionType': 'NOT_AN_ENUM'}, {'resultSize': -1}):
            events = numeric_events(); events[3].update(changes)
            capture, report = self.observe(events)
            self.assertEqual(report['status'], 'PARTIAL')
            self.assertEqual(capture.summary()['status'], 'CAPTURED')
            self.assertNotIn(SYNTHETIC_CANARY, json.dumps(report))

    def test_oversized_numeric_line_after_raw_cap_is_bounded_and_recovers_at_lf(self):
        events = numeric_events()
        large = numeric_event('CACHE_LOOKUP', 20, predicateName=SYNTHETIC_CANARY+'x'*profile.NUMERIC_LINE_CAP)
        data = numeric_bytes(events[:2]+[large]+events[2:])
        capture, report = self.observe(limit=200, chunks=[data[i:i+4096] for i in range(0, len(data), 4096)])
        self.assertEqual(capture.summary()['error'], 'MEMORY_LIMIT')
        self.assertEqual(report['status'], 'PARTIAL')
        self.assertEqual(report['pipeline_tail'][-1]['generation'], 1539)
        self.assertLessEqual(len(json.dumps(report, indent=2).encode())+1, runtime.MAX_CONTROL_RECEIPT)
        self.assertNotIn(SYNTHETIC_CANARY, json.dumps(report))

    def test_projection_exceptions_do_not_escape_into_capture(self):
        projection = profile.NumericProjection()
        capture = profile.Capture(projection=projection); self.addCleanup(capture.close)
        with patch.object(projection, 'accept', side_effect=ValueError(SYNTHETIC_CANARY)):
            capture.feed(numeric_bytes(numeric_events()))
        capture.finish(True, True)
        self.assertEqual(capture.summary()['status'], 'CAPTURED')
        self.assertEqual(projection.report()['status'], 'PARTIAL')
        self.assertNotIn(SYNTHETIC_CANARY, json.dumps(projection.report()))

    def test_duplicate_active_identity_and_unclosed_target_are_partial(self):
        events = numeric_events()
        for bad in (events[:2]+[events[1]]+events[2:], events[:-2]+[events[-1]]):
            _, report = self.observe(bad)
            self.assertEqual(report['status'], 'PARTIAL')

    def test_known_target_completion_is_not_a_valid_pipeline_parent(self):
        events = numeric_events()
        bad = [numeric_event('PIPELINE_STARTED', 11, predicateStartEvent=9, generationNumber=4),
               numeric_event('PIPELINE_COMPLETED', 12, startEvent=11, resultSize=1)]
        _, report = self.observe(events[:-1]+bad+[events[-1]])
        self.assertEqual(report['status'], 'PARTIAL')
        self.assertGreater(report['errors']['LINK_ORDER'], 0)

    def test_write_failure_or_short_write_marks_unframed_drain_partial(self):
        for short in (False, True):
            projection = profile.NumericProjection()
            capture = profile.Capture(projection=projection); self.addCleanup(capture.close)
            capture.feed(numeric_bytes(numeric_events())+b'\n')
            real_write = os.write
            failure = (lambda fd, data: real_write(fd, data[:3])) if short else OSError('synthetic write failure')
            with patch.object(os, 'write', side_effect=failure):
                with self.assertRaises(profile.ObservationError): capture.feed(b'{"unprocessed":true}\n')
            capture.feed(b'{"drained":true}\n'); capture.finish(True, True)
            report = projection.report()
            self.assertEqual(report['status'], 'PARTIAL')
            self.assertGreater(report['errors']['INPUT_GAP'], 0)
            self.assertEqual(report['input_unframed_bytes'], len(b'{"unprocessed":true}\n{"drained":true}\n'))
            self.assertEqual(capture.summary()['error'], 'SHORT_MEMORY_WRITE' if short else 'MEMORY_WRITE_FAILED')

    def test_closed_target_ids_and_duplicate_completions_remain_known(self):
        events = numeric_events()
        duplicate_ends = (numeric_event('PIPELINE_COMPLETED', 11, startEvent=3, resultSize=0),
                          numeric_event('PREDICATE_COMPLETED', 11, startEvent=2, resultSize=201596))
        for repeated in (*duplicate_ends, events[1], events[3]):
            _, report = self.observe(events[:-1]+[repeated, events[-1]])
            self.assertEqual(report['status'], 'PARTIAL')
            self.assertEqual(report['predicate_starts'], 1)
            self.assertEqual(report['pipeline_completed'], 3)
            self.assertEqual(report['predicate_completed'], 1)
        # An unrelated completed relation was never selected by the fixed probe.
        _, report = self.observe(events[:-1]+[numeric_event('PREDICATE_COMPLETED', 11, startEvent=999), events[-1]])
        self.assertEqual(report['status'], 'OBSERVED')

    def test_target_identity_history_is_bounded_and_saturation_stays_partial(self):
        with patch.object(profile, 'NUMERIC_EVENT_CAP', 2):
            _, report = self.observe()
        self.assertEqual(report['status'], 'PARTIAL')
        self.assertEqual(report['remembered_target_events'], 2)
        self.assertGreater(report['errors']['TABLE_LIMIT'], 0)

    def test_failure_states_and_missing_sizes_are_not_silently_success_or_zero(self):
        for completion in ('FAILURE', 'INCOMPLETE'):
            events = numeric_events(); events[3].update(completionType=completion, exceptionName=SYNTHETIC_CANARY)
            events[3].pop('resultSize')
            _, report = self.observe(events)
            row = report['pipeline_tail'][0]
            self.assertEqual(report['status'], 'PARTIAL')
            self.assertEqual(row['completion'], completion)
            self.assertTrue(row['exception_present'])
            self.assertIsNone(row['pipeline_result_size'])
            self.assertNotIn(SYNTHETIC_CANARY, json.dumps(report))

    def test_signed_unprojected_markers_are_not_reinterpreted_as_sizes_or_query_attribution(self):
        events = numeric_events()
        events[1]['queryCausingWork'] = -1
        events[1]['appearsAs']['TypeInference::inferType/2#fixture'] = [-1, 1]
        events.insert(-1, numeric_event('CACHE_LOOKUP', 11, relationSize=-1, cacheLookupResult='CACHACA_HIT'))
        _, report = self.observe(events)
        self.assertEqual(report['status'], 'OBSERVED')
        self.assertNotIn('relationSize', json.dumps(report))
        self.assertNotIn('queryCausingWork', json.dumps(report))

    def test_post_cap_oversized_framing_recovers_without_changing_memory_failure(self):
        events = numeric_events()
        huge = numeric_event('CACHE_LOOKUP', 20, predicateName='x'*(profile.LINE_CAP+100))
        data = numeric_bytes(events[:2]+[huge]+events[2:])
        capture, report = self.observe(limit=200, chunks=[data[i:i+65536] for i in range(0, len(data), 65536)])
        self.assertEqual(capture.summary()['error'], 'MEMORY_LIMIT')
        self.assertEqual(report['status'], 'PARTIAL')
        self.assertEqual(report['pipeline_completed'], 3)
        self.assertEqual(capture.pending, b'')

    def test_state_caps_and_receipt_bound_are_fixed(self):
        events = numeric_events()[:1]
        for index in range(profile.NUMERIC_PREDICATE_CAP+1):
            events.append(numeric_event('PREDICATE_STARTED', index+2,
                appearsAs={'TypeInference::inferType/2#fixed': [1]}, predicateType='RECURSIVE_INTENSIONAL'))
        events.append(numeric_event('LOG_FOOTER', 1000))
        _, report = self.observe(events)
        self.assertEqual(report['status'], 'PARTIAL')
        self.assertEqual(report['open_predicates'], profile.NUMERIC_PREDICATE_CAP)
        self.assertLessEqual(len(json.dumps(report, indent=2).encode())+1, runtime.MAX_CONTROL_RECEIPT)

    def test_observation_reserves_numeric_state_globally_and_restores_caps(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory); out = root/'evidence'; out.mkdir(); work = root/'work'; work.mkdir()
            original = runtime.CAP, runtime.WRITE_CAP
            observation = profile.Observation(out, work, [QUERY])
            try:
                observation.start()
                self.assertEqual(runtime.CAP, original[0]-profile.RAW_CAP-profile.NUMERIC_RESERVE)
                self.assertEqual(runtime.WRITE_CAP, original[1]-profile.RAW_CAP-profile.NUMERIC_RESERVE)
                with observation.output_budget():
                    self.assertEqual(runtime.CAP, profile.OUTPUT_CAP-profile.NUMERIC_RESERVE)
            finally:
                observation.close()
            self.assertEqual((runtime.CAP, runtime.WRITE_CAP), original)


class OwnedProcessTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(); self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name); self.out = self.root / 'evidence'; self.out.mkdir()
        self.cap = profile.Capture(); self.addCleanup(self.cap.close)
        self.saved_caps = runtime.CAP, runtime.WRITE_CAP
        runtime.CAP, runtime.WRITE_CAP = 64*1024**2, 63*1024**2
        self.addCleanup(self.restore_caps)

    def restore_caps(self): runtime.CAP, runtime.WRITE_CAP = self.saved_caps

    def child(self, body):
        path = self.root / 'child.py'; path.write_text(body)
        return [sys.executable, str(path), '{EVALUATOR_FIFO}']

    def execute(self, body, timeout=8):
        return runtime.run(self.child(body), self.root, self.out / 'batch.log', time.monotonic()+10,
                           timeout, self.out, os.environ.copy(), file_limit=None,
                           evaluator=self.root/'events.jsonl', evaluator_capture=self.cap,
                           observer=profile.Progress([QUERY]), restricted_output=True)

    def test_numeric_parser_failure_preserves_real_child_exit(self):
        self.cap.close()
        projection = profile.NumericProjection()
        self.cap = profile.Capture(projection=projection); self.addCleanup(self.cap.close)
        data = numeric_bytes(numeric_events())
        body = 'import os,sys\nf=os.open(sys.argv[1],os.O_WRONLY)\n'+f'os.write(f,{data!r})\nos.close(f)\nsys.exit(7)\n'
        with patch.object(projection, 'accept', side_effect=ValueError(SYNTHETIC_CANARY)), \
             patch.object(time, 'thread_time_ns', side_effect=NotImplementedError(SYNTHETIC_CANARY)):
            with self.assertRaises(runtime.PhaseError) as caught:
                self.execute(body)
        self.assertEqual(caught.exception.result['execution_failure_kind'], 'child_exit')
        self.assertEqual(caught.exception.result['failure_kind'], 'child_exit')
        self.assertEqual(caught.exception.result['exit'], 7)
        self.assertEqual(projection.report()['status'], 'PARTIAL')
        cost = self.cap.feed_cost_summary()['buckets']['BEFORE_LIMIT']
        self.assertGreater(cost['calls'], 0)
        self.assertIsNone(cost['thread_cpu']['total_ns'])
        self.assertEqual(cost['wall']['status'], 'MEASURED')
        for path in self.out.iterdir():
            if path.is_file(): self.assertNotIn(SYNTHETIC_CANARY.encode(), path.read_bytes())

    def test_numeric_projection_keeps_term_tail_after_raw_cap_without_overwriting_timeout(self):
        events = numeric_events(); prefix = numeric_bytes(events[:3])+b'\n'; tail = numeric_bytes(events[3:])
        self.cap.close()
        projection = profile.NumericProjection()
        self.cap = profile.Capture(len(prefix), projection=projection); self.addCleanup(self.cap.close)
        body = ('import os,sys,signal,time\nf=os.open(sys.argv[1],os.O_WRONLY)\n'
                'def stopped(*_):\n'+f' os.write(f,{tail!r})\n os.close(f)\n sys.exit(143)\n'
                'signal.signal(signal.SIGTERM,stopped)\n'+f'os.write(f,{prefix!r})\n'
                'while True: time.sleep(.05)\n')
        with self.assertRaises(runtime.PhaseError) as caught:
            self.execute(body, timeout=4.2)
        self.assertEqual(caught.exception.result['execution_failure_kind'], 'timeout')
        buckets = self.cap.feed_cost_summary()['buckets']
        self.assertEqual(sum(bucket['input_bytes'] for bucket in buckets.values()), len(prefix)+len(tail))
        self.assertGreater(buckets['BEFORE_LIMIT']['calls'], 0)
        self.assertEqual(buckets['CAP_CROSSING']['calls'], 1)
        self.assertEqual(buckets['CAP_CROSSING']['thread_cpu']['status'], 'MEASURED')
        self.assertEqual(caught.exception.result['failure_kind'], 'timeout')
        self.assertEqual(self.cap.summary()['error'], 'MEMORY_LIMIT')
        self.assertEqual(projection.report()['status'], 'OBSERVED')
        self.assertEqual(projection.report()['pipeline_tail'][-1]['generation'], 1539)
        self.assertTrue(self.cap.summary()['transport_eof'])

    def assert_private(self):
        self.assertFalse((self.root/'events.jsonl').exists())
        self.assertFalse((self.root/'events.fifo').exists())
        for path in self.out.rglob('*'):
            if path.is_file(): self.assertNotIn(SYNTHETIC_CANARY.encode(), path.read_bytes(), str(path))

    def test_interleaved_three_streams_never_persist_raw_text(self):
        body = ('import os,sys\n'
                f'fd=os.open(sys.argv[1],os.O_WRONLY);os.write(fd,{RAW!r});os.close(fd)\n'
                f'os.write(1,{(SYNTHETIC_CANARY+chr(10)).encode()!r})\n'
                f'os.write(2,{(SYNTHETIC_CANARY+chr(10)).encode()!r})\n'
                'os.write(2,b"Compiling query plan for /frozen/Query.ql.\\n")\n'
                'os.write(2,b"[1/1 comp 2ms] Compiled /frozen/Query.ql.\\n")\n')
        result = self.execute(body)
        self.assertEqual(result['exit'], 0)
        self.assertEqual(result['evaluator_capture']['log_boundary_status'], 'COMPLETE')
        self.assertEqual(result['output_observation']['events']['COMPILE_DONE'], 1)
        self.assertTrue(all(v['final'] for v in result['output_observation']['streams'].values()))
        self.assert_private()

    def test_memory_limit_drains_beyond_pipe_capacity_and_preserves_normal_exit(self):
        self.cap.close(); self.cap = profile.Capture(4096); self.addCleanup(self.cap.close)
        prefix = (json.dumps(HEADER)+'\n').encode()
        event = (json.dumps(dict(type='PREDICATE_STARTED', predicateName=SYNTHETIC_CANARY,
                                payload='x'*1024))+'\n').encode()
        suffix = json.dumps(FOOTER).encode()
        raw = prefix + event*256 + suffix
        completed = self.root/'producer-completed'
        body = ('import os,sys\nfrom pathlib import Path\nfd=os.open(sys.argv[1],os.O_WRONLY)\n'
                f'os.write(fd,{prefix!r})\n'
                f'for _ in range(256): os.write(fd,{event!r})\n'
                f'os.write(fd,{suffix!r});os.close(fd)\n'
                f'Path({str(completed)!r}).write_text("completed")\n')
        result = self.execute(body)
        self.assertEqual(result['exit'], 0)
        self.assertIsNone(result['execution_failure_kind'])
        self.assertFalse(result['cleanup_errors'])
        self.assertEqual(completed.read_text(), 'completed')
        report = result['evaluator_capture']
        self.assertEqual(report['status'], 'INCOMPLETE')
        self.assertEqual(report['error'], 'MEMORY_LIMIT')
        self.assertTrue(report['transport_eof'])
        self.assertTrue(report['cleanup_ok'])
        self.assertEqual(report['observed_bytes'], len(raw))
        self.assertEqual(report['observed_sha256'], hashlib.sha256(raw).hexdigest())
        self.assertLessEqual(report['retained_bytes'], 4096)
        self.assertEqual(report['discarded_bytes'], len(raw)-report['retained_bytes'])
        self.assertEqual(Path(self.cap.path).read_bytes(), raw[:report['retained_bytes']])
        self.assert_private()

    def test_memory_limit_does_not_replace_a_later_child_exit_failure(self):
        self.cap.close(); self.cap = profile.Capture(4); self.addCleanup(self.cap.close)
        body = ('import os,sys\nfd=os.open(sys.argv[1],os.O_WRONLY)\n'
                f'os.write(fd,{RAW!r});os.close(fd)\nsys.exit(7)\n')
        with self.assertRaises(runtime.PhaseError) as caught:
            self.execute(body)
        result = caught.exception.result
        self.assertEqual(result['exit'], 7)
        self.assertEqual(result['execution_failure_kind'], 'child_exit')
        self.assertEqual(result['evaluator_capture']['error'], 'MEMORY_LIMIT')
        self.assertFalse(result['cleanup_errors'])
        self.assert_private()

    def test_timeout_preserves_term_handler_footer_and_original_failure(self):
        prefix = (json.dumps(HEADER)+'\n').encode()
        body = ('import os,sys,signal,time\nfd=os.open(sys.argv[1],os.O_WRONLY)\n'
                f'os.write(fd,{prefix!r})\n'
                'def stop(*_):\n'
                f' os.write(fd,{json.dumps(FOOTER).encode()!r});os.close(fd);sys.exit(0)\n'
                'signal.signal(signal.SIGTERM,stop)\ntime.sleep(60)\n')
        with self.assertRaises(runtime.PhaseError) as caught: self.execute(body, timeout=4.1)
        result = caught.exception.result
        self.assertEqual(result['execution_failure_kind'], 'timeout')
        self.assertTrue(caught.exception.clean_timeout)
        self.assertEqual(result['evaluator_capture']['log_boundary_status'], 'COMPLETE')
        self.assert_private()

    def test_parser_exception_does_not_escape_through_error_receipts(self):
        class Reject(profile.Lines):
            def line(self, name, data):
                json.loads(data)  # the resulting exception must not expose its input
        argv = self.child(f'import os,sys\nos.write(1,{(SYNTHETIC_CANARY+chr(10)).encode()!r})\n')
        with self.assertRaises(runtime.PhaseError) as caught:
            runtime.run(argv[:-1], self.root, self.out/'reject.log', time.monotonic()+8, 8,
                        self.out, os.environ.copy(), observer=Reject(), restricted_output=True)
        self.assertEqual(caught.exception.result['failure_kind'], 'unsafe')
        self.assert_private()

    def test_existing_fifo_is_not_deleted_and_no_child_is_started(self):
        fifo = self.root/'events.fifo'; os.mkfifo(fifo)
        with self.assertRaises(runtime.PhaseError) as caught: self.execute('raise RuntimeError("must not start")\n')
        self.assertIsNone(caught.exception.result['pid'])
        self.assertTrue(fifo.exists()); fifo.unlink()

    def test_final_receipt_crossing_end_is_failure(self):
        real_save = runtime.save
        deadline = time.monotonic()+4
        def late_save(path, value, root, terminal=False):
            real_save(path, value, root, terminal)
            if path.name.endswith('.result.json'):
                clock[0] = deadline+1
        clock = [time.monotonic()]
        with patch.object(runtime, 'save', side_effect=late_save), patch.object(runtime.time, 'monotonic', side_effect=lambda: clock[0]):
            with self.assertRaises(runtime.PhaseError) as caught:
                runtime.run([sys.executable,'-c','pass'],self.root,self.out/'late.log',deadline,4,
                            self.out,os.environ.copy(),observer=profile.Lines(),restricted_output=True)
        self.assertEqual(caught.exception.result['error'], 'RECEIPT_EXCEEDED_FIXED_DEADLINE')
        self.assertEqual(json.loads((self.out/'late.log.result.json').read_text())['failure_kind'], 'unsafe')

    def test_sigterm_and_repeat_interrupt_close_owned_process_and_memfd(self):
        child = self.root/'waiting-child.py'
        child.write_text('import os,sys,time\nfd=os.open(sys.argv[1],os.O_WRONLY)\n'
                         f'os.write(fd,{(json.dumps(HEADER)+chr(10)).encode()!r})\n'
                         'open(sys.argv[2],"w").write(str(os.getpid()))\ntime.sleep(60)\n')
        supervisor = self.root/'supervisor.py'
        supervisor.write_text('import sys,os,time,json\nfrom pathlib import Path\n'
            f'sys.path.insert(0,{str(HERE)!r})\nimport codeql_rust_candidate_profile as p\nimport codeql_rust_diagnostic_runtime as r\n'
            f'root=Path({str(self.root)!r});out=root/"evidence";c=p.Capture()\n'
            'try:\n'
            ' r.run([sys.executable,str(root/"waiting-child.py"),"{EVALUATOR_FIFO}",str(root/"ready")],root,out/"cancel.log",time.monotonic()+12,12,out,os.environ.copy(),evaluator=root/"cancel-events.jsonl",evaluator_capture=c,observer=p.Lines(),restricted_output=True)\n'
            'except r.PhaseError: pass\n'
            'finally:\n'
            ' c.close();(out/"closed.json").write_text(json.dumps({"closed":c.fd is None}))\n')
        proc = subprocess.Popen([sys.executable, str(supervisor)], stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        self.addCleanup(lambda: proc.kill() if proc.poll() is None else None)
        until = time.monotonic()+4
        while not (self.root/'ready').exists() and proc.poll() is None and time.monotonic()<until:
            time.sleep(.01)
        self.assertTrue((self.root/'ready').exists())
        proc.send_signal(signal.SIGTERM)
        time.sleep(.01)
        if proc.poll() is None: proc.send_signal(signal.SIGTERM)
        stdout, stderr = proc.communicate(timeout=5)
        self.assertEqual(proc.returncode, 0, stderr.decode(errors='replace'))
        result = json.loads((self.out/'cancel.log.result.json').read_text())
        self.assertEqual(result['failure_kind'], 'cancelled')
        self.assertFalse(result['cleanup_errors'])
        self.assertTrue(json.loads((self.out/'closed.json').read_text())['closed'])
        with self.assertRaises(ProcessLookupError): os.kill(result['pid'], 0)

    def test_sigterm_between_real_popen_and_registration_cannot_leak_child(self):
        real_popen = subprocess.Popen
        children = []
        def during_popen(*args, **kwargs):
            child = real_popen(*args, **kwargs); children.append(child)
            os.kill(os.getpid(), signal.SIGTERM)
            return child
        with patch.object(runtime.subprocess, 'Popen', side_effect=during_popen):
            with self.assertRaises(runtime.PhaseError) as caught:
                runtime.run([sys.executable,'-c','import time; time.sleep(60)'],self.root,self.out/'race.log',
                            time.monotonic()+8,8,self.out,os.environ.copy(),observer=profile.Lines(),restricted_output=True)
        self.assertEqual(caught.exception.result['failure_kind'],'cancelled')
        self.assertEqual(caught.exception.result['pid'],children[0].pid)
        self.assertIsNotNone(children[0].poll())
        with self.assertRaises(ProcessLookupError): os.kill(children[0].pid,0)

    def test_sigterm_during_last_result_write_is_not_success(self):
        real_save=runtime.save; signalled=[]
        def late_cancel(path,value,root,terminal=False):
            real_save(path,value,root,terminal)
            if path.name.endswith('.result.json') and not signalled:
                signalled.append(True);os.kill(os.getpid(),signal.SIGTERM)
        with patch.object(runtime,'save',side_effect=late_cancel):
            with self.assertRaises(runtime.PhaseError) as caught:
                runtime.run([sys.executable,'-c','pass'],self.root,self.out/'late-cancel.log',
                            time.monotonic()+8,8,self.out,os.environ.copy(),observer=profile.Lines(),restricted_output=True)
        self.assertEqual(caught.exception.result['failure_kind'],'cancelled')
        self.assertTrue(json.loads((self.out/'late-cancel.log.result.json').read_text())['cancelled'])


class BudgetTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(); self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name); self.work = self.root/'work'; self.work.mkdir()
        self.out = self.root/'out'; self.out.mkdir()
        self.caps = runtime.CAP, runtime.WRITE_CAP
        runtime.CAP, runtime.WRITE_CAP = 64*1024**2, 63*1024**2
        self.addCleanup(self.restore)

    def restore(self): runtime.CAP, runtime.WRITE_CAP = self.caps

    def observation(self):
        value = profile.Observation(self.out, self.work, [QUERY]); self.addCleanup(value.close)
        value.start(); return value

    def test_parent_cpu_spans_original_runtime_call_and_keeps_its_result(self):
        observation = self.observation()
        original = dict(exit=0, failure_kind=None, resources={'observed_group_own_cpu_seconds': 9999})
        deadline = time.monotonic()+10
        with patch.object(time, 'process_time_ns', side_effect=[100, 123]), \
             patch.object(runtime, 'run', return_value=original) as run:
            returned = observation.run_batch(['/cli', 'database', 'run-queries', '--', '/db', '/q'],
                                            self.root, deadline, 10, {})
        self.assertIs(returned, original)
        self.assertIs(observation.batch_result, original)
        self.assertEqual(run.call_args.args[3:5], (deadline, 10))
        self.assertEqual(observation.receipt()['batch_parent_cpu']['total_ns'], 23)
        self.assertEqual(observation.receipt()['batch_parent_cpu']['intervals'], 1)

    def test_parent_clock_failure_preserves_original_phase_error_and_records_unknown(self):
        observation = self.observation()
        original = runtime.PhaseError(dict(exit=143, failure_kind='timeout', cleanup_errors=[], evidence_errors=[]))
        with patch.object(time, 'process_time_ns', side_effect=[10, OSError(SYNTHETIC_CANARY)]), \
             patch.object(runtime, 'run', side_effect=original):
            with self.assertRaises(runtime.PhaseError) as caught:
                observation.run_batch(['/cli', '--', '/db'], self.root, time.monotonic()+10, 10, {})
        self.assertIs(caught.exception, original)
        self.assertIs(observation.batch_result, original.result)
        self.assertTrue(observation.batch_safe)
        cost = observation.receipt()['batch_parent_cpu']
        self.assertIsNone(cost['total_ns'])
        self.assertEqual(cost['unmeasured_intervals'], 1)
        self.assertNotIn(SYNTHETIC_CANARY, json.dumps(observation.receipt()))

    def test_unobserved_cost_and_clock_failure_do_not_change_completion_or_receipt_limit(self):
        observation = self.observation()
        with patch.object(time, 'perf_counter_ns', side_effect=NotImplementedError(SYNTHETIC_CANARY)):
            observation.capture.feed(RAW)
        observation.capture.finish(True, True)
        observation.summary_result = dict(status='PROJECTED')
        result = observation.finish()
        self.assertEqual(result['status'], 'COMPLETE')
        path = observation.out/'observation-terminal.json'
        self.assertLessEqual(path.stat().st_size, runtime.MAX_CONTROL_RECEIPT)
        receipt = json.loads(path.read_text())
        self.assertEqual(receipt['batch_parent_cpu']['status'], 'NOT_OBSERVED')
        self.assertIsNone(receipt['batch_parent_cpu']['total_ns'])
        self.assertEqual(receipt['capture']['feed_cost']['buckets']['BEFORE_LIMIT']['wall']['status'], 'PARTIAL')
        self.assertNotIn(SYNTHETIC_CANARY, path.read_text())

    def test_raw_memory_output_and_terminal_reservations_share_original_cap(self):
        observation = self.observation()
        self.assertEqual(runtime.CAP, 12*1024**2)
        self.assertEqual(runtime.WRITE_CAP, 11*1024**2)
        with observation.output_budget():
            self.assertEqual(runtime.CAP, 4*1024**2)
            self.assertEqual(runtime.WRITE_CAP, 3*1024**2)
        self.assertEqual(runtime.CAP, 12*1024**2)
        observation.close(); self.assertEqual(runtime.CAP, 64*1024**2)

    def test_insufficient_shared_capacity_rejects_before_memfd_allocation(self):
        with (self.out/'core').open('wb') as stream: stream.truncate(24*1024**2)
        with patch.object(profile, 'Capture') as capture:
            with self.assertRaises(profile.ObservationError): self.observation()
            capture.assert_not_called()
        self.assertEqual(runtime.CAP, 64*1024**2)

    def test_unsafe_or_low_time_never_spawns_summary(self):
        for safe, remaining, expected in ((False, 100, 'NOT_REACHED_UNSAFE_FAILURE'),
                                          (True, 19, 'NOT_REACHED_FIXED_DEADLINE')):
            observation = self.observation()
            observation.capture.feed(RAW); observation.capture.finish(True, True)
            observation.batch_safe = safe
            with patch.object(runtime, 'run') as run:
                observation.summarize('/cli', self.root, {}, time.monotonic()+remaining, admitted=True)
                run.assert_not_called()
            self.assertEqual(observation.summary_result['status'], expected)
            observation.close()
            for path in observation.out.iterdir(): path.unlink()
            observation.out.rmdir()

    def test_memory_limit_never_summarizes_the_retained_prefix(self):
        real_capture = profile.Capture
        with patch.object(profile, 'Capture', side_effect=lambda **kwargs: real_capture(4, **kwargs)):
            observation = self.observation()
        observation.capture.feed(RAW); observation.capture.finish(True, True)
        observation.batch_safe = True
        with patch.object(runtime, 'run') as run:
            observation.summarize('/cli', self.root, {}, time.monotonic()+100, admitted=True)
            run.assert_not_called()
        self.assertEqual(observation.summary_result['status'], 'NOT_REACHED_MEMORY_LIMIT')
        self.assertEqual(observation.receipt()['status'], 'INCOMPLETE')
        self.assertEqual(runtime.CAP, 12*1024**2)
        observation.close(); self.assertEqual(runtime.CAP, 64*1024**2)

    def test_optional_numeric_status_does_not_add_a_research_completion_gate(self):
        for expected, data in (('PARTIAL', RAW), ('NOT_OBSERVED', numeric_bytes(numeric_events({})))):
            with self.subTest(numeric_status=expected):
                observation = self.observation()
                observation.capture.feed(data); observation.capture.finish(True, True)
                observation.summary_result = dict(status='PROJECTED')
                result = observation.finish()
                self.assertEqual(result['status'], 'COMPLETE')
                self.assertEqual(result['numeric_status'], expected)
                numeric = observation.out/'numeric-projection.json'
                self.assertLessEqual(numeric.stat().st_size, runtime.MAX_CONTROL_RECEIPT)
                self.assertNotIn(SYNTHETIC_CANARY.encode(), numeric.read_bytes())
                observation.close()
                for path in observation.out.iterdir(): path.unlink()
                observation.out.rmdir()


class SummaryTests(unittest.TestCase):
    def row(self, **changes):
        value = dict(raHash='physical-relation-1', predicateName=SYNTHETIC_CANARY, appearsAs={SYNTHETIC_CANARY: {QUERY['path']: [1,2]}},
                     evaluationStrategy='EXTENSIONAL', completionTime='2026-11-11T00:00:01Z',
                     dependencies={SYNTHETIC_CANARY:'dependency-id'}, millis=100, position=SYNTHETIC_CANARY,
                     queryCausingWork=QUERY['path'], resultSize=5)
        value.update(changes); return value

    def project(self, rows, *, header=None, suffix=b'', stderr=b''):
        header = header or dict(summaryLogVersion='0.4.0', codeqlVersion='2.27.4', startTime='2026-11-11T00:00:00Z')
        data = '\n'.join(json.dumps(value) for value in [header,*rows]).encode()+suffix
        observer = profile.Summary([QUERY])
        for index in range(0,len(data),7): self.assertEqual(observer.feed('stdout', data[index:index+7]), b'')
        observer.feed('stdout', b'', final=True)
        observer.feed('stderr', stderr, final=True)
        report = observer.summary()
        self.assertEqual(report['streams']['stdout']['bytes'], len(data))
        self.assertEqual(report['streams']['stdout']['sha256'], hashlib.sha256(data).hexdigest())
        self.assertNotIn(SYNTHETIC_CANARY, json.dumps(report))
        return report

    def member(self, **changes):
        value = self.row(raHash='physical-member', evaluationStrategy='IN_LAYER',
                         predicateIterationMillis=[3,-1,7], deltaSizes=[1,-1,2], mainHash='physical-layer',
                         pipelineRuns=[{'raReference':SYNTHETIC_CANARY}])
        del value['millis']; del value['dependencies']
        value.update(changes); return value

    def test_real_observed_shapes_separate_literal_time_and_known_iteration_subtotal(self):
        report = self.project([self.row(), self.member()])
        self.assertEqual(report['status'], 'PROJECTED')
        self.assertEqual(report['accepted_records'], 2)
        self.assertEqual(report['negative_iteration_markers'], 1)
        literal = report['rankings']['official_millis']; iterations = report['rankings']['known_iteration_subtotal']
        self.assertEqual(len(literal), 1); self.assertEqual(len(iterations), 1)
        self.assertEqual(literal[0]['reported_millis'], 100)
        self.assertEqual(iterations[0]['known_iteration_subtotal'], 10)
        self.assertIsNone(iterations[0]['reported_millis'])
        self.assertEqual(iterations[0]['negative_iteration_markers'], 1)
        self.assertEqual(iterations[0]['first_requiring_query'], {'index':0,'query_sha256':'a'*64})
        self.assertEqual(iterations[0]['appears_in_query_indexes'], [0])

    def test_all_unknown_iterations_are_not_zero_cost(self):
        report = self.project([self.row(),self.member(predicateIterationMillis=[-1,-1])])
        self.assertEqual(report['untimed_records'],1)
        self.assertEqual(report['negative_iteration_markers'],2)
        self.assertEqual(report['rankings']['known_iteration_subtotal'],[])

    def test_shared_metadata_and_member_are_one_timing_role(self):
        member=self.member()
        sentinel={key:member[key] for key in ('completionTime','raHash','predicateName','appearsAs')}
        sentinel.update(evaluationStrategy='SENTINEL_EMPTY',sentinelRaHash='sentinel-id')
        report=self.project([sentinel,member])
        self.assertEqual(report['accepted_records'],2)
        self.assertEqual(report['reused_ra_hash_records'],1)
        self.assertEqual(len(report['rankings']['known_iteration_subtotal']),1)

    def test_reused_ra_hash_is_not_an_event_id_and_does_not_drop_official_records(self):
        for second in (self.row(), self.row(predicateName=SYNTHETIC_CANARY+'-another-name')):
            with self.subTest(second_name=second['predicateName']):
                report=self.project([self.row(),second])
                self.assertEqual(report['status'],'PROJECTED')
                self.assertEqual(report['reused_ra_hash_records'],1)
                self.assertEqual(report['accepted_records'],2)
                ranking=report['rankings']['official_millis']
                self.assertEqual({r['record'] for r in ranking},{1,2})
                self.assertEqual([r['reported_millis'] for r in ranking],[100,100])

    def test_unknown_shape_bad_type_nested_fields_and_bad_encoding_never_escape(self):
        mutations=[dict(millis=True),dict(millis=-1),dict(extra=SYNTHETIC_CANARY),dict(raHash=None),
                   dict(dependencies={SYNTHETIC_CANARY:17}),dict(appearsAs={SYNTHETIC_CANARY:{QUERY['path']:[True]}}),
                   dict(pipelineRuns=[{'unknown':SYNTHETIC_CANARY}]),dict(position='\ud800')]
        for mutation in mutations:
            with self.subTest(fields=sorted(mutation)):
                report=self.project([self.row(**mutation)])
                self.assertEqual(report['status'],'PARTIAL')
                self.assertEqual(report['accepted_records'],0)
        for suffix in (b'\n{"x":"'+SYNTHETIC_CANARY.encode()+b'",', b'\n\xff'+SYNTHETIC_CANARY.encode(),
                       b'\n{"millis":NaN}', b'\n{"x":1,"x":2}'):
            report=self.project([self.row()],suffix=suffix)
            self.assertEqual(report['status'],'PARTIAL')
            self.assertGreater(report['invalid_records'],0)

    def test_wrong_version_and_duplicate_header_remain_partial(self):
        header=dict(summaryLogVersion='0.4.0',codeqlVersion='2.27.3',startTime='now')
        self.assertEqual(self.project([self.row()],header=header)['status'],'PARTIAL')
        good=dict(summaryLogVersion='0.4.0',codeqlVersion='2.27.4',startTime='now')
        report=self.project([self.row(),good])
        self.assertEqual(report['status'],'PARTIAL')
        self.assertGreater(report['invalid_records'],0)

    def test_unobserved_incomplete_record_keeps_marker_without_claiming_compatible_shape(self):
        value=self.row(completionType='INCOMPLETE');del value['completionTime']
        report=self.project([value])
        self.assertEqual(report['status'],'PARTIAL')
        self.assertEqual(report['incomplete_predicates'],1)
        self.assertEqual(report['unknown_shapes'],1)
        self.assertEqual(report['rankings']['official_millis'],[])

    def test_missing_footer_stderr_and_unobserved_null_delta_are_partial(self):
        report=self.project([self.row()],stderr=b'ERROR: Reached end of log file before footer was found.\n')
        self.assertEqual(report['status'],'PARTIAL');self.assertTrue(report['input_footer_missing'])
        report=self.project([self.member(deltaSizes=[None])])
        self.assertEqual(report['status'],'PARTIAL')
        self.assertEqual(report['unobserved_delta_null_entries'],1)

    def test_pipeline_failure_or_exception_cannot_be_lost_in_implicit_success(self):
        report=self.project([self.member(pipelineRuns=[dict(raReference=SYNTHETIC_CANARY,completionType='FAILURE',exceptionName=SYNTHETIC_CANARY)])])
        self.assertEqual(report['status'],'PARTIAL')
        self.assertEqual(report['pipeline_non_success'],1)
        self.assertEqual(report['exception_records'],1)

    def test_real_pipe_limit_failures_keep_observed_bytes_and_sticky_partial_after_eof(self):
        prefix = (json.dumps(dict(summaryLogVersion='0.4.0',codeqlVersion='2.27.4',startTime='now'))+
                  '\n'+json.dumps(self.row())+'\n').encode()
        for mode in ('bytes','line','records'):
            with self.subTest(mode=mode), tempfile.TemporaryDirectory() as folder:
                root=Path(folder);out=root/'out';out.mkdir();ready=root/'parsed'
                extra=(SYNTHETIC_CANARY+'x'*2048+'\n').encode() if mode=='line' else (json.dumps(self.row(raHash='new-ra'))+'\n').encode()
                child=root/'producer.py'
                child.write_text('import os,time\nfrom pathlib import Path\n'+f'os.write(1,{prefix!r})\n'+
                    f'ready=Path({str(ready)!r})\nend=time.monotonic()+3\n'+
                    'while not ready.exists() and time.monotonic()<end:time.sleep(.01)\n'+f'os.write(1,{extra!r})\n')
                class Marked(profile.Summary):
                    def line(observer,name,data):
                        result=super().line(name,data)
                        if observer.accepted: ready.touch()
                        return result
                observer=Marked([QUERY])
                key,limit={'bytes':('RAW_CAP',len(prefix)+1),'line':('LINE_CAP',len(prefix)+1),
                           'records':('MAX_SUMMARY_RECORDS',1)}[mode]
                with patch.object(profile,key,limit):
                    with self.assertRaises(runtime.PhaseError) as caught:
                        runtime.run([sys.executable,str(child)],root,out/'summary.log',time.monotonic()+8,8,out,
                                    os.environ.copy(),observer=observer,restricted_output=True)
                report=caught.exception.result['output_observation']
                self.assertEqual(report['status'],'PARTIAL')
                self.assertIsNotNone(report['feed_failure'])
                stream=report['streams']['stdout']
                self.assertTrue(stream['final'])
                self.assertEqual(stream['bytes'],len(prefix)+len(extra))
                self.assertEqual(stream['sha256'],hashlib.sha256(prefix+extra).hexdigest())
                self.assertEqual(stream['processed_prefix_bytes'],len(prefix))
                for path in out.rglob('*'):
                    if path.is_file():self.assertNotIn(SYNTHETIC_CANARY.encode(),path.read_bytes())


if __name__ == '__main__':
    unittest.main()
