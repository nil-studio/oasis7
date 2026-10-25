#!/usr/bin/env python3
"""Locked Cargo process boundary for the isolated Rust capacity workflow only."""
import fcntl
import hashlib
import json
import os
from pathlib import Path
import shutil
import signal
import select
import stat
import subprocess
import sys
import time
import tomllib
import uuid
from codeql_rust_capacity_layout import _read_regular as read

MAX_CALLS = 2048
MAX_RECORD = 4096
MAX_AUDIT = 8 * 1024**2
MAX_CONFIG = 16384
PROTECTED = frozenset(('metadata', 'check', 'build', 'rustc'))
READ_ONLY = frozenset(('locate-project', 'read-manifest', 'config'))


def digest(path, deadline=None):
    target = Path(path).resolve()
    h = hashlib.sha256()
    with os.fdopen(os.open(target, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK), 'rb') as f:
        before = os.fstat(f.fileno())
        if not stat.S_ISREG(before.st_mode) or before.st_size > 128 * 1024**2:
            raise RuntimeError('Cargo tool identity bound/type')
        while data := f.read(65536):
            if deadline is not None and time.time() >= deadline:
                raise RuntimeError('Cargo identity deadline')
            h.update(data)
        after = os.fstat(f.fileno())
    identity = lambda s: (s.st_dev, s.st_ino, s.st_size, s.st_mtime_ns, s.st_ctime_ns)
    if identity(before) != identity(after) or identity(after) != identity(target.stat()):
        raise RuntimeError('Cargo tool changed during hashing')
    return h.hexdigest()


def standalone_locks(source, manifests, initial, frozen):
    """An independent workspace needs its own versioned, observed lock."""
    observed = {r['path']: r for r in initial['locks']}
    contexts, rejected = [], []
    for name in manifests:
        data = tomllib.loads(read(source / name, 1024**2).decode())
        if 'workspace' not in data:
            continue
        if not isinstance(data['workspace'], dict):
            raise RuntimeError('invalid workspace declaration')
        lock = str(Path(name).with_name('Cargo.lock'))
        record = observed.get(lock, {})
        sha = record.get('sha256', '')
        if (record.get('status') != 'CAPTURED' or len(sha) != 64
                or any(c not in '0123456789abcdef' for c in sha)
                or frozen.get(lock, {}).get('sha256') != sha):
            rejected.append(lock)
        contexts.append(name)
    if not contexts or len(contexts) > 256:
        raise RuntimeError('unknown independent workspace set')
    return {'status': 'FAIL' if rejected else 'PASS', 'manifests': contexts, 'rejected_locks': rejected}


def locked_argv(argv):
    """Accept the fixed extractor's command forms; reject unrecognised forms."""
    args = list(argv)
    index = 1 if args and args[0].startswith('+') else 0
    if index and (len(args[0]) > 256 or any(c.isspace() for c in args[0])):
        raise RuntimeError('invalid toolchain selector')
    # The pinned RA config query starts with `-Z unstable-options config get`.
    unstable = frozenset(('unstable-options', 'script', 'lockfile-path', 'json-target-spec'))
    while index < len(args):
        flag = args[index]
        if flag in ('-v', '-vv', '--verbose', '-q', '--quiet'):
            index += 1
        elif flag == '-Z' and index + 1 < len(args) and args[index + 1] in unstable:
            index += 2
        elif flag.startswith('-Z') and flag[2:] in unstable:
            index += 1
        else:
            break
    if index >= len(args):
        raise RuntimeError('missing Cargo command')
    verb = args[index]
    if verb in ('-V', '-vV', '--version') and index == len(args) - 1:
        return args, 'version', False
    if verb in READ_ONLY:
        # config get is a read-only capability query; aliases/set are not admitted.
        if verb == 'config' and (index + 1 >= len(args) or args[index + 1] != 'get'):
            raise RuntimeError('non-read-only Cargo config command')
        return args, verb, False
    if verb not in PROTECTED:
        raise RuntimeError('unsupported Cargo command')
    separator = args.index('--', index + 1) if '--' in args[index + 1:] else len(args)
    if '--locked' not in args[index + 1:separator]:
        args.insert(separator, '--locked')
    return args, verb, True


