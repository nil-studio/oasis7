"""Private candidate-only CodeQL 2.27.4 observation; raw events stay in memory."""
from contextlib import contextmanager
import fcntl
import hashlib
import heapq
import json
import os
from pathlib import Path
import re
import select
import threading
import time
import zlib

import codeql_rust_diagnostic_runtime as runtime

VERSION = 'candidate-evaluator-v1'
RAW_CAP = 48 * 1024**2
CODEC_RESERVE = 1024**2  # inside RAW_CAP, never a second memory allowance
ENCODED_CAP = RAW_CAP - CODEC_RESERVE
CODEC_CHUNK = 64 * 1024
REPLAY_CLOSE_SECONDS = .5  # inside the one SUMMARY_SECONDS deadline
OUTPUT_CAP = 8 * 1024**2
SUMMARY_SECONDS = 20
LINE_CAP = 2 * 1024**2
MAX_SUMMARY_RECORDS = 100000
NUMERIC_RESERVE = 4 * 1024**2  # inside OUTPUT_CAP, throughout the capture lifetime
NUMERIC_LINE_CAP = 64 * 1024
NUMERIC_ALIAS_CAP = 64
NUMERIC_PREDICATE_CAP = 128
NUMERIC_PIPELINE_CAP = 256
NUMERIC_EVENT_CAP = 16384
NUMERIC_TAIL = 16
NUMERIC_RECEIPT_CAP = 31 * 1024  # leave room for the observed-stream identity
# Linux UAPI values; some Python builds omit these names despite memfd support.
ADD_SEALS = getattr(fcntl, 'F_ADD_SEALS', 1033)
GET_SEALS = getattr(fcntl, 'F_GET_SEALS', 1034)
SEALS = 0x0008 | 0x0004 | 0x0002 | 0x0001


class ObservationError(ValueError):
    """All messages are fixed codes, never text taken from an observed stream."""


