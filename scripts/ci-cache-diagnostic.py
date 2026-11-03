#!/usr/bin/env python3
"""Read-only native cache experiment; never publish caches or test evidence.

Trace refs: O7-REQ-d9292810-5229-417f-9fc8-262ee04759c7.
"""
import argparse
from collections import Counter
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import selectors
import signal
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parent.parent
LINE_LIMIT = 64 * 1024
STREAM_LIMIT = 16 * 1024 * 1024
SAMPLE_LIMIT = 128
FINGERPRINT_LOG = 'cargo::core::compiler::fingerprint=info,cargo::compiler::fingerprint=info'
# Only discriminants are retained. Paths, environment values, source and raw logs
# may contain private information and are deliberately neither printed nor saved.
REASONS = frozenset(('NothingObvious', 'RustcChanged', 'FeaturesChanged',
    'DeclaredFeaturesChanged', 'TargetConfigurationChanged', 'ProfileConfigurationChanged',
    'PathToSourceChanged', 'CompileKindChanged', 'RustflagsChanged', 'ConfigSettingsChanged',
    'LocalLengthsChanged', 'LocalFingerprintTypeChanged', 'EnvVarChanged',
    'RerunIfChangedOutputPathsChanged', 'RerunIfChangedPathsChanged',
    'PrecalculatedComponentsChanged', 'UnitDependencyInfoChanged', 'UnitDependencyNameChanged',
    'UnitDependencyFingerprintChanged', 'NumberOfDependenciesChanged', 'FsStatusOutdated'))
ENV_KEYS = frozenset(('CARGO_TERM_COLOR', 'CARGO_INCREMENTAL', 'PYTHONDONTWRITEBYTECODE',
    'CI_CACHE_REPOSITORY_BUDGET_BYTES', 'CARGO_NET_RETRY', 'CARGO_REGISTRIES_CRATES_IO_PROTOCOL',
    'CARGO_PROFILE_DEV_DEBUG', 'CARGO_PROFILE_TEST_DEBUG', 'RUST_TOOLCHAIN', 'TRUNK_VERSION',
    'OASIS7_WASM_TOOLCHAIN', 'OASIS7_WASM_TARGET', 'OASIS7_WASM_BUILD_STD',
    'OASIS7_WASM_BUILD_STD_COMPONENTS'))


def workflow_environment(text):
    """Accept only the primary workflow's literal scalar env block; fail on drift."""
    blocks = re.findall(r'^env:\s*\n((?:[ \t].*\n|\n)+)', text, re.MULTILINE)
    if len(blocks) != 1:
        raise ValueError('expected one top-level env block')
    result = {}
    for line in blocks[0].splitlines():
        if not line.strip() or line.lstrip().startswith('#'):
            continue
        match = re.fullmatch(r'  ([A-Z][A-Z0-9_]*):\s*([\w.,/+:-]+|"[\w.,/+:-]+"|\'[\w.,/+:-]+\')\s*', line)
        if not match or match[1] not in ENV_KEYS or match[1] in result:
            raise ValueError('unsupported or duplicate primary workflow environment setting')
        result[match[1]] = match[2].strip('\"\'')
    if set(result) != ENV_KEYS:
        raise ValueError('primary workflow environment changed; review diagnostic parity')
    return result


