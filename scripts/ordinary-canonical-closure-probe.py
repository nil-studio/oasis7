#!/usr/bin/env python3
"""Read-only CI experiment for the original five canonical ordinary materials."""
import copy
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tarfile
import time

ROOT = Path('/home/runner/work/oasis7/oasis7')
MODULES = frozenset(('module.runtime.local-test-provider', 'm1.sensor.basic',
                     'm1.mobility.basic', 'm1.memory.core', 'm1.storage.cargo'))
RUN = subprocess.run


def state():
    return Path(os.environ['RUNNER_TEMP']) / 'ordinary-canonical-closure'


def emit(kind, **fields):
    record = dict(kind=kind, monotonic=round(time.monotonic(), 3), **fields)
    output = state() / 'receipt'
    output.mkdir(parents=True, exist_ok=True)
    with (output / 'events.jsonl').open('a') as stream:
        stream.write(json.dumps(record, sort_keys=True) + '\n')
    print(json.dumps(record, sort_keys=True), flush=True)


def module(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


def clean_environment():
    return {key: value for key, value in os.environ.items()
            if not key.startswith(('ACTIONS_', 'INPUT_'))
            and key not in ('GH_TOKEN', 'GITHUB_TOKEN', 'NODE_OPTIONS')}


def context():
    code, controller = (os.environ[name] for name in ('PROBE_CODE_SHA', 'PROBE_CONTROLLER_SHA'))
    if not all(re.fullmatch('[0-9a-f]{40}', value) for value in (code, controller)):
        raise ValueError('probe requires complete frozen source identities')
    actual = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip()
    if actual != code or Path.cwd() != ROOT:
        raise ValueError('probe must use the exact code checkout and canonical cwd')
    return dict(code_sha=code, controller_sha=controller, run_id=os.environ['GITHUB_RUN_ID'],
                run_attempt=os.environ['GITHUB_RUN_ATTEMPT'], image=os.environ['ORDINARY_BUILD_IMAGE'],
                diagnostic_only=True, full_required_passed=False, trusted_cache_writer_verified=False)


def docker_containers():
    value = subprocess.check_output(['docker', 'ps', '--quiet', '--no-trunc'],
                                   env=clean_environment(), text=True, timeout=15)
    identifiers = value.split()
    if len(identifiers) > 32 or any(not re.fullmatch('[0-9a-f]{64}', item) for item in identifiers):
        raise ValueError('unexpected daemon container inventory')
    return sorted(identifiers)


def material_files(bundle, provider, builtin):
    if set(builtin['modules']) != MODULES - {'module.runtime.local-test-provider'}:
        raise ValueError('canonical module inventory must contain exactly the original five')
    if set(provider['files']) != {'wasm', 'metadata', 'receipt', 'checker'}:
        raise ValueError('provider role inventory differs')
    declared = list(provider['files'].values()) + provider['libraries']
    declared += list(builtin['checker']['files'].values()) + builtin['checker']['libraries']
    modules = {'module.runtime.local-test-provider': {key: provider['files'][key]
               for key in ('wasm', 'metadata', 'receipt')}, **builtin['modules']}
    for files in builtin['modules'].values():
        if set(files) != {'wasm', 'metadata', 'receipt'}:
            raise ValueError('builtin material triplet differs')
        declared += list(files.values())
    expected = {entry['path']: entry['sha256'] for entry in declared}
    if len(expected) != len(declared):
        raise ValueError('duplicate material paths')
    actual = {str(path.relative_to(bundle)) for path in bundle.rglob('*')
              if path.is_file() or path.is_symlink()}
    if actual != set(expected):
        raise ValueError('material file inventory differs')
    for relative, digest in expected.items():
        path = bundle / relative
        if (path.is_symlink() or not path.is_file()
                or hashlib.sha256(path.read_bytes()).hexdigest() != digest):
            raise ValueError('material file bytes differ')
    return expected, modules


def worker():
    identity = context()
    environment = clean_environment()
    os.environ.clear()
    os.environ.update(environment)
    sys.path.insert(0, str(ROOT / 'scripts'))
    artifacts = module('actual_artifacts', ROOT / 'scripts/ci-test-artifacts.py')
    preflight = artifacts.ordinary_image_preflight()
    docker = artifacts.ordinary_docker_preflight()
    before_containers = docker_containers()
    emit('preflight', **identity, source=preflight, docker=docker,
         uid=os.getuid(), gid=os.getgid(), cpu_affinity=len(os.sched_getaffinity(0)))
    expected_source = artifacts.ProviderArtifact.identity(ROOT)
    if expected_source['tested_sha'] != identity['code_sha'] or expected_source['source_sha'] != identity['code_sha']:
        raise ValueError('canonical material source differs from frozen code')
    bundle = ROOT / 'target/ordinary-canonical-closure-probe'
    bundle.mkdir(parents=True, exist_ok=False)
    observed = []

    def observed_run(command, *arguments, **keywords):
        canonical = (isinstance(command, list) and len(command) > 2
                     and command[:3] == ['bash', str(ROOT / 'scripts/build-wasm-module.sh'), '--candidate-build'])
        if not canonical:
            return RUN(command, *arguments, **keywords)
        selected = command[command.index('--module-id') + 1]
        if selected not in MODULES or selected in observed:
            raise ValueError('unexpected or duplicate canonical build')
        started = time.monotonic()
        emit('canonical_build_started', module=selected)
        passed = False
        try:
            result = RUN(command, *arguments, **keywords)
            passed = result.returncode == 0
            return result
        finally:
            observed.append(selected)
            emit('canonical_build_finished', module=selected, passed=passed,
                 elapsed_seconds=round(time.monotonic() - started, 3))

    artifacts.subprocess.run = observed_run
    provider = artifacts.ProviderArtifact.build(ROOT, bundle, expected_source)
    builtin = artifacts.BuiltinTestArtifact.build(ROOT, bundle, expected_source)
    if set(observed) != MODULES or len(observed) != 5:
        raise ValueError('not all original canonical builds executed')
    artifacts.ProviderArtifact.validate(ROOT, bundle, provider, expected_source)
    artifacts.BuiltinTestArtifact.validate(ROOT, bundle, builtin, expected_source)
    files, modules = material_files(bundle, provider, builtin)

    # Rehashing a different module's real receipt must still reach and fail the
    # original compiled checker, rather than merely failing transport hashes.
    mutant = ROOT / 'target/ordinary-canonical-closure-cross-receipt'
    shutil.copytree(bundle, mutant)
    changed = copy.deepcopy(builtin)
    destination = changed['modules']['m1.sensor.basic']['receipt']
    source = changed['modules']['m1.memory.core']['receipt']
    shutil.copy2(mutant / source['path'], mutant / destination['path'])
    destination['sha256'] = hashlib.sha256((mutant / destination['path']).read_bytes()).hexdigest()
    try:
        artifacts.BuiltinTestArtifact.validate(ROOT, mutant, changed, expected_source)
    except subprocess.CalledProcessError:
        emit('cross_module_receipt_rejected', module='m1.sensor.basic')
    else:
        raise ValueError('compiled checker accepted another module receipt')
    artifacts.ProviderArtifact.validate(ROOT, bundle, provider, expected_source)
    artifacts.BuiltinTestArtifact.validate(ROOT, bundle, builtin, expected_source)
    if artifacts.ProviderArtifact.identity(ROOT) != expected_source:
        raise ValueError('source changed during canonical closure proof')
    if docker_containers() != before_containers:
        raise ValueError('daemon container inventory changed after synchronous builds')
    archive = state() / 'receipt/canonical-materials.tar.gz'
    with tarfile.open(archive, 'w:gz') as tar:
        for relative in sorted(files):
            tar.add(bundle / relative, arcname=relative, recursive=False)
    receipt = dict(identity, canonical_five_passed=True, modules=modules, files=files,
                   provider=provider, builtin=builtin, source=expected_source, docker=docker,
                   cross_module_receipt_rejected=True, daemon_container_inventory_unchanged=True,
                   material_archive_sha256=hashlib.sha256(archive.read_bytes()).hexdigest(),
                   material_archive_bytes=archive.stat().st_size)
    (state() / 'receipt/canonical-five.json').write_text(json.dumps(receipt, indent=2) + '\n')
    emit('canonical_five_verified', modules=sorted(modules), full_required_passed=False)


def supervise():
    identity = context()
    sys.path.insert(0, str(ROOT / 'scripts'))
    runner = module('actual_runner', ROOT / 'scripts/ci_process_runner.py')
    (state() / 'receipt').mkdir(parents=True, exist_ok=False)
    result = runner.run([sys.executable, str(Path(__file__)), 'worker'], ROOT,
                        clean_environment(), 'ordinary-canonical-diagnostic', 'original-five-materials',
                        state() / 'receipt/process.jsonl', timeout_seconds=1560)
    proof = state() / 'receipt/canonical-five.json'
    passed = result['passed'] and proof.is_file() and json.loads(proof.read_text()).get('canonical_five_passed') is True
    receipt = dict(identity, canonical_five_passed=passed, process=result,
                   process_cleanup_scope='job-container-owned processes; daemon inventory separately observed')
    (state() / 'receipt/receipt.json').write_text(json.dumps(receipt, indent=2) + '\n')
    emit('terminal', canonical_five_passed=passed, full_required_passed=False)
    return 0 if passed else 1


if __name__ == '__main__':
    if sys.argv[1:] == ['worker']:
        worker()
    elif sys.argv[1:] == ['supervise']:
        raise SystemExit(supervise())
    else:
        raise SystemExit('expected worker or supervise')
