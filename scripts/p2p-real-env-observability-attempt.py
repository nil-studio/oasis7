#!/usr/bin/env python3
"""Durable observability, host, snapshot and traffic collection ordering. Read this canonical state, not an unvalidated cache."""
import argparse
from datetime import datetime
import re
from contextlib import contextmanager
try:
    import fcntl
except ImportError:
    fcntl = None
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import stat
import subprocess
import sys
import tempfile
import time
import uuid

STATE = 'attempt_state.json'
ANCHOR = '.observability-initialized'
LOCK = '.observability-attempt.lock'
_ACTIVE_GUARD = None


class AttemptError(Exception):
    pass


def encoded(value):
    return (json.dumps(value, ensure_ascii=False, sort_keys=True, indent=2) + '\n').encode()


def digest(data):
    return hashlib.sha256(data).hexdigest()


def sync_directory(path):
    fd = os.open(path, os.O_RDONLY | getattr(os, 'O_DIRECTORY', 0))
    try:
        os.fsync(fd)
    finally:
        os.close(fd)


def reject_symlinks(path):
    for part in [path, *path.parents]:
        if part.is_symlink():
            raise AttemptError('symlink path is not supported')


def durable_mkdir(path):
    """Persist new directory bindings from ancestors down to the output root."""
    reject_symlinks(path)
    missing = []
    cursor = path
    while not cursor.exists():
        missing.append(cursor)
        cursor = cursor.parent
    for directory in reversed(missing):
        reject_symlinks(directory)
        try:
            directory.mkdir(mode=0o700)
        except FileExistsError:
            # Another initializer can create a parent before the lifecycle lock
            # exists. Preserve it; normal owner/path checks still gate begin.
            if not directory.is_dir() or directory.is_symlink():
                raise AttemptError('directory creation raced with a different path type')
        sync_directory(directory)
        sync_directory(directory.parent)
    # Also persist a caller-created root's binding before creating protocol state.
    if not missing:
        sync_directory(path)
        sync_directory(path.parent)


def atomic_write(path, data):
    if _ACTIVE_GUARD:
        _ACTIVE_GUARD()
    reject_symlinks(path)
    fd, temporary = tempfile.mkstemp(prefix='.' + path.name + '.', dir=path.parent)
    try:
        with os.fdopen(fd, 'wb') as handle:
            handle.write(data)
            handle.flush()
            os.fsync(handle.fileno())
        if _ACTIVE_GUARD:
            _ACTIVE_GUARD()
        os.replace(temporary, path)
        sync_directory(path.parent)
    finally:
        if os.path.exists(temporary):
            os.unlink(temporary)


def read_lock_token(descriptor):
    payload = os.pread(descriptor, 65, 0)
    if not payload:
        return None
    if len(payload) != 37 or not payload.endswith(b'\n'):
        raise AttemptError('persistent lock token is incomplete or invalid')
    try:
        token = payload[:-1].decode('ascii')
        if str(uuid.UUID(token)) != token:
            raise ValueError('noncanonical lock token')
    except (ValueError, UnicodeError) as error:
        raise AttemptError('persistent lock token is invalid') from error
    return token


@contextmanager
def locked(root, create=False):
    global _ACTIVE_GUARD
    if fcntl is None:
        raise AttemptError("POSIX lifecycle locking unavailable")
    reject_symlinks(root)
    if create:
        durable_mkdir(root)
    parent = os.open(root, os.O_RDONLY | getattr(os, 'O_DIRECTORY', 0) | getattr(os, 'O_NOFOLLOW', 0))
    descriptor = None
    try:
        initialized = (root / ANCHOR).exists() or (root / STATE).exists()
        flags = os.O_RDWR | getattr(os, 'O_NOFOLLOW', 0)
        if create and not initialized:
            flags |= os.O_CREAT
        descriptor = os.open(root / LOCK, flags, 0o600)
        def check(validate_anchor=True):
            reject_symlinks(root / LOCK)
            for opened, named, directory in [(os.fstat(parent), root.stat(), True),
                                              (os.fstat(descriptor), (root / LOCK).stat(), False)]:
                if (opened.st_dev, opened.st_ino) != (named.st_dev, named.st_ino):
                    raise AttemptError('lock path identity changed')
                if opened.st_uid != os.getuid() or opened.st_mode & 0o022:
                    raise AttemptError('lock or output directory is not owner protected')
                if not (stat.S_ISDIR(opened.st_mode) if directory else stat.S_ISREG(opened.st_mode)):
                    raise AttemptError('invalid lock path type')
                if not directory and stat.S_IMODE(opened.st_mode) != 0o600:
                    raise AttemptError('lock permissions changed')
            if validate_anchor and (root / ANCHOR).exists():
                anchor = read_json(root / ANCHOR)
                if not isinstance(anchor, dict):
                    raise AttemptError('invalid initialization anchor shape')
                metadata = os.fstat(descriptor)
                if anchor.get('lock_identity') != {'device': metadata.st_dev, 'inode': metadata.st_ino,
                                                   'token': read_lock_token(descriptor)}:
                    raise AttemptError('persistent lock identity changed')
        check(validate_anchor=False)
        fcntl.flock(descriptor, fcntl.LOCK_EX)
        if create and not (root / ANCHOR).exists() and not (root / STATE).exists():
            token = read_lock_token(descriptor)
            if token is None:
                payload = (str(uuid.uuid4()) + '\n').encode('ascii')
                if os.write(descriptor, payload) != len(payload):
                    raise AttemptError('persistent lock token write was incomplete')
            # A fully written pre-anchor token may survive a crash. Re-sync it
            # before initialization; never overwrite a nonempty partial token.
            os.fsync(descriptor)
            sync_directory(root)
        check.lock_token = read_lock_token(descriptor)
        check()
        _ACTIVE_GUARD = check
        yield check
        check()
    finally:
        _ACTIVE_GUARD = None
        if descriptor is not None:
            os.close(descriptor)
        os.close(parent)


def read_json(path):
    reject_symlinks(path)
    try:
        return json.loads(path.read_bytes())
    except (OSError, ValueError, TypeError) as error:
        raise AttemptError('state or artifact unavailable') from error


def validate_attempt_identity(attempt):
    if (not isinstance(attempt, dict) or type(attempt.get('sequence')) is not int
            or attempt['sequence'] <= 0 or attempt.get('result') not in ('pending', 'success', 'failed')):
        raise AttemptError('invalid attempt sequence or result')
    try:
        uuid.UUID(attempt['attempt_id'])
        uuid.UUID(attempt['generation'])
    except (ValueError, TypeError, KeyError, AttributeError) as error:
        raise AttemptError('invalid attempt identity') from error
    expected = 'runs/' + str(attempt['sequence']).zfill(12) + '-' + attempt['attempt_id']
    scope = attempt.get('scope_fingerprint')
    if (attempt.get('run_directory') != expected or not isinstance(scope, str) or len(scope) != 64
            or any(character not in '0123456789abcdef' for character in scope)):
        raise AttemptError('attempt directory or scope identity invalid')


