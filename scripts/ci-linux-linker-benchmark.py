#!/usr/bin/env python3
"""Replay one real, frozen native link with Rust's LLD and the CI mold.

This manual experiment measures linker invocation wall time, not a Cargo or
whole-CI speedup. A single untimed-for-comparison Cargo preparation produces an
LLD reproduce archive; every sample links the exact same archived inputs.
"""
import argparse
import hashlib
import importlib.util
import json
import math
import os
from pathlib import Path, PurePosixPath
import shlex
import shutil
import signal
import statistics
import subprocess
import sys
import tarfile
import tempfile
import time
import tomllib

ROOT = Path(__file__).resolve().parent.parent
PACKAGE = 'oasis7_newapi_bridge_service'
HOST = 'x86_64-unknown-linux-gnu'
OUTPUT = 'linker-benchmark-output.elf'
SPEC = importlib.util.spec_from_file_location('ecs_metrics', ROOT / 'scripts/ci-ecs-build-metrics.py')
COMMON = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(COMMON)
MeasurementError = COMMON.MeasurementError
digest = COMMON.digest
json_digest = COMMON.json_digest
write_json = COMMON.write_json


def invoke(command, cwd, env, deadline, prefix, *, limit=None):
    """Wait for both exit and pipe EOF, including mold's default fork child."""
    remaining = deadline - time.monotonic()
    if remaining <= 0:
        raise MeasurementError('measurement deadline exhausted before invocation')
    timeout = min(remaining, limit) if limit is not None else remaining
    result = {'command': list(map(str, command)), 'cwd': str(cwd), 'success': False}
    started = time.monotonic_ns()
    process = subprocess.Popen(command, cwd=cwd, env=env, stdout=subprocess.PIPE,
                               stderr=subprocess.PIPE, start_new_session=True)
    error = None
    try:
        stdout, stderr = process.communicate(timeout=timeout)
    except BaseException as caught:
        error = caught
        COMMON.stop_process(process)
        stdout, stderr = process.communicate()
    result.update(wall_seconds=(time.monotonic_ns() - started) / 1e9,
                  returncode=process.returncode, timed_out=isinstance(error, subprocess.TimeoutExpired),
                  success=error is None and process.returncode == 0,
                  stdout_log=prefix.name + '.stdout.log', stderr_log=prefix.name + '.stderr.log')
    # Disk writes and artifact validation do not enter invocation wall time.
    prefix.with_suffix('.stdout.log').write_bytes(stdout)
    prefix.with_suffix('.stderr.log').write_bytes(stderr)
    write_json(prefix.with_suffix('.invocation.json'), result)
    if error is not None and not isinstance(error, subprocess.TimeoutExpired):
        raise error
    return result, stdout, stderr


def require_success(result, phase):
    if not result['success']:
        raise MeasurementError(f'{phase} failed (exit={result.get("returncode")}, '
                               f'timeout={result.get("timed_out")}); retained invocation logs')


def normalized_env(target, jobs):
    env = dict(os.environ)
    for name in list(env):
        if (name in ('RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS', 'RUSTC', 'RUSTC_WRAPPER',
                     'RUSTC_WORKSPACE_WRAPPER', 'CARGO_BUILD_RUSTC', 'CARGO_BUILD_TARGET',
                     'CARGO_BUILD_RUSTFLAGS', 'CARGO_BUILD_RUSTC_WRAPPER',
                     'CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER', 'LLD_REPRODUCE', 'MOLD_REPRO')
                or name.startswith('CARGO_PROFILE_')
                or (name.startswith('CARGO_TARGET_') and name.endswith(('_LINKER', '_RUSTFLAGS')))):
            env.pop(name, None)
    env.update(CARGO_TARGET_DIR=str(target), CARGO_INCREMENTAL='0',
               CARGO_PROFILE_DEV_DEBUG='0', CARGO_PROFILE_TEST_DEBUG='0',
               CARGO_PROFILE_DEV_LTO='false', CARGO_BUILD_JOBS=str(jobs), CARGO_TERM_COLOR='never')
    return env


