#!/usr/bin/env python3
"""One fixed WASM Cargo graph observation; no CodeQL or arbitrary command interface."""
import ctypes
import hashlib
import io
import json
import math
import os
from pathlib import Path, PurePosixPath
import re
import resource
import selectors
import shutil
import signal
import stat
import subprocess
import tarfile
import time
import tomllib
import urllib.parse

import codeql_rust_cargo_guard as guard
import codeql_rust_diagnostic_runtime as runtime

SOURCE = '7c2f99a0ff2fbc3a3523dee7a1cc06fa5bf187ef'
SOURCE_TREE = 'a5fc43fa10637526fdc6b07ec52369c4aa57e588'
TOOLCHAIN = '1.99.0-x86_64-unknown-linux-gnu'
CARGO_COMMIT = '5f94df4789f005f9a352888e8355ffc645b7ed0e'
HOST = 'x86_64-unknown-linux-gnu'
TARGET = 'wasm32-unknown-unknown'
ROLE = 'recovery-fence'
LOCAL_PACKAGES = ('oasis7_custody_worker', 'oasis7_custody_core', 'oasis7_agent_api',
                  'oasis7_client_api', 'oasis7_proto', 'oasis7_wasm_abi')
ROLES = frozenset(('identity', 'owner', 'executor', 'recovery-fence'))
CAP = 64 * 1024**2
ORDINARY = 52 * 1024**2
AUDIT = 8 * 1024**2
TERMINAL = 4 * 1024**2
MAX_STDOUT = 32 * 1024**2
MAX_STREAM = 64 * 1024**2
MAX_SOURCE_BYTES = 512 * 1024**2
MAX_SOURCE_FILES = 20000
MAX_FILE_BYTES = 128 * 1024**2
MAX_PROCESS_RECEIPT = 256 * 1024
PHASES = ('toolchain-install', 'cargo-version', 'rustc-version', 'dependency-fetch',
          'cargo-version-after', 'rustc-version-after', 'discover', 'metadata')
PROCESS_RESERVE = len(PHASES) * MAX_PROCESS_RECEIPT
HELPERS = ('codeql-rust-wasm-metadata.py', 'codeql_rust_cargo_guard.py',
           'codeql_rust_capacity_layout.py', 'codeql_rust_capacity_evidence.py',
           'codeql_rust_diagnostic_runtime.py', 'workflow.yml')
_signal_scope = None


class Rejected(Exception):
    """Only this class name, never exception text, is persisted."""


class Stopped(BaseException):
    pass


class CommandFailed(Exception):
    def __init__(self, receipt):
        self.receipt = receipt
        super().__init__()  # Never embed argv, stderr or repr(receipt).


def interruption(_signum, _frame):
    # Python can schedule a handler after another thread receives a process-wide
    # signal, even while the main thread masks it. Defer repeated cancellation
    # from the first interruption through owned cleanup and receipt persistence.
    if _signal_scope is not None:
        if _signal_scope['interrupted'] or _signal_scope['cleanup']:
            _signal_scope['pending'] = True
            return
        _signal_scope['interrupted'] = True
    raise Stopped()


def encoded(value):
    return (json.dumps(value, indent=2, sort_keys=True, allow_nan=False) + '\n').encode()


def sha256(data):
    return hashlib.sha256(data).hexdigest()


def git_blob(data):
    return hashlib.sha1(b'blob ' + str(len(data)).encode() + b'\0' + data).hexdigest()


def parse_json(data):
    def pairs(items):
        value = {}
        for key, item in items:
            if key in value:
                raise Rejected()
            value[key] = item
        return value
    def invalid(_value):
        raise Rejected()
    return json.loads(data, object_pairs_hook=pairs, parse_constant=invalid)


def text(value, maximum=4096):
    if not isinstance(value, str) or len(value.encode()) > maximum or any(ord(c) < 32 for c in value):
        raise Rejected()
    return value


def name(value):
    value = text(value, 256)
    if not re.fullmatch('[A-Za-z0-9_.+-]+', value):
        raise Rejected()
    return value


def features(values, *, unique=False):
    if not isinstance(values, list) or len(values) > 4096:
        raise Rejected()
    for value in values:
        if not re.fullmatch('[A-Za-z0-9_.+?/:=-]{1,256}', text(value, 256)):
            raise Rejected()
    if unique and len(set(values)) != len(values):
        raise Rejected()
    return sorted(values)


class Clock:
    def __init__(self, receipt):
        self.receipt = receipt
        for key in ('deadline_monotonic', 'deadline_wall', 'prepare_end_monotonic', 'prepare_end_wall'):
            if not isinstance(receipt.get(key), (int, float)) or not math.isfinite(receipt[key]):
                raise Rejected()
        self.end = receipt['deadline_monotonic']
        self.wall = receipt['deadline_wall']
        self.prepare = receipt['prepare_end_monotonic']
        if (receipt.get('maximum_job_seconds') != 1200 or self.end - self.prepare != 420
                or abs(receipt['deadline_wall'] - receipt['prepare_end_wall'] - 420) > .001
                or not 0 < self.remaining() <= 1200):
            raise Rejected()
        self.metadata_end = None

    def remaining(self, end=None):
        cutoff = self.end if end is None else min(self.end, end)
        return min(cutoff - time.monotonic(), self.wall - time.time() - (self.end - cutoff))

    def check(self, end=None):
        if self.remaining(end) <= 0:
            raise Rejected()

    def admit_metadata(self):
        self.check(self.prepare)
        if self.remaining() < 420:
            raise Rejected()
        self.metadata_end = min(time.monotonic() + 300, self.end - 120)
        return self.metadata_end

    def arm(self, end):
        self.check(end)
        signal.setitimer(signal.ITIMER_REAL, self.remaining(end))


