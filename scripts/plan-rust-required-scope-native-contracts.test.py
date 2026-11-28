#!/usr/bin/env python3
import copy
import hashlib
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

    def component_config(self):
        config = self.bounded_config()
        config['groups'].append('launcher_web')
        config['resources']['launcher_web'] = []
        config['package_groups']['core'].append('launcher_web')
        config.update(fast_execution_version=1, architecture_build_policy_version=1,
                      ordinary_fast_component_rules=[{
            'match': ['crates/core/src/**'], 'exclude': ['crates/core/src/bin/**'],
            'targets': ['core-lib'], 'reason': 'whole component behavior'}])
        return config


class NativeSupportContracts(_ScopeFixtureBase):
    OWNER = 'oasis7_game_client'
    LEAF = 'crates/oasis7_game_client/src/dev_test_service_cli.rs'
    TEST = 'crates/oasis7_game_client/tests/dev_test_service_cli.rs'
    CONSUMERS = ['oasis7_agent_harness', 'oasis7_model_provider']

    def fixture(self, *, portable=False):
        config = json.loads((HERE / 'ci-required-scope.json').read_text())
        # This is a real Git/module/dependency fixture, not a mocked fast plan.
        packages = [self.OWNER, *self.CONSUMERS, 'oasis7_chain_runtime']
        for package in packages:
            manifest = f'[package]\nname="{package}"\nversion="0.1.0"\n'
            if package != self.OWNER:
                manifest += f'[dependencies]\n{self.OWNER}={{path="../{self.OWNER}"}}\n'
            self.write(f'crates/{package}/Cargo.toml', manifest)
            self.write(f'crates/{package}/src/lib.rs', '')
        gate = '' if portable else '#[cfg(unix)]\n'
        self.write(f'crates/{self.OWNER}/src/lib.rs', gate + 'mod dev_test_service_cli;\n'
                   + '#[cfg(unix)]\nmod dev_test_service;\npub mod commands;\n')
        self.write(self.LEAF, 'pub(crate) fn run() -> u8 { 1 }\n')
        contract = config['ordinary_fast_support_contracts'][0]
        callers = {f'crates/{self.OWNER}/src/commands.rs':
                   'pub fn run() { crate::dev_test_service_cli::run(); }\n',
                   f'crates/{self.OWNER}/src/bin/{self.OWNER}.rs':
                   f'fn main() {{ {self.OWNER}::commands::run(); }}\n'}
        for path, source in callers.items():
            self.write(path, source)
        contract['callers'] = {path: hashlib.sha256(source.encode()).hexdigest() for path, source in callers.items()}
        self.write(f'crates/{self.OWNER}/src/dev_test_service.rs', 'pub fn protected() {}\n')
        for target in ('identity_cli', 'dev_test_catalog_cli', 'legacy_profile', 'dev_test_service_cli'):
            self.write(f'crates/{self.OWNER}/tests/{target}.rs',
                       '#![cfg(unix)]\n#[test] fn contract() {}\n')
        return config, self.commit()

    def change(self, path=None):
        path = path or self.LEAF
        self.write(path, Path(path).read_text().replace('{ 1 }', '{ 2 }')
                   + ('#[test] fn added_regression() {}\n' if '/tests/' in path else ''))
        return self.commit()

    def test_private_unix_adapter_replaces_only_registered_support_consumers(self):
        config, base = self.fixture()
        head = self.change()
        for profile in ('pr-fast', 'main-fast'):
            plan = selector.select(config, base, head, profile=profile)
            self.assertEqual(set(plan['groups']), {'baseline', 'rust_baseline', 'oasis7_required'})
            self.assertEqual(plan['ordinary_execution'], 'inline')
            self.assertEqual(plan['ordinary_support_handoffs'], [self.OWNER])
            self.assertEqual(plan['ordinary_support_deferred'], self.CONSUMERS)
            self.assertEqual(plan['workspace_support_profiles'], [])
            self.assertEqual(plan['oasis7_required_wasm_packages'], [])
            self.assertEqual(plan['formal_suites'], [])
            self.assertEqual(plan['architecture']['build_policy'], 'ordinary-targets')
            self.assertTrue(set(self.CONSUMERS) <= set(plan['affected_packages']))
            self.assertEqual({target['package'] for target in plan['ordinary_fast_targets']}, {self.OWNER})
            self.assertEqual(len(plan['ordinary_fast_targets']), 5)

    def test_independent_cli_regression_promotes_support_to_real_inline_targets(self):
        config, base = self.fixture()
        plan = selector.select(config, base, self.change(self.TEST), profile='pr-fast')
        self.assertEqual(plan['ordinary_execution'], 'inline')
        self.assertEqual(plan['ordinary_support_handoffs'], [self.OWNER])
        self.assertEqual(plan['ordinary_support_deferred'], [])
        self.assertNotIn('workspace_support', plan['groups'])
        self.assertEqual(plan['affected_packages'], [self.OWNER])
        self.assertEqual(plan['architecture']['build_policy'], 'ordinary-targets')

    def test_portable_mount_and_host_reader_keep_support_and_platform_checks(self):
        for mode in ('portable', 'host'):
            config, base = self.fixture(portable=mode == 'portable')
            if mode == 'host':
                self.write(f'crates/{self.OWNER}/build.rs',
                           '#[path="src/dev_test_service_cli.rs"] mod adapter;\nfn main() {}\n')
                base = self.commit()
            plan = selector.select(config, base, self.change(), profile='pr-fast')
            self.assertIn('workspace_support', plan['groups'])
            self.assertNotIn('ordinary_support_handoffs', plan)
            self.assertNotIn('build_policy', plan['architecture'])

    def test_mixed_security_source_and_explicit_support_request_keep_original_checks(self):
        config, base = self.fixture()
        self.write(f'crates/{self.OWNER}/src/dev_test_service.rs', 'pub fn protected() { panic!(); }\n')
        plan = selector.select(config, base, self.change(), profile='pr-fast')
        self.assertNotIn('ordinary_support_handoffs', plan)
        self.assertNotIn('ordinary_execution', plan)
        self.assertIn('workspace_support', plan['groups'])
        self.git('reset', '--hard', base)
        plan = selector.select(config, base, self.change(), profile='pr-fast', additions=['workspace_support'])
        self.assertNotIn('ordinary_support_handoffs', plan)
        self.assertEqual(set(plan['workspace_support_profiles']), set(selector.SUPPORT_PROFILES))

    def test_mixed_direct_consumer_and_cross_package_path_contract_are_not_deferred(self):
        config, base = self.fixture()
        consumer = self.CONSUMERS[0]
        self.write(f'crates/{consumer}/tests/direct.rs', '#[test] fn direct() {}\n')
        plan = selector.select(config, base, self.change(), profile='pr-fast')
        self.assertIn(consumer, plan['workspace_support_profiles'])
        self.assertIn('workspace_support', plan['groups'])
        self.assertNotIn('build_policy', plan['architecture'])
        self.git('reset', '--hard', base)
        config['workspace_support_profile_paths'][self.OWNER] = ['doc/readme.md']
        self.write('doc/readme.md', 'direct audited support contract')
        plan = selector.select(config, base, self.change(), profile='pr-fast')
        self.assertIn(self.OWNER, plan['workspace_support_profiles'])
        self.assertNotIn('ordinary_support_handoffs', plan)

    def test_old_policy_and_full_profiles_cannot_borrow_support_handoff(self):
        config, base = self.fixture()
        old = copy.deepcopy(config)
        old.pop('ordinary_support_contract_version', None)
        old.pop('ordinary_fast_support_contracts', None)
        plan = selector.select(old, base, self.change(self.TEST), profile='pr-fast')
        self.assertNotIn('ordinary_support_handoffs', plan)
        self.assertNotIn('ordinary_execution', plan)
        self.assertIn('workspace_support', plan['groups'])
        for profile in ('periodic-full', 'candidate-full'):
            plan = selector.full_plan(config, profile, self.git('rev-parse', 'HEAD'))
            self.assertEqual(set(plan['workspace_support_profiles']), set(selector.SUPPORT_PROFILES))
            self.assertEqual(set(plan['groups']), set(config['groups']))
            self.assertNotIn('ordinary_support_handoffs', plan)

    def test_real_legacy_cli_body_retains_registered_complete_targets(self):
        config, _ = self.fixture()
        source = (HERE.parent / self.LEAF).read_text()
        self.write(self.LEAF, source)
        base = self.commit()
        changed = source.replace('args.len() != 3', 'args.len() != 3 || args.is_empty()')
        self.assertNotEqual(source, changed)
        self.write(self.LEAF, changed)
        plan = selector.select(config, base, self.commit(), profile='pr-fast')
        self.assertEqual(set(plan['groups']), {'baseline', 'rust_baseline', 'oasis7_required'})
        self.assertEqual(plan['ordinary_execution'], 'inline')
        self.assertEqual(plan['ordinary_support_handoffs'], [self.OWNER])
        self.assertEqual(plan['ordinary_support_deferred'], self.CONSUMERS)
        self.assertEqual({target['target_name'] for target in plan['ordinary_fast_targets']},
                         {self.OWNER, 'identity_cli', 'dev_test_catalog_cli',
                          'legacy_profile', 'dev_test_service_cli'})

    def test_platform_adapter_without_registered_module_keeps_full_coverage(self):
        config, _ = self.fixture()
        # Preserve the rejected platform boundary independently of the current
        # production leaf, which must remain limited to the legacy commands.
        source = ('pub(crate) fn run(args: &[String]) -> Result<(), String> {\n'
                  '  #[cfg(target_os = "linux")]\n'
                  '  if args.len() != 3 { return Err("invalid arguments".into()); }\n'
                  '  Ok(())\n}\n')
        self.assertIn('#[cfg(', source)
        self.write(self.LEAF, source)
        base = self.commit()
        self.write(self.LEAF, source.replace('args.len() != 3',
                                            'args.len() != 3 || args.is_empty()'))
        head = self.commit()
        plan = selector.select(config, base, head, profile='pr-fast')
        self.assertNotIn('ordinary_support_handoffs', plan)
        self.assertNotIn('ordinary_execution', plan)
        self.assertNotIn('ordinary_fast_targets', plan)
        self.assertNotIn('build_policy', plan['architecture'])
        self.assertIn('workspace_support', plan['groups'])
        self.assertEqual(set(plan['workspace_support_profiles']), {self.OWNER, *self.CONSUMERS})
        self.assertEqual(plan['scope'], 'targeted')
        self.assertTrue(any('production cfg changed; scoped package and platform coverage' in reason
                            for reason in plan['reasons']))
        legacy = copy.deepcopy(config)
        legacy.pop('scoped_production_cfg_version', None)
        self.assertEqual(selector.select(legacy, base, head, profile='pr-fast')['scope'], 'full')

    def test_local_adapter_and_public_dispatch_cannot_borrow_legacy_handoff(self):
        config, _ = self.fixture()
        local = f'crates/{self.OWNER}/src/dev_test_local_adapter.rs'
        parent = f'crates/{self.OWNER}/src/lib.rs'
        dispatch = f'crates/{self.OWNER}/src/commands.rs'
        self.write(local, 'pub fn transport() -> u8 { 1 }\n')
        self.write(parent, Path(parent).read_text()
                   + '#[cfg(unix)]\npub mod dev_test_local_adapter;\n')
        base = self.commit()
        for path in (local, dispatch):
            for mixed in (False, True):
                with self.subTest(path=path, mixed=mixed):
                    self.git('reset', '--hard', base)
                    self.write(path, Path(path).read_text() + '\npub fn new_entry() {}\n')
                    head = self.change() if mixed else self.commit()
                    plan = selector.select(config, base, head, profile='pr-fast')
                    self.assertNotIn('ordinary_support_handoffs', plan)
                    self.assertNotIn('ordinary_execution', plan)
                    self.assertIn('workspace_support', plan['groups'])

    def test_adapter_body_is_supported_but_new_items_macros_and_exports_are_not(self):
        config, _ = self.fixture()
        # Keep this synthetic positive input inside the registered contract;
        # real source and platform boundaries have independent cases above.
        source = ('pub(crate) fn run(args: &[String]) -> Result<(), String> { '
                  'if args.len() != 3 { return Err("invalid arguments".into()); } '
                  'match args[0].as_str() { "serve" => Ok(()), '
                  '_ => Err("unsupported command".into()) } }\n')
        self.write(self.LEAF, source)
        base = self.commit()
        changed = source.replace('args.len() != 3', 'args.len() != 3 || args.is_empty()')
        self.assertNotEqual(source, changed)
        self.write(self.LEAF, changed)
        plan = selector.select(config, base, self.commit(), profile='pr-fast')
        self.assertEqual(plan['ordinary_support_handoffs'], [self.OWNER])
        for source in (
            'pub(crate) fn run(args: &[String]) -> Result<(), String> { impl Local {} Ok(()) }\n',
            'pub(crate) fn run(args: &[String]) -> Result<(), String> { custom!(); Ok(()) }\n',
            'pub fn run(args: &[String]) -> Result<(), String> { Ok(()) }\n',
            changed + '\nmod extra;\n',
            changed + '\npub(crate) fn added() {}\n',
        ):
            self.write(self.LEAF, source)
            with self.subTest(source=source):
                plan = selector.select(config, base, self.commit(), profile='pr-fast')
                self.assertNotIn('ordinary_support_handoffs', plan)
                self.assertNotIn('ordinary_execution', plan)
                self.assertIn('workspace_support', plan['groups'])

    def test_candidate_cannot_change_stable_mount_or_retire_cli_cases(self):
        config, base = self.fixture()
        self.write(f'crates/{self.OWNER}/src/lib.rs',
                   '#[cfg(unix)] pub mod dev_test_service_cli;\n')
        plan = selector.select(config, base, self.change(), profile='pr-fast')
        self.assertNotIn('ordinary_support_handoffs', plan)
        self.git('reset', '--hard', base)
        self.write(self.TEST, '#![cfg(unix)]\n#[test] #[ignore] fn contract() {}\n')
        with self.assertRaises(selector.TestPolicyError):
            selector.select(config, base, self.commit(), profile='pr-fast')

    def test_contract_rejects_incomplete_targets_and_unknown_capability(self):
        config, _ = self.fixture()
        for key, value in (('ordinary_support_contract_version', True),
                           ('ordinary_support_contract_version', 2),
                           ('ordinary_fast_support_contracts', [])):
            with self.subTest(key=key), self.assertRaises(ValueError):
                selector.validate_config(config | {key: value})
        for key, value in (('default_features', False), ('features', ['test_tier_required'])):
            changed = copy.deepcopy(config)
            changed['ordinary_fast_targets']['game-client-lib'][key] = value
            with self.subTest(key=key), self.assertRaises(ValueError):
                selector.validate_config(changed)

    def test_preexisting_public_wrapper_and_new_consumer_import_invalidate_old_contract(self):
        config, base = self.fixture()
        root = f'crates/{self.OWNER}/src/lib.rs'
        self.write(root, Path(root).read_text()
                   + 'pub fn sdk_behavior() -> u8 { dev_test_service_cli::run() }\n')
        consumer = f'crates/{self.CONSUMERS[0]}/src/lib.rs'
        self.write(consumer, f'pub fn sdk() -> u8 {{ {self.OWNER}::sdk_behavior() }}\n')
        target = self.commit()
        plan = selector.select(config, target, self.change(), profile='pr-fast')
        self.assertNotIn('ordinary_support_handoffs', plan)
        self.assertNotIn('ordinary_execution', plan)
        self.git('reset', '--hard', base)
        self.write(root, Path(root).read_text()
                   + 'pub fn sdk_behavior() -> u8 { r#dev_test_service_cli::run() }\n')
        target = self.commit()
        plan = selector.select(config, target, self.change(), profile='pr-fast')
        self.assertNotIn('ordinary_support_handoffs', plan)
        for source in (f'use {self.OWNER}::commands as cli;\npub fn sdk() {{ cli::run(); }}\n',
                       f'use {self.OWNER}::{{commands as cli}};\npub fn sdk() {{ cli::run(); }}\n',
                       f'use {self.OWNER}::*;\npub fn sdk() {{ commands::run(); }}\n',
                       f'use {self.OWNER}::r#commands as cli;\npub fn sdk() {{ cli::run(); }}\n',
                       f'use {self.OWNER}::*;\nmacro_rules! invoke {{ ($m:ident) => {{ $m::run() }} }}\n'
                       + 'pub fn sdk() { invoke!(commands); }\n'):
            self.git('reset', '--hard', base)
            self.write(consumer, source)
            target = self.commit()
            plan = selector.select(config, target, self.change(), profile='pr-fast')
            with self.subTest(source=source):
                self.assertNotIn('ordinary_support_handoffs', plan)
                self.assertIn(self.CONSUMERS[0], plan['workspace_support_profiles'])

    def test_exported_macro_without_a_dependency_edge_cannot_hide_a_cli_consumer(self):
        config, _ = self.fixture()
        self.write('crates/macro_tools/Cargo.toml', '[package]\nname="macro_tools"\nversion="0.1.0"\n')
        self.write('crates/macro_tools/src/lib.rs',
                   '#[macro_export] macro_rules! call_game_client { () => { oasis7_game_client::commands::run() } }\n')
        manifest = f'crates/{self.CONSUMERS[0]}/Cargo.toml'
        self.write(manifest, Path(manifest).read_text() + 'macro_tools={path="../macro_tools"}\n')
        self.write(f'crates/{self.CONSUMERS[0]}/src/lib.rs',
                   'pub fn sdk() { macro_tools::call_game_client!(); }\n')
        target = self.commit()
        plan = selector.select(config, target, self.change(), profile='pr-fast')
        self.assertNotIn('ordinary_support_handoffs', plan)
        self.assertIn(self.CONSUMERS[0], plan['workspace_support_profiles'])
        self.write('crates/macro_tools/src/lib.rs', 'mod commands {}\n'
                   + '#[macro_export] macro_rules! call_game_client { ($method:ident) => { commands::$method() } }\n')
        self.write(f'crates/{self.CONSUMERS[0]}/src/lib.rs',
                   f'use {self.OWNER}::*;\npub fn sdk() {{ macro_tools::call_game_client!(run); }}\n')
        target = self.commit()
        self.write(self.LEAF, 'pub(crate) fn run() -> u8 { 3 }\n')
        plan = selector.select(config, target, self.commit(), profile='pr-fast')
        self.assertNotIn('ordinary_support_handoffs', plan)

    def test_unrelated_local_commands_and_asserted_data_fields_do_not_disable_real_contract(self):
        config, _ = self.fixture()
        self.write('crates/other/src/lib.rs',
                   'mod commands { pub fn apply() {} }\npub fn ordinary() { commands::apply(); }\n'
                   + '#[test] fn data() { let mut commands = std::collections::BTreeSet::new(); '
                   + 'assert!(commands.insert("value")); struct Manifest { commands: Vec<u8> } '
                   + 'let manifest = Manifest { commands: vec![] }; assert!(manifest.commands.is_empty()); '
                   + 'let commands = [1]; assert_eq!(commands[0], 1); assert_eq!(commands, [1]); }\n')
        base = self.commit()
        plan = selector.select(config, base, self.change(), profile='pr-fast')
        self.assertEqual(plan['ordinary_support_handoffs'], [self.OWNER])
        self.write('crates/other/src/lib.rs', '#[macro_export] macro_rules! assert { ($m:ident) => { $m::run() } }\n')
        target = self.commit()
        self.write(self.LEAF, 'pub(crate) fn run() -> u8 { 3 }\n')
        plan = selector.select(config, target, self.commit(), profile='pr-fast')
        self.assertNotIn('ordinary_support_handoffs', plan)

    def test_local_proc_macro_in_consumer_dependencies_requires_generator_review(self):
        config, _ = self.fixture()
        self.write('crates/macro_tools/Cargo.toml',
                   '[package]\nname="macro_tools"\nversion="0.1.0"\n[lib]\nproc-macro=true\n')
        self.write('crates/macro_tools/src/lib.rs', 'extern crate proc_macro;\n#[proc_macro]\n'
                   + 'pub fn call(_:proc_macro::TokenStream)->proc_macro::TokenStream { '
                   + '"oasis7_game_client::commands::run()".parse().unwrap() }\n')
        manifest = f'crates/{self.CONSUMERS[0]}/Cargo.toml'
        self.write(manifest, Path(manifest).read_text() + 'macro_tools={path="../macro_tools"}\n')
        self.write(f'crates/{self.CONSUMERS[0]}/src/lib.rs', 'pub fn sdk() { macro_tools::call!(); }\n')
        target = self.commit()
        plan = selector.select(config, target, self.change(), profile='pr-fast')
        self.assertNotIn('ordinary_support_handoffs', plan)
        self.assertIn(self.CONSUMERS[0], plan['workspace_support_profiles'])

    def test_unknown_macro_cannot_generate_a_shadowed_standard_assertion(self):
        config, _ = self.fixture()
        self.write('crates/macro_tools/Cargo.toml', '[package]\nname="macro_tools"\nversion="0.1.0"\n')
        manifest = f'crates/{self.CONSUMERS[0]}/Cargo.toml'
        self.write(manifest, Path(manifest).read_text() + 'macro_tools={path="../macro_tools"}\n')
        for factory, invocation in (
            ('#[macro_export] macro_rules! define_expr { ($name:ident,$d:tt) => { '
             'macro_rules! $name { ($d module:ident) => { $d module::run() }; } }; }\n',
             'macro_tools::define_expr!(assert, $);'),
            ('#[macro_export] macro_rules! forward { ($m:ident) => { $m::run() }; }\n'
             '#[macro_export] macro_rules! import_as { ($p:path,$name:ident) => { use $p as $name; }; }\n',
             'macro_tools::import_as!(macro_tools::forward, assert);'),
        ):
            self.write('crates/macro_tools/src/lib.rs', factory)
            self.write(f'crates/{self.CONSUMERS[0]}/src/lib.rs',
                       f'use {self.OWNER}::*;\n{invocation}\npub fn sdk() {{ assert!(commands); }}\n')
            self.write(self.LEAF, 'pub(crate) fn run() -> u8 { 1 }\n')
            target = self.commit()
            plan = selector.select(config, target, self.change(), profile='pr-fast')
            with self.subTest(factory=factory):
                self.assertNotIn('ordinary_support_handoffs', plan)
                self.assertIn(self.CONSUMERS[0], plan['workspace_support_profiles'])


