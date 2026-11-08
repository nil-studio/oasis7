#!/usr/bin/env python3
"""Bind runtime observations to numeric Actions jobs without guessing from names.

This additive report does not change the v1/v2 telemetry evidence contracts or gates.
"""
import argparse
import hashlib
import json
from pathlib import Path
import re
import sys

import engineering_telemetry as telemetry
import engineering_telemetry_adapters as adapters

SCHEMA = 'oasis7-engineering-bindings/v1'
MARKER = 'OASIS7_ENGINEERING_JOB_V1='
STEP = 'Record engineering job observation'
# This standalone body is embedded once as a YAML step anchor. It must also work
# in controller jobs that deliberately have no checkout or candidate executable.
EMITTER = r'''import gzip, hashlib, json, os, pathlib, re, subprocess
E = os.environ
limit = 1024 * 1024
missing = []
def digest(raw):
    return hashlib.sha256(raw).hexdigest()
def bounded(path):
    path = pathlib.Path(path)
    if any(p.is_symlink() for p in (path, *path.parents)):
        raise ValueError('symlink')
    with path.open('rb') as stream:
        raw = stream.read(limit + 1)
    if len(raw) > limit:
        raise ValueError('size')
    return raw
def sha(value):
    return value if isinstance(value, str) and re.fullmatch('[0-9a-f]{40}', value) else None
def token(value):
    return isinstance(value, str) and re.fullmatch('[A-Za-z0-9][A-Za-z0-9_.:-]{0,127}', value) and not re.search(r'(?i)(password|secret|authorization|bearer|access.token|gh[pousr]_|github_pat_)', value)
def safe_json(name, fallback):
    raw = E.get(name, '')
    if len(raw) > 65536:
        raise ValueError('context_size')
    return json.loads(raw) if raw else fallback
needs = safe_json('TELEMETRY_NEEDS', {})
matrix = safe_json('TELEMETRY_MATRIX', {}) or {}
if not isinstance(needs, dict) or not isinstance(matrix, dict):
    raise ValueError('context_type')
if len(needs) > 100 or len(matrix) > 32 or any(not token(k) or not isinstance(v, dict) or v.get('result') not in ('success', 'failure', 'cancelled', 'skipped') for k, v in needs.items()):
    raise ValueError('needs_values')
if any(not token(k) or not (type(v) is bool or (type(v) is int and abs(v) <= 2**31) or token(v)) for k, v in matrix.items()):
    raise ValueError('matrix_values')
needs = {key: value.get('result') for key, value in needs.items()}
workspace = pathlib.Path(E['GITHUB_WORKSPACE'])
checkout = None
if (workspace / '.git').exists():
    try:
        checkout = sha(subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=workspace,
                         timeout=5, stderr=subprocess.DEVNULL, text=True).strip())
    except (OSError, subprocess.SubprocessError):
        missing.append('checkout_unavailable')
temp = pathlib.Path(E['RUNNER_TEMP'])
configs = {}
for label, path in [('checkout-test-config', workspace / 'scripts/ci-formal-process-suites.json'),
                    ('checkout-scope-config', workspace / 'scripts/ci-required-scope.json'),
                    ('formal-authority', temp / 'ci-authority/ci-formal-process-suites.json')]:
    if path.exists():
        try:
            configs[label] = digest(bounded(path))
        except (OSError, ValueError):
            missing.append('configuration_unavailable')
job = E['GITHUB_JOB']
locations = []
if job in ('formal-debug-build', 'formal-debug-build-trusted'):
    locations = [('produced', 'debug', temp / 'formal-debug-tests/manifest.json', False)]
elif job in ('formal-endpoint-build', 'formal-endpoint-build-trusted'):
    locations = [('produced', 'endpoint', temp / 'formal-endpoint-tests/manifest.json', False)]
elif job in ('formal-test-build', 'formal-test-build-trusted'):
    locations = [('observed-input', 'debug', temp / 'formal-debug-tests/manifest.json', False),
                 ('observed-input', 'endpoint', temp / 'formal-endpoint-tests/manifest.json', False)]
    for shard in ('runtime-protocol', 'world-history', 'healthy-set', 'storage-recovery', 'lifecycle-cli'):
        locations.append(('produced', shard, temp / 'formal-archives' / (shard + '.tar.gz'), True))
elif job == 'formal-process-tests':
    locations = [('observed-input', matrix.get('shard'), temp / 'native-tests/manifest.json', False)]
manifests = []
for direction, label, path, archived in locations:
    if not path.exists():
        continue
    try:
        if archived:
            if any(p.is_symlink() for p in (path, *path.parents)):
                raise ValueError('symlink')
            with gzip.open(path, 'rb') as stream:
                header = stream.read(512)
                if (len(header) != 512 or header[:100].split(b'\0', 1)[0] != b'manifest.json'
                        or header[156:157] not in (b'0', b'\0')
                        or int(header[148:156].strip(b'\0 '), 8) != sum(header[:148]) + 256 + sum(header[156:])):
                    raise ValueError('manifest_header')
                size = int(header[124:136].strip(b'\0 '), 8)
                if not 0 <= size <= limit:
                    raise ValueError('manifest_size')
                raw = stream.read(size)
                if len(raw) != size:
                    raise ValueError('manifest_truncated')
        else:
            raw = bounded(path)
        manifest = json.loads(raw)
        if type(manifest.get('schema_version')) is not int or manifest['schema_version'] != 2:
            raise ValueError('manifest_schema')
        if any(not isinstance(manifest.get(k), str) or not re.fullmatch('[1-9][0-9]{0,18}', manifest[k]) or int(manifest[k]) > 2**63-1 for k in ('producer_run_id', 'producer_attempt')):
            raise ValueError('manifest_producer')
        if not isinstance(manifest.get('config_sha256'), str) or not re.fullmatch('[0-9a-f]{64}', manifest['config_sha256']) or not token(label):
            raise ValueError('manifest_config')
        manifests.append(dict(direction=direction, label=label, manifest_sha256=digest(raw),
            source_sha=sha(manifest.get('source_sha')), tested_sha=sha(manifest.get('tested_sha')),
            producer_run_id=manifest.get('producer_run_id'), producer_attempt=manifest.get('producer_attempt'),
            config_digest=manifest.get('config_sha256')))
    except (OSError, ValueError, EOFError, KeyError, TypeError):
        missing.append('manifest_observation_unavailable')
row = dict(schema_version='oasis7-engineering-job/v1', repository=E['GITHUB_REPOSITORY'],
    run_id=int(E['GITHUB_RUN_ID']), run_attempt=int(E['GITHUB_RUN_ATTEMPT']), job_key=job,
    workflow_sha=sha(E.get('TELEMETRY_WORKFLOW_SHA')), matrix=matrix, needs=needs,
    target_source_sha=sha(E.get('TELEMETRY_SOURCE_SHA')), target_tested_sha=sha(E.get('TELEMETRY_TESTED_SHA')),
    target_policy_sha=sha(E.get('TELEMETRY_POLICY_SHA')), checkout_sha=checkout,
    configuration_digests=configs, manifests=manifests, missing_reasons=sorted(set(missing)))
print('OASIS7_' + 'ENGINEERING_JOB_V1=' + json.dumps(row, sort_keys=True, separators=(',', ':')))
'''


