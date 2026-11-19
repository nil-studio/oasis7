import importlib.util
import hashlib
import json
import tempfile
import unittest
from contextlib import contextmanager
from pathlib import Path
from types import SimpleNamespace
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


class ProducerOrder(unittest.TestCase):
    @contextmanager
    def producer(self, base, *, requested=('test_tier_full', 'wasmtime'),
                 actual=('test_tier_full', 'wasmtime'), package='oasis7_runtime',
                 mismatched=False, tampered_after_native=False):
        root = base / 'checkout'
        root.mkdir()
        config = root / 'suites.json'
        config.write_text('{}')
        package_manifest = root / 'crates' / package / 'Cargo.toml'
        package_manifest.parent.mkdir(parents=True)
        package_manifest.write_text('[package]\nname="' + package + '"\n')
        executable = root / 'native-fixture'
        executable.write_bytes(b'native fixture')
        release_manifest = root / 'crates/oasis7_runtime/src/world/artifacts/m1_builtin_modules.sha256'
        release_manifest.parent.mkdir(parents=True, exist_ok=True)
        wasm = b'\0asm\x01\0\0\0'
        frozen_hash = hashlib.sha256(wasm).hexdigest()
        release_manifest.write_text(''.join(module + ' linux-x86_64=' + frozen_hash + '\n'
                                           for module in a.BuiltinTestArtifact.MODULES))
        bundle = base / 'bundle'
        events = []
        identity = dict(tested_sha='tested', source_sha='source', checkout_tree_sha256='tree')

        def prepare(command, **kwargs):
            if command[0] == 'bash':
                self.assertIn('--candidate-build', command)
                self.assertEqual(command[command.index('--profile') + 1], 'release')
                module = command[command.index('--module-id') + 1]
                events.append('wasm:' + module)
                directory = Path(command[command.index('--out-dir') + 1])
                (directory / (module + '.wasm')).write_bytes(wasm + (b'candidate' if mismatched else b''))
                for suffix in ('.metadata.json', '.build-receipt.json'):
                    (directory / (module + suffix)).write_text(json.dumps(dict(
                        purpose='candidate-build', profile='release', module_id=module)))
            elif '--verify-builtin-test-artifact' in command:
                events.append('verify:' + command[3])
            else:
                self.assertEqual(command[0], 'ldd')
            return SimpleNamespace(returncode=0, stdout='')

        def build_checker(root, bundle, directory):
            events.append('checker-build')
            path = bundle / directory / 'sync_builtin_wasm_identity'
            path.write_bytes(b'checker fixture')
            path.chmod(0o700)
            return {'checker': str(path.relative_to(bundle))}, []

        class CargoOutput:
            def __init__(self):
                self.stdout = iter([json.dumps(dict(reason='compiler-artifact', fresh=False,
                    manifest_path=str(package_manifest), executable=str(executable),
                    target=dict(name=package, kind=['lib']), profile=dict(test=True),
                    features=list(actual))) + '\n'])
            def __enter__(self): return self
            def __exit__(self, *args): pass
            def wait(self): return 0

        def native(command, **kwargs):
            self.assertEqual(command[0:2], ['cargo', 'test'])
            events.append('native')
            if tampered_after_native:
                (bundle / a.BuiltinTestArtifact.DIRECTORY / 'm1.sensor.basic.wasm').write_bytes(b'tampered')
            return CargoOutput()

        def output(command):
            if command[0] == 'git': return 'tested'
            if command == ['rustc', '--version']: return 'rustc fixture'
            self.assertEqual(command, ['rustc', '-vV'])
            return 'host: fixture'

        args = ['test', '-p', package, '--lib', '--no-default-features',
                '--features', ','.join(requested), '--no-run']
        with patch.object(a, 'ROOT', root), patch.object(a, 'CONFIG', config), \
             patch.object(a, 'FORMAL_AUTHORITY', None), \
             patch.object(a, 'build_variants', return_value=[('required', args, list(requested))]), \
             patch.object(a, 'output', side_effect=output), \
             patch.object(a, 'inventory', side_effect=lambda path, ignored=False, env=None: [] if ignored else ['actual_case']), \
             patch.object(a.ProviderArtifact, 'identity', return_value=identity), \
             patch.object(a.ProviderArtifact, 'tree_digest', return_value='tree'), \
             patch.object(a.ProviderArtifact, 'build_checker', side_effect=build_checker), \
             patch.object(a.subprocess, 'run', side_effect=prepare), \
             patch.object(a.subprocess, 'Popen', side_effect=native), \
             patch.dict(a.os.environ, {'SOURCE_SHA': 'source'}):
            yield bundle, events, frozen_hash

    def test_real_build_orchestration_rejects_hash_before_native_or_checker(self):
        with tempfile.TemporaryDirectory() as temporary, self.producer(Path(temporary), mismatched=True) as (bundle, events, frozen_hash):
            with self.assertRaises(ValueError) as raised:
                a.build(bundle, 'ordinary')
            self.assertEqual(events, ['wasm:m1.sensor.basic'])
            for detail in ('m1.sensor.basic', 'candidate-build', 'release', frozen_hash,
                           hashlib.sha256(b'\0asm\x01\0\0\0candidate').hexdigest()):
                self.assertIn(detail, str(raised.exception))
            self.assertFalse((bundle / 'manifest.json').exists())

    def test_nonconsumer_plans_do_not_prepare_resources(self):
        cases = [dict(package='unrelated'), dict(requested=('test_tier_required',), actual=('test_tier_required',)),
                 dict(requested=('test_tier_full',), actual=('test_tier_full',)),
                 dict(requested=('unrelated/test_tier_full', 'unrelated/wasmtime'), actual=('test_tier_required',))]
        for options in cases:
            with self.subTest(options=options), tempfile.TemporaryDirectory() as temporary, self.producer(Path(temporary), **options) as (bundle, events, _):
                a.build(bundle, 'ordinary')
                self.assertEqual(events, ['native'])
                self.assertNotIn('builtin_test_resources', json.loads((bundle / 'manifest.json').read_text()))

    def test_actual_inventory_revalidates_early_materials(self):
        with tempfile.TemporaryDirectory() as temporary, self.producer(Path(temporary), requested=('oasis7_runtime/test_tier_full', 'oasis7_runtime/wasmtime')) as (bundle, events, _):
            a.build(bundle, 'ordinary')
            modules = list(a.BuiltinTestArtifact.MODULES)
            verified = ['verify:' + module for module in modules]
            self.assertEqual(events, ['wasm:' + module for module in modules] + ['checker-build'] + verified + ['native'] + verified)
            manifest = json.loads((bundle / 'manifest.json').read_text())
            self.assertEqual(manifest['builtin_test_resources']['source_sha'], 'source')
            self.assertEqual(next(iter(manifest['binaries'].values()))['tests'], ['actual_case'])

    def test_unpredicted_actual_consumer_keeps_late_preparation(self):
        with tempfile.TemporaryDirectory() as temporary, self.producer(Path(temporary), requested=('test_tier_required',)) as (bundle, events, _):
            a.build(bundle, 'ordinary')
            self.assertEqual(events[0], 'native')
            self.assertEqual(events.count('wasm:m1.sensor.basic'), 1)
            self.assertIn('builtin_test_resources', json.loads((bundle / 'manifest.json').read_text()))

    def test_actual_nonconsumer_discards_early_materials(self):
        with tempfile.TemporaryDirectory() as temporary, self.producer(Path(temporary), actual=('test_tier_required',)) as (bundle, events, _):
            a.build(bundle, 'ordinary')
            self.assertEqual(events[-1], 'native')
            self.assertNotIn('builtin_test_resources', json.loads((bundle / 'manifest.json').read_text()))
            self.assertFalse((bundle / a.BuiltinTestArtifact.DIRECTORY).exists())

    def test_native_build_cannot_bypass_final_material_validation(self):
        with tempfile.TemporaryDirectory() as temporary, self.producer(Path(temporary), tampered_after_native=True) as (bundle, events, _):
            with self.assertRaisesRegex(ValueError, 'builtin artifact hash mismatch'):
                a.build(bundle, 'ordinary')
            self.assertIn('native', events)
            self.assertFalse((bundle / 'manifest.json').exists())


if __name__ == '__main__': unittest.main()
