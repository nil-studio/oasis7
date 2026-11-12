"""Private candidate-only CodeQL 2.27.4 observation; raw events stay in memory."""
from contextlib import contextmanager
import fcntl
import hashlib
import heapq
import json
import os
from pathlib import Path
import re
import time

import codeql_rust_diagnostic_runtime as runtime

VERSION = 'candidate-evaluator-v1'
RAW_CAP = 48 * 1024**2
OUTPUT_CAP = 8 * 1024**2
SUMMARY_SECONDS = 20
LINE_CAP = 2 * 1024**2
MAX_SUMMARY_RECORDS = 100000
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


class Capture:
    """Caller owns the sealed memfd until the official summary has exited."""
    def __init__(self, limit=RAW_CAP):
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

    def _record(self, line):
        self.records += 1
        try:
            value = parse_json(line)
            if not isinstance(value, dict) or not isinstance(value.get('type'), str):
                raise ObservationError('INVALID_EVENT_ENVELOPE')
            kind = value['type']
            self.first = kind if self.first is None else self.first
            self.last = kind
            self.headers += kind == 'LOG_HEADER'
            self.footers += kind == 'LOG_FOOTER'
            if kind == 'LOG_HEADER':
                self.header_version = value.get('codeqlVersion') == '2.27.4' and value.get('logVersion') == '0.5.0'
        except ObservationError:
            self.invalid += 1

    def feed(self, chunk):
        if self.sealed or self.fd is None:
            raise ObservationError('CAPTURE_ALREADY_CLOSED')
        self.observed += len(chunk)
        self.observed_hash.update(chunk)
        if self.error:
            return  # capped observation or failed-run cleanup: hash only, never persist
        if self.retained + len(chunk) > self.limit:
            self.error = 'MEMORY_LIMIT'
            # Exhausting observation storage must not kill the actual query.
            # The caller keeps the same process deadline and owns the final drain.
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
        self.pending += chunk
        while b'\n' in self.pending:
            line, self.pending = self.pending.split(b'\n', 1)
            if len(line) > LINE_CAP:
                self.invalid += 1
            elif line:
                self._record(line)
        if len(self.pending) > LINE_CAP:
            self.error = 'EVENT_RECORD_LIMIT'
            raise ObservationError(self.error)

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
        invalid_before = self.invalid
        if len(self.pending) > LINE_CAP:
            self.invalid += 1
        elif self.pending:
            self._record(self.pending)
        self.pending_bytes = len(self.pending) if self.invalid > invalid_before else 0
        self.pending = b''
        size = os.fstat(self.fd).st_size
        if size != self.retained:
            self.retained_identity_valid = False
            self.retained = size
            self.error = self.error or 'MEMORY_IDENTITY_MISMATCH'

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
                           'discarded bytes are hashed and counted, not parsed; official summary validates captured events.')

    def close(self):
        if self.fd is not None:
            fd, self.fd = self.fd, None
            os.close(fd)


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
    def __init__(self, records):
        super().__init__()
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
        self.batch_safe = False
        self.summary_result = dict(status='NOT_REACHED')
        self.summary_attempted = False

    def start(self):
        if runtime.WRITE_CAP - runtime.output_bytes(self.root) < RAW_CAP + OUTPUT_CAP:
            raise ObservationError('INSUFFICIENT_SHARED_EVIDENCE_BUDGET')
        self.out.mkdir(exist_ok=False)
        self.original_caps = (runtime.CAP, runtime.WRITE_CAP)
        runtime.CAP -= RAW_CAP
        runtime.WRITE_CAP -= RAW_CAP
        try:
            self.capture = Capture()
            self.progress = Progress(self.records)
        except BaseException:
            self.close()
            raise

    @contextmanager
    def output_budget(self):
        before = (runtime.CAP, runtime.WRITE_CAP)
        other = runtime.output_bytes(self.root) - runtime.output_bytes(self.out)
        runtime.CAP = min(runtime.CAP, other + OUTPUT_CAP)
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
        if (OUTPUT_CAP - runtime.output_bytes(self.out) < needed
                or runtime.WRITE_CAP - runtime.output_bytes(self.root) < needed):
            self.summary_result = dict(status='NOT_REACHED_EVIDENCE_BUDGET')
            return
        with self.output_budget():
            try:
                observer = Summary(self.records)
                result = runtime.run([str(cli), 'generate', 'log-summary', '--format=predicates',
                                      '--minify-output', '--utc', '--verbosity=errors', self.capture.path],
                    cwd, self.out / 'official-summary.log', end, SUMMARY_SECONDS, self.root, env,
                    file_limit=None, observer=observer, restricted_output=True)
                self.summary_result = dict(status=observer.summary()['status'], exit=result['exit'],
                                           failure_kind=None, projection=observer.summary())
            except runtime.PhaseError as error:
                self.summary_result = dict(status='PARTIAL', exit=error.result['exit'],
                    failure_kind=error.result['failure_kind'], projection=error.result.get('output_observation'))
            except Exception:
                self.summary_result = dict(status='PARTIAL', failure_kind='SUMMARY_ADMISSION_ERROR')

    def receipt(self):
        capture = self.capture.summary() if self.capture else dict(status='NOT_REACHED')
        complete = (capture['status'] == 'CAPTURED' and capture['log_boundary_status'] == 'COMPLETE'
                    and self.summary_result['status'] == 'PROJECTED')
        return dict(version=VERSION, instrumented=True, status='COMPLETE' if complete else 'INCOMPLETE',
                    capture=capture, progress=self.progress.summary() if self.progress else None,
                    summary=self.summary_result, raw_memory_reserve_bytes=RAW_CAP,
                    observation_output_limit_bytes=OUTPUT_CAP, summary_limit_seconds=SUMMARY_SECONDS,
                    budget_scope='Raw memory reservation plus persisted private evidence and terminal reserve share 64MiB.',
                    limits='Observation completeness never certifies batch computation or oracle success.')

    def finish(self):
        receipt = self.receipt()
        with self.output_budget():
            runtime.save(self.out / 'observation-terminal.json', receipt, self.root, terminal=True)
        return dict(version=VERSION, status=receipt['status'], instrumented=True,
                    capture_status=receipt['capture']['status'], summary_status=self.summary_result['status'],
                    receipt='candidate-observation/observation-terminal.json',
                    receipt_sha256=runtime.digest(self.out / 'observation-terminal.json'))

    def close(self):
        try:
            if self.capture:
                self.capture.close()
        finally:
            if self.original_caps is not None:
                runtime.CAP, runtime.WRITE_CAP = self.original_caps
                self.original_caps = None
