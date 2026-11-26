#!/usr/bin/env python3
import copy
import fnmatch
import hashlib
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


class ExpandedNativeContracts(_ScopeFixtureBase):
    ROOT = 'crates/core/src/bounded.rs'
    LEAF = 'crates/core/src/bounded/added.rs'

    def fixture(self):
        config = self.component_config()
        config['ordinary_fast_rules'] = [{
            'paths': [self.ROOT], 'targets': ['core-lib'], 'reason': 'static library wiring',
            'added_declarations': ['mod registered;'], 'static_module_additions': True}]
        self.write('crates/core/src/lib.rs', '#[cfg(not(target_arch="wasm32"))]\nmod bounded;\n')
        self.write(self.ROOT, 'pub fn unchanged() {}\n')
        return config, self.commit()

    def test_static_new_module_and_actual_library_source_use_inline_execution(self):
        config, base = self.fixture()
        self.write(self.ROOT, 'pub fn unchanged() {}\npub(super) mod added;\n')
        self.write(self.LEAF, '#[test] fn behavior() {}\n#[test] fn rejection() {}\n')
        head = self.commit()
        for profile in ('pr-fast', 'main-fast'):
            plan = selector.select(config, base, head, profile=profile)
            self.assertEqual(plan['ordinary_execution'], 'inline')
            self.assertEqual(set(plan['groups']), {'baseline', 'rust_baseline', 'oasis7_required'})
            self.assertFalse(plan['test_policy_changed'])
            self.assertEqual(plan['architecture']['build_policy'], 'ordinary-targets')
            legacy = json.loads(json.dumps(config))
            del legacy['ordinary_fast_rules'][0]['static_module_additions']
            self.assertNotIn('ordinary_execution', selector.select(legacy, base, head, profile=profile))

    def test_static_root_requires_opt_in_complete_lib_and_no_other_edits(self):
        config, base = self.fixture()
        self.write(self.ROOT, 'pub fn changed() {}\nmod added;\n')
        self.write(self.LEAF, '#[test] fn behavior() {}\n')
        self.assertNotIn('ordinary_execution', selector.select(config, base, self.commit(), profile='pr-fast'))
        for mutation in ('nonboolean', 'integration_only'):
            invalid = json.loads(json.dumps(config))
            if mutation == 'nonboolean':
                invalid['ordinary_fast_rules'][0]['static_module_additions'] = 1
            else:
                invalid['ordinary_fast_targets']['core-lib']['target_kind'] = 'test'
            with self.subTest(mutation=mutation), self.assertRaises(ValueError):
                selector.validate_config(invalid)

    def test_static_only_rule_does_not_recheck_unchanged_full_feature_bodies(self):
        config, base = self.fixture()
        del config['ordinary_fast_rules'][0]['added_declarations']
        body = '#[cfg(feature="test_tier_full")] pub fn existing_full() {}\n'
        self.write(self.ROOT, body)
        base = self.commit()
        self.write(self.ROOT, body + 'mod added;\n')
        self.write(self.LEAF, '#[test] fn behavior() {}\n')
        plan = selector.select(config, base, self.commit(), profile='pr-fast')
        self.assertEqual(plan['ordinary_execution'], 'inline')
        self.assertFalse(plan['test_policy_changed'])

    def test_static_module_helper_rejects_attributes_nested_code_and_existing_file(self):
        config, base = self.fixture()
        for statement in ('#[cfg(test)] mod added;', '#[path="bounded/added.rs"] mod added;',
                          'mod added {}', 'fn inner() { mod added; }',
                          'include!("bounded/added.rs");', 'pub use added::Value;'):
            self.write(self.ROOT, 'pub fn unchanged() {}\n' + statement + '\n')
            self.write(self.LEAF, '#[test] fn behavior() {}\n')
            head = self.commit()
            with self.subTest(statement=statement):
                self.assertFalse(selector.only_static_modules_added(self.ROOT, [(base, head)]))
            self.git('reset', '--hard', base)
        self.write(self.LEAF, '#[test] fn existing() {}\n')
        base = self.commit()
        self.write(self.ROOT, 'pub fn unchanged() {}\nmod added;\n')
        self.assertFalse(selector.only_static_modules_added(self.ROOT, [(base, self.commit())]))

    def test_static_module_requires_unique_regular_file_and_unchanged_prior_binding(self):
        config, base = self.fixture()
        self.write(self.ROOT, 'pub fn unchanged() {}\nmod added;\n')
        self.assertFalse(selector.only_static_modules_added(self.ROOT, [(base, self.commit())]))

        self.write(self.LEAF, '#[test] fn behavior() {}\n')
        self.write('crates/core/src/bounded/added/mod.rs', '#[test] fn ambiguous() {}\n')
        self.assertFalse(selector.only_static_modules_added(self.ROOT, [(base, self.commit())]))
        self.git('reset', '--hard', base)
        self.write(self.ROOT, '#[cfg(test)]\nmod existing;\n')
        self.write('crates/core/src/bounded/existing.rs', '#[test] fn existing() {}\n')
        base = self.commit()
        self.write(self.ROOT, '#[cfg(test)]\nmod added;\nmod existing;\n')
        self.write(self.LEAF, '#[test] fn behavior() {}\n')
        self.assertFalse(selector.only_static_modules_added(self.ROOT, [(base, self.commit())]))

    def test_inserted_comments_cannot_hide_or_change_existing_rust_tokens(self):
        config, base = self.fixture()
        self.write(self.ROOT, '/*\npub fn unchanged() {}\n*/\nmod added;\n')
        self.write(self.LEAF, '#[test] fn behavior() {}\n')
        head = self.commit()
        self.assertFalse(selector.only_static_modules_added(self.ROOT, [(base, head)]))
        self.assertNotIn('ordinary_execution', selector.select(config, base, head, profile='pr-fast'))

    def test_static_module_keeps_formal_risk_and_unavailable_feature_expansion(self):
        config, base = self.fixture()
        config['formal_risk_rules'].append({'match': [self.LEAF], 'suites': ['world-history'],
                                           'reason': 'explicit retained history'})
        self.write(self.ROOT, 'pub fn unchanged() {}\nmod added;\n')
        self.write(self.LEAF, '#[test] fn behavior() {}\n')
        plan = selector.select(config, base, self.commit(), profile='pr-fast')
        self.assertEqual(plan['ordinary_execution'], 'inline')
        self.assertEqual(plan['formal_suites'], ['world-history'])
        self.assertNotIn('build_policy_version', plan['architecture'])
        self.write(self.LEAF, '#![cfg(feature="test_tier_full")]\n#[test] fn behavior() {}\n')
        self.assertNotIn('ordinary_execution', selector.select(config, base, self.commit(), profile='pr-fast'))

    def test_static_module_proof_checks_every_source_and_tested_comparison(self):
        config, base = self.fixture()
        self.write(self.ROOT, 'pub fn unchanged() {}\nmod added;\n')
        self.write(self.LEAF, '#[test] fn behavior() {}\n')
        source = self.commit()
        self.write(self.ROOT, 'pub fn replaced() {}\nmod added;\n')
        tested = self.commit()
        self.assertFalse(selector.only_static_modules_added(self.ROOT, [(base, source), (source, tested)]))

    def test_viewer_inventory_is_bounded_and_full_retains_exact_feature_variant(self):
        config = json.loads((HERE / 'ci-required-scope.json').read_text())
        target = config['ordinary_fast_targets']['viewer-runtime-lib']
        self.assertEqual((target['package'], target['target_kind'], target['target_name']),
                         ('oasis7_viewer_runtime', 'lib', 'oasis7_viewer_runtime'))
        self.assertFalse(target['default_features'])
        self.assertEqual(target['features'], ['test_tier_required'])
        viewer = [rule for rule in config['ordinary_fast_component_rules']
                  if 'viewer-runtime-lib' in rule['targets']]
        self.assertEqual(len(viewer), 1)
        for path in ('crates/oasis7_viewer_runtime/src/viewer/auth.rs',
                     'crates/oasis7_viewer_runtime/src/viewer/runtime_live/authoritative.rs',
                     'crates/oasis7_viewer_runtime/src/viewer/runtime_live.rs'):
            self.assertFalse(any(fnmatch.fnmatchcase(path, pattern) for pattern in viewer[0]['match']))
        for profile in ('periodic-full', 'candidate-full'):
            plan = selector.full_plan(config, profile, self.base)
            self.assertIn(target, plan['ordinary_full_fast_targets'])
            self.assertIn('oasis7_viewer_runtime', plan['oasis7_required_packages'])


