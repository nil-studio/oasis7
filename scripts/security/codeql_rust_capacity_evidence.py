"""Bounded, best-effort evidence; these helpers never certify Action cleanup.

The caller supplies one monotonic deadline with its terminal reserve already
removed.  Only small terminal receipts may be written after that deadline.
Captured streams are original bytes, including any prefix left by a limit or
failure; a PARTIAL receipt must not be treated as complete coverage evidence.
"""
import hashlib
import gzip
import json
import os
from pathlib import Path
import re
import stat
import time

import codeql_rust_diagnostic_runtime as runtime


MAX_LOG_FILES = 128
MAX_SCAN_ENTRIES = 8192
MAX_OUTPUT_ENTRIES = 32768
MAX_DIRECTORY_DEPTH = 32
MAX_RELATIVE_PATH_BYTES = 512
MAX_FILE_BYTES = 16 * 1024**2
MAX_CAPTURE_BYTES = 128 * 1024**2
COMPRESS_MIN_BYTES = 16 * 1024**2
MAX_COMPRESSED_FILE_BYTES = 64 * 1024**2
MAX_RAW_FILE_BYTES = 512 * 1024**2
MAX_RAW_CAPTURE_BYTES = 1024**3
MAX_LOG_LINE_BYTES = 1024**2
CHUNK_BYTES = 64 * 1024
MAX_RECEIPT_BYTES = 256 * 1024
MAX_REASON_COUNT = 64
GIT_TIMEOUT_SECONDS = 10
MAX_GATE_OUTPUT_BYTES = 8192
_DIRECTORY_FLAGS = os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW


def _reason(report, reason):
    if reason not in report['reasons']:
        if len(report['reasons']) < MAX_REASON_COUNT:
            report['reasons'].append(reason)
        else:
            report['omitted_reason_count'] = report.get('omitted_reason_count', 0) + 1


def _error(exc):
    return (type(exc).__name__ + ': ' + str(exc))[:1024]


def _absolute(path):
    # abspath normalizes spelling without dereferencing any symbolic links.
    return Path(os.path.abspath(path))


def _open_directory(path):
    """Open every directory component with O_NOFOLLOW, including ancestors."""
    path = _absolute(path)
    fd = os.open(path.anchor, _DIRECTORY_FLAGS)
    try:
        for component in path.parts[1:]:
            next_fd = os.open(component, _DIRECTORY_FLAGS, dir_fd=fd)
            os.close(fd)
            fd = next_fd
        return fd
    except Exception:
        os.close(fd)
        raise


def _child_directory(parent_fd, name, create=False):
    if create:
        try:
            os.mkdir(name, 0o700, dir_fd=parent_fd)
        except FileExistsError:
            pass
    return os.open(name, _DIRECTORY_FLAGS, dir_fd=parent_fd)


def _output_bytes(root_fd):
    """Bounded accounting of our evidence directory, without following links."""
    count = total = 0

    def visit(fd, depth):
        nonlocal count, total
        if depth > MAX_DIRECTORY_DEPTH:
            raise RuntimeError('output accounting directory depth limit')
        with os.scandir(fd) as entries:
            for entry in entries:
                count += 1
                if count > MAX_OUTPUT_ENTRIES:
                    raise RuntimeError('output accounting entry limit')
                info = entry.stat(follow_symlinks=False)
                if stat.S_ISDIR(info.st_mode):
                    child = _child_directory(fd, entry.name)
                    try:
                        visit(child, depth + 1)
                    finally:
                        os.close(child)
                elif stat.S_ISREG(info.st_mode):
                    total += info.st_size
                else:
                    raise RuntimeError('unsafe non-regular evidence entry')
    visit(root_fd, 0)
    return total


def _write_all(fd, data):
    view = memoryview(data)
    while view:
        written = os.write(fd, view)
        if not written:
            raise OSError('zero-byte evidence write')
        view = view[written:]


def _save_terminal(out, name, report):
    """Use reserved bytes, never replace an earlier artifact, never raise."""
    root_fd = target_fd = None
    try:
        root_fd = _open_directory(out)
        data = (json.dumps(report, indent=2) + '\n').encode()
        if len(data) > MAX_RECEIPT_BYTES:
            raise RuntimeError('terminal receipt size limit')
        if _output_bytes(root_fd) + len(data) > runtime.CAP:
            raise RuntimeError('terminal evidence reserve exhausted')
        target_fd = os.open(name, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW,
                            0o600, dir_fd=root_fd)
        _write_all(target_fd, data)
    except Exception as exc:
        report['receipt_error'] = _error(exc)
        _reason(report, 'TERMINAL_RECEIPT_INCOMPLETE')
        if report['status'] in ('COMPLETE', 'PASS'):
            report['status'] = 'PARTIAL'
    finally:
        for fd in (target_fd, root_fd):
            if fd is not None:
                try:
                    os.close(fd)
                except OSError as exc:
                    report['receipt_error'] = _error(exc)
                    _reason(report, 'TERMINAL_RECEIPT_CLOSE_FAILED')
                    report['status'] = 'PARTIAL'
    return report


