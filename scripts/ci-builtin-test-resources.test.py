import importlib.util
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch
spec = importlib.util.spec_from_file_location('artifacts', Path(__file__).with_name('ci-test-artifacts.py'))
a = importlib.util.module_from_spec(spec)
spec.loader.exec_module(a)

class Resources(unittest.TestCase):
    def fixture(self, root):
        directory = root / a.BuiltinTestArtifact.DIRECTORY
        directory.mkdir()
        modules = {}
        for module in a.BuiltinTestArtifact.MODULES:
            modules[module] = {}
            for kind, suffix in (('wasm', '.wasm'), ('metadata', '.metadata.json'), ('receipt', '.build-receipt.json')):
                path = directory / (module + suffix)
                path.write_bytes((module + suffix).encode())
                modules[module][kind] = dict(path=str(path.relative_to(root)), sha256=a.ProviderArtifact.digest(path))
        checker = directory / 'sync_builtin_wasm_identity'
        checker.write_text('fixture checker')
        checker.chmod(0o700)
        identity = dict(tested_sha='tested', source_sha='source', checkout_tree_sha256='tree')
        return identity, dict(identity, modules=modules, checker=dict(files=dict(checker=dict(path=str(checker.relative_to(root)), sha256=a.ProviderArtifact.digest(checker))), libraries=[]))

    def test_preflight_and_all_four_receipts(self):
        with tempfile.TemporaryDirectory() as t:
            root = Path(t)
            identity, record = self.fixture(root)
            with patch.object(a.ProviderArtifact, 'tree_digest', return_value='tree'), patch.object(a.subprocess, 'run') as run:
                a.BuiltinTestArtifact.validate(root, root, record, identity)
                self.assertEqual(run.call_count, 4)
                self.assertTrue(all(call.args[0][1] == '--verify-builtin-test-artifact' for call in run.call_args_list))
                for mutation in ('missing', 'wrong-source', 'tamper', 'symlink'):
                    import copy
                    changed = copy.deepcopy(record)
                    if mutation == 'wrong-source': changed['source_sha'] = 'other'
                    elif mutation == 'missing': changed['modules'].pop(next(iter(changed['modules'])))
                    else:
                        path = root / changed['modules']['m1.sensor.basic']['wasm']['path']
                        if mutation == 'tamper': path.write_bytes(b'tampered')
                        else:
                            path.unlink()
                            path.symlink_to(root / 'outside')
                    run.reset_mock()
                    with self.assertRaises(ValueError): a.BuiltinTestArtifact.validate(root, root, changed, identity)
                    run.assert_not_called()

    def test_runtime_selection_and_closure(self):
        manifest = dict(binaries=dict(runtime=dict(package='oasis7_runtime', test=True, features=['wasmtime', 'test_tier_full'])), included_binaries=['runtime'])
        self.assertTrue(a.BuiltinTestArtifact.required(manifest))
        manifest['binaries']['runtime']['features'] = ['wasmtime', 'test_tier_required']
        self.assertFalse(a.BuiltinTestArtifact.required(manifest))
        manifest['binaries']['runtime']['features'] = ['test_tier_full']
        self.assertFalse(a.BuiltinTestArtifact.required(manifest))
        manifest['included_binaries'] = []
        self.assertFalse(a.BuiltinTestArtifact.required(manifest))

if __name__ == '__main__': unittest.main()
