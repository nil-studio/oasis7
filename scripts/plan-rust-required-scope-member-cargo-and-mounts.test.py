#!/usr/bin/env python3
import copy
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

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

    def fast_config(self):
        config = json.loads(json.dumps(self.config))
        for group in ('oasis7_required', 'formal_process', 'command_center'):
            config['groups'].append(group)
            config['resources'][group] = []
        config.update(
            fast_scope_version=1,
            oasis7_required_packages=['core', 'consumer', 'other'],
            oasis7_required_wasm_packages=['core', 'other'],
            formal_risk_rules=[
                {'match': ['crates/core/src/**', 'crates/core/tests/**'], 'suites': [],
                 'reason': 'core behavior has ordinary contracts'},
                {'match': ['crates/core/src/history.rs'], 'suites': ['world-history'],
                 'reason': 'retained history'}],
            command_center_fast_paths=['tools/oasis7-command-center/ui/src/**'])
        config['package_groups'] = {name: ['oasis7_required', 'formal_process']
                                    for name in ('core', 'consumer', 'other')}
        config['rules'].append({'match': ['tools/oasis7-command-center/**'],
                                'groups': ['command_center'], 'reason': 'desktop'})
        return config

    def bounded_config(self, paths=('crates/core/src/lib.rs',)):
        config = self.fast_config()
        config.update(fast_test_inventory_version=1, ordinary_fast_targets={
            'core-lib': {'package': 'core', 'target_kind': 'lib', 'target_name': 'core',
                         'default_features': False, 'features': ['test_tier_required'],
                         'cases': ['existing']}}, ordinary_fast_rules=[
                {'paths': list(paths), 'targets': ['core-lib'], 'reason': 'bounded contract'}])
        return config

    def ordinary_route_cargo(self, arguments, *, list_only=False):
        environment = dict(os.environ)
        environment.pop('RUSTC_WRAPPER', None)
        command = ['cargo', 'test', *arguments, '--offline']
        if list_only:
            command += ['--', '--list']
        result = subprocess.run(command, env=environment, text=True, capture_output=True, timeout=60)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        return result.stdout


