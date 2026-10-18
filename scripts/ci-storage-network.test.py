#!/usr/bin/env python3
import importlib.util
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('network', Path(__file__).with_name('ci-storage-network.py'))
network = importlib.util.module_from_spec(spec)
spec.loader.exec_module(network)


class NetworkTests(unittest.TestCase):
    def invoke(self, action, entries, profile='delay', after=None):
        with tempfile.TemporaryDirectory() as directory, patch.dict(os.environ, GITHUB_ACTIONS='true'), \
                patch.object(network.platform, 'system', return_value='Linux'), \
                patch.object(network, 'qdiscs', side_effect=[entries, after or []]), \
                patch.object(network.subprocess, 'run') as run:
            value = network.operate(action, profile, Path(directory) / 'network.json')
            return value, run.call_args_list

    def test_refuses_existing_discipline(self):
        with self.assertRaisesRegex(ValueError, 'refusing'):
            self.invoke('setup', [{'kind': 'fq_codel'}])

    def test_setup_is_loopback_only_and_compatible_with_noble(self):
        value, calls = self.invoke('setup', [{'kind': 'noqueue'}], after=[{'kind': 'netem', 'handle': network.HANDLE}])
        self.assertEqual(calls[0].args[0][5], 'lo')
        self.assertNotIn('seed', calls[0].args[0])
        self.assertEqual(value['device'], 'lo')

    def test_missing_traffic_and_unobserved_loss_are_not_passes(self):
        for packets, drops in [(0, 0), (100, 0)]:
            with self.assertRaises(ValueError):
                self.invoke('check', [{'kind': 'netem', 'handle': network.HANDLE, 'packets': packets, 'drops': drops}], 'loss')
        self.invoke('check', [{'kind': 'netem', 'handle': network.HANDLE, 'packets': 100, 'drops': 2}], 'loss')

    def test_cleanup_never_removes_other_owner(self):
        _, calls = self.invoke('cleanup', [{'kind': 'netem', 'handle': '123:'}])
        self.assertEqual(calls, [])

    def test_cleanup_checks_actual_removal(self):
        entry = {'kind': 'netem', 'handle': network.HANDLE}
        with self.assertRaisesRegex(ValueError, 'remains'):
            self.invoke('cleanup', [entry], after=[entry])

    def test_refuses_normal_local_host(self):
        with patch.dict(os.environ, GITHUB_ACTIONS='false'):
            with self.assertRaises(ValueError):
                network.operate('setup', 'delay', Path('/unused'))


if __name__ == '__main__':
    unittest.main()
