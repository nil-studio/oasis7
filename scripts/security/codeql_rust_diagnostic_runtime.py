"""Owned process execution and bounded evidence for the manual Rust model diagnostic."""
import hashlib
from contextlib import contextmanager
from itertools import islice
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
CLEANUP_SIGNALS = ((signal.SIGTERM, 1), (signal.SIGKILL, 2))
CLEANUP_SECONDS = sum(wait for _, wait in CLEANUP_SIGNALS)


def output_bytes(root):
    total = 0
    stack = [os.fspath(root)]
    while stack:
        current = stack.pop()
        try:
            scandir_it = os.scandir(current)
        except OSError:
            continue
        with scandir_it as entries:
            for entry in entries:
                try:
                    if entry.is_dir(follow_symlinks=False):
                        stack.append(entry.path)
                    elif entry.is_file(follow_symlinks=True):
                        total += entry.stat(follow_symlinks=True).st_size
                except OSError:
                    continue
    return total


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


class SamplingLimit(Exception):
    """A fixed observation limit, never a reason to stop the owned workload."""


class ResourceTimeline:
    """Optional, bounded Linux counters; no process names, command lines or raw proc text."""
    PERIOD = 15
    MAX_SAMPLES = 320
    MAX_RECORD = 12 * 1024
    MAX_BYTES = 4 * 1024**2
    MAX_PROCESSES = 8
    MAX_THREADS = 64
    MAX_PROC_ENTRIES = 1024
    MAX_THREAD_ENTRIES = 128
    MAX_READ = 4096
    SAMPLE_SECONDS = .05

    def __init__(self, pgid, proc=Path('/proc')):
        self.pgid, self.proc = pgid, proc
        self.next_sample = 0
        self.attempts = self.records = self.bytes = self.read_errors = 0
        self.observed_process_samples = self.counter_regressions = 0
        self.pending_process_samples = 0
        self.previous, self.previous_pressure = {}, {}
        self.previous_time = None
        self.stopped = None
        self.limit_counts = {}

    @staticmethod
    def unsigned(value):
        if not value.isascii() or not value.isdecimal() or len(value) > 20:
            raise ValueError('invalid counter')
        result = int(value)
        if result > 2**64-1:
            raise ValueError('counter overflow')
        return result

    def read(self, path, end):
        if time.monotonic() >= end:
            raise SamplingLimit('TIME_LIMIT')
        with path.open('rb') as stream:
            value = stream.read(self.MAX_READ + 1)
        if len(value) > self.MAX_READ:
            raise ValueError('proc input limit')
        return value.decode('ascii')

    def stat(self, folder, pid, end):
        value = self.read(folder / 'stat', end)
        if self.unsigned(value.split(' (', 1)[0]) != pid:
            raise ValueError('stat PID differs')
        fields = value[value.rindex(')') + 2:].split()
        # /proc/PID/stat fields 5, 10, 12, 14, 15 and 22; exclude child counters.
        return {name: self.unsigned(fields[index]) for name, index in
                (('pgid', 2), ('minor_faults', 7), ('major_faults', 9),
                 ('utime', 11), ('stime', 12), ('start_ticks', 19))}

    def io(self, folder, end):
        result = {}
        for line in self.read(folder / 'io', end).splitlines():
            name, value = line.split(':', 1)
            if name not in IO_KEYS or name in result:
                raise ValueError('unknown or duplicate I/O counter')
            result[name] = self.unsigned(value.strip())
        if set(result) != IO_KEYS:
            raise ValueError('incomplete I/O counters')
        return result

    def pressure(self, name, end):
        result = {}
        for line in self.read(self.proc / 'pressure' / name, end).splitlines():
            fields = line.split()
            if not fields or fields[0] not in ('some', 'full') or fields[0] in result:
                raise ValueError('unknown or duplicate pressure counter')
            values = {}
            for field in fields[1:]:
                key, value = field.split('=', 1)
                if key in values:
                    raise ValueError('duplicate pressure field')
                values[key] = value
            if set(values) != {'avg10', 'avg60', 'avg300', 'total'}:
                raise ValueError('incomplete pressure counters')
            result[fields[0]] = self.unsigned(values['total'])
        wanted = {'some'} if name == 'cpu' else {'some', 'full'}
        if not wanted <= result.keys():
            raise ValueError('missing pressure counter')
        # System CPU full is undefined; do not interpret its compatibility zero.
        return {key: result[key] for key in sorted(wanted)}

    def delta(self, value, old):
        if old is None:
            return None
        if value < old:
            self.counter_regressions += 1
            return None
        return value - old

    def stop(self, reason):
        self.stopped = self.stopped or reason
        self.previous, self.previous_pressure = {}, {}
        self.pending_process_samples = 0

    def collect(self, end):
        processes, current, pressure, limits = [], {}, {}, set()
        process_entries = thread_entries = thread_reads = selected = 0
        errors_before, regressions_before = self.read_errors, self.counter_regressions
        try:
            # scandir is lazy: the directory cap also bounds enumeration memory.
            with os.scandir(self.proc) as entries:
                for entry in islice(entries, self.MAX_PROC_ENTRIES):
                    if time.monotonic() >= end:
                        raise SamplingLimit('TIME_LIMIT')
                    process_entries += 1
                    if not entry.name.isascii() or not entry.name.isdecimal():
                        continue
                    folder, pid = Path(entry.path), self.unsigned(entry.name)
                    try:
                        before = self.stat(folder, pid, end)
                        if before['pgid'] != self.pgid:
                            continue
                        if selected >= self.MAX_PROCESSES:
                            raise SamplingLimit('PROCESS_LIMIT')
                        selected += 1
                        key = (pid, before['start_ticks'])
                        old = self.previous.get(key, {})
                        counters, threads = None, {}
                        try:
                            counters = self.io(folder, end)
                        except (OSError, ValueError, IndexError):
                            self.read_errors += 1
                        threads_complete = True
                        try:
                            with os.scandir(folder / 'task') as tasks:
                                for task in islice(tasks, max(0, self.MAX_THREAD_ENTRIES-thread_entries)):
                                    if time.monotonic() >= end:
                                        raise SamplingLimit('TIME_LIMIT')
                                    thread_entries += 1
                                    if thread_reads >= self.MAX_THREADS:
                                        threads_complete = False
                                        limits.add('THREAD_LIMIT')
                                        break
                                    if not task.name.isascii() or not task.name.isdecimal():
                                        continue
                                    thread_reads += 1
                                    tid = self.unsigned(task.name)
                                    try:
                                        thread = self.stat(Path(task.path), tid, end)
                                        if thread['pgid'] != self.pgid:
                                            threads_complete = False
                                            limits.add('THREAD_IDENTITY_CHANGED')
                                            continue
                                        threads[(tid, thread['start_ticks'])] = thread['utime'] + thread['stime']
                                    except (OSError, ValueError, IndexError):
                                        threads_complete = False
                                        self.read_errors += 1
                                if thread_entries >= self.MAX_THREAD_ENTRIES:
                                    threads_complete = False
                                    limits.add('THREAD_LIMIT')
                        except OSError:
                            threads_complete = False
                            self.read_errors += 1
                        after = self.stat(folder, pid, end)
                        if (after['pgid'], after['start_ticks']) != (self.pgid, before['start_ticks']):
                            limits.add('PROCESS_IDENTITY_CHANGED')
                            continue  # Discard the entire stat/I/O/thread combination.
                        cpu = before['utime'] + before['stime']
                        faults = {name: before[name] for name in ('minor_faults', 'major_faults')}
                        processes.append(dict(pid=pid, start_ticks=before['start_ticks'], cpu_ticks=cpu,
                            cpu_delta_ticks=self.delta(cpu, old.get('cpu')), faults=faults,
                            fault_deltas={name: self.delta(value, old.get('faults', {}).get(name))
                                          for name, value in faults.items()},
                            io=counters, io_deltas={name: self.delta(value, (old.get('io') or {}).get(name))
                                                   for name, value in counters.items()} if counters is not None else None,
                            threads=[[tid, start, value, self.delta(value, old.get('threads', {}).get((tid, start)))]
                                     for (tid, start), value in sorted(threads.items())],
                            threads_complete=threads_complete))
                        current[key] = dict(cpu=cpu, faults=faults, io=counters, threads=threads)
                    except (OSError, ValueError, IndexError):
                        self.read_errors += 1
        except SamplingLimit as exc:
            limits.add(str(exc))
        except OSError:
            self.read_errors += 1
        if process_entries >= self.MAX_PROC_ENTRIES:
            limits.add('PROCESS_DIRECTORY_LIMIT')
        for name in ('cpu', 'memory', 'io'):
            try:
                values = self.pressure(name, end)
                pressure[name] = dict(total_us=values, delta_us={key: self.delta(
                    value, self.previous_pressure.get(name, {}).get(key)) for key, value in values.items()})
            except SamplingLimit as exc:
                limits.add(str(exc))
                break
            except (OSError, ValueError, IndexError):
                self.read_errors += 1
        self.previous = current
        self.previous_pressure = {name: value['total_us'] for name, value in pressure.items()}
        for reason in limits:
            self.limit_counts[reason] = self.limit_counts.get(reason, 0) + 1
        return dict(pgid=self.pgid, processes=processes, system_pressure=pressure,
                    pressure_scope='system', limits=sorted(limits),
                    read_errors=self.read_errors-errors_before,
                    counter_regressions=self.counter_regressions-regressions_before)

    def poll(self, end, available):
        now = time.monotonic()
        if self.stopped or now < self.next_sample:
            return None
        if now >= end:
            self.stop('FIXED_DEADLINE')
            return None
        if self.attempts >= self.MAX_SAMPLES:
            self.stop('SAMPLE_LIMIT')
            return None
        if available <= 0 or self.bytes >= self.MAX_BYTES:
            self.stop('EVIDENCE_LIMIT')
            return None
        self.attempts += 1
        self.next_sample = now + self.PERIOD
        try:
            wall = time.time()
            value = self.collect(min(end, now + self.SAMPLE_SECONDS))
            value.update(schema='linux-resource-timeline/v1', sample=self.attempts,
                         started_wall=wall, started_monotonic=now, finished_monotonic=time.monotonic(),
                         interval_seconds=now-self.previous_time if self.previous_time is not None else None)
            value['status'] = ('UNKNOWN_NO_ACCEPTED_IDENTITY' if not value['processes'] else
                               'PARTIAL' if value['read_errors'] or value['limits'] or value['counter_regressions'] else 'OBSERVED_PARTIAL_SAMPLING')
            data = (json.dumps(value, separators=(',', ':')) + '\n').encode()
            if len(data) > self.MAX_RECORD:
                self.stop('RECORD_LIMIT')
                return None
            if self.bytes + len(data) > self.MAX_BYTES or len(data) > available:
                self.stop('EVIDENCE_LIMIT')
                return None
            if time.monotonic() >= end:
                self.stop('FIXED_DEADLINE')
                return None
            self.previous_time = now
            self.pending_process_samples = len(value['processes'])
            return data
        except Exception:
            # Extra diagnostics cannot replace cancellation, timeout or the primary failure.
            self.stop('SAMPLING_ERROR')
            return None

    def written(self, data):
        self.records += 1
        self.bytes += len(data)
        self.observed_process_samples += self.pending_process_samples
        self.pending_process_samples = 0

    def result(self):
        return dict(schema='linux-resource-timeline/v1', instrumented=True,
                    status='OBSERVED_PARTIAL_SAMPLING' if self.observed_process_samples and self.records else 'UNAVAILABLE',
                    stopped=self.stopped, attempts=self.attempts, records=self.records, bytes=self.bytes,
                    observed_process_samples=self.observed_process_samples,
                    read_errors=self.read_errors, limit_counts=self.limit_counts,
                    counter_regressions=self.counter_regressions, sample_period_seconds=self.PERIOD,
                    clock_ticks_per_second=os.sysconf('SC_CLK_TCK'),
                    thread_columns=['tid', 'start_ticks', 'cpu_ticks', 'cpu_delta_ticks'],
                    bounds=dict(samples=self.MAX_SAMPLES, record_bytes=self.MAX_RECORD, total_bytes=self.MAX_BYTES,
                                processes=self.MAX_PROCESSES, threads=self.MAX_THREADS,
                                process_directory_entries=self.MAX_PROC_ENTRIES,
                                thread_directory_entries=self.MAX_THREAD_ENTRIES,
                                accepted_read_bytes=self.MAX_READ, read_probe_bytes=1,
                                sample_soft_seconds=self.SAMPLE_SECONDS),
                    limits='Same PGID/PID/start and parent-bound TID/start snapshots; missing/first/decreasing deltas are null. '
                           'May miss short-lived/final work. Per-process I/O includes waited-for children; do not add as device traffic. '
                           'PSI is system-wide, not attributable workload stalls. Time checks are soft, not syscall/RSS limits.')