def configuration(checkout, env):
    configs = {}
    homes = [base / '.cargo' for base in (checkout, *checkout.parents)]
    homes.append(Path(env.get('CARGO_HOME', Path.home() / '.cargo')).resolve())
    for directory in dict.fromkeys(homes):
        for name in ('config', 'config.toml'):
            path = directory / name
            if not path.is_file():
                continue
            value = tomllib.loads(path.read_text())
            build = value.get('build', {})
            if any(build.get(k) for k in ('target', 'rustc', 'rustflags', 'rustc-wrapper',
                                          'rustc-workspace-wrapper')):
                raise MeasurementError('Cargo configuration overrides the compiler, flags or native target')
            if any(t.get('linker') or t.get('rustflags') for t in value.get('target', {}).values()):
                raise MeasurementError('Cargo configuration overrides the default linker or Rust flags')
            if value.get('profile'):
                raise MeasurementError('Cargo configuration overrides the measurement profile')
            for key in value.get('env', {}):
                if key.startswith(('RUST', 'CARGO_', 'LLD_', 'MOLD_')):
                    raise MeasurementError('Cargo config environment changes measurement tools or flags')
            configs[str(path)] = digest(path)
    keys = ('CC', 'CXX', 'AR', 'CFLAGS', 'CXXFLAGS', 'CPPFLAGS', 'LDFLAGS', 'LIBRARY_PATH',
            'CPATH', 'PKG_CONFIG_PATH', 'PKG_CONFIG_LIBDIR', 'RUSTUP_TOOLCHAIN', 'CARGO_INCREMENTAL',
            'CARGO_PROFILE_DEV_DEBUG', 'CARGO_PROFILE_TEST_DEBUG', 'CARGO_PROFILE_DEV_LTO',
            'CARGO_BUILD_JOBS')
    return {'cargo_configs': configs, 'environment': {k: env[k] for k in keys if k in env},
            'manifest_profiles': tomllib.loads((checkout / 'Cargo.toml').read_text()).get('profile', {})}


def unpack_inputs(archive_path, destination):
    """Extract only regular files/directories from our LLD-produced archive."""
    seen, total = set(), 0
    with tarfile.open(archive_path, 'r:') as archive:
        for member in archive:
            relative = PurePosixPath(member.name)
            if (relative.is_absolute() or '..' in relative.parts or not relative.parts
                    or member.name in seen or not (member.isfile() or member.isdir())):
                raise MeasurementError('unsafe, duplicate or non-regular reproduce archive member')
            seen.add(member.name)
            total += member.size
            if len(seen) > 50000 or total > 8 * 1024 ** 3:
                raise MeasurementError('reproduce archive exceeds the bounded input budget')
            target = destination.joinpath(*relative.parts)
            if member.isdir():
                target.mkdir(parents=True, exist_ok=True)
            else:
                target.parent.mkdir(parents=True, exist_ok=True)
                with archive.extractfile(member) as source, target.open('xb') as output:
                    shutil.copyfileobj(source, output)
    responses = list(destination.glob('*/response.txt'))
    if len(responses) != 1:
        raise MeasurementError('reproduce archive must contain one root response.txt')
    response = responses[0]
    arguments = shlex.split(response.read_text(), comments=False)
    if arguments[:2] != ['--chroot', '.']:
        raise MeasurementError('reproduce response must close absolute inputs with --chroot .')
    if any(arg.startswith('@') or '..' in PurePosixPath(arg).parts for arg in arguments):
        raise MeasurementError('unexpanded response or escaping input in reproduce response')
    if any(arg.startswith(('--reproduce', '--repro', '--thinlto', '--lto-')) for arg in arguments):
        raise MeasurementError('reproduction or linker-LTO option is outside native replay scope')
    if (response.parent / OUTPUT).exists():
        raise MeasurementError('measurement output collides with an archived input')
    return response.parent


def input_manifest(directory):
    result = {}
    for path in sorted(directory.rglob('*')):
        relative = path.relative_to(directory).as_posix()
        if relative == OUTPUT:
            continue
        if path.is_symlink() or not (path.is_dir() or path.is_file()):
            raise MeasurementError('replay inputs contain a symbolic or special file')
        if path.is_file():
            result[relative] = {'bytes': path.stat().st_size, 'sha256': digest(path)}
    return result


