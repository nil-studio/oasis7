#!/usr/bin/env python3
"""Compare full ECS native builds and one controlled Viewer rebuild on one runner.

This is an opt-in measurement, not the V2 cargo-check gate or ECS packaging.
Both revisions are immutable, disposable Git worktrees with separate external
targets. Registry downloads are prefetched; timed builds are offline and locked.
"""
import argparse
from contextlib import contextmanager
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import signal
import stat
import statistics
import subprocess
import sys
import tempfile
import time
import tomllib

ROOT = Path(__file__).resolve().parent.parent
ROOTS = {
    'oasis7_chain_runtime': 'oasis7_chain_runtime',
    'oasis7_launcher_backend': 'oasis7_game_launcher',
    'oasis7_viewer_runtime': 'oasis7_viewer_live',
}
VIEWER_PATH = 'crates/oasis7_viewer_runtime/src/viewer/runtime_live/world_view_source.rs'
PATCH_BYTES = b'\n// ECS build measurement: deterministic Viewer source-only invalidation.\n'
CODEC_PATH = 'crates/oasis7_viewer_codec/src/lib.rs'
RUNTIME_EVENT_CODEC_PATH = 'crates/oasis7_world_state/src/codec.rs'
PHASES = ('seed', 'no_op', 'viewer_touch')
LOWER_LIBRARIES = ('oasis7_world_state', 'oasis7_world_projection', 'oasis7_runtime',
                   'oasis7_world_service', 'oasis7_game_client')


class MeasurementError(RuntimeError):
    pass


def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def json_digest(value):
    return hashlib.sha256(json.dumps(value, sort_keys=True, separators=(',', ':')).encode()).hexdigest()


def write_json(path, value):
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + '\n')


def runner_resources():
    result = {'platform': platform.platform(), 'machine': platform.machine(),
              'logical_cpu_count': os.cpu_count(), 'runner_name': os.environ.get('RUNNER_NAME'),
              'image': os.environ.get('ImageOS'), 'image_version': os.environ.get('ImageVersion')}
    if hasattr(os, 'sched_getaffinity'):
        result['cpu_affinity'] = sorted(os.sched_getaffinity(0))
    # Host RAM and mounted cgroup limits are separate observations; neither is
    # the measured Cargo process tree's concurrent peak memory consumption.
    meminfo = Path('/proc/meminfo')
    if meminfo.is_file():
        result['host_memory_bytes'] = {name: int(value.split()[0]) * 1024
                                       for line in meminfo.read_text().splitlines()
                                       for name, value in [line.split(':', 1)]
                                       if name in ('MemTotal', 'MemAvailable')}
    cpuinfo = Path('/proc/cpuinfo')
    if cpuinfo.is_file():
        result['cpu_model'] = next((line.split(':', 1)[1].strip()
                                    for line in cpuinfo.read_text().splitlines()
                                    if line.startswith('model name')), None)
    result['cgroup_observations'] = {str(path): path.read_text().strip()
                                     for name in ('cpu.max', 'memory.max', 'memory.current')
                                     for path in [Path('/sys/fs/cgroup') / name] if path.is_file()}
    return result


def output(command, cwd, env=None):
    return subprocess.check_output(command, cwd=cwd, env=env, text=True).strip()


def git(repo, *args):
    return output(['git', *args], repo)


def external(path, repositories, *, parent=False):
    resolved = path.expanduser().resolve()
    if any(resolved.is_relative_to(repo) or (not parent and repo.is_relative_to(resolved))
           for repo in repositories):
        raise MeasurementError('measurement output and work directories must be outside every checkout')
    return resolved