class SelectedCargoMountPaths(_ScopeFixtureBase):
    LEAF = 'crates/core/src/bridge/writer.rs'

    def fixture(self, *, test_gate='test', extra_gate=None, local_gate='', defaults=True, optional_forward=False):
        self.write('.gitignore', 'target/\nCargo.lock\n')
        self.write('Cargo.toml', '[workspace]\nresolver="2"\nmembers=["crates/core"]\n')
        self.write('crates/core/Cargo.toml', '[package]\nname="core"\nversion="0.1.0"\nedition="2021"\n'
                   '[features]\ndefault=["transport"]\ntransport=[]\n'
                   'test_tier_required=[]\ntest_tier_full=["test_tier_required"]\n')
        self.write('crates/core/src/lib.rs',
                   '#[cfg(all(not(target_arch="wasm32"), feature="transport"))] mod host;\n')
        host = '#[cfg(not(test))] #[path="bridge/mod.rs"] mod production;\n'
        if test_gate is not None:
            host += '#[cfg(' + test_gate + ')] #[path="bridge/mod.rs"] mod test_bridge;\n'
        if extra_gate is not None:
            host += extra_gate + ' #[path="bridge/mod.rs"] mod extra;\n'
        self.write('crates/core/src/host.rs', host)
        self.write('crates/core/src/bridge/mod.rs', 'mod writer;\n'
                   '#[test] fn required() { assert_eq!(writer::value(), 2); }\n')
        if optional_forward:
            self.write('Cargo.toml', '[workspace]\nresolver="2"\nmembers=["crates/core", "crates/wire"]\n')
            self.write('crates/wire/Cargo.toml', '[package]\nname="wire"\nversion="0.1.0"\n'
                       '[features]\nlibp2p=[]\n')
            self.write('crates/wire/src/lib.rs', '#[cfg(feature="libp2p")] pub fn value() -> u8 { 2 }\n')
            self.write('crates/core/Cargo.toml', Path('crates/core/Cargo.toml').read_text()
                       + 'node-libp2p=["wire/libp2p", "activate-wire"]\nactivate-wire=["dep:wire"]\n'
                       + '[target.\'cfg(not(target_arch="wasm32"))\'.dependencies]\n'
                       + 'wire={path="../wire", optional=true}\n')
            self.write('crates/core/src/lib.rs',
                       '#[cfg(all(not(target_arch="wasm32"), feature="node-libp2p"))] mod host;\n')
        self.write(self.LEAF, local_gate + 'pub fn value() -> u8 { 1 }\n')
        base = self.commit()
        self.write(self.LEAF, local_gate + ('pub fn value() -> u8 { wire::value() }\n' if optional_forward
                                          else 'pub fn value() -> u8 { 2 }\n'))
        config = self.bounded_config((self.LEAF,))
        config['fast_execution_version'] = 1
        config['architecture_build_policy_version'] = 1
        config['ordinary_fast_component_rules'] = [dict(
            match=['crates/core/src/**'], exclude=['crates/core/src/bin/**'],
            targets=['core-lib'], reason='complete selected library')]
        config['ordinary_fast_targets']['core-lib'].update(
            default_features=defaults, cases=['host::test_bridge::required'])
        if optional_forward:
            config['ordinary_fast_targets']['core-lib'].update(
                default_features=False, features=['node-libp2p', 'test_tier_required'])
        return config, base, self.commit()

    def test_selected_test_route_does_not_and_the_production_only_mount(self):
        config, base, head = self.fixture(test_gate='all(test, feature="transport")')
        plan = selector.select(config, base, head, profile='pr-fast')
        self.assertFalse(plan['test_policy_changed'])
        self.assertEqual(plan['test_policy_paths'], [])
        self.assertEqual(plan['ordinary_fast_targets'], [config['ordinary_fast_targets']['core-lib']])
        self.assertEqual(plan['ordinary_execution'], 'inline')
        output = self.ordinary_route_cargo(['-p', 'core', '--lib', '--features', 'test_tier_required'])
        self.assertIn('host::test_bridge::required ... ok', output)
        self.assertIn('1 passed; 0 failed', output)

    def test_selected_optional_forwarding_matches_actual_node_style_cargo_tuple(self):
        config, base, head = self.fixture(optional_forward=True)
        plan = selector.select(config, base, head, profile='pr-fast')
        self.assertFalse(plan['test_policy_changed'])
        self.assertEqual(plan['ordinary_fast_targets'], [config['ordinary_fast_targets']['core-lib']])
        output = self.ordinary_route_cargo(['-p', 'core', '--lib', '--no-default-features',
                                           '--features', 'node-libp2p,test_tier_required'])
        self.assertIn('host::test_bridge::required ... ok', output)
        self.assertIn('1 passed; 0 failed', output)

    def test_explicit_optional_feature_proof_is_order_independent_and_worker_stays_strict(self):
        for edges in (['wire/libp2p', 'activate'], ['activate', 'wire/libp2p']):
            manifest = {'features': {'node-libp2p': edges, 'activate': ['dep:wire', 'node-libp2p']},
                        'target': {'cfg(unix)': {'dependencies': {'wire': {'optional': True}}}}}
            with self.subTest(edges=edges):
                self.assertEqual(selector.local_feature_closure(
                    manifest, ['node-libp2p'], include_dev=True, allow_enabled_optional=True),
                    {'node-libp2p', 'activate'})
                with self.assertRaises(selector.TestPolicyError):
                    selector.local_feature_closure(manifest, ['node-libp2p'], include_dev=True)
                with self.assertRaises(selector.TestPolicyError):
                    selector.worker_local_feature_closure(manifest, ['node-libp2p'])

    def test_optional_implicit_weak_missing_and_ambiguous_declarations_remain_rejected(self):
        base = {'features': {'node-libp2p': ['wire/libp2p', 'activate'], 'activate': ['dep:wire']},
                'target': {'cfg(unix)': {'dependencies': {'wire': {'optional': True}}}}}
        for mode in ('not-activated', 'weak', 'missing', 'duplicate', 'inconsistent', 'unknown-optional'):
            manifest = copy.deepcopy(base)
            if mode == 'not-activated':
                manifest['features']['node-libp2p'] = ['wire/libp2p']
            elif mode == 'weak':
                manifest['features']['node-libp2p'][0] = 'wire?/libp2p'
            elif mode == 'missing':
                manifest['target'] = {}
            elif mode in ('duplicate', 'inconsistent'):
                manifest['target']['cfg(windows)'] = {'dependencies': {'wire': {'optional': mode == 'duplicate'}}}
            else:
                manifest['target']['cfg(unix)']['dependencies']['wire']['optional'] = 'true'
            with self.subTest(mode=mode), self.assertRaises(selector.TestPolicyError):
                selector.local_feature_closure(manifest, ['node-libp2p'], include_dev=True, allow_enabled_optional=True)

    def test_unselected_full_unknown_and_opaque_routes_do_not_supply_proof(self):
        variants = (
            {'test_gate': None},
            {'test_gate': 'all(test, feature="test_tier_full")'},
            {'test_gate': 'all(test, unknown_target)'},
            {'defaults': False},
            {'local_gate': '#![cfg(feature="test_tier_full")]\n'},
            {'extra_gate': '#[cfg(feature="test_tier_full")]'},
            {'extra_gate': '#[cfg(unknown_target)]'},
            {'extra_gate': '#[opaque]'},
        )
        for options in variants:
            config, base, head = self.fixture(**options)
            with self.subTest(options=options):
                plan = selector.select(config, base, head, profile='pr-fast')
                self.assertTrue(plan['test_policy_changed'])
                self.assertIn(self.LEAF, plan['test_policy_paths'])
                self.assertNotIn('ordinary_fast_targets', plan)

    def test_inline_literal_path_test_module_does_not_poison_sibling_component_inline_selection(self):
        self.write('.gitignore', 'target/\nCargo.lock\n')
        self.write('Cargo.toml', '[workspace]\nresolver="2"\nmembers=["crates/core"]\n')
        self.write(
            'crates/core/Cargo.toml',
            '[package]\nname="core"\nversion="0.1.0"\nedition="2021"\n'
            '[features]\ndefault=[]\ntest_tier_required=[]\n'
            'test_tier_full=["test_tier_required"]\ncustom_off=[]\n',
        )
        self.write('crates/core/src/lib.rs', '#[cfg(not(target_arch="wasm32"))] include!("native.rs");\n')
        self.write(
            'crates/core/src/native.rs',
            'mod events {\n'
            '    #[cfg(test)]\n'
            '    #[path = "recipe_origin_tests.rs"]\n'
            '    pub(crate) mod recipe_origin_tests;\n'
            '}\n'
            'mod world;\n',
        )
        self.write('crates/core/src/events/recipe_origin_tests.rs', '#[test] fn origin_ok() {}\n')
        self.write(
            'crates/core/src/world/mod.rs',
            'pub mod governance;\n#[test] fn existing() { assert_eq!(governance::val(), 2); }\n',
        )
        self.write('crates/core/src/world/governance.rs', 'pub fn val() -> u8 { 1 }\n')
        base = self.commit()
        self.write('crates/core/src/world/governance.rs', 'pub fn val() -> u8 { 2 }\n')
        head = self.commit()

        config = self.bounded_config(('crates/core/src/world/governance.rs',))
        config['fast_execution_version'] = 1
        config['architecture_build_policy_version'] = 1
        config['ordinary_fast_component_rules'] = [
            dict(
                match=['crates/core/src/**'],
                exclude=['crates/core/src/bin/**'],
                targets=['core-lib'],
                reason='complete selected library',
            )
        ]
        config['ordinary_fast_targets']['core-lib'].update(
            default_features=False,
            features=['test_tier_required'],
            cases=['world::existing'],
        )
        plan = selector.select(config, base, head, profile='pr-fast')
        self.assertFalse(plan['test_policy_changed'])
        self.assertEqual(plan['test_policy_paths'], [])
        self.assertEqual(plan['ordinary_execution'], 'inline')
        self.assertEqual(plan['architecture']['build_policy'], 'ordinary-targets')
        summary = selector.explain_plan_fallbacks(plan)
        self.assertEqual(summary['execution_tier'], 'targeted-inline')
        self.assertFalse(summary['is_fallback'])

        # Inherited gates on the inline #[path] child must still be enforced.
        for gate_feat in ('test_tier_full', 'custom_off'):
            self.write(
                'crates/core/src/native.rs',
                'mod events {\n'
                f'    #[cfg(all(test, feature = "{gate_feat}"))]\n'
                '    #[path = "recipe_origin_tests.rs"]\n'
                '    pub(crate) mod recipe_origin_tests;\n'
                '}\n'
                'mod world;\n',
            )
            self.write('crates/core/src/events/recipe_origin_tests.rs', '#[test] fn origin_ok() {}\n')
            gated_base = self.commit()
            self.write(
                'crates/core/src/events/recipe_origin_tests.rs',
                '#[test] fn origin_ok() {}\n#[test] fn added_case() {}\n',
            )
            gated_head = self.commit()
            with self.subTest(gate_feat=gate_feat):
                if gate_feat == 'custom_off':
                    with self.assertRaisesRegex(selector.TestPolicyError, 'validated test feature combinations'):
                        selector.select(config, gated_base, gated_head, profile='pr-fast')
                else:
                    fallback_plan = selector.select(config, gated_base, gated_head, profile='pr-fast')
                    self.assertTrue(fallback_plan['test_policy_changed'])
                    self.assertIn(
                        'crates/core/src/events/recipe_origin_tests.rs',
                        fallback_plan['test_policy_paths'],
                    )
                    self.assertTrue(selector.explain_plan_fallbacks(fallback_plan)['is_fallback'])

    def test_struct_field_attribute_before_closing_brace_does_not_leak_to_following_path_module(self):
        self.write('.gitignore', 'target/\nCargo.lock\n')
        self.write('Cargo.toml', '[workspace]\nresolver="2"\nmembers=["crates/core"]\n')
        self.write(
            'crates/core/Cargo.toml',
            '[package]\nname="core"\nversion="0.1.0"\nedition="2021"\n'
            '[features]\ndefault=[]\ntest_tier_required=[]\ntest_tier_full=["test_tier_required"]\n',
        )
        self.write(
            'crates/core/src/lib.rs',
            'pub struct Header {\n'
            '    #[allow(dead_code)]\n'
            '    pub version: u32\n'
            '}\n'
            '#[path = "journal.rs"]\n'
            'pub mod journal;\n'
            '#[test] fn existing() { assert_eq!(journal::seq(), 2); }\n',
        )
        self.write('crates/core/src/journal.rs', 'pub fn seq() -> u32 { 1 }\n')
        base = self.commit()
        self.write('crates/core/src/journal.rs', 'pub fn seq() -> u32 { 2 }\n')
        head = self.commit()

        config = self.bounded_config(('crates/core/src/journal.rs',))
        config['fast_execution_version'] = 1
        config['architecture_build_policy_version'] = 1
        config['ordinary_fast_component_rules'] = [
            dict(
                match=['crates/core/src/**'],
                exclude=['crates/core/src/bin/**'],
                targets=['core-lib'],
                reason='complete selected library',
            )
        ]
        config['ordinary_fast_targets']['core-lib'].update(
            default_features=False,
            features=['test_tier_required'],
            cases=['existing'],
        )
        plan = selector.select(config, base, head, profile='pr-fast')
        self.assertFalse(plan['test_policy_changed'])
        self.assertEqual(plan['test_policy_paths'], [])
        self.assertEqual(plan['ordinary_execution'], 'inline')

    def test_non_root_main_and_dual_include_mod_inline_literal_path_stay_conservative(self):
        # 1. `src/lib.rs` mounts `mod main;` where `src/main.rs` has an inline
        #    `mod outer { #[path = "checks.rs"] mod checks; }`: `cargo test --lib`
        #    executes `src/main/outer/checks.rs`, NOT the bin root's `src/outer/checks.rs`.
        self.write('.gitignore', 'target/\nCargo.lock\n')
        self.write('Cargo.toml', '[workspace]\nresolver="2"\nmembers=["crates/core"]\n')
        self.write(
            'crates/core/Cargo.toml',
            '[package]\nname="core"\nversion="0.1.0"\nedition="2021"\n'
            '[features]\ndefault=[]\ntest_tier_required=[]\ntest_tier_full=["test_tier_required"]\n',
        )
        self.write('crates/core/src/lib.rs', 'mod main;\n#[test] fn existing() {}\n')
        self.write(
            'crates/core/src/main.rs',
            'mod outer {\n    #[path = "checks.rs"]\n    mod checks;\n}\npub fn main() {}\n',
        )
        self.write('crates/core/src/main/outer/checks.rs', '#[test] fn lib_child() {}\n')
        decoy = 'crates/core/src/outer/checks.rs'
        self.write(decoy, '#[test] fn bin_only_test() {}\n')
        base = self.commit()
        self.write(decoy, '#[test] fn bin_only_test() { panic!("boom"); }\n#[test] fn new_bin_case() {}\n')
        head = self.commit()

        config = self.bounded_config((decoy,))
        config['fast_execution_version'] = 1
        config['architecture_build_policy_version'] = 1
        config['ordinary_fast_component_rules'] = [
            dict(
                match=['crates/core/src/**'],
                exclude=['crates/core/src/bin/**'],
                targets=['core-lib'],
                reason='complete selected library',
            )
        ]
        config['ordinary_fast_targets']['core-lib'].update(
            default_features=False,
            features=['test_tier_required'],
            cases=['existing'],
        )
        plan = selector.select(config, base, head, profile='pr-fast')
        self.assertTrue(plan['test_policy_changed'])
        self.assertIn(decoy, plan['test_policy_paths'])
        self.assertNotIn('ordinary_fast_targets', plan)

        # 2. A file included via `include!("shared.rs")` in an integration target
        #    but mounted as `mod shared;` in `src/lib.rs` must not authorize the
        #    sibling `src/outer/checks.rs` route under `core-lib`.
        self.write('crates/core/src/lib.rs', 'mod shared;\n#[test] fn existing() {}\n')
        self.write(
            'crates/core/src/shared.rs',
            'mod outer {\n    #[path = "checks.rs"]\n    mod checks;\n}\n',
        )
        self.write('crates/core/src/shared/outer/checks.rs', '#[test] fn lib_child() {}\n')
        self.write('crates/core/tests/other.rs', '#![cfg(feature="test_tier_full")]\ninclude!("../src/shared.rs");\n')
        self.write(decoy, '#[test] fn integration_only() {}\n')
        dual_base = self.commit()
        self.write(decoy, '#[test] fn integration_only() {}\n#[test] fn added_integration_only() {}\n')
        dual_head = self.commit()
        dual_plan = selector.select(config, dual_base, dual_head, profile='pr-fast')
        self.assertTrue(dual_plan['test_policy_changed'])
        self.assertIn(decoy, dual_plan['test_policy_paths'])
        self.assertNotIn('ordinary_fast_targets', dual_plan)


