#!/usr/bin/env python3
"""Manual storage acceptance using immutable native artifacts and owned process cleanup."""
import argparse
import contextlib
import importlib.util
import json
import os
from pathlib import Path
import sys
import tarfile
import threading
import shutil
import signal
import uuid
import platform
import subprocess

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
spec = importlib.util.spec_from_file_location('native_artifacts', HERE / 'ci-test-artifacts.py')
artifacts = importlib.util.module_from_spec(spec)
spec.loader.exec_module(artifacts)
from ci_process_runner import run

GROUPS = ('units', *artifacts.SHARDS, 'capacity', 'network', 'disk')
NETWORK_SUFFIXES = (
    'protected_endpoint_children_replica_unavailable_preserves_exact_read_and_reconciles_original',
    'protected_endpoint_children_effect_lost_reply_sigkill_resolves_original_once',
    'formal_endpoint_child_process_dual_qualification_sigkill_reopen_original',
)
LIBRARIES = ('distfs-tests', 'storage-lib-oasis7_cas',
             'storage-lib-oasis7_formal_protocol', 'storage-lib-oasis7_runtime',
             'storage-lib-oasis7_world_service')


def host_observation(directory):
    disk = shutil.disk_usage(directory)
    memory = None
    if Path('/proc/meminfo').is_file():
        for line in Path('/proc/meminfo').read_text().splitlines():
            if line.startswith('MemTotal:'):
                memory = int(line.split()[1]) * 1024
                break
    filesystem = None
    if sys.platform == 'linux':
        filesystem = subprocess.check_output(['stat', '-f', '-c', '%T', str(directory)],
                                             text=True, timeout=5).strip()
    return dict(kernel=platform.release(), architecture=platform.machine(), cpu_count=os.cpu_count(),
                total_memory_bytes=memory, filesystem_type=filesystem,
                runner_image_version=os.environ.get('ImageVersion'),
                disk_before=dict(total=disk.total, used=disk.used, free=disk.free))


def select(manifest, group, scale=10):
    binaries = manifest['binaries']
    if group == 'disk':
        suffix = 'physical_real_filesystem_enospc_preserves_published_bytes_and_retry'
        binary = binaries['distfs-tests']
        names = [name for name in binary['ignored_tests'] if name.endswith('::' + suffix)]
        if len(names) != 1 or names[0] not in binary['tests']:
            raise ValueError('real ENOSPC test inventory changed')
        return [dict(binary='distfs-tests', name=names[0], ignored=True)]
    if group in artifacts.SHARDS:
        return artifacts.selected_entries(manifest, json.loads(artifacts.CONFIG.read_text()), shard=group)
    if group == 'network':
        entries = [entry for values in manifest['suites'].values() for entry in values
                   if entry.get('name', '').rsplit('::', 1)[-1] in NETWORK_SUFFIXES]
        if len(entries) != len(NETWORK_SUFFIXES) or {e['name'].rsplit('::', 1)[-1] for e in entries} != set(NETWORK_SUFFIXES):
            raise ValueError('network fault inventory missing or duplicated')
        return entries
    if group == 'capacity':
        suffix = ('actual_formal_storage_capacity_smoke_10_heights' if scale == 10
                  else 'actual_formal_storage_capacity_1000_or_10000_heights')
        names = [name for name in binaries['runtime-tests']['tests'] if name.endswith('::' + suffix)]
        if len(names) != 1:
            raise ValueError('capacity test inventory changed')
        return [dict(binary='runtime-tests', name=names[0], ignored=scale != 10)]
    if group != 'units':
        raise ValueError('unknown group')
    entries = []
    for logical in LIBRARIES:
        binary = binaries[logical]
        count = len(set(binary['tests']) - set(binary['ignored_tests']))
        if count <= 0:
            raise ValueError('empty storage library inventory: ' + logical)
        entries.append(dict(binary=logical, batch=True, count=count))
    binary = binaries['runtime-tests']
    names = sorted(name for name in set(binary['tests']) - set(binary['ignored_tests'])
                   if 'actual_formal_storage_capacity_' not in name and any(part in name.lower() for part in ('storage', 'snapshot', 'migration', 'merkle', 'retention', 'gc_', 'native', 'formal_')))
    if not names:
        raise ValueError('empty runtime storage inventory')
    entries.extend(dict(binary='runtime-tests', name=name, ignored=False) for name in names)
    return entries