def validate_collector(value, collector):
    if collector not in ('observability', 'host', 'snapshot', 'traffic') or value.get('collector', 'observability') != collector:
        raise AttemptError('collector profile mismatch')



def legacy_traffic_run(root, directory):
    """Recognize the old timestamp namespace without adopting any old success."""
    if not re.fullmatch(r'[0-9]{8}-[0-9]{6}', directory.name):
        raise AttemptError('unrecognized legacy traffic run name')
    try:
        datetime.strptime(directory.name, '%Y%m%d-%H%M%S')
    except ValueError as error:
        raise AttemptError('invalid legacy traffic run timestamp') from error
    required = {'config.json', 'summary.json', 'summary.md', 'samples.ndjson', 'raw'}
    if not directory.is_dir() or {path.name for path in directory.iterdir()} != required:
        raise AttemptError('legacy traffic run artifacts incomplete or mixed')
    forbidden = {ANCHOR, STATE, LOCK, 'publication', 'terminal_result.json', 'history_receipt.json', 'report'}
    for path in (directory.parent, directory, *directory.rglob('*')):
        reject_symlinks(path)
        metadata = path.stat()
        if (metadata.st_uid != os.getuid() or metadata.st_mode & 0o022
                or not (stat.S_ISDIR(metadata.st_mode) or stat.S_ISREG(metadata.st_mode))
                or (stat.S_ISREG(metadata.st_mode) and metadata.st_nlink != 1)
                or path.name in forbidden):
            raise AttemptError('unsafe or versioned legacy traffic artifact')
    if not (directory / 'raw').is_dir() or any(not (directory / name).is_file() for name in required - {'raw'}):
        raise AttemptError('invalid legacy traffic artifact types')
    config, summary = read_json(directory / 'config.json'), read_json(directory / 'summary.json')
    config_fields = {'run_id', 'run_dir', 'history_path', 'latest_summary_json', 'latest_summary_md',
                     'samples', 'interval_secs', 'window_minutes', 'history_retention_minutes', 'top_n',
                     'ssh_timeout_secs', 'summary_only', 'nodes'}
    if not isinstance(config, dict) or set(config) != config_fields or not isinstance(summary, dict):
        raise AttemptError('unrecognized legacy traffic configuration')
    def absolute(value):
        if not isinstance(value, str) or not value:
            raise AttemptError('invalid legacy traffic path')
        return Path(os.path.abspath(Path(__file__).parent.parent / value))
    summary_fields = {'ok', 'schema_version', 'as_of_unix_ms', 'freshness_policy', 'generated_at', 'history_path',
                      'history_retention_minutes', 'history_record_count_before_prune', 'history_record_count',
                      'history_pruned_count', 'window_minutes_requested', 'top_n', 'run_id', 'run_dir', 'nodes', 'aggregate'}
    if (set(summary) != summary_fields or config['run_id'] != directory.name or absolute(config['run_dir']) != directory
            or summary.get('run_id') != config['run_id'] or summary.get('run_dir') != config['run_dir']
            or absolute(config['latest_summary_json']) != root / 'latest_summary.json'
            or absolute(config['latest_summary_md']) != root / 'latest_summary.md'
            or not isinstance(config['history_path'], str) or not config['history_path']
            or summary.get('history_path') != config['history_path']
            or summary.get('schema_version') != 2 or summary.get('ok') is not True
            or not isinstance(summary.get('nodes'), dict) or set(summary['nodes']) != {'local_node', 'sequencer_ecs', 'storage_ecs'}
            or any(not isinstance(node, dict) for node in summary['nodes'].values())
            or not isinstance(summary.get('aggregate'), dict)
            or any(type(summary.get(key)) is not int or summary[key] < 0 for key in
                   ('as_of_unix_ms', 'history_record_count_before_prune', 'history_record_count', 'history_pruned_count'))
            or summary.get('history_retention_minutes') != config['history_retention_minutes']
            or summary.get('window_minutes_requested') != config['window_minutes'] or summary.get('top_n') != config['top_n']
            or not isinstance(summary.get('freshness_policy'), dict) or not isinstance(summary.get('generated_at'), str)
            or any(key in summary for key in ('collection', 'collection_attempt', 'attempt', 'latest_attempt',
                                               'last_success', 'canonical_state'))
            or type(config['summary_only']) is not bool
            or any(type(config[key]) is not int or config[key] <= 0 for key in
                   ('samples', 'interval_secs', 'window_minutes', 'history_retention_minutes', 'top_n', 'ssh_timeout_secs'))):
        raise AttemptError('legacy traffic configuration or summary identity mismatch')
    nodes = config['nodes']
    if not isinstance(nodes, dict) or set(nodes) != {'local_node', 'sequencer_ecs', 'storage_ecs'}:
        raise AttemptError('invalid legacy traffic requested inventory')
    for label, node in nodes.items():
        expected = {'mode', 'status_url'} | (set() if label == 'local_node' else {'target'})
        if (not isinstance(node, dict) or set(node) != expected
                or node['mode'] != ('local' if label == 'local_node' else 'remote')
                or any(not isinstance(value, str) or not value for value in node.values())):
            raise AttemptError('invalid legacy traffic node configuration')
    try:
        (directory / 'summary.md').read_text(encoding='utf-8')
    except UnicodeError as error:
        raise AttemptError('invalid legacy traffic markdown encoding') from error
    return {name: digest((directory / name).read_bytes()) for name in ('config.json', 'summary.json', 'summary.md')}