def verify_elf(path, expected_linker, env, deadline, prefix, help_output=None):
    if path.is_symlink() or not path.is_file() or not os.access(path, os.X_OK):
        raise MeasurementError('link invocation did not create the expected executable')
    with path.open('rb') as stream:
        header = stream.read(20)
    if (header[:6] != b'\x7fELF\x02\x01' or int.from_bytes(header[18:20], 'little') != 62
            or int.from_bytes(header[16:18], 'little') not in (2, 3)):
        raise MeasurementError('link output is not an x86_64 executable ELF')
    observation, comments, _ = invoke(['readelf', '-p', '.comment', str(path)], path.parent, env,
                                       deadline, prefix.with_name(prefix.name + '-elf'), limit=10)
    require_success(observation, 'ELF linker inspection')
    if expected_linker not in comments.decode(errors='replace'):
        raise MeasurementError('ELF does not identify the expected linker')
    smoke, stdout, _ = invoke([str(path), '--help'], path.parent, env, deadline,
                              prefix.with_name(prefix.name + '-help'), limit=10)
    require_success(smoke, 'binary help smoke')
    if not stdout.strip() or (help_output is not None and stdout != help_output):
        raise MeasurementError('binary help output is empty or differs from the prepared binary')
    return {'sha256': digest(path), 'bytes': path.stat().st_size,
            'linker_comment': comments.decode(errors='replace'),
            'help_stdout_sha256': hashlib.sha256(stdout).hexdigest()}, stdout


def cargo_artifact(stdout, target):
    artifacts, finished, units = [], [], []
    for line in stdout.splitlines():
        try:
            message = json.loads(line)
        except (ValueError, UnicodeDecodeError) as error:
            raise MeasurementError('Cargo stdout is not its requested JSON stream') from error
        if message.get('reason') == 'build-finished':
            finished.append(message.get('success'))
        if message.get('reason') != 'compiler-artifact':
            continue
        if type(message.get('fresh')) is not bool:
            raise MeasurementError('Cargo artifact lacks its Fresh observation')
        units.append({k: message.get(k) for k in ('package_id', 'target', 'profile', 'features', 'fresh')})
        if message['target'].get('name') == PACKAGE and message['target'].get('kind') == ['bin']:
            artifacts.append(message)
    if finished != [True] or len(artifacts) != 1 or artifacts[0]['fresh']:
        raise MeasurementError('preparation requires one newly built target and successful Cargo completion')
    executable = Path(artifacts[0].get('executable') or '').resolve()
    if executable != target / 'debug' / PACKAGE or not executable.is_file():
        raise MeasurementError('Cargo target artifact escaped the owned target or is missing')
    return executable, {'target': artifacts[0], 'units': units,
                        'fresh': sum(x['fresh'] for x in units),
                        'rebuilt': sum(not x['fresh'] for x in units)}


def sample_order(pairs):
    return [(number, linker) for number in range(1, pairs + 1)
            for linker in (('lld', 'mold') if number % 2 else ('mold', 'lld'))]


def compare(samples, pairs):
    measured = [s for s in samples if s['phase'] == 'measured']
    expected = sample_order(pairs)
    if [(s['pair'], s['linker']) for s in measured] != expected:
        raise MeasurementError('missing, duplicated or reordered paired samples')
    if any(not s.get('success') or 'artifact' not in s or not math.isfinite(s['wall_seconds'])
           or s['wall_seconds'] <= 0 for s in measured):
        raise MeasurementError('failed or unverified link cannot contribute a performance comparison')
    stats = {}
    for name in ('lld', 'mold'):
        values = [s['wall_seconds'] for s in measured if s['linker'] == name]
        stats[name] = {'samples': len(values), 'median_seconds': statistics.median(values),
                       'min_seconds': min(values), 'max_seconds': max(values)}
    paired = []
    for pair in range(1, pairs + 1):
        values = {s['linker']: s['wall_seconds'] for s in measured if s['pair'] == pair}
        paired.append({'pair': pair, 'lld_seconds': values['lld'], 'mold_seconds': values['mold'],
                       'saved_seconds': values['lld'] - values['mold'],
                       'mold_change_pct': (values['mold'] / values['lld'] - 1) * 100})
    return {'linkers': stats, 'pairs': paired,
            'median_paired_saved_seconds': statistics.median(x['saved_seconds'] for x in paired),
            'median_paired_mold_change_pct': statistics.median(x['mold_change_pct'] for x in paired),
            'lld_over_mold_median_ratio': stats['lld']['median_seconds'] / stats['mold']['median_seconds']}


