"""Allowlisted APT observations. Never persist an unrecognised debug payload.

Worker framing: Debian/apt f9f65657ec84ac930c9d9af183913a5ae063bf0f
(Noble 2.8.1), acquire-worker.cc and contrib/{strutl,configuration}.cc.
The actual runner version is recorded separately; unknown formats stay unknown.
"""
import hashlib
from itertools import islice
import json
import os
from pathlib import Path
import re
import stat
import subprocess
import sys
import time
from urllib.parse import unquote_to_bytes, urlsplit

MAX_LINE = 256 * 1024
TOKEN = r'[A-Za-z0-9][A-Za-z0-9.+:_%~/-]{0,239}'
EVENTS = {102: 'Status', 103: 'Redirect', 200: 'URI Start', 201: 'URI Done',
          400: 'URI Failure', 600: 'URI Acquire', 601: 'Configuration'}


def decode(value):
    if re.search(r'%(?![0-9a-fA-F]{2})', value):
        raise ValueError('invalid percent encoding')
    return unquote_to_bytes(value).decode('utf-8', errors='strict')


def uri(value):
    """No userinfo/query/fragment or raw fallback, even for malformed URIs."""
    try:
        if len(value) > 8192 or re.search(r'[\x00-\x20\x7f]', value):
            return {'status': 'UNKNOWN_URI'}
        parsed = urlsplit(value)
        if parsed.scheme not in ('http', 'https', 'mirror+http', 'mirror+https', 'mirror+file', 'file'):
            return {'status': 'UNKNOWN_URI'}
        result = {'scheme': parsed.scheme}
        if parsed.hostname:
            if not re.fullmatch(r'[A-Za-z0-9.:-]{1,253}', parsed.hostname):
                return {'status': 'UNKNOWN_URI'}
            result.update(host=parsed.hostname, port=parsed.port)
        elif parsed.scheme not in ('file', 'mirror+file'):
            return {'status': 'UNKNOWN_URI'}
        # Paths aid correlation, but only public archive/index path shapes survive.
        if re.fullmatch(r'/(?:ubuntu|debian|dists|pool)(?:/[A-Za-z0-9._+:%~/-]*)?', parsed.path):
            result['path'] = parsed.path
        elif parsed.path.startswith('/etc/apt/apt-mirrors.txt'):
            result['path'] = '/etc/apt/apt-mirrors.txt'
        else:
            result['path_sha256'] = hashlib.sha256(parsed.path.encode()).hexdigest()
        return result
    except (ValueError, UnicodeError):
        return {'status': 'UNKNOWN_URI'}


def message_classes(value):
    low = value.lower()
    rules = (('timeout', ('timed out', 'timeout')), ('dns', ('resolv', 'name resolution')),
             ('connection_refused', ('connection refused',)), ('tls', ('tls', 'certificate', 'ssl')),
             ('connecting', ('connecting to',)), ('waiting_headers', ('waiting for headers',)),
             ('connected', ('connected to',)), ('hash_mismatch', ('hash sum mismatch',)),
             ('missing_module', ('not found in the pkg-config', 'no package')))
    return [name for name, needles in rules if any(needle in low for needle in needles)]


def config_projection(fields):
    values = {}
    for name, item in fields:
        if name != 'Config-Item' or '=' not in item:
            continue
        key, value = (decode(part) for part in item.split('=', 1))
        key = key.lower()
        if re.fullmatch(r'acquire::(?:retries|forceipv[46]|queue-mode|brokenproxy)', key) or re.fullmatch(
                r'acquire::(?:http|https)::(?:[a-z0-9.-]{1,253}::)?(?:timeout|pipeline-depth|dl-limit|no-cache)', key):
            if re.fullmatch(r'[0-9]{1,10}|true|false|host|access', value, re.I):
                values[key] = value
        elif re.fullmatch(r'acquire::(?:http|https)::proxy(?:::[a-z0-9.-]{1,253})?', key):
            values[key] = {'direct': True} if value == 'DIRECT' else uri(value)
        elif key in ('acquire::http::proxy-auto-detect', 'acquire::https::proxy-auto-detect'):
            values[key] = {'configured': bool(value)}
        if len(values) > 128:
            raise ValueError('configuration projection limit')
    return values


