#!/usr/bin/env python3
"""Ordinary case prefixes, shared incremental harnesses, and support handoff selection regressions split from ci-formal-process.test.py."""
import copy
import importlib.util
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import threading
import unittest
from unittest.mock import patch

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))


def module(name):
    spec = importlib.util.spec_from_file_location(name.replace('-', '_'), HERE / (name + '.py'))
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


artifacts = module('ci-test-artifacts')
from ci_process_runner import run


def compile_rust_fixture(source, executable, *, test=False, env=None):
    environment = os.environ if env is None else env
    command = ['rustc', *(['--test'] if test else []), str(source), '-o', str(executable)]
    linker = None
    if sys.platform == 'linux':
        version = subprocess.check_output(['rustc', '-vV'], text=True, env=environment)
        host = next(line[6:] for line in version.splitlines() if line.startswith('host: '))
        linker = environment.get('CARGO_TARGET_' + host.upper().replace('-', '_') + '_LINKER')
        if linker:
            # Direct rustc invocations do not read Cargo's target linker setting.
            command += ['-C', 'linker=' + linker]
    subprocess.run(command, check=True, env=environment)
    if linker and Path(linker).name == 'oasis7-mold-gcc':
        comment = subprocess.check_output(
            ['readelf', '-p', '.comment', str(executable)], text=True, env=environment)
        if not re.search(r'\bmold 3\.0\.0(?:\s|$)', comment):
            raise AssertionError('native Rust fixture was not linked by the pinned mold')


def fast_execution_selection(packages=('oasis7_viewer_codec',), suites=()):
    return dict(fast_plan_version=1, execution_profile='pr-fast', scope='targeted',
                groups=['baseline', 'oasis7_required'] + (['formal_process'] if suites else []),
                oasis7_required_packages=list(packages),
                oasis7_required_wasm_packages=[p for p in packages if p in artifacts.PORTABLE_REQUIRED_PACKAGES],
                formal_suites=list(suites), test_policy_changed=False,
                architecture=dict(source='source', tested='tested'))


def risk_target_selection():
    target = dict(package='oasis7_viewer_codec', target_kind='test', target_name='scale',
                  default_features=True, features=['test_tier_full'], cases=['full_scale'])
    return fast_execution_selection(('oasis7_viewer_codec',)) | dict(
        risk_test_inventory_version=1, ordinary_risk_targets=[target])


def fast_target(**changes):
    return dict(package='oasis7_viewer_codec', target_kind='test', target_name='pr_fast',
                default_features=False, features=['test_tier_required'], cases=['required::round_trip']) | changes


def fast_target_selection(targets=None, full=False):
    selected = fast_execution_selection(('oasis7_viewer_codec', 'oasis7_runtime'))
    key = 'ordinary_full_fast_targets' if full else 'ordinary_fast_targets'
    selected['full_fast_test_inventory_version' if full else 'fast_test_inventory_version'] = 1
    selected[key] = [fast_target()] if targets is None else targets
    if full:
        selected.update(execution_profile='periodic-full', scope='explicit-full')
    return selected


def fast_target_binary(target=None, variant='required-fast-0'):
    target = target or fast_target()
    return dict(ordinary=True, test=True, package=target['package'], target=target['target_name'],
                target_kind=[target['target_kind']], default_features=target['default_features'],
                requested_features=target['features'], features=target['features'],
                tests=target['cases'] + ['new_regression'], ignored_tests=[], variant=variant)


def inline_execution_selection(checks=None):
    return fast_target_selection() | {
        'fast_execution_version': 1, 'ordinary_execution': 'inline',
        'rust_baseline_checks': [] if checks is None else checks,
    }


