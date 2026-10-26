#!/usr/bin/env python3
"""One fixed Cargo compile-time dependency check, without CodeQL execution."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import time

import codeql_rust_capacity_evidence as evidence
import codeql_rust_capacity_layout as layout
import codeql_rust_diagnostic_runtime as runtime

SOURCE = 'd2bf383d8c377b36de83aae4a06d9ec17612e205'
MANIFEST = 'tools/oasis7-command-center/Cargo.toml'
CARGO_COMMIT = '5f94df4789f005f9a352888e8355ffc645b7ed0e'
ORIGINAL_ARGV_SHA = 'ae8fc3d8d27bc6d0e9686f6ce5f9932442f35b7713d59fd503a7ffc8779750ed'
CAP = 16 * 1024**2


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
