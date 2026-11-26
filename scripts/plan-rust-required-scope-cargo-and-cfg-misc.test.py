#!/usr/bin/env python3
import copy
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import tempfile
import textwrap
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

    def ordinary_route_cargo(self, arguments, *, list_only=False):
        environment = dict(os.environ)
        environment.pop('RUSTC_WRAPPER', None)
        command = ['cargo', 'test', *arguments, '--offline']
        if list_only:
            command += ['--', '--list']
        result = subprocess.run(command, env=environment, text=True, capture_output=True, timeout=60)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        return result.stdout


class ScaleRiskPlans(unittest.TestCase):
    def test_body_boundary_keeps_topology_const_evaluation_and_macro_inputs(self):
        source = 'struct Feedback; impl Feedback { fn value(&self) -> u8 { 1 } }\nfn leaf() -> u8 { 2 }\n'
        expected = selector.scale_source_boundary(source)
        self.assertEqual(expected, selector.scale_source_boundary(source.replace('{ 1 }', '{ 3 }')))
        self.assertEqual(expected, selector.scale_source_boundary(source + '// impl Drop for Store {}\n'))
        for changed in (source + 'fn added() {}', source.replace('impl Feedback', 'impl Other'),
                        source.replace('{ 1 }', '{ impl Drop for Store { fn drop(&mut self) {} } 1 }'),
                        source.replace('{ 1 }', '{ { impl Trait for Store {} } 1 }'),
                        source.replace('{ 1 }', '{ unsafe { 1 } }'),
                        source.replace('{ 1 }', '{ unknown_macro!(); 1 }'),
                        source.replace('fn leaf', 'const fn leaf'),
                        source.replace('fn leaf', '#[unknown_attribute] fn leaf'),
                        source.replace('{ 1 }', '{ #[cfg(test)] let x = 1; x }')):
            with self.subTest(changed=changed):
                self.assertNotEqual(expected, selector.scale_source_boundary(changed))
        macro = 'fn leaf() -> u8 { assert!(true); 2 }'
        self.assertEqual(selector.scale_source_boundary(macro), selector.scale_source_boundary(macro.replace('; 2', '; 3')))
        self.assertNotEqual(selector.scale_source_boundary(macro), selector.scale_source_boundary(macro.replace('true', 'false')))
        constant = 'const fn leaf() -> u8 { 1 }'
        self.assertNotEqual(selector.scale_source_boundary(constant), selector.scale_source_boundary(constant.replace('1', '2')))

    def test_isolation_is_pinned_to_trusted_topology_and_both_differences(self):
        path, anchor = 'crates/feedback/src/feedback.rs', 'crates/feedback/src/lib.rs'
        source = b'fn leaf() -> u8 { 1 }'
        root = b'mod feedback;'
        contract = dict(isolated_sources={path: selector.scale_source_boundary(source.decode())},
                        anchors={anchor: hashlib.sha256(root).hexdigest()})
        inputs = {ref: dict(sources={path: source, anchor: root}, includes=set(), data_readers={},
                           unresolved_data_readers=set(), unresolved_includes=set())
                  for ref in ('base', 'head', 'target', 'merge')}
        inputs['head']['sources'][path] = source.replace(b'1', b'2')
        inputs['merge']['sources'][path] = source.replace(b'1', b'2')
        comparisons = [('base', 'head'), ('target', 'merge')]
        with patch.object(selector, 'resolve_commit', side_effect=lambda ref: ref):
            self.assertTrue(selector.scale_isolation_proven(contract, [path], comparisons, inputs))
            for extra_path, contents in ((path, b'fn leaf() -> u8 { bad!(); 2 }'),
                    (anchor, b'mod feedback; mod another;'),
                    ('crates/feedback/src/other.rs', b'macro_rules! assert { () => {} }'),
                    ('crates/feedback/src/other.rs', b'use custom::format;')):
                changed = copy.deepcopy(inputs)
                changed['merge']['sources'][extra_path] = contents
                with self.subTest(extra_path=extra_path, contents=contents):
                    self.assertFalse(selector.scale_isolation_proven(contract, [path], comparisons, changed))
            self.assertFalse(selector.scale_isolation_proven(contract, [path, 'new.rs'], comparisons, inputs))
            for change in (dict(includes={path}), dict(data_readers={path: {'crates/core/tests/scale.rs'}}),
                           dict(unresolved_data_readers={'crates/core/tests/scale.rs'}),
                           dict(unresolved_includes={'core'})):
                changed = copy.deepcopy(inputs)
                changed['merge'].update(change)
                with self.subTest(reader_change=change):
                    self.assertFalse(selector.scale_isolation_proven(contract, [path], comparisons, changed))
            changed = copy.deepcopy(inputs)
            del changed['merge']['data_readers']
            self.assertFalse(selector.scale_isolation_proven(contract, [path], comparisons, changed))

    def test_generic_affected_packages_add_full_targets_without_changing_existing_fast_contracts(self):
        target = dict(package='oasis7', target_kind='test', target_name='scale', default_features=True,
                      features=['test_tier_full'], cases=['full_scale'])
        config = dict(ordinary_scale_contract=dict(version=1, targets=[target], isolated_sources={}, anchors={}))
        original = dict(groups=['baseline', 'oasis7_required'], scope='targeted', test_policy_changed=False,
                        oasis7_required_packages=['oasis7'], reasons=[])
        for change in ({}, {'scope': 'full'}, {'test_policy_changed': True}):
            selected = copy.deepcopy(original) | change
            selector.add_ordinary_scale_contract(config, selected, ['production.rs'],
                {'production.rs': {'oasis7'}}, [('base', 'head')], {})
            self.assertEqual(selected['ordinary_risk_targets'], [target])
        for change in ({'oasis7_required_packages': ['oasis7_runtime']}, {'ordinary_fast_targets': [target]}):
            selected = copy.deepcopy(original) | change
            selector.add_ordinary_scale_contract(config, selected, [], {}, [], {})
            self.assertNotIn('ordinary_risk_targets', selected)