def summary(report):
    lines = ['# Linux linker A/B: frozen native inputs', '',
             'Scope: NewAPI bridge dev binary, default features, debug=0, incremental=0, native LTO disabled.',
             'Direct linker invocation wall time includes process startup and output-pipe EOF. '
             'Cargo compilation, validation, input hashing and archive I/O are outside the sample.', '',
             f'Status: {"PASS" if report["success"] else "FAIL"}',
             f'Source: {report.get("source", "unresolved")}', '',
             'Inputs are hashed before every invocation, warming their filesystem pages. '
             'One warm-up per linker precedes alternating AB/BA pairs. All samples are retained.',
             'This is not cold linking, a whole-Cargo A/B, or a whole-CI speedup measurement.', '']
    if report.get('error'):
        lines += ['Error: ' + report['error'], 'No performance conclusion is available.', '']
    for key in ('prefetch', 'preparation'):
        if key in report:
            lines.append(f'{key.capitalize()} (separate cost): {report[key]["wall_seconds"]:.3f} s')
    lines += ['', '| Phase | Pair | Linker | Seconds | Validated |', '| --- | ---: | --- | ---: | --- |']
    for sample in report['samples']:
        duration = f'{sample["wall_seconds"]:.6f}' if 'wall_seconds' in sample else 'unknown'
        lines.append(f'| {sample["phase"]} | {sample["pair"]} | {sample["linker"]} | '
                     f'{duration} | {sample.get("success", False)} |')
    if report['success']:
        comparison = report['comparison']
        lines += ['', '| Linker | Samples | Median seconds | Min | Max |', '| --- | ---: | ---: | ---: | ---: |']
        for name, stats in comparison['linkers'].items():
            lines.append(f'| {name} | {stats["samples"]} | {stats["median_seconds"]:.6f} | '
                         f'{stats["min_seconds"]:.6f} | {stats["max_seconds"]:.6f} |')
        lines += ['', f'Median paired mold change: {comparison["median_paired_mold_change_pct"]:+.2f}% '
                  '(negative means faster for this input).',
                  f'Median paired seconds saved: {comparison["median_paired_saved_seconds"]:+.6f}.',
                  'These few samples describe this runner and input, not a stable population estimate.']
    return '\n'.join(lines) + '\n'


