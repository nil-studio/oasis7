#!/usr/bin/env bash
set -euo pipefail
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
PYTHON_BIN="$("$ROOT_DIR/scripts/find-python-with-module.sh" json)"
"$PYTHON_BIN" - "$ROOT_DIR" <<'PY'
"""Real bounded Cargo workspace parsing without product dependency resolution."""
import json
import sys
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(sys.argv.pop()).resolve()
HELPER = ROOT / 'docker/wasm-builder/prepare-workspace.sh'
PRODUCERS = ('crates/oasis7_wasm_build', 'tools/wasm_build_suite')
VENDORS = ('vendor-if-watch-3.2.2', 'vendor-libp2p-swarm-0.48.0', 'vendor-libp2p-rendezvous-0.18.1')

class Workspace(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='oasis7-5548-metadata-')
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.write('Cargo.toml', '[workspace]\nmembers=["crates/oasis7_wasm_build","tools/wasm_build_suite","crates/business"]\nresolver="2"\n')
        self.write(PRODUCERS[0] + '/Cargo.toml', '[package]\nname="oasis7_wasm_build"\nversion="0.1.0"\nedition="2021"\n')
        self.write(PRODUCERS[0] + '/src/lib.rs', 'pub fn real_value() -> u8 { 7 }\n')
        self.write(PRODUCERS[1] + '/Cargo.toml', '[package]\nname="wasm_build_suite"\nversion="0.1.0"\nedition="2021"\n[dependencies]\noasis7_wasm_build={path="../../crates/oasis7_wasm_build"}\n')
        self.write(PRODUCERS[1] + '/src/main.rs', 'fn main() { assert_eq!(oasis7_wasm_build::real_value(),7); }\n')
        self.write(PRODUCERS[1] + '/templates/minimal_module/Cargo.toml', '[package]\nname="real_template"\nversion="0.1.0"\n[workspace]\n')
        self.write(PRODUCERS[1] + '/templates/minimal_module/src/lib.rs', 'pub fn actual_template() {}\n')
        self.write('crates/business/Cargo.toml', '[package]\nname="business"\nversion="0.1.0"\nedition="2021"\n')
        for vendor in VENDORS:
            self.write(vendor + '/Cargo.toml', '[package]\nname="' + vendor + '"\nversion="0.1.0"\n')
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
    def prepare(self):
        return subprocess.run(['bash', str(HELPER), str(self.root)], capture_output=True, text=True)
    def producer_bytes(self):
        return {str(path.relative_to(self.root)): path.read_bytes() for package in PRODUCERS for path in (self.root / package).rglob('*') if path.is_file()}
    def test_original_workspace_needs_metadata_targets_and_locked_producer_build_keeps_real_sources(self):
        args = ('metadata', '--no-deps', '--format-version', '1', '--manifest-path', PRODUCERS[1] + '/Cargo.toml')
        self.assertNotEqual(self.cargo(*args).returncode, 0)
        real = self.producer_bytes()
        manifest = (self.root / 'Cargo.toml').read_bytes()
        prepared = self.prepare()
        self.assertEqual(prepared.returncode, 0, prepared.stderr)
        self.assertEqual(self.producer_bytes(), real)
        self.assertEqual((self.root / 'Cargo.toml').read_bytes(), manifest)
        generated = self.cargo('generate-lockfile')
        self.assertEqual(generated.returncode, 0, generated.stderr)
        lock = (self.root / 'Cargo.lock').read_bytes()
        metadata = self.cargo(*args, '--locked')
        self.assertEqual(metadata.returncode, 0, metadata.stderr)
        packages = json.loads(metadata.stdout)['packages']
        self.assertEqual({p['name'] for p in packages}, {'business', 'oasis7_wasm_build', 'wasm_build_suite'})
        built = self.cargo('build', '--locked', '--manifest-path', PRODUCERS[1] + '/Cargo.toml')
        self.assertEqual(built.returncode, 0, built.stderr)
        self.assertNotIn('Compiling business', built.stderr)
        self.assertEqual((self.root / 'Cargo.lock').read_bytes(), lock)
        for vendor in VENDORS:
            target = self.root / vendor / 'src/lib.rs'
            self.assertIn('Metadata-only', target.read_text())
            self.assertEqual(target.stat().st_mode & 0o777, 0o644)
    def test_nonproducer_business_source_is_not_silently_overwritten(self):
        self.write('crates/business/src/lib.rs', 'compile_error!("real business source");\n')
        before = (self.root / 'crates/business/src/lib.rs').read_bytes()
        self.assertNotEqual(self.prepare().returncode, 0)
        self.assertEqual((self.root / 'crates/business/src/lib.rs').read_bytes(), before)
    def test_links_and_missing_actual_producer_are_rejected(self):
        source = self.root / PRODUCERS[0] / 'src/lib.rs'
        source.unlink()
        self.assertNotEqual(self.prepare().returncode, 0)
        self.write(PRODUCERS[0] + '/src/lib.rs', 'pub fn real_value() -> u8 {7}\n')
        (self.root / 'crates/business/src').symlink_to(self.root / PRODUCERS[0] / 'src', target_is_directory=True)
        before = self.producer_bytes()
        self.assertNotEqual(self.prepare().returncode, 0)
        self.assertEqual(self.producer_bytes(), before)
    def test_context_and_install_lane_do_not_reintroduce_business_source_or_toolchain_copyup(self):
        ignore = (ROOT / '.dockerignore').read_text().splitlines()
        self.assertNotIn('!crates/**', ignore)
        self.assertIn('crates/*/*', ignore)
        self.assertIn('!crates/*/Cargo.toml', ignore)
        self.assertIn('!crates/oasis7_wasm_build/**', ignore)
        self.assertIn('!tools/wasm_build_suite/**', ignore)
        dockerfile = (ROOT / 'docker/wasm-builder/Dockerfile').read_text()
        final = dockerfile.split('RUN bash ')[1].split('\n\n')[0]
        self.assertIn('CARGO_TARGET_DIR=/tmp/oasis7-producer-target', final)
        self.assertIn('rm -rf /tmp/oasis7-producer-target /cargo/registry /cargo/git', final)
        self.assertNotIn('chmod -R', final)
        self.assertTrue(all('--chmod=0755' in line for line in dockerfile.splitlines() if line.startswith('COPY ')))

if __name__ == '__main__':
    unittest.main(verbosity=2)

PY
