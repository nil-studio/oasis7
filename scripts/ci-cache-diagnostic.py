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


def snapshot_outputs(root, limit=100_000, deadline=None):
    """Bounded path-only observation before Cargo; no file contents are read."""
    target = root.resolve() / 'target'
    files = set()
    complete = True
    if type(limit) is not int or limit <= 0:
        raise ValueError('output inventory limit must be positive')
    stop_at = min(time.monotonic() + 5, deadline if deadline is not None else float('inf'))
    if target.is_symlink():
        return target, files, False
    if not target.exists():
        return target, files, True
    if not target.is_dir():
        return target, files, False
    pending, visited = [target], 0
    try:
        while pending:
            with os.scandir(pending.pop()) as entries:
                for entry in entries:
                    visited += 1
                    if visited > limit or time.monotonic() >= stop_at:
                        return target, files, False
                    if entry.is_symlink():
                        complete = False
                    elif entry.is_dir(follow_symlinks=False):
                        pending.append(Path(entry.path))
                    elif entry.is_file(follow_symlinks=False):
                        files.add(Path(entry.path).relative_to(target).as_posix())
                    else:
                        complete = False
    except OSError:
        complete = False
    return target, files, complete


def output_presence(filenames, snapshot):
    """Existence is not validity, provenance, or proof of Cargo freshness."""
    target, files, complete = snapshot
    if not complete or not isinstance(filenames, list) or not filenames or len(filenames) > 64:
        return {'prebuild_outputs': 'unknown'}
    relative = []
    for filename in filenames:
        if not isinstance(filename, str):
            return {'prebuild_outputs': 'unknown'}
        path = Path(filename)
        if (not path.is_absolute() or '..' in path.parts
                or not path.is_relative_to(target)):
            return {'prebuild_outputs': 'unknown'}
        relative.append(path.relative_to(target).as_posix())
    if len(relative) != len(set(relative)):
        return {'prebuild_outputs': 'unknown'}
    present = sum(name in files for name in relative)
    return dict(prebuild_outputs='present' if present == len(relative) else 'partial' if present else 'absent',
                output_count=len(relative), present_output_count=present,
                output_set_sha256=hashlib.sha256('\0'.join(sorted(relative)).encode()).hexdigest())


def safe_unit_identity(value):
    """Keep bounded Cargo names/flags, never package IDs or filesystem paths."""
    target, features, profile = value.get('target'), value.get('features'), value.get('profile')
    if not isinstance(target, dict) or not isinstance(target.get('name'), str):
        return {}
    name, kinds = target['name'], target.get('kind')
    allowed_kinds = {'lib', 'rlib', 'dylib', 'cdylib', 'staticlib', 'proc-macro', 'bin', 'test', 'example', 'bench', 'custom-build'}
    if (re.fullmatch(r'[A-Za-z0-9_-]{1,128}', name) is None
            or not isinstance(kinds, list) or not kinds or len(kinds) > 8
            or any(not isinstance(kind, str) or kind not in allowed_kinds for kind in kinds)
            or not isinstance(features, list) or len(features) > 64
            or any(not isinstance(feature, str) or re.fullmatch(r'[A-Za-z0-9_+.-]{1,128}', feature) is None for feature in features)
            or not isinstance(profile, dict)):
        return {}
    safe_profile = {key: profile[key] for key in ('test', 'debug_assertions', 'overflow_checks')
                    if type(profile.get(key)) is bool}
    level = profile.get('opt_level')
    if isinstance(level, str) and level in ('0', '1', '2', '3', 's', 'z'):
        safe_profile['opt_level'] = level
    return dict(target=name, target_kind=kinds, features=sorted(features), profile=safe_profile)


class Observation:
    def __init__(self, root, snapshot=None):
        self.root = root.resolve()
        self.snapshot = snapshot if snapshot is not None else (self.root / 'target', set(), False)
        self.output_presence = Counter()
        self.rebuilt_units = []
        self.rebuilt_unit_count = 0
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
                if not fresh:
                    presence = output_presence(value.get('filenames'), self.snapshot)
                    self.output_presence[source + '_' + presence['prebuild_outputs']] += 1
                    self.rebuilt_unit_count += 1
                    if sum(sample['source'] == source for sample in self.rebuilt_units) < SAMPLE_LIMIT // 2:
                        self.rebuilt_units.append(dict(source=source, **safe_unit_identity(value), **presence))
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
            prebuild_output_inventory_complete=self.snapshot[2],
            rebuilt_output_observations=dict(self.output_presence), rebuilt_unit_samples=self.rebuilt_units,
            rebuilt_unit_samples_truncated=self.rebuilt_unit_count > len(self.rebuilt_units),
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
    started = time.monotonic()
    snapshot = snapshot_outputs(root, deadline=deadline)
    output_inventory_seconds = round(time.monotonic() - started, 3)
    observer = Observation(root, snapshot)
    buffers = {'stdout': b'', 'stderr': b''}
    dropping = dict.fromkeys(buffers, False)
    totals = Counter()
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
        line_limit=line_limit, stream_limit=stream_limit, output_inventory_seconds=output_inventory_seconds)
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
        print(json.dumps(dict(phase=phase, variant=variant, **{k: v for k, v in result.items() if k not in ('samples', 'rebuilt_unit_samples')})), flush=True)
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