def workflow_step():
    return "      - &engineering-job-observation\n        name: Record engineering job observation\n        if: always() && !cancelled()\n        continue-on-error: true\n        timeout-minutes: 1\n        shell: bash\n        env:\n          TELEMETRY_WORKFLOW_SHA: ${{ github.workflow_sha }}\n          TELEMETRY_SOURCE_SHA: ${{ needs.select.outputs.source_sha || steps.select.outputs.source_sha }}\n          TELEMETRY_TESTED_SHA: ${{ needs.select.outputs.tested_sha || steps.select.outputs.tested_sha }}\n          TELEMETRY_POLICY_SHA: ${{ needs.select.outputs.policy_sha || steps.select.outputs.policy_sha }}\n          TELEMETRY_NEEDS: ${{ toJSON(needs) }}\n          TELEMETRY_MATRIX: ${{ toJSON(matrix) }}\n        run: |\n          python3 - <<'PY_OBSERVATION' # oasis7-engineering-job-observation/v1\n" + ''.join('          ' + line + '\n' for line in EMITTER.splitlines()) + '          PY_OBSERVATION\n'


def validate_observation(row, repository, run_id, attempt):
    fields = {'schema_version', 'repository', 'run_id', 'run_attempt', 'job_key', 'workflow_sha',
              'matrix', 'needs', 'target_source_sha', 'target_tested_sha', 'target_policy_sha',
              'checkout_sha', 'configuration_digests', 'manifests', 'missing_reasons'}
    if not isinstance(row, dict) or set(row) != fields or row['schema_version'] != 'oasis7-engineering-job/v1':
        raise ValueError('invalid_observation_schema')
    if (row['repository'] != repository or type(row['run_id']) is not int or row['run_id'] != run_id
            or type(row['run_attempt']) is not int or row['run_attempt'] != attempt):
        raise ValueError('observation_identity_mismatch')
    adapters._field_value('job_key', row['job_key'])
    adapters._field_value('matrix', row['matrix'])
    for field in ('workflow_sha', 'target_source_sha', 'target_tested_sha', 'target_policy_sha', 'checkout_sha'):
        if row[field] is not None and adapters.sha(row[field]) is None:
            raise ValueError('invalid_observation_sha')
    needs = row['needs']
    if (not isinstance(needs, dict) or len(needs) > 100 or
            any(not adapters._token(key) or value not in ('success', 'failure', 'cancelled', 'skipped')
                for key, value in needs.items())):
        raise ValueError('invalid_observation_dependencies')
    configurations = row['configuration_digests']
    if (not isinstance(configurations, dict) or set(configurations) - {'checkout-test-config', 'checkout-scope-config', 'formal-authority'}
            or any(not isinstance(v, str) or not adapters._DIGEST.fullmatch(v) for v in configurations.values())):
        raise ValueError('invalid_observation_configuration')
    if (not isinstance(row['missing_reasons'], list) or len(row['missing_reasons']) > 8 or
            any(v not in ('checkout_unavailable', 'configuration_unavailable', 'manifest_observation_unavailable') for v in row['missing_reasons'])):
        raise ValueError('invalid_observation_reasons')
    if not isinstance(row['manifests'], list) or len(row['manifests']) > 8:
        raise ValueError('invalid_observation_manifests')
    seen = set()
    for item in row['manifests']:
        if (not isinstance(item, dict) or set(item) != {'direction', 'label', 'manifest_sha256', 'source_sha', 'tested_sha',
                                                      'producer_run_id', 'producer_attempt', 'config_digest'} or
                item['direction'] not in ('produced', 'observed-input') or not adapters._token(item['label'])):
            raise ValueError('invalid_manifest_observation')
        for key in ('manifest_sha256', 'config_digest'):
            if not isinstance(item[key], str) or not adapters._DIGEST.fullmatch(item[key]):
                raise ValueError('invalid_manifest_digest')
        for key in ('source_sha', 'tested_sha'):
            if item[key] is not None and adapters.sha(item[key]) is None:
                raise ValueError('invalid_manifest_sha')
        for key in ('producer_run_id', 'producer_attempt'):
            if not isinstance(item[key], str) or not re.fullmatch(r'[1-9][0-9]{0,18}', item[key]) or int(item[key]) > 2**63-1:
                raise ValueError('invalid_manifest_producer')
        identity = (item['direction'], item['label'])
        if identity in seen:
            raise ValueError('duplicate_manifest_observation')
        seen.add(identity)
    return row


