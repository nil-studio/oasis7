#!/usr/bin/env python3
"""Pure in-memory unit tests (<0.5s, no git/cargo subprocesses) for scripts/ci_engine and launcher."""

from __future__ import annotations
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

HERE = Path(__file__).resolve().parent
if str(HERE) not in sys.path:
    sys.path.insert(0, str(HERE))

from ci_engine import (  # noqa: E402
    assign_lpt_shards, classify_item_bodies, compute_artifact_partitions, compute_inline_partitions,
    compute_support_partitions, derive_ordinary_packages, derive_required_wasm_packages,
    derive_support_profiles, eval_cfg_expr, extract_source_inputs, reconcile_cross_version_members,
    scan_rust_source_inputs, supports_universal_v2, tokenize_rust_source,
    validate_package_capabilities_against_workspace, validate_package_capabilities_static,
)


class RustSourceGraphTests(unittest.TestCase):
    def test_lexer_and_source_inputs(self):
        src = (
            '/* outer /* nested */ comment */\n'
            '#[path = "alt/sub.rs"] mod sub;\n'
            'mod normal;\n'
            'const RAW: &str = r#"mod fake; include!("nope")"#;\n'
            'const A: &str = include_str!("data.txt");\n'
            'include!(concat!(env!("OUT_DIR"), "/gen.rs"));\n'
            'include!(dynamic_macro!());\n'
            'struct S { a: u8 }\n'
            'fn f<\'a>(x: &\'a str) -> char { \'z\' }\n'
        )
        tokens, closing = tokenize_rust_source(src)
        self.assertTrue(closing)
        mods, includes, uncertain = extract_source_inputs(src)
        self.assertFalse(uncertain)
        self.assertEqual([(m["name"], m["path"], m["inline"]) for m in mods], [("sub", "alt/sub.rs", False), ("normal", None, False)])
        self.assertEqual([(c["macro"], c["kind"], c["path"]) for c in includes], [
            ("include_str", "literal", "data.txt"), ("include", "generated", "/gen.rs"), ("include", "unknown", None),
        ])
        bodies = classify_item_bodies(tokens, closing)
        self.assertEqual([(b["kind"], b["name"]) for b in bodies], [("type_body", "S"), ("body", "f")])

    def test_uncertain_paths_and_cfg_three_valued(self):
        _, _, uncertain_inline = extract_source_inputs('#[path = "x.rs"] mod inline_m { fn a() {} }')
        _, _, uncertain_cfg = extract_source_inputs('#[cfg_attr(unix, path = "u.rs")] mod u;')
        self.assertTrue(uncertain_inline)
        self.assertTrue(uncertain_cfg)
        toks, _ = tokenize_rust_source('all(feature = "alpha", not(target_arch = "wasm32"), test)')
        self.assertIs(eval_cfg_expr(toks, target_archs=frozenset({"x86_64"}), features=frozenset({"alpha"}), test=True), True)
        self.assertIs(eval_cfg_expr(toks, target_archs=frozenset({"wasm32"}), features=frozenset({"alpha"}), test=True), False)
        self.assertIs(eval_cfg_expr(toks, target_archs=frozenset(), features=frozenset({"alpha"}), test=True), None)

    def test_full_fidelity_scan_rust_source_inputs(self):
        src = '#[path = "mounted.rs"] mod m;\ne\u0301include_str!("skip.txt");\nstd::include!(concat!(env!("CARGO_MANIFEST_DIR"), "/frag.rs"));\nother::include_str!("raw.txt");\n'
        lits, calls, unc, opaque, data_lits = scan_rust_source_inputs(src, with_module_paths=True, with_module_safety=True)
        self.assertEqual((unc, opaque, "mounted.rs" in lits), (False, True, True))
        self.assertEqual([(c["kind"], c["path"]) for c in calls], [("manifest", "/frag.rs"), ("unknown", None)])
        self.assertEqual(data_lits, ("skip.txt", "CARGO_MANIFEST_DIR", "/frag.rs", "raw.txt"))
        with self.assertRaises(ValueError):
            scan_rust_source_inputs('const BAD: &str = "unterminated')