class Evidence:
    """A small task-local allocator; guard's separate 8 MiB allowance is reserved."""
    def __init__(self, root):
        self.root = Path(root)
        if self.root.resolve() != self.root or not self.root.is_dir():
            raise Rejected()
        self.terminal_names = set()
        self.sealed = False

    def inventory(self):
        result = {}
        for current, directories, files in os.walk(self.root, followlinks=False):
            for item in directories + files:
                path = Path(current) / item
                if path.is_symlink():
                    raise Rejected()
            for item in files:
                path = Path(current) / item
                if not stat.S_ISREG(path.stat().st_mode):
                    raise Rejected()
                result[path.relative_to(self.root).as_posix()] = path.stat().st_size
                if len(result) > 2300:
                    raise Rejected()
        return result

    def totals(self):
        values = self.inventory()
        audit = sum(n for p, n in values.items() if p.startswith('cargo-guard-calls/'))
        terminal = sum(n for p, n in values.items() if p in self.terminal_names or p == 'cargo-guard.json')
        ordinary = sum(values.values()) - audit - terminal
        if audit > AUDIT or terminal > TERMINAL or ordinary > ORDINARY or sum(values.values()) > CAP:
            raise Rejected()
        return values, ordinary, terminal

    def save(self, filename, value, *, terminal=False):
        if self.sealed or not re.fullmatch(r'[a-z0-9][a-z0-9.-]{0,100}\.json', filename):
            raise Rejected()
        data = encoded(value)
        values, ordinary, final = self.totals()
        old = values.get(filename, 0)
        old_final = filename in self.terminal_names
        ordinary -= old if not old_final else 0
        final -= old if old_final else 0
        if terminal:
            allowed = {'source-after.json', 'helpers-after.json', 'toolchain-after.json', 'proxy-after.json',
                       'toolchain-versions-after.json', 'guard-verdict.json', 'terminal.json', 'sha256.json'}
            if filename not in allowed:
                raise Rejected()
            if filename not in ('source-after.json', 'helpers-after.json', 'sha256.json') and len(data) > 64 * 1024:
                raise Rejected()
            final += len(data)
        else:
            ordinary += len(data)
        if ordinary > ORDINARY or final > TERMINAL or ordinary + final + AUDIT > CAP:
            raise Rejected()
        process_names = {p + '.result.json' for p in PHASES}
        if filename in process_names:
            if terminal or len(data) > MAX_PROCESS_RECEIPT:
                raise Rejected()
        elif not terminal:
            process_bytes = sum(n for p, n in values.items() if p in process_names)
            if ordinary - process_bytes > ORDINARY - PROCESS_RESERVE:
                raise Rejected()
        path = self.root / filename
        if path.exists() and (path.is_symlink() or not path.is_file()):
            raise Rejected()
        path.write_bytes(data)  # All callers construct/validate the approved object first.
        if terminal:
            self.terminal_names.add(filename)
        else:
            self.terminal_names.discard(filename)

    def admit_final(self, before, helper_identity):
        needed = len(encoded({'unchanged': True, 'source': before}))
        needed += len(encoded({'unchanged': True, 'tools': helper_identity}))
        # Source-after is byte-bounded by the frozen before inventory. One MiB
        # covers the other final identities/verdicts and a <=512 KiB seal.
        # All eight process receipts have a separate 2 MiB ordinary reservation.
        needed += 1024 * 1024 + guard.MAX_CONFIG
        if needed > TERMINAL:
            raise Rejected()
        value = {'maximum_terminal_bytes': TERMINAL, 'required_frozen_terminal_bytes': needed,
                 'ordinary_bytes': ORDINARY, 'ordinary_process_reserve_bytes': PROCESS_RESERVE,
                 'guard_audit_bytes': AUDIT, 'total_bytes': CAP}
        self.save('evidence-admission.json', value)
        return value

    def seal(self, clock, end):
        clock.check(end)
        paths, _, _ = self.totals()
        if 'sha256.json' in paths:
            raise Rejected()
        manifest = {}
        for relative in sorted(paths):
            clock.check(end)
            data = (self.root / relative).read_bytes()
            manifest[relative] = {'bytes': len(data), 'sha256': sha256(data)}
        if len(encoded({'files': manifest, 'excludes': ['sha256.json']})) > 512 * 1024:
            raise Rejected()
        self.save('sha256.json', {'files': manifest, 'excludes': ['sha256.json']}, terminal=True)
        actual, _, _ = self.totals()
        if set(actual) != set(manifest) | {'sha256.json'}:
            raise Rejected()
        for relative, expected in manifest.items():
            clock.check(end)
            data = (self.root / relative).read_bytes()
            if len(data) != expected['bytes'] or sha256(data) != expected['sha256']:
                raise Rejected()
        clock.check(end)
        self.sealed = True


def safe_relative(path):
    if not isinstance(path, str) or not path or '\\' in path or '\0' in path:
        raise Rejected()
    pure = PurePosixPath(path)
    if pure.is_absolute() or any(p in ('', '.', '..', '.git') for p in pure.parts) or pure.as_posix() != path:
        raise Rejected()
    if len(path.encode()) > 4096:
        raise Rejected()
    return pure


def verify_git_tree(document, expected):
    """Recompute every recursive Git tree object, not merely trust row hashes."""
    if document.get('sha') != expected or document.get('truncated') is not False:
        raise Rejected()
    rows = document.get('tree')
    if not isinstance(rows, list) or not rows or len(rows) > 30000:
        raise Rejected()
    by_path = {}
    children = {'': []}
    for row in rows:
        path = safe_relative(row.get('path'))
        if str(path) in by_path or not re.fullmatch('[0-9a-f]{40}', row.get('sha', '')):
            raise Rejected()
        if (row.get('type'), row.get('mode')) not in (('tree', '040000'), ('blob', '100644'),
                                                      ('blob', '100755'), ('commit', '160000')):
            raise Rejected()
        if row['type'] == 'blob' and (not isinstance(row.get('size'), int) or not 0 <= row['size'] <= MAX_FILE_BYTES):
            raise Rejected()
        by_path[str(path)] = {k: row[k] for k in ('path', 'type', 'mode', 'sha')}
        if row['type'] == 'blob':
            by_path[str(path)]['size'] = row['size']
        parent = '' if str(path.parent) == '.' else str(path.parent)
        children.setdefault(parent, []).append(by_path[str(path)])
        if row['type'] == 'tree':
            children.setdefault(str(path), [])
    for folder in children:
        if folder and (folder not in by_path or by_path[folder]['type'] != 'tree'):
            raise Rejected()
        payload = bytearray()
        items = sorted(children[folder], key=lambda r: (PurePosixPath(r['path']).name + ('/' if r['type'] == 'tree' else '')).encode())
        for row in items:
            payload.extend(row['mode'].lstrip('0').encode() + b' ' + PurePosixPath(row['path']).name.encode()
                           + b'\0' + bytes.fromhex(row['sha']))
        oid = hashlib.sha1(b'tree ' + str(len(payload)).encode() + b'\0' + payload).hexdigest()
        if oid != (expected if not folder else by_path[folder]['sha']):
            raise Rejected()
    blobs = {p: r for p, r in by_path.items() if r['type'] == 'blob'}
    if len(blobs) > MAX_SOURCE_FILES or sum(r['size'] for r in blobs.values()) > MAX_SOURCE_BYTES:
        raise Rejected()
    return by_path


def unpack_source(archive, tree, destination, clock, end):
    """Only a verified blob can reach the source filesystem; archive bytes stay in memory."""
    destination.mkdir(mode=0o700)
    expected = {p: r for p, r in tree.items() if r['type'] == 'blob'}
    seen = set(); prefix = None; members = 0
    with tarfile.open(fileobj=io.BytesIO(archive), mode='r:gz') as stream:
        for member in stream:
            clock.check(end); members += 1
            if members > 40000:
                raise Rejected()
            pure = safe_relative(member.name.rstrip('/'))
            if prefix is None:
                prefix = pure.parts[0]
            if pure.parts[0] != prefix:
                raise Rejected()
            if len(pure.parts) == 1:
                if not member.isdir():
                    raise Rejected()
                continue
            relative = PurePosixPath(*pure.parts[1:]).as_posix()
            if member.isdir():
                if relative not in tree or tree[relative]['type'] not in ('tree', 'commit'):
                    raise Rejected()
                continue
            if not member.isreg() or relative not in expected or relative in seen:
                raise Rejected()
            row = expected[relative]
            if member.size != row['size'] or bool(member.mode & 0o111) != (row['mode'] == '100755'):
                raise Rejected()
            extracted = stream.extractfile(member)
            if extracted is None:
                raise Rejected()
            data = extracted.read(MAX_FILE_BYTES + 1)
            if len(data) != row['size'] or git_blob(data) != row['sha']:
                raise Rejected()
            target = destination / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            if target.exists() or target.resolve() != target:
                raise Rejected()
            target.write_bytes(data); target.chmod(0o755 if row['mode'] == '100755' else 0o644)
            seen.add(relative)
    if seen != set(expected):
        raise Rejected()
    return snapshot_source(destination, tree, clock, end)


