#!/usr/bin/env python3
"""Coverage, transition-gate and real libtest runner regressions."""
import copy
from contextlib import ExitStack, redirect_stdout
from concurrent.futures import ThreadPoolExecutor
import hashlib
import importlib.util
import itertools
import io
import json
import os
from pathlib import Path
import re
import shutil
import signal
import subprocess
import sys
import tempfile
import tarfile
import threading
import time
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
plan = module('ci-formal-plan')
from ci_process_runner import run, owned_processes, _Capture, READ_BYTES, SUMMARY_LINE_BYTES


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


class NativeRustFixtureLinker(unittest.TestCase):
    def test_native_linker_preserves_fixture_environment_and_verifies_elf(self):
        environment = dict(CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER='/local/oasis7-mold-gcc',
                           CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER='/wrong-host',
                           CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_LINKER='/wasm-only',
                           CARGO_BIN_EXE_tool='/prepared/tool')
        with patch.object(sys, 'platform', 'linux'), \
             patch.object(subprocess, 'check_output', side_effect=[
                 'host: aarch64-unknown-linux-gnu\n', '  [  0]  mold 3.0.0\n']) as output, \
             patch.object(subprocess, 'run') as compile:
            compile_rust_fixture('fixture.rs', 'fixture', test=True, env=environment)
        self.assertEqual(compile.call_args.args[0], ['rustc', '--test', 'fixture.rs', '-o',
                         'fixture', '-C', 'linker=/local/oasis7-mold-gcc'])
        self.assertIs(compile.call_args.kwargs['env'], environment)
        self.assertEqual(output.call_args.args[0], ['readelf', '-p', '.comment', 'fixture'])

    def test_mold_configuration_rejects_an_elf_from_another_linker(self):
        with patch.object(sys, 'platform', 'linux'), \
             patch.object(subprocess, 'check_output', side_effect=[
                 'host: x86_64-unknown-linux-gnu\n', 'GCC: GNU ld\n']), \
             patch.object(subprocess, 'run'), \
             self.assertRaisesRegex(AssertionError, 'not linked by the pinned mold'):
            compile_rust_fixture('fixture.rs', 'fixture',
                env={'CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER': '/local/oasis7-mold-gcc'})

    def test_unconfigured_native_and_other_platforms_keep_the_existing_linker(self):
        for platform, environment in [
            ('linux', {}),
            ('linux', {'CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_LINKER': '/wasm-only'}),
            ('darwin', {'CARGO_TARGET_AARCH64_APPLE_DARWIN_LINKER': '/unrelated'}),
            ('win32', {'CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER': '/unrelated'}),
        ]:
            with self.subTest(platform=platform, environment=environment), \
                 patch.object(sys, 'platform', platform), \
                 patch.object(subprocess, 'check_output', return_value='host: x86_64-unknown-linux-gnu\n'), \
                 patch.object(subprocess, 'run') as compile:
                compile_rust_fixture('fixture.rs', 'fixture', env=environment)
                self.assertEqual(compile.call_args.args[0], ['rustc', 'fixture.rs', '-o', 'fixture'])