def measure(args):
    repo = Path(COMMON.git(args.repo.resolve(), 'rev-parse', '--show-toplevel')).resolve()
    repositories = [Path(x[9:]).resolve() for x in COMMON.git(repo, 'worktree', 'list', '--porcelain').splitlines()
                    if x.startswith('worktree ')]
    out = COMMON.external(args.out_dir, repositories)
    source = COMMON.git(repo, 'rev-parse', '--verify', '--end-of-options', args.source_ref + '^{commit}')
    out.mkdir(parents=True, exist_ok=False)
    logs = out / 'logs'
    logs.mkdir()
    report = {'schema_version': 1, 'success': False, 'scope': 'frozen-native-link-replay',
              'source': source, 'package': PACKAGE, 'binary': PACKAGE, 'pairs_requested': args.pairs,
              'jobs': args.jobs, 'runner': COMMON.runner_resources(), 'samples': [],
              'harness_sha256': digest(Path(__file__)),
              'shared_metrics_helper_sha256': digest(ROOT / 'scripts/ci-ecs-build-metrics.py'),
              'cache_condition': 'same archived inputs, hashed before every invocation; warm page-cache replay',
              'wait_policy': 'stdout/stderr PIPE and communicate EOF; default mold fork retained; no CPU/RSS claim'}
    deadline = time.monotonic() + args.timeout_seconds
    temporary = checkout = None
    linked = False
    failure = None
    try:
        temporary = Path(tempfile.mkdtemp(prefix='oasis7-linker-benchmark-')).resolve()
        COMMON.external(temporary, repositories)
        checkout, target = temporary / 'source', temporary / 'target'
        subprocess.run(['git', 'worktree', 'add', '--detach', str(checkout), source], cwd=repo,
                       check=True, stdout=subprocess.DEVNULL, timeout=30)
        linked = True
        target.mkdir()
        env = normalized_env(target, args.jobs)
        before = COMMON.source_fingerprint(checkout)
        if COMMON.git(checkout, 'status', '--porcelain', '--untracked-files=all'):
            raise MeasurementError('temporary source checkout is not clean')
        config = configuration(checkout, env)
        compiler = COMMON.output(['rustc', '-vV'], checkout, env)
        if 'release: 1.96.0\n' not in compiler + '\n' or f'host: {HOST}' not in compiler:
            raise MeasurementError('benchmark requires Rust 1.96.0 on x86_64-unknown-linux-gnu')
        sysroot = Path(COMMON.output(['rustc', '--print', 'sysroot'], checkout, env))
        lld = sysroot / 'lib/rustlib' / HOST / 'bin/gcc-ld/ld.lld'
        mold_hint = args.mold
        if mold_hint is None:
            wrapper = os.environ.get('CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER')
            mold_hint = Path(wrapper).parent / 'mold' if wrapper else shutil.which('mold')
        if mold_hint is None:
            raise MeasurementError('mold is unavailable; run setup-linux-mold or pass --mold')
        mold = Path(mold_hint).resolve()
        linkers = {'lld': lld, 'mold': mold}
        versions = {name: COMMON.output([str(path), '--version'], checkout, env).splitlines()[0]
                    for name, path in linkers.items()}
        if not versions['lld'].startswith('LLD ') or not versions['mold'].startswith('mold 3.0.0 '):
            raise MeasurementError('expected the toolchain LLD and mold 3.0.0')
        # The CLI adds this compatibility suffix; the archive and ELF omit it.
        markers = {**versions, 'lld': versions['lld'].removesuffix(' (compatible with GNU linkers)')}
        report['identity'] = {'rustc': compiler, 'cargo': COMMON.output(['cargo', '-V'], checkout, env),
                              'configuration': config, 'source_fingerprint': before,
                              'cargo_lock_sha256': digest(checkout / 'Cargo.lock'),
                              'linkers': {name: {'path': str(path), 'version': versions[name],
                                                'sha256': digest(path)} for name, path in linkers.items()}}
        result, _, _ = invoke(['cargo', 'fetch', '--locked', '--target', HOST], checkout, env,
                              deadline, logs / 'prefetch', limit=600)
        report['prefetch'] = result
        require_success(result, 'dependency prefetch')
        archive = out / 'capture.tar'
        command = ['cargo', 'rustc', '--offline', '--locked', '--profile', 'dev', '-p', PACKAGE,
                   '--bin', PACKAGE, '--message-format=json-render-diagnostics', '--',
                   '-C', 'link-arg=-Wl,--reproduce=' + str(archive)]
        result, cargo_stdout, _ = invoke(command, checkout, env, deadline, logs / 'preparation')
        report['preparation'] = result
        require_success(result, 'Cargo preparation')
        executable, observation = cargo_artifact(cargo_stdout, target)
        write_json(out / 'cargo-artifacts.json', observation)
        report['preparation']['artifact_observation'] = 'cargo-artifacts.json'
        original, help_output = verify_elf(executable, markers['lld'], env, deadline, logs / 'original')
        report['preparation']['artifact'] = original
        if COMMON.source_fingerprint(checkout) != before or COMMON.git(checkout, 'rev-parse', 'HEAD') != source:
            raise MeasurementError('Cargo preparation modified the pinned source')
        if not archive.is_file() or archive.is_symlink():
            raise MeasurementError('default LLD did not produce its reproduction archive')
        bundle = unpack_inputs(archive, temporary / 'inputs')
        if (bundle / 'version.txt').read_text().strip() != markers['lld']:
            raise MeasurementError('archive producer differs from the pinned toolchain LLD')
        manifest = input_manifest(bundle)
        write_json(out / 'inputs.json', manifest)
        report['inputs'] = {'archive_sha256': digest(archive), 'archive_bytes': archive.stat().st_size,
                            'tree_sha256': json_digest(manifest), 'files': len(manifest),
                            'bytes': sum(v['bytes'] for v in manifest.values()),
                            'response_sha256': digest(bundle / 'response.txt')}
        sequence = [('warmup', 0, name) for name in ('lld', 'mold')]
        sequence += [('measured', pair, name) for pair, name in sample_order(args.pairs)]
        for index, (phase, pair, name) in enumerate(sequence):
            sample = {'phase': phase, 'pair': pair, 'linker': name, 'order': index, 'success': False}
            report['samples'].append(sample)
            output = bundle / OUTPUT
            if output.exists() or output.is_symlink():
                if output.is_symlink() or not output.is_file():
                    raise MeasurementError('unexpected replay output type')
                output.unlink()
            if input_manifest(bundle) != manifest:
                raise MeasurementError('frozen link inputs changed before sample')
            if digest(linkers[name]) != report['identity']['linkers'][name]['sha256']:
                raise MeasurementError('linker executable changed during experiment')
            print(f'{phase} pair={pair} linker={name}', flush=True)
            prefix = logs / f'{index:02d}-{phase}-{name}'
            result, _, _ = invoke([str(linkers[name]), '@response.txt', '-o', OUTPUT], bundle, env,
                                  deadline, prefix, limit=120)
            sample.update(result)
            sample['success'] = False
            require_success(result, name + ' replay')
            artifact, _ = verify_elf(output, markers[name], env, deadline, prefix, help_output)
            sample['artifact'] = artifact
            if input_manifest(bundle) != manifest:
                raise MeasurementError('frozen link inputs changed during sample')
            sample['success'] = True
            write_json(out / 'report.json', report)
        if COMMON.source_fingerprint(checkout) != before or COMMON.git(checkout, 'rev-parse', 'HEAD') != source:
            raise MeasurementError('pinned source changed during experiment')
        report['comparison'] = compare(report['samples'], args.pairs)
        report['success'] = True
    except BaseException as error:
        failure = error
        report['error'] = str(error) or type(error).__name__
        report['success'] = False
    finally:
        try:
            if linked:
                removed = subprocess.run(['git', 'worktree', 'remove', str(checkout)], cwd=repo,
                                         capture_output=True, text=True, timeout=30)
                if removed.returncode:
                    raise MeasurementError(removed.stderr.strip())
            elif checkout is not None:
                raise MeasurementError('worktree creation did not complete; preserve partial registration')
            if temporary:
                shutil.rmtree(temporary)
        except BaseException as cleanup_error:
            report.update(success=False, cleanup_error=str(cleanup_error), preserved_work_dir=str(temporary))
            failure = failure or MeasurementError('owned cleanup failed; preserved temporary directory')
            report.setdefault('error', str(failure))
        write_json(out / 'report.json', report)
        (out / 'summary.md').write_text(summary(report))
    if failure is not None:
        raise failure
    print('wrote linker benchmark to ' + str(out), flush=True)
    return report


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--repo', type=Path, default=ROOT)
    parser.add_argument('--source-ref', default='HEAD')
    parser.add_argument('--out-dir', type=Path, required=True)
    parser.add_argument('--mold', type=Path)
    parser.add_argument('--pairs', type=int, default=4)
    parser.add_argument('--jobs', type=int, default=2)
    parser.add_argument('--timeout-seconds', type=int, default=1800)
    args = parser.parse_args(argv)
    if args.pairs not in (2, 4, 6, 8, 10):
        parser.error('--pairs must be an even number from 2 to 10')
    if not 1 <= args.jobs <= 8 or not 30 <= args.timeout_seconds <= 2400:
        parser.error('--jobs must be 1..8 and --timeout-seconds must be 30..2400')
    def interrupted(signum, _frame):
        raise MeasurementError('measurement interrupted by signal ' + str(signum))
    previous_handlers = {signum: signal.signal(signum, interrupted)
                         for signum in (signal.SIGINT, signal.SIGTERM)}
    try:
        measure(args)
    except (MeasurementError, OSError, subprocess.SubprocessError, ValueError) as error:
        print('error: ' + str(error), file=sys.stderr)
        return 1
    finally:
        for signum, handler in previous_handlers.items():
            signal.signal(signum, handler)
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
