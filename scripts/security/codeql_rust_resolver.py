#!/usr/bin/env python3
"""One owned CLI server for the frozen Rust library-path preparation sequence."""
import argparse
import ctypes
import hashlib
import importlib.util
import json
import math
import os
from pathlib import Path
import selectors
import signal
import stat
import struct
import subprocess
import sys
import time

import codeql_rust_diagnostic_runtime as runtime

SCHEMA = 'oasis7/rust-library-path-session/v1'
READ_LIMIT = 16 * 1024**2
ARGV_LIMIT = 16 * 1024
HEADER_LIMIT = 4096
CHUNK = 65536
COMMAND_SECONDS = 20
CLEANUP_SECONDS = runtime.CLEANUP_SECONDS + .75
COUNTS = {'candidate': 44, 'production': 37}
STATUSES = {'candidate': 'PASS_CANDIDATE_CLOSURE', 'production': 'PASS_REPAIRED_CLOSURE'}
HERE = Path(__file__).resolve().parent
_validator_module = None


class ResolverError(RuntimeError):
    def __init__(self, code, index=None):
        self.code, self.index = code, index
        super().__init__(code + (':' + str(index) if index is not None else ''))


def _sha(data):
    return hashlib.sha256(data).hexdigest()


def _json(data):
    return (json.dumps(data, separators=(',', ':'), allow_nan=False) + '\n').encode()


def _decode(data):
    def pairs(items):
        result = {}
        for key, value in items:
            if key in result:
                raise ResolverError('DUPLICATE_JSON_KEY')
            result[key] = value
        return result
    try:
        return json.loads(data.decode('utf-8'), object_pairs_hook=pairs,
                          parse_constant=lambda _: (_ for _ in ()).throw(ValueError()))
    except (UnicodeError, ValueError, RecursionError):
        raise ResolverError('INVALID_JSON') from None


def _finite(value):
    return type(value) in (int, float) and math.isfinite(value)


def _check(end):
    if not _finite(end) or time.monotonic() >= end:
        raise ResolverError('DEADLINE')


def _regular(path, end, limit=READ_LIMIT):
    """Bound the read, reject special files and bind the opened identity."""
    _check(end)
    fd = os.open(path, os.O_RDONLY | os.O_CLOEXEC | os.O_NOFOLLOW | os.O_NONBLOCK)
    try:
        before = os.fstat(fd)
        if not stat.S_ISREG(before.st_mode) or not 0 <= before.st_size <= limit:
            raise ResolverError('INPUT_FILE_BOUNDARY')
        result = bytearray()
        while len(result) <= limit:
            _check(end)
            part = os.read(fd, min(CHUNK, limit + 1 - len(result)))
            if not part:
                break
            result.extend(part)
        after = os.fstat(fd)
        current = os.stat(path, follow_symlinks=False)
        identity = lambda s: (s.st_dev, s.st_ino, s.st_mode, s.st_size, s.st_mtime_ns, s.st_ctime_ns)
        if len(result) != before.st_size or identity(before) != identity(after) or identity(after) != identity(current):
            raise ResolverError('INPUT_CHANGED')
        return bytes(result)
    finally:
        os.close(fd)


def _verify(value, record, closure, work, end):
    global _validator_module
    _check(end)
    if _validator_module is None:
        spec = importlib.util.spec_from_file_location(
            'resolver_model_validator', HERE / 'codeql-rust-model-diagnostic.py')
        _validator_module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(_validator_module)
    try:
        _validator_module.verify_library_path(
            value, Path(record['root']), str(Path(record['relative']).with_suffix('')),
            closure, record['pack'])
        if value['compilationCache'] != [str(Path(work) / 'resolve-compilation-cache')]:
            raise ResolverError('COMPILATION_CACHE')
    except ResolverError:
        raise
    except (RuntimeError, KeyError, TypeError, ValueError) as exc:
        code = {
            'actual library dependency closure differs from selected bundle': 'DEPENDENCY_CLOSURE',
            'actual dbscheme differs': 'DBSCHEME',
            'actual query identity differs': 'QUERY_IDENTITY',
        }.get(str(exc), 'LIBRARY_PATH_IDENTITY')
        raise ResolverError(code) from None
    if _sha(_regular(record['path'], end)) != record['sha256']:
        raise ResolverError('QUERY_IDENTITY')
    _check(end)


def command(plan, index):
    return [plan['codeql'], 'resolve', 'library-path',
            '--query=' + plan['records'][index]['path'], '--format=json',
            '--additional-packs=' + os.pathsep.join(p['root'] for p in plan['closures']['default'].values()),
            '--no-default-compilation-cache',
            '--compilation-cache=' + str(Path(plan['work']) / 'resolve-compilation-cache'),
            '--common-caches=' + str(Path(plan['work']) / 'resolve-common-cache')]


