#!/usr/bin/env python3
"""Compare image warming with real readers and execute a tiny Cargo producer."""
from collections import Counter
import importlib.util
import json
import os
from pathlib import Path
import shlex
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch


ROOT = Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location('image_cache', ROOT / 'scripts/ci-image-cache.py')
cache = importlib.util.module_from_spec(spec)
spec.loader.exec_module(cache)


def normalized(command, environment=None):
    command, environment = list(command), dict(environment or {})
    if command[0] == 'env':
        command.pop(0)
        while command and command[0] != 'cargo':
            item = command.pop(0)
            if item == '-u':
                command.pop(0)
            elif '=' in item:
                name, value = item.split('=', 1)
                environment[name] = value
            else:
                raise AssertionError('unexpected compiler environment: ' + item)
    if command[0] != 'cargo':
        return None
    if '--' in command:
        command = command[:command.index('--')]
    command = [arg for arg in command if arg not in ('--locked', '--no-run')]
    if command[1] == 'clippy':
        command[1] = 'check'
    return tuple(command), tuple(sorted(environment.items()))


class CompileContexts(unittest.TestCase):
    def consumer_commands(self, functions):
        # Run the real driver functions, replacing only the command boundary.
        # This catches changes to target/feature tuples in the actual readers.
        record = ('import json, os, sys; print(json.dumps([sys.argv[1:], '
                  '{k:os.environ[k] for k in ("CC_wasm32_unknown_unknown", '
                  '"AR_wasm32_unknown_unknown") if k in os.environ}]))')
        script = '\n'.join((
            'set -euo pipefail',
            'source ' + shlex.quote(str(ROOT / 'scripts/ci-tests.sh')),
            'repo_root=' + shlex.quote(str(ROOT)),
            'tier=full',
            'record() { ' + shlex.quote(sys.executable) + ' -c ' + shlex.quote(record) + ' "$@"; }',
            'run_cargo() { record cargo "$@"; }',
            'run_cargo_clippy() { record cargo clippy "$@"; }',
            'run() { record "$@"; }',
            *functions,
        ))
        env = {key: value for key, value in os.environ.items()
               if key not in ('CC_wasm32_unknown_unknown', 'AR_wasm32_unknown_unknown')}
        result = subprocess.run(['bash', '-c', script], cwd=ROOT,
            env=env | {'GITHUB_ACTIONS': 'true', 'RUNNER_OS': 'Linux'}, text=True, capture_output=True)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        return Counter(item for line in result.stdout.splitlines()
                       if line.startswith('[') and (item := normalized(*json.loads(line))) is not None)

    def test_native_node_and_net_warm_every_actual_reader_compile_tuple(self):
        readers = {
            'native': ['run_oasis7_consensus_clippy', 'run_oasis7_consensus_tests',
                       'run_oasis7_distfs_clippy', 'run_oasis7_distfs_tests'],
            'node': ['run_oasis7_node_clippy', 'run_oasis7_node_tests'],
            'net': ['run_oasis7_net_clippy', 'run_oasis7_net_libp2p_clippy',
                    'run_oasis7_net_tests', 'run_oasis7_net_libp2p_tests'],
        }
        for profile, functions in readers.items():
            with self.subTest(profile=profile):
                variants = cache.build_variants(ROOT, profile)
                self.assertEqual(Counter(normalized(command, env) for _, command, env in variants),
                                 self.consumer_commands(functions))
                for _, command, _ in variants:
                    self.assertIn('--locked', command)
                    if command[1] == 'test':
                        self.assertIn('--no-run', command)
                    self.assertNotIn('--', command, 'warming must not contain runtime test arguments')

    def test_launcher_warms_the_bin_and_default_features_selected_by_trunk(self):
        html = (ROOT / 'crates/oasis7_client_launcher/index.html').read_text()
        self.assertIn('<link data-trunk rel="rust" data-bin="oasis7_client_launcher" />', html)
        self.assertEqual(html.count('data-trunk rel="rust"'), 1)
        self.assertFalse((ROOT / 'crates/oasis7_client_launcher/Trunk.toml').exists())
        self.assertEqual(cache.build_variants(ROOT, 'launcher'), [
            ('launcher-wasm-release', ['cargo', 'build', '--locked', '-p', 'oasis7_client_launcher',
             '--bin', 'oasis7_client_launcher', '--release', '--target', 'wasm32-unknown-unknown'], {})])

    def test_support_warms_the_selected_image_profile_compile_tuples(self):
        plan = dict(scope='targeted', groups=['workspace_support'],
                    workspace_support_profile_version=1,
                    workspace_support_profiles=['oasis7_context', 'oasis7_wasm_executor'],
                    workspace_support_partition_version=1, workspace_support_partitions=['other'])
        commands = self.consumer_commands([
            'ci_python=' + shlex.quote(sys.executable),
            'driver_dir=' + shlex.quote(str(ROOT / 'scripts')),
            'support_profiles=' + shlex.quote('plan:' + json.dumps(plan)),
            'support_partition=other',
            'support_execution=image',
            'run_selected_workspace_support',
        ])
        variants = cache.build_variants(ROOT, 'support')
        self.assertEqual(Counter(normalized(command, env) for _, command, env in variants), commands)
        for _, command, _ in variants:
            self.assertIn('--locked', command)
            self.assertIn('--no-run', command)

    def test_unknown_profile_is_rejected_before_any_command(self):
        for profile in ('', 'all', 'native;echo unsafe', 'Native'):
            with self.subTest(profile=profile), patch.object(cache.subprocess, 'run') as execute:
                with self.assertRaisesRegex(ValueError, 'unknown image cache'):
                    cache.build(ROOT, profile)
                execute.assert_not_called()


