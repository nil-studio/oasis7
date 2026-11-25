#!/usr/bin/env python3
"""One frozen, diagnostic-only replay; never a replacement for required CI."""
import contextlib
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import shutil
import stat
import subprocess
import sys
import tarfile
import threading
import time
import urllib.request
import zipfile

SOURCE = '0b9df7ac95518e3f808599c529b7bb2cb19ef17b'
TESTED = 'b4efdb5f2d0a5438d37160dbd30a0a7c82cd4797'
ORIGIN = '45424957574'
POLICY = '07dad2bc506320ede7cc4c61b89b174fa111e1ca'
CASES = (
    'node_lifecycle::process::package_failed_start_tests::package_failed_candidate_start_rolls_back_and_restarts_exact_prior_image',
    'preview_world_init::tests::provider_profile::provider_preview_actual_module_commit_agent_read_and_immutable_rerun',
)
ARTIFACTS = (
    (14936669901, 1375385287, '11e1df9d94b3564d3dc15774571257c30f23fc8f18f3c94a89ddab291e56c976', 'native-tests.tar.gz'),
    (14934713502, 1094, '382ab6b64e3e208fda3dd22471293196e3d1bba96ad9e9e49381ee7b5e8538fd', 'selection.json'),
)
TEMP = Path(os.environ['RUNNER_TEMP']) / 'ordinary-phase-probe'
OUT = TEMP / 'receipt'
ROOT = Path('/home/runner/work/oasis7/oasis7')


def emit(kind, **values):
    record = dict(kind=kind, monotonic=round(time.monotonic(), 3), **values)
    OUT.mkdir(parents=True, exist_ok=True)
    with (OUT / 'events.jsonl').open('a') as stream:
        stream.write(json.dumps(record, sort_keys=True) + '\n')
    print(json.dumps(record, sort_keys=True), flush=True)


def module(name, file):
    spec = importlib.util.spec_from_file_location(name, file)
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


def clean_environment():
    return {key: value for key, value in os.environ.items()
            if not key.startswith(('ACTIONS_', 'INPUT_', 'OASIS7_PREVIEW_TEST_PROVIDER_'))
            and key not in ('GH_TOKEN', 'GITHUB_TOKEN', 'NODE_OPTIONS', 'LD_PRELOAD')}


def download():
    TEMP.mkdir(parents=True, exist_ok=False)
    token = os.environ['GH_TOKEN']
    for artifact_id, size, digest, name in ARTIFACTS:
        # Metadata is independent of the signed redirect and downloaded bytes.
        base = f'https://api.github.com/repos/nil-studio/oasis7/actions/artifacts/{artifact_id}'
        headers = {'Authorization': 'Bearer ' + token, 'Accept': 'application/vnd.github+json'}
        with urllib.request.urlopen(urllib.request.Request(base, headers=headers), timeout=20) as response:
            metadata = json.load(response)
        assert metadata['workflow_run']['id'] == int(ORIGIN)
        assert metadata['size_in_bytes'] == size and not metadata['expired']
        # urllib's redirect handler must not forward the bearer to blob storage.
        class Redirect(urllib.request.HTTPRedirectHandler):
            def redirect_request(self, req, fp, code, msg, response_headers, newurl):
                redirected = super().redirect_request(req, fp, code, msg, response_headers, newurl)
                if redirected:
                    redirected.remove_header('Authorization')
                return redirected
        opener = urllib.request.build_opener(Redirect())
        archive = TEMP / (name + '.zip')
        started, count, checksum = time.monotonic(), 0, hashlib.sha256()
        with opener.open(urllib.request.Request(base + '/zip', headers=headers), timeout=20) as response, archive.open('xb') as stream:
            while chunk := response.read(1024 * 1024):
                count += len(chunk)
                if count > size or time.monotonic() - started > 180:
                    raise ValueError('download bound exceeded')
                checksum.update(chunk)
                stream.write(chunk)
        assert count == size and checksum.hexdigest() == digest
        with zipfile.ZipFile(archive) as zipped:
            assert zipped.namelist() == [name]
            info = zipped.getinfo(name)
            assert info.file_size <= 3 * 1024**3
            with zipped.open(info) as source, (TEMP / name).open('xb') as target:
                shutil.copyfileobj(source, target, 1024 * 1024)
        archive.unlink()  # Only this probe's verified transport copy.
        emit('artifact_verified', artifact_id=artifact_id, zip_sha256=digest,
             size=count, seconds=round(time.monotonic() - started, 3), origin_run=ORIGIN)


