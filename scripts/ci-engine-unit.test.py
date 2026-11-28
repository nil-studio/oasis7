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
            validate_package_capabilities_static({
                "a_pkg": {"ordinary_package": False, "support_profile": {
                    "profile_id": "other_id", "partition": "other", "requires_host_runner": False,
                    "extra_features": [], "wasm_compile_feature_sets": []}},
            })
        with self.assertRaises(ValueError):
            validate_package_capabilities_static({
                "a_pkg": {"ordinary_package": True, "group_library_proof": {"group": "g1"}},
                "b_pkg": {"ordinary_package": True, "group_library_proof": {"group": "g1"}},
            }, known_groups={"g1"}, package_groups={"a_pkg": ["g1"], "b_pkg": ["g1"]})
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


class ExecutorContractAndAstRatchetTests(unittest.TestCase):
    def _load_mod(self, name: str, filename: str):
        import importlib.util
        spec = importlib.util.spec_from_file_location(name, HERE / filename)
        mod = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(mod)
        return mod

    def test_artifacts_consumes_materialized_contract_and_capabilities(self):
        import json
        art = self._load_mod("_art_test", "ci-test-artifacts.py")
        with tempfile.TemporaryDirectory() as tmp:
            cfg_path = Path(tmp) / "scope.json"
            cfg_path.write_text(json.dumps({
                "package_capabilities": {
                    "pkg_a": {"ordinary_package": True},
                    "pkg_b": {"ordinary_package": False},
                },
            }), encoding="utf-8")
            self.assertEqual(art.load_oasis7_required_packages(cfg_path), ["pkg_a"])
            cfg_path.write_text(json.dumps({
                "package_capabilities": {"pkg_a": {"ordinary_package": True}},
                "oasis7_required_packages": ["pkg_other"],
            }), encoding="utf-8")
            with self.assertRaises(ValueError):
                art.load_oasis7_required_packages(cfg_path)
        contract_plan = {
            "ordinary_execution_contract": {
                "version": 1, "mode": "artifact-v2", "supports_universal_v2": True,
                "candidate_container_eligible": True, "artifact_partitions": ["native-0", "native-1"],
                "partitions": ["all"], "logical_harness_aliases": {"oasis7_synth": "synth-lib-tests"},
                "required_wasm_packages": [], "required_host_docker_wasm_fixtures": [],
                "deferred_cases": {}, "delegated_clippy": [],
            },
        }
        self.assertEqual(
            art.artifact_logical_name("required", "oasis7_synth", {"name": "oasis7_synth", "kind": ["lib"]}, True, plan=contract_plan),
            "synth-lib-tests",
        )
        manifest = {
            "selection": {"suite": "ordinary"},
            "execution_plan": contract_plan,
            "binaries": {
                "rt": {"package": "oasis7_runtime", "test": True, "target_kind": ["lib"], "features": ["wasmtime", "test_tier_full"]},
                "ch": {"package": "oasis7_chain_runtime", "test": True, "target_kind": ["lib"], "features": ["test_tier_full"]},
            },
        }
        self.assertFalse(art.BuiltinTestArtifact.required(manifest))
        self.assertFalse(art.preview_provider_required(manifest))
        with_fx = {
            **manifest,
            "execution_plan": {
                "ordinary_execution_contract": {
                    **contract_plan["ordinary_execution_contract"],
                    "candidate_container_eligible": False,
                    "required_host_docker_wasm_fixtures": ["builtin_test_modules", "provider_preview_wasm"],
                },
            },
        }
        self.assertTrue(art.BuiltinTestArtifact.required(with_fx))
        self.assertTrue(art.preview_provider_required(with_fx))
        for bad_override in (
            {"logical_harness_aliases": {"oasis7_synth": "../manifest.json"}},
            {"logical_harness_aliases": ["not-a-dict"]},
            {"logical_harness_aliases": {"a": "dup", "b": "dup"}},
            {"candidate_container_eligible": False, "required_host_docker_wasm_fixtures": ["unknown_fixture"]},
        ):
            bad_plan = {"ordinary_execution_contract": {**contract_plan["ordinary_execution_contract"], **bad_override}}
            with self.subTest(bad_override=bad_override), self.assertRaises(ValueError):
                art.resolve_ordinary_execution_contract(bad_plan)
        # Verify real planner scope=full pr-fast contract (candidate_container_eligible=True + fixtures list) validates cleanly.
        full_pr_fast = {
            "fast_plan_version": 1, "execution_profile": "pr-fast", "scope": "full",
            "groups": ["baseline", "rust_baseline", "oasis7_required"], "test_policy_changed": False,
            "oasis7_required_packages": ["oasis7_chain_runtime", "oasis7_runtime"],
            "oasis7_required_wasm_packages": [],
            "ordinary_artifact_partition_version": 2, "ordinary_artifact_delivery_version": 2,
            "ordinary_controls_in_rust_baseline_version": 1, "ordinary_clippy_group": "rust_baseline",
            "ordinary_wasm_group": "rust_baseline",
            "ordinary_partitions": ["native-0", "native-1", "native-2", "native-3"],
            "ordinary_execution_contract": {
                "version": 1, "mode": "artifact-v2", "supports_universal_v2": False,
                "candidate_container_eligible": True,
                "artifact_partitions": ["native-0", "native-1", "native-2", "native-3"],
                "partitions": ["native-0", "native-1", "native-2", "native-3"],
                "logical_harness_aliases": {"oasis7_chain_runtime": "runtime-tests"},
                "required_wasm_packages": [],
                "required_host_docker_wasm_fixtures": ["builtin_test_modules", "provider_preview_wasm"],
                "deferred_cases": {}, "delegated_clippy": [],
            },
        }
        self.assertIsNotNone(art.fast_plan(full_pr_fast))

    def test_ci_tests_consumes_support_contract_partitions_and_complete_features(self):
        import json
        import shutil
        driver = (HERE / "ci-tests.sh").read_text(encoding="utf-8")
        marker = '  run_group "$group"\n'
        stubbed = driver.replace(marker, '  run_cargo() { printf "cargo %s\\n" "$*"; }\n  run_cargo_clippy() { :; }\n' + marker)
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            scripts = root / "scripts"
            scripts.mkdir()
            for fname in ("ci-test-artifacts.py", "ci-required-scope.json", "find-python-with-module.sh", "viewer-dependency-preflight.sh"):
                shutil.copyfile(HERE / fname, scripts / fname)
            (scripts / "ci-tests.sh").write_text(stubbed, encoding="utf-8")
            (root / "Cargo.toml").write_text('[workspace]\nmembers = ["crates/*"]\n', encoding="utf-8")
            for pkg in ("oasis7_custody_core", "oasis7_proto"):
                pdir = root / "crates" / pkg
                pdir.mkdir(parents=True)
                (pdir / "Cargo.toml").write_text(
                    f'[package]\nname = "{pkg}"\nversion = "0.1.0"\n[features]\ntest_tier_required = []\nextra_policy_feat = []\n',
                    encoding="utf-8",
                )
            plan = {
                "groups": ["workspace_support"], "scope": "targeted", "execution_profile": "pr-fast",
                "fast_plan_version": 1,
                "test_policy_changed": True, "workspace_support_profile_version": 1,
                "workspace_support_profiles": ["oasis7_custody_core", "oasis7_proto"],
                "workspace_support_partition_version": 1, "workspace_support_partitions": ["custody", "other"],
                "workspace_support_execution_contract": {
                    "version": 1,
                    "selected_profiles": ["oasis7_custody_core", "oasis7_proto"],
                    "partitions": {"custody": ["oasis7_custody_core"], "other": ["oasis7_proto"]},
                    "host_profiles": [], "image_profiles": ["oasis7_custody_core", "oasis7_proto"],
                    "expanded_targets_by_profile": {},
                    "complete_features_by_profile": {"oasis7_proto": ["extra_policy_feat"]},
                    "wasm_compile_checks": {},
                },
            }
            pf = root / "support.json"
            pf.write_text(json.dumps(plan), encoding="utf-8")
            custody_res = subprocess.run(
                ["bash", str(scripts / "ci-tests.sh"), "required", "--group", "workspace_support",
                 "--support-plan-file", str(pf), "--support-partition", "custody", "--support-execution", "all"],
                cwd=root, text=True, capture_output=True,
            )
            self.assertEqual(custody_res.returncode, 0, custody_res.stderr)
            self.assertIn("+ support profile: oasis7_custody_core", custody_res.stdout)
            self.assertNotIn("+ support profile: oasis7_proto", custody_res.stdout)
            other_res = subprocess.run(
                ["bash", str(scripts / "ci-tests.sh"), "required", "--group", "workspace_support",
                 "--support-plan-file", str(pf), "--support-partition", "other", "--support-execution", "image"],
                cwd=root, text=True, capture_output=True,
            )
            self.assertEqual(other_res.returncode, 0, other_res.stderr)
            self.assertIn("+ support profile: oasis7_proto", other_res.stdout)
            self.assertIn("oasis7_proto/extra_policy_feat", other_res.stdout)
            self.assertNotIn("+ support profile: oasis7_custody_core", other_res.stdout)
            for tampered in (
                {**plan, "workspace_support_partitions": ["other"]},
                {**plan, "workspace_support_execution_contract": {
                    **plan["workspace_support_execution_contract"],
                    "partitions": {"custody": [], "other": ["oasis7_proto"]},
                }},
                {**plan, "workspace_support_execution_contract": {
                    **plan["workspace_support_execution_contract"],
                    "partitions": {"custody": "oasis7_custody_core", "other": ["oasis7_proto"]},
                }},
                {**plan, "workspace_support_execution_contract": {
                    **plan["workspace_support_execution_contract"],
                    "complete_features_by_profile": {"oasis7_proto": "extra_policy_feat"},
                }},
            ):
                pf.write_text(json.dumps(tampered), encoding="utf-8")
                bad_res = subprocess.run(
                    ["bash", str(scripts / "ci-tests.sh"), "required", "--group", "workspace_support",
                     "--support-plan-file", str(pf), "--support-partition", "other", "--support-execution", "all"],
                    cwd=root, text=True, capture_output=True,
                )
                self.assertNotEqual(bad_res.returncode, 0)

    def test_quantitative_ast_package_literal_ratchet_blocks_new_hardcoding(self):
        gate = self._load_mod("_scope_gate", "check-ci-scope-paths.py")
        repo_root = HERE.parent
        self.assertEqual(gate.check_executor_hardcoded_package_ratchet(repo_root), [])
        with tempfile.TemporaryDirectory() as tmp:
            sandbox = Path(tmp)
            (sandbox / "crates/oasis7_proto").mkdir(parents=True)
            (sandbox / "crates/oasis7_proto/Cargo.toml").write_text('[package]\nname = "oasis7_proto"\nversion = "0.1.0"\n', encoding="utf-8")
            (sandbox / "scripts").mkdir()
            limit = gate.AST_WORKSPACE_PACKAGE_LITERAL_CEILINGS["scripts/ci-test-artifacts.py"]
            (sandbox / "scripts/ci-test-artifacts.py").write_text(
                "X = [" + ", ".join("'oasis7_proto'" for _ in range(limit + 1)) + "]\n",
                encoding="utf-8",
            )
            errs = gate.check_executor_hardcoded_package_ratchet(sandbox)
            self.assertEqual(len(errs), 1)
            self.assertIn("exceeds ratchet ceiling", errs[0])
            # Deleting the crate shrinks active package names monotonically without tripping the ceiling.
            (sandbox / "crates/oasis7_proto/Cargo.toml").unlink()
            self.assertEqual(gate.check_executor_hardcoded_package_ratchet(sandbox), [])


if __name__ == "__main__":
    unittest.main()
