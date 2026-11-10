#!/usr/bin/env python3
"""One native preparation diagnostic, without Rust, CodeQL or business extraction."""
import argparse
import importlib.util
import json
import math
import os
from pathlib import Path
import re
import subprocess
import time

import codeql_rust_capacity_layout as layout
import codeql_rust_diagnostic_runtime as runtime
import codeql_rust_native_observer as observation

spec = importlib.util.spec_from_file_location('native_prepare', Path(__file__).with_name('codeql-rust-cargo-check.py'))
native = importlib.util.module_from_spec(spec); spec.loader.exec_module(native)
CAP = 16 * 1024**2
HELPERS = ('codeql-rust-native-diagnostic.py', 'codeql_rust_native_observer.py',
           'codeql-rust-cargo-check.py', 'codeql_rust_diagnostic_runtime.py',
           'codeql_rust_capacity_evidence.py', 'codeql_rust_capacity_layout.py')


def identity(source, expected_sha, env):
    git = subprocess.run(['git', 'rev-parse', 'HEAD'], cwd=source, env=env,
                         capture_output=True, timeout=5, check=True)
    if git.stdout.decode().strip() != expected_sha:
        raise ValueError('unexpected helper checkout')
    status = subprocess.run(['git', 'status', '--porcelain', '--untracked-files=no'],
                            cwd=source, env=env, capture_output=True, timeout=5, check=True)
    if status.stdout: raise ValueError('modified helper checkout')
    import hashlib
    return {'tool_sha': expected_sha, 'helper_sha256': {
        name: hashlib.sha256(layout._read_regular(source/'scripts/security'/name)).hexdigest() for name in HELPERS},
        'workflow_sha256': hashlib.sha256(layout._read_regular(source/'.github/workflows/codeql-rust-native-diagnostic.yml')).hexdigest(),
        'runner': {key: env[key] for key in ('ImageOS', 'ImageVersion', 'RUNNER_OS', 'RUNNER_ARCH')
                   if key in env and re.fullmatch(r'[A-Za-z0-9_.-]{1,100}', env[key])},
        'scope': 'Native preparation only. This helper checkout is not a business extraction source identity.'}


def diagnose(source, out, deadline, expected_sha, env):
    runtime.CAP, runtime.WRITE_CAP = CAP, CAP-1024**2
    now = time.time()
    end = time.monotonic() + deadline - now
    child_env = {key: value for key, value in env.items() if key not in ('GH_TOKEN', 'GITHUB_TOKEN', 'RUSTC_WRAPPER')}
    controller = observation.NativeObservation(out, child_env)
    terminal = {'status': 'NATIVE_INCOMPLETE', 'started_wall': now, 'deadline_wall': deadline,
                'maximum_job_seconds': 600, 'maximum_native_seconds': 360,
                'production_capacity': 'UNKNOWN', 'full_rust_coverage': 'HOLD',
                'rust_codeql_execution': 'NOT_REQUESTED', 'error': None}
    before = None; stage = 'admission'
    try:
        if not math.isfinite(deadline) or not 0 < deadline-now <= 600:
            raise ValueError('invalid remaining job clock')
        if not re.fullmatch('[0-9a-f]{40}', expected_sha):
            raise ValueError('invalid helper identity')
        stage = 'identity'
        before = identity(source, expected_sha, child_env)
        runtime.save(out/'identity.json', before, out)
        stage = 'source-observation'
        runtime.save(out/'sources.json', observation.source_snapshot(), out)
        runtime.save(out/'proxy-environment.json', {key: {'present': True, 'endpoint': observation.uri(child_env[key])}
            for key in ('http_proxy', 'https_proxy', 'HTTP_PROXY', 'HTTPS_PROXY', 'ALL_PROXY') if key in child_env}, out)
        stage = 'apt-version'
        runtime.run(['apt-get', '--version'], source, out/'apt-version.stdout', end-90, 5,
                    out, child_env, file_limit=None, observer=controller.phase(stage))
        stage = 'native-preparation'
        native.prepare_native(source, out, end, child_env, maximum_seconds=360, observation=controller)
        terminal['status'] = 'NATIVE_READY'
    except Exception as exc:
        terminal['error'] = {'stage': stage, 'type': type(exc).__name__}
        if isinstance(exc, runtime.PhaseError):
            terminal['error'].update(exit=exc.result.get('exit'), failure_kind=exc.result.get('failure_kind'))
    finally:
        try:
            if before is not None:
                after = identity(source, expected_sha, child_env)
                runtime.save(out/'identity-after.json', {'unchanged': before == after, 'identity': after}, out, terminal=True)
                if before != after: raise ValueError('helper identity changed')
            runtime.save(out/'observations.json', controller.summary(), out, terminal=True)
        except Exception as exc:
            terminal['evidence_error'] = type(exc).__name__
            terminal['status'] = 'NATIVE_INCOMPLETE'
        terminal['completed_wall'] = time.time()
        runtime.save(out/'native-terminal.json', terminal, out, terminal=True)
        try:
            hashes = {path.name: runtime.digest(path) for path in sorted(out.iterdir()) if path.is_file()}
            runtime.save(out/'sha256.json', hashes, out, terminal=True)
        except Exception as exc:
            terminal.update(status='NATIVE_INCOMPLETE', seal_error=type(exc).__name__)
            runtime.save(out/'native-terminal.json', terminal, out, terminal=True)
    print(json.dumps(terminal, sort_keys=True))
    return 0 if terminal['status'] == 'NATIVE_READY' else 1


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--deadline', type=float, required=True)
    parser.add_argument('--tool-sha', required=True)
    args = parser.parse_args()
    source, out = args.source.absolute(), args.output.absolute()
    if source.resolve() != source or out.resolve() != out or source == out or source in out.parents:
        parser.error('evidence must be a separate non-symlink directory')
    out.mkdir(parents=True, exist_ok=True)
    if any(p.name != 'job-clock.json' or not p.is_file() or p.is_symlink() for p in out.iterdir()):
        parser.error('evidence directory is not fresh')
    return diagnose(source, out, args.deadline, args.tool_sha, dict(os.environ))


if __name__ == '__main__':
    raise SystemExit(main())