def safe_markers(root, relatives):
    found = []
    for relative in relatives:
        current = root
        try:
            for part in Path(relative).parts:
                current /= part
                if stat.S_ISLNK(current.lstat().st_mode):
                    raise ValueError('symlink marker')
            value = current.stat()
            found.append(dict(marker=relative, bytes=value.st_size,
                              mtime_ns=value.st_mtime_ns))
        except (OSError, ValueError):
            continue
    return found


def observe(stop):
    # Only direct children of /tmp with these exact owned fixture prefixes;
    # no recursive directory enumeration or authority/environment contents.
    baseline = {entry.name for entry in Path('/tmp').iterdir()}
    for tick in range(115):
        if stop.wait(5):
            break
        fixtures = []
        for index, entry in enumerate(Path('/tmp').iterdir()):
            if index >= 512 or len(fixtures) >= 4:
                break
            if entry.name in baseline or not entry.name.startswith(('oasis7-node-test-', 'oasis7-preview-init-test-')):
                continue
            try:
                metadata = entry.lstat()
                if metadata.st_uid != os.getuid() or not stat.S_ISDIR(metadata.st_mode):
                    continue
                package = entry.name.startswith('oasis7-node-test-')
                names = ('package-test-runtime-image', 'candidate-runtime', 'existing-world', 'operation-request.json',
                         'package-operation-manager-progress.log', 'management-package-failed-start/package-failed-start-test-result.json') if package else (
                             'preview/initialization-receipt.json', 'preview/private/provider-authority.json',
                             'preview/private/agent-viewer.env', 'preview/records', 'preview/store')
                item = dict(role='package' if package else 'provider', markers=safe_markers(entry, names))
                if package:
                    # Fixed phase alphabet only; never upload manager stdout.
                    path = entry / 'package-operation-manager-progress.log'
                    try:
                        fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
                        with os.fdopen(fd, 'rb') as stream:
                            if stat.S_ISREG(os.fstat(stream.fileno()).st_mode):
                                lines = stream.read(8192).decode('ascii', 'ignore').splitlines()
                                phases = [line.split()[0] for line in lines if re.fullmatch(r'[a-z_0-9]+ elapsed_ms=[0-9]+', line)]
                                item['phase'] = phases[-1] if phases else None
                    except OSError:
                        pass
                fixtures.append(item)
            except OSError:
                continue
        processes, pending, seen = [], [os.getpid()], set()
        while pending and len(seen) < 32:
            pid = pending.pop(0)
            if pid in seen:
                continue
            seen.add(pid)
            try:
                directory = Path('/proc') / str(pid)
                if directory.stat().st_uid != os.getuid():
                    continue
                fields = (directory / 'stat').read_text().rsplit(') ', 1)[1].split()
                processes.append(dict(pid=pid, state=fields[0], cpu_ticks=int(fields[11]) + int(fields[12]),
                                      start_ticks=int(fields[19]), rss_pages=int(fields[21]),
                                      wchan=(directory / 'wchan').read_text()[:64]))
                pending += [int(value) for value in (directory / 'task' / str(pid) / 'children').read_text().split()]
            except (OSError, ValueError, IndexError):
                continue
        emit('sample', tick=tick, fixtures=fixtures, processes=processes)


