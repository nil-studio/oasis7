#!/usr/bin/env python3
import importlib.util
from pathlib import Path
import subprocess
import re
import tempfile
import unittest

SPEC = importlib.util.spec_from_file_location("release_web", Path(__file__).with_name("ci-release-web-image.py"))
M = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(M)


class SourceCompatibility(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.git("init", "-q")
        self.git("config", "user.name", "Fixture")
        self.git("config", "user.email", "fixture@example.invalid")
        for name in M.TOOL_INPUTS:
            p = self.root / name
            p.parent.mkdir(parents=True, exist_ok=True)
            p.write_text("original\n")
        workflow = self.root / ".github/workflows/rust.yml"
        workflow.parent.mkdir(parents=True)
        workflow.write_text("env:\n  RUST_TOOLCHAIN: 1.96.0\n  OASIS7_WASM_TOOLCHAIN: nightly-2025-12-11\n  OASIS7_WASM_TARGET: wasm32-unknown-unknown\n")
        self.base = self.commit()

    def git(self, *args):
        return subprocess.check_output(["git", "-C", str(self.root), *args], text=True).strip()

    def commit(self):
        self.git("add", "-A")
        self.git("commit", "-qm", "fixture")
        return self.git("rev-parse", "HEAD")

    def test_same_tool_closure_across_product_source_changes(self):
        (self.root / "product.txt").write_text("new product\n")
        self.assertTrue(M.compatible(self.root, self.base, self.commit()))

    def test_each_tool_change_and_missing_source_reject(self):
        for name in M.TOOL_INPUTS:
            with self.subTest(name=name):
                p = self.root / name
                p.write_text("changed\n")
                self.assertFalse(M.compatible(self.root, self.base, self.commit()))
                p.write_text("original\n")
        self.assertFalse(M.compatible(self.root, self.base, "0" * 40))
        self.assertFalse(M.compatible(self.root, self.base, "HEAD"))

    def test_compiler_changes_reject_but_unrelated_workflow_changes_do_not(self):
        p = self.root / ".github/workflows/rust.yml"
        original = p.read_text()
        p.write_text(original + "jobs: {}\n")
        self.assertTrue(M.compatible(self.root, self.base, self.commit()))
        p.write_text(original.replace("1.96.0", "1.97.0"))
        self.assertFalse(M.compatible(self.root, self.base, self.commit()))
        p.write_text(original + "  RUST_TOOLCHAIN: 1.96.0\n")
        self.assertFalse(M.compatible(self.root, self.base, self.commit()))

    def test_matching_symlinks_or_missing_material_do_not_admit(self):
        p = self.root / M.TOOL_INPUTS[0]
        p.unlink()
        p.symlink_to("elsewhere")
        revision = self.commit()
        self.assertFalse(M.compatible(self.root, revision, revision))
        p.unlink()
        revision = self.commit()
        self.assertFalse(M.compatible(self.root, revision, revision))


class WorkflowRouting(unittest.TestCase):
    ROOT = Path(__file__).resolve().parent.parent
    IMAGE = "ghcr.io/nil-studio/oasis7/ci-rust-release-web@sha256:f47cf4a0ce572de9b871475aed4e38c35fc7e6e128dda93a7d96e50480e03c59"
    KEY = "ci-rust-release-web-f47cf4a0ce572de9b871475aed4e38c35fc7e6e128dda93a7d96e50480e03c59"

    @staticmethod
    def job(text, name):
        return re.search(r"(?ms)^  " + name + r":\n.*?(?=^  [A-Za-z0-9_-]+:\n|\Z)", text)[0]

    @classmethod
    def reader_contract(cls, job):
        header, body = job.split("    steps:\n", 1)
        assert "timeout-minutes: 25\n" in header
        assert "image: ${{ needs.plan.outputs.webImageCompatible == 'true' && '" + cls.IMAGE + "' || '' }}" in header
        assert "options: --user 1001:1001 --init\n" in header
        assert "volumes:" not in header and "privileged" not in header
        assert "RUST_RELEASE_WEB_IMAGE:" not in header  # No empty value may poison host fingerprints.
        assert 'ref: ${{ github.workflow_sha }}' in body
        assert 'ref: ${{ needs.plan.outputs.checkoutSha }}' in body
        assert 'test "$(git rev-parse HEAD)" = "$EXPECTED_SHA"' in body
        assert body.index('Verify the fixed Release Web image from trusted workflow tools') < body.index('Configure pinned mold')
        assert body.index('Configure pinned mold') < body.index('ref: ${{ needs.plan.outputs.checkoutSha }}')
        assert "if: env.RELEASE_WEB_IMAGE_COMPATIBLE != 'true' && (hashFiles" in body
        cache = body.split('      - name: Restore the matching Release Web image dependency cache\n', 1)[1].split('      - ', 1)[0]
        assert "if: env.RELEASE_WEB_IMAGE_COMPATIBLE == 'true'" in cache
        assert 'RUST_RELEASE_WEB_IMAGE: ' + cls.IMAGE in cache
        assert 'shared-key: ' + cls.KEY in cache
        assert 'save-if: false\n' in cache
        assert 'env-vars: CARGO CC CFLAGS CXX CMAKE RUST\n' in cache
        assert 'shared-key: release-packages-web-wasm-trusted-v3\n' in body
        assert 'xvfb-run -a ./scripts/release-gate.sh' in body and '--web-headed' in body

    def test_two_readers_keep_host_route_and_bind_readonly_image_cache(self):
        for filename in ('release-packages.yml', 'mainnet-packages.yml'):
            text = (self.ROOT / '.github/workflows' / filename).read_text()
            job = self.job(text, 'release_gate_web')
            with self.subTest(filename=filename):
                if self.IMAGE not in job:
                    header = job.split("    steps:", 1)[0]
                    self.assertNotIn('container:', header)
                    self.assertNotIn('RUST_RELEASE_WEB_IMAGE', job)
                    self.assertNotIn(self.KEY, job)
                    self.assertIn('timeout-minutes: 25\n', header)
                    self.assertIn('shared-key: release-packages-web-wasm-trusted-v3\n', job)
                    self.assertIn('rustup toolchain install', job)
                    self.assertIn('sudo apt-get update', job)
                    self.assertIn('xvfb-run -a ./scripts/release-gate.sh', job)
                    self.assertIn('--web-headed', job)
                    continue
                self.reader_contract(job)
                for bad in (job.replace("save-if: false", "save-if: true"),
                            job.replace("env.RELEASE_WEB_IMAGE_COMPATIBLE == 'true'", "always()"),
                            job.replace(self.KEY, 'release-packages-web-wasm-trusted-v3'),
                            job.replace("      RELEASE_WEB_IMAGE_COMPATIBLE:", "      RUST_RELEASE_WEB_IMAGE: injected\n      RELEASE_WEB_IMAGE_COMPATIBLE:")):
                    with self.assertRaises(AssertionError):
                        self.reader_contract(bad)

    def test_writer_keeps_original_budget_guard_and_one_image_family(self):
        text = (self.ROOT / '.github/workflows/rust-cache.yml').read_text()
        job = self.job(text, 'web-wasm-cache')
        self.assertIn('timeout-minutes: 22\n', job)
        if self.IMAGE not in job:
            self.assertNotIn('container:', job.split('    steps:', 1)[0])
            self.assertNotIn('release-web-image', job)
            self.assertIn('OASIS7_DEPENDENCY_CACHE_FAMILY: release-web\n', job)
            self.assertIn('shared-key: release-packages-web-wasm-trusted-v3\n', job)
            self.assertIn('rustup toolchain install', job)
            self.assertIn('ci-rust-cache-toolchains.py', job)
            self.assertIn('uses: ./.github/actions/setup-linux-mold', job)
            return
        self.assertIn('image: ' + self.IMAGE, job)
        self.assertIn('RUST_RELEASE_WEB_IMAGE: ' + self.IMAGE, job)
        self.assertIn("github.ref == 'refs/heads/main' && github.workflow_sha == github.sha", job)
        self.assertIn('OASIS7_DEPENDENCY_CACHE_FAMILY: release-web-image', job)
        self.assertIn('shared-key: ' + self.KEY, job)
        self.assertIn('env-vars: CARGO CC CFLAGS CXX CMAKE RUST\n', job)
        self.assertIn("steps.dependency-cache-tools.outputs.guard_ready == 'true'", job)
        self.assertIn('cache-on-failure: false\n', job)
        self.assertNotIn('rustup toolchain install', job)
        self.assertNotIn('ci-rust-cache-toolchains.py', job)
        self.assertNotIn('shared-key: release-packages-web-wasm-trusted-v3', job)
        self.assertNotIn('uses: ./.github/actions/setup-linux-mold', job)


if __name__ == "__main__":
    unittest.main()
