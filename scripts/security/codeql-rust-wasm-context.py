#!/usr/bin/env python3
"""One frozen WASM CodeQL context. Only the verified default-off bootstrap calls it."""
import hashlib
import importlib.util
import json
import math
import os
from pathlib import Path
import re
import resource
import selectors
import shutil
import signal
import stat
import subprocess
import time
import tomllib

import codeql_rust_cargo_guard as guard
import codeql_rust_candidate_profile as candidate_profile
import codeql_rust_diagnostic_runtime as runtime
import codeql_rust_wasm_witnesses as witnesses


def load(name, filename):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(filename))
    module = importlib.util.module_from_spec(spec); spec.loader.exec_module(module)
    return module


metadata = load('wasm_context_metadata', 'codeql-rust-wasm-metadata.py')
model = load('wasm_context_model', 'codeql-rust-model-diagnostic.py')
SOURCE, SOURCE_TREE = metadata.SOURCE, metadata.SOURCE_TREE
TARGET, TOOLCHAIN = metadata.TARGET, metadata.TOOLCHAIN
LOCAL_PACKAGES = metadata.LOCAL_PACKAGES
VERSION = '2.27.4'
LIBRARY_VARIANT = 'wasm-shared-helper-v1'
BUNDLE_SHA = model.BUNDLES[VERSION]['sha256']
BUNDLE_URL = 'https://github.com/github/codeql-action/releases/download/codeql-bundle-v2.27.4/codeql-bundle-rust-linux64.tar.zst'
HELPERS = ('codeql-rust-wasm-context.py', 'codeql_rust_wasm_witnesses.py',
           'codeql_rust_candidate_profile.py',
           'codeql-rust-wasm-metadata.py', 'codeql-rust-model-diagnostic.py',
           'codeql-rust-shared-constraint.patch',
           'codeql_rust_cargo_guard.py', 'codeql_rust_capacity_layout.py',
           'codeql_rust_capacity_evidence.py', 'codeql_rust_diagnostic_runtime.py', 'workflow.yml')
PHASES = ('bundle-download', 'bundle-unpack', 'codeql-version', 'apply-shared-helper-patch', 'toolchain-install',
          'cargo-version', 'rustc-version', 'dependency-fetch', 'query-library', 'metadata',
          'database-init', 'database-index', 'database-finalize', 'database-identity',
          'witness-query', 'witness-decode', 'query-library-after', 'cargo-version-after', 'rustc-version-after')
Rejected, Stopped, CommandFailed = metadata.Rejected, metadata.Stopped, metadata.CommandFailed
encoded, sha256, parse_json = metadata.encoded, metadata.sha256, metadata.parse_json
MAX_STREAM, MAX_STDOUT, MAX_PROCESS_RECEIPT = metadata.MAX_STREAM, metadata.MAX_STDOUT, metadata.MAX_PROCESS_RECEIPT
projected_arguments, cleanup_group = metadata.projected_arguments, metadata.cleanup_group
_signal_scope = None


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