def validate_plan(plan):
    keys = {'schema', 'mode', 'codeql', 'work', 'records', 'closures', 'started', 'end', 'ack', 'output_limit'}
    if not isinstance(plan, dict) or set(plan) != keys or plan['schema'] != SCHEMA or plan['mode'] not in COUNTS:
        raise ResolverError('REQUEST_SCHEMA')
    if (not _finite(plan['started']) or not _finite(plan['end']) or plan['started'] >= plan['end']
            or type(plan['output_limit']) is not int or not 0 < plan['output_limit'] <= 256 * 1024**2):
        raise ResolverError('REQUEST_BUDGET')
    for name in ('codeql', 'work'):
        value = plan[name]
        if (not isinstance(value, str) or not value or '\0' in value
                or Path(value).resolve() != Path(value)):
            raise ResolverError('REQUEST_PATH')
    kinds = {'default', 'witness'} if plan['mode'] == 'candidate' else {'default'}
    if not isinstance(plan['closures'], dict) or set(plan['closures']) != kinds:
        raise ResolverError('REQUEST_CLOSURES')
    for kind, closure in plan['closures'].items():
        if not isinstance(closure, dict) or len(closure) != (13 if kind == 'witness' else 14):
            raise ResolverError('REQUEST_CLOSURES')
        for value in closure.values():
            root = value.get('root') if isinstance(value, dict) else None
            if (not isinstance(root, str) or not root or any(c in root for c in ('\0', '\n', '\r', os.pathsep))
                    or Path(root).resolve() != Path(root)):
                raise ResolverError('REQUEST_CLOSURES')
    records = plan['records']
    if not isinstance(records, list) or len(records) != COUNTS[plan['mode']]:
        raise ResolverError('REQUEST_QUERY_COUNT')
    paths = set()
    for index, record in enumerate(records):
        if not isinstance(record, dict) or set(record) != {'path', 'root', 'relative', 'id', 'sha256', 'pack', 'kind'}:
            raise ResolverError('REQUEST_QUERY')
        if not all(isinstance(v, str) and v and '\0' not in v for v in record.values()):
            raise ResolverError('REQUEST_QUERY')
        expected_kind = 'witness' if plan['mode'] == 'candidate' and index >= 37 else 'default'
        root, path, relative = Path(record['root']), Path(record['path']), Path(record['relative'])
        if (record['kind'] != expected_kind or path in paths or path.resolve() != path
                or root.resolve() != root or not path.is_relative_to(root)
                or relative.is_absolute() or '..' in relative.parts or path != root / relative
                or len(record['sha256']) != 64 or any(c not in '0123456789abcdef' for c in record['sha256'])):
            raise ResolverError('REQUEST_QUERY')
        paths.add(path)
        if len(json.dumps(command(plan, index)).encode()) > ARGV_LIMIT:
            raise ResolverError('ARGV_LIMIT', index)
    ack = plan['ack']
    if (not isinstance(ack, dict) or set(ack) != {'path', 'device', 'inode'}
            or not isinstance(ack['path'], str)
            or any(type(ack[k]) is not int or ack[k] < 0 for k in ('device', 'inode'))):
        raise ResolverError('ACK_IDENTITY')
    _check(plan['end'])


class Subreaper:
    def __init__(self):
        self.libc = ctypes.CDLL(None, use_errno=True)
        old = ctypes.c_int()
        if self.libc.prctl(37, ctypes.byref(old), 0, 0, 0) != 0:
            raise ResolverError('SUBREAPER_GET')
        self.old, self.changed = old.value, False
        if self.libc.prctl(36, 1, 0, 0, 0) != 0:
            raise ResolverError('SUBREAPER_SET')
        self.changed = True

    def close(self):
        if self.changed:
            if self.libc.prctl(36, self.old, 0, 0, 0) != 0:
                raise ResolverError('SUBREAPER_RESTORE')
            self.changed = False