def _unique_object(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ObservationError('DUPLICATE_JSON_KEY')
        result[key] = value
    return result


def _nonfinite(_):
    raise ObservationError('NONFINITE_JSON_NUMBER')


def parse_json(data):
    try:
        return json.loads(data.decode('utf-8'), object_pairs_hook=_unique_object,
                          parse_constant=_nonfinite)
    except (ValueError, UnicodeError, RecursionError):
        raise ObservationError('INVALID_JSON_RECORD') from None


def _hash(data):
    return hashlib.sha256(data).hexdigest()


def _clock_ns(name):
    try:
        value = getattr(time, name)()
    except Exception:
        return None  # unavailable clocks must not replace the original failure
    return value if type(value) is int and 0 <= value <= 2**63-1 else None


class ClockTotal:
    """Fixed-size clock accounting; unknown intervals never become measured zero."""
    def __init__(self):
        self.intervals = self.unmeasured = self.nanoseconds = 0

    def record(self, start, end):
        self.intervals += 1
        if (start is None or end is None or end < start
                or self.nanoseconds + end - start > 2**63-1):
            self.unmeasured += 1
        else:
            self.nanoseconds += end - start

    def report(self):
        measured = self.intervals - self.unmeasured
        return dict(status='NOT_OBSERVED' if not self.intervals else 'PARTIAL' if self.unmeasured else 'MEASURED',
                    intervals=self.intervals, unmeasured_intervals=self.unmeasured,
                    measured_ns=self.nanoseconds if measured else None,
                    total_ns=self.nanoseconds if self.intervals and not self.unmeasured else None)


class NumericProjection:
    """One fixed source family; event instances and literal sizes, never CPU/deltas."""
    VERSION = 'candidate-numeric-v1'
    BASE = {'time': str, 'type': str, 'eventId': int, 'nanoTime': int}
    FIELDS = dict(BASE, codeqlVersion=str, logVersion=str, queryName=str, stages=list,
                  startEvent=int, terminationType=str, raHash=str, predicateName=str,
                  appearsAs=dict, layerId=int, isCached=bool, predicateType=str,
                  position=str, ra=dict, dependencies=dict, queryCausingWork=int,
                  resultSize=int, completionType=str, exceptionName=str,
                  predicateStartEvent=int, raReference=str, generationNumber=int,
                  counts=list, duplicationPercentages=list, cacheLookupResult=str,
                  relationSize=int, sentinelRaHash=str)
    PREDICATE = 'raHash predicateName appearsAs layerId isCached'
    KINDS = {
        'LOG_HEADER': 'codeqlVersion logVersion', 'LOG_FOOTER': '',
        'QUERY_STARTED': 'queryName stages', 'QUERY_COMPLETED': 'startEvent terminationType',
        'PREDICATE_STARTED': PREDICATE+' predicateType position ra dependencies queryCausingWork',
        'PREDICATE_COMPLETED': 'startEvent resultSize completionType exceptionName',
        'PIPELINE_STARTED': 'predicateStartEvent raReference generationNumber',
        'PIPELINE_COMPLETED': 'startEvent counts duplicationPercentages resultSize completionType exceptionName',
        'CACHE_LOOKUP': PREDICATE+' cacheLookupResult relationSize',
        'SENTINEL_EMPTY': PREDICATE+' sentinelRaHash',
    }
    PREDICATE_TYPES = {'SIMPLE_INTENSIONAL', 'RECURSIVE_INTENSIONAL', 'EXTENSIONAL',
                       'EXTERNAL_PREDICATE', 'STAGE_RESULT', 'EXTENSIBLE_PREDICATE', 'NAMED_LOCAL'}
    ERRORS = ('INVALID_RECORD', 'FIELD_OR_TYPE', 'HEADER', 'RECORD_LIMIT', 'TABLE_LIMIT',
              'DUPLICATE_START', 'LINK_ORDER', 'INTERVAL', 'NON_SUCCESS', 'PROCESSING_FAILED', 'RECEIPT_LIMIT', 'INPUT_GAP')

    def __init__(self):
        self.records = self.post_limit_records = self.headers = self.footers = 0
        self.input_framed_bytes = 0
        self.input_observed_bytes = None
        self.header_valid = self.finished = self.eof = self.cleanup_ok = self.after_limit = False
        self.first = self.last = None
        self.errors = {key: 0 for key in self.ERRORS}
        self.events = {key: 0 for key in self.KINDS}
        self.aliases, self.predicates, self.pipelines = {}, {}, {}
        self.target_events = {}
        self.predicate_starts = self.pipeline_starts = self.predicate_completed = self.pipeline_completed = 0
        self.maximum_generation = None
        self.predicate_tail, self.pipeline_tail = [], []

    def fail(self, code):
        code = code if code in self.errors else 'PROCESSING_FAILED'
        self.errors[code] = min(2**63-1, self.errors[code]+1)

    @staticmethod
    def _family(alias):
        # Components inside generic arguments are not owning namespaces. The
        # compiler suffix is opaque and is included in the full alias digest.
        core = alias.partition('#')[0]
        parts, depth, start, offset = [], 0, 0, 0
        while offset < len(core):
            char = core[offset]
            if char == '<':
                depth += 1
                if depth > 32:
                    return False
            elif char == '>':
                depth -= 1
                if depth < 0:
                    return False
            elif not depth and core[offset:offset+2] == '::':
                parts.append(core[start:offset]); offset += 1; start = offset+1
            offset += 1
        parts.append(core[start:])
        return (not depth and all(parts) and parts[-1] == 'inferType/2'
                and 'TypeInference' in parts[:-1])

    def _aliases(self, value):
        if type(value) is not dict or len(value) > NUMERIC_ALIAS_CAP:
            raise ObservationError('APPEARANCE_LIMIT')
        matched = set()
        for alias, stages in value.items():
            raw = text_value(alias, 2048)
            if any(ord(char) < 32 for char in alias) or type(stages) is not list or len(stages) > 1024:
                raise ObservationError('APPEARANCE_SHAPE')
            for stage in stages:
                integer(stage, -(2**31), 2**31-1)  # opaque raw stage markers, not query attribution
            identity = _hash(raw)
            if identity not in self.aliases and self._family(alias):
                if len(self.aliases) >= NUMERIC_ALIAS_CAP:
                    self.fail('TABLE_LIMIT')
                    continue
                self.aliases[identity] = len(self.aliases)
            if identity in self.aliases:
                matched.add(self.aliases[identity])
        return sorted(matched)

    def _elapsed(self, end, start):
        elapsed = end-start
        if not 0 <= elapsed <= 2**63-1:
            self.fail('INTERVAL')
            return None
        return elapsed

    def _remember(self, event, kind):
        if len(self.target_events) >= NUMERIC_EVENT_CAP:
            self.fail('TABLE_LIMIT')
            return
        self.target_events[event] = kind

    def accept(self, value):
        self.records += 1
        self.post_limit_records += self.after_limit
        try:
            self._event(value)
        except Exception:
            self.fail('FIELD_OR_TYPE')  # no exception string or observed text

    def _event(self, value):
        if type(value) is not dict or value.get('type') not in self.KINDS:
            raise ObservationError('EVENT_KIND')
        kind = value['type']
        allowed = set(self.BASE) | set(self.KINDS[kind].split())
        if not self.BASE.keys() <= value.keys() or any(
                key not in allowed or type(item) is not self.FIELDS[key] for key, item in value.items()):
            raise ObservationError('EVENT_FIELDS')
        for key, item in value.items():
            if self.FIELDS[key] is int:
                # Unprojected nullable Java counters may carry signed markers.
                # Only event links and concrete layer identities require >= 0.
                minimum = 0 if key in ('eventId', 'startEvent', 'predicateStartEvent', 'layerId') else -(2**63)
                integer(item, -(2**31) if key == 'generationNumber' else minimum,
                        2**31-1 if key == 'generationNumber' else 2**63-1)
            elif self.FIELDS[key] is str:
                text_value(item, NUMERIC_LINE_CAP)
        self.events[kind] += 1
        self.first = kind if self.first is None else self.first
        self.last = kind
        event, nano = value['eventId'], value['nanoTime']
        if event in self.target_events:
            self.fail('DUPLICATE_START')
            return
        if kind == 'LOG_HEADER':
            self.headers += 1
            self.header_valid = value.get('codeqlVersion') == '2.27.4' and value.get('logVersion') == '0.5.0'
            if not self.header_valid or self.headers != 1 or self.records != 1:
                self.fail('HEADER')
            return
        if not self.header_valid or self.footers:
            self.fail('HEADER')
        if kind == 'LOG_FOOTER':
            self.footers += 1
            return
        completion = value.get('completionType', 'IMPLICIT_SUCCESS')
        if completion not in ('IMPLICIT_SUCCESS', 'SUCCESS', 'FAILURE', 'INCOMPLETE'):
            raise ObservationError('COMPLETION_ENUM')
        if completion in ('FAILURE', 'INCOMPLETE') or 'exceptionName' in value:
            self.fail('NON_SUCCESS')
        if kind == 'QUERY_COMPLETED' and value.get('terminationType') != 'COMPLETED_SUCCESSFULLY':
            self.fail('NON_SUCCESS')
        if kind == 'PREDICATE_STARTED':
            if value.get('predicateType') not in self.PREDICATE_TYPES:
                raise ObservationError('PREDICATE_TYPE')
            aliases = self._aliases(value.get('appearsAs'))
            if not aliases:
                return
            self._remember(event, 'PREDICATE_STARTED')
            if event in self.predicates or event in self.pipelines:
                self.fail('DUPLICATE_START'); return
            if len(self.predicates) >= NUMERIC_PREDICATE_CAP:
                self.fail('TABLE_LIMIT'); return
            self.predicates[event] = dict(aliases=aliases, nano=nano, layer=value.get('layerId'),
                predicate_type=value['predicateType'], started_before_limit=not self.after_limit)
            self.predicate_starts += 1
        elif kind == 'PIPELINE_STARTED':
            parent = value['predicateStartEvent']
            if parent not in self.predicates:
                if parent in self.target_events:
                    self.fail('LINK_ORDER')
                return
            self._remember(event, 'PIPELINE_STARTED')
            if event in self.pipelines or event in self.predicates:
                self.fail('DUPLICATE_START'); return
            if len(self.pipelines) >= NUMERIC_PIPELINE_CAP:
                self.fail('TABLE_LIMIT'); return
            generation = value.get('generationNumber')
            if generation is not None and generation < 0:
                self.fail('FIELD_OR_TYPE')
            elif generation is not None:
                self.maximum_generation = max(generation, self.maximum_generation or 0)
            self.pipelines[event] = dict(predicate_event=parent, nano=nano, generation=generation,
                layer=self.predicates[parent]['layer'], started_before_limit=not self.after_limit)
            self.pipeline_starts += 1
        elif kind == 'PIPELINE_COMPLETED':
            if value['startEvent'] not in self.pipelines:
                if value['startEvent'] in self.target_events:
                    self.fail('LINK_ORDER')
                return  # ordinary completions of unselected relations are not missing links
            start = value['startEvent']; begin = self.pipelines.pop(start)
            self._remember(event, 'PIPELINE_COMPLETED')
            if value.get('resultSize', 0) < 0:
                self.fail('FIELD_OR_TYPE')
            self.pipeline_completed += 1
            self.pipeline_tail.append(dict(probe_id=0, predicate_event=begin['predicate_event'],
                pipeline_event=start, completion_event=event, layer=begin['layer'], generation=begin['generation'],
                pipeline_result_size=value.get('resultSize'), elapsed_ns=self._elapsed(nano, begin['nano']),
                completion=completion, exception_present='exceptionName' in value,
                started_before_limit=begin['started_before_limit']))
            del self.pipeline_tail[:-NUMERIC_TAIL]
        elif kind == 'PREDICATE_COMPLETED':
            if value['startEvent'] not in self.predicates:
                if value['startEvent'] in self.target_events:
                    self.fail('LINK_ORDER')
                return
            start = value['startEvent']; begin = self.predicates.pop(start)
            self._remember(event, 'PREDICATE_COMPLETED')
            if value.get('resultSize', 0) < 0:
                self.fail('FIELD_OR_TYPE')
            if any(item['predicate_event'] == start for item in self.pipelines.values()):
                self.fail('LINK_ORDER')
            self.predicate_completed += 1
            self.predicate_tail.append(dict(probe_id=0, predicate_event=start, completion_event=event,
                alias_ids=begin['aliases'], layer=begin['layer'], predicate_type=begin['predicate_type'],
                predicate_result_size=value.get('resultSize'), elapsed_ns=self._elapsed(nano, begin['nano']),
                completion=completion, exception_present='exceptionName' in value))
            del self.predicate_tail[:-(NUMERIC_TAIL//2)]

    def finish(self, eof, cleanup_ok):
        self.finished, self.eof, self.cleanup_ok = True, eof, cleanup_ok

    def report(self):
        complete = (self.finished and self.eof and self.cleanup_ok and self.headers == self.footers == 1
                    and self.header_valid and self.first == 'LOG_HEADER' and self.last == 'LOG_FOOTER'
                    and self.input_framed_bytes == self.input_observed_bytes
                    and not any(self.errors.values()) and not self.predicates and not self.pipelines)
        report = dict(version=self.VERSION, probe_id=0, scope='FIXED_SOURCE_FAMILY_ONLY',
            status=('OBSERVED' if self.predicate_starts else 'NOT_OBSERVED') if complete else 'PARTIAL',
            records=self.records, event_counts=self.events, errors=dict(self.errors),
            raw_memory_limit_seen=self.after_limit, post_limit_records=self.post_limit_records,
            input_framed_bytes=self.input_framed_bytes,
            input_unframed_bytes=None if self.input_observed_bytes is None else self.input_observed_bytes-self.input_framed_bytes,
            header_count=self.headers, footer_count=self.footers, header_version_valid=self.header_valid,
            transport_eof=self.eof, cleanup_ok=self.cleanup_ok,
            predicate_starts=self.predicate_starts, pipeline_starts=self.pipeline_starts,
            predicate_completed=self.predicate_completed, pipeline_completed=self.pipeline_completed,
            maximum_generation=self.maximum_generation,
            aliases=[dict(alias_id=index, sha256=identity) for identity, index in self.aliases.items()],
            open_predicates=len(self.predicates), open_pipelines=len(self.pipelines),
            remembered_target_events=len(self.target_events), target_event_limit=NUMERIC_EVENT_CAP,
            unfinished_predicates=[dict(predicate_event=event, alias_ids=value['aliases'], layer=value['layer'],
                                       predicate_type=value['predicate_type'])
                                   for event, value in list(self.predicates.items())[:8]],
            unfinished_pipelines=[dict(pipeline_event=event, predicate_event=value['predicate_event'],
                                       layer=value['layer'], generation=value['generation'])
                                  for event, value in list(self.pipelines.items())[:8]],
            pipeline_tail=list(self.pipeline_tail), predicate_tail=list(self.predicate_tail),
            receipt_omitted_samples=0)
        samples = ('pipeline_tail', 'predicate_tail', 'unfinished_predicates', 'unfinished_pipelines')
        while len((json.dumps(report, indent=2)+'\n').encode()) > NUMERIC_RECEIPT_CAP:
            largest = max(samples, key=lambda key: len(report[key]))
            if not report[largest]:
                raise ObservationError('NUMERIC_RECEIPT_LIMIT')
            report[largest].pop(0)
            report['receipt_omitted_samples'] += 1
            report['status'] = 'PARTIAL'
            report['errors']['RECEIPT_LIMIT'] = 1
        return report


class Capture:
    """Caller owns the sealed memfd until the official summary has exited."""
    def __init__(self, limit=RAW_CAP, *, projection=None):
        if type(limit) is not int or not 0 < limit <= RAW_CAP:
            raise ObservationError('INVALID_MEMORY_LIMIT')
        self.limit = limit
        self.fd = os.memfd_create('codeql-candidate-events', os.MFD_CLOEXEC | os.MFD_ALLOW_SEALING)
        self.observed = self.retained = 0
        self.observed_hash, self.retained_hash = hashlib.sha256(), hashlib.sha256()
        self.error = None
        self.retained_identity_valid = True
        self.sealed = self.eof = self.cleanup_ok = False
        self.pending = b''
        self.records = self.headers = self.footers = self.invalid = 0
        self.first = self.last = None
        self.header_version = False
        self.projection = projection
        self.prefix_finished = self.skip_projection_line = False
        self.feed_cost = ({key: dict(input_bytes=0, wall=ClockTotal(), thread_cpu=ClockTotal())
                           for key in ('BEFORE_LIMIT', 'CAP_CROSSING', 'AFTER_LIMIT')}
                          if projection is not None else None)

    def _record(self, line, *, retained=True, project=True):
        if retained:
            self.records += 1
        projected = project and self.projection is not None
        if projected and len(line) > NUMERIC_LINE_CAP:
            self.projection.fail('RECORD_LIMIT')
            projected = False
        if not retained and not projected:
            return
        try:
            value = parse_json(line)
            if not isinstance(value, dict) or not isinstance(value.get('type'), str):
                raise ObservationError('INVALID_EVENT_ENVELOPE')
            if retained:
                kind = value['type']
                self.first = kind if self.first is None else self.first
                self.last = kind
                self.headers += kind == 'LOG_HEADER'
                self.footers += kind == 'LOG_FOOTER'
                if kind == 'LOG_HEADER':
                    self.header_version = value.get('codeqlVersion') == '2.27.4' and value.get('logVersion') == '0.5.0'
        except ObservationError:
            if retained:
                self.invalid += 1
            if projected:
                self.projection.fail('INVALID_RECORD')
            return
        if projected:
            try:
                self.projection.accept(value)
            except Exception:
                self.projection.fail('PROCESSING_FAILED')

    def _frame(self, chunk, *, retained):
        if self.projection is not None:
            self.projection.input_framed_bytes += len(chunk)
        if self.skip_projection_line:
            offset = chunk.find(b'\n')
            if offset < 0:
                return
            chunk = chunk[offset+1:]
            self.skip_projection_line = False
        self.pending += chunk
        while b'\n' in self.pending:
            line, self.pending = self.pending.split(b'\n', 1)
            if len(line) > LINE_CAP:
                if retained:
                    self.invalid += 1
                if self.projection:
                    self.projection.fail('RECORD_LIMIT')
            elif line:
                self._record(line, retained=retained)
        if len(self.pending) > LINE_CAP:
            if retained:
                self.error = 'EVENT_RECORD_LIMIT'
                raise ObservationError(self.error)
            self.projection.fail('RECORD_LIMIT')
            self.pending = b''
            self.skip_projection_line = True

    def _prefix_tail(self, *, project):
        # Freeze exactly what the legacy retained-prefix finish would observe.
        # The same pending bytes can then continue in the full-stream framer.
        invalid_before = self.invalid
        if len(self.pending) > LINE_CAP:
            self.invalid += 1
        elif self.pending:
            self._record(self.pending, project=project)
        self.pending_bytes = len(self.pending) if self.invalid > invalid_before else 0
        self.prefix_finished = True

    def feed(self, chunk):
        if self.sealed or self.fd is None:
            raise ObservationError('CAPTURE_ALREADY_CLOSED')
        if self.feed_cost is None:
            return self._feed(chunk)
        phase = ('AFTER_LIMIT' if self.error == 'MEMORY_LIMIT' else
                 'CAP_CROSSING' if not self.error and self.retained + len(chunk) > self.limit else
                 'BEFORE_LIMIT')
        cost = self.feed_cost[phase]
        wall, cpu = _clock_ns('perf_counter_ns'), _clock_ns('thread_time_ns')
        try:
            return self._feed(chunk)
        finally:
            cpu_end, wall_end = _clock_ns('thread_time_ns'), _clock_ns('perf_counter_ns')
            cost['input_bytes'] += len(chunk)
            cost['wall'].record(wall, wall_end)
            cost['thread_cpu'].record(cpu, cpu_end)

    def _feed(self, chunk):
        self.observed += len(chunk)
        self.observed_hash.update(chunk)
        if self.error:
            if self.projection is not None and self.error == 'MEMORY_LIMIT':
                self._frame(chunk, retained=False)
            return  # raw capture stays frozen; only explicit numeric observation continues
        if self.retained + len(chunk) > self.limit:
            self.error = 'MEMORY_LIMIT'
            # Exhausting observation storage must not kill the actual query.
            # The caller keeps the same process deadline and owns the final drain.
            if self.projection is not None:
                self._prefix_tail(project=False)
                self.projection.after_limit = True
                self._frame(chunk, retained=False)
            return
        try:
            written = os.write(self.fd, chunk)
        except BaseException:
            self.error = 'MEMORY_WRITE_FAILED'
            self.retained_identity_valid = False
            raise ObservationError(self.error) from None
        self.retained += written
        self.retained_hash.update(chunk[:written])
        if written != len(chunk):
            self.error = 'SHORT_MEMORY_WRITE'
            raise ObservationError(self.error)
        self._frame(chunk, retained=True)

    def finish(self, eof, cleanup_ok):
        self.eof, self.cleanup_ok = eof, cleanup_ok
        if self.fd is None:
            raise ObservationError('CAPTURE_FD_CLOSED')
        fcntl.fcntl(self.fd, ADD_SEALS, SEALS)
        self.sealed = fcntl.fcntl(self.fd, GET_SEALS) == SEALS
        if not self.sealed:
            self.error = 'MEMORY_NOT_SEALED'
            raise ObservationError(self.error)
        # The official JSON generator separates roots, but may omit a final LF.
        # A complete last JSON object is valid; truncated JSON remains invalid.
        if not self.prefix_finished:
            self._prefix_tail(project=True)
        elif self.pending and not self.skip_projection_line:
            self._record(self.pending, retained=False)
        self.pending = b''
        size = os.fstat(self.fd).st_size
        if size != self.retained:
            self.retained_identity_valid = False
            self.retained = size
            self.error = self.error or 'MEMORY_IDENTITY_MISMATCH'
        if self.projection is not None:
            try:
                self.projection.input_observed_bytes = self.observed
                if self.error and self.error != 'MEMORY_LIMIT':
                    self.projection.fail('INPUT_GAP')
                self.projection.finish(eof, cleanup_ok)
            except Exception:
                self.projection.fail('PROCESSING_FAILED')

    @property
    def path(self):
        if not self.sealed or self.fd is None:
            raise ObservationError('UNSEALED_SUMMARY_INPUT')
        # The CLI launcher may close inherited FDs. Reopen the live owner's FD.
        return f'/proc/{os.getpid()}/fd/{self.fd}'

    def summary(self):
        transport = self.eof and self.cleanup_ok and self.sealed
        captured = transport and not self.error and self.observed == self.retained
        log_complete = (captured and self.first == 'LOG_HEADER' and self.last == 'LOG_FOOTER'
                        and self.headers == self.footers == 1 and self.header_version
                        and not self.invalid and not getattr(self, 'pending_bytes', len(self.pending)))
        return dict(status='CAPTURED' if captured else 'INCOMPLETE',
                    observed_bytes=self.observed, retained_bytes=self.retained,
                    discarded_bytes=self.observed-self.retained if self.retained_identity_valid else None,
                    observed_sha256=self.observed_hash.hexdigest(),
                    retained_sha256=self.retained_hash.hexdigest() if self.retained_identity_valid else None,
                    memory_limit_bytes=self.limit, sealed=self.sealed, transport_eof=self.eof,
                    cleanup_ok=self.cleanup_ok, error=self.error,
                    log_boundary_status='COMPLETE' if log_complete else 'PARTIAL',
                    header_count=self.headers, footer_count=self.footers, records=self.records,
                    header_version_valid=self.header_version,
                    invalid_envelopes=self.invalid, truncated_bytes=getattr(self, 'pending_bytes', len(self.pending)),
                    limits='Transport, retained header/footer envelopes and query completion are independent; '
                           + ('numeric projection is separate from the frozen retained prefix; ' if self.projection is not None else
                              'discarded bytes are hashed and counted, not parsed; ')
                           + 'official summary validates captured events.')

    def feed_cost_summary(self):
        if self.feed_cost is None:
            return None
        return dict(version='capture-feed-cost-v1',
            buckets={key: dict(calls=value['wall'].intervals, input_bytes=value['input_bytes'],
                               wall=value['wall'].report(), thread_cpu=value['thread_cpu'].report())
                     for key, value in self.feed_cost.items()},
            scope='Whole feed processing body per entry-state bucket; wall includes waiting, '
                  'thread CPU is the calling thread only. Clock reads and aggregation add overhead; '
                  'not evaluator CPU or an uninstrumented comparison.')

    def close(self):
        try:
            if self.fd is not None:
                fd, self.fd = self.fd, None
                os.close(fd)
        finally:
            if self.projection is not None:
                self.projection = None
                self.pending = b''


class CompressedCapture(Capture):
    """Candidate-only lossless storage; inherited retained counters are encoded bytes."""
    def __init__(self, limit=ENCODED_CAP, *, projection=None):
        if type(limit) is not int or not 0 < limit <= ENCODED_CAP:
            raise ObservationError('INVALID_ENCODED_LIMIT')
        super().__init__(limit, projection=projection)
        self.encoder = None
        self.encoded_finished = False
        self.capacity_failure_phase = None
        self.encoding_input_bytes = 0
        self.encoding_input_hash = hashlib.sha256()
        try:
            self.encoder = zlib.compressobj(level=1, wbits=31, memLevel=8)
        except BaseException:
            self.close()
            raise ObservationError('ENCODER_CREATE_FAILED') from None

    def _store(self, data, phase):
        # Fixed codec parameters and <=64KiB input bound the pending deflate
        # output as well as the finish buffer; neither is an entire raw stream.
        if len(data) > 2 * CODEC_CHUNK:
            self.error = 'ENCODER_OUTPUT_LIMIT'
            raise ObservationError(self.error)
        if self.retained + len(data) > self.limit:
            self.error = 'MEMORY_LIMIT'
            self.capacity_failure_phase = phase
            self.encoder = None
            if self.projection is not None:
                self._prefix_tail(project=False)
                self.projection.after_limit = True
            return False
        try:
            written = os.write(self.fd, data) if data else 0
        except BaseException:
            self.error = 'MEMORY_WRITE_FAILED'
            self.retained_identity_valid = False
            raise ObservationError(self.error) from None
        self.retained += written
        self.retained_hash.update(data[:written])
        if written != len(data):
            self.error = 'SHORT_MEMORY_WRITE'
            raise ObservationError(self.error)
        return True

    def _feed(self, chunk):
        self.observed += len(chunk)
        self.observed_hash.update(chunk)
        for offset in range(0, len(chunk), CODEC_CHUNK):
            part = chunk[offset:offset + CODEC_CHUNK]
            if self.error:
                if self.error == 'MEMORY_LIMIT' and self.projection is not None:
                    self._frame(part, retained=False)
                continue
            try:
                data = self.encoder.compress(part)
            except BaseException:
                self.error = 'ENCODING_FAILED'
                raise ObservationError(self.error) from None
            if self._store(data, 'FEED'):
                self.encoding_input_bytes += len(part)
                self.encoding_input_hash.update(part)
                self._frame(part, retained=True)
            elif self.projection is not None:
                self._frame(part, retained=False)

    def feed(self, chunk):
        if self.sealed or self.fd is None:
            raise ObservationError('CAPTURE_ALREADY_CLOSED')
        if self.feed_cost is None:
            return self._feed(chunk)
        already_limited = self.error == 'MEMORY_LIMIT'
        wall, cpu = _clock_ns('perf_counter_ns'), _clock_ns('thread_time_ns')
        try:
            return self._feed(chunk)
        finally:
            cpu_end, wall_end = _clock_ns('thread_time_ns'), _clock_ns('perf_counter_ns')
            phase = ('AFTER_LIMIT' if already_limited else
                     'CAP_CROSSING' if self.error == 'MEMORY_LIMIT' else 'BEFORE_LIMIT')
            cost = self.feed_cost[phase]
            cost['input_bytes'] += len(chunk)
            cost['wall'].record(wall, wall_end)
            cost['thread_cpu'].record(cpu, cpu_end)

    def finish(self, eof, cleanup_ok):
        if self.sealed or self.fd is None:
            raise ObservationError('CAPTURE_ALREADY_CLOSED')
        if not self.error:
            try:
                data = self.encoder.flush(zlib.Z_FINISH)
            except BaseException:
                self.error = 'ENCODING_FINISH_FAILED'
                raise ObservationError(self.error) from None
            finally:
                self.encoder = None  # release deflate state before any replay
            self.encoded_finished = self._store(data, 'FINISH')
        self.encoder = None
        super().finish(eof, cleanup_ok)

    @property
    def path(self):
        raise ObservationError('COMPRESSED_INPUT_REQUIRES_REPLAY')

    def summary(self):
        captured = (self.eof and self.cleanup_ok and self.sealed and not self.error
                    and self.encoded_finished and self.retained_identity_valid
                    and self.encoding_input_bytes == self.observed)
        complete = (captured and self.first == 'LOG_HEADER' and self.last == 'LOG_FOOTER'
                    and self.headers == self.footers == 1 and self.header_version
                    and not self.invalid and not getattr(self, 'pending_bytes', len(self.pending)))
        return dict(version='gzip-capture-v1', encoding='gzip',
                    status='CAPTURED' if captured else 'INCOMPLETE',
                    observed_bytes=self.observed, observed_sha256=self.observed_hash.hexdigest(),
                    encoding_input_bytes=self.encoding_input_bytes,
                    encoding_input_sha256=self.encoding_input_hash.hexdigest(),
                    encoded_bytes=self.retained,
                    encoded_sha256=self.retained_hash.hexdigest() if self.retained_identity_valid else None,
                    encoded_limit_bytes=self.limit, codec_buffer_reserve_bytes=CODEC_RESERVE,
                    memory_reserve_bytes=RAW_CAP, codec=dict(level=1, window_bits=31, memory_level=8),
                    encoding_finished=self.encoded_finished, capacity_failure_phase=self.capacity_failure_phase,
                    sealed=self.sealed, transport_eof=self.eof, cleanup_ok=self.cleanup_ok, error=self.error,
                    log_boundary_status='COMPLETE' if complete else 'PARTIAL',
                    header_count=self.headers, footer_count=self.footers, records=self.records,
                    header_version_valid=self.header_version, invalid_envelopes=self.invalid,
                    truncated_bytes=getattr(self, 'pending_bytes', len(self.pending)),
                    limits='Observed input, bytes admitted to encoding and stored encoded bytes are distinct. '
                           'An unfinished encoding is not a replayable retained prefix. Complete replay and '
                           'official summary are independently required; the reserve is not a Python/RSS limit.')

    def feed_cost_summary(self):
        report = super().feed_cost_summary()
        if report is not None:
            report['version'] = 'compressed-capture-feed-cost-v1'
            report['scope'] = ('Whole feed body including compression, framing and hashing. Buckets use actual '
                'encoded-cap transitions; finish-only capacity failures are separate and are not feed calls. '
                'Wall includes waiting; thread CPU excludes other threads and child processes. '
                'Clock reads and aggregation add overhead; not an uninstrumented comparison.')
        return report

    def close(self):
        self.encoder = None
        super().close()


class GzipReplay:
    """One bounded writer, started only after the existing runtime has spawned."""
    ERRORS = {'REPLAY_STOPPED', 'REPLAY_DEADLINE', 'REPLAY_WRITE_FAILED',
              'REPLAY_ENCODED_TRUNCATED', 'REPLAY_TRAILING_DATA', 'REPLAY_RAW_LENGTH',
              'REPLAY_GZIP_TRUNCATED', 'REPLAY_IDENTITY_MISMATCH'}
    def __init__(self, capture, deadline):
        if capture.summary()['status'] != 'CAPTURED' or capture.fd is None:
            raise ObservationError('INCOMPLETE_REPLAY_INPUT')
        self.capture, self.deadline = capture, deadline
        self.input_fd = self.read_fd = self.write_fd = None
        self.thread = None
        self.start_attempted = self.worker_started = False
        self.stop = threading.Event()
        self.error = None
        self.encoded_bytes = self.decoded_bytes = self.written_bytes = 0
        self.encoded_hash, self.decoded_hash = hashlib.sha256(), hashlib.sha256()
        self.written_hash = hashlib.sha256()
        self.gzip_eof = self.identity_verified = self.consumer_eof = False
        self.input_closed = self.write_closed = self.read_closed = False
        self.group_safe = self.finished = self.cleanup_ok = False
        try:
            if (os.fstat(capture.fd).st_size != capture.retained
                    or fcntl.fcntl(capture.fd, GET_SEALS) != SEALS):
                raise ObservationError('REPLAY_STORAGE_IDENTITY')
            self.input_fd = os.dup(capture.fd)
            self.read_fd, self.write_fd = os.pipe2(os.O_CLOEXEC | os.O_NONBLOCK)
            self.pipe_capacity = fcntl.fcntl(self.write_fd, fcntl.F_GETPIPE_SZ)
            if not 0 < self.pipe_capacity <= CODEC_CHUNK:
                raise ObservationError('REPLAY_PIPE_LIMIT')
        except BaseException:
            for name in ('input', 'write', 'read'):
                self._close_fd(name)
            raise ObservationError('REPLAY_ADMISSION_FAILED') from None

    def _fail(self, code):
        self.error = self.error or code

    def _close_fd(self, name):
        fd = getattr(self, name + '_fd')
        if fd is not None:
            setattr(self, name + '_fd', None)
            try:
                os.close(fd)
                setattr(self, name + '_closed', True)
            except BaseException:
                self._fail('REPLAY_CLOSE_FAILED')

    @property
    def path(self):
        if self.read_fd is None:
            raise ObservationError('REPLAY_PIPE_CLOSED')
        return f'/proc/{os.getpid()}/fd/{self.read_fd}'

    def start(self, payload_end):
        if self.start_attempted:
            return
        self.start_attempted = True
        self.payload_end = min(self.deadline, payload_end)
        self.thread = threading.Thread(target=self._write, name='codeql-gzip-replay', daemon=False)
        try:
            self.thread.start()
            self.worker_started = True
        except BaseException:
            self._fail('REPLAY_START_FAILED')
            raise ObservationError(self.error) from None

    def _check(self):
        if self.stop.is_set():
            raise ObservationError('REPLAY_STOPPED')
        if time.monotonic() >= self.payload_end:
            raise ObservationError('REPLAY_DEADLINE')

    def _send(self, data):
        view = memoryview(data)
        offset = 0
        while offset < len(view):
            self._check()
            try:
                written = os.write(self.write_fd, view[offset:])
            except InterruptedError:
                continue
            except BlockingIOError:
                try:
                    select.select([], [self.write_fd], [], min(.02, max(0, self.payload_end-time.monotonic())))
                except InterruptedError:
                    pass
                continue
            if written <= 0:
                raise ObservationError('REPLAY_WRITE_FAILED')
            self.written_hash.update(view[offset:offset+written])
            self.written_bytes += written
            offset += written

    def _write(self):
        try:
            decoder = zlib.decompressobj(wbits=31)
            while self.encoded_bytes < self.capture.retained:
                self._check()
                data = os.pread(self.input_fd, min(CODEC_CHUNK, self.capture.retained-self.encoded_bytes),
                                self.encoded_bytes)
                if not data:
                    raise ObservationError('REPLAY_ENCODED_TRUNCATED')
                self.encoded_bytes += len(data)
                self.encoded_hash.update(data)
                while True:
                    self._check()
                    raw = decoder.decompress(data, CODEC_CHUNK)
                    data = decoder.unconsumed_tail
                    if decoder.unused_data:
                        raise ObservationError('REPLAY_TRAILING_DATA')
                    self.decoded_bytes += len(raw)
                    if self.decoded_bytes > self.capture.observed:
                        raise ObservationError('REPLAY_RAW_LENGTH')
                    self.decoded_hash.update(raw)
                    self._send(raw)
                    if not data and len(raw) < CODEC_CHUNK:
                        break
            self.gzip_eof = decoder.eof
            if not self.gzip_eof:
                raise ObservationError('REPLAY_GZIP_TRUNCATED')
            if (self.encoded_bytes != self.capture.retained
                    or self.encoded_hash.digest() != self.capture.retained_hash.digest()
                    or self.decoded_bytes != self.capture.observed
                    or self.decoded_hash.digest() != self.capture.observed_hash.digest()
                    or self.written_bytes != self.decoded_bytes
                    or self.written_hash.digest() != self.decoded_hash.digest()):
                raise ObservationError('REPLAY_IDENTITY_MISMATCH')
            self.identity_verified = True
        except ObservationError as error:
            code = str(error)
            self._fail(code if code in self.ERRORS else 'REPLAY_CODEC_OR_IO_FAILED')
        except BaseException:
            self._fail('REPLAY_CODEC_OR_IO_FAILED')
        finally:
            # After start, only this worker closes these descriptors. Closing
            # them from another thread could reuse an fd for a raw evidence write.
            self._close_fd('input')
            self._close_fd('write')

    def alive(self):
        return self.worker_started and self.thread.is_alive()

    def finish(self, *, group_safe):
        if self.finished:
            return self.report()
        self.finished = True
        self.group_safe = group_safe
        self.stop.set()
        if self.worker_started:
            self.thread.join(timeout=max(0, min(REPLAY_CLOSE_SECONDS, self.deadline-time.monotonic())))
        else:
            self._fail('REPLAY_NOT_STARTED')
            self._close_fd('input')
            self._close_fd('write')
        if self.alive():
            self._fail('REPLAY_WORKER_NOT_CLOSED')
            return self.report()  # retain live-owned fds, never allow fd reuse
        if group_safe and self.write_closed:
            try:
                self.consumer_eof = os.read(self.read_fd, 1) == b''
                if not self.consumer_eof:
                    self._fail('REPLAY_UNCONSUMED_DATA')
            except BlockingIOError:
                self._fail('REPLAY_EOF_NOT_OBSERVED')
            except BaseException:
                self._fail('REPLAY_EOF_FAILED')
        elif not group_safe:
            self._fail('REPLAY_GROUP_UNCLEAN')
        self._close_fd('read')
        if time.monotonic() > self.deadline:
            self._fail('REPLAY_FIXED_DEADLINE_EXCEEDED')
        self.cleanup_ok = self.input_closed and self.write_closed and self.read_closed and group_safe
        return self.report()

    def report(self):
        complete = (self.finished and not self.error and self.identity_verified and self.gzip_eof
                    and self.consumer_eof and self.cleanup_ok and not self.alive())
        return dict(version='gzip-replay-v1', status='COMPLETE' if complete else 'INCOMPLETE', error=self.error,
                    encoded_bytes=self.encoded_bytes, encoded_sha256=self.encoded_hash.hexdigest(),
                    decoded_bytes=self.decoded_bytes, decoded_sha256=self.decoded_hash.hexdigest(),
                    written_bytes=self.written_bytes, written_sha256=self.written_hash.hexdigest(),
                    identity_verified=self.identity_verified, gzip_eof=self.gzip_eof,
                    consumer_eof=self.consumer_eof, worker_started=self.worker_started,
                    worker_alive=self.alive(), owned_group_safe=self.group_safe, cleanup_ok=self.cleanup_ok,
                    pipe_capacity_bytes=self.pipe_capacity,
                    limits='Exact decoded identity and final pipe EOF after both owners exit are required. '
                           'Writing a complete pipe stream alone does not prove consumption.')

    def close(self):
        if not self.finished:
            self.finish(group_safe=False)
        if self.alive():
            raise ObservationError('REPLAY_WORKER_NOT_CLOSED')
        self._close_fd('read')


class Lines:
    """Bounded line framing with per-stream hashes; raw bytes never leave feed."""
    def __init__(self):
        self.streams = {name: dict(pending=b'', bytes=0, sha=hashlib.sha256(), lines=0,
                                  rejected=0, final=False, processed_prefix_bytes=0) for name in ('stdout', 'stderr')}
        self.feed_failure = None

    def fail_feed(self, code):
        self.feed_failure = self.feed_failure or code
        for stream in self.streams.values():
            stream['pending'] = b''

    def line(self, name, data):
        return b''

    def feed(self, name, chunk, final=False):
        stream = self.streams[name]
        if stream['final']:
            if chunk:
                raise ObservationError('STREAM_AFTER_EOF')
            return b''
        stream['bytes'] += len(chunk)
        stream['sha'].update(chunk)
        if self.feed_failure:
            stream['final'] = stream['final'] or final
            return b''
        stream['pending'] += chunk
        projected = []
        try:
            while b'\n' in stream['pending']:
                line, remaining = stream['pending'].split(b'\n', 1)
                if len(line) > LINE_CAP:
                    raise ObservationError('STREAM_RECORD_LIMIT')
                projected.append(self.line(name, line))
                stream['lines'] += 1
                stream['processed_prefix_bytes'] += len(line) + 1
                stream['pending'] = remaining
            if len(stream['pending']) > LINE_CAP:
                raise ObservationError('STREAM_RECORD_LIMIT')
            if final:
                stream['final'] = True
                if stream['pending']:
                    stream['rejected'] += 1
                    stream['pending'] = b''
        except BaseException:
            self.fail_feed('STREAM_PROCESSING_FAILED')
            raise
        return b''.join(projected)

    def poll(self, deadline):
        if time.monotonic() >= deadline:
            raise TimeoutError('OBSERVATION_DEADLINE')

    def summary(self):
        return {name: {**{key: stream[key] for key in ('bytes', 'lines', 'rejected', 'final', 'processed_prefix_bytes')},
                       'pending_bytes': len(stream['pending']),
                       'feed_failure': self.feed_failure,
                       'sha256': stream['sha'].hexdigest()} for name, stream in self.streams.items()}


class Progress(Lines):
    def __init__(self, records):
        super().__init__()
        self.queries = {}
        self.counts = {phase: 0 for phase in ('COMPILE_START', 'COMPILE_DONE', 'EVALUATE_START', 'EVALUATE_DONE')}
        for index, record in enumerate(records):
            identity = dict(index=index, query_sha256=record['sha256'])
            relative = record.get('relative', Path(record['path']).name)
            for path in (record['path'], record['pack'] + '/' + relative,
                         record['pack'] + '/' + str(Path(relative).with_suffix('.bqrs'))):
                if path in self.queries and self.queries[path] != identity:
                    raise ObservationError('AMBIGUOUS_FROZEN_QUERY')
                self.queries[path] = identity

    def line(self, name, data):
        try:
            line = data.decode('utf-8')
        except UnicodeError:
            self.streams[name]['rejected'] += 1
            return b''
        phase = path = None
        for prefix, event in (('Compiling query plan for ', 'COMPILE_START'),
                              ('Starting evaluation of ', 'EVALUATE_START')):
            if line.startswith(prefix) and line.endswith('.'):
                phase, path = event, line[len(prefix):-1]
        match = re.fullmatch(r'\[([0-9]{1,3})/([0-9]{1,3}) (comp|eval) ([0-9hms.]{1,32})\] (Compiled |Evaluation done; writing results to )(.+)\.', line)
        if match and ((match[3], match[5]) in (('comp', 'Compiled '), ('eval', 'Evaluation done; writing results to '))):
            phase, path = ('COMPILE_DONE' if match[3] == 'comp' else 'EVALUATE_DONE'), match[6]
        if phase is None or path not in self.queries:
            self.streams[name]['rejected'] += 1
            return b''
        self.counts[phase] += 1
        return (json.dumps(dict(event=phase, stream=name, **self.queries[path],
                               received_wall=time.time(), received_monotonic=time.monotonic()),
                           separators=(',', ':')) + '\n').encode()

    def summary(self):
        return dict(streams=super().summary(), events=self.counts,
                    timestamp_scope='Receive times include buffering; not internal CLI phase boundaries.')


FIELD_TYPES = {
    'appearsAs': dict, 'completionTime': str, 'completionType': str, 'dependencies': dict,
    'evaluationStrategy': str, 'isCached': bool, 'millis': int, 'position': str,
    'predicateName': str, 'queryCausingWork': str, 'raHash': str, 'resultSize': int,
    'ra': dict, 'pipelineRuns': list, 'predicateIterationMillis': list, 'deltaSizes': list,
    'mainHash': str, 'layerSize': int, 'sentinelRaHash': str, 'exceptionName': str,
}
STRATEGIES = {'IN_LAYER', 'COMPUTE_RECURSIVE', 'COMPUTE_SIMPLE', 'NAMED_LOCAL', 'EXTENSIONAL',
              'EXTERNAL_PREDICATE', 'COMPUTED_EXTENSIONAL', 'CACHE_HIT', 'CACHACA',
              'SENTINEL_EMPTY', 'EXTENSIBLE_PREDICATE'}
COMPLETIONS = {'SUCCESS', 'FAILURE', 'INCOMPLETE'}


def text_value(value, limit=8192):
    if type(value) is not str:
        raise ObservationError('STRING_TYPE')
    try:
        data = value.encode('utf-8')
    except UnicodeError:
        raise ObservationError('STRING_ENCODING') from None
    if len(data) > limit:
        raise ObservationError('STRING_LIMIT')
    return data


def integer(value, minimum=-1, maximum=2**63-1):
    if type(value) is not int or not minimum <= value <= maximum:
        raise ObservationError('INTEGER_TYPE_OR_RANGE')


def validate_record(value):
    if type(value) is not dict or any(key not in FIELD_TYPES or type(item) is not FIELD_TYPES[key]
                                     for key, item in value.items()):
        raise ObservationError('FIELD_OR_TYPE')
    if not {'raHash', 'predicateName', 'evaluationStrategy', 'appearsAs'} <= value.keys():
        raise ObservationError('MISSING_IDENTITY')
    if value['evaluationStrategy'] not in STRATEGIES or not value['raHash'] or not value['predicateName']:
        raise ObservationError('IDENTITY_OR_STRATEGY')
    if 'completionType' in value and value['completionType'] not in COMPLETIONS:
        raise ObservationError('COMPLETION_ENUM')
    for key, item in value.items():
        if FIELD_TYPES[key] is str:
            text_value(item)
        elif FIELD_TYPES[key] is int:
            integer(item, 0 if key in ('millis', 'layerSize') else -1)
    for key in ('predicateIterationMillis', 'deltaSizes'):
        for item in value.get(key, []):
            if item is None and key == 'deltaSizes':
                continue  # explicitly unknown, never zero; separate counter below
            integer(item)
    for alias, queries in value['appearsAs'].items():
        text_value(alias)
        if type(queries) is not dict:
            raise ObservationError('APPEARANCE_QUERY_MAP')
        for query, stages in queries.items():
            text_value(query)
            if type(stages) is not list:
                raise ObservationError('APPEARANCE_STAGES')
            for stage in stages:
                integer(stage, 0, 2**31-1)
    for name, identity in value.get('dependencies', {}).items():
        text_value(name); text_value(identity)
    for name, expressions in value.get('ra', {}).items():
        text_value(name)
        if type(expressions) is not list:
            raise ObservationError('RA_EXPRESSIONS')
        for expression in expressions:
            text_value(expression, LINE_CAP)
    pipeline_types = {'raReference': str, 'counts': list, 'duplicationPercentages': list,
                      'completionType': str, 'exceptionName': str}
    for pipeline in value.get('pipelineRuns', []):
        if type(pipeline) is not dict or any(key not in pipeline_types or type(item) is not pipeline_types[key]
                                           for key, item in pipeline.items()):
            raise ObservationError('PIPELINE_FIELD_OR_TYPE')
        if 'completionType' in pipeline and pipeline['completionType'] not in COMPLETIONS:
            raise ObservationError('PIPELINE_COMPLETION')
        for key, item in pipeline.items():
            if pipeline_types[key] is str:
                text_value(item)
            else:
                for number in item:
                    integer(number)  # percentages can exceed 100 in official estimates


class Summary(Lines):
    """Strict 2.27.4 summary projection. Unknown combinations remain partial."""
    def __init__(self, records, *, replay=None):
        super().__init__()
        self.replay = replay
        schema_path = Path(__file__).with_name('codeql_summary_schema_2_27_4.json')
        if schema_path.stat().st_size > 256 * 1024:
            raise ObservationError('SCHEMA_LIMIT')
        schema = parse_json(schema_path.read_bytes())
        if schema.get('codeqlVersion') != '2.27.4' or schema.get('summaryLogVersion') != '0.4.0':
            raise ObservationError('SCHEMA_VERSION')
        self.signatures = schema['signatures']
        if not 0 < len(self.signatures) <= 128:
            raise ObservationError('SCHEMA_SIGNATURE_LIMIT')
        self.schema_sha256 = runtime.digest(schema_path)
        self.queries = Progress(records).queries
        self.header = None
        self.records = self.accepted = self.untimed = self.markers = self.incomplete = self.failed = 0
        self.unknown = self.invalid = self.ra_reuse = self.delta_unknown = 0
        self.null_deltas = self.pipeline_non_success = self.exception_records = 0
        self.footer_missing = False
        self.seen = {}
        self.reuse_examples = []
        self.rejected_hashes = []
        self.tops = {'official_millis': [], 'known_iteration_subtotal': []}

    def poll(self, deadline):
        super().poll(deadline)
        if self.replay is not None:
            # runtime calls poll only after Popen and stream registration. No
            # writer thread exists during its fork/preexec path.
            self.replay.start(deadline)
            if self.replay.error:
                raise ObservationError('SUMMARY_REPLAY_FAILED')

    def _reject(self, field, data):
        setattr(self, field, getattr(self, field) + 1)
        if len(self.rejected_hashes) < 16:
            self.rejected_hashes.append(_hash(data))

    def _known_shape(self, value):
        shape = {key: type(item).__name__ for key, item in value.items()}
        enums = {key: value[key] for key in ('evaluationStrategy', 'completionType', 'isCached') if key in value}
        return any(shape == signature['types'] and enums == signature['enums'] for signature in self.signatures)

    def _query(self, name):
        return self.queries.get(name, dict(unmapped_query_sha256=_hash(text_value(name))))

    def line(self, name, data):
        if name == 'stderr':
            try:
                line = data.decode('utf-8').removeprefix('ERROR: ')
            except UnicodeError:
                line = ''
            if line.startswith('Reached end of log file before footer was found.'):
                self.footer_missing = True
            self.streams[name]['rejected'] += 1
            return b''
        if self.records >= MAX_SUMMARY_RECORDS:
            raise ObservationError('SUMMARY_RECORD_LIMIT')
        try:
            value = parse_json(data)
            if type(value) is dict and 'summaryLogVersion' in value:
                if (self.header is not None or self.records
                        or set(value) != {'summaryLogVersion', 'codeqlVersion', 'startTime'}
                        or value['summaryLogVersion'] != '0.4.0' or value['codeqlVersion'] != '2.27.4'):
                    raise ObservationError('SUMMARY_HEADER')
                self.header = dict(summaryLogVersion='0.4.0', codeqlVersion='2.27.4',
                                   start_time_sha256=_hash(text_value(value['startTime'], 128)))
                return b''
            self.records += 1
            if self.header is None:
                raise ObservationError('MISSING_HEADER')
            validate_record(value)
            completion = value.get('completionType', 'IMPLICIT_SUCCESS')
            self.incomplete += completion == 'INCOMPLETE'
            self.failed += completion == 'FAILURE'
            self.pipeline_non_success += sum(p.get('completionType') in ('FAILURE', 'INCOMPLETE')
                                             for p in value.get('pipelineRuns', []))
            self.exception_records += 'exceptionName' in value or any('exceptionName' in p for p in value.get('pipelineRuns', []))
            self.null_deltas += value.get('deltaSizes', []).count(None)
            if not self._known_shape(value):
                self._reject('unknown', data)
                return b''
            ra_id, name_id = _hash(text_value(value['raHash'])), _hash(text_value(value['predicateName']))
            strategy = value['evaluationStrategy']
            if ra_id in self.seen:
                # Summary rows are event instances. Equal RA content hashes can
                # describe aliases, metadata or separate evaluations, not one ID.
                self.ra_reuse += 1
                if len(self.reuse_examples) < 16:
                    self.reuse_examples.append(dict(ra_identity_sha256=ra_id,
                        first_record=self.seen[ra_id], record=self.records, strategy=strategy))
            else:
                self.seen[ra_id] = self.records
            self.accepted += 1
            millis = value.get('millis')
            iterations = value.get('predicateIterationMillis', [])
            known = [number for number in iterations if number >= 0]
            subtotal = sum(known) if known else None
            if subtotal is not None:
                integer(subtotal, 0)
            missing = iterations.count(-1)
            self.markers += missing
            self.delta_unknown += sum(number is None or number == -1 for number in value.get('deltaSizes', []))
            if millis is None and subtotal is None:
                self.untimed += 1
                return b''
            associations = {query for queries in value['appearsAs'].values() for query in queries}
            mapped = sorted({self.queries[query]['index'] for query in associations if query in self.queries})
            row = dict(record=self.records, ra_identity_sha256=ra_id, predicate_name_sha256=name_id,
                       first_record_with_ra_hash=self.seen[ra_id],
                       evaluation_strategy=strategy, completion=completion,
                       category='recursive_layer' if strategy == 'COMPUTE_RECURSIVE' else
                                'layer_member' if strategy in ('IN_LAYER', 'NAMED_LOCAL') else 'nonrecursive',
                       has_completion_time='completionTime' in value,
                       cached_annotation=value.get('isCached'), reported_millis=millis,
                       known_iteration_subtotal=subtotal, known_iteration_entries=len(known),
                       negative_iteration_markers=missing, appears_in_query_indexes=mapped,
                       unmapped_query_count=sum(query not in self.queries for query in associations),
                       dependencies_count=len(value.get('dependencies', {})))
            for key in ('position', 'exceptionName', 'mainHash', 'sentinelRaHash'):
                if key in value:
                    row[key + '_sha256'] = _hash(text_value(value[key]))
            if 'queryCausingWork' in value:
                row['first_requiring_query'] = self._query(value['queryCausingWork'])
            for metric, score in (('official_millis', millis), ('known_iteration_subtotal', subtotal)):
                if score is not None:
                    heapq.heappush(self.tops[metric], (score, self.records, row))
                    if len(self.tops[metric]) > 50:
                        heapq.heappop(self.tops[metric])
        except ObservationError:
            self._reject('invalid', data)
        return b''

    def feed(self, name, chunk, final=False):
        if not self.feed_failure and sum(stream['bytes'] for stream in self.streams.values()) + len(chunk) > RAW_CAP:
            self.fail_feed('SUMMARY_INPUT_LIMIT')
            super().feed(name, chunk, final=final)  # include the rejected chunk in observed bytes/hash
            raise ObservationError('SUMMARY_INPUT_LIMIT')
        output = super().feed(name, chunk, final=False)
        stream = self.streams[name]
        if final and not stream['final']:
            try:
                if stream['pending'] and not self.feed_failure:
                    # Root separators need no trailing LF. Validate the last object.
                    self.line(name, stream['pending'])
                    stream['lines'] += 1
                    stream['processed_prefix_bytes'] += len(stream['pending'])
                    stream['pending'] = b''
            except BaseException:
                self.fail_feed('SUMMARY_FINAL_RECORD_FAILED')
                raise
            stream['final'] = True
        return output

    def summary(self):
        streams = super().summary()
        complete = (self.header is not None and self.accepted and any(self.tops.values())
                    and not self.feed_failure
                    and not any((self.unknown, self.invalid, self.incomplete, self.failed))
                    and not any((self.null_deltas, self.pipeline_non_success, self.exception_records))
                    and all(value['final'] for value in streams.values()) and not streams['stderr']['bytes'])
        return dict(status='PROJECTED' if complete else 'PARTIAL', header=self.header,
                    schema_sha256=self.schema_sha256, streams=streams, records=self.records,
                    accepted_records=self.accepted, unknown_shapes=self.unknown, invalid_records=self.invalid,
                    reused_ra_hash_records=self.ra_reuse, distinct_ra_hashes=len(self.seen),
                    ra_reuse_examples=self.reuse_examples, rejected_record_sha256=self.rejected_hashes,
                    untimed_records=self.untimed, incomplete_predicates=self.incomplete, failed_predicates=self.failed,
                    negative_iteration_markers=self.markers, unknown_delta_entries=self.delta_unknown,
                    unobserved_delta_null_entries=self.null_deltas, pipeline_non_success=self.pipeline_non_success,
                    exception_records=self.exception_records,
                    input_footer_missing=self.footer_missing,
                    feed_failure=self.feed_failure,
                    rankings={key: [item[2] for item in sorted(items, reverse=True)] for key, items in self.tops.items()},
                    limits='Only admitted observed records are ranked. Millis and known iteration subtotals are separate, '
                           'not additive CPU. Layer/member work overlaps. Cache annotation is not a cache hit. '
                           'Official record ordinals identify observations; reused RA hashes are not event IDs and do not '
                           'justify deduplication or summation. First requiring query is not exclusive cost attribution; no cross-run joins.')


class Observation:
    """One batch and at most one summary, charged to the original private budget."""
    def __init__(self, root, work, records):
        self.root, self.work, self.records = root, work, records
        self.out = root / 'candidate-observation'
        self.capture = self.progress = None
        self.original_caps = None
        self.batch_result = None
        self.batch_parent_cpu = ClockTotal()
        self.batch_safe = False
        self.summary_result = dict(status='NOT_REACHED')
        self.summary_attempted = False
        self.numeric_result = dict(status='NOT_REACHED')
        self.replay = None

    def start(self):
        if runtime.WRITE_CAP - runtime.output_bytes(self.root) < RAW_CAP + OUTPUT_CAP:
            raise ObservationError('INSUFFICIENT_SHARED_EVIDENCE_BUDGET')
        self.out.mkdir(exist_ok=False)
        self.original_caps = (runtime.CAP, runtime.WRITE_CAP)
        runtime.CAP -= RAW_CAP + NUMERIC_RESERVE
        runtime.WRITE_CAP -= RAW_CAP + NUMERIC_RESERVE
        try:
            self.capture = CompressedCapture(projection=NumericProjection())
            self.progress = Progress(self.records)
        except BaseException:
            self.close()
            raise

    @contextmanager
    def output_budget(self):
        before = (runtime.CAP, runtime.WRITE_CAP)
        other = runtime.output_bytes(self.root) - runtime.output_bytes(self.out)
        runtime.CAP = min(runtime.CAP, other + OUTPUT_CAP - NUMERIC_RESERVE)
        runtime.WRITE_CAP = min(runtime.WRITE_CAP, runtime.CAP - 1024**2)
        try:
            yield
        finally:
            runtime.CAP, runtime.WRITE_CAP = before

    def run_batch(self, argv, cwd, end, timeout, env):
        if self.capture is None or self.batch_result is not None:
            raise ObservationError('BATCH_LIFECYCLE')
        separator = argv.index('--')
        argv = [*argv[:separator], '--evaluator-log', '{EVALUATOR_FIFO}', '--evaluator-log-minify',
                '--evaluator-log-level=3', '--no-tuple-counting', *argv[separator:]]
        with self.output_budget():
            parent_cpu = _clock_ns('process_time_ns')
            try:
                self.batch_result = runtime.run(argv, cwd, self.out / 'candidate-workload-batch.log',
                    end, timeout, self.root, env, file_limit=None, evaluator=self.work / 'candidate-events.jsonl',
                    observer=self.progress, evaluator_capture=self.capture, restricted_output=True)
                self.batch_safe = True
                return self.batch_result
            except runtime.PhaseError as error:
                self.batch_result = error.result
                self.batch_safe = error.clean_timeout
                raise
            finally:
                self.batch_parent_cpu.record(parent_cpu, _clock_ns('process_time_ns'))

    def summarize(self, cli, cwd, env, end, *, admitted):
        if self.summary_attempted:
            raise ObservationError('SUMMARY_ALREADY_ATTEMPTED')
        self.summary_attempted = True
        if not admitted or not self.batch_safe or not self.capture:
            self.summary_result = dict(status='NOT_REACHED_UNSAFE_FAILURE')
            return
        if self.capture.summary()['status'] != 'CAPTURED':
            self.summary_result = dict(status='NOT_REACHED_MEMORY_LIMIT' if self.capture.error == 'MEMORY_LIMIT'
                                       else 'NOT_REACHED_UNSAFE_FAILURE')
            return
        if end - time.monotonic() < SUMMARY_SECONDS:
            self.summary_result = dict(status='NOT_REACHED_FIXED_DEADLINE')
            return
        needed = 2 * runtime.MAX_PROCESS_RECEIPT + runtime.MAX_CONTROL_RECEIPT
        if (OUTPUT_CAP - NUMERIC_RESERVE - runtime.output_bytes(self.out) < needed
                or runtime.WRITE_CAP - runtime.output_bytes(self.root) < needed):
            self.summary_result = dict(status='NOT_REACHED_EVIDENCE_BUDGET')
            return
        with self.output_budget():
            summary_end = min(end, time.monotonic() + SUMMARY_SECONDS)
            result = None
            try:
                self.replay = GzipReplay(self.capture, summary_end)
                observer = Summary(self.records, replay=self.replay)
                result = runtime.run([str(cli), 'generate', 'log-summary', '--format=predicates',
                                      '--minify-output', '--utc', '--verbosity=errors', self.replay.path],
                    cwd, self.out / 'official-summary.log', summary_end-REPLAY_CLOSE_SECONDS,
                    SUMMARY_SECONDS, self.root, env,
                    file_limit=None, observer=observer, restricted_output=True)
                self.summary_result = dict(status=observer.summary()['status'], exit=result['exit'],
                                           failure_kind=None, projection=observer.summary())
            except runtime.PhaseError as error:
                result = error.result
                self.summary_result = dict(status='PARTIAL', exit=error.result['exit'],
                    failure_kind=error.result['failure_kind'], projection=error.result.get('output_observation'))
            except Exception:
                self.summary_result = dict(status='PARTIAL', failure_kind='SUMMARY_ADMISSION_ERROR')
            finally:
                if self.replay is not None:
                    group_safe = (result is not None and bool(result.get('pid'))
                                  and not result.get('cleanup_errors'))
                    replay = self.replay.finish(group_safe=group_safe)
                    self.summary_result['replay'] = replay
                    if replay['status'] != 'COMPLETE':
                        self.summary_result['status'] = 'PARTIAL'
                        self.summary_result['failure_kind'] = (self.summary_result.get('failure_kind')
                                                               or 'SUMMARY_REPLAY_INCOMPLETE')

    def receipt(self):
        capture = self.capture.summary() if self.capture else dict(status='NOT_REACHED')
        if self.capture and self.capture.feed_cost is not None:
            capture['feed_cost'] = self.capture.feed_cost_summary()
        complete = (capture['status'] == 'CAPTURED' and capture['log_boundary_status'] == 'COMPLETE'
                    and self.summary_result['status'] == 'PROJECTED'
                    and self.summary_result.get('replay', {}).get('status') == 'COMPLETE')
        return dict(version=VERSION, instrumented=True, status='COMPLETE' if complete else 'INCOMPLETE',
                    capture=capture, progress=self.progress.summary() if self.progress else None,
                    batch_parent_cpu=dict(self.batch_parent_cpu.report(),
                        scope='Parent process CPU across the single runtime.run call, including all parent '
                              'threads, supervision, sampling, feed, drain, cleanup and process receipt writes; '
                              'excludes child/PGID CPU and other job phases. Overlaps feed CPU; do not add '
                              'or subtract these counters to attribute exclusive component costs.'),
                    summary=self.summary_result, numeric_projection=self.numeric_result,
                    raw_memory_reserve_bytes=RAW_CAP, numeric_memory_reserve_bytes=NUMERIC_RESERVE,
                    observation_output_limit_bytes=OUTPUT_CAP, summary_limit_seconds=SUMMARY_SECONDS,
                    persisted_observation_limit_bytes=OUTPUT_CAP-NUMERIC_RESERVE,
                    budget_scope='Encoded storage and codec/transport reserve, numeric state, persisted evidence '
                                 'and terminal reserve share 64MiB; '
                                 'numeric state and persisted observation together share 8MiB, not a Python/RSS hard limit.',
                    limits='Observation completeness never certifies batch computation or oracle success.')

    def finish(self):
        with self.output_budget():
            if self.capture and self.capture.projection is not None:
                try:
                    numeric = self.capture.projection.report()
                except Exception:
                    numeric = dict(version=NumericProjection.VERSION, status='PARTIAL', error='PROJECTION_RECEIPT_FAILED')
                numeric['observed_stream_sha256'] = self.capture.observed_hash.hexdigest()
                numeric['observed_stream_bytes'] = self.capture.observed
                numeric['capacity_basis'] = 'gzip-encoded-bytes'
                if len((json.dumps(numeric, indent=2)+'\n').encode()) > runtime.MAX_CONTROL_RECEIPT:
                    numeric = dict(version=NumericProjection.VERSION, status='PARTIAL', error='PROJECTION_RECEIPT_LIMIT')
                path = self.out / 'numeric-projection.json'
                runtime.save(path, numeric, self.root, terminal=True)
                self.numeric_result = dict(version=NumericProjection.VERSION, status=numeric['status'],
                    receipt='candidate-observation/numeric-projection.json', receipt_sha256=runtime.digest(path))
            receipt = self.receipt()
            runtime.save(self.out / 'observation-terminal.json', receipt, self.root, terminal=True)
        return dict(version=VERSION, status=receipt['status'], instrumented=True,
                    capture_status=receipt['capture']['status'], summary_status=self.summary_result['status'],
                    numeric_status=self.numeric_result['status'],
                    receipt='candidate-observation/observation-terminal.json',
                    receipt_sha256=runtime.digest(self.out / 'observation-terminal.json'))

    def close(self):
        if self.replay is not None:
            # A failed join keeps its reservation and descriptors owned until
            # the worker exits. Never release them into an unrelated operation.
            self.replay.close()
        try:
            if self.capture:
                self.capture.close()
        finally:
            if self.original_caps is not None:
                runtime.CAP, runtime.WRITE_CAP = self.original_caps
                self.original_caps = None