def file_hash(path, clock, end, *, maximum=MAX_FILE_BYTES):
    before = path.stat(follow_symlinks=False)
    if path.is_symlink() or not stat.S_ISREG(before.st_mode) or before.st_size > maximum:
        raise Rejected()
    h = hashlib.sha256(); blob = hashlib.sha1(b'blob ' + str(before.st_size).encode() + b'\0')
    with path.open('rb') as stream:
        while True:
            clock.check(end)
            data = stream.read(1024**2)
            if not data:
                break
            h.update(data); blob.update(data)
    after = path.stat(follow_symlinks=False)
    if (before.st_dev, before.st_ino, before.st_size, before.st_mtime_ns) != (after.st_dev, after.st_ino, after.st_size, after.st_mtime_ns):
        raise Rejected()
    return {'bytes': before.st_size, 'sha256': h.hexdigest(), 'git_blob': blob.hexdigest(),
            'mode': '100755' if before.st_mode & 0o111 else '100644'}


def snapshot_source(root, tree, clock, end):
    expected = {p: r for p, r in tree.items() if r['type'] == 'blob'}
    actual = {}
    for folder, dirs, files in os.walk(root, followlinks=False):
        clock.check(end)
        if any((Path(folder) / p).is_symlink() for p in dirs + files):
            raise Rejected()
        for filename in files:
            path = Path(folder) / filename; relative = path.relative_to(root).as_posix()
            if relative not in expected:
                raise Rejected()
            value = file_hash(path, clock, end); row = expected[relative]
            if value['git_blob'] != row['sha'] or value['bytes'] != row['size'] or value['mode'] != row['mode']:
                raise Rejected()
            actual[relative] = value
    if set(actual) != set(expected):
        raise Rejected()
    return {'source_sha': SOURCE, 'source_tree': SOURCE_TREE, 'files': actual,
            'gitlinks': {p: r['sha'] for p, r in tree.items() if r['type'] == 'commit'},
            'gitlink_contents': 'NOT_MATERIALIZED; not root workspace members'}


def check_helpers(directory, identity, clock, end):
    if set(identity['files']) != set(HELPERS) or not re.fullmatch('[0-9a-f]{40}', identity.get('tool_sha', '')):
        raise Rejected()
    if {p.name for p in directory.iterdir()} != set(HELPERS):
        raise Rejected()
    for filename, expected in identity['files'].items():
        got = file_hash(directory / filename, clock, end, maximum=4 * 1024**2)
        if any(got[key] != expected[key] for key in ('bytes', 'sha256', 'git_blob')):
            raise Rejected()
    return identity


def set_subreaper():
    if os.uname().sysname != 'Linux' or ctypes.CDLL(None, use_errno=True).prctl(36, 1, 0, 0, 0) != 0:
        raise Rejected()


def projected_arguments(argv, semantic):
    allowed = {'manifest', 'cwd', 'toolchain', 'target', 'features', 'no_default_features'}
    if set(semantic) - allowed:
        raise Rejected()
    return {'argument_count': len(argv), 'argv_sha256': sha256(json.dumps(argv).encode()),
            'semantic_arguments': semantic}


def cleanup_group(proc, reap=True):
    errors = []
    for sig, seconds in ((signal.SIGTERM, 1), (signal.SIGKILL, 2)):
        try:
            os.killpg(proc.pid, sig)
        except ProcessLookupError:
            pass
        except BaseException as exc:
            errors.append(type(exc).__name__)
        until = time.monotonic() + seconds
        while True:
            try:
                proc.poll()
                if reap:
                    try:
                        while os.waitpid(-proc.pid, os.WNOHANG)[0] > 0:
                            pass
                    except ChildProcessError:
                        pass
                os.killpg(proc.pid, 0)
            except ProcessLookupError:
                break
            except BaseException as exc:
                errors.append(type(exc).__name__); break
            if time.monotonic() >= until:
                break
            time.sleep(min(.02, until - time.monotonic()))
    try:
        os.killpg(proc.pid, 0)
    except ProcessLookupError:
        pass
    except BaseException as exc:
        errors.append(type(exc).__name__)
    else:
        errors.append('OWNED_GROUP_REMAINS')
    return errors


def run_owned(phase, argv, cwd, env, clock, end, evidence, semantic, *, capture=False, timeout=None):
    """Private supervisor, called only by the fixed phase table below. Never log raw argv."""
    if phase not in PHASES:
        raise Rejected()
    start = time.monotonic()
    stop = min(end, start + timeout) if timeout is not None else end
    clock.check(stop - 4)  # TERM/KILL and receipt work stay inside this phase window.
    record = {'phase': phase, **projected_arguments(argv, semantic), 'started_monotonic': start,
              'stop_monotonic': stop, 'pid': None, 'pgid': None, 'exit': None, 'error_class': None,
              'failure_kind': None, 'cleanup_errors': [], 'streams': {}, 'resource_observation': None}
    global _signal_scope
    previous_scope = _signal_scope
    scope = {'interrupted': False, 'cleanup': False, 'pending': False}
    proc = None; selector = selectors.DefaultSelector(); output = bytearray(); sample = None
    hashes = {k: hashlib.sha256() for k in ('stdout', 'stderr')}; sizes = {k: 0 for k in hashes}
    eof = {k: False for k in hashes}
    names = {}; last_sample = 0; oldmask = None
    signals = {signal.SIGINT, signal.SIGTERM, signal.SIGALRM}

    def consume(fd, *, retain):
        chunk = os.read(fd, 65536)
        kind = names[fd]
        if not chunk:
            eof[kind] = True
            if fd in selector.get_map(): selector.unregister(fd)
            return False
        sizes[kind] += len(chunk); hashes[kind].update(chunk)
        if sizes[kind] > MAX_STREAM or (capture and kind == 'stdout' and sizes[kind] > MAX_STDOUT):
            record['failure_kind'] = 'output_limit'
            raise Rejected()
        if retain and capture and kind == 'stdout': output.extend(chunk)
        return True

    try:
        _signal_scope = scope
        if len(json.dumps(argv).encode()) > 16 * 1024:
            raise Rejected()
        evidence.totals()
        oldmask = signal.pthread_sigmask(signal.SIG_BLOCK, signals)
        def child_limits():
            signal.pthread_sigmask(signal.SIG_SETMASK, oldmask)
            resource.setrlimit(resource.RLIMIT_FSIZE, (1024**3, 1024**3))
        proc = subprocess.Popen(argv, cwd=cwd, env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                start_new_session=True, preexec_fn=child_limits)
        record['pid'] = record['pgid'] = proc.pid
        saved_mask, oldmask = oldmask, None
        signal.pthread_sigmask(signal.SIG_SETMASK, saved_mask)
        for kind, pipe in (('stdout', proc.stdout), ('stderr', proc.stderr)):
            os.set_blocking(pipe.fileno(), False)
            names[pipe.fileno()] = kind; selector.register(pipe.fileno(), selectors.EVENT_READ)
        sample = runtime.GroupSamples(proc.pid)
        while selector.get_map() or proc.poll() is None:
            if clock.remaining(stop - 4) <= 0:
                record['failure_kind'] = 'timeout'; raise Rejected()
            if time.monotonic() - last_sample >= .2:
                sample.sample(); last_sample = time.monotonic()
                evidence.totals()
            for event, _ in selector.select(.05): consume(event.fd, retain=True)
        record['exit'] = proc.wait()
        if record['exit'] != 0:
            record['failure_kind'] = 'child_exit'; raise Rejected()
    except BaseException as exc:
        record['error_class'] = type(exc).__name__
        record['failure_kind'] = record['failure_kind'] or ('cancelled' if isinstance(exc, (Stopped, KeyboardInterrupt)) else 'rejected')
    finally:
        scope['cleanup'] = True
        # Keep deferred cancellation blocked until BOTH owned cleanup and the
        # projected receipt have completed. A pending signal then fails the caller.
        blocked = signal.pthread_sigmask(signal.SIG_BLOCK, signals)
        restore = oldmask if oldmask is not None else blocked
        try:
            if proc is not None:
                record['cleanup_errors'] = cleanup_group(proc)
                record['exit'] = proc.returncode
                # Pipes can receive final TERM-handler bytes. Hash only a bounded
                # tail; incomplete draining is explicitly a prefix observation.
                for pipe in (proc.stdout, proc.stderr):
                    if pipe.fileno() in names:
                        for _ in range(16):
                            try:
                                if not consume(pipe.fileno(), retain=False): break
                            except BlockingIOError:
                                break
                            except Rejected:
                                break
                    pipe.close()
            selector.close()
            record['streams'] = {k: {'bytes': sizes[k], 'sha256': hashes[k].hexdigest(),
                                    'observation': 'EOF_COMPLETE' if eof[k] else 'OBSERVED_PREFIX_ONLY',
                                    'persistence': 'APPROVED_JSON_AFTER_VALIDATION' if capture and k == 'stdout' else 'DIGEST_ONLY'} for k in sizes}
            if sample is not None: record['resource_observation'] = sample.result()
            record['elapsed_seconds'] = time.monotonic() - start
            if len(encoded(record)) > MAX_PROCESS_RECEIPT:
                record['resource_observation'] = {'status': 'UNAVAILABLE_RECEIPT_LIMIT'}
                record['failure_kind'] = 'receipt_limit'; record['error_class'] = 'Rejected'
            if scope['pending']:
                record['failure_kind'] = 'cancelled'; record['error_class'] = 'Stopped'
            evidence.save(phase + '.result.json', record)
        finally:
            try:
                signal.pthread_sigmask(signal.SIG_SETMASK, restore)
            finally:
                _signal_scope = previous_scope
    if scope['pending']:
        record['failure_kind'] = 'cancelled'; record['error_class'] = 'Stopped'
    if record['failure_kind'] or record['cleanup_errors']:
        raise CommandFailed(record)
    return bytes(output), record