def worker():
    assert Path.cwd().resolve() == ROOT and os.getuid() == 1001
    assert subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip() == TESTED
    assert not subprocess.check_output(['git', 'status', '--porcelain', '--untracked-files=no'], text=True).strip()
    env = clean_environment()
    env.update(SOURCE_SHA=SOURCE, GITHUB_RUN_ID=ORIGIN, RUST_TOOLCHAIN='1.96.0')
    os.environ.clear()
    os.environ.update(env)
    sys.path.insert(0, str(ROOT / 'scripts'))
    runner = module('frozen_runner', ROOT / 'scripts/ci_process_runner.py')
    artifact = module('frozen_artifact', ROOT / 'scripts/ci-test-artifacts.py')
    bundle = TEMP / 'bundle'
    bundle.mkdir()
    emit('phase', phase='unpack')
    with tarfile.open(TEMP / 'native-tests.tar.gz', 'r:gz') as archive:
        members = archive.getmembers()
        assert len(members) <= 4096 and sum(item.size for item in members) <= 20 * 1024**3
        archive.extractall(bundle, members=members, filter='data')
    plan = json.loads((TEMP / 'selection.json').read_bytes())
    assert plan['source_sha'] == SOURCE and plan['tested_sha'] == TESTED
    manifest = json.loads((bundle / 'manifest.json').read_bytes())
    assert str(manifest['producer_attempt']) == '1'
    artifact.validate_artifact_manifest(manifest, suite='ordinary', plan=plan)
    emit('phase', phase='ldd')
    for entry in manifest['binaries'].values():
        binary = artifact.checked_file(bundle, entry['path'], entry['sha256'])
        result = subprocess.run(['ldd', str(binary)], env=dict(env, LD_LIBRARY_PATH=str(bundle / 'lib')),
                                stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=10)
        assert result.returncode == 0 and b'not found' not in result.stdout
    emit('phase', phase='strict_load_inventory')
    manifest, native_env = artifact.load(bundle, 'ordinary', plan=plan)
    tests = manifest['binaries']['runtime-tests']['tests']
    assert all(case in tests for case in CASES)
    emit('phase', phase='execute_two_exact_cases', cases=CASES, cpu_affinity=len(os.sched_getaffinity(0)),
         explicit_test_threads=native_env.get('RUST_TEST_THREADS'), diagnostic_only=True)
    result = runner.run([str(bundle / manifest['binaries']['runtime-tests']['path']), '--exact', *CASES],
                        ROOT, native_env, 'ordinary-diagnostic', 'two-original-full-cases',
                        OUT / 'cases.jsonl', expected_count=2, timeout_seconds=480)
    emit('phase', phase='case_cleanup_complete', result=result)
    return 0 if result['passed'] else 1


def supervise():
    assert os.environ['ImageOS'] == 'ubuntu24' and os.getuid() == 1001
    assert 'VERSION_ID="24.04"' in Path('/etc/os-release').read_text()
    identity = dict(controller_run=os.environ['GITHUB_RUN_ID'], controller_head=os.environ['CONTROLLER_SHA'],
                    origin_run=ORIGIN, origin_attempt=1, source_sha=SOURCE, tested_sha=TESTED,
                    policy_sha=POLICY, diagnostic_only=True, full_required_passed=False)
    emit('identity', **identity)
    sys.path.insert(0, str(ROOT / 'scripts'))
    runner = module('outer_runner', ROOT / 'scripts/ci_process_runner.py')
    stop = threading.Event()
    observer = threading.Thread(target=observe, args=(stop,), daemon=True)
    observer.start()
    try:
        # Raw harness output stays in an unuploaded local file; original runner
        # still parses every byte and retains its owned-process cleanup oracle.
        with (TEMP / 'private-native-output.log').open('w') as output, contextlib.redirect_stdout(output):
            result = runner.run([sys.executable, str(Path(__file__)), 'worker'], ROOT, clean_environment(),
                                'diagnostic', 'frozen-loader-and-two-cases', OUT / 'supervisor.jsonl',
                                timeout_seconds=600)
    finally:
        stop.set()
        observer.join(timeout=6)
    passed = bool(result['passed'] and (OUT / 'cases.jsonl').is_file()
                  and json.loads((OUT / 'cases.jsonl').read_text())['passed'])
    receipt = dict(identity, isolated_cases_passed=passed, supervisor=result)
    (OUT / 'receipt.json').write_text(json.dumps(receipt, indent=2) + '\n')
    emit('terminal', isolated_cases_passed=passed, full_required_passed=False)
    return 0 if passed else 1


if __name__ == '__main__':
    try:
        if sys.argv[1] == 'download':
            download()
        else:
            raise SystemExit(worker() if sys.argv[1] == 'worker' else supervise())
    except Exception as error:
        # Exception values may contain private native stdout: publish type only.
        emit('failure', error_type=type(error).__name__)
        raise SystemExit(1)