class Observation:
    def __init__(self, root):
        self.root = root.resolve()
        self.units = Counter()
        self.reasons = Counter()
        self.samples = []
        self.discarded = Counter()
        self.fingerprint_lines = 0

    def line(self, stream, raw):
        text = raw.decode('utf-8', errors='replace')
        if stream == 'stdout':
            try:
                value = json.loads(text)
                if not isinstance(value, dict) or value.get('reason') != 'compiler-artifact':
                    return
                path, fresh = value['manifest_path'], value['fresh']
                if not isinstance(path, str) or type(fresh) is not bool or not Path(path).is_absolute():
                    raise ValueError('invalid compiler artifact')
                source = 'local' if Path(path).is_relative_to(self.root) else 'dependency'
                self.units[source + ('_fresh' if fresh else '_rebuilt')] += 1
            except (ValueError, KeyError, TypeError):
                self.discarded['invalid_stdout_records'] += 1
            return
        if 'compiler::fingerprint' not in text:
            return
        self.fingerprint_lines += 1
        dirty = re.search(r'\bdirty(?: reason)?:\s*([A-Za-z][A-Za-z0-9_]*)', text)
        if dirty:
            kind = dirty[1] if dirty[1] in REASONS else 'unclassified_dirty'
        elif 'fingerprint error for ' in text:
            # An error is not necessarily a miss: do not infer ENOENT or staleness.
            kind = 'fingerprint_error'
        elif 'failed to read ' in text:
            kind = 'fingerprint_read_error'
        else:
            kind = 'unclassified_fingerprint'
        self.reasons[kind] += 1
        if len(self.samples) < SAMPLE_LIMIT:
            sample = {'reason': kind}
            unit = re.search(r'fingerprint (?:dirty|error) for ([A-Za-z0-9_-]{1,128}) v([0-9][A-Za-z0-9.+-]{0,63})(?:[ /]|$)', text)
            if unit:
                sample.update(package=unit[1], version=unit[2])
            self.samples.append(sample)

    def report(self):
        return dict(unit_observations=dict(self.units), fingerprint_line_observations=self.fingerprint_lines,
            reason_line_observations=dict(self.reasons), samples=self.samples,
            discarded=dict(self.discarded), unit_counts_complete=bool(self.units) and not any(
                value for key, value in self.discarded.items() if 'stdout' in key),
            sample_limit=SAMPLE_LIMIT, samples_truncated=self.fingerprint_lines > len(self.samples))


def stop(process):
    # Cargo's compiler/build-script descendants must not outlive a timed-out run.
    try:
        os.killpg(process.pid, signal.SIGTERM)
    except ProcessLookupError:
        pass
    time.sleep(0.1)
    try:
        os.killpg(process.pid, signal.SIGKILL)
    except ProcessLookupError:
        pass
    process.wait()


def observe(command, root, environment, deadline, line_limit=LINE_LIMIT, stream_limit=STREAM_LIMIT):
    if os.name != 'posix':
        raise ValueError('diagnostic process containment requires POSIX')
    observer = Observation(root)
    buffers = {'stdout': b'', 'stderr': b''}
    dropping = dict.fromkeys(buffers, False)
    totals = Counter()
    started = time.monotonic()
    timed_out = False
    process = subprocess.Popen(command, cwd=root, env=environment, stdin=subprocess.DEVNULL,
        stdout=subprocess.PIPE, stderr=subprocess.PIPE, start_new_session=True)
    try:
        with selectors.DefaultSelector() as selector:
            for name in buffers:
                pipe = getattr(process, name)
                os.set_blocking(pipe.fileno(), False)
                selector.register(pipe, selectors.EVENT_READ, name)
            while selector.get_map():
                if time.monotonic() >= deadline:
                    timed_out = True
                    stop(process)
                    break
                for key, _ in selector.select(min(0.1, max(0, deadline - time.monotonic()))):
                    name = key.data
                    chunk = os.read(key.fileobj.fileno(), 8192)
                    if not chunk:
                        if buffers[name] and not dropping[name]:
                            observer.line(name, buffers[name])
                        selector.unregister(key.fileobj)
                        continue
                    old = totals[name]
                    totals[name] += len(chunk)
                    if totals[name] > stream_limit:
                        observer.discarded[name + '_bytes_over_limit'] += len(chunk) - max(0, stream_limit - old)
                        chunk = chunk[:max(0, stream_limit - old)]
                        # Discard a partial record at the byte boundary.
                        if b'\n' not in chunk:
                            buffers[name] = b''
                            dropping[name] = True
                            continue
                        chunk = chunk[:chunk.rfind(b'\n') + 1]
                    for part in chunk.splitlines(keepends=True):
                        complete = part.endswith(b'\n')
                        if not dropping[name]:
                            buffers[name] += part
                            if len(buffers[name]) > line_limit:
                                observer.discarded[name + '_oversized_lines'] += 1
                                buffers[name] = b''
                                dropping[name] = True
                        if complete:
                            if not dropping[name]:
                                observer.line(name, buffers[name])
                            buffers[name] = b''
                            dropping[name] = False
            if not timed_out:
                remaining = max(0.001, deadline - time.monotonic())
                try:
                    process.wait(timeout=remaining)
                except subprocess.TimeoutExpired:
                    timed_out = True
                    stop(process)
    except BaseException:
        stop(process)
        raise
    finally:
        process.stdout.close()
        process.stderr.close()
    report = observer.report()
    report.update(exit_code=124 if timed_out else process.returncode, timed_out=timed_out,
        elapsed_seconds=round(time.monotonic() - started, 3), bytes_observed=dict(totals),
        line_limit=line_limit, stream_limit=stream_limit)
    return report