def pack(bundle, destination):
    manifest, _ = artifacts.load(bundle)
    if manifest['scope'] != 'storage' or manifest.get('selection') is not None:
        raise ValueError('expected complete storage producer bundle')
    files = [('manifest.json', None)]
    files.extend((entry['path'], entry['sha256']) for entry in manifest['binaries'].values())
    files.extend(manifest['libraries'].items())
    destination.parent.mkdir(parents=True, exist_ok=True)
    with tarfile.open(destination, 'w:gz', compresslevel=1) as archive:
        for name, sha in files:
            path = artifacts.checked_file(bundle, name, sha) if sha else bundle / name
            archive.add(path, arcname=name, recursive=False)


class Tee:
    def __init__(self, stream):
        self.stream = stream
    def write(self, value):
        sys.__stdout__.write(value)
        self.stream.write(value)
        self.stream.flush()
    def flush(self):
        self.stream.flush()
        sys.__stdout__.flush()


class MemorySampler:
    """Sample sum RSS of processes inheriting a private marker; not system peak RSS."""
    def __init__(self, marker, directory):
        self.directory = directory
        self.disk_floor_breached = False
        self.minimum_free = None
        self.monitor_error = None
        self.marker = ('OASIS7_STORAGE_SAMPLE_OWNER=' + marker).encode()
        self.stop = threading.Event()
        self.samples, self.peak = 0, 0
        self.thread = threading.Thread(target=self.observe, daemon=True)
    def observe(self):
        while not self.stop.is_set():
            total, seen = 0, 0
            try:
                free = shutil.disk_usage(self.directory).free
            except OSError as error:
                self.monitor_error = str(error)
                return
            self.minimum_free = free if self.minimum_free is None else min(free, self.minimum_free)
            self.disk_floor_breached |= free < 3 * 1024 ** 3
            for directory in Path('/proc').glob('[0-9]*'):
                try:
                    if directory.stat().st_uid != os.getuid() or self.marker not in (directory / 'environ').read_bytes().split(b'\0'):
                        continue
                    identity = (directory / 'stat').read_text().rsplit(')', 1)[1].split()[19]
                    if self.disk_floor_breached:
                        if ((directory / 'stat').read_text().rsplit(')', 1)[1].split()[19] == identity
                                and self.marker in (directory / 'environ').read_bytes().split(b'\0')):
                            os.kill(int(directory.name), signal.SIGTERM)
                    for line in (directory / 'status').read_text().splitlines():
                        if line.startswith('VmRSS:'):
                            total += int(line.split()[1]) * 1024
                            seen += 1
                except (OSError, ValueError):
                    continue
            if seen:
                self.samples += 1
                self.peak = max(self.peak, total)
            self.stop.wait(1)
    def __enter__(self):
        self.thread.start()
        return self
    def __exit__(self, *_):
        self.stop.set()
        self.thread.join(timeout=3)
    def report(self):
        return dict(scope='sum RSS of processes inheriting private marker; sampled every 1s; shared pages counted per process; short peaks may be missed',
                    samples=self.samples, sampled_peak_rss_bytes=self.peak if self.samples else None,
                    minimum_free_disk_bytes=self.minimum_free, disk_floor_breached=self.disk_floor_breached, monitor_error=self.monitor_error)


def validate_result(result):
    return (result.get('passed') is True and result.get('cleanup_result') == 'clean'
            and result.get('count_ok') is True and not result.get('leaked_processes')
            and result.get('exit_code') == 0 and not result.get('timeout') and not result.get('cancelled'))