class Observation:
    """The parent is the only evidence writer; poll acknowledges prior writes."""
    def __init__(self, plan, out, on_stage=None):
        self.plan, self.out, self.on_stage = plan, Path(out), on_stage
        self.read_fd, self.write_fd = os.pipe2(os.O_CLOEXEC | os.O_NONBLOCK)
        identity = os.fstat(self.read_fd)
        plan['ack'] = dict(path=f'/proc/{os.getpid()}/fd/{self.read_fd}',
                           device=identity.st_dev, inode=identity.st_ino)
        self.header = bytearray()
        self.raw = None
        self.response_header = None
        self.server = self.terminal = None
        self.current = None
        self.receipts, self.commands = [], []
        self.pending_ack = None
        self.ack_offset = self.acked = 0
        self.error = None
        self.parsing_stopped = False
        self.eof = dict(stdout=False, stderr=False)
        self.bytes = dict(stdout=0, stderr=0)
        self.hashes = {name: hashlib.sha256() for name in self.bytes}
        self.pipe_eof = False
        self.closed = False
        self.payload_end = plan['end'] - CLEANUP_SECONDS
        self.first_work_end = min(plan['started'] + COMMAND_SECONDS, self.payload_end) - CLEANUP_SECONDS

    def _fail(self, code):
        self.error = self.error or code
        raise ResolverError(self.error, self.current['index'] if self.current else 0)

    def _event(self, value):
        if not isinstance(value, dict) or self.terminal is not None:
            self._fail('CONTROL_SCHEMA')
        kind = value.get('kind')
        if kind == 'server':
            if (self.server is not None or set(value) != {'kind', 'adapter_pid', 'pid', 'pgid'}
                    or any(type(value[k]) is not int or value[k] <= 0 for k in ('adapter_pid', 'pid', 'pgid'))
                    or value['pgid'] != value['adapter_pid']):
                self._fail('SERVER_IDENTITY')
            self.server = value
        elif kind == 'command':
            index = len(self.commands)
            if (self.server is None or set(value) != {'kind', 'index', 'started', 'deadline'}
                    or type(value['index']) is not int or value['index'] != index
                    or index >= len(self.plan['records']) or self.acked != index
                    or not _finite(value['started']) or not _finite(value['deadline'])
                    or (index == 0 and value['started'] != self.plan['started'])
                    or value['started'] < self.plan['started'] or value['started'] > time.monotonic()
                    or value['deadline'] != min(value['started'] + COMMAND_SECONDS, self.payload_end)):
                self._fail('COMMAND_SEQUENCE')
            self.current = dict(index=index, started=value['started'], deadline=value['deadline'],
                                nul=False, persisted=False, ack=False)
            self.commands.append(self.current)
            if self.on_stage:
                self.on_stage('resolve-' + str(index).zfill(2))
        elif kind == 'response':
            if (set(value) != {'kind', 'index', 'bytes', 'sha256', 'nul'}
                    or self.current is None or type(value['index']) is not int
                    or value['index'] != self.current['index']
                    or self.current['persisted'] or self.pending_ack is not None
                    or type(value['bytes']) is not int or not 0 < value['bytes'] <= READ_LIMIT
                    or type(value['nul']) is not bool or not value['nul']
                    or not isinstance(value['sha256'], str) or len(value['sha256']) != 64):
                self._fail('RESPONSE_BOUNDARY')
            self.response_header, self.raw = value, bytearray()
            return b''
        elif kind == 'terminal':
            fields = {'kind', 'error', 'index', 'completed', 'acks', 'shutdown', 'server'}
            if (set(value) != fields or not self._server_result(value['server'])
                    or value['error'] is not None and not valid_code(value['error'])
                    or type(value['completed']) is not int
                    or not len(self.receipts) <= value['completed'] <= min(len(self.receipts) + 1, len(self.commands))
                    or type(value['acks']) is not int
                    or not max(0, self.acked - 1) <= value['acks'] <= min(self.acked, value['completed'])
                    or type(value['shutdown']) is not bool):
                self._fail('TERMINAL_SCHEMA')
            index = value['index']
            last = len(self.commands) - 1
            if (index is not None and (type(index) is not int or not 0 <= index < len(self.plan['records']))
                    or (index is None and (last >= 0 or value['error'] is None))
                    or (index is not None and index != last
                        and not (value['error'] is not None and index == last + 1))
                    or (value['shutdown'] and value['acks'] != len(self.plan['records']))
                    or (value['error'] is None and (not value['shutdown'] or value['acks'] != len(self.plan['records'])))):
                self._fail('TERMINAL_SEQUENCE')
            self.terminal = value
            self.error = self.error or value['error']
        else:
            self._fail('CONTROL_SCHEMA')
        return _json(value)

    def _server_result(self, value):
        fields = {'pid', 'pgid', 'exit', 'stdout_eof', 'stderr_eof', 'remaining',
                  'cleanup_errors', 'reaped_orphans'}
        if not isinstance(value, dict) or set(value) != fields:
            return False
        if any(value[k] is not None and (type(value[k]) is not int or value[k] <= 0)
               for k in ('pid', 'pgid')):
            return False
        if (value['exit'] is not None and type(value['exit']) is not int
                or any(type(value[k]) is not bool for k in ('stdout_eof', 'stderr_eof'))):
            return False
        if self.server and any(value[k] != self.server[k] for k in ('pid', 'pgid')):
            return False
        if (not isinstance(value['remaining'], list) or len(value['remaining']) > 16
                or any(type(pid) is not int or pid <= 0 for pid in value['remaining'])
                or not isinstance(value['cleanup_errors'], list) or len(value['cleanup_errors']) > 8
                or not all(valid_code(code) for code in value['cleanup_errors'])
                or not isinstance(value['reaped_orphans'], list) or len(value['reaped_orphans']) > 16):
            return False
        return all(isinstance(row, dict) and set(row) == {'pid', 'wait_status'}
                   and type(row['pid']) is int and row['pid'] > 0
                   and type(row['wait_status']) is int and row['wait_status'] >= 0
                   for row in value['reaped_orphans'])

    def _response(self):
        header, raw = self.response_header, bytes(self.raw)
        self.response_header = self.raw = None
        index = header['index']
        end = self.current['deadline'] - CLEANUP_SECONDS
        _check(end)
        if _sha(raw) != header['sha256']:
            self._fail('RESPONSE_HASH')
        record = self.plan['records'][index]
        value = _decode(raw)
        _verify(value, record, self.plan['closures'][record['kind']], self.plan['work'], end)
        receipt = dict(query=record['path'], actual=value, status=STATUSES[self.plan['mode']])
        if self.plan['mode'] == 'candidate':
            receipt['kind'] = record['kind']
        path = self.out / ('resolve-' + str(index).zfill(2) + '.log')
        if path.exists() or path.is_symlink():
            self._fail('RESPONSE_ALREADY_EXISTS')
        runtime.evidence_write(path, raw, self.out)
        _check(end)
        updated = self.receipts + [receipt]
        runtime.save(self.out / 'library-resolution.json', updated, self.out)
        _check(end)
        self.receipts = updated
        self.current.update(nul=True, persisted=True, bytes=len(raw), sha256=header['sha256'])
        self.pending_ack, self.ack_offset = struct.pack('!I', index), 0
        return _json(dict(kind='persisted', index=index, bytes=len(raw), sha256=header['sha256']))

    def feed(self, stream, chunk, *, final=False):
        self.bytes[stream] += len(chunk)
        self.hashes[stream].update(chunk)
        if final:
            self.eof[stream] = True
        if stream == 'stderr':
            return chunk
        if self.parsing_stopped:
            return b''
        output = bytearray()
        view = memoryview(chunk)
        try:
            while view:
                if self.raw is not None:
                    needed = self.response_header['bytes'] - len(self.raw)
                    take = min(needed, len(view))
                    self.raw.extend(view[:take])
                    view = view[take:]
                    if len(self.raw) == self.response_header['bytes']:
                        output.extend(self._response())
                else:
                    # A header never retains the following raw payload.
                    newline = bytes(view[:HEADER_LIMIT + 1]).find(b'\n')
                    take = len(view) if newline < 0 else newline + 1
                    if len(self.header) + take > HEADER_LIMIT:
                        self._fail('CONTROL_LIMIT')
                    self.header.extend(view[:take])
                    view = view[take:]
                    if newline >= 0:
                        data = bytes(self.header)
                        self.header.clear()
                        output.extend(self._event(_decode(data)))
            if final and (self.header or self.raw is not None or self.terminal is None):
                self._fail('CONTROL_EOF')
        except BaseException as exc:
            self.error = self.error or error_code(exc)
            self.parsing_stopped = True
            raise
        return bytes(output)

    def closing_deadline(self):
        return min(self.plan['end'], self.current['deadline'] if self.current else
                   self.plan['started'] + COMMAND_SECONDS)

    def poll(self, payload_end):
        self.payload_end = min(self.payload_end, payload_end)
        try:
            if self.terminal is not None:
                # A control terminal is not actual adapter exit/EOF. Allow the
                # existing cleanup reserve, but never start another clock.
                _check(self.closing_deadline())
                return
            if self.error:
                raise ResolverError(self.error)
            end = min(self.payload_end,
                      self.current['deadline'] - CLEANUP_SECONDS if self.current else self.first_work_end)
            _check(end)
            if self.pending_ack is not None:
                if runtime.output_bytes(self.out) >= runtime.WRITE_CAP:
                    self._fail('SESSION_OUTPUT_LIMIT')
                try:
                    count = os.write(self.write_fd, self.pending_ack[self.ack_offset:])
                except (BlockingIOError, InterruptedError):
                    return
                if count <= 0:
                    self._fail('ACK_WRITE')
                self.ack_offset += count
                if self.ack_offset == len(self.pending_ack):
                    self.current.update(ack=True, acked_at=time.monotonic())
                    self.acked += 1
                    self.pending_ack = None
                    _check(end)
        except BaseException as exc:
            self.error = self.error or error_code(exc)
            raise

    def close(self, group_safe):
        if self.closed:
            return
        self.closed = True
        for name in ('write_fd', 'read_fd'):
            fd = getattr(self, name)
            if fd is None:
                continue
            try:
                if name == 'read_fd' and group_safe:
                    self.pipe_eof = os.read(fd, 1) == b''
                    if not self.pipe_eof:
                        self.error = self.error or 'ACK_RESIDUAL'
            except OSError:
                self.error = self.error or 'ACK_CLOSE'
            finally:
                try:
                    os.close(fd)
                except OSError:
                    self.error = self.error or 'ACK_CLOSE'
                setattr(self, name, None)

    def summary(self):
        return dict(schema=SCHEMA, status='PARTIAL' if self.error else 'OBSERVED',
                    error=self.error, command_starts=len(self.commands),
                    validated_responses=len(self.receipts), acknowledged=self.acked,
                    stream_bytes=dict(self.bytes),
                    stream_sha256={k: h.hexdigest() for k, h in self.hashes.items()},
                    stream_eof=dict(self.eof))