class PhaseError(RuntimeError):
    def __init__(self, result):
        self.result = result
        super().__init__(repr(result))

    @property
    def clean_timeout(self):
        return self.result['failure_kind'] == 'timeout' and not self.result['cleanup_errors'] and not self.result.get('evidence_errors')


class CancelledError(RuntimeError):
    pass


@contextmanager
def cancellation_guard():
    """Opt-in polling cancellation; signals never interrupt resource registration."""
    current = signal.getsignal(signal.SIGTERM)
    if getattr(current, '_codeql_cancellation', None) is not None:
        yield current._codeql_cancellation
        return
    state = dict(cancelled=False)
    previous = {sig: signal.getsignal(sig) for sig in (signal.SIGTERM, signal.SIGINT)}
    def cancel(signum, frame):
        state['cancelled'] = True
    cancel._codeql_cancellation = state
    try:
        for sig in previous:
            signal.signal(sig, cancel)
        yield state
    finally:
        for sig, handler in previous.items():
            signal.signal(sig, handler)


def run(argv, cwd, output, end, timeout, root, env, file_limit=CAP, evaluator=None, observer=None, *, inherit_stdio=False, reap_orphans=False,
        evaluator_capture=None, restricted_output=False, resource_details=False):
    options = dict(file_limit=file_limit, evaluator=evaluator, observer=observer,
                   inherit_stdio=inherit_stdio, reap_orphans=reap_orphans,
                   evaluator_capture=evaluator_capture, restricted_output=restricted_output,
                   resource_details=resource_details)
    active_guard = getattr(signal.getsignal(signal.SIGTERM), '_codeql_cancellation', None)
    if restricted_output or evaluator_capture is not None or active_guard is not None:
        with cancellation_guard() as state:
            return _run(argv, cwd, output, end, timeout, root, env, _cancellation=state, **options)
    return _run(argv, cwd, output, end, timeout, root, env, **options)


