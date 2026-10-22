"""Owned process execution and bounded evidence for the manual Rust model diagnostic."""
import hashlib
import json
import os
from pathlib import Path
import resource
import selectors
import signal
import subprocess
import time

CAP = 64 * 1024**2
WRITE_CAP = CAP - 1024**2  # reserve terminal and hash receipts
DATA_LIMIT = 1024**3
MAX_PROCESS_RECEIPT = 256 * 1024
MAX_CONTROL_RECEIPT = 32 * 1024
MAX_HASH_RECEIPT = 64 * 1024
IO_KEYS = {'rchar', 'wchar', 'syscr', 'syscw', 'read_bytes', 'write_bytes', 'cancelled_write_bytes'}


def output_bytes(root):
    return sum(p.stat().st_size for p in root.rglob('*') if p.is_file())


def evidence_write(path, data, root, terminal=False):
    previous = path.stat().st_size if path.is_file() else 0
    if output_bytes(root) - previous + len(data) > (CAP if terminal else WRITE_CAP):
        raise RuntimeError('combined evidence budget exceeded before write')
    path.write_bytes(data)


def save(path, value, root, terminal=False):
    evidence_write(path, (json.dumps(value, indent=2) + '\n').encode(), root, terminal)


def admit_terminal_snapshots(protected, official):
    snapshots = sum(len((json.dumps(value, indent=2)+'\n').encode()) for value in
                    (protected, {'unchanged': False, 'hashes': official}))
    needed = snapshots + MAX_PROCESS_RECEIPT + 2 * MAX_CONTROL_RECEIPT + MAX_HASH_RECEIPT
    if needed > CAP-WRITE_CAP:
        raise RuntimeError('frozen terminal footprint exceeds reserved evidence budget')
    return {'frozen_snapshot_bytes': snapshots, 'maximum_process_receipt_bytes': MAX_PROCESS_RECEIPT,
            'two_control_receipts_bytes': 2*MAX_CONTROL_RECEIPT, 'hash_index_bytes': MAX_HASH_RECEIPT,
            'required_bytes': needed, 'reserved_bytes': CAP-WRITE_CAP}


def digest(path):
    h = hashlib.sha256()
    with path.open('rb') as stream:
        for chunk in iter(lambda: stream.read(1024**2), b''):
            h.update(chunk)
    return h.hexdigest()