# Same reviewed owned-process loop as metadata, with this entrypoint's closed phase table.
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
    progress = None; query_sha256 = None; progress_error = None; progress_timing = {}

    def observe(kind, data, *, final=False):
        nonlocal progress_error
        if progress is None:
            return
        try:
            for line in progress.feed(kind, data, final=final).splitlines():
                event = parse_json(line)
                if (set(event) != {'event', 'stream', 'index', 'query_sha256', 'received_wall', 'received_monotonic'}
                        or event['event'] not in progress.counts or event['stream'] != kind
                        or event['index'] != 0 or event['query_sha256'] != query_sha256
                        or any(type(event[key]) not in (int, float) or not math.isfinite(event[key])
                               or not 0 <= event[key] <= 10**12 for key in ('received_wall', 'received_monotonic'))
                        or not start <= event['received_monotonic'] <= clock.end):
                    raise Rejected()
                first = {key: event[key] for key in ('received_wall', 'received_monotonic')}
                times = progress_timing.setdefault(event['event'], {'first_' + k: v for k, v in first.items()})
                times.update({'last_' + k: v for k, v in first.items()})
        except Exception:
            # Keep draining/hashing the real command; observation cannot replace its first failure.
            progress_error = 'PROGRESS_REJECTED'
            progress.fail_feed(progress_error)

    def consume(fd, *, retain):
        chunk = os.read(fd, 65536)
        kind = names[fd]
        if not chunk:
            eof[kind] = True
            observe(kind, b'', final=True)
            if fd in selector.get_map(): selector.unregister(fd)
            return False
        sizes[kind] += len(chunk); hashes[kind].update(chunk)
        observe(kind, chunk)
        if sizes[kind] > MAX_STREAM or (capture and kind == 'stdout' and sizes[kind] > MAX_STDOUT):
            record['failure_kind'] = record['failure_kind'] or 'output_limit'
            raise Rejected()
        if retain and capture and kind == 'stdout': output.extend(chunk)
        return True

    try:
        _signal_scope = scope
        if len(json.dumps(argv).encode()) > 16 * 1024:
            raise Rejected()
        evidence.totals()
        if phase == 'witness-query':
            query = Path(argv[-1])
            if not query.is_absolute() or query.name != witnesses.NAME + '.ql':
                raise Rejected()
            query_sha256 = metadata.file_hash(query, clock, stop - 4, maximum=1024**2)['sha256']
            progress = candidate_profile.Progress([{'path': str(query), 'pack': witnesses.PACK, 'sha256': query_sha256}])
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
            if phase == 'witness-query':
                if progress is None:
                    record['query_progress'] = {'status': 'NOT_RUN', 'query_sha256': query_sha256}
                else:
                    observed = progress.summary()
                    if (progress.feed_failure or any(observed['streams'][k]['bytes'] != sizes[k]
                            or observed['streams'][k]['sha256'] != hashes[k].hexdigest()
                            or observed['streams'][k]['final'] != eof[k] for k in sizes)):
                        progress_error = 'PROGRESS_REJECTED'
                    record['query_progress'] = dict(observed, schema=1, codeql_version=VERSION,
                        query_sha256=query_sha256, timing=progress_timing,
                        status='INCOMPLETE' if progress_error else 'OBSERVED', error_code=progress_error)
                    if progress_error and not record['failure_kind']:
                        record['failure_kind'] = 'progress_observation'; record['error_class'] = 'Rejected'
            if sample is not None: record['resource_observation'] = sample.result()
            record['elapsed_seconds'] = time.monotonic() - start
            if len(encoded(record)) > MAX_PROCESS_RECEIPT:
                record['resource_observation'] = {'status': 'UNAVAILABLE_RECEIPT_LIMIT'}
                record['failure_kind'] = record['failure_kind'] or 'receipt_limit'
                record['error_class'] = record['error_class'] or 'Rejected'
            if scope['pending']:
                record['failure_kind'] = record['failure_kind'] or 'cancelled'
                record['error_class'] = record['error_class'] or 'Stopped'
            evidence.save(phase + '.result.json', record)
        finally:
            try:
                signal.pthread_sigmask(signal.SIG_SETMASK, restore)
            finally:
                _signal_scope = previous_scope
    if scope['pending']:
        record['failure_kind'] = record['failure_kind'] or 'cancelled'
        record['error_class'] = record['error_class'] or 'Stopped'
    if record['failure_kind'] or record['cleanup_errors']:
        raise CommandFailed(record)
    return bytes(output), record


class Clock:
    """The bootstrap's original job clock; no stage may reset it."""
    def __init__(self, receipt):
        self.receipt = receipt
        for key in ('deadline_monotonic', 'deadline_wall', 'prepare_end_monotonic', 'prepare_end_wall'):
            if type(receipt.get(key)) not in (int, float) or not math.isfinite(receipt[key]):
                raise Rejected()
        self.end, self.wall = receipt['deadline_monotonic'], receipt['deadline_wall']
        self.prepare = receipt['prepare_end_monotonic']
        if (receipt.get('maximum_job_seconds') != 1800
                or abs(self.end - self.prepare - 1020) > math.ulp(self.end) + math.ulp(self.prepare)
                or abs(self.wall - receipt['prepare_end_wall'] - 1020) > .001
                or not 0 < self.remaining() <= 1800):
            raise Rejected()
        self.analysis_end = None

    def remaining(self, end=None):
        cutoff = self.end if end is None else min(self.end, end)
        return min(cutoff - time.monotonic(), self.wall - time.time() - (self.end - cutoff))

    def check(self, end=None):
        if self.remaining(end) <= 0:
            raise Rejected()

    def admit(self):
        self.check(self.prepare)
        if self.analysis_end is not None or self.remaining() < 1020:
            raise Rejected()
        self.analysis_end = min(time.monotonic() + 900, self.end - 120)
        return self.analysis_end

    def arm(self, end):
        self.check(end)
        signal.setitimer(signal.ITIMER_REAL, self.remaining(end))


def check_helpers(directory, identity, clock, end):
    if (set(identity['files']) != set(HELPERS) or {p.name for p in directory.iterdir()} != set(HELPERS)
            or not re.fullmatch('[0-9a-f]{40}', identity.get('tool_sha', ''))):
        raise Rejected()
    for filename, expected in identity['files'].items():
        got = metadata.file_hash(directory / filename, clock, end, maximum=4 * 1024**2)
        if any(got[key] != expected[key] for key in ('bytes', 'sha256', 'git_blob')):
            raise Rejected()


