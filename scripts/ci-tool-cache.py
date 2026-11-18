#!/usr/bin/env python3
"""Read-only save admission shared by bounded Linux tool/archive caches.

Callers own their exact key, payload validation and one trusted main writer.
This module checks that identity, measures the pinned cache action's upload
bound, and observes complete repository capacity. It never uploads or deletes.
"""
import contextlib
from functools import lru_cache
import importlib.util
from itertools import chain
import json
import os
from pathlib import Path
import re
import stat
import subprocess
import sys
import tarfile
import tempfile
import time

from ci_process_runner import run


class ToolCacheError(ValueError):
    def __init__(self, message, code=1):
        super().__init__(message)
        self.code = code


def command_environment(environment):
    return {key: value for key, value in environment.items()
            if key not in ('RUSTC_WRAPPER', 'GH_TOKEN', 'GITHUB_TOKEN', 'ACTIONS_RUNTIME_TOKEN')}


def cache_root(value, *, label='tool'):
    root = Path(value)
    if (not root.is_absolute() or '..' in root.parts
            or any(ord(char) < 32 for char in str(root))):
        raise ToolCacheError(f'{label} cache root must be a safe absolute path')
    if any(parent.is_symlink() for parent in (root, *root.parents)):
        raise ToolCacheError(f'{label} cache root must not use symlinks')
    return root


@lru_cache(maxsize=4)
def load_maintenance(repository_root):
    path = Path(repository_root) / 'scripts' / 'ci-cache-maintenance.py'
    spec = importlib.util.spec_from_file_location('tool_cache_maintenance', path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def checkout_sha(environment, repository_root):
    return subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=repository_root,
        env=command_environment(environment), text=True, stderr=subprocess.DEVNULL, timeout=10).strip()


def trusted_writer(environment, api, key, root, *, repository_root, expected_key,
                   writer_workflow, writer_job, label='tool', checkout_reader=None):
    repository_root = Path(repository_root).resolve()
    sha, event, run_id = (environment.get(name, '') for name in
                         ('GITHUB_SHA', 'GITHUB_EVENT_NAME', 'GITHUB_RUN_ID'))
    if callable(expected_key):
        expected_key = expected_key(environment, root)
    if (environment.get('GITHUB_ACTIONS') != 'true'
            or (environment.get('RUNNER_OS'), environment.get('RUNNER_ARCH')) != ('Linux', 'X64')
            or environment.get('GITHUB_REPOSITORY') != api.repository
            or environment.get('GITHUB_REF') != 'refs/heads/main'
            or environment.get('GITHUB_WORKFLOW_SHA') != sha
            or environment.get('GITHUB_WORKFLOW_REF') != f'{api.repository}/{writer_workflow}@refs/heads/main'
            or environment.get('GITHUB_JOB') != writer_job
            or event not in ('schedule', 'workflow_dispatch')
            or not re.fullmatch(r'[0-9a-f]{40}', sha)
            or not re.fullmatch(r'[1-9][0-9]*', run_id)
            or not Path(environment.get('GITHUB_WORKSPACE', '')).is_absolute()
            or Path(environment.get('GITHUB_WORKSPACE', '')).resolve() != repository_root
            or not isinstance(expected_key, str) or not expected_key or key != expected_key):
        raise ToolCacheError(f'{label} cache save requires the exact trusted main writer')
    if event == 'workflow_dispatch':
        path = Path(environment.get('GITHUB_EVENT_PATH', ''))
        if path.stat().st_size > 1024 * 1024:
            raise ToolCacheError(f'{label} manual event exceeds its bounded contract')
        payload = json.loads(path.read_text())
        if (not isinstance(payload, dict) or not isinstance(payload.get('inputs'), dict)
                or payload['inputs'].get('run_mode') != 'full'):
            raise ToolCacheError(f'{label} manual cache save requires run_mode=full')
    actual_sha = (checkout_reader(environment) if checkout_reader is not None
                  else checkout_sha(environment, repository_root))
    if actual_sha != sha:
        raise ToolCacheError(f'{label} checkout does not match the trusted main source')
    workflow_run = api.request('GET', f'/actions/runs/{run_id}')
    if (not isinstance(workflow_run, dict) or type(workflow_run.get('id')) is not int
            or workflow_run['id'] != int(run_id) or workflow_run.get('head_sha') != sha
            or workflow_run.get('head_branch') != 'main' or workflow_run.get('event') != event
            or workflow_run.get('path') != writer_workflow
            or not isinstance(workflow_run.get('repository'), dict)
            or workflow_run['repository'].get('full_name') != api.repository):
        raise ToolCacheError(f'REST run identity does not match the {label} writer')