def parse_version(data, kind):
    if kind not in ('cargo', 'rustc') or len(data) > 4096:
        raise Rejected()
    lines = data.decode('utf-8').splitlines()
    header = re.fullmatch(r'(cargo|rustc) 1\.99\.0 \(([0-9a-f]{7,40}) ([0-9-]{10})\)', lines[0]) if lines else None
    if header is None or header[1] != kind:
        raise Rejected()
    result = {}; seen = set()
    allowed = {'release', 'commit-hash', 'commit-date', 'host'}
    allowed |= {'binary', 'LLVM version'} if kind == 'rustc' else {'libgit2', 'libcurl', 'ssl', 'os'}
    for line in lines[1:]:
        key, sep, value = line.partition(': ')
        if not sep or key not in allowed or key in seen or len(value) > 512:
            raise Rejected()
        seen.add(key)
        # Auxiliary Cargo library/OS prose is bounded but never copied.
        if key in ('release', 'commit-hash', 'commit-date', 'host', 'binary', 'LLVM version'):
            result[key] = value
    if result.get('release') != '1.99.0' or result.get('host') != HOST or not re.fullmatch('[0-9a-f]{40}', result.get('commit-hash', '')):
        raise Rejected()
    if kind == 'cargo' and result['commit-hash'] != CARGO_COMMIT:
        raise Rejected()
    if kind == 'rustc' and result.get('binary') != 'rustc':
        raise Rejected()
    if (not re.fullmatch('[0-9]{4}-[0-9]{2}-[0-9]{2}', result.get('commit-date', ''))
            or not result['commit-hash'].startswith(header[2]) or result['commit-date'] != header[3]):
        raise Rejected()
    if 'LLVM version' in result and not re.fullmatch('[0-9.]+', result['LLVM version']):
        raise Rejected()
    result['tool'] = kind
    result['history_scope'] = 'MATCHES_RECORDED_CARGO_COMMIT' if kind == 'cargo' else 'EXACT_RUSTC_COMMIT_FIRST_OBSERVED_THIS_RUN'
    return result


def confined_path(value, roots, *, exists=False):
    value = text(value)
    path = Path(value)
    if not path.is_absolute() or path.resolve() != path or not any(path == r or path.is_relative_to(r) for r in roots):
        raise Rejected()
    if exists and not path.is_file():
        raise Rejected()
    return str(path)


def cfg(value):
    if value is None:
        return None
    value = text(value, 2048)
    if not re.fullmatch(r'[A-Za-z0-9_ \t()=",.!&|+-]+', value):
        raise Rejected()
    return value


def source_id(value):
    if value is None:
        return None
    value = text(value, 2048)
    if not value.startswith(('registry+https://', 'git+https://')):
        raise Rejected()
    parsed = urllib.parse.urlsplit(value.split('+', 1)[1])
    if parsed.username or parsed.password or not parsed.hostname or parsed.port not in (None, 443):
        raise Rejected()
    if set(urllib.parse.parse_qs(parsed.query)) - {'rev', 'tag', 'branch'}:
        raise Rejected()
    return value


def package_identifier(value):
    value = text(value)
    if not re.fullmatch(r'[A-Za-z0-9_./+:@?#=%~\[\]-]+', value):
        raise Rejected()
    return value