class PrivateUnitTestContracts(_ScopeFixtureBase):
    ROOT = 'crates/core/src/lib.rs'
    PARENT = 'crates/core/src/config.rs'
    LEAF = 'crates/core/src/faults.rs'
    BODY = '#[test] fn existing() { assert_eq!(1, 1); }\n'

    def fixture(self):
        config = self.component_config()
        config.update(ordinary_unit_test_contract_version=1, ordinary_unit_test_targets=['core-lib'],
                      ordinary_partition_version=1)
        config['groups'].append('node')
        config['resources']['node'] = []
        config['package_groups']['consumer'] = ['oasis7_required', 'node']
        config['ordinary_fast_targets']['consumer-lib'] = dict(
            config['ordinary_fast_targets']['core-lib'], package='consumer', target_name='consumer')
        config['ordinary_fast_component_rules'][0]['targets'].append('consumer-lib')
        # Match the real WorldState two-layer mount, not a filename convention.
        self.write(self.ROOT, '#[cfg(not(target_arch="wasm32"))] mod config;\n')
        self.write(self.PARENT, '#[cfg(test)] #[path="faults.rs"] mod fault_cases;\n')
        self.write(self.LEAF, self.BODY)
        return config, self.commit()

    def change(self):
        self.write(self.LEAF, self.BODY.replace('1, 1', '2, 2'))
        return self.commit()

    def selected(self, config, base, head=None, **kwargs):
        return selector.select(config, base, head or self.change(), profile='pr-fast', **kwargs)

    def assert_isolated(self, plan):
        self.assertEqual(plan['ordinary_unit_test_contract_version'], 1)
        self.assertEqual(plan['ordinary_unit_test_sources'], {self.LEAF: 'core'})
        self.assertEqual(plan['groups'], ['baseline', 'oasis7_required', 'rust_baseline'])
        self.assertEqual(plan['oasis7_required_packages'], ['core'])
        self.assertEqual(plan['affected_packages'], ['consumer', 'core'])
        self.assertEqual([target['package'] for target in plan['ordinary_fast_targets']], ['core'])
        self.assertEqual(plan['ordinary_unit_test_excluded_groups'], ['node'])
        self.assertEqual(plan['ordinary_unit_test_excluded_targets'], [
            {'package': 'consumer', 'target_kind': 'lib', 'target_name': 'consumer'}])
        self.assertEqual(plan['ordinary_partitions'], ['all'])
        self.assertEqual(plan['ordinary_clippy_group'], 'rust_baseline')
        self.assertEqual(plan['architecture']['build_policy'], 'ordinary-targets')
        self.assertEqual(plan['fast_plan_exceptions'], [])
        self.assertFalse(plan['test_policy_changed'])

    def assert_original(self, plan):
        self.assertNotIn('ordinary_unit_test_contract_version', plan)
        self.assertIn('node', plan['groups'])

    def test_complete_owner_and_original_affected_evidence_for_pr_and_push(self):
        config, base = self.fixture()
        head = self.change()
        for profile in ('pr-fast', 'main-fast'):
            plan = selector.select(config, base, head, profile=profile)
            self.assert_isolated(plan)
            self.assertEqual(plan['ordinary_unit_test_proof_refs'], sorted([base, head]))
            self.assertEqual(plan['ordinary_fast_targets'][0], config['ordinary_fast_targets']['core-lib'])

    def test_no_test_filename_is_required_and_same_owner_multiple_files_are_complete(self):
        config, _ = self.fixture()
        helper = 'crates/core/src/material.rs'
        self.write(self.PARENT, '#[cfg(test)] #[path="faults.rs"] mod fault_cases;\n'
                   '#[cfg(test)] #[path="material.rs"] mod material;\n')
        self.write(helper, 'pub fn material() -> u8 { 1 }\n')
        base = self.commit()
        self.write(helper, 'pub fn material() -> u8 { 2 }\n')
        plan = self.selected(config, base)
        self.assertEqual(plan['ordinary_unit_test_sources'], {self.LEAF: 'core', helper: 'core'})
        self.assertEqual(plan['oasis7_required_packages'], ['core'])

    def test_legacy_and_candidate_configuration_cannot_self_authorize(self):
        config, base = self.fixture()
        old = copy.deepcopy(config)
        for field in ('ordinary_unit_test_contract_version', 'ordinary_unit_test_targets'):
            del old[field]
        head = self.change()
        self.assert_original(self.selected(old, base, head))
        self.write('scripts/ci-required-scope.json', json.dumps(config))
        self.assert_original(self.selected(old, base, self.commit()))
        for profile in ('periodic-full', 'candidate-full'):
            full = selector.full_plan(config, profile, head)
            self.assertNotIn('ordinary_unit_test_contract_version', full)
            self.assertEqual(full['groups'], sorted(config['groups']))
            self.assertEqual(full['ordinary_full_fast_targets'],
                             [config['ordinary_fast_targets'][key] for key in sorted(config['ordinary_fast_targets'])])

    def test_capability_requires_complete_registered_library(self):
        config, _ = self.fixture()
        for changes in ({'ordinary_unit_test_contract_version': True},
                        {'ordinary_unit_test_contract_version': 2},
                        {'ordinary_unit_test_targets': []}, {'ordinary_unit_test_targets': ['unknown']},
                        {'ordinary_unit_test_targets': ['core-lib', 'core-lib']},
                        {'ordinary_unit_test_targets': 'core-lib'}):
            with self.subTest(changes=changes), self.assertRaises(ValueError):
                selector.validate_config(config | changes)
        for missing in ('ordinary_unit_test_targets', 'ordinary_unit_test_contract_version'):
            invalid = copy.deepcopy(config)
            del invalid[missing]
            with self.subTest(missing=missing), self.assertRaises(ValueError):
                selector.validate_config(invalid)
        config['ordinary_fast_targets']['core-lib']['target_kind'] = 'test'
        with self.assertRaises(ValueError):
            selector.validate_config(config)

    def test_native_gate_or_a_second_production_mount_does_not_isolate(self):
        config, original = self.fixture()
        for declaration in (
            '#[cfg(unix)] #[path="faults.rs"] mod fault_cases;\n',
            '#[cfg(not(target_arch="wasm32"))] #[path="faults.rs"] mod fault_cases;\n',
            '#[cfg(test)] #[path="faults.rs"] mod fault_cases;\n#[path="faults.rs"] mod production;\n',
        ):
            self.git('reset', '--hard', original)
            self.write(self.PARENT, declaration)
            with self.subTest(declaration=declaration):
                self.assert_original(self.selected(config, self.commit()))

    def test_all_cargo_roots_including_disabled_and_custom_targets_veto(self):
        config, original = self.fixture()
        manifest = Path('crates/core/Cargo.toml').read_text()
        for kind in ('bin', 'test', 'example', 'bench'):
            for extra in ('', 'harness=false\n', 'required-features=["test_tier_full"]\n'):
                self.git('reset', '--hard', original)
                self.write('crates/core/Cargo.toml', manifest
                           + f'\n[[{kind}]]\nname="other"\npath="src/faults.rs"\n' + extra)
                with self.subTest(kind=kind, extra=extra):
                    self.assert_original(self.selected(config, self.commit()))
        for folder in ('tests', 'examples', 'benches', 'src/bin'):
            self.git('reset', '--hard', original)
            self.write(f'crates/core/{folder}/consumer.rs',
                       '#[path="' + ('../' if folder == 'src/bin' else '../src/')
                       + 'faults.rs"] mod shared;\n')
            with self.subTest(folder=folder):
                self.assert_original(self.selected(config, self.commit()))

    def test_other_package_root_or_mount_vetoes_even_when_test_only(self):
        config, original = self.fixture()
        for body in ('#[cfg(test)] #[path="../../core/src/faults.rs"] mod shared;\n',
                     '#[cfg(test)] mod nested { include!("../../core/src/faults.rs"); }\n'):
            self.git('reset', '--hard', original)
            self.write('crates/consumer/src/lib.rs', body)
            with self.subTest(body=body):
                self.assert_original(self.selected(config, self.commit()))
        self.git('reset', '--hard', original)
        self.write('crates/consumer/Cargo.toml', Path('crates/consumer/Cargo.toml').read_text()
                   + '\n[lib]\npath="../core/src/lib.rs"\n')
        self.assert_original(self.selected(config, self.commit()))

    def test_other_cargo_roots_resolve_implicit_children_from_their_actual_directory(self):
        config, original = self.fixture()
        manifest = Path('crates/core/Cargo.toml').read_text()
        for kind, extra in (('example', ''), ('bench', ''), ('test', 'harness=false\n'),
                            ('test', 'required-features=["test_tier_full"]\n')):
            self.git('reset', '--hard', original)
            self.write('crates/core/Cargo.toml', manifest
                       + f'\n[[{kind}]]\nname="other"\npath="src/another.rs"\n' + extra)
            self.write('crates/core/src/another.rs', 'mod faults; fn main() {}\n')
            with self.subTest(kind=kind, extra=extra):
                self.assert_original(self.selected(config, self.commit()))

    def test_include_aliases_reexports_and_short_source_data_paths_veto(self):
        config, original = self.fixture()
        for body in (
            'use std::include as mount; mount!("faults.rs");',
            'pub use std::{include as mount}; mount!("faults.rs");',
            'use std::include_str as bytes; pub const TEXT: &str = bytes!("faults.rs");',
            'use std::{include_bytes as bytes}; pub const TEXT: &[u8] = bytes!("faults.rs");',
            'use std::r#include as mount; mount!(concat!("fault", "s.rs"));',
            'pub use std::{r#include as mount}; mount!(concat!("fault", "s.rs"));',
            'use std::r#include_str as bytes; pub const TEXT: &str = bytes!(concat!("fault", "s.rs"));',
            'fn data() { std::fs::read("faults.rs"); }',
            'fn data() { std::fs::read(r#"faults.rs"#); }',
        ):
            self.git('reset', '--hard', original)
            # The same filename is also a real #[path] mount in this file.
            # Exempting its token must not exempt the independent byte reader.
            self.write(self.PARENT, Path(self.PARENT).read_text() + '\n' + body + '\n')
            with self.subTest(body=body):
                self.assert_original(self.selected(config, self.commit()))

    def test_literal_source_mounts_do_not_masquerade_as_data_readers(self):
        config, _ = self.fixture()
        self.write(self.ROOT, '#[cfg(not(target_arch="wasm32"))] include!("native.rs");\n')
        self.write('crates/core/src/native.rs', '#[path="config/mod.rs"] mod config;\n')
        self.git('rm', self.PARENT)
        self.write('crates/core/src/config/mod.rs', '#[cfg(test)] #[path="../faults.rs"] mod faults;\n')
        self.assert_isolated(self.selected(config, self.commit()))

    def test_only_changed_file_bytes_not_unchanged_ancestor_bytes_are_inputs(self):
        config, _ = self.fixture()
        self.write(self.PARENT, Path(self.PARENT).read_text()
                   + 'const ROOT_TEXT: &str = include_str!("lib.rs");\n')
        self.assert_isolated(self.selected(config, self.commit()))
        self.write(self.PARENT, Path(self.PARENT).read_text()
                   + 'const CHANGED_TEXT: &str = include_str!("faults.rs");\n')
        base = self.commit()
        self.write(self.LEAF, self.BODY + '// another change\n')
        self.assert_original(self.selected(config, base, self.commit()))

    def test_tracked_file_and_directory_symlinks_cannot_hide_another_consumer(self):
        config, original = self.fixture()
        for alias, destination, mount in (('alias.rs', 'faults.rs', 'alias.rs'),
                                          ('alias_dir', '.', 'alias_dir/faults.rs')):
            self.git('reset', '--hard', original)
            self.write(self.ROOT, Path(self.ROOT).read_text() + f'#[path="{mount}"] pub mod production;\n')
            link = 'crates/core/src/' + alias
            self.write(link, destination)
            self.git('add', self.ROOT, link)
            blob = self.git('rev-parse', ':' + link)
            # Model Git's exact symlink mode without requiring OS symlink
            # privileges; neither commit re-stages the placeholder as regular.
            self.git('update-index', '--cacheinfo', '120000', blob, link)
            self.git('commit', '-qm', 'existing source alias')
            base = self.git('rev-parse', 'HEAD')
            self.write(self.LEAF, self.BODY.replace('1, 1', '2, 2'))
            self.git('add', self.LEAF)
            self.git('commit', '-qm', 'change only private-looking leaf')
            head = self.git('rev-parse', 'HEAD')
            self.assertEqual(self.git('diff', '--name-only', base, head), self.LEAF)
            with self.subTest(alias=alias):
                self.assert_original(self.selected(config, base, head))

    def test_non_normalized_identifiers_cannot_hide_an_inline_module_route(self):
        config, _ = self.fixture()
        self.LEAF = 'crates/core/src/café/faults.rs'
        self.write(self.LEAF, self.BODY)
        self.write(self.PARENT, '#[cfg(test)] #[path="café/faults.rs"] mod fault_cases;\n')
        self.write(self.ROOT, Path(self.ROOT).read_text() + 'pub mod cafe\u0301 { pub mod faults; }\n')
        self.assert_original(self.selected(config, self.commit()))

    def test_extra_inventory_is_scoped_and_missing_scan_evidence_cannot_authorize(self):
        config, base = self.fixture()
        head = self.change()
        with patch.object(selector, 'rust_source_inputs', wraps=selector.rust_source_inputs) as parse:
            self.assert_isolated(self.selected(config, base, head))
            self.assertTrue(any(call.kwargs.get('with_module_safety') is True for call in parse.call_args_list))
        self.write('doc/readme.md', 'documentation does not need module isolation scans\n')
        docs = self.commit()
        with patch.object(selector, 'rust_source_inputs', wraps=selector.rust_source_inputs) as parse:
            selector.select(config, head, docs, profile='pr-fast')
            self.assertTrue(parse.call_args_list)
            self.assertFalse(any(call.kwargs.get('with_module_safety') for call in parse.call_args_list))
        graph = selector.cargo_graph
        def unscanned(*args, **kwargs):
            result = graph(*args, **kwargs)
            result[2]['__private_test_input_inventory__'] = False
            return result
        with patch.object(selector, 'cargo_graph', side_effect=unscanned):
            self.assert_original(self.selected(config, base, head))

    def test_data_and_unknown_readers_in_any_package_veto(self):
        config, original = self.fixture()
        readers = [
            ('crates/consumer/src/lib.rs', 'const D: &str = include_str!("../../core/src/faults.rs");'),
            ('crates/consumer/src/lib.rs', '#[cfg(test)] const D: &[u8] = include_bytes!("../../core/src/faults.rs");'),
            ('crates/consumer/src/lib.rs', 'fn data() { std::fs::read("crates/core/src/faults.rs"); }'),
            ('crates/consumer/src/lib.rs', 'const D: &str = include_str!(env!("UNKNOWN"));'),
            (self.ROOT, '#[cfg(test)] const D: &str = include_str!("faults.rs");'),
        ]
        for path, body in readers:
            self.git('reset', '--hard', original)
            self.write(path, Path(path).read_text() + '\n' + body + '\n')
            with self.subTest(path=path, body=body):
                self.assert_original(self.selected(config, self.commit()))

    def test_macro_generated_mounts_do_not_hide_behind_a_static_test_mount(self):
        config, original = self.fixture()
        for body in (
            'macro_rules! mount { ($name:ident) => { mod $name; }; } mount!(faults);',
            'macro_rules! mount { () => { mod faults; }; } mount!();',
            'forward!(mod faults;);',
            'paste! { mod [<fault s>]; }',
            'macro_rules! mount { () => { #[path="faults.rs"] mod extra; }; }',
            'macro_rules! mount { ($mac:ident, $name:ident) => { $mac!(concat!(stringify!($name), ".rs")); }; } mount!(include, faults);',
            'macro_rules! mount { ($mac:ident) => { $mac!("faults.rs"); }; }',
        ):
            self.git('reset', '--hard', original)
            self.write(self.ROOT, Path(self.ROOT).read_text() + '\n' + body + '\n')
            with self.subTest(body=body):
                self.assert_original(self.selected(config, self.commit()))
        self.git('reset', '--hard', original)
        self.write(self.ROOT, Path(self.ROOT).read_text()
                   + '\n// mod $name;\nconst EXAMPLE: &str = "macro_rules! mount { () => { mod faults; }; }";\n')
        self.assert_isolated(self.selected(config, self.commit()))

    def test_unregistered_host_sources_and_local_proc_macros_veto(self):
        config, original = self.fixture()
        for path, body in (
            ('crates/other/build.rs', 'fn main() {}\n'),
            ('crates/consumer/Cargo.toml', Path('crates/consumer/Cargo.toml').read_text() + '\n[lib]\nproc-macro=true\n'),
            ('crates/other/Cargo.toml', Path('crates/other/Cargo.toml').read_text()
             + '\n[build-dependencies]\ncore={path="../core"}\n'),
        ):
            self.git('reset', '--hard', original)
            self.write(path, body)
            with self.subTest(path=path):
                self.assert_original(self.selected(config, self.commit()))

    def test_trusted_self_contained_generator_requires_actual_unchanged_digest(self):
        config, _ = self.fixture()
        producer = 'crates/other/build.rs'
        body = 'fn main() {}\n'
        self.write(producer, body)
        config['generated_inputs'] = [{'package': 'other', 'producer': producer,
                                      'producer_sha256': hashlib.sha256(body.encode()).hexdigest(),
                                      'outputs': ['metadata.txt']}]
        self.assert_isolated(self.selected(config, self.commit()))
        self.write(producer, 'fn main() { println!("changed generator"); }\n')
        base = self.commit()
        self.write(self.LEAF, self.BODY + '// new delta\n')
        self.assert_original(self.selected(config, base, self.commit()))

    def test_mixed_input_explicit_group_risk_or_test_retirement_keeps_original(self):
        config, original = self.fixture()
        for path, body in ((self.PARENT, Path(self.PARENT).read_text() + 'pub fn production() {}\n'),
                           ('doc/readme.md', 'mixed documentation\n'),
                           (self.LEAF, self.BODY.replace('existing', 'renamed')),
                           (self.LEAF, '#[cfg(feature="test_tier_full")]\n' + self.BODY)):
            self.git('reset', '--hard', original)
            self.write(self.LEAF, self.BODY.replace('1, 1', '2, 2'))
            self.write(path, body)
            with self.subTest(path=path, body=body):
                self.assert_original(self.selected(config, original, self.commit()))
        self.git('reset', '--hard', original)
        head = self.change()
        self.assert_original(self.selected(config, original, head, additions=('node',)))
        risk = copy.deepcopy(config)
        risk['formal_risk_rules'].append({'match': [self.LEAF], 'suites': ['world-history'], 'reason': 'explicit risk'})
        self.assert_original(self.selected(risk, original, head))
        direct = copy.deepcopy(config)
        direct['rules'].append({'match': [self.LEAF], 'groups': ['node'], 'reason': 'independent obligation'})
        self.assert_original(self.selected(direct, original, head))

    def test_merge_target_reader_and_before_tree_are_both_checked(self):
        config, base = self.fixture()
        source = self.change()
        self.git('checkout', '-q', base)
        self.write('crates/consumer/src/lib.rs', '#[cfg(test)] const D: &str = include_str!("../../core/src/faults.rs");\n')
        self.commit()
        self.git('merge', '--no-ff', '--no-commit', source)
        tested = self.commit()
        self.assert_original(self.selected(config, base, source, test_head=tested))
        # A merge resolution removing a pre-existing reader does not erase its
        # before-tree obligation, even when source and tested appear isolated.
        self.write('crates/consumer/src/lib.rs', '// no reader after merge\n')
        self.git('add', '.')
        self.git('commit', '--amend', '--no-edit')
        tested = self.git('rev-parse', 'HEAD')
        self.assert_original(self.selected(config, base, source, test_head=tested))

    def test_unchanged_disabled_feature_and_new_or_deleted_files_do_not_isolate(self):
        config, original = self.fixture()
        self.write(self.LEAF, '#[cfg(feature="test_tier_full")]\n' + self.BODY)
        base = self.commit()
        self.write(self.LEAF, '#[cfg(feature="test_tier_full")]\n' + self.BODY.replace('1, 1', '2, 2'))
        self.assert_original(self.selected(config, base, self.commit()))
        self.git('reset', '--hard', original)
        self.git('rm', self.LEAF)
        base = self.commit()
        self.assert_original(self.selected(config, base))
        self.git('reset', '--hard', original)
        self.git('rm', self.LEAF)
        self.assert_original(self.selected(config, original, self.commit()))


if __name__ == '__main__':
    unittest.main()
