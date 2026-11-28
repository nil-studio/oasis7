#!/usr/bin/env python3
import copy
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location('selector', HERE / 'plan-rust-required-scope.py')
selector = importlib.util.module_from_spec(spec)
spec.loader.exec_module(selector)


class _ScopeFixtureBase(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.previous = Path.cwd()
        os.chdir(self.temp.name)
        self.git('init', '-q')
        self.git('config', 'user.email', 'ci@example.invalid')
        self.git('config', 'user.name', 'CI fixture')
        self.write('Cargo.toml', '[workspace]\nmembers=["crates/*"]\n[workspace.dependencies]\ncore={path="crates/core"}\n')
        for package in ['core', 'consumer', 'other']:
            manifest = f'[package]\nname="{package}"\nversion="0.1.0"\n'
            if package == 'consumer':
                manifest += '[target.\'cfg(unix)\'.dev-dependencies]\ncore={workspace=true,optional=true}\n'
            manifest += '[features]\ntest_tier_required=[]\ntest_tier_full=["test_tier_required"]\n'
            self.write(f'crates/{package}/Cargo.toml', manifest)
            self.write(f'crates/{package}/src/lib.rs', '')
        self.write('doc/readme.md', 'ordinary doc')
        # Plans that grant an optional execution protocol also inspect its
        # declaration in this immutable tested source, independently of policy.
        self.write('scripts/ci-required-scope.json', (HERE / 'ci-required-scope.json').read_text())
        self.base = self.commit()
        groups = ['baseline', 'rust_baseline', 'core', 'consumer', 'other', *selector.PLATFORM_GROUPS]
        self.config = {'groups': groups, 'resources': {g: [] for g in groups},
                       'package_groups': {p: [p] for p in ['core', 'consumer', 'other']},
                       'rules': [{'match': ['doc/**'], 'groups': [], 'reason': 'doc'}]}

    def tearDown(self):
        os.chdir(self.previous)
        self.temp.cleanup()

    def git(self, *args):
        return subprocess.check_output(['git', *args], stderr=subprocess.DEVNULL).decode().strip()

    def write(self, path, content):
        Path(path).parent.mkdir(parents=True, exist_ok=True)
        Path(path).write_text(content)

    def commit(self):
        self.git('add', '-A')
        self.git('commit', '-qm', 'fixture')
        return self.git('rev-parse', 'HEAD')


class ViewerNpmLockScopeTests(_ScopeFixtureBase):
    def test_viewer_npm_manifest_and_lockfile_select_launcher_web_while_other_viewer_assets_do_not(self):
        config = json.loads((HERE / 'ci-required-scope.json').read_text())
        self.write('crates/oasis7_viewer/package.json', '{}\n')
        base = self.commit()
        graph = ({}, {}, {})
        with patch.object(selector, 'cargo_graph', return_value=graph):
            for path in ('crates/oasis7_viewer/package.json', 'crates/oasis7_viewer/package-lock.json',
                         'crates/oasis7_viewer/src/app.tsx'):
                with self.subTest(path=path):
                    self.write(path, '{"updated": true}\n')
                    head = self.commit()
                    plan = selector.select(config, base, head)
                    self.assertEqual(plan['scope'], 'targeted')
                    self.assertIn('viewer_js_required', plan['groups'])
                    self.assertNotIn('launcher_web', plan['groups'])
                    base = head

    def test_launcher_web_is_retired_from_active_groups(self):
        config = json.loads((HERE / 'ci-required-scope.json').read_text())
        self.assertEqual(config['resources']['launcher_web'], [])
        real_manifests, real_reverse, _ = selector.cargo_graph(
            'HEAD', config.get('generated_inputs', []), repo=HERE.parent,
        )
        clean_manifests = {pkg: (directory, {}) for pkg, (directory, _) in real_manifests.items()}
        graph = (clean_manifests, real_reverse, {})
        sample_inputs = (
            'crates/oasis7_proto/src/lib.rs',
            'crates/oasis7_launcher_backend/src/lib.rs',
        )
        for rel_path in sample_inputs:
            self.write(rel_path, '// baseline\n')
        base = self.commit()
        with patch.object(selector, 'cargo_graph', return_value=graph):
            for rel_path in sample_inputs:
                self.write(rel_path, f'// changed {rel_path}\n')
                head = self.commit()
                plan = selector.select(config, base, head)
                self.assertEqual(plan['scope'], 'targeted')
                self.assertNotIn('launcher_web', plan['groups'])
                base = head


class MainFastHistoricalRetiredPathsTests(_ScopeFixtureBase):
    ROOT_TOML = (
        '[workspace]\n'
        'members=["crates/oasis7_runtime", "crates/oasis7_runtime_host", '
        '"crates/oasis7_chain_runtime", "crates/oasis7_context", "crates/oasis7_viewer"]\n'
        'resolver="2"\n'
        '[workspace.dependencies]\n'
        'serde = "1.0"\n'
    )

    BASE_LOCK = (
        'version = 3\n\n'
        '[[package]]\nname = "oasis7_chain_runtime"\nversion = "0.1.0"\n'
        'dependencies = ["oasis7_runtime_host"]\n\n'
        '[[package]]\nname = "oasis7_context"\nversion = "0.1.0"\n\n'
        '[[package]]\nname = "oasis7_runtime"\nversion = "0.1.0"\n\n'
        '[[package]]\nname = "oasis7_runtime_host"\nversion = "0.1.0"\n'
        'dependencies = ["oasis7_runtime"]\n\n'
        '[[package]]\nname = "oasis7_viewer"\nversion = "0.1.0"\n\n'
        '[[package]]\nname = "serde"\nversion = "1.0.210"\n'
        'source = "registry+https://github.com/rust-lang/crates.io-index"\n'
        'checksum = "c8e3592472072e6e22e0a54d5904d9febf8508f65fb8552499a1abc7d1078c3a"\n'
    )

    def setUp(self):
        super().setUp()
        self.config = json.loads((HERE / 'ci-required-scope.json').read_text())
        self.write('Cargo.toml', self.ROOT_TOML)
        self.write('Cargo.lock', self.BASE_LOCK)
        self.write('crates/oasis7_runtime/Cargo.toml', '[package]\nname="oasis7_runtime"\nversion="0.1.0"\nedition="2021"\n')
        self.write('crates/oasis7_runtime/src/lib.rs', 'pub fn rt() {}\n')
        self.write(
            'crates/oasis7_runtime_host/Cargo.toml',
            '[package]\nname="oasis7_runtime_host"\nversion="0.1.0"\nedition="2021"\n'
            '[dependencies]\noasis7_runtime={path="../oasis7_runtime"}\n',
        )
        self.write('crates/oasis7_runtime_host/src/lib.rs', 'pub fn host() {}\n')
        self.write(
            'crates/oasis7_chain_runtime/Cargo.toml',
            '[package]\nname="oasis7_chain_runtime"\nversion="0.1.0"\nedition="2021"\n'
            '[dependencies]\noasis7_runtime_host={path="../oasis7_runtime_host"}\n',
        )
        self.write('crates/oasis7_chain_runtime/build.rs', 'fn main() {}\n')
        self.write('crates/oasis7_chain_runtime/src/lib.rs', 'pub fn chain() {}\n')
        self.write('crates/oasis7_context/Cargo.toml', '[package]\nname="oasis7_context"\nversion="0.1.0"\nedition="2021"\n')
        self.write('crates/oasis7_context/src/lib.rs', 'pub fn ctx() {}\n')
        self.write('crates/oasis7_viewer/Cargo.toml', '[package]\nname="oasis7_viewer"\nversion="0.1.0"\nedition="2021"\n')
        self.write('crates/oasis7_viewer/src/lib.rs', 'pub fn view() {}\n')
        self.base = self.commit()

    def test_bridges_only_historically_deleted_and_retired_paths_in_widened_main_fast_range(self):
        # Simulate exact main sequence: legacy file in range_base -> registered in c1 -> deleted in c2 -> rule retired in policy_sha
        self.write('scripts/retired-devnet-smoke.sh', '#!/usr/bin/env bash\nexit 0\n')
        range_base = self.commit()

        cfg_with_rule = copy.deepcopy(self.config)
        cfg_with_rule['rules'].append({
            'match': ['scripts/retired-devnet-smoke.sh'],
            'groups': ['operational_contracts'],
            'reason': 'retired_legacy_scripts',
        })
        self.write('scripts/ci-required-scope.json', json.dumps(cfg_with_rule, indent=2) + '\n')
        self.commit()

        # Commit D on main deletes the script while its rule is present in D^1
        os.remove('scripts/retired-devnet-smoke.sh')
        self.git('add', '-A')
        self.commit()

        # Commit B (policy_sha) retires the rule from ci-required-scope.json
        self.write('scripts/ci-required-scope.json', json.dumps(self.config, indent=2) + '\n')
        policy_sha = self.commit()

        # Commit C (head) makes an ordinary Rust edit
        self.write('crates/oasis7_runtime_host/src/lib.rs', 'pub fn host() { let _ = 1; }\n')
        head_sha = self.commit()

        # In main-fast with range_base < policy_sha, the historically deleted + rule-retired path bridges cleanly
        plan = selector.select(
            self.config, range_base, head_sha, head_sha,
            profile='main-fast', policy_ref=policy_sha,
        )
        self.assertIn('operational_contracts', plan['groups'])
        self.assertTrue(any(
            'bridged historical retired path in main-fast range: scripts/retired-devnet-smoke.sh' == r
            for r in plan['reasons']
        ))

        # pr-fast MUST still reject unknown paths even with the same commits
        with self.assertRaises(selector.UnknownInputsError):
            selector.select(
                self.config, range_base, head_sha, head_sha,
                profile='pr-fast', policy_ref=policy_sha,
            )

        # Unwidened main-fast (base == policy_sha) and widened main-fast where the file still
        # existed at policy_sha MUST both reject newly deleted unregistered paths!
        self.git('checkout', '-q', range_base)
        self.write('scripts/unregistered-now.sh', '#!/usr/bin/env bash\n')
        rb_with_unreg = self.commit()
        self.write('crates/oasis7_runtime_host/src/lib.rs', 'pub fn host() { let _ = 9; }\n')
        unreg_policy = self.commit()
        os.remove('scripts/unregistered-now.sh')
        self.git('add', '-A')
        unreg_head = self.commit()
        for b_ref in (unreg_policy, rb_with_unreg):
            with self.assertRaises(selector.UnknownInputsError):
                selector.select(
                    self.config, b_ref, unreg_head, unreg_head,
                    profile='main-fast', policy_ref=unreg_policy,
                )

        # Re-introducing the retired path in head MUST still fail UnknownInputsError
        self.git('checkout', '-q', head_sha)
        self.write('scripts/retired-devnet-smoke.sh', '#!/usr/bin/env bash\necho back\n')
        reintroduced_head = self.commit()
        with self.assertRaises(selector.UnknownInputsError):
            selector.select(
                self.config, range_base, reintroduced_head, reintroduced_head,
                profile='main-fast', policy_ref=policy_sha,
            )

        # If historical rule mapped to a group name no longer in current groups, fall back to scope=full
        self.git('checkout', '-q', range_base)
        cfg_retired_group = copy.deepcopy(self.config)
        cfg_retired_group['rules'].append({
            'match': ['scripts/retired-group-tool.sh'],
            'groups': ['obsolete_group_name'],
            'reason': 'obsolete_group_rule',
        })
        self.write('scripts/ci-required-scope.json', json.dumps(cfg_retired_group, indent=2) + '\n')
        self.write('scripts/retired-group-tool.sh', '#!/usr/bin/env bash\n')
        rb2 = self.commit()
        os.remove('scripts/retired-group-tool.sh')
        self.write('scripts/ci-required-scope.json', json.dumps(self.config, indent=2) + '\n')
        self.git('add', '-A')
        pol2 = self.commit()
        self.write('crates/oasis7_runtime_host/src/lib.rs', 'pub fn host() { let _ = 2; }\n')
        hd2 = self.commit()
        fallback_plan = selector.select(
            self.config, rb2, hd2, hd2,
            profile='main-fast', policy_ref=pol2,
        )
        self.assertEqual(fallback_plan['scope'], 'full')
        self.assertTrue(any(
            'historical retired path group fallback in main-fast range: scripts/retired-group-tool.sh' == r
            for r in fallback_plan['reasons']
        ))

    def test_function_pointer_signature_preserves_let_cfg_owner_stability_and_move_detection(self):
        target_path = 'crates/oasis7_runtime_host/src/lib.rs'
        self.write(
            target_path,
            'pub fn alpha(cb: fn()) {\n'
            '    cb();\n'
            '    #[cfg(target_os = "macos")]\n'
            '    let _x = 1;\n'
            '}\n'
            'pub fn beta(cb: fn()) {\n'
            '    cb();\n'
            '}\n',
        )
        base_fn_ptr = self.commit()

        # Prepending an unrelated fn(f: fn()) helper must not destabilize alpha's unchanged macos let gate
        self.write(
            target_path,
            'pub fn helper(f: fn()) { f(); }\n'
            'pub fn alpha(cb: fn()) {\n'
            '    cb();\n'
            '    #[cfg(target_os = "macos")]\n'
            '    let _x = 1;\n'
            '}\n'
            'pub fn beta(cb: fn()) {\n'
            '    cb();\n'
            '}\n',
        )
        stable_head = self.commit()
        policy_paths, ignored_risk, prod_cfg = selector.changed_test_policy(
            base_fn_ptr, stable_head, [target_path], {target_path: {'oasis7_runtime_host'}},
            set(self.config['oasis7_required_packages']), {}, set(),
        )
        self.assertEqual((policy_paths, ignored_risk, prod_cfg), ([], False, []))

        # Moving the #[cfg(target_os = "macos")] let statement from alpha(cb: fn()) to beta(cb: fn()) MUST be detected!
        self.git('checkout', '-q', base_fn_ptr)
        self.write(
            target_path,
            'pub fn alpha(cb: fn()) {\n'
            '    cb();\n'
            '}\n'
            'pub fn beta(cb: fn()) {\n'
            '    cb();\n'
            '    #[cfg(target_os = "macos")]\n'
            '    let _x = 1;\n'
            '}\n',
        )
        moved_head = self.commit()
        policy_paths2, _, prod_cfg2 = selector.changed_test_policy(
            base_fn_ptr, moved_head, [target_path], {target_path: {'oasis7_runtime_host'}},
            set(self.config['oasis7_required_packages']), {}, set(),
        )
        self.assertEqual(policy_paths2, [])
        self.assertEqual(prod_cfg2, [target_path])

    def test_oasis7_required_package_groups_are_registered_in_required_packages(self):
        cfg = json.loads((HERE / 'ci-required-scope.json').read_text(encoding='utf-8'))
        required_packages = cfg['oasis7_required_packages']
        self.assertEqual(len(required_packages), len(set(required_packages)))
        grouped_required = {
            package for package, groups in cfg['package_groups'].items()
            if 'oasis7_required' in groups
        }
        missing = sorted(grouped_required - set(required_packages))
        self.assertEqual(
            missing,
            [],
            'packages mapped to oasis7_required in package_groups must also be in oasis7_required_packages',
        )

    def test_managed_script_subdirectories_route_without_unknown_inputs(self):
        cases = (
            ('scripts/governance/new-gate.sh', 'workflow_governance'),
            ('scripts/ops/new-drill.sh', 'operational_contracts'),
            ('scripts/packaging/new-check.sh', 'packaging_contracts'),
            ('scripts/viewer/new-helper.mjs', 'viewer_js_required'),
            ('scripts/security/new-guard.py', 'workflow_governance'),
        )
        for path, expected_group in cases:
            with self.subTest(path=path):
                self.git('checkout', '-q', self.base)
                self.write(path, '#!/usr/bin/env bash\nexit 0\n')
                head = self.commit()
                plan = selector.select(self.config, self.base, head, profile='pr-fast')
                self.assertEqual(plan['scope'], 'targeted', plan['reasons'])
                self.assertIn(expected_group, plan['groups'])
        self.git('checkout', '-q', self.base)
        self.write('scripts/unregistered-root-script.sh', '#!/usr/bin/env bash\nexit 0\n')
        unregistered_head = self.commit()
        with self.assertRaises(selector.UnknownInputsError):
            selector.select(self.config, self.base, unregistered_head, profile='pr-fast')

    def test_operational_decoupling_and_fleet_health_subcontracts_v1(self):
        self.write(
            'Cargo.toml',
            self.ROOT_TOML.replace(
                '"crates/oasis7_viewer"]',
                '"crates/oasis7_viewer", "crates/oasis7_domain_types", '
                '"crates/oasis7_local_signer", "crates/oasis7_hosted_access"]',
            ),
        )
        self.write(
            'crates/oasis7_domain_types/Cargo.toml',
            '[package]\nname="oasis7_domain_types"\nversion="0.1.0"\nedition="2021"\n',
        )
        self.write('crates/oasis7_domain_types/src/lib.rs', 'pub fn domain() {}\n')
        self.write(
            'crates/oasis7_local_signer/Cargo.toml',
            '[package]\nname="oasis7_local_signer"\nversion="0.1.0"\nedition="2021"\n'
            '[dependencies]\noasis7_domain_types={path="../oasis7_domain_types"}\n',
        )
        self.write('crates/oasis7_local_signer/src/lib.rs', 'pub fn signer() {}\n')
        self.write(
            'crates/oasis7_hosted_access/Cargo.toml',
            '[package]\nname="oasis7_hosted_access"\nversion="0.1.0"\nedition="2021"\n'
            '[dependencies]\noasis7_domain_types={path="../oasis7_domain_types"}\n',
        )
        self.write('crates/oasis7_hosted_access/src/lib.rs', 'pub fn hosted() {}\n')
        self.base = self.commit()

        # 1) Unrelated operational scripts and local-signer runbook no longer drag fleet_health or windows_rollout
        decoupled_cases = (
            ('scripts/ops/new-drill.sh', {'operational_contracts'}),
            ('scripts/traffic-monitor-summary.sh', {'operational_contracts'}),
            ('scripts/provider-remote-https/smoke.sh', {'operational_contracts'}),
            ('scripts/run-launcher-stack.sh', {'operational_contracts'}),
            ('doc/p2p/blockchain/local-file-signing-backend.runbook.md',
             {'packaging_contracts', 'macos_package_contract'}),
        )
        for path, required_groups in decoupled_cases:
            with self.subTest(decoupled=path):
                self.git('checkout', '-q', self.base)
                self.write(path, '# fixture\n')
                head = self.commit()
                plan = selector.select(self.config, self.base, head, profile='pr-fast')
                self.assertEqual(plan['scope'], 'targeted', plan['reasons'])
                self.assertTrue(required_groups.issubset(plan['groups']))
                self.assertNotIn('fleet_health', plan['groups'])
                self.assertNotIn('windows_rollout', plan['groups'])
                self.assertEqual(plan['fleet_health_subcontracts_version'], 1)
                self.assertEqual(plan['fleet_health_checks'], [])
                self.assertEqual(plan['fleet_health_oses'], [])

        # 2) Exact OS & subcontract slices for each real consumer
        slice_cases = (
            ('scripts/p2p-public-testnet-package-rollout.py',
             {'operational_contracts', 'windows_rollout'}, False,
             [], ['windows-2022']),
            ('.github/workflows/testnet-packages.yml',
             {'workflow_governance', 'packaging_contracts', 'operational_contracts', 'windows_rollout'}, False,
             [], ['windows-2022']),
            ('scripts/p2p-public-testnet-fleet-health.test.py',
             {'operational_contracts', 'fleet_health'}, True,
             ['fleet_script'], ['ubuntu-24.04', 'windows-2022', 'macos-15']),
            ('scripts/public-testnet-validator-triad-inventory.v1.json',
             {'operational_contracts', 'fleet_health'}, True,
             ['fleet_script'], ['ubuntu-24.04', 'windows-2022', 'macos-15']),
            ('scripts/resource-cleanup-executor.py',
             {'workflow_governance', 'fleet_health'}, True,
             ['host_cleanup'], ['ubuntu-24.04', 'macos-15']),
            ('scripts/new-task-worktree.sh',
             {'cargo_tooling_contracts', 'fleet_health'}, True,
             ['host_cleanup'], ['ubuntu-24.04', 'macos-15']),
            ('scripts/local-signer/installer.py',
             {'packaging_contracts', 'fleet_health', 'workspace_support'}, True,
             ['macos_signer'], ['macos-15']),
            ('scripts/ci-local-signer-tests.py',
             {'workflow_governance', 'packaging_contracts', 'fleet_health'}, True,
             ['macos_signer'], ['macos-15']),
            ('crates/oasis7_local_signer/src/lib.rs',
             {'packaging_contracts', 'fleet_health', 'workspace_support'}, True,
             ['macos_signer'], ['macos-15']),
            ('crates/oasis7_hosted_access/src/lib.rs',
             {'oasis7_required', 'fleet_health'}, True,
             ['hosted_access'], ['ubuntu-24.04', 'windows-2022', 'macos-15']),
        )
        for path, required_groups, has_fleet, expected_checks, expected_oses in slice_cases:
            with self.subTest(slice_path=path):
                self.git('checkout', '-q', self.base)
                self.write(path, '// fixture\n')
                head = self.commit()
                plan = selector.select(self.config, self.base, head, profile='pr-fast')
                self.assertEqual(plan['scope'], 'targeted', plan['reasons'])
                self.assertTrue(required_groups.issubset(plan['groups']))
                self.assertEqual('fleet_health' in plan['groups'], has_fleet)
                self.assertEqual(plan['fleet_health_subcontracts_version'], 1)
                self.assertEqual(plan['fleet_health_checks'], expected_checks)
                self.assertEqual(plan['fleet_health_oses'], expected_oses)

        # 3) Mixed changes take the exact union of subcontracts and host OSes
        self.git('checkout', '-q', self.base)
        self.write('scripts/resource-cleanup-executor.py', '# cleanup\n')
        self.write('scripts/p2p-public-testnet-package-rollout.py', '# rollout\n')
        union_head = self.commit()
        union_plan = selector.select(self.config, self.base, union_head, profile='pr-fast')
        self.assertEqual(union_plan['fleet_health_checks'], ['host_cleanup'])
        self.assertEqual(union_plan['fleet_health_oses'], ['ubuntu-24.04', 'windows-2022', 'macos-15'])

        # 4) Shared upstream Rust crate affecting both oasis7_hosted_access and oasis7_local_signer selects both checks
        self.git('checkout', '-q', self.base)
        self.write('crates/oasis7_domain_types/src/lib.rs', '// shared upstream\n')
        shared_head = self.commit()
        shared_plan = selector.select(self.config, self.base, shared_head, profile='pr-fast')
        self.assertEqual(shared_plan['fleet_health_checks'], ['hosted_access', 'macos_signer'])
        self.assertEqual(shared_plan['fleet_health_oses'], ['ubuntu-24.04', 'windows-2022', 'macos-15'])

        # 5) Full profiles, explicit --add-group, and unmapped rules retain all 4 checks across all 3 OSes
        full_checks = ['fleet_script', 'hosted_access', 'macos_signer', 'host_cleanup']
        full_oses = ['ubuntu-24.04', 'windows-2022', 'macos-15']
        explicit_full = selector.full_plan(self.config, 'periodic-full', self.base)
        self.assertEqual(explicit_full['fleet_health_checks'], full_checks)
        self.assertEqual(explicit_full['fleet_health_oses'], full_oses)

        self.git('checkout', '-q', self.base)
        self.write('scripts/ops/new-drill.sh', '#!/usr/bin/env bash\n')
        add_head = self.commit()
        added_plan = selector.select(self.config, self.base, add_head, additions=('fleet_health',), profile='pr-fast')
        self.assertEqual(added_plan['fleet_health_checks'], full_checks)
        self.assertEqual(added_plan['fleet_health_oses'], full_oses)

        unmapped_cfg = copy.deepcopy(self.config)
        unmapped_cfg['rules'].append({
            'match': ['scripts/ops/custom-fleet-probe.sh'],
            'groups': ['fleet_health'],
            'reason': 'custom_unmapped_fleet_probe',
        })
        self.git('checkout', '-q', self.base)
        self.write('scripts/ops/custom-fleet-probe.sh', '#!/usr/bin/env bash\n')
        unmapped_head = self.commit()
        unmapped_plan = selector.select(unmapped_cfg, self.base, unmapped_head, profile='pr-fast')
        self.assertEqual(unmapped_plan['fleet_health_checks'], full_checks)
        self.assertEqual(unmapped_plan['fleet_health_oses'], full_oses)

    def test_command_center_offline_version_decouples_shared_crates_and_stamps_all_profiles(self):
        real_cfg = json.loads((HERE / 'ci-required-scope.json').read_text())
        self.assertEqual(real_cfg.get('command_center_offline_version'), 1)
        self.assertEqual(real_cfg.get('group_jobs', {}).get('command_center'), 'baseline')
        for package in ('oasis7_signer_protocol', 'oasis7_local_signer', 'oasis7_context', 'oasis7_secure_envelope'):
            self.assertNotIn('command_center', real_cfg['package_groups'][package])

        for profile in ('periodic-full', 'candidate-full'):
            full = selector.full_plan(real_cfg, profile, self.base)
            self.assertEqual(full.get('command_center_offline_version'), 1)
            self.assertEqual(full.get('contract_job_consolidation_version'), 5)
            self.assertIn('command_center', full['groups'])

        self.git('checkout', '-q', self.base)
        self.write('tools/oasis7-command-center/ui/src/App.tsx', '// paused command-center edit\n')
        cc_head = self.commit()
        for profile in ('pr-fast', 'main-fast'):
            cc_plan = selector.select(real_cfg, self.base, cc_head, profile=profile)
            self.assertEqual(cc_plan['scope'], 'targeted')
            self.assertEqual(set(cc_plan['groups']), {'baseline', 'command_center'})
            self.assertEqual(cc_plan.get('command_center_offline_version'), 1)

        for bad in (
            dict(real_cfg, command_center_offline_version=2),
            dict(real_cfg, command_center_offline_version=True),
            dict(real_cfg, group_jobs={**real_cfg['group_jobs'], 'command_center': 'node'}),
        ):
            with self.assertRaises(ValueError):
                selector.validate_config(bad)


if __name__ == '__main__':
    unittest.main()