def valid_code(value):
    return isinstance(value, str) and 0 < len(value) <= 80 and all(c in 'ABCDEFGHIJKLMNOPQRSTUVWXYZ_0123456789' for c in value)


def error_code(exc):
    if isinstance(exc, ResolverError):
        return exc.code if valid_code(exc.code) else 'RESOLVER_ERROR'
    if isinstance(exc, runtime.PhaseError):
        first = exc.result.get('execution_failure_kind') or exc.result.get('failure_kind')
        return {'cancelled': 'CANCELLED', 'timeout': 'DEADLINE',
                'child_exit': 'ADAPTER_EXIT'}.get(first, 'RUNTIME_FAILURE')
    if isinstance(exc, RuntimeError) and str(exc) == 'combined evidence budget exceeded before write':
        return 'EVIDENCE_LIMIT'
    return ('DEADLINE' if isinstance(exc, TimeoutError) else
            'CANCELLED' if isinstance(exc, (KeyboardInterrupt, SystemExit, runtime.CancelledError)) else
            'IO_ERROR' if isinstance(exc, OSError) else 'RESOLVER_ERROR')


def resolve(codeql, records, closures, work, out, run, *, end, mode, on_stage=None):
    """Run one adapter; the caller supplies its original absolute end and runner."""
    started = time.monotonic()
    out = Path(out)
    if any(path.exists() or path.is_symlink() for path in
           (out / name for name in ('resolver-request.json', 'resolver-session.json', 'library-resolution.json'))):
        raise ResolverError('SESSION_ALREADY_EXISTS')
    normalized = [dict(record, kind=record.get('kind', 'default')) for record in records]
    plan = dict(schema=SCHEMA, mode=mode, codeql=str(codeql), work=str(work), records=normalized,
                closures=closures, started=started, end=end, ack={},
                output_limit=runtime.WRITE_CAP - runtime.output_bytes(out))
    observer = reaper = result = failure = None
    report = dict(schema=SCHEMA, status='INCOMPLETE', error=None, error_source=None, observer_error=None, commands=[],
                  started_monotonic=started, absolute_end=end)
    if on_stage:
        on_stage('resolve-00')
    try:
        observer = Observation(plan, out, on_stage)
        validate_plan(plan)
        request = out / 'resolver-request.json'
        if request.exists() or request.is_symlink() or (out / 'library-resolution.json').exists():
            raise ResolverError('SESSION_ALREADY_EXISTS')
        encoded_request = (json.dumps(plan, indent=2) + '\n').encode()
        if len(encoded_request) > READ_LIMIT:
            raise ResolverError('REQUEST_LIMIT')
        runtime.evidence_write(request, encoded_request, out)
        del encoded_request
        request_bytes = _regular(request, observer.first_work_end)
        report['request'] = dict(path=request.name, bytes=len(request_bytes), sha256=_sha(request_bytes),
                                 mode=mode, query_count=len(normalized))
        reaper = Subreaper()
        report['parent_subreaper_original'] = reaper.old
        argv = [sys.executable, '-B', str(Path(__file__).resolve()), '--request', str(request),
                '--request-sha256', _sha(request_bytes)]
        del request_bytes
        result = run(argv, 'resolver-session', None, observer=observer,
                     restricted_output=True, reap_orphans=True)
        _check(observer.closing_deadline())
    except BaseException as exc:
        failure = exc
        result = getattr(exc, 'result', result)
        # Runtime preserves its first execution error. A later observer error
        # during cleanup cannot identify or replace that primary failure.
        report['error'] = error_code(exc)
        report['error_source'] = 'runtime' if isinstance(exc, runtime.PhaseError) else 'resolver'
    finally:
        if observer:
            safe = bool(result and result.get('pid') and not result.get('cleanup_errors'))
            observer.close(safe)
            original_error = report['error']
            report.update(observer.summary(), commands=observer.commands,
                          server=observer.server, terminal=observer.terminal,
                          ack_pipe_eof=observer.pipe_eof, observer_error=observer.error)
            report['error'] = original_error or observer.error
            if not original_error and observer.error:
                report['error_source'] = 'observer'
        if reaper:
            try:
                reaper.close()
                report['parent_subreaper_restored'] = True
            except BaseException as exc:
                report['parent_subreaper_restored'] = False
                if not report['error']:
                    report.update(error=error_code(exc), error_source='resolver')
                failure = failure or exc
        else:
            report['parent_subreaper_restored'] = True
        report['runtime'] = {key: result.get(key) for key in
                             ('pid', 'pgid', 'exit', 'execution_failure_kind', 'failure_kind', 'error',
                              'cleanup_errors', 'evidence_errors', 'elapsed')} if result else None
        report['elapsed_seconds'] = time.monotonic() - started
        closing_end = observer.closing_deadline() if observer else min(end, started + COMMAND_SECONDS)
        report['closing_deadline'] = closing_end
        if time.monotonic() >= closing_end and not report['error']:
            report.update(error='DEADLINE', error_source='closing_deadline')
        terminal = observer.terminal if observer else None
        server = terminal.get('server', {}) if terminal else {}
        complete = bool(
            failure is None and not report['error'] and observer and terminal
            and len(observer.receipts) == COUNTS[mode] == observer.acked
            and terminal['completed'] == terminal['acks'] == COUNTS[mode] and terminal['shutdown']
            and server.get('exit') == 0 and server.get('stdout_eof') and server.get('stderr_eof')
            and not server.get('remaining') and not server.get('cleanup_errors')
            and observer.eof == dict(stdout=True, stderr=True) and observer.pipe_eof
            and observer.server and observer.server['adapter_pid'] == result.get('pid')
            and observer.server['pgid'] == result.get('pgid')
            and server.get('pid') == observer.server['pid'] and server.get('pgid') == result.get('pgid')
            and type(result.get('exit')) is int and result['exit'] == 0 and not result.get('failure_kind')
            and not result.get('cleanup_errors') and not result.get('evidence_errors')
            and report['parent_subreaper_restored'] and time.monotonic() < closing_end)
        report['status'] = 'COMPLETE' if complete else 'INCOMPLETE'
        if not complete:
            if not report['error']:
                report.update(error='SESSION_INCOMPLETE', error_source='resolver')
        def save_report():
            encoded = (json.dumps(report, indent=2) + '\n').encode()
            if len(encoded) > runtime.MAX_CONTROL_RECEIPT:
                raise ResolverError('SESSION_RECEIPT_LIMIT')
            runtime.evidence_write(out / 'resolver-session.json', encoded, out, terminal=True)
        save_report()
        if time.monotonic() >= closing_end:
            report.update(status='INCOMPLETE', receipt_deadline_exceeded=True,
                          elapsed_seconds=time.monotonic() - started)
            if not report['error']:
                report.update(error='DEADLINE', error_source='receipt_deadline')
            # As in runtime, this is only a late failure receipt, never a new
            # execution or cleanup window and never a replacement first error.
            save_report()
    if report['status'] != 'COMPLETE':
        index = (observer.current['index'] if observer and observer.current else 0)
        raise ResolverError(report['error'], index) from failure
    return observer.receipts