def project(line, phase):
    try:
        text = line.decode('utf-8', errors='strict').rstrip('\r\n')
        worker = re.fullmatch(r'\s*(<-|->) ([a-z0-9+.-]{1,40}):(.*)', text)
        if worker:
            direction, method, encoded = worker.groups()
            parts = decode(encoded).splitlines()
            header = re.fullmatch(r'([0-9]{3}) (.+)', parts[0]) if parts else None
            if not header or EVENTS.get(int(header[1])) != header[2]:
                return {'kind': 'UNKNOWN_FRAME'}
            code = int(header[1]); fields = []
            for part in parts[1:]:
                if not part:
                    continue
                if part.startswith(' ') and fields:
                    name, value = fields[-1]; fields[-1] = (name, value + '\n' + part[1:])
                elif ': ' in part:
                    fields.append(tuple(part.split(': ', 1)))
                else:
                    return {'kind': 'UNKNOWN_FRAME'}
            result = {'kind': 'WORKER', 'code': code, 'name': EVENTS[code],
                      'direction': direction, 'method': method}
            if code == 601:
                result['config'] = config_projection(fields)
                return result
            for name, value in fields:
                if name in ('URI', 'New-URI', 'UsedMirror'):
                    result[name] = uri(value)
                elif name in ('Size', 'Resume-Point') and re.fullmatch(r'[0-9]{1,20}', value):
                    result[name] = int(value)
                elif name in ('Transient-Failure', 'IMS-Hit') and value in ('true', 'false'):
                    result[name] = value == 'true'
                elif name == 'FailReason':
                    known = ('Timeout', 'ConnectionRefused', 'ConnectionTimedOut', 'ResolveFailure',
                             'TmpResolveFailure', 'HashSumMismatch', 'MaximumSizeExceeded', 'WrongSize')
                    result[name] = value if value in known or re.fullmatch(r'HttpError[0-9]{3}', value) else 'UNKNOWN'
                    status = re.fullmatch(r'HttpError([1-5][0-9]{2})', value)
                    if status: result['http_status_code'] = int(status[1])
                elif name == 'Message':
                    result['message_classes'] = message_classes(value)
            return result
        progress = re.match(r'^(Get|Hit|Ign|Err):([0-9]+) (\S+)', text)
        if progress:
            return {'kind': 'APT_PROGRESS', 'action': progress[1], 'item': int(progress[2]), 'URI': uri(progress[3])}
        if phase == 'apt-version':
            version = re.fullmatch(r'apt ([0-9][A-Za-z0-9.+:~_-]{0,63}) \(([A-Za-z0-9_-]{1,32})\)', text)
            if version: return {'kind': 'APT_VERSION', 'version': version[1], 'arch': version[2]}
        if phase == 'native-packages':
            package = re.fullmatch('(' + TOKEN + ')\t(' + TOKEN + ')', text)
            if package: return {'kind': 'PACKAGE_VERSION', 'package': package[1], 'version': package[2]}
        if phase == 'native-modules' and re.fullmatch(r'[0-9]+(?:\.[0-9]+){0,4}', text):
            return {'kind': 'MODULE_VERSION', 'version': text}
        installed = re.match(r'^(Unpacking|Setting up) (' + TOKEN + r') \((' + TOKEN + r')\)', text)
        if installed:
            return {'kind': 'DPKG_PROGRESS', 'action': installed[1], 'package': installed[2], 'version': installed[3]}
        return {'kind': 'UNKNOWN_LINE', 'message_classes': message_classes(text)}
    except (ValueError, UnicodeError, IndexError):
        # Do not include exception text: parsers can embed rejected input in it.
        return {'kind': 'UNKNOWN_FORMAT'}