class AuthorityConsumerContracts(_ScopeFixtureBase):
    def fixture(self, *, disabled=False, harness=True):
        config = self.component_config()
        config.update(ordinary_fast_group_contract_version=1,
                      ordinary_fast_group_contracts={}, formal_full_profiles_only_version=1)
        dependencies = []
        for group, package in selector.ORDINARY_GROUP_LIBRARIES.items():
            config['groups'].append(group)
            config['resources'][group] = []
            config['package_groups'][package] = [group, 'formal_process']
            source = f'crates/{package}/src/authority.rs'
            config['ordinary_fast_group_contracts'][group] = [source]
            config['ordinary_fast_rules'].append(dict(paths=[source], targets=['core-lib'],
                                                      reason='complete independent group and real consumer'))
            self.write(f'crates/{package}/Cargo.toml',
                       f'[package]\nname="{package}"\nversion="0.1.0"\n'
                       f'[lib]\nharness={str(harness).lower()}\n'
                       '[features]\ndefault=["live"]\nlive=[]\noptional=[]\n')
            gate = 'optional' if disabled else 'live'
            self.write(f'crates/{package}/src/lib.rs',
                       f'#[cfg(feature="{gate}")] pub mod authority;\n')
            self.write(source, 'pub fn value() -> u8 { 1 }\n')
            dependencies.append(f'{package}={{path="../{package}"}}')
        manifest = Path('crates/core/Cargo.toml').read_text()
        self.write('crates/core/Cargo.toml', manifest + '[dependencies]\n' + '\n'.join(dependencies) + '\n')
        self.write('crates/core/src/lib.rs', '#[test] fn existing() {}\n')
        return config, self.commit()

    def change(self, group='consensus'):
        path = f'crates/{selector.ORDINARY_GROUP_LIBRARIES[group]}/src/authority.rs'
        self.write(path, 'pub fn value() -> u8 { 2 }\n')
        return self.commit()

    def test_default_group_root_proves_input_without_duplicating_its_execution(self):
        config, base = self.fixture()
        self.change()
        head = self.change('node')
        for profile in ('pr-fast', 'main-fast'):
            plan = selector.select(config, base, head, profile=profile)
            self.assertEqual(plan['ordinary_fast_targets'], [config['ordinary_fast_targets']['core-lib']])
            self.assertEqual(plan['ordinary_fast_retained_groups'], ['consensus', 'node'])
            self.assertTrue({'consensus', 'node'} <= set(plan['groups']))
            self.assertFalse(plan['test_policy_changed'])
            self.assertNotIn('formal_process', plan['groups'])

    def test_old_policy_missing_group_proof_cannot_borrow_the_consumer_library(self):
        config, base = self.fixture()
        del config['ordinary_fast_group_contract_version']
        del config['ordinary_fast_group_contracts']
        plan = selector.select(config, base, self.change(), profile='pr-fast')
        self.assertNotIn('ordinary_fast_targets', plan)
        self.assertIn('consensus', plan['groups'])

    def test_optional_source_and_disabled_harness_do_not_prove_execution(self):
        config, base = self.fixture(disabled=True)
        plan = selector.select(config, base, self.change(), profile='pr-fast')
        self.assertNotIn('ordinary_fast_targets', plan)
        self.assertIn('consensus', plan['groups'])
        config, base = self.fixture(harness=False)
        # A non-libtest root cannot register this source as an executable test
        # input. Rejecting its ownership is safer than accepting a narrow plan.
        with self.assertRaises(selector.UnknownInputsError):
            selector.select(config, base, self.change(), profile='pr-fast')

    def test_unmapped_sibling_and_tested_merge_cannot_narrow(self):
        config, base = self.fixture()
        head = self.change()
        self.write('crates/oasis7_consensus/src/unmapped.rs', '#[test] fn future_contract() {}\n')
        tested = self.commit()
        for source, merge in ((tested, None), (head, tested)):
            plan = selector.select(config, base, source, test_head=merge, profile='pr-fast')
            self.assertNotIn('ordinary_fast_targets', plan)
            self.assertTrue({'consensus', 'oasis7_required'} <= set(plan['groups']))

    def test_filtered_consumer_cannot_borrow_disabled_independent_group_source(self):
        config, _ = self.fixture(disabled=True)
        config['ordinary_case_prefix_contract_version'] = 1
        config['ordinary_fast_targets']['core-lib'].update(
            cases=['behavior::existing'], case_prefixes=['behavior::'])
        manifest = Path('crates/core/Cargo.toml').read_text().replace(
            'test_tier_required=[]', 'test_tier_required=["optional"]\noptional=[]')
        self.write('crates/core/Cargo.toml', manifest)
        self.write('crates/core/src/lib.rs',
                   '#[path="../../oasis7_consensus/src/authority.rs"] mod borrowed;\n'
                   'mod behavior { #[test] fn existing() {} }\n')
        path = 'crates/oasis7_consensus/src/authority.rs'
        body = 'pub fn value() -> u8 { 1 }\n#[test] fn contract() { assert_eq!(value(), 1); }\n'
        self.write(path, body)
        base = self.commit()
        self.write(path, body.replace('-> u8 { 1 }', '-> u8 { 2 }'))
        plan = selector.select(config, base, self.commit(), profile='pr-fast')
        self.assertNotIn('ordinary_fast_targets', plan)
        self.assertIn('consensus', plan['groups'])

    def test_group_capability_requires_existing_exact_paths_and_real_group(self):
        config, _ = self.fixture()
        mutations = (
            lambda c: c.update(ordinary_fast_group_contract_version=True),
            lambda c: c['ordinary_fast_group_contracts'].update(unknown=['crates/core/src/lib.rs']),
            lambda c: c['ordinary_fast_group_contracts'].update(consensus=['crates/oasis7_consensus/src/unknown.rs']),
            lambda c: c['package_groups'].update(oasis7_consensus=['formal_process']),
        )
        for mutate in mutations:
            current = copy.deepcopy(config)
            mutate(current)
            with self.subTest(mutate=mutate), self.assertRaises(ValueError):
                selector.validate_config(current)

    def test_case_prefix_capability_and_full_inventory_preserve_module_contract(self):
        config = self.component_config()
        config['ordinary_case_prefix_contract_version'] = 1
        target = config['ordinary_fast_targets']['core-lib']
        target.update(cases=['behavior::existing'], case_prefixes=['behavior::'])
        selector.validate_config(config)
        for profile in ('periodic-full', 'candidate-full'):
            plan = selector.full_plan(config, profile, self.base)
            self.assertEqual(plan['ordinary_case_prefix_contract_version'], 1)
            self.assertEqual(plan['ordinary_full_fast_targets'], [target])
        for value in ([], ['behavior'], ['behavior::*'], ['behavior::', 'behavior::'], ['other::'], [None]):
            current = copy.deepcopy(config)
            current['ordinary_fast_targets']['core-lib']['case_prefixes'] = value
            with self.subTest(value=value), self.assertRaises(ValueError):
                selector.validate_config(current)
        current = copy.deepcopy(config)
        del current['ordinary_case_prefix_contract_version']
        with self.assertRaises(ValueError):
            selector.validate_config(current)

    def test_registered_authority_targets_keep_recovery_and_existing_independent_groups(self):
        config = json.loads((HERE / 'ci-required-scope.json').read_text())
        selector.validate_config(config)
        target = config['ordinary_fast_targets']['chain-authority']
        self.assertEqual(target['features'], ['node-libp2p', 'test_tier_required', 'wasmtime'])
        self.assertFalse(target['default_features'])
        self.assertEqual(len(target['case_prefixes']), 3)
        for fragment in ('recovery_commit_adapter_uses_original_authority_from_successor_anchor',
                         'actual_node_tcp_provider_cold_object_restart_continues',
                         'actual_node_pending_apply_recovers_after_publication_failure',
                         'same_node_restart_delivers_original_pending_completion'):
            self.assertTrue(any(fragment in case for case in target['cases']))
        rule = next(rule for rule in config['ordinary_fast_rules']
                    if rule['paths'] == ['crates/oasis7_consensus/src/lib.rs'])
        self.assertNotIn('static_module_additions', rule)
        self.assertIn('#[cfg(test)] mod authority_commit_verifier_tests;', rule['added_declarations'])


