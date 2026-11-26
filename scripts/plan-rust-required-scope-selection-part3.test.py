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


class SelectionPart3(unittest.TestCase):
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

    def plan(self, head=None):
        return selector.select(self.config, self.base, head or self.commit())

    def merge_fixture(self, target_path='Cargo.lock', resolution=None, target_content='// target advance'):
        self.git('checkout', '-qb', 'source', self.base)
        self.write('doc/readme.md', 'source doc')
        source = self.commit()
        self.git('checkout', '-qb', 'target', self.base)
        self.write(target_path, target_content)
        target = self.commit()
        self.git('merge', '--no-ff', '-qm', 'merge fixture', source)
        if resolution:
            self.write(*resolution)
            self.git('add', '-A')
            self.git('commit', '--amend', '-qm', 'merge resolution')
        return source, target, self.git('rev-parse', 'HEAD')

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

    def test_bounded_full_profiles_supplement_every_target_without_reducing_inventory(self):
        config = self.bounded_config()
        config['ordinary_fast_targets']['other-test'] = {
            'package': 'other', 'target_kind': 'test', 'target_name': 'fast',
            'default_features': False, 'features': [], 'cases': ['contract']}
        for profile in ('periodic-full', 'candidate-full'):
            with self.subTest(profile=profile):
                plan = selector.full_plan(config, profile, self.base, policy_ref=self.base)
                self.assertEqual(plan['full_fast_test_inventory_version'], 1)
                self.assertEqual(plan['ordinary_full_fast_targets'],
                                 [config['ordinary_fast_targets'][key]
                                  for key in sorted(config['ordinary_fast_targets'])])
                self.assertEqual(set(plan['groups']), set(config['groups']))
                self.assertEqual(plan['formal_suites'], sorted(selector.FORMAL_SUITES))
                self.assertEqual(plan['oasis7_required_packages'], ['consumer', 'core', 'other'])
                self.assertEqual(plan['oasis7_required_wasm_packages'], ['core', 'other'])
                self.assertNotIn('ordinary_fast_targets', plan)

    def test_bounded_inventory_schema_rejects_invalid_capabilities_and_targets(self):
        for field, value in (('fast_test_inventory_version', True), ('fast_test_inventory_version', 2),
                             ('ordinary_fast_targets', []), ('ordinary_fast_rules', [])):
            config = self.bounded_config()
            config[field] = value
            with self.subTest(field=field, value=value), self.assertRaises(ValueError):
                selector.validate_config(config)
        for field, value in (('package', 'unknown'), ('target_kind', 'bin'), ('target_name', '--all'),
                             ('default_features', 'false'), ('features', ['default']),
                             ('features', ['x', 'x']), ('features', ['dep/feature']),
                             ('cases', []), ('cases', ['existing', 'existing']), ('cases', ['a b'])):
            config = self.bounded_config()
            config['ordinary_fast_targets']['core-lib'][field] = value
            with self.subTest(field=field, value=value), self.assertRaises(ValueError):
                selector.validate_config(config)
        config = self.bounded_config()
        config['ordinary_fast_targets']['duplicate'] = dict(config['ordinary_fast_targets']['core-lib'])
        with self.assertRaisesRegex(ValueError, 'duplicate'):
            selector.validate_config(config)

    def test_bounded_inventory_schema_rejects_nonexact_and_duplicate_rules(self):
        for path in ('/crates/core/src/lib.rs', 'crates/core/src/../lib.rs', './crates/core/src/lib.rs',
                     'crates/core/src//lib.rs', 'crates/core/src/*.rs', 'crates/core/src/[a].rs'):
            with self.subTest(path=path), self.assertRaises(ValueError):
                selector.validate_config(self.bounded_config((path,)))
        config = self.bounded_config()
        config['ordinary_fast_rules'].append(dict(config['ordinary_fast_rules'][0]))
        with self.assertRaisesRegex(ValueError, 'duplicate'):
            selector.validate_config(config)
        for targets in (['absent'], ['core-lib', 'core-lib']):
            config = self.bounded_config()
            config['ordinary_fast_rules'][0]['targets'] = targets
            with self.subTest(targets=targets), self.assertRaises(ValueError):
                selector.validate_config(config)

    def test_bounded_declaration_rules_accept_only_one_exact_wiring_item(self):
        valid = ('pub mod incremental;', 'mod incremental;', 'pub(super) mod clock;',
                 'pub use world::incremental::{Commit, Parent,};',
                 '#[cfg(not(target_arch = "wasm32"))] pub mod incremental;')
        invalid = ('pub mod incremental; fn bypass() {}', 'pub mod incremental {}',
                   '#[cfg(feature="full")] pub mod incremental;',
                   'pub use world::incremental::*;', 'pub use world::incremental::Commit as Other;')
        for declaration in valid + invalid:
            with self.subTest(declaration=declaration):
                config = self.bounded_config()
                config['ordinary_fast_rules'][0]['added_declarations'] = [declaration]
                if declaration in valid:
                    selector.validate_config(config)
                else:
                    with self.assertRaises(ValueError):
                        selector.validate_config(config)

    def test_bounded_declarations_allow_additions_and_multiline_exports(self):
        config = self.bounded_config()
        allowed = ['pub mod incremental;', 'pub use incremental::{Commit, Parent,};']
        config['ordinary_fast_rules'][0]['added_declarations'] = allowed
        self.write('crates/core/src/lib.rs', 'pub mod incremental;\npub use incremental::{\nCommit,\nParent,\n};\n')
        plan = selector.select(config, self.base, self.commit(), profile='pr-fast')
        self.assertEqual(plan['fast_test_inventory_version'], 1)

    def test_bounded_declarations_reject_removed_replaced_or_extra_statements(self):
        config = self.bounded_config()
        config['ordinary_fast_rules'][0]['added_declarations'] = ['pub mod incremental;']
        path = 'crates/core/src/lib.rs'
        self.write(path, 'pub mod original;\n')
        base = self.commit()
        for after in ('pub mod incremental;\n', 'pub mod original;\npub mod incremental;\nfn hidden() {}\n', ''):
            self.write(path, after)
            with self.subTest(after=after):
                plan = selector.select(config, base, self.commit(), profile='pr-fast')
                self.assertNotIn('ordinary_fast_targets', plan)

    def test_bounded_declarations_check_the_tested_merge_diff(self):
        config = self.bounded_config()
        config['ordinary_fast_rules'][0]['added_declarations'] = ['pub mod incremental;']
        source, target, tested = self.merge_fixture(
            resolution=('crates/core/src/lib.rs', 'pub mod incremental;\nfn hidden() {}\n'))
        plan = selector.select(config, self.base, source, tested, profile='pr-fast')
        self.assertNotIn('ordinary_fast_targets', plan)
        self.assertEqual(plan['scope'], 'targeted')

    def test_bounded_existing_gated_test_body_keeps_expanded_features(self):
        config = self.bounded_config()
        path = 'crates/core/src/lib.rs'
        for gate in ('feature="test_tier_full"', 'all(test, feature="test_tier_full")'):
            self.write(path, f'#[cfg({gate})] #[test] fn existing() {{ assert!(true); }}\n')
            base = self.commit()
            self.write(path, f'#[cfg({gate})] #[test] fn existing() {{ assert!(false); }}\n')
            with self.subTest(gate=gate):
                plan = selector.select(config, base, self.commit(), profile='pr-fast')
                self.assertTrue(plan['test_policy_changed'])
                self.assertNotIn('ordinary_fast_targets', plan)

    def test_bounded_existing_gated_production_body_keeps_expanded_features(self):
        config = self.bounded_config()
        path = 'crates/core/src/lib.rs'
        self.write(path, '#[cfg(feature="test_tier_full")] pub fn production() -> u8 { 1 }\n')
        base = self.commit()
        self.write(path, '#[cfg(feature="test_tier_full")] pub fn production() -> u8 { 2 }\n')
        plan = selector.select(config, base, self.commit(), profile='pr-fast')
        self.assertTrue(plan['test_policy_changed'])
        self.assertNotIn('ordinary_fast_targets', plan)

    def test_bounded_existing_inherited_gate_keeps_expanded_features(self):
        path = 'crates/core/src/child.rs'
        config = self.bounded_config((path,))
        self.write('crates/core/src/lib.rs', '#[cfg(feature="test_tier_full")] mod child;\n')
        self.write(path, '#[test] fn existing() { assert!(true); }\n')
        base = self.commit()
        self.write(path, '#[test] fn existing() { assert!(false); }\n')
        plan = selector.select(config, base, self.commit(), profile='pr-fast')
        self.assertTrue(plan['test_policy_changed'])
        self.assertNotIn('ordinary_fast_targets', plan)

    def test_bounded_existing_enabled_gate_body_stays_fast(self):
        config = self.bounded_config()
        path = 'crates/core/src/lib.rs'
        for gate in ('feature="test_tier_required"', 'test', 'unix', 'not(target_arch="wasm32")'):
            self.write(path, f'#[cfg({gate})] #[test] fn existing() {{ assert!(true); }}\n')
            base = self.commit()
            self.write(path, f'#[cfg({gate})] #[test] fn existing() {{ assert!(false); }}\n')
            with self.subTest(gate=gate):
                plan = selector.select(config, base, self.commit(), profile='pr-fast')
                self.assertFalse(plan['test_policy_changed'])
                self.assertEqual(plan['fast_test_inventory_version'], 1)

    def test_bounded_new_lib_case_uses_actual_selected_target_root(self):
        config = self.bounded_config()
        self.write('crates/core/src/lib.rs', '#[test] fn existing() {}\n')
        base = self.commit()
        self.write('crates/core/src/lib.rs', '#[test] fn existing() {}\n#[test] fn added() {}\n')
        plan = selector.select(config, base, self.commit(), profile='pr-fast')
        self.assertFalse(plan['test_policy_changed'])
        self.assertEqual(plan['fast_test_inventory_version'], 1)

    def bounded_shared_fixture(self, cross_owner=False, root_contents=None):
        child = ('crates/consumer/tests/shared.rs' if cross_owner else 'crates/core/src/shared.rs')
        root = 'crates/core/tests/fast.rs'
        relative = '../../consumer/tests/shared.rs' if cross_owner else '../src/shared.rs'
        self.write('crates/core/src/lib.rs', 'mod shared;\n' if not cross_owner else '')
        self.write(root, root_contents if root_contents is not None else
                   f'mod contract {{ include!("{relative}"); }}\n')
        self.write(child, '#[test] fn existing() {}\n')
        base = self.commit()
        config = self.bounded_config((child,))
        config['ordinary_fast_targets']['core-lib'].update(target_kind='test', target_name='fast')
        self.write(child, '#[test] fn existing() {}\n#[test] fn added() {}\n')
        return config, base, self.commit(), child

    def test_bounded_shared_mod_and_include_new_case_has_selected_root_proof(self):
        config, base, head, _ = self.bounded_shared_fixture()
        plan = selector.select(config, base, head, profile='pr-fast')
        self.assertFalse(plan['test_policy_changed'])
        self.assertEqual(plan['fast_test_inventory_version'], 1)
        legacy = selector.select(self.fast_config(), base, head, profile='pr-fast')
        self.assertTrue(legacy['test_policy_changed'])

    def test_bounded_cross_package_literal_path_new_case_has_selected_root_proof(self):
        config, base, head, _ = self.bounded_shared_fixture(
            cross_owner=True, root_contents='#[path="../../consumer/tests/shared.rs"] mod contract;\n')
        plan = selector.select(config, base, head, profile='pr-fast')
        self.assertFalse(plan['test_policy_changed'])
        self.assertEqual(plan['fast_test_inventory_version'], 1)

    def test_bounded_shared_case_wrong_or_disabled_selected_root_expands(self):
        for root_contents in ('#[test] fn unrelated() {}\n',
                              '#![cfg(feature="test_tier_full")]\n'
                              'mod contract { include!("../src/shared.rs"); }\n'):
            config, base, head, _ = self.bounded_shared_fixture(root_contents=root_contents)
            with self.subTest(root_contents=root_contents):
                plan = selector.select(config, base, head, profile='pr-fast')
                self.assertTrue(plan['test_policy_changed'])
                self.assertNotIn('ordinary_fast_targets', plan)
        config['ordinary_fast_targets']['core-lib']['target_name'] = 'missing'
        self.assertNotIn('ordinary_fast_targets', selector.select(config, base, head, profile='pr-fast'))

    def test_bounded_shared_case_unselected_manifest_harness_does_not_prove_execution(self):
        self.write('crates/core/Cargo.toml', Path('crates/core/Cargo.toml').read_text() +
                   '\n[[test]]\nname="fast"\nharness=false\n')
        config, base, head, _ = self.bounded_shared_fixture()
        plan = selector.select(config, base, head, profile='pr-fast')
        self.assertTrue(plan['test_policy_changed'])
        self.assertNotIn('ordinary_fast_targets', plan)

    def test_bounded_shared_new_case_mixed_unmapped_input_keeps_legacy_proof(self):
        config, base, _, _ = self.bounded_shared_fixture()
        self.write('crates/core/src/unmapped.rs', 'pub fn changed() {}\n')
        plan = selector.select(config, base, self.commit(), profile='pr-fast')
        self.assertTrue(plan['test_policy_changed'])
        self.assertNotIn('ordinary_fast_targets', plan)

    def test_fast_history_risk_selects_logical_suite_and_cross_shard_dependencies(self):
        config = self.fast_config()
        inventory = json.loads((HERE / 'ci-formal-process-suites.json').read_text())
        self.write('crates/core/src/history.rs', 'pub fn prune() {}')
        head = self.commit()
        plan = selector.select(config, self.base, head, profile='pr-fast', formal_inventory=inventory)
        self.assertEqual(plan['formal_suites'], ['world-history'])
        self.assertEqual(plan['formal_shards'], ['world-history'])
        self.assertIn('formal_process', plan['groups'])
        self.assertEqual(selector.formal_shards(['healthy-set'], inventory),
                         ['healthy-set', 'lifecycle-cli', 'storage-recovery'])

    def test_fast_unknown_formal_mapping_and_controls_preserve_old_coverage(self):
        config = self.fast_config()
        self.write('crates/consumer/src/lib.rs', 'pub fn new_protocol_behavior() {}')
        plan = selector.select(config, self.base, self.commit(), profile='pr-fast')
        self.assertEqual(plan['formal_suites'], sorted(selector.FORMAL_SUITES))
        self.assertEqual(plan['oasis7_required_packages'], ['consumer'])
        self.write('Cargo.lock', '# shared dependency update')
        plan = selector.select(config, self.base, self.commit(), profile='pr-fast')
        self.assertEqual(plan['scope'], 'full')
        self.assertEqual(plan['oasis7_required_packages'], ['consumer', 'core', 'other'])
        self.assertEqual(plan['oasis7_required_wasm_packages'], ['core', 'other'])
        self.assertEqual(set(plan['groups']), set(config['groups']))

    def test_fast_directory_rule_without_package_graph_keeps_complete_required_inventory(self):
        config = self.fast_config()
        config['rules'].append({'match': ['data/runtime-input.json'],
                                'groups': ['oasis7_required'], 'reason': 'external fixture'})
        self.write('data/runtime-input.json', '{}')
        plan = selector.select(config, self.base, self.commit(), profile='main-fast')
        self.assertEqual(plan['oasis7_required_packages'], ['consumer', 'core', 'other'])
        self.assertEqual(plan['oasis7_required_wasm_packages'], ['core', 'other'])

    def test_fast_added_plain_and_tokio_tests_keep_the_ordinary_feature_plan(self):
        config = self.fast_config()
        path = 'crates/core/src/lib.rs'
        before = ('#[test] fn existing() {}\n'
                  '#[cfg(test)] mod tests { #[test] fn existing_nested() {} }\n')
        self.write(path, before)
        base = self.commit()
        candidates = [
            before + '#[test] fn added() {}\n',
            before + '#[tokio::test] async fn added() {}\n',
            before + '#[tokio::test(flavor = "current_thread")] async fn added() {}\n',
            before.replace('fn existing_nested() {}', 'fn existing_nested() {} #[test] fn added() {}'),
            before + '#[cfg(test)] mod additional_tests { #[test] fn added() {} }\n',
        ]
        for after in candidates:
            with self.subTest(after=after):
                self.write(path, after)
                plan = selector.select(config, base, self.commit(), profile='pr-fast')
                self.assertEqual(plan['scope'], 'targeted')
                self.assertFalse(plan['test_policy_changed'])
                self.assertEqual(plan['test_policy_paths'], [])
                self.assertEqual(plan['fast_plan_exceptions'], [])
                self.assertEqual(plan['oasis7_required_packages'], ['consumer', 'core'])

    def test_fast_test_support_helper_does_not_change_test_execution(self):
        config = self.fast_config()
        manifest = Path('crates/core/Cargo.toml').read_text()
        self.write('crates/core/Cargo.toml', manifest + 'test-support=[]\n')
        self.write('crates/core/src/lib.rs', '#[cfg(test)] mod tests { #[test] fn existing() {} }\n')
        base = self.commit()
        self.write('crates/core/src/lib.rs', '#[cfg(test)] mod tests { #[test] fn existing() {} }\n'
                   '#[cfg(any(test, feature="test-support"))] pub mod measurement_counts;\n')
        self.write('crates/core/src/measurement_counts.rs', 'pub fn count() -> usize { 0 }\n')
        plan = selector.select(config, base, self.commit(), profile='pr-fast')
        self.assertFalse(plan['test_policy_changed'])
        self.assertEqual(plan['scope'], 'targeted')
        self.assertEqual(plan['oasis7_required_packages'], ['consumer', 'core'])
        # A known --lib driver needs no expanded-feature support when `test`
        # proves this helper's condition independently of the extra feature.
        config['oasis7_required_packages'].remove('core')
        config['package_groups']['core'] = ['consensus']
        config['groups'].append('consensus')
        config['resources']['consensus'] = []
        plan = selector.select(config, base, self.git('rev-parse', 'HEAD'), profile='pr-fast')
        self.assertFalse(plan['test_policy_changed'])

    def test_fast_test_support_boolean_proof_preserves_declaration_and_gate_checks(self):
        config = self.fast_config()
        path = 'crates/core/src/lib.rs'
        self.write(path, '#[test] fn existing() {}\n')
        base = self.commit()
        self.write(path, '#[test] fn existing() {}\n'
                   '#[cfg(any(test, feature="undeclared"))] mod helper {}\n')
        with self.assertRaisesRegex(selector.TestPolicyError, 'not declared by its executing package'):
            selector.select(config, base, self.commit(), profile='pr-fast')
        manifest = Path('crates/core/Cargo.toml').read_text()
        self.write('crates/core/Cargo.toml', manifest + 'test-support=[]\n')
        self.write(path, '#[test] fn existing() {}\n')
        base = self.commit()
        for gate in ('all(test, feature="test-support")', 'not(test)',
                     'all(any(test, feature="test-support"), any())'):
            self.write(path, f'#[cfg({gate})] #[test] fn existing() {{}}\n')
            with self.subTest(gate=gate), self.assertRaisesRegex(
                    selector.TestPolicyError, 'validated test feature combinations'):
                selector.select(config, base, self.commit(), profile='pr-fast')

    def pixel_config(self):
        config = self.fast_config()
        config['groups'].append('pixel_world_bridge')
        config['resources']['pixel_world_bridge'] = []
        config['package_groups']['pixel_world_bridge'] = ['pixel_world_bridge']
        self.write('crates/pixel_world_bridge/Cargo.toml',
                   '[package]\nname="pixel_world_bridge"\nversion="0.1.0"\n'
                   '[features]\ndefault=["webgl2_runtime"]\nwebgl2_runtime=[]\noptional=[]\n')
        self.write('crates/pixel_world_bridge/src/lib.rs',
                   '#[cfg(feature="webgl2_runtime")] #[path="navigation.rs"] mod navigation;\n')
        self.write('crates/pixel_world_bridge/src/navigation.rs', 'pub fn existing() {}\n')
        return config, self.commit()

    def test_pixel_new_library_tests_and_platform_statements_keep_real_driver(self):
        config, base = self.pixel_config()
        self.write('crates/pixel_world_bridge/src/navigation.rs',
                   'pub fn existing() {}\npub fn finish_overview() {\n'
                   '#[cfg(target_arch="wasm32")] let _ = json!({"status": "done"});\n'
                   '#[cfg(not(target_arch="wasm32"))] let _ = 1;\n}\n'
                   '#[test] fn direct_case() {}\n'
                   '#[cfg(test)] mod tests { #[test] fn added() {} }\n')
        self.write('crates/pixel_world_bridge/src/lib.rs',
                   '#[cfg(feature="webgl2_runtime")] #[path="navigation.rs"] mod navigation;\nmod overview;\n')
        self.write('crates/pixel_world_bridge/src/overview.rs',
                   '#[cfg(test)] mod tests { #[test] fn query() {} }\n')
        plan = selector.select(config, base, self.commit(), profile='pr-fast')
        self.assertEqual(plan['scope'], 'targeted')
        self.assertIn('pixel_world_bridge', plan['groups'])
        self.assertFalse(plan['test_policy_changed'])
        self.assertEqual(plan['formal_suites'], [])
        self.assertEqual(plan['oasis7_required_packages'], [])

    def test_pixel_platform_statement_proof_rejects_existing_opaque_and_unknown_context(self):
        config, base = self.pixel_config()
        path = 'crates/pixel_world_bridge/src/navigation.rs'
        statements = [
            'pub fn existing() { #[cfg(target_arch="wasm32")] let _ = 1; }',
            'pub fn existing() {} fn new() { #[cfg(windows)] let _ = 1; }',
            'pub fn existing() {} fn new() { #[cfg(unknown_target)] let _ = 1; }',
            'pub fn existing() {} fn new() { #[cfg(feature="optional")] let _ = 1; }',
            'pub fn existing() {} fn new() { #[cfg_attr(unix,cfg(target_arch="wasm32"))] let _ = 1; }',
            *('pub fn existing() {} fn new() { make!' + left +
              '#[cfg(target_arch="wasm32")] let _ = 1;' + right + '; }'
              for left, right in [('(', ')'), ('[', ']'), ('{', '}')]),
            'pub fn existing() {} impl Thing { fn new() { #[cfg(target_arch="wasm32")] let _ = 1; } }',
            'pub fn existing() {} #[cfg(unix)] fn new() { #[cfg(target_arch="wasm32")] let _ = 1; }',
            'pub fn existing() {} #[cfg(unix)] mod native { fn new() { #[cfg(target_arch="wasm32")] let _ = 1; } }',
        ]
        for content in statements:
            with self.subTest(content=content):
                self.write(path, content)
                result = selector.select(config, base, self.commit(), profile='pr-fast')
                self.assertEqual(result['scope'], 'full')
        self.write(path, 'pub fn existing() {} #[cfg(target_arch="wasm32")] #[test] fn wasm_case() {}')
        with self.assertRaises(selector.TestPolicyError):
            selector.select(config, base, self.commit(), profile='pr-fast')

    def test_pixel_platform_statement_cannot_hide_unknown_route_or_retired_test(self):
        config, base = self.pixel_config()
        path = 'crates/pixel_world_bridge/src/navigation.rs'
        added = 'pub fn existing() {} fn new() { #[cfg(target_arch="wasm32")] let _ = 1; }'
        self.write(path, added)
        self.write('crates/pixel_world_bridge/src/lib.rs',
                   '#[path="navigation.rs"] mod known;\n'
                   '#[cfg(unknown_target)] #[path="navigation.rs"] mod unknown;\n')
        self.assertEqual(selector.select(config, base, self.commit(), profile='pr-fast')['scope'], 'full')
        self.write('crates/pixel_world_bridge/src/lib.rs', 'mod navigation;\n')
        self.write(path, 'pub fn existing() {} #[test] fn retired() {}\n')
        base = self.commit()
        self.write(path, added)
        result = selector.select(config, base, self.commit(), profile='pr-fast')
        self.assertEqual(result['scope'], 'full')
        self.assertTrue(result['test_policy_changed'])

    def test_full_artifact_and_support_partitions_require_trusted_capabilities(self):
        config = self.fast_config()
        config.update(ordinary_artifact_partition_version=1, workspace_support_partition_version=1)
        self.write('Cargo.toml', Path('Cargo.toml').read_text() + '\n# control input\n')
        head = self.commit()
        complete = selector.select(config, self.base, head, profile='pr-fast')
        self.assertEqual(complete['ordinary_partitions'], ['controls', 'runtime', 'component-libs', 'integrations-0', 'integrations-1'])
        for profile in ('periodic-full', 'candidate-full'):
            full = selector.full_plan(config, profile, head)
            self.assertEqual(full['ordinary_partitions'], complete['ordinary_partitions'])
        old = copy.deepcopy(config)
        old.pop('ordinary_artifact_partition_version')
        old.pop('workspace_support_partition_version')
        self.assertNotIn('ordinary_partitions', selector.select(old, self.base, head, profile='pr-fast'))
        for field in ('ordinary_artifact_partition_version', 'workspace_support_partition_version'):
            for invalid in (True, 3 if field == 'ordinary_artifact_partition_version' else 2, None):
                with self.subTest(field=field, invalid=invalid), self.assertRaises(ValueError):
                    selector.validate_config(dict(config, **{field: invalid}))

    def test_fast_new_native_items_keep_affected_packages_and_formal_risks(self):
        config = self.fast_config()
        path = 'crates/core/src/lib.rs'
        self.write(path, 'pub fn existing() {}\n')
        base = self.commit()
        for addition in ('#[cfg(not(target_arch="wasm32"))] pub fn added() {}\n',
                         '#[cfg(all(unix, not(target_arch="wasm32")))] mod native;\n'):
            self.write(path, 'pub fn existing() {}\n' + addition)
            self.write('crates/core/src/native.rs', 'pub fn behavior() {}\n')
            with self.subTest(addition=addition):
                plan = selector.select(config, base, self.commit(), profile='pr-fast')
                self.assertEqual(plan['scope'], 'targeted')
                self.assertEqual(plan['oasis7_required_packages'], ['consumer', 'core'])
                self.assertEqual(plan['formal_suites'], [])
                self.assertFalse(plan['test_policy_changed'])
        self.write(path, 'pub fn existing() {}\nmod history;\n')
        self.write('crates/core/src/history.rs', '#[cfg(unix)] pub fn added_history() {}\n')
        plan = selector.select(config, base, self.commit(), profile='pr-fast')
        self.assertEqual(plan['formal_suites'], ['world-history'])

    def test_fast_new_native_items_do_not_hide_existing_or_unknown_gates(self):
        config = self.fast_config()
        path = 'crates/core/src/lib.rs'
        self.write(path, 'pub fn existing() {}\n')
        base = self.commit()
        for after in ('#[cfg(unix)] pub fn existing() {}\n',
                      'pub fn existing() {}\n#[cfg(windows)] pub fn added() {}\n',
                      'pub fn existing() {}\n#[cfg(unknown_target)] pub fn added() {}\n',
                      'pub fn existing() {}\ngenerate! { #[cfg(unix)] pub fn added() {} }\n'):
            self.write(path, after)
            with self.subTest(after=after):
                plan = selector.select(config, base, self.commit(), profile='pr-fast')
                self.assertEqual(plan['scope'], 'full')

    def test_fast_native_integration_file_and_literal_child_use_ordinary_driver(self):
        config = self.fast_config()
        parent = 'crates/core/tests/behavior.rs'
        child = 'crates/core/tests/support/behavior.rs'
        base = self.git('rev-parse', 'HEAD')
        self.write(parent, '#![cfg(all(unix, not(target_arch="wasm32")))]\n'
                   '#[path="support/behavior.rs"] mod support;\n')
        self.write(child, '#[test] fn added() {}\n')
        plan = selector.select(config, base, self.commit(), profile='pr-fast')
        self.assertFalse(plan['test_policy_changed'])
        self.assertEqual(plan['scope'], 'targeted')
        self.assertEqual(plan['oasis7_required_packages'], ['core'])
        base = self.git('rev-parse', 'HEAD')
        self.write(child, '#[test] fn added() {}\n#[test] fn another() {}\n')
        plan = selector.select(config, base, self.commit(), profile='pr-fast')
        self.assertFalse(plan['test_policy_changed'])

    def test_fast_external_module_requires_every_parent_and_real_harness(self):
        config = self.fast_config()
        parent = 'crates/core/tests/behavior.rs'
        child = 'crates/core/tests/support/behavior.rs'
        self.write(parent, '#[path="support/behavior.rs"] mod support;\n')
        self.write(child, '#[test] fn existing() {}\n')
        self.write('crates/core/tests/full.rs', '#![cfg(feature="test_tier_full")]\n'
                   '#[path="support/behavior.rs"] mod full_support;\n')
        base = self.commit()
        self.write(child, '#[test] fn existing() {}\n#[test] fn added() {}\n')
        plan = selector.select(config, base, self.commit(), profile='pr-fast')
        self.assertTrue(plan['test_policy_changed'])

        self.assertIn(child, plan['test_policy_paths'])
        self.write('crates/core/tests/full.rs', '')
        self.write('crates/core/Cargo.toml', Path('crates/core/Cargo.toml').read_text()
                   + '\n[[test]]\nname="behavior"\npath="tests/behavior.rs"\nharness=false\n')
        self.write(child, '#[test] fn existing() {}\n')
        base = self.commit()
        self.write(child, '#[test] fn existing() {}\n#[test] fn added() {}\n')
        plan = selector.select(config, base, self.commit(), profile='pr-fast')
        self.assertTrue(plan['test_policy_changed'])

    def test_fast_raw_path_cannot_hide_a_full_parent_of_a_shared_test_file(self):
        config = self.fast_config()
        child = 'crates/core/tests/support/cases.rs'
        self.write('crates/core/tests/ordinary.rs', '#[path="support/cases.rs"] mod ordinary;\n')
        self.write(child, '#[test] fn existing() {}\n')
        for spelling in ('r"support/cases.rs"', 'r#"support/cases.rs"#'):
            self.write('crates/core/tests/full.rs', '#![cfg(feature="test_tier_full")]\n'
                       f'#[path={spelling}] mod expanded;\n')
            self.write(child, '#[test] fn existing() {}\n')
            base = self.commit()
            self.write(child, '#[test] fn existing() {}\n#[test] fn added() {}\n')
            with self.subTest(spelling=spelling):
                plan = selector.select(config, base, self.commit(), profile='pr-fast')
                self.assertTrue(plan['test_policy_changed'])


if __name__ == '__main__':
    unittest.main()