def load_state(root, collector='observability'):
    anchor, state_path = root / ANCHOR, root / STATE
    if not anchor.exists() or not state_path.exists():
        raise AttemptError('initialization anchor or sequence state missing')
    initialized = read_json(anchor)
    state = read_json(state_path)
    if (not isinstance(state, dict) or state.get('schema_version') != 1
            or not isinstance(initialized, dict) or initialized.get('schema_version') != 1
            or initialized.get('store_id') != state.get('store_id')
            or initialized.get('high_water_sequence') != state.get('last_sequence')
            or not isinstance(state.get('store_id'), str)
            or type(state.get('last_sequence')) is not int or state['last_sequence'] < 0
            or 'last_success' not in state):
        raise AttemptError('invalid sequence state')
    validate_collector(initialized, collector)
    validate_collector(state, collector)
    runs = root / 'runs'
    reject_symlinks(runs)
    legacy_runs = initialized.get('legacy_runs', {})
    if not isinstance(legacy_runs, dict) or (legacy_runs and collector != 'traffic'):
        raise AttemptError('invalid legacy run registry')
    if any(not isinstance(name, str) or not re.fullmatch(r'[0-9]{8}-[0-9]{6}', name)
           or not isinstance(reference, dict) or set(reference) != {'config.json', 'summary.json', 'summary.md'}
           or any(not isinstance(value, str) or not re.fullmatch(r'[0-9a-f]{64}', value) for value in reference.values())
           for name, reference in legacy_runs.items()):
        raise AttemptError('invalid legacy run registry entry')
    if runs.exists():
        for directory in runs.iterdir():
            if directory.name in legacy_runs:
                if legacy_traffic_run(root, directory) != legacy_runs[directory.name]:
                    raise AttemptError('registered legacy traffic metadata changed')
                continue
            try:
                sequence = int(directory.name.split('-', 1)[0])
            except ValueError as error:
                raise AttemptError('invalid retained run sequence evidence') from error
            if sequence > state['last_sequence']:
                raise AttemptError('retained run evidence exceeds persisted sequence')
    if any(not (runs / name).exists() for name in legacy_runs):
        raise AttemptError('registered legacy traffic run missing')
    attempt = state.get('latest_attempt')
    if state['last_sequence']:
        if (not isinstance(attempt, dict) or attempt.get('sequence') != state['last_sequence']
                or attempt.get('result') not in ('pending', 'success', 'failed')
                or not isinstance(attempt.get('attempt_id'), str)
                or not isinstance(attempt.get('scope_fingerprint'), str)):
            raise AttemptError('invalid latest attempt state')
    elif attempt is not None:
        raise AttemptError('invalid initial attempt state')
    if attempt:
        validate_collector(attempt, collector)
        validate_attempt_identity(attempt)
    success = state['last_success']
    if success is not None and (not isinstance(success, dict) or success.get('result') != 'success'
            or type(success.get('sequence')) is not int or not 0 < success['sequence'] <= state['last_sequence']
            or not isinstance(success.get('publication'), dict)):
        raise AttemptError('invalid last-success reference')
    if success is not None:
        validate_collector(success, collector)
        validate_attempt_identity(success)
        if success['publication'].get('generation') != success['generation']:
            raise AttemptError('last-success generation mismatch')
    if attempt and attempt['result'] == 'success':
        if (success is None or success['sequence'] != attempt['sequence']
                or success.get('attempt_id') != attempt['attempt_id']):
            raise AttemptError('successful state lacks matching last-success reference')
    elif success is not None and success['sequence'] >= state['last_sequence']:
        raise AttemptError('historical success is not older than latest attempt')
    return state


def scope_fingerprint(args, collector='observability'):
    sources = [str(Path(__file__).resolve()), str(Path(__file__).with_name('p2p-real-env-observability-monitor.sh').resolve())]
    if collector == 'host':
        sources = [str(Path(__file__).resolve()), *[str(Path(__file__).with_name(name).resolve()) for name in
            ('p2p-real-env-host-monitor.sh', 'p2p-real-env-host-summary.py', 'p2p-real-env-node-host-sample.sh')]]
    if collector == 'snapshot':
        sources = [str(Path(__file__).resolve()), str(Path(__file__).with_name('p2p-real-env-triad-snapshot.sh').resolve())]
    if collector == 'traffic':
        sources = [str(Path(__file__).resolve()), *[str(Path(__file__).with_name(name).resolve()) for name in
            ('p2p-real-env-traffic-monitor.sh', 'traffic-monitor-summary.py', 'traffic_monitor_freshness.py')]]
    candidate = subprocess.run(['git', 'rev-parse', 'HEAD'], cwd=Path(__file__).parent.parent,
                               capture_output=True, text=True)
    material = {'candidate': candidate.stdout.strip() if candidate.returncode == 0 else 'unknown',
                'collector': collector, 'arguments': args, 'code': {path: digest(Path(path).read_bytes()) for path in sources}}
    # Bind file inputs by content too, without persisting their paths or contents.
    file_args = [arg for index, arg in enumerate(args) if collector != 'traffic' or index == 0 or args[index - 1] != '--history-path']
    material['files'] = [digest(Path(arg).read_bytes()) for arg in file_args if Path(arg).is_file()]
    return digest(encoded(material))


def report_for(attempt, last_success, source=None, collector='observability'):
    report = dict(source or {})
    if collector == 'host':
        report['schema_version'] = 2
        report['aggregate'] = dict(report.get('aggregate') or {})
        if attempt['result'] != 'success':
            report['aggregate']['complete'] = False
        report['overall'] = {'status': 'pass', 'alerts': [], 'scope': 'collection_execution_only'}
    if collector == 'snapshot':
        report['schema_version'] = 2
        if attempt['result'] != 'success':
            report['claim_status'] = 'blocked'
            report['collection'] = dict(report.get('collection') or {}, complete=False)
            report['failure_signatures'] = sorted(set(report.get('failure_signatures', []) +
                ['collection_' + attempt['result']] + [row['stage'] + '_failed' for row in attempt.get('errors', [])]))
    if collector == 'traffic':
        report['schema_version'] = 2
        if attempt['result'] != 'success':
            report['collection'] = dict(report.get('collection') or {}, complete=False)
            nodes = report.setdefault('nodes', {})
            for label in ('local_node', 'sequencer_ecs', 'storage_ecs'):
                node = nodes.setdefault(label, {})
                node['available'] = False
                node['freshness'] = dict(node.get('freshness') or {}, state='unknown')
    report['collection_attempt'] = dict(attempt)
    report['attempt'] = dict(attempt)
    report['latest_attempt'] = dict(attempt)
    report['last_success'] = last_success
    report['collection_attempt']['execution_succeeded'] = attempt['result'] == 'success'
    if attempt['result'] != 'success':
        report['current_measurement_available'] = False
    if attempt['result'] != 'success':
        overall = dict(report.get('overall') or {})
        alerts = list(overall.get('alerts') or [])
        alerts.extend(row['stage'] + '_failed' for row in attempt.get('errors', []))
        alerts.append(attempt.get('error') or 'attempt_pending')
        overall.update(status='blocked', alerts=sorted(set(alerts)))
        report['overall'] = overall
    return report


def publish(root, state, attempt, report, markdown):
    directory = root / attempt['run_directory'] / 'publication' / attempt['generation']
    reject_symlinks(directory)
    directory.mkdir(parents=True, exist_ok=False)
    sync_directory(directory.parent)
    sync_directory(directory.parent.parent)
    json_bytes = encoded(report)
    md_bytes = markdown.encode()
    atomic_write(directory / 'summary.json', json_bytes)
    atomic_write(directory / 'summary.md', md_bytes)
    reference = {'generation': attempt['generation'], 'json_path': str((directory / 'summary.json').relative_to(root)),
                 'md_path': str((directory / 'summary.md').relative_to(root)),
                 'json_sha256': digest(json_bytes), 'md_sha256': digest(md_bytes)}
    # These are compatibility caches. Canonical readers validate both against state.
    atomic_write(root / 'latest_summary.json', json_bytes)
    atomic_write(root / 'latest_summary.md', md_bytes)
    return reference


