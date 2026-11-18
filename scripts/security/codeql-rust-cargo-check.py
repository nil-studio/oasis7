#!/usr/bin/env python3
"""One fixed Cargo compile-time dependency check, without CodeQL execution."""
import argparse
from contextlib import contextmanager, nullcontext
import hashlib
import importlib.util
from itertools import islice
import json
import os
from pathlib import Path
import re
import tempfile
import time

import codeql_rust_capacity_evidence as evidence
import codeql_rust_capacity_layout as layout
import codeql_rust_diagnostic_runtime as runtime

SOURCE = 'd2bf383d8c377b36de83aae4a06d9ec17612e205'
MANIFEST = 'tools/oasis7-command-center/Cargo.toml'
CARGO_COMMIT = '5f94df4789f005f9a352888e8355ffc645b7ed0e'
ORIGINAL_ARGV_SHA = 'ae8fc3d8d27bc6d0e9686f6ce5f9932442f35b7713d59fd503a7ffc8779750ed'
CAP = 16 * 1024**2
NATIVE_PACKAGES = ('pkg-config', 'libgtk-3-dev', 'libwebkit2gtk-4.1-dev', 'libdbus-1-dev')
NATIVE_MODULES = ('gdk-3.0 >= 3.22', 'atk >= 2.28', 'dbus-1 >= 1.6',
                  'glib-2.0 >= 2.70', 'cairo >= 1.14', 'gdk-pixbuf-2.0 >= 2.36.8',
                  'pango >= 1.40', 'gobject-2.0 >= 2.70', 'gio-2.0 >= 2.70',
                  'javascriptcoregtk-4.1 >= 2.38', 'libsoup-3.0 >= 3.0')
NATIVE_APT_ROOT = Path('/etc/apt')
UBUNTU_MIRROR = b'mirror+file:/etc/apt/apt-mirrors.txt'
UBUNTU_ARCHIVE = b'https://archive.ubuntu.com/ubuntu/'


def _native_source_files(root, *, mirror=True):
    """Read the complete supported source set; never save its raw contents."""
    parts = root/'sources.list.d'
    if root.resolve() != root or parts.resolve() != parts or not parts.is_dir():
        raise ValueError('unsupported apt source directory')
    with os.scandir(parts) as entries:
        names = [entry.name for entry in islice(entries, 65)]
    if len(names) > 64:
        raise ValueError('apt source listing limit')
    names = sorted(name for name in names if re.fullmatch(r'[A-Za-z0-9_.-]+\.(?:list|sources)', name))
    if len(names) > 32:
        raise ValueError('apt source file limit')
    result = {'sources.list.d/'+name: layout._read_regular(parts/name, 256*1024) for name in names}
    main = root/'sources.list'
    if main.exists() or main.is_symlink():
        result['sources.list'] = layout._read_regular(main, 256*1024)
    if mirror:
        result['apt-mirrors.txt'] = layout._read_regular(root/'apt-mirrors.txt', 16*1024)
    if sum(map(len, result.values())) > 1024*1024:
        raise ValueError('apt source byte limit')
    return result