def parse_job_log(raw, job, repository, run_id, attempt):
    """Read exactly one marker emitted inside the dedicated step's time window.

    Raw logs are never persisted in a report. API step number/time and the marker
    offset are retained. Boundary-equal timestamps are rejected as ambiguous.
    """
    steps = [s for s in job.get('steps', []) if s.get('name') == STEP]
    if len(steps) != 1 or steps[0].get('conclusion') != 'success':
        raise ValueError('observation_step_unavailable')
    step = steps[0]
    start, end = (telemetry.precise_timestamp(step.get(k)) for k in ('started_at', 'completed_at'))
    # Actions timestamps have whole-second precision. End is exclusive at the
    # next step start only when it is strictly later than this step's start.
    next_starts = [telemetry.precise_timestamp(s['started_at']) for s in job.get('steps', [])
                   if type(s.get('number')) is int and s['number'] > step['number'] and s.get('started_at')]
    stop = min([end + 1] + next_starts)
    candidates = []
    active = False
    groups = 0
    for number, line in enumerate(raw.decode('utf-8').splitlines(), 1):
        if '##[group]' in line:
            active = "##[group]Run python3 - <<'PY_OBSERVATION' # oasis7-engineering-job-observation/v1" in line
            if active:
                header_stamp = telemetry.precise_timestamp(line.split(' ', 1)[0])
                if not start <= header_stamp < stop:
                    raise ValueError('observation_header_outside_step')
            groups += int(active)
        match = re.match(r'^(\d{4}-\d\d-\d\dT\S+) ' + MARKER + r'(\{.*\})$', line)
        if not match:
            continue
        if not active:
            raise ValueError('observation_outside_dedicated_step')
        stamp = telemetry.precise_timestamp(match[1])
        if not start <= stamp < stop or stop <= start:
            raise ValueError('observation_outside_step')
        candidates.append((number, json.loads(match[2])))
    if len(candidates) != 1 or groups != 1:
        raise ValueError('observation_missing_or_ambiguous')
    number, row = candidates[0]
    validate_observation(row, repository, run_id, attempt)
    return row, dict(step_number=step['number'], log_line=number, raw_sha256=hashlib.sha256(raw).hexdigest())