def cache_snapshot(root, *, max_cache_bytes, max_entries, label='tool'):
    root = cache_root(root, label=label)
    if not root.is_dir():
        raise ToolCacheError(f'{label} cache payload must be one directory')
    if (type(max_cache_bytes) is not int or max_cache_bytes <= 0
            or type(max_entries) is not int or max_entries <= 0):
        raise ToolCacheError('tool cache payload budgets must be positive integers')
    rows, raw_bytes = [], 0
    for path in chain((root,), root.rglob('*')):
        value = path.lstat()
        relative = path.relative_to(root).as_posix()
        if (path.is_symlink() or not (stat.S_ISDIR(value.st_mode) or stat.S_ISREG(value.st_mode))
                or any(ord(char) < 32 for char in str(path)) or len(os.fsencode(path)) > 4096
                or path.name in ('cache.tzst', 'cache.tgz')):
            raise ToolCacheError(f'unsafe {label} cache payload')
        if stat.S_ISREG(value.st_mode):
            if value.st_nlink != 1:
                raise ToolCacheError(f'{label} cache payload must not use hardlinks')
            raw_bytes += value.st_size
        rows.append((relative, value.st_dev, value.st_ino, value.st_mode, value.st_size,
                     value.st_mtime_ns, value.st_ctime_ns))
        if len(rows) > max_entries or raw_bytes > max_cache_bytes:
            raise ToolCacheError(f'{label} cache exceeds the small-tool payload budget')
    return tuple(sorted(rows)), raw_bytes


def checked_process(command, cwd, evidence, name, timeout, *, environment, label):
    if timeout <= 0:
        raise ToolCacheError(f'{label} save admission deadline expired', 124)
    with contextlib.redirect_stdout(sys.stderr):
        result = run(command, cwd, command_environment(environment), 'ci-tools', name,
                     evidence / 'processes.jsonl', timeout_seconds=timeout)
    if result['passed']:
        return
    code = 124 if result['timeout'] else 130 if result['cancelled'] else result['exit_code']
    if type(code) is not int or not 1 <= code <= 255:
        code = 1
    raise ToolCacheError(f'{label} {name} failed: exit={result["exit_code"]}, '
                         f'timeout={result["timeout"]}, cancelled={result["cancelled"]}, '
                         f'cleanup={result["cleanup_result"]}', code)


