#!/usr/bin/env python3
"""Bind complete custody build outputs and Node file shards to one selected run."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shutil
import stat
import subprocess
import tomllib

from ci_process_runner import run

WORKER = Path('crates/oasis7_custody_worker')
COMPONENTS = ('native', 'wasm-production', 'wasm-approval')
SHARDS = [0, 1, 2, 3]
ROLES = ('identity', 'owner', 'executor', 'recovery-fence', 'native-harness')
SCHEMA = 'oasis7.custody-platform/v1'
# Complete Node file durations from run 42934311475, jobs
# 130245943288/130245943018/130245943112/130245942945. Weights only
# determine placement; every discovered file still executes exactly once.
FILE_SECONDS = {
    'approval.test.mjs': 19.848,
    'client-abuse.integration.test.mjs': 1.410,
    'client-enrollment.integration.test.mjs': 1.410,
    'context-body-crypto.integration.test.mjs': 7.415,
    'context-body-signatures.integration.test.mjs': 2.011,
    'context-body.integration.test.mjs': 5.616,
    'contract.test.mjs': 1.209,
    'host-control-https.integration.test.mjs': 0.809,
    'live-context-read.integration.test.mjs': 1.609,
    'management-metadata.integration.test.mjs': 2.210,
    'native-continuation-reference.test.mjs': 31.037,
    'native-identity.integration.test.mjs': 29.234,
    'native-provision.integration.test.mjs': 75.466,
    'native-runtime-manifest.integration.test.mjs': 4.412,
    'owner-directory.test.mjs': 51.057,
    'owner-setup.integration.test.mjs': 183.737,
    'participant-provision.integration.test.mjs': 60.076,
    'participant-role-capability.test.mjs': 4.813,
    'platform.test.mjs': 2.210,
    'recovery-activation.integration.test.mjs': 15.619,
    'rejection-diagnostic.test.mjs': 0.208,
    'role-runtime-negative.integration.test.mjs': 14.020,
    'role-runtime-options.test.mjs': 0.612,
    'setup-baseline-settlement.test.mjs': 0.208,
    'storage.test.mjs': 2.212,
    'test-session-activation.integration.test.mjs': 186.354,
    'test-session-alarm.integration.test.mjs': 130.904,
    'test-session-alarm.test.mjs': 0.209,
    'test-session-paid.integration.test.mjs': 357.084,
    'test-session-results.integration.test.mjs': 157.919,
    'test-session-tools.integration.test.mjs': 170.361,
    'test-session.integration.test.mjs': 265.218,
}
BUILD_CONTRACTS = {
    'native': ['launcher:custody', 'world:commit-barrier', 'contract:check', 'context-body:export'],
    'wasm-production': [*ROLES],
    'wasm-approval': ['owner,approval-harness', 'approval-admission:export',
                      'role-feature-negative-checks', 'worker-wasm-six-profiles'],
}


def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def regular(path, root):
    if ('..' in path.parts or not path.resolve().is_relative_to(root.resolve())
            or any(p.is_symlink() for p in (path, *path.parents))):
        raise ValueError(f'unsafe custody file: {path}')
    if not stat.S_ISREG(path.stat().st_mode):
        raise ValueError(f'not a regular custody file: {path}')
    return path


def load(path):
    if path.stat().st_size > 1024 * 1024:
        raise ValueError('oversized custody metadata')
    return json.loads(path.read_text())


def enabled(plan):
    version = plan.get('custody_execution_version')
    if version is None:
        return False
    if type(version) is not int or version != 1:
        raise ValueError('unknown custody execution version')
    if (plan.get('workspace_support_profile_version') != 1
            or 'workspace_support' not in plan.get('groups', [])
            or 'oasis7_custody_worker' not in plan.get('workspace_support_profiles', [])
            or plan.get('workspace_support_partition_version') != 1
            or 'custody' not in plan.get('workspace_support_partitions', [])
            or plan.get('custody_node_shards') != SHARDS
            or any(type(item) is not int for item in plan['custody_node_shards'])):
        raise ValueError('incomplete custody execution plan')
    return True


def inventory(root):
    rows = []
    for path in sorted((root / WORKER / 'tests').glob('*.test.mjs')):
        regular(path, root)
        rows.append(dict(path=path.name, size=path.stat().st_size, sha256=digest(path)))
    if not rows:
        raise ValueError('empty custody test inventory')
    return rows


def partition(rows):
    # Known long files use measured complete-case weights; new files use a
    # deterministic size estimate. Weight changes never remove tests.
    batches = [[] for _ in SHARDS]
    weights = [0 for _ in SHARDS]
    weight = lambda row: FILE_SECONDS.get(row['path'], max(1, row['size'] / 1000))
    for row in sorted(rows, key=lambda row: (-weight(row), row['path'])):
        index = min(SHARDS, key=lambda index: (weights[index], index))
        batches[index].append(row['path'])
        weights[index] += weight(row)
    return [sorted(batch) for batch in batches]


def identity(root, plan, run_id):
    if not enabled(plan) or not re.fullmatch(r'[1-9][0-9]*', str(run_id)):
        raise ValueError('custody execution requires a selected run')
    context = {key: plan.get(key) for key in ('source_sha', 'tested_sha', 'policy_sha')}
    if any(not isinstance(value, str) or not re.fullmatch(r'[0-9a-f]{40}', value)
           for value in context.values()):
        raise ValueError('invalid custody source identity')
    toolchain = tomllib.loads((root / 'rust-toolchain.toml').read_text())['toolchain']['channel']
    return dict(**context, run_id=str(run_id), platform=platform.system(), machine=platform.machine(),
                rust_toolchain=toolchain, worker_build='0.7.5', tests=inventory(root))


def positive_attempt(value):
    if type(value) is not int or value < 1:
        raise ValueError('invalid custody producer attempt')
    return value


def allowed_file(component, relative):
    if not isinstance(relative, str) or '\\' in relative or str(Path(relative)) != relative:
        return False
    parts = Path(relative).parts
    if not parts or Path(relative).is_absolute() or any(part in ('.', '..') for part in parts):
        return False
    if component == 'native':
        return relative in ('bin/native', 'bin/world', 'context-body-fixture.json', 'context-body-native-fixture.json')
    if component == 'wasm-approval':
        return relative == 'approval-fixture.json' or (len(parts) > 1 and parts[0] == 'approval-harness')
    return component == 'wasm-production' and len(parts) > 1 and parts[0] in ROLES and 'dry-run' not in parts


def minimum_files(component):
    if component == 'native':
        return {'bin/native', 'bin/world', 'context-body-fixture.json', 'context-body-native-fixture.json'}
    roles = ROLES if component == 'wasm-production' else ('approval-harness',)
    return {f'{role}/{name}' for role in roles for name in ('entry.mjs', 'index.js', 'index_bg.wasm')} | (
        {'approval-fixture.json'} if component == 'wasm-approval' else set())


def pack(root, plan, component, bundle, run_id, attempt):
    context = identity(root, plan, run_id)
    positive_attempt(attempt)
    artifacts = root / WORKER / 'artifacts'
    sources = {}
    if component == 'native':
        for name in ('native', 'world'):
            pointer = regular(artifacts / f'{name}-test-binary.txt', root)
            binary = Path(pointer.read_text().strip())
            regular(binary, root / 'target')
            if not os.access(binary, os.X_OK):
                raise ValueError('native fixture is not executable')
            sources['bin/' + name] = binary
        for name in ('context-body-fixture.json', 'context-body-native-fixture.json'):
            sources[name] = regular(artifacts / name, root)
    else:
        roles = ROLES if component == 'wasm-production' else ('approval-harness',)
        for role in roles:
            for path in sorted((artifacts / role).rglob('*')):
                if 'dry-run' in path.relative_to(artifacts).parts:
                    continue
                if path.is_symlink():
                    raise ValueError('symlink in custody build')
                if path.is_file():
                    sources[path.relative_to(artifacts).as_posix()] = regular(path, root)
        if component == 'wasm-approval':
            sources['approval-fixture.json'] = regular(artifacts / 'approval-fixture.json', root)
    if not minimum_files(component) <= sources.keys():
        raise ValueError('incomplete custody build outputs')
    bundle.mkdir(parents=True, exist_ok=False)
    rows = []
    for name, source in sorted(sources.items()):
        if not allowed_file(component, name):
            raise ValueError('unexpected custody build output')
        destination = bundle / 'files' / name
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(source, destination)
        mode = 0o755 if name.startswith('bin/') else 0o644
        destination.chmod(mode)
        rows.append(dict(path=name, size=destination.stat().st_size, sha256=digest(destination), mode=mode))
    manifest = dict(schema=SCHEMA, context=context, component=component,
                    producer_attempt=attempt, build_contract=BUILD_CONTRACTS[component], files=rows)
    (bundle / 'manifest.json').write_text(json.dumps(manifest, sort_keys=True) + '\n')
    return manifest


def check_bundle(root, plan, component, bundle, run_id, attempt):
    manifest = load(regular(bundle / 'manifest.json', bundle))
    if (manifest.get('schema') != SCHEMA or manifest.get('context') != identity(root, plan, run_id)
            or manifest.get('component') != component
            or manifest.get('build_contract') != BUILD_CONTRACTS[component]
            or type(manifest.get('producer_attempt')) is not int
            or manifest['producer_attempt'] != positive_attempt(attempt)):
        raise ValueError('custody build identity mismatch')
    rows = manifest.get('files')
    if not isinstance(rows, list) or not rows:
        raise ValueError('empty custody build manifest')
    names = set()
    for row in rows:
        if not isinstance(row, dict):
            raise ValueError('invalid custody file entry')
        name = row.get('path')
        if not allowed_file(component, name) or name in names:
            raise ValueError('unsafe or duplicate custody file')
        names.add(name)
        path = regular(bundle / 'files' / name, bundle)
        if (type(row.get('size')) is not int or path.stat().st_size != row['size']
                or digest(path) != row.get('sha256')
                or type(row.get('mode')) is not int
                or row['mode'] != (0o755 if name.startswith('bin/') else 0o644)):
            raise ValueError('custody file content or mode mismatch')
    for path in bundle.rglob('*'):
        if path.is_symlink():
            raise ValueError('symlink in custody bundle')
    actual = {path.relative_to(bundle).as_posix() for path in bundle.rglob('*') if path.is_file()}
    if actual != {'manifest.json', *('files/' + name for name in names)} or not minimum_files(component) <= names:
        raise ValueError('custody bundle inventory mismatch')
    return manifest


def restore(root, plan, bundles, run_id, attempts, current_attempt):
    if set(bundles) != set(COMPONENTS) or set(attempts) != set(COMPONENTS):
        raise ValueError('all three custody producers are required')
    positive_attempt(current_attempt)
    manifests = {}
    for component in COMPONENTS:
        if positive_attempt(attempts[component]) > current_attempt:
            raise ValueError('future custody producer attempt')
        manifests[component] = check_bundle(root, plan, component, bundles[component], run_id, attempts[component])
    # Validate every producer before installing any executable or module.
    artifacts = root / WORKER / 'artifacts'
    if artifacts.is_symlink() or (artifacts.exists() and any(artifacts.iterdir())):
        raise ValueError('restore requires empty custody artifacts')
    artifacts.mkdir(parents=True, exist_ok=True)
    for component, manifest in manifests.items():
        for row in manifest['files']:
            destination = artifacts / row['path']
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(bundles[component] / 'files' / row['path'], destination)
            destination.chmod(row['mode'])
    for name in ('native', 'world'):
        (artifacts / f'{name}-test-binary.txt').write_text(str(artifacts / 'bin' / name) + '\n')


def tap_summary(text, filename):
    if re.search(r'^# Subtest: (?:.*[/\\])?' + re.escape(filename) + r'$', text, re.M):
        raise ValueError('Node file exited without registering tests')
    values = {}
    for field in ('tests', 'pass', 'fail', 'cancelled', 'skipped', 'todo'):
        matches = re.findall(r'^# ' + field + r' ([0-9]+)$', text, re.M)
        if len(matches) != 1:
            raise ValueError('missing or ambiguous Node test summary')
        values[field] = int(matches[0])
    allowed_skip = filename == 'host-control-https.integration.test.mjs'
    if (values['tests'] < 1 or values['fail'] or values['cancelled'] or values['todo']
            or values['pass'] + values['skipped'] != values['tests']
            or values['skipped'] > (1 if allowed_skip else 0)
            or (not values['pass'] and not (allowed_skip and values['tests'] == values['skipped'] == 1))):
        raise ValueError('incomplete or failed Node tests')
    return values


def execute(root, plan, shard, results, run_id, attempt):
    context = identity(root, plan, run_id)
    positive_attempt(attempt)
    if type(shard) is not int or shard not in SHARDS:
        raise ValueError('invalid custody shard')
    files = partition(context['tests'])[shard]
    if not files:
        raise ValueError('empty custody shard')
    results.mkdir(parents=True, exist_ok=False)
    receipts = []
    for filename in files:
        log = results / (filename + '.log')
        with log.open('w') as stream:
            result = run(['node', '--test', '--test-concurrency=1', '--test-reporter=tap', 'tests/' + filename],
                         root / WORKER, os.environ.copy(), 'custody', filename, results / 'processes.jsonl',
                         timeout_seconds=2400, log_stream=stream)
        if not result['passed']:
            raise ValueError(f'custody Node file failed: {filename}')
        receipts.append(dict(file=filename, counts=tap_summary(log.read_text(), filename),
                             process=result))
    (results / 'result.json').write_text(json.dumps(dict(schema=SCHEMA, context=context, shard=shard,
        consumer_attempt=attempt, files=receipts), sort_keys=True) + '\n')


def validate_results(root, plan, paths, run_id, attempt):
    context = identity(root, plan, run_id)
    positive_attempt(attempt)
    batches = partition(context['tests'])
    seen = set()
    for path in paths:
        receipt = load(path)
        shard = receipt.get('shard')
        producer = receipt.get('consumer_attempt')
        if (receipt.get('schema') != SCHEMA or receipt.get('context') != context
                or type(shard) is not int or shard not in SHARDS or shard in seen
                or type(producer) is not int or not 1 <= producer <= attempt):
            raise ValueError('custody result identity mismatch')
        rows = receipt.get('files')
        if not isinstance(rows, list) or [row.get('file') for row in rows] != batches[shard]:
            raise ValueError('missing or duplicate custody test files')
        for row in rows:
            counts = row.get('counts', {})
            if any(type(value) is not int for value in counts.values()):
                raise ValueError('invalid Node counts')
            tap_summary('\n'.join(f'# {key} {value}' for key, value in counts.items()), row['file'])
            process = row.get('process', {})
            if (process.get('test_name') != row['file'] or process.get('passed') is not True
                    or process.get('exit_code') != 0 or process.get('timeout') is not False
                    or process.get('cancelled') is not False or process.get('leaked_processes') is not False
                    or process.get('cleanup_result') != 'clean' or process.get('log_complete') is not True):
                raise ValueError('custody process did not complete cleanly')
        seen.add(shard)
    if seen != set(SHARDS):
        raise ValueError('missing custody shard results')


def validate_jobs(plan, needs):
    if needs.get('select', {}).get('result') != 'success':
        raise ValueError('support selection failed')
    selected = 'workspace_support' in plan.get('groups', [])
    split = enabled(plan)
    expected = {'workspace-support-legacy': selected and not split,
                'workspace-support-other': split and 'other' in plan.get('workspace_support_partitions', []),
                **{f'custody-{component}': split for component in COMPONENTS},
                'custody-native-contracts': split, 'custody-node': split}
    if set(needs) != {'select', *expected}:
        raise ValueError('unknown or missing support dependencies')
    for job, required in expected.items():
        if needs.get(job, {}).get('result') != ('success' if required else 'skipped'):
            raise ValueError(f'{job}: missing, failed or unexpected support result')
    return split


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('action', choices=('pack', 'restore', 'run', 'validate'))
    parser.add_argument('--repo-root', type=Path, default=Path.cwd())
    parser.add_argument('--plan', type=Path, required=True)
    parser.add_argument('--run-id', default=os.environ.get('GITHUB_RUN_ID', ''))
    parser.add_argument('--attempt', type=int, default=int(os.environ.get('GITHUB_RUN_ATTEMPT', '0')))
    parser.add_argument('--component', choices=COMPONENTS)
    parser.add_argument('--bundle', type=Path)
    parser.add_argument('--bundles', type=Path)
    parser.add_argument('--producer-attempts', help='Exact successful producer outputs, keyed by component')
    parser.add_argument('--shard', type=int)
    parser.add_argument('--results', type=Path)
    parser.add_argument('--needs')
    args = parser.parse_args()
    root, plan = args.repo_root.resolve(), load(args.plan)
    actual = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, text=True).strip()
    if actual != plan.get('tested_sha'):
        raise ValueError('custody checkout is not the selected tested commit')
    if args.action == 'pack':
        if args.component is None or args.bundle is None:
            parser.error('pack needs --component and --bundle')
        pack(root, plan, args.component, args.bundle.resolve(), args.run_id, args.attempt)
    elif args.action == 'restore':
        if args.bundles is None or args.producer_attempts is None:
            parser.error('restore needs --bundles and --producer-attempts')
        restore(root, plan, {key: args.bundles.resolve() / key for key in COMPONENTS}, args.run_id,
                json.loads(args.producer_attempts), args.attempt)
    elif args.action == 'run':
        if args.results is None:
            parser.error('run needs --results')
        execute(root, plan, args.shard, args.results, args.run_id, args.attempt)
    else:
        if args.needs is None:
            parser.error('validate needs --needs')
        if validate_jobs(plan, json.loads(args.needs)):
            if args.results is None:
                parser.error('split validation needs --results')
            validate_results(root, plan, list(args.results.glob('*/result.json')), args.run_id, args.attempt)


if __name__ == '__main__':
    main()