def workflow_dependencies(raw):
    """Recognize only this repository's static top-level needs syntax, fail closed."""
    text = raw.decode('utf-8')
    if text.count('\njobs:\n') != 1:
        raise ValueError('unsupported_workflow_jobs')
    jobs = re.findall(r'^  ([a-z][a-z0-9-]*):\n(.*?)(?=^  [a-z][a-z0-9-]*:|\Z)', text.split('\njobs:\n')[1], re.M | re.S)
    declarations = re.findall(r'^  [^ #\n][^\n]*:\s*$', text.split('\njobs:\n')[1], re.M)
    if len(declarations) != len(jobs):
        raise ValueError('unsupported_workflow_job_declaration')
    result = {}
    for key, body in jobs:
        if key in result:
            raise ValueError('unsupported_workflow_job')
        header = body.split('    steps:', 1)[0]
        declarations = re.findall(r'''^    ["']?needs["']?\s*:.*$''', body, re.M)
        lines = re.findall(r'^    needs: (.+)$', header, re.M)
        if len(declarations) != len(lines) or re.search(r'^    <<\s*:', body, re.M):
            raise ValueError('unsupported_workflow_needs')
        if len(lines) > 1:
            raise ValueError('ambiguous_workflow_needs')
        value = lines[0] if lines else ''
        if value.startswith('[') and value.endswith(']'):
            values = [v.strip() for v in value[1:-1].split(',')]
        else:
            values = [value] if value else []
        if any(not re.fullmatch('[a-z][a-z0-9-]*', v) for v in values) or len(values) != len(set(values)):
            raise ValueError('unsupported_workflow_needs')
        result[key] = values
    if not result or any(dep not in result for needs in result.values() for dep in needs):
        raise ValueError('missing_workflow_dependency')
    return result