def metadata_graph(raw, source, cache, target, expected_members, lock_packages):
    """Retain the complete graph, omit package free text/metadata before persistence."""
    document = parse_json(raw)
    if not isinstance(document, dict) or type(document.get('version')) is not int or document['version'] != 1:
        raise Rejected()
    known_top = {'packages', 'workspace_members', 'workspace_default_members', 'resolve', 'target_directory',
                 'build_directory', 'workspace_root', 'metadata', 'version'}
    if set(document) - known_top or document.get('workspace_root') != str(source):
        raise Rejected()
    confined_path(document['target_directory'], [target])
    if document.get('build_directory') is not None:
        confined_path(document['build_directory'], [target])
    package_rows = document.get('packages')
    if not isinstance(package_rows, list) or not 1 <= len(package_rows) <= 10000:
        raise Rejected()
    packages = {}; manifests = set(); roots = [source, cache]
    known_package = {'name', 'version', 'id', 'source', 'dependencies', 'targets', 'features', 'manifest_path',
                     'authors', 'categories', 'default_run', 'description', 'documentation', 'edition', 'homepage',
                     'keywords', 'license', 'license_file', 'links', 'metadata', 'publish', 'readme', 'repository', 'rust_version', 'hints'}
    for row in package_rows:
        if not isinstance(row, dict) or set(row) - known_package:
            raise Rejected()
        pid = package_identifier(row['id']); pkg_name = name(row['name']); version = name(row['version'])
        manifest = confined_path(row['manifest_path'], roots, exists=True)
        if pid in packages or manifest in manifests:
            raise Rejected()
        origin = source_id(row['source'])
        if origin is not None and (pkg_name, version, origin) not in lock_packages:
            raise Rejected()
        if origin is None and not Path(manifest).is_relative_to(source):
            raise Rejected()
        base = 'path+' + Path(manifest).parent.as_uri() if origin is None else origin.split('#', 1)[0]
        permitted_ids = {base + '#' + pkg_name + '@' + version}
        if origin is None and Path(manifest).parent.name == pkg_name:
            permitted_ids.add(base + '#' + version)
        if pid not in permitted_ids:
            raise Rejected()
        declared = row['features']
        if not isinstance(declared, dict) or len(declared) > 4096:
            raise Rejected()
        declared = {name(k): features(v) for k, v in declared.items()}
        dependencies = []
        if not isinstance(row['dependencies'], list) or len(row['dependencies']) > 4096:
            raise Rejected()
        for dep in row['dependencies']:
            if set(dep) - {'name', 'source', 'req', 'kind', 'rename', 'optional', 'uses_default_features', 'features', 'target', 'registry', 'path', 'artifact', 'bindep_target', 'lib'}:
                raise Rejected()
            if dep['kind'] not in (None, 'dev', 'build') or type(dep['optional']) is not bool or type(dep['uses_default_features']) is not bool:
                raise Rejected()
            requirement = text(dep['req'], 256)
            if not re.fullmatch(r'[A-Za-z0-9.*<>=^~+|, -]+', requirement):
                raise Rejected()
            item = {'name': name(dep['name']), 'source': source_id(dep['source']), 'req': requirement,
                    'kind': dep['kind'], 'rename': name(dep['rename']) if dep['rename'] else None,
                    'optional': dep['optional'], 'uses_default_features': dep['uses_default_features'],
                    'features': features(dep['features']), 'target': cfg(dep['target'])}
            if dep.get('path') is not None:
                item['path'] = confined_path(dep['path'], roots)
            # Artifact dependency semantics are not silently discarded.
            if any(dep.get(k) not in (None, False) for k in ('artifact', 'bindep_target', 'lib')):
                raise Rejected()
            dependencies.append(item)
        targets = []
        for obj in row['targets']:
            if set(obj) - {'kind', 'crate_types', 'name', 'src_path', 'edition', 'doc', 'doctest', 'test', 'required-features'}:
                raise Rejected()
            targets.append({'kind': [name(x) for x in obj['kind']], 'crate_types': [name(x) for x in obj['crate_types']],
                            'name': name(obj['name']), 'src_path': confined_path(obj['src_path'], roots),
                            'edition': name(obj['edition']), 'required_features': features(obj.get('required-features', []))})
        packages[pid] = {'id': pid, 'name': pkg_name, 'version': version, 'source': origin, 'manifest_path': manifest,
                         'features': declared, 'dependencies': dependencies, 'targets': targets}
        manifests.add(manifest)
    members = document['workspace_members']; defaults = document['workspace_default_members']
    if not isinstance(members, list) or len(members) != len(set(members)) or any(x not in packages for x in members):
        raise Rejected()
    actual = set()
    for pid in members:
        path = Path(packages[pid]['manifest_path'])
        if not path.is_relative_to(source) or packages[pid]['source'] is not None:
            raise Rejected()
        actual.add(path.parent.relative_to(source).as_posix())
    if not isinstance(defaults, list) or len(defaults) != len(set(defaults)) or not set(defaults) <= set(members):
        raise Rejected()
    resolved = document['resolve']
    if not isinstance(resolved, dict) or set(resolved) != {'root', 'nodes'} or resolved['root'] is not None:
        raise Rejected()
    nodes = {}
    for row in resolved['nodes']:
        if set(row) != {'id', 'dependencies', 'deps', 'features'} or row['id'] not in packages or row['id'] in nodes:
            raise Rejected()
        links = []
        for link in row['deps']:
            if set(link) != {'name', 'pkg', 'dep_kinds'} or link['pkg'] not in packages:
                raise Rejected()
            kinds = []
            for kind in link['dep_kinds']:
                if set(kind) != {'kind', 'target'} or kind['kind'] not in (None, 'dev', 'build'):
                    raise Rejected()
                kinds.append({'kind': kind['kind'], 'target': cfg(kind['target'])})
            if not kinds:
                raise Rejected()
            links.append({'name': name(link['name']), 'pkg': link['pkg'], 'dep_kinds': kinds})
        if len(row['dependencies']) != len(set(row['dependencies'])) or set(row['dependencies']) != {x['pkg'] for x in links}:
            raise Rejected()
        nodes[row['id']] = {'id': row['id'], 'features': features(row['features'], unique=True), 'dependencies': sorted(row['dependencies']), 'deps': links}
    if any(link['pkg'] not in nodes for row in nodes.values() for link in row['deps']) or not set(members) <= set(nodes):
        raise Rejected()
    local = {}; missing_local = []
    for pkg_name in LOCAL_PACKAGES:
        matches = [p for p in packages.values() if p['name'] == pkg_name and p['manifest_path'] == str(source/'crates'/pkg_name/'Cargo.toml')]
        if len(matches) != 1 or matches[0]['source'] is not None or matches[0]['id'] not in nodes:
            missing_local.append(pkg_name)
            continue
        local[pkg_name] = {'id': matches[0]['id'], 'manifest_path': matches[0]['manifest_path'], 'features': nodes[matches[0]['id']]['features']}
    custody_id = local.get('oasis7_custody_worker', {}).get('id')
    chosen = set(nodes[custody_id]['features']) if custody_id is not None else set()
    default_sources = []
    for pid, node in nodes.items():
        for edge in node['deps']:
            if edge['pkg'] != custody_id: continue
            for dep in packages[pid]['dependencies']:
                alias = (dep['rename'] or dep['name']).replace('-', '_')
                actual_kind = {'kind': dep['kind'], 'target': dep['target']}
                if (dep['name'] == 'oasis7_custody_worker' and alias == edge['name']
                        and actual_kind in edge['dep_kinds']
                        and (dep['uses_default_features'] or 'default' in dep['features'])):
                    default_sources.append({'package': pid, 'resolved_dependency_name': edge['name'],
                                            'kind': dep['kind'], 'target': dep['target'],
                                            'uses_default_features': dep['uses_default_features'], 'features': dep['features']})
    reachable = set(); pending = [custody_id] if custody_id is not None else []
    while pending:
        pid = pending.pop()
        if pid not in reachable:
            reachable.add(pid); pending.extend(nodes[pid]['dependencies'])
    checks = {'exact_workspace_member_set': actual == set(expected_members),
              'six_local_identities_present': not missing_local,
              'role_intersection_exact': chosen & ROLES == {ROLE},
              'no_other_custody_features': not (chosen - {'default', ROLE}),
              'empty_default_declaration': custody_id is not None and packages[custody_id]['features'].get('default') == [],
              'default_marker_explained': 'default' not in chosen or bool(default_sources),
              'six_local_packages_reachable': not missing_local and {item['id'] for item in local.values()} <= reachable}
    graph = {'schema': 'oasis7.cargo-graph-projection.v1', 'cargo_format_version': 1,
             'workspace_root': str(source), 'workspace_members': members, 'workspace_default_members': defaults,
             'target_directory': document['target_directory'], 'build_directory': document.get('build_directory'),
             'packages': list(packages.values()), 'resolve': {'root': None, 'nodes': list(nodes.values())},
             'projection': 'Complete package identities/declaration features/dependencies/targets and resolve graph. '
                           'Non-graph free text, package build hints, arbitrary package/workspace metadata, and registry prose omitted before persistence.',
             'stdout_bytes': len(raw), 'stdout_sha256': sha256(raw)}
    observation = {'status': 'ROLE_CONTEXT_OBSERVED' if all(checks.values()) else 'ROLE_CONTEXT_MISMATCH',
                   'checks': checks, 'failed_checks': sorted(k for k, value in checks.items() if not value),
                   'workspace_member_directories': sorted(actual),
                   'missing_workspace_members': sorted(set(expected_members) - actual),
                   'unexpected_workspace_members': sorted(actual - set(expected_members)),
                   'missing_local_packages': missing_local, 'actual_custody_features': sorted(chosen),
                   'local_packages': local, 'role': ROLE, 'target_filter': TARGET, 'default_marker_sources': default_sources,
                   'custody_reachable_package_ids': sorted(reachable), 'custody_reachable_nodes': [nodes[p] for p in sorted(reachable)],
                   'single_crate_isolation': False, 'per_compilation_unit_features': 'NOT_PROVED',
                   'wasm_compilation': 'NOT_RUN', 'proc_macro': 'NOT_RUN', 'codeql': 'NOT_RUN'}
    return graph, observation


