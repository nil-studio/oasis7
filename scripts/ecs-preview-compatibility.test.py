#!/usr/bin/env python3
"""Exercise complete Git proofs and native viewer compatibility authorization."""
import copy
import hashlib
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from ecs_preview_compatibility import source_evidence, validate_source_evidence, evaluate_viewer


class CompatibilityTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.repo = self.root / "repo"
        self.repo.mkdir()
        self.git("init", "-q")
        self.git("config", "user.email", "fixture@example.invalid")
        self.git("config", "user.name", "Fixture")
        (self.repo / "crates").mkdir()
        (self.repo / "crates/render.rs").write_text("old pixels")
        (self.repo / "Cargo.lock").write_text("fixed dependencies")
        self.old = self.commit()
        (self.repo / "crates/render.rs").write_text("new pixels")
        self.new = self.commit()
        self.before = self.bundle("before", self.old)
        self.after = self.bundle("after", self.new)
        self.policy = {"schema_version": 1, "allowed_inputs": ["crates/render.rs"], "evidence": "independent audit: viewer reads external world only", "read_only_viewer": True}

    def git(self, *args):
        return subprocess.check_output(["git", *args], cwd=self.repo, stderr=subprocess.DEVNULL).decode().strip()

    def commit(self):
        self.git("add", ".")
        self.git("commit", "-qm", "Fixture source")
        return self.git("rev-parse", "HEAD")

    def bundle(self, name, source):
        root = self.root / name
        (root / "bin").mkdir(parents=True)
        for binary in ("oasis7_chain_runtime", "oasis7_game_launcher", "oasis7_game_client"):
            (root / "bin" / binary).write_bytes(binary.encode())
        (root / "manifest.json").write_text(json.dumps({"source": source}))
        data = json.dumps(source_evidence(self.repo, source)).encode()
        (root / "compatibility-inputs.json").write_bytes(data)
        (root / "SHA256SUMS").write_text(hashlib.sha256(data).hexdigest() + "  compatibility-inputs.json\n")
        return root

    def test_complete_real_git_commit_and_tree_proof(self):
        evidence = source_evidence(self.repo, self.old)
        self.assertEqual(set(validate_source_evidence(evidence, self.old)), {"Cargo.lock", "crates/render.rs"})
        for remove in (True, False):
            forged = copy.deepcopy(evidence)
            if remove:
                del forged["entries"]["Cargo.lock"]
            else:
                forged["entries"]["Cargo.lock"]["oid"] = "a" * 40
            with self.assertRaises(ValueError):
                validate_source_evidence(forged, self.old)

    def test_new_native_viewer_input_auto_approved_by_local_audit(self):
        self.assertTrue(evaluate_viewer(self.before, self.after, self.policy))

    def test_backend_bytes_change_rejected_even_with_same_source_proof(self):
        (self.after / "bin/oasis7_game_launcher").write_bytes(b"different backend")
        with self.assertRaisesRegex(ValueError, "unchanged"):
            evaluate_viewer(self.before, self.after, self.policy)

    def test_dependency_change_or_new_source_rejected(self):
        for name in ("Cargo.lock", "crates/new.rs"):
            (self.repo / name).write_text("changed")
            release = self.bundle(name.replace("/", "-"), self.commit())
            with self.assertRaises(ValueError):
                evaluate_viewer(self.before, release, self.policy)

    def test_remote_safe_flag_and_corrupt_proof_do_not_authorize(self):
        invalid = {**self.policy, "read_only_viewer": False}
        with self.assertRaises(ValueError):
            evaluate_viewer(self.before, self.after, invalid)
        (self.after / "compatibility-inputs.json").write_text("{}")
        with self.assertRaises(ValueError):
            evaluate_viewer(self.before, self.after, self.policy)


if __name__ == "__main__":
    unittest.main()