def bind(run, pages, logs, workflow_raw=None, workflow_sha=None, pagination_complete=False, scope='formal-process'):
    """Return facts and limitations, not a fabricated v2 complete graph."""
    repository = run.get('repository', {}).get('full_name')
    run_id, attempt = run.get('id'), run.get('run_attempt')
    if (not isinstance(repository, str) or not adapters._REPOSITORY.fullmatch(repository)
            or any(type(v) is not int or v <= 0 for v in (run_id, attempt)) or not adapters.sha(run.get('head_sha'))):
        raise ValueError('invalid_run_identity')
    result = dict(schema_version=SCHEMA, repository=repository, run_id=run_id, run_attempt=attempt,
                  scope=scope, source_sha=run['head_sha'], jobs=[], dependency_bindings=[], artifact_bindings=[],
                  planned_unexecuted=[], missing_reasons=[], analysis=dict(status='unavailable', reason='complete_execution_input_contract_not_proven'))
    missing = result['missing_reasons']
    if not pagination_complete:
        missing.append('incomplete_jobs_pagination')
    jobs, seen, duplicate_ids = [], set(), set()
    for page in pages:
        for job in page.get('jobs', []):
            jid = job.get('id')
            if (type(jid) is not int or jid <= 0 or jid in seen or job.get('run_id') != run_id
                    or job.get('run_attempt') != attempt or job.get('head_sha') != run['head_sha']):
                if jid in seen:
                    duplicate_ids.add(jid)
                missing.append('job_identity_mismatch_or_duplicate'); continue
            seen.add(jid)
            jobs.append(job)
    jobs = [job for job in jobs if job['id'] not in duplicate_ids]
    observations, keyed, conflicting_keys = {}, {}, set()
    for job in jobs:
        jid = job['id']
        if job.get('conclusion') == 'skipped':
            continue  # Never assign a symbolic identity from its display name.
        if jid not in logs:
            continue
        try:
            row, evidence = parse_job_log(logs[jid], job, repository, run_id, attempt)
            if row['workflow_sha'] != workflow_sha or adapters.sha(workflow_sha) is None:
                raise ValueError('workflow_identity_unverified')
            if row['target_source_sha'] != run['head_sha']:
                raise ValueError('target_source_mismatch')
            key = (row['job_key'], json.dumps(row['matrix'], sort_keys=True))
            if key in keyed or key in conflicting_keys:
                if key in keyed:
                    observations.pop(keyed.pop(key), None)
                conflicting_keys.add(key)
                raise ValueError('duplicate_runtime_instance')
            keyed[key] = jid
            observations[jid] = dict(job_id=jid, execution_result=job.get('conclusion'),
                                     started_at=adapters.stamp(job.get('started_at')),
                                     completed_at=adapters.stamp(job.get('completed_at')),
                                     observation=row, evidence=evidence)
        except (ValueError, TypeError, KeyError, UnicodeError):
            missing.append('job_observation_unusable:' + str(jid))
    dependencies = {}
    if workflow_raw is not None and adapters.sha(workflow_sha):
        try:
            dependencies = workflow_dependencies(workflow_raw)
            result['workflow'] = dict(sha=workflow_sha, path='.github/workflows/rust.yml',
                                     raw_sha256=hashlib.sha256(workflow_raw).hexdigest())
        except ValueError:
            missing.append('workflow_dependencies_unavailable')
    else:
        missing.append('workflow_source_unavailable')
    closure, pending = set(), [scope]
    while pending:
        key = pending.pop()
        if key not in closure:
            closure.add(key)
            if key not in dependencies:
                missing.append('scope_dependency_unavailable:' + key)
            pending.extend(dependencies.get(key, []))
    groups = {}
    for job in observations.values():
        key = job['observation']['job_key']
        if key in closure:
            groups.setdefault(key, []).append(job)
    for key in sorted(closure):
        if key not in groups:
            skipped = [job['job_id'] for job in observations.values() if job['observation']['needs'].get(key) == 'skipped']
            if skipped:
                result['planned_unexecuted'].append(dict(job_key=key, job_id=None, result='skipped', observed_by=skipped))
            else:
                missing.append('runtime_job_binding_unavailable:' + key)
    for key, group in sorted(groups.items()):
        for job in group:
            row = job['observation']
            result['jobs'].append(job)
            if set(row['needs']) != set(dependencies.get(key, [])):
                missing.append('runtime_dependency_mismatch:' + str(job['job_id'])); continue
            for dep, execution in row['needs'].items():
                upstream = groups.get(dep, [])
                ids = [item['job_id'] for item in upstream]
                if any(item['execution_result'] != execution for item in upstream) and len(upstream) == 1:
                    missing.append('dependency_result_mismatch:' + str(job['job_id']))
                    ids = []
                result['dependency_bindings'].append(dict(consumer_job_id=job['job_id'], producer_job_key=dep,
                    producer_job_ids=ids, observed_result=execution, completeness='unknown' if len(ids) != 1 or (upstream and upstream[0]['observation']['matrix']) else 'single-instance'))
                if len(ids) != 1 or (upstream and upstream[0]['observation']['matrix']):
                    missing.append('dependency_instance_completeness_unverified:' + dep)
    produced = {}
    for job in result['jobs']:
        for item in job['observation']['manifests']:
            if item['direction'] == 'produced':
                if (item['producer_run_id'] == str(run_id) and item['producer_attempt'] == str(attempt)):
                    produced.setdefault(item['manifest_sha256'], []).append((job, item))
    for job in result['jobs']:
        for item in job['observation']['manifests']:
            if item['direction'] != 'observed-input':
                continue
            matches = produced.get(item['manifest_sha256'], [])
            good = [(producer, manifest) for producer, manifest in matches if all(manifest[k] == item[k] for k in
                    ('source_sha', 'tested_sha', 'producer_run_id', 'producer_attempt', 'config_digest'))]
            bound = len(good) == 1 and good[0][0]['job_id'] != job['job_id']
            result['artifact_bindings'].append(dict(manifest_sha256=item['manifest_sha256'],
                producer_job_id=good[0][0]['job_id'] if bound else None, consumer_job_id=job['job_id'],
                content_binding='bound' if bound else 'unknown', artifact_id=None, validation_result='unknown'))
            if not bound:
                missing.append('manifest_producer_binding_unavailable:' + str(job['job_id']))
    # Configuration observations and manifest equality do not prove every actual
    # input, matrix cardinality, artifact transport or consumer validation.
    missing += ['matrix_cardinality_not_proven', 'artifact_transport_and_validation_not_proven',
                'complete_execution_input_contract_not_proven']
    result['missing_reasons'] = sorted(set(missing))
    result['collection_status'] = 'partial'
    return result