def begin(root, arguments, collector='observability'):
    validate_collector({'collector': collector}, collector)
    with locked(root, create=True) as check:
        reject_symlinks(root / 'runs')
        if not (root / ANCHOR).exists() and not (root / STATE).exists():
            legacy = root / 'latest_summary.json'
            if legacy.exists():
                old = read_json(legacy)
                if isinstance(old, dict) and ('attempt' in old or 'latest_attempt' in old):
                    raise AttemptError('versioned artifacts exist without sequence anchor')
            legacy_runs = {}
            if (root / 'runs').exists():
                for directory in (root / 'runs').iterdir():
                    if collector != 'traffic':
                        raise AttemptError('run artifacts exist without sequence anchor')
                    legacy_runs[directory.name] = legacy_traffic_run(root, directory)
            store_id = str(uuid.uuid4())
            metadata = (root / LOCK).stat()
            atomic_write(root / ANCHOR, encoded({'schema_version': 1, 'collector': collector, 'store_id': store_id,
                         'high_water_sequence': 0, 'legacy_runs': legacy_runs, 'lock_identity': {'device': metadata.st_dev, 'inode': metadata.st_ino,
                         'token': check.lock_token}}))
            atomic_write(root / STATE, encoded({'schema_version': 1, 'collector': collector, 'store_id': store_id,
                         'last_sequence': 0, 'latest_attempt': None, 'last_success': None}))
        state = load_state(root, collector)
        attempt_id = str(uuid.uuid4())
        sequence = state['last_sequence'] + 1
        attempt = {'collector': collector, 'sequence': sequence, 'attempt_id': attempt_id, 'result': 'pending',
                   'scope_fingerprint': scope_fingerprint(arguments, collector), 'started_at_unix_ms': time.time_ns() // 1_000_000,
                   'generation': str(uuid.uuid4()), 'run_directory': 'runs/' + str(sequence).zfill(12) + '-' + attempt_id}
        directory = root / attempt['run_directory']
        reject_symlinks(directory)
        anchor = read_json(root / ANCHOR)
        anchor['high_water_sequence'] = sequence
        atomic_write(root / ANCHOR, encoded(anchor))
        directory = root / attempt['run_directory']
        reject_symlinks(directory)
        directory.mkdir(parents=True, exist_ok=False)
        sync_directory(directory.parent)
        # Reserve the sequence durably before any compatibility publication.
        state.update(last_sequence=sequence, latest_attempt=attempt, publication=None)
        check()
        atomic_write(root / STATE, encoded(state))
        report = report_for(attempt, state.get('last_success'), collector=collector)
        reference = publish(root, state, attempt, report, '# ' + collector.capitalize() + ' attempt pending\n')
        state['publication'] = reference
        check()
        atomic_write(root / STATE, encoded(state))
        return dict(attempt, run_dir=str(directory))


