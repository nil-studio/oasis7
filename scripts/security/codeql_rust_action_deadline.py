#!/usr/bin/env python3
"""Own research or production Action main under its original absolute analysis clock."""
import ctypes
import hashlib
import json
import math
import os
from pathlib import Path
import re
import signal
import sys
import time

import codeql_rust_diagnostic_runtime as runtime
import codeql_rust_capacity_layout as layout
from codeql_rust_capacity_layout import ACTION_SHA, _read_regular

MODE = 'candidate-workload-research'
PRODUCTION_MODE = 'rust-default-production'
FIXED_SOURCE_MODE = 'rust-default-fixed-source-observation'
FIXED_SOURCE = '2bcc4edafdc615f395c9138a0b39ede366726d34'
FIXED_SOURCE_WORKFLOW = '.github/workflows/codeql-rust-capacity.yml'
SOURCE = '7c2f99a0ff2fbc3a3523dee7a1cc06fa5bf187ef'
MAIN_BLOB = '8b8462e5a06f394cd0504d57868488cebc7030c9'
OFFICIAL_CAP = 256 * 1024**2


def fixed_source_identity(source_sha):
    """Only this closed workflow may pair an old business tree with new main tools."""
    tool_sha = os.environ.get('GITHUB_SHA', '')
    workflow_ref = os.environ.get('GITHUB_REPOSITORY', '') + '/' + FIXED_SOURCE_WORKFLOW + '@refs/heads/main'
    if (source_sha != FIXED_SOURCE or not re.fullmatch(r'[0-9a-f]{40}', tool_sha)
            or os.environ.get('GITHUB_EVENT_NAME') != 'workflow_dispatch'
            or os.environ.get('GITHUB_REF') != 'refs/heads/main'
            or os.environ.get('GITHUB_WORKFLOW_SHA') != tool_sha
            or os.environ.get('GITHUB_WORKFLOW_REF') != workflow_ref
            or not os.environ.get('GITHUB_REPOSITORY') or os.environ.get('CAPACITY_MODE')
            or os.environ.get('COMPARE_QUALIFIER_PATCH') != 'false'
            or os.environ.get('OASIS7_CODEQL_PRODUCTION_MODE') != FIXED_SOURCE_MODE):
        raise RuntimeError('fixed-source observation requires its trusted main workflow and separate fixed business source')
    return dict(mode=FIXED_SOURCE_MODE, source_sha=source_sha, tool_sha=tool_sha,
                workflow_ref=workflow_ref, workflow_sha=tool_sha)


def fixed_source_resources():
    if (os.environ.get('CODEQL_THREADS') != '4' or os.environ.get('CODEQL_RAM') != '14570'):
        raise RuntimeError('fixed-source observation Action resources differ from 4 threads / 14570 MiB')


def fixed_source_provenance(source, source_sha):
    identity = fixed_source_identity(source_sha)
    runner = Path(os.environ['RUNNER_TEMP']).resolve()
    tools, out = runner / 'codeql-rust-fixed-source-helper', runner / 'codeql-rust-fixed-source-layout'
    paths, errors = layout._paths(source, tools)
    if Path(__file__).resolve().parent != tools:
        errors.append('FIXED_SOURCE_HELPER_NOT_FROM_EXTERNAL_SNAPSHOT')
    snapshot_errors, observed = layout._check_snapshot(tools, out, paths, production_default=True)
    errors.extend(snapshot_errors)
    checked = read_json(out / 'collection-layout.json')
    if (checked.get('status') != 'PASS' or checked.get('source_sha') != source_sha
            or checked.get('paths') != paths or checked.get('observed_tools') != observed
            or checked.get('identity', {}).get('actual_head') != source_sha):
        errors.append('FIXED_SOURCE_COLLECTION_LAYOUT_DIFFERS')
    if errors:
        raise RuntimeError('fixed-source observation tool/layout identity differs: ' + '; '.join(errors))
    return dict(identity, tools_root=str(tools),
                tool_snapshot_sha256=runtime.digest(out / 'tool-snapshot.json'),
                collection_layout_sha256=runtime.digest(out / 'collection-layout.json'))