def _stat(info):
    return {'device': info.st_dev, 'inode': info.st_ino, 'bytes': info.st_size,
            'mtime_ns': info.st_mtime_ns, 'ctime_ns': info.st_ctime_ns}


def _settle_logs(database, end):
    """Observe metadata only; a quiet interval never proves producer exit."""
    started = time.monotonic()
    deadline = min(end - 1, started + 5)
    report = {'status': 'PARTIAL', 'samples': 0, 'scanned_entries': 0,
              'maximum_seconds': 5, 'required_quiet_seconds': 1,
              'producer_exit': 'NOT_OBSERVED', 'reason': 'STABILITY_WINDOW_EXHAUSTED'}

    def snapshot():
        if time.monotonic() >= deadline:
            raise TimeoutError('settlement deadline')
        values = {}
        logs = 0
        def visit(fd, parts=()):
            nonlocal logs
            if len(parts) > MAX_DIRECTORY_DEPTH:
                raise RuntimeError('settlement directory depth limit')
            values['/'.join(parts)] = (stat.S_IFDIR, _stat(os.fstat(fd)))
            with os.scandir(fd) as entries:
                for entry in entries:
                    if time.monotonic() >= deadline:
                        raise TimeoutError('settlement deadline')
                    report['scanned_entries'] += 1
                    if report['scanned_entries'] > MAX_SCAN_ENTRIES:
                        raise RuntimeError('settlement scan limit')
                    relative = '/'.join((*parts, entry.name))
                    if len(json.dumps(relative).encode()) > MAX_RELATIVE_PATH_BYTES:
                        raise RuntimeError('settlement relative path limit')
                    info = entry.stat(follow_symlinks=False)
                    if stat.S_ISDIR(info.st_mode):
                        child = _child_directory(fd, entry.name)
                        try:
                            if _stat(os.fstat(child)) != _stat(info):
                                raise RuntimeError('settlement directory changed')
                            visit(child, (*parts, entry.name))
                        finally:
                            os.close(child)
                    elif stat.S_ISREG(info.st_mode):
                        if entry.name.endswith('.log'):
                            logs += 1
                            if logs > MAX_LOG_FILES:
                                raise RuntimeError('settlement log limit')
                            values[relative] = (stat.S_IFREG, _stat(info))
                    else:
                        raise RuntimeError('settlement unsafe entry')
        fd = _open_directory(database / 'log')
        try:
            visit(fd)
        finally:
            os.close(fd)
        if time.monotonic() >= deadline:
            raise TimeoutError('settlement deadline')
        return values

    try:
        if deadline - started < 1:
            raise TimeoutError('insufficient settlement budget')
        previous = snapshot()
        report['samples'] = 1
        quiet_since = time.monotonic()
        while time.monotonic() < deadline:
            time.sleep(min(0.2, max(0, deadline - time.monotonic())))
            current = snapshot()
            report['samples'] += 1
            if previous.keys() - current.keys():
                raise RuntimeError('settlement entry disappeared')
            for name in previous.keys() & current.keys():
                before, after = previous[name], current[name]
                if (before[0], before[1]['device'], before[1]['inode']) != (after[0], after[1]['device'], after[1]['inode']):
                    raise RuntimeError('settlement path replaced')
            now = time.monotonic()
            if now >= deadline:
                raise TimeoutError('settlement deadline')
            if current != previous:
                quiet_since = now
            elif now - quiet_since >= 1:
                report.update(status='STABLE_WINDOW', reason=None,
                              quiet_seconds=now - quiet_since)
                break
            previous = current
    except Exception as exc:
        report['reason'] = _error(exc)
    report['elapsed_seconds'] = time.monotonic() - started
    return report