def collect_live(repository, run_id, attempt, budget, transport):
    """Reuse bounded read-only transport. No login, workflow dispatch or retries."""
    import base64
    if (not isinstance(repository, str) or not adapters._REPOSITORY.fullmatch(repository)
            or adapters._SECRET.search(repository) or any(type(v) is not int or v <= 0 for v in (run_id, attempt))):
        raise ValueError('invalid_collection_identity')
    reader = telemetry.ActionsReader(repository, budget, transport)
    try:
        run = reader.json(f'runs/{run_id}/attempts/{attempt}', 'actions/run.json')
    except telemetry.CollectionStopped as exc:
        return dict(schema_version=SCHEMA, repository=repository, run_id=run_id, run_attempt=attempt,
                    collection_status='partial', transport='live', jobs=[], dependency_bindings=[],
                    artifact_bindings=[], planned_unexecuted=[], missing_reasons=[str(exc)],
                    analysis=dict(status='unavailable', reason='run_source_unavailable'),
                    sources=reader.sources, budget=budget.summary())
    if (not isinstance(run, dict) or run.get('id') != run_id or type(run.get('id')) is not int
            or run.get('run_attempt') != attempt or type(run.get('run_attempt')) is not int
            or not isinstance(run.get('repository'), dict) or run['repository'].get('full_name') != repository
            or adapters.sha(run.get('head_sha')) is None):
        return dict(schema_version=SCHEMA, repository=repository, run_id=run_id, run_attempt=attempt,
                    collection_status='partial', transport='live', jobs=[], dependency_bindings=[],
                    artifact_bindings=[], planned_unexecuted=[], missing_reasons=['run_identity_mismatch'],
                    analysis=dict(status='unavailable', reason='run_identity_mismatch'),
                    sources=reader.sources, budget=budget.summary())
    pages, complete, reasons = reader.pages(f'runs/{run_id}/attempts/{attempt}/jobs', 'jobs', 'jobs')
    logs, revisions = {}, set()
    for page in pages:
        for job in page['jobs']:
            if not any(s.get('name') == STEP for s in job.get('steps', []) if isinstance(s, dict)):
                continue
            if (job.get('run_id') != run_id or job.get('run_attempt') != attempt
                    or job.get('head_sha') != run.get('head_sha')):
                reasons.append('job_identity_mismatch'); continue
            try:
                jid = job['id']
                raw = reader.get(f'jobs/{jid}/logs', f'actions/job-{jid}.log', archive=True)
                budget.charge(len(raw), decoded=True)
                row, _ = parse_job_log(raw, job, repository, run_id, attempt)
                logs[jid] = raw
                if row['workflow_sha']:
                    revisions.add(row['workflow_sha'])
            except (telemetry.CollectionStopped, ValueError, TypeError, KeyError, UnicodeError):
                reasons.append('job_log_binding_unavailable:' + str(job['id']))
    workflow, revision = None, next(iter(revisions)) if len(revisions) == 1 else None
    if revision:
        try:
            reader.base = 'https://api.github.com/repos/' + repository + '/'
            data = reader.json('contents/.github/workflows/rust.yml?ref=' + revision, 'workflow/rust.json')
            if data.get('encoding') != 'base64' or not isinstance(data.get('content'), str):
                raise ValueError('invalid_workflow_response')
            workflow = base64.b64decode(data['content'].replace('\n', ''), validate=True)
            budget.charge(len(workflow), decoded=True)
        except (telemetry.CollectionStopped, ValueError, TypeError):
            reasons.append('workflow_source_unavailable')
    result = bind(run, pages, logs, workflow, revision, complete)
    result['missing_reasons'] = sorted(set(result['missing_reasons'] + reasons))
    result['transport'] = 'live'
    result['sources'] = reader.sources
    result['budget'] = budget.summary()
    return result