class ProviderArtifactContracts(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.bundle = self.root / 'bundle'
        (self.bundle / artifacts.ProviderArtifact.DIRECTORY).mkdir(parents=True)
        self.expected = dict(tested_sha='a' * 40, source_sha='b' * 40)
        files = {}
        for kind, name in [('wasm',artifacts.ProviderArtifact.MODULE+'.wasm'), ('metadata',artifacts.ProviderArtifact.MODULE+'.metadata.json'),
                           ('receipt',artifacts.ProviderArtifact.MODULE+'.build-receipt.json'), ('checker','sync_builtin_wasm_identity'+('.exe' if os.name=='nt' else ''))]:
            name = artifacts.ProviderArtifact.DIRECTORY + '/' + name
            path = self.bundle / name
            path.write_bytes(b'synthetic contract data; not a valid canonical artifact')
            path.chmod(0o700 if kind == 'checker' else 0o600)
            files[kind] = dict(path=name, sha256=artifacts.ProviderArtifact.digest(path))
        self.record = dict(self.expected, checkout_tree_sha256='tree',files=files,libraries=[])
        self.tree = patch.object(artifacts.ProviderArtifact,'tree_digest',return_value='tree')
        self.tree.start()
        self.addCleanup(self.tree.stop)
        # An explicit mock of the official verifier, never synthetic provenance
        # accepted in production. This contract checks invocation and fail-closed
        # orchestration; real source/receipt validity is a separate Rust/build test.
        self.invoke = patch.object(artifacts.ProviderArtifact.subprocess,'run')
        self.native = self.invoke.start()
        self.addCleanup(self.invoke.stop)

    def validate(self, record=None):
        return artifacts.ProviderArtifact.validate(self.root,self.bundle,self.record if record is None else record,self.expected)

    def test_expanded_native_provider_material_follows_actual_full_consumer(self):
        manifest = dict(selection={"suite": "ordinary"},
                        execution_plan=dict(test_policy_changed=True),
                        binaries={"runtime-tests": dict(package="oasis7_chain_runtime", test=True, features=["test_tier_full"])})
        self.assertTrue(artifacts.preview_provider_required(manifest))
        for key, value in [("package", "oasis7_viewer_codec"), ("test", False), ("features", ["test_tier_required"])]:
            changed = copy.deepcopy(manifest)
            changed["binaries"]["runtime-tests"][key] = value
            self.assertFalse(artifacts.preview_provider_required(changed))
        manifest["execution_plan"] = dict(scope="targeted", execution_profile="pr-fast")
        self.assertFalse(artifacts.preview_provider_required(manifest))

    def test_merge_dual_identity_and_exact_official_checker(self):
        environment = self.validate()
        self.assertEqual(set(environment),set(artifacts.ProviderArtifact.VARIABLES))
        command = self.native.call_args.args[0]
        self.assertEqual(command[1:3], ['--verify-provider-artifact',str(self.root)])
        self.assertEqual(len(command),6)
        self.assertNotIn('cargo', command)
        self.assertNotEqual(self.record['tested_sha'],self.record['source_sha'])

    def test_missing_pair_hash_and_source_fail_closed_before_checker(self):
        mutations = [lambda r:r['files'].pop('wasm'),
                     lambda r:r.update(source_sha='c'*40),
                     lambda r:r.update(tested_sha='c'*40),
                     lambda r:r.update(checkout_tree_sha256='other-tree'),
                     lambda r:r['files']['metadata'].update(sha256='0'*64)]
        for mutate in mutations:
            record=copy.deepcopy(self.record)
            mutate(record)
            with self.subTest(record=record), self.assertRaises(ValueError): self.validate(record)
        self.native.assert_not_called()

    def test_bad_path_and_symlink_fail_closed(self):
        record=copy.deepcopy(self.record)
        record['files']['wasm']['path']='../foreign.wasm'
        with self.assertRaises(ValueError): self.validate(record)
        wasm=self.bundle/self.record['files']['wasm']['path']
        foreign=self.root/'foreign'
        foreign.write_bytes(wasm.read_bytes())
        wasm.unlink()
        wasm.symlink_to(foreign)
        with self.assertRaises(ValueError): self.validate()
        self.native.assert_not_called()

    def test_official_checker_failure_cannot_inject_environment(self):
        self.native.side_effect=subprocess.CalledProcessError(2,['official-checker'])
        with self.assertRaises(subprocess.CalledProcessError): self.validate()


class Transition(unittest.TestCase):
    def test_full_only_fast_plans_skip_heavy_jobs_but_full_still_requires_them(self):
        groups = ['baseline', 'formal_process']
        for profile in ('pr-fast', 'main-fast'):
            for scope in ('targeted', 'full'):
                with self.subTest(profile=profile, scope=scope):
                    selection = dict(fast_plan_version=1, execution_profile=profile, scope=scope,
                                     groups=['baseline'], formal_full_profiles_only_version=1,
                                     formal_suites=[], formal_shards=[])
                    self.assertEqual(artifacts.formal_suites(selection), [])
                    self.assertFalse(plan.selected(selection, groups))
                    needs = {name: dict(result='skipped') for name in
                             ('formal-test-build', 'formal-test-build-trusted', 'formal-process-tests')}
                    needs['select'] = dict(result='success')
                    self.assertEqual(plan.aggregate(False, needs), 'not_selected')
        for profile in ('periodic-full', 'candidate-full'):
            selection = dict(fast_plan_version=1, execution_profile=profile, scope='explicit-full',
                             groups=groups, formal_full_profiles_only_version=1,
                             formal_suites=sorted(artifacts.SHARDS), formal_shards=sorted(artifacts.SHARDS))
            self.assertTrue(plan.selected(selection, groups))
            self.assertEqual(set(artifacts.formal_suites(selection)), set(artifacts.SHARDS))
            for field in ('formal_suites', 'formal_shards'):
                with self.subTest(profile=profile, missing=field), self.assertRaises(ValueError):
                    artifacts.formal_suites(selection | {field: []})

    def test_full_only_formal_plan_rejects_malformed_capability_and_legacy_full_skip(self):
        selection = dict(fast_plan_version=1, execution_profile='pr-fast', scope='full',
                         groups=['baseline'], formal_full_profiles_only_version=1,
                         formal_suites=[], formal_shards=[])
        mutations = [('formal_full_profiles_only_version', value) for value in (True, 0, 2, '1', None)]
        mutations += [('groups', ['baseline', 'formal_process']),
                      ('formal_suites', ['world-history']), ('formal_shards', ['world-history']),
                      ('execution_profile', 'periodic-full'), ('scope', 'explicit-full')]
        for field, value in mutations:
            with self.subTest(field=field, value=value), self.assertRaises(ValueError):
                artifacts.formal_suites(selection | {field: value})
        for field in ('fast_plan_version', 'formal_suites', 'formal_shards'):
            broken = dict(selection)
            del broken[field]
            with self.subTest(missing=field), self.assertRaises(ValueError):
                artifacts.formal_suites(broken)
        del selection['formal_full_profiles_only_version']
        with self.assertRaisesRegex(ValueError, 'complete profile is missing formal suites'):
            artifacts.formal_suites(selection)

    def test_old_baseline_can_only_widen(self):
        self.assertTrue(plan.selected(dict(scope='full', groups=['baseline']), ['baseline']))
        for group in plan.CORE:
            self.assertTrue(plan.selected(dict(scope='targeted', groups=['baseline', group]), ['baseline', group]))
        for group in ['viewer_js_required', 'site_quality']:
            self.assertFalse(plan.selected(dict(scope='targeted', groups=['baseline', group]), ['baseline', group]))
        self.assertFalse(plan.selected(dict(scope='full', groups=['baseline']), ['baseline', 'formal_process']))
        self.assertTrue(plan.selected(dict(scope='targeted', groups=['formal_process']), ['formal_process']))

    def test_ordinary_formal_deferral_requires_exact_trusted_plan_and_target(self):
        selection = FastExecutionSelection().selection(('oasis7_chain_runtime',)) | dict(
            formal_full_profiles_only_version=1, formal_shards=[],
            formal_deferred_ordinary_cases={'runtime-tests': ['formal::heavy']})
        for deferred in ([], {'other': ['formal::heavy']}, {'runtime-tests': []},
                         {'runtime-tests': ['formal::heavy', 'formal::heavy']},
                         {'runtime-tests': ['formal::*']}, {'runtime-tests': [None]}):
            with self.subTest(deferred=deferred), self.assertRaises(ValueError):
                artifacts.fast_plan(selection | {'formal_deferred_ordinary_cases': deferred})
        old = dict(selection)
        del old['formal_full_profiles_only_version']
        with self.assertRaisesRegex(ValueError, 'compatible capability'):
            artifacts.fast_plan(old)
        binary = dict(package='oasis7_chain_runtime', target='oasis7_chain_runtime', target_kind=['lib'],
                      ordinary=True, tests=['formal::heavy', 'formal::heavy_neighbor'], ignored_tests=[])
        manifest = dict(execution_plan=selection, binaries={'runtime-tests': binary})
        self.assertEqual(artifacts.ordinary_entries(manifest),
                         [dict(binary='runtime-tests', batch=True, skip=['formal::heavy'])])
        for field, value in [('package', 'unrelated'), ('target', 'unrelated'), ('target_kind', ['bin'])]:
            changed = copy.deepcopy(manifest)
            changed['binaries']['runtime-tests'][field] = value
            with self.subTest(field=field), self.assertRaisesRegex(ValueError, 'target identity'):
                artifacts.ordinary_entries(changed)

    def test_selected_requires_real_success_and_not_selected_requires_skip(self):
        needs = {name: dict(result='success') for name in ['select', 'formal-test-build', 'formal-process-tests']}
        needs['formal-test-build-trusted'] = dict(result='skipped')
        self.assertEqual(plan.aggregate(True, needs), 'passed')
        for job in ['select', 'formal-test-build', 'formal-process-tests']:
            for state in ['failure', 'cancelled', 'skipped', None]:
                changed = copy.deepcopy(needs)
                changed[job]['result'] = state
                with self.assertRaises(ValueError):
                    plan.aggregate(True, changed)
        for name in ('formal-test-build', 'formal-test-build-trusted'):
            needs[name]['result'] = 'skipped'
        needs['formal-process-tests']['result'] = 'skipped'
        self.assertEqual(plan.aggregate(False, needs), 'not_selected')
        del needs['formal-process-tests']
        with self.assertRaises(ValueError):
            plan.aggregate(False, needs)

    def test_formal_builder_failure_and_skip_combinations(self):
        states = ('success', 'skipped', 'failure', 'cancelled', None)
        for reader in states:
            for writer in states:
                needs = {'select': {'result': 'success'}, 'formal-process-tests': {'result': 'success'},
                         'formal-test-build': {'result': reader}, 'formal-test-build-trusted': {'result': writer}}
                if (reader, writer) in (('success', 'skipped'), ('skipped', 'success')):
                    self.assertEqual(plan.aggregate(True, needs), 'passed')
                else:
                    with self.assertRaises(ValueError):
                        plan.aggregate(True, needs)


class Coverage(unittest.TestCase):
    def setUp(self):
        self.config = json.loads((HERE / 'ci-formal-process-suites.json').read_text())
        self.binaries = {name: dict(tests=['formal::logic'], ignored_tests=[]) for name in self.config['environment_formal_batches']}
        for entries in self.config['suites'].values():
            for entry in entries:
                b = self.binaries[entry['binary']]
                b['tests'].append(entry['name'])
                b['ignored_tests'].append(entry['name'])
        for binary, names in self.config['auxiliary_entries'].items():
            self.binaries[binary]['tests'].extend(names)
        for binary, overrides in self.config.get('case_shards', {}).items():
            self.binaries[binary]['tests'].extend(name for name in overrides
                                                if name not in self.binaries[binary]['tests'])

    def test_inventory_and_auxiliaries_are_disjoint(self):
        suites = artifacts.resolve_suites(self.config, self.binaries)
        cases = [(e['binary'], e['name']) for entries in suites.values() for e in entries if 'binary' in e]
        self.assertEqual(len(cases), len(set(cases)))
        self.assertEqual(sum(e.get('ignored', False) for entries in suites.values() for e in entries), 29)
        self.assertEqual(sum('script' in e for entries in suites.values() for e in entries), 2)

    def test_rename_duplicate_and_new_unassigned_acceptance_fail(self):
        for mutation in ['rename', 'duplicate', 'missing']:
            config, binaries = copy.deepcopy(self.config), copy.deepcopy(self.binaries)
            entry = config['suites']['world-history'][0]
            if mutation == 'rename':
                entry['name'] += '_renamed'
            elif mutation == 'duplicate':
                config['suites']['runtime-protocol'].append(entry)
            else:
                binaries['runtime-tests']['ignored_tests'].append('new::protected_endpoint_children_new_acceptance')
            with self.assertRaises(ValueError):
                artifacts.resolve_suites(config, binaries)

    def test_initial_lost_reply_runs_unconditionally_under_trusted_parent(self):
        name = ('formal_live_runtime::tests::protected_process_tests::'
                'protected_endpoint_children_submit_qualify_restart_and_read_from_external_pin')
        suites = artifacts.resolve_suites(self.config, self.binaries)
        shards = artifacts.resolve_shards(self.config, suites)
        selected = [(shard, entry) for shard, entries in shards.items()
                    for entry in entries if entry.get('name') == name]
        self.assertEqual(len(selected), 1)
        shard, entry = selected[0]
        self.assertEqual(shard, 'runtime-protocol')
        self.assertEqual(entry['binary'], 'runtime-tests')
        self.assertTrue(entry['ignored'])
        self.assertIn('endpoint', self.config['binary_dependencies'][entry['binary']])
        source = (HERE.parent / 'crates/oasis7_chain_runtime/src/host/formal_live_protected_process_tests.rs').read_text()
        parent = re.search(r'fn protected_endpoint_children_submit_qualify_restart_and_read_from_external_pin\(\) \{(.*?)\n\}', source, re.S).group(1)
        self.assertRegex(parent, r'run_on_fixture_stack\(false\)\.unwrap\(\);\s*//[^\n]*\n\s*protected_initial_closure_lost_reply_recovers_original_with_new_coordinator_session\(\);')
        helper = 'protected_initial_closure_lost_reply_recovers_original_with_new_coordinator_session'
        self.assertNotIn(helper, [e.get('name', '').split('::')[-1] for values in suites.values() for e in values])
        self.assertIn('FixtureCase::InitialLostResponse', source)

    def test_physical_shards_preserve_every_logical_case_once(self):
        suites = artifacts.resolve_suites(self.config, self.binaries)
        shards = artifacts.resolve_shards(self.config, suites)
        expected = [(suite, json.dumps(entry, sort_keys=True))
                    for suite, entries in suites.items() for entry in entries]
        actual = [(entry['suite'], json.dumps({k: v for k, v in entry.items() if k != 'suite'}, sort_keys=True))
                  for entries in shards.values() for entry in entries]
        self.assertCountEqual(actual, expected)
        cold = next(entry for entry in shards['storage-recovery'] if 'cold_original_lookup' in entry.get('name', ''))
        operation = next(entry for entry in shards['lifecycle-cli'] if 'operation_cli_owner' in entry.get('name', ''))
        self.assertEqual(cold['suite'], 'healthy-set')
        self.assertEqual(operation['suite'], 'healthy-set')
        self.assertEqual(len(shards['world-history']), 1)
        changed = copy.deepcopy(self.config)
        changed['case_shards']['runtime-tests']['renamed_or_missing_case'] = 'healthy-set'
        with self.assertRaises(ValueError):
            artifacts.resolve_shards(changed, suites)

    def test_deferred_ordinary_cases_remain_exactly_once_in_complete_formal_inventory(self):
        scope = json.loads((HERE / 'ci-required-scope.json').read_text())
        deferred = scope['formal_deferred_ordinary_cases']
        self.assertEqual(scope['formal_full_profiles_only_version'], 1)
        self.assertEqual(set(deferred), {'runtime-tests'})
        self.assertEqual(len(deferred['runtime-tests']), 2)
        suites = artifacts.resolve_suites(self.config, self.binaries)
        shards = artifacts.resolve_shards(self.config, suites)
        for binary, names in deferred.items():
            for name in names:
                with self.subTest(binary=binary, name=name):
                    self.assertIn(name, self.binaries[binary]['tests'])
                    self.assertNotIn(name, self.binaries[binary]['ignored_tests'])
                    matches = [entry for entries in shards.values() for entry in entries
                               if entry.get('binary') == binary and entry.get('name') == name]
                    self.assertEqual(len(matches), 1)
                    self.assertFalse(matches[0]['ignored'])

    def test_trusted_formal_inventory_survives_candidate_removal_and_keeps_additions(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            authority, candidate = root / 'authority.json', root / 'candidate.json'
            authority.write_text(json.dumps(self.config))
            changed = copy.deepcopy(self.config)
            old = changed['suites']['world-history'].pop()
            for key in ('environment_formal_batches', 'shell_tests', 'binary_dependencies',
                        'script_dependencies', 'legacy_ignored_filters', 'auxiliary_entries'):
                changed[key] = {}
            candidate.write_text(json.dumps(changed))
            with patch.object(artifacts, 'CONFIG', candidate), patch.object(artifacts, 'FORMAL_AUTHORITY', authority):
                effective = artifacts.formal_configuration()
                self.assertEqual(effective, self.config)
                artifacts.resolve_suites(effective, self.binaries)
                missing = copy.deepcopy(self.binaries)
                missing[old['binary']]['ignored_tests'].remove(old['name'])
                with self.assertRaisesRegex(ValueError, 'missing ignored acceptance'):
                    artifacts.resolve_suites(effective, missing)
                added = dict(binary='runtime-tests', name='protected_endpoint_children_new_acceptance', ignored=True)
                changed['suites']['runtime-protocol'].append(added)
                candidate.write_text(json.dumps(changed))
                binaries = copy.deepcopy(self.binaries)
                binaries['runtime-tests']['tests'].append(added['name'])
                binaries['runtime-tests']['ignored_tests'].append(added['name'])
                suites = artifacts.resolve_suites(artifacts.formal_configuration(), binaries)
                self.assertIn(old, suites['world-history'])
                self.assertIn(added, suites['runtime-protocol'])
                self.assertEqual(sum(entry.get('ignored', False) for cases in suites.values() for entry in cases), 30)

    def test_candidate_cannot_relabel_formal_case_or_move_it_to_an_unplanned_shard(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            authority, candidate = root / 'authority.json', root / 'candidate.json'
            authority.write_text(json.dumps(self.config))
            changed = copy.deepcopy(self.config)
            entry = changed['suites']['world-history'].pop()
            changed['suites']['runtime-protocol'].append(entry)
            candidate.write_text(json.dumps(changed))
            with patch.object(artifacts, 'CONFIG', candidate), patch.object(artifacts, 'FORMAL_AUTHORITY', authority):
                with self.assertRaisesRegex(ValueError, 'trusted Formal case identity'):
                    artifacts.formal_configuration()
                changed = copy.deepcopy(self.config)
                changed['case_shards'].setdefault(entry['binary'], {})[entry['name']] = 'healthy-set'
                candidate.write_text(json.dumps(changed))
                selected = FastExecutionSelection().selection(suites=('world-history',))
                selected['formal_shards'] = ['world-history']
                with self.assertRaisesRegex(ValueError, 'outside the trusted plan'):
                    artifacts.formal_shards(selected)
                selected['formal_shards'].append('healthy-set')
                self.assertEqual(artifacts.formal_shards(selected), ['healthy-set'])

    def test_formal_bundle_rejects_another_authority_before_files_or_execution(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            authority, candidate = root / 'authority.json', root / 'candidate.json'
            authority.write_text(json.dumps(self.config))
            candidate.write_text(json.dumps(self.config))
            manifest = dict(tested_sha='tested', source_sha='source', producer_run_id='local',
                checkout=str(root), config_sha256=artifacts.digest(candidate), formal_authority_sha256='other',
                rust_toolchain=os.environ.get('RUST_TOOLCHAIN', 'local'),
                runner_image=os.environ.get('ImageOS', sys.platform))
            (root / 'manifest.json').write_text(json.dumps(manifest))
            with patch.object(artifacts, 'ROOT', root), patch.object(artifacts, 'CONFIG', candidate), \
                 patch.object(artifacts, 'FORMAL_AUTHORITY', authority), patch.object(artifacts, 'output', return_value='tested'), \
                 patch.dict(os.environ, SOURCE_SHA='source', GITHUB_RUN_ID='local'):
                with self.assertRaisesRegex(ValueError, 'Formal authority mismatch'):
                    artifacts.pack(root, root / 'unused.tar.gz', shard='world-history')
                with self.assertRaisesRegex(ValueError, 'formal_authority_sha256'):
                    artifacts.load(root, shard='world-history')

    def test_environment_batch_case_cannot_escape_the_trusted_physical_shards(self):
        config = copy.deepcopy(self.config)
        config['case_shards'].setdefault('runtime-tests', {})['formal::logic'] = 'healthy-set'
        selected = FastExecutionSelection().selection(suites=('runtime-protocol',))
        selected['formal_shards'] = ['runtime-protocol']
        manifest = dict(execution_plan=selected, suites=artifacts.resolve_suites(config, self.binaries))
        with self.assertRaisesRegex(ValueError, 'resolved Formal coverage'):
            artifacts.selected_entries(manifest, config, shard='runtime-protocol')


class FastExecutionSelection(unittest.TestCase):
    def selection(self, packages=('oasis7_viewer_codec',), suites=()):
        return dict(fast_plan_version=1, execution_profile='pr-fast', scope='targeted',
                    groups=['baseline', 'oasis7_required'] + (['formal_process'] if suites else []),
                    oasis7_required_packages=list(packages),
                    oasis7_required_wasm_packages=[p for p in packages if p in artifacts.PORTABLE_REQUIRED_PACKAGES],
                    formal_suites=list(suites), test_policy_changed=False,
                    architecture=dict(source='source', tested='tested'))

    def test_leaf_package_does_not_build_unrelated_harnesses_or_viewer(self):
        variants = artifacts.build_variants('ordinary', self.selection())
        self.assertEqual([name for name, _, _ in variants], ['required'])
        command = variants[0][1]
        self.assertEqual([command[i + 1] for i, arg in enumerate(command) if arg == '-p'], ['oasis7_viewer_codec'])
        self.assertIn('--tests', command)
        self.assertIn('--no-run', command)
        self.assertEqual(command[command.index('--features') + 1], 'oasis7_viewer_codec/test_tier_required')
        self.assertEqual(artifacts.required_arguments(self.selection(), 'wasm'),
                         ['-p', 'oasis7_viewer_codec', '--lib', '--target', 'wasm32-unknown-unknown', '--locked'])

    def test_process_consumers_keep_viewer_and_only_applicable_default_regressions(self):
        plan = self.selection(('oasis7_chain_runtime', 'oasis7_viewer_runtime'))
        variants = artifacts.build_variants('ordinary', plan)
        self.assertEqual({name for name, _, _ in variants}, {'required', 'viewer', 'viewer-snapshot-tests'})
        manifest = dict(execution_plan=plan)
        self.assertEqual(artifacts.snapshot_regressions(manifest), (artifacts.DEFAULT_SNAPSHOT_REGRESSIONS[0],))
        core = self.selection(('oasis7',))
        self.assertEqual(artifacts.snapshot_regressions(dict(execution_plan=core)),
                         (artifacts.DEFAULT_SNAPSHOT_REGRESSIONS[1],))

    def test_old_planner_and_uncertain_scope_cannot_narrow_old_inventory(self):
        old = dict(groups=['oasis7_required'], scope='targeted')
        self.assertEqual(artifacts.build_variants('ordinary', old), artifacts.build_variants('ordinary'))
        plan = self.selection()
        plan['scope'] = 'full'
        self.assertEqual(set(artifacts.required_packages(plan)), set(artifacts.required_packages()))
        self.assertEqual(artifacts.required_wasm_packages(plan), list(artifacts.PORTABLE_REQUIRED_PACKAGES))

    def test_changed_test_policy_includes_expanded_features_before_merge(self):
        plan = self.selection(('oasis7_runtime', 'oasis7_viewer_codec'))
        plan['test_policy_changed'] = True
        features = artifacts.package_features(plan['oasis7_required_packages'], plan)
        self.assertIn('oasis7_runtime/test_tier_full', features)
        self.assertIn('oasis7_runtime/test_tier_required', features)
        self.assertIn('oasis7_viewer_codec/test_tier_required', features)
        self.assertNotIn('oasis7_viewer_codec/test_tier_full', features)

    def test_invalid_or_missing_package_selections_fail_before_build(self):
        for key, bad in [('fast_plan_version', 2), ('fast_plan_version', True),
                         ('oasis7_required_packages', []), ('oasis7_required_packages', ['unknown']),
                         ('oasis7_required_packages', ['oasis7', 'oasis7']),
                         ('groups', ['baseline']), ('test_policy_changed', 'false')]:
            plan = self.selection()
            plan[key] = bad
            with self.subTest(key=key, bad=bad), self.assertRaises(ValueError):
                artifacts.build_variants('ordinary', plan)
        plan = self.selection()
        del plan['oasis7_required_wasm_packages']
        with self.assertRaises(ValueError):
            artifacts.required_arguments(plan, 'wasm')

    def test_case_shard_overrides_do_not_change_logical_selection(self):
        plan = self.selection(suites=('healthy-set',))
        self.assertEqual(artifacts.formal_shards(plan), ['healthy-set', 'storage-recovery', 'lifecycle-cli'])
        config = json.loads(artifacts.CONFIG.read_text())
        fixture = Coverage()
        fixture.setUp()
        manifest = dict(execution_plan=plan, suites=artifacts.resolve_suites(config, fixture.binaries))
        entries = artifacts.selected_entries(manifest, config, shard='storage-recovery')
        self.assertEqual(len(entries), 1)
        self.assertEqual(entries[0]['suite'], 'healthy-set')
        self.assertIn('cold_original_lookup', entries[0]['name'])
        self.assertEqual(artifacts.selected_entries(manifest, config, shard='runtime-protocol'), [])
        with self.assertRaises(ValueError):
            artifacts.selected_entries(manifest, config, suite='storage-recovery')

    def test_source_and_tested_merge_are_bound_separately(self):
        plan = self.selection()
        with patch.object(artifacts, 'output', return_value='tested'), patch.dict(os.environ, SOURCE_SHA='source'):
            artifacts.validate_plan_identity(plan)
            for key in ('source', 'tested'):
                altered = copy.deepcopy(plan)
                altered['architecture'][key] = 'other'
                with self.subTest(key=key), self.assertRaisesRegex(ValueError, 'identity mismatch'):
                    artifacts.validate_plan_identity(altered)
                altered[key + '_sha'] = 'source' if key == 'source' else 'tested'
                with self.subTest(key=key, contradictory=True), self.assertRaisesRegex(ValueError, 'identity mismatch'):
                    artifacts.validate_plan_identity(altered)

    def test_subset_artifact_rejects_another_consumer_plan_before_files(self):
        plan = self.selection()
        other = self.selection(('oasis7_runtime',))
        with tempfile.TemporaryDirectory() as temp:
            bundle = Path(temp)
            (bundle / 'manifest.json').write_text(json.dumps(dict(execution_plan=plan)))
            with self.assertRaisesRegex(ValueError, 'execution plan mismatch'):
                artifacts.pack(bundle, bundle / 'archive.tar.gz', suite='ordinary', plan=other)

    def test_selected_package_or_behavior_cannot_disappear_from_artifact(self):
        plan = self.selection()
        binary = dict(ordinary=True, package='oasis7_viewer_codec', tests=['round_trip'], ignored_tests=[])
        manifest = dict(scope='ordinary', execution_plan=plan, binaries={'codec': binary})
        artifacts.validate_ordinary_selection(manifest)
        for mutation in ('missing', 'other', 'no_behavior'):
            changed = copy.deepcopy(manifest)
            if mutation == 'missing':
                changed['binaries'].clear()
            elif mutation == 'other':
                changed['binaries']['codec']['package'] = 'unselected'
            else:
                changed['binaries']['codec']['ignored_tests'] = ['round_trip']
            with self.subTest(mutation=mutation), self.assertRaises(ValueError):
                artifacts.validate_ordinary_selection(changed)

    def test_selected_fast_bundle_runs_behavior_and_rejects_zero_execution(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            executable = root / 'fixture'
            manifest = dict(scope='ordinary', execution_plan=self.selection(), binaries={
                'codec': dict(path='fixture', ordinary=True, test=True, cwd='.',
                              package='oasis7_viewer_codec', tests=['round_trip'], ignored_tests=[])})
            for count in (1, 0):
                executable.write_text('#!' + sys.executable + '\n'
                                      'print("test result: ok. ' + str(count) +
                                      ' passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s")\n')
                executable.chmod(0o755)
                results = root / ('results-' + str(count) + '.jsonl')
                # Process ownership has its own real-process suite below. This
                # fixture exercises dispatch and libtest count propagation.
                with patch.object(artifacts, 'ROOT', root), patch.object(artifacts, 'load', return_value=(manifest, dict(os.environ))), \
                     patch('ci_process_runner.cleanup', return_value=('clean', False)):
                    if count:
                        artifacts.execute(root, 'ordinary', results, plan=self.selection())
                    else:
                        with self.assertRaises(RuntimeError):
                            artifacts.execute(root, 'ordinary', results, plan=self.selection())
                records = [json.loads(line) for line in results.read_text().splitlines()]
                self.assertEqual(len(records), 1)
                self.assertEqual(records[0]['passed'], count == 1)
                self.assertEqual(records[0]['suite'], 'ordinary')


class RiskTargetInventory(unittest.TestCase):
    def target(self):
        return dict(package='oasis7_viewer_codec', target_kind='test', target_name='scale',
                    default_features=True, features=['test_tier_full'], cases=['full_scale'])

    def selection(self):
        return FastExecutionSelection().selection(('oasis7_viewer_codec',)) | dict(
            risk_test_inventory_version=1, ordinary_risk_targets=[self.target()])

    def test_risk_targets_are_real_builds_and_clippy_without_expanding_every_package(self):
        selected = self.selection()
        variants = artifacts.build_variants('ordinary', selected)
        self.assertEqual([(name, args) for name, args, _ in variants if name.startswith('required-risk-')],
                         [('required-risk-0', ['test', '-p', 'oasis7_viewer_codec', '--test', 'scale',
                                               '--features', 'test_tier_full', '--no-run'])])
        original = next(args for name, args, _ in variants if name == 'required')
        self.assertNotIn('oasis7_viewer_codec/test_tier_full', original[original.index('--features') + 1])
        self.assertIn(['-p', 'oasis7_viewer_codec', '--test', 'scale', '--features', 'test_tier_full'],
                      artifacts.required_native_commands(selected))
        expanded = selected | {'test_policy_changed': True}
        self.assertFalse(any(name.startswith('required-risk-') for name, _, _ in
                             artifacts.build_variants('ordinary', expanded)))

    def test_risk_case_and_full_feature_cannot_disappear_or_replace_required(self):
        selected = self.selection()
        full = FastTargetInventory().binary(self.target(), 'required-risk-0')
        original = dict(full, target='ordinary', variant='required', features=['test_tier_required'],
                        requested_features=['oasis7_viewer_codec/test_tier_required'], tests=['required_case'])
        manifest = dict(scope='ordinary', execution_plan=selected, binaries={'ordinary': original, 'risk': full})
        artifacts.validate_ordinary_selection(manifest)
        for change in ('missing', 'ignored', 'feature', 'required'):
            broken = copy.deepcopy(manifest)
            if change == 'missing':
                del broken['binaries']['risk']
            elif change == 'required':
                del broken['binaries']['ordinary']
            elif change == 'ignored':
                broken['binaries']['risk']['ignored_tests'] = ['full_scale']
            else:
                broken['binaries']['risk']['features'] = ['test_tier_required']
            with self.subTest(change=change), self.assertRaises(ValueError):
                artifacts.validate_ordinary_selection(broken)
        expanded = dict(manifest, execution_plan=selected | {'test_policy_changed': True},
                        binaries={'full-required': dict(full, variant='required',
                            requested_features=['oasis7_viewer_codec/test_tier_required',
                                                'oasis7_viewer_codec/test_tier_full'])})
        artifacts.validate_ordinary_selection(expanded)
        expanded['binaries']['full-required']['tests'] = ['unrelated_case']
        with self.assertRaises(ValueError):
            artifacts.validate_ordinary_selection(expanded)

    def test_all_scope_cannot_silently_drop_risk_contracts(self):
        for expanded in (False, True):
            selected = self.selection() | {'test_policy_changed': expanded}
            with self.subTest(expanded=expanded):
                with self.assertRaisesRegex(ValueError, 'ordinary risk contracts'):
                    artifacts.build_variants('all', selected)
                with self.assertRaisesRegex(ValueError, 'ordinary risk contracts'):
                    artifacts.validate_ordinary_selection(dict(
                        scope='all', execution_plan=selected, binaries={}))
                # Formal components use the same parent plan but do not own the
                # ordinary inventory; they must still build independently.
                self.assertTrue(artifacts.build_variants('formal-debug', selected))
                artifacts.validate_ordinary_selection(dict(
                    scope='formal-debug', execution_plan=selected, binaries={}))

    def test_weighted_owners_keep_full_risk_variant_complete_with_its_own_cost_identity(self):
        fixture = TargetedArtifactPartitions()
        selected = fixture.selection() | dict(
            oasis7_required_packages=['oasis7_chain_runtime', 'oasis7_viewer_codec'],
            risk_test_inventory_version=1, ordinary_risk_targets=[self.target()])
        selected['ordinary_harness_costs'] = fixture.costs([dict(
            package='oasis7_viewer_codec', target_kind='test', target_name='scale',
            variant='required', mode='batch', selector='', elapsed_ms=100000)])
        manifest = fixture.manifest(selected)
        full = FastTargetInventory().binary(self.target(), 'required-risk-0')
        manifest['binaries']['full-scale'] = full | dict(path='full-scale', cwd='.')
        manifest['binaries']['required-scale'] = full | dict(
            path='required-scale', cwd='.', variant='required', tests=['required_chunk'],
            features=['test_tier_required'], requested_features=['oasis7_viewer_codec/test_tier_required'])
        artifacts.validate_ordinary_selection(manifest)
        actual, receipts, full_owners = [], [], []
        for owner in selected['ordinary_partitions'][1:]:
            captured = io.StringIO()
            with redirect_stdout(captured):
                entries = artifacts.ordinary_artifact_entries(manifest, selected, owner)
            receipt = json.loads(captured.getvalue().split('=', 1)[1])
            covered = fixture.coverage(manifest, entries)
            self.assertEqual(receipt['expected_count'], len(covered))
            actual.extend(covered)
            receipts.append(receipt)
            full_owners.extend(owner for entry in entries if entry['binary'] == 'full-scale')
        self.assertEqual(len(full_owners), 1)
        self.assertIn(('full-scale', 'full_scale'), actual)
        self.assertCountEqual(actual, fixture.coverage(manifest, artifacts.ordinary_entries(manifest)))
        self.assertEqual(len(actual), len(set(actual)))
        # The old required timing is not reused for the distinct full feature
        # variant; an unknown full harness still executes with the fallback.
        self.assertEqual(sum(item['cost_matched'] for item in receipts), 1)
        fallback_count = sum(item['cost_fallback'] for item in receipts)
        self.assertGreaterEqual(fallback_count, 1)
        self.assertEqual(sum(item['harness_estimated_ms'] for item in receipts),
                         100000 + fallback_count * selected['ordinary_harness_costs']['fallback_ms'])


class FastTargetInventory(unittest.TestCase):
    def target(self, **changes):
        return dict(package='oasis7_viewer_codec', target_kind='test', target_name='pr_fast',
                    default_features=False, features=['test_tier_required'], cases=['required::round_trip']) | changes

    def selection(self, targets=None, full=False):
        selected = FastExecutionSelection().selection(('oasis7_viewer_codec', 'oasis7_runtime'))
        key = 'ordinary_full_fast_targets' if full else 'ordinary_fast_targets'
        selected['full_fast_test_inventory_version' if full else 'fast_test_inventory_version'] = 1
        selected[key] = [self.target()] if targets is None else targets
        if full:
            selected.update(execution_profile='periodic-full', scope='explicit-full')
        return selected

    def binary(self, target=None, variant='required-fast-0'):
        target = target or self.target()
        return dict(ordinary=True, test=True, package=target['package'], target=target['target_name'],
                    target_kind=[target['target_kind']], default_features=target['default_features'],
                    requested_features=target['features'], features=target['features'],
                    tests=target['cases'] + ['new_regression'], ignored_tests=[], variant=variant)

    def test_explicit_targets_select_before_compilation_and_match_clippy(self):
        selected = self.selection([self.target(), self.target(package='oasis7_runtime',
                                  target_kind='lib', target_name='oasis7_runtime')])
        commands = artifacts.required_native_commands(selected)
        self.assertEqual(commands, [
            ['-p', 'oasis7_viewer_codec', '--test', 'pr_fast', '--no-default-features', '--features', 'test_tier_required'],
            ['-p', 'oasis7_runtime', '--lib', '--no-default-features', '--features', 'test_tier_required'],
        ])
        variants = artifacts.build_variants('ordinary', selected)
        expected = [['test', *args, '--no-run'] for args in commands]
        if sys.platform == 'linux' and hasattr(artifacts, 'RUNTIME_WORKER_VARIANT'):
            expected.append(['build', '-p', 'oasis7_wasm_executor', '--no-default-features',
                             '--features', 'wasmtime', '--bin', 'oasis7-wasm-worker'])
        self.assertEqual([command for _, command, _ in variants], expected)
        self.assertEqual(artifacts.snapshot_regressions(dict(execution_plan=selected)), ())
        viewer = self.selection([self.target(package='oasis7_viewer_runtime')])
        viewer['oasis7_required_packages'] = ['oasis7_viewer_runtime']
        self.assertEqual(len(artifacts.build_variants('ordinary', viewer)), 1)
        manifest = dict(execution_plan=viewer, binaries={'selected': self.binary(viewer['ordinary_fast_targets'][0])})
        self.assertEqual(artifacts.dependency_closure(manifest, {}, suite='ordinary'), {'selected'})

    def test_native_commands_cli_emits_json_for_both_fast_profiles(self):
        sha = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=artifacts.ROOT, text=True).strip()
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / 'plan.json'
            for profile in ('pr-fast', 'main-fast'):
                selected = self.selection()
                selected.update(execution_profile=profile, architecture=dict(source=sha, tested=sha))
                path.write_text(json.dumps(selected))
                command = [sys.executable, str(HERE / 'ci-test-artifacts.py'), 'required-args',
                           '--repo-root', str(artifacts.ROOT), '--plan', str(path)]
                result = subprocess.run([*command, '--phase', 'native-commands'], check=True,
                                        text=True, capture_output=True, env=dict(os.environ, SOURCE_SHA=sha))
                self.assertEqual(json.loads(result.stdout), [['-p', 'oasis7_viewer_codec', '--test', 'pr_fast',
                                 '--no-default-features', '--features', 'test_tier_required']])
                legacy = subprocess.run([*command, '--phase', 'packages'], check=True,
                                        text=True, capture_output=True, env=dict(os.environ, SOURCE_SHA=sha))
                self.assertEqual(legacy.stdout.splitlines(), ['oasis7_viewer_codec', 'oasis7_runtime'])

    def test_old_full_and_policy_plans_keep_original_inventory(self):
        ordinary = self.selection()
        for changes in ({'scope': 'full'}, {'scope': 'explicit-full'}, {'test_policy_changed': True},
                        {'execution_profile': 'periodic-full'}, {'execution_profile': 'candidate-full'}):
            selected = ordinary | changes
            # Inactive capability data cannot break or narrow the fallback.
            selected.update(fast_test_inventory_version=True, ordinary_fast_targets='invalid')
            with self.subTest(changes=changes):
                self.assertIsNone(artifacts.ordinary_fast_targets(selected))
                required = next(args for name, args, _ in artifacts.build_variants('ordinary', selected) if name == 'required')
                self.assertIn('--tests', required)
                self.assertEqual(len(artifacts.required_native_commands(selected)), 1)
        legacy = copy.deepcopy(ordinary)
        del legacy['fast_test_inventory_version']
        self.assertIsNone(artifacts.ordinary_fast_targets(legacy))
        self.assertIn('--tests', artifacts.required_native_commands(legacy)[0])
        self.assertEqual(artifacts.required_native_commands(None), [artifacts.required_arguments(None)])

    def test_invalid_target_inventory_fails_before_build(self):
        for key, value in [('fast_test_inventory_version', True), ('fast_test_inventory_version', 2),
                           ('ordinary_fast_targets', []), ('ordinary_fast_targets', [self.target(), self.target()])]:
            with self.subTest(key=key, value=value), self.assertRaises(ValueError):
                artifacts.build_variants('ordinary', self.selection() | {key: value})
        for changes in (dict(package='unselected'), dict(target_kind='bin'), dict(target_name='../escape'),
                        dict(default_features='false'), dict(features=['default']), dict(features=['x/y']),
                        dict(features=['repeated', 'repeated']), dict(cases=[]), dict(cases=['x\ny']),
                        dict(cases=['repeated', 'repeated']), dict(extra=True)):
            with self.subTest(changes=changes), self.assertRaises(ValueError):
                artifacts.build_variants('ordinary', self.selection([self.target(**changes)]))

    def test_minimum_cases_target_and_actual_features_are_verified(self):
        manifest = dict(scope='ordinary', execution_plan=self.selection(), binaries={'fast': self.binary()})
        artifacts.validate_ordinary_selection(manifest)
        for change in (dict(target='renamed'), dict(target_kind=['lib']), dict(test=False),
                       dict(requested_features=[]), dict(features=[]), dict(features=['default', 'test_tier_required']),
                       dict(default_features=True), dict(tests=['new_regression']),
                       dict(ignored_tests=['required::round_trip'])):
            altered = copy.deepcopy(manifest)
            altered['binaries']['fast'].update(change)
            with self.subTest(change=change), self.assertRaises(ValueError):
                artifacts.validate_ordinary_selection(altered)
        altered = copy.deepcopy(manifest)
        altered['binaries']['duplicate'] = self.binary()
        with self.assertRaisesRegex(ValueError, 'target inventory'):
            artifacts.validate_ordinary_selection(altered)

    def test_same_package_lib_and_test_have_distinct_artifact_names(self):
        lib = artifacts.artifact_logical_name('required-fast-0', 'oasis7_runtime',
                                            dict(name='oasis7_runtime', kind=['lib']), True)
        test = artifacts.artifact_logical_name('required-fast-1', 'oasis7_runtime',
                                             dict(name='oasis7_runtime', kind=['test']), True)
        self.assertNotEqual(lib, test)
        full = artifacts.artifact_logical_name('required-full-fast-0', 'oasis7_runtime',
                                             dict(name='oasis7_runtime', kind=['lib']), True)
        self.assertNotEqual(lib, full)

    def test_full_variants_preserve_originals_and_append_exact_fast_commands(self):
        selected = self.selection(full=True)
        selected.update(fast_test_inventory_version='invalid', ordinary_fast_targets='invalid')
        legacy = copy.deepcopy(selected)
        del legacy['full_fast_test_inventory_version']
        for scope in ('ordinary', 'all'):
            full_variants = artifacts.build_variants(scope, selected)
            original = artifacts.build_variants(scope, legacy)
            if sys.platform == 'linux' and hasattr(artifacts, 'RUNTIME_WORKER_VARIANT'):
                worker = (artifacts.RUNTIME_WORKER_VARIANT,
                          ['build', '-p', 'oasis7_wasm_executor', '--no-default-features',
                           '--features', 'wasmtime', '--bin', 'oasis7-wasm-worker'], ['wasmtime'])
                self.assertEqual(full_variants[-1], worker)
                self.assertEqual(original[-1], worker)
                full_variants, original = full_variants[:-1], original[:-1]
            self.assertEqual(full_variants[:-1], original)
            self.assertEqual(full_variants[-1][1], ['test', '-p', 'oasis7_viewer_codec', '--test', 'pr_fast',
                                                   '--no-default-features', '--features', 'test_tier_required', '--no-run'])
        self.assertEqual(artifacts.required_native_commands(selected), [artifacts.required_arguments(selected),
                         ['-p', 'oasis7_viewer_codec', '--test', 'pr_fast', '--no-default-features', '--features', 'test_tier_required']])
        for profile in ('periodic-full', 'candidate-full'):
            for key, value in [('full_fast_test_inventory_version', True), ('full_fast_test_inventory_version', 2),
                               ('ordinary_full_fast_targets', [])]:
                with self.subTest(profile=profile, key=key), self.assertRaises(ValueError):
                    artifacts.build_variants('ordinary', selected | {'execution_profile': profile, key: value})

    def test_full_supplements_cannot_replace_original_required_inventory(self):
        selected = self.selection(full=True)
        originals = {package: dict(ordinary=True, test=True, package=package, tests=['old'], ignored_tests=[], variant='required')
                     for package in artifacts.required_packages()}
        manifest = dict(scope='ordinary', execution_plan=selected,
                        binaries=originals | {'supplement': self.binary(variant='required-full-fast-0')})
        for scope in ('ordinary', 'all'):
            manifest['scope'] = scope
            artifacts.validate_ordinary_selection(manifest)
            for missing in ('oasis7_viewer_codec', 'supplement'):
                altered = copy.deepcopy(manifest)
                del altered['binaries'][missing]
                with self.subTest(scope=scope, missing=missing), self.assertRaises(ValueError):
                    artifacts.validate_ordinary_selection(altered)


class OrdinaryCasePrefixes(unittest.TestCase):
    def selection(self, *, full=False):
        target = FastTargetInventory().target(cases=['required::round_trip'],
                                             case_prefixes=['required::', 'recovery::'])
        return FastTargetInventory().selection([target], full=full) | {
            'ordinary_case_prefix_contract_version': 1,
        }

    def manifest(self, *, full=False):
        selected = self.selection(full=full)
        target = selected['ordinary_full_fast_targets' if full else 'ordinary_fast_targets'][0]
        binary = FastTargetInventory().binary(target, 'required-full-fast-0' if full else 'required-fast-0')
        binary['tests'] = ['required::round_trip', 'required::new_regression', 'required::nested::reject',
                           'recovery::resume', 'required_neighbor::unrelated', 'outside::unrelated',
                           'required::ignored_child']
        binary['ignored_tests'] = ['required::ignored_child']
        return dict(scope='ordinary', execution_plan=selected, binaries={'fixture': binary})

    def test_prefixes_require_the_versioned_complete_schema(self):
        valid = self.selection()
        self.assertEqual(artifacts.required_native_commands(valid),
                         artifacts.required_native_commands(FastTargetInventory().selection()))
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
        risk = RiskTargetInventory().selection() | {'ordinary_case_prefix_contract_version': 1}
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
        target = FastTargetInventory().target(package='oasis7_chain_runtime', target_kind='lib',
            target_name='oasis7_chain_runtime', cases=['header::required'], case_prefixes=['header::'])
        selected = FastExecutionSelection().selection(('oasis7_chain_runtime',)) | dict(
            ordinary_case_prefix_contract_version=1, fast_test_inventory_version=1, ordinary_fast_targets=[target],
            formal_full_profiles_only_version=1, formal_shards=[],
            formal_deferred_ordinary_cases={'runtime-tests': ['header::four_node']})
        binary = FastTargetInventory().binary(target)
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
        full['execution_plan'] = FastTargetInventory().selection([target], full=True) | dict(
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
                target = FastTargetInventory().target(package='oasis7_chain_runtime', target_kind='lib',
                    target_name='oasis7_chain_runtime', cases=['required::keep'], case_prefixes=['required::'])
                selected = FastTargetInventory().selection([target], full=full) | {
                    'ordinary_case_prefix_contract_version': 1,
                    'oasis7_required_packages': ['oasis7_chain_runtime'],
                    'oasis7_required_wasm_packages': [],
                }
                binary = FastTargetInventory().binary(target, 'required' if full else 'required-fast-0')
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
        return FastExecutionSelection().selection(('oasis7', 'oasis7_runtime')) | {
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
        return InlineExecutionSelection().selection() | {
            'groups': ['baseline', 'rust_baseline', 'oasis7_required'],
            'ordinary_clippy_group': 'rust_baseline',
            'ordinary_support_contract_version': 1,
            'ordinary_support_handoffs': [owner],
            'ordinary_support_deferred': ['oasis7_agent_harness', 'oasis7_model_provider'],
            'oasis7_required_packages': [owner],
            'workspace_support_profiles': [],
            'ordinary_fast_targets': [FastTargetInventory().target(
                package=owner, target_kind='lib', target_name=owner, default_features=True, features=[]),
                FastTargetInventory().target(package=owner, target_name='dev_test_service_cli',
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


class InlineExecutionSelection(unittest.TestCase):
    def selection(self, checks=None):
        return FastTargetInventory().selection() | {
            'fast_execution_version': 1, 'ordinary_execution': 'inline',
            'rust_baseline_checks': [] if checks is None else checks,
        }

    def test_only_compatible_bounded_plans_select_baseline_extras(self):
        for profile in ('pr-fast', 'main-fast'):
            selected = self.selection() | {'execution_profile': profile}
            self.assertEqual(artifacts.required_arguments(selected, 'baseline-checks'), [])
            self.assertEqual(artifacts.required_native_commands(selected),
                             artifacts.required_native_commands(FastTargetInventory().selection()))
        selected = self.selection(['newapi_accounting'])
        self.assertEqual(artifacts.required_arguments(selected, 'baseline-checks'), ['newapi_accounting'])
        for selected in (None, {}, FastTargetInventory().selection(),
                         self.selection() | {'fast_execution_version': 2}):
            with self.subTest(legacy=selected):
                self.assertEqual(artifacts.required_arguments(selected, 'baseline-checks'),
                                 list(artifacts.RUST_BASELINE_CHECKS))

    def test_known_inline_capability_rejects_incomplete_or_expanded_plans(self):
        for version in (True, '1', 1.0, None):
            with self.subTest(legacy_version=version), self.assertRaises(ValueError):
                artifacts.required_arguments({'fast_execution_version': version}, 'baseline-checks')
        invalid = [('fast_execution_version', True), ('fast_execution_version', '1'),
                   ('ordinary_execution', 'artifact'), ('execution_profile', 'periodic-full'),
                   ('execution_profile', 'candidate-full'), ('scope', 'full'), ('scope', 'explicit-full'),
                   ('test_policy_changed', True), ('groups', ['baseline']),
                   ('fast_test_inventory_version', True), ('fast_test_inventory_version', 2),
                   ('ordinary_fast_targets', []), ('rust_baseline_checks', 'newapi_accounting'),
                   ('rust_baseline_checks', ['unknown']),
                   ('rust_baseline_checks', ['newapi_accounting', 'newapi_accounting'])]
        for key, value in invalid:
            for phase in ('baseline-checks', 'native', 'wasm', 'clippy-group'):
                with self.subTest(field=key, phase=phase), self.assertRaises(ValueError):
                    artifacts.required_arguments(self.selection() | {key: value}, phase)
        for key in ('fast_plan_version', 'fast_test_inventory_version', 'ordinary_execution',
                    'ordinary_fast_targets', 'rust_baseline_checks'):
            selected = self.selection()
            del selected[key]
            with self.subTest(missing=key), self.assertRaises(ValueError):
                artifacts.required_arguments(selected, 'baseline-checks')
        selected = self.selection()
        selected['ordinary_fast_targets'][0]['cases'] = []
        with self.assertRaises(ValueError):
            artifacts.required_arguments(selected, 'baseline-checks')

    def test_clippy_owner_requires_the_complete_known_inline_plan(self):
        selected = self.selection() | {'ordinary_clippy_group': 'rust_baseline',
                                      'groups': ['baseline', 'rust_baseline', 'oasis7_required']}
        self.assertEqual(artifacts.required_arguments(selected, 'clippy-group'), ['rust_baseline'])
        for legacy in (None, {}, self.selection(), selected | {'fast_execution_version': 2},
                       FastTargetInventory().selection(full=True)):
            with self.subTest(legacy=legacy):
                self.assertEqual(artifacts.required_arguments(legacy, 'clippy-group'), ['oasis7_required'])
        invalid = [('ordinary_clippy_group', None), ('ordinary_clippy_group', ['rust_baseline']),
                   ('ordinary_clippy_group', 'other'), ('groups', ['baseline', 'oasis7_required']),
                   ('groups', ['baseline', 'rust_baseline']), ('ordinary_execution', 'artifact'),
                   ('execution_profile', 'periodic-full'), ('scope', 'full')]
        for key, value in invalid:
            for phase in ('clippy-group', 'native', 'baseline-checks'):
                with self.subTest(field=key, value=value, phase=phase), self.assertRaises(ValueError):
                    artifacts.required_arguments(selected | {key: value}, phase)
        checker = module('ci-required-result')
        jobs = {group: group.replace('_', '-') for group in selected['groups']}
        successful = {name: {'result': 'success'} for name in ['select', *jobs.values()]}
        self.assertTrue(checker.validate(selected, successful, jobs))
        for failed in ('failure', 'cancelled', 'skipped', None):
            with self.subTest(clippy_owner_status=failed), self.assertRaisesRegex(ValueError, 'rust_baseline'):
                checker.validate(selected, successful | {'rust-baseline': {'result': failed}}, jobs)

    def test_inline_build_combines_only_identical_package_feature_selections(self):
        target = FastTargetInventory().target
        targets = [
            target(target_name='first'),
            target(target_kind='lib', target_name='oasis7_viewer_codec'),
            target(target_name='different_features', features=[]),
            target(target_name='different_defaults', default_features=True),
            target(package='oasis7_runtime', target_name='runtime'),
        ]
        selected = self.selection() | {'ordinary_fast_targets': targets}
        variants = artifacts.build_variants('ordinary', selected)
        self.assertEqual(len(variants), 4)
        self.assertEqual(variants[0], ('required-fast-0',
            ['test', '-p', 'oasis7_viewer_codec', '--test', 'first', '--lib',
             '--no-default-features', '--features', 'test_tier_required', '--no-run'],
            ['test_tier_required']))
        self.assertEqual([name for name, _, _ in variants],
                         ['required-fast-0', 'required-fast-2', 'required-fast-3', 'required-fast-4'])
        # Clippy still checks every exact original target and feature contract.
        self.assertEqual(len(artifacts.required_native_commands(selected)), len(targets))
        for capability in (None, 2):
            legacy = dict(selected)
            if capability is None:
                del legacy['fast_execution_version']
            else:
                legacy['fast_execution_version'] = capability
            self.assertEqual(len(artifacts.build_variants('ordinary', legacy)), len(targets))
        full = FastTargetInventory().selection(targets, full=True)
        supplements = [v for v in artifacts.build_variants('ordinary', full)
                       if v[0].startswith('required-full-fast-')]
        self.assertEqual(len(supplements), len(targets))


class PrivateUnitTestSelection(unittest.TestCase):
    def selection(self):
        owner = 'oasis7_world_state'
        target = FastTargetInventory().target(package=owner, target_kind='lib', target_name=owner)
        selected = InlineExecutionSelection().selection()
        return selected | {
            'source_sha': 'a' * 40, 'tested_sha': 'b' * 40, 'policy_sha': 'c' * 40,
            'architecture': dict(base='c' * 40, source='a' * 40, tested='b' * 40),
            'groups': ['baseline', 'rust_baseline', 'oasis7_required'],
            'ordinary_fast_targets': [target], 'oasis7_required_packages': [owner],
            'oasis7_required_wasm_packages': [],
            'ordinary_clippy_group': 'rust_baseline', 'formal_suites': [], 'fast_plan_exceptions': [],
            'ordinary_unit_test_contract_version': 1,
            'ordinary_unit_test_sources': {f'crates/{owner}/src/faults.rs': owner},
            'ordinary_unit_test_proof_refs': ['a' * 40, 'b' * 40, 'c' * 40],
            'ordinary_unit_test_excluded_groups': ['distfs', 'net', 'node'],
            'ordinary_unit_test_excluded_targets': [dict(package='oasis7_runtime', target_kind='lib',
                                                        target_name='oasis7_runtime')],
        }

    def test_complete_owner_build_and_clippy_keep_their_original_commands(self):
        selected = self.selection()
        args = ['-p', 'oasis7_world_state', '--lib', '--no-default-features', '--features', 'test_tier_required']
        self.assertEqual(artifacts.required_native_commands(selected), [args])
        self.assertEqual(artifacts.build_variants('ordinary', selected),
                         [('required-fast-0', ['test', *args, '--no-run'], ['test_tier_required'])])
        target = selected['ordinary_fast_targets'][0]
        binary = FastTargetInventory().binary(target)
        manifest = dict(scope='ordinary', execution_plan=selected, binaries={'world': binary})
        artifacts.validate_ordinary_selection(manifest)
        for names in ([], ['required::round_trip']):
            changed = copy.deepcopy(manifest)
            changed['binaries']['world']['tests'] = names
            if names:
                changed['binaries']['world']['ignored_tests'] = names
            with self.subTest(tests=names), self.assertRaises(ValueError):
                artifacts.validate_ordinary_selection(changed)

    def test_partial_or_inconsistent_source_contract_cannot_erase_execution(self):
        selected = self.selection()
        mutations = [{key: None} for key in artifacts.UNIT_TEST_CONTRACT_FIELDS]
        mutations += [
            {'ordinary_unit_test_contract_version': True}, {'ordinary_unit_test_contract_version': 2},
            {'ordinary_unit_test_sources': {}}, {'ordinary_unit_test_sources': {'../faults.rs': 'oasis7_world_state'}},
            {'ordinary_unit_test_proof_refs': ['a' * 40]},
            {'ordinary_unit_test_proof_refs': ['a' * 40, 'b' * 40]},
            {'ordinary_unit_test_excluded_groups': ['rust_baseline']},
            {'ordinary_unit_test_excluded_targets': [dict(package='oasis7_world_state', target_kind='lib',
                                                         target_name='oasis7_world_state')]},
            {'ordinary_fast_targets': []}, {'test_policy_changed': True},
            {'groups': ['baseline', 'oasis7_required']}, {'oasis7_required_packages': []},
            {'ordinary_clippy_group': 'oasis7_required'}, {'formal_suites': ['world-history']},
            {'scope': 'full'}, {'execution_profile': 'periodic-full'},
        ]
        for mutation in mutations:
            with self.subTest(mutation=mutation), self.assertRaises(ValueError):
                artifacts.required_native_commands(selected | mutation)
        for key in artifacts.UNIT_TEST_CONTRACT_FIELDS | {'fast_plan_version'}:
            partial = dict(selected)
            del partial[key]
            with self.subTest(missing=key), self.assertRaises(ValueError):
                artifacts.required_native_commands(partial)

    def test_every_remaining_required_owner_still_blocks_on_failure_or_missing(self):
        selected = self.selection()
        checker = module('ci-required-result')
        jobs = {group: group.replace('_', '-') for group in selected['groups']}
        success = {name: {'result': 'success'} for name in ['select', *jobs.values()]}
        self.assertTrue(checker.validate(selected, success, jobs))
        for group, job in jobs.items():
            for result in ('failure', 'cancelled', 'skipped', None):
                with self.subTest(group=group, result=result), self.assertRaises(ValueError):
                    checker.validate(selected, success | {job: {'result': result}}, jobs)
            with self.subTest(group=group, result='missing'), self.assertRaises(ValueError):
                checker.validate(selected, {name: value for name, value in success.items() if name != job}, jobs)


class DistfsClippyHandoff(unittest.TestCase):
    def selection(self):
        selected = native_package_partition_selection()
        for target in selected['ordinary_fast_targets']:
            if target['target_kind'] == 'test':
                target['target_name'] = ('pr_fast_incremental' if target['package'] == 'oasis7_runtime'
                                         else 'pr_fast_reducers')
        return selected | {
            'distfs_clippy_contract_version': 1, 'distfs_clippy_group': 'rust_baseline',
            'groups': ['baseline', 'rust_baseline', 'oasis7_required', 'distfs'],
            'formal_suites': [], 'fast_plan_exceptions': [],
            'architecture': dict(mode='targeted', build_policy_version=1,
                build_policy='ordinary-targets', consumer_groups={'distfs': ['oasis7_distfs']}),
        }

    def test_original_lint_inventory_has_one_owner_and_keeps_both_required_groups(self):
        selected = self.selection()
        expected = [['-p', 'oasis7_cas', '-p', 'oasis7_formal_protocol', '--lib'],
                    ['-p', 'oasis7_distfs', '--lib', '--bin', 'oasis7_durability_endpoint']]
        self.assertEqual(artifacts.required_native_commands(selected)[-2:], expected)
        self.assertEqual(artifacts.required_arguments(selected, 'distfs-clippy-group'), ['rust_baseline'])
        legacy = {key: value for key, value in selected.items()
                  if key not in ('distfs_clippy_contract_version', 'distfs_clippy_group')}
        self.assertEqual(len(artifacts.required_native_commands(legacy)), 4)
        for plan in (None, {}, legacy, FastTargetInventory().selection(full=True)):
            self.assertEqual(artifacts.required_arguments(plan, 'distfs-clippy-group'), ['distfs'])
        checker = module('ci-required-result')
        jobs = {group: group.replace('_', '-') for group in selected['groups']}
        successful = {name: {'result': 'success'} for name in ['select', *jobs.values()]}
        self.assertTrue(checker.validate(selected, successful, jobs))
        for owner in ('rust-baseline', 'distfs'):
            for failure in ('failure', 'cancelled', 'skipped', None):
                with self.subTest(owner=owner, status=failure), self.assertRaises(ValueError):
                    checker.validate(selected, successful | {owner: {'result': failure}}, jobs)

    def test_partial_untrusted_or_incomplete_handoff_cannot_skip_lint(self):
        selected = self.selection()
        invalid = [('distfs_clippy_contract_version', value) for value in (True, None, '1', 2)]
        invalid += [('distfs_clippy_group', value) for value in ('distfs', None, 'other')]
        invalid += [('ordinary_clippy_group', 'oasis7_required'), ('ordinary_execution', 'artifact'),
                    ('fast_execution_version', 2), ('scope', 'full'), ('execution_profile', 'candidate-full'),
                    ('test_policy_changed', True), ('fast_plan_exceptions', ['extra']),
                    ('formal_suites', ['world-history']), ('oasis7_required_wasm_packages', ['oasis7_runtime']),
                    ('ordinary_partitions', ['all'])]
        invalid += [('groups', [group for group in selected['groups'] if group != missing])
                    for missing in ('distfs', 'rust_baseline', 'oasis7_required')]
        candidates = [selected | {key: value} for key, value in invalid]
        candidates += [{key: value for key, value in selected.items() if key != missing}
                       for missing in ('distfs_clippy_contract_version', 'distfs_clippy_group', 'fast_plan_version')]
        candidates += [selected | {'ordinary_fast_targets': selected['ordinary_fast_targets'][:-1]},
                       selected | {'ordinary_fast_targets': selected['ordinary_fast_targets'][:1] * 4}]
        for changes in ({'features': []}, {'default_features': True}, {'target_name': 'unknown'},
                        {'target_kind': []}, {'cases': []}):
            candidate = copy.deepcopy(selected)
            candidate['ordinary_fast_targets'][0].update(changes)
            candidates.append(candidate)
        for candidate in candidates:
            with self.subTest(plan=candidate):
                with self.assertRaises(ValueError):
                    artifacts.required_arguments(candidate, 'distfs-clippy-group')
                with self.assertRaises(ValueError):
                    artifacts.required_native_commands(candidate)

    def test_whole_driver_moves_exact_lints_preserves_tests_and_propagates_failures(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            repo, binary = root / 'repo', root / 'bin'
            scripts = repo / 'scripts'
            scripts.mkdir(parents=True)
            binary.mkdir()
            (scripts / 'cargo-dev-architecture.py').write_text('print("architecture fixture")\n')
            (scripts / 'check-rust-hygiene.py').write_text('print("rust hygiene fixture")\n')
            for name in ('check-rustsec-ignore-baseline.test.sh', 'check-rustsec-ignore-baseline.sh',
                         'ensure-cargo-deny.sh', 'check-rust-file-size.test.sh', 'check-rust-file-size.sh'):
                path = scripts / name
                path.write_text('#!/usr/bin/env bash\nexit 0\n')
                path.chmod(0o755)
            cargo = binary / 'cargo'
            cargo.write_text('''#!/usr/bin/env bash
set -euo pipefail
args_json=""; sep=""
for arg in "$@"; do
  s="${arg//\\\\/\\\\\\\\}"; s="${s//\\"/\\\\\\"}"
  args_json+="${sep}\\"${s}\\""; sep=", "
done
printf '[%s]\\n' "$args_json" >> "$CARGO_LOG"
if [[ "${1:-}" == "clippy" && -n "${FAIL_CLIPPY_PACKAGE:-}" ]]; then
  args=("$@")
  for (( i = 0; i + 1 < ${#args[@]}; i++ )); do
    if [[ "${args[$i]}" == "-p" ]]; then
      [[ "${args[$((i + 1))]}" != "$FAIL_CLIPPY_PACKAGE" ]] || exit 19
      break
    fi
  done
fi
''')
            cargo.chmod(0o755)
            subprocess.run(['git', 'init', '-q', str(repo)], check=True)
            subprocess.run(['git', '-C', str(repo), 'add', '.'], check=True)
            subprocess.run(['git', '-C', str(repo), '-c', 'user.name=CI fixture', '-c',
                            'user.email=fixture@example.invalid', 'commit', '-qm', 'fixture'], check=True)
            sha = subprocess.check_output(['git', '-C', str(repo), 'rev-parse', 'HEAD'], text=True).strip()
            selected = self.selection() | {'source_sha': sha, 'tested_sha': sha}
            selected['architecture'].update(source=sha, tested=sha)
            selection, log = root / 'selection.json', root / 'cargo.jsonl'
            original_lints = [
                ['clippy', '-p', 'oasis7_cas', '-p', 'oasis7_formal_protocol', '--lib'],
                ['clippy', '-p', 'oasis7_distfs', '--lib', '--bin', 'oasis7_durability_endpoint']]
            original_lints = [args + ['--', '-D', 'warnings', '-D', 'clippy::correctness',
                                      '-D', 'clippy::suspicious'] for args in original_lints]
            distfs_lib_test_variants = (
                [['test', '-p', 'oasis7_distfs', '--lib', '--bin', 'oasis7_durability_endpoint', '--', '--test-threads=1']],
                [
                    ['test', '-p', 'oasis7_distfs', '--lib', '--bin', 'oasis7_durability_endpoint',
                     '--', '--skip', 'controlled_authority::formal::transport::'],
                    ['test', '-p', 'oasis7_distfs', '--lib', 'controlled_authority::formal::transport::',
                     '--', '--test-threads=1'],
                ],
            )
            allowed_tests = tuple(
                [cas_cmd, *distfs_cmds]
                for cas_cmd in (
                    ['test', '-p', 'oasis7_cas', '-p', 'oasis7_formal_protocol', '--lib', '--', '--test-threads=1'],
                    ['test', '-p', 'oasis7_cas', '-p', 'oasis7_formal_protocol', '--lib'],
                )
                for distfs_cmds in distfs_lib_test_variants
            )
            for mode, failed in (('handoff', ''), ('legacy', ''), ('handoff', 'oasis7_cas'),
                                 ('handoff', 'oasis7_distfs'), ('manual', '')):
                current = copy.deepcopy(selected)
                if mode == 'legacy':
                    del current['distfs_clippy_contract_version'], current['distfs_clippy_group']
                selection.write_text(json.dumps(current))
                observed = {}
                for group in (('distfs',) if mode == 'manual' else ('rust_baseline', 'distfs')):
                    log.unlink(missing_ok=True)
                    plan_args = [] if mode == 'manual' else ['--plan', str(selection)]
                    result = subprocess.run(['bash', str(HERE / 'ci-tests.sh'), 'required', '--group', group,
                        *plan_args, '--repo-root', str(repo)], text=True, capture_output=True,
                        env=dict(os.environ, PATH=str(binary) + os.pathsep + os.environ['PATH'], SOURCE_SHA=sha,
                                 CI_VERBOSE='', CARGO_LOG=str(log), FAIL_CLIPPY_PACKAGE=failed,
                                 OASIS7_CI_RUN_PROVIDER_LIVE_GATE='false', OASIS7_CI_RUN_HOSTED_ACCOUNT_SMOKE='false'))
                    commands = [json.loads(line) for line in log.read_text().splitlines()]
                    observed[group] = [args for args in commands if args[0] == 'clippy' and args[2] in ('oasis7_cas', 'oasis7_distfs')]
                    with self.subTest(mode=mode, failed=failed, group=group):
                        self.assertEqual(result.returncode, 19 if group == 'rust_baseline' and failed else 0,
                                         result.stdout + result.stderr)
                        if group == 'distfs':
                            self.assertIn([args for args in commands if args[0] == 'test'], allowed_tests)
                owner = 'rust_baseline' if mode == 'handoff' else 'distfs'
                self.assertEqual(observed[owner], original_lints[:1] if failed == 'oasis7_cas' else original_lints)
                self.assertEqual(observed.get('distfs' if owner == 'rust_baseline' else 'rust_baseline', []), [])
            for field in ('source_sha', 'tested_sha'):
                selection.write_text(json.dumps(selected | {field: '0' * 40}))
                log.unlink(missing_ok=True)
                result = subprocess.run(['bash', str(HERE / 'ci-tests.sh'), 'required', '--group', 'distfs',
                    '--plan', str(selection), '--repo-root', str(repo)], text=True, capture_output=True,
                    env=dict(os.environ, PATH=str(binary) + os.pathsep + os.environ['PATH'], SOURCE_SHA=sha,
                             CI_VERBOSE='', CARGO_LOG=str(log), FAIL_CLIPPY_PACKAGE=''))
                self.assertNotEqual(result.returncode, 0)
                self.assertIn('execution plan identity mismatch', result.stderr)
                self.assertFalse(log.exists())


class DistfsNamedIntegrationDriver(unittest.TestCase):
    def selection(self, targets=None):
        selected = FastTargetInventory().selection(targets)
        selected['groups'].append('distfs')
        selected['oasis7_required_packages'] = sorted({target['package']
            for target in selected['ordinary_fast_targets']})
        selected['oasis7_required_wasm_packages'] = [package for package in selected['oasis7_required_packages']
            if package in artifacts.PORTABLE_REQUIRED_PACKAGES]
        return selected

    def test_industry_phase_requires_the_exact_selected_test_identity(self):
        industry = FastTargetInventory().target(package='oasis7', target_name='incremental_industry_jobs')
        cases = [([industry], 'run'),
                 ([industry, FastTargetInventory().target()], 'run'),
                 ([industry | {'package': 'oasis7_runtime'}], 'skip'),
                 ([industry | {'target_kind': 'lib'}], 'skip'),
                 ([industry | {'target_name': 'another_industry_test'}], 'skip')]
        for profile in ('pr-fast', 'main-fast'):
            for targets, expected in cases:
                selected = self.selection(targets) | {'execution_profile': profile}
                with self.subTest(profile=profile, targets=targets):
                    self.assertEqual(artifacts.required_arguments(selected, 'distfs-industry'), [expected])
        # The phase cannot turn a future, currently unselected package into an
        # authorized ordinary consumer merely because its target name matches.
        with self.assertRaises(ValueError):
            artifacts.required_arguments(self.selection([industry | {'package': 'oasis7_distfs'}]),
                                         'distfs-industry')

    def test_industry_phase_keeps_complete_runs_when_fast_inventory_is_inactive(self):
        selected = self.selection()
        legacy = {key: value for key, value in selected.items() if key != 'fast_test_inventory_version'}
        legacy['ordinary_fast_targets'] = 'stale capability data'
        candidates = [None, {}, FastExecutionSelection().selection(), legacy]
        for changes in ({'scope': 'full'}, {'scope': 'explicit-full'}, {'test_policy_changed': True},
                        {'execution_profile': 'periodic-full'}, {'execution_profile': 'candidate-full'}):
            candidates.append(selected | changes | {
                'fast_test_inventory_version': True, 'ordinary_fast_targets': 'stale capability data'})
        for candidate in candidates:
            with self.subTest(plan=candidate):
                self.assertEqual(artifacts.required_arguments(candidate, 'distfs-industry'), ['run'])

    def test_industry_phase_rejects_missing_empty_and_invalid_active_inventory(self):
        selected = self.selection()
        candidates = [selected | {key: value} for key, value in (
            ('fast_plan_version', True), ('fast_test_inventory_version', True),
            ('fast_test_inventory_version', 2), ('ordinary_fast_targets', []),
            ('ordinary_fast_targets', None), ('ordinary_fast_targets', 'invalid'),
            ('ordinary_fast_targets', selected['ordinary_fast_targets'] * 2))]
        candidates.append({key: value for key, value in selected.items() if key != 'ordinary_fast_targets'})
        for changes in ({'package': 'unselected'}, {'target_kind': 'bin'}, {'cases': []},
                        {'target_name': '../incremental_industry_jobs'}):
            candidate = copy.deepcopy(selected)
            candidate['ordinary_fast_targets'][0].update(changes)
            candidates.append(candidate)
        for candidate in candidates:
            with self.subTest(plan=candidate), self.assertRaises(ValueError):
                artifacts.required_arguments(candidate, 'distfs-industry')

    def test_named_target_presence_handoff_and_failures_preserve_original_commands(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            repo, binary = root / 'repo', root / 'bin'
            scripts = repo / 'scripts'
            scripts.mkdir(parents=True)
            binary.mkdir()
            (scripts / 'cargo-dev-architecture.py').write_text('print("architecture fixture")\n')
            (scripts / 'check-rust-hygiene.py').write_text('print("rust hygiene fixture")\n')
            for name in ('check-rustsec-ignore-baseline.test.sh', 'check-rustsec-ignore-baseline.sh',
                         'ensure-cargo-deny.sh', 'check-rust-file-size.test.sh', 'check-rust-file-size.sh'):
                path = scripts / name
                path.write_text('#!/usr/bin/env bash\nexit 0\n')
                path.chmod(0o755)
            crate = repo / 'crates/oasis7_distfs'
            (crate / 'tests').mkdir(parents=True)
            (crate / 'Cargo.toml').write_text(
                '[package]\nname="oasis7_distfs"\nversion="0.1.0"\nedition="2024"\n')
            # A different integration file must not authorize the named driver.
            (crate / 'tests/other.rs').write_text('#[test] fn unrelated() {}\n')
            named = crate / 'tests/incremental_industry_jobs.rs'
            cargo = binary / 'cargo'
            cargo.write_text('''#!/usr/bin/env bash
set -euo pipefail
args_json=""; sep=""
for arg in "$@"; do
  s="${arg//\\\\/\\\\\\\\}"; s="${s//\\"/\\\\\\"}"
  args_json+="${sep}\\"${s}\\""; sep=", "
done
printf '[%s]\\n' "$args_json" >> "$CARGO_LOG"
if [[ "${1:-}" == "${FAIL_NAMED_COMMAND:-}" && -n "${1:-}" ]]; then
  args=("$@")
  for (( i = 0; i + 1 < ${#args[@]}; i++ )); do
    if [[ "${args[$i]}" == "--test" ]]; then
      if [[ "${args[$((i + 1))]}" == "incremental_industry_jobs" ]]; then
        if [[ "$1" == "test" ]]; then exit 23; else exit 29; fi
      fi
      break
    fi
  done
fi
''')
            cargo.chmod(0o755)
            subprocess.run(['git', 'init', '-q', str(repo)], check=True)
            selection, env_selection, log = root / 'selection.json', root / 'env-selection.json', root / 'cargo.jsonl'
            lint_flags = ['--', '-D', 'warnings', '-D', 'clippy::correctness', '-D', 'clippy::suspicious']
            original_lints = [
                ['clippy', '-p', 'oasis7_cas', '-p', 'oasis7_formal_protocol', '--lib'] + lint_flags,
                ['clippy', '-p', 'oasis7_distfs', '--lib', '--bin', 'oasis7_durability_endpoint'] + lint_flags]
            distfs_lib_test_variants = (
                [['test', '-p', 'oasis7_distfs', '--lib', '--bin', 'oasis7_durability_endpoint', '--', '--test-threads=1']],
                [
                    ['test', '-p', 'oasis7_distfs', '--lib', '--bin', 'oasis7_durability_endpoint',
                     '--', '--skip', 'controlled_authority::formal::transport::'],
                    ['test', '-p', 'oasis7_distfs', '--lib', 'controlled_authority::formal::transport::',
                     '--', '--test-threads=1'],
                ],
            )
            allowed_base_tests = tuple(
                [cas_cmd, *distfs_cmds]
                for cas_cmd in (
                    ['test', '-p', 'oasis7_cas', '-p', 'oasis7_formal_protocol', '--lib', '--', '--test-threads=1'],
                    ['test', '-p', 'oasis7_cas', '-p', 'oasis7_formal_protocol', '--lib'],
                )
                for distfs_cmds in distfs_lib_test_variants
            )
            target_args = ['-p', 'oasis7_distfs', '--test', 'incremental_industry_jobs']
            named_lint = ['clippy', *target_args, *lint_flags]
            merged_distfs_lint = [
                'clippy', '-p', 'oasis7_distfs', '--lib', '--bin', 'oasis7_durability_endpoint',
                '--test', 'incremental_industry_jobs', *lint_flags]
            allowed_named_tests = (
                ['test', *target_args, '--', '--test-threads=1'],
                ['test', *target_args],
            )
            for present in (False, True):
                if present:
                    named.write_text('#[test] fn industry_contract() {}\n')
                subprocess.run(['git', '-C', str(repo), 'add', '.'], check=True)
                subprocess.run(['git', '-C', str(repo), '-c', 'user.name=CI fixture', '-c',
                                'user.email=fixture@example.invalid', 'commit', '-qm', 'fixture'], check=True)
                sha = subprocess.check_output(['git', '-C', str(repo), 'rev-parse', 'HEAD'], text=True).strip()
                selected = DistfsClippyHandoff().selection() | {'source_sha': sha, 'tested_sha': sha}
                selected['architecture'].update(source=sha, tested=sha)
                fast = {key: value for key, value in selected.items()
                        if key not in ('distfs_clippy_contract_version', 'distfs_clippy_group')}
                package = FastExecutionSelection().selection(('oasis7_runtime', 'oasis7_world_state'))
                package['groups'].append('distfs')
                package.update(source_sha=sha, tested_sha=sha)
                package['architecture'].update(source=sha, tested=sha)
                industry = self.selection([FastTargetInventory().target(
                    package='oasis7', target_name='incremental_industry_jobs')])
                industry.update(source_sha=sha, tested_sha=sha)
                industry['architecture'].update(source=sha, tested=sha)
                # Explicit --plan stays authoritative; the env-only handoff is
                # consumed solely by the new industry check, not by old lint routing.
                cases = [
                    ('handoff', selected, None, False), ('fast', fast, None, False),
                    ('package', package, None, True), ('full', package | {'scope': 'full'}, None, True),
                    ('policy', industry | {'test_policy_changed': True}, None, True),
                    ('industry', industry, None, True), ('legacy', {}, None, True), ('manual', None, None, True),
                    ('env-fast', None, fast, False), ('env-package', None, package, True),
                    ('env-full', None, package | {'scope': 'full'}, True),
                    ('explicit-skip', fast, industry, False), ('explicit-run', industry, fast, True),
                    ('explicit-invalid-env', industry, {'fast_plan_version': True}, True),
                    ('full-tier', None, fast | {'source_sha': '0' * 40}, True),
                ]
                for mode, current, environment_plan, run_named in cases:
                    selection.write_text(json.dumps(current))
                    env_selection.write_text(json.dumps(environment_plan))
                    for failed in (('', 'clippy', 'test') if present and run_named else ('',)):
                        for group in (('rust_baseline', 'distfs') if mode in ('handoff', 'fast') else ('distfs',)):
                            log.unlink(missing_ok=True)
                            plan_args = [] if current is None else ['--plan', str(selection)]
                            command = ['bash', str(HERE / 'ci-tests.sh'), 'required', '--group', group,
                                       *plan_args, '--repo-root', str(repo)]
                            if mode == 'full-tier':
                                # Exercise the sourced full-tier entry without
                                # invoking unrelated full-suite product groups.
                                command = ['bash', '-c', '''set -euo pipefail
source "$1/ci-tests.sh"
driver_dir="$1"; repo_root="$2"; ci_python="$3"; tier=full; required_plan=""
cd "$repo_root"
run_oasis7_distfs_clippy
run_oasis7_distfs_tests
''', 'fixture', str(HERE), str(repo), sys.executable]
                            result = subprocess.run(
                                command, text=True, capture_output=True,
                                env=dict(os.environ, PATH=str(binary) + os.pathsep + os.environ['PATH'],
                                         SOURCE_SHA=sha, CI_VERBOSE='', CARGO_LOG=str(log),
                                         FAIL_NAMED_COMMAND=failed,
                                         OASIS7_CI_DISTFS_PLAN_FILE=str(env_selection) if environment_plan is not None else '',
                                         OASIS7_CI_RUN_PROVIDER_LIVE_GATE='false',
                                         OASIS7_CI_RUN_HOSTED_ACCOUNT_SMOKE='false'))
                            commands = [json.loads(line) for line in log.read_text().splitlines()]
                            observed_lints = [args for args in commands if args[0] == 'clippy'
                                              and args[2] in ('oasis7_cas', 'oasis7_distfs')]
                            owner = 'rust_baseline' if mode == 'handoff' else 'distfs'
                            expected_lints = list(original_lints) if group == owner else []
                            if present and run_named and group == 'distfs':
                                expected_lints.append(named_lint)
                            expected_lint_variants = [expected_lints]
                            if present and run_named and group == 'distfs' and owner == 'distfs':
                                expected_lint_variants.append([original_lints[0], merged_distfs_lint])
                            expected_test_variants = [[]]
                            if group == 'distfs' and failed != 'clippy':
                                if present and run_named:
                                    expected_test_variants = [
                                        base + [named_cmd]
                                        for base in allowed_base_tests
                                        for named_cmd in allowed_named_tests
                                    ]
                                else:
                                    expected_test_variants = list(allowed_base_tests)
                            expected_status = (29 if failed == 'clippy' else 23 if failed == 'test' else 0)
                            with self.subTest(present=present, mode=mode, failed=failed, group=group):
                                self.assertEqual(result.returncode, expected_status if group == 'distfs' else 0,
                                                 result.stdout + result.stderr)
                                self.assertIn(observed_lints, expected_lint_variants)
                                self.assertIn([args for args in commands if args[0] == 'test'], expected_test_variants)
                if not present:
                    continue
                invalid = [industry | {'ordinary_fast_targets': []},
                           {key: value for key, value in industry.items() if key != 'ordinary_fast_targets'},
                           industry | {'source_sha': '0' * 40}, industry | {'tested_sha': '0' * 40}]
                malformed = copy.deepcopy(industry)
                malformed['ordinary_fast_targets'][0]['cases'] = []
                invalid.append(malformed)
                for current in invalid:
                    for via_env in (False, True):
                        selection.write_text(json.dumps(current))
                        log.unlink(missing_ok=True)
                        result = subprocess.run(
                            ['bash', str(HERE / 'ci-tests.sh'), 'required', '--group', 'distfs',
                             *([] if via_env else ['--plan', str(selection)]), '--repo-root', str(repo)],
                            text=True, capture_output=True,
                            env=dict(os.environ, PATH=str(binary) + os.pathsep + os.environ['PATH'],
                                     SOURCE_SHA=sha, CI_VERBOSE='', CARGO_LOG=str(log), FAIL_NAMED_COMMAND='',
                                     OASIS7_CI_DISTFS_PLAN_FILE=str(selection) if via_env else '',
                                     OASIS7_CI_RUN_PROVIDER_LIVE_GATE='false',
                                     OASIS7_CI_RUN_HOSTED_ACCOUNT_SMOKE='false'))
                        commands = [json.loads(line) for line in log.read_text().splitlines()] if log.exists() else []
                        with self.subTest(plan=current, via_env=via_env):
                            self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
                            self.assertNotIn('industry integration is not selected', result.stdout)
                            self.assertNotIn(named_lint, commands)
                            self.assertEqual([args for args in commands if args[0] == 'test'], [])
                            if current.get('source_sha') != sha or current.get('tested_sha') != sha:
                                self.assertIn('execution plan identity mismatch', result.stderr)


class CompleteArtifactPartitions(unittest.TestCase):
    def selection(self):
        return FastExecutionSelection().selection() | dict(
            scope='full', ordinary_artifact_partition_version=1,
            ordinary_partitions=['controls', 'runtime', 'component-libs', 'integrations-0', 'integrations-1'])

    def policy_selection(self):
        return self.selection() | dict(scope='targeted', test_policy_changed=True,
            oasis7_required_packages=['oasis7', 'oasis7_chain_runtime', 'oasis7_viewer_runtime'],
            oasis7_required_wasm_packages=['oasis7_viewer_runtime'])

    def manifest(self, selected):
        binaries = {}
        for name, package, kind, variant in [
            ('runtime-tests', 'oasis7_chain_runtime', 'lib', 'required'),
            ('oasis7-lib-tests', 'oasis7', 'lib', 'required'),
            ('component', 'oasis7_runtime', 'lib', 'required'),
            ('new-integration', 'oasis7_runtime', 'test', 'required'),
            ('supplement', 'oasis7', 'lib', 'required-full-fast-0'),
            ('empty', 'oasis7', 'test', 'required')]:
            binaries[name] = dict(package=package, target=package, target_kind=[kind],
                                  variant=variant, ordinary=True, tests=[] if name == 'empty' else ['actual_behavior'],
                                  ignored_tests=[], path=name, cwd='.')
        for owner, pattern in artifacts.DEFAULT_SNAPSHOT_REGRESSIONS:
            binaries[owner] = dict(package='oasis7', target='oasis7', target_kind=['lib'],
                                   variant=owner, ordinary=False, tests=[pattern + 'regression'],
                                   ignored_tests=[], path=owner, cwd='.')
        return dict(scope='ordinary', execution_plan=selected, binaries=binaries)

    def test_complete_partition_union_keeps_variants_snapshots_and_new_harnesses(self):
        selected = self.selection()
        manifest = self.manifest(selected)
        groups = {partition: artifacts.ordinary_artifact_entries(manifest, selected, partition)
                  for partition in selected['ordinary_partitions']}
        self.assertEqual(groups['controls'], [])
        self.assertEqual([entry['binary'] for entry in groups['runtime']], ['runtime-tests'])
        self.assertEqual([entry['binary'] for entry in groups['component-libs']], ['component', 'oasis7-lib-tests', 'supplement'])
        self.assertEqual(len(groups['integrations-0']), 2)
        self.assertEqual(len(groups['integrations-1']), 1)
        self.assertCountEqual([entry for values in groups.values() for entry in values],
                              artifacts.ordinary_entries(manifest))
        reordered = copy.deepcopy(manifest)
        reordered['binaries'] = dict(reversed(list(reordered['binaries'].items())))
        reordered['binaries']['new-integration']['tests'].append('new_case_same_complete_harness')
        for partition in selected['ordinary_partitions']:
            self.assertEqual(artifacts.ordinary_artifact_entries(reordered, selected, partition), groups[partition])
        original = {key: value for key, value in selected.items()
                    if key not in ('ordinary_artifact_partition_version', 'ordinary_partitions')}
        self.assertEqual(artifacts.build_variants('ordinary', selected), artifacts.build_variants('ordinary', original))
        with self.assertRaises(ValueError):
            artifacts.build_variants('ordinary', selected, ordinary_partition='runtime')

    def test_complete_partition_rejects_partial_unknown_and_inline_capabilities(self):
        for key, value in [('ordinary_artifact_partition_version', True),
                           ('ordinary_artifact_partition_version', 2),
                           ('ordinary_partitions', ['runtime']), ('ordinary_partitions', ['all']),
                           ('ordinary_partitions', ['runtime'] * 5), ('scope', 'targeted'),
                           ('fast_execution_version', 1), ('ordinary_partition_version', 1)]:
            with self.subTest(key=key, value=value), self.assertRaises(ValueError):
                artifacts.ordinary_partitions(self.selection() | {key: value})
        for field in ('ordinary_artifact_partition_version', 'ordinary_partitions', 'fast_plan_version'):
            selected = self.selection()
            del selected[field]
            with self.subTest(missing=field), self.assertRaises(ValueError):
                artifacts.ordinary_partitions(selected)

    def test_targeted_policy_partitions_preserve_build_features_and_complete_inventory(self):
        for profile in ('pr-fast', 'main-fast'):
            selected = self.policy_selection() | dict(execution_profile=profile, ordinary_artifact_delivery_version=1)
            manifest = self.manifest(selected)
            manifest['binaries']['supplement'].update(package='oasis7_viewer_runtime',
                target='oasis7_viewer_runtime', variant='required', tests=['required_behavior', 'new_full_behavior'])
            with self.subTest(profile=profile):
                partitions = artifacts.ordinary_partitions(selected)
                groups = {owner: artifacts.ordinary_artifact_entries(manifest, selected, owner) for owner in partitions}
                self.assertEqual(groups['controls'], [])
                self.assertTrue(all(groups[owner] for owner in partitions[1:]))
                self.assertCountEqual([entry for entries in groups.values() for entry in entries],
                                      artifacts.ordinary_entries(manifest))
                original = {key: value for key, value in selected.items() if key not in (
                    'ordinary_artifact_partition_version', 'ordinary_partitions', 'ordinary_artifact_delivery_version')}
                self.assertEqual(artifacts.build_variants('ordinary', selected),
                                 artifacts.build_variants('ordinary', original))
                self.assertEqual(artifacts.required_native_commands(selected), artifacts.required_native_commands(original))
                self.assertEqual(artifacts.snapshot_regressions(manifest), artifacts.DEFAULT_SNAPSHOT_REGRESSIONS)
                required = next(args for name, args, _ in artifacts.build_variants('ordinary', selected) if name == 'required')
                self.assertIn('oasis7_chain_runtime/test_tier_full', required[required.index('--features') + 1].split(','))

    def test_targeted_policy_partition_consumer_rejects_incomplete_or_mixed_plans(self):
        original = self.policy_selection()
        changes = [
            ('oasis7_required_packages', [package for package in original['oasis7_required_packages'] if package != missing])
            for missing in original['oasis7_required_packages']
        ] + [
            ('oasis7_required_packages', None), ('oasis7_required_packages', 'oasis7'),
            ('test_policy_changed', False), ('test_policy_changed', 1),
            ('execution_profile', 'candidate-full'), ('execution_profile', 'periodic-full'),
            ('ordinary_fast_targets', []), ('ordinary_full_fast_targets', []),
            ('ordinary_execution', 'inline'), ('fast_execution_version', 1), ('ordinary_partition_version', 1),
            ('ordinary_artifact_partition_version', 2), ('ordinary_artifact_delivery_version', 2),
            ('ordinary_partitions', ['all']), ('ordinary_partitions', ['controls', 'runtime']),
            ('ordinary_partitions', ['controls', 'runtime', 'component-libs', 'integrations-0', 'integrations-0']),
            ('groups', ['baseline']),
        ]
        for key, value in changes:
            with self.subTest(key=key, value=value), self.assertRaises(ValueError):
                artifacts.ordinary_partitions(original | {key: value})
        for key in ('test_policy_changed', 'oasis7_required_packages'):
            selected = dict(original)
            selected.pop(key)
            with self.subTest(missing=key), self.assertRaises(ValueError):
                artifacts.ordinary_partitions(selected)

    def test_full_parent_still_executes_formal_and_native_failure_is_not_hidden(self):
        selected = self.selection()
        manifest = self.manifest(selected)
        formal = dict(scope='formal', formal_execution_version=1, binaries=manifest['binaries'])
        with tempfile.TemporaryDirectory() as temporary, \
             patch.object(artifacts, 'load', return_value=(formal, {})) as load, \
             patch.object(artifacts, 'selected_entries', return_value=[dict(binary='runtime-tests', name='case', ignored=False, suite='runtime-protocol')]), \
             patch.object(artifacts, 'execute_inline_jobs') as pool:
            artifacts.execute(Path(temporary), None, Path(temporary) / 'results', shard='runtime-protocol', plan=selected)
            pool.assert_called_once()
            self.assertEqual(pool.call_args.args[0][0]['suite'], 'runtime-protocol')
        with tempfile.TemporaryDirectory() as temporary, \
             patch.object(artifacts, 'load', return_value=(manifest, {'OASIS7_FORMAL_TEST': 'must-be-removed'})) as load, \
             patch('ci_process_runner.run', side_effect=lambda command, cwd, env, suite, name, results, expected_count=None, **kwargs:
                   parallel_native_receipt(suite, name, results, expected_count, passed=False, exit_code=101, count_ok=False)) as runner:
            with self.assertRaises(RuntimeError):
                artifacts.execute(Path(temporary), 'ordinary', Path(temporary) / 'results', plan=selected, ordinary_partition='runtime')
            self.assertEqual(runner.call_count, 1)
            self.assertNotIn('OASIS7_FORMAL_TEST', runner.call_args.args[2])
            self.assertNotIn('ordinary_partition', load.call_args.kwargs)

    def test_unpartitioned_complete_execution_runs_every_harness_and_propagates_failure(self):
        selected = self.selection()
        manifest = self.manifest(selected)
        for passed in (True, False):
            with self.subTest(passed=passed), tempfile.TemporaryDirectory() as temporary, \
                 patch.object(artifacts, 'load', return_value=(manifest, {})) as load, \
                 patch('ci_process_runner.run', return_value=dict(passed=passed, cleanup_result='clean')) as runner:
                def execute():
                    artifacts.execute(Path(temporary), 'ordinary', Path(temporary) / 'results', plan=selected)
                if passed:
                    execute()
                else:
                    with self.assertRaises(RuntimeError):
                        execute()
                self.assertCountEqual([call.args[4] for call in runner.call_args_list],
                    ['runtime-tests', 'oasis7-lib-tests', 'component', 'new-integration', 'supplement',
                     *[pattern for _, pattern in artifacts.DEFAULT_SNAPSHOT_REGRESSIONS]])
                self.assertNotIn('ordinary_partition', load.call_args.kwargs)
        for partition in ('controls', 'all', 'unknown'):
            with self.subTest(partition=partition), patch.object(artifacts, 'load') as load:
                with self.assertRaises(ValueError):
                    artifacts.execute(Path('.'), 'ordinary', Path('results'), plan=selected,
                                      ordinary_partition=partition)
                load.assert_not_called()

    def test_environment_batch_destinations_are_exact_and_nonempty(self):
        fixture = Coverage()
        fixture.setUp()
        selected = FastExecutionSelection().selection(suites=('runtime-protocol',))
        self.assertEqual(set(artifacts.formal_shards(selected)), {'runtime-protocol', 'healthy-set', 'lifecycle-cli'})
        selected['formal_shards'] = artifacts.formal_shards(selected)
        manifest = dict(execution_plan=selected, suites=artifacts.resolve_suites(fixture.config, fixture.binaries))
        combined = []
        for shard in selected['formal_shards']:
            entries = artifacts.selected_entries(manifest, fixture.config, shard=shard)
            self.assertTrue(entries, shard)
            self.assertTrue(all(entry['suite'] == 'runtime-protocol' for entry in entries))
            combined.extend(entries)
        self.assertEqual(len(combined), len(manifest['suites']['runtime-protocol']))
        self.assertEqual(len({(entry.get('binary'), entry.get('name'), entry.get('script')) for entry in combined}), len(combined))


class OrdinaryExclusiveExecution(unittest.TestCase):
    def manifest(self, selected=None):
        selected = selected or CompleteArtifactPartitions().selection()
        manifest = CompleteArtifactPartitions().manifest(selected)
        self.cases = sorted(artifacts.ORDINARY_EXCLUSIVE_RUNTIME_CASES)
        manifest['binaries']['runtime-tests']['tests'] = [
            *self.cases, self.cases[0] + '_similar', 'unrelated', 'ignored']
        manifest['binaries']['runtime-tests']['ignored_tests'] = ['ignored']
        return manifest

    def test_complete_inventory_is_partitioned_once_with_exact_skips(self):
        manifest = self.manifest()
        entries = [entry for entry in artifacts.expand_ordinary_batches(
                   artifacts.ordinary_entries(manifest), manifest['binaries'])
                   if entry['binary'] == 'runtime-tests']
        self.assertEqual(len(entries), 3)
        residual = next(entry for entry in entries if entry.get('batch'))
        self.assertEqual(residual['skip_exact'], self.cases)
        singles = [entry for entry in entries if not entry.get('batch')]
        self.assertEqual([entry['name'] for entry in singles], self.cases)
        self.assertTrue(all(entry['exclusive'] and not entry['ignored'] for entry in singles))
        covered = set(manifest['binaries']['runtime-tests']['tests']) - {'ignored'}
        self.assertEqual((covered - set(residual['skip_exact'])) | {entry['name'] for entry in singles}, covered)
        self.assertFalse((covered - set(residual['skip_exact'])) & {entry['name'] for entry in singles})
        selected = manifest['execution_plan']
        groups = {part: artifacts.ordinary_artifact_entries(manifest, selected, part)
                  for part in selected['ordinary_partitions']}
        self.assertEqual(artifacts.expand_ordinary_batches(groups['runtime'], manifest['binaries']), entries)
        self.assertCountEqual([entry for group in groups.values() for entry in group],
                              artifacts.ordinary_entries(manifest))

    def test_deferred_cases_stay_out_of_exclusive_singletons(self):
        manifest = self.manifest()
        entry = dict(binary='runtime-tests', batch=True, skip=[self.cases[0]])
        expanded = artifacts.expand_ordinary_batches([entry], manifest['binaries'])
        self.assertEqual([item['name'] for item in expanded if not item.get('batch')],
                         [self.cases[1]])
        residual = next(item for item in expanded if item.get('batch'))
        self.assertEqual(residual['skip'], [self.cases[0]])
        self.assertEqual(residual['skip_exact'], [self.cases[1]])
        self.assertEqual(residual['count'], 2)

    def test_exact_skips_survive_exclusive_splitting_alongside_deferred_cases(self):
        manifest = self.manifest()
        entry = dict(binary='runtime-tests', batch=True, skip=[self.cases[0]], skip_exact=['unrelated'])
        expanded = artifacts.expand_ordinary_batches([entry], manifest['binaries'])
        self.assertEqual([item['name'] for item in expanded if not item.get('batch')], [self.cases[1]])
        residual = next(item for item in expanded if item.get('batch'))
        self.assertEqual(residual['skip'], [self.cases[0]])
        self.assertEqual(residual['skip_exact'], sorted(['unrelated', self.cases[1]]))
        self.assertEqual(residual['count'], 1)
        self.assertEqual(artifacts.expand_ordinary_batches(expanded, manifest['binaries']), expanded)
        excluded = dict(entry, skip_exact=['unrelated', *self.cases])
        self.assertEqual(artifacts.expand_ordinary_batches([excluded], manifest['binaries']), [excluded])

    def test_absent_ignored_other_targets_and_empty_residual_do_not_lose_tests(self):
        for change in ('absent', 'ignored', 'other-package', 'other-target', 'other-kind', 'only-exclusive'):
            manifest = self.manifest()
            binary = manifest['binaries']['runtime-tests']
            if change == 'absent':
                binary['tests'] = ['unrelated']
                binary['ignored_tests'] = []
            elif change == 'ignored':
                binary['ignored_tests'] += self.cases
            elif change == 'other-package':
                binary['package'] = 'oasis7_runtime'
            elif change == 'other-target':
                binary['target'] = 'integration'
            elif change == 'other-kind':
                binary['target_kind'] = ['test']
            else:
                binary['tests'] = self.cases
                binary['ignored_tests'] = []
            entries = [entry for entry in artifacts.expand_ordinary_batches(
                       artifacts.ordinary_entries(manifest), manifest['binaries'])
                       if entry['binary'] == 'runtime-tests']
            with self.subTest(change=change):
                self.assertEqual(len(entries), 2 if change == 'only-exclusive' else 1)
                self.assertEqual(sum(entry.get('exclusive', False) for entry in entries),
                                 2 if change == 'only-exclusive' else 0)

    def test_ordinary_commands_keep_counts_environment_and_failure(self):
        manifest = self.manifest()
        selected = manifest['execution_plan']
        for fail in (False, True):
            def invoke(command, cwd, env, suite, name, results, expected_count=None, **kwargs):
                self.assertEqual(suite, 'ordinary')
                self.assertNotIn('OASIS7_FORMAL_TEST', env)
                if name == 'runtime-tests':
                    self.assertEqual(command[1:], ['--exact', '--skip', self.cases[0], '--skip', self.cases[1]])
                    self.assertEqual(expected_count, 2)
                else:
                    self.assertIn(name, self.cases)
                    self.assertEqual(command[1:], ['--exact', name, '--test-threads=1'])
                    self.assertEqual(expected_count, 1)
                passed = not (fail and name == self.cases[0])
                return parallel_native_receipt(
                    suite, name, results, expected_count,
                    passed=passed, exit_code=0 if passed else 101, count_ok=passed,
                )
            with tempfile.TemporaryDirectory() as temporary, \
                 patch.object(artifacts, 'load', return_value=(manifest, {'OASIS7_FORMAL_TEST': 'remove'})), \
                 patch('ci_process_runner.run', side_effect=invoke) as runner:
                if fail:
                    with self.assertRaises(RuntimeError):
                        artifacts.execute(Path(temporary), 'ordinary', Path(temporary) / 'result',
                                          plan=selected, ordinary_partition='runtime')
                else:
                    artifacts.execute(Path(temporary), 'ordinary', Path(temporary) / 'result',
                                      plan=selected, ordinary_partition='runtime')
                self.assertEqual(runner.call_count, 3)

    def test_inline_exclusive_jobs_never_overlap_another_harness(self):
        selected = InlineExecutionSelection().selection()
        manifest = self.manifest(selected)
        lock = threading.Lock()
        active, calls = set(), []
        def invoke(command, cwd, env, suite, name, results, expected_count=None, **kwargs):
            with lock:
                if name in self.cases:
                    self.assertFalse(active)
                    self.assertIn('--test-threads=1', command)
                    self.assertEqual(env['RUST_TEST_THREADS'], '1')
                self.assertFalse(active & set(self.cases))
                active.add(name)
                calls.append(name)
            time.sleep(0.01)
            try:
                return parallel_native_receipt(suite, name, results, expected_count)
            finally:
                with lock:
                    active.remove(name)
        with tempfile.TemporaryDirectory() as temporary, \
             patch.object(artifacts, 'load', return_value=(manifest, {})), \
             patch.object(artifacts, 'native_cpu_budget', return_value=4), \
             patch('ci_process_runner.run', side_effect=invoke):
            artifacts.execute(Path(temporary), 'ordinary', Path(temporary) / 'result', plan=selected)
        self.assertEqual(sum(name in self.cases for name in calls), 2)
        self.assertEqual(len(calls), len(set(calls)))

    def test_real_libtest_exact_skip_preserves_similar_names_and_counts(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source = root / 'fixture.rs'
            source.write_text('''
#[test] fn heavy() {}
#[test] fn other() {}
#[test] fn heavy_suffix() {}
#[test] fn prefix_heavy() {}
#[test] fn unrelated() {}
#[test] #[ignore] fn ignored() { panic!("must remain ignored"); }
''')
            executable = root / 'fixture'
            compile_rust_fixture(source, executable, test=True)
            tests = artifacts.inventory(executable)
            ignored = artifacts.inventory(executable, True)
            manifest = dict(binaries={'runtime': dict(path='fixture', cwd='.', ordinary=True,
                package='oasis7_chain_runtime', target='oasis7_chain_runtime', target_kind=['lib'],
                tests=tests, ignored_tests=ignored)})
            with patch.object(artifacts, 'ROOT', root), \
                 patch.object(artifacts, 'ORDINARY_EXCLUSIVE_RUNTIME_CASES', frozenset(('heavy', 'other'))), \
                 patch.object(artifacts, 'snapshot_regressions', return_value=()), \
                 patch.object(artifacts, 'load', return_value=(manifest, {})):
                artifacts.execute(root, 'ordinary', root / 'results.jsonl')
            records = [json.loads(line) for line in (root / 'results.jsonl').read_text().splitlines()]
            self.assertEqual([(record['test_name'], record['expected_count']) for record in records],
                             [('runtime', 3), ('heavy', 1), ('other', 1)])
            self.assertTrue(all(record['passed'] and record['count_ok'] for record in records))


class TargetedArtifactPartitions(unittest.TestCase):
    full_sync_cases = (
        'formal_full_sync_pins_the_first_verified_target_cut_across_providers',
        'formal_full_sync_rejects_over_budget_material_without_publishing_then_recovers',
        'formal_full_sync_pins_only_objects_the_successful_candidate_persisted',
        'formal_full_sync_shared_budget_accumulates_prior_charges_without_publishing',
    )

    def selection(self):
        return FastExecutionSelection().selection(packages=('oasis7_chain_runtime',)) | dict(
            ordinary_artifact_partition_version=2,
            ordinary_partitions=['controls', 'native-0', 'native-1', 'native-2', 'native-3'])

    def manifest(self, selected=None):
        cases = [f'case_{index:04d}' for index in range(1288)] + list(self.full_sync_cases)
        cases += sorted(artifacts.ORDINARY_EXCLUSIVE_RUNTIME_CASES)
        return dict(scope='ordinary', execution_plan=selected or self.selection(), binaries={
            'runtime-tests': dict(package='oasis7_chain_runtime', target='oasis7_chain_runtime',
                target_kind=['lib'], variant='required', ordinary=True, test=True, path='runtime', cwd='.',
                tests=cases + ['ignored_child'], ignored_tests=['ignored_child']),
            'integration': dict(package='oasis7_chain_runtime', target='real_http',
                target_kind=['test'], variant='required', ordinary=True, test=True, path='integration', cwd='.',
                tests=['real_http_success', 'real_http_refusal', 'real_http_recovery'], ignored_tests=[]),
            'new-integration': dict(package='oasis7_chain_runtime', target='new_fixture',
                target_kind=['test'], variant='required', ordinary=True, test=True, path='new', cwd='.',
                tests=['new_case'], ignored_tests=[]),
        })

    @staticmethod
    def costs(rows=()):
        return dict(version=1, fallback_ms=1000, evidence=dict(
            run_id=43179733413, run_attempt=1,
            source_sha='718dee8464b5681082d7ab4ea7d267cd838ead8c',
            tested_sha='40c170e7837b1bf0d30ab877a2a826b37555ba71',
            policy_sha='e2dbafdaf507a1f228c09c1d2d49420f278117ae'), harnesses=list(rows))

    def test_measured_costs_balance_whole_harnesses_without_changing_any_case(self):
        selected = self.selection()
        manifest = self.manifest(selected)
        rows = []
        for index, milliseconds in enumerate((100000, 90000, 80000, 70000, 1000)):
            name = f'cost_{index}'
            manifest['binaries'][name] = dict(package='oasis7_chain_runtime', target=name,
                target_kind=['test'], variant='required', ordinary=True, test=True, path=name, cwd='.',
                tests=[f'case_{i}' for i in range(1000 if index == 4 else 1)], ignored_tests=[])
            rows.append(dict(package='oasis7_chain_runtime', target_kind='test', target_name=name,
                             variant='required', mode='batch', selector='', elapsed_ms=milliseconds))
        selected['ordinary_harness_costs'] = self.costs(rows)
        groups = {owner: artifacts.ordinary_artifact_entries(manifest, selected, owner)
                  for owner in selected['ordinary_partitions'][1:]}
        ownership = {entry['binary']: owner for owner, entries in groups.items() for entry in entries
                     if entry['binary'].startswith('cost_')}
        self.assertEqual(ownership, dict(cost_0='native-0', cost_1='native-1', cost_2='native-2',
                                         cost_3='native-3', cost_4='native-3'))
        actual = [case for entries in groups.values() for case in self.coverage(manifest, entries)]
        self.assertCountEqual(actual, self.coverage(manifest, artifacts.ordinary_entries(manifest)))
        self.assertEqual(len(actual), len(set(actual)))
        reordered = copy.deepcopy(manifest)
        reordered['binaries'] = dict(reversed(list(reordered['binaries'].items())))
        reordered['execution_plan']['ordinary_harness_costs']['harnesses'].reverse()
        for owner, entries in groups.items():
            self.assertEqual(artifacts.ordinary_artifact_entries(
                reordered, reordered['execution_plan'], owner), entries)
        legacy = {key: value for key, value in selected.items() if key != 'ordinary_harness_costs'}
        self.assertEqual(artifacts.build_variants('ordinary', selected), artifacts.build_variants('ordinary', legacy))
        self.assertEqual(artifacts.required_native_commands(selected), artifacts.required_native_commands(legacy))

    def test_cost_profile_rejects_invalid_data_and_incompatible_plans(self):
        row = dict(package='oasis7_chain_runtime', target_kind='test', target_name='real_http',
                   variant='required', mode='batch', selector='', elapsed_ms=100)
        good = self.costs([row])
        invalid = [good | {'version': value} for value in (True, 0, 2, None, '1')]
        invalid += [good | {'fallback_ms': value} for value in (True, 0, -1, 3600001, 1.5, float('nan'))]
        invalid += [good | {'harnesses': [row, row]}, good | {'evidence': {}}]
        for field, values in dict(elapsed_ms=[True, 0, -1, 3600001, 1.5, float('inf')],
                                  mode=['unknown'], selector=['unexpected'], target_kind=['unknown'],
                                  target_name=['../target'], variant=['']).items():
            invalid.extend(good | {'harnesses': [row | {field: value}]} for value in values)
        for profile in invalid:
            with self.subTest(profile=profile), self.assertRaises(ValueError):
                artifacts.ordinary_partitions(self.selection() | {'ordinary_harness_costs': profile})
        for base in (CompleteArtifactPartitions().selection(), {'ordinary_harness_costs': good},
                     self.selection() | {'ordinary_execution': 'inline'}):
            with self.subTest(base=base), self.assertRaises(ValueError):
                artifacts.ordinary_partitions(base | {'ordinary_harness_costs': good})

    def coverage(self, manifest, entries):
        selected = []
        for entry in entries:
            binary = manifest['binaries'][entry['binary']]
            if entry.get('batch'):
                names = set(binary['tests']) - set(binary['ignored_tests'])
                names -= set(entry.get('skip', ())) | set(entry.get('skip_exact', ()))
            elif 'filter' in entry:
                names = {name for name in binary['tests'] if entry['filter'] in name
                         and name not in binary['ignored_tests']}
            else:
                names = {entry['name']}
            selected.extend((entry['binary'], name) for name in names)
        return selected

    def test_complete_runtime_and_other_harness_inventory_has_one_nonempty_owner(self):
        selected = self.selection()
        manifest = self.manifest(selected)
        groups = {owner: artifacts.ordinary_artifact_entries(manifest, selected, owner)
                  for owner in selected['ordinary_partitions']}
        self.assertEqual(groups.pop('controls'), [])
        actual = [item for entries in groups.values() for item in self.coverage(manifest, entries)]
        expected = [('runtime-tests', name) for name in manifest['binaries']['runtime-tests']['tests']
                    if name != 'ignored_child']
        expected += [('integration', name) for name in
                     ('real_http_success', 'real_http_refusal', 'real_http_recovery')]
        expected.append(('new-integration', 'new_case'))
        self.assertCountEqual(actual, expected)
        self.assertEqual(len(actual), len(set(actual)))
        self.assertTrue(all(self.coverage(manifest, entries) for entries in groups.values()))
        self.assertEqual(sum(owner == 'runtime-tests' for owner, _ in actual), 1294)
        self.assertTrue(all(('runtime-tests', name) in actual for name in self.full_sync_cases))
        exclusive = [entry for entries in groups.values() for entry in entries if entry.get('exclusive')]
        self.assertCountEqual([entry['name'] for entry in exclusive], artifacts.ORDINARY_EXCLUSIVE_RUNTIME_CASES)
        self.assertTrue(all(entry.get('batch') is not True for entry in exclusive))
        reordered = copy.deepcopy(manifest)
        reordered['binaries'] = dict(reversed(list(reordered['binaries'].items())))
        for binary in reordered['binaries'].values():
            binary['tests'].reverse()
        for owner, entries in groups.items():
            self.assertEqual(artifacts.ordinary_artifact_entries(reordered, selected, owner), entries)
        manifest['binaries']['runtime-tests']['tests'].append('new_runtime_case')
        updated = [item for owner in groups for item in self.coverage(manifest,
                   artifacts.ordinary_artifact_entries(manifest, selected, owner))]
        self.assertCountEqual(updated, expected + [('runtime-tests', 'new_runtime_case')])

    def test_same_original_producer_features_and_complete_targets_are_required(self):
        selected = self.selection()
        original = {key: value for key, value in selected.items()
                    if key not in ('ordinary_artifact_partition_version', 'ordinary_partitions')}
        self.assertEqual(artifacts.build_variants('ordinary', selected),
                         artifacts.build_variants('ordinary', original))
        self.assertEqual(artifacts.required_native_commands(selected),
                         artifacts.required_native_commands(original))
        command = next(args for name, args, _ in artifacts.build_variants('ordinary', selected)
                       if name == 'required')
        self.assertIn('--tests', command)
        self.assertNotIn('--no-default-features', command)
        self.assertIn('oasis7_chain_runtime/test_tier_required', command)
        for owner in selected['ordinary_partitions']:
            with self.subTest(owner=owner), self.assertRaisesRegex(ValueError, 'cannot rebuild'):
                artifacts.build_variants('ordinary', selected, ordinary_partition=owner)
        self.assertEqual(artifacts.ordinary_partitions(original), ['all'])
        self.assertEqual(artifacts.ordinary_partitions(CompleteArtifactPartitions().selection()),
                         list(artifacts.ORDINARY_ARTIFACT_PARTITIONS))

    def test_capability_rejects_missing_unknown_duplicate_and_incompatible_owners(self):
        changes = [
            ('ordinary_artifact_partition_version', value) for value in (True, None, 0, 3, '2')
        ] + [
            ('ordinary_partitions', []), ('ordinary_partitions', ['all']),
            ('ordinary_partitions', ['controls', 'native-0', 'native-1', 'native-2']),
            ('ordinary_partitions', ['controls', 'native-0', 'native-1', 'native-2', 'native-2']),
            ('ordinary_partitions', ['controls', 'native-0', 'native-1', 'native-2', 'unknown']),
            ('scope', 'full'), ('execution_profile', 'candidate-full'),
            ('test_policy_changed', True), ('test_policy_changed', 0),
            ('oasis7_required_packages', []), ('oasis7_required_packages', None),
            ('groups', ['baseline']), ('fast_execution_version', 1),
            ('ordinary_partition_version', 1), ('ordinary_execution', 'inline'),
            ('ordinary_fast_targets', []), ('ordinary_full_fast_targets', []),
        ]
        for key, value in changes:
            with self.subTest(key=key, value=value), self.assertRaises(ValueError):
                artifacts.ordinary_partitions(self.selection() | {key: value})
        for field in ('ordinary_artifact_partition_version', 'ordinary_partitions', 'fast_plan_version',
                      'test_policy_changed', 'oasis7_required_packages'):
            selected = self.selection()
            del selected[field]
            with self.subTest(missing=field), self.assertRaises(ValueError):
                artifacts.ordinary_partitions(selected)
        for owner in ('all', 'controls', 'runtime', 'unknown', '../native-0'):
            with self.subTest(owner=owner), patch.object(artifacts, 'load') as load:
                with self.assertRaises(ValueError):
                    artifacts.execute(Path('.'), 'ordinary', Path('unused'), plan=self.selection(),
                                      ordinary_partition=owner)
                load.assert_not_called()

    def test_runtime_identity_empty_and_duplicate_inventory_fail_closed(self):
        for change in ('missing', 'package', 'target', 'kind', 'variant', 'duplicate-target',
                       'duplicate-case', 'empty', 'too-small'):
            manifest = self.manifest()
            binary = manifest['binaries']['runtime-tests']
            if change == 'missing':
                del manifest['binaries']['runtime-tests']
            elif change in ('package', 'target', 'variant'):
                binary[change] = 'unrelated'
            elif change == 'kind':
                binary['target_kind'] = ['test']
            elif change == 'duplicate-target':
                manifest['binaries']['second-runtime'] = dict(binary)
            elif change == 'duplicate-case':
                binary['tests'].append(binary['tests'][0])
            else:
                binary['tests'] = [] if change == 'empty' else ['one', 'two', 'three']
                binary['ignored_tests'] = []
            with self.subTest(change=change), self.assertRaises(ValueError):
                artifacts.ordinary_artifact_entries(manifest, self.selection(), 'native-0')

    def test_deferred_cases_stay_deferred_and_unpartitioned_fallback_stays_complete(self):
        selected = self.selection() | dict(formal_full_profiles_only_version=1, formal_shards=[],
            formal_deferred_ordinary_cases={'runtime-tests': sorted(artifacts.ORDINARY_EXCLUSIVE_RUNTIME_CASES)})
        manifest = self.manifest(selected)
        actual = [item for owner in selected['ordinary_partitions'][1:] for item in self.coverage(
                  manifest, artifacts.ordinary_artifact_entries(manifest, selected, owner))]
        expected = self.coverage(manifest, artifacts.ordinary_entries(manifest))
        self.assertCountEqual(actual, expected)
        self.assertFalse(set(name for _, name in actual) & artifacts.ORDINARY_EXCLUSIVE_RUNTIME_CASES)
        calls = []
        with tempfile.TemporaryDirectory() as temporary, \
             patch.object(artifacts, 'load', return_value=(manifest, {})), \
             patch('ci_process_runner.run', side_effect=lambda command, cwd, env, suite, name, *args, **kwargs:
                   calls.append(name) or dict(passed=True, cleanup_result='clean')):
            artifacts.execute(Path(temporary), 'ordinary', Path(temporary) / 'results', plan=selected)
        self.assertCountEqual(calls, ['runtime-tests', 'integration', 'new-integration'])


def native_package_partition_selection():
    targets = [FastTargetInventory().target(package=package, target_kind=kind, target_name=name)
               for package in ('oasis7_runtime', 'oasis7_world_state')
               for kind, name in (('lib', package), ('test', 'pr_fast'))]
    return InlineExecutionSelection().selection() | {
        'ordinary_fast_targets': targets, 'ordinary_partition_version': 1,
        'ordinary_partitions': ['oasis7_runtime', 'oasis7_world_state'],
        'groups': ['baseline', 'rust_baseline', 'oasis7_required'],
        'oasis7_required_packages': ['oasis7_runtime', 'oasis7_world_state'],
        'ordinary_clippy_group': 'rust_baseline', 'oasis7_required_wasm_packages': [],
    }


def parallel_native_mixed_selection():
    selected = InlineExecutionSelection().selection() | {
        'ordinary_fast_targets': [FastTargetInventory().target(target_name='case_' + str(index))
                                  for index in range(5)],
    }
    for index, package in ((1, 'oasis7_viewer_codec'), (3, 'oasis7_runtime')):
        selected['ordinary_fast_targets'][index].update(
            package=package, target_kind='lib', target_name=package)
    return selected


def parallel_native_receipt(suite, name, results, count, **changes):
    result = dict(suite=suite, test_name=name, expected_count=count, count_ok=True,
                  passed=True, cleanup_result='clean', cancelled=False, timeout=False,
                  exit_code=0, leaked_processes=False) | changes
    results.parent.mkdir(parents=True, exist_ok=True)
    with results.open('a') as stream:
        stream.write(json.dumps(result) + '\n')
    return result


class DriverCoverage(unittest.TestCase):
    def test_targeted_native_driver_keeps_one_controls_owner_and_rejects_unknown_before_cargo(self):
        root = HERE.parent
        sha = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, text=True).strip()
        selected = TargetedArtifactPartitions().selection() | dict(
            source_sha=sha, tested_sha=sha, policy_sha=sha, architecture=dict(source=sha, tested=sha),
            oasis7_required_packages=['oasis7_chain_runtime', 'oasis7_viewer_codec'],
            oasis7_required_wasm_packages=['oasis7_viewer_codec'])
        with tempfile.TemporaryDirectory() as temporary:
            temporary = Path(temporary)
            log, selection = temporary / 'calls', temporary / 'selection.json'
            selection.write_text(json.dumps(selected))
            cargo = temporary / 'cargo'
            cargo.write_text('''#!/usr/bin/env bash
set -euo pipefail
args_json=""; sep=""; first_arg=""
for arg in "$@"; do
  [[ "$arg" == "--verbose" ]] && continue
  [[ -n "$first_arg" ]] || first_arg="$arg"
  s="${arg//\\\\/\\\\\\\\}"; s="${s//\\"/\\\\\\"}"
  args_json+="${sep}\\"${s}\\""; sep=", "
done
printf '[%s]\\n' "$args_json" >> "$CARGO_CALLS"
[[ -z "${FAIL_CARGO:-}" || "$FAIL_CARGO" != "$first_arg" ]] || exit 19
''')
            cargo.chmod(0o755)
            environment = dict(os.environ, PATH=str(temporary) + os.pathsep + os.environ['PATH'],
                CARGO_CALLS=str(log), SOURCE_SHA=sha, CI_VERBOSE='1',
                OASIS7_CI_TEST_BUNDLE=str(temporary / 'missing-bundle'))
            for owner, failure in [('controls', ''), ('controls', 'clippy'), ('controls', 'check'),
                                   *[(owner, '') for owner in selected['ordinary_partitions'][1:]],
                                   ('all', ''), ('unknown', ''), ('native-4', '')]:
                log.unlink(missing_ok=True)
                result = subprocess.run(['bash', str(HERE / 'ci-tests.sh'), 'required',
                    '--group', 'oasis7_required', '--plan', str(selection), '--ordinary-partition', owner,
                    '--repo-root', str(root)], cwd=root, env=environment | {'FAIL_CARGO': failure},
                    capture_output=True, text=True)
                calls = [json.loads(line) for line in log.read_text().splitlines()] if log.exists() else []
                with self.subTest(owner=owner, failure=failure):
                    if owner == 'controls':
                        self.assertEqual(result.returncode, 19 if failure else 0, result.stderr)
                        self.assertEqual([call[0] for call in calls],
                                         ['clippy'] if failure == 'clippy' else ['clippy', 'check'])
                        self.assertIn('oasis7_chain_runtime', calls[0])
                        self.assertIn('oasis7_viewer_codec', calls[0])
                        if len(calls) == 2:
                            self.assertIn('wasm32-unknown-unknown', calls[1])
                    else:
                        self.assertNotEqual(result.returncode, 0)
                        self.assertEqual(calls, [])
                        if owner in selected['ordinary_partitions']:
                            self.assertIn('manifest.json', result.stderr)
                        else:
                            self.assertIn('unknown complete ordinary artifact partition', result.stderr)
                            self.assertNotIn('manifest.json', result.stderr)

    def test_complete_artifact_cli_controls_run_once_and_consumers_never_call_cargo(self):
        for selected in (CompleteArtifactPartitions().selection(), CompleteArtifactPartitions().policy_selection()):
            with self.subTest(scope=selected['scope'], policy=selected['test_policy_changed']):
                self.assert_complete_artifact_cli_controls(selected)

    def assert_complete_artifact_cli_controls(self, selected):
        root = HERE.parent
        sha = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, text=True).strip()
        selected['architecture'] = dict(source=sha, tested=sha)
        with tempfile.TemporaryDirectory() as temporary:
            temporary = Path(temporary)
            executable = temporary / 'cargo'
            executable.write_text('''#!/usr/bin/env bash
set -euo pipefail
args_json=""; sep=""; first_arg=""
for arg in "$@"; do
  [[ "$arg" == "--verbose" ]] && continue
  [[ -n "$first_arg" ]] || first_arg="$arg"
  s="${arg//\\\\/\\\\\\\\}"; s="${s//\\"/\\\\\\"}"
  args_json+="${sep}\\"${s}\\""; sep=", "
done
printf '[%s]\\n' "$args_json" >> "$CARGO_CALLS"
[[ -z "${FAIL_CARGO:-}" || "$FAIL_CARGO" != "$first_arg" ]] || exit 19
''')
            executable.chmod(0o755)
            selection = temporary / 'selection.json'
            selection.write_text(json.dumps(selected))
            log = temporary / 'calls'
            environment = dict(os.environ, PATH=str(temporary) + os.pathsep + os.environ['PATH'],
                               CARGO_CALLS=str(log), SOURCE_SHA=sha,
                               OASIS7_CI_TEST_BUNDLE=str(temporary / 'missing-bundle'))
            def invoke(partition, failure=''):
                log.unlink(missing_ok=True)
                result = subprocess.run(['bash', str(HERE / 'ci-tests.sh'), 'required',
                    '--group', 'oasis7_required', '--plan', str(selection), '--ordinary-partition', partition,
                    '--repo-root', str(root)], cwd=root, env=environment | {'FAIL_CARGO': failure},
                    capture_output=True, text=True)
                calls = [json.loads(line) for line in log.read_text().splitlines()] if log.exists() else []
                return result, calls
            # Hosted governance enables verbosity. Cargo accepts it before the
            # subcommand as well as after clippy; neither form changes dispatch.
            for verbose in ('', '1'):
                environment['CI_VERBOSE'] = verbose
                with self.subTest(verbose=verbose):
                    result, calls = invoke('controls')
                    self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                    self.assertEqual([call[0] for call in calls], ['clippy', 'check'])
                    self.assertIn('-D', calls[0])
                    self.assertIn('clippy::correctness', calls[0])
                    self.assertIn('wasm32-unknown-unknown', calls[1])
                    if selected.get('test_policy_changed') is True:
                        self.assertIn('oasis7_chain_runtime/test_tier_full',
                                      calls[0][calls[0].index('--features') + 1].split(','))
                        self.assertEqual([calls[0][index + 1] for index, arg in enumerate(calls[0]) if arg == '-p'],
                                         selected['oasis7_required_packages'])
                        self.assertEqual([calls[1][index + 1] for index, arg in enumerate(calls[1]) if arg == '-p'],
                                         ['oasis7_viewer_runtime'])
                    for failure, expected in [('clippy', ['clippy']), ('check', ['clippy', 'check'])]:
                        result, calls = invoke('controls', failure)
                        self.assertEqual(result.returncode, 19)
                        self.assertEqual([call[0] for call in calls], expected)
                        self.assertNotIn('manifest.json', result.stderr)
                    for partition in artifacts.ORDINARY_ARTIFACT_PARTITIONS[1:]:
                        result, calls = invoke(partition)
                        self.assertNotEqual(result.returncode, 0)
                        self.assertEqual(calls, [])
                        self.assertIn('manifest.json', result.stderr)

    def test_partition_driver_rejects_explicit_all_for_a_split_parent_before_consuming_artifacts(self):
        sha = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=artifacts.ROOT, text=True).strip()
        plan = native_package_partition_selection() | {
            'source_sha': sha, 'tested_sha': sha, 'policy_sha': sha,
            'architecture': dict(source=sha, tested=sha)}
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            selection = root / 'selection.json'
            selection.write_text(json.dumps(plan))
            for partition in ('all', 'unknown'):
                result = subprocess.run(['bash', str(HERE / 'ci-tests.sh'), 'required',
                    '--group', 'oasis7_required', '--plan', str(selection),
                    '--ordinary-partition', partition, '--repo-root', str(artifacts.ROOT)],
                    text=True, capture_output=True,
                    env=dict(os.environ, SOURCE_SHA=sha, OASIS7_CI_TEST_BUNDLE=str(root / 'missing-bundle')))
                with self.subTest(partition=partition):
                    self.assertNotEqual(result.returncode, 0)
                    self.assertIn('unknown ordinary partition', result.stderr)
                    self.assertNotIn('missing-bundle/manifest.json', result.stderr)

    def test_whole_cli_assigns_exact_strict_clippy_commands_once_and_preserves_failures(self):
        # Keep the actual entrypoint, trusted-plan parser and command builder.
        # Stub only unrelated baseline executables and the expensive Cargo process.
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            repo, binary = root / 'repo', root / 'bin'
            scripts = repo / 'scripts'
            scripts.mkdir(parents=True)
            binary.mkdir()
            (scripts / 'cargo-dev-architecture.py').write_text('print("architecture fixture")\n')
            (scripts / 'check-rust-hygiene.py').write_text('print("rust hygiene fixture")\n')
            for name in ('check-rustsec-ignore-baseline.test.sh', 'check-rustsec-ignore-baseline.sh',
                         'ensure-cargo-deny.sh', 'check-rust-file-size.test.sh', 'check-rust-file-size.sh',
                         'check-standalone-tool-lockfiles.test.sh', 'check-standalone-tool-lockfiles.sh'):
                path = scripts / name
                path.write_text('#!/usr/bin/env bash\nexit 0\n')
                path.chmod(0o755)
            cargo = binary / 'cargo'
            cargo.write_text('''#!/usr/bin/env bash
set -euo pipefail
args_json=""; sep=""
for arg in "$@"; do
  s="${arg//\\\\/\\\\\\\\}"; s="${s//\\"/\\\\\\"}"
  args_json+="${sep}\\"${s}\\""; sep=", "
done
printf '[%s]\\n' "$args_json" >> "$CARGO_LOG"
if [[ "${1:-}" == "clippy" && -n "${FAIL_CLIPPY_PACKAGE:-}" ]]; then
  args=("$@")
  for (( i = 0; i + 1 < ${#args[@]}; i++ )); do
    if [[ "${args[$i]}" == "-p" ]]; then
      [[ "${args[$((i + 1))]}" != "$FAIL_CLIPPY_PACKAGE" ]] || exit 19
      break
    fi
  done
fi
''')
            cargo.chmod(0o755)
            subprocess.run(['git', 'init', '-q', str(repo)], check=True)
            subprocess.run(['git', '-C', str(repo), 'add', '.'], check=True)
            subprocess.run(['git', '-C', str(repo), '-c', 'user.name=CI fixture', '-c',
                            'user.email=fixture@example.invalid', 'commit', '-qm', 'fixture'], check=True)
            sha = subprocess.check_output(['git', '-C', str(repo), 'rev-parse', 'HEAD'], text=True).strip()
            selected = InlineExecutionSelection().selection() | {
                'source_sha': sha, 'tested_sha': sha, 'architecture': {'source': sha, 'tested': sha},
                'groups': ['baseline', 'rust_baseline', 'oasis7_required'],
                'ordinary_clippy_group': 'rust_baseline', 'oasis7_required_wasm_packages': [],
                'ordinary_fast_targets': [FastTargetInventory().target(),
                    FastTargetInventory().target(package='oasis7_runtime', target_kind='lib', target_name='oasis7_runtime')]}
            path = root / 'selection.json'
            log = root / 'cargo.jsonl'
            expected = [['clippy', *command, '--', '-D', 'warnings', '-D', 'clippy::correctness',
                         '-D', 'clippy::suspicious'] for command in artifacts.required_native_commands(selected)]
            for mode, failed in (('parallel', ''), ('legacy-inline', ''), ('unknown-version', ''),
                                 ('parallel', 'oasis7_viewer_codec'), ('parallel', 'oasis7_runtime')):
                current = dict(selected)
                if mode == 'legacy-inline':
                    del current['ordinary_clippy_group']
                elif mode == 'unknown-version':
                    current['fast_execution_version'] = 2
                path.write_text(json.dumps(current))
                lint_calls = {}
                for group in ('rust_baseline', 'oasis7_required'):
                    log.unlink(missing_ok=True)
                    result = subprocess.run(['bash', str(HERE / 'ci-tests.sh'), 'required', '--group', group,
                                             '--plan', str(path), '--repo-root', str(repo)], text=True, capture_output=True,
                        env=dict(os.environ, PATH=str(binary) + os.pathsep + os.environ['PATH'],
                                 SOURCE_SHA=sha, CI_VERBOSE='', CARGO_LOG=str(log), FAIL_CLIPPY_PACKAGE=failed,
                                 OASIS7_CI_TEST_BUNDLE=str(root / 'missing-native-bundle'),
                                 OASIS7_CI_RUN_PROVIDER_LIVE_GATE='false', OASIS7_CI_RUN_HOSTED_ACCOUNT_SMOKE='false'))
                    commands = [json.loads(line) for line in log.read_text().splitlines()] if log.exists() else []
                    lint_calls[group] = [args for args in commands if args[0] == 'clippy']
                    with self.subTest(mode=mode, failed=failed, group=group):
                        if group == 'rust_baseline':
                            self.assertEqual(result.returncode, 19 if failed else 0, result.stdout + result.stderr)
                        else:
                            # Reaching the real artifact reader proves lint deferral
                            # never skips the test consumer or accepts absent tests.
                            self.assertNotEqual(result.returncode, 0)
                            self.assertIn('missing-native-bundle/manifest.json', result.stderr)
                owner = 'rust_baseline' if mode == 'parallel' else 'oasis7_required'
                expected_calls = expected[:1] if failed == 'oasis7_viewer_codec' else expected
                self.assertEqual(lint_calls[owner], expected_calls, (mode, failed, lint_calls))
                self.assertEqual(lint_calls['oasis7_required' if owner == 'rust_baseline' else 'rust_baseline'], [])

    def test_whole_driver_cli_accepts_baseline_plan_and_rejects_invalid_inputs_before_cargo(self):
        sha = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=artifacts.ROOT, text=True).strip()
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            cargo = root / 'cargo'
            # This fixture checks plan validation and fetch failure propagation,
            # not whether a formatting preflight runs before dependency fetches.
            cargo.write_text('#!/usr/bin/env bash\nprintf "cargo-sentinel:%s\\n" "$*"\n'
                             'if [[ "$*" == "fmt --all -- --check" ]]; then exit 0; fi\nexit 73\n')
            cargo.chmod(0o755)
            selected = InlineExecutionSelection().selection() | {
                'source_sha': sha, 'tested_sha': sha, 'architecture': {'source': sha, 'tested': sha}}
            legacy = dict(selected)
            del legacy['fast_execution_version']
            cases = [('required', 'rust_baseline', selected, 73, 'cargo-sentinel:fetch --locked'),
                     ('required', 'rust_baseline', legacy, 73, 'cargo-sentinel:fetch --locked'),
                     ('required', 'rust_baseline', selected | {'rust_baseline_checks': ['unknown']},
                      1, 'invalid execution selection: rust_baseline_checks'),
                     ('required', 'rust_baseline', selected | {'tested_sha': '0' * 40},
                      1, 'execution plan identity mismatch: tested'),
                     ('required', 'baseline', selected, 2, '--plan requires required --group'),
                     ('full', 'rust_baseline', selected, 2, '--plan requires required --group')]
            path = root / 'selection.json'
            for tier, group, plan, code, diagnostic in cases:
                path.write_text(json.dumps(plan))
                result = subprocess.run(['bash', str(HERE / 'ci-tests.sh'), tier, '--group', group,
                                         '--plan', str(path), '--repo-root', str(artifacts.ROOT)],
                                        text=True, capture_output=True,
                                        env=dict(os.environ, PATH=str(root) + os.pathsep + os.environ['PATH'],
                                                 SOURCE_SHA=sha, CI_VERBOSE='', OASIS7_CI_TEST_BUNDLE=''))
                with self.subTest(tier=tier, group=group, expected=diagnostic):
                    self.assertEqual(result.returncode, code, result.stdout + result.stderr)
                    self.assertIn(diagnostic, result.stdout + result.stderr)
                    self.assertEqual('cargo-sentinel:' in result.stdout, code == 73)

    def test_inline_driver_builds_once_runs_manifest_and_propagates_each_failure(self):
        driver = (HERE / 'ci-tests.sh').read_text()
        body = dict(re.findall(r'^(\w+)\(\) \{\n(.*?)^\}', driver, re.M | re.S))['run_oasis7_required_tier_tests']
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            helper = root / 'ci-test-artifacts.py'
            helper.write_text('''import json, os, pathlib, sys
action = sys.argv[1]
if action == 'required-args':
    sys.exit(0)
bundle = pathlib.Path(sys.argv[sys.argv.index('--bundle') + 1])
with open(os.environ['CALL_LOG'], 'a') as stream:
    stream.write(json.dumps({'args': sys.argv[1:], 'bundle': str(bundle)}) + '\\n')
if action == 'build':
    bundle.mkdir(parents=True)
if os.environ['FAILED'] == action:
    sys.exit(19)
''')
            for partition, failed in ((partition, failed) for partition in ('', 'all', 'oasis7_runtime')
                                      for failed in ('', 'build', 'run')):
                log = root / 'calls.jsonl'
                log.unlink(missing_ok=True)
                script = 'set -euo pipefail\nci_python="$1"; driver_dir="$2"; repo_root="$2"; required_plan=fixture; ordinary_partition="$3"\n'
                script += 'run() { "$@"; }\nrun_cargo() { echo unexpected-Cargo; return 83; }\n'
                script += 'run_oasis7_required_tier_tests() {\n' + body + '}\nrun_oasis7_required_tier_tests\n'
                result = subprocess.run(['bash', '-c', script, 'fixture', sys.executable, str(root), partition],
                                        text=True, capture_output=True,
                                        env=dict(os.environ, FAILED=failed, CALL_LOG=str(log), OASIS7_CI_TEST_BUNDLE=''))
                calls = [json.loads(line) for line in log.read_text().splitlines()]
                with self.subTest(partition=partition, failed=failed):
                    self.assertEqual(result.returncode, 19 if failed else 0, result.stderr)
                    self.assertEqual([call['args'][0] for call in calls], ['build'] if failed == 'build' else ['build', 'run'])
                    self.assertEqual(calls[0]['args'][calls[0]['args'].index('--scope') + 1], 'ordinary')
                    self.assertEqual(len({call['bundle'] for call in calls}), 1)
                    self.assertFalse(Path(calls[0]['bundle']).exists())
                    for call in calls:
                        if partition and partition != 'all':
                            self.assertEqual(call['args'][call['args'].index('--ordinary-partition') + 1], partition)
                        else:
                            self.assertNotIn('--ordinary-partition', call['args'])

    def test_baseline_executes_selected_real_extra_commands_and_propagates_failure(self):
        driver = (HERE / 'ci-tests.sh').read_text()
        bodies = dict(re.findall(r'^(\w+)\(\) \{\n(.*?)^\}', driver, re.M | re.S))
        body = bodies['run_rust_baseline']
        sha = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=artifacts.ROOT, text=True).strip()
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / 'selection.json'
            for checks, failed in (([], ''), (['newapi_accounting'], ''),
                                   (list(artifacts.RUST_BASELINE_CHECKS), ''),
                                   (list(artifacts.RUST_BASELINE_CHECKS), 'newapi_accounting'),
                                   (list(artifacts.RUST_BASELINE_CHECKS), 'standalone_tool_lockfiles'),
                                   (None, '')):
                selected = InlineExecutionSelection().selection(checks)
                selected.update(source_sha=sha, tested_sha=sha, architecture=dict(source=sha, tested=sha))
                if checks is None:
                    del selected['fast_execution_version']
                path.write_text(json.dumps(selected))
                script = 'set -euo pipefail\ntier=required; ci_python="$1"; driver_dir="$2"; repo_root="$3"; required_plan="$4"\n'
                script += 'run() { printf "command:%s\\n" "$*"; }\nrun_cargo() { printf "cargo:%s\\n" "$*"; }\n'
                script += 'run_rustsec_advisory_check() { echo advisories; }\n'
                script += 'run_newapi_bridge_service_accounting_tests() { echo newapi_accounting; [[ "$FAILED" != newapi_accounting ]] || return 19; }\n'
                script += 'run_standalone_tool_lockfiles_checks() { echo standalone_tool_lockfiles; [[ "$FAILED" != standalone_tool_lockfiles ]] || return 19; }\n'
                script += 'run_oasis7_required_clippy_for_group() {\n' + bodies['run_oasis7_required_clippy_for_group'] + '}\n'
                script += 'run_rust_baseline() {\n' + body + '}\nrun_rust_baseline\n'
                result = subprocess.run(['bash', '-c', script, 'fixture', sys.executable, str(HERE),
                                         str(artifacts.ROOT), str(path)], text=True, capture_output=True,
                                        env=dict(os.environ, FAILED=failed, SOURCE_SHA=sha))
                expected = list(artifacts.RUST_BASELINE_CHECKS) if checks is None else checks
                with self.subTest(checks=checks, failed=failed):
                    self.assertEqual(result.returncode, 19 if failed else 0, result.stderr)
                    self.assertIn('cargo:fetch --locked', result.stdout)
                    self.assertIn('cargo-dev-architecture.py', result.stdout)
                    self.assertIn('advisories', result.stdout)
                    self.assertEqual('newapi_accounting' in result.stdout.splitlines(), 'newapi_accounting' in expected)
                    self.assertEqual('standalone_tool_lockfiles' in result.stdout.splitlines(),
                                     'standalone_tool_lockfiles' in expected and failed != 'newapi_accounting')

    def test_format_failure_precedes_fetch_architecture_and_clippy(self):
        body = dict(re.findall(r'^(\w+)\(\) \{\n(.*?)^\}',
                    (HERE / 'ci-tests.sh').read_text(), re.M | re.S))['run_rust_baseline']
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            cargo = root / 'cargo'
            cargo.write_text('#!/bin/sh\necho "cargo:$*"\nexit "$FMT_EXIT"\n')
            cargo.chmod(0o755)
            script = 'set -euo pipefail\ntier=required; required_plan=""\n'
            script += 'run() { if [[ "$1" == env ]]; then "$@"; else echo "run:$*"; fi; }\n'
            script += 'run_cargo() { echo "fetch:$*"; }\n'
            for function in ('run_rustsec_advisory_check', 'run_oasis7_required_clippy_for_group',
                             'run_newapi_bridge_service_accounting_tests', 'run_standalone_tool_lockfiles_checks'):
                script += function + '() { echo ' + function + '; }\n'
            script += 'run_rust_baseline() {\n' + body + '}\nrun_rust_baseline\n'
            for code in (0, 37):
                result = subprocess.run(['bash', '-c', script], text=True, capture_output=True,
                    env=dict(os.environ, PATH=str(root) + os.pathsep + os.environ['PATH'], FMT_EXIT=str(code)))
                self.assertEqual(result.returncode, code, result.stderr)
                lines = result.stdout.splitlines()
                self.assertEqual(lines[0], 'cargo:fmt --all -- --check')
                if code:
                    self.assertEqual(lines, ['cargo:fmt --all -- --check'])
                else:
                    self.assertEqual(lines[1], 'fetch:fetch --locked')
                    self.assertIn('cargo-dev-architecture.py', lines[2])
                    self.assertIn('run_rustsec_advisory_check', lines)
                    self.assertIn('run_oasis7_required_clippy_for_group', lines)
                    self.assertIn('run_newapi_bridge_service_accounting_tests', lines)
                    self.assertIn('run_standalone_tool_lockfiles_checks', lines)

    def test_target_clippy_executes_every_command_and_propagates_each_failure(self):
        driver = (HERE / 'ci-tests.sh').read_text()
        body = dict(re.findall(r'^(\w+)\(\) \{\n(.*?)^\}', driver, re.M | re.S))['run_oasis7_required_tier_clippy']
        commands = [['-p', 'first', '--test', 'smoke', '--no-default-features'], ['-p', 'second', '--lib']]
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / 'ci-test-artifacts.py').write_text('import json\nprint(json.dumps(' + repr(commands) + '))\n')
            for fail in ('none', 'first', 'second'):
                script = 'set -euo pipefail\nci_python="$1"; driver_dir="$2"; repo_root="$2"; required_plan="fixture"\n'
                script += 'run_cargo_clippy() { printf "%s\\n" "$*"; [[ "$2" != "$FAIL_TARGET" ]] || return 19; }\n'
                script += 'run_oasis7_required_tier_clippy() {\n' + body + '}\nrun_oasis7_required_tier_clippy\n'
                result = subprocess.run(['bash', '-c', script, 'fixture', sys.executable, str(root)],
                                        text=True, capture_output=True, env=dict(os.environ, FAIL_TARGET=fail))
                with self.subTest(fail=fail):
                    self.assertEqual(result.returncode, 0 if fail == 'none' else 19, result.stderr)
                    self.assertIn('-p first --test smoke --no-default-features', result.stdout)
                    self.assertEqual('-p second --lib' in result.stdout, fail != 'first')

    def invoke(self, commands, fail_browser=False):
        driver = (HERE / 'ci-tests.sh').read_text()
        functions = dict(re.findall(r'^(\w+)\(\) \{\n(.*?)^\}', driver, re.M | re.S))
        retained = {'run_group', 'run_full_required_superset', 'run_all_required_gate_capability_contract_tests',
                    'run_selected_workspace_support', 'run_oasis7_required_clippy_for_group'}
        definitions = []
        for name, body in functions.items():
            if name in retained:
                definitions.append(name + '() {\n' + body + '}\n')
            elif name not in ('run', 'run_cargo'):
                definitions.append(name + '() { printf "%s\\n" ' + name + '; }\n')
        script = 'set -euo pipefail\ntier=required; support_profiles=""; required_plan=""\n'
        script += '\n'.join(definitions)
        script += 'run_cargo() { printf "cargo:%s\\n" "$*"; }\n'
        script += 'run() { printf "command:%s\\n" "$*"; [[ "${FAIL_BROWSER:-}" != 1 || "$*" != *viewer-auth-browser-security-smoke.mjs* ]] || return 17; }\n'
        script += commands
        return subprocess.run(['bash', '-c', script], text=True, capture_output=True,
                              env=dict(os.environ, FAIL_BROWSER='1' if fail_browser else '0'))

    def test_full_driver_contains_every_locally_supported_required_group(self):
        groups = ['baseline', 'rust_baseline', 'oasis7_required', 'consensus', 'distfs', 'node', 'net',
                  'viewer_js_required', 'viewer_performance_report', 'pixel_world_bridge', 'launcher_web',
                  'workspace_support', 'scenario_regression', 'operational_contracts', 'packaging_contracts',
                  'workflow_governance', 'codex_agent_config_validation', 'compile_metrics', 'site_quality',
                  'doc_checker_contracts', 'cargo_tooling_contracts']
        required = self.invoke('\n'.join('run_group ' + group for group in groups))
        full = self.invoke('run_full_required_superset\nrun_rust_baseline\n')
        self.assertEqual(required.returncode, 0, required.stderr)
        self.assertEqual(full.returncode, 0, full.stderr)
        self.assertLessEqual(set(required.stdout.splitlines()), set(full.stdout.splitlines()))
        self.assertIn('run_oasis7_required_tier_clippy', full.stdout)
        self.assertIn('viewer-auth-browser-security-smoke.mjs', full.stdout)

    def test_full_browser_failure_cannot_be_hidden_by_later_success(self):
        result = self.invoke('run_full_required_superset\n', fail_browser=True)
        self.assertEqual(result.returncode, 17, result.stderr)
        self.assertIn('run_oasis7_viewer_software_safe_build', result.stdout)
        self.assertIn('viewer-auth-browser-security-smoke.mjs', result.stdout)
        self.assertNotIn('run_pixel_world_bridge_lib_tests', result.stdout)


class FormalBuildComponents(unittest.TestCase):
    def test_components_preserve_each_original_compile_context_once(self):
        # Frozen command oracle from b3658f94, independent of the new splitter.
        required = ['test_tier_required']
        formal = ['test_tier_required', 'ci_formal_required_features']
        original = [
            ('formal-required', ['test', '-p', 'oasis7', '-p', 'oasis7_chain_runtime',
                '--lib', '--bins', '--features', ','.join(formal), '--no-run'], formal),
            ('runtime', ['build', '-p', 'oasis7_chain_runtime', '--bin', 'oasis7_chain_runtime',
                '--features', 'test_tier_required'], required),
            ('endpoint', ['build', '-p', 'oasis7_distfs', '--bin', 'oasis7_durability_endpoint', '--release'], []),
            ('distfs-tests', ['test', '-p', 'oasis7_distfs', '--lib', '--no-run'], []),
        ]
        self.assertEqual(artifacts.build_variants('formal'), original)
        debug = artifacts.build_variants('formal-debug')
        endpoint = artifacts.build_variants('formal-endpoint')
        self.assertEqual(debug, [original[0], original[1], original[3]])
        self.assertEqual(endpoint, [original[2]])
        self.assertCountEqual(debug + endpoint, original)
        self.assertEqual(len({name for name, _, _ in debug + endpoint}), 4)

    def test_formal_debug_profile_keeps_assertions_and_endpoint_release_contract(self):
        with patch.dict(os.environ, RUSTC_WRAPPER='must-not-wrap',
                        CARGO_PROFILE_DEV_OPT_LEVEL='2', CARGO_PROFILE_TEST_OPT_LEVEL='2',
                        CARGO_PROFILE_DEV_DEBUG_ASSERTIONS='false', CARGO_PROFILE_TEST_DEBUG_ASSERTIONS='false',
                        CARGO_PROFILE_DEV_OVERFLOW_CHECKS='false', CARGO_PROFILE_TEST_OVERFLOW_CHECKS='false'):
            for scope, level in (('formal', '0'), ('formal-debug', '0'),
                                 ('formal-endpoint', '0'), ('storage', '2')):
                with self.subTest(scope=scope):
                    environment = artifacts.build_environment(scope)
                    self.assertNotIn('RUSTC_WRAPPER', environment)
                    for profile in ('DEV', 'TEST'):
                        self.assertEqual(environment['CARGO_PROFILE_' + profile + '_OPT_LEVEL'], level)
                        self.assertEqual(environment['CARGO_PROFILE_' + profile + '_DEBUG_ASSERTIONS'], 'true')
                        self.assertEqual(environment['CARGO_PROFILE_' + profile + '_OVERFLOW_CHECKS'], 'true')
            self.assertEqual(artifacts.build_environment('ordinary')['CARGO_PROFILE_DEV_OPT_LEVEL'], '2')
        for scope, level in (('formal', '0'), ('formal-debug', '0'), ('storage', '2')):
            debug = dict(profile=dict(opt_level=level, debug_assertions=True, overflow_checks=True))
            artifacts.validate_profile('runtime-tests', debug, scope)
            for field, wrong in (('opt_level', '2' if level == '0' else '0'),
                                 ('debug_assertions', False), ('overflow_checks', False)):
                altered = copy.deepcopy(debug)
                altered['profile'][field] = wrong
                with self.subTest(scope=scope, field=field), self.assertRaises(ValueError):
                    artifacts.validate_profile('runtime-tests', altered, scope)
        endpoint = dict(target='oasis7_durability_endpoint', test=False,
                        original_executable='/checkout/target/release/oasis7_durability_endpoint',
                        profile=dict(opt_level='3', debug_assertions=False))
        artifacts.validate_profile('endpoint', endpoint, 'formal-endpoint')
        endpoint['profile']['opt_level'] = '2'
        with self.assertRaises(ValueError):
            artifacts.validate_profile('endpoint', endpoint, 'formal-endpoint')


class BuildDiagnostics(unittest.TestCase):
    def test_completed_unit_receipt_is_bounded_and_does_not_claim_active_stage(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            observation = artifacts.BuildObservation('required', root)
            observation.directory.mkdir()
            observation.last_unit_write -= 21
            observation.observe_unit(dict(reason='compiler-message', message='not a completed unit'))
            self.assertEqual(observation.report['completed_units'], 0)
            observation.observe_unit(dict(reason='compiler-artifact', target={
                'name': 'a' * 400, 'kind': ['lib']}, fresh=False,
                package_id='private-registry-secret', manifest_path='/private/path'))
            report = json.loads((observation.directory / 'build.json').read_text())
            self.assertEqual(report['completed_units'], 1)
            self.assertEqual(len(report['latest_completed_unit']['target_name']), 256)
            self.assertFalse(report['latest_completed_unit']['fresh'])
            self.assertEqual(report['status'], 'running')
            self.assertIsNone(report['cargo_exit_code'])
            self.assertNotIn('private', json.dumps(report))
            self.assertIn('observed_at', report['latest_completed_unit'])

    def test_only_current_timing_files_are_published_with_start_and_end_state(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            timings, diagnostics = root / 'timings', root / 'diagnostics'
            timings.mkdir()
            diagnostics.mkdir()
            stale = timings / 'cargo-timing-previous.html'
            stale.write_text('<html>stale previous command</html>')
            alias = timings / 'cargo-timing.html'
            alias.write_text('<html>old alias</html>')
            with artifacts.BuildObservation('current', diagnostics, timings) as observation:
                start = json.loads((diagnostics / 'current/build.json').read_text())
                self.assertEqual((start['status'], start['cargo_exit_code']), ('running', None))
                self.assertEqual(start['timings'], [])
                alias.write_text('<html>updated current command alias</html>')
                new = timings / 'cargo-timing-current.html'
                new.write_text('<html>current command</html>')
                # The real Cargo exit object is tested by the standalone Rust
                # fixtures; this case isolates changed-file collection.
                observation.process = type('CompletedCargo', (), {'returncode': 0})()
            report = json.loads((diagnostics / 'current/build.json').read_text())
            self.assertEqual((report['status'], report['cargo_exit_code']), ('success', 0))
            self.assertCountEqual(report['timings'], [alias.name, new.name])
            self.assertEqual((diagnostics / 'current' / alias.name).read_text(), alias.read_text())
            self.assertEqual((diagnostics / 'current' / new.name).read_text(), new.read_text())
            self.assertFalse((diagnostics / 'current' / stale.name).exists())


class ParallelFormalExecution(unittest.TestCase):
    def execute(self, root, entries, invoke, weights=None, cpus=4, version=1, shard='runtime-protocol', plan=None):
        config = dict(parallel_cases={'runtime-tests': weights or {}})
        manifest = dict(scope='formal', formal_execution_version=version, binaries={
            'runtime-tests': dict(path='bin/runtime-tests', cwd='.', test=True, ordinary=False)})
        with patch.object(artifacts, 'load', return_value=(manifest, {})), \
             patch.object(artifacts, 'formal_configuration', return_value=config), \
             patch.object(artifacts, 'selected_entries', return_value=entries), \
             patch.object(artifacts, 'native_cpu_budget', return_value=cpus), \
             patch('ci_process_runner.run', side_effect=invoke):
            artifacts.execute(root, 'ordinary', root / 'results.jsonl', shard=shard, plan=plan)

    @staticmethod
    def entry(name, ignored=False):
        return dict(binary='runtime-tests', name=name, ignored=ignored, suite='runtime-protocol')

    def test_weighted_exact_cases_overlap_and_unknown_case_runs_alone(self):
        entries = [self.entry(name) for name in ('light-a', 'light-b', 'unknown', 'light-c')]
        barrier = threading.Barrier(2)
        lock = threading.Lock()
        active, maximum, calls = {}, [0], []
        def invoke(command, cwd, env, suite, name, results, expected_count=None, **kwargs):
            weight = {'light-a': 2, 'light-b': 2, 'light-c': 1}.get(name, 4)
            self.assertEqual(command[1:], ['--exact', name, '--test-threads=1', '--nocapture'])
            self.assertEqual(expected_count, 1)
            self.assertEqual(env['RUST_TEST_THREADS'], '1')
            with lock:
                active[name] = weight
                self.assertLessEqual(sum(active.values()), 4)
                if name == 'unknown':
                    self.assertEqual(list(active), ['unknown'])
                maximum[0] = max(maximum[0], len(active))
                calls.append(name)
            try:
                if name in ('light-a', 'light-b'):
                    barrier.wait(timeout=2)
                return parallel_native_receipt(suite, name, results, expected_count)
            finally:
                with lock:
                    del active[name]
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            self.execute(root, entries, invoke, {'light-a': 2, 'light-b': 2, 'light-c': 1})
            records = [json.loads(line) for line in (root / 'results.jsonl').read_text().splitlines()]
        self.assertEqual(maximum[0], 2)
        self.assertCountEqual(calls, [entry['name'] for entry in entries])
        self.assertEqual(len(records), 4)
        self.assertTrue(all(record['passed'] and record['shard'] == 'runtime-protocol' for record in records))

    def test_parallel_case_allowance_cannot_exceed_four_workers_or_cpu(self):
        for cpus, expected in ((1, 1), (2, 2), (8, 4)):
            entries = [self.entry('case-' + str(index)) for index in range(8)]
            barrier = threading.Barrier(expected)
            active, maximum, calls = [0], [0], []
            lock = threading.Lock()
            def invoke(command, cwd, env, suite, name, results, expected_count=None, **kwargs):
                with lock:
                    active[0] += 1
                    maximum[0] = max(maximum[0], active[0])
                    calls.append(name)
                try:
                    barrier.wait(timeout=2)
                    return parallel_native_receipt(suite, name, results, expected_count)
                finally:
                    with lock:
                        active[0] -= 1
            with self.subTest(cpus=cpus), tempfile.TemporaryDirectory() as temporary:
                self.execute(Path(temporary), entries, invoke, {entry['name']: 1 for entry in entries}, cpus)
            self.assertEqual(maximum[0], expected)
            self.assertCountEqual(calls, [entry['name'] for entry in entries])

    def test_formal_first_failure_cancels_active_and_never_launches_pending_case(self):
        entries = [self.entry(name) for name in ('failure', 'active', 'pending')]
        barrier = threading.Barrier(2)
        calls = []
        def invoke(command, cwd, env, suite, name, results, expected_count=None, **kwargs):
            calls.append(name)
            barrier.wait(timeout=2)
            if name == 'failure':
                return parallel_native_receipt(suite, name, results, expected_count,
                    passed=False, exit_code=7)
            self.assertTrue(kwargs['cancellation_event'].wait(timeout=2))
            return parallel_native_receipt(suite, name, results, expected_count,
                passed=False, cancelled=True, exit_code=-signal.SIGTERM)
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            with self.assertRaisesRegex(RuntimeError, 'formal failed'):
                self.execute(root, entries, invoke, {entry['name']: 1 for entry in entries}, cpus=2)
            records = [json.loads(line) for line in (root / 'results.jsonl').read_text().splitlines()]
        self.assertCountEqual(calls, ['failure', 'active'])
        self.assertEqual(len(records), 2)
        self.assertEqual(next(record for record in records if record['test_name'] == 'failure')['exit_code'], 7)
        self.assertTrue(all(record['shard'] == 'runtime-protocol' for record in records))

    def test_old_formal_bundle_keeps_original_serial_order_and_environment(self):
        entries = [self.entry('first', True), self.entry('second')]
        calls = []
        def invoke(command, cwd, env, suite, name, results, expected_count=None, **kwargs):
            self.assertNotIn('cancellation_event', kwargs)
            self.assertNotIn('RUST_TEST_THREADS', env)
            calls.append(name)
            return parallel_native_receipt(suite, name, results, expected_count)
        with tempfile.TemporaryDirectory() as temporary:
            self.execute(Path(temporary), entries, invoke, {'first': 1, 'second': 1}, version=None)
        self.assertEqual(calls, ['first', 'second'])

    def test_formal_shard_with_fast_plan_keeps_case_order_and_receipt_identity(self):
        # The CLI's default suite is ordinary even for a Formal shard. A fast
        # plan must not apply library priority or ordinary receipt semantics.
        calls = []
        def invoke(command, cwd, env, suite, name, results, expected_count=None, **kwargs):
            calls.append(name)
            self.assertEqual(env['RUST_TEST_THREADS'], '1')
            self.assertIn('cancellation_event', kwargs)
            return parallel_native_receipt(suite, name, results, expected_count)
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            self.execute(root, [self.entry('first', True), self.entry('second')], invoke,
                         {'first': 1, 'second': 1}, cpus=1,
                         plan=parallel_native_mixed_selection())
            records = [json.loads(line) for line in (root / 'results.jsonl').read_text().splitlines()]
        self.assertEqual(calls, ['first', 'second'])
        self.assertTrue(all(record['binary'] == 'runtime-tests'
                            and record['shard'] == 'runtime-protocol'
                            and record['suite'] == 'runtime-protocol'
                            and record['passed'] for record in records))

    def test_malformed_weight_fails_before_any_case_starts(self):
        for value in (0, -1, 3, True, '1', None):
            calls = []
            with self.subTest(weight=value), tempfile.TemporaryDirectory() as temporary:
                with self.assertRaises(ValueError):
                    self.execute(Path(temporary), [self.entry('first')], lambda *a, **k: calls.append(a), {'first': value})
            self.assertEqual(calls, [])

    def test_candidate_cannot_expand_or_reduce_a_trusted_case_weight(self):
        base = dict(schema_version=2, suites={}, environment_formal_batches={}, shell_tests={},
                    parallel_cases={'runtime-tests': {'known': 2}})
        candidate = copy.deepcopy(base)
        candidate['parallel_cases']['runtime-tests'] = {'known': 1, 'unreviewed': 1}
        with tempfile.TemporaryDirectory() as temporary:
            authority, source = Path(temporary) / 'base.json', Path(temporary) / 'candidate.json'
            authority.write_text(json.dumps(base))
            source.write_text(json.dumps(candidate))
            with patch.object(artifacts, 'CONFIG', source), patch.object(artifacts, 'FORMAL_AUTHORITY', authority):
                self.assertEqual(artifacts.formal_parallel_cases(artifacts.formal_configuration()),
                                 {('runtime-tests', 'known'): 2})
            with patch.object(artifacts, 'CONFIG', source), patch.object(artifacts, 'FORMAL_AUTHORITY', None):
                self.assertEqual(artifacts.formal_parallel_cases(artifacts.formal_configuration()),
                                 {('runtime-tests', 'known'): 1, ('runtime-tests', 'unreviewed'): 1})

    def test_missing_duplicate_wrong_or_unclean_receipt_stops_dispatch(self):
        for damage in ('missing', 'duplicate', 'identity', 'count', 'read-error', 'leak', 'late-exit-zero'):
            calls = []
            def invoke(command, cwd, env, suite, name, results, expected_count=None, **kwargs):
                calls.append(name)
                if damage == 'late-exit-zero':
                    raise SystemExit(0)
                result = parallel_native_receipt(suite, name, results, expected_count)
                if damage == 'missing':
                    results.unlink()
                elif damage == 'duplicate':
                    with results.open('a') as stream:
                        stream.write(json.dumps(result) + '\n')
                else:
                    field, value = {'identity': ('test_name', 'other'), 'count': ('expected_count', 0),
                                    'read-error': ('output_read_error', 'OSError'),
                                    'leak': ('leaked_processes', True)}[damage]
                    result[field] = value
                    results.write_text(json.dumps(result) + '\n')
                return result
            with self.subTest(damage=damage), tempfile.TemporaryDirectory() as temporary:
                with self.assertRaisesRegex(RuntimeError, 'formal failed'):
                    self.execute(Path(temporary), [self.entry('first'), self.entry('pending')], invoke, cpus=1)
            self.assertEqual(calls, ['first'])

    def test_later_failure_does_not_rewrite_a_completed_success(self):
        completed = threading.Event()
        def invoke(command, cwd, env, suite, name, results, expected_count=None, **kwargs):
            if name == 'success':
                result = parallel_native_receipt(suite, name, results, expected_count)
                completed.set()
                return result
            self.assertTrue(completed.wait(timeout=2))
            time.sleep(0.02)
            return parallel_native_receipt(suite, name, results, expected_count, passed=False, exit_code=7)
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            with self.assertRaisesRegex(RuntimeError, 'formal failed'):
                self.execute(root, [self.entry('success'), self.entry('failure')], invoke,
                             {'success': 1, 'failure': 1}, cpus=2)
            records = [json.loads(line) for line in (root / 'results.jsonl').read_text().splitlines()]
        self.assertTrue(next(record for record in records if record['test_name'] == 'success')['passed'])
        self.assertFalse(next(record for record in records if record['test_name'] == 'failure')['passed'])


class UnhashedLibraryCache(unittest.TestCase):
    """Model Cargo's path-library layout, including delayed fingerprints."""

    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.base = Path(temporary.name)
        self.root = self.base / 'checkout'
        self.root.mkdir()
        self.target = self.root / 'target'
        self.cache = self.base / 'cache'
        self.manifest = self.root / 'Cargo.toml'
        self.manifest.write_text('[package]\nname="mixed-lib"\nversion="0.1.0"\n')
        self.fingerprint = 'debug/.fingerprint/mixed-lib-' + 'a' * 16
        self.filenames = ['debug/deps/libmixed_lib.so', 'debug/deps/libmixed_lib.rlib']
        for name in [*self.filenames, 'debug/deps/mixed_lib.d']:
            self.write(name)
        self.message = dict(reason='compiler-artifact', manifest_path=str(self.manifest),
            package_id='mixed-lib', filenames=[str(self.target / name) for name in self.filenames],
            target=dict(name='mixed_lib', kind=['cdylib', 'lib']),
            profile=dict(test=False), features=[], fresh=False)

    def write(self, name, text=None):
        path = self.target / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(name if text is None else text)
        return path

    def fingerprint_files(self, directory=None, features=()):
        directory = directory or self.fingerprint
        for name in ('lib-mixed_lib', 'dep-lib-mixed_lib', 'invoked.timestamp'):
            self.write(directory + '/' + name)
        self.write(directory + '/lib-mixed_lib.json', json.dumps(dict(features=json.dumps(list(features)))))

    def finish(self, state):
        artifacts.cache_finish(state)
        self.assertTrue(state['complete'])
        return artifacts.cache_stage(state, self.cache)

    def test_deferred_complete_units_roundtrip_for_both_selections(self):
        for selection in ('libraries', 'all'):
            with self.subTest(selection=selection):
                if (self.target / 'debug/.fingerprint').exists():
                    shutil.rmtree(self.target / 'debug/.fingerprint')
                state = artifacts.cache_begin(self.root, selection=selection)
                artifacts.cache_record(state, self.message)
                self.assertFalse(state['complete'])
                self.assertFalse(artifacts.cache_stage(state, self.cache)['save'])
                self.fingerprint_files()
                # A valid timestamp marker need not contain payload bytes.
                self.write(self.fingerprint + '/invoked.timestamp', '')
                report = self.finish(state)
                self.assertTrue(report['save'], report)
                manifest = json.loads((self.cache / 'manifest.json').read_text())
                self.assertTrue(set(self.filenames) <= set(manifest['files']))
                self.assertIn('debug/deps/mixed_lib.d', manifest['files'])
                self.assertIn(self.fingerprint, manifest['directories'])
                shutil.rmtree(self.target)
                restored = artifacts.cache_restore(self.root, self.cache)
                self.assertTrue(restored['restored'], restored)
                for name in self.filenames:
                    self.assertEqual((self.target / name).read_text(), name)

    def test_uplifted_library_preserves_proven_concrete_outputs(self):
        self.fingerprint_files()
        message = copy.deepcopy(self.message)
        public_names = []
        for name in self.filenames:
            public = self.target / 'debug' / Path(name).name
            os.link(self.target / name, public)
            public_names.append(str(public))
        message['filenames'] = public_names
        self.write('debug/libmixed_lib.d')
        state = artifacts.cache_begin(self.root)
        artifacts.cache_record(state, message)
        self.assertTrue(self.finish(state)['save'])
        manifest = json.loads((self.cache / 'manifest.json').read_text())
        for name in self.filenames:
            public = Path('debug') / Path(name).name
            self.assertIn(public.as_posix(), manifest['files'])
            self.assertTrue(os.path.samefile(self.cache / 'target' / name, self.cache / 'target' / public))
        self.assertIn('debug/libmixed_lib.d', manifest['files'])

    def test_hashed_check_and_test_units_do_not_own_unhashed_library(self):
        self.fingerprint_files()
        other = 'debug/.fingerprint/mixed-lib-' + 'b' * 16
        self.fingerprint_files(other)
        self.write('debug/deps/libmixed_lib-' + 'b' * 16 + '.rmeta')
        self.write('debug/.fingerprint/mixed-lib-' + 'c' * 16 + '/test-lib-mixed_lib')
        state = artifacts.cache_begin(self.root)
        artifacts.cache_record(state, self.message)
        self.assertTrue(self.finish(state)['save'])
        self.assertEqual(state['directories'], [self.fingerprint])
        self.assertFalse(any('b' * 16 in name for name in state['files']))

    def test_last_feature_variant_owns_the_shared_output_names(self):
        state = artifacts.cache_begin(self.root)
        first = dict(self.message, features=['alternate'])
        artifacts.cache_record(state, first)
        artifacts.cache_record(state, self.message)
        self.fingerprint_files()
        self.assertTrue(self.finish(state)['save'])
        self.assertEqual(len(state['unhashed_libraries']), 1)
        self.assertEqual(next(iter(state['unhashed_libraries'].values()))['features'], [])

    def test_ambiguous_fingerprint_is_not_selected_by_timestamp(self):
        self.fingerprint_files()
        self.fingerprint_files('debug/.fingerprint/mixed-lib-' + 'b' * 16)
        state = artifacts.cache_begin(self.root)
        artifacts.cache_record(state, self.message)
        with self.assertRaisesRegex(ValueError, 'unique unhashed.*mixed-lib/mixed_lib'):
            artifacts.cache_finish(state)
        self.assertFalse(state['complete'])
        self.assertEqual(state['files'], [])
        self.assertFalse(artifacts.cache_stage(state, self.cache)['save'])

    def test_missing_fingerprint_members_are_rejected(self):
        for name in ('lib-mixed_lib', 'lib-mixed_lib.json', 'dep-lib-mixed_lib', 'invoked.timestamp'):
            with self.subTest(missing=name):
                self.fingerprint_files()
                (self.target / self.fingerprint / name).unlink()
                state = artifacts.cache_begin(self.root)
                artifacts.cache_record(state, self.message)
                with self.assertRaises(ValueError):
                    artifacts.cache_finish(state)
                self.assertFalse(state['complete'])
                self.assertFalse(artifacts.cache_stage(state, self.cache)['save'])

    def test_missing_outputs_and_dep_info_are_rejected(self):
        self.fingerprint_files()
        for name in [*self.filenames, 'debug/deps/mixed_lib.d']:
            with self.subTest(missing=name):
                state = artifacts.cache_begin(self.root)
                artifacts.cache_record(state, self.message)
                (self.target / name).unlink()
                with self.assertRaisesRegex(ValueError, 'missing unhashed'):
                    artifacts.cache_finish(state)
                self.assertFalse(state['complete'])
                self.write(name)

    def test_fingerprint_features_and_json_must_match(self):
        for text in ('{', '{}', '[]', json.dumps(dict(features='["other"]'))):
            with self.subTest(metadata=text):
                self.fingerprint_files()
                self.write(self.fingerprint + '/lib-mixed_lib.json', text)
                state = artifacts.cache_begin(self.root)
                artifacts.cache_record(state, self.message)
                with self.assertRaises(ValueError):
                    artifacts.cache_finish(state)
                self.assertFalse(state['complete'])

    def test_symlinked_fingerprint_is_rejected(self):
        self.fingerprint_files()
        path = self.target / self.fingerprint
        moved = self.base / 'elsewhere'
        path.rename(moved)
        path.symlink_to(moved, target_is_directory=True)
        state = artifacts.cache_begin(self.root)
        artifacts.cache_record(state, self.message)
        with self.assertRaisesRegex(ValueError, 'symlink'):
            artifacts.cache_finish(state)
        self.assertFalse(state['complete'])

    def test_unbound_public_copy_is_not_treated_as_a_cargo_hardlink(self):
        self.fingerprint_files()
        public = self.write('debug/libmixed_lib.so', self.filenames[0])
        message = copy.deepcopy(self.message)
        message['filenames'][0] = str(public)
        state = artifacts.cache_begin(self.root)
        artifacts.cache_record(state, message)
        with self.assertRaisesRegex(ValueError, 'unbound unhashed'):
            artifacts.cache_finish(state)
        self.assertFalse(state['complete'])

    def test_different_package_cannot_claim_the_same_output_namespace(self):
        other = self.root / 'other/Cargo.toml'
        other.parent.mkdir()
        other.write_text('[package]\nname="another-package"\nversion="0.1.0"\n')
        state = artifacts.cache_begin(self.root)
        artifacts.cache_record(state, self.message)
        message = dict(self.message, manifest_path=str(other), package_id='another-package')
        with self.assertRaisesRegex(ValueError, 'output collision'):
            artifacts.cache_record(state, message)

    def test_unknown_unhashed_binary_still_fails(self):
        message = copy.deepcopy(self.message)
        message['target']['kind'] = ['bin']
        state = artifacts.cache_begin(self.root)
        with self.assertRaisesRegex(ValueError, 'unique local Cargo output unit'):
            artifacts.cache_record(state, message)


class WorkspaceInputPaths(unittest.TestCase):
    """Exercise input retiming through the real stage/restore boundary."""

    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.base = Path(temporary.name).resolve()
        self.root = self.base / 'checkout'
        self.cache = self.base / 'cache'
        self.target = self.root / 'target'
        sources = {
            'Cargo.toml': '[workspace]\nmembers=["a", "b"]\n',
            'a/Cargo.toml': '[package]\nname="a"\nversion="0.1.0"\n',
            'a/src/lib.rs': 'pub fn value() {}\n',
            'a/build.rs': 'fn main() {}\n',
            'b/Cargo.toml': '[package]\nname="b"\nversion="0.1.0"\n',
            'b/src/lib.rs': 'pub fn other() {}\n',
            'doc/index.md': 'source input, not an ignored path\n',
            'assets/input.txt': 'original\n',
        }
        for name, text in sources.items():
            self.write(name, text)
        directory = 'debug/.fingerprint/a-' + 'a' * 16
        self.payload = ['debug/deps/liba-' + 'a' * 16 + '.rlib', directory + '/lib-a']
        for name in self.payload:
            self.write('target/' + name, 'complete fixture unit')
        state = artifacts.cache_begin(self.root)
        state['input_owners'] = {'a/Cargo.toml': ['a/build.rs'], 'b/Cargo.toml': []}
        state['files'] = self.payload
        state['directories'] = [directory]
        artifacts.cache_finish(state)
        self.assertTrue(artifacts.cache_stage(state, self.cache)['save'])
        self.manifest_path = self.cache / 'manifest.json'
        self.manifest = json.loads(self.manifest_path.read_text())
        shutil.rmtree(self.target)

    def write(self, name, text='new input'):
        path = self.root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text)
        return path

    def stamp(self, name):
        return (self.root / name).stat().st_mtime_ns

    def assert_restored(self):
        report = artifacts.cache_restore(self.root, self.cache)
        self.assertTrue(report['restored'], report)
        self.assertTrue(all((self.target / name).is_file() for name in self.payload))
        return report

    def assert_miss(self, pattern):
        before = artifacts._cache_inputs(self.root)
        report = artifacts.cache_restore(self.root, self.cache)
        self.assertFalse(report['restored'], report)
        self.assertRegex(report['reason'], pattern)
        self.assertFalse(self.target.exists())
        self.assertEqual(artifacts._cache_inputs(self.root), before)
        return report

    def test_unchanged_input_paths_preserve_exact_mtimes(self):
        report = self.assert_restored()
        self.assertEqual(report['refreshed_inputs'], 0)
        for name, metadata in self.manifest['inputs'].items():
            self.assertEqual(self.stamp(name), metadata['mtime_ns'])

    def test_added_source_promotes_owner_not_unrelated_package(self):
        added = self.write('a/src/new/mod.rs')
        os.utime(added, ns=(1, 1))
        report = self.assert_restored()
        self.assertEqual(report['input_path_changes']['added']['paths'], ['a/src/new/mod.rs'])
        self.assertEqual(report['refreshed_input_scopes'], ['a'])
        self.assertGreater(self.stamp('a/src/lib.rs'), self.manifest['inputs']['a/src/lib.rs']['mtime_ns'])
        self.assertGreater(self.stamp('a/src/new/mod.rs'), 1)
        self.assertEqual(self.stamp('b/src/lib.rs'), self.manifest['inputs']['b/src/lib.rs']['mtime_ns'])

    def test_removed_and_renamed_inputs_stay_absent(self):
        old = self.root / 'a/src/lib.rs'
        old.rename(old.with_name('renamed.rs'))
        report = self.assert_restored()
        self.assertFalse(old.exists())
        self.assertEqual(report['input_path_changes']['removed']['paths'], ['a/src/lib.rs'])
        self.assertEqual(report['input_path_changes']['added']['paths'], ['a/src/renamed.rs'])
        self.assertGreater(self.stamp('a/src/renamed.rs'), self.manifest['inputs']['a/src/lib.rs']['mtime_ns'])

    def test_document_addition_keeps_libraries_but_refreshes_observers(self):
        self.write('doc/new/index.md')
        report = self.assert_restored()
        self.assertEqual(report['refreshed_input_scopes'], ['doc'])
        for name in ('a/src/lib.rs', 'b/src/lib.rs'):
            self.assertEqual(self.stamp(name), self.manifest['inputs'][name]['mtime_ns'])
        for name in ('a/build.rs', 'doc/index.md'):
            self.assertGreater(self.stamp(name), self.manifest['inputs'][name]['mtime_ns'])
        self.assertEqual(report['refreshed_generator_inputs'], 1)

    def test_unowned_asset_removal_promotes_surviving_parent(self):
        (self.root / 'assets/input.txt').unlink()
        self.write('assets/replacement.txt')
        report = self.assert_restored()
        self.assertEqual(report['refreshed_input_scopes'], ['assets'])
        self.assertFalse((self.root / 'assets/input.txt').exists())
        self.assertEqual(self.stamp('b/src/lib.rs'), self.manifest['inputs']['b/src/lib.rs']['mtime_ns'])

    def test_new_top_level_tree_conservatively_promotes_checkout(self):
        self.write('unknown/first.txt')
        report = self.assert_restored()
        self.assertEqual(report['refreshed_input_scopes'], ['.'])
        self.assertGreater(self.stamp('b/src/lib.rs'), self.manifest['inputs']['b/src/lib.rs']['mtime_ns'])

    def test_legacy_snapshot_keeps_exact_path_support(self):
        del self.manifest['input_owners']
        self.manifest_path.write_text(json.dumps(self.manifest))
        self.assert_restored()
        shutil.rmtree(self.target)
        self.write('a/new.txt')
        report = self.assert_miss('ownership metadata')
        self.assertEqual(report['input_path_changes']['added']['paths'], ['a/new.txt'])

    def test_configuration_path_change_is_a_diagnosed_miss(self):
        self.write('.cargo/config.toml', '[build]\n')
        report = self.assert_miss('configuration paths changed')
        self.assertEqual(report['input_path_changes']['added']['paths'], ['.cargo/config.toml'])

    def test_removed_input_metadata_is_still_validated(self):
        (self.root / 'doc/index.md').unlink()
        self.manifest['inputs']['doc/index.md']['mtime_ns'] = 2 ** 63 - 1
        self.manifest_path.write_text(json.dumps(self.manifest))
        self.assert_miss('invalid cache timestamp')

    def test_removed_manifest_path_cannot_escape_source_root(self):
        self.manifest['inputs']['../escape'] = self.manifest['inputs'].pop('doc/index.md')
        self.manifest_path.write_text(json.dumps(self.manifest))
        self.assert_miss('invalid cache path')

    def test_git_and_target_cannot_be_forged_as_removed_inputs(self):
        for name in ('.git/config', 'target/source.rs'):
            with self.subTest(name=name):
                manifest = copy.deepcopy(self.manifest)
                manifest['inputs'][name] = manifest['inputs']['doc/index.md']
                self.manifest_path.write_text(json.dumps(manifest))
                self.assert_miss('invalid cache source input')

    def test_unrecorded_generator_and_bad_ownership_are_rejected(self):
        self.write('doc/new.md')
        for owners in ([], {}, {'a/Cargo.toml': ['not-observed.rs']}, {'a/Cargo.toml': ['../escape']}):
            with self.subTest(owners=owners):
                self.manifest['input_owners'] = owners
                self.manifest_path.write_text(json.dumps(self.manifest))
                self.assert_miss('metadata|unrecorded|invalid cache path')

    def test_input_diagnostics_are_bounded(self):
        for index in range(60):
            self.write(f'doc/{index:03d}.md')
        report = self.assert_restored()
        added = report['input_path_changes']['added']
        self.assertEqual(added['count'], 60)
        self.assertEqual(len(added['paths']), 50)
        self.assertTrue(added['truncated'])

    def test_retiming_failure_discards_restored_fingerprints(self):
        self.write('a/src/new.rs')
        original = os.utime
        def fail(path, *args, **kwargs):
            if Path(path) == self.root / 'a/src/lib.rs':
                raise OSError('retiming unavailable')
            return original(path, *args, **kwargs)
        with patch.object(artifacts.os, 'utime', side_effect=fail):
            report = artifacts.cache_restore(self.root, self.cache)
        self.assertFalse(report['restored'], report)
        self.assertIn('retiming unavailable', report['reason'])
        self.assertFalse(any((self.target / name).exists() for name in self.payload))

    def test_record_uses_cargo_manifest_and_generator_source(self):
        source = self.root / 'a/build.rs'
        executable = self.write('target/debug/build/a-' + 'a' * 16 + '/build-script-build')
        state = artifacts.cache_begin(self.root)
        artifacts.cache_record(state, dict(reason='compiler-artifact',
            manifest_path=str(self.root / 'a/Cargo.toml'), package_id='a',
            target=dict(name='build-script-build', kind=['custom-build'], src_path=str(source)),
            profile=dict(test=False), filenames=[str(executable)]))
        self.assertEqual(state['input_owners'], {'a/Cargo.toml': ['a/build.rs']})


class Process(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.temp = tempfile.TemporaryDirectory()
        cls.root = Path(cls.temp.name)
        source = cls.root / 'fixture.rs'
        source.write_text('#[test] fn ordinary() {}\n#[test] #[ignore] fn acceptance() {}\n#[test] #[ignore] fn crash_child() {}\n')
        cls.binary = cls.root / 'fixture'
        compile_rust_fixture(source, cls.binary, test=True)

    @classmethod
    def tearDownClass(cls):
        cls.temp.cleanup()

    def execute(self, args, count=1, **kwargs):
        return run(args, self.root, os.environ, 'fixture', 'test', self.root / 'results.jsonl', expected_count=count, **kwargs)

    def test_real_libtest_lists_and_exact_ignored_execution(self):
        self.assertEqual(artifacts.inventory(self.binary, True), ['acceptance', 'crash_child'])
        self.assertTrue(self.execute([str(self.binary), '--exact', 'acceptance', '--ignored', '--test-threads=1'])['passed'])
        self.assertTrue(self.execute([str(self.binary)], count=1)['passed'])

    def test_zero_match_cannot_pass(self):
        result = self.execute([str(self.binary), '--exact', 'renamed', '--ignored'])
        self.assertEqual(result['exit_code'], 0)
        self.assertFalse(result['passed'])

    def test_real_fast_formal_deferral_is_exact_and_full_preserves_failure(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source, executable = root / 'tests.rs', root / 'runtime-tests'
            source.write_text('''
#[test] fn ordinary() { std::fs::write("ordinary-ran", "yes").unwrap(); }
#[test] fn formal_heavy() { std::fs::write("heavy-ran", "yes").unwrap(); panic!("full failure"); }
#[test] fn formal_heavy_neighbor() {
    std::fs::write("neighbor-ran", "yes").unwrap();
    assert!(std::env::var_os("FAIL_ORDINARY_NEIGHBOR").is_none());
}
#[test] #[ignore] fn ignored_case() { panic!("ignored"); }
''')
            compile_rust_fixture(source, executable, test=True)
            selected = FastExecutionSelection().selection(('oasis7_chain_runtime',)) | dict(
                formal_full_profiles_only_version=1, formal_shards=[],
                formal_deferred_ordinary_cases={'runtime-tests': ['formal_heavy']})
            binary = dict(package='oasis7_chain_runtime', target='oasis7_chain_runtime',
                          target_kind=['lib'], variant='required', ordinary=True,
                          tests=artifacts.inventory(executable), ignored_tests=artifacts.inventory(executable, True),
                          path=executable.name, cwd='.')
            manifest = dict(scope='ordinary', binaries={'runtime-tests': binary}, execution_plan=selected)
            environment = dict(os.environ)
            environment.pop('FAIL_ORDINARY_NEIGHBOR', None)
            def execute(selection, name, fail_neighbor=False):
                manifest['execution_plan'] = selection
                env = dict(environment)
                if fail_neighbor:
                    env['FAIL_ORDINARY_NEIGHBOR'] = '1'
                with patch.object(artifacts, 'ROOT', root), \
                     patch.object(artifacts, 'load', return_value=(manifest, env)), \
                     patch.object(artifacts, 'snapshot_regressions', return_value=()):
                    artifacts.execute(root, 'ordinary', root / (name + '.jsonl'), plan=selection)
            execute(selected, 'fast')
            self.assertTrue((root / 'ordinary-ran').is_file())
            self.assertTrue((root / 'neighbor-ran').is_file())
            self.assertFalse((root / 'heavy-ran').exists())
            receipt = json.loads((root / 'fast.jsonl').read_text())
            self.assertTrue(receipt['passed'])
            self.assertEqual(receipt['expected_count'], 2)
            with self.assertRaises(RuntimeError):
                execute(selected, 'neighbor-failure', fail_neighbor=True)
            full = selected | dict(execution_profile='periodic-full', scope='explicit-full',
                                   groups=['baseline', 'oasis7_required', 'formal_process'],
                                   formal_suites=list(artifacts.SHARDS), formal_shards=list(artifacts.SHARDS),
                                   formal_deferred_ordinary_cases={})
            legacy = {key: value for key, value in selected.items()
                      if key not in ('formal_full_profiles_only_version', 'formal_deferred_ordinary_cases')}
            for name, selection in [('full', full), ('old-base', legacy)]:
                with self.subTest(profile=name), self.assertRaises(RuntimeError):
                    execute(selection, name)
                self.assertTrue((root / 'heavy-ran').is_file())
                receipt = json.loads((root / (name + '.jsonl')).read_text())
                self.assertFalse(receipt['passed'])
                self.assertEqual(receipt['expected_count'], 3)

    def test_real_native_partitions_preserve_feature_isolation_and_complete_harnesses(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            (root / 'Cargo.toml').write_text('[workspace]\nmembers=["runtime","world"]\nresolver="2"\n')
            for directory, package in (('runtime', 'oasis7_runtime'), ('world', 'oasis7_world_state')):
                member = root / directory
                (member / 'src').mkdir(parents=True)
                (member / 'tests').mkdir()
                manifest = ('[package]\nname="' + package + '"\nversion="0.1.0"\nedition="2021"\n'
                            '[features]\ndefault=["bad-default"]\nbad-default=[]\ntest_tier_required=[]\ntest-support=[]\n')
                if directory == 'runtime':
                    manifest += ('[dev-dependencies]\noasis7_world_state={path="../world",'
                                 'default-features=false,features=["test-support"]}\n')
                (member / 'Cargo.toml').write_text(manifest)
                source = ('pub fn test_support_enabled() -> bool { cfg!(feature="test-support") }\n'
                          '#[test] fn required_case() { assert!(cfg!(feature="test_tier_required")); '
                          'assert!(!cfg!(feature="bad-default")); CHECK }\n'
                          '#[test] fn added_regression() { assert_eq!(2 + 2, 4); }\n'
                          '#[test] #[ignore] fn ignored_case() { panic!("ignored inventory changed"); }\n')
                if directory == 'runtime':
                    source = source.replace('CHECK', 'assert!(oasis7_world_state::test_support_enabled());')
                else:
                    source = ('#[cfg(all(test, feature="test-support"))]\n'
                              'compile_error!("cross-package test-support leaked into WorldState root");\n'
                              + source.replace('CHECK', 'assert!(!test_support_enabled());'))
                (member / 'src/lib.rs').write_text(source)
                (member / 'tests/pr_fast.rs').write_text(
                    '#[test] fn required_case() { assert!(cfg!(feature="test_tier_required")); }\n'
                    '#[test] fn added_integration() { assert_eq!(3 + 3, 6); }\n')
            config = root / 'scripts/ci-formal-process-suites.json'
            config.parent.mkdir()
            config.write_text('{}')
            subprocess.run(['cargo', 'generate-lockfile', '--offline'], cwd=root, check=True,
                           stdout=subprocess.PIPE, stderr=subprocess.PIPE)
            for command in (['git', 'init', '-q'], ['git', 'add', '.'],
                            ['git', '-c', 'user.name=Fixture', '-c', 'user.email=fixture@example.invalid',
                             'commit', '-qm', 'fixture']):
                subprocess.run(command, cwd=root, check=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
            sha = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, text=True).strip()
            plan = native_package_partition_selection() | {
                'source_sha': sha, 'tested_sha': sha, 'policy_sha': sha,
                'architecture': dict(source=sha, tested=sha)}
            for target in plan['ordinary_fast_targets']:
                target['cases'] = ['required_case']
            # The negative control proves this real fixture detects the feature
            # union which independent package builds must continue to avoid.
            merged = subprocess.run(['cargo', 'test', '--locked', '-p', 'oasis7_runtime', '-p', 'oasis7_world_state',
                '--lib', '--no-default-features', '--features', 'test_tier_required', '--no-run'],
                cwd=root, text=True, capture_output=True)
            self.assertNotEqual(merged.returncode, 0)
            self.assertIn('cross-package test-support leaked', merged.stderr)
            observed = []
            with patch.object(artifacts, 'ROOT', root), patch.object(artifacts, 'CONFIG', config), \
                    patch.object(artifacts, 'native_cpu_budget', return_value=4), \
                    patch.dict(os.environ, SOURCE_SHA=sha):
                for partition in plan['ordinary_partitions']:
                    bundle = root / ('bundle-' + partition)
                    artifacts.build(bundle, 'ordinary', plan=plan, ordinary_partition=partition)
                    manifest = json.loads((bundle / 'manifest.json').read_text())
                    self.assertEqual(manifest['execution_plan'], plan)
                    self.assertEqual(manifest['ordinary_partition'], partition)
                    binaries = [binary for binary in manifest['binaries'].values() if binary['ordinary']]
                    self.assertEqual(len(binaries), 2)
                    self.assertTrue(all(binary['package'] == partition for binary in binaries))
                    self.assertEqual(sorted(len(binary['ignored_tests']) for binary in binaries), [0, 1])
                    observed.extend(binary['variant'] for binary in binaries)
                    archive = root / ('partial-' + partition + '.tar.gz')
                    with self.assertRaisesRegex(ValueError, 'complete bundles'):
                        artifacts.pack(bundle, archive, suite='ordinary', plan=plan)
                    self.assertFalse(archive.exists())
                    results = root / (partition + '.jsonl')
                    artifacts.execute(bundle, 'ordinary', results, plan=plan, ordinary_partition=partition)
                    records = [json.loads(line) for line in results.read_text().splitlines()]
                    self.assertEqual(len(records), 2)
                    self.assertTrue(all(record['passed'] and record['expected_count'] == 2 for record in records))
            self.assertEqual(sorted(observed), ['required-fast-' + str(index) for index in range(4)])

    def test_real_selected_build_keeps_required_features_and_excludes_unrelated_package(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            (root / 'Cargo.toml').write_text('[workspace]\nmembers=["codec","unrelated"]\nresolver="2"\n')
            for directory, package, source in (
                ('codec', 'oasis7_viewer_codec', '#[cfg(feature="test_tier_required")] #[test] fn round_trip() { assert_eq!(1 + 1, 2); }\n'
                 '#[cfg(feature="test_tier_full")] #[test] fn extended_round_trip() { assert_eq!(2 + 2, 4); }'),
                ('unrelated', 'oasis7_runtime', 'compile_error!("unrelated target was incorrectly selected");'),
            ):
                member = root / directory
                (member / 'src').mkdir(parents=True)
                (member / 'Cargo.toml').write_text('[package]\nname="' + package +
                    '"\nversion="0.1.0"\nedition="2021"\n[features]\ntest_tier_required=[]\ntest_tier_full=[]\n')
                (member / 'src/lib.rs').write_text(source)
            config = root / 'scripts/ci-formal-process-suites.json'
            config.parent.mkdir()
            config.write_text('{}')
            subprocess.run(['cargo', 'generate-lockfile', '--offline'], cwd=root, check=True,
                           stdout=subprocess.PIPE, stderr=subprocess.PIPE)
            for command in (['git', 'init', '-q'], ['git', 'add', '.'],
                            ['git', '-c', 'user.name=Fixture', '-c', 'user.email=fixture@example.invalid',
                             'commit', '-qm', 'fixture']):
                subprocess.run(command, cwd=root, check=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
            sha = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, text=True).strip()
            plan = FastExecutionSelection().selection()
            plan['architecture'] = dict(source=sha, tested=sha)
            with patch.object(artifacts, 'ROOT', root), patch.object(artifacts, 'CONFIG', config), \
                 patch.dict(os.environ, SOURCE_SHA=sha):
                for expanded, expected_count in ((False, 1), (True, 2)):
                    plan['test_policy_changed'] = expanded
                    bundle, consumer = root / ('producer-' + str(expanded)), root / ('consumer-' + str(expanded))
                    artifacts.build(bundle, 'ordinary', plan=plan)
                    manifest = json.loads((bundle / 'manifest.json').read_text())
                    self.assertEqual({entry['package'] for entry in manifest['binaries'].values()}, {'oasis7_viewer_codec'})
                    archive_path = root / ('ordinary-' + str(expanded) + '.tar.gz')
                    artifacts.pack(bundle, archive_path, suite='ordinary', plan=plan)
                    with tarfile.open(archive_path) as archive:
                        archive.extractall(consumer, filter='data')
                    results = root / ('results-' + str(expanded) + '.jsonl')
                    artifacts.execute(consumer, 'ordinary', results, plan=plan)
                    records = [json.loads(line) for line in results.read_text().splitlines()]
                    self.assertEqual(len(records), 1)
                    self.assertTrue(records[0]['passed'])
                    self.assertEqual(records[0]['expected_count'], expected_count)
                plan['oasis7_required_packages'].append('oasis7_runtime')
                with self.assertRaisesRegex(RuntimeError, 'build failed'):
                    artifacts.build(root / 'expanded', 'ordinary', plan=plan)

    def prepare_target_workspace(self, root, source, integration=None, unrelated_source=None, worker=False):
        (root / 'Cargo.toml').write_text('[workspace]\nmembers=["codec","unrelated"]\nresolver="2"\n')
        for directory, package, text in (
            ('codec', 'oasis7_viewer_codec', source),
            ('unrelated', 'oasis7_runtime', unrelated_source if unrelated_source is not None
             else 'compile_error!("unselected package was compiled");'),
        ):
            member = root / directory
            (member / 'src').mkdir(parents=True)
            (member / 'Cargo.toml').write_text('[package]\nname="' + package +
                '"\nversion="0.1.0"\nedition="2021"\n[features]\ndefault=[]\ntest_tier_required=[]\n')
            (member / 'src/lib.rs').write_text(text)
        if integration is not None:
            (root / 'codec/tests').mkdir()
            (root / 'codec/tests/pr_fast.rs').write_text(integration)
            (root / 'codec/tests/heavy.rs').write_text('compile_error!("unselected integration was compiled");')
        if worker and sys.platform == 'linux' and hasattr(artifacts, 'RUNTIME_WORKER_VARIANT'):
            # Full Linux bundles must deliver a genuine Cargo-built companion,
            # even though this fixture's executed harnesses belong to codec.
            manifest = root / 'Cargo.toml'
            manifest.write_text(manifest.read_text().replace('"unrelated"]', '"unrelated","worker"]'))
            (root / 'worker/src').mkdir(parents=True)
            (root / 'worker/Cargo.toml').write_text(
                '[package]\nname="oasis7_wasm_executor"\nversion="0.1.0"\nedition="2021"\n'
                '[features]\nwasmtime=[]\n[[bin]]\nname="oasis7-wasm-worker"\npath="src/main.rs"\n')
            (root / 'worker/src/main.rs').write_text('fn main() {}\n')
        config = root / 'scripts/ci-formal-process-suites.json'
        config.parent.mkdir()
        config.write_text('{}')
        subprocess.run(['cargo', 'generate-lockfile', '--offline'], cwd=root, check=True,
                       stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        for command in (['git', 'init', '-q'], ['git', 'add', '.'],
                        ['git', '-c', 'user.name=Fixture', '-c', 'user.email=fixture@example.invalid',
                         'commit', '-qm', 'fixture']):
            subprocess.run(command, cwd=root, check=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        sha = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, text=True).strip()
        return config, sha

    def test_real_scale_risk_build_executes_full_once_and_propagates_failure(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            config, _ = self.prepare_target_workspace(root, '#[test] fn required_lib() {}\n')
            manifest_path = root / 'codec/Cargo.toml'
            manifest_path.write_text(manifest_path.read_text() + 'test_tier_full=["test_tier_required"]\n'
                '\n[[test]]\nname="scale"\npath="tests/scale.rs"\nrequired-features=["test_tier_full"]\n')
            (root / 'codec/tests').mkdir()
            (root / 'codec/tests/required.rs').write_text(
                '#[test] fn required_chunk() { assert_eq!(2 * 65536 + 17, 131089); }\n')
            (root / 'codec/tests/scale.rs').write_text('''
#[cfg(not(feature="test_tier_full"))]
compile_error!("full scale target must not be compiled by the required tuple");
#[test] fn full_scale() {
    std::fs::write(std::env::var("SCALE_WITNESS").unwrap(), "full executed").unwrap();
    assert!(std::env::var_os("FAIL_SCALE").is_none(), "real full scale failure");
}
''')
            subprocess.run(['git', 'add', 'codec'], cwd=root, check=True)
            subprocess.run(['git', '-c', 'user.name=Fixture', '-c', 'user.email=fixture@example.invalid',
                            'commit', '-qm', 'scale contract'], cwd=root, check=True, stdout=subprocess.PIPE)
            sha = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, text=True).strip()
            for risk, expanded, total in ((False, False, 2), (True, False, 3), (True, True, 3)):
                selected = RiskTargetInventory().selection() if risk else FastExecutionSelection().selection()
                selected.update(source_sha=sha, tested_sha=sha, policy_sha=sha,
                                architecture=dict(source=sha, tested=sha), test_policy_changed=expanded)
                name = f'{risk}-{expanded}'
                witness = root / ('witness-' + name)
                with patch.object(artifacts, 'ROOT', root), patch.object(artifacts, 'CONFIG', config), \
                     patch.dict(os.environ, SOURCE_SHA=sha, SCALE_WITNESS=str(witness)):
                    bundle, consumer = root / ('build-' + name), root / ('consumer-' + name)
                    artifacts.build(bundle, 'ordinary', plan=selected)
                    archive = root / (name + '.tar.gz')
                    artifacts.pack(bundle, archive, suite='ordinary', plan=selected)
                    with tarfile.open(archive) as compressed:
                        compressed.extractall(consumer, filter='data')
                    results = root / (name + '.jsonl')
                    artifacts.execute(consumer, 'ordinary', results, plan=selected)
                    receipts = [json.loads(line) for line in results.read_text().splitlines()]
                    self.assertTrue(all(record['passed'] and record['count_ok'] for record in receipts))
                    self.assertEqual(sum(record['expected_count'] for record in receipts), total)
                    self.assertEqual(witness.exists(), risk)
                    manifest = json.loads((bundle / 'manifest.json').read_text())
                    self.assertEqual(sum('full_scale' in binary.get('tests', [])
                                         for binary in manifest['binaries'].values()), int(risk))
                    original = [binary for binary in manifest['binaries'].values()
                                if binary.get('variant') == 'required' and binary.get('target') == 'scale']
                    self.assertEqual(len(original), int(expanded))
                    if expanded:
                        self.assertEqual(original[0]['tests'], ['full_scale'])
                    self.assertEqual(sum('required_chunk' in binary.get('tests', [])
                                         for binary in manifest['binaries'].values()), 1)
                    if risk and not expanded:
                        with patch.dict(os.environ, FAIL_SCALE='1'), self.assertRaises(RuntimeError):
                            artifacts.execute(consumer, 'ordinary', root / 'failed.jsonl', plan=selected)

    def test_real_target_build_excludes_heavy_harnesses_and_runs_new_nonignored_cases(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            integration = ('#[cfg(feature="test_tier_required")] mod required {\n'
                           '  #[test] fn round_trip() { assert_eq!(1 + 1, 2); }\n'
                           '  #[test] fn newly_added() { assert_eq!(2 + 2, 4); }\n'
                           '  #[test] #[ignore] fn expensive() { panic!("ignored test ran"); }\n}\n')
            config, sha = self.prepare_target_workspace(root,
                '#[cfg(test)] compile_error!("unselected lib harness was compiled");\n'
                '#[cfg(feature="default")] compile_error!("default feature was incorrectly enabled");\n', integration)
            selected = FastTargetInventory().selection()
            selected['architecture'] = dict(source=sha, tested=sha)
            with patch.object(artifacts, 'ROOT', root), patch.object(artifacts, 'CONFIG', config), \
                 patch.dict(os.environ, SOURCE_SHA=sha):
                bundle, consumer = root / 'producer', root / 'consumer'
                artifacts.build(bundle, 'ordinary', plan=selected)
                manifest = json.loads((bundle / 'manifest.json').read_text())
                self.assertEqual(len(manifest['binaries']), 1)
                binary = next(iter(manifest['binaries'].values()))
                self.assertEqual(binary['target_kind'], ['test'])
                self.assertEqual(binary['target'], 'pr_fast')
                self.assertEqual(binary['features'], ['test_tier_required'])
                archive_path = root / 'ordinary.tar.gz'
                artifacts.pack(bundle, archive_path, suite='ordinary', plan=selected)
                with tarfile.open(archive_path) as archive:
                    archive.extractall(consumer, filter='data')
                results = root / 'results.jsonl'
                artifacts.execute(consumer, 'ordinary', results, plan=selected)
                records = [json.loads(line) for line in results.read_text().splitlines()]
                self.assertEqual(len(records), 1)
                self.assertTrue(records[0]['passed'])
                self.assertEqual(records[0]['expected_count'], 2)
                # Minimum case identities are checked against the actual binary,
                # including when the recorded manifest claims a case still exists.
                altered = copy.deepcopy(manifest)
                next(iter(altered['binaries'].values()))['tests'].append('invented_case')
                (consumer / 'manifest.json').write_text(json.dumps(altered))
                with self.assertRaisesRegex(ValueError, 'downloaded test inventory'):
                    artifacts.load(consumer, suite='ordinary', plan=selected)
                for label, rewritten in (
                    ('renamed', integration.replace('fn round_trip()', 'fn renamed()')),
                    ('ignored', integration.replace('#[test] fn round_trip()', '#[test] #[ignore] fn round_trip()')),
                ):
                    (root / 'codec/tests/pr_fast.rs').write_text(rewritten)
                    with self.subTest(label=label), self.assertRaisesRegex(ValueError, 'minimum case inventory'):
                        artifacts.build(root / label, 'ordinary', plan=selected)

    def test_real_inline_combined_build_keeps_each_original_target_and_features(self):
        source = '''
#[cfg(feature="default")] compile_error!("default feature was incorrectly enabled");
#[cfg(all(test,feature="test_tier_required"))] mod required {
    #[test] fn round_trip() { assert_eq!(1 + 1, 2); }
    #[test] fn newly_added() { assert_eq!(2 + 2, 4); }
    #[test] #[ignore] fn expensive() { panic!("ignored test ran"); }
}
'''
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            config, sha = self.prepare_target_workspace(root, source, source)
            target = FastTargetInventory().target
            selected = InlineExecutionSelection().selection() | {
                'architecture': dict(source=sha, tested=sha),
                'ordinary_fast_targets': [target(), target(target_kind='lib', target_name='oasis7_viewer_codec')],
            }
            with patch.object(artifacts, 'ROOT', root), patch.object(artifacts, 'CONFIG', config), \
                 patch.object(artifacts, 'native_cpu_budget', return_value=2), \
                 patch.dict(os.environ, SOURCE_SHA=sha):
                bundle = root / 'bundle'
                artifacts.build(bundle, 'ordinary', plan=selected)
                manifest = json.loads((bundle / 'manifest.json').read_text())
                self.assertEqual(len(manifest['build_timings']), 1)
                binaries = list(manifest['binaries'].values())
                self.assertEqual({binary['variant'] for binary in binaries}, {'required-fast-0', 'required-fast-1'})
                self.assertEqual({tuple(binary['target_kind']) for binary in binaries}, {('lib',), ('test',)})
                self.assertTrue(all(binary['features'] == ['test_tier_required'] for binary in binaries))
                self.assertTrue(all(binary['default_features'] is False for binary in binaries))
                results = root / 'results.jsonl'
                artifacts.execute(bundle, 'ordinary', results, plan=selected)
                records = [json.loads(line) for line in results.read_text().splitlines()]
                self.assertEqual(len(records), 2)
                self.assertTrue(all(record['passed'] and record['expected_count'] == 2 for record in records))
                altered = copy.deepcopy(manifest)
                del altered['binaries'][next(iter(altered['binaries']))]
                (bundle / 'manifest.json').write_text(json.dumps(altered))
                with patch('ci_process_runner.run') as execute_test:
                    with self.assertRaisesRegex(ValueError, 'target inventory'):
                        artifacts.execute(bundle, 'ordinary', root / 'bad-results.jsonl', plan=selected)
                    execute_test.assert_not_called()

    def test_ordinary_build_strips_staged_elf_and_keeps_original_target_intact(self):
        if sys.platform != 'linux' or shutil.which('strip') is None:
            return
        source = '''
#[cfg(feature="default")] compile_error!("default feature was incorrectly enabled");
#[cfg(all(test,feature="test_tier_required"))] mod required {
    #[test] fn round_trip() { assert_eq!(1 + 1, 2); }
    #[test] fn newly_added() { assert_eq!(2 + 2, 4); }
}
'''
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            config, sha = self.prepare_target_workspace(root, source, source)
            target = FastTargetInventory().target
            selected = InlineExecutionSelection().selection() | {
                'architecture': dict(source=sha, tested=sha),
                'ordinary_fast_targets': [target()],
            }
            with patch.object(artifacts, 'ROOT', root), patch.object(artifacts, 'CONFIG', config), \
                 patch.object(artifacts, 'native_cpu_budget', return_value=2), \
                 patch.dict(os.environ, SOURCE_SHA=sha):
                bundle = root / 'bundle'
                artifacts.build(bundle, 'ordinary', plan=selected)
                manifest = json.loads((bundle / 'manifest.json').read_text())
                entry = next(iter(manifest['binaries'].values()))
                staged = bundle / entry['path']
                original = Path(entry['original_executable'])
                self.assertEqual(entry['sha256'], artifacts.digest(staged))
                self.assertEqual(entry['bytes'], staged.stat().st_size)
                self.assertLess(staged.stat().st_size, original.stat().st_size)
                self.assertNotEqual(artifacts.digest(staged), artifacts.digest(original))
                readelf = shutil.which('readelf')
                if readelf is not None:
                    original_sections = subprocess.check_output([readelf, '-SW', str(original)], text=True)
                    staged_sections = subprocess.check_output([readelf, '-SW', str(staged)], text=True)
                    self.assertIn('.symtab', original_sections)
                    self.assertNotIn('.symtab', staged_sections)
                results = root / 'results.jsonl'
                artifacts.execute(bundle, 'ordinary', results, plan=selected)
                records = [json.loads(line) for line in results.read_text().splitlines()]
                self.assertEqual(len(records), 1)
                self.assertTrue(records[0]['passed'] and records[0]['expected_count'] == 2)

    def test_real_inline_harnesses_overlap_without_dropping_new_cases(self):
        def source(name, other):
            return '''
#[cfg(feature="default")] compile_error!("default feature was incorrectly enabled");
#[cfg(all(test,feature="test_tier_required"))] mod required {
    #[test] fn round_trip() {
        let root = std::path::PathBuf::from(std::env::var("OASIS7_CI_NATIVE_PARALLEL_FIXTURE").unwrap());
        std::fs::write(root.join("SELF.started"), b"started").unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while !root.join("OTHER.started").is_file() {
            assert!(std::time::Instant::now() < deadline, "other full harness never started");
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }
    #[test] fn newly_added() {
        let root = std::path::PathBuf::from(std::env::var("OASIS7_CI_NATIVE_PARALLEL_FIXTURE").unwrap());
        assert_eq!(std::env::var("RUST_TEST_THREADS").unwrap(), "1");
        std::fs::write(root.join("SELF.new"), b"executed").unwrap();
    }
    #[test] #[ignore] fn expensive() { panic!("ignored test ran"); }
}
'''.replace('SELF', name).replace('OTHER', other)
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            config, sha = self.prepare_target_workspace(root, source('lib', 'integration'),
                                                       source('integration', 'lib'))
            target = FastTargetInventory().target
            selected = InlineExecutionSelection().selection() | {
                'architecture': dict(source=sha, tested=sha),
                'ordinary_fast_targets': [target(), target(target_kind='lib', target_name='oasis7_viewer_codec')],
            }
            with patch.object(artifacts, 'ROOT', root), patch.object(artifacts, 'CONFIG', config), \
                 patch.object(artifacts, 'native_cpu_budget', return_value=2), \
                 patch.dict(os.environ, SOURCE_SHA=sha, OASIS7_CI_NATIVE_PARALLEL_FIXTURE=str(root)):
                bundle = root / 'bundle'
                artifacts.build(bundle, 'ordinary', plan=selected)
                results = root / 'results.jsonl'
                artifacts.execute(bundle, 'ordinary', results, plan=selected)
                records = [json.loads(line) for line in results.read_text().splitlines()]
                self.assertEqual(len(records), 2)
                self.assertTrue(all(record['passed'] and record['expected_count'] == 2 for record in records))
                self.assertTrue(all((root / (name + suffix)).is_file()
                                    for name in ('lib', 'integration') for suffix in ('.started', '.new')))

    def test_real_inline_libraries_start_together_before_validated_integrations(self):
        def source(name, other=None):
            barrier = ('''
        std::fs::write(root.join("SELF.started"), b"started").unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while !root.join("OTHER.started").is_file() {
            assert!(std::time::Instant::now() < deadline, "other library never started");
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
''' if other else '''
        assert!(root.join("codec.started").is_file(), "codec library was delayed");
        assert!(root.join("runtime.started").is_file(), "runtime library was delayed");
''')
            return ('''
#[cfg(feature="default")] compile_error!("default feature was incorrectly enabled");
#[cfg(all(test,feature="test_tier_required"))] mod required {
    #[test] fn round_trip() {
        let root = std::path::PathBuf::from(std::env::var("OASIS7_CI_NATIVE_PRIORITY_FIXTURE").unwrap());
        BARRIER
    }
    #[test] fn newly_added() {
        let root = std::path::PathBuf::from(std::env::var("OASIS7_CI_NATIVE_PRIORITY_FIXTURE").unwrap());
        assert_eq!(std::env::var("RUST_TEST_THREADS").unwrap(), "1");
        std::fs::write(root.join("SELF.new"), b"executed").unwrap();
    }
    #[test] #[ignore] fn expensive() { panic!("ignored test ran"); }
}
'''.replace('BARRIER', barrier).replace('SELF', name).replace('OTHER', other or 'unused'))
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            config, sha = self.prepare_target_workspace(root, source('codec', 'runtime'),
                source('integration'), unrelated_source=source('runtime', 'codec'), worker=True)
            target = FastTargetInventory().target
            selected = InlineExecutionSelection().selection() | {
                'architecture': dict(source=sha, tested=sha),
                'ordinary_fast_targets': [target(),
                    target(target_kind='lib', target_name='oasis7_viewer_codec'),
                    target(package='oasis7_runtime', target_kind='lib', target_name='oasis7_runtime')],
            }
            with patch.object(artifacts, 'ROOT', root), patch.object(artifacts, 'CONFIG', config), \
                 patch.object(artifacts, 'native_cpu_budget', return_value=2), \
                 patch.dict(os.environ, SOURCE_SHA=sha, OASIS7_CI_NATIVE_PRIORITY_FIXTURE=str(root)):
                bundle = root / 'bundle'
                artifacts.build(bundle, 'ordinary', plan=selected)
                manifest_path = bundle / 'manifest.json'
                manifest = json.loads(manifest_path.read_text())
                # Compiler completion order is not the desired dispatch order.
                # Reproduce the hosted integration-before-library inventory.
                manifest['binaries'] = dict(sorted(manifest['binaries'].items(),
                    key=lambda item: item[1]['target_kind'] == ['lib']))
                manifest_path.write_text(json.dumps(manifest))
                results = root / 'results.jsonl'
                artifacts.execute(bundle, 'ordinary', results, plan=selected)
                records = [json.loads(line) for line in results.read_text().splitlines()]
                self.assertEqual(len(records), 3)
                self.assertTrue(all(record['passed'] and record['expected_count'] == 2 for record in records))
                self.assertTrue(all((root / (name + '.new')).is_file()
                                    for name in ('codec', 'runtime', 'integration')))
                integration = next(name for name, binary in manifest['binaries'].items()
                                   if binary['target_kind'] == ['test'])
                # Deferred integration validation must still finish before any
                # priority library can launch, including its actual --list data.
                for damage in ('inventory', 'features', 'source'):
                    altered = copy.deepcopy(manifest)
                    if damage == 'inventory':
                        altered['binaries'][integration]['tests'].append('invented_case')
                    elif damage == 'features':
                        altered['binaries'][integration]['features'].append('default')
                    else:
                        altered['source_sha'] = '0' * 40
                    manifest_path.write_text(json.dumps(altered))
                    with self.subTest(damage=damage), patch('ci_process_runner.run') as execute_test:
                        with self.assertRaises(ValueError):
                            artifacts.execute(bundle, 'ordinary', root / 'bad-results.jsonl', plan=selected)
                        execute_test.assert_not_called()

    def test_real_full_bundle_filters_only_the_native_supplement_variant(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            source = '''
fn witness(name: &str) {
    let owner = if cfg!(feature="default") { "original" } else { "supplement" };
    let path = std::path::PathBuf::from(std::env::var("FULL_PREFIX_WITNESS").unwrap()).join(format!("{owner}-{name}"));
    std::fs::OpenOptions::new().write(true).create_new(true).open(path).unwrap();
}
mod family {
    #[test] fn minimum() { super::witness("minimum"); }
    #[test] fn new_case() { super::witness("new_case"); }
    mod nested { #[test] fn new_case() { super::super::witness("nested_new_case"); } }
    #[cfg(not(feature="default"))] #[test] fn native_only() { super::witness("native_only"); }
}
#[test] fn outside_contract() {
    assert!(cfg!(feature="default"), "native supplement must retain its registered module contract");
    witness("outside_contract");
}
#[cfg(feature="default")] #[test] fn original_default() { witness("original_default"); }
#[cfg(feature="test_tier_full")] #[test] fn original_full() {
    witness("original_full");
    assert!(std::env::var_os("FULL_PREFIX_FAIL").is_none());
}
'''
            config, _ = self.prepare_target_workspace(root, source, worker=True)
            package = root / 'codec/Cargo.toml'
            package.write_text(package.read_text() + 'test_tier_full=["test_tier_required"]\n')
            subprocess.run(['git', 'add', 'codec/Cargo.toml'], cwd=root, check=True)
            subprocess.run(['git', '-c', 'user.name=Fixture', '-c', 'user.email=fixture@example.invalid',
                            'commit', '-qm', 'full feature fixture'], cwd=root, check=True, stdout=subprocess.PIPE)
            sha = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, text=True).strip()
            target = FastTargetInventory().target(target_kind='lib', target_name='oasis7_viewer_codec',
                cases=['family::minimum'], case_prefixes=['family::'])
            selected = FastTargetInventory().selection([target], full=True) | dict(
                ordinary_case_prefix_contract_version=1, test_policy_changed=True,
                architecture=dict(source=sha, tested=sha))
            with patch.object(artifacts, 'ROOT', root), patch.object(artifacts, 'CONFIG', config), \
                 patch.object(artifacts, 'required_packages', return_value=['oasis7_viewer_codec']), \
                 patch.dict(os.environ, SOURCE_SHA=sha):
                bundle, consumer = root / 'producer', root / 'consumer'
                artifacts.build(bundle, 'ordinary', plan=selected)
                manifest = json.loads((bundle / 'manifest.json').read_text())
                original = next(name for name, entry in manifest['binaries'].items() if entry['variant'] == 'required')
                supplement = next(name for name, entry in manifest['binaries'].items()
                                  if entry['variant'] == 'required-full-fast-0')
                self.assertEqual(len(manifest['binaries']), 2)
                self.assertEqual({(entry['package'], tuple(entry['target_kind']), entry['target'])
                                  for entry in manifest['binaries'].values()},
                                 {('oasis7_viewer_codec', ('lib',), 'oasis7_viewer_codec')})
                self.assertIn('original_full', manifest['binaries'][original]['tests'])
                self.assertIn('outside_contract', manifest['binaries'][supplement]['tests'])
                archive_path = root / 'ordinary.tar.gz'
                artifacts.pack(bundle, archive_path, suite='ordinary', plan=selected)
                with tarfile.open(archive_path) as archive:
                    archive.extractall(consumer, filter='data')
                expected = {'original-' + case for case in (
                    'minimum', 'new_case', 'nested_new_case', 'outside_contract', 'original_default', 'original_full')}
                expected |= {'supplement-' + case for case in ('minimum', 'new_case', 'nested_new_case', 'native_only')}
                for fail in (False, True):
                    witnesses = root / ('failed-witnesses' if fail else 'witnesses')
                    witnesses.mkdir()
                    results = root / ('failed-results.jsonl' if fail else 'results.jsonl')
                    environment = dict(FULL_PREFIX_WITNESS=str(witnesses))
                    if fail:
                        environment['FULL_PREFIX_FAIL'] = '1'
                    with self.subTest(fail=fail), patch.dict(os.environ, environment):
                        if fail:
                            with self.assertRaisesRegex(RuntimeError, 'ordinary failed'):
                                artifacts.execute(consumer, 'ordinary', results, plan=selected)
                        else:
                            artifacts.execute(consumer, 'ordinary', results, plan=selected)
                    records = {record['test_name']: record for record in map(json.loads, results.read_text().splitlines())}
                    self.assertEqual({name: record['expected_count'] for name, record in records.items()},
                                     {original: 6, supplement: 4})
                    self.assertEqual(records[original]['passed'], not fail)
                    self.assertTrue(records[supplement]['passed'])
                    self.assertEqual({path.name for path in witnesses.iterdir()}, expected)
                for damage in ('wrong-index', 'missing-supplement', 'missing-original', 'missing-minimum'):
                    changed = copy.deepcopy(manifest)
                    if damage == 'wrong-index':
                        changed['binaries'][supplement]['variant'] = 'required-full-fast-1'
                    elif damage == 'missing-supplement':
                        del changed['binaries'][supplement]
                    elif damage == 'missing-original':
                        del changed['binaries'][original]
                    else:
                        changed['binaries'][supplement]['tests'].remove('family::minimum')
                    with self.subTest(damage=damage), self.assertRaises(ValueError):
                        artifacts.validate_ordinary_selection(changed)

    def test_real_full_bundle_runs_default_and_no_default_variants_of_same_lib(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            config, sha = self.prepare_target_workspace(root,
                '#[test] fn common() {}\n'
                '#[cfg(feature="default")] #[test] fn original_default() {}\n'
                '#[cfg(not(feature="default"))] #[test] fn fast_only() {}\n'
                '#[cfg(not(feature="default"))] #[test] fn newly_added() {}\n', worker=True)
            target = FastTargetInventory().target(target_kind='lib', target_name='oasis7_viewer_codec', cases=['fast_only'])
            selected = FastTargetInventory().selection([target], full=True)
            selected.update(ordinary_artifact_partition_version=1,
                            ordinary_partitions=list(artifacts.ORDINARY_ARTIFACT_PARTITIONS))
            selected['architecture'] = dict(source=sha, tested=sha)
            # Restrict only the old package inventory to this real tiny workspace;
            # build, Cargo feature resolution, pack, download checks and run stay real.
            with patch.object(artifacts, 'ROOT', root), patch.object(artifacts, 'CONFIG', config), \
                 patch.object(artifacts, 'required_packages', return_value=['oasis7_viewer_codec']), \
                 patch.dict(os.environ, SOURCE_SHA=sha):
                bundle, consumer = root / 'producer', root / 'consumer'
                artifacts.build(bundle, 'ordinary', plan=selected)
                manifest = json.loads((bundle / 'manifest.json').read_text())
                if sys.platform == 'linux' and hasattr(artifacts, 'RUNTIME_WORKER_VARIANT'):
                    companion = manifest['worker_companion']
                    self.assertEqual(companion['package'], 'oasis7_wasm_executor')
                    self.assertEqual(companion['target'], 'oasis7-wasm-worker')
                    self.assertEqual(companion['requested_features'], ['wasmtime'])
                    self.assertTrue((bundle / companion['path']).is_file())
                self.assertEqual(len(manifest['binaries']), 2)
                self.assertEqual({entry['target'] for entry in manifest['binaries'].values()}, {'oasis7_viewer_codec'})
                self.assertEqual(sorted('default' in entry['features'] for entry in manifest['binaries'].values()), [False, True])
                archive_path = root / 'ordinary.tar.gz'
                artifacts.pack(bundle, archive_path, suite='ordinary', plan=selected)
                with tarfile.open(archive_path) as archive:
                    archive.extractall(consumer, filter='data')
                for partition in ('component-libs', None):
                    results = root / f'results-{partition}.jsonl'
                    artifacts.execute(consumer, 'ordinary', results, plan=selected, ordinary_partition=partition)
                    records = [json.loads(line) for line in results.read_text().splitlines()]
                    self.assertEqual(sorted(record['expected_count'] for record in records), [2, 3])
                    self.assertTrue(all(record['passed'] for record in records))
                original = json.loads((consumer / 'manifest.json').read_text())
                for logical in manifest['binaries']:
                    altered = copy.deepcopy(original)
                    altered['included_binaries'].remove(logical)
                    (consumer / 'manifest.json').write_text(json.dumps(altered))
                    with self.subTest(missing=logical), self.assertRaisesRegex(ValueError, 'executable closure'):
                        artifacts.load(consumer, suite='ordinary', plan=selected)
                (consumer / 'manifest.json').write_text(json.dumps(original))
                for logical, entry in manifest['binaries'].items():
                    path = consumer / entry['path']
                    data = path.read_bytes()
                    path.unlink()
                    try:
                        with self.subTest(missing_file=logical), self.assertRaisesRegex(ValueError, 'corrupt executable'):
                            artifacts.load(consumer, suite='ordinary', plan=selected)
                    finally:
                        path.write_bytes(data)
                        path.chmod(0o755)

    def test_absolute_timeout_is_not_reset_by_heartbeats(self):
        result = self.execute([sys.executable, '-c', 'import time; time.sleep(30)'], count=None, timeout_seconds=0.2, heartbeat_seconds=0.05)
        self.assertTrue(result['timeout'])
        self.assertFalse(result['passed'])
        self.assertEqual(result['cleanup_result'], 'clean')
        self.assertLess(result['elapsed_seconds'], 3)

    def test_pre_cancelled_worker_never_starts_a_child(self):
        cancelled = threading.Event()
        cancelled.set()
        with patch('ci_process_runner.subprocess.Popen') as child:
            with self.assertRaisesRegex(InterruptedError, 'cancelled before'):
                self.execute([sys.executable, '-c', 'raise SystemExit(0)'], cancellation_event=cancelled)
            child.assert_not_called()

    def test_shared_cancellation_cannot_be_overridden_by_a_late_zero_exit(self):
        cancelled = threading.Event()
        real_popen = subprocess.Popen
        def start(*args, **kwargs):
            process = real_popen(*args, **kwargs)
            real_poll = process.poll
            def poll():
                code = real_poll()
                if code == 0:
                    cancelled.set()
                return code
            process.poll = poll
            return process
        with patch('ci_process_runner.subprocess.Popen', side_effect=start):
            result = self.execute([sys.executable, '-c',
                'print("test result: ok. 1 passed; 0 failed; 0 ignored;")'], cancellation_event=cancelled)
        self.assertTrue(result['cancelled'])
        self.assertFalse(result['passed'])
        self.assertEqual(result['cleanup_result'], 'clean')

    def test_cancelling_one_parallel_owner_preserves_other_and_unrelated_processes(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            cancelled, other_cancelled = threading.Event(), threading.Event()
            observing, resume_observation = threading.Event(), threading.Event()
            children = []
            owners = {}
            real_popen = subprocess.Popen
            foreign = real_popen([sys.executable, '-c', 'import time; time.sleep(20)'])
            def start(*args, **kwargs):
                process = real_popen(*args, **kwargs)
                children.append(process)
                role = kwargs['env']['OASIS7_NATIVE_FIXTURE_ROLE']
                owners[role] = process
                if role == 'first':
                    real_poll = process.poll
                    def poll():
                        if not observing.is_set():
                            observing.set()
                            if not resume_observation.wait(timeout=5):
                                raise RuntimeError('cancellation observation was not released')
                        return real_poll()
                    process.poll = poll
                return process
            def execute(name, cancellation):
                program = (
                    'from pathlib import Path; import os,signal,subprocess,sys,time\n'
                    f'root=Path({str(root)!r}); name={name!r}\n'
                    'if name == "first":\n'
                    '    child=subprocess.Popen([sys.executable,"-c","import time; time.sleep(.05)"])\n'
                    '    def finish(signum,frame):\n'
                    '        child.wait(timeout=2)\n'
                    '        (root/"first.reaped").write_text(str(child.returncode))\n'
                    '        print("test result: ok. 1 passed; 0 failed; 0 ignored;",flush=True)\n'
                    '        raise SystemExit(0)\n'
                    '    signal.signal(signal.SIGTERM,finish)\n'
                    '(root/(name+".ready")).write_text(str(os.getpid()))\n'
                    'while not (root/(name+".release")).exists(): time.sleep(0.01)\n'
                    'print("test result: ok. 1 passed; 0 failed; 0 ignored;")\n'
                )
                return run([sys.executable, '-c', program], root,
                           dict(os.environ, OASIS7_NATIVE_FIXTURE_ROLE=name), 'fixture', name,
                           root / (name + '.jsonl'), expected_count=1,
                           timeout_seconds=10, cancellation_event=cancellation)
            try:
                with patch('ci_process_runner.subprocess.Popen', side_effect=start):
                    with ThreadPoolExecutor(max_workers=2) as pool:
                        first = pool.submit(execute, 'first', cancelled)
                        other = pool.submit(execute, 'other', other_cancelled)
                        try:
                            deadline = time.monotonic() + 3
                            while not observing.is_set() or not all(
                                    (root / (name + '.ready')).exists() for name in ('first', 'other')):
                                self.assertLess(time.monotonic(), deadline, 'both children must really start')
                                time.sleep(0.01)
                            cancelled.set()
                            # Deliver TERM through the original Popen handle in
                            # this PID namespace. The fixture reaps its real
                            # descendant before supervision observes its zero
                            # exit; cancellation must still win. /proc may be
                            # mounted from the host namespace in local sandboxes.
                            owners['first'].terminate()
                            self.assertEqual(owners['first'].wait(timeout=3), 0)
                            self.assertEqual((root / 'first.reaped').read_text(), '0')
                            resume_observation.set()
                            first_result = first.result(timeout=6)
                            self.assertIsNone(foreign.poll(), 'unrelated same-user process was killed')
                            self.assertFalse(other.done(), 'other owner was cancelled or killed')
                            os.kill(int((root / 'other.ready').read_text()), 0)
                            (root / 'other.release').touch()
                            other_result = other.result(timeout=3)
                            self.assertTrue(first_result['cancelled'])
                            self.assertFalse(first_result['passed'])
                            self.assertEqual(first_result['exit_code'], 0)
                            self.assertTrue(first_result['count_ok'])
                            self.assertEqual(first_result['cleanup_result'], 'clean')
                            self.assertTrue(other_result['passed'])
                        finally:
                            resume_observation.set()
                            for name in ('first', 'other'):
                                (root / (name + '.release')).touch()
                            cancelled.set()
                            other_cancelled.set()
            finally:
                # These are the fixture's original Popen handles, never PIDs
                # inferred from another process or another owner's /proc view.
                for process in [*children, foreign]:
                    if process.poll() is None:
                        process.terminate()
                    process.wait(timeout=3)

    def test_fresh_consumer_restores_compiled_cargo_binary_path_and_prior_attempt(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp).resolve()
            original = root / 'target/debug/tool'
            original.parent.mkdir(parents=True)
            source = root / 'tool.rs'
            source.write_text('fn main() {}')
            compile_rust_fixture(source, original)
            harness = root / 'harness'
            source.write_text('#[test] fn calls_tool() { assert!(std::process::Command::new(env!("CARGO_BIN_EXE_tool")).status().unwrap().success()); }')
            compile_rust_fixture(source, harness, test=True,
                                 env=dict(os.environ, CARGO_BIN_EXE_tool=str(original)))
            bundle = root / 'bundle'
            bundle.mkdir()
            config_path = root / 'suites.json'
            config = dict(suites={'fixture': []}, environment_formal_batches={}, legacy_ignored_filters={}, shell_tests={})
            config_path.write_text(json.dumps(config))
            binaries = {}
            for logical in ['viewer', 'runtime', 'endpoint', 'runtime-tests']:
                destination = bundle / logical
                destination.write_bytes((harness if logical == 'runtime-tests' else original).read_bytes())
                destination.chmod(0o755)
                binaries[logical] = dict(path=logical, sha256=artifacts.digest(destination),
                    original_executable=str(harness if logical == 'runtime-tests' else original),
                    test=logical == 'runtime-tests', ordinary=logical == 'runtime-tests', cwd='.',
                    tests=['calls_tool'], ignored_tests=[])
            # Include a legitimate empty Cargo harness and the two original
            # default-feature regressions. Ordinary execution must skip only
            # the empty batch, while retaining both snapshot combinations.
            for logical, rust_source in {
                'empty-tests': '',
                'viewer-snapshot-tests': '#[cfg(test)] mod snapshot_progress { #[test] fn one() {} }',
                'snapshot-tests': '#[test] fn snapshot_player_gameplay_execution_state_backfills_from_legacy_fields() {}',
            }.items():
                source.write_text(rust_source)
                destination = bundle / logical
                compile_rust_fixture(source, destination, test=True)
                binaries[logical] = dict(path=logical, sha256=artifacts.digest(destination),
                    original_executable=str(destination), test=True, ordinary=logical == 'empty-tests', cwd='.',
                    tests=artifacts.inventory(destination), ignored_tests=[])
            original.unlink()
            manifest = dict(tested_sha='tested', source_sha='source', producer_run_id='run', producer_attempt='1',
                checkout=str(root), config_sha256=artifacts.digest(config_path),
                rust_toolchain=os.environ.get('RUST_TOOLCHAIN', 'local'), runner_image=os.environ.get('ImageOS', sys.platform),
                binaries=binaries, suites={'fixture': []}, libraries={})
            (bundle / 'manifest.json').write_text(json.dumps(manifest))
            with patch.object(artifacts, 'ROOT', root), patch.object(artifacts, 'CONFIG', config_path), patch.object(artifacts, 'output', return_value='tested'), patch.dict(os.environ, SOURCE_SHA='source', GITHUB_RUN_ID='run', GITHUB_RUN_ATTEMPT='2'):
                restored, env = artifacts.load(bundle)
                self.assertEqual(restored['producer_attempt'], '1')
                self.assertTrue(original.is_file())
                with patch('ci_process_runner.run', wraps=run) as child_runner:
                    artifacts.execute(bundle, 'ordinary', root / 'results.jsonl')
                    self.assertTrue(all('--test-threads=1' not in call.args[0] for call in child_runner.call_args_list))
                    self.assertTrue(all('--nocapture' not in call.args[0] for call in child_runner.call_args_list))
                    commands = [call.args[0] for call in child_runner.call_args_list]
                    self.assertIn([str(bundle / 'viewer-snapshot-tests'), 'snapshot_progress::'], commands)
                    self.assertIn([str(bundle / 'snapshot-tests'), 'snapshot_player_gameplay_execution_state_backfills_from_legacy_fields'], commands)
                results = [json.loads(line) for line in (root / 'results.jsonl').read_text().splitlines()]
                self.assertEqual(len(results), 3)
                self.assertTrue(all(result['passed'] for result in results))

    def test_proc_scan_never_reads_other_users_environment(self):
        class OtherUser:
            def stat(self):
                return type('Stat', (), {'st_uid': os.getuid()+1})()
            def __truediv__(self, name):
                raise AssertionError('must not read another user process')
        with patch.object(Path, 'glob', return_value=[OtherUser()]):
            self.assertEqual(owned_processes('owned-test'), {})

    def test_packed_moved_case_preserves_logical_suite_and_restores_child(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp).resolve()
            original = root / 'target/debug/tool'
            original.parent.mkdir(parents=True)
            source = root / 'fixture.rs'
            source.write_text('fn main() {}')
            compile_rust_fixture(source, original)
            bundle = root / 'bundle'
            bundle.mkdir()
            source.write_text('#[test] #[ignore] fn formal_acceptance() { assert!(std::process::Command::new(env!("CARGO_BIN_EXE_tool")).status().unwrap().success()); }\n#[test] fn ordinary() {}\n#[test] #[ignore] fn crash_child() {}\n')
            compile_rust_fixture(source, bundle / 'runtime-tests', test=True,
                                 env=dict(os.environ, CARGO_BIN_EXE_tool=str(original)))
            binaries = {}
            for name in ('runtime', 'runtime-tests', 'viewer'):
                destination = bundle / name
                if name != 'runtime-tests':
                    destination.write_bytes(original.read_bytes())
                    destination.chmod(0o755)
                binaries[name] = dict(path=name, sha256=artifacts.digest(destination),
                    original_executable=str(original), test=name == 'runtime-tests', ordinary=False,
                    profile=dict(opt_level='0', debug_assertions=True, overflow_checks=True),
                    cwd='.', tests=artifacts.inventory(destination) if name == 'runtime-tests' else [],
                    ignored_tests=artifacts.inventory(destination, True) if name == 'runtime-tests' else [],
                    libraries=[])
            config_path = root / 'suites.json'
            config = dict(schema_version=2, suites={'healthy-set': [dict(binary='runtime-tests', name='formal_acceptance', ignored=True)]},
                environment_formal_batches={}, legacy_ignored_filters={'runtime-tests': ['formal_acceptance']},
                shell_tests={}, auxiliary_entries={'runtime-tests': ['crash_child']},
                binary_dependencies={'runtime-tests': ['runtime']},
                case_shards={'runtime-tests': {'formal_acceptance': 'storage-recovery'}})
            authority_path = root / 'trusted-suites.json'
            authority_path.write_text(json.dumps(config))
            candidate = copy.deepcopy(config)
            candidate['suites']['healthy-set'] = []
            candidate['legacy_ignored_filters'] = {}
            config_path.write_text(json.dumps(candidate))
            manifest = dict(schema_version=2, scope='formal', tested_sha='tested', source_sha='source',
                producer_run_id='run', checkout=str(root), config_sha256=artifacts.digest(config_path),
                formal_authority_sha256=artifacts.digest(authority_path),
                rust_toolchain=os.environ.get('RUST_TOOLCHAIN', 'local'), runner_image=os.environ.get('ImageOS', sys.platform),
                binaries=binaries, suites=artifacts.resolve_suites(config, binaries), libraries={})
            (bundle / 'manifest.json').write_text(json.dumps(manifest))
            archive_path, consumer = root / 'storage-recovery.tar.gz', root / 'consumer'
            with patch.object(artifacts, 'ROOT', root), patch.object(artifacts, 'CONFIG', config_path), \
                 patch.object(artifacts, 'FORMAL_AUTHORITY', authority_path), patch.object(artifacts, 'output', return_value='tested'), \
                 patch.dict(os.environ, SOURCE_SHA='source', GITHUB_RUN_ID='run'):
                artifacts.pack(bundle, archive_path, shard='storage-recovery')
                with tarfile.open(archive_path) as archive:
                    archive.extractall(consumer, filter='data')
                self.assertFalse((consumer / 'viewer').exists())
                original.unlink()
                results = root / 'results.jsonl'
                artifacts.execute(consumer, 'ordinary', results, shard='storage-recovery')
                self.assertTrue(original.is_file())
                records = [json.loads(line) for line in results.read_text().splitlines()]
                self.assertEqual(len(records), 1)
                self.assertTrue(records[0]['passed'])
                self.assertEqual(records[0]['suite'], 'healthy-set')
                self.assertEqual(records[0]['shard'], 'storage-recovery')
                self.assertEqual(records[0]['test_name'], 'formal_acceptance')
                previous = results.read_text()
                original_open = Path.open
                def reject_final_append(path, *args, **kwargs):
                    if path == results and args == ('a',):
                        raise OSError('final journal is not writable')
                    return original_open(path, *args, **kwargs)
                with patch.object(Path, 'open', reject_final_append):
                    with self.assertRaisesRegex(OSError, 'final journal is not writable'):
                        artifacts.execute(consumer, 'ordinary', results, shard='storage-recovery')
                self.assertEqual(results.read_text(), previous)
                with self.assertRaisesRegex(ValueError, 'selection mismatch'):
                    artifacts.load(consumer, shard='healthy-set')
                manifest_path = consumer / 'manifest.json'
                packaged = json.loads(manifest_path.read_text())
                changed = copy.deepcopy(packaged)
                changed['included_binaries'].remove('runtime')
                manifest_path.write_text(json.dumps(changed))
                with self.assertRaisesRegex(ValueError, 'closure mismatch'):
                    artifacts.load(consumer, shard='storage-recovery')
                changed = copy.deepcopy(packaged)
                changed['binaries']['runtime-tests']['tests'].append('invented_case')
                manifest_path.write_text(json.dumps(changed))
                with self.assertRaisesRegex(ValueError, 'downloaded test inventory mismatch'):
                    artifacts.load(consumer, shard='storage-recovery')

    def test_proc_scan_ignores_inaccessible_environment_of_unowned_same_uid_process(self):
        class DeniedProcess:
            name = '123456'
            def stat(self):
                return type('Stat', (), {'st_uid': os.getuid()})()
            def __truediv__(self, name):
                if name == 'stat':
                    return type('StatFile', (), {'read_text': lambda self: '123456 (unrelated) S 1 42 42 ' + '0 '*15 + '99'})()
                return type('Denied', (), {'read_bytes': lambda self: (_ for _ in ()).throw(PermissionError('denied'))})()
        with patch.object(Path, 'glob', return_value=[DeniedProcess()]):
            self.assertEqual(owned_processes('owned-test'), {})
            self.assertEqual(owned_processes('owned-test', group=42), {123456: '99'})

    def test_explicit_checkout_also_owns_default_results(self):
        with patch.object(artifacts, 'ROOT', artifacts.ROOT), patch.object(artifacts, 'CONFIG', artifacts.CONFIG), patch.object(artifacts, 'execute') as execute:
            with patch.object(sys, 'argv', ['ci-test-artifacts.py', 'run', '--repo-root', str(self.root), '--bundle', str(self.root / 'bundle')]):
                artifacts.main()
            self.assertEqual(execute.call_args.args[2], self.root.resolve() / 'output/formal-process/results.jsonl')

    def test_corrupt_executable_and_identity_fail_before_execution(self):
        bundle = self.root / 'bundle'
        bundle.mkdir(exist_ok=True)
        entry = dict(path='fixture', sha256='wrong')
        expected = dict(tested_sha='tested', source_sha='source', producer_run_id='run', checkout=str(artifacts.ROOT), config_sha256=artifacts.digest(artifacts.CONFIG), rust_toolchain=os.environ.get('RUST_TOOLCHAIN', 'local'), runner_image=os.environ.get('ImageOS', sys.platform), binaries={'test':entry})
        (bundle / 'fixture').write_bytes(self.binary.read_bytes())
        (bundle / 'fixture').chmod(0o755)
        (bundle / 'manifest.json').write_text(json.dumps(expected))
        with patch.object(artifacts, 'output', return_value='tested'), patch.dict(os.environ, SOURCE_SHA='source', GITHUB_RUN_ID='run'):
            with self.assertRaisesRegex(ValueError, 'corrupt executable'):
                artifacts.load(bundle)
            expected['tested_sha'] = 'another-commit'
            (bundle / 'manifest.json').write_text(json.dumps(expected))
            with self.assertRaisesRegex(ValueError, 'tested_sha'):
                artifacts.load(bundle)

    def test_coalesce_build_invocations_merges_only_both_default_snapshot_libs(self):
        variants = artifacts.build_variants('ordinary')
        invocations = artifacts.coalesce_build_invocations(variants)
        self.assertEqual([name for name, _, _, _ in invocations], ['viewer', 'required', 'snapshot-tests'])
        _, args, features, pkg_map = invocations[2]
        self.assertEqual(args, ['test', '-p', 'oasis7', '-p', 'oasis7_viewer_runtime', '--lib', '--no-run'])
        self.assertEqual(features, [])
        self.assertEqual(pkg_map, {'oasis7': 'snapshot-tests', 'oasis7_viewer_runtime': 'viewer-snapshot-tests'})
        single = [v for v in variants if v[0] != 'viewer-snapshot-tests']
        self.assertEqual(
            artifacts.coalesce_build_invocations(single),
            [(name, a, f, None) for name, a, f in single],
        )

    def test_snapshot_profiles_reject_leaked_test_tier_features(self):
        for logical in ('snapshot-tests', 'viewer-snapshot-tests'):
            artifacts.validate_profile(logical, {'features': ['default']}, 'ordinary')
            for leaked in ('test_tier_required', 'test_tier_full'):
                with self.subTest(logical=logical, leaked=leaked):
                    with self.assertRaisesRegex(ValueError, 'default-only features'):
                        artifacts.validate_profile(logical, {'features': ['default', leaked]}, 'ordinary')

    def test_ordinary_controls_in_rust_baseline_omits_controls_partition_and_runs_clippy_and_wasm_in_baseline(self):
        v1_plan = dict(
            fast_plan_version=1,
            execution_profile='periodic-full',
            scope='explicit-full',
            groups=['baseline', 'rust_baseline', 'oasis7_required'],
            test_policy_changed=False,
            ordinary_artifact_partition_version=1,
            ordinary_artifact_delivery_version=1,
            ordinary_controls_in_rust_baseline_version=1,
            ordinary_clippy_group='rust_baseline',
            ordinary_wasm_group='rust_baseline',
            ordinary_partitions=['runtime', 'component-libs', 'integrations-0', 'integrations-1'],
            oasis7_required_packages=['oasis7', 'oasis7_chain_runtime', 'oasis7_viewer_runtime'],
            oasis7_required_wasm_packages=['oasis7_agent_api', 'oasis7_simulator'],
        )
        self.assertEqual(artifacts.ordinary_partitions(v1_plan),
                         ['runtime', 'component-libs', 'integrations-0', 'integrations-1'])
        self.assertEqual(artifacts.native_ordinary_partitions(v1_plan),
                         ['runtime', 'component-libs', 'integrations-0', 'integrations-1'])
        self.assertEqual(artifacts.required_arguments(v1_plan, 'clippy-group'), ['rust_baseline'])
        self.assertEqual(artifacts.required_arguments(v1_plan, 'wasm-group'), ['rust_baseline'])

        v2_plan = dict(
            fast_plan_version=1,
            execution_profile='pr-fast',
            scope='targeted',
            groups=['baseline', 'rust_baseline', 'oasis7_required'],
            test_policy_changed=False,
            ordinary_artifact_partition_version=2,
            ordinary_artifact_delivery_version=2,
            ordinary_controls_in_rust_baseline_version=1,
            ordinary_clippy_group='rust_baseline',
            ordinary_wasm_group='rust_baseline',
            ordinary_partitions=['native-0', 'native-1', 'native-2', 'native-3'],
            oasis7_required_packages=['oasis7_chain_runtime'],
            oasis7_required_wasm_packages=['oasis7_simulator'],
        )
        self.assertEqual(artifacts.ordinary_partitions(v2_plan),
                         ['native-0', 'native-1', 'native-2', 'native-3'])
        self.assertEqual(artifacts.native_ordinary_partitions(v2_plan),
                         ['native-0', 'native-1', 'native-2', 'native-3'])
        self.assertEqual(artifacts.required_arguments(v2_plan, 'clippy-group'), ['rust_baseline'])
        self.assertEqual(artifacts.required_arguments(v2_plan, 'wasm-group'), ['rust_baseline'])

        # Reject retaining controls or malformed handoff fields fail-closed
        for bad in (
            v2_plan | {'ordinary_partitions': ['controls', 'native-0', 'native-1', 'native-2', 'native-3']},
            v2_plan | {'ordinary_clippy_group': 'oasis7_required'},
            v2_plan | {'ordinary_wasm_group': 'oasis7_required'},
            v2_plan | {'groups': ['baseline', 'oasis7_required']},
            v2_plan | {'ordinary_controls_in_rust_baseline_version': 99},
            v2_plan | {'ordinary_controls_in_rust_baseline_version': True},
            {k: v for k, v in v2_plan.items() if k != 'ordinary_controls_in_rust_baseline_version'},
        ):
            with self.subTest(bad=bad), self.assertRaises(ValueError):
                artifacts.fast_plan(bad)

        # Verify shell driver executes Clippy + WASM in rust_baseline and skips WASM in oasis7_required
        driver = (HERE / 'ci-tests.sh').read_text()
        bodies = dict(re.findall(r'^(\w+)\(\) \{\n(.*?)^\}', driver, re.M | re.S))
        sha = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=artifacts.ROOT, text=True).strip()
        plan_file = self.root / 'baseline-controls-plan.json'
        plan_file.write_text(json.dumps(v2_plan | {'source_sha': sha, 'tested_sha': sha}))
        bundle_dir = self.root / 'dummy-bundle'
        bundle_dir.mkdir(exist_ok=True)
        script = (
            'set -euo pipefail\n'
            'tier=required; ci_python="$1"; driver_dir="$2"; repo_root="$3"; required_plan="$4"\n'
            'ordinary_partition="native-0"; export OASIS7_CI_TEST_BUNDLE="$5"\n'
            'run_oasis7_required_tier_clippy() { echo "CLIPPY_EXECUTED"; }\n'
            'run_cargo() { printf "CARGO:%s\\n" "$*"; }\n'
            'run() { printf "RUN:%s\\n" "$*"; }\n'
            'run_oasis7_required_clippy_for_group() {\n' + bodies['run_oasis7_required_clippy_for_group'] + '}\n'
            'run_oasis7_required_tier_tests() {\n' + bodies['run_oasis7_required_tier_tests'] + '}\n'
            'echo "---BASELINE---"\n'
            'run_oasis7_required_clippy_for_group rust_baseline\n'
            'echo "---ORDINARY---"\n'
            'run_oasis7_required_clippy_for_group oasis7_required\n'
            'run_oasis7_required_tier_tests\n'
        )
        res = subprocess.run(
            ['bash', '-c', script, 'fixture', sys.executable, str(HERE), str(artifacts.ROOT),
             str(plan_file), str(bundle_dir)],
            text=True, capture_output=True, env=dict(os.environ, SOURCE_SHA=sha),
        )
        self.assertEqual(res.returncode, 0, res.stderr)
        baseline_out, ordinary_out = res.stdout.split('---ORDINARY---\n', 1)
        self.assertIn('CLIPPY_EXECUTED', baseline_out)
        self.assertIn('CARGO:check -p oasis7_simulator --lib --target wasm32-unknown-unknown --locked', baseline_out)
        self.assertNotIn('CLIPPY_EXECUTED', ordinary_out)
        self.assertNotIn('CARGO:check', ordinary_out)

    def test_parallel_ordinary_bundle_consumer_uses_bounded_pool_and_honors_local_check_serial(self):
        selected = CompleteArtifactPartitions().selection()
        selected.pop('ordinary_artifact_partition_version', None)
        selected.pop('ordinary_partitions', None)
        manifest = dict(
            execution_plan=selected,
            binaries={
                'short-test': dict(path='bin/short', cwd='crates/oasis7', package='oasis7', target='short',
                                   target_kind=['test'], variant='required', ordinary=True,
                                   tests=['a', 'b', 'c'], ignored_tests=[]),
                'heavy-test': dict(path='bin/heavy', cwd='crates/oasis7_runtime', package='oasis7_runtime',
                                   target='incremental_full_scales', target_kind=['test'],
                                   variant='required-risk-0', ordinary=True, tests=['x'], ignored_tests=[]),
            },
        )
        with tempfile.TemporaryDirectory() as temporary:
            bundle = Path(temporary)
            results = bundle / 'results.jsonl'
            with patch.object(artifacts, 'load', return_value=(manifest, {})), \
                 patch.object(artifacts, 'snapshot_regressions', return_value=()), \
                 patch.object(artifacts, 'execute_inline_jobs') as pool, \
                 patch.dict(os.environ, {}, clear=False):
                os.environ.pop('OASIS7_LOCAL_CHECK', None)
                artifacts.execute(bundle, 'ordinary', results, plan=selected, parallel_ordinary=True)
                pool.assert_called_once()
                self.assertTrue(pool.call_args.kwargs['artifact_partition'])
                # Heavy calibrated cost (incremental_full_scales) starts before higher-case-count short target
                self.assertEqual([job['name'] for job in pool.call_args.args[0]], ['heavy-test', 'short-test'])
            with patch.object(artifacts, 'load', return_value=(manifest, {})), \
                 patch.object(artifacts, 'snapshot_regressions', return_value=()), \
                 patch.object(artifacts, 'execute_inline_jobs') as pool, \
                 patch('ci_process_runner.run', return_value=dict(passed=True, cleanup_result='clean')) as serial, \
                 patch.dict(os.environ, OASIS7_LOCAL_CHECK='1'):
                artifacts.execute(bundle, 'ordinary', results, plan=selected, parallel_ordinary=True)
                pool.assert_not_called()
                self.assertEqual(serial.call_count, 2)
        for bad_args in (['build', '--bundle', 'b', '--parallel-ordinary'],
                         ['run', '--bundle', 'b', '--shard', 'storage-recovery', '--parallel-ordinary']):
            proc = subprocess.run([sys.executable, str(HERE / 'ci-test-artifacts.py'), *bad_args],
                                  text=True, capture_output=True)
            self.assertNotEqual(proc.returncode, 0)


if __name__ == '__main__':
    unittest.main()