def tree_files(root, clock, end):
    """Bounded regular-file inventory, also used for the fixed shared patch difference."""
    clock.check(end)
    if root.resolve() != root or not root.is_dir():
        raise Rejected()
    rows = {}; total = 0
    for current, directories, files in os.walk(root, followlinks=False):
        clock.check(end)
        if any((Path(current) / p).is_symlink() for p in directories + files):
            raise Rejected()
        for name in files:
            path = Path(current) / name
            value = metadata.file_hash(path, clock, end, maximum=1024**3)
            total += value['bytes']
            rows[path.relative_to(root).as_posix()] = value
            if len(rows) > 100000 or total > 8 * 1024**3:
                raise Rejected()
    if not rows:
        raise Rejected()
    return rows


def tree_identity(root, clock, end):
    """Compact before/after identity of an entire task-owned immutable tree."""
    rows = tree_files(root, clock, end)
    return {'files': len(rows), 'bytes': sum(value['bytes'] for value in rows.values()), 'tree_sha256': sha256(encoded(rows))}


def shared_helper_identity(official, private, clock, end):
    identity = model.shared_helper_identity(official, private, snapshot=lambda root: tree_files(root, clock, end))
    return dict(variant=LIBRARY_VARIANT, **identity)


def prepare_shared_helper(official, work, clock, evidence, command):
    def copy_file(source, target):
        clock.check(clock.prepare)
        return shutil.copy2(source, target)
    private = work / 'candidate-typeinference'
    identity = model.prepare_shared_helper(official['codeql/typeinference'], private,
        Path(__file__).with_name('codeql-rust-shared-constraint.patch'),
        snapshot=lambda root: tree_files(root, clock, clock.prepare), copy_file=copy_file,
        file_digest=lambda path: metadata.file_hash(path, clock, clock.prepare, maximum=4 * 1024**2)['sha256'],
        run=lambda argv, label, limit: command(label, argv, work, clock.prepare, timeout=limit))
    identity = dict(variant=LIBRARY_VARIANT, **identity)
    evidence.save('shared-helper-identity.json', identity)
    selected = {name: dict(value) for name, value in official.items()}
    selected['codeql/typeinference']['root'] = str(private)
    return selected, identity


def prepare_query(source, work, closure):
    query = work / 'query'; query.mkdir(mode=0o700)
    (query / 'qlpack.yml').write_text('name: ' + witnesses.PACK + '\nversion: 0.0.0\ndependencies:\n  codeql/rust-all: 0.2.25\n')
    (query / 'codeql-pack.lock.yml').write_text('lockVersion: 1.0.0\ndependencies:\n' + ''.join(
        '  ' + name + ':\n    version: ' + value['version'] + '\n' for name, value in sorted(closure.items())))
    (query / (witnesses.NAME + '.ql')).write_text(witnesses.query(source))
    (work / 'qlconfig.yml').write_text('registries: []\n')
    return query


def query_options(work, closure):
    return ['--additional-packs=' + os.pathsep.join(value['root'] for value in closure.values()),
            '--no-default-compilation-cache', '--compilation-cache=' + str(work / 'compilation-cache'),
            '--common-caches=' + str(work / 'common-cache'), '--no-use-global-qlconfig',
            '--qlconfig-file=' + str(work / 'qlconfig.yml')]


def extractor_arguments(source, work, wrapped):
    return [str(wrapped), 'check', '--locked', '--offline', '--manifest-path', str(source / 'Cargo.toml'),
            '-p', 'oasis7_custody_worker', '--lib', '--target', TARGET, '--no-default-features',
            '--features', 'oasis7_custody_worker/recovery-fence', '--message-format=json',
            '--target-dir', str(work / 'target')]


def index_arguments(database, source):
    # --working-dir is explicit even though the process cwd and source-root agree.
    return ['database', 'index-files', '--language=rust', '--threads=2', '--ram=3072',
            '--working-dir=' + str(source), '--no-follow-symlinks',
            *['--include=crates/' + name + '/**/*.rs' for name in LOCAL_PACKAGES],
            '-O=cargo_target=' + TARGET, '-O=cargo_features=oasis7_custody_worker/recovery-fence',
            '--', str(database)]


def selected_files(source, before):
    prefixes = tuple('crates/' + name + '/' for name in LOCAL_PACKAGES)
    result = {p: v['sha256'] for p, v in before['files'].items() if p.startswith(prefixes) and p.endswith('.rs')}
    if not result or not all(any(p.startswith(prefix) for p in result) for prefix in prefixes):
        raise Rejected()
    return result


