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


class FilteredTargetOrdinaryProof(unittest.TestCase):
    ROOT = 'crates/core/src/lib.rs'
    BEFORE = 'mod selected { #[test] fn existing() {} } pub fn product() -> u8 { 1 }'
    FORMAL_WRITER_PATH = 'crates/oasis7_chain_runtime/src/host/execution_bridge/formal_writer.rs'

    def policy(self, after, *, before=None, complete=False, unrelated=False, narrow=True):
        before = self.BEFORE if before is None else before
        target = dict(package='core', target_kind='lib', target_name='core',
                      default_features=True, features=['test_tier_required'],
                      cases=['selected::existing'], case_prefixes=['selected::'])
        targets = [target]
        if complete:
            targets.append({key: value for key, value in target.items() if key != 'case_prefixes'})
        if unrelated:
            targets.append(dict(package='other', target_kind='lib', target_name='other',
                                default_features=True, features=[], cases=[]))
        manifest = {'features': {'test_tier_required': [], 'test_tier_full': []}}
        sources = {self.ROOT: after.encode(), 'crates/other/src/lib.rs': b'pub fn unrelated() {}'}
        unavailable = set()
        def read(*args):
            self.assertEqual(args[0], 'show')
            ref, path = args[1].split(':', 1)
            self.assertEqual(path, self.ROOT)
            return {'base': before, 'source': after}[ref].encode()
        with patch.object(selector, 'git', side_effect=read):
            result = selector.changed_test_policy(
                'base', 'source', [self.ROOT], {self.ROOT: {'core'}}, {'core'},
                {'core': set(manifest['features'])}, ordinary_roots={self.ROOT},
                source_inputs={'sources': sources,
                               'directories': {'core': 'crates/core', 'other': 'crates/other'},
                               'manifests': {'core': manifest, 'other': {'features': {}}},
                               'default_features': {'core': set(), 'other': set()}},
                narrow_features_by_path={self.ROOT: {'test_tier_required'}} if narrow else None,
                narrow_unavailable=unavailable, selected_targets=targets,
                required_source_paths={self.ROOT})
        return result, unavailable

    def test_filtered_target_does_not_prove_new_test_or_test_module_execution(self):
        for addition in (
            '#[test] fn outside_filter() {}',
            '#[cfg(test)] mod outside { #[test] fn added() {} }',
        ):
            for narrow in (False, True):
                for unrelated in (False, True):
                    with self.subTest(addition=addition, narrow=narrow, unrelated=unrelated):
                        result, _ = self.policy(self.BEFORE + addition, narrow=narrow, unrelated=unrelated)
                        self.assertEqual(result, ([self.ROOT], False, []))

    def test_complete_same_source_target_still_proves_new_ordinary_test(self):
        result, _ = self.policy(self.BEFORE + '#[test] fn outside_filter() {}', complete=True)
        self.assertEqual(result, ([], False, []))

    def test_filter_preserves_production_reachability_without_test_changes(self):
        result, unavailable = self.policy(self.BEFORE.replace('{ 1 }', '{ 2 }'))
        self.assertEqual(result, ([], False, []))
        self.assertEqual(unavailable, set())

    def test_existing_test_and_test_helper_body_edits_invalidate_filter(self):
        fixtures = (
            self.BEFORE.replace('fn existing() {}', 'fn existing() { assert_eq!(1, 1); }'),
            self.BEFORE.replace('mod selected {', 'mod selected { fn fixture() -> u8 { 1 }'),
            self.BEFORE + '#[cfg(test)] fn fixture() -> u8 { 1 }',
            self.BEFORE + 'impl<T> Writer<T> { #[cfg(test)] fn fixture() -> u8 { 1 } }',
        )
        for before in fixtures:
            # Leave the production body untouched; change the test-only token.
            after = (before.replace('assert_eq!(1, 1)', 'assert_eq!(1, 2)')
                     if 'assert_eq!' in before else before.replace('fixture() -> u8 { 1 }', 'fixture() -> u8 { 2 }'))
            for complete in (False, True):
                with self.subTest(before=before, complete=complete):
                    result, unavailable = self.policy(after, before=before, complete=complete)
                    self.assertEqual(result, ([], False, []))
                    self.assertEqual(unavailable, set() if complete else {self.ROOT})

    def test_test_helper_moves_and_opaque_test_regions_invalidate_filter(self):
        before = self.BEFORE + 'impl<T> First<T> { #[cfg(test)] fn fixture() {} }'
        _, unavailable = self.policy(before.replace('First<T>', 'Other<T>'), before=before)
        self.assertEqual(unavailable, {self.ROOT})
        for region in (
            '#[cfg(test)] generated_tests!();',
            'impl<T> Writer<T> { fn outer() { #[cfg(test)] fn nested() {} } }',
        ):
            before = self.BEFORE + region
            with self.subTest(region=region):
                _, unavailable = self.policy(before.replace('{ 1 }', '{ 2 }'), before=before)
                self.assertEqual(unavailable, {self.ROOT})

    def test_real_unchanged_test_helpers_and_hooks_allow_business_edit(self):
        before = (HERE.parent / self.FORMAL_WRITER_PATH).read_text()
        after = before.replace('let head = saved_candidate::verify_qualified_commit(',
                               'let unrelated = 0; let head = saved_candidate::verify_qualified_commit(')
        self.assertNotEqual(before, after)
        result, unavailable = self.policy(after, before=before)
        self.assertEqual(result, ([], False, []))
        self.assertEqual(unavailable, set())

    def test_item_macros_and_proc_attributes_cannot_hide_changed_or_new_cases(self):
        pairs = (
            ('other::test_case!(outside_filter, 1);', 'other::test_case!(outside_filter, 2);'),
            ('', 'other::test_case!(outside_filter, 2);'),
            ('#[other::case] fn generated() { assert_eq!(1, 1); }',
             '#[other::case] fn generated() { assert_eq!(1, 2); }'),
            ('#[derive(other::Cases)] struct Input { bytes: [u8; 1] }',
             '#[derive(other::Cases)] struct Input { bytes: [u8; 2] }'),
            ('#[other::cases] impl Input { fn value() -> u8 { 1 } }',
             '#[other::cases] impl Input { fn value() -> u8 { 2 } }'),
        )
        for before, after in pairs:
            with self.subTest(before=before):
                _, unavailable = self.policy(self.BEFORE + after, before=self.BEFORE + before)
                self.assertEqual(unavailable, {self.ROOT})

    def test_qualified_attributes_do_not_borrow_builtin_transparency(self):
        for name in ('allow', 'warn', 'deny', 'forbid', 'expect', 'doc', 'should_panic', 'inline', 'cold'):
            for item in ('fn generated() { assert_eq!(1, 1); }',
                         'mod generated { fn case() { assert_eq!(1, 1); } }',
                         'impl Input { fn generated() { assert_eq!(1, 1); } }'):
                before = self.BEFORE + '#[' + name + '::case] ' + item
                with self.subTest(attribute=name, item=item):
                    _, unavailable = self.policy(before.replace('assert_eq!(1, 1)', 'assert_eq!(1, 2)'),
                                                 before=before)
                    self.assertEqual(unavailable, {self.ROOT})

    def test_function_local_macro_definitions_cannot_hide_exported_test_changes(self):
        definition = ('fn install() { #[macro_export] macro_rules! exported_case { ($attr:meta) => { '
                      '#[$attr] fn outside() { assert_eq!(1, 1); } }; } }')
        for before, after in ((definition, definition.replace('assert_eq!(1, 1)', 'assert_eq!(1, 2)')),
                              ('', definition)):
            with self.subTest(addition=not before):
                _, unavailable = self.policy(self.BEFORE + after, before=self.BEFORE + before)
                self.assertEqual(unavailable, {self.ROOT})

    def test_unchanged_opaque_items_do_not_capture_plain_business_bodies(self):
        before = (self.BEFORE + '#[derive(Debug)] struct Input { value: u8 }').replace(
            'pub fn product() -> u8 { 1 }', 'pub fn product() -> u8 { let _message = format!("same"); '
            'let _values = vec![1, 2]; let _value = 1; let _source = "macro_rules! example {}"; 1 }')
        after = before.replace('let _value = 1', 'let _value = 2 + 3').replace('example {}', 'different {}')
        after += 'fn additional_business_helper() { use std::mem::size_of; let _size = size_of::<u8>(); }'
        result, unavailable = self.policy(after, before=before)
        self.assertEqual(result, ([], False, []))
        self.assertEqual(unavailable, set())

    def test_macro_inputs_paths_headers_and_occurrence_counts_cannot_change_under_filter(self):
        body = 'impl<T> First<T> { fn install() { ::other::define!(1); let value = 1; } }'
        for changed in (
            body.replace('define!(1)', 'define!(2)'),
            body.replace('::other::define!', '::different::define!'),
            body.replace('::other::define!', 'other::define!'),
            body.replace('::other::define!(1);', '::other::define!(1); ::other::define!(1);'),
            body.replace('::other::define!(1);', ''),
            body.replace('fn install()', 'fn moved()'),
            body.replace('First<T>', 'Second<T>'),
            body.replace('impl<T>', 'impl<T: Clone>'),
        ):
            with self.subTest(changed=changed):
                _, unavailable = self.policy(self.BEFORE + changed, before=self.BEFORE + body)
                self.assertEqual(unavailable, {self.ROOT})
        for before, after in (('fn product_macro() { format!("before"); }', 'fn product_macro() { format!("after"); }'),
                              ('fn product_macro() { vec![1]; }', 'fn product_macro() { vec![2]; }'),
                              ('fn product_macro() {}', 'fn product_macro() { format!("new"); }')):
            with self.subTest(macro=before):
                _, unavailable = self.policy(self.BEFORE + after, before=self.BEFORE + before)
                self.assertEqual(unavailable, {self.ROOT})
        _, unavailable = self.policy(self.BEFORE + body.replace('let value = 1', 'let value = 2 + 3'),
                                     before=self.BEFORE + body)
        self.assertEqual(unavailable, set())

    def test_macro_binding_items_keep_their_complete_context(self):
        for declaration, replacement in (
            ('use first::define as generate;', 'use second::define as generate;'),
            ('extern crate first as macros;', 'extern crate second as macros;'),
            ('const VALUE: u8 = 1;', 'const VALUE: u8 = 2;'),
            ('static VALUE: u8 = 1;', 'static VALUE: u8 = 2;'),
            ('fn nested() -> u8 { 1 }', 'fn nested() -> u8 { 2 }'),
        ):
            for local in (False, True):
                before = (self.BEFORE + ('fn install() { ' + declaration + ' generate!(1); }' if local
                                        else declaration + ' fn install() { generate!(1); }'))
                with self.subTest(declaration=declaration, local=local):
                    _, unavailable = self.policy(before.replace(declaration, replacement), before=before)
                    # Ordinary uncalled sibling functions may still change;
                    # declarations binding macro inputs may not.
                    expected = set() if not local and declaration.startswith('fn ') else {self.ROOT}
                    self.assertEqual(unavailable, expected)