class LocalStatementCfgOwners(unittest.TestCase):
    PATH = 'crates/oasis7_chain_runtime/src/host/execution_bridge/tests/qa_conformance/application_stream_boundaries.rs'
    BEFORE = ('#[test] fn existing() {}\n'
              '#[cfg(any(target_os = "macos", target_os = "linux"))]\n'
              'fn small_buffers() {\n'
              '  #[cfg(target_os = "macos")]\n'
              '  let (level, send, receive) = (0xffff, 0x1001, 0x1002);\n'
              '  #[cfg(target_os = "linux")]\n'
              '  let (level, send, receive) = (1, 7, 8);\n}')

    def policy(self, before, after):
        def source(*args):
            self.assertEqual(args[0], 'show')
            ref, path = args[1].split(':', 1)
            self.assertEqual(path, self.PATH)
            return {'base': before, 'source': after}[ref].encode()
        with patch.object(selector, 'git', side_effect=source):
            return selector.changed_test_policy(
                'base', 'source', [self.PATH], {self.PATH: {'oasis7_chain_runtime'}},
                {'oasis7_chain_runtime'}, {'oasis7_chain_runtime': {'test_tier_required', 'test_tier_full'}})

    def test_unchanged_local_cfg_is_stable_after_unrelated_tokens(self):
        after = 'struct OrderedResponseProgress { complete: bool }\n' + self.BEFORE
        self.assertEqual(self.policy(self.BEFORE, after), ([], False, []))
        after = self.BEFORE.replace('fn small_buffers() {', 'fn small_buffers() { let unrelated = 7;')
        self.assertEqual(self.policy(self.BEFORE, after), ([], False, []))

    def test_changed_or_new_local_cfg_still_requires_validated_driver(self):
        variants = [
            self.BEFORE.replace('cfg(target_os = "macos")', 'cfg(target_os = "windows")'),
            self.BEFORE.replace('fn small_buffers() {',
                                'fn small_buffers() { #[cfg(target_os = "macos")] let extra = 1;'),
            self.BEFORE.replace('(0xffff, 0x1001, 0x1002)', '(0xffff, 0x1001, 0x1003)'),
        ]
        for after in variants:
            with self.subTest(after=after), self.assertRaisesRegex(
                    selector.TestPolicyError, 'validated test feature combinations'):
                self.policy(self.BEFORE, after)

    def test_method_scopes_keep_conservative_handling(self):
        body = self.BEFORE.split('fn small_buffers() ', 1)[1]
        before = '#[test] fn existing() {}\nimpl First { fn small_buffers() ' + body + ' }'
        after = 'struct NewItem;\n' + before.replace('impl First', 'impl Second')
        with self.assertRaisesRegex(selector.TestPolicyError, 'validated test feature combinations'):
            self.policy(before, after)

    def test_moving_local_cfg_to_another_function_is_not_unchanged(self):
        moved = self.BEFORE.replace(
            '  #[cfg(target_os = "macos")]\n  let (level, send, receive) = (0xffff, 0x1001, 0x1002);\n', '')
        moved += '\nfn other() { #[cfg(target_os = "macos")] let (level, send, receive) = (0xffff, 0x1001, 0x1002); }'
        with self.assertRaisesRegex(selector.TestPolicyError, 'validated test feature combinations'):
            self.policy(self.BEFORE, moved)


