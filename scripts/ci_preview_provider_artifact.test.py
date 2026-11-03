#!/usr/bin/env python3
"""Pure orchestration contracts. Never invokes Cargo, Docker or the builder."""
import copy
import importlib.util
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location('provider', HERE / 'ci_preview_provider_artifact.py')
provider = importlib.util.module_from_spec(spec)
spec.loader.exec_module(provider)


class ProviderArtifact(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.bundle = self.root / 'bundle'
        (self.bundle / provider.DIRECTORY).mkdir(parents=True)
        self.expected = dict(tested_sha='a' * 40, source_sha='b' * 40)
        files = {}
        for kind, name in [('wasm',provider.MODULE+'.wasm'), ('metadata',provider.MODULE+'.metadata.json'),
                           ('receipt',provider.MODULE+'.build-receipt.json'), ('checker','sync_builtin_wasm_identity'+('.exe' if os.name=='nt' else ''))]:
            name = provider.DIRECTORY + '/' + name
            path = self.bundle / name
            path.write_bytes(b'synthetic contract data; not a valid canonical artifact')
            path.chmod(0o700 if kind == 'checker' else 0o600)
            files[kind] = dict(path=name, sha256=provider.digest(path))
        self.record = dict(self.expected, checkout_tree_sha256='tree',files=files,libraries=[])
        self.tree = patch.object(provider,'tree_digest',return_value='tree')
        self.tree.start()
        self.addCleanup(self.tree.stop)
        # An explicit mock of the official verifier, never synthetic provenance
        # accepted in production. This contract checks invocation and fail-closed
        # orchestration; real source/receipt validity is a separate Rust/build test.
        self.invoke = patch.object(provider.subprocess,'run')
        self.native = self.invoke.start()
        self.addCleanup(self.invoke.stop)

    def validate(self, record=None):
        return provider.validate(self.root,self.bundle,self.record if record is None else record,self.expected)

    def test_merge_dual_identity_and_exact_official_checker(self):
        environment = self.validate()
        self.assertEqual(set(environment),set(provider.VARIABLES))
        command = self.native.call_args.args[0]
        self.assertEqual(command[1:3], ['--verify-provider-artifact',str(self.root)])
        self.assertEqual(len(command),6)
        self.assertNotIn('cargo', command)
        self.assertNotEqual(self.record['tested_sha'],self.record['source_sha'])

    def test_missing_pair_hash_and_source_fail_closed_before_checker(self):
        mutations = [lambda r:r['files'].pop('wasm'),
                     lambda r:r.update(source_sha='c'*40),
                     lambda r:r.update(tested_sha='c'*40),
                     lambda r:r.update(checkout_tree_sha256='other-tree'),
                     lambda r:r['files']['metadata'].update(sha256='0'*64)]
        for mutate in mutations:
            record=copy.deepcopy(self.record)
            mutate(record)
            with self.subTest(record=record), self.assertRaises(ValueError): self.validate(record)
        self.native.assert_not_called()

    def test_bad_path_and_symlink_fail_closed(self):
        record=copy.deepcopy(self.record)
        record['files']['wasm']['path']='../foreign.wasm'
        with self.assertRaises(ValueError): self.validate(record)
        wasm=self.bundle/self.record['files']['wasm']['path']
        foreign=self.root/'foreign'
        foreign.write_bytes(wasm.read_bytes())
        wasm.unlink()
        wasm.symlink_to(foreign)
        with self.assertRaises(ValueError): self.validate()
        self.native.assert_not_called()

    def test_official_checker_failure_cannot_inject_environment(self):
        self.native.side_effect=subprocess.CalledProcessError(2,['official-checker'])
        with self.assertRaises(subprocess.CalledProcessError): self.validate()



if __name__=='__main__': unittest.main()
