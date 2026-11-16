"""Bounded GDB evidence for the single-runtime client-abuse test (#5900)."""
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shutil
import stat

TEST_FILE = 'client-abuse.integration.test.mjs'
SCHEMA = 'oasis7.custody-workerd-gdb/v1'
ENGINE_SHA256 = 'd7ce7f7de38dba233a2924b777b51c65a407e524154d9b66fa8d2bc0a1ff5dcf'
ENGINE_VERSION = '1.20261019.1'
KNOWN_FAULT_OFFSET = '0x573f313'
MAX_RECORD_BYTES = 1024 * 1024
MAX_RECORDS = 256
REGISTERS = {'pc', 'r13', 'rbp', 'rsp'}
MAPPINGS = {'pc', 'r13', 'r13_slot', 'rbp', 'rsp'}


def read_json(path):
    return json.loads(read_regular(path))


def read_regular(path):
    flags = os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK
    descriptor = os.open(path, flags)
    try:
        info = os.fstat(descriptor)
        if not stat.S_ISREG(info.st_mode) or info.st_size > MAX_RECORD_BYTES:
            raise ValueError('invalid workerd capture file')
        with os.fdopen(descriptor, 'rb', closefd=False) as stream:
            data = stream.read(MAX_RECORD_BYTES + 1)
        if len(data) > MAX_RECORD_BYTES:
            raise ValueError('oversized workerd capture file')
        return data
    finally:
        os.close(descriptor)


def debug_command(command, config):
    # GDB follows the original Node process tree. Miniflare still owns the real
    # workerd PID, binary stdin and FD 3, including its uncatchable SIGKILL dispose.
    return [config['gdb'], '--quiet', '--batch', '--nx', '--nh',
            '-iex', 'set auto-load python-scripts off',
            '-iex', 'set auto-load guile-scripts off',
            '-iex', 'set auto-load gdb-scripts off',
            '-iex', 'set auto-load local-gdbinit off',
            '-iex', 'set debuginfod enabled off',
            '--command=' + config['driver'], '--args', *command]


def prepare(root, results, environment, context, attempt, command):
    if platform.system() != 'Linux' or platform.machine() != 'x86_64':
        raise ValueError('workerd GDB capture requires Linux x86_64')
    if any(environment.get(key) for key in ('MINIFLARE_WORKERD_PATH', 'LD_PRELOAD')):
        raise ValueError('workerd capture cannot combine runtime overrides')
    executable = (root / 'crates/oasis7_custody_worker/node_modules'
                  / '@cloudflare/workerd-linux-64/bin/workerd').resolve()
    with executable.open('rb') as stream:
        actual = hashlib.file_digest(stream, 'sha256').hexdigest()
    if actual != ENGINE_SHA256:
        raise ValueError('workerd capture needs an explicitly reviewed engine identity')
    debugger = shutil.which('gdb', path=environment.get('PATH'))
    if debugger is None:
        raise ValueError('workerd capture requires the standard gdb package')
    directory = results.resolve() / 'workerd-fault'
    directory.mkdir(mode=0o700, exist_ok=False)
    config = dict(schema=SCHEMA, context=context, attempt=attempt,
                  engine_sha256=actual, engine_version=ENGINE_VERSION,
                  executable=str(executable), gdb=debugger,
                  driver=str(root / 'scripts/ci-custody-workerd-gdb.py'),
                  records=str(directory / 'records.jsonl'))
    config_path = directory / 'config.json'
    config_path.write_text(json.dumps(config, sort_keys=True) + '\n')
    config_path.chmod(0o600)
    child_environment = dict(environment, OASIS7_WORKERD_GDB_CONFIG=str(config_path),
                             DEBUGINFOD_URLS='')
    return directory, debug_command(command, config), child_environment


def address(value):
    return isinstance(value, str) and re.fullmatch(r'0x[0-9a-f]{1,16}', value) is not None


