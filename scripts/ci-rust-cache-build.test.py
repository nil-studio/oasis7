#!/usr/bin/env python3
"""Exercise the shared cache producer's actual build-only shell entrypoint."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parent.parent


class CacheBuildTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        self.log = self.root / 'cargo.jsonl'
        cargo = self.root / 'cargo'
        cargo.write_text(f'#!{sys.executable}\n' + r'''
import json, os, pathlib, sys
log = pathlib.Path(os.environ['TEST_CARGO_LOG'])
number = len(log.read_text().splitlines()) + 1 if log.exists() else 1
with log.open('a') as stream:
    stream.write(json.dumps({'args': sys.argv[1:], 'wrapper': os.environ.get('RUSTC_WRAPPER'), 'cwd': os.getcwd()}) + '\n')
if str(number) == os.environ.get('TEST_FAIL_CALL'):
    raise SystemExit(23)
''')
        cargo.chmod(0o755)
        self.env = dict(os.environ, PATH=str(self.root) + os.pathsep + os.environ['PATH'],
                        CI='true', TEST_CARGO_LOG=str(self.log), RUSTC_WRAPPER='must-be-unset')

    def build(self):
        return subprocess.run(['bash', str(ROOT / 'scripts/ci-tests.sh'), 'cache-native',
                               '--repo-root', str(self.root)], env=self.env, text=True, capture_output=True)

    def calls(self):
        return [json.loads(line) for line in self.log.read_text().splitlines()]

    def test_exact_consumer_variants_are_compiled_without_executing_tests(self):
        result = self.build()
        self.assertEqual(result.returncode, 0, result.stderr)
        calls = self.calls()
        self.assertEqual(len(calls), 8)
        args = [call['args'] for call in calls]
        for call in calls:
            self.assertIsNone(call['wrapper'])
            self.assertEqual(call['cwd'], str(self.root))
            self.assertIn('--locked', call['args'])
            self.assertNotIn('--all-features', call['args'])
            if call['args'][0] == 'test':
                self.assertIn('--no-run', call['args'])
        # Downloading an ordinary test bundle cannot substitute for these real
        # target builds. Required, default snapshot and plain wasmtime differ.
        self.assertEqual(args[0], ['build', '-p', 'oasis7_viewer_runtime', '--bin', 'oasis7_viewer_live',
                                   '--features', 'test_tier_required', '--locked'])
        self.assertEqual(args[1][args[1].index('--features') + 1], 'test_tier_required')
        self.assertIn('--tests', args[1])
        for command, package in zip(args[2:4], ('oasis7', 'oasis7_viewer_runtime')):
            self.assertEqual(command, ['test', '-p', package, '--lib', '--no-run', '--locked'])
        support = args[4]
        self.assertIn('--lib', support)
        self.assertIn('--bins', support)
        self.assertEqual(support[support.index('--features') + 1], 'wasmtime')
        required_packages = {args[1][i + 1] for i, arg in enumerate(args[1]) if arg == '-p'}
        support_packages = {support[i + 1] for i, arg in enumerate(support) if arg == '-p'}
        self.assertEqual(support_packages, required_packages)
        self.assertEqual(args[5], ['build', '--locked', '-p', 'oasis7_chain_runtime', '--bin', 'oasis7_chain_runtime'])
        self.assertEqual(args[6], ['build', '--locked', '-p', 'oasis7_distfs', '--bin', 'hydrate_builtin_wasm'])
        self.assertEqual(args[7], ['build', '--locked', '-p', 'oasis7_distfs', '--features',
                                   'sync-builtin-wasm-identity', '--bin', 'sync_builtin_wasm_identity'])

    def test_compile_failure_stops_remaining_cache_population(self):
        for number in (1, 5, 7):
            with self.subTest(number=number):
                self.log.unlink(missing_ok=True)
                self.env['TEST_FAIL_CALL'] = str(number)
                result = self.build()
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(len(self.calls()), number)


if __name__ == '__main__':
    unittest.main()
