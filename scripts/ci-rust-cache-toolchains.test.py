#!/usr/bin/env python3
"""Exercise the real preparation CLI with an isolated Rustup process fixture."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parent.parent
SCRIPT = ROOT / 'scripts/ci-rust-cache-toolchains.py'
STABLE = '1.96.0'
NIGHTLY = 'nightly-2025-12-11'
HOST = 'x86_64-unknown-linux-gnu'


class ToolchainTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        self.state = self.root / 'state.json'
        self.log = self.root / 'calls.jsonl'
        self.initial = {
            'installed': [STABLE + '-' + HOST, NIGHTLY + '-' + HOST, '1.99.1-' + HOST, 'stable-' + HOST],
            'active': STABLE + '-' + HOST,
            'host': HOST,
        }
        self.state.write_text(json.dumps(self.initial))
        fixture = self.root / 'rustup'
        fixture.write_text(f'#!{sys.executable}\n' + r'''
import json, os, pathlib, sys
state_path = pathlib.Path(os.environ['TEST_RUSTUP_STATE'])
state = json.loads(state_path.read_text())
args = sys.argv[1:]
with open(os.environ['TEST_RUSTUP_LOG'], 'a') as log:
    log.write(json.dumps(args) + '\n')
if args == ['toolchain', 'list']:
    for name in state['installed']:
        print(name + (' (active, default)' if name == state['active'] else ''))
elif args == ['show', 'active-toolchain']:
    print(state['active'] + ' (overridden by rust-toolchain.toml)')
elif args[:1] == ['run'] and args[2:] == ['rustc', '-vV']:
    pin = args[1]
    if pin + '-' + state['host'] not in state['installed']:
        raise SystemExit(3)
    release = '1.94.0-nightly' if pin.startswith('nightly-') else pin
    print('rustc ' + release + '\nhost: ' + state['host'] + '\nrelease: ' + state.get('release', release))
elif args[:2] == ['toolchain', 'uninstall']:
    if state.get('uninstall_failure'):
        raise SystemExit(7)
    if not state.get('uninstall_noop'):
        state['installed'].remove(args[2])
    state_path.write_text(json.dumps(state))
else:
    raise SystemExit(99)
''')
        fixture.chmod(0o755)
        self.env = dict(os.environ, PATH=str(self.root) + os.pathsep + os.environ['PATH'],
                        TEST_RUSTUP_STATE=str(self.state), TEST_RUSTUP_LOG=str(self.log),
                        GITHUB_ACTIONS='true', RUNNER_ENVIRONMENT='github-hosted', RUNNER_OS='Linux')

    def run_script(self, pins=(STABLE, NIGHTLY)):
        return subprocess.run([sys.executable, str(SCRIPT), *pins], env=self.env,
                              text=True, capture_output=True)

    def calls(self):
        return [json.loads(line) for line in self.log.read_text().splitlines()] if self.log.exists() else []

    def assert_no_uninstall(self):
        self.assertFalse(any(call[:2] == ['toolchain', 'uninstall'] for call in self.calls()))

    def change_state(self, **changes):
        self.state.write_text(json.dumps(dict(self.initial, **changes)))

    def test_removes_preinstalled_drift_keeps_both_required_and_is_idempotent(self):
        result = self.run_script()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(set(json.loads(self.state.read_text())['installed']),
                         {STABLE + '-' + HOST, NIGHTLY + '-' + HOST})
        self.assertIn('rust_cache_toolchains=', result.stdout)
        self.log.unlink()
        self.assertEqual(self.run_script().returncode, 0)
        self.assert_no_uninstall()

    def test_web_keeps_only_its_stable_toolchain(self):
        self.assertEqual(self.run_script((STABLE,)).returncode, 0)
        self.assertEqual(json.loads(self.state.read_text())['installed'], [STABLE + '-' + HOST])

    def test_rejects_local_self_hosted_and_non_linux_without_rustup_calls(self):
        for key, value in [('GITHUB_ACTIONS', 'false'), ('RUNNER_ENVIRONMENT', 'self-hosted'), ('RUNNER_OS', 'macOS')]:
            with self.subTest(key=key):
                original = self.env[key]
                self.env[key] = value
                self.assertNotEqual(self.run_script().returncode, 0)
                self.assertEqual(self.calls(), [])
                self.env[key] = original

    def test_rejects_unpinned_or_duplicate_inputs_before_rustup_calls(self):
        for pins in [('stable',), ('nightly',), (STABLE, STABLE), (STABLE + ';false',)]:
            with self.subTest(pins=pins):
                self.assertNotEqual(self.run_script(pins).returncode, 0)
                self.assertEqual(self.calls(), [])

    def test_missing_required_compiler_never_uninstalls_existing_compilers(self):
        self.change_state(installed=[STABLE + '-' + HOST, '1.99.1-' + HOST])
        self.assertNotEqual(self.run_script().returncode, 0)
        self.assert_no_uninstall()

    def test_wrong_stable_release_or_active_compiler_never_uninstalls(self):
        for changes in [{'release': '1.99.1'}, {'active': 'stable-' + HOST}]:
            with self.subTest(changes=changes):
                self.change_state(**changes)
                self.assertNotEqual(self.run_script().returncode, 0)
                self.assert_no_uninstall()

    def test_uninstall_failure_or_unexpected_final_inventory_fails(self):
        for changes in [{'uninstall_failure': True}, {'uninstall_noop': True}]:
            with self.subTest(changes=changes):
                self.change_state(**changes)
                self.assertNotEqual(self.run_script().returncode, 0)

    def test_host_suffix_is_derived_without_assuming_x64(self):
        host = 'aarch64-unknown-linux-gnu'
        self.change_state(host=host, active=STABLE + '-' + host,
                          installed=[STABLE + '-' + host, NIGHTLY + '-' + host, 'stable-' + host])
        self.assertEqual(self.run_script().returncode, 0)
        self.assertEqual(set(json.loads(self.state.read_text())['installed']),
                         {STABLE + '-' + host, NIGHTLY + '-' + host})


if __name__ == '__main__':
    unittest.main()