class OrdinaryCasePrefixes(unittest.TestCase):
    def selection(self, *, full=False):
        target = fast_target(cases=['required::round_trip'],
                             case_prefixes=['required::', 'recovery::'])
        return fast_target_selection([target], full=full) | {
            'ordinary_case_prefix_contract_version': 1,
        }

    def manifest(self, *, full=False):
        selected = self.selection(full=full)
        target = selected['ordinary_full_fast_targets' if full else 'ordinary_fast_targets'][0]
        binary = fast_target_binary(target, 'required-full-fast-0' if full else 'required-fast-0')
        binary['tests'] = ['required::round_trip', 'required::new_regression', 'required::nested::reject',
                           'recovery::resume', 'required_neighbor::unrelated', 'outside::unrelated',
                           'required::ignored_child']
        binary['ignored_tests'] = ['required::ignored_child']
        return dict(scope='ordinary', execution_plan=selected, binaries={'fixture': binary})

    def test_prefixes_require_the_versioned_complete_schema(self):
        valid = self.selection()
        self.assertEqual(artifacts.required_native_commands(valid),
                         artifacts.required_native_commands(fast_target_selection()))
        for value in (None, True, 0, 2, '1'):
            invalid = copy.deepcopy(valid)
            if value is None:
                del invalid['ordinary_case_prefix_contract_version']
            else:
                invalid['ordinary_case_prefix_contract_version'] = value
            with self.subTest(capability=value), self.assertRaises(ValueError):
                artifacts.build_variants('ordinary', invalid)
        for prefixes in ([], '', [''], ['required'], ['required:'], ['required::::'],
                         ['required::*'], ['1required::'], ['required::\n'], ['required::', 'required::'],
                         ['required::', None], ['outside::']):
            invalid = copy.deepcopy(valid)
            invalid['ordinary_fast_targets'][0]['case_prefixes'] = prefixes
            with self.subTest(prefixes=prefixes), self.assertRaises(ValueError):
                artifacts.build_variants('ordinary', invalid)
        invalid = copy.deepcopy(valid)
        invalid['ordinary_fast_targets'][0]['unknown_selector'] = ['required::']
        with self.assertRaisesRegex(ValueError, 'target identity'):
            artifacts.build_variants('ordinary', invalid)
        risk = risk_target_selection() | {'ordinary_case_prefix_contract_version': 1}
        risk['ordinary_risk_targets'][0].update(cases=['scale::one'], case_prefixes=['scale::'])
        with self.assertRaisesRegex(ValueError, 'case prefixes'):
            artifacts.ordinary_risk_targets(risk)

    def test_actual_inventory_includes_new_and_nested_cases_but_not_neighbor_modules(self):
        manifest = self.manifest()
        artifacts.validate_ordinary_selection(manifest)
        self.assertEqual(artifacts.ordinary_entries(manifest), [dict(binary='fixture', batch=True,
                         skip_exact=['outside::unrelated', 'required_neighbor::unrelated'])])
        for mode in ('missing-minimum', 'ignored-minimum', 'empty-prefix', 'ignored-prefix', 'all-empty'):
            changed = copy.deepcopy(manifest)
            binary = changed['binaries']['fixture']
            if mode == 'missing-minimum':
                binary['tests'].remove('required::round_trip')
            elif mode == 'ignored-minimum':
                binary['ignored_tests'].append('required::round_trip')
            elif mode == 'empty-prefix':
                binary['tests'].remove('recovery::resume')
            elif mode == 'ignored-prefix':
                binary['ignored_tests'].append('recovery::resume')
            else:
                binary['tests'] = []
            with self.subTest(mode=mode), self.assertRaises(ValueError):
                artifacts.validate_ordinary_selection(changed)
            with self.subTest(mode=mode, execution=True), self.assertRaises(ValueError):
                artifacts.ordinary_entries(changed)

    def test_full_supplement_prefixes_cannot_filter_original_or_other_variants(self):
        manifest = self.manifest(full=True)
        target = manifest['execution_plan']['ordinary_full_fast_targets'][0]
        artifacts.validate_target_inventory([target], [manifest['binaries']['fixture']])
        manifest['binaries']['original'] = dict(manifest['binaries']['fixture'], variant='required')
        manifest['binaries']['other-variant'] = dict(manifest['binaries']['fixture'], variant='required-full-fast-1')
        for profile in ('periodic-full', 'candidate-full'):
            manifest['execution_plan']['execution_profile'] = profile
            with self.subTest(profile=profile), patch.object(artifacts, 'snapshot_regressions', return_value=()):
                self.assertEqual(artifacts.ordinary_entries(manifest), [dict(binary='fixture', batch=True,
                    skip_exact=['outside::unrelated', 'required_neighbor::unrelated']),
                    dict(binary='original', batch=True), dict(binary='other-variant', batch=True)])
        for changes in ({'scope': 'full'}, {'test_policy_changed': True}):
            manifest = self.manifest()
            manifest['execution_plan'].update(changes)
            with self.subTest(fallback=changes), patch.object(artifacts, 'snapshot_regressions', return_value=()):
                self.assertEqual(artifacts.ordinary_entries(manifest), [dict(binary='fixture', batch=True)])

    def test_generated_chain_library_names_preserve_deferred_formal_cases(self):
        target = fast_target(package='oasis7_chain_runtime', target_kind='lib',
            target_name='oasis7_chain_runtime', cases=['header::required'], case_prefixes=['header::'])
        selected = fast_execution_selection(('oasis7_chain_runtime',)) | dict(
            ordinary_case_prefix_contract_version=1, fast_test_inventory_version=1, ordinary_fast_targets=[target],
            formal_full_profiles_only_version=1, formal_shards=[],
            formal_deferred_ordinary_cases={'runtime-tests': ['header::four_node']})
        binary = fast_target_binary(target)
        binary['tests'] = ['header::required', 'header::four_node', 'header::four_node_neighbor', 'outside::unrelated']
        manifest = dict(scope='ordinary', execution_plan=selected, binaries={'generated': binary})
        artifacts.validate_ordinary_selection(manifest)
        self.assertEqual(artifacts.ordinary_entries(manifest), [dict(binary='generated', batch=True,
                         skip=['header::four_node'], skip_exact=['outside::unrelated'])])
        for changes in ({'package': 'oasis7'}, {'target_kind': ['test']}, {'target': 'different'}):
            wrong = copy.deepcopy(manifest)
            wrong['binaries']['generated'].update(changes)
            with self.subTest(identity=changes):
                self.assertEqual(artifacts.ordinary_entries(wrong), [dict(binary='generated', batch=True)])
                with self.assertRaises(ValueError):
                    artifacts.validate_ordinary_selection(wrong)
        minimum = copy.deepcopy(manifest)
        minimum['execution_plan']['ordinary_fast_targets'][0]['cases'].append('header::four_node')
        with self.assertRaisesRegex(ValueError, 'minimum case inventory'):
            artifacts.ordinary_entries(minimum)
        full = copy.deepcopy(manifest)
        full['execution_plan'] = fast_target_selection([target], full=True) | dict(
            ordinary_case_prefix_contract_version=1, formal_full_profiles_only_version=1,
            formal_suites=list(artifacts.SHARDS), formal_shards=list(artifacts.SHARDS),
            formal_deferred_ordinary_cases={}, groups=['baseline', 'oasis7_required', 'formal_process'])
        full['binaries']['generated']['variant'] = 'required'
        with patch.object(artifacts, 'snapshot_regressions', return_value=()):
            self.assertEqual(artifacts.ordinary_entries(full), [dict(binary='generated', batch=True)])

    def test_real_prefix_execution_and_full_keep_their_exclusive_case_coverage(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source, executable = root / 'fixture.rs', root / 'fixture'
            source.write_text('''
mod required {
    #[test] fn keep() {}
    #[test] fn heavy() {}
}
mod outside {
    #[test] fn unrelated() { assert!(std::env::var_os("PREFIX_EXCLUSIVE_FULL").is_some()); }
    #[test] fn heavy() { assert!(std::env::var_os("PREFIX_EXCLUSIVE_FULL").is_some()); }
}
''')
            compile_rust_fixture(source, executable, test=True)
            for full in (False, True):
                target = fast_target(package='oasis7_chain_runtime', target_kind='lib',
                    target_name='oasis7_chain_runtime', cases=['required::keep'], case_prefixes=['required::'])
                selected = fast_target_selection([target], full=full) | {
                    'ordinary_case_prefix_contract_version': 1,
                    'oasis7_required_packages': ['oasis7_chain_runtime'],
                    'oasis7_required_wasm_packages': [],
                }
                binary = fast_target_binary(target, 'required' if full else 'required-fast-0')
                binary.update(path=executable.name, cwd='.', tests=artifacts.inventory(executable), ignored_tests=[])
                manifest = dict(scope='ordinary', execution_plan=selected, binaries={'generated': binary})
                environment = dict(os.environ)
                if full:
                    environment['PREFIX_EXCLUSIVE_FULL'] = '1'
                results = root / ('full.jsonl' if full else 'fast.jsonl')
                with self.subTest(full=full), patch.object(artifacts, 'ROOT', root), \
                     patch.object(artifacts, 'load', return_value=(manifest, environment)), \
                     patch.object(artifacts, 'snapshot_regressions', return_value=()), \
                     patch.object(artifacts, 'ORDINARY_EXCLUSIVE_RUNTIME_CASES',
                                  frozenset(('required::heavy', 'outside::heavy'))):
                    entries = artifacts.expand_ordinary_batches(artifacts.ordinary_entries(manifest), manifest['binaries'])
                    self.assertEqual(artifacts.expand_ordinary_batches(entries, manifest['binaries']), entries)
                    artifacts.execute(root, 'ordinary', results, plan=selected)
                records = [json.loads(line) for line in results.read_text().splitlines()]
                self.assertTrue(all(record['passed'] for record in records))
                self.assertEqual(sum(record['expected_count'] for record in records), 4 if full else 2)
                self.assertEqual({record['test_name'] for record in records},
                    {'generated', 'required::heavy', 'outside::heavy'} if full else {'generated', 'required::heavy'})

    def test_real_libtest_execution_failure_zero_match_and_cancellation_are_not_green(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source, binary = root / 'fixture.rs', root / 'fixture'
            source.write_text('''
mod required {
    #[test] fn round_trip() { assert!(std::env::var_os("PREFIX_FAIL").is_none()); }
    #[test] fn new_regression() {}
    mod nested { #[test] fn reject() {} }
    #[test] #[ignore] fn ignored_child() { panic!("ignored child must remain ignored"); }
}
mod recovery { #[test] fn resume() {} }
mod required_neighbor { #[test] fn unrelated() { assert!(std::env::var_os("PREFIX_FULL").is_some()); } }
mod outside { #[test] fn unrelated() { assert!(std::env::var_os("PREFIX_FULL").is_some()); } }
''')
            compile_rust_fixture(source, binary, test=True)
            manifest = self.manifest()
            entry = manifest['binaries']['fixture']
            entry.update(path='fixture', cwd='.', tests=artifacts.inventory(binary),
                         ignored_tests=artifacts.inventory(binary, True))
            selected = manifest['execution_plan']
            artifacts.validate_ordinary_selection(manifest)
            for mode in ('success', 'failure', 'cancelled', 'full', 'zero-match'):
                results = root / (mode + '.jsonl')
                environment = dict(os.environ)
                current = copy.deepcopy(manifest)
                if mode == 'failure':
                    environment['PREFIX_FAIL'] = '1'
                if mode == 'full':
                    environment['PREFIX_FULL'] = '1'
                    current['execution_plan'] = self.selection(full=True)
                    current['binaries']['fixture']['variant'] = 'required'
                if mode == 'zero-match':
                    source.write_text('')
                    compile_rust_fixture(source, binary, test=True)
                cancellation = threading.Event()
                if mode == 'cancelled':
                    cancellation.set()
                def observed_run(*args, **kwargs):
                    return run(*args, **kwargs, cancellation_event=cancellation)
                with self.subTest(mode=mode), patch.object(artifacts, 'ROOT', root), \
                     patch.object(artifacts, 'load', return_value=(current, environment)), \
                     patch.object(artifacts, 'snapshot_regressions', return_value=()), \
                     patch('ci_process_runner.run', side_effect=observed_run):
                    if mode in ('success', 'full'):
                        artifacts.execute(root, 'ordinary', results, plan=current['execution_plan'])
                    elif mode == 'cancelled':
                        with self.assertRaisesRegex(InterruptedError, 'cancelled before starting child'):
                            artifacts.execute(root, 'ordinary', results, plan=selected)
                    else:
                        with self.assertRaisesRegex(RuntimeError, 'ordinary failed'):
                            artifacts.execute(root, 'ordinary', results, plan=selected)
                if mode == 'cancelled':
                    self.assertFalse(results.exists(), 'cancellation must not produce a success receipt')
                    continue
                record = json.loads(results.read_text().splitlines()[0])
                self.assertEqual(record['expected_count'], 6 if mode == 'full' else 4)
                self.assertEqual(record['passed'], mode in ('success', 'full'))


class SharedIncrementalHarnesses(unittest.TestCase):
    def selection(self):
        return fast_execution_selection(('oasis7', 'oasis7_runtime')) | {
            'ordinary_shared_incremental_contract_version': 1,
        }

    def manifest(self):
        checkout = artifacts.ROOT
        profile = dict(test=True, opt_level='0', debuginfo=2, debug_assertions=True, overflow_checks=True)
        binaries, units = {}, []
        cases = {target: ('oasis7', [case]) for target, (_, case) in artifacts.SHARED_INCREMENTAL_CASES.items()}
        cases['pr_fast_incremental'] = ('oasis7_runtime', [module + '::' + case
                                                        for module, case in artifacts.SHARED_INCREMENTAL_CASES.values()])
        for target, (package, names) in cases.items():
            executable = str(checkout / 'target/debug/deps' / (target + '-fixture'))
            features = ['default', 'test_tier_required'] + (['test-support'] if package == 'oasis7_runtime' else [])
            binaries[target] = dict(ordinary=True, test=True, variant='required', package=package,
                target=target, target_kind=['test'], tests=names, ignored_tests=[], default_features=True,
                sha256='a' * 64, features=features, requested_features=['test_tier_required'],
                profile=profile, original_executable=executable)
            units.append(dict(package_id=package, manifest_path=str(checkout / 'crates' / package / 'Cargo.toml'),
                target=dict(name=target, kind=['test'], src_path=str(checkout / 'crates' / package / 'tests' / (target + '.rs'))),
                profile=profile, features=features, filenames=[executable], executable=executable))
        for package in sorted(artifacts.SHARED_INCREMENTAL_LIBRARIES):
            features = ['default', 'test_tier_required'] + (['test-support'] if package == 'oasis7_runtime' else [])
            units.append(dict(package_id=package, manifest_path=str(checkout / 'crates' / package / 'Cargo.toml'),
                target=dict(name=package, kind=['lib'], src_path=str(checkout / 'crates' / package / 'src/lib.rs')),
                profile=profile | {'test': False}, features=features,
                filenames=[str(checkout / 'target/debug/deps' / ('lib' + package + '-fixture.rlib'))], executable=None))
        return dict(scope='ordinary', checkout=str(checkout), execution_plan=self.selection(), binaries=binaries,
                    ordinary_shared_incremental=dict(version=1, units=units, binaries={name: entry['sha256']
                                                                                     for name, entry in binaries.items()}))

    def test_source_changes_disable_optimization_without_requiring_new_pins(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source = root / 'reviewed.rs'
            source.write_text('pub struct AgentState;\n')
            reviewed = {'reviewed.rs': artifacts.digest(source)}
            with patch.object(artifacts, 'ROOT', root), patch.object(artifacts, 'SHARED_INCREMENTAL_SOURCES', reviewed):
                self.assertTrue(artifacts.shared_incremental_sources_match())
                source.write_text('pub struct ChangedState;\n')
                self.assertFalse(artifacts.shared_incremental_sources_match())
                source.unlink()
                self.assertFalse(artifacts.shared_incremental_sources_match())

    def test_actual_unit_proof_keeps_all_original_cases(self):
        manifest = self.manifest()
        self.assertNotEqual(manifest['binaries']['pr_fast_incremental']['features'],
                            manifest['binaries']['incremental_runtime_commit']['features'])
        with patch.object(artifacts, 'shared_incremental_sources_match', return_value=True), \
             patch.object(artifacts, 'snapshot_regressions', return_value=()):
            self.assertEqual(artifacts.shared_incremental_duplicate(manifest), 'pr_fast_incremental')
            self.assertEqual({item['binary'] for item in artifacts.ordinary_entries(manifest)},
                             set(artifacts.SHARED_INCREMENTAL_CASES))

    def test_missing_or_changed_proof_never_removes_an_execution(self):
        for mode in ('no-proof', 'no-capability', 'full', 'periodic', 'candidate', 'policy-change',
                     'wrong-variant', 'changed-features', 'changed-profile', 'no-default-features',
                     'changed-request', 'different-native-target', 'new-case', 'ignored-case',
                     'missing-original', 'missing-unit', 'duplicate-unit', 'different-library-target',
                     'different-library-features', 'different-library-package', 'changed-binary'):
            manifest = self.manifest()
            reference = manifest['binaries']['pr_fast_incremental']
            proof = manifest['ordinary_shared_incremental']
            if mode == 'no-proof':
                del manifest['ordinary_shared_incremental']
            elif mode == 'no-capability':
                del manifest['execution_plan']['ordinary_shared_incremental_contract_version']
            elif mode in ('full', 'periodic', 'candidate'):
                manifest['execution_plan'].update(scope='explicit-full',
                    execution_profile='candidate-full' if mode == 'candidate' else 'periodic-full')
                if mode == 'full':
                    manifest['scope'] = 'all'
            elif mode == 'policy-change':
                manifest['execution_plan']['test_policy_changed'] = True
            elif mode == 'wrong-variant':
                reference['variant'] = 'required-fast-0'
            elif mode == 'changed-features':
                reference['features'] = ['test_tier_full']
            elif mode == 'changed-profile':
                reference['profile'] = reference['profile'] | {'opt_level': '3'}
            elif mode == 'no-default-features':
                reference['default_features'] = False
            elif mode == 'changed-request':
                reference['requested_features'] = ['test_tier_full']
            elif mode == 'different-native-target':
                reference['original_executable'] = '/different-target/debug/deps/combined'
            elif mode == 'new-case':
                reference['tests'].append('new_regression')
            elif mode == 'ignored-case':
                reference['ignored_tests'] = reference['tests'][:1]
            elif mode == 'missing-original':
                del manifest['binaries']['incremental_runtime_commit']
            elif mode == 'missing-unit':
                proof['units'].pop()
            elif mode == 'duplicate-unit':
                proof['units'].append(copy.deepcopy(proof['units'][-1]))
            elif mode == 'different-library-target':
                proof['units'][-1]['filenames'] = ['/different-target/libunit.rlib']
            elif mode == 'different-library-features':
                next(unit for unit in proof['units'] if unit['target']['name'] == 'oasis7_runtime')['features'] = ['different']
            elif mode == 'different-library-package':
                next(unit for unit in proof['units'] if unit['target']['name'] == 'oasis7_runtime')['manifest_path'] = '/other/Cargo.toml'
            else:
                reference['sha256'] = 'b' * 64
            with self.subTest(mode=mode), patch.object(artifacts, 'shared_incremental_sources_match', return_value=True), \
                 patch.object(artifacts, 'snapshot_regressions', return_value=()):
                self.assertIsNone(artifacts.shared_incremental_duplicate(manifest))
                self.assertEqual(len(artifacts.ordinary_entries(manifest)), len(manifest['binaries']))
        manifest = self.manifest()
        with patch.object(artifacts, 'shared_incremental_sources_match', return_value=False), \
             patch.object(artifacts, 'snapshot_regressions', return_value=()):
            self.assertEqual(len(artifacts.ordinary_entries(manifest)), 4)
        for value in (True, 0, 2, '1'):
            with self.subTest(capability=value), self.assertRaises(ValueError):
                artifacts.fast_plan(self.selection() | {'ordinary_shared_incremental_contract_version': value})

    def test_actual_cargo_feature_unification_and_execution_preserve_full_and_failures(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            (root / 'Cargo.toml').write_text('[workspace]\nmembers=["crates/*"]\nresolver="2"\n')
            for package in sorted(artifacts.SHARED_INCREMENTAL_LIBRARIES):
                directory = root / 'crates' / package
                (directory / 'src').mkdir(parents=True)
                text = '[package]\nname="' + package + '"\nversion="0.1.0"\nedition="2021"\n'
                text += '[features]\ndefault=[]\ntest_tier_required=[]\ntest-support=[]\n'
                source = 'pub fn value() {}\n'
                if package == 'oasis7_world_state':
                    source = 'pub struct AgentState;\n'
                if package in ('oasis7', 'oasis7_runtime'):
                    dependencies = artifacts.SHARED_INCREMENTAL_LIBRARIES - {'oasis7', 'oasis7_runtime'}
                    text += '[dependencies]\n' + ''.join(name + '={path="../' + name + '"}\n'
                                                          for name in sorted(dependencies))
                    if package == 'oasis7':
                        text += 'oasis7_runtime={path="../oasis7_runtime", features=["test-support"]}\n'
                        source = 'pub use oasis7_runtime::AgentState;\n'
                    else:
                        source = ('pub use oasis7_world_state::AgentState;\n'
                                  'pub fn shared_feature() -> bool { cfg!(feature="test-support") }\n')
                (directory / 'Cargo.toml').write_text(text)
                (directory / 'src/lib.rs').write_text(source)
            originals = root / 'crates/oasis7/tests'
            originals.mkdir()
            for target, (_, case) in artifacts.SHARED_INCREMENTAL_CASES.items():
                (originals / (target + '.rs')).write_text('#[test] fn ' + case + '() {\n'
                    'let _: oasis7_runtime::AgentState = oasis7::AgentState;\n'
                    'assert!(oasis7_runtime::shared_feature());\n'
                    'assert!(std::env::var_os("SHARED_INCREMENTAL_FAIL").is_none());\n}\n')
            combined = root / 'crates/oasis7_runtime/tests/pr_fast_incremental.rs'
            combined.parent.mkdir()
            combined.write_text('extern crate oasis7_world_state as oasis7;\n' + ''.join(
                '#[path="../../oasis7/tests/' + target + '.rs"] mod ' + module + ';\n'
                for target, (module, _) in artifacts.SHARED_INCREMENTAL_CASES.items()))
            config = root / 'suites.json'
            config.write_text('{}')
            subprocess.run(['cargo', 'generate-lockfile', '--offline'], cwd=root, check=True,
                           stdout=subprocess.PIPE, stderr=subprocess.PIPE)
            for command in (['git', 'init', '-q'], ['git', 'add', '.'],
                            ['git', '-c', 'user.name=Fixture', '-c', 'user.email=fixture@example.invalid',
                             'commit', '-qm', 'shared source fixture']):
                subprocess.run(command, cwd=root, check=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
            sha = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, text=True).strip()
            selected = self.selection() | {'architecture': dict(source=sha, tested=sha)}
            # This reviewed toy contract uses a real type re-export and real
            # differently-featured Cargo roots. The production pins remain fixed.
            sources = {str(path.relative_to(root)): artifacts.digest(path)
                       for path in (root / 'crates').rglob('*') if path.is_file()}
            variants = [('required', ['test', '-p', 'oasis7', '-p', 'oasis7_runtime', '--tests',
                         '--features', 'test_tier_required', '--no-run'], ['test_tier_required'])]
            with patch.object(artifacts, 'ROOT', root), patch.object(artifacts, 'CONFIG', config), \
                 patch.object(artifacts, 'SHARED_INCREMENTAL_SOURCES', sources), \
                 patch.object(artifacts, 'build_variants', return_value=variants), \
                 patch.object(artifacts, 'snapshot_regressions', return_value=()), \
                 patch.dict(os.environ, SOURCE_SHA=sha):
                bundle = root / 'bundle'
                artifacts.build(bundle, 'ordinary', plan=selected)
                manifest = json.loads((bundle / 'manifest.json').read_text())
                self.assertIn('ordinary_shared_incremental', manifest)
                duplicate = artifacts.shared_incremental_duplicate(manifest)
                self.assertIsNotNone(duplicate)
                for mode, counts in (('fast', [1, 1, 1]), ('full', [1, 1, 1, 3]), ('failure', [1, 1, 1])):
                    current = copy.deepcopy(manifest)
                    environment = dict(os.environ)
                    if mode == 'full':
                        current['execution_plan'].update(scope='explicit-full', execution_profile='periodic-full')
                    if mode == 'failure':
                        environment['SHARED_INCREMENTAL_FAIL'] = '1'
                    results = root / (mode + '.jsonl')
                    with self.subTest(mode=mode), patch.object(artifacts, 'load', return_value=(current, environment)):
                        if mode == 'failure':
                            with self.assertRaisesRegex(RuntimeError, 'ordinary failed'):
                                artifacts.execute(bundle, 'ordinary', results, plan=current['execution_plan'])
                        else:
                            artifacts.execute(bundle, 'ordinary', results, plan=current['execution_plan'])
                    records = [json.loads(line) for line in results.read_text().splitlines()]
                    self.assertEqual(sorted(record['expected_count'] for record in records), counts)
                    self.assertEqual(all(record['passed'] for record in records), mode != 'failure')
                combined.write_text(combined.read_text() + '\n// source drift requires review\n')
                self.assertIsNone(artifacts.shared_incremental_duplicate(manifest))
                self.assertEqual(len(artifacts.ordinary_entries(manifest)), 4)


class SupportHandoffSelection(unittest.TestCase):
    def selection(self):
        owner = 'oasis7_game_client'
        return inline_execution_selection() | {
            'groups': ['baseline', 'rust_baseline', 'oasis7_required'],
            'ordinary_clippy_group': 'rust_baseline',
            'ordinary_support_contract_version': 1,
            'ordinary_support_handoffs': [owner],
            'ordinary_support_deferred': ['oasis7_agent_harness', 'oasis7_model_provider'],
            'oasis7_required_packages': [owner],
            'workspace_support_profiles': [],
            'ordinary_fast_targets': [fast_target(
                package=owner, target_kind='lib', target_name=owner, default_features=True, features=[]),
                fast_target(package=owner, target_name='dev_test_service_cli',
                            default_features=True, features=[])]}

    def test_every_original_support_clippy_survives_without_duplicate_owner_lint(self):
        selected = self.selection()
        self.assertEqual(artifacts.required_native_commands(selected), [
            ['-p', 'oasis7_agent_harness', '--all-targets'],
            ['-p', 'oasis7_game_client', '--all-targets'],
            ['-p', 'oasis7_model_provider', '--all-targets']])
        # Tests retain precise targets; all-targets is a lint obligation only.
        variants = artifacts.build_variants('ordinary', selected)
        self.assertEqual(len(variants), 1)
        self.assertIn('--lib', variants[0][1])
        self.assertIn('dev_test_service_cli', variants[0][1])
        self.assertNotIn('--all-targets', variants[0][1])

    def test_handoff_requires_complete_same_default_library_and_unique_baseline_owner(self):
        for field, value in (
            ('ordinary_support_contract_version', True),
            ('ordinary_support_contract_version', 2),
            ('ordinary_support_handoffs', []),
            ('ordinary_support_handoffs', ['oasis7_model_provider']),
            ('ordinary_support_deferred', ['oasis7_game_client']),
            ('ordinary_clippy_group', 'oasis7_required'),
            ('workspace_support_profiles', ['oasis7_model_provider']),
            ('ordinary_execution', 'artifact'),
        ):
            with self.subTest(field=field, value=value), self.assertRaises(ValueError):
                artifacts.required_native_commands(self.selection() | {field: value})
        for changes in ({'default_features': False}, {'features': ['new-feature']},
                        {'target_kind': 'test'}):
            selected = self.selection()
            selected['ordinary_fast_targets'][0].update(changes)
            with self.subTest(changes=changes), self.assertRaises(ValueError):
                artifacts.required_native_commands(selected)
        for field in ('ordinary_support_contract_version', 'ordinary_support_deferred'):
            selected = self.selection()
            del selected[field]
            with self.subTest(missing=field), self.assertRaises(ValueError):
                artifacts.required_native_commands(selected)


if __name__ == '__main__':
    unittest.main()
