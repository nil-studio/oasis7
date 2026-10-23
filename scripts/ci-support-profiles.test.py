#!/usr/bin/env python3
"""Execute the real profile dispatcher with recording Cargo functions."""
import json
import re
import subprocess
import sys
import tomllib
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DRIVER = (ROOT / 'scripts/ci-tests.sh').read_text()
CONTRACT_FUNCTION = DRIVER.split('run_signed_read_protocol_contract_tests() {', 1)[1].split('\n}', 1)[0]
FUNCTION = DRIVER.split('run_selected_workspace_support() {', 1)[1].split(
    '\nrun_rustsec_advisory_check()', 1)[0]


def production_workspace_closure(package, *, wasm):
    """Inspect local normal dependencies, including optional production edges.

    Keep this conservative about features: an optional native client in a portable
    target is still a boundary violation. Dev/build dependencies are not linked
    into the application's production target.
    """
    target_matches = {
        'cfg(not(target_arch = "wasm32"))': not wasm,
        'cfg(target_arch = "wasm32")': wasm,
        'cfg(unix)': not wasm,
        'cfg(windows)': False,
    }
    workspace = tomllib.loads((ROOT / 'Cargo.toml').read_text())['workspace']
    pending = [ROOT / 'crates' / package / 'Cargo.toml']
    seen = set()
    packages = set()
    while pending:
        path = pending.pop().resolve()
        if path in seen:
            continue
        seen.add(path)
        manifest = tomllib.loads(path.read_text())
        packages.add(manifest['package']['name'])
        dependencies = list(manifest.get('dependencies', {}).items())
        for target, table in manifest.get('target', {}).items():
            if not table.get('dependencies'):
                continue
            # New target expressions need an explicit classification; never
            # silently skip an unfamiliar condition that could admit a client.
            if target not in target_matches:
                raise AssertionError(f'Unclassified production target: {path}: {target}')
            if target_matches[target]:
                dependencies.extend(table['dependencies'].items())
        for name, dependency in dependencies:
            dependency_root = path.parent
            if isinstance(dependency, dict) and dependency.get('workspace'):
                dependency = workspace['dependencies'][name]
                dependency_root = ROOT
            if isinstance(dependency, dict):
                packages.add(dependency.get('package', name))
                if 'path' in dependency:
                    pending.append(dependency_root / dependency['path'] / 'Cargo.toml')
            else:
                packages.add(name)
    return packages