def _archive_source_candidate(inputs):
    """Replace one supported Noble URI field, retaining all other bytes."""
    candidates = [line.split() for line in inputs['apt-mirrors.txt'].splitlines()
                  if line.strip() and not line.lstrip().startswith(b'#')]
    matches = [items for items in candidates if items[0] == UBUNTU_ARCHIVE]
    if (len(matches) != 1 or len(matches[0]) > 2 or
            (len(matches[0]) == 2 and not re.fullmatch(rb'priority:[0-9]+', matches[0][1]))):
        raise ValueError('official archive is not an unconditional listed candidate')
    if sum(value.count(UBUNTU_MIRROR) for key, value in inputs.items() if key != 'apt-mirrors.txt') != 1:
        raise ValueError('ambiguous Ubuntu mirror source')
    original = inputs.get('sources.list.d/ubuntu.sources')
    if original is None:
        raise ValueError('missing supported Ubuntu source')
    lines = original.splitlines(keepends=True)
    stanzas = []; fields = {}
    for index, line in enumerate(lines):
        raw = line.rstrip(b'\r\n')
        if not raw.strip():
            if fields: stanzas.append(fields); fields = {}
            continue
        if raw.lstrip().startswith(b'#'): continue
        match = re.fullmatch(rb'([A-Za-z0-9-]+):[ \t]*(.*)', raw)
        if not match:
            raise ValueError('unsupported Ubuntu source field or continuation')
        key = match[1].lower()
        if key in fields:
            raise ValueError('duplicate Ubuntu source field')
        fields[key] = (match[2].strip(), index)
    if fields: stanzas.append(fields)
    targets = [item for item in stanzas if item.get(b'uris', (None,))[0] == UBUNTU_MIRROR]
    if len(targets) != 1:
        raise ValueError('missing unique Ubuntu mirror URI field')
    target = targets[0]
    value = lambda key: target.get(key, (b'',))[0]
    if (value(b'types') != b'deb' or
            sorted(value(b'suites').split()) != [b'noble', b'noble-backports', b'noble-updates'] or
            sorted(value(b'components').split()) != [b'main', b'multiverse', b'restricted', b'universe'] or
            value(b'signed-by') != b'/usr/share/keyrings/ubuntu-archive-keyring.gpg' or
            value(b'enabled').lower() not in (b'', b'yes')):
        raise ValueError('unsupported Ubuntu Noble source layout')
    index = target[b'uris'][1]
    lines[index] = lines[index].replace(UBUNTU_MIRROR, UBUNTU_ARCHIVE, 1)
    effective = {key: data for key, data in inputs.items() if key != 'apt-mirrors.txt'}
    effective.setdefault('sources.list', b'')
    effective['sources.list.d/ubuntu.sources'] = b''.join(lines)
    return effective


def _source_digests(files):
    return {name: hashlib.sha256(data).hexdigest() for name, data in sorted(files.items())}


@contextmanager
def _https_archive_sources(out):
    """One private source override, with before/after identity and cleanup."""
    receipt = {'schema_version': 1, 'source_profile': 'ubuntu-archive-https',
               'maximum_native_seconds': 180, 'started_monotonic': time.monotonic(),
               'target': UBUNTU_ARCHIVE.decode(), 'replacement_count': 0,
               'status': 'INCOMPLETE', 'system_sources_unchanged': None,
               'private_sources_unchanged': None, 'cleanup': 'NOT_CREATED'}
    private = None; inputs = None; effective = None; primary_error = None
    try:
        inputs = _native_source_files(NATIVE_APT_ROOT)
        receipt['input_sha256'] = _source_digests(inputs)
        effective = _archive_source_candidate(inputs)
        receipt.update(effective_sha256=_source_digests(effective), replacement_count=1)
        private = tempfile.TemporaryDirectory(prefix='native-apt-', dir=out.parent)
        root = Path(private.name)
        (root/'sources.list.d').mkdir()
        for name, data in effective.items():
            with (root/name).open('xb') as stream: stream.write(data)
        receipt.update(status='PREPARED', cleanup='PENDING')
        runtime.save(out/'native-source-profile.json', receipt, out)
        yield ['-o', 'Dir::Etc::sourcelist='+str(root/'sources.list'),
               '-o', 'Dir::Etc::sourceparts='+str(root/'sources.list.d')]
    except BaseException as exc:
        primary_error = exc
        raise
    finally:
        errors = []
        if inputs is not None:
            try:
                after = _native_source_files(NATIVE_APT_ROOT)
                receipt['input_after_sha256'] = _source_digests(after)
                receipt['system_sources_unchanged'] = after == inputs
            except Exception as exc: errors.append({'stage':'system-identity', 'type':type(exc).__name__})
        if private is not None:
            try:
                after = _native_source_files(Path(private.name), mirror=False)
                receipt['effective_after_sha256'] = _source_digests(after)
                receipt['private_sources_unchanged'] = after == effective
            except Exception as exc: errors.append({'stage':'private-identity', 'type':type(exc).__name__})
            try:
                private.cleanup()
                receipt['cleanup'] = 'REMOVED'
            except Exception as exc:
                receipt['cleanup'] = 'FAILED'
                errors.append({'stage':'cleanup', 'type':type(exc).__name__})
        complete = (receipt['system_sources_unchanged'] is True and
                    receipt['private_sources_unchanged'] is True and receipt['cleanup'] == 'REMOVED')
        receipt.update(status='VERIFIED' if complete and not errors else 'INCOMPLETE', errors=errors)
        receipt['completed_monotonic'] = time.monotonic()
        try:
            runtime.save(out/'native-source-profile.json', receipt, out, terminal=True)
        except Exception as exc:
            if primary_error is None: raise
            primary_error.native_source_evidence_error = {'stage':'final-source-receipt', 'type':type(exc).__name__}
        if not complete and primary_error is None:
            raise ValueError('native source identity or cleanup failed')