def execute(args):
    bundle, directory = args.bundle.resolve(), args.results_dir.resolve()
    directory.mkdir(parents=True, exist_ok=False)
    manifest, env = artifacts.load(bundle)
    if manifest['scope'] != 'storage' or manifest.get('selection') is not None:
        raise ValueError('expected complete storage bundle')
    entries = select(manifest, args.group, args.capacity_scale)
    selected = [dict(entry) for entry in entries]
    summary = dict(schema_version=1, label=args.label or args.group, group=args.group,
                   tested_sha=manifest['tested_sha'], source_sha=manifest['source_sha'],
                   producer_run_id=manifest['producer_run_id'], producer_attempt=manifest['producer_attempt'],
                   runner_image=manifest['runner_image'], rustc_version=manifest['rustc_version'],
                   host=host_observation(directory),
                   capacity_scale=args.capacity_scale, repeat_faults=args.repeat_faults,
                   selected_inventory=selected, artifact_manifest=manifest, expected_cases=len(entries) * (args.repeat_faults if args.group == 'network' else 1),
                   passed=False, results=[], capacity_reports=[])
    (directory / 'selected-inventory.json').write_text(json.dumps(summary, indent=2) + '\n')
    env['OASIS7_STORAGE_BENCHMARK_HEIGHTS'] = str(args.capacity_scale)
    if args.group == 'capacity':
        env['OASIS7_STORAGE_PROGRESS_PATH'] = str(directory / 'capacity-progress.jsonl')
        env.setdefault('OASIS7_STORAGE_MAX_LOGICAL_BYTES', str(4 * 1024 ** 3))
        env.setdefault('OASIS7_STORAGE_MAX_SECONDS', '18000')
        env.setdefault('OASIS7_STORAGE_PROGRESS_INTERVAL', '100')
    marker = uuid.uuid4().hex
    env['OASIS7_STORAGE_SAMPLE_OWNER'] = marker
    try:
        with MemorySampler(marker, directory) as sampler:
            for repeat in range(args.repeat_faults if args.group == 'network' else 1):
                for index, entry in enumerate(entries):
                    if 'script' in entry:
                        command = ['bash', str(artifacts.ROOT / 'scripts' / entry['script'])]
                        cwd, count, name = artifacts.ROOT, None, entry['script']
                    else:
                        binary = manifest['binaries'][entry['binary']]
                        command = [str(bundle / binary['path'])]
                        cwd = artifacts.ROOT / binary['cwd']
                        name = entry.get('name', entry['binary'])
                        count = entry.get('count', 1)
                        if not entry.get('batch'):
                            command += ['--exact', name]
                            if entry.get('ignored'):
                                command.append('--ignored')
                        command += ['--test-threads=1', '--nocapture']
                    free = shutil.disk_usage(directory).free
                    summary.setdefault('disk_available_before_cases_bytes', []).append(free)
                    if free < 3 * 1024 ** 3:
                        raise ValueError('disk below 3 GiB safety floor; incomplete acceptance')
                    log = directory / f'{repeat:02}-{index:04}.log'
                    with log.open('w') as stream, contextlib.redirect_stdout(Tee(stream)):
                        result = run(command, cwd, dict(env, CARGO_MANIFEST_DIR=str(cwd)),
                                     args.group, name, directory / 'results.jsonl', expected_count=count,
                                     timeout_seconds=args.timeout_seconds or (1200 if args.capacity_scale == 10 else 19000)
                                     if args.group == 'capacity' else args.timeout_seconds or 2400)
                    summary['results'].append(dict(result, log=log.name, repetition=repeat + 1))
                    if args.group == 'capacity':
                        reports = [json.loads(line.split('STORAGE_CAPACITY_REPORT=', 1)[1])
                                   for line in log.read_text().splitlines() if 'STORAGE_CAPACITY_REPORT=' in line]
                        if len(reports) != 1 or not isinstance(reports[0], dict) or not reports[0]:
                            raise ValueError('missing or duplicate structured capacity report')
                        if reports[0].get('status') != 'passed' or reports[0].get('executed_heights') != args.capacity_scale:
                            raise ValueError('partial or wrong-scale capacity report')
                        summary['capacity_reports'].extend(reports)
                    if result.get('cancelled'):
                        raise ValueError('acceptance cancelled; remaining cases not executed')
                    if result['cleanup_result'] != 'clean':
                        raise ValueError('unresolved owned processes')
        summary['memory'] = sampler.report()
        summary['passed'] = (not sampler.disk_floor_breached and not sampler.monitor_error and len(summary['results']) == summary['expected_cases']
                             and all(validate_result(result) for result in summary['results']))
        if not summary['passed']:
            raise ValueError('storage acceptance case failed')
    finally:
        if 'sampler' in locals():
            summary['memory'] = sampler.report()
        (directory / 'summary.json').write_text(json.dumps(summary, indent=2) + '\n')