def shared_budget(value, deadline, analysis_deadline, now, monotonic, *, mode=MODE):
    """Translate persisted absolute clocks; never allocate a new analysis window."""
    allocated = value.get('analysis_allocated_at')
    minutes = value.get('analysis_allocated_minutes')
    if (mode not in (MODE, PRODUCTION_MODE, FIXED_SOURCE_MODE)
            or any(type(v) not in (int, float) or not math.isfinite(v) for v in (deadline, analysis_deadline, allocated, now))
            or type(minutes) is not int or not 1 <= minutes <= 75
            or value.get('mode') != mode or value.get('job_budget_seconds') != 5400
            or value.get('fixed_job_deadline') != deadline or value.get('analysis_deadline') != analysis_deadline
            or value.get('post_analysis_reserve_seconds') != 600
            or value.get('helper_terminal_and_upload_reserve_seconds') != (120 if mode == MODE else 90)
            or not 0 < deadline - allocated <= 5400 or allocated > now
            or not allocated < analysis_deadline <= min(deadline - 600, allocated + minutes * 60)
            or deadline - now > 5400):
        raise RuntimeError('unbound or extended candidate analysis/job budget')
    return monotonic + analysis_deadline - now, monotonic + deadline - now


def read_json(path):
    return json.loads(_read_regular(path, limit=256 * 1024))


def process_identity(pid):
    value = Path('/proc') / str(pid) / 'stat'
    fields = value.read_text().rsplit(')', 1)[1].split()
    if fields[0] == 'Z': raise RuntimeError('outer Action process has exited')
    return dict(pid=pid, start_ticks=int(fields[19]))


class OuterAction:
    def __init__(self, pid):
        if pid != os.getppid(): raise RuntimeError('outer Action parent differs')
        self.identity = process_identity(pid)

    def poll(self, deadline):
        if os.getppid() != self.identity['pid'] or process_identity(self.identity['pid']) != self.identity:
            raise RuntimeError('outer Action cancelled or replaced')

    def summary(self):
        return dict(scope='outer Action PID and start ticks, not a hosted cancellation verdict', identity=self.identity)


def invocation(value, source):
    if (not isinstance(value, dict) or set(value) != {'node', 'exec_argv', 'argv', 'cwd', 'parent_pid'}
            or value['cwd'] != str(source) or type(value['parent_pid']) is not int
            or not isinstance(value['exec_argv'], list) or not isinstance(value['argv'], list) or not value['argv']
            or any(not isinstance(s, str) or '\0' in s for s in [value['node'], *value['exec_argv'], *value['argv']])):
        raise RuntimeError('invalid official Action invocation')
    parent = OuterAction(value['parent_pid'])
    if Path(value['node']).resolve() != (Path('/proc') / str(value['parent_pid']) / 'exe').resolve():
        raise RuntimeError('Node executable differs from the runner invocation')
    entry = Path(value['argv'][0])
    if (entry.resolve() != entry or entry.name != 'analyze-entry.js' or entry.parent.name != 'lib'
            or entry.parent.parent.name != ACTION_SHA
            or entry.parent.parent.parent.name != 'codeql-action' or entry.parent.parent.parent.parent.name != 'github'
            or not entry.is_file() or entry.stat().st_size > 4096):
        raise RuntimeError('unexpected fixed Action entry')
    raw = entry.read_bytes()
    if hashlib.sha1(b'blob ' + str(len(raw)).encode() + b'\0' + raw).hexdigest() != MAIN_BLOB:
        raise RuntimeError('official Action main bootstrap changed')
    return [value['node'], *value['exec_argv'], *value['argv']], parent


def _admit(out, source, deadline, mode, source_sha, budget_file):
    budget = read_json(out / budget_file)
    terminal = read_json(out / 'action-deadline-terminal.json')
    result = read_json(out / 'action-main.log.result.json')
    start = read_json(out / 'action-deadline-start.json')
    if mode == FIXED_SOURCE_MODE:
        identity = fixed_source_identity(source_sha)
        if (any(terminal.get(key) != value for key, value in identity.items() if key != 'source_sha')
                or any(start.get(key) != value for key, value in identity.items())
                or budget.get('tool_sha') != identity['tool_sha']):
            raise RuntimeError('fixed-source Action tool/source supervision differs')
    if (terminal.get('status') != 'ACTION_MAIN_COMPLETED_WITHIN_DEADLINE'
            or terminal.get('mode') != mode or terminal.get('source') != source_sha
            or terminal.get('source_root') != str(source) or terminal.get('fixed_job_deadline') != deadline
            or terminal.get('analysis_deadline') != budget.get('analysis_deadline')
            or terminal.get('budget_sha256') != runtime.digest(out / budget_file)
            or terminal.get('action_sha') != ACTION_SHA or terminal.get('main_blob') != MAIN_BLOB
            or terminal.get('process_receipt_sha256') != runtime.digest(out / 'action-main.log.result.json')
            or terminal.get('start_receipt_sha256') != runtime.digest(out / 'action-deadline-start.json')
            or terminal.get('completed_before_analysis_deadline') is not True
            or not isinstance(terminal.get('remaining_analysis_seconds'), (int, float))
            or not 0 <= terminal['remaining_analysis_seconds'] <= 4500
            or result.get('exit') != 0 or result.get('error') or result.get('failure_kind')
            or result.get('cleanup_errors') or result.get('evidence_errors')
            or start.get('pid') != result.get('pid') or result.get('pid') != result.get('pgid')
            or start.get('supervisor_pid') != terminal.get('supervisor_pid')
            or start.get('analysis_deadline') != budget.get('analysis_deadline')
            or start.get('budget_sha256') != terminal.get('budget_sha256')
            or start.get('source_root') != str(source) or start.get('action_sha') != ACTION_SHA):
        raise RuntimeError('absolute Action deadline supervision was not admitted')
    return dict(status='PASS', analysis_deadline=budget['analysis_deadline'], action_sha=ACTION_SHA,
                terminal_sha256=runtime.digest(out / 'action-deadline-terminal.json'), scope='official main only; post remains runner-owned')