class MethodCallCfgExecution(_ScopeFixtureBase):
    def test_static_generic_and_trait_impl_fixtures_keep_real_ordinary_hooks(self):
        self.write('.gitignore', 'target/\nCargo.lock\n')
        self.write('Cargo.toml', '[workspace]\nresolver="2"\nmembers=["crates/core"]\n')
        fixture = textwrap.dedent('''\
            #![allow(dead_code)]
            use std::marker::PhantomData;
            trait Endpoint {}
            impl Endpoint for () {}
            struct Writer<P, R>(PhantomData<(P, R)>);
            trait Resume { fn resume(&mut self) -> Result<usize, String>; }
            mod crash {
                use std::sync::atomic::{AtomicUsize, Ordering};
                pub static CALLS: AtomicUsize = AtomicUsize::new(0);
                pub fn stop_after(_: &str) -> Result<(), String> {
                    CALLS.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                }
            }
            fn verify_qualified_commit() -> Result<usize, String> { Ok(7) }
            impl<P: Endpoint, R: Endpoint> Writer<P, R> {
                fn resume(&mut self) -> Result<usize, String> {
                    #[cfg(all(test, unix))]
                    crash::stop_after("dual_qualified")?;
                    let head = 3usize.checked_add(4).ok_or("overflow")?;
                    #[cfg(all(test, unix))]
                    crash::stop_after("publication_saved")?;
                    Ok(head)
                }
            }
            #[test]
            fn executes_retained_hooks() {
                let mut writer = Writer::<(), ()>(PhantomData);
                assert_eq!(writer.resume().unwrap(), 7);
                assert_eq!(crash::CALLS.load(std::sync::atomic::Ordering::SeqCst),
                           if cfg!(unix) { 2 } else { 0 });
            }
        ''')
        for trait_impl in (False, True):
            before = fixture.replace('> Writer<P, R> {', '> Resume for Writer<P, R> {') if trait_impl else fixture
            after = before.replace('3usize.checked_add(4).ok_or("overflow")?', 'verify_qualified_commit()?')
            with self.subTest(trait_impl=trait_impl):
                self.write('crates/core/src/lib.rs', before)
                base = self.commit()
                before_output = self.ordinary_route_cargo(['-p', 'core', '--lib', '--features', 'test_tier_required'])
                self.assertIn('executes_retained_hooks ... ok', before_output)
                self.write('crates/core/src/lib.rs', after)
                plan = selector.select(self.fast_config(), base, self.commit(), profile='pr-fast')
                self.assertFalse(plan['test_policy_changed'])
                self.assertEqual(plan['test_policy_paths'], [])
                self.assertIn('core', plan['oasis7_required_packages'])
                after_output = self.ordinary_route_cargo(['-p', 'core', '--lib', '--features', 'test_tier_required'])
                self.assertIn('executes_retained_hooks ... ok', after_output)