def child_environment(environment, work, proxy):
    keep = ('PATH', 'HOME', 'CODEX_HOME', 'USER', 'LOGNAME', 'LANG', 'LC_ALL', 'TMPDIR', 'SSL_CERT_FILE', 'SSL_CERT_DIR')
    result = {k: environment[k] for k in keep if k in environment}
    result.update(PATH=str(proxy.parent) + os.pathsep + environment.get('PATH', ''),
                  CARGO_HOME=str(work/'cargo-home'), RUSTUP_HOME=str(work/'rustup'),
                  CARGO_TARGET_DIR=str(work/'target'), RUSTUP_TOOLCHAIN='1.99.0',
                  CARGO_NET_RETRY='0', CARGO_TERM_COLOR='never', PYTHONDONTWRITEBYTECODE='1')
    if any(result.get(k) != environment.get(k) for k in ('HOME', 'CODEX_HOME')):
        raise Rejected()
    return result


def proxy_identity(environment, work, clock, end):
    cargo = shutil.which('cargo', path=environment.get('PATH'))
    rustup = shutil.which('rustup', path=environment.get('PATH'))
    if not cargo or not rustup:
        raise Rejected()
    cargo, rustup = Path(cargo).absolute(), Path(rustup).absolute()
    if any(p.is_relative_to(work) or not p.is_file() for p in (cargo, rustup)):
        raise Rejected()
    # Host proxies may be rustup symlinks/hardlinks; bind both target and bytes.
    values = {}
    for role, path in (('cargo_proxy', cargo), ('rustup_proxy', rustup)):
        values[role] = {'path': str(path), 'target': str(path.resolve()), **file_hash(path.resolve(), clock, end)}
    if values['cargo_proxy']['sha256'] != values['rustup_proxy']['sha256']:
        raise Rejected()
    return cargo, rustup, values


def analysis_binary_identity(work, clock, end):
    root = work/'rustup/toolchains'/TOOLCHAIN
    result = {kind: {'path': str(root/'bin'/kind), **file_hash(root/'bin'/kind, clock, end)} for kind in ('cargo', 'rustc')}
    result['distribution_manifest'] = file_hash(root/'lib/rustlib/multirust-channel-manifest.toml', clock, end, maximum=8*1024**2)
    return result


def metadata_manifest_hashes(graph, clock, end):
    return {p['id']: {'manifest_path': p['manifest_path'], **file_hash(Path(p['manifest_path']), clock, end, maximum=4*1024**2)}
            for p in graph['packages']}


def install_guard_adapter(home, evidence, identity, clock, end):
    """One task-owned wrapper binding before admission; shared guard stays unchanged."""
    clock.check(end)
    config_path = evidence.root/'cargo-guard.json'; wrapper = home/'bin/cargo'
    config_before = file_hash(config_path, clock, end, maximum=guard.MAX_CONFIG)
    config = parse_json(config_path.read_bytes())
    audit = evidence.root/'cargo-guard-calls'
    if ((audit/'.counter').read_bytes() != b'0000000000000000' or (audit/'.failed').read_bytes() != b'0'
            or list(audit.glob('*.json')) or 'wasm_version_adapter' in config
            or config.get('wrapper') != str(wrapper) or config.get('private_home') != str(home)
            or config.get('source_sha') != SOURCE):
        raise Rejected()
    guard.verify_config(config)
    wrapper_before = file_hash(wrapper, clock, end, maximum=64 * 1024)
    adapter = Path(__file__).resolve()
    adapter_identity = file_hash(adapter, clock, end, maximum=4 * 1024**2)
    expected = identity['files']['codeql-rust-wasm-metadata.py']
    if any(adapter_identity[k] != expected[k] for k in ('bytes', 'sha256', 'git_blob')):
        raise Rejected()
    # Execute exactly the bytes just verified, without a second import-time read.
    stub = ('#!' + __import__('sys').executable + '\n'
            'import hashlib,json,sys,types\nfrom pathlib import Path\n'
            'sys.dont_write_bytecode=True\n'
            'code=101\ntry:\n'
            ' p=Path(' + repr(str(adapter)) + ')\n'
            ' if p.is_symlink() or not p.is_file(): raise RuntimeError()\n'
            ' data=p.read_bytes()\n'
            ' if len(data)!=' + str(expected['bytes']) + ' or hashlib.sha256(data).hexdigest()!=' + repr(expected['sha256']) + ': raise RuntimeError()\n'
            ' sys.path.insert(0,str(p.parent))\n'
            ' m=types.ModuleType("wasm_metadata_guard_adapter");m.__file__=str(p)\n'
            ' exec(compile(data,str(p),"exec"),m.__dict__)\n'
            ' code=m.invoke_guard_adapter(sys.argv[1:],Path(' + repr(str(config_path)) + '))\n'
            'except BaseException as exc:\n'
            ' print(json.dumps({"status":"GUARD_ADAPTER_REJECTED","error_class":type(exc).__name__}),file=sys.stderr)\n'
            'sys.exit(code)\n').encode()
    if len(stub) > 64 * 1024:
        raise Rejected()
    config['wrapper_sha256'] = sha256(stub)
    config['wasm_version_adapter'] = str(adapter)
    config['wasm_version_adapter_sha256'] = expected['sha256']
    raw = encoded(config)
    if len(raw) > guard.MAX_CONFIG:
        raise Rejected()
    clock.check(end)
    for path, data in ((wrapper, stub), (config_path, raw)):
        before = path.stat(follow_symlinks=False)
        if not stat.S_ISREG(before.st_mode) or before.st_uid != os.geteuid() or before.st_nlink != 1:
            raise Rejected()
        with os.fdopen(os.open(path, os.O_WRONLY | os.O_NOFOLLOW | os.O_NONBLOCK), 'wb') as stream:
            actual = os.fstat(stream.fileno())
            if (actual.st_dev, actual.st_ino, actual.st_uid, actual.st_nlink) != (before.st_dev, before.st_ino, os.geteuid(), 1):
                raise Rejected()
            stream.write(data)
            stream.truncate()
    guard.verify_config(config)
    evidence.save('guard-adapter.json', {'scope': 'task-only version first-write projection; original invoke/inspect and real Cargo checks retained',
                  'counter_before': 0, 'failed_before': 0, 'adapter': {'path': str(adapter), **adapter_identity},
                  'wrapper_before': wrapper_before, 'wrapper_after': file_hash(wrapper, clock, end, maximum=64 * 1024),
                  'config_before': config_before, 'config_after': file_hash(config_path, clock, end, maximum=guard.MAX_CONFIG)})


