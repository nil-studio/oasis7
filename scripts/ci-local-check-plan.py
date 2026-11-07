#!/usr/bin/env python3
"""Plan affected checks for a clean commit; explicit dry-run only, never execution."""
import argparse
from datetime import datetime, timezone
import hashlib
import json
import math
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import time


class Blocked(Exception):
    status = 'blocked'
    code = 2


class Failed(Blocked):
    status = 'failed'
    code = 1


class NoChecks(Blocked):
    status = 'no_checks'
    code = 3


class Parser(argparse.ArgumentParser):
    def error(self, message):
        raise Blocked(message)


def git_environment():
    env = os.environ.copy()
    for key in ('GIT_DIR', 'GIT_WORK_TREE', 'GIT_INDEX_FILE', 'GIT_EXTERNAL_DIFF'):
        env.pop(key, None)
    env['GIT_OPTIONAL_LOCKS'] = '0'
    return env


def git(root, *args):
    try:
        return subprocess.check_output(['git', '-C', str(root), *args],
                                       stderr=subprocess.PIPE, env=git_environment(), timeout=30)
    except (OSError, subprocess.SubprocessError) as exc:
        raise Blocked(f'Git command failed: {args[0]}: {exc}') from exc


def commit(root, ref):
    if not ref or ref.startswith('-'):
        raise Blocked('invalid ref')
    return git(root, 'rev-parse', '--verify', '--end-of-options', ref + '^{commit}').decode('ascii').strip()


def state(root):
    head = commit(root, 'HEAD')
    flags = git(root, 'ls-files', '-v', '-z').split(b'\0')
    if flags.pop() != b'' or any(not record or record[:1] == b'S' or record[:1].islower() for record in flags):
        raise Blocked('assume-unchanged or skip-worktree index entries are unsupported')
    if git(root, 'ls-files', '--unmerged', '-z'):
        raise Blocked('unmerged index entries')
    status = git(root, '-c', 'core.filemode=true', '-c', 'core.fsmonitor=false',
                 '-c', 'core.untrackedCache=false', 'status', '--porcelain=v1', '-z',
                 '--untracked-files=all', '--ignore-submodules=none')
    if status:
        raise Blocked('dirty or untracked inputs are not supported by this planning slice')
    index = os.fsdecode(git(root, 'rev-parse', '--git-path', 'index').rstrip(b'\n'))
    index_path = Path(index)
    if not index_path.is_absolute():
        index_path = root / index_path
    try:
        digest = hashlib.sha256(index_path.read_bytes()).hexdigest()
    except OSError as exc:
        raise Blocked(f'cannot read index: {exc}') from exc
    return head, digest


def report_directory(root, requested):
    if requested is None:
        return Path(tempfile.mkdtemp(prefix='oasis7-local-plan-report-'))
    parent = Path(requested).resolve()
    # Reports may not turn a clean source tree dirty or overwrite existing output.
    # Each invocation owns only its unique child directory.
    if parent == root or root in parent.parents:
        relative = parent.relative_to(root).as_posix() + '/'
        try:
            git(root, 'check-ignore', '-q', '--', relative)
        except Blocked as exc:
            raise Blocked('--output-dir inside the worktree must be ignored') from exc
    try:
        parent.mkdir(parents=True, exist_ok=True)
        return Path(tempfile.mkdtemp(prefix='local-plan-', dir=parent))
    except OSError as exc:
        raise Failed(f'cannot create report directory: {exc}') from exc