def collect_logs(database: Path, out: Path, end: float, *, settle=False) -> dict:
    """Copy a bounded initial-length snapshot of regular database/log/*.log files.

    files[].destination is relative to out.  COMPLETE means all log candidates
    observed by a finished bounded traversal were copied unchanged; it does not
    establish that an external producer has stopped or that its output is valid.
    No failed copy or exhausted budget removes bytes already captured.
    """
    start = time.monotonic()
    report = {'status': 'NOT_REACHED', 'source': str(database / 'log'),
              'files': [], 'reasons': [], 'scanned_entries': 0, 'copied_bytes': 0, 'raw_bytes_read': 0,
              'limits': {'files': MAX_LOG_FILES, 'scan_entries': MAX_SCAN_ENTRIES,
                         'directory_depth': MAX_DIRECTORY_DEPTH,
                         'relative_path_bytes': MAX_RELATIVE_PATH_BYTES,
                         'per_file_bytes': MAX_FILE_BYTES, 'capture_bytes': MAX_CAPTURE_BYTES,
                         'compressed_file_bytes': MAX_COMPRESSED_FILE_BYTES,
                         'raw_file_bytes': MAX_RAW_FILE_BYTES, 'raw_capture_bytes': MAX_RAW_CAPTURE_BYTES,
                         'write_cap_bytes': runtime.WRITE_CAP, 'terminal_cap_bytes': runtime.CAP,
                         'deadline_monotonic': end, 'read_chunk_bytes': CHUNK_BYTES},
              'producer_cleanup': 'NOT_OBSERVED_BY_THIS_HELPER'}
    if settle:
        report['settlement'] = _settle_logs(database, end)
        if report['settlement']['status'] != 'STABLE_WINDOW':
            _reason(report, 'LOG_STABILITY_NOT_OBSERVED')
    root_fd = out_fd = destination_fd = None
    stop = False
    remaining_bytes = 0
    remaining_source_bytes = MAX_RAW_CAPTURE_BYTES

    def limited(reason):
        nonlocal stop
        _reason(report, reason)
        stop = True

    def copy_file(parent_fd, target_parent_fd, name, relative):
        nonlocal remaining_bytes, remaining_source_bytes
        record = {'source': relative, 'destination': 'database-logs/' + relative,
                  'status': 'PARTIAL', 'artifact_created': False, 'copied_bytes': 0, 'reasons': [],
                  'source_ranges': [], 'omitted_ranges': []}
        report['files'].append(record)
        src_fd = dst_fd = None
        digest = hashlib.sha256()
        try:
            src_fd = os.open(name, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK,
                             dir_fd=parent_fd)
            before = os.fstat(src_fd)
            if not stat.S_ISREG(before.st_mode):
                raise RuntimeError('source log is not a regular file')
            record['initial'] = _stat(before)
            compress = (COMPRESS_MIN_BYTES < before.st_size <= MAX_RAW_FILE_BYTES
                        and before.st_size <= remaining_source_bytes)
            planned = min(before.st_size, MAX_FILE_BYTES, remaining_bytes, remaining_source_bytes)
            if not compress and planned < min(before.st_size, MAX_FILE_BYTES, remaining_bytes):
                record['reasons'].append('RAW_CAPTURE_BYTE_LIMIT')
            if not compress and planned < before.st_size:
                record['reasons'].append('FILE_SIZE_LIMIT' if before.st_size > MAX_FILE_BYTES
                                         and MAX_FILE_BYTES <= remaining_bytes else 'CAPTURE_BYTE_LIMIT')
            if time.monotonic() >= end:
                record['reasons'].append('DEADLINE_EXHAUSTED')
                limited('DEADLINE_EXHAUSTED')
                return
            target_name = name + '.gz' if compress else name
            if compress:
                record.update(encoding='gzip', range_coordinates='decoded_bytes', gzip_mtime=0, compression_level=1,
                              destination='database-logs/' + relative + '.gz')
            dst_fd = os.open(target_name, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW,
                             0o600, dir_fd=target_parent_fd)
            record['artifact_created'] = True
            if compress:
                class Sink:
                    def write(self, data):
                        nonlocal remaining_bytes
                        view = memoryview(data)
                        while view:
                            if time.monotonic() >= end:
                                record['reasons'].append('DEADLINE_EXHAUSTED')
                                limited('DEADLINE_EXHAUSTED')
                                raise TimeoutError('gzip evidence deadline')
                            available = min(remaining_bytes, MAX_COMPRESSED_FILE_BYTES - record['copied_bytes'])
                            if available <= 0:
                                record['reasons'].append('COMPRESSED_SIZE_LIMIT' if remaining_bytes else 'CAPTURE_BYTE_LIMIT')
                                if not remaining_bytes:
                                    limited('CAPTURE_BYTE_LIMIT')
                                raise RuntimeError('gzip stored byte limit')
                            written = os.write(dst_fd, view[:available])
                            if not written:
                                raise OSError('zero-byte gzip write')
                            digest.update(view[:written])
                            record['copied_bytes'] += written
                            report['copied_bytes'] += written
                            remaining_bytes -= written
                            view = view[written:]
                        return len(data)
                raw_hash = hashlib.sha256()
                raw_bytes = 0
                with gzip.GzipFile(filename='', mode='wb', fileobj=Sink(), compresslevel=1, mtime=0) as compressed:
                    while raw_bytes < before.st_size:
                        if time.monotonic() >= end:
                            record['reasons'].append('DEADLINE_EXHAUSTED')
                            limited('DEADLINE_EXHAUSTED')
                            raise TimeoutError('gzip source deadline')
                        data = os.read(src_fd, min(CHUNK_BYTES, before.st_size - raw_bytes))
                        if not data:
                            raise RuntimeError('gzip source short read')
                        raw_hash.update(data)
                        raw_bytes += len(data)
                        remaining_source_bytes -= len(data)
                        report['raw_bytes_read'] += len(data)
                        record['raw_bytes_read'] = raw_bytes
                        compressed.write(data)
                os.fsync(dst_fd)
                if time.monotonic() >= end:
                    record['reasons'].append('DEADLINE_EXHAUSTED')
                    limited('DEADLINE_EXHAUSTED')
                    raise TimeoutError('gzip finalization deadline')
                record.update(raw_bytes=raw_bytes, raw_sha256=raw_hash.hexdigest())
                record['source_ranges'] = [{'source_offset': 0, 'destination_offset': 0,
                                           'length': raw_bytes, 'sha256': raw_hash.hexdigest()}]
            else:
                # Keep disjoint initial-length head/tail ranges under the same cap.
                head = (planned + 1) // 2 if planned < before.st_size else planned
                ranges = [(0, head)]
                if planned > head:
                    ranges.append((before.st_size - (planned - head), planned - head))
                record['source_ranges'] = []
                record['omitted_ranges'] = []
                for offset, length in ranges:
                    os.lseek(src_fd, offset, os.SEEK_SET)
                    segment = {'source_offset': offset, 'destination_offset': record['copied_bytes'],
                               'length': 0, 'sha256': None}
                    record['source_ranges'].append(segment)
                    segment_hash = hashlib.sha256()
                    while segment['length'] < length:
                        if time.monotonic() >= end:
                            record['reasons'].append('DEADLINE_EXHAUSTED')
                            limited('DEADLINE_EXHAUSTED')
                            break
                        data = os.read(src_fd, min(CHUNK_BYTES, length - segment['length']))
                        if not data:
                            record['reasons'].append('SOURCE_SHORT_READ')
                            break
                        remaining_source_bytes -= len(data)
                        report['raw_bytes_read'] += len(data)
                        view = memoryview(data)
                        while view:
                            written = os.write(dst_fd, view)
                            if not written:
                                raise OSError('zero-byte log write')
                            digest.update(view[:written])
                            segment_hash.update(view[:written])
                            segment['sha256'] = segment_hash.hexdigest()
                            segment['length'] += written
                            record['copied_bytes'] += written
                            report['copied_bytes'] += written
                            remaining_bytes -= written
                            view = view[written:]
                    segment['sha256'] = segment_hash.hexdigest()
                    if segment['length'] < length:
                        break
            record['final'] = _stat(os.fstat(src_fd))
            fresh_parent = _open_directory((database / 'log').joinpath(*Path(relative).parts[:-1]))
            try:
                held = os.fstat(parent_fd)
                fresh = os.fstat(fresh_parent)
                if (held.st_dev, held.st_ino) != (fresh.st_dev, fresh.st_ino):
                    raise RuntimeError('source log parent changed during capture')
                current = os.stat(name, dir_fd=fresh_parent, follow_symlinks=False)
            finally:
                os.close(fresh_parent)
            record['path_final'] = _stat(current)
            if record['initial'] != record['final'] or record['final'] != record['path_final']:
                record['reasons'].append('SOURCE_CHANGED_DURING_CAPTURE')
            if stat.S_ISLNK(current.st_mode):
                record['reasons'].append('SOURCE_PATH_BECAME_SYMLINK')
            observed = record.get('raw_bytes') if compress else record['copied_bytes']
            if observed == before.st_size and not record['reasons']:
                record['status'] = 'COMPLETE'
            if remaining_bytes <= 0:
                limited('CAPTURE_BYTE_LIMIT')
            if remaining_source_bytes <= 0:
                limited('RAW_CAPTURE_BYTE_LIMIT')
        except Exception as exc:
            record['error'] = _error(exc)
            record['reasons'].append('LOG_CAPTURE_ERROR')
        finally:
            cursor = 0
            for segment in record.get('source_ranges', []):
                if segment['source_offset'] > cursor:
                    record['omitted_ranges'].append({'offset': cursor, 'length': segment['source_offset'] - cursor})
                cursor = segment['source_offset'] + segment['length']
            if cursor < record.get('initial', {}).get('bytes', cursor):
                record['omitted_ranges'].append({'offset': cursor, 'length': record.get('initial', {}).get('bytes', cursor) - cursor})
            record['sha256'] = digest.hexdigest()
            for fd in (dst_fd, src_fd):
                if fd is not None:
                    try:
                        os.close(fd)
                    except OSError as exc:
                        record['error'] = _error(exc)
                        record['reasons'].append('LOG_CLOSE_ERROR')
                        record['status'] = 'PARTIAL'
            if record['status'] != 'COMPLETE':
                _reason(report, 'INCOMPLETE_LOG_FILE')

    def visit(fd, target_fd, parts=()):
        if len(parts) > MAX_DIRECTORY_DEPTH:
            limited('DIRECTORY_DEPTH_LIMIT')
            return
        with os.scandir(fd) as entries:
            for entry in entries:
                if stop:
                    return
                if time.monotonic() >= end:
                    limited('DEADLINE_EXHAUSTED')
                    return
                report['scanned_entries'] += 1
                if report['scanned_entries'] > MAX_SCAN_ENTRIES:
                    limited('SCAN_ENTRY_LIMIT')
                    return
                relative = '/'.join((*parts, entry.name))
                if len(json.dumps(relative).encode()) > MAX_RELATIVE_PATH_BYTES:
                    _reason(report, 'RELATIVE_PATH_LIMIT')
                    continue
                info = entry.stat(follow_symlinks=False)
                if stat.S_ISLNK(info.st_mode):
                    _reason(report, 'SYMLINK_SKIPPED')
                elif stat.S_ISDIR(info.st_mode):
                    child = target_child = None
                    try:
                        child = _child_directory(fd, entry.name)
                        target_child = _child_directory(target_fd, entry.name, create=True)
                        visit(child, target_child, (*parts, entry.name))
                    except Exception as exc:
                        _reason(report, 'DIRECTORY_CAPTURE_ERROR')
                        report.setdefault('errors', []).append({'source': relative, 'error': _error(exc)})
                        if len(report['errors']) >= MAX_REASON_COUNT:
                            limited('DIRECTORY_ERROR_LIMIT')
                    finally:
                        for child_fd in (target_child, child):
                            if child_fd is not None:
                                os.close(child_fd)
                elif entry.name.endswith('.log'):
                    if len(report['files']) >= MAX_LOG_FILES:
                        limited('LOG_FILE_LIMIT')
                        return
                    if not stat.S_ISREG(info.st_mode):
                        _reason(report, 'NON_REGULAR_LOG_SKIPPED')
                        continue
                    copy_file(fd, target_fd, entry.name, relative)

    try:
        if time.monotonic() >= end:
            _reason(report, 'DEADLINE_EXHAUSTED')
        else:
            out_fd = _open_directory(out)
            remaining_bytes = min(MAX_CAPTURE_BYTES, max(0, runtime.WRITE_CAP - _output_bytes(out_fd)))
            if remaining_bytes <= 0:
                _reason(report, 'CAPTURE_BYTE_LIMIT')
            else:
                root_fd = _open_directory(database / 'log')
                destination_fd = _child_directory(out_fd, 'database-logs', create=True)
                report['status'] = 'PARTIAL'
                visit(root_fd, destination_fd)
                if not report['reasons']:
                    report['status'] = 'COMPLETE'
    except FileNotFoundError as exc:
        report['status'] = 'UNAVAILABLE'
        report['error'] = _error(exc)
        _reason(report, 'LOG_DIRECTORY_UNAVAILABLE')
    except Exception as exc:
        report['status'] = 'PARTIAL'
        report['error'] = _error(exc)
        _reason(report, 'LOG_CAPTURE_ERROR')
    finally:
        for fd in (destination_fd, root_fd, out_fd):
            if fd is not None:
                try:
                    os.close(fd)
                except OSError as exc:
                    report['error'] = _error(exc)
                    _reason(report, 'DIRECTORY_CLOSE_ERROR')
                    report['status'] = 'PARTIAL'
        report['elapsed_seconds'] = time.monotonic() - start
    return _save_terminal(out, 'database-log-capture.json', report)



