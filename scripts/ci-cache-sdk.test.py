#!/usr/bin/env python3
"""Offline source/distribution integrity and real SDK archive regression entry."""
import hashlib
import json
from pathlib import Path
import subprocess
import unittest

ROOT = Path(__file__).resolve().parents[1]
ACTION = ROOT / '.github/actions/cache-sdk'
VENDOR = ROOT / 'vendor/actions-cache-explicit-context'


class CacheSdkTests(unittest.TestCase):
    def test_fixed_upstream_source_and_distribution_receipt(self):
        receipt = json.loads((ACTION / 'build-receipt.json').read_text())
        self.assertEqual(receipt['schema'], 1)
        self.assertEqual(set(receipt['outputs']), {item.name for item in VENDOR.iterdir()})
        for category, directory in (('inputs', ACTION), ('outputs', VENDOR)):
            for name, expected in receipt[category].items():
                self.assertEqual(hashlib.sha256((directory / name).read_bytes()).hexdigest(), expected, name)
        provenance = json.loads((ACTION / 'provenance.json').read_text())
        self.assertEqual(provenance['upstream_action']['sha'], '55cc8345863c7cc4c66a329aec7e433d2d1c52a9')
        lock = json.loads((ACTION / 'package-lock.json').read_text())
        self.assertEqual(lock['packages']['node_modules/@actions/cache']['version'], '6.1.0')
        self.assertEqual(lock['packages']['node_modules/@actions/cache']['integrity'], provenance['sdk']['dist']['integrity'])
        self.assertTrue(all('integrity' in entry and entry.get('resolved', '').startswith('https://registry.npmjs.org/')
                            for name, entry in lock['packages'].items() if name))
        self.assertFalse((ACTION / 'action.yml').exists(), 'Dark prerequisite must not add a runnable cache action')

    def test_real_sdk_tar_handoffs_and_v2_pre_extract_failure(self):
        result = subprocess.run(['node', '--test', str(ACTION / 'sdk.test.cjs')], cwd=ROOT,
                                capture_output=True, text=True, timeout=120)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)


if __name__ == '__main__':
    unittest.main()
