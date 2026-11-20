#!/usr/bin/env bash
set -euo pipefail
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
PYTHON_BIN="$("$ROOT_DIR/scripts/find-python-with-module.sh" json)"
"$PYTHON_BIN" - "$ROOT_DIR" <<'PY'
"""Verify the standalone docker/wasm-builder workspace builds without root Cargo files."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(sys.argv.pop()).resolve()
PRODUCERS = ('crates/oasis7_wasm_build', 'tools/wasm_build_suite', 'docker/wasm-builder')


class StandaloneBuilderWorkspace(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='oasis7-wasm-builder-ws-')
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.write('crates/oasis7_wasm_build/Cargo.toml', '[package]\nname="oasis7_wasm_build"\nversion="0.1.0"\nedition="2024"\n')
        self.write('crates/oasis7_wasm_build/src/lib.rs', 'pub fn real_value() -> u8 { 7 }\n')
        self.write(
            'tools/wasm_build_suite/Cargo.toml',
            '[package]\nname="wasm_build_suite"\nversion="0.1.0"\nedition="2024"\n[dependencies]\noasis7_wasm_build={path="../../crates/oasis7_wasm_build"}\n',
        )
        self.write('tools/wasm_build_suite/src/lib.rs', 'pub fn run() { assert_eq!(oasis7_wasm_build::real_value(), 7); }\n')
        self.write(
            'docker/wasm-builder/Cargo.toml',
            '[workspace]\n[package]\nname="oasis7_wasm_builder_launcher"\nversion="0.1.0"\nedition="2024"\n[[bin]]\nname="wasm_build_suite"\npath="src/main.rs"\n[dependencies]\nwasm_build_suite={path="../../tools/wasm_build_suite"}\n',
        )
        self.write('docker/wasm-builder/src/main.rs', 'fn main() { wasm_build_suite::run(); }\n')
        self.env = dict(os.environ)
        self.env.pop('RUSTC_WRAPPER', None)
        self.env.pop('CARGO_TARGET_DIR', None)
        self.env['CARGO_HOME'] = str(self.root / 'private-cargo-home')

    def write(self, name, text):
        path = self.root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text)

    def cargo(self, *args):
        return subprocess.run(['cargo', *args], cwd=self.root, env=self.env, capture_output=True, text=True)

    def test_standalone_builder_workspace_resolves_and_builds_without_root_cargo_files(self):
        manifest = 'docker/wasm-builder/Cargo.toml'
        self.assertFalse((self.root / 'Cargo.toml').exists())
        self.assertFalse((self.root / 'Cargo.lock').exists())
        generated = self.cargo('generate-lockfile', '--manifest-path', manifest)
        self.assertEqual(generated.returncode, 0, generated.stderr)
        lock = (self.root / 'docker/wasm-builder/Cargo.lock').read_bytes()
        metadata = self.cargo('metadata', '--locked', '--format-version', '1', '--manifest-path', manifest)
        self.assertEqual(metadata.returncode, 0, metadata.stderr)
        packages = json.loads(metadata.stdout)['packages']
        self.assertEqual(
            {p['name'] for p in packages},
            {'oasis7_wasm_build', 'wasm_build_suite', 'oasis7_wasm_builder_launcher'},
        )
        built = self.cargo('build', '--locked', '--manifest-path', manifest)
        self.assertEqual(built.returncode, 0, built.stderr)
        self.assertEqual((self.root / 'docker/wasm-builder/Cargo.lock').read_bytes(), lock)

    def test_context_and_install_lane_exclude_root_cargo_and_business_sources(self):
        ignore = (ROOT / '.dockerignore').read_text().splitlines()
        for forbidden in ('!Cargo.toml', '!Cargo.lock', '!crates/**', '!crates/*/Cargo.toml', '!tools/**', '!tools/*/Cargo.toml'):
            self.assertNotIn(forbidden, ignore)
        for required in ('!docker/wasm-builder/Cargo.toml', '!docker/wasm-builder/Cargo.lock', '!crates/oasis7_wasm_build/**', '!tools/wasm_build_suite/**'):
            self.assertIn(required, ignore)
        dockerfile = (ROOT / 'docker/wasm-builder/Dockerfile').read_text()
        final = dockerfile.split('RUN env -u RUSTC_WRAPPER ')[1].split('\n\n')[0]
        self.assertIn('CARGO_TARGET_DIR=/tmp/oasis7-producer-target', final)
        self.assertIn('cargo install --locked --path /opt/oasis7/docker/wasm-builder', final)
        self.assertIn('rm -rf /tmp/oasis7-producer-target /cargo/registry /cargo/git', final)
        self.assertNotIn('chmod -R', final)
        self.assertTrue(all('--chmod=0755' in line for line in dockerfile.splitlines() if line.startswith('COPY ')))


if __name__ == '__main__':
    unittest.main(verbosity=2)
PY
