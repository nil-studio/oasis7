#!/usr/bin/env python3
"""Exact PostgreSQL execution proofs, bounded wire decoding and owned child failures."""
import importlib.util
import json
import os
from pathlib import Path
import struct
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
SPEC = importlib.util.spec_from_file_location('postgres', HERE / 'ci-postgres.py')
pg = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(pg)


def packet(tag, body):
    return tag + struct.pack('!I', len(body) + 4) + body


class Wire:
    def __init__(self, data):
        self.data = data
    def __enter__(self):
        return self
    def __exit__(self, *_):
        pass
    def settimeout(self, _):
        pass
    def sendall(self, _):
        pass
    def recv(self, size):
        result, self.data = self.data[:min(size, 2)], self.data[min(size, 2):]
        return result


class Postgres(unittest.TestCase):
    def test_probe_requires_exact_version_and_durable_settings(self):
        start = packet(b'R', b'\0' * 4) + packet(b'Z', b'I')
        for values in (['170012', 'on', 'on'], ['170011', 'on', 'on'], ['170012', 'off', 'on'], ['170012', 'on', 'off']):
            row = struct.pack('!H', 3) + b''.join(struct.pack('!I', len(v)) + v.encode() for v in values)
            wire = start + packet(b'D', row) + packet(b'C', b'SELECT 1\0') + packet(b'Z', b'I')
            with patch.object(pg.socket, 'create_connection', return_value=Wire(wire)):
                if values == ['170012', 'on', 'on']:
                    pg.probe('postgres', 5432, 170012)
                else:
                    with self.assertRaises(ValueError):
                        pg.probe('postgres', 5432, 170012)
        for wire in (b'R' + struct.pack('!I', 65537), packet(b'R', struct.pack('!I', 10)), start, packet(b'Z', b'I')):
            with patch.object(pg.socket, 'create_connection', return_value=Wire(wire)), self.assertRaises(ValueError):
                pg.probe('postgres', 5432, 170012)
        with patch.object(pg.socket, 'create_connection', return_value=Wire(start)), \
                patch.object(pg.time, 'monotonic', side_effect=[0, 6, 6]), self.assertRaises(TimeoutError):
            pg.probe('postgres', 5432, 170012)

    def test_trusted_contract_cannot_drop_case_or_forge_absence(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            def git(*args):
                return subprocess.check_output(['git', *args], cwd=root, text=True).strip()
            git('init', '-q')
            git('config', 'user.email', 'test@example.invalid')
            git('config', 'user.name', 'CI test')
            descriptor = dict(version=1, group='distfs', runner='scripts/ci-postgres.py', server_version_num=170012,
                              module='src/backend.rs', source='src/backend/tests.rs', case='backend::tests::acceptance',
                              target=dict(package='fixture', target_kind='lib', target_name='fixture', default_features=True, features=[]))
            config = root / pg.CONFIG
            config.parent.mkdir()
            config.write_text('{}')
            git('add', '.')
            git('commit', '-qm', 'Old policy')
            old = git('rev-parse', 'HEAD')
            config.write_text(json.dumps({pg.KEY: descriptor}))
            plan = dict(architecture=dict(base=old), source_sha=old, tested_sha=old, policy_sha=old)
            self.assertEqual(pg.contract(root, plan), (descriptor, False))
            git('add', '.')
            git('commit', '-qm', 'Trusted prerequisite')
            trusted = git('rev-parse', 'HEAD')
            plan.update(source_sha=trusted, tested_sha=trusted, policy_sha=trusted)
            with self.assertRaises(ValueError):
                pg.contract(root, plan)
            plan[pg.KEY] = dict(descriptor, required=False, proof_refs=[old, trusted])
            self.assertEqual(pg.contract(root, plan), (descriptor, False))
            source = root / descriptor['source']
            source.parent.mkdir(parents=True)
            source.write_text('#[test] #[ignore] fn acceptance() {}')
            git('add', '.')
            git('commit', '-qm', 'Product source')
            present = git('rev-parse', 'HEAD')
            plan.update(source_sha=present, tested_sha=present)
            plan[pg.KEY]['proof_refs'].append(present)
            with self.assertRaises(ValueError):
                pg.contract(root, plan)
            plan[pg.KEY]['required'] = True
            self.assertEqual(pg.contract(root, plan), (descriptor, True))
            plan['policy_sha'] = old
            with self.assertRaises(ValueError):
                pg.contract(root, plan)
            git('checkout', '--detach', '-q', old)
            plan = dict(architecture=dict(base=present), source_sha=old, tested_sha=old, policy_sha=present,
                        execution_profile='periodic-full')
            plan[pg.KEY] = dict(descriptor, required=False, proof_refs=[old, present])
            self.assertEqual(pg.contract(root, plan), (descriptor, False))
            plan['execution_profile'] = 'pr-fast'
            with self.assertRaises(ValueError):
                pg.contract(root, plan)
            plan['execution_profile'] = 'candidate-full'
            git('commit', '--allow-empty', '-qm', 'Unrelated branch after old source')
            other = git('rev-parse', 'HEAD')
            plan.update(source_sha=other, tested_sha=other)
            plan[pg.KEY]['proof_refs'] = [other, present]
            with self.assertRaises(ValueError):
                pg.contract(root, plan)
            git('checkout', '--detach', '-q', present)
            git('rm', '-q', descriptor['source'])
            git('commit', '-qm', 'Delete product case')
            deleted = git('rev-parse', 'HEAD')
            plan.update(source_sha=deleted, tested_sha=deleted)
            plan[pg.KEY]['proof_refs'] = [deleted, present]
            with self.assertRaises(ValueError):
                pg.contract(root, plan)

    def test_actual_target_and_full_ignored_inventory_are_required(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            binary = root / 'fixture'
            binary.touch()
            descriptor = dict(case='backend::tests::acceptance',
                              target=dict(package='fixture', target_name='fixture'))
            artifact = dict(reason='compiler-artifact', target=dict(kind=['lib'], name='fixture'),
                            profile=dict(test=True), executable=str(binary))
            exact = descriptor['case'] + ': test\n'
            for inventory in (exact, 'other::acceptance: test\n', exact * 2, ''):
                with patch.object(pg, 'child', side_effect=[json.dumps(artifact), inventory, '']) as child:
                    if inventory == exact:
                        pg.execute(root, descriptor, 'localhost', 5432)
                        self.assertEqual(child.call_args.args[0], [str(binary), descriptor['case'], '--exact', '--ignored', '--nocapture', '--test-threads=1'])
                    else:
                        with self.assertRaises(ValueError):
                            pg.execute(root, descriptor, 'localhost', 5432)
                        self.assertEqual(child.call_count, 2)
            artifact['target']['name'] = 'other'
            with patch.object(pg, 'child', return_value=json.dumps(artifact)), self.assertRaises(ValueError):
                pg.execute(root, descriptor, 'localhost', 5432)

    def test_owned_child_rejects_zero_ignored_and_timeout(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for name, summary in [('pass', '1 passed; 0 failed; 0 ignored;'), ('zero', '0 passed; 0 failed; 0 ignored;'), ('ignored', '1 passed; 0 failed; 1 ignored;')]:
                args = [sys.executable, '-c', 'print("test result: ok. ' + summary + '")']
                if name == 'pass':
                    pg.child(args, root, os.environ, root, name, expected=1)
                else:
                    with self.assertRaises(ValueError):
                        pg.child(args, root, os.environ, root, name, expected=1)
            with self.assertRaises(ValueError):
                pg.child([sys.executable, '-c', 'import time; time.sleep(30)'], root, os.environ, root, 'timeout', timeout=0.1)
            results = [json.loads(line) for line in (root / 'results.jsonl').read_text().splitlines()]
            self.assertTrue(results[-1]['timeout'])
            self.assertEqual(results[-1]['cleanup_result'], 'clean')


if __name__ == '__main__':
    unittest.main()
