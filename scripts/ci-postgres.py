#!/usr/bin/env python3
"""Verify the disposable PostgreSQL service and execute its trusted exact libtest."""
import argparse
import json
import os
from pathlib import Path
import re
import socket
import struct
import subprocess
import tempfile
import time

from ci_process_runner import run

KEY = 'postgres_test_contract'
CONFIG = 'scripts/ci-required-scope.json'


def contract(root, plan):
    def git(*args):
        return subprocess.check_output(['git', *args], cwd=root, text=True, timeout=10).strip()
    refs = {plan['architecture']['base'], *(plan[key] for key in ('source_sha', 'tested_sha', 'policy_sha'))}
    if any(not isinstance(ref, str) or not re.fullmatch(r'[0-9a-f]{40}', ref) for ref in refs):
        raise ValueError('PostgreSQL proof requires complete commit identities')
    if git('rev-parse', 'HEAD') != plan['tested_sha']:
        raise ValueError('PostgreSQL tested checkout differs from the plan')
    trusted = json.loads(git('show', plan['policy_sha'] + ':' + CONFIG)).get(KEY)
    descriptor = trusted if trusted is not None else json.loads((root / CONFIG).read_text())[KEY]
    target = descriptor['target']
    if (set(descriptor) != {'version', 'group', 'runner', 'server_version_num', 'module', 'source', 'target', 'case'}
            or descriptor['version'] != 1 or descriptor['group'] != 'distfs'
            or descriptor['runner'] != 'scripts/ci-postgres.py'
            or type(descriptor['server_version_num']) is not int
            or set(target) != {'package', 'target_kind', 'target_name', 'default_features', 'features'}
            or target['target_kind'] != 'lib' or target['default_features'] is not True or target['features'] != []
            or any(not re.fullmatch(r'[A-Za-z_][A-Za-z_0-9-]*', target[key]) for key in ('package', 'target_name'))
            or not re.fullmatch(r'[A-Za-z_][A-Za-z_0-9]*(?:::[A-Za-z_][A-Za-z_0-9]*)+', descriptor['case'])):
        raise ValueError('invalid PostgreSQL test contract')
    paths = [descriptor[key] for key in ('module', 'source')]
    if any(Path(path).is_absolute() or '..' in Path(path).parts or not path.endswith('.rs') for path in paths):
        raise ValueError('invalid PostgreSQL source proof paths')
    for ref in refs:
        git('cat-file', '-e', ref + '^{commit}')
    present = {ref: bool(git('ls-tree', '--name-only', ref, '--', *paths)) for ref in refs}
    source, policy = plan['source_sha'], plan['policy_sha']
    historical = (trusted is not None and plan.get('execution_profile') in ('periodic-full', 'candidate-full')
                  and source == plan['tested_sha'] and source != policy and not present[source]
                  and git('merge-base', source, policy) == source)
    required = any(present.values()) and not historical
    selected = plan.get(KEY)
    if trusted is None:
        if selected is not None or required:
            raise ValueError('old policy cannot authorize a PostgreSQL product test')
    elif (not isinstance(selected, dict) or selected != dict(descriptor, required=required, proof_refs=selected.get('proof_refs'))
          or type(selected['required']) is not bool or not isinstance(selected['proof_refs'], list)
          or len(selected['proof_refs']) != len(refs) or set(selected['proof_refs']) != refs):
        raise ValueError('missing or changed trusted PostgreSQL execution proof')
    return descriptor, required