def source_fingerprint(checkout):
    """Include tracked bytes/modes, symlink targets and all untracked inputs."""
    tracked = subprocess.check_output(['git', 'ls-files', '--stage', '-z'], cwd=checkout)
    entries = {}
    for record in tracked.split(b'\0'):
        if record:
            header, raw_path = record.split(b'\t', 1)
            mode, oid, stage = header.decode().split()
            if stage != '0':
                raise MeasurementError('unmerged measurement input')
            entries[os.fsdecode(raw_path)] = {'index_mode': mode, 'index_oid': oid}
    others = subprocess.check_output(['git', 'ls-files', '--others', '-z'], cwd=checkout)
    for raw_path in others.split(b'\0'):
        if raw_path:
            entries.setdefault(os.fsdecode(raw_path), {})
    for name, entry in list(entries.items()):
        path = checkout / name
        if entry.get('index_mode') == '160000' and path.is_dir():
            # Worktrees do not initialize submodules. Still detect any material
            # added to one by a build without trusting the outer repository HEAD.
            entry['submodule_files'] = sorted(str(p.relative_to(path)) for p in path.rglob('*'))
        try:
            info = path.lstat()
        except FileNotFoundError:
            entry['kind'] = 'missing'
            continue
        entry['mode'] = stat.S_IMODE(info.st_mode)
        if stat.S_ISLNK(info.st_mode):
            entry.update(kind='symlink', destination=os.readlink(path))
        elif stat.S_ISREG(info.st_mode):
            entry.update(kind='file', sha256=digest(path))
        else:
            entry.update(kind='directory' if path.is_dir() else 'other')
    return json_digest(entries)


def normalized_env(target, jobs):
    env = dict(os.environ)
    for name in ('RUSTC_WRAPPER', 'RUSTC_WORKSPACE_WRAPPER', 'CARGO_BUILD_TARGET',
                 'CARGO_BUILD_RUSTC_WRAPPER', 'CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER'):
        env.pop(name, None)
    # Caller profile overrides must not silently change the named ECS profile.
    for name in list(env):
        if name.startswith(('CARGO_PROFILE_ECS_', 'CARGO_PROFILE_RELEASE_')):
            del env[name]
    env.update(CARGO_TARGET_DIR=str(target), CARGO_INCREMENTAL='0',
               CARGO_BUILD_JOBS=str(jobs), CARGO_TERM_COLOR='never')
    return env


def build_command():
    return ['cargo', 'build', '--offline', '--locked', '--timings', '--profile', 'ecs',
            '--message-format=json-render-diagnostics',
            *[arg for package in ROOTS for arg in ('-p', package)],
            *[arg for binary in ROOTS.values() for arg in ('--bin', binary)]]


def configuration(checkout, env):
    manifest = tomllib.loads((checkout / 'Cargo.toml').read_text())
    profiles = manifest.get('profile', {})
    ecs = profiles.get('ecs')
    if not isinstance(ecs, dict) or ecs.get('inherits') != 'release':
        raise MeasurementError('both revisions must define the release-derived ecs profile')
    config_files = {}
    # Cargo also reads ancestor configuration and the shared Cargo home.
    candidates = [(str(base), base / '.cargo') for base in (checkout, *checkout.parents)]
    cargo_home = Path(env.get('CARGO_HOME', Path.home() / '.cargo')).resolve()
    candidates.append(('cargo_home', cargo_home))
    for label, directory in candidates:
        for filename in ('config', 'config.toml'):
            path = directory / filename
            if path.is_file():
                key = 'checkout/' + filename if directory == checkout / '.cargo' else label + '/' + filename
                config_files[key] = digest(path)
                value = tomllib.loads(path.read_text())
                build = value.get('build', {})
                if any(build.get(key) for key in ('target', 'rustc', 'rustc-wrapper', 'rustc-workspace-wrapper')):
                    raise MeasurementError('Cargo configuration overrides native target or compiler wrapper')
    compiler = env.get('RUSTC') or env.get('CARGO_BUILD_RUSTC') or 'rustc'
    rustc = output([compiler, '-vV'], checkout, env)
    host = next((line[6:] for line in rustc.splitlines() if line.startswith('host: ')), None)
    if not host or not host.startswith('x86_64-unknown-linux-'):
        raise MeasurementError('ECS native measurement requires an x86_64 Linux Rust host')
    build_env = {key: value for key, value in env.items()
                 if key in ('RUSTUP_TOOLCHAIN', 'RUSTC', 'CARGO_BUILD_RUSTC', 'RUSTDOC', 'RUSTFLAGS',
                            'CARGO_ENCODED_RUSTFLAGS', 'CARGO_INCREMENTAL', 'CARGO_BUILD_JOBS',
                            'CC', 'CXX', 'AR', 'CFLAGS', 'CXXFLAGS', 'CPPFLAGS', 'LDFLAGS',
                            'LIBRARY_PATH', 'CPATH', 'PKG_CONFIG_PATH', 'PKG_CONFIG_LIBDIR')
                 or (key.startswith(('CARGO_TARGET_', 'CC_', 'CXX_', 'AR_')) and key != 'CARGO_TARGET_DIR')}
    return {'rustc': rustc, 'cargo': output(['cargo', '-V'], checkout, env),
            'host': host, 'profile': {'ecs': ecs, 'release': profiles.get('release', {})},
            'features': 'default', 'environment': build_env, 'cargo_configuration': config_files}