class ShardSchedulerTests(unittest.TestCase):
    def test_inline_artifact_lpt_and_support_partitions(self):
        self.assertEqual(compute_inline_partitions(["pkg_b", "pkg_a"], frozenset({"pkg_a", "pkg_b"})), ("pkg_a", "pkg_b"))
        self.assertEqual(compute_inline_partitions(["pkg_a"], frozenset({"pkg_a", "pkg_b"})), ("all",))
        self.assertTrue(supports_universal_v2({"pkg_anchor"}, anchor_packages={"pkg_anchor"}, harnesses=()))
        self.assertEqual(
            compute_artifact_partitions(["pkg_a"], compact_excluded_packages=frozenset(), dynamic_enabled=True, strip_controls=True),
            ("native-0", "native-1"),
        )
        self.assertEqual(
            compute_artifact_partitions(["pkg_heavy", "pkg_a"], compact_excluded_packages=frozenset({"pkg_heavy"}),
                                        dynamic_enabled=True, strip_controls=True),
            ("native-0", "native-1", "native-2", "native-3"),
        )
        self.assertEqual(assign_lpt_shards({"a": 10, "b": 6, "c": 5, "d": 4}, 2), [["a", "d"], ["b", "c"]])
        sp = compute_support_partitions(["p_b", "p_a"], {
            "p_a": {"partition": "custody", "requires_host_runner": True},
            "p_b": {"partition": "other", "requires_host_runner": False},
        })
        self.assertEqual(sp, {"partitions": {"custody": ["p_a"], "other": ["p_b"]}, "host_profiles": ["p_a"], "image_profiles": ["p_b"]})


class CapabilitySchemaTests(unittest.TestCase):
    def _sample(self):
        return {
            "alpha_pkg": {
                "ordinary_package": True, "inline_dedicated_partition": True,
                "full_tier_requires_host_docker": True, "host_docker_wasm_fixtures": ["builtin_test_modules"],
                "support_profile": {"profile_id": "alpha_pkg", "partition": "other", "requires_host_runner": True,
                                    "extra_features": [], "wasm_compile_feature_sets": []},
                "platform_cfg_assumptions": {"assumed_target_archs": ["x86_64"], "known_target_features": ["feat_a"]},
            },
            "beta_pkg": {"ordinary_package": False, "required_wasm_build": True,
                         "support_profile": {"profile_id": "beta_pkg", "partition": "custody", "requires_host_runner": False,
                                             "extra_features": ["f1"], "wasm_compile_feature_sets": [["f1"]]}},
        }

    def test_valid_dual_domain_and_projections(self):
        caps = self._sample()
        validate_package_capabilities_against_workspace(caps, {"alpha_pkg", "beta_pkg"}, package_features={"alpha_pkg": {"feat_a"}})
        self.assertEqual(derive_ordinary_packages(caps), ("alpha_pkg",))
        self.assertEqual(derive_required_wasm_packages(caps), ("beta_pkg",))
        self.assertEqual(sorted(derive_support_profiles(caps)), ["alpha_pkg", "beta_pkg"])
        self.assertEqual(reconcile_cross_version_members(caps, {"alpha_pkg", "gamma_pkg"}),
                         {"added": ("gamma_pkg",), "removed": ("beta_pkg",), "kept": ("alpha_pkg",)})

    def test_rejects_order_physical_and_orphan_violations(self):
        with self.assertRaises(ValueError):
            validate_package_capabilities_static({"z_pkg": {"ordinary_package": True}, "a_pkg": {"ordinary_package": True}})
        with self.assertRaises(ValueError):
            validate_package_capabilities_static({"a_pkg": {"ordinary_package": False, "inline_dedicated_partition": True}})
        with self.assertRaises(ValueError):
            validate_package_capabilities_static({"a_pkg": {"ordinary_package": True, "host_docker_wasm_fixtures": ["builtin_test_modules"]}})
        with self.assertRaises(ValueError):
            validate_package_capabilities_static({"a_pkg": {"ordinary_package": False}})
        with self.assertRaises(ValueError):
            validate_package_capabilities_against_workspace(self._sample(), {"alpha_pkg"})


class AuthorityLauncherTests(unittest.TestCase):
    def test_launcher_isolates_cwd_allows_sibling_and_rejects_symlink(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            auth, cwd = root / "auth", root / "cwd"
            auth.mkdir()
            cwd.mkdir()
            (auth / "run-ci-authority-python.py").write_bytes((HERE / "run-ci-authority-python.py").read_bytes())
            (auth / "helper_mod.py").write_text('TAG = "trusted-sibling"\n', encoding="utf-8")
            (cwd / "helper_mod.py").write_text('TAG = "untrusted-cwd"\n', encoding="utf-8")
            (auth / "entry.py").write_text("import helper_mod; print(helper_mod.TAG)\n", encoding="utf-8")
            env = {k: v for k, v in os.environ.items() if k != "PYTHONPATH"}
            ok = subprocess.run([sys.executable, "-P", str(auth / "run-ci-authority-python.py"), str(auth / "entry.py")],
                                cwd=cwd, env=env, text=True, capture_output=True)
            self.assertEqual(ok.returncode, 0, ok.stderr)
            self.assertEqual(ok.stdout.strip(), "trusted-sibling")
            (auth / "evil_link.py").symlink_to(cwd / "helper_mod.py")
            bad = subprocess.run([sys.executable, "-P", str(auth / "run-ci-authority-python.py"), str(auth / "entry.py")],
                                 cwd=cwd, env=env, text=True, capture_output=True)
            self.assertNotEqual(bad.returncode, 0)


if __name__ == "__main__":
    unittest.main()