class GroupSamples:
    """Linux observed counters, not hard RSS/CPU or physical-I/O measurements."""
    def __init__(self, pgid, proc=Path('/proc')):
        self.pgid, self.proc = pgid, proc
        self.identities = {}
        self.peak_rss = 0
        self.samples = 0
        self.errors = 0
        self.skipped_identity_observations = 0
        self.supported = proc.is_dir()

    def sample(self):
        if not self.supported:
            return {'status': 'UNAVAILABLE'}
        rss = cpu = accepted = 0
        for folder in self.proc.iterdir():
            if not folder.name.isdigit():
                continue
            try:
                text = (folder / 'stat').read_text()
                fields = text[text.rindex(')') + 2:].split()
                if int(fields[2]) != self.pgid:  # field5: process group
                    continue
                pid, start = int(folder.name), int(fields[19])  # field22
                ticks = int(fields[11]) + int(fields[12])  # own utime+stime, not children
                resident = max(0, int(fields[21])) * os.sysconf('SC_PAGE_SIZE')
                counters = {}
                io_valid = False
                try:
                    counters = {key: int(value) for key, value in
                                (line.split(':', 1) for line in (folder / 'io').read_text().splitlines())}
                    if set(counters) <= IO_KEYS and counters and all(0 <= x <= 2**64-1 for x in counters.values()):
                        io_valid = True
                    else:
                        counters = {}; self.errors += 1
                except (OSError, ValueError):
                    self.errors += 1
                again = (folder / 'stat').read_text()
                fresh = again[again.rindex(')')+2:].split()
                if int(fresh[2]) != self.pgid or int(fresh[19]) != start:
                    self.errors += 1
                    continue  # discard stat+io tuple if identity changed across either read
                key = f'{pid}:{start}:{self.pgid}'
                rss += resident
                cpu += ticks
                accepted += 1
                if key not in self.identities and len(self.identities) >= 256:
                    self.skipped_identity_observations += 1
                    continue  # diagnostic saturation is not an extraction safety failure
                old = self.identities.setdefault(key, {'pid': pid, 'start_ticks': start,
                    'pgid': self.pgid, 'cpu_ticks_observed_max': 0, 'io_observed_max': {}})
                old['io_validity'] = 'OBSERVED' if io_valid or old.get('io_validity') == 'OBSERVED' else 'UNKNOWN'
                old['cpu_ticks_observed_max'] = max(old['cpu_ticks_observed_max'], ticks)
                for name, value in counters.items():
                    old['io_observed_max'][name] = max(old['io_observed_max'].get(name, 0), value)
            except (OSError, ValueError, IndexError):
                # Exiting processes are normal; record observation limitations.
                self.errors += 1
        self.samples += 1
        self.peak_rss = max(self.peak_rss, rss)
        observed = bool(accepted)
        return {'sample': self.samples, 'monotonic_time': time.monotonic(),
                'counter_validity': 'OBSERVED' if observed else 'UNKNOWN_NO_ACCEPTED_LIVE_IDENTITY',
                'group_rss_bytes': rss if observed else None, 'live_own_cpu_ticks': cpu if observed else None}

    def result(self):
        return {'status': 'OBSERVED' if self.identities else 'UNAVAILABLE_NO_IDENTITIES',
            'sample_period_seconds': .2, 'samples': self.samples, 'read_errors': self.errors,
            'clock_ticks_per_second': os.sysconf('SC_CLK_TCK'),
            'counter_validity': 'INCOMPLETE_IDENTITY_SATURATION' if self.skipped_identity_observations else ('OBSERVED_PARTIAL_SAMPLING' if self.identities else 'UNKNOWN'),
            'skipped_identity_observations': self.skipped_identity_observations,
            'identity_history_limit': 256,
            'observed_tracked_identity_cpu_seconds': sum(p['cpu_ticks_observed_max'] for p in self.identities.values()) / os.sysconf('SC_CLK_TCK') if self.identities else None,
            'observed_group_own_cpu_seconds': sum(p['cpu_ticks_observed_max'] for p in self.identities.values()) / os.sysconf('SC_CLK_TCK') if self.identities and not self.skipped_identity_observations else None,
            'observed_peak_group_rss_bytes': self.peak_rss if self.identities else None,
            'processes': list(self.identities.values()),
            'limits': 'Saturation omits untracked identity cumulative CPU/I/O; skipped count is observations, not unique processes. Tracked CPU is a subset, never a complete total. Sampled PGID+start-tick identities; may miss short-lived/final work. '
                      'RSS is sampled aggregate, not true peak/hard limit. I/O counters are '
                      'per-process observations, not additive physical device traffic.'}


class PhaseError(RuntimeError):
    def __init__(self, result):
        self.result = result
        super().__init__(repr(result))

    @property
    def clean_timeout(self):
        return self.result['failure_kind'] == 'timeout' and not self.result['cleanup_errors'] and not self.result.get('evidence_errors')


