#!/usr/bin/env python3
import importlib.util
from pathlib import Path
import subprocess
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

    def test_matching_symlinks_or_missing_material_do_not_admit(self):
        p = self.root / M.TOOL_INPUTS[0]
        p.unlink()
        p.symlink_to("elsewhere")
        revision = self.commit()
        self.assertFalse(M.compatible(self.root, revision, revision))
        p.unlink()
        revision = self.commit()
        self.assertFalse(M.compatible(self.root, revision, revision))


if __name__ == "__main__":
    unittest.main()