def live_main(argv):
    parser = argparse.ArgumentParser(description='Collect numeric-job-bound observations using existing read-only permissions.')
    parser.add_argument('repository'); parser.add_argument('run_id', type=int); parser.add_argument('attempt', type=int)
    parser.add_argument('output', type=Path)
    parser.add_argument('--max-seconds', type=float, default=120)
    parser.add_argument('--max-requests', type=int, default=100)
    parser.add_argument('--max-bytes', type=int, default=telemetry.MAX_INPUT_BYTES)
    args = parser.parse_args(argv)
    budget = telemetry.CollectionBudget(args.max_requests, args.max_seconds, args.max_bytes)
    transport = telemetry.LiveTransport(budget)
    import signal
    def cancel(signum, frame):
        budget.cancelled = True
        transport.cancel()
    previous = {sig: signal.signal(sig, cancel) for sig in (signal.SIGINT, signal.SIGTERM)}
    try:
        result = collect_live(args.repository, args.run_id, args.attempt, budget, transport)
        telemetry.write_json(args.output, result)
        return 1 if result['collection_status'] != 'complete' else 0
    except (OSError, ValueError, TypeError, KeyError, UnicodeError, telemetry.CollectionStopped):
        print('engineering bindings: collection unavailable', file=sys.stderr)
        return 1
    finally:
        for sig, handler in previous.items():
            signal.signal(sig, handler)


def main(argv=None):
    argv = sys.argv[1:] if argv is None else argv
    if argv and argv[0] == 'collect-live':
        return live_main(argv[1:])
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('directory', nargs='?', type=Path, help='fixed run.json/jobs-N.json/job-ID.log/workflow-SHA.yml exports')
    parser.add_argument('output', nargs='?', type=Path)
    parser.add_argument('--print-emitter', action='store_true')
    parser.add_argument('--workflow-sha')
    parser.add_argument('--scope', default='formal-process', choices=('formal-process', 'required-gate'))
    args = parser.parse_args(argv)
    if args.print_emitter:
        print(EMITTER, end=''); return 0
    if not args.directory or not args.output:
        parser.error('directory and output required')
    root = args.directory.resolve()
    if root == args.output.resolve() or root in args.output.resolve().parents:
        parser.error('output must be outside input directory')
    budget = telemetry.CollectionBudget()
    try:
        def read(path):
            if path.is_symlink() or not path.is_file() or path.parent.resolve() != root:
                raise ValueError('input_unavailable')
            budget.remaining()
            raw = path.read_bytes() if path.stat().st_size <= budget.limits['bytes'] - budget.decoded_bytes else b''
            if not raw:
                raise ValueError('input_empty_or_over_budget')
            budget.charge(len(raw), decoded=True)
            return raw
        run = json.loads(read(root / 'run.json'))
        pages, ids, total = [], set(), None
        for number in range(1, 101):
            page = json.loads(read(root / ('jobs-' + str(number) + '.json')))
            count = page.get('total_count')
            rows = page.get('jobs')
            if type(count) is not int or not 0 <= count <= 10000 or not isinstance(rows, list) or len(rows) > 100 or (total is not None and total != count):
                raise ValueError('invalid_jobs_page')
            current = [job.get('id') for job in rows]
            if len(set(current)) != len(current) or ids.intersection(current):
                raise ValueError('duplicate_jobs_page')
            total = count; ids.update(current); pages.append(page)
            if len(ids) == total or len(rows) < 100:
                break
        logs = {jid: read(root / ('job-' + str(jid) + '.log')) for jid in ids if (root / ('job-' + str(jid) + '.log')).exists()}
        workflow = read(root / ('workflow-' + args.workflow_sha + '.yml')) if adapters.sha(args.workflow_sha) else None
        result = bind(run, pages, logs, workflow, args.workflow_sha, len(ids) == total, args.scope)
        result['transport'] = 'recorded'
        result['budget'] = budget.summary()
        telemetry.write_json(args.output, result)
        return 1 if result['collection_status'] != 'complete' else 0
    except (OSError, ValueError, TypeError, KeyError, UnicodeError) as exc:
        print('engineering bindings: input or observation unavailable', file=sys.stderr)
        return 1


if __name__ == '__main__':
    raise SystemExit(main())
