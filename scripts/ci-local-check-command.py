#!/usr/bin/env python3
"""Record one actual ci-tests.sh command; the parent owns deadlines and cleanup."""
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import time


def evidence(command, output, returncode, group):
    """Require supported execution evidence for commands that claim test cases.

    Baseline contains validators and fixture self-checks, not selected business
    test targets. Its effective checks remain distinct from business cases.
    """
    expected = executed = 0
    status = 'passed' if returncode == 0 else 'failed'
    reason = 'command completed' if returncode == 0 else f'command exited {returncode}'
    structured = []
    for line in output.splitlines():
        if line.startswith('{'):
            try:
                record = json.loads(line)
            except ValueError:
                continue
            if isinstance(record, dict) and 'expected_count' in record and 'count_ok' in record:
                structured.append(record)
    rust = re.findall(r'test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored;', output)
    python = re.findall(r'Ran (\d+) tests? in [\d.]+s', output)
    node = re.findall(r'^# tests (\d+)$', output, re.M)
    is_test = ('test' in command and any(Path(arg).name == 'cargo' for arg in command))
    # Shell validator fixtures expose a command-level contract, not a libtest
    # case inventory. Do not invent a case count from a script filename.
    is_test |= any(arg.endswith(('.test.py', '.test.mjs')) or '/test_' in arg for arg in command)
    is_test |= '-m' in command and 'unittest' in command
    is_test |= any(arg.endswith('ci-test-artifacts.py') for arg in command) and 'run' in command
    if structured:
        counts = [item.get('expected_count') for item in structured]
        if any(not isinstance(value, int) or value <= 0 for value in counts):
            status, reason = 'incomplete', 'unknown or zero expected artifact case count'
            expected = executed = None
        else:
            expected = sum(counts)
            executed = sum(item['expected_count'] for item in structured if item.get('passed') and item.get('count_ok'))
            if executed != expected:
                status, reason = 'incomplete', 'artifact execution did not prove every selected case'
    elif rust:
        executed = sum(int(passed) for passed, _, _ in rust)
        expected = sum(int(passed) + int(failed) + int(ignored) for passed, failed, ignored in rust)
        declared = sum(map(int, re.findall(r'^running (\d+) tests?$', output, re.M)))
        if declared != expected or any(int(failed) or int(ignored) for _, failed, ignored in rust):
            status, reason = 'incomplete', 'Rust execution count mismatch, failure or required skip'
    elif python:
        expected = sum(map(int, python))
        skipped = sum(map(int, re.findall(r'skipped=(\d+)', output)))
        executed = expected - skipped
        if skipped or len(re.findall(r'^OK(?: \(.*\))?$', output, re.M)) != len(python):
            status, reason = 'incomplete', 'Python test completion is missing or includes required skips'
    elif node:
        expected = sum(map(int, node))
        executed = sum(map(int, re.findall(r'^# pass (\d+)$', output, re.M)))
        if expected != executed:
            status, reason = 'incomplete', 'Node test count mismatch or required skip'
    elif is_test and group not in ('baseline', 'codex_agent_config_validation'):
        expected = executed = None
        status, reason = 'incomplete', 'test command has no supported case-count evidence'
    if is_test and group != 'baseline' and expected == 0:
        status, reason = 'incomplete', 'selected test command executed zero cases'
    if returncode:
        status, reason = 'failed', f'command exited {returncode}'
    return dict(status=status, reason=reason, tests_expected=expected, tests_executed=executed,
                business_tests_executed=0 if group == 'baseline' else executed)


def write_record(path, value):
    temporary = path.with_suffix('.tmp')
    temporary.write_text(json.dumps(value, ensure_ascii=True, sort_keys=True, indent=2) + '\n')
    os.replace(temporary, path)


def summaries(path):
    """Read count/completion lines without loading a heavy build log in memory."""
    selected = []
    with path.open('rb') as stream:
        while True:
            line = stream.readline(65537)
            if not line:
                break
            if not line.endswith(b'\n') and len(line) > 65536:
                while line and not line.endswith(b'\n'):
                    line = stream.readline(65537)
                continue
            if (b'test result:' in line or line.startswith((b'running ', b'Ran ', b'OK', b'# ', b'{'))):
                selected.append(line.decode('utf-8', 'replace'))
    return ''.join(selected)


def main(argv=None):
    argv = sys.argv[1:] if argv is None else argv
    if not argv or argv[0] != '--' or len(argv) == 1:
        raise SystemExit('internal command runner requires -- COMMAND')
    command = argv[1:]
    directory = Path(os.environ['OASIS7_LOCAL_CHECK_RUN_DIR']) / 'commands'
    directory.mkdir(exist_ok=True)
    fd, name = tempfile.mkstemp(prefix='command-', suffix='.log', dir=directory)
    log = Path(name)
    record_path = log.with_suffix('.json')
    group = os.environ['OASIS7_LOCAL_CHECK_GROUP']
    record = dict(argv=command, cwd=str(Path.cwd()), group=group, status='running',
                  started_at=datetime.now(timezone.utc).isoformat(), log=str(log),
                  exit_code=None, tests_expected=None, tests_executed=None)
    for arg in command:
        path = Path(arg)
        if path.is_file() and path.suffix in ('.sh', '.py', '.mjs'):
            record.setdefault('checker_sha256', {})[arg] = hashlib.sha256(path.read_bytes()).hexdigest()
    started = time.monotonic()
    try:
        write_record(record_path, record)
        # Preserve stdin (including driver heredocs). Do not create a detached
        # session: the outer supervisor owns this command and its descendants.
        with os.fdopen(fd, 'wb') as stream:
            process = subprocess.Popen(command, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
            while True:
                data = process.stdout.read1(65536)
                if not data:
                    break
                stream.write(data)
                stream.flush()
                sys.stdout.buffer.write(data)
                sys.stdout.buffer.flush()
            process.stdout.close()
            record['exit_code'] = process.wait()
        output = summaries(log)
        record.update(evidence(command, output, record['exit_code'], group))
        record.update(finished_at=datetime.now(timezone.utc).isoformat(),
                      duration_seconds=time.monotonic() - started)
        write_record(record_path, record)
        return 0 if record['status'] == 'passed' else 1
    except FileNotFoundError as error:
        record.update(status='blocked', reason=f'missing tool or command input: {error}',
                      finished_at=datetime.now(timezone.utc).isoformat(),
                      duration_seconds=time.monotonic() - started)
        try:
            write_record(record_path, record)
        except OSError:
            return 1
        print(record['reason'], file=sys.stderr)
        return 2
    except (OSError, ValueError) as error:
        # A report failure can never be converted into a successful command.
        print(f'local command/report failure: {error}', file=sys.stderr)
        return 1


if __name__ == '__main__':
    raise SystemExit(main())