class CompressedLogText:
    """Reiterable verified lines; every traversal validates both byte identities."""
    def __init__(self, path, record, end):
        self.path, self.record, self.end = path, record, end

    def __iter__(self):
        record = self.record
        if record.get('status') != 'COMPLETE' or record.get('encoding') != 'gzip':
            raise RuntimeError('partial/unknown gzip is not a complete text stream')
        expected = record.get('raw_bytes')
        if type(expected) is not int or not 0 <= expected <= MAX_RAW_FILE_BYTES:
            raise RuntimeError('gzip raw size bound')
        def check():
            if time.monotonic() >= self.end:
                raise TimeoutError('gzip evidence reader deadline')
        check()
        parent = _open_directory(self.path.parent)
        try:
            parent_info = os.fstat(parent)
            fd = os.open(self.path.name, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK, dir_fd=parent)
        finally:
            os.close(parent)
        with os.fdopen(fd, 'rb') as stream:
            before = os.fstat(stream.fileno())
            if not stat.S_ISREG(before.st_mode) or before.st_size > MAX_COMPRESSED_FILE_BYTES:
                raise RuntimeError('gzip stored file type/size bound')
            if before.st_size != record.get('copied_bytes'):
                raise RuntimeError('gzip stored length mismatch')
            stored_hash = hashlib.sha256()
            while True:
                check()
                chunk = stream.read(CHUNK_BYTES)
                if not chunk:
                    break
                stored_hash.update(chunk)
            if stored_hash.hexdigest() != record.get('sha256'):
                raise RuntimeError('gzip stored hash mismatch')
            stream.seek(0)
            raw_hash = hashlib.sha256()
            count = 0
            pending = b''
            with gzip.GzipFile(fileobj=stream, mode='rb') as decoded:
                while True:
                    check()
                    chunk = decoded.read(min(CHUNK_BYTES, expected - count + 1))
                    if not chunk:
                        break
                    count += len(chunk)
                    if count > expected:
                        raise RuntimeError('gzip expanded length exceeds declared bound')
                    raw_hash.update(chunk)
                    pending += chunk
                    lines = pending.split(b'\n')
                    pending = lines.pop()
                    for line in lines:
                        if len(line) > MAX_LOG_LINE_BYTES:
                            raise RuntimeError('gzip log line bound')
                        yield line.decode(errors='replace').rstrip('\r')
                    if len(pending) > MAX_LOG_LINE_BYTES:
                        raise RuntimeError('gzip log line bound')
            # Ignore an unterminated final line, but include all bytes in SHA.
            if count != expected or raw_hash.hexdigest() != record.get('raw_sha256'):
                raise RuntimeError('gzip original length/hash mismatch')
            after = os.fstat(stream.fileno())
            parent = _open_directory(self.path.parent)
            try:
                final_parent = os.fstat(parent)
                if (parent_info.st_dev, parent_info.st_ino) != (final_parent.st_dev, final_parent.st_ino):
                    raise RuntimeError('gzip artifact parent changed')
                current = os.stat(self.path.name, dir_fd=parent, follow_symlinks=False)
            finally:
                os.close(parent)
            if _stat(before) != _stat(after) or _stat(after) != _stat(current):
                raise RuntimeError('gzip artifact changed during verification')
            check()