def summarize(data, context, attempt):
    if not data or not data.endswith(b'\n') or len(data) > MAX_RECORD_BYTES:
        raise ValueError('missing or incomplete workerd capture')
    lines = data.splitlines()
    if len(lines) > MAX_RECORDS:
        raise ValueError('too many workerd capture records')
    rows = [json.loads(line) for line in lines]
    if any(not isinstance(row, dict) for row in rows):
        raise ValueError('workerd capture records must be objects')
    session = rows[0]
    if (session.get('event') != 'session' or session.get('schema') != SCHEMA
            or session.get('context') != context or session.get('attempt') != attempt
            or type(session.get('attempt')) is not int
            or session.get('engine_sha256') != ENGINE_SHA256
            or not isinstance(session.get('gdb_version'), str)):
        raise ValueError('workerd capture identity mismatch')
    root = None
    root_exit = None
    targets = {}
    signal_count = 0
    known_fault = False
    for row in rows[1:-1]:
        event = row.get('event')
        if event not in ('root_started', 'root_exited', 'runtime_started', 'runtime_exited', 'signal'):
            raise ValueError('unknown workerd capture event')
        key = (row.get('inferior'), row.get('pid'))
        if any(type(value) is not int or value < 1 for value in key):
            raise ValueError('invalid captured process identity')
        if event == 'root_started':
            if root is not None or key[0] != 1:
                raise ValueError('ambiguous root Node process')
            root = key
        elif event == 'root_exited':
            if root != key or root_exit is not None or type(row.get('exit_code')) is not int:
                raise ValueError('missing root Node exit status')
            root_exit = row['exit_code']
        elif event == 'runtime_started':
            if (key in targets or root is None or key[0] == root[0] or root_exit is not None
                    or row.get('engine_sha256') != ENGINE_SHA256):
                raise ValueError('duplicate or late workerd startup')
            targets[key] = None
        elif event == 'runtime_exited':
            if key not in targets or targets[key] is not None:
                raise ValueError('unbound or duplicate workerd exit')
            code, sig = row.get('exit_code'), row.get('exit_signal')
            if ((code is None) == (sig is None)
                    or (code is not None and (type(code) is not int or not 0 <= code <= 255))
                    or (sig is not None and (type(sig) is not int or not 1 <= sig <= 64))):
                raise ValueError('unknown workerd exit status')
            targets[key] = dict(exit_code=code, exit_signal=sig)
        else:
            if key not in targets or targets[key] is not None:
                raise ValueError('signal outside a live workerd process')
            registers, mappings = row.get('registers', {}), row.get('mappings', {})
            thread = row.get('thread')
            if (row.get('signal') not in ('SIGSEGV', 'SIGBUS', 'SIGILL', 'SIGFPE', 'SIGABRT')
                    or not isinstance(thread, list) or len(thread) != 3
                    or any(type(value) is not int or value < 0 for value in thread)
                    or thread[0] != key[1] or thread[1] < 1
                    or not isinstance(registers, dict) or set(registers) != REGISTERS
                    or not all(address(value) for value in registers.values())
                    or row.get('maps_complete') is not True or not isinstance(mappings, dict)
                    or set(mappings) != MAPPINGS
                    or (row.get('si_code') is not None and type(row['si_code']) is not int)
                    or (row.get('si_addr') is not None and not address(row['si_addr']))
                    or (row.get('pc_offset') is not None and not address(row['pc_offset']))):
                raise ValueError('incomplete workerd signal context')
            for mapping in mappings.values():
                if mapping is not None and (not isinstance(mapping, dict)
                        or set(mapping) != {'start', 'end', 'permissions'}
                        or not address(mapping['start']) or not address(mapping['end'])
                        or int(mapping['start'], 16) >= int(mapping['end'], 16)
                        or not re.fullmatch(r'[r-][w-][x-][ps]', mapping['permissions'])):
                    raise ValueError('invalid workerd mapping context')
            signal_count += 1
            known_fault |= row['signal'] == 'SIGSEGV' and row['pc_offset'] == KNOWN_FAULT_OFFSET
    complete = rows[-1]
    if (len(rows) < 4 or complete.get('event') != 'complete'
            or complete.get('errors') != [] or complete.get('open_runtimes') != []
            or root is None or root_exit is None or any(value is None for value in targets.values())):
        raise ValueError('workerd debugger did not complete observation')
    # Exactly one runtime is part of this test's current fixture contract. A
    # restart cannot erase its predecessor's failure even when Node later exits 0.
    unexpected_exit = any(value['exit_code'] not in (None, 0)
                          or value['exit_signal'] not in (None, 9) for value in targets.values())
    passed = root_exit == 0 and len(targets) == 1 and not known_fault and not unexpected_exit
    return dict(root_exit_code=root_exit, runtime_count=len(targets), signal_count=signal_count,
                known_fault=known_fault, unexpected_exit=unexpected_exit,
                terminations=[dict(value, origin='unknown',
                                   compatible_with_dispose=value['exit_signal'] == 9)
                              for value in targets.values()], passed=passed)


def finish(directory, context, attempt, node_passed):
    report = dict(schema=SCHEMA, context=context, attempt=attempt,
                  engine_sha256=ENGINE_SHA256, node_passed=node_passed,
                  complete=False, passed=False)
    try:
        summary = summarize(read_regular(directory / 'records.jsonl'), context, attempt)
        report.update(summary=summary, complete=True, passed=node_passed is True and summary['passed'])
    except (OSError, ValueError, TypeError, KeyError):
        report['error'] = 'missing_invalid_or_incomplete_capture'
    (directory / 'report.json').write_text(json.dumps(report, sort_keys=True) + '\n')
    return report


def validate(directory, context, attempt):
    report = read_json(directory / 'report.json')
    summary = summarize(read_regular(directory / 'records.jsonl'), context, attempt)
    if (not isinstance(report, dict) or report.get('schema') != SCHEMA or report.get('context') != context
            or type(report.get('attempt')) is not int or report['attempt'] != attempt
            or report.get('engine_sha256') != ENGINE_SHA256
            or report.get('node_passed') is not True or report.get('complete') is not True
            or report.get('passed') is not True or summary['passed'] is not True
            or report.get('summary') != summary):
        raise ValueError('workerd capture did not complete successfully')
