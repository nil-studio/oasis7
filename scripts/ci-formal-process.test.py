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
        non_viewer = self.selection(('oasis7_chain_runtime',))
        self.assertEqual(artifacts.snapshot_regressions(dict(execution_plan=non_viewer)), ())

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
        selected = self.selection(['standalone_tool_lockfiles'])
        self.assertEqual(artifacts.required_arguments(selected, 'baseline-checks'), ['standalone_tool_lockfiles'])
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


def complete_partition_selection():
    return FastExecutionSelection().selection() | dict(
        scope='full', ordinary_artifact_partition_version=1,
        ordinary_partitions=['controls', 'runtime', 'component-libs', 'integrations-0', 'integrations-1'])


def complete_partition_policy_selection():
    return complete_partition_selection() | dict(scope='targeted', test_policy_changed=True,
        oasis7_required_packages=['oasis7_runtime_host', 'oasis7_chain_runtime', 'oasis7_viewer_runtime'],
        oasis7_required_wasm_packages=['oasis7_viewer_runtime'])


def complete_partition_manifest(selected):
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


class OrdinaryExclusiveExecution(unittest.TestCase):
    def manifest(self, selected=None):
        selected = selected or complete_partition_selection()
        manifest = complete_partition_manifest(selected)
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
        selected = FastExecutionSelection().selection(packages=('oasis7_chain_runtime',)) | dict(
            ordinary_artifact_partition_version=2,
            ordinary_partitions=['controls', 'native-0', 'native-1', 'native-2', 'native-3'],
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
        for selected in (complete_partition_selection(), complete_partition_policy_selection()):
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
            for checks, failed in (([], ''),
                                   (list(artifacts.RUST_BASELINE_CHECKS), ''),
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
                    self.assertEqual('standalone_tool_lockfiles' in result.stdout.splitlines(),
                                     'standalone_tool_lockfiles' in expected)

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
        # The old umbrella emitted no Formal cases at retirement. The remaining
        # four compile contexts still execute once while Chain owns the harness.
        required = ['test_tier_required']
        formal = ['test_tier_required', 'ci_formal_required_features']
        original = [
            ('formal-required', ['test', '-p', 'oasis7_chain_runtime',
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


if __name__ == '__main__':
    unittest.main()