class RealCargoWarming(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='image-cache-contract-')
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        (self.root / 'src').mkdir()
        (self.root / 'Cargo.toml').write_text(
            '[package]\nname="oasis7_node"\nversion="0.1.0"\nedition="2024"\n')
        (self.root / 'src/lib.rs').write_text(
            '#[test]\nfn must_not_execute() { panic!("cache warming executed a test"); }\n')
        self.env = dict(os.environ, CARGO_NET_OFFLINE='true')
        self.env.pop('RUSTC_WRAPPER', None)
        self.command(['cargo', 'generate-lockfile', '--offline'])

    def command(self, args, **kwargs):
        result = subprocess.run(args, cwd=self.root, env=self.env, text=True, capture_output=True, **kwargs)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        return result

    def warm(self, profile):
        # An unusable inherited wrapper must not enter canonical warming.
        return subprocess.run([sys.executable, str(ROOT / 'scripts/ci-image-cache.py'),
                               '--profile', profile, '--repo-root', str(self.root)], cwd=self.root,
            env=self.env | {'RUSTC_WRAPPER': str(self.root / 'must-not-run-wrapper')},
            text=True, capture_output=True)

    def test_real_check_and_test_build_succeed_without_executing_the_test(self):
        result = self.warm('node')
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn('image_cache_complete profile=node variants=2 verification=false', result.stdout)
        self.assertTrue(list((self.root / 'target/debug/deps').glob('oasis7_node-*')))
        self.assertNotIn('running 1 test', result.stdout + result.stderr)

    def test_second_compile_failure_cannot_report_a_complete_cache(self):
        (self.root / 'src/lib.rs').write_text('#[cfg(test)] compile_error!("test build must fail");\n')
        result = self.warm('node')
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('image_cache_variant=node-check', result.stdout)
        self.assertIn('image_cache_variant=node-test', result.stdout)
        self.assertIn('test build must fail', result.stderr)
        self.assertNotIn('image_cache_complete', result.stdout)

    def test_tooling_resolves_real_tracked_standalone_metadata_without_compiling(self):
        tools = self.root / 'tools/example'
        (tools / 'src').mkdir(parents=True)
        (tools / 'Cargo.toml').write_text(
            '[package]\nname="example"\nversion="0.1.0"\nedition="2024"\n[workspace]\n')
        (tools / 'src/lib.rs').write_text('compile_error!("metadata must not compile this crate");\n')
        self.command(['cargo', 'generate-lockfile', '--offline', '--manifest-path', str(tools / 'Cargo.toml')])
        (self.root / 'scripts').mkdir()
        (self.root / 'scripts/check-standalone-tool-lockfiles.sh').write_bytes(
            (ROOT / 'scripts/check-standalone-tool-lockfiles.sh').read_bytes())
        self.command(['git', 'init', '-q', '-b', 'main'])
        self.command(['git', 'add', 'tools'])
        result = self.warm('tooling')
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn('manifest-consistent (1 manifests)', result.stdout)
        self.assertIn('image_cache_complete profile=tooling variants=1 verification=false', result.stdout)
        self.assertFalse((self.root / 'target').exists())
        self.assertFalse((tools / 'target').exists())


if __name__ == '__main__':
    unittest.main()
