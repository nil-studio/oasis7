#!/usr/bin/env python3
"""Durable observability, host and snapshot collection ordering. Read this canonical state, not an unvalidated cache."""
import argparse
from contextlib import contextmanager
try:
    import fcntl
except ImportError:
    fcntl = None
import hashlib
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
    if collector not in ('observability', 'host', 'snapshot') or value.get('collector', 'observability') != collector:
        raise AttemptError('collector profile mismatch')


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
    if runs.exists():
        for directory in runs.iterdir():
            try:
                sequence = int(directory.name.split('-', 1)[0])
            except ValueError as error:
                raise AttemptError('invalid retained run sequence evidence') from error
            if sequence > state['last_sequence']:
                raise AttemptError('retained run evidence exceeds persisted sequence')
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
    candidate = subprocess.run(['git', 'rev-parse', 'HEAD'], cwd=Path(__file__).parent.parent,
                               capture_output=True, text=True)
    material = {'candidate': candidate.stdout.strip() if candidate.returncode == 0 else 'unknown',
                'collector': collector, 'arguments': args, 'code': {path: digest(Path(path).read_bytes()) for path in sources}}
    # Bind file inputs by content too, without persisting their paths or contents.
    material['files'] = [digest(Path(arg).read_bytes()) for arg in args if Path(arg).is_file()]
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
            if (root / 'runs').exists() and any((root / 'runs').iterdir()):
                raise AttemptError('run artifacts exist without sequence anchor')
            store_id = str(uuid.uuid4())
            metadata = (root / LOCK).stat()
            atomic_write(root / ANCHOR, encoded({'schema_version': 1, 'collector': collector, 'store_id': store_id,
                         'high_water_sequence': 0, 'lock_identity': {'device': metadata.st_dev, 'inode': metadata.st_ino,
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
            except (AttemptError, OSError, UnicodeError):
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
    parser.add_argument('command', choices=('begin', 'finish', 'read'))
    parser.add_argument('--out-dir', required=True)
    parser.add_argument('--collector', choices=('observability', 'host', 'snapshot'), default='observability')
    parser.add_argument('--sequence', type=int)
    parser.add_argument('--attempt-id')
    parser.add_argument('--scope-arg', action='append', default=[])
    parser.add_argument('--error', action='append', default=[])
    parser.add_argument('--format', choices=('json', 'markdown'), default='json')
    args = parser.parse_args()
    try:
        root = Path(os.path.abspath(args.out_dir)).resolve(strict=False)
        if args.command == 'begin':
            print(json.dumps(begin(root, args.scope_arg, args.collector)))
            return 0
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