class Profiles(unittest.TestCase):
    def invoke(self, profiles, fail=False, fail_protocol=False):
        script = '''set -euo pipefail
ci_python="$1"; driver_dir="$2"; support_profiles="$3"
run() { echo "contract $*"; }
run_cargo() { echo "cargo $*"; }
run_cargo_clippy() { echo "clippy $*"; }
run_oasis7_workspace_support_crate_tests() { echo full-inventory; }
'''
        script += 'run_signed_read_protocol_contract_tests() {' + CONTRACT_FUNCTION + '\n}\n'
        if fail_protocol:
            script += 'run() { echo "contract $*"; return 19; }\n'
        if fail:
            script += 'run_cargo() { return 17; }\n'
        script += 'run_selected_workspace_support() {' + FUNCTION
        script += '\nrun_selected_workspace_support\n'
        return subprocess.run(['bash', '-c', script, 'test', sys.executable,
                               str(ROOT / 'scripts'), profiles],
                              text=True, capture_output=True)

    def test_harness_isolated_and_strict(self):
        result = self.invoke(json.dumps(['oasis7_agent_harness']))
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('clippy -p oasis7_agent_harness --all-targets', result.stdout)
        self.assertIn('cargo test -p oasis7_agent_harness --lib', result.stdout)
        self.assertNotIn('oasis7_local_signer', result.stdout)
        self.assertNotIn('full-inventory', result.stdout)

    def test_application_keeps_native_sdk_out_of_wasm(self):
        manifest = tomllib.loads((ROOT / "crates/oasis7/Cargo.toml").read_text())
        native = manifest["target"]['cfg(not(target_arch = "wasm32"))']["dependencies"]
        # The SDK is owned by World Service; the core reaches it only through
        # native host services, rather than retaining an unused direct edge.
        self.assertNotIn("oasis7_game_client", manifest["dependencies"])
        self.assertNotIn("oasis7_game_client", native)
        self.assertIn("oasis7_world_service", native)
        world_service = tomllib.loads(
            (ROOT / "crates/oasis7_world_service/Cargo.toml").read_text())
        self.assertIn("oasis7_game_client", world_service["dependencies"])
        self.assertIn("oasis7_game_client",
                      production_workspace_closure('oasis7', wasm=False))
        for root in ('oasis7', 'oasis7_viewer_runtime'):
            with self.subTest(root=root):
                closure = production_workspace_closure(root, wasm=True)
                for package in ('oasis7_world_service', 'oasis7_game_client',
                                'oasis7_model_provider'):
                    self.assertNotIn(package, closure)

    def test_game_client_isolated_and_strict(self):
        result = self.invoke(json.dumps(['oasis7_game_client']))
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('clippy -p oasis7_game_client --all-targets', result.stdout)
        self.assertIn('cargo test -p oasis7_game_client --lib', result.stdout)
        self.assertNotIn('cargo test -p oasis7 ', result.stdout)
        self.assertNotIn('full-inventory', result.stdout)

    def test_model_provider_runs_integration_tests_and_strict_clippy(self):
        result = self.invoke(json.dumps(['oasis7_model_provider']))
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('clippy -p oasis7_model_provider --all-targets', result.stdout)
        self.assertIn('cargo test -p oasis7_model_provider --all-targets', result.stdout)
        self.assertNotIn('cargo test -p oasis7_model_provider --lib', result.stdout)
        self.assertNotIn('cargo test -p oasis7 ', result.stdout)
        self.assertNotIn('full-inventory', result.stdout)

    def test_main_portable_harness_keeps_native_clients_out_of_wasm(self):
        manifest = tomllib.loads((ROOT / 'crates/oasis7/Cargo.toml').read_text())
        native = manifest['target']['cfg(not(target_arch = "wasm32"))']['dependencies']
        self.assertIn('oasis7_agent_harness', manifest['dependencies'])
        self.assertNotIn('oasis7_agent_harness', native)
        self.assertNotIn('oasis7_model_provider', manifest['dependencies'])
        self.assertIn('oasis7_model_provider', native)
        harness = tomllib.loads((ROOT / 'crates/oasis7_agent_harness/Cargo.toml').read_text())
        native_harness = harness['target']['cfg(not(target_arch = "wasm32"))']['dependencies']
        for package in ('oasis7_game_client', 'oasis7_client_api'):
            self.assertNotIn(package, harness['dependencies'])
            self.assertIn(package, native_harness)
        for root in ('oasis7', 'oasis7_agent_harness'):
            with self.subTest(root=root):
                closure = production_workspace_closure(root, wasm=True)
                self.assertIn('oasis7_agent_harness', closure)
                for package in ('oasis7_game_client', 'oasis7_model_provider'):
                    self.assertNotIn(package, closure)

    def test_invalid_lists_fail_before_execution(self):
        for value in ['[]', '["unknown"]', '["oasis7_agent_api", "oasis7_agent_api"]',
                      'null', '{}', '[1]', 'invalid']:
            with self.subTest(value=value):
                result = self.invoke(value)
                self.assertNotEqual(result.returncode, 0)
                self.assertNotIn('cargo test', result.stdout)

    def test_agent_api_runs_protocol_contract_before_cargo(self):
        result = self.invoke('["oasis7_agent_api"]')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('protocol_codegen/generate.py check', result.stdout)
        self.assertIn('unittest discover -s crates/oasis7_agent_api/protocol_codegen', result.stdout)
        self.assertIn('npm ci --prefix crates/oasis7_agent_api/protocol_conformance --ignore-scripts', result.stdout)
        self.assertIn('npm test --prefix crates/oasis7_agent_api/protocol_conformance', result.stdout)
        self.assertLess(result.stdout.index('protocol_codegen/generate.py check'),
                        result.stdout.index('clippy -p oasis7_agent_api'))
        other = self.invoke('["oasis7_agent_harness"]')
        self.assertNotIn('contract ', other.stdout)

    def test_protocol_failure_stops_profile_before_cargo(self):
        result = self.invoke('["oasis7_agent_api"]', fail_protocol=True)
        self.assertEqual(result.returncode, 19, result.stderr)
        self.assertNotIn('clippy ', result.stdout)
        self.assertNotIn('cargo test', result.stdout)
        self.assertNotIn('support profile completed', result.stdout)

    def test_failure_is_not_completed(self):
        result = self.invoke('["oasis7_agent_api"]', fail=True)
        self.assertEqual(result.returncode, 17)
        self.assertNotIn('support profile completed', result.stdout)

    def test_legacy_full_default_preserved(self):
        result = self.invoke('')
        self.assertEqual(result.returncode, 0)
        self.assertIn('full-inventory', result.stdout)

    def test_plan_omissions_and_unknown_versions_rejected(self):
        for plan in [dict(groups=['workspace_support'], scope='targeted',
                          workspace_support_profile_version=1),
                     dict(groups=['workspace_support'], workspace_support_profile_version=2)]:
            result = self.invoke('plan:' + json.dumps(plan))
            self.assertNotEqual(result.returncode, 0, result.stdout)
            self.assertNotIn('cargo test', result.stdout)

    def test_trusted_base_full_plan_expands_new_candidate_profiles(self):
        plan = dict(groups=['workspace_support'], scope='full',
                    workspace_support_profile_version=1,
                    workspace_support_profiles=['oasis7_agent_api'])
        result = self.invoke('plan:' + json.dumps(plan))
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('support profile completed: oasis7_game_client', result.stdout)
        self.assertIn('support profile completed: oasis7_model_provider', result.stdout)
        self.assertIn('support profile completed: wasm_module_observe', result.stdout)

    def test_full_inventory_preserves_packages_and_special_commands(self):
        import importlib.util
        spec = importlib.util.spec_from_file_location('scope', ROOT / 'scripts/plan-rust-required-scope.py')
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        legacy = DRIVER.split('run_oasis7_workspace_support_crate_tests() {', 1)[1].split('\n}', 1)[0]
        self.assertIn('run_cargo test -p oasis7_model_provider --all-targets', legacy)
        self.assertIn('run_cargo_clippy -p oasis7_model_provider --all-targets', legacy)
        # Independent old full entry is retained: profiles must cover every package
        # in it, not merely agree with their own inventory constant.
        self.assertEqual(set(module.SUPPORT_PROFILES),
                         set(re.findall(r'-p\s+([a-zA-Z0-9_]+)', legacy)))
        plan = dict(groups=['workspace_support'], scope='full',
                    workspace_support_profile_version=1,
                    workspace_support_profiles=list(module.SUPPORT_PROFILES))
        result = self.invoke('plan:' + json.dumps(plan))
        self.assertEqual(result.returncode, 0, result.stderr)
        for profile in module.SUPPORT_PROFILES:
            self.assertIn('+ support profile completed: ' + profile, result.stdout)
        for command in ['oasis7_wasm_executor --features wasmtime --lib',
                        'oasis7_context --all-targets --locked',
                        'oasis7_secure_envelope --all-targets --locked',
                        'oasis7_local_signer --test context_cli --locked',
                        'oasis7_client_launcher --bin oasis7_client_launcher']:
            self.assertIn(command, result.stdout)

    def test_old_trusted_plan_cannot_narrow_support(self):
        result = self.invoke('plan:' + json.dumps(dict(groups=['workspace_support'], scope='targeted')))
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('support profile completed: wasm_module_observe', result.stdout)


if __name__ == '__main__':
    unittest.main()