class StreamObserver:
    """Bounded line framing with original stream and projected-file offsets."""
    def __init__(self, phase, poll=None):
        self.phase, self._poll = phase, poll
        self.states = {}; self.counts = {}

    def poll(self, deadline):
        if self._poll: self._poll(deadline)

    def feed(self, stream, chunk, final=False):
        state = self.states.setdefault(stream, {'data': bytearray(), 'length': 0, 'raw_offset': 0,
            'raw_bytes': 0, 'projection_offset': 0, 'hash': hashlib.sha256()})
        output = []
        now, wall = time.monotonic(), time.time()

        def emit():
            event = project(bytes(state['data']), self.phase) if state['length'] <= MAX_LINE else {'kind': 'OVERLONG'}
            key = str(event.get('code', event['kind']))
            self.counts[key] = self.counts.get(key, 0) + 1
            row = {'phase': self.phase, 'stream': stream, 'raw_offset': state['raw_offset'],
                'raw_length': state['length'], 'raw_sha256': state['hash'].hexdigest(),
                'projection_offset': state['projection_offset'],
                'received_first_monotonic': state['first_mono'], 'received_first_wall': state['first_wall'],
                'received_last_monotonic': state['last_mono'], 'received_last_wall': state['last_wall'], 'event': event}
            data = (json.dumps(row, separators=(',', ':')) + '\n').encode()
            output.append(data); state['projection_offset'] += len(data)
            state['raw_offset'] += state['length']; state['length'] = 0
            state['data'].clear(); state['hash'] = hashlib.sha256()

        parts = chunk.split(b'\n')
        for index, part in enumerate(parts):
            terminated = index < len(parts)-1
            if terminated: part += b'\n'
            if not part: continue
            if state['length'] == 0: state.update(first_mono=now, first_wall=wall)
            state.update(last_mono=now, last_wall=wall)
            state['raw_bytes'] += len(part); state['length'] += len(part); state['hash'].update(part)
            if state['length'] <= MAX_LINE: state['data'].extend(part)
            else: state['data'].clear()
            if terminated: emit()
        if final and state['length']: emit()
        return b''.join(output)

    def summary(self):
        return {'format': 'allowlisted-jsonl-v1', 'event_counts': self.counts,
                'raw_bytes': {k: v['raw_bytes'] for k, v in self.states.items()},
                'max_buffered_line_bytes': MAX_LINE,
                'limits': 'Receiver timestamps; projections, not raw logs. Unknown content is discarded. '
                          'Counts include parsed records, not proof all records were persisted after a write failure.'}


def archive_snapshot(paths=None):
    """Fixed directory metadata only in privileged CLI; paths is for local fixtures."""
    paths = paths if paths is not None else (Path('/var/cache/apt/archives'), Path('/var/cache/apt/archives/partial'))
    rows = []
    for path in paths:
        row = {'directory': str(path), 'status': 'OBSERVED', 'files': [], 'non_regular': 0, 'changed': 0}
        try:
            if not stat.S_ISDIR(path.lstat().st_mode) or path.resolve() != path:
                raise OSError('unsafe archive directory')
            with os.scandir(path) as entries:
                for index, entry in enumerate(entries):
                    if index >= 512:
                        row['status'] = 'PARTIAL_LIMIT'; break
                    try:
                        info = entry.stat(follow_symlinks=False)
                        if not stat.S_ISREG(info.st_mode):
                            row['non_regular'] += 1; continue
                        if not re.fullmatch(r'[A-Za-z0-9][A-Za-z0-9.+:_%~-]{0,239}\.deb', entry.name):
                            continue
                        row['files'].append({'name': entry.name, 'size': info.st_size,
                                             'mtime_ns': info.st_mtime_ns, 'inode': info.st_ino})
                    except FileNotFoundError:
                        row['changed'] += 1
            row['files'].sort(key=lambda item: item['name'])
        except OSError:
            row = {'directory': str(path), 'status': 'UNREADABLE'}
        rows.append(row)
    return rows


