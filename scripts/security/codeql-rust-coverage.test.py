#!/usr/bin/env python3
"""Offline Rust extraction configuration and real Cargo manifest contracts."""
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
import tomllib
import unittest

ROOT = Path(__file__).resolve().parents[2]
PATCH = "vendor-egui-winit-0.33.3"


class RustCoverageTests(unittest.TestCase):
    def metadata(self, exclusions):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "Cargo.toml").write_text(
                '[workspace]\nmembers = ["app"]\nresolver = "2"\n'
                + "exclude = " + json.dumps(exclusions) + "\n"
                + '[patch.crates-io]\negui-winit = { path = "' + PATCH + '" }\n')
            for folder, name in (("app", "app"), (PATCH, "egui-winit")):
                package = root / folder
                (package / "src").mkdir(parents=True)
                (package / "Cargo.toml").write_text(
                    '[package]\nname = "' + name + '"\nversion = "0.1.0"\nedition = "2024"\n')
                (package / "src/lib.rs").write_text("pub fn value() -> u8 { 1 }\n")
            manifest = root / PATCH / "Cargo.toml"
            original = manifest.read_bytes()
            result = subprocess.run(
                ["cargo", "metadata", "--format-version=1", "--no-deps", "--locked",
                 "--offline", "--manifest-path", str(manifest)],
                env={**os.environ, "RUSTC_WRAPPER": "", "CARGO_NET_OFFLINE": "true"},
                text=True, capture_output=True, timeout=20)
            self.assertEqual(manifest.read_bytes(), original)
            return result

    def test_existing_nonmember_patch_loads_with_repository_exclusion(self):
        config = tomllib.loads((ROOT / "Cargo.toml").read_text())
        self.assertEqual(config["patch"]["crates-io"]["egui-winit"]["path"], PATCH)
        self.assertNotIn(PATCH, config["workspace"]["members"])
        result = self.metadata(config["workspace"].get("exclude", []))
        self.assertEqual(result.returncode, 0, result.stderr)
        graph = json.loads(result.stdout)
        self.assertEqual([p["name"] for p in graph["packages"]], ["egui-winit"])

    def test_missing_exclusion_reproduces_workspace_identity_error(self):
        result = self.metadata([])
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("current package believes it's in a workspace when it's not", result.stderr)

    def test_extractor_options_are_rust_only_and_shared_by_profiles(self):
        workflow = (ROOT / ".github/workflows/codeql.yml").read_text()
        scan_env = workflow.split("    env:\n      RUSTC_WRAPPER:", 1)[1].split("    steps:", 1)[0]
        self.assertIn("CODEQL_EXTRACTOR_RUST_OPTION_CARGO_ALL_TARGETS: ${{ matrix.language == 'rust' && 'true' || 'false' }}", scan_env)
        self.assertIn("CODEQL_EXTRACTOR_RUST_OPTION_EXTRA_INCLUDES: ${{ matrix.language == 'rust' && format('[\"{0}/crates/oasis7_builtin_wasm_modules/_templates\"]', github.workspace) || '[]' }}", scan_env)
        self.assertNotIn("needs.plan.outputs.profile", scan_env)
        self.assertNotIn("CODEQL_EXTRACTOR_RUST_OPTION_CARGO_FEATURES:", workflow)
        ignored = workflow.split("paths-ignore:", 1)[1].split("      - id: analyze", 1)[0]
        self.assertNotIn(PATCH, ignored)
        self.assertNotIn("_templates", ignored)
        self.assertNotIn("CODEQL_EXTRACTOR_RUST_OPTION_CARGO_TARGET:", workflow)

    def test_generated_tauri_context_is_the_only_new_rust_exclusion(self):
        excluded="tools/oasis7-command-center/src-tauri/src/generated_context.rs"
        for name in ("codeql.yml","codeql-rust-capacity.yml","codeql-rust-cfg-diagnostic.yml"):
            text=(ROOT/".github/workflows"/name).read_text()
            block=re.split(r"(?m)^      - ",text.split("paths-ignore:\n",1)[1],maxsplit=1)[0]
            paths=re.findall(r"^              - (.+)$",block,re.M)
            self.assertEqual(paths,["third_party/**","target/**","node_modules/**","dist/**",".pm/**",excluded],name)
        main=(ROOT/"tools/oasis7-command-center/src-tauri/src/main.rs").read_text()
        self.assertNotIn("tauri::generate_context!",main)
        self.assertIn(".build(generated_context::create())",main)
        self.assertIn("mod generated_context;",main)
        source=(ROOT/excluded).read_text()
        self.assertEqual(source.count("tauri::generate_context!()"),1)
        self.assertIn("pub(super) fn create() -> tauri::Context<tauri::Wry>",source)
        self.assertNotIn("cfg(",source)
        self.assertNotIn("panic!",source)
        substantive=[line.strip() for line in source.splitlines() if line.strip() and not line.lstrip().startswith("//")]
        self.assertEqual(substantive,["pub(super) fn create() -> tauri::Context<tauri::Wry> {","tauri::generate_context!()","}"])

    def test_extra_include_array_resolves_to_existing_checkout_templates(self):
        workflow = (ROOT / ".github/workflows/codeql.yml").read_text()
        encoded = re.search(r"OPTION_EXTRA_INCLUDES: .*format\('([^']+)'", workflow).group(1)
        paths = json.loads(encoded.replace("{0}", str(ROOT)))
        self.assertEqual(paths, [str(ROOT / "crates/oasis7_builtin_wasm_modules/_templates")])
        self.assertEqual({p.name for p in Path(paths[0]).glob("*.rs")}, {
            "m4_factory_module_template.rs", "m4_product_module_template.rs",
            "m4_recipe_module_template.rs"})


if __name__ == "__main__":
    unittest.main()