class WorkspaceMemberCargoAndScopedCfgTests(_ScopeFixtureBase):
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

    def test_workspace_member_manifest_and_internal_lock_edges_scope_and_include_lock_consumers(self):
        self.write(
            'crates/oasis7_runtime_host/Cargo.toml',
            '[package]\nname="oasis7_runtime_host"\nversion="0.1.0"\nedition="2021"\n'
            '[dependencies]\noasis7_runtime={path="../oasis7_runtime"}\n'
            'oasis7_context={path="../oasis7_context"}\nserde={workspace=true}\n',
        )
        updated_lock = self.BASE_LOCK.replace(
            'dependencies = ["oasis7_runtime"]',
            'dependencies = ["oasis7_context", "oasis7_runtime", "serde"]',
        )
        self.write('Cargo.lock', updated_lock)
        head = self.commit()
        plan = selector.select(self.config, self.base, head, profile='pr-fast')
        self.assertEqual(plan['scope'], 'targeted', plan['reasons'])
        self.assertEqual(
            set(plan['groups']),
            {'baseline', 'rust_baseline', 'cargo_tooling_contracts', 'oasis7_required'},
        )
        self.assertEqual(plan['affected_packages'], ['oasis7_chain_runtime', 'oasis7_runtime_host'])
        self.assertEqual(plan['oasis7_required_packages'], ['oasis7_chain_runtime', 'oasis7_runtime_host'])
        self.assertNotIn('ordinary_fast_targets', plan)
        self.assertEqual(plan['architecture']['mode'], 'targeted')
        self.assertNotEqual(plan['architecture'].get('build_policy'), 'ordinary-targets')
        self.assertTrue(any('scoped workspace member Cargo manifest/lockfile changed: Cargo.lock' == r
                            for r in plan['reasons']))

        # Even if oasis7_context Cargo.toml changes + Cargo.lock changes, generated_inputs native-Cargo.lock
        # ensures oasis7_chain_runtime stays in oasis7_required_packages!
        self.git('checkout', '-q', self.base)
        self.write(
            'crates/oasis7_context/Cargo.toml',
            '[package]\nname="oasis7_context"\nversion="0.1.0"\nedition="2021"\n'
            '[dependencies]\nserde={workspace=true}\n',
        )
        ctx_lock = self.BASE_LOCK.replace(
            '[[package]]\nname = "oasis7_context"\nversion = "0.1.0"\n',
            '[[package]]\nname = "oasis7_context"\nversion = "0.1.0"\ndependencies = ["serde"]\n',
        )
        self.write('Cargo.lock', ctx_lock)
        ctx_head = self.commit()
        ctx_plan = selector.select(self.config, self.base, ctx_head, profile='pr-fast')
        self.assertEqual(ctx_plan['scope'], 'targeted', ctx_plan['reasons'])
        self.assertIn('oasis7_chain_runtime', ctx_plan['oasis7_required_packages'])
        self.assertNotIn('command_center', ctx_plan['groups'])
        self.assertFalse(ctx_plan['command_center_package'])
        self.assertEqual(ctx_plan.get('command_center_offline_version'), 1)
        self.assertNotIn('viewer_js_required', ctx_plan['groups'])
        self.assertNotIn('site_quality', ctx_plan['groups'])

        # Old baseline without workspace_member_cargo_manifest_scope_version stays full
        old_cfg = copy.deepcopy(self.config)
        del old_cfg['workspace_member_cargo_manifest_scope_version']
        self.assertEqual(selector.select(old_cfg, self.base, head, profile='pr-fast')['scope'], 'full')

        # Removing an internal member dependency edge still retains reverse consumers across compared graphs
        self.git('checkout', '-q', self.base)
        self.write(
            'crates/oasis7_runtime_host/Cargo.toml',
            '[package]\nname="oasis7_runtime_host"\nversion="0.1.0"\nedition="2021"\n',
        )
        removed_lock = self.BASE_LOCK.replace(
            '[[package]]\nname = "oasis7_runtime_host"\nversion = "0.1.0"\ndependencies = ["oasis7_runtime"]\n',
            '[[package]]\nname = "oasis7_runtime_host"\nversion = "0.1.0"\n',
        )
        self.write('Cargo.lock', removed_lock)
        removed_head = self.commit()
        removed_plan = selector.select(self.config, self.base, removed_head, profile='pr-fast')
        self.assertEqual(removed_plan['scope'], 'targeted', removed_plan['reasons'])
        self.assertEqual(removed_plan['affected_packages'], ['oasis7_chain_runtime', 'oasis7_runtime_host'])
        self.assertEqual(removed_plan['oasis7_required_packages'], ['oasis7_chain_runtime', 'oasis7_runtime_host'])

    def test_workspace_member_manifest_rejects_unsafe_manifest_or_lock_mutations(self):
        valid_member = (
            '[package]\nname="oasis7_runtime_host"\nversion="0.1.0"\nedition="2021"\n'
            '[dependencies]\noasis7_runtime={path="../oasis7_runtime"}\nserde={workspace=true}\n'
        )
        cases = [
            ('lone_lock', {'Cargo.lock': self.BASE_LOCK + '\n'}),
            ('root_toml', {'Cargo.toml': self.ROOT_TOML + '# root comment\n',
                           'crates/oasis7_runtime_host/Cargo.toml': valid_member}),
            ('ext_lock_checksum', {
                'crates/oasis7_runtime_host/Cargo.toml': valid_member,
                'Cargo.lock': self.BASE_LOCK.replace('1.0.210', '1.0.211'),
            }),
            ('other_member_lock_edge', {
                'crates/oasis7_runtime_host/Cargo.toml': valid_member,
                'Cargo.lock': self.BASE_LOCK.replace(
                    '[[package]]\nname = "oasis7_viewer"\nversion = "0.1.0"\n',
                    '[[package]]\nname = "oasis7_viewer"\nversion = "0.1.0"\ndependencies = ["serde"]\n',
                ),
            }),
            ('features_changed', {
                'crates/oasis7_runtime_host/Cargo.toml': valid_member + '[features]\nextra = []\n',
            }),
            ('optional_dep_changed', {
                'crates/oasis7_runtime_host/Cargo.toml': (
                    '[package]\nname="oasis7_runtime_host"\nversion="0.1.0"\nedition="2021"\n'
                    '[dependencies]\noasis7_runtime={path="../oasis7_runtime"}\nserde={workspace=true, optional=true}\n'
                ),
            }),
            ('windows_target_deps_changed', {
                'crates/oasis7_runtime_host/Cargo.toml': (
                    valid_member + '[target.\'cfg(windows)\'.dependencies]\nserde={workspace=true}\n'
                ),
            }),
            ('macos_target_deps_changed', {
                'crates/oasis7_runtime_host/Cargo.toml': (
                    valid_member + '[target.\'cfg(target_os = "macos")\'.dependencies]\nserde={workspace=true}\n'
                ),
            }),
            ('target_build_deps_changed', {
                'crates/oasis7_runtime_host/Cargo.toml': (
                    valid_member + '[target.\'cfg(not(target_arch = "wasm32"))\'.build-dependencies]\nserde={workspace=true}\n'
                ),
            }),
            ('target_optional_dep_changed', {
                'crates/oasis7_runtime_host/Cargo.toml': (
                    valid_member + '[target.\'cfg(not(target_arch = "wasm32"))\'.dependencies]\nserde={workspace=true, optional=true}\n'
                ),
            }),
            ('target_git_dep_changed', {
                'crates/oasis7_runtime_host/Cargo.toml': (
                    valid_member + '[target.\'cfg(not(target_arch = "wasm32"))\'.dependencies]\nfoo={git="https://example.invalid/foo.git"}\n'
                ),
            }),
            ('target_outside_path_dep_changed', {
                'crates/oasis7_runtime_host/Cargo.toml': (
                    valid_member + '[target.\'cfg(not(target_arch = "wasm32"))\'.dependencies]\noutside={path="../../outside"}\n'
                ),
            }),
            ('build_deps_changed', {
                'crates/oasis7_runtime_host/Cargo.toml': valid_member + '[build-dependencies]\nserde={workspace=true}\n',
            }),
            ('git_dep', {
                'crates/oasis7_runtime_host/Cargo.toml': (
                    '[package]\nname="oasis7_runtime_host"\nversion="0.1.0"\nedition="2021"\n'
                    '[dependencies]\nfoo={git="https://example.invalid/foo.git"}\n'
                ),
            }),
            ('outside_path_dep', {
                'crates/oasis7_runtime_host/Cargo.toml': (
                    '[package]\nname="oasis7_runtime_host"\nversion="0.1.0"\nedition="2021"\n'
                    '[dependencies]\noutside={path="../../outside"}\n'
                ),
            }),
            ('member_with_build_rs', {
                'crates/oasis7_chain_runtime/Cargo.toml': (
                    '[package]\nname="oasis7_chain_runtime"\nversion="0.1.0"\nedition="2021"\n'
                    '[dependencies]\noasis7_runtime_host={path="../oasis7_runtime_host"}\nserde={workspace=true}\n'
                ),
            }),
        ]
        for label, edits in cases:
            with self.subTest(case=label):
                self.git('checkout', '-q', self.base)
                for path, content in edits.items():
                    self.write(path, content)
                head = self.commit()
                self.assertEqual(selector.select(self.config, self.base, head, profile='pr-fast')['scope'], 'full')

    def test_scoped_production_cfg_retains_affected_closure_without_unrelated_groups(self):
        self.write('crates/oasis7_runtime_host/src/lib.rs', '#[cfg(unix)]\npub fn host() {}\n')
        head = self.commit()
        plan = selector.select(self.config, self.base, head, profile='pr-fast')
        self.assertEqual(plan['scope'], 'targeted', plan['reasons'])
        self.assertEqual(set(plan['groups']), {'baseline', 'rust_baseline', 'oasis7_required'})
        self.assertEqual(plan['affected_packages'], ['oasis7_chain_runtime', 'oasis7_runtime_host'])
        self.assertEqual(plan['oasis7_required_packages'], ['oasis7_chain_runtime', 'oasis7_runtime_host'])
        self.assertNotIn('ordinary_fast_targets', plan)
        self.assertTrue(any('production cfg changed; scoped package and platform coverage' in r
                            for r in plan['reasons']))

        old_cfg = copy.deepcopy(self.config)
        del old_cfg['scoped_production_cfg_version']
        self.assertEqual(selector.select(old_cfg, self.base, head, profile='pr-fast')['scope'], 'full')

        # Unowned standalone .rs production cfg falls back to conservative full,
        # even when embedded/referenced by a root-workspace member (populate path_affected)
        # or nested inside a standalone Cargo.toml subdirectory under a workspace member.
        self.git('checkout', '-q', self.base)
        self.write('tools/oasis7-command-center/src-tauri/Cargo.toml',
                   '[package]\nname="oasis7-command-center"\nversion="0.1.0"\nedition="2021"\n')
        self.write('tools/oasis7-command-center/src-tauri/src/main.rs', 'fn main() {}\n')
        self.write('crates/oasis7_runtime_host/templates/subcrate/Cargo.toml',
                   '[package]\nname="subcrate"\nversion="0.1.0"\nedition="2021"\n')
        self.write('crates/oasis7_runtime_host/templates/subcrate/src/lib.rs', 'pub fn f() {}\n')
        self.write(
            'crates/oasis7_runtime_host/src/lib.rs',
            'pub const CC_REF: &str = include_str!("../../../tools/oasis7-command-center/src-tauri/src/main.rs");\n'
            'pub const PATH_LIT: &str = "tools/oasis7-command-center/src-tauri/src/main.rs";\n'
            'pub fn host() {}\n',
        )
        cc_base = self.commit()
        self.write('tools/oasis7-command-center/src-tauri/src/main.rs', '#[cfg(target_os = "macos")]\nfn main() {}\n')
        cc_head = self.commit()
        cc_plan = selector.select(self.config, cc_base, cc_head, profile='pr-fast')
        self.assertIn('oasis7_runtime_host', cc_plan['affected_packages'])
        self.assertEqual(cc_plan['scope'], 'full')

        self.git('checkout', '-q', cc_base)
        nested_cfg = copy.deepcopy(self.config)
        nested_cfg['rules'].append({
            'match': ['crates/oasis7_runtime_host/templates/subcrate/**'],
            'groups': ['packaging_contracts'],
            'reason': 'nested_template_fixture',
        })
        self.write('crates/oasis7_runtime_host/templates/subcrate/src/lib.rs', '#[cfg(unix)]\npub fn f() {}\n')
        nested_head = self.commit()
        self.assertEqual(selector.select(nested_cfg, cc_base, nested_head, profile='pr-fast')['scope'], 'full')

    def test_v5_capability_version_validation(self):
        for key, bad_values in (
            ('workspace_member_cargo_manifest_scope_version', (0, 3, True, '1')),
            ('ordinary_controls_in_rust_baseline_version', (0, 2, True, '1')),
            ('contract_job_consolidation_version', (0, 6, True, '5')),
            ('workspace_support_direct_gate_version', (0, 3, True, '2')),
            ('ordinary_scope_config_slice_version', (0, 3, True, '2')),
            ('scoped_production_cfg_version', (0, 2, True, '1')),
            ('main_fast_coalesce_concurrency_version', (0, 2, True, '1')),
            ('architecture_artifact_build_policy_version', (0, 2, True, '1')),
        ):
            for bad in bad_values:
                with self.subTest(key=key, bad=bad), self.assertRaises(ValueError):
                    cfg = copy.deepcopy(self.config)
                    cfg[key] = bad
                    selector.validate_config(cfg)

    def test_workspace_member_v2_scopes_linux_and_wasm_target_deps_and_preserves_v1_fallback(self):
        for cfg_pred in ('cfg(not(target_arch = "wasm32"))',
                         'cfg(target_arch = "wasm32")',
                         'cfg(unix)',
                         'cfg(target_os = "linux")',
                         'wasm32-unknown-unknown'):
            with self.subTest(cfg=cfg_pred):
                self.git('checkout', '-q', self.base)
                self.write(
                    'crates/oasis7_runtime_host/Cargo.toml',
                    '[package]\nname="oasis7_runtime_host"\nversion="0.1.0"\nedition="2021"\n'
                    '[dependencies]\noasis7_runtime={path="../oasis7_runtime"}\n'
                    f'[target.\'{cfg_pred}\'.dependencies]\noasis7_viewer={{path="../oasis7_viewer"}}\n'
                    f'[target.\'{cfg_pred}\'.dev-dependencies]\nserde={{workspace=true}}\n',
                )
                self.write(
                    'Cargo.lock',
                    self.BASE_LOCK.replace(
                        '[[package]]\nname = "oasis7_runtime_host"\nversion = "0.1.0"\ndependencies = ["oasis7_runtime"]\n',
                        '[[package]]\nname = "oasis7_runtime_host"\nversion = "0.1.0"\n'
                        'dependencies = ["oasis7_runtime", "oasis7_viewer", "serde"]\n',
                    ),
                )
                head = self.commit()
                v2_plan = selector.select(self.config, self.base, head, profile='pr-fast')
                self.assertEqual(v2_plan['scope'], 'targeted', v2_plan['reasons'])
                self.assertIn('cargo_tooling_contracts', v2_plan['groups'])
                self.assertIn('rust_baseline', v2_plan['groups'])
                self.assertIn('oasis7_chain_runtime', v2_plan['affected_packages'])
                self.assertNotIn('ordinary_fast_targets', v2_plan)
                # v1 policy must conservatively fall back to full on target.<cfg> edits
                v1_cfg = self.config | {'workspace_member_cargo_manifest_scope_version': 1}
                self.assertEqual(selector.select(v1_cfg, self.base, head, profile='pr-fast')['scope'], 'full')

        # Historical tested_sha without ordinary_controls_in_rust_baseline_version retains controls partition
        self.git('checkout', '-q', self.base)
        old_scope_cfg = {k: v for k, v in self.config.items() if k != 'ordinary_controls_in_rust_baseline_version'}
        self.write('scripts/ci-required-scope.json', json.dumps(old_scope_cfg))
        self.write('crates/oasis7_chain_runtime/src/lib.rs', 'pub fn chain_updated() {}\n')
        old_source_head = self.commit()
        old_source_plan = selector.select(
            self.config, self.base, old_source_head, old_source_head,
            profile='pr-fast', policy_ref=self.base,
        )
        self.assertNotIn('ordinary_controls_in_rust_baseline_version', old_source_plan)
        if 'ordinary_partitions' in old_source_plan:
            self.assertEqual(old_source_plan['ordinary_partitions'][0], 'controls')


if __name__ == '__main__':
    unittest.main()