def inspect_checkout(checkout, source, target, jobs, out):
    env = normalized_env(target, jobs)
    before = source_fingerprint(checkout)
    config = configuration(checkout, env)
    viewer = checkout / VIEWER_PATH
    if viewer.is_symlink() or not viewer.is_file() or PATCH_BYTES in viewer.read_bytes():
        raise MeasurementError('missing, symbolic or already modified Viewer source fixture: ' + VIEWER_PATH)
    if git(checkout, 'ls-files', '--error-unmatch', '--', VIEWER_PATH) != VIEWER_PATH:
        raise MeasurementError('Viewer source fixture must be tracked')
    with (out / 'prefetch.log').open('w') as log:
        subprocess.run(['cargo', 'fetch', '--locked', '--target', config['host']], cwd=checkout,
                       env=env, stdout=log, stderr=subprocess.STDOUT, check=True)
    metadata = json.loads(output(['cargo', 'metadata', '--offline', '--locked', '--no-deps',
                                  '--format-version=1'], checkout, env))
    packages = {package['id']: package for package in metadata['packages']
                if Path(package['manifest_path']).resolve().is_relative_to(checkout)}
    for package, binary in ROOTS.items():
        owners = [p for p in packages.values() if p['name'] == package]
        if len(owners) != 1 or not any(t['name'] == binary and t['kind'] == ['bin'] for t in owners[0]['targets']):
            raise MeasurementError(f'missing production binary target: {package}/{binary}')
    if source_fingerprint(checkout) != before:
        raise MeasurementError('prefetch/metadata modified measurement source')
    identity = {'source': source, 'source_fingerprint': before, 'configuration': config,
                'cargo_lock_sha256': digest(checkout / 'Cargo.lock'),
                'manifests': {str(Path(p['manifest_path']).relative_to(checkout)): digest(Path(p['manifest_path']))
                              for p in packages.values()},
                'checkout': str(checkout), 'target': str(target),
                'cache_condition': 'empty isolated Cargo target; shared prefetched registry/source cache',
                'remote_workspace_cache': 'not restored'}
    write_json(out / 'identity.json', identity)
    return {'checkout': checkout, 'target': target, 'env': env, 'packages': packages, 'identity': identity}


def read_artifacts(path, fixture):
    units, binaries, finished = [], {}, []
    checkout, target = fixture['checkout'], fixture['target']
    with path.open() as stream:
        for line in stream:
            try:
                message = json.loads(line)
            except ValueError as error:
                raise MeasurementError('non-JSON Cargo stdout; retain raw log for diagnosis') from error
            if message.get('reason') == 'build-finished':
                finished.append(message.get('success'))
            if message.get('reason') != 'compiler-artifact':
                continue
            if type(message.get('fresh')) is not bool:
                raise MeasurementError('Cargo artifact has no boolean Fresh value')
            package = fixture['packages'].get(message.get('package_id'))
            unit = {key: message.get(key) for key in ('package_id', 'target', 'profile', 'features', 'fresh')}
            unit['local_package'] = package['name'] if package else None
            unit['manifest_path'] = (str(Path(package['manifest_path']).relative_to(checkout)) if package else None)
            units.append(unit)
            if package and package['name'] in ROOTS and message['target']['kind'] == ['bin']:
                name = ROOTS[package['name']]
                if message['target']['name'] != name or name in binaries:
                    raise MeasurementError('unexpected or duplicate ECS binary artifact')
                raw = message.get('executable')
                executable = Path(raw) if isinstance(raw, str) else None
                if (executable is None or executable.is_symlink() or not executable.is_file()
                        or executable.resolve() != target / 'ecs' / name
                        or not os.access(executable, os.X_OK)
                        or executable.stat().st_size == 0):
                    raise MeasurementError('missing, empty or misdirected ECS binary artifact: ' + name)
                if message['profile'].get('test') or message['profile'].get('opt_level') != '3':
                    raise MeasurementError('binary artifact does not have the production ECS optimization mode')
                binaries[name] = {'package': package['name'], 'path': str(executable),
                                  'sha256': digest(executable), 'bytes': executable.stat().st_size,
                                  'mode': stat.S_IMODE(executable.stat().st_mode), 'fresh': message['fresh'],
                                  'features': message['features'], 'profile': message['profile']}
    if finished != [True] or set(binaries) != set(ROOTS.values()):
        raise MeasurementError('Cargo success requires build-finished and all three actual native binaries')
    per_crate = {}
    for unit in units:
        if unit['local_package']:
            counts = per_crate.setdefault(unit['local_package'], {'fresh': 0, 'rebuilt': 0})
            counts['fresh' if unit['fresh'] else 'rebuilt'] += 1
    lower = {}
    for name in (*LOWER_LIBRARIES, 'oasis7_viewer_codec'):
        matched = [u for u in units if u['local_package'] == name and 'lib' in u['target']['kind']]
        lower[name] = {'observed_units': len(matched),
                       'all_fresh': all(u['fresh'] for u in matched) if matched else None}
    return {'units': units, 'fresh': sum(u['fresh'] for u in units),
            'rebuilt': sum(not u['fresh'] for u in units), 'local_crates': per_crate,
            'lower_libraries': lower, 'binaries': binaries}