def stop_planner(process):
    """The planner and its Git children inherit this invocation's new group."""
    if os.name == 'posix':
        try:
            os.killpg(process.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
    elif process.poll() is None:
        subprocess.run(['taskkill', '/PID', str(process.pid), '/T', '/F'],
                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=10, check=True)
    process.wait(timeout=10)


def run_planner(argv, root, timeout, result):
    details = result['planner']
    details.update(argv=argv, cwd=str(root), timeout_seconds=timeout, returncode=None,
                   environment={'GIT_OPTIONAL_LOCKS': '0'}, started_at=now())
    started = time.monotonic()
    options = {'start_new_session': True} if os.name == 'posix' else {
        'creationflags': subprocess.CREATE_NEW_PROCESS_GROUP}
    try:
        with open(result['logs']['stdout'], 'xb') as stdout, open(result['logs']['stderr'], 'xb') as stderr:
            try:
                process = subprocess.Popen(argv, cwd=root, env=git_environment(),
                                           stdout=stdout, stderr=stderr, **options)
            except OSError as exc:
                raise Blocked(f'cannot start trusted planner: {exc}') from exc
            try:
                details['returncode'] = process.wait(timeout=timeout)
            except (KeyboardInterrupt, subprocess.TimeoutExpired) as exc:
                try:
                    stop_planner(process)
                    details['cleanup'] = 'completed'
                except (OSError, subprocess.SubprocessError) as cleanup_error:
                    details['cleanup'] = 'failed'
                    raise Failed(f'cannot clean up owned planner processes: {cleanup_error}') from cleanup_error
                details['returncode'] = process.returncode
                if isinstance(exc, KeyboardInterrupt):
                    raise
                raise Failed(f'trusted planner timeout after {timeout:g} seconds') from exc
        if details['returncode']:
            raise Failed(f'trusted planner failed ({details["returncode"]}); see stderr log')
        try:
            return json.loads(Path(result['logs']['stdout']).read_bytes())
        except (ValueError, UnicodeDecodeError) as exc:
            raise Failed('trusted planner returned invalid JSON') from exc
    finally:
        details.update(finished_at=now(), duration_seconds=time.monotonic() - started)


def plan(root, base_ref, timeout, result):
    before = state(root)
    result.update(source_sha=before[0], input={'scope': 'committed-only', 'dirty': False,
                  'untracked': False, 'index_sha256': before[1]}, scope='committed-only')
    # The local origin/main ref is the trust anchor; never execute candidate policy.
    remote = commit(root, 'refs/remotes/origin/main')
    result['trust_anchor_sha'] = remote
    if base_ref is None:
        bases = git(root, 'merge-base', '--all', before[0], remote).decode('ascii').splitlines()
        if len(bases) != 1:
            raise Blocked('HEAD and origin/main must have a unique merge base')
        base = bases[0]
        base_source = 'merge-base(origin/main,HEAD)'
    else:
        base = commit(root, base_ref)
        base_source = base_ref
    try:
        git(root, 'merge-base', '--is-ancestor', base, remote)
    except Blocked as exc:
        raise Blocked('base is not an ancestor of local origin/main; refusing untrusted policy') from exc
    result.update(base_sha=base, base_source=base_source, policy_sha=base)
    try:
        with tempfile.TemporaryDirectory(prefix='oasis7-local-plan-') as directory:
            temp = Path(directory)
            blobs = {}
            for name in ('plan-rust-required-scope.py', 'ci-required-scope.json', 'ci-formal-process-suites.json'):
                blobs[name] = git(root, 'show', f'{base}:scripts/{name}')
                (temp / name).write_bytes(blobs[name])
            try:
                config = json.loads(blobs['ci-required-scope.json'])
            except (ValueError, UnicodeDecodeError) as exc:
                raise Blocked('trusted configuration is not valid JSON') from exc
            if not isinstance(config, dict):
                raise Blocked('trusted configuration must be an object')
            known_groups = config.get('groups')
            if not isinstance(known_groups, list) or any(not isinstance(group, str) for group in known_groups):
                raise Blocked('trusted configuration has no valid group inventory')
            result['planner'].update(
                sha256=hashlib.sha256(blobs['plan-rust-required-scope.py']).hexdigest(),
                config_sha256={name: hashlib.sha256(data).hexdigest() for name, data in blobs.items() if name.endswith('.json')})
            argv = [sys.executable, str(temp / 'plan-rust-required-scope.py'),
                    '--base-ref', base, '--head-ref', before[0], '--policy-ref', base,
                    '--profile', 'pr-fast', '--config', str(temp / 'ci-required-scope.json'),
                    '--formal-config', str(temp / 'ci-formal-process-suites.json')]
            planned = run_planner(argv, root, timeout, result)
            if not isinstance(planned, dict):
                raise Failed('trusted planner returned a non-object plan')
            groups, reasons = planned.get('groups'), planned.get('reasons')
            if (not isinstance(groups, list) or any(not isinstance(group, str) or group not in known_groups for group in groups)
                    or len(groups) != len(set(groups)) or not isinstance(reasons, list)
                    or any(not isinstance(reason, str) for reason in reasons)):
                raise Failed('trusted planner returned invalid groups or reasons')
            result['plan'] = planned
            result['groups'] = [dict(name=group, status='not_run', reason='dry_run') for group in groups]
            result['not_selected_groups'] = [dict(name=group, reason='not selected by trusted planner')
                                             for group in known_groups if group not in groups]
            if not groups:
                raise NoChecks('trusted planner selected no checks')
            result.update(status='planned', exit_code=0, reason='explicit dry-run; no checks executed')
    finally:
        # Check after success, failure, cancellation and timeout, preserving any plan.
        try:
            if state(root) != before or commit(root, 'refs/remotes/origin/main') != remote:
                raise Blocked('HEAD, index or trust anchor changed while planning')
        except (Blocked, OSError, UnicodeError) as exc:
            result['stale_reason'] = str(exc)


def now():
    return datetime.now(timezone.utc).isoformat()


def requested_option(argv, flag, default=None):
    value = default
    for index, item in enumerate(argv):
        if item.startswith(flag + '='):
            value = item.split('=', 1)[1]
        elif item == flag and index + 1 < len(argv):
            value = argv[index + 1]
    return value


def main(argv=None):
    argv = sys.argv[1:] if argv is None else argv
    output_format = requested_option(argv, '--format', 'text')
    started = time.monotonic()
    result = {'schema_version': 1, 'status': 'blocked', 'exit_code': 2, 'validated': False,
              'profile': requested_option(argv, '--profile'), 'tier': None, 'reason': '',
              'source_sha': None, 'base_sha': None, 'base_source': requested_option(argv, '--base-ref'),
              'policy_sha': None, 'trust_anchor_sha': None, 'input': {'status': 'not_inspected'},
              'planner': {'status': 'not_started'}, 'driver': {'status': 'not_used', 'reason': 'planning only'},
              'checks_expected': 0, 'checks_executed': 0, 'tests_expected': 0, 'tests_executed': 0,
              'groups': [], 'not_selected_groups': [], 'logs': {}, 'started_at': now(),
              'limitations': ['No checks or tests are executed by this preparation entry.',
                              'Only clean committed input is supported; quick/formal and execution are not implemented.',
                              'Ignored/external runtime inputs and other platforms are not validated.',
                              'Policy trust assumes the local origin/main ref is trustworthy.'],
              'allowed_writes': ['owned temporary policy files', 'owned report directory; existing reports are retained']}
    try:
        parser = Parser(description=__doc__)
        parser.add_argument('--profile', choices=('affected',), required=True)
        parser.add_argument('--dry-run', action='store_true', required=True)
        parser.add_argument('--base-ref', help='trusted ancestor of local origin/main; defaults to unique merge-base')
        parser.add_argument('--format', choices=('text', 'json'), default='text')
        parser.add_argument('--output-dir', help='report parent outside the worktree or under an ignored directory')
        parser.add_argument('--timeout-seconds', type=float, default=120, help='planner deadline (default: 120)')
        args = parser.parse_args(argv)
        if not math.isfinite(args.timeout_seconds) or args.timeout_seconds <= 0:
            raise Blocked('--timeout-seconds must be positive and finite')
        root = Path(os.fsdecode(git(Path.cwd(), 'rev-parse', '--show-toplevel').rstrip(b'\n'))).resolve()
        directory = report_directory(root, args.output_dir)
        result['logs'] = dict(directory=str(directory), stdout=str(directory / 'planner.stdout.json'),
                              stderr=str(directory / 'planner.stderr.log'), report=str(directory / 'result.json'))
        plan(root, args.base_ref, args.timeout_seconds, result)
    except Blocked as exc:
        result.update(status=exc.status, exit_code=exc.code, reason=str(exc))
    except KeyboardInterrupt:
        result.update(status='cancelled', exit_code=130, reason='interrupted; no checks executed')
    except (OSError, UnicodeError, ValueError) as exc:
        result.update(status='failed', exit_code=1, reason=str(exc))
    if result.get('stale_reason') and result['status'] != 'cancelled':
        result.update(status='stale_input', exit_code=2, reason=result['stale_reason'])
    result.update(finished_at=now(), duration_seconds=time.monotonic() - started)
    if result['planner'].get('returncode') is not None:
        result['planner']['status'] = 'completed' if result['planner']['returncode'] == 0 else 'failed'
    report = result['logs'].get('report')
    if report:
        temporary = Path(report + '.tmp')
        try:
            temporary.write_text(json.dumps(result, ensure_ascii=True, sort_keys=True, indent=2) + '\n')
            os.replace(temporary, report)
        except OSError as exc:
            result.update(status='failed', exit_code=1, reason=f'cannot write report: {exc}')
            result['logs']['report'] = None
            try:
                temporary.unlink(missing_ok=True)
            except OSError:
                pass
    if output_format == 'json':
        print(json.dumps(result, ensure_ascii=True, sort_keys=True))
    else:
        print(f"{result['status']}, not validated: {result['reason']}")
        print('Checks executed: 0; tests executed: 0')
        print('Full result:')
        print(json.dumps(result, ensure_ascii=True, sort_keys=True, indent=2))
    if result['exit_code']:
        print(f"{result['status']}: {result['reason']}", file=sys.stderr)
    return result['exit_code']


if __name__ == '__main__':
    raise SystemExit(main())
