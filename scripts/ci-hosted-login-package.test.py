#!/usr/bin/env python3
"""Run the real stage and assembly commands against deterministic package inputs."""

import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tarfile
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts/ci-hosted-login-package.py"
SOURCE = "a" * 40
NAMES = ("bin/oasis7_game_launcher_v3", "bin/oasis7_viewer_live")
BRIDGE = "pixel-world-bridge/webgl2/pixel_world_bridge_bindgen_bg.wasm"


class PackageTest(unittest.TestCase):
    def setUp(self):
        scratch = tempfile.TemporaryDirectory()
        self.addCleanup(scratch.cleanup)
        self.root = Path(scratch.name)
        self.native = self.root / "native"
        self.web = self.root / "web"
        (self.native / "bin").mkdir(parents=True)
        self.web.mkdir()
        required = subprocess.check_output(
            ["bash", "-ec", "source scripts/viewer-web-dist-contract.sh; viewer_web_dist_required_files"],
            cwd=ROOT, text=True,
        ).splitlines()
        for name in [*required, BRIDGE, "pixel-world-bridge/webgl2/pixel_world_bridge.js",
                     "pixel-world-bridge/webgl2/pixel_world_bridge_bindgen.js"]:
            path = self.web / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(b"\x00asm\x01\x00\x00\x00" if name.endswith(".wasm") else b"viewer")
        header = b"\x7fELF\x02\x01\x01" + b"\x00" * 9 + b"\x03\x00\x3e\x00" + b"\x01\x00\x00\x00"
        for name in NAMES:
            path = self.native / name
            # Use the host executable when exercising Linux CI.
            executable = Path("/usr/bin/true").read_bytes()
            path.write_bytes(executable if executable[:4] == b"\x7fELF" else header.ljust(64, b"\x00"))
            path.chmod(0o755)
        (self.native / "BUILDINFO").write_text(
            f"git_sha={SOURCE}\nprofile=release\ntarget=x86_64-unknown-linux-gnu\n"
            f"binary={NAMES[0]}\nviewer_live={NAMES[1]}\nweb=web/\n")
        (self.native / "SHA256SUMS").write_text("".join(
            f"{hashlib.sha256((self.native / name).read_bytes()).hexdigest()}  {name}\n" for name in NAMES))
        self.make_stages()

    def run_cli(self, *args):
        return subprocess.run([sys.executable, SCRIPT, *args], text=True, capture_output=True)

    def make_stages(self):
        subprocess.run(
            ["bash", "-ec", 'source scripts/viewer-web-dist-contract.sh; viewer_web_dist_write_manifest "$1" "$2"',
             "fixture", ROOT, self.web], check=True, cwd=ROOT,
        )
        for kind, source, arcname in (("native", self.native, "."), ("viewer", self.web, "web")):
            stage = self.root / f"hosted-{kind}"
            stage.mkdir(exist_ok=True)
            with tarfile.open(stage / "stage.tar.gz", "w:gz") as archive:
                archive.add(source, arcname)
            result = self.run_cli("seal", stage, kind, SOURCE)
            self.assertEqual(result.returncode, 0, result.stderr)

    def assemble(self, source=SOURCE):
        return self.run_cli("assemble", self.root, source, "release")

    def assert_rejected(self, expected, source=SOURCE):
        result = self.assemble(source)
        self.assertNotEqual(result.returncode, 0, result.stdout)
        self.assertIn(expected, result.stderr)

    def test_valid_same_source_package(self):
        result = self.assemble()
        self.assertEqual(result.returncode, 0, result.stderr)
        out = self.root / "hosted-login"
        self.assertEqual({p.name for p in out.iterdir()}, {"bin", "BUILDINFO", "SHA256SUMS", "web"})
        self.assertTrue((out / "web" / ".oasis7-viewer-dist-manifest.json").is_file())
        self.assertTrue((out / "web" / BRIDGE).is_file())

    def test_rejects_old_source(self):
        self.assert_rejected("source or archive hash mismatch", "b" * 40)

    def test_rejects_missing_adjacent_payload_even_with_valid_stage_receipt(self):
        (self.web / BRIDGE).unlink()
        self.make_stages()
        self.assert_rejected("Viewer output missing")

    def test_rejects_modified_archive(self):
        with (self.root / "hosted-native" / "stage.tar.gz").open("ab") as stream:
            stream.write(b"modified")
        self.assert_rejected("source or archive hash mismatch")

    def test_rejects_modified_binary_hash_even_with_valid_stage_receipt(self):
        (self.native / "SHA256SUMS").write_text("incorrect\n")
        self.make_stages()
        self.assert_rejected("native SHA256SUMS mismatch")

    def test_rejects_symlinks_before_extraction(self):
        target = self.web / "unexpected-link"
        target.symlink_to("../native/BUILDINFO")
        self.make_stages()
        self.assert_rejected("unexpected entry: web/unexpected-link")

    def test_rejects_invalid_receipt_schema(self):
        path = self.root / "hosted-viewer" / "stage.json"
        receipt = json.loads(path.read_text())
        receipt["schema"] = True
        path.write_text(json.dumps(receipt))
        self.assert_rejected("stage schema")


if __name__ == "__main__":
    unittest.main()