def invoke_guard_adapter(argv, config_path):
    """Called only by the bound wrapper; project the original live version return."""
    config = parse_json(guard.read(config_path, guard.MAX_CONFIG))
    if (config.get('wasm_version_adapter') != str(Path(__file__).resolve())
            or config.get('wasm_version_adapter_sha256') != guard.digest(Path(__file__), config['deadline'])):
        raise Rejected()
    version = guard.cargo_version; finish = guard.finish_record
    observation = {'status': 'NOT_REQUESTED', 'bytes': None, 'sha256': None}

    def projected_version(command, environment, deadline):
        observation.update(status='NOT_RETURNED', bytes=None, sha256=None)
        # On original timeout/overflow/exit/UTF-8 failure no complete return is
        # available. Keep UNKNOWN counts/digest; preserve its original exception.
        returned = version(command, environment, deadline)
        raw = returned.encode('utf-8')
        observation.update(status='REJECTED_BEFORE_WRITE', bytes=len(raw), sha256=sha256(raw))
        parsed = parse_version(raw, 'cargo')
        normalized = ('cargo 1.99.0 (' + parsed['commit-hash'][:9] + ' ' + parsed['commit-date'] + ')\n'
                      + ''.join(key + ': ' + parsed[key] + '\n' for key in ('release', 'commit-hash', 'commit-date', 'host')))
        observation['status'] = 'VALIDATED_PROJECTED'
        return normalized

    def projected_finish(path, record):
        if 'wasm_version_projection' in record:
            raise Rejected()
        # Add only these three fields. Original status/error/identity stay intact.
        return finish(path, dict(record, wasm_version_projection=dict(observation)))

    guard.cargo_version, guard.finish_record = projected_version, projected_finish
    try:
        return guard.invoke(argv, config_path)
    finally:
        guard.cargo_version, guard.finish_record = version, finish


def inspect_guard(evidence, deadline_wall, end, source, expected_argv):
    verdict = guard.inspect(evidence.root, deadline_wall, SOURCE, end=end)
    receipts = [parse_json(p.read_bytes()) for p in (evidence.root/'cargo-guard-calls').glob('*.json')]
    if (verdict['status'] != 'PASS' or verdict['calls'] != 2 or verdict['full_metadata_calls'] != 1
            or {r.get('operation') for r in receipts} != {'locate-project', 'metadata'}):
        raise Rejected()
    for receipt in receipts:
        operation = receipt['operation']
        if (receipt['argv_sha256'] != sha256(json.dumps(expected_argv[operation]).encode())
                or receipt.get('cwd_sha256') != sha256(os.fsencode(source/'crates/oasis7_custody_worker'))
                or receipt.get('manifest_path_sha256') != sha256(os.fsencode(source/('Cargo.toml' if operation == 'metadata' else 'crates/oasis7_custody_worker/Cargo.toml')))
                or receipt.get('rustup_toolchain') != '1.99.0'):
            raise Rejected()
        if operation == 'metadata':
            parse_version(receipt['actual_cargo_version'].encode(), 'cargo')
            projection = receipt.get('wasm_version_projection', {})
            if (projection.get('status') != 'VALIDATED_PROJECTED' or type(projection.get('bytes')) is not int
                    or not 0 < projection['bytes'] <= 2048 or not re.fullmatch('[0-9a-f]{64}', projection.get('sha256', ''))):
                raise Rejected()
    return verdict