def probe(host, port, expected_version):
    """Bounded protocol-3 trust-authentication probe; issues one read-only SELECT."""
    with socket.create_connection((host, port), timeout=3) as connection:
        deadline = time.monotonic() + 5
        def read(size):
            data = bytearray()
            while len(data) < size:
                connection.settimeout(max(0.001, deadline - time.monotonic()))
                if time.monotonic() >= deadline:
                    raise TimeoutError('PostgreSQL probe deadline')
                chunk = connection.recv(size - len(data))
                if not chunk:
                    raise ValueError('truncated PostgreSQL protocol message')
                data.extend(chunk)
            return bytes(data)
        def messages():
            for _ in range(64):
                tag, size = read(1), struct.unpack('!I', read(4))[0]
                if not 4 <= size <= 65536:
                    raise ValueError('unbounded PostgreSQL protocol message')
                yield tag, read(size - 4)
                if tag == b'Z':
                    return
            raise ValueError('too many PostgreSQL protocol messages')
        startup = struct.pack('!I', 196608) + b'user\0postgres\0database\0postgres\0client_encoding\0UTF8\0\0'
        connection.sendall(struct.pack('!I', len(startup) + 4) + startup)
        authenticated = False
        for tag, body in messages():
            if tag == b'R' and body == b'\0\0\0\0':
                authenticated = True
            elif tag in (b'S', b'K'):
                continue
            elif tag != b'Z' or body != b'I' or not authenticated:
                raise ValueError('PostgreSQL service must provide isolated trust authentication')
        query = b"SELECT current_setting('server_version_num'),current_setting('fsync'),current_setting('synchronous_commit')\0"
        connection.sendall(b'Q' + struct.pack('!I', len(query) + 4) + query)
        rows, completed = [], False
        for tag, body in messages():
            if tag == b'D':
                if len(body) < 2 or struct.unpack('!H', body[:2])[0] != 3:
                    raise ValueError('unexpected PostgreSQL settings row')
                values, offset = [], 2
                for _ in range(3):
                    size = struct.unpack('!i', body[offset:offset + 4])[0]
                    offset += 4
                    if not 0 <= size <= 32 or offset + size > len(body):
                        raise ValueError('invalid PostgreSQL settings value')
                    values.append(body[offset:offset + size].decode('ascii'))
                    offset += size
                if offset != len(body):
                    raise ValueError('trailing PostgreSQL settings data')
                rows.append(values)
            elif tag == b'C' and body == b'SELECT 1\0':
                completed = True
            elif tag != b'T' and not (tag == b'Z' and body == b'I'):
                raise ValueError('PostgreSQL settings query failed')
        if not completed or rows != [[str(expected_version), 'on', 'on']]:
            raise ValueError('PostgreSQL version/fsync/synchronous_commit does not match the contract')
        print('PostgreSQL service verified: version=' + rows[0][0] + ' fsync=on synchronous_commit=on', flush=True)


def child(command, root, env, directory, name, expected=None, timeout=600):
    log = directory / (name + '.log')
    with log.open('wb') as stream:
        result = run(command, root, env, 'postgres', name, directory / 'results.jsonl',
                     expected_count=expected, timeout_seconds=timeout, log_stream=stream)
    if not result['passed'] or log.stat().st_size > 16 * 1024 * 1024:
        raise ValueError('PostgreSQL child failed or exceeded log budget: ' + name)
    output = log.read_text()
    if expected is not None and re.findall(r'^test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;', output, re.M) != [('1', '0', '0')]:
        raise ValueError('PostgreSQL exact case did not execute once with zero ignored')
    return output


def execute(root, descriptor, host, port):
    target, case = descriptor['target'], descriptor['case']
    env = dict(os.environ, OASIS7_POSTGRES_TEST_URL=f'host={host} port={port} user=postgres dbname=postgres connect_timeout=5')
    env.pop('RUSTC_WRAPPER', None)
    with tempfile.TemporaryDirectory(prefix='ci-postgres-') as temporary:
        directory = Path(temporary)
        output = child(['cargo', 'test', '--locked', '-p', target['package'], '--lib', '--no-run', '--message-format=json'],
                       root, env, directory, 'build')
        artifacts = [json.loads(line) for line in output.splitlines() if line.startswith('{')]
        binaries = [row['executable'] for row in artifacts if row.get('reason') == 'compiler-artifact'
                    and row.get('target', {}).get('kind') == ['lib']
                    and row['target']['name'] == target['target_name'] and row.get('profile', {}).get('test')
                    and row.get('executable')]
        if len(binaries) != 1 or not Path(binaries[0]).is_file():
            raise ValueError('PostgreSQL target did not produce one library test harness')
        inventory = child([binaries[0], '--list', '--ignored'], root, env, directory, 'inventory', timeout=30)
        if inventory.splitlines().count(case + ': test') != 1:
            raise ValueError('PostgreSQL qualified ignored case is missing or duplicated')
        child([binaries[0], case, '--exact', '--ignored', '--nocapture', '--test-threads=1'],
              root, env, directory, 'acceptance', expected=1, timeout=120)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--repo-root', type=Path, required=True)
    parser.add_argument('--plan', type=Path, required=True)
    parser.add_argument('--host', default='postgres')
    parser.add_argument('--port', type=int, default=5432)
    args = parser.parse_args()
    if not re.fullmatch(r'[A-Za-z0-9.-]+', args.host) or not 0 < args.port < 65536:
        parser.error('invalid PostgreSQL endpoint')
    descriptor, required = contract(args.repo_root, json.loads(args.plan.read_text()))
    probe(args.host, args.port, descriptor['server_version_num'])
    if required:
        execute(args.repo_root, descriptor, args.host, args.port)
    else:
        print('PostgreSQL smoke only under Git-verified absence/history proof; product acceptance NOT executed.', flush=True)


if __name__ == '__main__':
    main()