def prepare_native(source, out, end, env, *, maximum_seconds=180, observation=None,
                   source_profile='runner-sources'):
    """Repair the observed native prerequisites within the original job clock."""
    if type(maximum_seconds) is not int or maximum_seconds not in (180, 360):
        raise ValueError('unsupported native preparation budget')
    if source_profile not in ('runner-sources', 'ubuntu-archive-https'):
        raise ValueError('unsupported native source profile')
    if source_profile == 'ubuntu-archive-https' and (maximum_seconds != 180 or observation is None):
        raise ValueError('source candidate requires the observed 180-second diagnostic')
    started = time.monotonic()
    preparation_end = min(end - 90, started + maximum_seconds)
    budget = {'maximum_seconds': maximum_seconds, 'started_monotonic': started,
              'parent_deadline_monotonic': end, 'parent_reserve_seconds': 90,
              'preparation_deadline_monotonic': preparation_end,
              'probe_reserve_seconds': 20, 'privileged_cleanup_reserve_seconds': 5,
              'scope': 'allocations only; actual outcomes are in native-*.stdout.result.json',
              'allocations': []}
    if observation is not None: budget['source_profile'] = source_profile
    runtime.save(out / 'native-budget.json', budget, out)

    def record(name, seconds, privileged_seconds=None):
        budget['allocations'].append({'phase': name, 'timeout_seconds': seconds,
                                      'privileged_timeout_seconds': privileged_seconds})
        runtime.save(out / 'native-budget.json', budget, out)

    sources = _https_archive_sources(out) if source_profile == 'ubuntu-archive-https' else nullcontext([])
    with sources as source_options:
        for name, args, limit in (
                ('native-update', ['update'], 60),
                ('native-install', ['install', '-y', '--no-install-recommends', *NATIVE_PACKAGES], maximum_seconds)):
            # Source setup and both probes share this original preparation window.
            remaining = min(limit, preparation_end - time.monotonic() - 20)
            if remaining < 10:
                raise RuntimeError('insufficient native preparation budget')
            # The privileged timeout terminates its own apt children before our outer watchdog.
            record(name, remaining, int(remaining) - 5)
            argv = ['sudo', '-n', 'timeout', '--signal=TERM', '--kill-after=2s',
                    str(int(remaining) - 5), 'env', 'DEBIAN_FRONTEND=noninteractive',
                    'apt-get', '-o', 'Acquire::Retries=0', '-o', 'Acquire::http::Timeout=20',
                    *source_options, *(['-o', 'Debug::pkgAcquire::Worker=1'] if observation else []), *args]
            observed = {'observer': observation.phase(name)} if observation else {}
            runtime.run(argv, source, out / (name + '.stdout'), preparation_end,
                        remaining, out, env, file_limit=None, **observed)
        for name, argv in (
                ('native-packages', ['dpkg-query', '-W', '-f=${binary:Package}\t${Version}\n', *NATIVE_PACKAGES]),
                ('native-modules', ['pkg-config', '--print-errors', '--modversion', *NATIVE_MODULES])):
            remaining = min(10, preparation_end - time.monotonic())
            if remaining < 1:
                raise RuntimeError('insufficient native observation budget')
            record(name, remaining)
            observed = {'observer': observation.phase(name)} if observation else {}
            runtime.run(argv, source, out / (name + '.stdout'), preparation_end,
                        remaining, out, env, file_limit=None, **observed)

    if source_profile == 'ubuntu-archive-https' and time.monotonic() > preparation_end:
        raise RuntimeError('native source candidate exceeded its preparation deadline')