def observe_index(database, source, expected):
    paths = list((database / 'working').glob('files-to-index*'))
    if len(paths) != 1 or paths[0].is_symlink() or not paths[0].is_file() or paths[0].stat().st_size > 4 * 1024**2:
        raise Rejected()
    raw = paths[0].read_bytes(); lines = raw.decode('utf-8').splitlines()
    actual = []
    for line in lines:
        path = Path(line)
        if not path.is_absolute() or path.resolve() != path or not path.is_relative_to(source):
            raise Rejected()
        actual.append(path.relative_to(source).as_posix())
    if len(actual) != len(set(actual)) or set(actual) != set(expected):
        raise Rejected()
    return {'status': 'PASS', 'selected_count': len(actual), 'files_to_index_sha256': sha256(raw),
            'cwd': str(source), 'source_root': str(source), 'working_dir': str(source),
            'input_paths': sorted(actual), 'scope': 'exact CLI input selection, not semantic coverage'}


def inspect_guard(evidence, deadline, end, source, preflight, check_args):
    verdict = guard.inspect(evidence.root, deadline, SOURCE, end=end)
    files = {p.name: p.read_bytes() for p in (evidence.root / 'cargo-guard-calls').glob('*.json')}
    if any(name not in files or sha256(files[name]) != value for name, value in preflight.items()):
        raise Rejected()
    records = [parse_json(raw) for raw in files.values()]
    extraction = [parse_json(raw) for name, raw in files.items() if name not in preflight]
    manifest = sha256(os.fsencode(source / 'Cargo.toml'))
    cwd = sha256(os.fsencode(source))
    build_hash = sha256(json.dumps(check_args[1:]).encode())
    roots = [r for r in extraction if r.get('full_metadata') and r.get('manifest_path_sha256') == manifest]
    checks = [r for r in extraction if r.get('operation') == 'check' and r.get('argv_sha256') == build_hash]
    exact = (verdict['status'] == 'PASS' and len(preflight) == 1 and len(roots) >= 1 and len(checks) >= 1
             and all(r.get('rustup_toolchain') == '1.99.0' for r in records)
             and all(r.get('cwd_sha256') == cwd and r.get('manifest_path_sha256') == manifest for r in checks)
             and all(r.get('operation') != 'check' or r.get('argv_sha256') == build_hash for r in extraction))
    for record in records:
        if record.get('full_metadata'):
            metadata.parse_version(record.get('actual_cargo_version', '').encode(), 'cargo')
            projection = record.get('wasm_version_projection', {})
            exact = exact and projection.get('status') == 'VALIDATED_PROJECTED'
    return dict(verdict, status='PASS' if exact else 'INCOMPLETE',
                preflight_calls=len(preflight), extraction_calls=len(extraction),
                extraction_root_metadata_calls=len(roots), exact_build_script_calls=len(checks),
                build_script_argv_sha256=build_hash,
                scope='fresh root metadata plus extractor root/check receipts; not old D1/D2 or 47-workspace coverage')


def finalized_database(raw, database, source, clock, end):
    value = parse_json(raw); path = database / 'codeql-database.yml'
    identity = metadata.file_hash(path, clock, end, maximum=1024**2)
    if (value.get('sourceLocationPrefix') != str(source) or value.get('languages') != ['rust']
            or value.get('datasetFolder') != str(database / 'db-rust')
            or '\nfinalised: true\n' not in path.read_text()):
        raise Rejected()
    return {'status': 'PASS', 'source_root': str(source), 'languages': ['rust'], 'metadata': identity}