def _raw_stream(out, name, read_limit=0):
    """Return a raw-artifact reference; optional gate reads are small and exact."""
    metadata = {'path': name, 'bytes': None, 'status': 'UNAVAILABLE'}
    fd = root_fd = None
    data = None
    try:
        root_fd = _open_directory(out)
        fd = os.open(name, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK, dir_fd=root_fd)
        info = os.fstat(fd)
        if not stat.S_ISREG(info.st_mode):
            raise RuntimeError('stream artifact is not regular')
        metadata.update(bytes=info.st_size, status='CAPTURED')
        if read_limit and info.st_size <= read_limit:
            data = b''
            while len(data) < info.st_size:
                chunk = os.read(fd, info.st_size - len(data))
                if not chunk:
                    raise RuntimeError('stream artifact short read')
                data += chunk
            if _stat(info) != _stat(os.fstat(fd)):
                raise RuntimeError('stream artifact changed during gate read')
    except Exception as exc:
        metadata.update(status='UNAVAILABLE', error=_error(exc))
        data = None
    finally:
        for item in (fd, root_fd):
            if item is not None:
                try:
                    os.close(item)
                except OSError as exc:
                    metadata.update(status='UNAVAILABLE', error=_error(exc))
                    data = None
    return metadata, data


def _lock_record(source, name, end, *, allow_absent=False, maximum=1024**2):
    record = {'path': name, 'status': 'UNKNOWN', 'origin': 'UNKNOWN',
              'initial_presence': 'UNKNOWN', 'dependency_resolution_impact': 'UNKNOWN'}
    fd = parent = None
    try:
        parts = Path(name).parts
        if (not parts or Path(name).is_absolute() or any(x in ('.', '..') for x in parts)
                or len(os.fsencode(name)) > MAX_RELATIVE_PATH_BYTES):
            raise RuntimeError('unsafe lock path')
        parent_path = source.joinpath(*parts[:-1])
        parent = _open_directory(parent_path)
        parent_info = os.fstat(parent)
        try:
            fd = os.open(parts[-1], os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK, dir_fd=parent)
        except FileNotFoundError:
            if allow_absent:
                fresh = _open_directory(parent_path)
                try:
                    current_parent = os.fstat(fresh)
                    if (parent_info.st_dev, parent_info.st_ino) != (current_parent.st_dev, current_parent.st_ino):
                        raise RuntimeError('lock parent changed during observation')
                    try:
                        os.stat(parts[-1], dir_fd=fresh, follow_symlinks=False)
                    except FileNotFoundError:
                        record['status'] = 'ABSENT'
                        return record
                    raise RuntimeError('lock appeared during absence observation')
                finally:
                    os.close(fresh)
            raise
        before = os.fstat(fd)
        if not stat.S_ISREG(before.st_mode) or before.st_size > maximum:
            raise RuntimeError('lock is not a bounded regular file')
        digest = hashlib.sha256()
        size = 0
        while size < before.st_size:
            if time.monotonic() >= end:
                raise TimeoutError('lock evidence deadline')
            data = os.read(fd, min(CHUNK_BYTES, before.st_size - size))
            if not data:
                raise RuntimeError('short lock read')
            digest.update(data)
            size += len(data)
        after = os.fstat(fd)
        fresh = _open_directory(parent_path)
        try:
            current_parent = os.fstat(fresh)
            if (parent_info.st_dev, parent_info.st_ino) != (current_parent.st_dev, current_parent.st_ino):
                raise RuntimeError('lock parent changed during observation')
            current = os.stat(parts[-1], dir_fd=fresh, follow_symlinks=False)
        finally:
            os.close(fresh)
        if _stat(before) != _stat(after) or _stat(after) != _stat(current):
            raise RuntimeError('lock changed during capture')
        record.update(status='CAPTURED', bytes=size, sha256=digest.hexdigest(), identity=_stat(before))
    except Exception as exc:
        record['error'] = _error(exc)
    finally:
        for descriptor in (fd, parent):
            if descriptor is not None:
                os.close(descriptor)
    return record


