#!/usr/bin/env python3
import argparse
import copy
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

SCRIPT = Path(__file__).with_name('dot-todo.py')
spec = importlib.util.spec_from_file_location('todo', SCRIPT)
todo = importlib.util.module_from_spec(spec)
spec.loader.exec_module(todo)


class SnapshotTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.path = Path(self.temp.name) / 'state.json'
        self.state = todo.seal({'schema': 1, 'revision': 0, 'tasks': [
            {'id': 'example-1', 'text': 'Inspect synthetic source', 'status': 'pending', 'history': []},
            {'id': 'example-2', 'text': 'Inspect another synthetic source', 'status': 'pending', 'history': []}]})
        self.save(self.state)

    def save(self, state):
        self.path.write_text(json.dumps(state))

    def args(self, command, **kwargs):
        return argparse.Namespace(state=str(self.path), revision=self.state['revision'],
                                  checksum=self.state['checksum'], command=command, **kwargs)

    def advance(self, command, **kwargs):
        output = todo.run(self.args(command, **kwargs))
        self.state = output['snapshot']
        return output

    def result(self, status, remaining=None):
        path = self.path.with_name('result.json')
        path.write_text(json.dumps({'id': 'example-1', 'text_checksum': todo.digest(self.state['tasks'][0]['text']),
                                    'status': status, 'evidence': ['synthetic readback evidence'],
                                    'remaining': remaining or []}))
        return self.args('result', id='example-1', result=str(path))

    def test_normal_completion_and_roundtrip(self):
        self.advance('claim')
        self.advance('start', id='example-1')
        self.state = todo.run(self.result('completed'))['snapshot']
        exported = json.loads(json.dumps(self.state))
        self.save(exported)  # parent restores exact synthetic snapshot
        self.assertEqual(self.advance('show')['snapshot'], exported)
        self.assertEqual(self.advance('claim')['selected']['id'], 'example-2')
        self.assertEqual(self.state['tasks'][0]['status'], 'completed')

    def test_duplicate_claim_and_running_recovery(self):
        stale = self.args('claim')
        self.advance('claim')
        with self.assertRaises(todo.Stop):
            todo.run(stale)
        with self.assertRaises(todo.Stop):
            self.advance('claim')
        self.advance('start', id='example-1')
        self.save(json.loads(json.dumps(self.state)))
        with self.assertRaises(todo.Stop):
            self.advance('claim')
        with self.assertRaises(todo.Stop):
            self.advance('start', id='example-1')
        self.assertEqual(self.advance('show')['snapshot']['tasks'][0]['status'], 'running')

    def test_unknown_blocks_retry(self):
        self.advance('claim')
        self.state = todo.run(self.result('unknown', ['Find original dispatch']))['snapshot']
        with self.assertRaises(todo.Stop):
            self.advance('claim')
        with self.assertRaises(todo.Stop):
            self.advance('start', id='example-1')
        self.assertEqual(len(self.state['tasks']), 2)

    def test_blocked_retained_and_other_work_allowed(self):
        self.advance('claim')
        self.state = todo.run(self.result('blocked', ['Missing source permission; no action started']))['snapshot']
        self.advance('claim')
        self.assertEqual(self.state['tasks'][0]['history'][-1]['remaining'],
                         ['Missing source permission; no action started'])
        self.assertEqual(self.state['tasks'][0]['status'], 'blocked')

    def test_missing_corrupt_old_version_conflict(self):
        self.path.unlink()
        with self.assertRaises(OSError):
            self.advance('claim')
        self.path.write_text('{')
        with self.assertRaises(ValueError):
            self.advance('claim')
        for change in ({'schema': 0}, {'revision': 9}, {'checksum': 'bad'}):
            bad = dict(self.state, **change)
            self.save(bad)
            with self.assertRaises(todo.Stop):
                self.advance('claim')
        changed = copy.deepcopy(self.state)
        changed['tasks'][0]['text'] = 'Changed obligation'
        self.save(todo.seal(changed))
        with self.assertRaises(todo.Stop):
            self.advance('claim')

    def test_atomic_replace_failure_preserves_bytes(self):
        before = self.path.read_bytes()
        with patch.object(todo.os, 'replace', side_effect=OSError('synthetic failure')):
            with self.assertRaises(OSError):
                self.advance('claim')
        self.assertEqual(self.path.read_bytes(), before)
        self.assertEqual(list(self.path.parent.glob('.state.json-*')), [])

    def test_post_replace_fsync_failure_requires_readback(self):
        with patch.object(todo.os, 'fsync', side_effect=[None, OSError('directory fsync failed')]):
            with self.assertRaises(OSError):
                self.advance('claim')
        restored = todo.read_json(self.path)
        self.assertEqual(restored['tasks'][0]['status'], 'claimed')
        with self.assertRaises(todo.Stop):
            self.advance('claim')

    def test_two_processes_compete(self):
        cmd = [sys.executable, str(SCRIPT), '--state', str(self.path), '--revision', '0',
               '--checksum', self.state['checksum'], 'claim']
        processes = [subprocess.Popen(cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE) for _ in range(2)]
        outputs = [p.communicate(timeout=10) for p in processes]
        self.assertEqual(sorted(p.returncode for p in processes), [0, 2], outputs)
        state = todo.read_json(self.path)
        self.assertEqual(state['revision'], 1)
        self.assertEqual([t['status'] for t in state['tasks']], ['claimed', 'pending'])

    def test_lock_contention(self):
        with todo.locked(self.path):
            with self.assertRaises(todo.Stop):
                self.advance('claim')

    def test_result_requires_evidence_and_original_content(self):
        self.advance('claim')
        with self.assertRaises(todo.Stop):
            todo.run(self.result('completed'))
        self.advance('start', id='example-1')
        args = self.result('completed')
        path = Path(args.result)
        data = todo.read_json(path)
        for change in ({'evidence': []}, {'text_checksum': 'wrong'}, {'remaining': ['unfinished']}):
            path.write_text(json.dumps(dict(data, **change)))
            with self.assertRaises(todo.Stop):
                todo.run(args)

    def test_duplicate_json_keys_rejected(self):
        self.path.write_text('{"schema":1,"schema":1}')
        with self.assertRaises(todo.Stop):
            self.advance('show')


if __name__ == '__main__':
    unittest.main()