def _members(end):
    found = []
    for path in Path('/proc').iterdir():
        _check(end)
        if not path.name.isdigit():
            continue
        try:
            value = (path / 'stat').read_text()
            fields = value[value.rindex(')') + 2:].split()
            if int(fields[2]) == os.getpgrp():
                found.append(dict(pid=int(path.name), ppid=int(fields[1]),
                                  ticks=int(fields[19]), state=fields[0]))
        except (OSError, ValueError, IndexError):
            continue
        if len(found) > 64:
            raise ResolverError('GROUP_IDENTITY_LIMIT')
    return found


class Server:
    """Child-side CLI pipes; never writes an evidence file or starts a new PGID."""
    def __init__(self, plan, ack_fd, cancelled):
        self.plan, self.ack_fd, self.cancelled = plan, ack_fd, cancelled
        self.end = plan['end'] - CLEANUP_SECONDS
        self.selector = selectors.DefaultSelector()
        self.proc = None
        self.eof = dict(stdout=False, stderr=False)
        self.stdout = bytearray()
        self.stream_bytes = 0
        self.output_bytes = 0
        self.cleanup_errors = []
        self.reaped = []
        self.remaining = []
        self.server_pgid = None
        self.index = None
        self.completed = self.acks = 0
        self.shutdown = False
        self.work_end = min(plan['started'] + COMMAND_SECONDS, self.end) - CLEANUP_SECONDS

    def check(self, end, cleanup=False):
        _check(end)
        if self.cancelled[0] and not cleanup:
            raise ResolverError('CANCELLED', self.index)

    def write(self, fd, data, end, *, terminal=False, cleanup=False):
        view = memoryview(data)
        reserve = 0 if terminal else 2 * HEADER_LIMIT
        if self.output_bytes + len(data) > self.plan['output_limit'] - reserve:
            raise ResolverError('SESSION_OUTPUT_LIMIT', self.index)
        while view:
            self.check(end, cleanup)
            try:
                count = os.write(fd, view[:CHUNK])
            except InterruptedError:
                continue
            except BlockingIOError:
                import select
                try:
                    select.select([], [fd], [], min(.02, max(0, end - time.monotonic())))
                except InterruptedError:
                    pass
                continue
            if count <= 0:
                raise ResolverError('OUTPUT_WRITE', self.index)
            self.output_bytes += count
            view = view[count:]

    def emit(self, value, end, **kwargs):
        data = _json(value)
        if len(data) > HEADER_LIMIT:
            raise ResolverError('CONTROL_LIMIT', self.index)
        self.write(1, data, end, **kwargs)

    def spawn(self):
        self.check(self.work_end)
        self.proc = subprocess.Popen(
            [self.plan['codeql'], 'execute', 'cli-server'], stdin=subprocess.PIPE,
            stdout=subprocess.PIPE, stderr=subprocess.PIPE, bufsize=0, start_new_session=False)
        self.server_pgid = os.getpgid(self.proc.pid)
        if self.server_pgid != os.getpgrp():
            raise ResolverError('SERVER_PGID')
        for name, stream in (('stdout', self.proc.stdout), ('stderr', self.proc.stderr)):
            os.set_blocking(stream.fileno(), False)
            self.selector.register(stream, selectors.EVENT_READ, name)
        os.set_blocking(self.proc.stdin.fileno(), False)
        self.selector.register(self.ack_fd, selectors.EVENT_READ, 'ack')
        self.emit(dict(kind='server', adapter_pid=os.getpid(), pid=self.proc.pid,
                       pgid=self.server_pgid), self.work_end)

    def pump(self, end, *, cleanup=False):
        self.check(end, cleanup)
        for key, _ in self.selector.select(min(.02, max(0, end - time.monotonic()))):
            if key.data == 'ack':
                continue
            try:
                part = os.read(key.fd, CHUNK)
            except (BlockingIOError, InterruptedError):
                continue
            if not part:
                self.eof[key.data] = True
                self.selector.unregister(key.fileobj)
                continue
            self.stream_bytes += len(part)
            if self.stream_bytes > self.plan['output_limit'] - 2 * HEADER_LIMIT:
                raise ResolverError('SESSION_OUTPUT_LIMIT', self.index)
            if key.data == 'stderr':
                self.write(2, part, end, cleanup=cleanup)
            else:
                if len(self.stdout) + len(part) > READ_LIMIT + 1:
                    raise ResolverError('RESPONSE_LIMIT', self.index)
                self.stdout.extend(part)
        self.proc.poll()

    def send(self, data, end):
        view = memoryview(data)
        while view:
            self.check(end)
            try:
                count = os.write(self.proc.stdin.fileno(), view)
            except InterruptedError:
                continue
            except BlockingIOError:
                self.pump(end)
                continue
            if count <= 0:
                raise ResolverError('COMMAND_WRITE', self.index)
            view = view[count:]

    def acknowledge(self, index, end):
        data = bytearray()
        while len(data) < 4:
            self.check(end)
            try:
                part = os.read(self.ack_fd, 5 - len(data))
            except InterruptedError:
                continue
            except BlockingIOError:
                self.pump(end)
                continue
            if not part:
                raise ResolverError('ACK_EOF', index)
            data.extend(part)
            if len(data) > 4:
                raise ResolverError('ACK_SEQUENCE', index)
        if struct.unpack('!I', data)[0] != index:
            raise ResolverError('ACK_SEQUENCE', index)
        try:
            extra = os.read(self.ack_fd, 1)
        except BlockingIOError:
            extra = None
        if extra == b'':
            raise ResolverError('ACK_EOF', index)
        if extra:
            raise ResolverError('ACK_SEQUENCE', index)
        self.acks += 1

    def query(self, index, started):
        self.index = index
        deadline = min(started + COMMAND_SECONDS, self.end)
        self.work_end = deadline - CLEANUP_SECONDS
        self.check(self.work_end)
        self.pump(self.work_end)
        if self.stdout or self.eof['stdout'] or self.proc.poll() is not None:
            raise ResolverError('UNASSIGNED_STDOUT', index)
        self.emit(dict(kind='command', index=index, started=started, deadline=deadline), self.work_end)
        argv = command(self.plan, index)
        if len(json.dumps(argv).encode()) > ARGV_LIMIT:
            raise ResolverError('ARGV_LIMIT', index)
        record = self.plan['records'][index]
        if _sha(_regular(record['path'], self.work_end)) != record['sha256']:
            raise ResolverError('QUERY_IDENTITY', index)
        self.send(json.dumps(argv[1:], separators=(',', ':')).encode() + b'\0', self.work_end)
        while True:
            self.check(self.work_end)
            at = self.stdout.find(b'\0')
            if at >= 0:
                raw = bytes(self.stdout[:at])
                del self.stdout[:at + 1]
                if self.stdout:
                    raise ResolverError('UNASSIGNED_STDOUT', index)
                if not raw or len(raw) > READ_LIMIT:
                    raise ResolverError('RESPONSE_LIMIT', index)
                _verify(_decode(raw), record, self.plan['closures'][record['kind']],
                        self.plan['work'], self.work_end)
                self.completed += 1
                self.emit(dict(kind='response', index=index, bytes=len(raw),
                               sha256=_sha(raw), nul=True), self.work_end)
                self.write(1, raw, self.work_end)
                del raw
                self.acknowledge(index, self.work_end)
                return deadline
            if self.eof['stdout']:
                raise ResolverError('RESPONSE_EOF_WITHOUT_NUL', index)
            self.pump(self.work_end)

    def await_exit(self, end, *, cleanup=False):
        while self.proc.poll() is None or not all(self.eof.values()):
            self.pump(end, cleanup=cleanup)
        self.proc.wait(timeout=max(.001, end - time.monotonic()))

    def finish(self, deadline):
        if self.proc is None:
            self.selector.close()
            return
        try:
            if self.proc.poll() is None:
                os.killpg(os.getpgrp(), signal.SIGTERM)
                try:
                    self.await_exit(min(deadline - .75, time.monotonic() + 1), cleanup=True)
                except ResolverError:
                    pass
            live = [p for p in _members(deadline - .5)
                    if p['pid'] != os.getpid() and p['state'] != 'Z']
            for process in live:
                fresh = next((p for p in _members(deadline - .5)
                              if p['pid'] == process['pid'] and p['ticks'] == process['ticks']), None)
                if fresh:
                    try:
                        os.kill(process['pid'], signal.SIGKILL)
                    except ProcessLookupError:
                        pass
            # Always collect the direct launcher through Popen, retaining its status.
            self.await_exit(deadline - .25, cleanup=True)
            while time.monotonic() < deadline - .1:
                children = [p for p in _members(deadline - .1)
                            if p['pid'] != os.getpid() and p['ppid'] == os.getpid()]
                for process in children:
                    try:
                        pid, status = os.waitpid(process['pid'], os.WNOHANG)
                        if pid:
                            self.reaped.append(dict(pid=pid, wait_status=status))
                    except ChildProcessError:
                        pass
                self.remaining = [p for p in _members(deadline - .1) if p['pid'] != os.getpid()]
                if not self.remaining:
                    break
                time.sleep(min(.01, max(0, deadline - .1 - time.monotonic())))
            if self.remaining:
                self.cleanup_errors.append('OWNED_GROUP_REMAINS')
            if self.stdout:
                self.cleanup_errors.append('UNASSIGNED_STDOUT')
        except BaseException as exc:
            self.cleanup_errors.append(error_code(exc))
        finally:
            for stream in (self.proc.stdin, self.proc.stdout, self.proc.stderr):
                try:
                    stream.close()
                except OSError:
                    self.cleanup_errors.append('PIPE_CLOSE')
            self.selector.close()

    def result(self):
        return dict(pid=self.proc.pid if self.proc else None, pgid=self.server_pgid,
                    exit=self.proc.returncode if self.proc else None,
                    stdout_eof=self.eof['stdout'], stderr_eof=self.eof['stderr'],
                    remaining=[p['pid'] for p in self.remaining[:16]],
                    cleanup_errors=self.cleanup_errors[:8],
                    reaped_orphans=self.reaped[:16])


