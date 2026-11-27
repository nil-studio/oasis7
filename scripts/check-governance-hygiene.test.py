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

    def test_fast_path_canary_accepts_healthy_fixture_and_catches_regressions(self) -> None:
        import hashlib

        planner = SCRIPTS_DIR / "plan-rust-required-scope.py"
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "scripts").mkdir()
            src = root / "crates/demo/src"
            src.mkdir(parents=True)
            build_rs = root / "crates/demo/build.rs"
            build_rs.write_text("fn main() {}\n", encoding="utf-8")
            anchor_rs = root / "crates/demo/src/anchor.rs"
            anchor_rs.write_text("pub fn anchor() -> u8 { 1 }\n", encoding="utf-8")
            caller_rs = root / "crates/demo/src/caller.rs"
            caller_rs.write_text("pub fn call() -> u8 { super::anchor::anchor() }\n", encoding="utf-8")
            (root / "crates/demo/Cargo.toml").write_text(
                '[package]\nname = "demo"\nversion = "0.1.0"\nedition = "2021"\n'
                '[features]\ndefault = []\ntest_tier_required = []\ntest_tier_full = ["test_tier_required"]\n',
                encoding="utf-8",
            )
            (src / "lib.rs").write_text(
                "pub mod anchor;\npub mod caller;\npub mod domain;\n",
                encoding="utf-8",
            )
            domain_rs = src / "domain.rs"
            domain_rs.write_text(
                'const META: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/meta.txt"));\n'
                "pub fn ok() -> usize { META.len() }\n",
                encoding="utf-8",
            )

            def sha(path: Path) -> str:
                return hashlib.sha256(path.read_bytes()).hexdigest()

            base_scope = {
                "generated_inputs": [
                    {
                        "package": "demo",
                        "producer": "crates/demo/build.rs",
                        "producer_sha256": sha(build_rs),
                        "outputs": ["meta.txt"],
                    }
                ],
                "ordinary_scale_contract": {
                    "anchors": {"crates/demo/src/anchor.rs": sha(anchor_rs)}
                },
                "ordinary_fast_support_contracts": [
                    {"package": "demo", "callers": {"crates/demo/src/caller.rs": sha(caller_rs)}}
                ],
                "ordinary_fast_targets": {
                    "demo-lib": {
                        "package": "demo",
                        "target_kind": "lib",
                        "target_name": "demo",
                        "default_features": False,
                        "features": ["test_tier_required"],
                        "cases": [],
                    }
                },
                "ordinary_fast_component_rules": [
                    {"match": ["crates/demo/src/**"], "exclude": [], "targets": ["demo-lib"]}
                ],
            }
            scope = root / "scripts/ci-required-scope.json"
            scope.write_text(json.dumps(base_scope), encoding="utf-8")

            self.assertEqual(scope_paths.check_ci_fast_path_health(root, scope, planner), [])

            # 1. Stale producer_sha256 anchor caught
            build_rs.write_text("fn main() { println!(\"changed\"); }\n", encoding="utf-8")
            errs = scope_paths.check_ci_fast_path_health(root, scope, planner)
            self.assertTrue(any("stale SHA-256 for crates/demo/build.rs" in e for e in errs), errs)
            build_rs.write_text("fn main() {}\n", encoding="utf-8")

            # 2. Unregistered OUT_DIR include_bytes! caught
            domain_rs.write_text(
                'const SECRET: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/unregistered.bin"));\n'
                "pub fn ok() -> usize { SECRET.len() }\n",
                encoding="utf-8",
            )
            errs = scope_paths.check_ci_fast_path_health(root, scope, planner)
            self.assertTrue(any("unregistered OUT_DIR" in e for e in errs), errs)

            # 3. Narrow-unproven test_tier_full gate on production file caught
            domain_rs.write_text(
                "pub fn ok() -> usize { 1 }\n"
                '#[cfg(all(test, feature = "test_tier_full"))]\n'
                "fn full_only_helper() -> usize { 2 }\n",
                encoding="utf-8",
            )
            errs = scope_paths.check_ci_fast_path_health(root, scope, planner)
            self.assertTrue(any("narrow-unproven #[cfg(...test_tier_full...)]" in e for e in errs), errs)

            # 4. Conditional #[cfg_attr(..., path = ...)] (including bracket inside string literal) caught
            domain_rs.write_text(
                '#[cfg_attr(all(test, doc = "a]b"), path = "anchor.rs")]\nmod sub;\npub fn ok() -> usize { 1 }\n',
                encoding="utf-8",
            )
            errs = scope_paths.check_ci_fast_path_health(root, scope, planner)
            self.assertTrue(any("uncertain module path attribute" in e for e in errs), errs)

            # 4b. Comment-separated include_bytes /* c */ ! OUT_DIR & >200-char doc-comment inline #[path] caught
            domain_rs.write_text(
                'const X: &[u8] = include_bytes /* c */ !(concat!(env!("OUT_DIR"), "/unregistered.bin"));\n'
                '# /* c */ [r#path = "sub"]\n'
                + ("/// " + ("x" * 220) + "\n")
                + "pub mod inline_mod {}\npub fn ok() -> usize { X.len() }\n",
                encoding="utf-8",
            )
            errs = scope_paths.check_ci_fast_path_health(root, scope, planner)
            self.assertTrue(any("unregistered OUT_DIR" in e for e in errs), errs)
            self.assertTrue(any("uncertain module path attribute" in e for e in errs), errs)

            # 5. Ambiguous foo.rs + foo/mod.rs caught
            domain_rs.write_text("pub fn ok() -> usize { 1 }\n", encoding="utf-8")
            (src / "domain").mkdir()
            (src / "domain/mod.rs").write_text("pub fn conflict() {}\n", encoding="utf-8")
            errs = scope_paths.check_ci_fast_path_health(root, scope, planner)
            self.assertTrue(any("ambiguous Rust module files" in e for e in errs), errs)


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
