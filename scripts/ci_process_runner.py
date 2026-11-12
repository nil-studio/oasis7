"""Bounded child execution, process heartbeats and owned-process cleanup."""
from datetime import datetime, timezone
import json
import os
from pathlib import Path
import queue
import re
import select
import signal
import stat
import subprocess
import sys
import threading
import time
import uuid

READ_BYTES = 64 * 1024
SUMMARY_LINE_BYTES = 4096
LIVE_CHUNKS = 16


class _Capture:
    """Bounded summary parsing is independent of best-effort live output."""
    def __init__(self):
        self.line = bytearray()
        self.discard_line = False
        self.summary = None
        self.complete = False
        self.read_error = None
        self.lock = threading.Lock()
        self.idle = threading.Event()
        self.idle.set()
        self.output_bytes = self.pending = self.suppressed = 0

    def parse_line(self):
        if b'test result:' in self.line:
            self.summary = None
        match = re.search(rb'test result: ok\. ([0-9]{1,20}) passed; ([0-9]{1,20}) failed; ([0-9]{1,20}) ignored;', self.line)
        if match:
            self.summary = tuple(map(int, match.groups()))

    def feed(self, data):
        parts = data.split(b'\n')
        for index, part in enumerate(parts):
            if not self.discard_line and len(self.line) + len(part) <= SUMMARY_LINE_BYTES:
                self.line.extend(part)
            else:
                if b'test result:' in self.line or b'test result:' in part:
                    self.summary = None
                self.line.clear()
                self.discard_line = True
            if index < len(parts) - 1:
                if not self.discard_line:
                    self.parse_line()
                self.line.clear()
                self.discard_line = False

    def queued(self, size):
        with self.lock:
            self.output_bytes += size
            self.pending += size
            self.idle.clear()

    def settled(self, size, suppressed=False):
        with self.lock:
            self.pending -= size
            if suppressed:
                self.suppressed += size
            if not self.pending:
                self.idle.set()

    def output_state(self):
        with self.lock:
            return dict(output_bytes=self.output_bytes, output_suppressed_bytes=self.suppressed,
                        output_pending_bytes=self.pending)


class _LiveOutput:
    """One daemon for the entire process; never change shared stdout flags."""
    def __init__(self):
        self.queue = queue.Queue(maxsize=LIVE_CHUNKS)
        threading.Thread(target=self.forward, daemon=True).start()

    def emit(self, data, capture=None, completed=None, *, fd):
        if stat.S_ISREG(os.fstat(fd).st_mode):
            # Explicit file redirection is a durable consumer, not a lossy
            # console. It must not wait behind a blocked console packet.
            if capture is not None:
                capture.queued(len(data))
            offset = 0
            try:
                while offset < len(data):
                    written = write_fd(fd, data[offset:])
                    offset += written
                    if capture is not None:
                        capture.settled(written)
            except OSError:
                if capture is not None:
                    capture.settled(len(data) - offset, suppressed=True)
                raise
            if completed is not None:
                completed.set()
            return True
        accepted = True
        for offset in range(0, len(data), READ_BYTES):
            last = offset + READ_BYTES >= len(data)
            accepted &= self.emit_chunk(data[offset:offset + READ_BYTES], capture,
                                        completed if last else None, fd)
        return accepted

    def emit_chunk(self, data, capture, completed, fd):
        if capture is not None:
            capture.queued(len(data))
        packet_fd = None
        try:
            packet_fd = os.dup(fd)
            self.queue.put_nowait((data, capture, completed, packet_fd))
            return True
        except (queue.Full, OSError):
            if packet_fd is not None:
                os.close(packet_fd)
            if capture is not None:
                capture.settled(len(data), suppressed=True)
            return False

    def forward(self):
        while True:
            data, capture, completed, fd = self.queue.get()
            offset = 0
            try:
                while offset < len(data):
                    written = write_fd(fd, data[offset:])
                    offset += written
                    if capture is not None:
                        capture.settled(written)
            except OSError:
                if capture is not None:
                    capture.settled(len(data) - offset, suppressed=True)
            finally:
                os.close(fd)
                if completed is not None:
                    completed.set()
                self.queue.task_done()


_LIVE = None
_LIVE_LOCK = threading.Lock()


def live_output():
    global _LIVE
    with _LIVE_LOCK:
        if _LIVE is None:
            _LIVE = _LiveOutput()
        return _LIVE


def write_fd(fd, data):
    while True:
        try:
            written = os.write(fd, data)
            if written <= 0:
                raise OSError('output made no progress')
            return written
        except InterruptedError:
            continue


def snapshot_output(stream, *, regular=False):
    fd = os.dup(stream.fileno())
    try:
        is_regular = stat.S_ISREG(os.fstat(fd).st_mode)
        if regular and not is_regular:
            raise ValueError('persistent process log must be a regular file')
        if is_regular:
            stream.flush()
        return fd
    except BaseException:
        os.close(fd)
        raise