def run(scope, output_dir, seconds):
    if not 1 <= seconds <= 1800:
        raise ValueError('diagnostic deadline must be between 1 and 1800 seconds')
    config = workflow_environment((ROOT / '.github/workflows/rust.yml').read_text())
    if any(os.environ.get(key) != value for key, value in config.items()):
        raise ValueError('diagnostic environment differs from the primary workflow')
    sha = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip()
    if not re.fullmatch('[0-9a-f]{40}', sha):
        raise ValueError('source HEAD is not an immutable commit')
    if os.environ.get('GITHUB_ACTIONS') == 'true' and sha != os.environ.get('GITHUB_SHA'):
        raise ValueError('checkout does not match the immutable workflow source')
    if subprocess.check_output(['git', 'status', '--porcelain', '--untracked-files=all'], cwd=ROOT):
        raise ValueError('source checkout must be clean')
    spec = importlib.util.spec_from_file_location('native_artifacts', ROOT / 'scripts/ci-test-artifacts.py')
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    variant, args, _ = module.build_variants(scope)[0]
    if args[0] not in ('build', 'test') or (args[0] == 'test' and '--no-run' not in args):
        raise ValueError('diagnostic variants must compile without running tests')
    command = ['cargo', *args, '--locked', '--message-format=json-render-diagnostics']
    env = module.build_environment(scope)
    # Inject after cache restore/identity: this diagnostic setting must not
    # change the cache lookup or the primary workflow's compilation flags.
    env['CARGO_LOG'] = FINGERPRINT_LOG
    report = dict(schema_version=1, scope=scope, variant=variant, source_sha=sha,
        command=command, cargo_version=subprocess.check_output(['cargo', '-V'], cwd=ROOT, text=True).strip(),
        rustc_version=subprocess.check_output(['rustc', '-vV'], cwd=ROOT, text=True).strip(),
        lock_sha256=hashlib.sha256((ROOT / 'Cargo.lock').read_bytes()).hexdigest(),
        dependency_exact_hit={'true': True, 'false': False}.get(os.environ.get('DEPENDENCY_EXACT_HIT')),
        cache_writes=False, test_execution=False, phases=[])
    deadline = time.monotonic() + seconds
    for phase in ('after_restore_attempt', 'same_job_warm'):
        result = observe(command, ROOT, env, deadline)
        report['phases'].append(dict(phase=phase, **result))
        (output_dir / 'build.json').write_text(json.dumps(report, indent=2) + '\n')
        print(json.dumps(dict(phase=phase, variant=variant, **{k: v for k, v in result.items() if k != 'samples'})), flush=True)
        if result['exit_code'] != 0:
            return result['exit_code'] if result['exit_code'] > 0 else 128 - result['exit_code']
    return 0 if all(phase['unit_counts_complete'] for phase in report['phases']) else 2


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('mode', choices=('prepare', 'run'))
    parser.add_argument('--scope', choices=('ordinary', 'formal'), default='formal')
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--seconds', type=int, default=1200)
    args = parser.parse_args()
    try:
        output_dir = args.output.resolve()
        if output_dir == ROOT or output_dir.is_relative_to(ROOT):
            raise ValueError('reports must be outside the source checkout')
        output_dir.mkdir(parents=True, exist_ok=True)
        if args.mode == 'prepare':
            env = workflow_environment((ROOT / '.github/workflows/rust.yml').read_text())
            with open(os.environ['GITHUB_ENV'], 'a') as stream:
                stream.write(''.join(key + '=' + value + '\n' for key, value in sorted(env.items())))
            return 0
        return run(args.scope, output_dir, args.seconds)
    except Exception as error:
        # Do not echo arbitrary exception text containing environment values.
        print('native cache diagnostic failed: ' + type(error).__name__, file=sys.stderr)
        return 1


if __name__ == '__main__':
    raise SystemExit(main())
