#!/usr/bin/env python3
import importlib.util
import os
from pathlib import Path
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

os.environ.setdefault('RUNNER_TEMP', tempfile.gettempdir())
spec = importlib.util.spec_from_file_location('probe', Path(__file__).with_name('ordinary-full-phase-probe.py'))
probe = importlib.util.module_from_spec(spec)
spec.loader.exec_module(probe)


class PublishedClosure(unittest.TestCase):
    def setUp(self):
        self.manifest = {'binaries': {
            name: {'path': name, 'sha256': name + '-sha'}
            for name in ('runtime-tests', 'viewer', 'omitted-parent-harness')}}

    def check_file(self, bundle, path, digest):
        if path == 'omitted-parent-harness':
            raise ValueError('legally absent outside published closure')
        self.assertEqual(digest, path + '-sha')
        return bundle / path

    def test_original_preflight_reproduces_unpublished_parent_failure(self):
        # This is the original live-probe failing loop, prior to any ldd failure.
        with self.assertRaisesRegex(ValueError, 'legally absent'):
            for entry in self.manifest['binaries'].values():
                self.check_file(Path('/bundle'), entry['path'], entry['sha256'])

    def test_preflight_only_independently_validated_published_closure(self):
        with patch.object(probe.subprocess, 'run', return_value=SimpleNamespace(returncode=0, stdout=b'')) as run:
            probe.preflight_executables(SimpleNamespace(checked_file=self.check_file), Path('/bundle'),
                                        self.manifest, {'runtime-tests', 'viewer'}, {})
        self.assertEqual([call.args[0] for call in run.call_args_list],
                         [['ldd', '/bundle/runtime-tests'], ['ldd', '/bundle/viewer']])

    def test_missing_included_file_remains_rejected(self):
        with self.assertRaisesRegex(ValueError, 'legally absent'):
            probe.preflight_executables(SimpleNamespace(checked_file=self.check_file), Path('/bundle'),
                                        self.manifest, {'omitted-parent-harness'}, {})

    def test_missing_dynamic_library_remains_rejected(self):
        with patch.object(probe.subprocess, 'run', return_value=SimpleNamespace(returncode=0, stdout=b'libmissing.so => not found')):
            with self.assertRaises(AssertionError):
                probe.preflight_executables(SimpleNamespace(checked_file=self.check_file), Path('/bundle'),
                                            self.manifest, {'runtime-tests'}, {})


if __name__ == '__main__':
    unittest.main()