def check_argv(source, work):
    return ['check', '--quiet', '--workspace', '--message-format=json',
            '--ignore-rust-version', '--manifest-path', str(source / MANIFEST),
            '--target-dir', str(work / 'target'), '--all-features', '--keep-going',
            '--compile-time-deps', '--all-targets', '-Zunstable-options']


def version_matches(text):
    return all(line in text.splitlines() for line in (
        'release: 1.99.0', 'commit-hash: ' + CARGO_COMMIT,
        'host: x86_64-unknown-linux-gnu'))


def single_check(source, work, out, end, env, cargo='cargo', observation=None):
    observation = observation if observation is not None else {}
    # RA 0.0.357 / Rust >=1.97 copies an existing lock and redirects through this env.
    lock = source / Path(MANIFEST).with_name('Cargo.lock')
    original = layout._read_regular(lock, 1024**2)
    copied = work / 'Cargo.lock'
    with copied.open('xb') as stream:
        stream.write(original)
    child_env = dict(env, CARGO_RESOLVER_LOCKFILE_PATH=str(copied),
                     __CARGO_TEST_CHANNEL_OVERRIDE_DO_NOT_USE_THIS='nightly',
                     RUSTUP_TOOLCHAIN='1.99.0', RUST_BACKTRACE='1')
    for key in ('RUSTC_WRAPPER', 'GH_TOKEN', 'GITHUB_TOKEN'):
        child_env.pop(key, None)
    argv = check_argv(source, work) + ['--locked']
    observation.update(check_attempts=1, check_invocations='UNKNOWN_PENDING_RESULT',
                       argv=argv, locked=True)
    result = None
    try:
        result = runtime.run([cargo, *argv], source / Path(MANIFEST).parent,
                             out / 'check.stdout', end - 30,
                             min(600, max(0, end - 30 - time.monotonic())),
                             out, child_env, file_limit=None)
    except runtime.PhaseError as error:
        result = error.result
    # Preserve the actual child result before any fallible post-check inspection.
    observation.update(process=result, argv=argv, locked=True,
                       check_invocations=int(result.get('pid') is not None),
                       lock_sha256=hashlib.sha256(original).hexdigest())
    for name, path in (('source_lock_unchanged', lock), ('copied_lock_unchanged', copied)):
        try:
            observation[name] = layout._read_regular(path, 1024**2) == original
        except Exception as error:
            observation[name] = None
            observation.setdefault('lock_errors', {})[name] = type(error).__name__
    return observation


def rustc_matches(text):
    return all(line in text.splitlines() for line in (
        'release: 1.99.0', 'host: x86_64-unknown-linux-gnu'))


