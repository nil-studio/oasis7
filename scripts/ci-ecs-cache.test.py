#!/usr/bin/env python3
"""Exercise ECS policy independently from network and expensive native builds."""
import importlib.util
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('ecs', Path(__file__).with_name('ci-ecs-cache.py'))
ecs = importlib.util.module_from_spec(spec)
spec.loader.exec_module(ecs)


class EcsCacheTests(unittest.TestCase):
    def test_budget_generation_and_main_scope(self):
        key = 'ecs-workspace-v1-' + 'a' * 64 + '-' + 'b' * 40
        entry = dict(ref='refs/heads/main', key=key[:-40] + 'c' * 40, size_in_bytes=100)
        self.assertTrue(ecs.save_allowed([entry], key, 100))
        self.assertFalse(ecs.save_allowed([entry, dict(entry, key=key[:-40] + 'd' * 40)], key, 100))
        self.assertFalse(ecs.save_allowed([dict(entry, size_in_bytes=ecs.FAMILY_BYTES)], key, 1))
        self.assertFalse(ecs.save_allowed([dict(entry, key=key)], key, 1))
        self.assertTrue(ecs.save_allowed([dict(entry, ref='refs/heads/topic', size_in_bytes=ecs.FAMILY_BYTES),
                                                dict(entry, key='formal-workspace-unrelated', size_in_bytes=ecs.FAMILY_BYTES)], key, 1))

    def test_corrupt_cache_falls_back(self):
        with tempfile.TemporaryDirectory() as checkout, tempfile.TemporaryDirectory() as cache:
            root = Path(checkout)
            (root / 'Cargo.toml').write_text('not a real crate')
            (Path(cache) / 'manifest.json').write_text('{invalid')
            report = ecs.units.cache_restore(root, Path(cache), ecs.STAGING_BYTES)
            self.assertFalse(report['restored'])
            self.assertFalse((root / 'target').exists())

    def test_record_failure_still_runs_cargo(self):
        with tempfile.TemporaryDirectory() as tmp:
            state = Path(tmp) / 'state.json'
            with patch.object(ecs.units, 'cache_begin', side_effect=ValueError('unsafe input')):
                ecs.build(['python3', '-c', 'print(\'{"reason":"build-finished","success":true}\')'], {}, state)
            self.assertEqual(json.loads(state.read_text()), {})
            self.assertEqual(json.loads(state.with_name('ecs-units.json').read_text())['rebuilt'], 0)

    def test_build_failure_is_never_hidden(self):
        with tempfile.TemporaryDirectory() as tmp:
            with patch.object(ecs.units, 'cache_begin', side_effect=ValueError('unsafe input')):
                with self.assertRaises(subprocess.CalledProcessError):
                    ecs.build(['python3', '-c', 'raise SystemExit(7)'], {}, Path(tmp) / 'state.json')

    def test_reports_cannot_be_source_inputs(self):
        with self.assertRaises(ValueError):
            ecs.temporary(ecs.ROOT / 'output' / 'cache-state.json')

    def test_untracked_inputs_and_path_changes_invalidate_collection(self):
        for change in ('add', 'remove', 'rename', 'content'):
            with self.subTest(change=change), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                (root / '.gitignore').write_text('ignored.txt\n')
                original = root / 'ignored.txt'
                original.write_text('before')
                state = ecs.units.cache_begin(root)
                self.assertIn('ignored.txt', state['inputs'])
                if change == 'add':
                    (root / 'untracked.txt').write_text('new')
                elif change == 'remove':
                    original.unlink()
                elif change == 'rename':
                    original.rename(root / 'renamed.txt')
                else:
                    original.write_text('after')
                with self.assertRaisesRegex(ValueError, 'source inputs changed'):
                    ecs.units.cache_finish(state)


if __name__ == '__main__':
    unittest.main()
