#!/usr/bin/env python3
"""Check the independent trusted-full SDK cleanup authority without touching SDKs."""
import importlib.util
import json
from pathlib import Path
import stat
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location(
    'reclaim', Path(__file__).with_name('ci-trusted-full-reclaim.py'))
reclaim = importlib.util.module_from_spec(spec)
spec.loader.exec_module(reclaim)
SHA = '1'*40
RELEASE = ('ID=ubuntu', 'VERSION_ID="24.04"')


class TrustedFullReclaimTests(unittest.TestCase):
    def test_only_fixed_hosted_main_identity_and_exact_checkout(self):
        event = dict(reclaim.IDENTITY, GITHUB_SHA=SHA, GITHUB_WORKFLOW_SHA=SHA,
                     GITHUB_EVENT_NAME='schedule')
        reclaim.check_authority(event, SHA, RELEASE, platform='linux', uid=0)
        for key, value in {**dict.fromkeys(reclaim.IDENTITY, 'invalid'),
                           'GITHUB_EVENT_NAME': 'pull_request', 'GITHUB_WORKFLOW_SHA': '0'*40}.items():
            with self.subTest(key=key), self.assertRaises(ValueError):
                reclaim.check_authority({**event, key: value}, SHA, RELEASE, platform='linux', uid=0)
        with self.assertRaises(ValueError):
            reclaim.check_authority(event, '0'*40, RELEASE, platform='linux', uid=0)
        with self.assertRaises(ValueError):
            reclaim.check_authority(event, SHA, RELEASE, platform='linux', uid=1001)

    def test_invalid_threshold_cannot_call_reclaim(self):
        with patch.object(reclaim, 'load_capacity') as load, self.assertRaises(ValueError):
            reclaim.reclaim_owned_sdk(apply=True, min_free_bytes=0, min_free_inodes=1)
        load.assert_not_called()

    def test_all_paths_are_validated_before_any_reclaim(self):
        capacity = reclaim.load_capacity()
        actual_stat = Path.stat
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            first = root / 'first'
            second = root / 'second'
            first.mkdir()
            second.mkdir()
            alias = root / 'alias'
            alias.symlink_to(second, target_is_directory=True)
            for failure, paths, mountpoints in (
                    ('owner', [first, second], []),
                    ('device', [first, second], []),
                    ('world-write', [first, second], []),
                    ('mount', [first, second], [second / 'nested-mount']),
                    ('symlink', [first, alias], [])):
                def fake_stat(path, *args, **kwargs):
                    metadata = actual_stat(path, *args, **kwargs)
                    return SimpleNamespace(
                        st_mode=(stat.S_IFDIR | 0o777 if path == second and failure == 'world-write'
                                 else metadata.st_mode & ~0o022),
                        st_dev=2 if path == second and failure == 'device' else 1,
                        st_uid=1001 if path == second and failure == 'owner' else 0)

                mount_table = Path('/proc/self/mountinfo')
                read_text = Path.read_text

                def fake_mount_info(path, *args, **kwargs):
                    return '' if path == mount_table else read_text(path, *args, **kwargs)

                with (self.subTest(failure=failure),
                      patch.object(Path, 'stat', fake_stat),
                      patch.object(Path, 'read_text', fake_mount_info),
                      patch.object(capacity, 'ALLOWLIST', [str(path) for path in paths]),
                      patch.object(capacity, 'mounts', return_value=mountpoints),
                      patch.object(capacity, 'capacity_domains'),
                      patch.object(capacity, 'reclaim') as apply,
                      patch.object(reclaim, 'load_capacity', return_value=capacity),
                      self.assertRaises((ValueError, RuntimeError))):
                    reclaim.reclaim_owned_sdk(apply=True, min_free_bytes=1, min_free_inodes=1)
                apply.assert_not_called()

    def test_dispatch_requires_full_mode_and_no_candidate_source(self):
        with tempfile.TemporaryDirectory() as temporary:
            receipt = Path(temporary).resolve() / 'event.json'
            event = dict(reclaim.IDENTITY, GITHUB_SHA=SHA, GITHUB_WORKFLOW_SHA=SHA,
                         GITHUB_EVENT_NAME='workflow_dispatch', GITHUB_EVENT_PATH=str(receipt))
            receipt.write_text(json.dumps({'inputs': {'run_mode': 'full', 'source_ref': ''}}))
            reclaim.check_authority(event, SHA, RELEASE, platform='linux', uid=0)
            for inputs in ({'run_mode': 'full', 'source_ref': 'candidate'},
                           {'run_mode': 'other'}, {}):
                receipt.write_text(json.dumps({'inputs': inputs}))
                with self.assertRaises(ValueError):
                    reclaim.check_authority(event, SHA, RELEASE, platform='linux', uid=0)
            receipt.write_bytes(b'x' * (reclaim.EVENT_LIMIT + 1))
            with self.assertRaises(ValueError):
                reclaim.read_dispatch(receipt)
            alias = receipt.with_name('alias')
            alias.symlink_to(receipt)
            with self.assertRaises(ValueError):
                reclaim.read_dispatch(alias)


if __name__ == '__main__':
    unittest.main()