def _run(argv, cwd, output, end, timeout, root, env, file_limit=CAP, evaluator=None, observer=None, *, inherit_stdio=False, reap_orphans=False,
         evaluator_capture=None, restricted_output=False, resource_details=False, _cancellation=None):
    """Separate stdout/stderr/evaluator FIFO; all evidence writes pass the shared cap."""
    start = time.monotonic()
    deadline = min(end, start + timeout)
    restricted_output = restricted_output or evaluator_capture is not None
    # The opt-in private observation owns its cleanup/drain inside this same end.
    payload_end = deadline - CLEANUP_SECONDS - .75 if restricted_output else deadline
    def error_text(exc):
        if not restricted_output:
            return repr(exc)
        # Parser exceptions can contain an entire raw line, including bad UTF-8.
        return next((name for kind, name in ((CancelledError, 'CANCELLED'), (TimeoutError, 'TIMEOUT'), (OSError, 'OS_ERROR'),
                    (ValueError, 'INVALID_DATA'), (KeyboardInterrupt, 'INTERRUPTED'),
                    (SystemExit, 'CANCELLED')) if isinstance(exc, kind)), 'OBSERVATION_ERROR')
    result = {'argv': argv, 'cwd': str(cwd), 'started_wall': time.time(), 'pid': None,
              'exit': None, 'error': None, 'failure_kind': None, 'cleanup_errors': [], 'evidence_errors': [],
              'file_size_limit_bytes': file_limit}
    if file_limit is not None and not 0 < file_limit <= DATA_LIMIT:
        raise ValueError('invalid process data file bound')
    if evaluator_capture is not None and (evaluator is None or observer is None or inherit_stdio):
        raise ValueError('private evaluator capture requires FIFO and projected streams')
    if observer is not None and evaluator is not None and evaluator_capture is None:
        raise ValueError('stream observation does not support evaluator FIFO')
    if inherit_stdio and evaluator is not None:
        raise ValueError('inherited Action streams cannot use an evaluator FIFO')
    if inherit_stdio:
        result['stdio'] = 'inherited runner channels; raw Action output is not persisted here'
    selector = selectors.DefaultSelector()
    proc = fifo_fd = fifo_anchor = None
    fifo_eof = False
    fifo_created = False
    files = []
    sample = detail = None
    streams = {}
    stream_names = {}
    finalized_streams = set()
    def write_chunk(fd, chunk, final=False):
        # Projection precedes persistence, including EOF and TERM-handler drain.
        # An observer error never falls back to raw bytes.
        if fd == fifo_fd and evaluator_capture is not None:
            if chunk:
                evaluator_capture.feed(chunk)
            return
        if final and restricted_output and fd in finalized_streams:
            return
        data = observer.feed(stream_names[fd], chunk, final=final) if observer else chunk
        if final:
            finalized_streams.add(fd)
        if output_bytes(root) + len(data) > WRITE_CAP:
            raise RuntimeError('combined evidence budget exceeded before write')
        streams[fd].write(data); streams[fd].flush()
    try:
        if _cancellation and _cancellation['cancelled']:
            raise CancelledError('CANCELLED_BEFORE_SPAWN')
        if payload_end <= start:
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
            if evaluator_capture is None:
                fifo_fd = os.open(fifo, os.O_RDWR | os.O_NONBLOCK)
            else:
                fifo_fd = os.open(fifo, os.O_RDONLY | os.O_NONBLOCK)
                fifo_anchor = os.open(fifo, os.O_WRONLY | os.O_NONBLOCK)
            argv = [str(fifo) if x == '{EVALUATOR_FIFO}' else x for x in argv]
            result['argv'] = argv
            if evaluator_capture is None:
                streams[fifo_fd] = evaluator.open('xb')
                files.append(streams[fifo_fd])
            selector.register(fifo_fd, selectors.EVENT_READ)
        def limits():
            if file_limit is not None:
                resource.setrlimit(resource.RLIMIT_FSIZE, (file_limit, file_limit))
        proc = subprocess.Popen(argv, cwd=cwd, env=env, stdout=None if inherit_stdio else subprocess.PIPE,
                                stderr=None if inherit_stdio else subprocess.PIPE, start_new_session=True, preexec_fn=limits)
        result['pid'] = result['pgid'] = proc.pid
        for pipe, path in ((proc.stdout, output), (proc.stderr, output.with_name(output.name + '.stderr'))):
            if pipe is None:
                continue
            os.set_blocking(pipe.fileno(), False)
            streams[pipe.fileno()] = path.open('xb')
            stream_names[pipe.fileno()] = 'stdout' if pipe is proc.stdout else 'stderr'
            files.append(streams[pipe.fileno()])
            selector.register(pipe.fileno(), selectors.EVENT_READ)
        sample = GroupSamples(proc.pid)
        samples = output.with_name(output.name + '.samples.jsonl').open('xb')
        files.append(samples)
        if resource_details:
            detail = ResourceTimeline(proc.pid)
            detail_file = output.with_name(output.name + '.resources.jsonl').open('xb')
            files.append(detail_file)
        last_sample = 0
        while True:
            if _cancellation and _cancellation['cancelled']:
                raise CancelledError('CANCELLED')
            if time.monotonic() >= payload_end:
                raise TimeoutError('diagnostic phase deadline exhausted')
            if observer:
                observer.poll(payload_end)
            if time.monotonic() - last_sample >= .2:
                data = (json.dumps(sample.sample(), separators=(',', ':')) + '\n').encode()
                if output_bytes(root) + len(data) > WRITE_CAP:
                    raise RuntimeError('combined evidence budget exceeded before write')
                samples.write(data); samples.flush()
                if detail:
                    data = detail.poll(payload_end, WRITE_CAP-output_bytes(root))
                    if data is not None:
                        detail_file.write(data); detail_file.flush()
                        detail.written(data)
                last_sample = time.monotonic()
            events = selector.select(0 if proc.poll() is not None else .05)
            for key, _ in events:
                chunk = os.read(key.fd, 65536)
                if not chunk:
                    if key.fd == fifo_fd:
                        fifo_eof = True
                    if observer:
                        write_chunk(key.fd, b'', final=True)
                    selector.unregister(key.fd)
                else:
                    write_chunk(key.fd, chunk)
            if proc.poll() is not None and not events:
                # FIFO is parent-held RDWR: one empty drain after child exit ends it.
                break
        result['exit'] = proc.wait()
        if result['exit'] != 0:
            result['failure_kind'] = 'child_exit'
            raise RuntimeError('child nonzero exit')
    except BaseException as exc:
        result['error'] = error_text(exc)[:8192]
        result['failure_kind'] = result['failure_kind'] or ('cancelled' if isinstance(exc, CancelledError) else 'timeout' if isinstance(exc, TimeoutError) else 'unsafe')
    finally:
        if restricted_output:
            result['execution_failure_kind'] = result['failure_kind']
        # Every cleanup step is independent; no close exception may hide owned reaping.
        if proc:
            for sig, wait in CLEANUP_SIGNALS:
                try:
                    os.killpg(proc.pid, sig)
                except ProcessLookupError:
                    pass
                except BaseException as exc:
                    result['cleanup_errors'].append(error_text(exc))
                try:
                    if reap_orphans or restricted_output:
                        # The Action supervisor is a Linux subreaper. Reap only this owned PGID;
                        # normal diagnostic callers retain their original cleanup behavior.
                        until = min(deadline - .75, time.monotonic() + wait) if restricted_output else time.monotonic() + wait
                        while True:
                            proc.poll()
                            if reap_orphans:
                                try:
                                    with proc._waitpid_lock:
                                        while True:
                                            waited_pid, status = os.waitpid(-proc.pid, os.WNOHANG)
                                            if waited_pid <= 0:
                                                break
                                            if waited_pid == proc.pid:
                                                # A leader may exit after poll(). Preserve its
                                                # real status before a later poll sees ECHILD.
                                                proc.returncode = os.waitstatus_to_exitcode(status)
                                except ChildProcessError:
                                    pass
                            try:
                                os.killpg(proc.pid, 0)
                            except ProcessLookupError:
                                break
                            if time.monotonic() >= until:
                                break
                            time.sleep(min(.02, until - time.monotonic()))
                    else:
                        proc.wait(timeout=wait)
                except subprocess.TimeoutExpired:
                    pass
                except BaseException as exc:
                    result['cleanup_errors'].append(error_text(exc))
            result['exit'] = proc.returncode
            try:
                os.killpg(proc.pid, 0)
            except ProcessLookupError:
                pass
            except BaseException as exc:
                result['cleanup_errors'].append(error_text(exc))
            else:
                result['cleanup_errors'].append('owned process group still exists')
            if fifo_anchor is not None:
                try:
                    os.close(fifo_anchor)
                except BaseException as exc:
                    result['cleanup_errors'].append(error_text(exc))
                finally:
                    fifo_anchor = None
            # TERM handlers can emit final events. Preserve pending bytes after reaping;
            # never close the read ends first, and never bypass the combined cap.
            try:
                drain_end = min(deadline - .25, time.monotonic() + .5)
                for _ in range(256 if restricted_output else 16):
                    if restricted_output and (not selector.get_map() or time.monotonic() >= drain_end):
                        break
                    ready = selector.select(min(.01, max(0, drain_end-time.monotonic())) if restricted_output else 0)
                    if not ready:
                        if restricted_output:
                            continue
                        break
                    for key, _ in ready:
                        chunk = os.read(key.fd, 65536)
                        if not chunk:
                            if key.fd == fifo_fd:
                                fifo_eof = True
                            if observer:
                                write_chunk(key.fd, b'', final=True)
                            selector.unregister(key.fd)
                        else:
                            write_chunk(key.fd, chunk)
                else:
                    raise RuntimeError('bounded final drain incomplete')
                if restricted_output and selector.get_map():
                    raise RuntimeError('bounded final drain incomplete')
            except BaseException as exc:
                result['evidence_errors'].append(error_text(exc)[:1024])
                result['failure_kind'] = 'unsafe'
                result['error'] = result['error'] or error_text(exc)[:8192]
            if observer:
                try:
                    for fd in stream_names:
                        if not restricted_output or fd in finalized_streams:
                            write_chunk(fd, b'', final=True)
                    result['output_observation'] = observer.summary()
                except BaseException as exc:
                    result['evidence_errors'].append(error_text(exc)[:1024])
                    result['failure_kind'] = 'unsafe'
                    result['error'] = result['error'] or 'stream observation incomplete'
            for pipe in (proc.stdout, proc.stderr):
                if pipe is None:
                    continue
                try:
                    pipe.close()
                except BaseException as exc:
                    result['cleanup_errors'].append(error_text(exc))
        for file in files:
            try:
                file.close()
            except BaseException as exc:
                result['cleanup_errors'].append(error_text(exc))
        for closer in (selector.close, (lambda: os.close(fifo_fd)) if fifo_fd is not None else lambda: None,
                       (lambda: os.close(fifo_anchor)) if fifo_anchor is not None else lambda: None):
            try:
                closer()
            except BaseException as exc:
                result['cleanup_errors'].append(error_text(exc))
        if fifo_created:
            try:
                fifo.unlink()
            except FileNotFoundError:
                pass
            except BaseException as exc:
                result['cleanup_errors'].append(error_text(exc))
        if evaluator_capture is not None:
            try:
                evaluator_capture.finish(fifo_eof, not result['cleanup_errors'] and not result['evidence_errors'])
                result['evaluator_capture'] = evaluator_capture.summary()
            except BaseException as exc:
                result['evidence_errors'].append(error_text(exc))
                result['failure_kind'] = 'unsafe'
                result['error'] = result['error'] or 'CAPTURE_CLOSEOUT_ERROR'
        if restricted_output and time.monotonic() > deadline:
            result['cleanup_errors'].append('FIXED_DEADLINE_EXCEEDED')
        if _cancellation and _cancellation['cancelled']:
            result['cancelled'] = True
            result['failure_kind'] = 'cancelled'
            result['error'] = result['error'] or 'CANCELLED'
        result['elapsed'] = time.monotonic() - start
        result['cleanup_errors'] = [str(error)[:1024] for error in result['cleanup_errors']]
        if sample:
            result['resource_observation'] = sample.result()
        if detail:
            result['resource_timeline'] = detail.result()
        if len((json.dumps(result,indent=2)+'\n').encode()) > MAX_PROCESS_RECEIPT:
            result['resource_observation'] = {'status': 'UNKNOWN_RECEIPT_LIMIT'}
            result['error'] = result['error'] or 'maximum process receipt exceeded'
            result['failure_kind'] = 'unsafe'
        save(output.with_name(output.name + '.result.json'), result, root, terminal=True)
        late_cancel = _cancellation and _cancellation['cancelled'] and not result.get('cancelled')
        late_write = restricted_output and time.monotonic() > deadline
        if late_cancel or late_write:
            result['error'] = result['error'] or ('CANCELLED' if late_cancel else 'RECEIPT_EXCEEDED_FIXED_DEADLINE')
            result['failure_kind'] = 'cancelled' if late_cancel else 'unsafe'
            if late_cancel:
                result['cancelled'] = True
            # Late evidence is a failure receipt, never a fresh execution window.
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