def run(argv, cwd, output, end, timeout, root, env, file_limit=CAP, evaluator=None, safety_check=None):
    """Separate stdout/stderr/evaluator FIFO; all evidence writes pass the shared cap."""
    start = time.monotonic()
    deadline = min(end, start + timeout)
    result = {'argv': argv, 'cwd': str(cwd), 'started_wall': time.time(), 'pid': None,
              'exit': None, 'error': None, 'failure_kind': None, 'cleanup_errors': [], 'evidence_errors': [],
              'file_size_limit_bytes': file_limit}
    if not 0 < file_limit <= DATA_LIMIT:
        raise ValueError('invalid process data file bound')
    selector = selectors.DefaultSelector()
    proc = fifo_fd = None
    fifo_created = False
    files = []
    sample = None
    streams = {}
    try:
        if safety_check is not None:
            safety_check()
        if deadline <= start:
            raise TimeoutError('diagnostic phase deadline exhausted before spawn')
        if len(json.dumps(argv).encode()) > 16*1024:
            result['argv'] = {'rejected_count': len(argv), 'sha256': hashlib.sha256(json.dumps(argv).encode()).hexdigest()}
            raise RuntimeError('argv admission rejected before spawn')
        if output_bytes(root) >= WRITE_CAP:
            raise RuntimeError('evidence/argv admission rejected before spawn')
        if evaluator:
            fifo = evaluator.with_suffix('.fifo')
            os.mkfifo(fifo, 0o600)
            fifo_created = True
            fifo_fd = os.open(fifo, os.O_RDWR | os.O_NONBLOCK)
            argv = [str(fifo) if x == '{EVALUATOR_FIFO}' else x for x in argv]
            result['argv'] = argv
            streams[fifo_fd] = evaluator.open('xb')
            files.append(streams[fifo_fd])
            selector.register(fifo_fd, selectors.EVENT_READ)
        def limits():
            resource.setrlimit(resource.RLIMIT_FSIZE, (file_limit, file_limit))
        proc = subprocess.Popen(argv, cwd=cwd, env=env, stdout=subprocess.PIPE,
                                stderr=subprocess.PIPE, start_new_session=True, preexec_fn=limits)
        result['pid'] = result['pgid'] = proc.pid
        for pipe, path in ((proc.stdout, output), (proc.stderr, output.with_name(output.name + '.stderr'))):
            os.set_blocking(pipe.fileno(), False)
            streams[pipe.fileno()] = path.open('xb')
            files.append(streams[pipe.fileno()])
            selector.register(pipe.fileno(), selectors.EVENT_READ)
        sample = GroupSamples(proc.pid)
        samples = output.with_name(output.name + '.samples.jsonl').open('xb')
        files.append(samples)
        last_sample = 0
        last_safety = 0
        while True:
            if safety_check is not None and time.monotonic() - last_safety >= 2:
                safety_check()
                last_safety = time.monotonic()
            if time.monotonic() >= deadline:
                raise TimeoutError('diagnostic phase deadline exhausted')
            if time.monotonic() - last_sample >= .2:
                data = (json.dumps(sample.sample(), separators=(',', ':')) + '\n').encode()
                if output_bytes(root) + len(data) > WRITE_CAP:
                    raise RuntimeError('combined evidence budget exceeded before write')
                samples.write(data); samples.flush()
                last_sample = time.monotonic()
            events = selector.select(.05)
            for key, _ in events:
                chunk = os.read(key.fd, 65536)
                if not chunk:
                    selector.unregister(key.fd)
                else:
                    if output_bytes(root) + len(chunk) > WRITE_CAP:
                        raise RuntimeError('combined evidence budget exceeded before write')
                    streams[key.fd].write(chunk); streams[key.fd].flush()
            if proc.poll() is not None and not events:
                # FIFO is parent-held RDWR: one empty drain after child exit ends it.
                break
        result['exit'] = proc.wait()
        if result['exit'] != 0:
            result['failure_kind'] = 'child_exit'
            raise RuntimeError('child nonzero exit')
    except BaseException as exc:
        result['error'] = repr(exc)[:8192]
        result['failure_kind'] = result['failure_kind'] or ('timeout' if isinstance(exc, TimeoutError) else 'unsafe')
    finally:
        # Every cleanup step is independent; no close exception may hide owned reaping.
        if proc:
            for sig, wait in ((signal.SIGTERM, 1), (signal.SIGKILL, 2)):
                try:
                    os.killpg(proc.pid, sig)
                except ProcessLookupError:
                    pass
                except BaseException as exc:
                    result['cleanup_errors'].append(repr(exc))
                try:
                    proc.wait(timeout=wait)
                except subprocess.TimeoutExpired:
                    pass
                except BaseException as exc:
                    result['cleanup_errors'].append(repr(exc))
            result['exit'] = proc.returncode
            try:
                os.killpg(proc.pid, 0)
            except ProcessLookupError:
                pass
            except BaseException as exc:
                result['cleanup_errors'].append(repr(exc))
            else:
                result['cleanup_errors'].append('owned process group still exists')
            # TERM handlers can emit final events. Preserve pending bytes after reaping;
            # never close the read ends first, and never bypass the combined cap.
            try:
                for _ in range(16):
                    ready = selector.select(0)
                    if not ready:
                        break
                    for key, _ in ready:
                        chunk = os.read(key.fd, 65536)
                        if not chunk:
                            selector.unregister(key.fd)
                        else:
                            if output_bytes(root) + len(chunk) > WRITE_CAP:
                                raise RuntimeError('combined evidence budget exceeded during final drain')
                            streams[key.fd].write(chunk); streams[key.fd].flush()
                else:
                    raise RuntimeError('bounded final drain incomplete')
            except BaseException as exc:
                result['evidence_errors'].append(repr(exc)[:1024])
                result['failure_kind'] = 'unsafe'
                result['error'] = result['error'] or repr(exc)[:8192]
            for pipe in (proc.stdout, proc.stderr):
                try:
                    pipe.close()
                except BaseException as exc:
                    result['cleanup_errors'].append(repr(exc))
        for file in files:
            try:
                file.close()
            except BaseException as exc:
                result['cleanup_errors'].append(repr(exc))
        for closer in (selector.close, (lambda: os.close(fifo_fd)) if fifo_fd is not None else lambda: None):
            try:
                closer()
            except BaseException as exc:
                result['cleanup_errors'].append(repr(exc))
        if fifo_created:
            try:
                fifo.unlink()
            except FileNotFoundError:
                pass
            except BaseException as exc:
                result['cleanup_errors'].append(repr(exc))
        result['elapsed'] = time.monotonic() - start
        result['cleanup_errors'] = [str(error)[:1024] for error in result['cleanup_errors']]
        if sample:
            result['resource_observation'] = sample.result()
        if len((json.dumps(result,indent=2)+'\n').encode()) > MAX_PROCESS_RECEIPT:
            result['resource_observation'] = {'status': 'UNKNOWN_RECEIPT_LIMIT'}
            result['error'] = result['error'] or 'maximum process receipt exceeded'
            result['failure_kind'] = 'unsafe'
        save(output.with_name(output.name + '.result.json'), result, root, terminal=True)
    if result['error'] or result['cleanup_errors']:
        raise PhaseError(result)
    return result