def admit(out, source, deadline):
    return _admit(out, source, deadline, MODE, SOURCE, 'capacity-budget.json')


def admit_production(out, source, source_sha, analysis_deadline, *, mode=PRODUCTION_MODE):
    budget = read_json(out / 'production-budget.json')
    if (budget.get('source_sha') != source_sha or budget.get('analysis_deadline') != analysis_deadline
            or not isinstance(source_sha, str) or len(source_sha) != 40
            or any(c not in '0123456789abcdef' for c in source_sha)):
        raise RuntimeError('production Action budget source differs')
    deadline = budget.get('fixed_job_deadline')
    if mode not in (PRODUCTION_MODE, FIXED_SOURCE_MODE):
        raise RuntimeError('unexpected default query Action mode')
    shared_budget(budget, deadline, analysis_deadline, time.time(), time.monotonic(), mode=mode)
    return _admit(out, source, deadline, mode, source_sha, 'production-budget.json')


def main():
    # Only the frozen preload calls this entry, with no shell or arbitrary executable selection.
    runner = Path(os.environ['RUNNER_TEMP']).resolve()
    requested = os.environ.get('OASIS7_CODEQL_PRODUCTION_MODE', '')
    production = requested == PRODUCTION_MODE
    observation = requested == FIXED_SOURCE_MODE
    default_queries = production or observation
    mode = requested or MODE
    source_sha = os.environ.get('OASIS7_CODEQL_PRODUCTION_SOURCE', '') if default_queries else SOURCE
    budget_file = 'production-budget.json' if default_queries else 'capacity-budget.json'
    out = runner / ('codeql-rust-production-evidence' if default_queries else 'rust-capacity-evidence')
    source = Path(os.environ['GITHUB_WORKSPACE']).resolve()
    if (out.resolve() != out or not out.is_dir() or out.is_relative_to(source)
            or (out / 'hashes.json').exists() or (out / 'action-deadline-terminal.json').exists()):
        raise RuntimeError('Action supervision requires fresh, unsealed official evidence')
    runtime.CAP, runtime.WRITE_CAP = OFFICIAL_CAP, OFFICIAL_CAP - 1024**2
    terminal = dict(status='ACTION_MAIN_INCOMPLETE', mode=mode, source=source_sha, source_root=str(source),
                    action_sha=ACTION_SHA, main_blob=MAIN_BLOB, supervisor_pid=os.getpid(),
                    cleanup_seconds_within_analysis=runtime.CLEANUP_SECONDS, primary_failure=None,
                    stdout_stderr='inherited runner channels; not copied into artifact', post='RUNNER_OWNED_UNMODIFIED')
    previous_signals = {}
    analysis_end = None
    try:
        if requested not in ('', PRODUCTION_MODE, FIXED_SOURCE_MODE):
            raise RuntimeError('unknown default query Action mode')
        if observation:
            provenance = fixed_source_provenance(source, source_sha)
            fixed_source_resources()
            terminal.update(fixed_source_identity(source_sha))
            if read_json(out / 'fixed-source-provenance.json') != provenance:
                raise RuntimeError('fixed-source Action preparation provenance differs')
        elif production:
            if (os.environ.get('CAPACITY_MODE')
                    or os.environ.get('GITHUB_EVENT_NAME') not in ('workflow_dispatch', 'schedule')
                    or os.environ.get('GITHUB_REF') != 'refs/heads/main'
                    or os.environ.get('GITHUB_SHA') != source_sha or len(source_sha) != 40
                    or any(c not in '0123456789abcdef' for c in source_sha)):
                raise RuntimeError('production Action requires the current trusted main source')
        elif os.environ.get('CAPACITY_MODE') != MODE or os.environ.get('CAPACITY_SOURCE') != SOURCE:
            raise RuntimeError('Action supervision is exclusive to the fixed candidate workload')
        if (os.environ.get('INPUT_SKIP-QUERIES') != 'true' or os.environ.get('INPUT_UPLOAD') != 'never'
                or os.environ.get('INPUT_UPLOAD-DATABASE') != 'false'):
            raise RuntimeError('unexpected Action query/upload inputs')
        if len(sys.argv) != 2 or len(sys.argv[1].encode()) > 16 * 1024:
            raise RuntimeError('bounded Action invocation missing')
        argv, monitor = invocation(json.loads(sys.argv[1]), source)
        budget_raw = _read_regular(out / budget_file, limit=256 * 1024)
        budget = json.loads(budget_raw)
        if default_queries:
            deadline = budget.get('fixed_job_deadline')
            analysis_deadline = float(os.environ['RUST_ANALYSIS_DEADLINE'])
            prepared = read_json(out / 'production-prepare.json')
            if (budget.get('source_sha') != source_sha or prepared.get('status') != 'PREPARED'
                    or prepared.get('source_sha') != source_sha or prepared.get('source') != str(source)
                    or prepared.get('analysis_deadline') != analysis_deadline):
                raise RuntimeError('production Action preparation/budget binding differs')
            if observation and (budget.get('tool_sha') != provenance['tool_sha']
                    or prepared.get('mode') != mode or prepared.get('tool_sha') != provenance['tool_sha']):
                raise RuntimeError('fixed-source Action preparation/budget mode or tool differs')
            if production and prepared.get('mode', PRODUCTION_MODE) != PRODUCTION_MODE:
                raise RuntimeError('production Action cannot consume observation preparation')
        else:
            deadline = float(os.environ['CAPACITY_DEADLINE'])
            analysis_deadline = float(os.environ['CAPACITY_ANALYSIS_DEADLINE'])
        analysis_end, end = shared_budget(budget, deadline, analysis_deadline, time.time(), time.monotonic(), mode=mode)
        terminal.update(fixed_job_deadline=deadline, analysis_deadline=analysis_deadline,
                        budget_sha256=hashlib.sha256(budget_raw).hexdigest(), invocation=argv,
                        outer_action=monitor.identity)
        # This window covers TERM (1s) and KILL/reaping (2s), not an estimate of step startup latency.
        child_end = min(analysis_end, end - 120) - runtime.CLEANUP_SECONDS
        if time.monotonic() >= child_end:
            raise TimeoutError('original analysis window expired before Action main spawn')
        # Linux subreaping lets the existing owner reap killed grandchildren in its own PGID.
        if ctypes.CDLL(None, use_errno=True).prctl(36, 1, 0, 0, 0) != 0:
            raise OSError(ctypes.get_errno(), 'could not become owned-child subreaper')
        def cancelled(signum, frame): raise RuntimeError('Action supervisor received signal ' + str(signum))
        for sig in (signal.SIGINT, signal.SIGTERM):
            previous_signals[sig] = signal.signal(sig, cancelled)
        env = dict(os.environ, OASIS7_CODEQL_ACTION_SUPERVISOR=str(os.getpid()),
                   OASIS7_CODEQL_ACTION_BUDGET_SHA256=terminal['budget_sha256'])
        monitor.poll(child_end)
        runtime.save(out / 'action-deadline-terminal.json', terminal, out, terminal=True)
        result = runtime.run(argv, source, out / 'action-main.log', child_end,
                             max(0, child_end - time.monotonic()), out, env, file_limit=None,
                             observer=monitor, inherit_stdio=True, reap_orphans=True)
        monitor.poll(child_end)
        start = read_json(out / 'action-deadline-start.json')
        if (start.get('pid') != result['pid'] or start.get('supervisor_pid') != os.getpid()
                or start.get('budget_sha256') != terminal['budget_sha256']):
            raise RuntimeError('actual Action child start was not witnessed')
        terminal.update(process_receipt_sha256=runtime.digest(out / 'action-main.log.result.json'),
                        start_receipt_sha256=runtime.digest(out / 'action-deadline-start.json'))
        if time.monotonic() >= analysis_end:
            raise TimeoutError('Action main or owned cleanup exceeded original analysis deadline')
        terminal.update(status='ACTION_MAIN_COMPLETED_WITHIN_DEADLINE', completed_before_analysis_deadline=True)
    except BaseException as error:
        terminal.update(status='ACTION_MAIN_INCOMPLETE', primary_failure=repr(error)[:4096])
    finally:
        for sig, prior in previous_signals.items(): signal.signal(sig, prior)
        terminal['finished_at'] = time.time()
        if analysis_end is not None: terminal['remaining_analysis_seconds'] = analysis_end - time.monotonic()
        runtime.save(out / 'action-deadline-terminal.json', terminal, out, terminal=True)
    return 0 if terminal['status'] == 'ACTION_MAIN_COMPLETED_WITHIN_DEADLINE' else 1


if __name__ == '__main__': raise SystemExit(main())
