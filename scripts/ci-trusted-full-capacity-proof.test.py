#!/usr/bin/env python3
"""Validate the bounded read-only measurement, with no Docker or runner writes."""
import importlib.util
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location(
    'capacity', Path(__file__).with_name('ci-trusted-full-capacity-proof.py'))
capacity = importlib.util.module_from_spec(spec)
spec.loader.exec_module(capacity)


class CapacityProofTests(unittest.TestCase):
    def test_real_statvfs_reports_actual_filesystem_capacity(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            record = capacity.disk(directory)
            self.assertEqual(record['device'], directory.stat().st_dev)
            self.assertGreater(record['free_bytes'], 0)
            self.assertGreater(record['free_inodes'], 0)

    def test_sizes_are_allocated_bytes_and_bad_paths_are_never_measured(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            actual = root / 'regular'
            actual.write_text('fixture')
            alias = root / 'alias'
            alias.symlink_to(actual)
            missing = root / 'missing'
            with patch.object(capacity, 'local_command',
                              return_value=subprocess.CompletedProcess([], 0, '16\tregular\n')) as run:
                self.assertEqual(capacity.allocated(actual), {'state': 'measured', 'allocated_bytes': 16384})
                self.assertEqual(capacity.allocated(alias), {'state': 'unsupported-path'})
                self.assertEqual(capacity.allocated(missing), {'state': 'absent'})
                run.assert_called_once_with('/usr/bin/du', '-skx', '--', str(actual))
            with patch.object(capacity, 'local_command', side_effect=subprocess.TimeoutExpired('du', 20)):
                self.assertEqual(capacity.allocated(actual), {'state': 'unavailable'})

    def test_docker_receipt_keeps_only_local_read_commands_and_fixed_image(self):
        usage = '{"Type":"Images","TotalCount":"1","Size":"1GB","Reclaimable":"0B","Extra":"ignored"}\n'
        outputs = [subprocess.CompletedProcess([], 0, value) for value in (usage, '12345\n', '/var/lib/docker\n')]
        with patch.object(Path, 'is_socket', return_value=True), patch.object(Path, 'is_dir', return_value=True), \
             patch.object(capacity, 'disk', return_value={'device': 1}), \
             patch.object(capacity, 'local_command', side_effect=outputs) as run:
            result = capacity.docker_receipt()
        self.assertEqual(result['fixed_image_bytes'], 12345)
        self.assertNotIn('Extra', result['storage'][0])
        cli = ('/usr/bin/docker', '--host', 'unix:///var/run/docker.sock')
        self.assertEqual([call.args for call in run.call_args_list], [
            (*cli, 'system', 'df', '--format', '{{json .}}'),
            (*cli, 'image', 'inspect', '--format', '{{.Size}}', capacity.IMAGE),
            (*cli, 'info', '--format', '{{.DockerRootDir}}'),
        ])

    def test_local_callers_cannot_claim_historical_receipt(self):
        with patch.object(capacity.sys, 'platform', 'darwin'), patch.object(capacity, 'local_command') as run:
            with self.assertRaisesRegex(ValueError, 'hosted Ubuntu workspace'):
                capacity.require_source()
        run.assert_not_called()


if __name__ == '__main__':
    unittest.main()