def lock_inventory(source, manifests, out, end, expected, *, stage, fixed_deadline):
    """Observe only fixed manifest siblings; no Cargo, traversal or provenance inference."""
    paths = sorted({str(Path(name).with_name('Cargo.lock')) for name in manifests})
    report = {'source': str(source), 'expected_head': expected, 'stage': stage,
              'fixed_job_deadline': fixed_deadline, 'status': 'PARTIAL', 'locks': [],
              'origin': 'UNKNOWN', 'dependency_resolution_impact': 'UNKNOWN'}
    if len(paths) > 256:
        raise RuntimeError('lock candidate count bound')
    remaining = 16 * 1024**2
    for name in paths:
        if time.monotonic() >= end:
            report['error'] = 'lock inventory deadline'
            break
        record = _lock_record(source, name, end, allow_absent=True, maximum=min(1024**2, remaining))
        report['locks'].append(record)
        if record['status'] not in ('CAPTURED', 'ABSENT'):
            break
        remaining -= record.get('bytes', 0)
    if time.monotonic() < end and len(report['locks']) == len(paths) and all(x['status'] in ('CAPTURED', 'ABSENT') for x in report['locks']):
        report['status'] = 'COMPLETE'
    return _save_terminal(out, 'lock-inventory-' + stage + '.json', report)