class FilteredOpaqueCargoItems(_ScopeFixtureBase):
    def fixture(self, kind):
        self.write('.gitignore', 'target/\nCargo.lock\n')
        self.write('Cargo.toml', '[workspace]\nresolver="2"\nmembers=["crates/core", "crates/other"]\n')
        self.write('crates/core/Cargo.toml', '[package]\nname="core"\nversion="0.1.0"\nedition="2021"\n'
                   '[features]\ntest_tier_required=[]\ntest_tier_full=["test_tier_required"]\n'
                   '[dependencies]\nother={path="../other"}\n')
        source = '#![cfg(not(target_arch="wasm32"))]\nmod selected { #[test] fn existing() {} }\n'
        if kind == 'qualified-proc-attribute':
            self.write('crates/other/Cargo.toml', '[package]\nname="other"\nversion="0.1.0"\n'
                       '[lib]\nproc-macro=true\n')
            self.write('crates/other/src/lib.rs', 'extern crate proc_macro;\nuse proc_macro::TokenStream;\n'
                       '#[proc_macro_attribute] pub fn case(_: TokenStream, item: TokenStream) -> TokenStream {\n'
                       'format!("#[test] {}", item).parse().unwrap()\n}\n')
            source += 'extern crate other as doc; #[doc::case] fn generated() { assert_eq!(1, 1); }\n'
            failure = 'generated'
        elif kind in ('function-local-wrapper', 'function-local-alias'):
            macro = ('#[macro_export] macro_rules! define_exported { ($value:expr) => {\n'
                     '#[macro_export] macro_rules! exported_case { () => {\n'
                     '#[test] fn generated() { assert_eq!($value, 1); }\n}; }\n}; }\n')
            self.write('crates/other/src/lib.rs', macro + macro.replace('define_exported', 'define_shifted')
                       .replace('assert_eq!($value, 1)', 'assert_eq!($value + 1, 1)'))
            invocation = ('use other::define_exported as generated; generated!(1);'
                          if kind == 'function-local-alias' else 'other::define_exported!(1);')
            source = ('#![allow(macro_expanded_macro_exports_accessed_by_absolute_paths)]\n' + source
                      + 'fn install() { ' + invocation + ' }\nmod outside;\n')
            self.write('crates/core/src/outside.rs', 'crate::exported_case!();\n')
            failure = 'outside::generated'
        else:
            source += ('fn install() { #[macro_export] macro_rules! exported_case { ($attr:meta) => { '
                       '#[$attr] fn generated() { assert_eq!(1, 1); } }; } }\nmod outside;\n')
            self.write('crates/core/src/outside.rs', 'crate::exported_case!(test);\n')
            failure = 'outside::generated'
        self.write('crates/core/src/lib.rs', source)
        base = self.commit()
        changed = (source.replace('other::define_exported!(1)', 'other::define_exported!(2)')
                   if kind == 'function-local-wrapper' else
                   source.replace('use other::define_exported', 'use other::define_shifted')
                   if kind == 'function-local-alias' else source.replace('assert_eq!(1, 1)', 'assert_eq!(1, 2)'))
        self.write('crates/core/src/lib.rs', changed)
        config = self.bounded_config()
        config.update(fast_execution_version=1, architecture_build_policy_version=1,
                      ordinary_case_prefix_contract_version=1,
                      ordinary_fast_component_rules=[dict(match=['crates/core/src/**'],
                          exclude=['crates/core/src/bin/**'], targets=['core-lib'], reason='component')])
        config['ordinary_fast_targets']['core-lib'].update(cases=['selected::existing'], case_prefixes=['selected::'])
        return config, base, self.commit(), failure

    def check_generated_case(self, kind):
        config, base, head, failure = self.fixture(kind)
        plan = selector.select(config, base, head, profile='pr-fast')
        self.assertNotIn('ordinary_fast_targets', plan)
        self.assertTrue(plan['fast_plan_exceptions'])
        environment = dict(os.environ)
        environment.pop('RUSTC_WRAPPER', None)
        command = ['cargo', 'test', '-p', 'core', '--lib', '--no-default-features',
                   '--features', 'test_tier_required', '--offline']
        filtered = subprocess.run(command + ['--', '--exact', 'selected::existing'],
                                  env=environment, text=True, capture_output=True, timeout=60)
        self.assertEqual(filtered.returncode, 0, filtered.stdout + filtered.stderr)
        self.assertIn('1 passed; 0 failed', filtered.stdout)
        self.assertIn('1 filtered out', filtered.stdout)
        complete = subprocess.run(command, env=environment, text=True, capture_output=True, timeout=60)
        self.assertEqual(complete.returncode, 101, complete.stdout + complete.stderr)
        self.assertIn('test ' + failure + ' ... FAILED', complete.stdout)
        self.assertIn('1 passed; 1 failed', complete.stdout)

    def test_qualified_proc_attribute_generated_case_forces_complete_library(self):
        self.check_generated_case('qualified-proc-attribute')

    def test_function_local_exported_macro_generated_case_forces_complete_library(self):
        self.check_generated_case('function-local-definition')

    def test_function_local_wrapper_generated_case_forces_complete_library(self):
        self.check_generated_case('function-local-wrapper')

    def test_function_local_macro_alias_change_forces_complete_library(self):
        self.check_generated_case('function-local-alias')


