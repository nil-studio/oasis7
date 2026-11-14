#!/usr/bin/env python3
"""Candidate observation contracts with real pipes, seals and owned children."""
import errno
import fcntl
import gzip
import hashlib
import io
import json
import os
from datetime import datetime, timezone
from pathlib import Path
import random
import select
import signal
import subprocess
import sys
import tempfile
import time
import unittest
import zlib
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

    def test_native_identity_and_open_span_use_signed_raw_clock_without_footer(self):
        events = numeric_events()[:3]+[numeric_event('CACHE_LOOKUP', 4)]
        identity = SYNTHETIC_CANARY+'é'
        events[1]['raHash'] = identity
        for event, nano in zip(events, (-100, -50, -25, 0)):
            event['nanoTime'] = nano
        capture, report = self.observe(events)
        self.assertEqual(report['version'], 'candidate-numeric-v2')
        self.assertEqual(report['status'], 'PARTIAL')
        self.assertEqual(report['footer_count'], 0)
        self.assertEqual(report['last_observed_offset_ns'], 100)
        row = report['unfinished_predicates'][0]
        expected = hashlib.sha256(identity.encode()).hexdigest()
        self.assertEqual(row['ra_identity_sha256'], expected)
        self.assertEqual(row['predicate_start_offset_ns'], 50)
        self.assertEqual(row['observed_open_span_ns'], 50)
        self.assertEqual(report['unfinished_pipelines'][0]['ra_identity_sha256'], expected)
        self.assertEqual(report['unfinished_pipelines'][0]['predicate_start_offset_ns'], 50)
        self.assertEqual(capture.summary()['status'], 'CAPTURED')
        self.assertEqual(capture.summary()['log_boundary_status'], 'PARTIAL')
        self.assertNotIn(SYNTHETIC_CANARY, json.dumps(report))

    def test_same_alias_distinct_ra_and_reused_ra_keep_separate_completed_instances(self):
        events = numeric_events()[:1]
        for start, identity in ((2, 'physical-one'), (6, 'physical-two'), (10, 'physical-one')):
            events.extend((numeric_event('PREDICATE_STARTED', start, raHash=identity,
                appearsAs={'TypeInference::inferType/2#same': [1]}, predicateType='RECURSIVE_INTENSIONAL'),
                numeric_event('PIPELINE_STARTED', start+1, predicateStartEvent=start, generationNumber=1),
                numeric_event('PIPELINE_COMPLETED', start+2, startEvent=start+1, resultSize=1),
                numeric_event('PREDICATE_COMPLETED', start+3, startEvent=start, resultSize=1)))
        events.append(numeric_event('LOG_FOOTER', 14))
        _, report = self.observe(events)
        self.assertEqual(report['status'], 'OBSERVED')
        self.assertEqual(report['predicate_completed'], 3)
        self.assertEqual(len(report['aliases']), 1)
        expected = [hashlib.sha256(value.encode()).hexdigest()
                    for value in ('physical-one', 'physical-two', 'physical-one')]
        for key in ('predicate_tail', 'pipeline_tail'):
            self.assertEqual([row['predicate_event'] for row in report[key]], [2, 6, 10])
            self.assertEqual([row['ra_identity_sha256'] for row in report[key]], expected)
            self.assertEqual([row['predicate_start_offset_ns'] for row in report[key]], [100, 500, 900])
        summary_fixture = SummaryTests()
        summary = summary_fixture.project([summary_fixture.row(raHash='physical-one')])
        self.assertEqual(summary['rankings']['official_millis'][0]['ra_identity_sha256'], expected[0])

    def test_pipeline_identity_survives_predicate_tail_and_receipt_trimming(self):
        events = numeric_events()[:4]
        events[1]['raHash'] = SYNTHETIC_CANARY
        events.append(numeric_event('PREDICATE_COMPLETED', 5, startEvent=2, resultSize=0))
        for start in range(6, 26, 2):
            events.extend((numeric_event('PREDICATE_STARTED', start, raHash='other',
                appearsAs=events[1]['appearsAs'], predicateType='STAGE_RESULT'),
                numeric_event('PREDICATE_COMPLETED', start+1, startEvent=start, resultSize=0)))
        events.append(numeric_event('LOG_FOOTER', 26))
        _, report = self.observe(events)
        self.assertNotIn(2, [row['predicate_event'] for row in report['predicate_tail']])
        expected = hashlib.sha256(SYNTHETIC_CANARY.encode()).hexdigest()
        for row in report['pipeline_tail']:
            self.assertEqual((row['predicate_event'], row['ra_identity_sha256'], row['predicate_start_offset_ns']),
                             (2, expected, 100))
        with patch.object(profile, 'NUMERIC_RECEIPT_CAP', 3000):
            _, bounded = self.observe(events)
        self.assertGreater(bounded['receipt_omitted_samples'], 0)
        self.assertLessEqual(len((json.dumps(bounded, indent=2)+'\n').encode()), 3000)
        self.assertEqual(bounded['status'], 'PARTIAL')
        self.assertEqual(bounded['pipeline_tail'][0]['ra_identity_sha256'], expected)
        self.assertEqual(bounded['pipeline_tail'][0]['predicate_start_offset_ns'], 100)
        self.assertNotIn(SYNTHETIC_CANARY, json.dumps(bounded))

    def test_optional_ra_keeps_old_acceptance_and_numeric_string_limit(self):
        for identity in (None, '', SYNTHETIC_CANARY*400):
            with self.subTest(identity_bytes=0 if identity is None else len(identity)):
                events = numeric_events()
                if identity is not None: events[1]['raHash'] = identity
                _, report = self.observe(events)
                self.assertEqual(report['status'], 'OBSERVED')
                self.assertFalse(any(report['errors'].values()))
                expected = hashlib.sha256(identity.encode()).hexdigest() if identity else None
                for key in ('predicate_tail', 'pipeline_tail'):
                    self.assertTrue(all(row['ra_identity_sha256'] == expected for row in report[key]))
                self.assertNotIn(SYNTHETIC_CANARY, json.dumps(report))

    def test_missing_and_repeated_headers_do_not_invent_or_reset_native_origin(self):
        events = numeric_events()[:2]
        events[0]['nanoTime'], events[1]['nanoTime'] = -100, -50
        _, missing = self.observe(events[1:])
        self.assertIsNone(missing['last_observed_offset_ns'])
        self.assertIsNone(missing['unfinished_predicates'][0]['predicate_start_offset_ns'])
        self.assertIsNone(missing['unfinished_predicates'][0]['observed_open_span_ns'])
        for version in ('2.27.4', 'wrong'):
            repeated = dict(events[0], eventId=20, nanoTime=9000, codeqlVersion=version)
            _, report = self.observe(events+[repeated])
            self.assertEqual(report['last_observed_offset_ns'], 50)
            self.assertEqual(report['unfinished_predicates'][0]['predicate_start_offset_ns'], 50)
            self.assertEqual(report['unfinished_predicates'][0]['observed_open_span_ns'], 0)
            self.assertGreater(report['errors']['HEADER'], 0)
        _, report = self.observe(events+[dict(events[0], eventId=20, nanoTime=9000),
            dict(numeric_event('CACHE_LOOKUP', 21), nanoTime=0)])
        self.assertEqual(report['last_observed_offset_ns'], 100)
        self.assertEqual(report['unfinished_predicates'][0]['observed_open_span_ns'], 50)

    def test_rejected_event_never_advances_last_native_time_and_next_valid_event_can(self):
        prefix = numeric_events()[:4]
        for event, nano in zip(prefix, (-100, -50, -25, 0)):
            event['nanoTime'] = nano
        bad = [numeric_event('CACHE_LOOKUP', 11, unknown=SYNTHETIC_CANARY),
               numeric_event('CACHE_LOOKUP', 11, raHash=None),
               numeric_event('PREDICATE_STARTED', 11, appearsAs=prefix[1]['appearsAs'], predicateType='BAD'),
               numeric_event('PREDICATE_COMPLETED', 11, startEvent=2, completionType='BAD'),
               numeric_event('PREDICATE_COMPLETED', 11, startEvent=2, completionType='FAILURE'),
               numeric_event('PREDICATE_COMPLETED', 11, startEvent=2, completionType='INCOMPLETE'),
               numeric_event('PIPELINE_STARTED', 11, predicateStartEvent=4),
               numeric_event('PIPELINE_STARTED', 11, predicateStartEvent=2, generationNumber=-1),
               numeric_event('PREDICATE_COMPLETED', 11, startEvent=2, resultSize=-1),
               dict(prefix[1])]
        missing_nano = numeric_event('CACHE_LOOKUP', 11); missing_nano.pop('nanoTime')
        bad.extend((missing_nano, dict(numeric_event('CACHE_LOOKUP', 11), nanoTime=2**63),
                    dict(numeric_event('CACHE_LOOKUP', 11), nanoTime=-(2**63)-1)))
        for rejected in bad:
            with self.subTest(kind=rejected['type'], keys=sorted(rejected)):
                _, report = self.observe(prefix+[rejected])
                self.assertEqual(report['status'], 'PARTIAL')
                self.assertEqual(report['last_observed_offset_ns'], 100)
                self.assertTrue(any(report['errors'].values()))
                _, recovered = self.observe(prefix+[rejected,
                    dict(numeric_event('CACHE_LOOKUP', 12), nanoTime=50)])
                self.assertEqual(recovered['last_observed_offset_ns'], 150)

    def test_last_native_event_is_literal_not_maximum_and_each_interval_can_recover(self):
        projection = profile.NumericProjection()
        events = numeric_events()[:2]
        events[0]['nanoTime'], events[1]['nanoTime'] = 0, 10
        for event in events: projection.accept(event)
        for index, (nano, offset, span) in enumerate(((50, 50, 40), (40, 40, 30), (5, 5, None),
                                                     (20, 20, 10), (-5, None, None), (30, 30, 20)), 3):
            projection.accept(dict(numeric_event('CACHE_LOOKUP', index), nanoTime=nano))
            report = projection.report()
            self.assertEqual(report['last_observed_offset_ns'], offset)
            self.assertEqual(report['unfinished_predicates'][0]['observed_open_span_ns'], span)
            self.assertEqual(report['unfinished_predicates'][0]['predicate_start_offset_ns'], 10)
            self.assertFalse(any(report['errors'].values()))

    def test_native_offsets_and_open_span_check_each_signed_int64_difference(self):
        cases = ((-(2**63), -(2**63)+1, 2**63-1, 1, None, None),
                 (0, -(2**63), 0, None, 0, None), (0, -1, 0, None, 0, 1),
                 (2**63-1, 2**63-1, 2**63-1, 0, 0, 0))
        for header, start, last, offset, last_offset, span in cases:
            events = numeric_events()[:2]+[numeric_event('CACHE_LOOKUP', 3)]
            for event, nano in zip(events, (header, start, last)): event['nanoTime'] = nano
            _, report = self.observe(events)
            self.assertEqual(report['last_observed_offset_ns'], last_offset)
            self.assertEqual(report['unfinished_predicates'][0]['predicate_start_offset_ns'], offset)
            self.assertEqual(report['unfinished_predicates'][0]['observed_open_span_ns'], span)
            self.assertFalse(any(report['errors'].values()))

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
            _, unfinished = self.observe(numeric_events()[:-1])
        self.assertEqual(report['status'], 'PARTIAL')
        self.assertEqual(report['remembered_target_events'], 2)
        self.assertGreater(report['errors']['TABLE_LIMIT'], 0)
        self.assertEqual(unfinished['last_observed_offset_ns'], 200)

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

    def test_timeout_without_footer_keeps_native_open_span_after_capacity_drain(self):
        events = numeric_events()[:3]+[numeric_event('CACHE_LOOKUP', 4)]
        events[1]['raHash'] = SYNTHETIC_CANARY
        for event, nano in zip(events, (-100, -50, -25, 0)): event['nanoTime'] = nano
        prefix, tail = numeric_bytes(events[:3])+b'\n', numeric_bytes(events[3:])
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
        self.assertEqual(caught.exception.result['exit'], 143)
        report = projection.report()
        self.assertEqual(report['status'], 'PARTIAL')
        self.assertEqual(report['footer_count'], 0)
        self.assertEqual(report['last_observed_offset_ns'], 100)
        self.assertEqual(report['unfinished_predicates'][0]['observed_open_span_ns'], 50)
        self.assertEqual(report['unfinished_pipelines'][0]['ra_identity_sha256'],
                         hashlib.sha256(SYNTHETIC_CANARY.encode()).hexdigest())
        self.assertGreater(report['post_limit_records'], 0)
        self.assertEqual(self.cap.summary()['error'], 'MEMORY_LIMIT')
        self.assertTrue(self.cap.summary()['transport_eof'])
        self.assert_private()

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
        observation.summary_result = dict(status='PROJECTED', replay=dict(status='COMPLETE'))
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
        with patch.object(profile, 'CompressedCapture') as capture:
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
        real_capture = profile.CompressedCapture
        with patch.object(profile, 'CompressedCapture', side_effect=lambda **kwargs: real_capture(4, **kwargs)):
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
                observation.summary_result = dict(status='PROJECTED', replay=dict(status='COMPLETE'))
                result = observation.finish()
                self.assertEqual(result['status'], 'COMPLETE')
                self.assertEqual(result['numeric_status'], expected)
                numeric = observation.out/'numeric-projection.json'
                self.assertLessEqual(numeric.stat().st_size, runtime.MAX_CONTROL_RECEIPT)
                self.assertNotIn(SYNTHETIC_CANARY.encode(), numeric.read_bytes())
                observation.close()
                for path in observation.out.iterdir(): path.unlink()
                observation.out.rmdir()

    def test_summary_admission_limit_keeps_numeric_and_original_batch_failure(self):
        observation = self.observation()
        observation.capture.feed(RAW); observation.capture.finish(True, True)
        original = dict(exit=143, failure_kind='timeout', cleanup_errors=[], evidence_errors=[])
        observation.batch_result = original; observation.batch_safe = True
        with patch.object(profile, 'SUMMARY_WORK_CAP', 1), patch.object(runtime, 'run') as run:
            observation.summarize('/cli', self.root, {}, time.monotonic()+100, admitted=True)
            run.assert_not_called()
        self.assertIs(observation.batch_result, original)
        self.assertEqual(observation.summary_result['failure_kind'], 'SUMMARY_ADMISSION_ERROR')
        self.assertIn(observation.summary_result['first_state_limit']['kind'], ('DECODE_PEAK', 'PARSE_PEAK'))
        result = observation.finish()
        self.assertEqual(result['status'], 'INCOMPLETE')
        self.assertNotEqual(result['numeric_status'], 'NOT_REACHED')
        for path in observation.out.iterdir():
            self.assertNotIn(SYNTHETIC_CANARY.encode(), path.read_bytes())


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

    def test_observed_3744_iteration_shape_keeps_all_literals_and_markers(self):
        value = self.member(predicateIterationMillis=[12345]*1232+[-1]*2512,
                            deltaSizes=[67890]*1232+[-1]*2512)
        report = self.project([self.row(), value])
        self.assertEqual(report['status'], 'PROJECTED')
        row = report['rankings']['known_iteration_subtotal'][0]
        self.assertEqual(row['known_iteration_entries'], 1232)
        self.assertEqual(row['negative_iteration_markers'], 2512)
        self.assertEqual(row['known_iteration_subtotal'], 12345*1232)
        self.assertEqual(report['unknown_delta_entries'], 2512)

    def accounting_record(self, *, punctuation=False):
        # Known public schema, with every key/value below the existing 8192B
        # bound. This is not a reconstruction of a private rejected record.
        content = (':,{[' if punctuation else 'abcd') * 1950
        return self.row(dependencies={f'public-key-{index:02d}-'+content: 'public-identity'
                                      for index in range(24)})

    def accounting_stream(self, data, *, chunk_size=65536):
        observer = profile.Summary([QUERY])
        header = json.dumps(dict(summaryLogVersion='0.4.0', codeqlVersion='2.27.4', startTime='now')).encode()+b'\n'
        observer.feed('stdout', header)
        for offset in range(0, len(data), chunk_size):
            observer.feed('stdout', data[offset:offset+chunk_size])
        observer.feed('stdout', b'', final=True)
        observer.feed('stderr', b'', final=True)
        report = observer.summary()
        self.assertEqual(report['streams']['stdout']['sha256'], hashlib.sha256(header+data).hexdigest())
        self.assertNotIn(SYNTHETIC_CANARY, json.dumps(report))
        self.assertNotIn('public-key-', json.dumps(report))
        return report

    def test_large_ascii_and_punctuation_records_fit_the_original_state_budget(self):
        for punctuation in (False, True):
            with self.subTest(punctuation=punctuation):
                data = json.dumps(self.accounting_record(punctuation=punctuation), separators=(',', ':')).encode()+b'\n'
                self.assertGreater(len(data), 128*1024)
                self.assertLess(len(data), 2*1024**2)
                report = self.accounting_stream(data)
                self.assertEqual(report['status'], 'PROJECTED')
                self.assertEqual(report['accepted_records'], 1)
                self.assertEqual(report['rankings']['official_millis'][0]['dependencies_count'], 24)
                self.assertEqual(report['state_limits']['active_bytes'], 4*1024**2)
                self.assertIsNone(report['first_state_limit'])

    def test_literal_and_escaped_punctuation_keep_the_same_projection(self):
        value = self.row(predicateName=':'*4000)
        literal = json.dumps(value, separators=(',', ':')).encode()
        escaped = literal.replace(b':'*4000, b'\\u003a'*4000)
        self.assertEqual(json.loads(literal), json.loads(escaped))
        left = self.accounting_stream(literal+b'\n')
        right = self.accounting_stream(escaped+b'\n')
        self.assertEqual(left['status'], 'PROJECTED')
        self.assertEqual(right['status'], 'PROJECTED')
        self.assertEqual(left['rankings'], right['rankings'])

    def test_unicode_widths_and_surrogates_keep_validation_and_identity(self):
        for character in ('é', '漢', '🙂', '"\\\b\n\t'):
            with self.subTest(character=character):
                value = self.row(predicateName=character*1000, position=character*500)
                literal = self.accounting_stream(json.dumps(value, ensure_ascii=False).encode()+b'\n')
                escaped = self.accounting_stream(json.dumps(value, ensure_ascii=True).encode()+b'\n')
                self.assertEqual(literal['status'], 'PROJECTED')
                self.assertEqual(escaped['status'], 'PROJECTED')
                self.assertEqual(literal['rankings'], escaped['rankings'])
        for invalid in ('\ud800', '\udc00', '\ud800x'):
            report = self.accounting_stream(json.dumps(self.row(position=invalid)).encode()+b'\n')
            self.assertEqual(report['status'], 'PARTIAL')
            self.assertEqual(report['invalid_records'], 1)

    def test_late_unicode_source_does_not_charge_whitespace_as_tree_nodes(self):
        for character, padding in (('a', 300000), ('漢', 260000), ('🙂', 170000)):
            with self.subTest(character=character):
                data = b' '*padding+json.dumps(self.row(position=character), ensure_ascii=False).encode()+b'\n'
                report = self.accounting_stream(data)
                self.assertEqual(report['status'], 'PROJECTED')
                self.assertEqual(report['accepted_records'], 1)

    def test_large_admitted_record_does_not_hide_late_unknown_duplicate_or_truncation(self):
        first = json.dumps(self.accounting_record()).encode()
        unknown = json.dumps(self.row(isCached=True)).encode()
        duplicate = json.dumps(self.row()).encode()[:-1]+b',"\\u006d\\u0069llis":7}'
        data = b'\n'.join((first, unknown, duplicate, b'{"raHash":"truncated'))
        report = self.accounting_stream(data)
        self.assertEqual(report['status'], 'PARTIAL')
        self.assertEqual(report['accepted_records'], 1)
        self.assertEqual(report['unknown_shapes'], 1)
        self.assertEqual(report['invalid_records'], 2)

    def test_late_wide_source_accounts_for_decoder_upgrade_before_json(self):
        observer = profile.Summary([QUERY])
        data = b' '*180000+json.dumps(self.row(position='🙂'), ensure_ascii=False).encode()+b'\n'
        with patch.object(profile, 'parse_json') as parse:
            with self.assertRaisesRegex(profile.ObservationError, '^SUMMARY_STATE_LIMIT$'):
                for offset in range(0, len(data), 65536): observer.feed('stdout', data[offset:offset+65536])
            parse.assert_not_called()
        observer.feed('stdout', b'', final=True); observer.feed('stderr', b'', final=True)
        report = observer.summary()
        limit = report['first_state_limit']
        self.assertEqual(limit['kind'], 'DECODE_PEAK')
        self.assertEqual(limit['source_kind'], 4)
        self.assertEqual(limit['decode_peak_bytes'], 9*(len(data)-1))
        self.assertEqual(report['records'], 0)
        self.assertEqual(report['status'], 'PARTIAL')

    def test_first_state_limit_is_bounded_sticky_and_keeps_full_drain_identity(self):
        observer = profile.Summary([QUERY])
        data = b'{"values":['+b'0,'*24000+b'0]}\n'
        with patch.object(profile, 'parse_json') as parse:
            with self.assertRaisesRegex(profile.ObservationError, '^SUMMARY_STATE_LIMIT$'):
                observer.feed('stdout', data)
            parse.assert_not_called()
        tail = SYNTHETIC_CANARY.encode()*3000
        observer.feed('stdout', tail, final=True); observer.feed('stderr', b'', final=True)
        report = observer.summary(); limit = report['first_state_limit']
        self.assertEqual(limit['kind'], 'PARSE_PEAK')
        self.assertEqual(limit['record_bytes'], len(data)-1)
        self.assertEqual(limit['commas'], 24000)
        self.assertGreater(limit['parse_peak_bytes'], limit['available_bytes'])
        self.assertLessEqual(len(json.dumps(limit).encode()), 2048)
        self.assertNotIn(SYNTHETIC_CANARY, json.dumps(report))
        self.assertEqual(report['streams']['stdout']['sha256'], hashlib.sha256(data+tail).hexdigest())
        self.assertEqual(report['streams']['stdout']['processed_prefix_bytes'], 0)
        self.assertEqual(report['status'], 'PARTIAL')

    def test_limit_diagnostic_failure_cannot_replace_the_original_limit(self):
        observer = profile.Summary([QUERY])
        with patch.object(observer, '_remember_limit', side_effect=OSError('public-injected')):
            with self.assertRaisesRegex(profile.ObservationError, '^SUMMARY_STATE_LIMIT$'):
                observer.feed('stdout', b'x'*65537)
        observer.feed('stdout', b'', final=True); observer.feed('stderr', b'', final=True)
        report = observer.summary()
        self.assertEqual(report['feed_failure'], 'SUMMARY_STATE_LIMIT')
        self.assertIsNone(report['first_state_limit'])
        self.assertEqual(report['status'], 'PARTIAL')

    def large_prefix(self):
        """Public small records: actual cumulative bytes, without patching RAW_CAP."""
        observer = profile.Summary([QUERY])
        header = json.dumps(dict(summaryLogVersion='0.4.0',codeqlVersion='2.27.4',startTime='now')).encode()+b'\n'
        observer.feed('stdout', header)
        expected = hashlib.sha256(header)
        row = (json.dumps(self.row(predicateName='public-fixture-'+'x'*7000))+'\n').encode()
        count = profile.RAW_CAP//len(row)+1
        for _ in range(count):
            observer.feed('stdout', row); expected.update(row)
        self.assertGreater(observer.streams['stdout']['bytes'], 48*1024**2)
        self.assertEqual(observer.accepted, count)
        self.assertEqual(observer.streams['stdout']['sha'].hexdigest(),expected.hexdigest())
        return observer, count

    def test_cumulative_stream_over_48mib_keeps_full_identity_and_first_ra_ordinal(self):
        observer, count = self.large_prefix()
        last = (json.dumps(self.row(millis=999, raHash='physical-relation-1'))+'\n').encode()
        observer.feed('stdout', last)
        observer.feed('stdout', b'', final=True); observer.feed('stderr', b'', final=True)
        report = observer.summary()
        self.assertEqual(report['status'], 'PROJECTED')
        self.assertEqual(report['accepted_records'], count+1)
        self.assertEqual(report['reused_ra_hash_records'], count)
        self.assertEqual(report['distinct_ra_hashes'], 1)
        self.assertEqual(report['rankings']['official_millis'][0]['record'], count+1)
        self.assertEqual(report['rankings']['official_millis'][0]['first_record_with_ra_hash'], 1)
        self.assertEqual(len(report['rankings']['official_millis']), 50)
        self.assertNotIn(SYNTHETIC_CANARY, json.dumps(report))

    def test_late_unknown_failure_invalid_and_truncated_records_after_48mib_are_checked(self):
        observer, count = self.large_prefix()
        unknown = self.row(isCached=True)
        invalid = self.row(millis=True)
        failure = self.row(completionType='FAILURE', exceptionName=SYNTHETIC_CANARY)
        suffix = b'\n'.join(json.dumps(row).encode() for row in (unknown,invalid,failure))
        suffix += b'\n{"raHash":"'+SYNTHETIC_CANARY.encode()
        observer.feed('stdout', suffix, final=True); observer.feed('stderr', b'', final=True)
        report = observer.summary()
        self.assertEqual(report['status'], 'PARTIAL')
        self.assertGreaterEqual(report['records'], count+3)
        self.assertGreater(report['unknown_shapes'], 0)
        self.assertGreaterEqual(report['invalid_records'], 2)
        self.assertEqual(report['failed_predicates'], 1)
        self.assertEqual(report['exception_records'], 1)
        self.assertNotIn(SYNTHETIC_CANARY, json.dumps(report))

    def test_post_48mib_record_line_and_state_limits_stay_partial_during_drain(self):
        for mode in ('records', 'line', 'state'):
            with self.subTest(mode=mode):
                observer, count = self.large_prefix()
                prefix_bytes = observer.streams['stdout']['bytes']
                expected_hash = observer.streams['stdout']['sha'].copy()
                suffix = (json.dumps(self.row(raHash='new-ra'))+'\n').encode()
                key, limit = {'records':('MAX_SUMMARY_RECORDS',count),
                              'line':('LINE_CAP',len(suffix)-2),
                              'state':('SUMMARY_RA_LIMIT',1)}[mode]
                with patch.object(profile,key,limit), self.assertRaises(profile.ObservationError):
                    observer.feed('stdout',suffix)
                tail = b'not parsed after fail-closed\n'
                observer.feed('stdout',tail,final=True); observer.feed('stderr',b'',final=True)
                expected_hash.update(suffix+tail)
                report = observer.summary(); stream = report['streams']['stdout']
                self.assertEqual(report['status'],'PARTIAL')
                self.assertIsNotNone(report['feed_failure'])
                if mode == 'state':
                    self.assertEqual(report['first_state_limit']['kind'], 'RA_CAPACITY')
                    self.assertEqual(report['first_state_limit']['ra_count'], 1)
                self.assertEqual(stream['bytes'],prefix_bytes+len(suffix)+len(tail))
                self.assertEqual(stream['processed_prefix_bytes'],prefix_bytes)
                self.assertEqual(stream['sha256'],expected_hash.hexdigest())
                self.assertEqual(stream['pending_bytes'],0)

    def test_summary_ra_full_digest_collision_probe_and_saturation_keep_first_ordinal(self):
        # Equal slot prefix is not equality. Saturation still permits old keys.
        with patch.object(profile,'SUMMARY_RA_LIMIT',3), patch.object(profile,'SUMMARY_RA_SLOTS',4):
            index = profile.SummaryRA()
            digests = [b'\0'*4+bytes([value])*28 for value in range(4)]
            for ordinal, identity in enumerate(digests[:3],1):
                self.assertEqual(index.first(identity,ordinal),(ordinal,False))
            self.assertEqual(index.first(digests[0],99),(1,True))
            with self.assertRaisesRegex(profile.ObservationError,'^SUMMARY_STATE_LIMIT$'):
                index.first(digests[3],100)
            self.assertEqual(len(index),3)
        with patch.object(profile,'SUMMARY_RA_LIMIT',4), patch.object(profile,'SUMMARY_RA_SLOTS',2):
            index = profile.SummaryRA()
            index.first(digests[0],1); index.first(digests[1],2)
            with self.assertRaisesRegex(profile.ObservationError,'^SUMMARY_STATE_LIMIT$'):
                index.first(digests[2],3)  # full probe cycle is bounded, never hangs
        with patch.object(profile,'SUMMARY_RA_PROBES',2):
            index = profile.SummaryRA()
            index.first(digests[0],1); index.first(digests[1],2)
            with self.assertRaisesRegex(profile.ObservationError,'^SUMMARY_STATE_LIMIT$'):
                index.first(digests[2],3)  # free capacity cannot extend the per-row probe bound

    def test_parse_state_preflight_and_both_stream_pendings_share_the_same_budget(self):
        observer = profile.Summary([QUERY])
        data = b'{"values":['+b'0,'*24000+b'0]}\n'
        with patch.object(profile,'parse_json') as parse:
            with self.assertRaisesRegex(profile.ObservationError,'^SUMMARY_STATE_LIMIT$'):
                observer.feed('stdout',data)
            parse.assert_not_called()
        observer.feed('stdout',b'',final=True); observer.feed('stderr',b'',final=True)
        self.assertEqual(observer.summary()['status'],'PARTIAL')
        observer = profile.Summary([QUERY])
        chunk = b'x'*(64*1024)
        for _ in range(8): observer.feed('stdout',chunk)
        for _ in range(4): observer.feed('stderr',chunk)
        with self.assertRaisesRegex(profile.ObservationError,'^SUMMARY_STATE_LIMIT$'):
            observer.feed('stderr',b'x')
        observer.feed('stdout',b'',final=True); observer.feed('stderr',b'',final=True)
        report = observer.summary()
        self.assertEqual(report['status'],'PARTIAL')
        self.assertTrue(all(stream['pending_bytes']==0 for stream in report['streams'].values()))
        self.assertEqual(report['state_limits']['active_bytes'],4*1024**2)
        self.assertEqual(report['first_state_limit']['kind'],'FRAMING')
        self.assertEqual(report['first_state_limit']['available_bytes'],profile.SUMMARY_WORK_CAP)

    def test_late_stderr_footer_error_after_48mib_cannot_certify_full_summary(self):
        observer, _ = self.large_prefix()
        observer.feed('stdout',b'',final=True)
        observer.feed('stderr',b'ERROR: Reached end of log file before footer was found.\n',final=True)
        report = observer.summary()
        self.assertEqual(report['status'],'PARTIAL')
        self.assertTrue(report['input_footer_missing'])
        self.assertIs(report,observer.summary())
        self.assertIsNone(observer.seen)
        self.assertFalse(any(observer.tops.values()))

    def test_non_eof_terminal_snapshot_releases_state_without_faking_transport_completion(self):
        observer = profile.Summary([QUERY])
        header = dict(summaryLogVersion='0.4.0',codeqlVersion='2.27.4',startTime='now')
        observer.feed('stdout',(json.dumps(header)+'\n'+json.dumps(self.row())+'\n').encode())
        observer.feed('stdout',b'{"truncated":')
        observer.feed('stderr',b'not drained')
        seen = observer.seen
        report = observer.summary()
        self.assertEqual(report['status'],'PARTIAL')
        self.assertEqual(report['streams']['stdout']['pending_bytes'],len(b'{"truncated":'))
        self.assertEqual(report['streams']['stderr']['pending_bytes'],len(b'not drained'))
        self.assertFalse(any(stream['final'] for stream in report['streams'].values()))
        self.assertEqual(report['distinct_ra_hashes'],len(seen))
        self.assertIsNone(observer.seen)
        self.assertFalse(any(observer.tops.values()))
        self.assertFalse(any(stream['pending'] for stream in observer.streams.values()))
        self.assertIs(observer.summary(),report)
        with self.assertRaisesRegex(profile.ObservationError,'^SUMMARY_AFTER_FINALIZATION$'):
            observer.feed('stdout',b'late data',final=True)
        self.assertIs(observer.summary(),report)

    def test_real_pipe_limit_failures_keep_observed_bytes_and_sticky_partial_after_eof(self):
        prefix = (json.dumps(dict(summaryLogVersion='0.4.0',codeqlVersion='2.27.4',startTime='now'))+
                  '\n'+json.dumps(self.row())+'\n').encode()
        for mode in ('state','line','records'):
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
                key,limit={'state':('SUMMARY_RA_LIMIT',1),'line':('LINE_CAP',len(prefix)+1),
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


class CompressedCaptureTests(unittest.TestCase):
    def capture(self, *args, **kwargs):
        value = profile.CompressedCapture(*args, **kwargs)
        self.addCleanup(value.close)
        return value

    def test_lossless_stream_larger_than_raw_reserve_stays_bounded_and_sealed(self):
        capture = self.capture()
        expected = hashlib.sha256(); observed = 0
        line = json.dumps(dict(type='PUBLIC_FIXTURE', payload=SYNTHETIC_CANARY+'x'*60000)).encode()+b'\n'
        def feed(data):
            nonlocal observed
            capture.feed(data); expected.update(data); observed += len(data)
        feed(json.dumps(HEADER).encode()+b'\n')
        for _ in range(profile.RAW_CAP//len(line)+2): feed(line)
        feed(json.dumps(FOOTER).encode())
        self.assertFalse(capture.summary()['encoding_finished'])
        capture.finish(True, True)
        report = capture.summary()
        self.assertGreater(observed, profile.RAW_CAP)
        self.assertEqual(report['status'], 'CAPTURED')
        self.assertEqual(report['log_boundary_status'], 'COMPLETE')
        self.assertEqual(report['observed_bytes'], observed)
        self.assertEqual(report['encoding_input_bytes'], observed)
        self.assertEqual(report['observed_sha256'], expected.hexdigest())
        self.assertEqual(report['encoding_input_sha256'], expected.hexdigest())
        self.assertEqual(report['encoded_limit_bytes']+report['codec_buffer_reserve_bytes'], profile.RAW_CAP)
        self.assertEqual(report['memory_reserve_bytes'], 48*1024**2)
        self.assertEqual(report['codec'], dict(level=1, window_bits=31, memory_level=8))
        self.assertLess(report['encoded_bytes'], profile.ENCODED_CAP)
        encoded = os.pread(capture.fd, report['encoded_bytes'], 0)
        self.assertEqual(report['encoded_sha256'], hashlib.sha256(encoded).hexdigest())
        decoded = hashlib.sha256(); size = 0
        with gzip.GzipFile(fileobj=io.BytesIO(encoded)) as source:
            for chunk in iter(lambda: source.read(65536), b''):
                decoded.update(chunk); size += len(chunk)
        self.assertEqual((size, decoded.hexdigest()), (observed, expected.hexdigest()))
        self.assertEqual(fcntl.fcntl(capture.fd, profile.GET_SEALS), profile.SEALS)
        with self.assertRaises(OSError): os.write(capture.fd, b'x')
        with self.assertRaises(profile.ObservationError): _ = capture.path

    def test_encoded_feed_limit_is_sticky_and_numeric_drain_keeps_complete_identity(self):
        numeric = profile.NumericProjection()
        capture = self.capture(512, projection=numeric)
        events = numeric_events()
        header = numeric_bytes(events[:1])+b'\n'
        noise = numeric_bytes([numeric_event('CACHE_LOOKUP',11,raHash=random.Random(7).randbytes(24000).hex())])+b'\n'
        tail = numeric_bytes(events[1:])
        capture.feed(header); capture.feed(noise)
        kept = capture.retained; kept_hash = capture.retained_hash.hexdigest()
        self.assertEqual(capture.error, 'MEMORY_LIMIT')
        capture.feed(tail); capture.feed(b' '); capture.finish(True, True)
        report = capture.summary(); raw = header+noise+tail+b' '
        self.assertEqual(report['capacity_failure_phase'], 'FEED')
        self.assertEqual(report['status'], 'INCOMPLETE')
        self.assertFalse(report['encoding_finished'])
        self.assertEqual((report['encoded_bytes'], report['encoded_sha256']), (kept, kept_hash))
        self.assertEqual((report['observed_bytes'], report['observed_sha256']),
                         (len(raw), hashlib.sha256(raw).hexdigest()))
        self.assertEqual(numeric.report()['status'], 'OBSERVED')
        cost = capture.feed_cost_summary()
        self.assertEqual(cost['version'], 'compressed-capture-feed-cost-v1')
        self.assertEqual([cost['buckets'][key]['calls'] for key in ('BEFORE_LIMIT','CAP_CROSSING','AFTER_LIMIT')], [1,1,2])
        self.assertEqual(sum(value['input_bytes'] for value in cost['buckets'].values()), len(raw))
        with self.assertRaises(profile.ObservationError): profile.GzipReplay(capture, time.monotonic()+2)

    def test_final_flush_limit_does_not_turn_an_unfinished_encoding_into_capture(self):
        capture = self.capture(10, projection=profile.NumericProjection())
        capture.feed(RAW)
        self.assertIsNone(capture.error)
        self.assertEqual(capture.summary()['encoding_input_bytes'], len(RAW))
        capture.finish(True, True)
        report = capture.summary()
        self.assertEqual((report['error'], report['capacity_failure_phase']), ('MEMORY_LIMIT','FINISH'))
        self.assertFalse(report['encoding_finished'])
        self.assertEqual(report['status'], 'INCOMPLETE')
        self.assertEqual(report['observed_sha256'], hashlib.sha256(RAW).hexdigest())
        self.assertEqual(report['encoded_bytes'], 10)
        costs = capture.feed_cost_summary()['buckets']
        self.assertEqual(costs['BEFORE_LIMIT']['calls'], 1)
        self.assertEqual(costs['CAP_CROSSING']['calls'], 0)
        with self.assertRaises(profile.ObservationError): profile.GzipReplay(capture, time.monotonic()+2)

    def test_encoded_short_write_and_write_error_never_claim_complete_identity(self):
        real_write = os.write
        for mode in ('short', 'error'):
            with self.subTest(mode=mode):
                capture = self.capture()
                def failed_write(fd, data):
                    if fd != capture.fd: return real_write(fd, data)
                    if mode == 'error': raise OSError(SYNTHETIC_CANARY)
                    return real_write(fd, data[:5])
                with patch.object(os, 'write', side_effect=failed_write):
                    with self.assertRaises(profile.ObservationError): capture.feed(RAW)
                capture.finish(True, True)
                report = capture.summary()
                self.assertEqual(report['status'], 'INCOMPLETE')
                self.assertEqual(report['observed_sha256'], hashlib.sha256(RAW).hexdigest())
                self.assertNotIn(SYNTHETIC_CANARY, json.dumps(report))
                if mode == 'short':
                    self.assertEqual(report['error'], 'SHORT_MEMORY_WRITE')
                    self.assertEqual(report['encoded_bytes'], 5)
                    self.assertEqual(report['encoded_sha256'], hashlib.sha256(os.pread(capture.fd,5,0)).hexdigest())
                else:
                    self.assertEqual(report['error'], 'MEMORY_WRITE_FAILED')
                    self.assertIsNone(report['encoded_sha256'])


class GzipReplayTests(unittest.TestCase):
    def capture(self, raw=RAW):
        capture = profile.CompressedCapture(); self.addCleanup(capture.close)
        capture.feed(raw); capture.finish(True, True)
        return capture

    def replay(self, capture):
        replay = profile.GzipReplay(capture, time.monotonic()+4)
        self.addCleanup(replay.close)
        return replay

    def consume(self, replay):
        digest = hashlib.sha256(); size = 0
        replay.start(time.monotonic()+2)
        until = time.monotonic()+3
        while time.monotonic() < until:
            try: chunk = os.read(replay.read_fd, 8192)
            except BlockingIOError:
                select.select([replay.read_fd], [], [], .02); continue
            if not chunk: break
            digest.update(chunk); size += len(chunk)
        else: self.fail('replay did not close its writer within the fixed test deadline')
        report = replay.finish(group_safe=True)
        return size, digest.hexdigest(), report

    def replace_encoded(self, capture, encoded):
        os.close(capture.fd)
        capture.fd = os.memfd_create('public-malformed-gzip', os.MFD_CLOEXEC|os.MFD_ALLOW_SEALING)
        self.assertEqual(os.write(capture.fd, encoded), len(encoded))
        fcntl.fcntl(capture.fd, profile.ADD_SEALS, profile.SEALS)
        capture.retained = len(encoded); capture.retained_hash = hashlib.sha256(encoded)

    def test_pipe_is_nonseekable_and_starts_once_with_exact_identities_and_cleanup(self):
        capture = self.capture(); replay = self.replay(capture)
        fds = [replay.input_fd, replay.read_fd, replay.write_fd]
        self.assertIsNone(replay.thread)
        self.assertFalse(replay.start_attempted)
        self.assertEqual(replay.path, f'/proc/{os.getpid()}/fd/{replay.read_fd}')
        with self.assertRaises(OSError) as caught: os.lseek(replay.read_fd, 0, os.SEEK_CUR)
        self.assertEqual(caught.exception.errno, errno.ESPIPE)
        replay.start(time.monotonic()+2); thread = replay.thread
        size, digest, report = self.consume(replay)
        self.assertIs(replay.thread, thread)
        self.assertEqual((size,digest), (len(RAW),hashlib.sha256(RAW).hexdigest()))
        self.assertEqual(report['status'], 'COMPLETE')
        self.assertTrue(report['gzip_eof'] and report['consumer_eof'] and report['identity_verified'])
        self.assertEqual(report['encoded_sha256'], capture.summary()['encoded_sha256'])
        replay.close(); replay.close()
        self.assertFalse(thread.is_alive())
        for fd in fds:
            with self.assertRaises(OSError): os.fstat(fd)

    def test_crc_truncation_trailing_members_and_identity_mismatch_are_incomplete(self):
        for mode in ('crc','truncated','trailing','second-member','encoded-identity','raw-identity'):
            with self.subTest(mode=mode):
                capture = self.capture(); encoded = os.pread(capture.fd, capture.retained, 0)
                if mode == 'crc':
                    encoded = encoded[:-8]+bytes([encoded[-8]^1])+encoded[-7:]
                elif mode == 'truncated': encoded = encoded[:-4]
                elif mode == 'trailing': encoded += b'PUBLIC_TRAILING_DATA'
                elif mode == 'second-member': encoded += gzip.compress(b'PUBLIC_SECOND_MEMBER', mtime=0)
                self.replace_encoded(capture, encoded)
                if mode == 'encoded-identity': capture.retained_hash = hashlib.sha256(b'wrong')
                if mode == 'raw-identity': capture.observed_hash = hashlib.sha256(b'wrong')
                replay = self.replay(capture)
                _, _, report = self.consume(replay)
                self.assertEqual(report['status'], 'INCOMPLETE')
                self.assertIsNotNone(report['error'])
                self.assertFalse(report['identity_verified'])
                self.assertTrue(report['cleanup_ok'])
                self.assertFalse(replay.alive())
                self.assertNotIn(SYNTHETIC_CANARY, json.dumps(report))
                replay.close(); capture.close()

    def test_short_reads_short_writes_and_eagain_keep_bounded_exact_replay(self):
        line = json.dumps(dict(type='PUBLIC_FIXTURE', payload=SYNTHETIC_CANARY+'x'*60000)).encode()+b'\n'
        raw = json.dumps(HEADER).encode()+b'\n'+line*3+json.dumps(FOOTER).encode()
        capture = self.capture(raw); replay = self.replay(capture)
        real_write, real_pread, real_decoder = os.write, os.pread, zlib.decompressobj
        write_fd, input_fd = replay.write_fd, replay.input_fd
        writes = []; reads = []; decoded_limits = []
        def write(fd, data):
            if fd != write_fd: return real_write(fd, data)
            writes.append(len(data))
            if len(writes) == 1: raise BlockingIOError(errno.EAGAIN, 'public backpressure fixture')
            if len(writes) == 2: raise InterruptedError()
            return real_write(fd, data[:4093])
        def pread(fd, size, offset):
            if fd == input_fd:
                reads.append(size); self.assertLessEqual(size, 65536)
                return real_pread(fd, min(size,17), offset)
            return real_pread(fd, size, offset)
        class Decoder:
            def __init__(self, *args, **kwargs): self.value = real_decoder(*args, **kwargs)
            def __getattr__(self, key): return getattr(self.value, key)
            def decompress(self, data, limit):
                decoded_limits.append(limit)
                if not 0 < limit <= 65536: raise AssertionError('unbounded decoder request')
                return self.value.decompress(data, limit)
        with patch.object(os,'write',side_effect=write), patch.object(os,'pread',side_effect=pread), \
             patch.object(zlib,'decompressobj',Decoder):
            size, digest, report = self.consume(replay)
        self.assertGreater(len(writes), 3); self.assertGreater(len(reads), 1)
        self.assertTrue(decoded_limits)
        self.assertEqual((size,digest), (len(raw),hashlib.sha256(raw).hexdigest()))
        self.assertEqual(report['status'], 'COMPLETE')


class CompressedProcessTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(); self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.caps = runtime.CAP, runtime.WRITE_CAP
        runtime.CAP, runtime.WRITE_CAP = 64*1024**2, 63*1024**2
        self.addCleanup(self.restore)

    def restore(self): runtime.CAP, runtime.WRITE_CAP = self.caps

    def observation(self, name='case', raw=RAW):
        out, work = self.root/name, self.root/(name+'-work'); out.mkdir(); work.mkdir()
        value = profile.Observation(out, work, [QUERY]); self.addCleanup(value.close)
        value.start(); value.capture.feed(raw); value.capture.finish(True, True); value.batch_safe = True
        return value

    def cli(self, name='reader', mode='all', ready=None):
        path = self.root/(name+'.py')
        header = dict(summaryLogVersion='0.4.0',codeqlVersion='2.27.4',startTime='2026-01-01T00:00:00Z')
        summary = b'\n'.join(json.dumps(row).encode() for row in (header,SummaryTests().row()))
        body = f'#!{sys.executable}\nimport os,sys,time\nfrom pathlib import Path\n'
        if mode == 'all':
            body += 'fd=os.open(sys.argv[-1],os.O_RDONLY)\nwhile os.read(fd,65536):pass\nos.close(fd)\n'
        elif mode == 'prefix':
            body += 'fd=os.open(sys.argv[-1],os.O_RDONLY)\nos.read(fd,3)\nos.close(fd)\n'
        if mode == 'sleep': body += 'time.sleep(60)\n'
        if ready:
            body += f'ready=Path({str(ready)!r});end=time.monotonic()+3\n'
            body += 'while not ready.exists() and time.monotonic()<end:time.sleep(.01)\n'
        body += f'os.write(1,{summary!r})\n'
        path.write_text(body); path.chmod(0o700)
        return path

    def assert_closed(self, observation):
        replay = observation.replay
        self.assertFalse(replay.alive())
        self.assertIsNone(replay.input_fd); self.assertIsNone(replay.write_fd); self.assertIsNone(replay.read_fd)
        result = json.loads((observation.out/'official-summary.log.result.json').read_text())
        if result['pid']:
            with self.assertRaises(ProcessLookupError): os.killpg(result['pid'],0)
        for path in observation.out.rglob('*'):
            if path.is_file(): self.assertNotIn(SYNTHETIC_CANARY.encode(), path.read_bytes(), str(path))
        return result

    def test_writer_starts_after_real_popen_and_complete_consumer_is_projected(self):
        observation = self.observation(); cli = self.cli()
        real_popen = subprocess.Popen; seen = []
        def popen(*args, **kwargs):
            self.assertIsNone(observation.replay.thread)
            self.assertFalse(observation.replay.start_attempted)
            self.assertIsNone(observation.capture.projection)
            numeric = observation.out/'numeric-projection.json'
            self.assertEqual(observation.numeric_result['receipt_sha256'], runtime.digest(numeric))
            seen.append(True); return real_popen(*args, **kwargs)
        with patch.object(runtime.subprocess,'Popen',side_effect=popen):
            observation.summarize(cli,self.root,os.environ.copy(),time.monotonic()+30,admitted=True)
        self.assertEqual(len(seen),1)
        self.assertEqual(observation.summary_result['status'],'PROJECTED')
        self.assertEqual(observation.summary_result['replay']['status'],'COMPLETE')
        self.assertEqual(observation.finish()['status'],'COMPLETE')
        self.assert_closed(observation)
        with self.assertRaises(profile.ObservationError):
            observation.summarize(cli,self.root,{},time.monotonic()+30,admitted=True)

    def test_numeric_is_frozen_before_summary_and_finish_reuses_the_same_receipt(self):
        observation = self.observation(raw=numeric_bytes(numeric_events()))
        cli = self.cli(); real_summary = profile.Summary; real_save = runtime.save
        saves = []; identity = {}
        def save(path, value, *args, **kwargs):
            if path.name == 'numeric-projection.json': saves.append(path)
            return real_save(path, value, *args, **kwargs)
        def summary(*args, **kwargs):
            self.assertIsNone(observation.capture.projection)
            path = observation.out/'numeric-projection.json'
            numeric = json.loads(path.read_bytes())
            self.assertEqual(numeric['status'], 'OBSERVED')
            self.assertEqual(numeric['pipeline_completed'], 3)
            self.assertEqual(numeric['observed_stream_sha256'], observation.capture.observed_hash.hexdigest())
            self.assertEqual(numeric['observed_stream_bytes'], observation.capture.observed)
            identity['sha'] = runtime.digest(path)
            self.assertEqual(observation.numeric_result['receipt_sha256'], identity['sha'])
            return real_summary(*args, **kwargs)
        with patch.object(runtime,'save',side_effect=save), patch.object(profile,'Summary',side_effect=summary):
            observation.summarize(cli,self.root,os.environ.copy(),time.monotonic()+30,admitted=True)
            self.assertEqual(observation.finish()['numeric_status'], 'OBSERVED')
        self.assertEqual(len(saves), 1)
        self.assertEqual(runtime.digest(saves[0]), identity['sha'])
        self.assert_closed(observation)

    def test_numeric_freeze_failure_keeps_state_and_cannot_construct_summary_or_spawn(self):
        observation = self.observation(raw=numeric_bytes(numeric_events()))
        projection = observation.capture.projection
        with patch.object(runtime,'save',side_effect=OSError(SYNTHETIC_CANARY)), \
             patch.object(profile,'Summary') as summary, patch.object(runtime,'run') as run:
            observation.summarize('/cli',self.root,{},time.monotonic()+30,admitted=True)
            summary.assert_not_called(); run.assert_not_called()
        self.assertIs(observation.capture.projection, projection)
        self.assertEqual(observation.summary_result['status'], 'PARTIAL')
        self.assertEqual(observation.finish()['numeric_status'], 'OBSERVED')
        self.assertNotIn(SYNTHETIC_CANARY, json.dumps(observation.receipt()))

    def test_numeric_hash_failure_reuses_existing_bytes_and_corruption_cannot_be_overwritten(self):
        observation = self.observation(raw=numeric_bytes(numeric_events()))
        with patch.object(os,'read',side_effect=OSError(SYNTHETIC_CANARY)), patch.object(profile,'Summary') as summary:
            observation.summarize('/cli',self.root,{},time.monotonic()+30,admitted=True)
            summary.assert_not_called()
        path = observation.out/'numeric-projection.json'; saved = path.read_bytes()
        self.assertIsNotNone(observation.capture.projection)
        with patch.object(runtime,'save',wraps=runtime.save) as save:
            self.assertEqual(observation.finish()['numeric_status'],'OBSERVED')
        self.assertEqual(path.read_bytes(),saved)
        self.assertTrue(all(call.args[0].name!='numeric-projection.json' for call in save.call_args_list))
        observation.close()
        observation = self.observation('corrupt',raw=numeric_bytes(numeric_events()))
        path = observation.out/'numeric-projection.json'; path.write_bytes(b'{}\n')
        with patch.object(profile,'Summary') as summary, patch.object(runtime,'run') as run:
            observation.summarize('/cli',self.root,{},time.monotonic()+30,admitted=True)
            summary.assert_not_called(); run.assert_not_called()
        with self.assertRaisesRegex(profile.ObservationError,'^NUMERIC_RECEIPT_IDENTITY$'):
            observation.finish()
        self.assertEqual(path.read_bytes(),b'{}\n')
        self.assertIsNotNone(observation.capture.projection)

    def test_numeric_existing_special_oversize_or_changed_files_never_block_or_get_overwritten(self):
        for mode in ('symlink','dangling','fifo','oversize','same-length-wrong','changed'):
            with self.subTest(mode=mode):
                observation = self.observation(mode,raw=numeric_bytes(numeric_events()))
                path = observation.out/'numeric-projection.json'; target = self.root/(mode+'-target')
                if mode == 'symlink': target.write_bytes(b'untouched')
                if mode in ('symlink','dangling'): path.symlink_to(target)
                elif mode == 'fifo': os.mkfifo(path)
                elif mode == 'oversize': path.write_bytes(b'x'*(runtime.MAX_CONTROL_RECEIPT+1))
                else:
                    with patch.object(os,'read',side_effect=OSError('first verification unavailable')):
                        with self.assertRaises(profile.ObservationError): observation._freeze_numeric()
                    if mode == 'same-length-wrong':
                        data = path.read_bytes(); path.write_bytes(b'x'+data[1:])
                real_read = os.read
                def read(fd, size):
                    self.assertLessEqual(size,runtime.MAX_CONTROL_RECEIPT+1)
                    data = real_read(fd,size)
                    if mode == 'changed':
                        data_path = path.read_bytes(); path.unlink(); path.write_bytes(data_path)
                    return data
                before = path.lstat()
                with patch.object(os,'read',side_effect=read), patch.object(profile,'Summary') as summary, \
                     patch.object(runtime,'run') as run, patch.object(runtime,'save') as save:
                    observation.summarize('/cli',self.root,{},time.monotonic()+30,admitted=True)
                    summary.assert_not_called(); run.assert_not_called(); save.assert_not_called()
                self.assertEqual(observation.summary_result['status'],'PARTIAL')
                self.assertIsNotNone(observation.capture.projection)
                if mode != 'changed': self.assertEqual(path.lstat().st_ino,before.st_ino)
                if mode == 'symlink': self.assertEqual(target.read_bytes(),b'untouched')
                if mode == 'dangling': self.assertFalse(target.exists())
                observation.close()

    def test_exit_zero_without_consuming_all_input_rejects_even_fully_written_stream(self):
        for mode in ('none','prefix'):
            with self.subTest(mode=mode):
                observation = self.observation(mode); ready = self.root/(mode+'-writer-closed')
                cli = self.cli(mode,mode,ready); real_poll = profile.Summary.poll
                def poll(observer, deadline):
                    real_poll(observer,deadline)
                    if observer.replay.write_closed: ready.touch()
                with patch.object(profile.Summary,'poll',poll):
                    observation.summarize(cli,self.root,os.environ.copy(),time.monotonic()+30,admitted=True)
                report = observation.summary_result
                self.assertEqual(report['exit'],0)
                self.assertEqual(report['projection']['status'],'PROJECTED')
                self.assertEqual(report['status'],'PARTIAL')
                self.assertEqual(report['replay']['error'],'REPLAY_UNCONSUMED_DATA')
                self.assertTrue(report['replay']['identity_verified'])
                self.assertEqual(report['replay']['written_bytes'],len(RAW))
                self.assertEqual(observation.finish()['status'],'INCOMPLETE')
                self.assert_closed(observation); observation.close()

    def test_spawn_or_registration_failure_never_starts_the_writer(self):
        real_open = Path.open
        for mode in ('spawn','registration'):
            with self.subTest(mode=mode):
                observation = self.observation(mode); cli = self.cli(mode)
                def failed_open(path,*args,**kwargs):
                    if path.name == 'official-summary.log': raise OSError(SYNTHETIC_CANARY)
                    return real_open(path,*args,**kwargs)
                target = patch.object(runtime.subprocess,'Popen',side_effect=OSError(SYNTHETIC_CANARY)) if mode == 'spawn' else patch.object(Path,'open',failed_open)
                with target:
                    observation.summarize(cli,self.root,os.environ.copy(),time.monotonic()+30,admitted=True)
                self.assertEqual(observation.summary_result['status'],'PARTIAL')
                self.assertFalse(observation.replay.start_attempted)
                self.assertIsNone(observation.replay.thread)
                self.assert_closed(observation); observation.close()

    def test_backpressure_timeout_stops_writer_and_group_within_the_shared_deadline(self):
        line = json.dumps(dict(type='PUBLIC_FIXTURE',payload='x'*60000)).encode()+b'\n'
        raw = json.dumps(HEADER).encode()+b'\n'+line*3+json.dumps(FOOTER).encode()
        observation = self.observation(raw=raw); cli = self.cli(mode='sleep'); started = time.monotonic()
        with patch.object(profile,'SUMMARY_SECONDS',5):
            observation.summarize(cli,self.root,os.environ.copy(),started+30,admitted=True)
        self.assertLess(time.monotonic()-started,5.5)
        self.assertEqual(observation.summary_result['status'],'PARTIAL')
        self.assertEqual(observation.summary_result['failure_kind'],'timeout')
        self.assertEqual(observation.summary_result['replay']['status'],'INCOMPLETE')
        self.assert_closed(observation)

    def test_cancel_during_backpressure_keeps_cancelled_and_closes_all_owners(self):
        line = json.dumps(dict(type='PUBLIC_FIXTURE',payload='x'*60000)).encode()+b'\n'
        raw = json.dumps(HEADER).encode()+b'\n'+line*3+json.dumps(FOOTER).encode()
        observation = self.observation(raw=raw); cli = self.cli(mode='sleep')
        real_poll = profile.Summary.poll; sent = []
        def cancel(observer,deadline):
            real_poll(observer,deadline)
            if observer.replay.written_bytes >= observer.replay.pipe_capacity and not sent:
                sent.append(True); os.kill(os.getpid(),signal.SIGTERM)
        with patch.object(profile.Summary,'poll',cancel):
            observation.summarize(cli,self.root,os.environ.copy(),time.monotonic()+30,admitted=True)
        self.assertEqual(sent,[True])
        self.assertEqual(observation.summary_result['failure_kind'],'cancelled')
        self.assertEqual(observation.summary_result['status'],'PARTIAL')
        self.assertTrue(self.assert_closed(observation)['cancelled'])

    def test_unjoined_writer_retains_descriptors_and_shared_reservation(self):
        observation = self.observation()
        replay = profile.GzipReplay(observation.capture,time.monotonic()+2)
        observation.replay = replay
        class PendingWriter:
            running = True
            def join(self, timeout): pass
            def is_alive(self): return self.running
        replay.thread = PendingWriter(); replay.worker_started = replay.start_attempted = True
        fds = [replay.input_fd,replay.write_fd,replay.read_fd]
        try:
            report = replay.finish(group_safe=True)
            self.assertEqual(report['error'],'REPLAY_WORKER_NOT_CLOSED')
            self.assertFalse(report['cleanup_ok'])
            with self.assertRaises(profile.ObservationError): observation.close()
            self.assertEqual(runtime.CAP,12*1024**2)
            self.assertIsNotNone(observation.capture.fd)
            for fd in fds: os.fstat(fd)
        finally:
            # Simulate the worker's eventual exit without closing live-owned fds
            # from another thread. No real worker is created by this test.
            replay.thread.running = False
            replay._close_fd('input'); replay._close_fd('write')
            observation.close()
        self.assertEqual(runtime.CAP,64*1024**2)
        for fd in fds:
            with self.assertRaises(OSError): os.fstat(fd)


@unittest.skipUnless(os.environ.get('OASIS7_CODEQL_PROFILE_CLI'), 'explicit pinned CodeQL profile input required')
class RealCodeQLProfile(unittest.TestCase):
    def test_fixed_cli_runs_actual_compressed_capture_replay_and_projection(self):
        """One fresh 600-second window; a public pure-QL fixture, never oasis7 extraction."""
        cli = Path(os.environ['OASIS7_CODEQL_PROFILE_CLI']).resolve()
        started = time.monotonic(); end = started+600
        started_utc = datetime.now(timezone.utc).isoformat()
        caps = runtime.CAP,runtime.WRITE_CAP
        runtime.CAP,runtime.WRITE_CAP = 64*1024**2,63*1024**2
        observation = None
        try:
            with tempfile.TemporaryDirectory(prefix='oasis7-profile-cli-') as folder:
                root = Path(folder).resolve(); work = root/'work'; out = root/'evidence'
                work.mkdir(); out.mkdir()
                runtime.save(out/'d0.json',dict(started_utc=started_utc,started_monotonic=started,
                    deadline_monotonic=end,budget_seconds=600,threads=2,ram_mib=3072),out)
                cache, traps, pack = work/'cache',work/'empty-traps',work/'pack'
                traps.mkdir(); pack.mkdir()
                env = {key:value for key,value in os.environ.items() if key not in ('GH_TOKEN','GITHUB_TOKEN')}
                def run(args,label,limit=20):
                    return runtime.run([str(cli),*map(str,args),'--common-caches='+str(cache)],work,
                                       out/(label+'.log'),end,limit,out,env,file_limit=None)
                cli_hash = runtime.digest(cli)
                run(['version','--format=json'],'version')
                self.assertEqual(json.loads((out/'version.log').read_text())['version'],'2.27.4')
                scheme = cli.parent/'rust/rust.dbscheme'; scheme_hash = runtime.digest(scheme)
                run(['dataset','import','--dbscheme='+str(scheme),'--threads=2','-J=-Xmx3072m',
                     work/'dataset',traps],'import',30)
                (pack/'qlpack.yml').write_text('name: synthetic/profile\nversion: 0.0.0\ndependencies:\n  codeql/rust-all: 0.2.25\n')
                query = pack/'ProfileProbe.ql'
                predicate = 'fixture_'+SYNTHETIC_CANARY
                query.write_text(f'cached predicate {predicate}(int x) {{ x in [1 .. 3] }}\n'
                                 f'from int x where {predicate}(x) select x, "fixture"\n')
                query_hash = runtime.digest(query)
                record = dict(path=str(query),relative=query.name,pack='synthetic/profile',sha256=query_hash)
                observation = profile.Observation(out,work,[record]); observation.start()
                batch = observation.run_batch([str(cli),'query','run','--dataset='+str(work/'dataset'),
                    '--threads=2','--ram=3072','--output='+str(work/'result.bqrs'),
                    '--no-default-compilation-cache','--compilation-cache='+str(work/'compile-cache'),
                    '--common-caches='+str(cache),'--',str(query)],work,end,60,env)
                self.assertEqual(batch['exit'],0)
                self.assertFalse(batch['cleanup_errors'] or batch['evidence_errors'])
                observation.summarize(cli,work,env,end,admitted=True)
                terminal = observation.finish(); receipt = observation.receipt()
                self.assertEqual(terminal['status'],'COMPLETE')
                capture = receipt['capture']; summary = receipt['summary']; replay = summary['replay']
                projection = summary['projection']
                self.assertEqual(capture['version'],'gzip-capture-v1')
                self.assertTrue(capture['encoding_finished'] and capture['sealed'])
                self.assertEqual(capture['observed_bytes'],replay['decoded_bytes'])
                self.assertEqual(capture['observed_sha256'],replay['decoded_sha256'])
                self.assertEqual(capture['encoded_bytes'],replay['encoded_bytes'])
                self.assertEqual(capture['encoded_sha256'],replay['encoded_sha256'])
                self.assertEqual(replay['written_bytes'],replay['decoded_bytes'])
                self.assertEqual(replay['written_sha256'],replay['decoded_sha256'])
                self.assertEqual(replay['status'],'COMPLETE')
                self.assertTrue(replay['cleanup_ok'] and replay['consumer_eof'] and replay['gzip_eof'])
                self.assertGreaterEqual(projection['accepted_records'],3)
                self.assertEqual((projection['unknown_shapes'],projection['invalid_records']),(0,0))
                rows = projection['rankings']['official_millis']
                self.assertTrue(rows)
                self.assertTrue(all(row['appears_in_query_indexes']==[0] and not row['unmapped_query_count'] for row in rows))
                self.assertTrue(all(row['first_requiring_query']['query_sha256']==query_hash for row in rows))
                self.assertTrue(any(row['evaluation_strategy']=='COMPUTED_EXTENSIONAL' for row in rows))
                numeric_path = observation.out/'numeric-projection.json'
                numeric = json.loads(numeric_path.read_bytes())
                self.assertEqual(numeric['status'],'NOT_OBSERVED')
                self.assertEqual(numeric['observed_stream_sha256'],capture['observed_sha256'])
                self.assertEqual(numeric['observed_stream_bytes'],capture['observed_bytes'])
                self.assertEqual(receipt['numeric_projection']['receipt_sha256'],runtime.digest(numeric_path))
                self.assertIsNone(observation.capture.projection)
                observation.close()
                self.assertIsNone(observation.capture.fd)
                self.assertFalse(observation.replay.alive())
                self.assertEqual((runtime.CAP,runtime.WRITE_CAP),(64*1024**2,63*1024**2))
                run(['bqrs','decode','--format=json',work/'result.bqrs'],'bqrs')
                tuples = json.loads((out/'bqrs.log').read_text())['#select']['tuples']
                self.assertEqual(sorted(tuples),[[1,'fixture'],[2,'fixture'],[3,'fixture']])
                self.assertEqual(runtime.digest(cli),cli_hash)
                self.assertEqual(runtime.digest(scheme),scheme_hash)
                self.assertEqual(runtime.digest(query),query_hash)
                self.assertFalse((work/'candidate-events.jsonl').exists())
                self.assertFalse((work/'candidate-events.fifo').exists())
                for result_path in out.rglob('*.result.json'):
                    result = json.loads(result_path.read_text())
                    self.assertEqual(result['exit'],0)
                    self.assertFalse(result['cleanup_errors'] or result['evidence_errors'])
                    with self.assertRaises(ProcessLookupError): os.killpg(result['pid'],0)
                for path in out.rglob('*'):
                    if path.is_file(): self.assertNotIn(SYNTHETIC_CANARY.encode(),path.read_bytes(),str(path))
                report = dict(status='INTEGRATION_PASSED',codeql_version='2.27.4',started_utc=started_utc,
                    finished_utc=datetime.now(timezone.utc).isoformat(),elapsed_seconds=time.monotonic()-started,
                    budget_seconds=600,threads=2,ram_mib=3072,query_sha256=query_hash,bqrs_rows=len(tuples),
                    capture={key:capture[key] for key in ('status','observed_bytes','observed_sha256','encoded_bytes','encoded_sha256','encoding_finished')},
                    replay={key:replay[key] for key in ('status','decoded_bytes','decoded_sha256','written_bytes','written_sha256','gzip_eof','consumer_eof','cleanup_ok','worker_alive')},
                    official_summary=dict(accepted=projection['accepted_records'],unknown=projection['unknown_shapes'],invalid=projection['invalid_records'],
                                          sha256=projection['streams']['stdout']['sha256']),owned_groups_closed=True)
                self.assertLess(time.monotonic(),end)
                runtime.save(out/'integration.json',report,out,terminal=True)
                self.assertLess((out/'integration.json').stat().st_size,runtime.MAX_CONTROL_RECEIPT)
                print(json.dumps(report,sort_keys=True),flush=True)
        finally:
            if observation is not None: observation.close()
            runtime.CAP,runtime.WRITE_CAP = caps


if __name__ == '__main__':
    unittest.main()