def owned_processes(marker, group=None):
    """Linux environment marker survives double-fork/setsid; identity includes starttime."""
    found = {}
    for directory in Path('/proc').glob('[0-9]*'):
        try:
            if directory.stat().st_uid != os.getuid():
                continue
            fields = (directory / 'stat').read_text().rsplit(')', 1)[1].split()
            in_group = group is not None and int(fields[2]) == group
            if not in_group:
                try:
                    environment = (directory / 'environ').read_bytes().split(b'\0')
                except PermissionError:
                    # Same-UID runner services can be non-dumpable. They are not
                    # ours merely because their environment cannot be inspected.
                    continue
                if ('OASIS7_CI_PROCESS_OWNER=' + marker).encode() not in environment:
                    ancestors = next((value.partition(b'=')[2].split(b':') for value in environment
                                      if value.startswith(b'OASIS7_CI_PROCESS_ANCESTORS=')), ())
                    if marker.encode() not in ancestors:
                        continue
            if fields[0] != 'Z':
                found[int(directory.name)] = fields[19]
        except (FileNotFoundError, ProcessLookupError):
            continue
    return found


def cleanup(process, marker):
    if Path('/proc').is_dir():
        # Never signal a saved PID without checking its current ownership and starttime.
        initial = owned_processes(marker, process.pid)
        for sig in (signal.SIGTERM, signal.SIGKILL):
            pending = owned_processes(marker, process.pid)
            for pid, identity in pending.items():
                if owned_processes(marker, process.pid).get(pid) == identity:
                    try:
                        os.kill(pid, sig)
                    except ProcessLookupError:
                        pass
            if pending:
                time.sleep(0.3)
        process.poll()
        return ('unresolved' if owned_processes(marker, process.pid) else 'clean'), bool(initial)
    # macOS: inspect only this new process group; zombies need no signal.
    def members():
        rows = subprocess.check_output(['ps', '-axo', 'pid=,pgid=,stat='], text=True)
        return [int(pid) for pid, group, state in (row.split() for row in rows.splitlines())
                if int(group) == process.pid and not state.startswith('Z')]
    initial = members()
    for sig in (signal.SIGTERM, signal.SIGKILL):
        if members():
            try:
                os.killpg(process.pid, sig)
            except ProcessLookupError:
                pass
            except PermissionError:
                process.poll()
                if members():
                    return 'unresolved', bool(initial)
            time.sleep(0.3)
        process.poll()
    return ('unresolved' if members() else 'clean'), bool(initial)


def run(command, cwd, env, suite, name, results, expected_count=None, timeout_seconds=2400, heartbeat_seconds=20, log_stream=None, cancellation_event=None):
    """Capture this call's stdout; an explicit file log receives every child byte."""
    started = datetime.now(timezone.utc).isoformat()
    start = time.monotonic()
    stdout_fd = snapshot_output(sys.stdout)
    log_fd = None
    try:
        if log_stream is not None:
            log_fd = snapshot_output(log_stream, regular=True)
            first, second = os.fstat(stdout_fd), os.fstat(log_fd)
            if stat.S_ISREG(first.st_mode) and (first.st_dev, first.st_ino) == (second.st_dev, second.st_ino):
                os.close(log_fd)
                log_fd = None
        return _run(command, cwd, env, suite, name, results, expected_count, timeout_seconds,
                    heartbeat_seconds, started, start, stdout_fd, log_fd, log_stream is not None, cancellation_event)
    finally:
        os.close(stdout_fd)
        if log_fd is not None:
            os.close(log_fd)