def install(work, out, deadline, source_sha, manifests=(), *, monotonic_deadline=None):
    existing = list(out.rglob('*'))
    if len(existing) > 8192 or any(p.is_symlink() for p in existing):
        raise RuntimeError('guard artifact inventory bound/type')
    if sum(p.stat().st_size for p in existing if p.is_file()) + MAX_AUDIT + 1024**2 >= 256 * 1024**2:
        raise RuntimeError('insufficient artifact capacity for Cargo audit')
    home = work / 'locked-cargo-home'
    (home / 'bin').mkdir(parents=True, exist_ok=False)
    audit = out / 'cargo-guard-calls'
    audit.mkdir(exist_ok=False)
    (audit / '.counter').write_text('0000000000000000')
    (audit / '.failed').write_bytes(b'0')
    real = Path(shutil.which('cargo') or '').absolute()
    if not real.is_file() or real.is_relative_to(work):
        raise RuntimeError('unknown real Cargo proxy')
    helper = Path(__file__).resolve()
    wrapper = home / 'bin/cargo'
    wrapper.write_text('#!' + sys.executable + '\nimport sys\nfrom pathlib import Path\nsys.path.insert(0,' + repr(str(helper.parent)) + ')\nimport codeql_rust_cargo_guard as guard\nsys.exit(guard.invoke(sys.argv[1:], Path(' + repr(str(out / 'cargo-guard.json')) + ')))\n')
    wrapper.chmod(0o700)
    fixed_end = time.monotonic() + max(0, deadline - time.time())
    if monotonic_deadline is not None:
        fixed_end = min(fixed_end, monotonic_deadline)
    config = {'deadline': deadline, 'monotonic_deadline': fixed_end, 'source_sha': source_sha, 'real_cargo': str(real),
              'real_target': str(real.resolve()), 'real_sha256': digest(real),
              'helper': str(helper), 'helper_sha256': digest(helper),
              'wrapper': str(wrapper), 'wrapper_sha256': digest(wrapper),
              'original_cargo_home': os.environ.get('CARGO_HOME'), 'audit': str(audit),
              'private_home': str(home),
              'expected_metadata_manifests': [hashlib.sha256(os.fsencode(Path(n).resolve())).hexdigest() for n in manifests]}
    data = json.dumps(config, indent=2) + '\n'
    if len(data.encode()) > MAX_CONFIG:
        raise RuntimeError('guard configuration bound')
    (out / 'cargo-guard.json').write_text(data)
    return home


def remaining(config):
    return min(config['deadline'] - time.time(), config['monotonic_deadline'] - time.monotonic())


def verify_config(config):
    if not isinstance(config.get('deadline'), (int, float)) or remaining(config) <= 0:
        raise RuntimeError('guard deadline expired')
    for path, expected in [('helper', 'helper_sha256'), ('wrapper', 'wrapper_sha256')]:
        if hashlib.sha256(read(Path(config[path]), 1024**2)).hexdigest() != config[expected]:
            raise RuntimeError('guard implementation changed')
    real = Path(config['real_cargo'])
    if str(real.resolve()) != config['real_target'] or digest(real, time.time() + max(0, remaining(config))) != config['real_sha256']:
        raise RuntimeError('real Cargo identity changed')
    if remaining(config) <= 0:
        raise RuntimeError('guard deadline expired during identity check')
    if real == Path(config['wrapper']) or Path(config['private_home']).resolve() in real.resolve().parents:
        raise RuntimeError('recursive Cargo proxy')