def source_snapshot(apt=Path('/etc/apt')):
    import codeql_rust_capacity_layout as layout
    paths = [apt/'sources.list', apt/'apt-mirrors.txt']
    parts = apt/'sources.list.d'
    listing_status = 'OBSERVED'
    if parts.is_dir() and parts.resolve() == parts:
        for pattern in ('*.sources', '*.list'):
            selected = sorted(islice(parts.glob(pattern), 33))
            if len(selected) > 32: listing_status = 'PARTIAL_LIMIT'
            paths.extend(selected[:32])
    else:
        listing_status = 'UNREADABLE'
    rows = []
    for path in paths:
        row = {'path': str(path), 'status': 'OBSERVED', 'entries': []}
        try:
            data = layout._read_regular(path, MAX_LINE)
            row['sha256'] = hashlib.sha256(data).hexdigest()
            lines = data.decode('utf-8').splitlines()
            if len(lines) > 1024: row['status'] = 'PARTIAL_LIMIT'
            for line in lines[:1024]:
                line = line.split('#', 1)[0].strip()
                if not line: continue
                if path.name == 'apt-mirrors.txt':
                    row['entries'].append({'URI': uri(line.split()[0])})
                elif ': ' in line and path.suffix == '.sources':
                    name, value = line.split(': ', 1)
                    tokens = value.split()
                    if len(tokens) > 32 and name in ('URIs', 'Types', 'Suites', 'Components', 'Enabled'):
                        row['status'] = 'PARTIAL_LIMIT'
                    if name == 'URIs': row['entries'].append({'URIs': [uri(v) for v in tokens[:32]]})
                    elif name in ('Types', 'Suites', 'Components', 'Enabled'):
                        row['entries'].append({name: [v for v in tokens[:32] if re.fullmatch(TOKEN, v)]})
                elif line.startswith(('deb ', 'deb-src ')):
                    values = [uri(v) for v in line.split() if v.startswith(('http:', 'https:', 'mirror+'))]
                    if len(values) > 32: row['status'] = 'PARTIAL_LIMIT'
                    row['entries'].append({'URIs': values[:32]})
        except (OSError, ValueError, UnicodeError, layout.LayoutError):
            row = {'path': str(path), 'status': 'UNREADABLE'}
        rows.append(row)
    return {'files': rows, 'listing_status': listing_status,
            'limits': 'Allowlisted source fields; no auth/config files. File digest covers the original, not the projection.'}


class NativeObservation:
    def __init__(self, out, env):
        self.out, self.env = out, env
        self.observers = []; self.next_sample = 0; self.samples = 0

    def phase(self, name):
        observer = StreamObserver(name, self.poll)
        self.observers.append(observer)
        return observer

    def poll(self, deadline):
        import codeql_rust_diagnostic_runtime as runtime
        now = time.monotonic()
        if now < self.next_sample or deadline - now < 2:
            return
        self.next_sample = now + 10
        row = {'received_wall': time.time(), 'started_monotonic': now}
        try:
            # This root process accepts no caller path or code. Its own timeout
            # survives an unprivileged observer being stopped by the outer job.
            process = subprocess.run(['sudo', '-n', 'timeout', '--signal=TERM', '--kill-after=.2s', '.8s',
                '/usr/bin/python3', '-I', str(Path(__file__).resolve()), '--archive-snapshot'],
                env=self.env, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, timeout=1.5, check=False)
            if process.returncode or len(process.stdout) > 256*1024:
                row.update(status='UNAVAILABLE', exit=process.returncode)
            else:
                row.update(status='OBSERVED', directories=json.loads(process.stdout))
        except (OSError, subprocess.TimeoutExpired, ValueError):
            row.update(status='UNAVAILABLE')
        row['elapsed'] = time.monotonic()-now
        data = (json.dumps(row, separators=(',', ':'))+'\n').encode()
        if runtime.output_bytes(self.out) + len(data) > runtime.WRITE_CAP:
            raise RuntimeError('combined evidence budget exceeded before archive observation')
        with (self.out/'archives.jsonl').open('ab') as output: output.write(data)
        self.samples += 1

    def summary(self):
        return {'phases': {o.phase: o.summary() for o in self.observers}, 'archive_samples': self.samples,
                'privileged_apt_subtree_cleanup': 'UNKNOWN',
                'configuration': 'Only observed 601 allowlisted values are effective-command evidence.'}


if __name__ == '__main__':
    if sys.argv[1:] != ['--archive-snapshot']:
        raise SystemExit('only the fixed archive snapshot is supported')
    print(json.dumps(archive_snapshot(), separators=(',', ':')))