def stage_observation(log):
    """Version-labelled observation only; official summaries remain primary."""
    starts, completed, seen_starts, unknown, header, footer = {}, [], set(), 0, 0, 0
    for line in log.read_text().splitlines():
        try:
            value = json.loads(line)
            if value.get('type') == 'LOG_HEADER':
                header += 1
            elif value.get('type') == 'LOG_FOOTER':
                footer += 1
            elif value.get('type') == 'PREDICATE_STARTED':
                appearances = value.get('appearsAs')
                if not isinstance(value.get('eventId'), int) or not isinstance(value.get('nanoTime'), int) or not isinstance(value.get('raHash'), str) or not isinstance(value.get('predicateName'), str) or not isinstance(appearances, dict) or not all(isinstance(stages, list) and all(isinstance(stage, int) for stage in stages) for stages in appearances.values()):
                    raise ValueError('unknown predicate event shape')
                if value['eventId'] in seen_starts:
                    raise ValueError('duplicate predicate start identity')
                seen_starts.add(value['eventId'])
                starts[value['eventId']] = value
            elif value.get('type') == 'PREDICATE_COMPLETED':
                begin = starts.pop(value['startEvent'])
                completed.append({'event_id': begin['eventId'], 'ra_hash': begin['raHash'],
                    'predicate': begin['predicateName'], 'appears_as': begin.get('appearsAs'),
                    'elapsed_ns': value['nanoTime'] - begin['nanoTime'], 'result_size': value.get('resultSize')})
        except (ValueError, KeyError, TypeError, AttributeError):
            unknown += 1
    shared = [event['event_id'] for event in completed if
              len({stage for stages in (event['appears_as'] or {}).values() for stage in stages}) > 1]
    return {'status': 'UNKNOWN_SCHEMA' if unknown or header != 1 else
                     'COMPLETE_OBSERVATION_ONLY' if footer == 1 else 'PARTIAL_OBSERVATION_ONLY',
        'header_count': header, 'footer_count': footer, 'unknown_records': unknown,
        'invocation_sha256': digest(log), 'completed_once_per_event': completed,
        'shared_event_ids_counted_once': shared,
        'unfinished': [{key: value.get(key) for key in
                       ('eventId', 'predicateName', 'raHash', 'appearsAs', 'nanoTime')}
                      for value in starts.values()],
        'limits': 'Each event counted once despite multiple appearsAs. Separate invocation costs are '
                  'not deduplicated away. Overlapping elapsed intervals are not additive CPU.'}


def official_summaries(cli, evaluator, work, out, end, env, original_error=None):
    """Only known-clean evaluation success/timeout permits more CLI processes."""
    if original_error is not None and not original_error.clean_timeout:
        return {'status': 'NOT_RUN_UNSAFE_FAILURE'}
    results = {}
    for fmt in ('overall', 'predicates'):
        if end - time.monotonic() < 5:
            results[fmt] = 'NOT_REACHED_FIXED_DEADLINE'; break
        try:
            run([cli, 'generate', 'log-summary', '--format=' + fmt, '--minify-output', str(evaluator)],
                work, out / (evaluator.stem + '-summary-' + fmt + '.jsonl'), end, 20, out, env)
            results[fmt] = 'GENERATED_NOT_EVALUATION_COMPLETION'
        except PhaseError as exc:
            results[fmt] = {'status': 'UNKNOWN', 'error': str(exc)}
            break
    return results