def _run(command, cwd, env, suite, name, results, expected_count, timeout_seconds,
         heartbeat_seconds, started, start, stdout_fd, log_fd, log_requested, cancellation_event=None):
    if cancellation_event is not None and cancellation_event.is_set():
        raise InterruptedError('cancelled before starting child')
    marker = uuid.uuid4().hex
    # Nested artifact runners start their own sessions. Preserve transitive
    # ownership so an outer deadline can clean their leaves even if the inner
    # supervisor is killed during its own cleanup. No unrelated process shares
    # these per-invocation identities.
    ancestors = [value for value in env.get('OASIS7_CI_PROCESS_ANCESTORS', '').split(':') if value]
    parent = env.get('OASIS7_CI_PROCESS_OWNER')
    if parent and parent not in ancestors:
        ancestors.append(parent)
    env = dict(env, OASIS7_CI_PROCESS_OWNER=marker, OASIS7_CI_PROCESS_ANCESTORS=':'.join(ancestors))
    capture = _Capture()
    live = live_output()
    stop_reader = threading.Event()
    reader_fds = []
    def consume(pipe):
        try:
            while not stop_reader.is_set():
                if not select.select([pipe], [], [], 0.05)[0]:
                    continue
                data = os.read(pipe.fileno(), READ_BYTES)
                if not data:
                    if not capture.discard_line:
                        capture.parse_line()
                    capture.complete = True
                    return
                capture.feed(data)
                live.emit(data, capture, fd=reader_stdout_fd)
                if reader_log_fd is not None:
                    offset = 0
                    while offset < len(data):
                        offset += write_fd(reader_log_fd, data[offset:])
        except (OSError, ValueError) as error:
            # Missing output cannot establish a complete libtest count. Avoid
            # an uncaught-thread traceback using the same stalled log channel.
            capture.read_error = type(error).__name__
        finally:
            pipe.close()
            for fd in reader_fds:
                os.close(fd)
    process = None
    try:
        # The reader and each queued packet own independent duplicates, so a
        # closed/reused caller descriptor can never retarget old output.
        reader_stdout_fd = os.dup(stdout_fd)
        reader_fds.append(reader_stdout_fd)
        reader_log_fd = os.dup(log_fd) if log_fd is not None else None
        if reader_log_fd is not None:
            reader_fds.append(reader_log_fd)
        process = subprocess.Popen(command, cwd=cwd, env=env, start_new_session=True,
                                   stdout=subprocess.PIPE, stderr=subprocess.STDOUT, bufsize=0)
        reader = threading.Thread(target=consume, args=(process.stdout,), daemon=True)
        reader.start()
    except BaseException:
        for fd in reader_fds:
            os.close(fd)
        if process is not None:
            cleanup(process, marker)
            process.wait(timeout=10)
            process.stdout.close()
        raise
    timed_out = False
    cancelled = False
    previous_handlers = {}
    def interrupt(signum, frame):
        raise InterruptedError(f'cancelled by signal {signum}')
    if threading.current_thread() is threading.main_thread():
        for signum in (signal.SIGTERM, signal.SIGINT):
            previous_handlers[signum] = signal.signal(signum, interrupt)
    next_heartbeat = start + heartbeat_seconds
    try:
        while True:
            now = time.monotonic()
            if now - start >= timeout_seconds:
                timed_out = True
                break
            if cancellation_event is not None and cancellation_event.is_set():
                cancelled = True
                break
            if process.poll() is not None:
                break
            if now >= next_heartbeat:
                live.emit(f'\nsuite={suite} test={name} elapsed={int(now-start)}s state=running\n'.encode(), fd=stdout_fd)
                next_heartbeat = now + heartbeat_seconds
            time.sleep(min(0.2, heartbeat_seconds, max(0, start + timeout_seconds - now)))
    except InterruptedError:
        cancelled = True
    finally:
        # Observing an exit after the absolute deadline cannot turn it into a
        # success, including a child that completed while supervision stalled.
        timed_out |= time.monotonic() - start >= timeout_seconds
        try:
            cleanup_result, leaked = cleanup(process, marker)
        except (OSError, subprocess.SubprocessError):
            cleanup_result, leaked = 'unresolved', True
        for signum, handler in previous_handlers.items():
            signal.signal(signum, handler)
    if cleanup_result == 'clean':
        process.wait(timeout=10)
    reader.join(timeout=2)
    if reader.is_alive():
        stop_reader.set()
        reader.join(timeout=0.2)
    if not reader.is_alive():
        process.stdout.close()
    capture.idle.wait(timeout=0.05)
    cancelled |= cancellation_event is not None and cancellation_event.is_set()
    count_ok = expected_count is None or (expected_count > 0 and capture.complete and capture.summary is not None and capture.summary[0] == expected_count and capture.summary[1] == 0)
    result = dict(suite=suite, test_name=name, started_at=started,
                  elapsed_seconds=round(time.monotonic()-start, 3), exit_code=process.returncode,
                  timeout=timed_out, cancelled=cancelled, cleanup_result=cleanup_result, leaked_processes=leaked,
                  expected_count=expected_count, count_ok=count_ok,
                  passed=process.returncode == 0 and not timed_out and not cancelled and count_ok and cleanup_result == 'clean' and not leaked)
    # Suppressed means explicitly rejected/discarded by the bounded live queue;
    # pending includes queued or blocked writes not yet confirmed at this point.
    result.update(capture.output_state())
    result['output_read_error'] = capture.read_error
    if log_requested:
        result['log_complete'] = capture.complete and capture.read_error is None
        result['passed'] &= result['log_complete']
    results.parent.mkdir(parents=True, exist_ok=True)
    with results.open('a') as stream:
        stream.write(json.dumps(result) + '\n')
    completed = threading.Event()
    if live.emit((json.dumps(result) + '\n').encode(), completed=completed, fd=stdout_fd):
        completed.wait(timeout=0.05)
    return result