def diagnose(session, clock_receipt, tools_identity, api, environment):
    session = Path(session); out = session / 'evidence'; work = session / 'work'
    source, bundle, database = work / 'source', work / 'codeql', work / 'database'
    evidence = metadata.Evidence(out); clock = Clock(clock_receipt)
    terminal = {'status': 'WASM_CONTEXT_INCOMPLETE', 'phase': 'admission', 'error_class': None,
                'metadata': 'NOT_RUN', 'extraction': 'NOT_RUN', 'witness_query': 'NOT_RUN', 'macros': 'NOT_RUN',
                'static_callees': 'NOT_RUN', 'wasm_role_cfg': 'NOT_RUN', 'guard': 'NOT_RUN', 'identity': 'INCOMPLETE',
                'evidence': 'INCOMPLETE', 'platform_completion': 'PENDING_EXTERNAL_READBACK',
                'production_capacity': 'UNKNOWN', 'full_rust_coverage': 'HOLD',
                'job_deadline_monotonic': clock.end, 'prepare_deadline_monotonic': clock.prepare,
                'analysis_deadline_monotonic': None, 'source_sha': SOURCE, 'tool_sha': tools_identity['tool_sha'],
                'library_variant': LIBRARY_VARIANT}
    before = tree = proxy_before = binary_before = toolchain_before = bundle_before = query_before = None
    shared_before = official_closure = None
    graph = manifests_before = guard_deadline = None
    guard_installed = False; preflight = {}; check_args = []; query = None
    try:
        for sig in (signal.SIGALRM, signal.SIGTERM, signal.SIGINT):
            signal.signal(sig, interruption)
        clock.arm(clock.prepare); metadata.set_subreaper()
        if (session.resolve() != session or tools_identity['tool_sha'] != clock_receipt['tool_sha']
                or tools_identity['tool_sha'] != environment['GITHUB_SHA']):
            raise Rejected()
        check_helpers(session / 'tools', tools_identity, clock, clock.prepare)
        work.mkdir(mode=0o700)
        terminal['phase'] = 'source-preparation'
        raw_tree, _ = api.get('source-tree', SOURCE_TREE)
        tree = metadata.verify_git_tree(raw_tree, SOURCE_TREE)
        archive = api.archive()
        archive_identity = {'bytes': len(archive), 'sha256': sha256(archive), 'source_sha': SOURCE, 'source_tree': SOURCE_TREE}
        before = metadata.unpack_source(archive, tree, source, clock, clock.prepare); del archive
        evidence.save('source-before.json', before); evidence.save('source-archive.json', archive_identity)
        evidence.admit_final(before, tools_identity)
        evidence.save('witness-source.json', witnesses.verify_source(source))
        manifest = tomllib.loads((source / 'Cargo.toml').read_text())
        members = sorted(set(manifest['workspace']['members']))
        if len(members) != 53 or 'package' in manifest:
            raise Rejected()
        for member in members:
            metadata.safe_relative(member)
            if not (source / member / 'Cargo.toml').is_file(): raise Rejected()
        for name in LOCAL_PACKAGES:
            if tomllib.loads((source / 'crates' / name / 'Cargo.toml').read_text())['package']['name'] != name:
                raise Rejected()
        locks = {(p['name'], p['version'], p.get('source')) for p in tomllib.loads((source / 'Cargo.lock').read_text())['package']}
        inputs = selected_files(source, before)
        evidence.save('expected-index.json', {'files': inputs, 'packages': list(LOCAL_PACKAGES),
                      'workspace_members': members, 'single_crate_isolation': False})
        cargo, rustup, proxy_before = metadata.proxy_identity(environment, work, clock, clock.prepare)
        env = metadata.child_environment(environment, work, cargo)
        for name in ('cargo-home', 'rustup', 'target'):
            (work / name).mkdir(mode=0o700)

        def command(phase, argv, cwd, end, *, capture=False, timeout=None, semantic=None, child_env=None):
            terminal['phase'] = phase
            return run_owned(phase, [str(x) for x in argv], cwd, env if child_env is None else child_env,
                             clock, end, evidence, semantic or {'cwd': str(cwd)}, capture=capture, timeout=timeout)[0]

        terminal['phase'] = 'bundle-download'
        bundle_archive = work / 'bundle.tar.zst'
        command('bundle-download', ['curl', '--fail', '--location', '--proto', '=https', '--proto-redir', '=https',
                '--connect-timeout', '20', '--max-time', str(max(1, int(clock.remaining(clock.prepare) - 4))),
                '--output', bundle_archive, BUNDLE_URL], work, clock.prepare)
        bundle_download = metadata.file_hash(bundle_archive, clock, clock.prepare, maximum=1024**3)
        if bundle_download['sha256'] != BUNDLE_SHA:
            raise Rejected()
        evidence.save('bundle-download.json', {'version': VERSION, **bundle_download})
        command('bundle-unpack', ['tar', '--zstd', '-xf', bundle_archive, '-C', work], work, clock.prepare)
        bundle_before = tree_identity(bundle, clock, clock.prepare)
        codeql = bundle / 'codeql'
        cli_base = ['--log-to-stderr', '--loglevel=ERROR', '--common-caches=' + str(work / 'common-cache')]
        raw = command('codeql-version', [codeql, 'version', '--format=json', *cli_base], work, clock.prepare, capture=True, timeout=20)
        actual = parse_json(raw)
        if actual.get('version') != VERSION: raise Rejected()
        evidence.save('codeql-version.json', {'version': VERSION, 'bundle': bundle_before})
        official_closure = model.bundle_closure(bundle, VERSION)
        if len(official_closure) != 13: raise Rejected()
        evidence.save('official-library-closure.json', official_closure)
        terminal['phase'] = 'shared-preparation'
        closure, shared_before = prepare_shared_helper(official_closure, work, clock, evidence, command)
        evidence.save('library-closure.json', closure)
        query = prepare_query(source, work, closure)
        options = query_options(work, closure)
        query_path = query / (witnesses.NAME + '.ql')
        raw = command('query-library', [codeql, 'resolve', 'library-path', '--query=' + str(query_path),
                      '--format=json', *options, '--log-to-stderr', '--loglevel=ERROR'], work, clock.prepare, capture=True, timeout=20)
        library = parse_json(raw)
        model.verify_library_path(library, query, witnesses.NAME, closure, witnesses.PACK)
        if library['compilationCache'] != [str(work / 'compilation-cache')]: raise Rejected()
        evidence.save('query-library.json', library)
        query_before = tree_identity(query, clock, clock.prepare)

        command('toolchain-install', [rustup, 'toolchain', 'install', '1.99.0', '--profile', 'minimal',
                '--no-self-update', '--component', 'rust-src', '--target', TARGET], work, clock.prepare)
        binary_before = metadata.analysis_binary_identity(work, clock, clock.prepare)
        toolchain_root = work / 'rustup/toolchains' / TOOLCHAIN
        if (not (toolchain_root / 'lib/rustlib/src/rust/library/Cargo.toml').is_file()
                or not list((toolchain_root / 'lib/rustlib' / TARGET / 'lib').glob('libstd-*.rlib'))):
            raise Rejected()
        toolchain_before = tree_identity(toolchain_root, clock, clock.prepare)
        versions = {}
        for kind in ('cargo', 'rustc'):
            tool = cargo if kind == 'cargo' else binary_before['rustc']['path']
            raw = command(kind + '-version', [tool, '-Vv'], work, clock.prepare, capture=True, timeout=10)
            versions[kind] = metadata.parse_version(raw, kind)
        evidence.save('toolchain-before.json', {'versions': versions, 'binaries': binary_before, 'tree': toolchain_before})
        command('dependency-fetch', [cargo, 'fetch', '--locked', '--manifest-path', source / 'Cargo.toml'],
                source, clock.prepare, semantic={'cwd': str(source), 'manifest': str(source / 'Cargo.toml'), 'toolchain': '1.99.0'})
        if metadata.snapshot_source(source, tree, clock, clock.prepare) != before:
            raise Rejected()

        terminal['phase'] = 'analysis-admission'
        analysis_end = clock.admit(); command_end = analysis_end - 60
        terminal['analysis_deadline_monotonic'] = analysis_end
        clock.arm(analysis_end)
        guard_deadline = time.time() + clock.remaining(analysis_end)
        previous = {key: os.environ.get(key) for key in ('CARGO_HOME', 'PATH')}
        try:
            os.environ.update(CARGO_HOME=env['CARGO_HOME'], PATH=env['PATH'])
            home = guard.install(work, out, guard_deadline, SOURCE, [source / 'Cargo.toml'], monotonic_deadline=analysis_end)
        finally:
            for key, value in previous.items():
                if value is None: os.environ.pop(key, None)
                else: os.environ[key] = value
        guard_installed = True
        metadata.install_guard_adapter(home, evidence, tools_identity, clock, command_end)
        wrapped = home / 'bin/cargo'
        env.update(CARGO_HOME=str(home), PATH=str(home / 'bin') + os.pathsep + env['PATH'], CARGO_NET_OFFLINE='true')
        metadata_args = ['metadata', '--locked', '--offline', '--format-version=1', '--manifest-path', str(source / 'Cargo.toml'),
                         '--no-default-features', '--features', 'oasis7_custody_worker/recovery-fence', '--filter-platform', TARGET]
        terminal['metadata'] = 'RUNNING'
        raw = command('metadata', [wrapped, *metadata_args], source, command_end, capture=True,
                      semantic={'cwd': str(source), 'manifest': str(source / 'Cargo.toml'), 'toolchain': '1.99.0',
                                'target': TARGET, 'features': ['oasis7_custody_worker/recovery-fence'], 'no_default_features': True})
        graph, observation = metadata.metadata_graph(raw, source, work / 'cargo-home', work / 'target', members, locks)
        evidence.save('metadata-graph.json', graph); evidence.save('role-observation.json', observation)
        manifests_before = metadata.metadata_manifest_hashes(graph, clock, command_end)
        evidence.save('metadata-manifests.json', manifests_before)
        terminal['metadata'] = observation['status']
        if observation['status'] != 'ROLE_CONTEXT_OBSERVED': raise Rejected()
        preflight = {p.name: sha256(p.read_bytes()) for p in (out / 'cargo-guard-calls').glob('*.json')}
        if len(preflight) != 1 or guard.inspect(out, guard_deadline, SOURCE, end=command_end)['status'] != 'PASS':
            raise Rejected()
        evidence.save('guard-preflight.json', {'status': 'PASS', 'calls': preflight, 'scope': 'fresh preparation only; extraction not yet observed'})
        check_args = extractor_arguments(source, work, wrapped)
        env.update(CODEQL_THREADS='2', CODEQL_RAM='3072',
                   CODEQL_EXTRACTOR_RUST_OPTION_BUILD_SCRIPT_COMMAND=json.dumps(check_args),
                   CODEQL_EXTRACTOR_RUST_OPTION_CARGO_TARGET_DIR=str(work / 'target'),
                   CODEQL_EXTRACTOR_RUST_OPTION_CARGO_ALL_TARGETS='true')
        evidence.save('extractor-request.json', {'cargo_target': TARGET, 'cargo_features': ['oasis7_custody_worker/recovery-fence'],
                      'cargo_all_targets': True, 'no_default_features': True, 'build_script_argv_sha256': sha256(json.dumps(check_args).encode()),
                      'build_script_command_count': len(check_args), 'build_manifest': str(source / 'Cargo.toml'),
                      'build_package': 'oasis7_custody_worker', 'build_library_only': True, 'cwd': str(source),
                      'source_root': str(source), 'index_packages': list(LOCAL_PACKAGES),
                      'scope': 'requested extractor settings; actual guard and QL cfg/semantic witnesses required'})
        terminal['extraction'] = 'RUNNING'
        command('database-init', [codeql, 'database', 'init', '--language=rust', '--build-mode=none', '--source-root=' + str(source),
                *cli_base, '--', database], source, command_end)
        index = index_arguments(database, source)
        command('database-index', [codeql, *index[:-2], *cli_base, *index[-2:]], source, command_end)
        evidence.save('actual-index.json', observe_index(database, source, inputs))
        command('database-finalize', [codeql, 'database', 'finalize', '--threads=2', '--ram=3072', *cli_base, '--', database],
                source, command_end)
        raw = command('database-identity', [codeql, 'resolve', 'database', database, '--format=json', *cli_base],
                      source, command_end, capture=True, timeout=20)
        evidence.save('database-identity.json', finalized_database(raw, database, source, clock, command_end))
        verdict = inspect_guard(evidence, guard_deadline, command_end, source, preflight, check_args)
        evidence.save('guard-extraction.json', verdict); terminal['guard'] = verdict['status']
        if verdict['status'] != 'PASS': raise Rejected()
        terminal['extraction'] = 'FINALIZED_CONTEXT_OBSERVED'
        if shared_helper_identity(Path(official_closure['codeql/typeinference']['root']),
                                  work / 'candidate-typeinference', clock, command_end) != shared_before:
            raise Rejected()
        terminal['witness_query'] = 'RUNNING'
        # Detailed stderr logging suppresses the pinned CLI's console progress.
        command('witness-query', [codeql, 'database', 'run-queries', '--no-use-precompiled', '--threads=2', '--ram=3072',
                *options, '--loglevel=ERROR', '--verbosity=progress', '--', database, query_path], source, command_end)
        terminal['witness_query'] = 'COMPUTED_UNVERIFIED'
        result = database / 'results' / witnesses.PACK / (witnesses.NAME + '.bqrs')
        bqrs = metadata.file_hash(result, clock, command_end, maximum=1024**2)
        raw = command('witness-decode', [codeql, 'bqrs', 'decode', result, '--format=json', *cli_base],
                      source, command_end, capture=True, timeout=20)
        observation = witnesses.validate(parse_json(raw))
        terminal['witness_query'] = 'PASS' if observation['status'] == 'PASS' else 'COMPUTED_ORACLE_INCOMPLETE'
        evidence.save('witness-observation.json', dict(observation, bqrs=bqrs, query_identity=query_before))
        for key in ('macros', 'static_callees', 'wasm_role_cfg'):
            terminal[key] = observation[key]
        if observation['status'] != 'PASS': raise Rejected()
        raw = command('query-library-after', [codeql, 'resolve', 'library-path', '--query=' + str(query_path),
                      '--format=json', *options, '--log-to-stderr', '--loglevel=ERROR'], source, command_end, capture=True, timeout=20)
        if parse_json(raw) != library: raise Rejected()
        evidence.save('query-library-after.json', library)
        final_env = metadata.child_environment(environment, work, cargo)
        after_versions = {}
        for kind in ('cargo', 'rustc'):
            tool = cargo if kind == 'cargo' else binary_before['rustc']['path']
            raw = command(kind + '-version-after', [tool, '-Vv'], work, command_end, capture=True, timeout=10, child_env=final_env)
            after_versions[kind] = metadata.parse_version(raw, kind)
        if after_versions != versions: raise Rejected()
        evidence.save('toolchain-versions-after.json', after_versions, terminal=True)
        terminal['status'] = 'WASM_CONTEXT_SEMANTICS_OBSERVED'
    except BaseException as exc:
        terminal['error_class'] = type(exc).__name__
        for key in ('metadata', 'extraction', 'witness_query'):
            if terminal[key] == 'RUNNING': terminal[key] = 'INCOMPLETE'
        if isinstance(exc, CommandFailed):
            terminal['command_failure'] = {k: exc.receipt.get(k) for k in ('phase', 'exit', 'failure_kind', 'cleanup_errors')}
    finally:
        finish_end = clock.analysis_end if clock.analysis_end is not None else min(clock.end - 120, time.monotonic() + 60)
        try:
            clock.arm(finish_end)
            terminal['final_phase'] = 'source'
            if before is not None:
                after = metadata.snapshot_source(source, tree, clock, finish_end)
                evidence.save('source-after.json', {'unchanged': after == before, 'source': after}, terminal=True)
                if after != before: raise Rejected()
            terminal['final_phase'] = 'tools'
            check_helpers(session / 'tools', tools_identity, clock, finish_end)
            final_tools = {'unchanged': True, 'tools': tools_identity}
            for label, path, original in (('bundle', bundle, bundle_before), ('query', query, query_before)):
                if original is not None:
                    actual = tree_identity(path, clock, finish_end)
                    if actual != original: raise Rejected()
                    final_tools[label] = actual
            if shared_before is not None:
                actual = shared_helper_identity(Path(official_closure['codeql/typeinference']['root']),
                                                work / 'candidate-typeinference', clock, finish_end)
                if actual != shared_before: raise Rejected()
                files = actual['candidate_files']
                final_tools['shared'] = {'variant': LIBRARY_VARIANT, 'files': len(files),
                    'bytes': sum(value['bytes'] for value in files.values()), 'tree_sha256': sha256(encoded(files))}
            evidence.save('helpers-after.json', final_tools, terminal=True)
            terminal['final_phase'] = 'toolchain'
            if binary_before is not None:
                binary_after = metadata.analysis_binary_identity(work, clock, finish_end)
                toolchain_after = tree_identity(work / 'rustup/toolchains' / TOOLCHAIN, clock, finish_end)
                if binary_after != binary_before or toolchain_after != toolchain_before: raise Rejected()
                evidence.save('toolchain-after.json', {'unchanged': True, 'binaries': binary_after, 'tree': toolchain_after}, terminal=True)
            terminal['final_phase'] = 'proxy'
            if proxy_before is not None:
                _, _, proxy_after = metadata.proxy_identity(environment, work, clock, finish_end)
                if proxy_after != proxy_before: raise Rejected()
                evidence.save('proxy-after.json', {'unchanged': True, 'proxy': proxy_after}, terminal=True)
            terminal['final_phase'] = 'metadata-manifests'
            if manifests_before is not None and metadata.metadata_manifest_hashes(graph, clock, finish_end) != manifests_before:
                raise Rejected()
            terminal['final_phase'] = 'guard'
            if guard_installed:
                try:
                    verdict = inspect_guard(evidence, guard_deadline, finish_end, source, preflight, check_args)
                except BaseException as exc:
                    verdict = {'status': 'INCOMPLETE', 'error_class': type(exc).__name__}
                evidence.save('guard-verdict.json', verdict, terminal=True)
                terminal['guard'] = verdict['status']
                if verdict['status'] != 'PASS': terminal['status'] = 'WASM_CONTEXT_INCOMPLETE'
            terminal['identity'] = ('PASS' if all(value is not None for value in
                                    (before, bundle_before, query_before, toolchain_before, shared_before)) else 'INCOMPLETE')
            evidence.save('http-observation.json', api.receipts)
            terminal['completed_monotonic'] = time.monotonic()
            terminal['remaining_job_seconds_before_upload'] = clock.remaining()
            terminal['evidence'] = 'SEAL_REQUIRED'
            terminal['final_phase'] = 'seal'
            evidence.save('terminal.json', terminal, terminal=True)
            evidence.seal(clock, finish_end)
        except BaseException as exc:
            terminal['status'] = 'WASM_CONTEXT_INCOMPLETE'; terminal['evidence'] = 'INCOMPLETE'
            terminal['final_error_class'] = type(exc).__name__
            try:
                if not evidence.sealed: evidence.save('terminal.json', terminal, terminal=True)
            except BaseException:
                pass
        finally:
            signal.setitimer(signal.ITIMER_REAL, 0)
    print(json.dumps({key: terminal[key] for key in ('status', 'phase', 'error_class', 'metadata', 'extraction',
          'wasm_role_cfg', 'macros', 'static_callees', 'guard', 'platform_completion', 'full_rust_coverage', 'production_capacity')}, sort_keys=True))
    return 0 if terminal['status'] == 'WASM_CONTEXT_SEMANTICS_OBSERVED' and evidence.sealed else 1


if __name__ == '__main__':
    raise SystemExit('Use the fixed verified workflow bootstrap')