def validate_host_source(source):
    labels = ('local_node', 'sequencer_ecs', 'storage_ecs')
    if not isinstance(source, dict) or source.get('schema_version') != 2:
        raise AttemptError('invalid host summary schema')
    aggregate, nodes = source.get('aggregate'), source.get('nodes')
    if (not isinstance(aggregate, dict) or type(aggregate.get('complete')) is not bool
            or not isinstance(nodes, dict) or set(nodes) != set(labels)):
        raise AttemptError('invalid host completeness shape')
    totals = dict(sample_count=0, success_sample_count=0, failed_sample_count=0)
    failed_nodes = []
    for label in labels:
        node = nodes[label]
        if not isinstance(node, dict) or type(node.get('available')) is not bool:
            raise AttemptError('invalid host node')
        for key in totals:
            if type(node.get(key)) is not int or node[key] < 0:
                raise AttemptError('invalid host sample counts')
            totals[key] += node[key]
        if node['sample_count'] != node['success_sample_count'] + node['failed_sample_count']:
            raise AttemptError('inconsistent host sample counts')
        latest = node.get('latest_attempt')
        if not isinstance(latest, dict) or 'last_success' not in node:
            raise AttemptError('invalid host node attempt history')
        if node['sample_count'] and (latest.get('result') not in ('success', 'failed')
                or type(latest.get('sample_fetch_ok')) is not bool
                or (latest['result'] == 'success') != latest['sample_fetch_ok']
                or node['available'] != latest['sample_fetch_ok']):
            raise AttemptError('inconsistent host latest sample')
        if node['failed_sample_count']:
            failed_nodes.append(label)
    for key in ('expected_sample_count', 'attempted_count', 'failed_sample_count', 'node_count'):
        if type(aggregate.get(key)) is not int or aggregate[key] < 0:
            raise AttemptError('invalid host aggregate counts')
    expected = aggregate['expected_sample_count']
    complete = (expected > 0 and expected % 3 == 0 and
                all(node['sample_count'] == expected // 3 and node['failed_sample_count'] == 0 for node in nodes.values()))
    if (aggregate['attempted_count'] != totals['sample_count']
            or aggregate['failed_sample_count'] != totals['failed_sample_count']
            or aggregate.get('failed_nodes') != failed_nodes
            or aggregate['node_count'] != sum(node['available'] for node in nodes.values())
            or aggregate['complete'] != complete):
        raise AttemptError('inconsistent host aggregate completeness')


def validate_snapshot_source(source, directory):
    config = read_json(directory / 'config.json')
    if (not isinstance(config, dict) or not isinstance(config.get('nodes'), dict)
            or not config['nodes'] or type(config.get('samples')) is not int or config['samples'] <= 0):
        raise AttemptError('invalid snapshot requested inventory')
    labels, samples_per_node = set(config['nodes']), config['samples']
    if any(not isinstance(label, str) or not label or not isinstance(node, dict)
           or node.get('label') != label for label, node in config['nodes'].items()):
        raise AttemptError('invalid snapshot requested node')
    if (not isinstance(source, dict) or source.get('schema_version') != 2
            or source.get('claim_status') not in ('pass_candidate', 'blocked', 'partial_with_node_health_blocker',
                'partial_with_local_validator_blocker', 'partial_with_observer_blocker')
            or not isinstance(source.get('failure_signatures'), list)
            or not all(isinstance(value, str) for value in source['failure_signatures'])):
        raise AttemptError('invalid snapshot summary schema')
    if (config.get('run_id') != source.get('run_id') or config.get('run_dir') != str(directory)
            or config.get('world_id') != source.get('world_id')
            or not isinstance(config.get('world_id'), str) or not config['world_id']):
        raise AttemptError('snapshot configuration identity mismatch')
    collection, nodes, samples, totals = (source.get(key) for key in ('collection', 'nodes', 'samples', 'totals'))
    if (not isinstance(collection, dict) or not isinstance(nodes, dict) or set(nodes) != labels
            or not isinstance(samples, list) or not isinstance(totals, dict)
            or not isinstance(collection.get('node_labels'), list)
            or len(collection['node_labels']) != len(labels)
            or not all(isinstance(label, str) for label in collection['node_labels'])
            or set(collection['node_labels']) != labels):
        raise AttemptError('invalid snapshot collection inventory')
    seen = set()
    counts = {label: [0, 0] for label in labels}
    for sample in samples:
        if (not isinstance(sample, dict) or not isinstance(sample.get('label'), str) or sample['label'] not in labels
                or type(sample.get('sample_index')) is not int or not 1 <= sample['sample_index'] <= samples_per_node
                or type(sample.get('healthz_fetch_ok')) is not bool or type(sample.get('status_fetch_ok')) is not bool):
            raise AttemptError('invalid snapshot sample identity or fetch result')
        identity = (sample['label'], sample['sample_index'])
        if identity in seen:
            raise AttemptError('duplicate snapshot sample')
        seen.add(identity)
        counts[sample['label']][0] += 1
        counts[sample['label']][1] += not (sample['healthz_fetch_ok'] and sample['status_fetch_ok'])
    for label, node in nodes.items():
        attempted, failed = counts[label]
        if (not isinstance(node, dict) or any(type(node.get(key)) is not int for key in
                ('sample_count', 'success_sample_count', 'failed_sample_count'))
                or (node['sample_count'], node['success_sample_count'], node['failed_sample_count'])
                != (attempted, attempted - failed, failed)):
            raise AttemptError('inconsistent snapshot node counts')
    expected = len(labels) * samples_per_node
    failed = sum(row[1] for row in counts.values())
    required_counts = {'samples_per_node': samples_per_node, 'expected_sample_count': expected,
                       'attempted_count': len(samples), 'failed_sample_count': failed}
    if (any(type(collection.get(key)) is not int or collection[key] != value for key, value in required_counts.items())
            or type(collection.get('complete')) is not bool
            or collection['complete'] != (len(samples) == expected and failed == 0)
            or type(totals.get('sample_record_count')) is not int or totals['sample_record_count'] != len(samples)
            or type(totals.get('node_count')) is not int or totals['node_count'] != len({sample['label'] for sample in samples})):
        raise AttemptError('inconsistent snapshot aggregate completeness')



TRAFFIC_LABELS = ('local_node', 'sequencer_ecs', 'storage_ecs')


def ndjson_rows(payload):
    try:
        rows = [json.loads(line) for line in payload.decode('utf-8').splitlines() if line.strip()]
    except (UnicodeError, ValueError) as error:
        raise AttemptError('invalid traffic history encoding') from error
    if any(not isinstance(row, dict) for row in rows):
        raise AttemptError('invalid traffic history record')
    return rows


def protected_regular(descriptor):
    metadata = os.fstat(descriptor)
    if (not stat.S_ISREG(metadata.st_mode) or metadata.st_uid != os.getuid()
            or metadata.st_mode & 0o022 or metadata.st_nlink != 1):
        raise AttemptError('history file is not an owner protected unaliased regular file')
    return metadata


def read_history(path):
    reject_symlinks(path)
    descriptor = os.open(path, os.O_RDONLY | getattr(os, 'O_NOFOLLOW', 0))
    try:
        before = protected_regular(descriptor)
        with os.fdopen(os.dup(descriptor), 'rb') as stream:
            payload = stream.read()
        after = protected_regular(descriptor)
        if (before.st_size, before.st_mtime_ns) != (after.st_size, after.st_mtime_ns):
            raise AttemptError('history changed while reading')
        return payload
    finally:
        os.close(descriptor)


def validate_history_path(path, requested_root=None):
    reject_symlinks(path)
    if not path.is_absolute() or path != Path(os.path.abspath(path)):
        raise AttemptError('history path is not canonical absolute path')
    for parent in (() if requested_root is not None and path == requested_root / 'history.ndjson' else path.parents):
        if (parent / ANCHOR).exists() or (parent / STATE).exists():
            if path != parent / 'history.ndjson':
                raise AttemptError('history path overlaps reserved collector artifacts')
            break
    if path.name.endswith(('.traffic-history.lock', '.traffic-history.identity')):
        raise AttemptError('history path overlaps persistent history metadata')
    if path.name in (STATE, ANCHOR, LOCK, 'latest_summary.json', 'latest_summary.md',
                     'config.json', 'samples.ndjson', 'history_receipt.json'):
        raise AttemptError('history path overlaps reserved artifact name')
    if path.exists():
        descriptor = os.open(path, os.O_RDONLY | getattr(os, 'O_NOFOLLOW', 0))
        try:
            protected_regular(descriptor)
        finally:
            os.close(descriptor)


@contextmanager
def locked_history(path):
    """Serialize cooperating writers with a durably bound, never replaced carrier."""
    reject_symlinks(path)
    durable_mkdir(path.parent)
    parent = os.open(path.parent, os.O_RDONLY | getattr(os, 'O_DIRECTORY', 0) | getattr(os, 'O_NOFOLLOW', 0))
    lock = path.with_name('.' + path.name + '.traffic-history.lock')
    anchor = path.with_name('.' + path.name + '.traffic-history.identity')
    descriptor = None
    try:
        reject_symlinks(lock)
        reject_symlinks(anchor)
        flags = os.O_RDWR | getattr(os, 'O_NOFOLLOW', 0)
        if not anchor.exists():
            flags |= os.O_CREAT
        descriptor = os.open(lock, flags, 0o600)
        def check(validate_anchor=True):
            reject_symlinks(path)
            reject_symlinks(lock)
            reject_symlinks(anchor)
            for opened, named, directory in ((os.fstat(parent), path.parent.stat(), True),
                                             (os.fstat(descriptor), lock.stat(), False)):
                if ((opened.st_dev, opened.st_ino) != (named.st_dev, named.st_ino)
                        or opened.st_uid != os.getuid() or opened.st_mode & 0o022
                        or not (stat.S_ISDIR(opened.st_mode) if directory else stat.S_ISREG(opened.st_mode))
                        or (not directory and (stat.S_IMODE(opened.st_mode) != 0o600 or opened.st_nlink != 1))):
                    raise AttemptError('history lock identity or permissions changed')
            token = read_lock_token(descriptor)
            if check.token is not None and token != check.token:
                raise AttemptError('history lock token changed')
            if validate_anchor:
                metadata = os.fstat(descriptor)
                expected = {'schema_version': 1, 'history_path': str(path), 'device': metadata.st_dev,
                            'inode': metadata.st_ino, 'token': token}
                if read_json(anchor) != expected:
                    raise AttemptError('persistent history lock identity changed')
                anchor_fd = os.open(anchor, os.O_RDONLY | getattr(os, 'O_NOFOLLOW', 0))
                try:
                    protected_regular(anchor_fd)
                finally:
                    os.close(anchor_fd)
        check.token = None
        check(validate_anchor=False)
        fcntl.flock(descriptor, fcntl.LOCK_EX)
        if not anchor.exists():
            # A token with no anchor is an interrupted initialization, never a
            # license to establish a second identity domain after recovery.
            if read_lock_token(descriptor) is not None:
                raise AttemptError('history identity anchor missing after token initialization')
            payload = (str(uuid.uuid4()) + '\n').encode('ascii')
            if os.write(descriptor, payload) != len(payload):
                raise AttemptError('history lock token write incomplete')
            os.fsync(descriptor)
            sync_directory(path.parent)
            metadata = os.fstat(descriptor)
            atomic_write(anchor, encoded({'schema_version': 1, 'history_path': str(path),
                'device': metadata.st_dev, 'inode': metadata.st_ino, 'token': read_lock_token(descriptor)}))
        check.token = read_lock_token(descriptor)
        check()
        yield check
        check()
    finally:
        if descriptor is not None:
            os.close(descriptor)
        os.close(parent)


def own_traffic_rows(directory, config):
    reject_symlinks(directory / 'samples.ndjson')
    rows = ndjson_rows((directory / 'samples.ndjson').read_bytes())
    seen = set()
    for row in rows:
        identity = (row.get('label'), row.get('sample_index'))
        if (row.get('run_id') != config['run_id'] or row.get('label') not in TRAFFIC_LABELS
                or type(row.get('sample_index')) is not int or not 1 <= row['sample_index'] <= config['samples']
                or type(row.get('status_fetch_ok')) is not bool
                or type(row.get('captured_at_unix_ms')) is not int or row['captured_at_unix_ms'] < 0
                or identity in seen):
            raise AttemptError('invalid own traffic sample identity')
        seen.add(identity)
    return rows


def history_transaction(root, sequence, attempt_id, history_path, retention_minutes, read_only=False):
    """Commit one history batch and freeze the reducer input before terminal CAS."""
    global _ACTIVE_GUARD
    if type(retention_minutes) is not int or retention_minutes <= 0 or type(read_only) is not bool:
        raise AttemptError('invalid history retention or mode')
    validate_history_path(history_path)
    with locked(root) as store_check:
        state = load_state(root, 'traffic')
        latest = state['latest_attempt']
        if (latest['sequence'], latest['attempt_id'], latest['result']) != (sequence, attempt_id, 'pending'):
            return 3
        directory = root / latest['run_directory']
        config = read_json(directory / 'config.json')
        if (not isinstance(config, dict) or type(config.get('summary_only')) is not bool
                or config['summary_only'] != read_only or type(config.get('samples')) is not int
                or config['samples'] <= 0 or config.get('run_id') != str(sequence) + '-' + attempt_id
                or config.get('run_dir') != str(directory) or config.get('history_path') != str(history_path)
                or config.get('history_input_path') != str(directory / 'history.ndjson')):
            raise AttemptError('traffic history configuration identity mismatch')
        samples = b'' if read_only else (directory / 'samples.ndjson').read_bytes()
        if not read_only:
            own_traffic_rows(directory, config)
        snapshot, receipt = directory / 'history.ndjson', directory / 'history_receipt.json'
        reject_symlinks(snapshot)
        reject_symlinks(receipt)
        identity = {'history_path': str(history_path), 'read_only': read_only,
                    'input_sha256': digest(samples), 'collector': 'traffic',
                    'sequence': sequence, 'attempt_id': attempt_id}
        if receipt.exists():
            recorded = read_json(receipt)
            if not isinstance(recorded, dict) or any(recorded.get(key) != value for key, value in identity.items()) or recorded.get('snapshot_sha256') != digest(snapshot.read_bytes()):
                raise AttemptError('immutable history receipt differs')
            return 0
        if snapshot.exists():
            raise AttemptError('history transaction interrupted; start a new attempt')
        if read_only:
            payload = read_history(history_path)
            ndjson_rows(payload)
            atomic_write(snapshot, payload)
            atomic_write(receipt, encoded(dict(identity, snapshot_sha256=digest(payload))))
            return 0
        with locked_history(history_path) as history_check:
            def check():
                store_check()
                history_check()
            _ACTIVE_GUARD = check
            try:
                previous = read_history(history_path) if history_path.exists() else b''
                old_rows = ndjson_rows(previous)
                cutoff = time.time_ns() // 1_000_000 - retention_minutes * 60_000
                retained = [row for row in old_rows if type(row.get('captured_at_unix_ms')) is not int
                            or row['captured_at_unix_ms'] >= cutoff]
                # New rows survive clock regression and preserve their append order.
                rows = retained + ndjson_rows(samples)
                payload = b''.join(encoded(row).replace(b'\n', b'') + b'\n' for row in rows)
                atomic_write(snapshot, payload)
                check()
                atomic_write(history_path, payload)
                check()
                atomic_write(receipt, encoded(dict(identity, snapshot_sha256=digest(payload))))
            finally:
                _ACTIVE_GUARD = store_check
        return 0


def validate_traffic_source(source, directory):
    config = read_json(directory / 'config.json')
    if (not isinstance(source, dict) or not isinstance(config, dict) or type(config.get('summary_only')) is not bool
            or type(config.get('samples')) is not int or config['samples'] <= 0
            or config.get('run_dir') != str(directory) or config.get('run_id') != source.get('run_id')
            or config.get('history_input_path') != str(directory / 'history.ndjson')
            or not isinstance(source, dict) or source.get('schema_version') != 2
            or source.get('history_path') != config['history_input_path']
            or type(source.get('as_of_unix_ms')) is not int or source['as_of_unix_ms'] < 0):
        raise AttemptError('invalid traffic summary configuration or schema')
    replay = config['summary_only']
    if config.get('as_of_unix_ms') is not None and config['as_of_unix_ms'] != source['as_of_unix_ms']:
        raise AttemptError('traffic replay boundary mismatch')
    rows = [] if replay else own_traffic_rows(directory, config)
    failed = sum(row['status_fetch_ok'] is False for row in rows)
    expected = 0 if replay else len(TRAFFIC_LABELS) * config['samples']
    required = {'mode': 'history_replay' if replay else 'live', 'samples_per_node': config['samples'],
                'expected_sample_count': expected, 'attempted_count': len(rows), 'failed_sample_count': failed,
                'complete': replay or (len(rows) == expected and failed == 0), 'node_labels': list(TRAFFIC_LABELS)}
    collection = source.get('collection')
    if not isinstance(collection, dict) or any(type(collection.get(key)) is not type(value) or collection[key] != value for key, value in required.items()):
        raise AttemptError('inconsistent traffic collection completeness')
    receipt = read_json(directory / 'history_receipt.json')
    reject_symlinks(directory / 'history.ndjson')
    history = (directory / 'history.ndjson').read_bytes()
    if (not isinstance(receipt, dict) or receipt.get('collector') != 'traffic'
            or str(receipt.get('sequence')) + '-' + str(receipt.get('attempt_id')) != config['run_id']
            or receipt.get('history_path') != config.get('history_path') or receipt.get('read_only') != replay
            or receipt.get('input_sha256') != digest(b'' if replay else (directory / 'samples.ndjson').read_bytes())
            or receipt.get('snapshot_sha256') != digest(history)):
        raise AttemptError('traffic history snapshot receipt mismatch')
    nodes = source.get('nodes')
    if not isinstance(nodes, dict) or set(nodes) != set(TRAFFIC_LABELS) or not isinstance(source.get('aggregate'), dict):
        raise AttemptError('invalid traffic triad shape')
    for key in ('window_minutes', 'max_sample_age_secs', 'max_sample_gap_secs'):
        if type(config.get(key)) is not int or config[key] <= 0:
            raise AttemptError('invalid traffic freshness configuration')
    module_path = Path(__file__).with_name('traffic_monitor_freshness.py')
    spec = importlib.util.spec_from_file_location('traffic_attempt_freshness', module_path)
    freshness = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(freshness)
    records = ndjson_rows(history)
    policy = {'version': 1, 'max_sample_age_ms': config['max_sample_age_secs'] * 1000,
              'max_sample_gap_ms': config['max_sample_gap_secs'] * 1000}
    if source.get('freshness_policy') != policy or source.get('window_minutes_requested') != config['window_minutes']:
        raise AttemptError('traffic freshness policy mismatch')
    if any(type(source.get(key)) is not int or source[key] != expected for key, expected in
           (('history_record_count', len(records)), ('history_record_count_before_prune', len(records)), ('history_pruned_count', 0))):
        raise AttemptError('traffic immutable history record counts mismatch')
    for label in TRAFFIC_LABELS:
        node = nodes[label]
        expected_metadata, _, _ = freshness.analyze([row for row in records if (row.get('label') or row.get('node_label')) == label],
            source['as_of_unix_ms'], config['window_minutes'], config['max_sample_age_secs'] * 1000,
            config['max_sample_gap_secs'] * 1000, config.get('as_of_unix_ms') is not None)
        if not isinstance(node, dict):
            raise AttemptError('invalid traffic node shape')
        for key, value in expected_metadata.items():
            if key == 'window':
                if not isinstance(node.get(key), dict) or any(node[key].get(field) != item for field, item in value.items()):
                    raise AttemptError('traffic window metadata mismatch')
            elif node.get(key) != value:
                raise AttemptError('traffic freshness metadata mismatch')
    if source['aggregate'].get('node_count') != sum(node['available'] for node in nodes.values()):
        raise AttemptError('traffic aggregate node count mismatch')

def finish(root, sequence, attempt_id, errors, collector='observability'):
    with locked(root) as check:
        state = load_state(root, collector)
        latest = state['latest_attempt']
        own_directory = root / ('runs/' + str(sequence).zfill(12) + '-' + attempt_id)
        source = None
        source_markdown = None
        if not any(row.get('stage') == 'summary' for row in errors):
            try:
                source = read_json(own_directory / 'report' / 'latest_summary.json')
                if collector == 'traffic':
                    validate_traffic_source(source, own_directory)
                    if source['collection']['complete'] is not True and not any(row.get('stage') == 'traffic' for row in errors):
                        raise AttemptError('incomplete traffic collection lacks failure result')
                if collector == 'snapshot':
                    validate_snapshot_source(source, own_directory)
                    if source['collection']['complete'] is not True and not any(row.get('stage') == 'snapshot' for row in errors):
                        raise AttemptError('incomplete snapshot collection lacks failure result')
                if collector == 'host':
                    validate_host_source(source)
                    if source['aggregate']['complete'] is not True and not any(row.get('stage') == 'host' for row in errors):
                        raise AttemptError('incomplete host collection lacks failure result')
                if (not isinstance(source, dict) or (collector == 'observability' and not isinstance(source.get('overall'), dict))
                        or (collector == 'observability' and (not isinstance(source.get('overall', {}).get('alerts', []), list)
                        or not all(isinstance(alert, str) for alert in source.get('overall', {}).get('alerts', []))))
                        or source.get('run_id') != str(sequence) + '-' + attempt_id
                        or source.get('run_dir') != str(own_directory)):
                    raise AttemptError('summary source is not associated with reserved run')
                source_markdown = (own_directory / 'report' / 'latest_summary.md').read_text(encoding='utf-8')
            except (AttemptError, OSError, UnicodeError, ValueError, TypeError, AttributeError):
                source = None
                source_markdown = None
                errors = [*errors, {'stage': 'summary', 'exit_code': 1, 'error': 'summary_failed'}]
        own_result = {'sequence': sequence, 'attempt_id': attempt_id,
                      'result': 'failed' if errors else 'success', 'errors': errors}
        # Superseded attempts may only leave their own completion evidence.
        result_path = own_directory / 'terminal_result.json'
        if result_path.exists():
            if read_json(result_path) != own_result:
                raise AttemptError('immutable terminal evidence differs')
        else:
            atomic_write(result_path, encoded(own_result))
        if latest['sequence'] != sequence or latest['attempt_id'] != attempt_id or latest['result'] != 'pending':
            return 3
        attempt = dict(latest, result=own_result['result'], errors=errors,
                       error=errors[0]['error'] if errors else None,
                       finished_at_unix_ms=time.time_ns() // 1_000_000, generation=str(uuid.uuid4()))
        previous_success = state.get('last_success')
        report = report_for(attempt, previous_success, source, collector)
        markdown = '# ' + collector.capitalize() + ' attempt ' + attempt['result'] + '\n'
        if errors:
            markdown += '\nCanonical overall status: `blocked`; current measurement available: `false`\n'
        if source_markdown is not None:
            markdown += source_markdown
        if errors:
            markdown += '\nCollectors failed: ' + ', '.join(row['stage'] for row in errors) + '\n'
        # Canonical state remains pending until both report artifacts are durable.
        reference = publish(root, state, attempt, report, markdown)
        if not errors:
            state['last_success'] = dict(attempt, publication=reference)
        state.update(latest_attempt=attempt, publication=reference)
        check()
        atomic_write(root / STATE, encoded(state))
        return 1 if errors else 0


def validated_artifacts(root, attempt, reference, check_cache=False):
    if not isinstance(reference, dict) or reference.get('generation') != attempt.get('generation'):
        raise AttemptError('canonical generation unavailable')
    artifacts = {}
    for kind in ('json', 'md'):
        expected = attempt['run_directory'] + '/publication/' + attempt['generation'] + '/summary.' + kind
        if reference.get(kind + '_path') != expected:
            raise AttemptError('artifact is not associated with its attempt generation')
        path = root / expected
        reject_symlinks(path)
        payload = path.read_bytes()
        if digest(payload) != reference.get(kind + '_sha256'):
            raise AttemptError('canonical artifact digest mismatch')
        if check_cache and (root / ('latest_summary.' + kind)).read_bytes() != payload:
            raise AttemptError('latest artifact does not match canonical generation')
        artifacts[kind] = payload
    return artifacts


def read(root, collector='observability'):
    with locked(root):
        state = load_state(root, collector)
        attempt = state['latest_attempt']
        if not attempt:
            raise AttemptError('no attempt available')
        artifacts = validated_artifacts(root, attempt, state.get('publication'), check_cache=True)
        if state.get('last_success'):
            success = state['last_success']
            historical = validated_artifacts(root, success, success['publication'])
            expected = {key: value for key, value in success.items() if key != 'publication'}
            if json.loads(historical['json']).get('attempt') != expected:
                raise AttemptError('last-success report identity mismatch')
        report = json.loads(artifacts['json'])
        if report.get('attempt') != attempt:
            raise AttemptError('report attempt identity mismatch')
        report['last_success'] = state.get('last_success')
        report['canonical_state'] = {'path': str(root / STATE), 'generation': attempt['generation'], 'validated': True,
                                     'validation_scope': 'attempt_order_and_artifact_integrity',
                                     'read_at_unix_ms': time.time_ns() // 1_000_000}
        markdown = ('# Canonically validated ' + collector + ' attempt\n\n'
                    + '- Sequence: `' + str(attempt['sequence']) + '`; result: `' + attempt['result'] + '`\n'
                    + '- Generation: `' + attempt['generation'] + '`\n'
                    + '- Last successful sequence: `' + str((state.get('last_success') or {}).get('sequence')) + '`\n\n'
                    + artifacts['md'].decode())
        return report, markdown, 0 if attempt['result'] == 'success' else 1


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('command', choices=('begin', 'finish', 'read', 'history', 'history-validate'))
    parser.add_argument('--out-dir', required=True)
    parser.add_argument('--collector', choices=('observability', 'host', 'snapshot', 'traffic'), default='observability')
    parser.add_argument('--sequence', type=int)
    parser.add_argument('--attempt-id')
    parser.add_argument('--scope-arg', action='append', default=[])
    parser.add_argument('--error', action='append', default=[])
    parser.add_argument('--history-path')
    parser.add_argument('--retention-minutes', type=int)
    parser.add_argument('--read-only-history', action='store_true')
    parser.add_argument('--format', choices=('json', 'markdown'), default='json')
    args = parser.parse_args()
    try:
        root = Path(os.path.abspath(args.out_dir)).resolve(strict=False)
        if args.command == 'begin':
            print(json.dumps(begin(root, args.scope_arg, args.collector)))
            return 0
        if args.command == 'history-validate':
            if args.collector != 'traffic' or not args.history_path:
                parser.error('history-validate needs traffic collector and history-path')
            path = Path(os.path.abspath(args.history_path))
            validate_history_path(path, requested_root=root)
            if root in path.parents and path != root / 'history.ndjson':
                raise AttemptError('history path overlaps requested collector store')
            if not args.read_only_history and path.parent.exists():
                parent = path.parent.stat()
                if not stat.S_ISDIR(parent.st_mode) or parent.st_uid != os.getuid() or parent.st_mode & 0o022:
                    raise AttemptError('history parent is not owner protected')
            if args.read_only_history and not path.is_file():
                raise AttemptError('history replay source missing')
            return 0
        if args.command == 'history':
            if args.collector != 'traffic' or not args.sequence or not args.attempt_id or not args.history_path or not args.retention_minutes or args.retention_minutes <= 0:
                parser.error('history needs traffic collector, sequence, attempt-id, history-path and positive retention-minutes')
            return history_transaction(root, args.sequence, args.attempt_id, Path(os.path.abspath(args.history_path)), args.retention_minutes, args.read_only_history)
        if args.command == 'finish':
            if not args.sequence or not args.attempt_id:
                parser.error('finish needs sequence and attempt-id')
            uuid.UUID(args.attempt_id)
            errors = []
            for value in args.error:
                stage, code = value.split(':', 1)
                if stage not in ('snapshot', 'host', 'traffic', 'wasm_local_node', 'wasm_sequencer_ecs', 'wasm_storage_ecs', 'summary'):
                    raise AttemptError('unsupported collector stage')
                errors.append({'stage': stage, 'exit_code': int(code), 'error': 'summary_failed' if stage == 'summary' else 'collector_failed'})
            return finish(root, args.sequence, args.attempt_id, errors, args.collector)
        report, markdown, code = read(root, args.collector)
        print(json.dumps(report) if args.format == 'json' else markdown, end='\n')
        return code
    except (AttemptError, OSError, ValueError, TypeError, KeyError, RuntimeError) as error:
        if args.command == 'read':
            unknown = {'current_measurement_available': False, 'attempt': {'result': 'unknown'},
                       'latest_attempt': {'result': 'unknown'},
                       'overall': {'status': 'unknown', 'alerts': ['state_unavailable']}}
            if args.collector == 'traffic':
                unknown.update(schema_version=2, collection={'complete': False}, nodes={label: {
                    'available': False, 'freshness': {'state': 'unknown'}} for label in ('local_node', 'sequencer_ecs', 'storage_ecs')})
            if args.collector == 'host':
                unknown.update(schema_version=2, aggregate={'complete': False})
            if args.collector == 'snapshot':
                unknown.update(schema_version=2, claim_status='blocked', collection={'complete': False},
                               failure_signatures=['state_unavailable'])
            print(json.dumps(unknown) if args.format == 'json' else '# Observability state unavailable\n\nCurrent measurement unavailable; latest attempt unknown.')
        print('observability attempt: state or publication unavailable', file=sys.stderr)
        return 2


if __name__ == '__main__':
    sys.exit(main())