class NativeRootModuleAdditions(_ScopeFixtureBase):
    ROOT = 'crates/core/src/lib.rs'
    LEAF = 'crates/core/src/added.rs'
    ORIGINAL = 'pub fn portable() -> u8 { 1 }\n#[test] fn existing() {}\n'
    DECLARATION = '#[cfg(not(target_arch = "wasm32"))]\npub mod added;\n'
    BODY = 'pub fn value() -> u8 { 2 }\n#[test] fn behavior() { assert_eq!(value(), 2); }\n'

    def fixture(self):
        config = self.component_config()
        # A trusted complete-library component contract, not a filename waiver
        # or an expansion of the attribute-free static_module_additions rule.
        config['ordinary_fast_rules'][0]['paths'] = ['crates/core/src/registered.rs']
        self.write(self.ROOT, self.ORIGINAL)
        return config, self.commit()

    def add_module(self, declaration=None, body=None):
        self.write(self.ROOT, self.ORIGINAL + '\n' + (declaration or self.DECLARATION))
        self.write(self.LEAF, body or self.BODY)
        return self.commit()

    def assert_portable(self, plan):
        self.assertIn('core', plan['oasis7_required_wasm_packages'])
        self.assertIn('launcher_web', plan['groups'])
        self.assertNotIn('build_policy', plan['architecture'])

    def test_pr5405_native_root_addition_defers_only_unrelated_consumers(self):
        config, base = self.fixture()
        head = self.add_module()
        for profile in ('pr-fast', 'main-fast'):
            with self.subTest(profile=profile):
                plan = selector.select(config, base, head, profile=profile)
                self.assertEqual(plan['scope'], 'targeted')
                self.assertEqual(set(plan['groups']), {'baseline', 'rust_baseline', 'oasis7_required'})
                self.assertEqual(plan['ordinary_execution'], 'inline')
                self.assertEqual(plan['ordinary_clippy_group'], 'rust_baseline')
                self.assertEqual(plan['ordinary_fast_targets'], [config['ordinary_fast_targets']['core-lib']])
                self.assertEqual(plan['affected_packages'], ['consumer', 'core'])
                self.assertEqual(plan['oasis7_required_wasm_packages'], [])
                self.assertEqual(plan['architecture']['packages'], ['core'])
                self.assertEqual(plan['architecture']['build_policy'], 'ordinary-targets')
                self.assertFalse(plan['test_policy_changed'])
                self.assertEqual(plan['fast_plan_exceptions'], [])

    def test_native_root_proof_uses_actual_selected_cargo_entry_and_regular_new_module(self):
        config, base = self.fixture()
        root = 'crates/core/src/entry.rs'
        self.write('crates/core/Cargo.toml', Path('crates/core/Cargo.toml').read_text()
                   + '\n[lib]\npath="src/entry.rs"\n')
        self.write(root, self.ORIGINAL)
        base = self.commit()
        self.write(root, self.ORIGINAL + self.DECLARATION)
        self.write(self.LEAF, self.BODY)
        plan = selector.select(config, base, self.commit(), profile='pr-fast')
        self.assertEqual(plan['oasis7_required_wasm_packages'], [])
        self.assertNotIn('launcher_web', plan['groups'])
        self.assertEqual(plan['architecture']['build_policy'], 'ordinary-targets')

    def test_root_delta_does_not_change_legacy_execution_or_full_inventory(self):
        config, base = self.fixture()
        head = self.add_module()
        legacy = dict(config)
        del legacy['fast_execution_version']
        del legacy['ordinary_fast_component_rules']
        del legacy['architecture_build_policy_version']
        plan = selector.select(legacy, base, head, profile='pr-fast')
        self.assert_portable(plan)
        self.assertNotIn('ordinary_execution', plan)
        for profile in ('periodic-full', 'candidate-full'):
            plan = selector.full_plan(config, profile, head)
            self.assertEqual(set(plan['groups']), set(config['groups']))
            self.assertEqual(set(plan['oasis7_required_wasm_packages']), {'core', 'other'})
            self.assertNotIn('build_policy', plan['architecture'])

    def test_changed_existing_cfg_and_mixed_portable_body_keep_original_coverage(self):
        config, base = self.fixture()
        for root in (self.ORIGINAL.replace('{ 1 }', '{ 2 }') + self.DECLARATION,
                     self.ORIGINAL + self.DECLARATION + 'pub const PORTABLE: u8 = 2;\n',
                     '#[cfg(not(target_arch="wasm32"))]\n' + self.ORIGINAL + self.DECLARATION):
            self.write(self.ROOT, root)
            self.write(self.LEAF, self.BODY)
            with self.subTest(root=root):
                self.assert_portable(selector.select(config, base, self.commit(), profile='pr-fast'))
            self.git('reset', '--hard', base)
        self.write(self.ROOT, '#[cfg(not(target_arch="wasm32"))] mod original;\n')
        self.write('crates/core/src/original.rs', 'pub fn original() {}\n')
        base = self.commit()
        self.write(self.ROOT, 'mod original;\n' + self.DECLARATION)
        self.write(self.LEAF, self.BODY)
        plan = selector.select(config, base, self.commit(), profile='pr-fast')
        self.assertEqual(plan['scope'], 'full')
        self.assert_portable(plan)

    def test_existing_unwired_or_renamed_module_cannot_borrow_new_source_proof(self):
        config, base = self.fixture()
        self.write(self.LEAF, self.BODY)
        base = self.commit()
        self.write(self.ROOT, self.ORIGINAL + self.DECLARATION)
        self.assert_portable(selector.select(config, base, self.commit(), profile='pr-fast'))
        self.git('reset', '--hard', base)
        self.write(self.ROOT, self.ORIGINAL + '#[cfg(not(target_arch="wasm32"))] mod added;\n')
        base = self.commit()
        self.write(self.ROOT, self.ORIGINAL + '#[cfg(not(target_arch="wasm32"))] mod renamed;\n')
        self.git('mv', self.LEAF, 'crates/core/src/renamed.rs')
        plan = selector.select(config, base, self.commit(), profile='pr-fast')
        self.assertNotIn('ordinary_execution', plan)
        self.assert_portable(plan)

    def test_portable_and_host_readers_of_root_or_new_module_keep_consumers(self):
        config, original_base = self.fixture()
        for target in ('lib.rs', 'added.rs'):
            for reader in ('data', 'module', 'build'):
                self.git('reset', '--hard', original_base)
                path = 'crates/consumer/' + ('build.rs' if reader == 'build' else 'src/lib.rs')
                relative = os.path.relpath('crates/core/src/' + target, Path(path).parent)
                body = (f'#[path="{relative}"] mod shared;\n' if reader == 'module'
                        else f'const SOURCE: &str = include_str!("{relative}");\n')
                self.write(path, body + ('fn main() {}\n' if reader == 'build' else ''))
                base = self.commit()
                with self.subTest(target=target, reader=reader):
                    self.assert_portable(selector.select(config, base, self.add_module(), profile='pr-fast'))

    def test_root_delta_preserves_transitive_host_generation_and_unknown_readers(self):
        config, original_base = self.fixture()
        consumer_manifest = Path('crates/consumer/Cargo.toml').read_text()
        for kind in ('build-dependency', 'proc-macro', 'dynamic-data', 'dynamic-include'):
            self.git('reset', '--hard', original_base)
            if kind == 'build-dependency':
                self.write('crates/consumer/Cargo.toml', consumer_manifest
                           + '\n[build-dependencies]\ncore={workspace=true}\n')
                self.write('crates/consumer/build.rs', 'fn main() { let _ = core::portable(); }\n')
            elif kind == 'proc-macro':
                self.write('crates/other/Cargo.toml', Path('crates/other/Cargo.toml').read_text()
                           + '\n[lib]\nproc-macro=true\n[dependencies]\ncore={workspace=true}\n')
                self.write('crates/consumer/Cargo.toml', consumer_manifest
                           + '\n[dependencies]\nother={path="../other"}\n')
            elif kind == 'dynamic-data':
                self.write('crates/consumer/src/lib.rs', 'const SOURCE: &str = include_str!(env!("SOURCE"));\n')
            else:
                self.write('crates/core/src/lib.rs', self.ORIGINAL + 'include!(env!("SOURCE"));\n')
            base = self.commit()
            original = Path(self.ROOT).read_text()
            self.write(self.ROOT, original + self.DECLARATION)
            self.write(self.LEAF, self.BODY)
            with self.subTest(kind=kind):
                head = self.commit()
                if kind == 'dynamic-include':
                    with self.assertRaisesRegex(selector.TestPolicyError, 'unresolved or cyclic module path'):
                        selector.select(config, base, head, profile='pr-fast')
                else:
                    self.assert_portable(selector.select(config, base, head, profile='pr-fast'))

    def test_uncovered_source_and_explicit_risk_are_not_waived_by_root_delta(self):
        config, base = self.fixture()
        head = self.add_module()
        risky = json.loads(json.dumps(config))
        risky['formal_risk_rules'].append({'match': [self.LEAF], 'suites': ['world-history'],
                                           'reason': 'retained history'})
        plan = selector.select(risky, base, head, profile='pr-fast')
        self.assertEqual(plan['formal_suites'], ['world-history'])
        self.assertNotIn('build_policy', plan['architecture'])
        self.write('crates/core/src/unwired.rs', '#[test] fn never_runs() {}\n')
        plan = selector.select(config, base, self.commit(), profile='pr-fast')
        self.assertNotIn('ordinary_execution', plan)
        self.assert_portable(plan)

        self.write('not-registered/added.rs', self.BODY)
        with self.assertRaises(selector.UnknownInputsError):
            selector.select(config, base, self.commit(), profile='pr-fast')

    def test_native_delta_parser_preserves_exact_gate_and_top_level_boundary(self):
        _, base = self.fixture()
        native = '#[cfg(not(target_arch="wasm32"))] '
        for declaration in (
                'mod added;', '#[cfg(test)] mod added;',
                '#[cfg(any(test,not(target_arch="wasm32")))] mod added;',
                native + '#[path="added.rs"] mod added;',
                native + '#[unknown] mod added;',
                native + 'mod added {}', native + 'pub(in crate) mod added;',
                'fn nested() { ' + native + 'mod added; }',
                'mount! { ' + native + 'mod added; }',
                'include!("added.rs");'):
            head = self.add_module(declaration=declaration)
            with self.subTest(declaration=declaration):
                self.assertIsNone(selector.added_static_module_sources(self.ROOT, [(base, head)], native_root=True))
            self.git('reset', '--hard', base)

        self.write(self.ROOT, '/*\n' + self.ORIGINAL + '*/\n' + self.DECLARATION)
        self.write(self.LEAF, self.BODY)
        self.assertIsNone(selector.added_static_module_sources(self.ROOT, [(base, self.commit())], native_root=True))

    def test_native_delta_parser_cannot_rebind_an_existing_attribute(self):
        _, base = self.fixture()
        original = self.ORIGINAL + '#[cfg(not(target_arch="wasm32"))]\nmod original;\n'
        self.write(self.ROOT, original)
        self.write('crates/core/src/original.rs', 'pub fn original() {}\n')
        base = self.commit()
        self.write(self.ROOT, original.replace('mod original;', 'mod added;\nmod original;'))
        self.write(self.LEAF, self.BODY)
        self.assertIsNone(selector.added_static_module_sources(self.ROOT, [(base, self.commit())], native_root=True))

    def test_native_delta_parser_requires_unique_new_regular_source(self):
        _, base = self.fixture()
        for kind in ('missing', 'ambiguous', 'symlink', 'executable'):
            self.git('reset', '--hard', base)
            self.write(self.ROOT, self.ORIGINAL + self.DECLARATION)
            if kind != 'missing':
                if kind == 'symlink':
                    Path(self.LEAF).symlink_to('lib.rs')
                else:
                    self.write(self.LEAF, self.BODY)
                if kind == 'ambiguous':
                    self.write('crates/core/src/added/mod.rs', self.BODY)
                elif kind == 'executable':
                    Path(self.LEAF).chmod(0o755)
            with self.subTest(kind=kind):
                self.assertIsNone(selector.added_static_module_sources(self.ROOT, [(base, self.commit())], native_root=True))

    def test_native_delta_accepts_independently_gated_visibility_and_directory_modules(self):
        config, base = self.fixture()
        root = self.ORIGINAL
        expected = set()
        for number, visibility in enumerate(('', 'pub ', 'pub(crate) ', 'pub(self) ', 'pub(super) ')):
            name = 'r#type' if number == 0 else 'added_' + str(number)
            root += '#[cfg(not(target_arch="wasm32"))]\n' + visibility + 'mod ' + name + ';\n'
            path = 'crates/core/src/' + name.removeprefix('r#') + ('/mod.rs' if number == 0 else '.rs')
            self.write(path, self.BODY)
            expected.add(path)
        self.write(self.ROOT, root)
        head = self.commit()
        self.assertEqual(selector.added_static_module_sources(self.ROOT, [(base, head)], native_root=True), expected)
        self.assertFalse(selector.only_static_modules_added(self.ROOT, [(base, head)]))
        self.assertEqual(selector.select(config, base, head, profile='pr-fast')['oasis7_required_wasm_packages'], [])

    def test_native_delta_requires_a_selected_complete_library_not_only_integration(self):
        config, base = self.fixture()
        self.write('crates/core/tests/probe.rs', '#[path="../src/lib.rs"] mod product;\n')
        base = self.commit()
        config['ordinary_fast_targets']['core-lib'].update(target_kind='test', target_name='probe')
        plan = selector.select(config, base, self.add_module(), profile='pr-fast')
        self.assertEqual(plan['ordinary_execution'], 'inline')
        self.assert_portable(plan)

    def test_native_delta_checks_source_and_actual_tested_merge_independently(self):
        config, base = self.fixture()
        for wider in ('source', 'tested'):
            self.git('checkout', '-qb', 'source-' + wider, base)
            self.write(self.ROOT, self.ORIGINAL + self.DECLARATION
                       + ('pub const PORTABLE: u8 = 2;\n' if wider == 'source' else ''))
            self.write(self.LEAF, self.BODY)
            source = self.commit()
            self.git('checkout', '-qb', 'target-' + wider, base)
            self.write('doc/readme.md', 'target history is not the PR delta')
            target = self.commit()
            self.git('merge', '--no-ff', '-qm', 'fixture merge', source)
            if wider == 'tested':
                clean = selector.select(config, target, source, self.git('rev-parse', 'HEAD'), profile='pr-fast')
                self.assertEqual(clean['architecture']['build_policy'], 'ordinary-targets')
            self.write(self.ROOT, self.ORIGINAL + self.DECLARATION
                       + ('pub const PORTABLE: u8 = 2;\n' if wider == 'tested' else ''))
            self.git('add', '-A')
            self.git('commit', '--amend', '-qm', 'fixture resolution')
            with self.subTest(wider=wider):
                plan = selector.select(config, target, source, self.git('rev-parse', 'HEAD'), profile='pr-fast')
                self.assert_portable(plan)

    def test_merge_restoring_native_addition_cannot_borrow_source_only_proof(self):
        config, base = self.fixture()
        source = self.add_module()
        self.git('checkout', '-qb', 'target', base)
        self.write('doc/readme.md', 'target advance')
        target = self.commit()
        self.git('merge', '--no-ff', '-qm', 'fixture merge', source)
        self.git('checkout', target, '--', self.ROOT)
        self.git('rm', self.LEAF)
        self.git('commit', '--amend', '-qm', 'restore complete module addition')
        plan = selector.select(config, target, source, self.git('rev-parse', 'HEAD'), profile='pr-fast')
        self.assert_portable(plan)


if __name__ == '__main__':
    unittest.main()
