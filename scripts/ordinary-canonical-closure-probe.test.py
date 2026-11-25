#!/usr/bin/env python3
import copy
import hashlib
import importlib.util
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location('probe', Path(__file__).with_name('ordinary-canonical-closure-probe.py'))
probe = importlib.util.module_from_spec(spec)
spec.loader.exec_module(probe)


class ClosedCanonicalMaterials(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)

        def file(relative):
            path = self.root / relative
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(relative)
            return {'path': relative, 'sha256': hashlib.sha256(path.read_bytes()).hexdigest()}

        self.provider = {'files': {role: file('preview/' + role) for role in ('wasm', 'metadata', 'receipt', 'checker')},
                         'libraries': []}
        self.builtin = {
            'checker': {'files': {'checker': file('builtin/checker')}, 'libraries': []},
            'modules': {name: {role: file(name + '/' + role) for role in ('wasm', 'metadata', 'receipt')}
                        for name in ('m1.sensor.basic', 'm1.mobility.basic', 'm1.memory.core', 'm1.storage.cargo')}}

    def test_exact_five_with_both_compiled_checker_roles(self):
        files, modules = probe.material_files(self.root, self.provider, self.builtin)
        self.assertEqual(set(modules), {'module.runtime.local-test-provider', 'm1.sensor.basic',
                                       'm1.mobility.basic', 'm1.memory.core', 'm1.storage.cargo'})
        self.assertEqual(len(files), 17)
        self.assertIn('preview/checker', files)
        self.assertIn('builtin/checker', files)

    def test_missing_or_additional_module_is_rejected(self):
        for name in ('m1.sensor.basic', 'module.runtime.local-test-provider'):
            changed = copy.deepcopy(self.builtin)
            if name in changed['modules']:
                changed['modules'].pop(name)
            else:
                changed['modules'][name] = self.provider['files']
            with self.subTest(name=name), self.assertRaisesRegex(ValueError, 'exactly the original five'):
                probe.material_files(self.root, self.provider, changed)

    def test_extra_file_corruption_and_symlink_are_rejected(self):
        extra = self.root / 'unexpected'
        extra.write_text('not in the declared closure')
        with self.assertRaisesRegex(ValueError, 'file inventory'):
            probe.material_files(self.root, self.provider, self.builtin)
        extra.unlink()
        wasm = self.root / 'preview/wasm'
        original = wasm.read_bytes()
        wasm.write_bytes(b'corrupt')
        with self.assertRaisesRegex(ValueError, 'file bytes'):
            probe.material_files(self.root, self.provider, self.builtin)
        wasm.write_bytes(original)
        wasm.unlink()
        wasm.symlink_to(self.root / 'preview/metadata')
        with self.assertRaisesRegex(ValueError, 'file bytes'):
            probe.material_files(self.root, self.provider, self.builtin)


if __name__ == '__main__':
    unittest.main()