class AlternativeModuleMounts(unittest.TestCase):
    LEAF = 'crates/core/src/cases.rs'
    ROOT = 'crates/core/src/lib.rs'

    def policy(self, *, test_mount=True, extra=None, test_gate='', defaults=(), root_gate=''):
        # Exact shape of host production mount versus its real test bridge.
        source = {
            self.ROOT: (root_gate + 'mod host;').encode(),
            'crates/core/src/host.rs': (
                '#[cfg(not(test))] #[path="shared/mod.rs"] mod production;\n'
                + ('#[cfg(test)] #[path="real_tests.rs"] mod real;\n' if test_mount else '')
            ).encode(),
            'crates/core/src/real_tests.rs': (test_gate + '#[path="shared/mod.rs"] mod real_bridge;').encode(),
            'crates/core/src/shared/mod.rs': b'#[cfg(test)] #[path="../cases.rs"] mod cases;',
            self.LEAF: b'#[test] fn existing() {}\n#[test] fn added() {}',
        }
        uncertain = set()
        if extra == 'expanded_include':
            source[self.ROOT] += b'\n#[cfg(feature="test_tier_full")] mod extra;'
            source['crates/core/src/extra.rs'] = b'include!("cases.rs");'
        elif extra:
            source[self.ROOT] += (b'\n#[cfg(test)] mod extra;' if extra.endswith('_plain')
                                  else b'\n#[cfg(feature="custom_off")] mod extra;')
            if extra in ('unknown_include', 'unknown_include_plain'):
                source['crates/core/src/extra.rs'] = b'include!(concat!("ca", "ses.rs"));'
                uncertain.add('crates/core/src/extra.rs')
            elif extra in ('cycle', 'cycle_plain'):
                source['crates/core/src/extra.rs'] = b'#[path="shared/mod.rs"] mod bridge; #[path="cycle.rs"] mod cycle;'
                source['crates/core/src/cycle.rs'] = b'#[path="extra.rs"] mod back;'
            else:
                source['crates/core/src/extra.rs'] = b'#[path="shared/mod.rs"] mod bridge;'
        before = '#[test] fn existing() {}'
        after = source[self.LEAF].decode()
        def read(*args):
            self.assertEqual(args[0], 'show')
            ref, path = args[1].split(':', 1)
            self.assertEqual(path, self.LEAF)
            return {'base': before, 'source': after}[ref].encode()
        with patch.object(selector, 'git', side_effect=read):
            return selector.changed_test_policy(
                'base', 'source', [self.LEAF], {self.LEAF: {'core'}}, {'core'},
                {'core': {'test_tier_required', 'test_tier_full', 'custom_off', 'native_driver', 'transport'}},
                ordinary_roots={self.ROOT}, source_inputs={
                    'sources': source, 'directories': {'core': 'crates/core'},
                    'uncertain_modules': uncertain, 'unresolved_includes': (),
                    'default_features': {'core': set(defaults)},
                })

    def test_production_and_test_mounts_are_alternative_paths(self):
        paths, ignored, production = self.policy()
        self.assertFalse(ignored)
        self.assertFalse(production)
        self.assertEqual(paths, [])

    def test_alternative_paths_do_not_or_together_contradictory_same_path_gates(self):
        with self.assertRaises(selector.TestPolicyError):
            self.policy(test_gate='#![cfg(not(test))]\n')

    def test_all_statically_disabled_paths_still_reject(self):
        with self.assertRaises(selector.TestPolicyError):
            self.policy(test_mount=False)

    def test_known_test_path_cannot_hide_unknown_alternative_provenance(self):
        for extra in ('unknown_feature', 'unknown_include', 'cycle'):
            with self.subTest(extra=extra), self.assertRaises(selector.TestPolicyError):
                self.policy(extra=extra)

    def test_good_path_cannot_hide_plain_nonliteral_include_or_exact_cycle(self):
        for extra in ('unknown_include_plain', 'cycle_plain'):
            with self.subTest(extra=extra), self.assertRaises(selector.TestPolicyError):
                self.policy(extra=extra)

    def test_default_features_are_derived_transitively_from_manifest(self):
        manifest = {'features': {'default': ['native_driver'],
                                'native_driver': ['transport', 'dep:secret', 'foreign/feature'],
                                'transport': []}}
        defaults = selector.default_feature_closure(manifest)
        self.assertEqual(defaults, {'default', 'native_driver', 'transport'})
        paths, ignored, production = self.policy(
            defaults=defaults, root_gate='#[cfg(feature="transport")] ')
        self.assertFalse(ignored or production)
        self.assertEqual(paths, [])
        with self.assertRaises(selector.TestPolicyError):
            self.policy(root_gate='#[cfg(feature="transport")] ')

    def test_reachable_expanded_include_retains_feature_policy_coverage(self):
        paths, ignored, production = self.policy(extra='expanded_include')
        self.assertEqual(paths, [self.LEAF])
        self.assertFalse(ignored)
        self.assertFalse(production)