def describe_status(raw, source, end, initial_locks=None):
    """Supplement raw porcelain; never exempts untracked files from clean gate."""
    result = {'tracked_status_gate': 'UNKNOWN', 'untracked_locks': []}
    if raw is None:
        return result
    lines = raw.decode('utf-8', errors='strict').splitlines()
    if any(len(line) < 3 or line[2] != ' ' for line in lines):
        return result
    result['tracked_status_gate'] = 'FAIL' if any(not line.startswith('?? ') for line in lines) else 'PASS'
    initial = {x['path']: x for x in (initial_locks or {}).get('locks', [])}
    for line in lines[:128]:
        if not line.startswith('?? ') or not (line.endswith('/Cargo.lock') or line == '?? Cargo.lock'):
            continue
        name = line[3:]
        record = _lock_record(source, name, end)
        record['observation'] = 'untracked at this Git status observation; not a producer attribution'
        if name in initial:
            record['initial_presence'] = initial[name]['status']
        result['untracked_locks'].append(record)
    return result


def capture_identity(source: Path, out: Path, end: float, expected: str, *, prefix='checkout', initial_locks=None) -> dict:
    """Independently capture HEAD, status and repository root under one budget.

    status is PASS only when identity_gate, clean_gate and repository_gate all
    pass.  A command error leaves its gate UNKNOWN; a known mismatch is FAIL.
    commands[*].stdout/stderr reference unchanged raw files relative to out.
    These process receipts supervise only the helper's own Git process groups.
    """
    if initial_locks is not None and (initial_locks.get('status') != 'COMPLETE' or initial_locks.get('source') != str(source) or initial_locks.get('expected_head') != expected):
        initial_locks = None
    start = time.monotonic()
    report = {'status': 'NOT_REACHED', 'expected_head': expected, 'actual_head': None,
              'identity_gate': 'UNKNOWN', 'clean_gate': 'UNKNOWN', 'repository_gate': 'UNKNOWN',
              'commands': {}, 'reasons': [], 'limits': {'command_seconds': GIT_TIMEOUT_SECONDS,
                'deadline_monotonic': end, 'gate_read_bytes': MAX_GATE_OUTPUT_BYTES},
              'cleanup_scope': 'helper-owned Git process groups only; Action cleanup unobserved'}
    if not isinstance(prefix, str) or not re.fullmatch(r'[a-z][a-z0-9_-]{0,31}', prefix):
        report['reasons'].append('INVALID_ARTIFACT_PREFIX')
        # An invalid name must not select a filesystem destination.
        return report
    report['source'] = str(_absolute(source))
    commands = [('head', ['rev-parse', 'HEAD']),
                ('status', ['status', '--porcelain=v1', '--untracked-files=all']),
                ('top_level', ['rev-parse', '--show-toplevel'])]
    source_fd = None
    try:
        source_fd = _open_directory(source)
        source_error = None
    except Exception as exc:
        source_error = _error(exc)
    finally:
        if source_fd is not None:
            try:
                os.close(source_fd)
            except OSError as exc:
                source_error = _error(exc)
    env = {key: value for key, value in os.environ.items()
           if key not in ('GH_TOKEN', 'GITHUB_TOKEN', 'RUSTC_WRAPPER')}
    env['GIT_OPTIONAL_LOCKS'] = '0'
    for name, arguments in commands:
        stdout_name = prefix + '-' + name + '.stdout'
        result = {'status': 'NOT_REACHED', 'exit': None, 'reasons': [],
                  'stdout': {'path': stdout_name, 'bytes': None, 'status': 'UNAVAILABLE'},
                  'stderr': {'path': stdout_name + '.stderr', 'bytes': None, 'status': 'UNAVAILABLE'},
                  'receipt': stdout_name + '.result.json'}
        report['commands'][name] = result
        root_fd = None
        try:
            if source_error:
                result['reasons'].append('SOURCE_DIRECTORY_UNSAFE_OR_UNAVAILABLE')
                result['error'] = source_error
                continue
            remaining = end - time.monotonic()
            if remaining <= 0:
                result['reasons'].append('DEADLINE_EXHAUSTED')
                continue
            root_fd = _open_directory(out)
            if _output_bytes(root_fd) >= runtime.WRITE_CAP:
                result['reasons'].append('EVIDENCE_BYTE_LIMIT')
                continue
            # Avoid spawning before discovering a previous raw artifact.  run()
            # also uses exclusive file creation, so existing bytes are preserved.
            collision = False
            for filename in (stdout_name, stdout_name + '.stderr', stdout_name + '.samples.jsonl',
                             stdout_name + '.result.json'):
                try:
                    os.stat(filename, dir_fd=root_fd, follow_symlinks=False)
                    collision = True
                except FileNotFoundError:
                    pass
            if collision:
                result['reasons'].append('ARTIFACT_ALREADY_EXISTS')
                continue
            argv = ['git', '--no-optional-locks', '-c', 'core.fsmonitor=false', *arguments]
            process = None
            try:
                process = runtime.run(argv, source, out / stdout_name, end,
                                      min(GIT_TIMEOUT_SECONDS, remaining), out, env, file_limit=None)
            except runtime.PhaseError as exc:
                process = exc.result
            except Exception as exc:
                result['error'] = _error(exc)
            if process is not None:
                for key in ('exit', 'failure_kind', 'error', 'cleanup_errors', 'evidence_errors',
                            'elapsed', 'pid', 'pgid'):
                    result[key] = process.get(key)
            stdout, raw = _raw_stream(out, stdout_name, MAX_GATE_OUTPUT_BYTES)
            stderr, _ = _raw_stream(out, stdout_name + '.stderr')
            result.update(stdout=stdout, stderr=stderr)
            complete = (process is not None and result['exit'] == 0 and not result.get('error')
                        and not result.get('cleanup_errors') and not result.get('evidence_errors')
                        and stdout['status'] == stderr['status'] == 'CAPTURED')
            result['status'] = 'COMPLETE' if complete else 'PARTIAL'
            if not complete:
                result['reasons'].append('GIT_COMMAND_INCOMPLETE')
                continue
            if name == 'status':
                report['clean_gate'] = 'PASS' if stdout['bytes'] == 0 else 'FAIL'
                report.update(describe_status(raw, source, end, initial_locks))
            elif raw is None:
                result['reasons'].append('GATE_OUTPUT_LIMIT')
                result['status'] = 'PARTIAL'
            elif name == 'head':
                actual = raw.decode('utf-8', errors='replace').strip()
                report['actual_head'] = actual
                report['identity_gate'] = 'PASS' if actual == expected else 'FAIL'
            else:
                actual = os.fsdecode(raw.rstrip(b'\r\n'))
                report['actual_repository_root'] = actual
                report['repository_gate'] = ('PASS' if actual and os.path.isabs(actual)
                                             and _absolute(actual) == _absolute(source) else 'FAIL')
        except Exception as exc:
            result['status'] = 'PARTIAL'
            result['error'] = _error(exc)
            result['reasons'].append('IDENTITY_CAPTURE_ERROR')
        finally:
            if root_fd is not None:
                try:
                    os.close(root_fd)
                except OSError as exc:
                    result['error'] = _error(exc)
                    result['status'] = 'PARTIAL'
                    result['reasons'].append('DIRECTORY_CLOSE_ERROR')
            for reason in result['reasons']:
                _reason(report, name + ': ' + reason)
    gates = [report[key] for key in ('identity_gate', 'clean_gate', 'repository_gate')]
    if gates == ['PASS', 'PASS', 'PASS'] and not report['reasons']:
        report['status'] = 'PASS'
    elif 'FAIL' in gates:
        report['status'] = 'FAIL'
    elif any(item['status'] != 'NOT_REACHED' for item in report['commands'].values()):
        report['status'] = 'PARTIAL'
    report['elapsed_seconds'] = time.monotonic() - start
    return _save_terminal(out, prefix + '-identity.json', report)
