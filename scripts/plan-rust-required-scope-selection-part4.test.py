#!/usr/bin/env python3
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


class SelectionPart4(unittest.TestCase):
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

    def test_fast_multiline_cfg_and_existing_attribute_on_another_case_cannot_hide_move(self):
        config = self.fast_config()
        path = 'crates/core/src/lib.rs'
        before = ('#[cfg(feature = "test_tier_full")]\n#[test]\nfn slow() {}\n'
                  '#[test]\nfn important_regression() {}\n')
        self.write(path, before)
        base = self.commit()
        self.write(path, before.replace('#[test]\nfn important_regression',
                                       '#[cfg(\nfeature = "test_tier_full"\n)]\n#[test]\nfn important_regression'))
        plan = selector.select(config, base, self.commit(), profile='pr-fast')
        self.assertTrue(plan['test_policy_changed'])
        self.write(path, '#[cfg(\nfeature = "never_enabled_in_ci"\n)]\n#[test]\nfn important_regression() {}\n')
        with self.assertRaisesRegex(selector.TestPolicyError, 'validated test feature combinations'):
            selector.select(config, base, self.commit(), profile='pr-fast')
        for expression in ('any()', 'never_enabled', 'not(feature = "test_tier_full")'):
            self.write(path, f'#[cfg({expression})]\n#[test]\nfn important_regression() {{}}\n')
            with self.subTest(expression=expression), self.assertRaisesRegex(
                    selector.TestPolicyError, 'validated test feature combinations'):
                selector.select(config, base, self.commit(), profile='pr-fast')

    def test_fast_cfg_examples_in_comments_and_strings_are_not_selection_controls(self):
        config = self.fast_config()
        self.write('crates/core/src/lib.rs',
                   '// #[cfg(feature = "unknown")]\n// #[ignore]\n'
                   'const EXAMPLE: &str = r#"#[ignore] fn fixture() {}"#;\npub fn behavior() {}\n')
        plan = selector.select(config, self.base, self.commit(), profile='pr-fast')
        self.assertFalse(plan['test_policy_changed'])

    def test_fast_nested_cfg_cannot_remove_case_in_expanded_feature_combination(self):
        config = self.fast_config()
        path = 'crates/core/src/lib.rs'
        self.write(path, '#[test]\nfn regression() {}\n')
        base = self.commit()
        for annotation in ('#[cfg_attr(feature = "test_tier_full", cfg(any()))]',
                           '#[cfg(all(feature = "test_tier_full", any()))]',
                           '#![cfg(any())]'):
            with self.subTest(annotation=annotation):
                self.write(path, f'{annotation}\n#[test]\nfn regression() {{}}\n')
                with self.assertRaisesRegex(selector.TestPolicyError, 'validated test feature combinations'):
                    selector.select(config, base, self.commit(), profile='pr-fast')

    def test_fast_native_test_cfg_must_be_true_on_its_actual_linux_driver(self):
        config = self.fast_config()
        path = 'crates/core/src/lib.rs'
        self.write(path, '#[test]\nfn regression() {}\n')
        base = self.commit()
        for expression in ('unix', 'target_os = "linux"', 'not(target_arch = "wasm32")',
                           'all(unix, feature = "test_tier_full")'):
            with self.subTest(expression=expression):
                self.write(path, f'#[cfg({expression})]\n#[test]\nfn regression() {{}}\n')
                plan = selector.select(config, base, self.commit(), profile='pr-fast')
                self.assertTrue(plan['test_policy_changed'])
        self.write(path, '#[cfg(windows)]\n#[test]\nfn regression() {}\n')
        with self.assertRaisesRegex(selector.TestPolicyError, 'validated test feature combinations'):
            selector.select(config, base, self.commit(), profile='pr-fast')

    def test_native_default_driver_inherits_unix_without_expanded_feature_permission(self):
        config = self.fast_config()
        package = 'oasis7_distfs'
        config['groups'].append('distfs')
        config['resources']['distfs'] = []
        config['package_groups'][package] = ['distfs']
        self.write(f'crates/{package}/Cargo.toml',
                   '[package]\nname="oasis7_distfs"\nversion="0.1.0"\n'
                   '[features]\ntest_tier_full=[]\noptional_lane=[]\n')
        self.write(f'crates/{package}/src/lib.rs', '#[cfg(all(test, unix))] mod nested;\n')
        path = f'crates/{package}/src/nested.rs'
        self.write(path, '#[test] fn original() {}\n')
        base = self.commit()
        self.write(path, '#[test] fn original() {}\n#[test] fn added() {}\n')
        plan = selector.select(config, base, self.commit(), profile='pr-fast')
        self.assertIn('distfs', plan['groups'])
        self.write(path, '#[test] fn original() {}\n'
                   '#[cfg(target_os="linux")] #[test] fn added() {}\n')
        plan = selector.select(config, base, self.commit(), profile='pr-fast')
        self.assertIn('distfs', plan['groups'])
        for expression in ('windows', 'target_os="windows"', 'not(target_os="linux")',
                           'feature="optional_lane"',
                           'all(unix, feature="test_tier_full")'):
            with self.subTest(expression=expression):
                self.write(path, '#[test] fn original() {}\n'
                           + f'#[cfg({expression})] #[test] fn added() {{}}\n')
                with self.assertRaises(selector.TestPolicyError):
                    selector.select(config, base, self.commit(), profile='pr-fast')

        # A shared source cannot borrow the known package's Linux mapping
        # for another owner without a proven native/expanded test driver.
        config['oasis7_required_packages'].remove('other')
        self.write('crates/other/src/lib.rs',
                   '#[path="../../oasis7_distfs/src/nested.rs"] mod shared;\n')
        self.write(path, '#[test] fn original() {}\n#[test] fn added() {}\n')
        with self.assertRaises(selector.TestPolicyError):
            selector.select(config, base, self.commit(), profile='pr-fast')

    def test_native_storage_mapping_does_not_authorize_an_unexecuted_binary(self):
        config = self.fast_config()
        config['groups'].append('distfs')
        config['resources']['distfs'] = []
        config['package_groups']['oasis7_distfs'] = ['distfs']
        self.write('crates/oasis7_distfs/Cargo.toml',
                   '[package]\nname="oasis7_distfs"\nversion="0.1.0"\n')
        self.write('crates/oasis7_distfs/src/lib.rs', '')
        path = 'crates/oasis7_distfs/src/bin/unexecuted.rs'
        self.write(path, 'fn main() {}\n')
        base = self.commit()
        self.write(path, 'fn main() {}\n#[cfg(unix)] #[test] fn unexecuted() {}\n')
        with self.assertRaises(selector.TestPolicyError):
            selector.select(config, base, self.commit(), profile='pr-fast')

    def test_net_default_lib_driver_proves_unix_and_default_libp2p_only(self):
        config = self.fast_config()
        config['groups'].append('net')
        config['resources']['net'] = []
        config['package_groups']['oasis7_net'] = ['net']
        self.write('crates/oasis7_net/Cargo.toml',
                   '[package]\nname="oasis7_net"\nversion="0.1.0"\n'
                   '[features]\ndefault=["self_tests","libp2p"]\n'
                   'self_tests=[]\nlibp2p=[]\noptional_lane=[]\ntest_tier_full=[]\n')
        self.write('crates/oasis7_net/src/lib.rs',
                   '#[cfg(all(feature="libp2p", not(target_arch="wasm32")))] mod transport;\n')
        self.write('crates/oasis7_net/src/transport.rs',
                   '#[cfg(all(test, unix))] mod receiver;\n')
        path = 'crates/oasis7_net/src/transport/receiver.rs'
        self.write(path, '#![cfg(all(test, unix))]\n#[test] fn original() {}\n')
        base = self.commit()
        added = '#![cfg(all(test, unix))]\n#[test] fn original() {}\n#[test] fn recovery() {}\n'
        self.write(path, added)
        head = self.commit()
        with patch.object(selector, 'NATIVE_DEFAULT_TEST_PACKAGES',
                          selector.NATIVE_DEFAULT_TEST_PACKAGES - {'oasis7_net'}):
            with self.assertRaises(selector.TestPolicyError):
                selector.select(config, base, head, profile='pr-fast')
        plan = selector.select(config, base, head, profile='pr-fast')
        self.assertIn('net', plan['groups'])
        for expression in ('windows', 'target_os="windows"', 'feature="optional_lane"',
                           'feature="test_tier_full"', 'feature="undeclared"'):
            with self.subTest(expression=expression):
                self.write(path, added + f'#[cfg({expression})] #[test] fn unsupported() {{}}\n')
                with self.assertRaises(selector.TestPolicyError):
                    selector.select(config, base, self.commit(), profile='pr-fast')

    def test_net_default_lib_driver_does_not_authorize_unexecuted_binary(self):
        config = self.fast_config()
        config['groups'].append('net')
        config['resources']['net'] = []
        config['package_groups']['oasis7_net'] = ['net']
        self.write('crates/oasis7_net/Cargo.toml',
                   '[package]\nname="oasis7_net"\nversion="0.1.0"\n')
        self.write('crates/oasis7_net/src/lib.rs', '')
        path = 'crates/oasis7_net/src/bin/unexecuted.rs'
        self.write(path, 'fn main() {}\n')
        base = self.commit()
        self.write(path, 'fn main() {}\n#[cfg(unix)] #[test] fn unexecuted() {}\n')
        with self.assertRaises(selector.TestPolicyError):
            selector.select(config, base, self.commit(), profile='pr-fast')

    def test_node_default_lib_driver_proves_nested_unix_not_optional_features(self):
        config = self.fast_config()
        config['groups'].append('node')
        config['resources']['node'] = []
        config['package_groups']['oasis7_node'] = ['node']
        self.write('crates/oasis7_node/Cargo.toml',
                   '[package]\nname="oasis7_node"\nversion="0.1.0"\n'
                   '[features]\noptional_lane=[]\ntest_tier_full=[]\n')
        self.write('crates/oasis7_node/src/lib.rs', 'mod runtime;\n')
        self.write('crates/oasis7_node/src/runtime.rs',
                   '#[cfg(all(test, unix))] #[path="transition.rs"] mod transition;\n')
        self.write('crates/oasis7_node/src/transition.rs', '#[test] fn original() {}\n')
        base = self.commit()
        self.write('crates/oasis7_node/src/transition.rs',
                   '#[test] fn original() {}\n'
                   '#[path="transition/socket_handoff.rs"] mod socket_handoff;\n')
        path = 'crates/oasis7_node/src/transition/socket_handoff.rs'
        added = '#[test] fn socket_handoff() {}\n'
        self.write(path, added)
        head = self.commit()
        with patch.object(selector, 'NATIVE_DEFAULT_TEST_PACKAGES',
                          selector.NATIVE_DEFAULT_TEST_PACKAGES - {'oasis7_node'}):
            with self.assertRaisesRegex(selector.TestPolicyError, 'validated test feature combinations'):
                selector.select(config, base, head, profile='pr-fast')
        plan = selector.select(config, base, head, profile='pr-fast')
        self.assertIn('node', plan['groups'])
        for expression in ('windows', 'target_os="windows"', 'not(target_os="linux")',
                           'feature="optional_lane"', 'feature="test_tier_full"'):
            with self.subTest(expression=expression):
                self.write(path, added + f'#[cfg({expression})] #[test] fn unsupported() {{}}\n')
                with self.assertRaises(selector.TestPolicyError):
                    selector.select(config, base, self.commit(), profile='pr-fast')
        self.write(path, added)
        config['oasis7_required_packages'].remove('other')
        self.write('crates/other/src/lib.rs',
                   '#[path="../../oasis7_node/src/transition/socket_handoff.rs"] mod shared;\n')
        with self.assertRaises(selector.TestPolicyError):
            selector.select(config, base, self.commit(), profile='pr-fast')

    def test_node_native_default_driver_does_not_authorize_unexecuted_binary(self):
        config = self.fast_config()
        config['groups'].append('node')
        config['resources']['node'] = []
        config['package_groups']['oasis7_node'] = ['node']
        self.write('crates/oasis7_node/Cargo.toml',
                   '[package]\nname="oasis7_node"\nversion="0.1.0"\n')
        self.write('crates/oasis7_node/src/lib.rs', '')
        path = 'crates/oasis7_node/src/bin/unexecuted.rs'
        self.write(path, 'fn main() {}\n')
        base = self.commit()
        self.write(path, 'fn main() {}\n#[cfg(unix)] #[test] fn unexecuted() {}\n')
        with self.assertRaises(selector.TestPolicyError):
            selector.select(config, base, self.commit(), profile='pr-fast')

    def native_default_integration_fixture(self, package_settings='', declarations=''):
        config = self.fast_config()
        config['groups'].append('distfs')
        config['resources']['distfs'] = []
        config['package_groups']['oasis7_distfs'] = ['distfs']
        self.write('.gitignore', 'target/\nCargo.lock\n')
        self.write('Cargo.toml', '[workspace]\nresolver="2"\n'
                   'members=["crates/oasis7_distfs","crates/consumer"]\n')
        self.write('crates/oasis7_distfs/Cargo.toml',
                   '[package]\nname="oasis7_distfs"\nversion="0.1.0"\nedition="2024"\n'
                   + package_settings + '\n[features]\ndefault=["transport"]\n'
                   'transport=["native_driver"]\nnative_driver=[]\noptional_lane=[]\n'
                   'test_tier_full=[]\n' + declarations)
        self.write('crates/oasis7_distfs/src/lib.rs', 'pub fn value() -> u8 { 42 }\n')
        self.write('crates/oasis7_distfs/src/bin/oasis7_durability_endpoint.rs', 'fn main() {}\n')
        self.write('crates/consumer/Cargo.toml',
                   '[package]\nname="consumer"\nversion="0.1.0"\nedition="2024"\n'
                   '[dependencies]\noasis7_distfs={path="../oasis7_distfs"}\n')
        self.write('crates/consumer/src/lib.rs', 'pub fn observed() -> u8 { oasis7_distfs::value() }\n')
        root = 'crates/oasis7_distfs/tests/incremental_industry_jobs.rs'
        support = 'crates/oasis7_distfs/tests/support/mod.rs'
        self.write(root, '#![cfg(all(unix, not(target_arch="wasm32")))]\n'
                   '#[cfg(feature="native_driver")] mod support;\n')
        self.write(support, '#[test] fn original() { assert_eq!(oasis7_distfs::value(), 42); }\n')
        return config, root, support

    def test_native_default_integration_executes_unix_and_transitive_default_support(self):
        for explicit in (False, True):
            config, root, support = self.native_default_integration_fixture(
                'autotests=false\n' if explicit else '',
                '\n[[test]]\nname="incremental_industry_jobs"\npath="tests/incremental_industry_jobs.rs"\n' if explicit else '')
            base = self.commit()
            self.write(root, Path(root).read_text()
                       + '#[test] fn native_added() { assert_eq!(oasis7_distfs::value(), 42); }\n')
            self.write(support, Path(support).read_text()
                       + '#[cfg(target_os="linux")] #[test] fn added() { '
                       'assert_eq!(oasis7_distfs::value(), 42); }\n')
            with self.subTest(explicit=explicit):
                plan = selector.select(config, base, self.commit(), profile='pr-fast')
                self.assertFalse(plan['test_policy_changed'])
                self.assertEqual(plan['test_policy_paths'], [])
                self.assertEqual(plan['fast_plan_exceptions'], [])
                self.assertEqual(set(plan['groups']), {'baseline', 'rust_baseline', 'distfs'})
                self.assertEqual(plan['affected_packages'], ['oasis7_distfs'])
                # The actual default-feature DistFS tuple must execute the new
                # support case; a listed target or an empty harness is not proof.
                output = self.ordinary_route_cargo(
                    ['-p', 'oasis7_distfs', '--test', 'incremental_industry_jobs'])
                self.assertIn('native_added ... ok', output)
                self.assertIn('support::added ... ok', output)
                self.assertIn('3 passed; 0 failed', output)

    def test_native_default_integration_rejects_disabled_and_feature_gated_targets(self):
        for setting, declaration in (
                ('autotests=false\n', ''),
                ('', '\n[[test]]\nname="incremental_industry_jobs"\nharness=false\n'),
                ('', '\n[[test]]\nname="incremental_industry_jobs"\ntest=false\n'),
                ('', '\n[[test]]\nname="incremental_industry_jobs"\nrequired-features=["optional_lane"]\n')):
            config, root, _ = self.native_default_integration_fixture(setting, declaration)
            base = self.commit()
            self.write(root, Path(root).read_text() + '#[test] fn unexecuted() {}\n')
            with self.subTest(setting=setting, declaration=declaration):
                with self.assertRaises(selector.TestPolicyError):
                    selector.select(config, base, self.commit(), profile='pr-fast')

    def test_native_default_integration_requires_canonical_name_path_and_group(self):
        for mismatch in ('alias', 'custom-path', 'other-target', 'other-group'):
            declarations = {
                'alias': '\n[[test]]\nname="alias"\npath="tests/incremental_industry_jobs.rs"\n',
                'custom-path': '\n[[test]]\nname="incremental_industry_jobs"\npath="custom/industry.rs"\n',
            }.get(mismatch, '')
            config, root, _ = self.native_default_integration_fixture(declarations=declarations)
            if mismatch in ('custom-path', 'other-target'):
                Path(root).unlink()
                root = ('crates/oasis7_distfs/custom/industry.rs' if mismatch == 'custom-path'
                        else 'crates/oasis7_distfs/tests/other.rs')
            elif mismatch == 'other-group':
                config['package_groups']['oasis7_distfs'] = ['core']
            self.write(root, '#![cfg(unix)]\n#[test] fn existing() {}\n')
            base = self.commit()
            self.write(root, Path(root).read_text() + '#[test] fn unexecuted() {}\n')
            with self.subTest(mismatch=mismatch):
                with self.assertRaises(selector.TestPolicyError):
                    selector.select(config, base, self.commit(), profile='pr-fast')

    def test_native_default_integration_rejects_unknown_or_disabled_support_routes(self):
        for condition in ('windows', 'feature="optional_lane"', 'feature="undeclared"',
                          'unknown_target', 'not(feature="native_driver")'):
            config, root, support = self.native_default_integration_fixture()
            self.write(root, f'#[cfg({condition})] mod support;\n')
            base = self.commit()
            self.write(support, Path(support).read_text() + '#[cfg(unix)] #[test] fn unexecuted() {}\n')
            with self.subTest(condition=condition):
                with self.assertRaises(selector.TestPolicyError):
                    selector.select(config, base, self.commit(), profile='pr-fast')

    def test_native_default_integration_inputs_do_not_select_library_consumers(self):
        config, root, support = self.native_default_integration_fixture()
        base = self.commit()
        for path in (root, support):
            self.write(path, Path(path).read_text()
                       + '#[test] fn later_case() { assert_eq!(oasis7_distfs::value(), 42); }\n')
            head = self.commit()
            with self.subTest(path=path):
                plan = selector.select(config, base, head, profile='pr-fast')
                self.assertFalse(plan['test_policy_changed'])
                self.assertEqual(plan['fast_plan_exceptions'], [])
                self.assertEqual(plan['affected_packages'], ['oasis7_distfs'])
                self.assertEqual(set(plan['groups']), {'baseline', 'rust_baseline', 'distfs'})
                self.assertEqual(plan['oasis7_required_packages'], [])
            base = head
        # The same real reverse dependency is retained for production changes.
        self.write('crates/oasis7_distfs/src/lib.rs', 'pub fn value() -> u8 { 43 }\n')
        production = selector.select(config, base, self.commit(), profile='pr-fast')
        self.assertIn('consumer', production['affected_packages'])
        self.assertIn('consumer', production['oasis7_required_packages'])

    def test_industry_inputs_expand_real_fast_inventory_even_when_formal_is_deferred(self):
        _, root, support = self.native_default_integration_fixture()
        config = json.loads((HERE / 'ci-required-scope.json').read_text())
        self.assertEqual(config['package_groups']['oasis7_distfs'], ['distfs', 'formal_process'])
        self.assertEqual(config['formal_full_profiles_only_version'], 1)
        config['package_groups']['consumer'] = list(config['package_groups']['oasis7_distfs'])
        # Keep the registered Runtime/World State target identities and input
        # rules; only their Rust bodies and minimum cases are miniature fixtures.
        target_keys = ('runtime-lib', 'runtime-incremental', 'world-state-lib', 'world-reducers')
        for key in target_keys:
            config['ordinary_fast_targets'][key]['cases'] = ['existing']
        self.write('Cargo.toml', '[workspace]\nresolver="2"\nmembers=["crates/*"]\n'
                   '[workspace.dependencies]\ncore={path="crates/core"}\n')
        for package in ('oasis7_runtime', 'oasis7_world_state'):
            self.write(f'crates/{package}/Cargo.toml',
                       f'[package]\nname="{package}"\nversion="0.1.0"\nedition="2024"\n'
                       '[features]\ntest_tier_required=[]\ntest_tier_full=["test_tier_required"]\n')
            self.write(f'crates/{package}/src/lib.rs', '#[test] fn existing() {}\n')
        self.write('crates/oasis7_world_state/src/lib.rs',
                   '#[cfg(not(target_arch="wasm32"))] pub mod incremental_industry_jobs;\n'
                   '#[test] fn existing() {}\n')
        for package, target in (('oasis7_runtime', 'pr_fast_incremental'),
                                ('oasis7_world_state', 'pr_fast_reducers')):
            self.write(f'crates/{package}/tests/{target}.rs', '#[test] fn existing() {}\n')
        for package in ('oasis7_runtime', 'oasis7_distfs'):
            manifest = f'crates/{package}/Cargo.toml'
            self.write(manifest, Path(manifest).read_text()
                       + '\n[dependencies]\noasis7_world_state={path="../oasis7_world_state"}\n')
        fast_input = 'crates/oasis7_world_state/src/incremental_industry_jobs.rs'
        self.write(fast_input, 'pub fn value() -> u8 { 1 }\n')
        original = {path: Path(path).read_text() for path in (root, support, fast_input)}
        base = self.commit()
        self.write(fast_input, 'pub fn value() -> u8 { 2 }\n')
        fast_head = self.commit()
        expected_targets = {tuple(config['ordinary_fast_targets'][key][field]
                                  for field in ('package', 'target_kind', 'target_name'))
                            for key in target_keys}
        for profile in ('pr-fast', 'main-fast'):
            fast = selector.select(config, base, fast_head, profile=profile)
            self.assertEqual({tuple(target[field] for field in ('package', 'target_kind', 'target_name'))
                              for target in fast['ordinary_fast_targets']}, expected_targets)
            self.assertEqual(fast['ordinary_execution'], 'inline')
            self.assertIn('distfs', fast['groups'])
        for path in (root, support):
            for mixed in (False, True):
                for restored, content in original.items():
                    self.write(restored, content)
                base = self.commit()
                self.write(path, original[path] + '#[test] fn later_case() { '
                           'assert_eq!(oasis7_distfs::value(), 42); }\n')
                if mixed:
                    self.write(fast_input, 'pub fn value() -> u8 { 2 }\n')
                head = self.commit()
                for profile in ('pr-fast', 'main-fast'):
                    with self.subTest(path=path, mixed=mixed, profile=profile):
                        plan = selector.select(config, base, head, profile=profile)
                        self.assertFalse(plan['test_policy_changed'])
                        self.assertIn('distfs', plan['groups'])
                        self.assertNotIn('ordinary_fast_targets', plan)
                        self.assertNotIn('ordinary_execution', plan)
                        self.assertNotIn('distfs_clippy_group', plan)
                        self.assertNotIn('formal_process', plan['groups'])
                        self.assertEqual(plan['formal_suites'], [])
                        if mixed:
                            self.assertTrue({'oasis7_runtime', 'oasis7_world_state'}.issubset(
                                plan['oasis7_required_packages']))
                        else:
                            self.assertEqual(plan['affected_packages'], ['oasis7_distfs'])
                            self.assertEqual(plan['oasis7_required_packages'], [])

    def test_fast_inherited_default_feature_uses_owning_package_manifest(self):
        config = self.fast_config()
        manifest = Path('crates/core/Cargo.toml').read_text()
        self.write('crates/core/Cargo.toml', manifest + '\ndefault=["transport"]\ntransport=["node-libp2p"]\nnode-libp2p=[]\noptional_lane=[]\n')
        self.write('crates/core/src/lib.rs', '#[cfg(all(unix, feature="node-libp2p"))] mod native;\n')
        path = 'crates/core/src/native.rs'
        self.write(path, '#[test] fn original() {}\n')
        base = self.commit()
        self.write(path, '#[test] fn original() {}\n#[test] fn added() {}\n')
        plan = selector.select(config, base, self.commit(), profile='pr-fast')
        self.assertFalse(plan['test_policy_changed'])
        self.assertEqual(plan['test_policy_paths'], [])
        for expression in ('feature="optional_lane"', 'not(feature="node-libp2p")'):
            with self.subTest(expression=expression):
                self.write(path, f'#[test] fn original() {{}}\n#[cfg({expression})] #[test] fn added() {{}}\n')
                with self.assertRaisesRegex(selector.TestPolicyError, 'validated test feature combinations'):
                    selector.select(config, base, self.commit(), profile='pr-fast')

    def ordinary_alternative_mount_fixture(self, *, test_mount=True):
        # Real Cargo counterpart of a default-feature host include, a production
        # mount and a separate test mount of the same implementation. No product
        # crate, external dependency or generated test stand-in is compiled.
        self.write('.gitignore', 'target/\nCargo.lock\n')
        self.write('Cargo.toml', '[workspace]\nresolver="2"\nmembers=["crates/core"]\n')
        manifest = Path('crates/core/Cargo.toml').read_text()
        self.write('crates/core/Cargo.toml', manifest +
                   'default=["transport"]\ntransport=["native_driver"]\nnative_driver=[]\n')
        self.write('crates/core/src/lib.rs',
                   '#[cfg(all(not(target_arch="wasm32"), feature="native_driver"))]\n'
                   'include!("host.rs");\n')
        self.write('crates/core/src/host.rs',
                   '#[cfg(not(test))] #[path="bridge/mod.rs"] mod production;\n'
                   + ('#[cfg(test)] #[path="real_tests.rs"] mod real_tests;\n' if test_mount else ''))
        self.write('crates/core/src/real_tests.rs', '#[path="bridge/mod.rs"] mod real_bridge;\n')
        self.write('crates/core/src/bridge/mod.rs',
                   'fn value() -> u8 { 42 }\n#[cfg(test)] mod tests;\n')
        self.write('crates/core/src/bridge/tests/mod.rs', 'mod retention;\n')
        parent = 'crates/core/src/bridge/tests/retention.rs'
        self.write(parent, 'use super::super::value;\n#[test] fn existing() { assert_eq!(value(), 42); }\n')
        base = self.commit()
        self.write(parent, Path(parent).read_text() + '#[path="hot_window.rs"] mod hot_window;\n')
        child = 'crates/core/src/bridge/tests/hot_window.rs'
        self.write(child, 'use super::*;\n#[test] fn added() { assert_eq!(value(), 42); }\n')
        return base, self.commit(), parent, child

    def ordinary_route_cargo(self, arguments, *, list_only=False):
        environment = dict(os.environ)
        environment.pop('RUSTC_WRAPPER', None)
        command = ['cargo', 'test', *arguments, '--offline']
        if list_only:
            command += ['--', '--list']
        result = subprocess.run(command, env=environment, text=True, capture_output=True, timeout=60)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        return result.stdout

    def test_ordinary_alternative_mounts_execute_new_case_with_real_default_features(self):
        base, head, _, child = self.ordinary_alternative_mount_fixture()
        plan = selector.select(self.fast_config(), base, head, profile='pr-fast')
        self.assertFalse(plan['test_policy_changed'])
        self.assertEqual(plan['test_policy_paths'], [])
        self.assertEqual(plan['oasis7_required_packages'], ['core'])
        self.assertEqual(plan['scope'], 'targeted')
        self.assertNotIn('ordinary_fast_targets', plan)
        # Exercise the selected package's ordinary --tests tuple with its real
        # defaults and required tier, without adding expanded features.
        # An exact listed name alone would not prove execution.
        output = self.ordinary_route_cargo(
            ['-p', plan['oasis7_required_packages'][0], '--tests', '--features', 'test_tier_required'])
        self.assertIn('real_tests::real_bridge::tests::retention::hot_window::added ... ok', output)
        self.assertIn('2 passed; 0 failed', output)
        self.assertNotIn(child, plan['test_policy_paths'])

    def test_ordinary_default_route_cannot_authorize_selected_no_default_features(self):
        base, head, parent, child = self.ordinary_alternative_mount_fixture()
        config = self.bounded_config((parent, child))
        self.assertFalse(config['ordinary_fast_targets']['core-lib']['default_features'])
        plan = selector.select(config, base, head, profile='pr-fast')
        self.assertTrue(plan['test_policy_changed'])
        self.assertIn(child, plan['test_policy_paths'])
        self.assertNotIn('ordinary_fast_targets', plan)
        output = self.ordinary_route_cargo(
            ['-p', 'core', '--lib', '--no-default-features', '--features', 'test_tier_required'],
            list_only=True)
        self.assertNotIn('hot_window::added', output)
        self.assertIn('0 tests', output)

    def test_ordinary_production_only_route_cannot_authorize_an_unexecuted_test(self):
        base, head, _, _ = self.ordinary_alternative_mount_fixture(test_mount=False)
        with self.assertRaisesRegex(selector.TestPolicyError, 'validated test feature combinations'):
            selector.select(self.fast_config(), base, head, profile='pr-fast')
        output = self.ordinary_route_cargo(
            ['-p', 'core', '--tests', '--features', 'test_tier_required'], list_only=True)
        self.assertNotIn('hot_window::added', output)
        self.assertIn('0 tests', output)

    def test_ordinary_alternative_route_preserves_policy_and_retirement_checks(self):
        base, _, parent, child = self.ordinary_alternative_mount_fixture()
        ordinary = Path(child).read_text()
        for attribute in ('#[ignore]', '#[cfg(feature="test_tier_full")]'):
            with self.subTest(attribute=attribute):
                self.write(child, ordinary.replace('#[test]', attribute + '\n#[test]'))
                head = self.commit()
                if attribute == '#[ignore]':
                    with self.assertRaisesRegex(selector.TestPolicyError, 'ignored test'):
                        selector.select(self.fast_config(), base, head, profile='pr-fast')
                else:
                    plan = selector.select(self.fast_config(), base, head, profile='pr-fast')
                    self.assertTrue(plan['test_policy_changed'])
                    self.assertIn(child, plan['test_policy_paths'])
        self.write(child, ordinary)
        self.write(parent, Path(parent).read_text().replace('fn existing()', 'fn replacement()'))
        plan = selector.select(self.fast_config(), base, self.commit(), profile='pr-fast')
        self.assertTrue(plan['test_policy_changed'])
        self.assertIn(parent, plan['test_policy_paths'])
        self.assertIn(child, plan['test_policy_paths'])


if __name__ == '__main__':
    unittest.main()