def summarize(directory, expected, needs=None):
    reports = [json.loads(path.read_text()) for path in directory.rglob('summary.json')]
    labels = [report['label'] for report in reports]
    if len(labels) != len(set(labels)) or set(labels) != set(expected) or len(expected) != len(set(expected)):
        raise ValueError('missing, duplicate or unexpected acceptance result')
    if needs is not None:
        if set(needs) != {'build', 'verify', 'capacity-long'}:
            raise ValueError('missing or unexpected acceptance job')
        long_selected = any(label in ('capacity-1000', 'capacity-10000') for label in expected)
        for name, job in needs.items():
            required = 'skipped' if name == 'capacity-long' and not long_selected else 'success'
            if job.get('result') != required:
                raise ValueError('required acceptance job failed, skipped, cancelled or missing')
    identities = {(report['tested_sha'], report['source_sha'], report['producer_run_id'], report['producer_attempt']) for report in reports}
    if len(identities) != 1:
        raise ValueError('acceptance result identity mismatch')
    for report in reports:
        if (report.get('passed') is not True or report.get('expected_cases', 0) <= 0
                or len(report.get('results', [])) != report['expected_cases']
                or not all(validate_result(result) for result in report['results'])
                or not report.get('selected_inventory')
                or (report['group'] == 'capacity' and (len(report.get('capacity_reports', [])) != 1
                    or report['capacity_reports'][0].get('status') != 'passed'
                    or report['capacity_reports'][0].get('executed_heights') != report.get('capacity_scale')))):
            raise ValueError('incomplete or failed acceptance result: ' + report['label'])
    return dict(passed=True, identity=list(next(iter(identities))), groups=sorted(labels),
                observations=[dict(label=report['label'], cases=report['expected_cases'],
                                   elapsed_seconds=round(sum(r.get('elapsed_seconds', 0) for r in report['results']), 3),
                                   memory=report.get('memory'), host=report.get('host'))
                              for report in sorted(reports, key=lambda value: value['label'])],
                not_run=[label for label in ('capacity-1000', 'capacity-10000') if label not in labels]
                        + ['real-independent-fault-domains', 'block-device-faults',
                           'cross-version-shipping-upgrade-rollback', 'autonomous-agent-playtest'],
                limitations=['Single CI runner per job; simulated faults do not certify independent infrastructure fault domains (S6).',
                            'No actual block-device ENOSPC/fsync fault injection, cross-version shipping upgrade/rollback, or autonomous Agent playtest.',
                            'RSS is sampled sum of marked processes, not exact process-tree high-water memory or system memory.'])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('action', choices=('build', 'pack', 'run', 'summarize'))
    parser.add_argument('--bundle', type=Path)
    parser.add_argument('--output', type=Path)
    parser.add_argument('--results-dir', type=Path)
    parser.add_argument('--group', choices=GROUPS)
    parser.add_argument('--label')
    parser.add_argument('--timeout-seconds', type=int)
    parser.add_argument('--capacity-scale', type=int, choices=(10, 1000, 10000), default=10)
    parser.add_argument('--repeat-faults', type=int, choices=(1, 2, 3), default=1)
    parser.add_argument('--expected', action='append', default=[])
    parser.add_argument('--needs-json', type=Path)
    args = parser.parse_args()
    if args.timeout_seconds is not None and not 1 <= args.timeout_seconds <= 21600:
        parser.error('--timeout-seconds must be between 1 and 21600')
    if args.action == 'build':
        artifacts.build(args.bundle.resolve(), 'storage')
    elif args.action == 'pack':
        pack(args.bundle.resolve(), args.output)
    elif args.action == 'run':
        if not args.group or not args.results_dir:
            parser.error('run requires --group and --results-dir')
        execute(args)
    else:
        report = summarize(args.results_dir, args.expected,
                           json.loads(args.needs_json.read_text()) if args.needs_json else None)
        print(json.dumps(report, indent=2))
        if args.output:
            args.output.write_text(json.dumps(report, indent=2) + '\n')
        if os.environ.get('GITHUB_STEP_SUMMARY'):
            with Path(os.environ['GITHUB_STEP_SUMMARY']).open('a') as stream:
                stream.write('## Storage acceptance\n\nAll selected groups passed. '
                             'This is a same-host fault-model result, not S6 acceptance.\n\n')
                stream.write('Source / tested / producer run / attempt: `' + ' / '.join(report['identity']) + '`\n\n')
                stream.write('| Group | Cases | Case execution seconds |\n| --- | ---: | ---: |\n')
                for group in report['observations']:
                    stream.write(f"| {group['label']} | {group['cases']} | {group['elapsed_seconds']} |\n")
                stream.write('\nNot run: ' + ', '.join(report['not_run']) + '.\n\n')
                for limitation in report['limitations']:
                    stream.write('- ' + limitation + '\n')


if __name__ == '__main__':
    main()
