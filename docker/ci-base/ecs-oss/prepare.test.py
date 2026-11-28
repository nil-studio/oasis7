#!/usr/bin/env python3
"""Exercise the distribution lock and bounded fetch failure paths without network."""
import copy
import hashlib
import importlib.util
import io
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('prepare', Path(__file__).with_name('prepare.py'))
prepare = importlib.util.module_from_spec(spec)
spec.loader.exec_module(prepare)


class Response(io.BytesIO):
    url = 'https://files.pythonhosted.org/fixture.whl'


class SupplyChain(unittest.TestCase):
    def test_exact_linux_inventory_and_build_tools(self):
        rows = prepare.artifacts()
        self.assertEqual(len(rows), 18)
        self.assertEqual({row['name'] for row in rows if row['group'] == 'build'},
                         {'setuptools', 'wheel', 'packaging'})
        self.assertEqual({row['name'] for row in rows if row['filename'].endswith('.tar.gz')},
                         {'oss2', 'crcmod'})
        self.assertEqual(sum(row['group'] == 'sdk' for row in rows), 15)
        self.assertFalse(any('macosx' in row['filename'] for row in rows))

    def test_bad_lock_identity_and_bounds_fail_before_network(self):
        original = prepare.artifacts()
        for field, value in [('url', 'https://untrusted.invalid/fixture.whl'),
                             ('sha256', 'x' * 64), ('size', True),
                             ('size', 21 * 1024 * 1024), ('group', 'unknown')]:
            rows = copy.deepcopy(original)
            rows[0][field] = value
            with self.subTest(field=field, value=value), patch.object(Path, 'read_text', return_value=json.dumps(rows)):
                with self.assertRaises(ValueError):
                    prepare.artifacts()

    def fetch_fixture(self, data, *, size=3, digest=None, url=None):
        row = dict(filename='fixture.whl', url=Response.url, size=size,
                   sha256=digest or hashlib.sha256(b'abc').hexdigest())
        response = Response(data)
        if url:
            response.url = url
        with tempfile.TemporaryDirectory() as tmp, patch.object(prepare, 'artifacts', return_value=[row]), \
                patch.object(prepare.urllib.request, 'urlopen', return_value=response):
            destination = Path(tmp) / 'new'
            prepare.fetch(destination)
            return (destination / 'fixture.whl').read_bytes()

    def test_exact_download_bytes(self):
        self.assertEqual(self.fetch_fixture(b'abc'), b'abc')

    def test_truncation_overflow_and_hash_mismatch(self):
        for data in (b'ab', b'abcd', b'xyz'):
            with self.subTest(data=data), self.assertRaises(ValueError):
                self.fetch_fixture(data)

    def test_redirect_outside_pypi_files_rejected(self):
        with self.assertRaises(ValueError):
            self.fetch_fixture(b'abc', url='https://untrusted.invalid/fixture.whl')


if __name__ == '__main__':
    unittest.main()