def check_status(process):
    if not process.get('error') and not process.get('cleanup_errors') and process['exit'] == 0:
        return 'CHECK_SUCCEEDED'
    return 'CHECK_FAILED' if process.get('failure_kind') == 'child_exit' else 'CHECK_INCOMPLETE'


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('source', 'work', 'output'):
        parser.add_argument('--' + name, type=Path, required=True)
    parser.add_argument('--deadline', type=float, required=True)
    args = parser.parse_args()
    source, work, out = (p.absolute() for p in (args.source, args.work, args.output))
    if any(p.resolve() != p for p in (source, work, out)):
        raise SystemExit('unsafe diagnostic paths')
    if work.is_relative_to(source) or out.is_relative_to(source):
        raise SystemExit('diagnostic output must be outside business source')
    out.mkdir(exist_ok=True)
    work.mkdir(exist_ok=False)
    runtime.CAP, runtime.WRITE_CAP = CAP, CAP - 1024**2
    end = time.monotonic() + max(0, args.deadline - time.time())
    terminal = {'status': 'PREPARATION_FAILED', 'source_sha': SOURCE,
                'fixed_deadline': args.deadline, 'check_invocations': 0,
                'production_capacity': 'UNKNOWN', 'rust_coverage': 'HOLD',
                'native_prerequisites': 'NOT_REACHED',
                'native_cleanup_scope': 'Unprivileged helper group observations do not certify the privileged apt subtree empty',
                'scope': 'single fresh-runner check, not replay of the original shared target history',
                'security_upload': False, 'database_upload': False}
    env = {k: v for k, v in os.environ.items() if k not in ('GH_TOKEN', 'GITHUB_TOKEN', 'RUSTC_WRAPPER')}
    env['RUSTUP_TOOLCHAIN'] = '1.99.0'
    before = None
    snapshot = None
    try:
        if not 60 < end - time.monotonic() <= 900:
            raise RuntimeError('invalid remaining diagnostic budget')
        identity = evidence.capture_identity(source, out, end - 30, SOURCE, prefix='before')
        if identity['status'] != 'PASS':
            raise RuntimeError('business source identity rejected')
        spec = importlib.util.spec_from_file_location('capacity', Path(__file__).with_name('codeql-rust-capacity.py'))
        capacity = importlib.util.module_from_spec(spec); spec.loader.exec_module(capacity)
        snapshot = capacity.snapshot
        before = snapshot(source, end=end - 30)
        runtime.save(out / 'source-before.json', before, out)
        runtime.run(['cargo', '-Vv'], source, out / 'cargo-version.stdout', end - 30, 10, out, env, file_limit=None)
        if not version_matches((out / 'cargo-version.stdout').read_text()):
            raise RuntimeError('Cargo differs from recorded analysis version')
        runtime.run(['rustc', '-vV'], source, out / 'rustc-version.stdout', end - 30, 10, out, env, file_limit=None)
        if not rustc_matches((out / 'rustc-version.stdout').read_text()):
            raise RuntimeError('unexpected rustc release or host')
        terminal['rustc_historical_commit'] = 'UNKNOWN_NOT_RECORDED_IN_ORIGINAL_CHECK'
        canonical = check_argv(Path('/home/runner/work/oasis7/oasis7'),
                               Path('/home/runner/work/_temp/rust-capacity-databases/rust/working'))
        if hashlib.sha256(json.dumps(canonical).encode()).hexdigest() != ORIGINAL_ARGV_SHA:
            raise RuntimeError('recorded invocation reconstruction differs')
        terminal['native_prerequisites'] = 'INCOMPLETE'
        prepare_native(source, out, end, env)
        terminal['native_prerequisites'] = 'PASS'
        single_check(source, work, out, end, env, observation=terminal)
        terminal['status'] = check_status(terminal['process'])
    except Exception as error:
        if terminal.get('check_attempts'):
            terminal['status'] = 'CHECK_INCOMPLETE'
        terminal['error'] = type(error).__name__ + ': ' + str(error)[:2048]
    finally:
        if before is not None:
            try:
                after = snapshot(source, end=end - 10)
                terminal['source_changes'] = [n for n in before if after.get(n) != before[n]]
                identity = evidence.capture_identity(source, out, end - 10, SOURCE, prefix='after')
                terminal['source_identity'] = identity['status']
            except Exception as error:
                terminal['source_identity'] = 'UNKNOWN'
                terminal['source_error'] = type(error).__name__
        runtime.save(out / 'terminal.json', terminal, out, terminal=True)
        hashes = {}
        for path in sorted(out.rglob('*')):
            if path.is_symlink() or not (path.is_file() or path.is_dir()):
                raise RuntimeError('unsafe evidence member')
            if path.is_file() and path.name != 'hashes.json':
                hashes[str(path.relative_to(out))] = runtime.digest(path)
        runtime.save(out / 'hashes.json', hashes, out, terminal=True)
    return 0 if (terminal['status'] == 'CHECK_SUCCEEDED' and terminal.get('source_identity') == 'PASS'
                 and not terminal.get('source_changes') and terminal.get('copied_lock_unchanged')
                 and terminal.get('source_lock_unchanged')) else 1


if __name__ == '__main__':
    raise SystemExit(main())