def stop_process(process):
    # Cargo may have exited just before cancellation while a descendant is
    # still alive. Signal its session's group even when the direct child exited.
    try:
        os.killpg(process.pid, signal.SIGTERM)
    except ProcessLookupError:
        pass
    try:
        process.wait(timeout=10)
    except subprocess.TimeoutExpired:
        pass
    finally:
        try:
            os.killpg(process.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        process.wait()


def measure_build(fixture, phase, out, expected_source):
    out.mkdir()
    command = build_command()
    result = {'phase': phase, 'success': False, 'command': command,
              'source': fixture['identity']['source'], 'expected_source_fingerprint': expected_source,
              'started_at': datetime.now(timezone.utc).isoformat()}
    timings = fixture['target'] / 'cargo-timings'
    try:
        if source_fingerprint(fixture['checkout']) != expected_source:
            raise MeasurementError('measurement source changed before ' + phase)
        # Each phase gets only its own timing files, including a failed build.
        # This directory is generated data inside our disposable target.
        if timings.is_symlink():
            raise MeasurementError('unexpected symbolic Cargo timing directory')
        if timings.exists():
            shutil.rmtree(timings)
        print(f"{fixture['identity']['source'][:12]}: {phase} full ECS native build", flush=True)
        with (out / 'cargo.jsonl').open('w') as stdout, (out / 'cargo.stderr.log').open('w') as stderr:
            started = time.monotonic_ns()
            process = subprocess.Popen(command, cwd=fixture['checkout'], env=fixture['env'],
                                       stdout=stdout, stderr=stderr, start_new_session=True)
            try:
                # wait4 is per child, unlike cumulative RUSAGE_CHILDREN. Cargo
                # waits for its compiler/linker descendants before exiting.
                _, status, usage = os.wait4(process.pid, 0)
                process.returncode = os.waitstatus_to_exitcode(status)
                result['exit_code'] = process.returncode
                result['resources'] = {
                    'method': 'Linux wait4 for this Cargo PID and its waited descendants',
                    'user_cpu_seconds': usage.ru_utime, 'system_cpu_seconds': usage.ru_stime,
                    'max_rss_kib': usage.ru_maxrss,
                    'max_rss_semantics': 'largest single-process RSS; not concurrent process-tree total',
                    'minor_page_faults': usage.ru_minflt, 'major_page_faults': usage.ru_majflt,
                    'filesystem_input_blocks': usage.ru_inblock,
                    'filesystem_output_blocks': usage.ru_oublock,
                    'voluntary_context_switches': usage.ru_nvcsw,
                    'involuntary_context_switches': usage.ru_nivcsw,
                }
            except BaseException:
                stop_process(process)
                raise
            finally:
                result['wall_seconds'] = (time.monotonic_ns() - started) / 1_000_000_000
        if result['exit_code'] != 0:
            raise MeasurementError(f'{phase} Cargo build failed with status {result["exit_code"]}')
        if source_fingerprint(fixture['checkout']) != expected_source:
            raise MeasurementError('build modified measurement source during ' + phase)
        result.update(read_artifacts(out / 'cargo.jsonl', fixture))
        if phase == 'no_op' and result['rebuilt'] != 0:
            raise MeasurementError('same-source no-op unexpectedly rebuilt Cargo units')
        viewer_units = [u for u in result['units'] if u['local_package'] == 'oasis7_viewer_runtime'
                        and 'lib' in u['target']['kind']]
        if phase.startswith('viewer_touch') and not any(not u['fresh'] for u in viewer_units):
            raise MeasurementError('Viewer source mutation did not rebuild the Viewer library')
        codec_units = [u for u in result['units'] if u['local_package'] == 'oasis7_viewer_codec'
                       and 'lib' in u['target']['kind']]
        codec_exists = (fixture['checkout'] / CODEC_PATH).is_file()
        if codec_exists and not codec_units:
            raise MeasurementError('new codec is missing from the production Cargo unit set')
        if phase.startswith('viewer_touch') and any(
                observation['all_fresh'] is not True
                for observation in result['lower_libraries'].values()
                if observation['observed_units']):
            raise MeasurementError('Viewer orchestration touch unexpectedly rebuilt a lower library')
        if phase.startswith('viewer_touch') and codec_exists and not all(u['fresh'] for u in codec_units):
            raise MeasurementError('Viewer orchestration touch unexpectedly rebuilt the codec')
        if phase == 'codec_touch' and (not codec_units or all(u['fresh'] for u in codec_units)
                                       or all(u['fresh'] for u in viewer_units)):
            raise MeasurementError('codec touch did not invalidate codec and Viewer consumer')
        if phase == 'runtime_event_codec_touch':
            for name in ('oasis7_world_state', 'oasis7_runtime',
                         'oasis7_viewer_codec', 'oasis7_viewer_runtime'):
                matched = [u for u in result['units'] if u['local_package'] == name
                           and 'lib' in u['target']['kind']]
                if not matched or any(u['fresh'] for u in matched):
                    raise MeasurementError('Runtime Event codec touch did not invalidate ' + name)
            if any(binary['fresh'] for binary in result['binaries'].values()):
                raise MeasurementError('Runtime Event codec touch did not invalidate all production binaries')
        if (timings / 'cargo-timing.html').is_symlink() or not (timings / 'cargo-timing.html').is_file():
            raise MeasurementError('Cargo did not preserve its requested timing HTML')
        result['success'] = True
    except BaseException as error:
        result['error'] = str(error) or type(error).__name__
        raise
    finally:
        try:
            if timings.is_dir() and not timings.is_symlink():
                (out / 'timings').mkdir()
                for path in sorted(timings.glob('*.html')):
                    if path.is_symlink() or not path.is_file():
                        raise MeasurementError('unexpected Cargo timing file type')
                    shutil.copy2(path, out / 'timings' / path.name)
                result['timings'] = sorted(str(p.relative_to(out)) for p in (out / 'timings').iterdir())
            if (out / 'cargo.jsonl').exists():
                with (out / 'diagnostics.log').open('w') as diagnostics:
                    for line in (out / 'cargo.jsonl').read_text().splitlines():
                        try:
                            message = json.loads(line)
                        except ValueError:
                            continue
                        if message.get('reason') == 'compiler-message':
                            diagnostics.write(message.get('message', {}).get('rendered') or '')
        except BaseException as error:
            result.update(success=False, evidence_error=str(error) or type(error).__name__)
            raise
        finally:
            write_json(out / 'metrics.json', result)
    return result


@contextmanager
def viewer_mutation(checkout, out, source_path=VIEWER_PATH):
    path = checkout / source_path
    original, info = path.read_bytes(), path.stat()
    before = source_fingerprint(checkout)
    try:
        path.write_bytes(original + PATCH_BYTES)
        patch = subprocess.check_output(['git', 'diff', '--no-ext-diff', '--no-textconv', '--binary',
                                         '--', source_path], cwd=checkout)
        if not patch:
            raise MeasurementError('Viewer mutation produced no source diff')
        (out / 'viewer.patch').write_bytes(patch)
        mutation = {'kind': 'append_rust_comment_v1', 'path': source_path,
                    'patch_sha256': hashlib.sha256(patch).hexdigest(),
                    'mutation_bytes_sha256': hashlib.sha256(PATCH_BYTES).hexdigest(),
                    'before_sha256': hashlib.sha256(original).hexdigest(), 'after_sha256': digest(path),
                    'source_fingerprint': source_fingerprint(checkout)}
        write_json(out / 'mutation.json', mutation)
        yield mutation
    finally:
        if path.parent.resolve() != path.parent or not path.parent.is_relative_to(checkout):
            raise MeasurementError('Viewer parent changed; refusing to follow a substituted path')
        if path.is_symlink():
            path.unlink()
        path.write_bytes(original)
        path.chmod(stat.S_IMODE(info.st_mode))
        os.utime(path, ns=(info.st_atime_ns, info.st_mtime_ns))
        if source_fingerprint(checkout) != before:
            raise MeasurementError('measurement checkout contains unexpected changes after Viewer restoration')


def compare(results):
    current, baseline = results['current'], results['baseline']
    rows = []
    for phase in PHASES:
        now, old = current['phases'][phase], baseline['phases'][phase]
        for binary in ROOTS.values():
            for key in ('features', 'profile'):
                if now['binaries'][binary][key] != old['binaries'][binary][key]:
                    raise MeasurementError('comparison root binary feature/profile mismatch')
        now_seconds, old_seconds = now['wall_seconds'], old['wall_seconds']
        if phase == 'viewer_touch':
            now_seconds = current.get('viewer_touch_summary', {}).get('median_seconds', now_seconds)
            old_seconds = baseline.get('viewer_touch_summary', {}).get('median_seconds', old_seconds)
        delta = now_seconds - old_seconds
        rows.append({'phase': phase, 'statistic': 'median' if phase == 'viewer_touch' else 'single',
                     'current_seconds': now_seconds, 'baseline_seconds': old_seconds,
                     'delta_seconds': delta,
                     'percent': delta * 100 / old_seconds if old_seconds else None})
    return rows


def measure(args):
    repo = Path(git(args.repo.resolve(), 'rev-parse', '--show-toplevel')).resolve()
    repositories = [Path(line[9:]).resolve() for line in git(repo, 'worktree', 'list', '--porcelain').splitlines()
                    if line.startswith('worktree ')]
    out = external(args.out_dir, repositories)
    work_parent = external(args.work_dir or Path(tempfile.gettempdir()), repositories, parent=True)
    if work_parent.is_relative_to(out):
        raise MeasurementError('temporary work directory must not be inside the report directory')
    if git(repo, 'status', '--porcelain', '--untracked-files=all'):
        raise MeasurementError('source checkout must be clean; only committed revisions are measured')
    revisions = {label: git(repo, 'rev-parse', '--verify', '--end-of-options', ref + '^{commit}')
                 for label, ref in (('current', args.current_ref), ('baseline', args.baseline_ref))}
    out.mkdir(parents=True, exist_ok=False)
    report = {'schema': 1, 'success': False, 'scope': 'ECS native three-binary compile/link only',
              'measurement_order': list(revisions), 'revisions': revisions, 'jobs': args.jobs,
              'profile': 'ecs', 'features': 'default', 'incremental': 0,
              'runner': runner_resources(),
              'limitations': ['one cold build per revision; Viewer samples are sequential within each revision',
                              'shared prefetched Cargo source cache; separate initially empty targets',
                              'no cross-run remote workspace cache measurement',
                              'no Web build, package assembly, execution or tests'], 'results': {}}
    linked = []
    temporary = None
    try:
        work_parent.mkdir(parents=True, exist_ok=True)
        temporary = Path(tempfile.mkdtemp(prefix='oasis7-ecs-build-metrics-', dir=work_parent)).resolve()
        fixtures = {}
        # Prepare both revisions before starting either timed sequence.
        for label, source in revisions.items():
            checkout, target = temporary / label, temporary / (label + '-target')
            subprocess.run(['git', 'worktree', 'add', '--detach', str(checkout), source], cwd=repo, check=True)
            linked.append(checkout)
            target.mkdir()
            (out / label).mkdir()
            fixtures[label] = inspect_checkout(checkout, source, target, args.jobs, out / label)
        if fixtures['current']['identity']['configuration'] != fixtures['baseline']['identity']['configuration']:
            raise MeasurementError('baseline/current toolchain, profile, flags or Cargo configuration differ')
        for label, fixture in fixtures.items():
            record = {'identity': fixture['identity'], 'phases': {}}
            report['results'][label] = record
            before = fixture['identity']['source_fingerprint']
            for phase in ('seed', 'no_op'):
                record['phases'][phase] = measure_build(fixture, phase, out / label / phase, before)
            try:
                with viewer_mutation(fixture['checkout'], out / label) as mutation:
                    record['mutation'] = mutation
                    record['phases']['viewer_touch'] = measure_build(
                        fixture, 'viewer_touch', out / label / 'viewer_touch', mutation['source_fingerprint'])
                for sample in range(2, getattr(args, 'viewer_touch_samples', 1) + 1):
                    sample_out = out / label / f'viewer_touch_{sample}'
                    sample_out.mkdir()
                    with viewer_mutation(fixture['checkout'], sample_out) as mutation:
                        record['phases'][f'viewer_touch_{sample}'] = measure_build(
                            fixture, f'viewer_touch_{sample}', sample_out / 'build',
                            mutation['source_fingerprint'])
                if (fixture['checkout'] / CODEC_PATH).is_file():
                    codec_out = out / label / 'codec_mutation'
                    codec_out.mkdir()
                    with viewer_mutation(fixture['checkout'], codec_out, CODEC_PATH) as mutation:
                        record['codec_mutation'] = mutation
                        record['phases']['codec_touch'] = measure_build(
                            fixture, 'codec_touch', out / label / 'codec_touch',
                            mutation['source_fingerprint'])
                if getattr(args, 'runtime_event_codec_touch', False):
                    event_out = out / label / 'runtime_event_codec_mutation'
                    event_out.mkdir()
                    with viewer_mutation(fixture['checkout'], event_out,
                                         RUNTIME_EVENT_CODEC_PATH) as mutation:
                        record['runtime_event_codec_mutation'] = mutation
                        record['phases']['runtime_event_codec_touch'] = measure_build(
                            fixture, 'runtime_event_codec_touch', out / label / 'runtime_event_codec_touch',
                            mutation['source_fingerprint'])
                cost_source = fixture['checkout'] / 'crates/oasis7_viewer_codec/examples/codec_cost.rs'
                if cost_source.is_file():
                    # Retain all production roots so feature unification matches the timed build.
                    command = [*build_command(), '-p', 'oasis7_viewer_codec', '--example', 'codec_cost']
                    with (out / label / 'codec-cost-build.jsonl').open('w') as log:
                        subprocess.run(command, cwd=fixture['checkout'], env=fixture['env'],
                                       stdout=log, check=True)
                    executable = fixture['target'] / 'ecs/examples/codec_cost'
                    with (out / label / 'codec-cost.json').open('w') as log:
                        subprocess.run(['/usr/bin/time', '-v', '-o',
                                        str(out / label / 'codec-cost-resources.txt'), str(executable)],
                                       cwd=fixture['checkout'], env=fixture['env'], stdout=log, check=True)
                    record['codec_cost'] = {'samples': 'codec-cost.json',
                                            'resources': 'codec-cost-resources.txt',
                                            'binary_sha256': digest(executable)}
                event_cost = fixture['checkout'] / 'crates/oasis7_world_state/examples/event_value_cost.rs'
                if event_cost.is_file():
                    command = [*build_command(), '-p', 'oasis7_world_state',
                               '--example', 'event_value_cost']
                    with (out / label / 'event-value-cost-build.jsonl').open('w') as log:
                        subprocess.run(command, cwd=fixture['checkout'], env=fixture['env'],
                                       stdout=log, check=True)
                    executable = fixture['target'] / 'ecs/examples/event_value_cost'
                    with (out / label / 'event-value-cost.json').open('w') as log:
                        subprocess.run(['/usr/bin/time', '-v', '-o',
                                        str(out / label / 'event-value-cost-resources.txt'), str(executable)],
                                       cwd=fixture['checkout'], env=fixture['env'], stdout=log, check=True)
                    record['event_value_cost'] = {'samples': 'event-value-cost.json',
                                                  'resources': 'event-value-cost-resources.txt',
                                                  'binary_sha256': digest(executable)}
            finally:
                record['source_restored'] = source_fingerprint(fixture['checkout']) == before
        for record in report['results'].values():
            samples = [phase['wall_seconds'] for name, phase in record['phases'].items()
                       if name.startswith('viewer_touch')]
            record['viewer_touch_summary'] = {
                'samples': len(samples), 'execution_order_seconds': samples,
                'median_seconds': statistics.median(samples),
                'min_seconds': min(samples), 'max_seconds': max(samples),
            }
        report['comparison'] = compare(report['results'])
        report['success'] = True
    except BaseException as error:
        report['error'] = str(error) or type(error).__name__
        raise
    finally:
        cleanup_errors = []
        for checkout in reversed(linked):
            removed = subprocess.run(['git', 'worktree', 'remove', '--force', str(checkout)], cwd=repo,
                                     text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
            if removed.returncode:
                cleanup_errors.append(removed.stderr.strip())
        if cleanup_errors:
            report.update(success=False, cleanup_errors=cleanup_errors, preserved_work_dir=str(temporary))
        elif temporary:
            try:
                shutil.rmtree(temporary)
            except OSError as error:
                cleanup_errors.append(str(error))
                report.update(success=False, cleanup_errors=cleanup_errors, preserved_work_dir=str(temporary))
        write_json(out / 'report.json', report)
        if cleanup_errors:
            raise MeasurementError('failed to unregister measurement worktree; preserved ' + str(temporary))
    write_json(out / 'comparison.json', {'schema': 1, 'success': True,
                                        'revisions': revisions, 'rows': report['comparison']})
    lines = ['# ECS native build comparison', '',
             'Full native production root set: Chain runtime, launcher backend and Viewer. '
             'This is compile/link evidence; Web, final package assembly and tests are separate.', '',
             f'- Current: `{revisions["current"]}`', f'- Baseline: `{revisions["baseline"]}`',
             f'- Profile: `ecs`; default features; jobs: `{args.jobs}`; incremental: `0`.',
             '- Each revision uses its own empty target, then the same target for no-op and Viewer touch.',
             '- Shared registry/source downloads are prefetched. No remote workspace overlay is restored.', '',
             '| Phase | Baseline seconds | Current seconds | Delta seconds | Change |',
             '| --- | ---: | ---: | ---: | ---: |']
    for row in report['comparison']:
        percent = f'{row["percent"]:+.2f}%' if row['percent'] is not None else 'n/a'
        lines.append(f'| {row["phase"]} | {row["baseline_seconds"]:.3f} | {row["current_seconds"]:.3f} '
                     f'| {row["delta_seconds"]:+.3f} | {percent} |')
    lines += ['', 'Single sequential samples do not establish a stable speedup or attribute changes solely '
              'to a crate split. `report.json` records manifests/locks, actual unit identities and Fresh/rebuilt '
              'counts per local crate, per-invocation CPU/RSS, lower-library observations and binary hashes. '
              'RSS is the maximum single-process RSS, not the concurrent process-tree total. The controlled comment '
              'patch and before/after hashes distinguish touched artifacts from pristine source artifacts.', '']
    (out / 'summary.md').write_text('\n'.join(lines))
    print('wrote ECS native build measurements to ' + str(out), flush=True)
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--repo', type=Path, default=ROOT)
    parser.add_argument('--current-ref', default='HEAD')
    parser.add_argument('--baseline-ref', required=True)
    parser.add_argument('--out-dir', type=Path, required=True)
    parser.add_argument('--work-dir', type=Path, help='external temporary parent; defaults to system temp')
    parser.add_argument('--viewer-touch-samples', type=int, choices=range(1, 6), default=1)
    parser.add_argument('--runtime-event-codec-touch', action='store_true',
                        help='also verify the existing world_state event codec and consumers invalidate')
    parser.add_argument('--jobs', type=int, choices=(1, 2), default=1)
    args = parser.parse_args()
    def interrupted(signum, _frame):
        raise MeasurementError('measurement interrupted by signal ' + str(signum))
    for signum in (signal.SIGINT, signal.SIGTERM):
        signal.signal(signum, interrupted)
    try:
        measure(args)
    except (OSError, ValueError, KeyError, TypeError, MeasurementError, subprocess.SubprocessError) as error:
        print('ECS build measurement failed: ' + str(error), file=sys.stderr)
        return 1
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