class LoadedExecutableComponent(_ScopeFixtureBase):
    def fixture(self):
        config = json.loads((HERE / 'ci-required-scope.json').read_text())
        image = 'oasis7_executable_image'
        self.write(f'crates/{image}/Cargo.toml',
                   '[package]\nname="oasis7_executable_image"\nversion="0.1.0"\nedition="2024"\n'
                   '[dependencies]\nblake3="1"\n')
        self.write('crates/oasis7_chain_runtime/Cargo.toml',
                   '[package]\nname="oasis7_chain_runtime"\nversion="0.1.0"\nedition="2024"\n'
                   '[dependencies]\noasis7_executable_image={path="../oasis7_executable_image"}\n')
        self.write('crates/oasis7_chain_runtime/src/lib.rs',
                   'pub use oasis7_executable_image::executable_digest;\n')
        for path in ('crates/oasis7_executable_image/src/lib.rs',
                     'crates/oasis7_executable_image/src/digest.rs',
                     'crates/oasis7_chain_runtime/src/host/executable_image/mod.rs',
                     'crates/oasis7_chain_runtime/src/host/executable_image/replacement_tests.rs',
                     'crates/oasis7_chain_runtime/src/host/executable_image/replacement_tests/fixture.rs',
                     'crates/oasis7_chain_runtime/src/host/executable_image/replacement_tests/expectations.rs'):
            self.write(path, (HERE.parent / path).read_text())
        return config, self.commit()

    def test_digest_uses_leaf_target_but_facade_and_platform_keep_host_consumers(self):
        config, base = self.fixture()
        path = 'crates/oasis7_executable_image/src/digest.rs'
        self.write(path, Path(path).read_text().replace('64 * 1024', '32 * 1024'))
        plan = selector.select(config, base, self.commit(), profile='pr-fast')
        self.assertEqual(plan['ordinary_execution'], 'inline')
        self.assertEqual(plan['ordinary_fast_targets'], [config['ordinary_fast_targets']['executable-image-lib']])
        self.assertEqual(set(plan['groups']), {'baseline', 'rust_baseline', 'oasis7_required'})
        self.assertEqual(plan['oasis7_required_wasm_packages'], [])
        self.assertEqual(plan['fast_plan_exceptions'], [])
        self.assertFalse(plan['test_policy_changed'])
        self.assertIn('oasis7_chain_runtime', plan['affected_packages'])
        for path in ('crates/oasis7_executable_image/src/lib.rs',
                     'crates/oasis7_chain_runtime/src/host/executable_image/mod.rs'):
            previous = self.git('rev-parse', 'HEAD')
            self.write(path, Path(path).read_text() + '\n// interface or platform input\n')
            expanded = selector.select(config, previous, self.commit(), profile='pr-fast')
            self.assertNotIn('ordinary_fast_targets', expanded)
            self.assertIn('oasis7_chain_runtime', expanded['oasis7_required_packages'])

    def test_full_inventory_keeps_leaf_behavior_and_real_host(self):
        config, base = self.fixture()
        target = config['ordinary_fast_targets']['executable-image-lib']
        self.assertEqual(len(target['cases']), 8)
        for profile in ('periodic-full', 'candidate-full'):
            plan = selector.full_plan(config, profile, base)
            self.assertIn(target, plan['ordinary_full_fast_targets'])
            self.assertIn('oasis7_executable_image', plan['oasis7_required_packages'])
            self.assertIn('oasis7_chain_runtime', plan['oasis7_required_packages'])


if __name__ == '__main__':
    unittest.main()