class PackageCapabilitiesAndContracts(unittest.TestCase):
    def test_dual_domain_full_plan_and_materialized_contracts(self):
        config = json.loads((HERE / 'ci-required-scope.json').read_text())
        selector.validate_config(config)
        caps = config['package_capabilities']
        for dual in ('oasis7_game_client', 'oasis7_browser_game_client'):
            self.assertTrue(caps[dual]['ordinary_package'])
            self.assertIsNotNone(caps[dual]['support_profile'])
        head = subprocess.check_output(['git', '-C', str(HERE.parent), 'rev-parse', 'HEAD'], text=True).strip()
        plan = selector.full_plan(config, 'periodic-full', head)
        ord_c = plan['ordinary_execution_contract']
        sup_c = plan['workspace_support_execution_contract']
        self.assertEqual(ord_c['version'], 1)
        self.assertFalse(ord_c['candidate_container_eligible'])
        self.assertIn('builtin_test_modules', ord_c['required_host_docker_wasm_fixtures'])
        self.assertEqual(ord_c['required_wasm_packages'], sorted(config['oasis7_required_wasm_packages']))
        self.assertEqual(sup_c['version'], 1)
        self.assertIn('oasis7_game_client', sup_c['host_profiles'])
        self.assertIn('oasis7_browser_game_client', sup_c['image_profiles'])
        self.assertEqual(sup_c['selected_profiles'], sorted(selector.SUPPORT_PROFILES))

    def test_cross_version_add_remove_rename_and_legacy_equivalence(self):
        config = json.loads((HERE / 'ci-required-scope.json').read_text())
        engine = selector._load_sibling_ci_engine()
        self.assertIsNotNone(engine)
        caps = config['package_capabilities']
        cand_members = (set(caps) - {'oasis7_proto'}) | {'oasis7_new_crate', 'oasis7_proto_v2'}
        diff = engine.reconcile_cross_version_members(caps, cand_members)
        self.assertEqual(diff['added'], ('oasis7_new_crate', 'oasis7_proto_v2'))
        self.assertEqual(diff['removed'], ('oasis7_proto',))
        # Candidate cannot relax host-docker requirement on policy_sha contract.
        risk_plan = {
            'scope': 'full', 'execution_profile': 'pr-fast', 'fast_plan_version': 1,
            'test_policy_changed': False,
            'ordinary_risk_targets': [{'package': 'oasis7_chain_runtime', 'target_kind': 'test'}],
            'oasis7_required_packages': ['oasis7_chain_runtime'], 'oasis7_required_wasm_packages': [],
            'workspace_support_profiles': ['oasis7_game_client', 'oasis7_browser_game_client'],
        }
        out = selector.materialize_execution_contracts(config, dict(risk_plan))
        self.assertFalse(out['ordinary_execution_contract']['candidate_container_eligible'])
        self.assertIn('provider_preview_wasm', out['ordinary_execution_contract']['required_host_docker_wasm_fixtures'])
        # Targeted inline plan keeps candidate_container_eligible=True and aligns with workflow-plan.
        inline_plan = {
            'scope': 'targeted', 'execution_profile': 'pr-fast', 'fast_plan_version': 1,
            'test_policy_changed': False, 'ordinary_execution': 'inline',
            'ordinary_partitions': ['oasis7_runtime', 'oasis7_world_state'],
            'ordinary_fast_targets': [{'package': 'oasis7_runtime'}, {'package': 'oasis7_world_state'}],
            'oasis7_required_packages': ['oasis7_runtime', 'oasis7_world_state'],
            'oasis7_required_wasm_packages': [], 'workspace_support_profiles': [],
        }
        out_inline = selector.materialize_execution_contracts(config, dict(inline_plan))
        self.assertTrue(out_inline['ordinary_execution_contract']['candidate_container_eligible'])
        self.assertEqual(out_inline['ordinary_execution_contract']['partitions'], ['oasis7_runtime', 'oasis7_world_state'])


if __name__ == '__main__':
    unittest.main()