def diagnose(session, clock_receipt, tools_identity, api, environment):
    """Single production call path. No CLI switches or reusable fetch/evaluation mode."""
    session = Path(session); out = session/'evidence'; work = session/'work'; source = work/'source'
    evidence = Evidence(out); clock = Clock(clock_receipt)
    terminal = {'status': 'METADATA_INCOMPLETE', 'phase': 'admission', 'error_class': None,
                'metadata_observation': 'NOT_RUN', 'evidence': 'INCOMPLETE', 'platform_completion': 'PENDING_EXTERNAL_READBACK',
                'production_capacity': 'UNKNOWN', 'full_rust_coverage': 'HOLD', 'codeql': 'NOT_RUN',
                'job_deadline_monotonic': clock.end, 'prepare_deadline_monotonic': clock.prepare,
                'metadata_deadline_monotonic': None, 'source_sha': SOURCE, 'tool_sha': tools_identity['tool_sha']}
    before = tree = binary_before = proxy_before = graph = manifests_before = None
    guard_installed = False; guard_deadline = None; expected_args = {}
    try:
        for sig in (signal.SIGALRM, signal.SIGINT, signal.SIGTERM):
            signal.signal(sig, interruption)
        clock.arm(clock.prepare); set_subreaper()
        if (session.resolve() != session or tools_identity['tool_sha'] != clock_receipt['tool_sha']
                or tools_identity['tool_sha'] != environment['GITHUB_SHA']):
            raise Rejected()
        check_helpers(session/'tools', tools_identity, clock, clock.prepare)
        work.mkdir(mode=0o700)
        terminal['phase'] = 'source-preparation'
        raw_tree, _ = api.get('source-tree', SOURCE_TREE)
        tree = verify_git_tree(raw_tree, SOURCE_TREE)
        archive = api.archive()
        archive_identity = {'bytes': len(archive), 'sha256': sha256(archive), 'source_tree': SOURCE_TREE,
                            'source_sha': SOURCE, 'transport': 'fixed GitHub archive; temporary redirect not persisted'}
        before = unpack_source(archive, tree, source, clock, clock.prepare); del archive
        evidence.save('source-before.json', before)
        evidence.save('source-archive.json', archive_identity)
        evidence.admit_final(before, tools_identity)
        root_manifest = tomllib.loads((source/'Cargo.toml').read_text())
        members = sorted(set(root_manifest['workspace']['members']))
        if len(members) != 53 or 'package' in root_manifest:
            raise Rejected()
        for member in members:
            safe_relative(member)
            if not (source/member/'Cargo.toml').is_file():
                raise Rejected()
        for pkg in LOCAL_PACKAGES:
            manifest = source/'crates'/pkg/'Cargo.toml'
            if tomllib.loads(manifest.read_text())['package']['name'] != pkg:
                raise Rejected()
        lock_data = tomllib.loads((source/'Cargo.lock').read_text())
        lock_packages = {(p['name'], p['version'], p.get('source')) for p in lock_data['package']}
        cargo, rustup, proxy_before = proxy_identity(environment, work, clock, clock.prepare)
        env = child_environment(environment, work, cargo)
        for folder in ('cargo-home', 'rustup', 'target'):
            (work/folder).mkdir(mode=0o700)
        evidence.save('preparation-identity.json', {'proxy': proxy_before, 'cargo_home': env['CARGO_HOME'],
                      'rustup_home': env['RUSTUP_HOME'], 'target_directory': env['CARGO_TARGET_DIR'],
                      'workspace_members': members, 'home_unchanged': True, 'codex_home_unchanged': True})
        terminal['phase'] = 'toolchain-install'
        run_owned('toolchain-install', [str(rustup), 'toolchain', 'install', '1.99.0', '--profile', 'minimal'],
                  work, env, clock, clock.prepare, evidence, {'toolchain': '1.99.0', 'cwd': str(work)})
        binary_before = analysis_binary_identity(work, clock, clock.prepare)
        versions = {}
        for kind, phase in (('cargo', 'cargo-version'), ('rustc', 'rustc-version')):
            binary = str(cargo) if kind == 'cargo' else binary_before['rustc']['path']
            raw, _ = run_owned(phase, [binary, '-Vv'], work, env, clock, clock.prepare, evidence,
                               {'toolchain': '1.99.0', 'cwd': str(work)}, capture=True, timeout=10)
            versions[kind] = parse_version(raw, kind)
        evidence.save('toolchain-before.json', {'binaries': binary_before, 'versions': versions})
        terminal['phase'] = 'dependency-fetch'
        run_owned('dependency-fetch', [str(cargo), 'fetch', '--locked', '--manifest-path', str(source/'Cargo.toml')],
                  source/'crates/oasis7_custody_worker', env, clock, clock.prepare, evidence,
                  {'manifest': str(source/'Cargo.toml'), 'cwd': str(source/'crates/oasis7_custody_worker'), 'toolchain': '1.99.0'})
        if snapshot_source(source, tree, clock, clock.prepare) != before:
            raise Rejected()
        if analysis_binary_identity(work, clock, clock.prepare) != binary_before:
            raise Rejected()
        terminal['phase'] = 'metadata-admission'
        metadata_end = clock.admit_metadata(); terminal['metadata_deadline_monotonic'] = metadata_end
        clock.arm(metadata_end)
        command_end = metadata_end - 60
        guard_deadline = time.time() + clock.remaining(metadata_end)
        original_cargo_home = os.environ.get('CARGO_HOME'); original_path = os.environ.get('PATH')
        try:
            os.environ['CARGO_HOME'] = env['CARGO_HOME']; os.environ['PATH'] = env['PATH']
            home = guard.install(work, out, guard_deadline, SOURCE, [source/'Cargo.toml'], monotonic_deadline=metadata_end)
        finally:
            for key, value in (('CARGO_HOME', original_cargo_home), ('PATH', original_path)):
                if value is None: os.environ.pop(key, None)
                else: os.environ[key] = value
        guard_installed = True
        install_guard_adapter(home, evidence, tools_identity, clock, command_end)
        wrapped = home/'bin/cargo'; env['CARGO_HOME'] = str(home)
        cwd = source/'crates/oasis7_custody_worker'
        expected_args = {
            'locate-project': ['locate-project', '--workspace', '--message-format=json', '--manifest-path', str(cwd/'Cargo.toml')],
            'metadata': ['metadata', '--locked', '--offline', '--format-version=1', '--manifest-path', str(source/'Cargo.toml'),
                         '--no-default-features', '--features', 'oasis7_custody_worker/recovery-fence', '--filter-platform', TARGET]}
        terminal['phase'] = 'discover'
        raw, _ = run_owned('discover', [str(wrapped), *expected_args['locate-project']], cwd, env, clock, command_end,
                           evidence, {'manifest': str(cwd/'Cargo.toml'), 'cwd': str(cwd), 'toolchain': '1.99.0'}, capture=True, timeout=10)
        discovery = parse_json(raw)
        if discovery != {'root': str(source/'Cargo.toml')}:
            raise Rejected()
        evidence.save('discovery.json', discovery)
        terminal['phase'] = 'metadata'
        raw, _ = run_owned('metadata', [str(wrapped), *expected_args['metadata']], cwd, env, clock, command_end, evidence,
                           {'manifest': str(source/'Cargo.toml'), 'cwd': str(cwd), 'toolchain': '1.99.0',
                            'target': TARGET, 'features': ['oasis7_custody_worker/recovery-fence'], 'no_default_features': True}, capture=True)
        graph, observation = metadata_graph(raw, source, work/'cargo-home', work/'target', members, lock_packages)
        evidence.save('metadata-graph.json', graph); evidence.save('role-observation.json', observation)
        manifests_before = metadata_manifest_hashes(graph, clock, metadata_end)
        evidence.save('metadata-manifests.json', manifests_before)
        terminal['metadata_observation'] = observation['status']
        if observation['status'] != 'ROLE_CONTEXT_OBSERVED':
            terminal['failed_checks'] = observation['failed_checks']
            raise Rejected()
        verdict = inspect_guard(evidence, guard_deadline, metadata_end, source, expected_args)
        evidence.save('guard-verdict.json', verdict, terminal=True)
        terminal['metadata_observation'] = 'ROOT_WORKSPACE_ROLE_METADATA_OBSERVED'
        terminal['phase'] = 'final-identity'
        # Use the same original Cargo home/proxy, not a new tool or environment.
        final_env = child_environment(environment, work, cargo)
        after_versions = {}
        for kind, phase in (('cargo', 'cargo-version-after'), ('rustc', 'rustc-version-after')):
            binary = str(cargo) if kind == 'cargo' else binary_before['rustc']['path']
            raw, _ = run_owned(phase, [binary, '-Vv'], work, final_env, clock, metadata_end, evidence,
                               {'toolchain': '1.99.0', 'cwd': str(work)}, capture=True, timeout=10)
            after_versions[kind] = parse_version(raw, kind)
        if after_versions != versions:
            raise Rejected()
        evidence.save('toolchain-versions-after.json', after_versions, terminal=True)
        terminal['status'] = 'ROOT_WORKSPACE_ROLE_METADATA_OBSERVED'
    except BaseException as exc:
        terminal['status'] = 'METADATA_INCOMPLETE'
        terminal['error_class'] = type(exc).__name__
        if isinstance(exc, CommandFailed):
            terminal['command_failure'] = {k: exc.receipt.get(k) for k in ('phase', 'exit', 'failure_kind', 'cleanup_errors')}
    finally:
        # Failed preparation also has bounded identity/seal time, never more commands.
        finish_end = clock.metadata_end if clock.metadata_end is not None else min(clock.end - 120, time.monotonic() + 60)
        try:
            clock.arm(finish_end)
            if before is not None:
                after = snapshot_source(source, tree, clock, finish_end)
                evidence.save('source-after.json', {'unchanged': after == before, 'source': after}, terminal=True)
                if after != before: raise Rejected()
            check_helpers(session/'tools', tools_identity, clock, finish_end)
            evidence.save('helpers-after.json', {'unchanged': True, 'tools': tools_identity}, terminal=True)
            if binary_before is not None:
                binary_after = analysis_binary_identity(work, clock, finish_end)
                if binary_after != binary_before: raise Rejected()
                evidence.save('toolchain-after.json', {'unchanged': True, 'binaries': binary_after}, terminal=True)
            if proxy_before is not None:
                _, _, proxy_after = proxy_identity(environment, work, clock, finish_end)
                if proxy_after != proxy_before: raise Rejected()
                evidence.save('proxy-after.json', {'unchanged': True, 'proxy': proxy_after}, terminal=True)
            if manifests_before is not None and metadata_manifest_hashes(graph, clock, finish_end) != manifests_before:
                raise Rejected()
            if guard_installed:
                if terminal['status'] == 'ROOT_WORKSPACE_ROLE_METADATA_OBSERVED':
                    final_guard = inspect_guard(evidence, guard_deadline, finish_end, source, expected_args)
                else:
                    try:
                        final_guard = guard.inspect(out, guard_deadline, SOURCE, end=finish_end)
                    except BaseException as exc:
                        final_guard = {'status': 'INCOMPLETE', 'error_class': type(exc).__name__}
                evidence.save('guard-verdict.json', final_guard, terminal=True)
            evidence.save('http-observation.json', api.receipts)
            terminal['completed_monotonic'] = time.monotonic()
            terminal['remaining_job_seconds_before_upload'] = clock.remaining()
            terminal['evidence'] = 'SEAL_REQUIRED'
            evidence.save('terminal.json', terminal, terminal=True)
            evidence.seal(clock, finish_end)
            # The sealed terminal says SEAL_REQUIRED; presence and verification of sha256.json
            # proves the seal, without mutating any member after its manifest was computed.
        except BaseException as exc:
            terminal['status'] = 'METADATA_INCOMPLETE'; terminal['evidence'] = 'INCOMPLETE'
            terminal['final_error_class'] = type(exc).__name__
            try:
                if not evidence.sealed:
                    evidence.save('terminal.json', terminal, terminal=True)
            except BaseException:
                pass
        finally:
            signal.setitimer(signal.ITIMER_REAL, 0)
    print(json.dumps({k: terminal[k] for k in ('status', 'phase', 'error_class', 'platform_completion', 'full_rust_coverage', 'production_capacity')}, sort_keys=True))
    return 0 if terminal['status'] == 'ROOT_WORKSPACE_ROLE_METADATA_OBSERVED' and evidence.sealed else 1


if __name__ == '__main__':
    # No arbitrary local CLI/bootstrap bypass. The verified workflow imports diagnose.
    raise SystemExit('Use the fixed verified workflow bootstrap')