def upload_bound(root, environment, deadline, *, repository_root, max_cache_bytes,
                 max_entries, label='tool', process_runner=None):
    """Measure the pinned Linux cache action's exact one-directory tar input.

    actions/cache@55cc8345 resolves the literal root with implicitDescendants
    disabled. Add a full tar record per entry for possible timestamp/PAX growth
    before save; the GNU compression bounds do not assume a compression ratio.
    """
    root = cache_root(root, label=label)
    repository_root = Path(repository_root).resolve()
    if (environment.get('TAR_OPTIONS') or environment.get('GZIP')
            or any(char in str(root) for char in '*?[]!{}')):
        raise ToolCacheError('custom archive options or glob paths cannot authorize a tool cache save')
    snapshot_options = dict(max_cache_bytes=max_cache_bytes, max_entries=max_entries, label=label)
    before, raw_bytes = cache_snapshot(root, **snapshot_options)
    for command, marker in ((['tar', '--version'], 'GNU tar'), (['gzip', '--version'], 'Free Software Foundation')):
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            raise ToolCacheError(f'{label} save admission deadline expired')
        output = subprocess.check_output(command, env=command_environment(environment),
                                         text=True, stderr=subprocess.DEVNULL, timeout=min(5, remaining))
        if marker not in output:
            raise ToolCacheError(f'{label} upload sizing requires GNU tar and GNU gzip')
    relative = os.path.relpath(root, repository_root)
    if relative.startswith('-'):
        raise ToolCacheError(f'{label} upload path is outside the fixed archive contract')
    with tempfile.TemporaryDirectory(prefix='tool-cache-size-') as temporary:
        evidence = Path(temporary)
        manifest = evidence / 'manifest.txt'
        manifest.write_text(relative + '\n')
        archive = evidence / 'cache.tar'
        command = ['tar', '--posix', '-cf', str(archive), '--exclude', 'cache.tzst',
                   '-P', '-C', str(repository_root), '--files-from', str(manifest)]
        timeout = min(30, deadline - time.monotonic())
        if process_runner is None:
            checked_process(command, evidence, evidence, 'cache-size', timeout,
                            environment=environment, label=label)
        else:
            process_runner(command, evidence, evidence, 'cache-size', timeout)
        size = archive.stat().st_size
    if cache_snapshot(root, **snapshot_options)[0] != before:
        raise ToolCacheError(f'{label} payload changed during upload sizing')
    bounded_tar = size + tarfile.RECORDSIZE * (len(before) + 1)
    zstd = bounded_tar + (bounded_tar >> 8) + max(0, ((128 << 10) - bounded_tar) >> 11)
    gzip_bound = bounded_tar + 5 * ((bounded_tar + 16_383) // 16_384 + 1) + 18
    return dict(raw_bytes=raw_bytes, payload_entries=len(before), measured_tar_bytes=size,
                upload_bound_bytes=max(zstd, gzip_bound)), before


def admit_save(root, key, environment, *, repository_root, expected_key, writer_workflow,
               writer_job, check_payload, max_cache_bytes=256 * 1024 ** 2, max_entries=128,
               api=None, label='tool', timeout_seconds=120, checkout_reader=None,
               process_runner=None):
    """Return a save decision; optional cache failures never authorize a save.

    expected_key may be a string or a caller-owned function(environment, root).
    check_payload(root, deadline=...) must verify the exact payload before size
    and capacity checks. Compatibility callbacks keep existing tool probes and
    process supervision at their original public entrypoints.
    """
    report = dict(save=False, mode=f'{label}-cache-admission', saved_confirmed=None,
                  reason='tool cache save not authorized')
    deadline = time.monotonic() + timeout_seconds
    try:
        root = cache_root(root, label=label)
        repository_root = Path(repository_root).resolve()
        maintenance = load_maintenance(repository_root)
        policy = maintenance.policy
        if api is None:
            if not environment.get('GH_TOKEN'):
                raise ToolCacheError(f'read-only Actions token missing for {label} cache admission')
            class ReadOnlyApi(maintenance.GitHubApi):
                def request(self, method, path):
                    if method != 'GET':
                        raise ToolCacheError(f'{label} cache admission is read-only')
                    return super().request(method, path)
            api = ReadOnlyApi(environment.get('GITHUB_REPOSITORY'), environment['GH_TOKEN'])
        if hasattr(api, 'deadline'):
            api.deadline = min(api.deadline, deadline)
        trusted_writer(environment, api, key, root, repository_root=repository_root,
                       expected_key=expected_key, writer_workflow=writer_workflow,
                       writer_job=writer_job, label=label, checkout_reader=checkout_reader)
        check_payload(root, deadline=deadline)
        snapshot_options = dict(max_cache_bytes=max_cache_bytes, max_entries=max_entries, label=label)
        measurement, before = upload_bound(root, environment, deadline,
            repository_root=repository_root, process_runner=process_runner, **snapshot_options)
        report.update(measurement)
        budget = policy.configured_budget(environment.get('CI_CACHE_REPOSITORY_BUDGET_BYTES'))
        if budget is None:
            raise ToolCacheError(f'verified {label} repository cache budget is missing')
        entries = policy.capacity_context(maintenance, api, budget, report)
        if cache_snapshot(root, **snapshot_options)[0] != before:
            raise ToolCacheError(f'{label} payload changed during capacity verification')
        if time.monotonic() >= deadline:
            raise ToolCacheError(f'{label} save admission deadline expired')
        projected = report['observed_repository_bytes'] + report['upload_bound_bytes']
        reserve = policy.DEPENDENCY_ROTATION_HEADROOM_BYTES
        report.update(projected_repository_bytes=projected, reserved_repository_bytes=reserve,
                      projected_repository_bytes_with_reserve=projected + reserve)
        if any(entry['key'] == key and entry['ref'] == 'refs/heads/main' for entry in entries):
            report['reason'] = 'existing immutable key'
        elif projected + reserve > report['effective_repository_budget_bytes']:
            report['reason'] = 'repository cache capacity or dependency rotation gap'
        else:
            report.update(save=True, reason='measured tool upload fits repository budget with rotation headroom')
    except (ValueError, OSError, subprocess.SubprocessError, KeyError, TypeError, AttributeError) as error:
        report['reason'] = str(error) if isinstance(error, ValueError) else 'tool save evidence could not be read'
    return report
