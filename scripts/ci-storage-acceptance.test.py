#!/usr/bin/env python3
"""Fail-closed contracts for manually dispatched storage acceptance."""
import copy
import importlib.util
import json
from pathlib import Path
import tempfile
import tarfile
import argparse
import os
import re
import sys
import subprocess
from unittest.mock import patch
import unittest

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location('storage_acceptance', HERE / 'ci-storage-acceptance.py')
acceptance = importlib.util.module_from_spec(spec)
spec.loader.exec_module(acceptance)


fixture_spec = importlib.util.spec_from_file_location('storage_fixture', HERE / 'ci-storage-fixture.py')
fixture = importlib.util.module_from_spec(fixture_spec)
fixture_spec.loader.exec_module(fixture)


class FixtureContracts(unittest.TestCase):
    def test_workflow_attempt_branch_passes_original_runner_bind_identity(self):
        workflow = (HERE.parent / '.github/workflows/storage-acceptance.yml').read_text()
        branch = workflow.split('          if [[ "$GROUP" == attempt-release ]]; then\n', 1)[1]
        branch = branch.split('          fi\n', 1)[0]
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            captured = root / 'docker-arguments'
            env = dict(os.environ, GROUP='attempt-release', GITHUB_WORKSPACE=str(root),
                STORAGE_ARTIFACT_CHECKOUT='/__w/oasis7/oasis7', RUNNER_TEMP=str(root / 'runner'),
                STORAGE_VERIFY_IMAGE='example@sha256:fixed', LABEL='attempt-release',
                SCALE='10', repeats='1', INVENTORY_ENTRY_LIMIT='5000000', CAPTURE=str(captured))
            script = 'docker() { printf "%s\\0" "$@" > "$CAPTURE"; }\n' + branch
            if sys.platform == 'darwin':
                script = 'stat() { command stat -f "$2" "$3"; }\n' + script
            subprocess.run(['bash', '-eu', '-c', script], env=env, check=True)
            arguments = captured.read_bytes().decode().rstrip('\0').split('\0')
            fixture_root = root / 'output/attempt-release-image'
            identity = fixture_root.stat()
            self.assertIn('type=bind,source=' + str(fixture_root)
                          + ',target=/__w/oasis7/oasis7/output/attempt-release-image', arguments)
            self.assertIn('OASIS7_STORAGE_FIXTURE_DEVICE=' + str(identity.st_dev), arguments)
            self.assertIn('OASIS7_STORAGE_FIXTURE_INODE=' + str(identity.st_ino), arguments)
            self.assertIn('TMPDIR=/__w/oasis7/oasis7/output/attempt-release-image/tmp', arguments)
            self.assertIn('output/attempt-release-image/results', arguments)
            self.assertIn('--init', arguments)
            self.assertIn('--user', arguments)
            self.assertFalse(any(arg.startswith(('--privileged', '--cap-add', '--pid')) for arg in arguments))
            self.assertEqual(identity.st_mode & 0o777, 0o700)

    def test_wrong_runner_identity_rejected_before_filesystem_operations(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            results = root / 'results'
            results.mkdir()
            info = root.stat()
            for device, inode in ((info.st_dev + 1, info.st_ino), (info.st_dev, info.st_ino + 1)):
                with self.assertRaisesRegex(ValueError, 'original identity'):
                    fixture.filesystem_probe(root, results, device, inode)
            root.chmod(0o755)
            with self.assertRaisesRegex(ValueError, 'private runner-owned'):
                fixture.filesystem_probe(root, results, info.st_dev, info.st_ino)

    def test_results_outside_fixture_and_temporary_symlink_rejected(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            info = root.stat()
            with self.assertRaisesRegex(ValueError, 'share one filesystem'):
                fixture.filesystem_probe(root, root.parent, info.st_dev, info.st_ino)
            results = root / 'results'
            results.mkdir()
            (root / 'tmp').symlink_to(results, target_is_directory=True)
            with self.assertRaisesRegex(ValueError, 'temporary directory'):
                fixture.filesystem_probe(root, results, info.st_dev, info.st_ino)

    def test_overlay_or_tmpfs_rejected_before_case(self):
        for filesystem in ('overlayfs', 'tmpfs'):
            with tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary).resolve()
                results = root / 'results'
                results.mkdir()
                info = root.stat()
                with patch.object(fixture.subprocess, 'check_output', return_value=filesystem), \
                        self.assertRaisesRegex(ValueError, 'not overlay or tmpfs'):
                    fixture.filesystem_probe(root, results, info.st_dev, info.st_ino)

    @unittest.skipUnless(sys.platform == 'linux', 'actual Linux filesystem/proc prerequisite')
    def test_actual_runner_filesystem_and_child_process(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            results = root / 'results'
            results.mkdir()
            info = root.stat()
            filesystem = subprocess.check_output(['stat', '-f', '-c', '%T', str(root)], text=True).strip()
            if filesystem in ('overlayfs', 'tmpfs'):
                with self.assertRaisesRegex(ValueError, 'not overlay or tmpfs'):
                    fixture.filesystem_probe(root, results, info.st_dev, info.st_ino)
            else:
                report = fixture.filesystem_probe(root, results, info.st_dev, info.st_ino)
                self.assertTrue(report['hardlink'])
                self.assertTrue(report['flock'])
                self.assertTrue(report['file_fsync'])
                self.assertTrue(report['directory_fsync'])
            process = fixture.process_probe()
            self.assertTrue(process['sigkill_reaped'])
            self.assertNotEqual(process['parent_pid'], process['child_pid'])

    def test_native_failure_saved_and_rejected(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            results = root / 'results'
            results.mkdir()
            manifest = {'binaries': {'runtime-tests': dict(path='bin/runtime', cwd='.',
                tests=['test::native_environment_original_loaded_bytes_verify'], ignored_tests=[])}}
            env = {'TMPDIR': str(root / 'tmp')}
            with patch.dict(os.environ, OASIS7_STORAGE_FIXTURE_DEVICE='1', OASIS7_STORAGE_FIXTURE_INODE='2'), \
                    patch.object(fixture, 'filesystem_probe', return_value={}), \
                    patch.object(fixture, 'process_probe', return_value={}), \
                    patch.object(fixture.tempfile, 'gettempdir', return_value=env['TMPDIR']), \
                    patch.object(fixture, 'run', return_value=dict(passed=False)) as runner:
                with self.assertRaisesRegex(ValueError, 'native environment capture'):
                    fixture.preflight(root, results, root, manifest, env)
            report = json.loads((results / 'fixture-preflight.json').read_text())
            self.assertFalse(report['passed'])
            self.assertIn('--exact', runner.call_args.args[0])
            self.assertEqual(runner.call_args.kwargs['expected_count'], 1)



class Contracts(unittest.TestCase):
    def test_opt_in_host_reference_requires_its_exact_result_and_job(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            image = self.report('attempt-release')
            image['group'] = 'attempt-release'
            (root / 'summary.json').write_text(json.dumps(image))
            needs = {name: {'result': status} for name, status in {
                'build': 'success', 'verify': 'success', 'capacity-long': 'skipped',
                'full-node-long': 'skipped', 'attempt-release-reference': 'skipped'}.items()}
            self.assertTrue(acceptance.summarize(root, ['attempt-release'], needs)['passed'])
            needs['attempt-release-reference']['result'] = 'success'
            with self.assertRaisesRegex(ValueError, 'host reference job'):
                acceptance.summarize(root, ['attempt-release'], needs)
            expected = ['attempt-release', 'attempt-release-host']
            with self.assertRaisesRegex(ValueError, 'missing, duplicate or unexpected'):
                acceptance.summarize(root, expected, needs)
            reference = copy.deepcopy(image)
            reference['label'] = 'attempt-release-host'
            nested = root / 'reference'
            nested.mkdir()
            (nested / 'summary.json').write_text(json.dumps(reference))
            self.assertTrue(acceptance.summarize(root, expected, needs)['passed'])
            needs['attempt-release-reference']['result'] = 'skipped'
            with self.assertRaisesRegex(ValueError, 'host reference job'):
                acceptance.summarize(root, expected, needs)
            del needs['attempt-release-reference']
            with self.assertRaisesRegex(ValueError, 'missing or unexpected acceptance job'):
                acceptance.summarize(root, expected, needs)

    def test_workflow_host_reference_is_opt_in_and_fixture_bytes_are_excluded(self):
        workflow = (HERE.parent / '.github/workflows/storage-acceptance.yml').read_text()
        self.assertIn('attempt_release_host_reference:\n', workflow)
        self.assertIn('        default: false\n', workflow)
        reference = workflow.split('  attempt-release-reference:\n', 1)[1].split('  capacity-long:', 1)[0]
        self.assertIn('if: inputs.attempt_release_host_reference', reference)
        self.assertIn('needs: build', reference)
        self.assertIn('steps: *acceptance-consumer', reference)
        self.assertIn("matrix.label == 'attempt-release-host' || !contains", workflow)
        self.assertIn("USE_PREBUILT: ${{ matrix.label != 'attempt-release-host' && contains", workflow)
        self.assertIn('            !output/attempt-release-image/tmp/**\n', workflow)
        self.assertIn('HOST_REFERENCE: ${{ inputs.attempt_release_host_reference }}', workflow)
        self.assertIn('if [[ "$HOST_REFERENCE" == true ]]; then args+=(--expected attempt-release-host); fi', workflow)

    def test_image_fixture_rejection_blocks_acceptance_and_summary(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            bundle = root / 'bundle'
            bundle.mkdir()
            manifest = dict(scope='storage', selection=None, tested_sha='abc', source_sha='abc',
                producer_run_id='1', producer_attempt='1', runner_image='test', rustc_version='test', binaries={})
            args = argparse.Namespace(bundle=bundle, results_dir=root / 'results', group='attempt-release',
                capacity_scale=10, repeat_faults=1, label='attempt-release', timeout_seconds=5)
            with patch.dict(os.environ, OASIS7_STORAGE_IMAGE_FIXTURE=str(root)), \
                    patch.object(acceptance.artifacts, 'load', return_value=(manifest, dict(os.environ))), \
                    patch.object(acceptance, 'select', return_value=[dict(binary='test', name='test')]), \
                    patch.object(acceptance, 'host_observation', return_value={}), \
                    patch.object(acceptance, 'image_fixture_preflight', side_effect=ValueError('real fixture rejected')), \
                    patch.object(acceptance, 'run') as runner:
                with self.assertRaisesRegex(ValueError, 'real fixture rejected'):
                    acceptance.execute(args)
            runner.assert_not_called()
            report = json.loads((args.results_dir / 'summary.json').read_text())
            self.assertFalse(report['passed'])
            self.assertFalse(report['image_fixture']['passed'])
            with self.assertRaisesRegex(ValueError, 'incomplete or failed'):
                acceptance.summarize(args.results_dir, ['attempt-release'])

    def manifest(self):
        binaries = {name: dict(tests=['one'], ignored_tests=[]) for name in acceptance.LIBRARIES}
        binaries['runtime-tests'] = dict(tests=['runtime::storage_test', 'runtime::other',
            'bench::actual_formal_storage_capacity_smoke_10_heights',
            'bench::actual_formal_storage_capacity_1000_or_10000_heights'],
            ignored_tests=['bench::actual_formal_storage_capacity_1000_or_10000_heights'])
        return dict(binaries=binaries, suites={'runtime-protocol': [
            dict(binary='runtime-tests', name='test::' + name, ignored=True)
            for name in acceptance.NETWORK_SUFFIXES]})

    def report(self, label='units'):
        return dict(label=label, group='units', tested_sha='abc', source_sha='abc',
                    producer_run_id='1', producer_attempt='1', passed=True,
                    expected_cases=1, selected_inventory=[{'binary': 'test'}], results=[dict(
                        passed=True, exit_code=0, cleanup_result='clean', timeout=False, cancelled=False, count_ok=True, leaked_processes=False)])

    def test_inventory_entry_limit_validation_and_dispatch_wiring(self):
        for value in ('0', '-1', '10000001', '1.5', '1e6', '', '１２'):
            with self.assertRaises(argparse.ArgumentTypeError):
                acceptance.inventory_entry_limit(value)
        for value in ('1', '5000000', '10000000'):
            self.assertEqual(acceptance.inventory_entry_limit(value), int(value))
        rejected = subprocess.run([sys.executable, str(HERE / 'ci-storage-acceptance.py'),
                                   'run', '--inventory-entry-limit', '10000001'],
                                  capture_output=True, text=True)
        self.assertEqual(rejected.returncode, 2)
        self.assertIn('inventory entry limit', rejected.stderr)
        workflow = (HERE.parent / '.github/workflows/storage-acceptance.yml').read_text()
        self.assertIn("INVENTORY_ENTRY_LIMIT: ${{ inputs.inventory_entry_limit }}", workflow)
        self.assertIn('--inventory-entry-limit "$INVENTORY_ENTRY_LIMIT"', workflow)

    def test_full_node_budget_reaches_real_child_and_summary(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            bundle = root / 'bundle'
            bundle.mkdir()
            binary = bundle / 'test'
            binary.write_text('#!' + sys.executable + '\nimport os\n'
                              'print("OBSERVED_LIMIT=" + os.environ["OASIS7_STORAGE_INVENTORY_ENTRY_LIMIT"])\n'
                              "print('STORAGE_FULL_NODE_REPORT={\"status\":\"partial\"}')\n"
                              'print("test result: ok. 1 passed; 0 failed; 0 ignored;")\n')
            binary.chmod(0o755)
            manifest = dict(scope='storage', selection=None, tested_sha='abc', source_sha='abc',
                producer_run_id='1', producer_attempt='1', runner_image='test', rustc_version='test',
                binaries={'test': dict(path='test', cwd='.')})
            args = argparse.Namespace(inventory_entry_limit=10_000_000, bundle=bundle,
                results_dir=root / 'results', group='full-node', capacity_scale=10,
                repeat_faults=1, label='full-node', timeout_seconds=5)
            with patch.object(acceptance.artifacts, 'load', return_value=(manifest, dict(os.environ))), \
                 patch.object(acceptance, 'select', return_value=[dict(binary='test', batch=True, count=1)]), \
                 patch.object(acceptance.shutil, 'disk_usage', return_value=argparse.Namespace(total=20*1024**3, used=1*1024**3, free=10*1024**3)), \
                 patch.object(acceptance, 'full_node_inventory_budget_passed', wraps=acceptance.full_node_inventory_budget_passed) as budget_check:
                with self.assertRaisesRegex(ValueError, 'partial'):
                    acceptance.execute(args)
            report = json.loads((args.results_dir / 'summary.json').read_text())
            budget_check.assert_called_once_with(dict(status='partial'), 10_000_000)
            self.assertEqual(report['inventory_entry_limit'], 10_000_000)
            self.assertFalse(report['passed'])
            self.assertIn('OBSERVED_LIMIT=10000000', (args.results_dir / report['results'][0]['log']).read_text())

    def test_only_capacity_long_overrides_logical_budget(self):
        workflow = (HERE.parent / '.github/workflows/storage-acceptance.yml').read_text()
        global_env = workflow.split('\nenv:\n', 1)[1].split('\njobs:\n', 1)[0]
        variable = 'OASIS7_STORAGE_MAX_LOGICAL_BYTES'
        self.assertIn(f"  {variable}: '4294967296'", global_env)
        jobs = dict(re.findall(
            r'^  ([a-z][a-z0-9-]*):\n(.*?)(?=^  [a-z][a-z0-9-]*:\n|\Z)',
            workflow.split('\njobs:\n', 1)[1], re.MULTILINE | re.DOTALL))
        self.assertIn(f"    env:\n      {variable}: '17179869184'", jobs['capacity-long'])
        self.assertIn("if: inputs.capacity_scale != '10'", jobs['capacity-long'])
        for name, body in jobs.items():
            if name != 'capacity-long':
                self.assertNotIn(variable, body, name)
        self.assertIn('timeout-minutes: 360', jobs['capacity-long'])
        self.assertIn('steps: *acceptance-consumer', jobs['capacity-long'])

    def test_execution_timing_is_enabled_only_for_full_node_consumers(self):
        workflow = (HERE.parent / '.github/workflows/storage-acceptance.yml').read_text()
        variable = 'OASIS7_FORMAL_EXECUTION_TIMING'
        self.assertEqual(workflow.count(variable), 1)
        step = workflow.split('- name: Run acceptance with exact inventory and process cleanup\n', 1)[1].split('      - name:', 1)[0]
        self.assertIn(variable + ": ${{ matrix.group == 'full-node' && '1' || '' }}", step)
        self.assertIn('python3 scripts/ci-storage-acceptance.py run', step)

    def test_build_components_reads_warmed_shared_image_cache_namespace(self):
        workflow = (HERE.parent / '.github/workflows/storage-acceptance.yml').read_text()
        digest = re.search(
            r'STORAGE_BUILD_IMAGE:\s*&storage-build-image\s+ghcr\.io/nil-studio/oasis7/ci-rust-build@sha256:([0-9a-f]{64})',
            workflow,
        )
        self.assertIsNotNone(digest)
        namespace = 'ci-rust-build-' + digest.group(1)
        build_job = re.search(
            r'^  build-components:\n(.*?)(?=^  [a-z][a-z0-9-]*:\n|\Z)',
            workflow.split('\njobs:\n', 1)[1],
            re.MULTILINE | re.DOTALL,
        )
        self.assertIsNotNone(build_job)
        body = build_job.group(1)
        self.assertIn('python3 scripts/ci-rust-cache-toolchains.py "$RUST_TOOLCHAIN" "$OASIS7_WASM_TOOLCHAIN"', body)
        self.assertIn('save-if: false', body)
        self.assertIn(f'shared-key: {namespace}\n', body)
        self.assertIn('add-rust-environment-hash-key: true', body)
        self.assertIn('env-vars: CARGO CC CFLAGS CXX CMAKE RUST OASIS7_WASM', body)
        self.assertIn('cache-on-failure: false', body)
        rust_cache = (HERE.parent / '.github/workflows/rust-cache.yml').read_text()
        if '\n  plan-image-component-cache:\n' in rust_cache:
            planner = rust_cache.split('\n  plan-image-component-cache:\n', 1)[1].split(
                '\n  rust-image-component-cache:\n', 1)[0]
            rows = json.loads(planner.split("<<'JSON'\n", 1)[1].split('          JSON\n', 1)[0])
            native = [row for row in rows if row['profile'] == 'native']
            self.assertEqual(len(native), 1)
            self.assertEqual(native[0]['namespace'], namespace)
            self.assertEqual(native[0]['image'], 'ghcr.io/nil-studio/oasis7/ci-rust-build@sha256:' + digest.group(1))
        else:
            self.assertIn(f'namespace: {namespace}\n', rust_cache)

    def test_storage_components_preserve_every_exact_build_variant_and_profile(self):
        native = acceptance.artifacts
        components = native.STORAGE_COMPONENTS
        scopes = [variant for component in components for variant in native.build_variants(component)]
        storage = native.build_variants('storage')
        self.assertEqual(scopes[1:], storage[1:])
        feature = 'oasis7_chain_runtime/ci_storage_acceptance'
        variant, args, features = scopes[0]
        self.assertEqual(features, [*storage[0][2], feature])
        args[args.index('--features') + 1] = ','.join(storage[0][2])
        self.assertEqual((variant, args), storage[0][:2])
        self.assertNotIn(feature, native.build_variants('formal')[0][2])
        for component in components:
            self.assertEqual(native.build_environment(component), native.build_environment('storage'))
            with self.assertRaisesRegex(ValueError, 'partial Storage component'):
                native.complete_bundle_scope({'scope': component})

    def test_storage_merge_requires_exact_archive_members(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            payload = root / 'payload'
            payload.write_text('executable')
            manifest = root / 'manifest.json'
            manifest.write_text(json.dumps({'binaries': {'fixture': {'path': 'bin/fixture'}}, 'libraries': {}}))
            inputs = root / 'parts'
            for name in acceptance.artifacts.STORAGE_COMPONENTS:
                directory = inputs / ('component-' + name)
                directory.mkdir(parents=True)
                with tarfile.open(directory / (name + '.tar.gz'), 'w:gz') as archive:
                    archive.add(manifest, arcname='manifest.json')
                    archive.add(payload, arcname='bin/fixture')
            output = root / 'bundle'
            with patch.object(acceptance.artifacts, 'merge_storage') as merge:
                acceptance.merge(inputs, output)
                self.assertEqual(set(merge.call_args.args[0]), set(acceptance.artifacts.STORAGE_COMPONENTS))
                self.assertEqual(merge.call_args.args[1], output)
            name = next(iter(acceptance.artifacts.STORAGE_COMPONENTS))
            with tarfile.open(inputs / ('component-' + name) / (name + '.tar.gz'), 'w:gz') as archive:
                archive.add(manifest, arcname='manifest.json')
                archive.add(payload, arcname='bin/fixture')
                archive.add(payload, arcname='bin/extra')
            with self.assertRaisesRegex(ValueError, 'archive does not match its manifest'):
                acceptance.merge(inputs, output)
            self.assertFalse(output.exists())

    def test_storage_build_optimizes_native_tests_without_disabling_checks(self):
        with patch.dict(os.environ, {'RUSTC_WRAPPER': 'untrusted-wrapper',
                                   'CARGO_PROFILE_TEST_DEBUG_ASSERTIONS': 'false'}):
            ordinary = acceptance.artifacts.build_environment('ordinary')
            formal = acceptance.artifacts.build_environment('formal')
            storage = acceptance.artifacts.build_environment('storage')
        self.assertNotIn('RUSTC_WRAPPER', storage)
        self.assertEqual(ordinary['CARGO_PROFILE_TEST_DEBUG_ASSERTIONS'], 'false')
        for environment, level in ((storage, '2'), (formal, '0')):
            for profile in ('DEV', 'TEST'):
                self.assertEqual(environment[f'CARGO_PROFILE_{profile}_OPT_LEVEL'], level)
                self.assertEqual(environment[f'CARGO_PROFILE_{profile}_DEBUG_ASSERTIONS'], 'true')
                self.assertEqual(environment[f'CARGO_PROFILE_{profile}_OVERFLOW_CHECKS'], 'true')
        self.assertEqual(storage.get('CARGO_PROFILE_RELEASE_OPT_LEVEL'),
                         ordinary.get('CARGO_PROFILE_RELEASE_OPT_LEVEL'))

    def test_storage_artifact_profile_is_verified_at_consumption(self):
        for scope, level in (('storage', '2'), ('formal', '0'), ('formal-debug', '0')):
            binary = dict(profile=dict(opt_level=level, debug_assertions=True, overflow_checks=True))
            acceptance.artifacts.validate_profile('runtime-tests', binary, scope)
            for key, value in [('opt_level', '0' if level == '2' else '2'),
                               ('debug_assertions', False), ('overflow_checks', False)]:
                changed = copy.deepcopy(binary)
                changed['profile'][key] = value
                with self.assertRaises(ValueError):
                    acceptance.artifacts.validate_profile('runtime-tests', changed, scope)
        # Ordinary consumers keep their independent profile contract.
        acceptance.artifacts.validate_profile('runtime-tests', dict(profile={}), 'ordinary')

    def test_storage_consumer_rejects_unoptimized_manifest(self):
        artifacts = acceptance.artifacts
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            binary = root / 'runtime'
            binary.write_text('#!/bin/sh\nexit 0\n')
            binary.chmod(0o755)
            config = root / 'suites.json'
            config.write_text(json.dumps(dict(suites={}, legacy_ignored_filters={},
                                             environment_formal_batches={}, shell_tests={})))
            entry = dict(path='runtime', sha256=artifacts.digest(binary),
                         original_executable=str(binary), test=False,
                         profile=dict(opt_level='2', debug_assertions=True, overflow_checks=True))
            manifest = dict(schema_version=2, scope='storage', tested_sha='tested', source_sha='source',
                            producer_run_id='run', checkout=str(root),
                            config_sha256=artifacts.digest(config),
                            rust_toolchain=os.environ.get('RUST_TOOLCHAIN', 'local'),
                            runner_image=os.environ.get('ImageOS', sys.platform),
                            binaries={'runtime': entry}, suites={}, libraries={})
            with patch.object(artifacts, 'ROOT', root), patch.object(artifacts, 'CONFIG', config), \
                 patch.object(artifacts, 'output', return_value='tested'), \
                 patch.dict(os.environ, SOURCE_SHA='source', GITHUB_RUN_ID='run'):
                (root / 'manifest.json').write_text(json.dumps(manifest))
                artifacts.load(root)
                entry['profile']['opt_level'] = '0'
                (root / 'manifest.json').write_text(json.dumps(manifest))
                with self.assertRaisesRegex(ValueError, 'optimized storage artifact'):
                    artifacts.load(root)
                for schema in (None, 1, 99):
                    if schema is None:
                        manifest.pop('schema_version', None)
                    else:
                        manifest['schema_version'] = schema
                    (root / 'manifest.json').write_text(json.dumps(manifest))
                    with self.assertRaisesRegex(ValueError, 'schema 2 storage artifact'):
                        artifacts.load(root)

    def test_storage_scope_adds_libraries_without_changing_formal(self):
        formal = acceptance.artifacts.build_variants('formal')
        storage = acceptance.artifacts.build_variants('storage')
        self.assertEqual(storage[:-1], formal)
        self.assertEqual(storage[-1][0], 'storage-libs')
        for package in ('oasis7_cas', 'oasis7_formal_protocol', 'oasis7_runtime', 'oasis7_world_service', 'oasis7_world_state', 'oasis7_world_projection'):
            self.assertIn(package, storage[-1][1])

    def test_units_are_nonempty_actual_tests_and_exclude_ignored(self):
        manifest = self.manifest()
        entries = acceptance.select(manifest, 'units')
        self.assertEqual(len(entries), len(acceptance.LIBRARIES) + 1)
        self.assertFalse(any('1000_or_10000' in entry.get('name', '') for entry in entries))
        manifest['binaries'][acceptance.LIBRARIES[0]]['tests'] = []
        with self.assertRaises(ValueError):
            acceptance.select(manifest, 'units')

    def test_capacity_inventory_exact_and_ignored_mode(self):
        manifest = self.manifest()
        self.assertFalse(acceptance.select(manifest, 'capacity', 10)[0]['ignored'])
        self.assertTrue(acceptance.select(manifest, 'capacity', 10000)[0]['ignored'])
        manifest['binaries']['runtime-tests']['tests'] = []
        with self.assertRaises(ValueError):
            acceptance.select(manifest, 'capacity')

    def test_network_missing_or_duplicate_is_not_pass(self):
        manifest = self.manifest()
        self.assertEqual(len(acceptance.select(manifest, 'network')), 3)
        manifest['suites']['runtime-protocol'].pop()
        with self.assertRaises(ValueError):
            acceptance.select(manifest, 'network')

        manifest = self.manifest()
        manifest['suites']['runtime-protocol'].append(manifest['suites']['runtime-protocol'][0])
        with self.assertRaises(ValueError):
            acceptance.select(manifest, 'network')

    def test_disk_requires_the_actual_ignored_enospc_case(self):
        manifest = self.manifest()
        with self.assertRaises(ValueError):
            acceptance.select(manifest, 'disk')
        name = 'physical::physical_real_filesystem_enospc_preserves_published_bytes_and_retry'
        binary = manifest['binaries']['distfs-tests']
        binary['tests'].append(name)
        binary['ignored_tests'].append(name)
        self.assertEqual(acceptance.select(manifest, 'disk'),
                         [dict(binary='distfs-tests', name=name, ignored=True)])
        binary['ignored_tests'].append(name)
        with self.assertRaises(ValueError):
            acceptance.select(manifest, 'disk')

    def test_summary_rejects_missing_duplicate_mixed_identity_and_failed_jobs(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            path = root / 'summary.json'
            path.write_text(json.dumps(self.report()))
            self.assertTrue(acceptance.summarize(root, ['units'])['passed'])
            for expected in ([], ['units', 'capacity'], ['units', 'units']):
                with self.assertRaises(ValueError):
                    acceptance.summarize(root, expected)
            for state in ('failure', 'cancelled', 'skipped', None):
                with self.assertRaises(ValueError):
                    acceptance.summarize(root, ['units'], {'consumer': {'result': state}})
            nested = root / 'nested'
            nested.mkdir()
            second = self.report('network')
            second['producer_run_id'] = '2'
            (nested / 'summary.json').write_text(json.dumps(second))
            with self.assertRaises(ValueError):
                acceptance.summarize(root, ['units', 'network'])

    def test_real_harness_execution_preserves_count_log_and_summary(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            bundle = root / 'bundle'
            bundle.mkdir()
            binary = bundle / 'test'
            binary.write_text('#!' + sys.executable + '\nprint("test result: ok. 1 passed; 0 failed; 0 ignored;")\n')
            binary.chmod(0o755)
            manifest = dict(scope='storage', selection=None, tested_sha='abc', source_sha='abc',
                producer_run_id='1', producer_attempt='1', runner_image='test', rustc_version='test',
                binaries={'test': dict(path='test', cwd='.')})
            args = argparse.Namespace(inventory_entry_limit=5_000_000, bundle=bundle, results_dir=root / 'results', group='units',
                capacity_scale=10, repeat_faults=1, label='units', timeout_seconds=5)
            with patch.object(acceptance.artifacts, 'load', return_value=(manifest, dict(os.environ))), \
                 patch.object(acceptance, 'select', return_value=[dict(binary='test', batch=True, count=1)]), \
                 patch.object(acceptance.shutil, 'disk_usage', return_value=argparse.Namespace(total=20*1024**3, used=1*1024**3, free=10 * 1024**3)):
                acceptance.execute(args)
            report = json.loads((args.results_dir / 'summary.json').read_text())
            self.assertTrue(report['passed'])
            self.assertEqual(report['results'][0]['expected_count'], 1)
            self.assertIn('test result: ok.', (args.results_dir / report['results'][0]['log']).read_text())
            self.assertTrue(acceptance.summarize(args.results_dir, ['units'])['passed'])

    def test_real_capacity_harness_preserves_structured_report(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            bundle = root / 'bundle'
            bundle.mkdir()
            binary = bundle / 'test'
            binary.write_text('#!' + sys.executable + '\nprint(\'STORAGE_CAPACITY_REPORT={"status":"passed","executed_heights":10}\')\nprint("test result: ok. 1 passed; 0 failed; 0 ignored;")\n')
            binary.chmod(0o755)
            manifest = dict(scope='storage', selection=None, tested_sha='abc', source_sha='abc',
                producer_run_id='1', producer_attempt='1', runner_image='test', rustc_version='test',
                binaries={'test': dict(path='test', cwd='.')})
            args = argparse.Namespace(inventory_entry_limit=5_000_000, bundle=bundle, results_dir=root / 'results', group='capacity',
                capacity_scale=10, repeat_faults=1, label='capacity', timeout_seconds=5)
            with patch.object(acceptance.artifacts, 'load', return_value=(manifest, dict(os.environ))), \
                 patch.object(acceptance, 'select', return_value=[dict(binary='test', batch=True, count=1)]), \
                 patch.object(acceptance.shutil, 'disk_usage', return_value=argparse.Namespace(total=20*1024**3, used=1*1024**3, free=10 * 1024**3)):
                acceptance.execute(args)
            report = json.loads((args.results_dir / 'summary.json').read_text())
            self.assertEqual(report['capacity_reports'], [dict(status='passed', executed_heights=10)])
            self.assertTrue(report['results'][0]['log_complete'])
            self.assertTrue(report['passed'])
            self.assertEqual(report['results'][0]['expected_count'], 1)
            self.assertIn('test result: ok.', (args.results_dir / report['results'][0]['log']).read_text())
            self.assertTrue(acceptance.summarize(args.results_dir, ['capacity'])['passed'])

    def test_optional_job_skip_is_exact(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / 'summary.json').write_text(json.dumps(self.report()))
            needs = {name: {'result': 'success'} for name in ('build', 'verify')}
            needs['capacity-long'] = {'result': 'skipped'}
            needs['full-node-long'] = {'result': 'skipped'}
            self.assertTrue(acceptance.summarize(root, ['units'], needs)['passed'])
            needs['capacity-long']['result'] = 'success'
            with self.assertRaises(ValueError):
                acceptance.summarize(root, ['units'], needs)

    def test_full_bundle_pack_checks_every_file_and_scope(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            bundle = root / 'bundle'
            (bundle / 'bin').mkdir(parents=True)
            binary = bundle / 'bin' / 'test'
            binary.write_bytes(b'original')
            manifest = dict(scope='storage', selection=None, libraries={}, binaries={
                'test': dict(path='bin/test', sha256=acceptance.artifacts.digest(binary))})
            (bundle / 'manifest.json').write_text(json.dumps(manifest))
            with patch.object(acceptance.artifacts, 'load', return_value=(manifest, {})):
                archive = root / 'bundle.tar.gz'
                acceptance.pack(bundle, archive)
                with tarfile.open(archive) as packed:
                    self.assertEqual(set(packed.getnames()), {'manifest.json', 'bin/test'})
                    self.assertEqual(packed.extractfile('bin/test').read(), b'original')
                binary.write_bytes(b'corrupt')
                with self.assertRaises(ValueError):
                    acceptance.pack(bundle, archive)
                manifest['scope'] = 'formal'
                with self.assertRaises(ValueError):
                    acceptance.pack(bundle, archive)

    def test_attempt_release_requires_exact_nonempty_ignored_inventory(self):
        manifest = self.manifest()
        with self.assertRaises(ValueError):
            acceptance.select(manifest, 'attempt-release')
        name = 'fixture::formal_attempt_release_crash_and_transport_failure_recover_live_remote_pins'
        manifest['binaries']['runtime-tests']['ignored_tests'].append(name)
        self.assertEqual(acceptance.select(manifest, 'attempt-release'), [dict(binary='runtime-tests', name=name, ignored=True)])
        manifest['binaries']['runtime-tests']['ignored_tests'].append(name)
        with self.assertRaises(ValueError):
            acceptance.select(manifest, 'attempt-release')

    def test_full_node_inventory_and_partial_reports_fail_closed(self):
        manifest = self.manifest()
        with self.assertRaises(ValueError):
            acceptance.select(manifest, 'full-node')
        name = 'fixture::formal_full_node_storage_maintenance_preserves_history'
        manifest['binaries']['runtime-tests']['ignored_tests'].append(name)
        self.assertEqual(acceptance.select(manifest, 'full-node'), [dict(binary='runtime-tests', name=name, ignored=True)])
        report = dict(status='passed', executed_commits=1000, inventory_entry_limit=5_000_000,
                      whole_case_time_budget_seconds=18000,
                      fixed_head_unchanged=True, logical_history_unchanged=True,
                      typed_history_unchanged=True, original_result_unchanged=True,
                      original_effect_unchanged=True, continuation_completed=True,
                      fixed_head='a'*64, authority_digest='b'*64, cold_history_evicted=True,
                      published_history_unchanged=True, accounting_quiescent=True,
                      fixed_head_position=1002, original_commit_position=2,
                      growth_before_maintenance_allocated_bytes=100,
                      maintenance_net_allocated_change_bytes=-10,
                      cold_restore_net_allocated_change_bytes=0)
        for field in ('initial', 'before', 'after_maintenance', 'after_cold_restore'):
            report[field] = dict(complete=True, total=dict(complete=True, allocated_bytes=100),
                                 entry_count_budget_per_inventory=5_000_000, whole_case_time_budget_seconds=18000)
        report['published_before'] = dict(complete=True, files=1002, bytes=64000, digest='d'*64,
            original_position=2, original_digest='e'*64, fixed_head_position=1002, fixed_head_digest='f'*64)
        report['published_after_maintenance'] = copy.deepcopy(report['published_before'])
        report['published_after_cold_restore'] = copy.deepcopy(report['published_before'])
        report['logical_before'] = {role: dict(retention_records_including_duplicate_digests=1, digest='a'*64)
                                    for role in ('primary', 'replica')}
        report['logical_after'] = copy.deepcopy(report['logical_before'])
        report['typed_history_before'] = dict(records=1000, digest='c'*64)
        report['typed_history_after'] = copy.deepcopy(report['typed_history_before'])
        report['original_commit_ref'] = dict(position=2, state_root_ref='original')
        ready = dict(schema=1, nonce='a'*64, parent_pid=11, child_pid=12, stage='ready',
                     fixed_head=report['fixed_head'], authority=report['authority_digest'],
                     typed=copy.deepcopy(report['typed_history_before']), result='b'*64,
                     effect='c'*64, emits=1, original_commit=None, final_position=1002,
                     continuation_completed=False, ready_node_running=False)
        report['cold_writer_ready'] = ready
        done = copy.deepcopy(ready)
        done.update(stage='done', original_commit=copy.deepcopy(report['original_commit_ref']),
                    final_position=1004, continuation_completed=True)
        report['cold_writer_done'] = done
        self.assertTrue(acceptance.full_node_passed(report, 1000))
        self.assertTrue(acceptance.full_node_inventory_budget_passed(report, 5_000_000))
        legacy = copy.deepcopy(report)
        del legacy['inventory_entry_limit']
        del legacy['whole_case_time_budget_seconds']
        for field in ('initial', 'before', 'after_maintenance', 'after_cold_restore'):
            del legacy[field]['entry_count_budget_per_inventory']
            del legacy[field]['whole_case_time_budget_seconds']
        self.assertTrue(acceptance.full_node_passed(legacy, 1000))
        self.assertFalse(acceptance.full_node_inventory_budget_passed(legacy, 5_000_000))
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            summary = self.report('full-node')
            summary.update(group='full-node', capacity_scale=1000,
                           inventory_entry_limit=5_000_000, full_node_reports=[report])
            path = directory / 'summary.json'
            path.write_text(json.dumps(summary))
            self.assertTrue(acceptance.summarize(directory, ['full-node'])['passed'])
            summary['full_node_reports'] = [legacy]
            path.write_text(json.dumps(summary))
            with self.assertRaisesRegex(ValueError, 'incomplete'):
                acceptance.summarize(directory, ['full-node'])

        self.assertFalse(acceptance.full_node_passed(report, 10000))
        self.assertFalse(acceptance.full_node_inventory_budget_passed(report, 10_000_000))
        for field in ('initial', 'before', 'after_maintenance', 'after_cold_restore'):
            for key in ('entry_count_budget_per_inventory', 'whole_case_time_budget_seconds'):
                changed = copy.deepcopy(report)
                del changed[field][key]
                self.assertFalse(acceptance.full_node_inventory_budget_passed(changed, 5_000_000), (field, key))
        exact = copy.deepcopy(report)
        exact['cold_writer_ready'].update(schema=2, continuation_commit=None)
        exact['cold_writer_done'].update(
            schema=2, emits=2, continuation_commit=dict(position=1003, execution_block_hash='b'*64))
        self.assertTrue(acceptance.full_node_passed(exact, 1000))
        for bad_commit in (None, dict(position=1002, execution_block_hash='b'*64),
                           dict(position=1005, execution_block_hash='b'*64),
                           dict(position=1003, execution_block_hash='wrong')):
            changed = copy.deepcopy(exact)
            changed['cold_writer_done']['continuation_commit'] = bad_commit
            self.assertFalse(acceptance.full_node_passed(changed, 1000), bad_commit)
        for missing in ('cold_writer_ready', 'cold_writer_done', 'original_commit_ref'):
            changed = copy.deepcopy(report)
            del changed[missing]
            self.assertFalse(acceptance.full_node_passed(changed, 1000), missing)
        for field, key, value in (
                ('cold_writer_ready', 'child_pid', 11),
                ('cold_writer_ready', 'parent_pid', True),
                ('cold_writer_ready', 'ready_node_running', True),
                ('cold_writer_ready', 'continuation_completed', True),
                ('cold_writer_ready', 'typed', {}),
                ('cold_writer_done', 'nonce', 'd'*64),
                ('cold_writer_done', 'child_pid', 13),
                ('cold_writer_done', 'parent_pid', 13),
                ('cold_writer_done', 'schema', 0),
                ('cold_writer_done', 'stage', 'ready'),
                ('cold_writer_done', 'result', 'd'*64),
                ('cold_writer_done', 'effect', 'd'*64),
                ('cold_writer_done', 'emits', 2),
                ('cold_writer_done', 'original_commit', dict(position=2, state_root_ref='wrong')),
                ('cold_writer_done', 'final_position', 1002),
                ('cold_writer_done', 'continuation_completed', False)):
            changed = copy.deepcopy(report)
            changed[field][key] = value
            self.assertFalse(acceptance.full_node_passed(changed, 1000), (field, key))
        for field in ('fixed_head_unchanged', 'logical_history_unchanged',
                      'typed_history_unchanged', 'original_result_unchanged',
                      'original_effect_unchanged', 'continuation_completed', 'published_history_unchanged', 'accounting_quiescent'):
            changed = copy.deepcopy(report)
            changed[field] = False
            self.assertFalse(acceptance.full_node_passed(changed, 1000))
        for field in ('before', 'after_maintenance', 'after_cold_restore'):
            changed = copy.deepcopy(report)
            changed[field]['complete'] = False
            self.assertFalse(acceptance.full_node_passed(changed, 1000))
        for field in ('published_before', 'published_after_maintenance', 'published_after_cold_restore'):
            changed = copy.deepcopy(report)
            changed[field]['digest'] = 'x'*64
            self.assertFalse(acceptance.full_node_passed(changed, 1000))
        report['status'] = 'partial'
        self.assertFalse(acceptance.full_node_passed(report, 1000))

    def test_summary_rejects_partial_nonzero_timeout_cancelled_or_unclean(self):
        for change in ({'exit_code': 1}, {'timeout': True}, {'cancelled': True},
                       {'cleanup_result': 'unresolved'}, {'passed': False}):
            with tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                report = self.report()
                report['results'][0].update(change)
                (root / 'summary.json').write_text(json.dumps(report))
                with self.assertRaises(ValueError):
                    acceptance.summarize(root, ['units'])
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            report = self.report('capacity-10')
            report['group'] = 'capacity'
            (root / 'summary.json').write_text(json.dumps(report))
            with self.assertRaises(ValueError):
                acceptance.summarize(root, ['capacity-10'])


if __name__ == '__main__':
    unittest.main()