def adapter(request, expected_sha):
    # Loading the frozen request is also charged to the parent's first command clock.
    data = _regular(request, time.monotonic() + COMMAND_SECONDS)
    if _sha(data) != expected_sha:
        raise ResolverError('REQUEST_HASH')
    plan = _decode(data)
    validate_plan(plan)
    ack_fd = None
    reaper = server = None
    error = None
    cancelled = [False]
    previous = {sig: signal.getsignal(sig) for sig in (signal.SIGTERM, signal.SIGINT)}
    def cancel(_signum, _frame):
        cancelled[0] = True
    for sig in previous:
        signal.signal(sig, cancel)
    for fd in (1, 2):
        os.set_blocking(fd, False)
    try:
        _check(min(plan['started'] + COMMAND_SECONDS, plan['end'] - CLEANUP_SECONDS) - CLEANUP_SECONDS)
        if os.getpgrp() != os.getpid():
            raise ResolverError('OWNED_PGID_REQUIRED')
        ack_fd = os.open(plan['ack']['path'], os.O_RDONLY | os.O_CLOEXEC | os.O_NONBLOCK)
        identity = os.fstat(ack_fd)
        if (not stat.S_ISFIFO(identity.st_mode)
                or (identity.st_dev, identity.st_ino) != (plan['ack']['device'], plan['ack']['inode'])):
            raise ResolverError('ACK_IDENTITY')
        reaper = Subreaper()
        server = Server(plan, ack_fd, cancelled)
        server.spawn()
        for index in range(len(plan['records'])):
            started = plan['started'] if index == 0 else time.monotonic()
            server.query(index, started)
        server.check(server.work_end)
        server.send(b'["shutdown"]\0', server.work_end)
        server.shutdown = True
        server.await_exit(server.work_end)
        if server.proc.returncode != 0:
            raise ResolverError('SERVER_EXIT')
        if server.stdout:
            raise ResolverError('UNASSIGNED_STDOUT')
    except BaseException as exc:
        error = error_code(exc)
    finally:
        if server:
            server.finish(min(server.end, server.work_end + CLEANUP_SECONDS))
            if server.cleanup_errors:
                error = error or 'SERVER_CLEANUP'
        if reaper:
            try:
                reaper.close()
            except BaseException as exc:
                error = error or error_code(exc)
        if ack_fd is not None:
            try:
                os.close(ack_fd)
            except OSError:
                error = error or 'ACK_CLOSE'
        if server:
            try:
                server.emit(dict(kind='terminal', error=error, index=server.index,
                                 completed=server.completed, acks=server.acks,
                                 shutdown=server.shutdown, server=server.result()),
                            min(server.end, server.work_end + CLEANUP_SECONDS), terminal=True, cleanup=True)
            except BaseException:
                error = error or 'TERMINAL_WRITE'
        for sig, handler in previous.items():
            signal.signal(sig, handler)
    return 0 if error is None else 1


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--request', type=Path, required=True)
    parser.add_argument('--request-sha256', required=True)
    args = parser.parse_args(argv)
    try:
        return adapter(args.request, args.request_sha256)
    except BaseException:
        # No traceback, environment or unvalidated response reaches the evidence.
        return 1


if __name__ == '__main__':
    raise SystemExit(main())