def mark_failed(config):
    # A preallocated sticky byte records failures even when no new receipt can be created.
    path = Path(config['audit']) / '.failed'
    fd = os.open(path, os.O_WRONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
    try:
        info = os.fstat(fd)
        if not stat.S_ISREG(info.st_mode) or info.st_size != 1 or os.pwrite(fd, b'1', 0) != 1:
            raise RuntimeError('invalid Cargo failure marker')
        os.fsync(fd)
    finally:
        os.close(fd)


def reserve(config, record):
    audit = Path(config['audit'])
    if audit.resolve() != audit:
        raise RuntimeError('unsafe audit root')
    fd = os.open(audit / '.lock', os.O_RDWR | os.O_CREAT | os.O_NOFOLLOW | os.O_NONBLOCK, 0o600)
    try:
        if not stat.S_ISREG(os.fstat(fd).st_mode):
            raise RuntimeError('unsafe audit mutex')
        until = time.monotonic() + min(5, max(0, remaining(config)))
        while True:
            if time.monotonic() >= until or remaining(config) <= 0:
                raise RuntimeError('Cargo audit mutex deadline')
            try:
                fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
                break
            except BlockingIOError:
                time.sleep(min(0.01, max(0, until - time.monotonic())))
        counter = os.open(audit / '.counter', os.O_RDWR | os.O_NOFOLLOW | os.O_NONBLOCK)
        try:
            info = os.fstat(counter)
            raw_count = os.read(counter, 17)
            if not stat.S_ISREG(info.st_mode) or len(raw_count) != 16 or not raw_count.isdigit():
                raise RuntimeError('invalid audit counter')
            count = int(raw_count) + 1
            if os.pwrite(counter, f'{count:016d}'.encode(), 0) != 16:
                raise RuntimeError('Cargo audit reservation short write')
            os.fsync(counter)
        finally:
            os.close(counter)
        if count > MAX_CALLS:
            raise RuntimeError('Cargo call count bound')
        path = audit / (uuid.uuid4().hex + '.json')
        data = json.dumps(record).encode() + b'\n'
        if len(data) > MAX_RECORD:
            raise RuntimeError('Cargo record bound')
        # Reserving a fixed-size slot bounds all concurrent final records too.
        output = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
        with os.fdopen(output, 'wb') as f:
            f.write(data); f.flush(); os.fsync(f.fileno())
        return path
    except Exception:
        mark_failed(config)
        raise
    finally:
        os.close(fd)


def finish_record(path, record):
    data = json.dumps(record).encode() + b'\n'
    if len(data) > MAX_RECORD:
        raise RuntimeError('Cargo final record bound')
    with os.fdopen(os.open(path, os.O_WRONLY | os.O_NOFOLLOW | os.O_NONBLOCK), 'wb') as f:
        f.write(data); f.truncate(); f.flush(); os.fsync(f.fileno())


def cargo_version(command, env, deadline):
    end = time.monotonic() + min(10, max(0, deadline - time.time()))
    if time.monotonic() >= end:
        raise RuntimeError('Cargo version deadline before launch')
    process = subprocess.Popen(command, env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    data = bytearray()
    try:
        while True:
            if time.monotonic() >= end or time.time() >= deadline:
                raise RuntimeError('Cargo version deadline')
            ready, _, _ = select.select([process.stdout], [], [], min(0.1, max(0, end - time.monotonic())))
            if not ready:
                continue
            chunk = os.read(process.stdout.fileno(), 2049 - len(data))
            if not chunk:
                break
            data.extend(chunk)
            if len(data) > 2048:
                raise RuntimeError('Cargo version output bound')
        if process.wait(timeout=max(0.001, end - time.monotonic())):
            raise RuntimeError('actual Cargo version unavailable')
        return data.decode('utf-8', errors='strict')
    finally:
        if process.poll() is None:
            process.kill(); process.wait()
        process.stdout.close()


def invoke(argv, config_path):
    child = None
    config = None
    record_path = None
    interrupted = 0
    record = {'status': 'PENDING', 'argv_sha256': hashlib.sha256(json.dumps(argv).encode()).hexdigest(),
              'started_at': time.time(), 'pid': os.getpid()}
    try:
        raw = read(config_path, MAX_CONFIG); config = json.loads(raw)
        record['config_sha256'] = hashlib.sha256(raw).hexdigest()
        record_path = reserve(config, record)
        verify_config(config)
        args, verb, protected = locked_argv(argv)
        record.update(operation=verb, locked=protected, full_metadata=verb == 'metadata' and '--no-deps' not in argv)
        if '--manifest-path' in args:
            offset = args.index('--manifest-path') + 1
            if offset >= len(args):
                raise RuntimeError('missing manifest argument')
            record['manifest_path_sha256'] = hashlib.sha256(os.fsencode(Path(args[offset]).resolve())).hexdigest()
        record['cwd_sha256'] = hashlib.sha256(os.fsencode(Path.cwd())).hexdigest()
        env = os.environ.copy()
        original = config['original_cargo_home']
        if original is None:
            env.pop('CARGO_HOME', None)
        else:
            env['CARGO_HOME'] = original
        # Keep RA's RUSTUP_TOOLCHAIN: analysis and project toolchains differ.
        record['rustup_toolchain'] = env.get('RUSTUP_TOOLCHAIN', '')[:256]
        if record['full_metadata']:
            prefix = [args[0]] if args and args[0].startswith('+') else []
            record['actual_cargo_version'] = cargo_version([config['real_cargo'], *prefix, '-Vv'], env, time.time() + max(0, remaining(config)))
        def forward(signum, _frame):
            nonlocal interrupted
            interrupted = signum
            if child is not None:
                child.send_signal(signum)
            raise RuntimeError('Cargo proxy interrupted')
        for sig in (signal.SIGTERM, signal.SIGINT):
            signal.signal(sig, forward)
        if remaining(config) <= 0:
            raise RuntimeError('Cargo deadline before launch')
        child = subprocess.Popen([config['real_cargo'], *args], env=env)
        try:
            code = child.wait(timeout=max(0.001, remaining(config)))
        except subprocess.TimeoutExpired:
            child.kill(); child.wait(); raise RuntimeError('Cargo fixed deadline')
        record.update(status='PASS' if code == 0 else 'FAIL', exit=code)
        return code if code >= 0 else 128 - code
    except Exception as exc:
        # Deliberately omit argv/env and untrusted exception text from public records.
        record.update(status='FAIL', error=type(exc).__name__)
        if config is not None:
            mark_failed(config)
        if child is not None and child.poll() is None:
            child.kill(); child.wait()
        print('CodeQL locked Cargo invocation rejected', file=sys.stderr)
        return 128 + interrupted if interrupted else 101
    finally:
        if record_path is not None:
            record['completed_at'] = time.time()
            try:
                finish_record(record_path, record)
            except Exception:
                mark_failed(config)
                raise


def inspect(out, deadline, source_sha, *, end=None):
    config_path = out / 'cargo-guard.json'
    raw = read(config_path, MAX_CONFIG); config = json.loads(raw)
    if config.get('deadline') != deadline or config.get('source_sha') != source_sha:
        raise RuntimeError('guard assessment identity differs')
    if end is not None:
        config = dict(config, monotonic_deadline=min(config['monotonic_deadline'], end))
    verify_config(config)
    audit = Path(config['audit'])
    if audit != out / 'cargo-guard-calls' or audit.resolve() != audit:
        raise RuntimeError('guard audit identity differs')
    health = read(audit / '.failed', 1)
    if health not in (b'0', b'1'):
        raise RuntimeError('invalid Cargo failure marker')
    rejected = health == b'1'
    paths = list(audit.glob('*.json'))
    count = int(read(audit / '.counter', 16))
    if not paths or len(paths) > MAX_CALLS or count != len(paths):
        raise RuntimeError('missing/oversized Cargo audit')
    total = full = failed = 0
    observed = set()
    expected = hashlib.sha256(raw).hexdigest()
    for path in paths:
        if remaining(config) <= 0:
            raise RuntimeError('Cargo audit deadline')
        data = read(path, MAX_RECORD); total += len(data)
        if total > MAX_AUDIT:
            raise RuntimeError('Cargo audit byte bound')
        r = json.loads(data)
        if r.get('config_sha256') != expected or r.get('status') != 'PASS' or r.get('exit') != 0:
            failed += 1
        if r.get('operation') in PROTECTED and r.get('locked') is not True:
            failed += 1
        if r.get('full_metadata') and r.get('locked') and r.get('actual_cargo_version'):
            full += 1
            observed.add(r.get('manifest_path_sha256'))
    missing = set(config['expected_metadata_manifests']) - observed
    return {'status': 'PASS' if full and not failed and not missing and not rejected else 'FAIL',
            'missing_metadata_contexts': len(missing), 'rejected_invocation': rejected, 'calls': len(paths),
            'full_metadata_calls': full, 'failed_or_pending': failed, 'bytes': total,
            'scope': 'proxy observations; not runner process cleanup or full Rust coverage'}
