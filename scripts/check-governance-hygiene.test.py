#!/usr/bin/env python3
"""Unit tests for check-rust-hygiene.py, check-ci-scope-paths.py, and check-doc-cargo-test-commands.py."""

from __future__ import annotations

import importlib.util
import json
import subprocess
import tempfile
import unittest
from pathlib import Path

SCRIPTS_DIR = Path(__file__).resolve().parent


def load_module(name: str, filename: str):
    spec = importlib.util.spec_from_file_location(name, SCRIPTS_DIR / filename)
    assert spec and spec.loader
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


rust_hygiene = load_module("check_rust_hygiene", "check-rust-hygiene.py")
scope_paths = load_module("check_ci_scope_paths", "check-ci-scope-paths.py")
doc_cargo = load_module("check_doc_cargo_test_commands", "check-doc-cargo-test-commands.py")


class RustHygieneTests(unittest.TestCase):
    def test_detects_orphan_and_accepts_mounted_and_included_files(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "Cargo.toml").write_text('[workspace]\nmembers = ["crates/demo"]\n', encoding="utf-8")
            src = root / "crates/demo/src"
            (src / "nested").mkdir(parents=True)
            (root / "crates/demo/Cargo.toml").write_text('[package]\nname = "demo"\n', encoding="utf-8")
            (src / "lib.rs").write_text('include!("native.rs");\n', encoding="utf-8")
            (src / "native.rs").write_text("mod active;\nmod nested {\n    mod leaf;\n}\n", encoding="utf-8")
            (src / "active.rs").write_text("pub fn ok() {}\n", encoding="utf-8")
            (src / "nested/leaf.rs").write_text("pub fn leaf() {}\n", encoding="utf-8")
            (src / "orphan.rs").write_text("pub fn forgotten() {}\n", encoding="utf-8")

            orphans = rust_hygiene.check_orphan_rust_files(root)
            self.assertEqual(orphans, ["crates/demo/src/orphan.rs"])

    def test_allow_dead_code_requires_reason_comment_on_new_lines(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            subprocess.run(["git", "init", "-q"], cwd=root, check=True)
            subprocess.run(["git", "config", "user.email", "test@example.com"], cwd=root, check=True)
            subprocess.run(["git", "config", "user.name", "Test"], cwd=root, check=True)
            src = root / "crates/demo/src"
            src.mkdir(parents=True)
            (src / "lib.rs").write_text("pub fn base() {}\n", encoding="utf-8")
            subprocess.run(["git", "add", "."], cwd=root, check=True)
            subprocess.run(["git", "commit", "-q", "-m", "base"], cwd=root, check=True)
            base_sha = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=root, text=True).strip()

            (src / "lib.rs").write_text(
                "pub fn base() {}\n"
                "// dark: #1234 pending integration\n"
                "#[allow(dead_code)]\n"
                "fn documented_dark() {}\n"
                "#[allow(dead_code)]\n"
                "fn undocumented() {}\n",
                encoding="utf-8",
            )
            subprocess.run(["git", "add", "."], cwd=root, check=True)
            subprocess.run(["git", "commit", "-q", "-m", "head"], cwd=root, check=True)
            head_sha = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=root, text=True).strip()

            violations = rust_hygiene.check_allow_dead_code_comments(root, base_sha, head_sha)
            self.assertEqual(len(violations), 1)
            self.assertIn("crates/demo/src/lib.rs:5", violations[0])


class ScopePathsTests(unittest.TestCase):
    def test_flags_missing_exact_path_and_skips_globs_and_reserved_prefixes(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "scripts").mkdir()
            (root / "scripts/exists.py").write_text("# ok\n", encoding="utf-8")
            scope = root / "scripts/ci-required-scope.json"
            scope.write_text(
                json.dumps(
                    {
                        "rules": [
                            "scripts/exists.py",
                            "scripts/glob-*.py",
                            "crates/missing/src/ghost.rs",
                        ],
                        "reserved_empty_dir_prefixes": ["crates/reserved/"],
                    }
                ),
                encoding="utf-8",
            )
            missing = scope_paths.find_missing_scope_paths(root, scope)
            self.assertEqual([p for _, p in missing], ["crates/missing/src/ghost.rs"])


class DocCargoTestCommandsTests(unittest.TestCase):
    def test_reports_missing_test_filter_and_accepts_existing_one(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "crates/demo/src").mkdir(parents=True)
            (root / "crates/demo/Cargo.toml").write_text('[package]\nname = "demo"\n', encoding="utf-8")
            (root / "crates/demo/src/lib.rs").write_text("fn live_case() {}\n", encoding="utf-8")
            doc = root / "doc/guide.md"
            doc.parent.mkdir(parents=True)
            doc.write_text(
                "- `cargo test -p demo live_case -- --nocapture`\n"
                "- `cargo test -p demo vanished_case -- --nocapture`\n",
                encoding="utf-8",
            )
            warnings = doc_cargo.check_docs(root, [doc])
            self.assertEqual(len(warnings), 1)
            self.assertIn("vanished_case", warnings[0])


if __name__ == "__main__":
    unittest.main()
