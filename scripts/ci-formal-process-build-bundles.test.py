#!/usr/bin/env python3
"""Build and bundle regressions split from ci-formal-process.test.py."""
from contextlib import redirect_stdout
import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tarfile
import tempfile
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


class BuildAndBundles(unittest.TestCase):
    def test_ordinary_docker_preflight_checks_daemon_and_default_driver(self):
        responses = {
            ('info', '--format', '{{json .ServerVersion}}'): '"28.0.4"',
            ('version', '--format', '{{.Client.Version}} {{.Server.APIVersion}}'): '28.5.2 1.48',
            ('buildx', 'version'): 'github.com/docker/buildx v0.29.1 test-commit',
            ('context', 'show'): 'default',
            ('buildx', 'inspect'): ('Name:          default\nDriver:        docker\n\nNodes:\n'
                                    'Name:          default\nEndpoint:      default\nStatus:        running\n'),
        }

        def invoke(command, **kwargs):
            self.assertEqual(command[0], 'docker')
            self.assertEqual(kwargs['timeout'], 15)
            self.assertTrue(kwargs['text'])
            for name in ('GH_TOKEN', 'GITHUB_TOKEN', 'ACTIONS_RUNTIME_TOKEN', 'INPUT_GITHUB-TOKEN', 'NODE_OPTIONS'):
                self.assertNotIn(name, kwargs['env'])
            if tuple(command[1:]) not in responses:
                raise subprocess.CalledProcessError(1, command, output='unknown command or flag')
            return responses[tuple(command[1:])]

        with patch.object(artifacts.Path, 'is_socket', return_value=True), \
             patch.dict(os.environ, DOCKER_HOST='', DOCKER_CONTEXT='', BUILDX_BUILDER='',
                        GH_TOKEN='private', GITHUB_TOKEN='private', ACTIONS_RUNTIME_TOKEN='private',
                        NODE_OPTIONS='private', **{'INPUT_GITHUB-TOKEN': 'private'}), \
             patch.object(artifacts.subprocess, 'check_output', side_effect=invoke) as execute:
            self.assertEqual(artifacts.ordinary_docker_preflight(), {
                'client_version': '28.5.2', 'server_version': '28.0.4', 'server_api': '1.48',
                'buildx_version': 'v0.29.1', 'context': 'default', 'builder': 'default', 'driver': 'docker',
                'builder_status': 'running'})
            self.assertEqual(execute.call_count, 5)
            for key, invalid in (
                    (('info', '--format', '{{json .ServerVersion}}'), 'null'),
                    (('version', '--format', '{{.Client.Version}} {{.Server.APIVersion}}'), '28.5.2 invalid-api'),
                    (('buildx', 'version'), 'unknown plugin'),
                    (('context', 'show'), 'remote'),
                    (('buildx', 'inspect'), 'Name: default\nDriver: docker-container\n\nNodes:\nStatus: running'),
                    (('buildx', 'inspect'), 'Name: default\nDriver: docker\nError: private\n\nNodes:\nStatus: running'),
                    (('buildx', 'inspect'), 'Name: default\nDriver: docker\n\nNodes:\nStatus: stopped'),
                    (('buildx', 'inspect'), 'Name: default\nDriver: docker\n\nNodes:\nStatus: running\nError: private')):
                with self.subTest(command=key):
                    original = responses[key]
                    responses[key] = invalid
                    try:
                        with self.assertRaises(ValueError):
                            artifacts.ordinary_docker_preflight()
                    finally:
                        responses[key] = original
            for environment in ({'DOCKER_HOST': 'tcp://remote:2375'},
                                {'DOCKER_CONTEXT': 'remote'}, {'BUILDX_BUILDER': 'remote'}):
                with self.subTest(environment=environment), patch.dict(os.environ, environment):
                    execute.reset_mock()
                    with self.assertRaises(ValueError):
                        artifacts.ordinary_docker_preflight()
                    execute.assert_not_called()
            with patch.object(artifacts.Path, 'is_socket', return_value=False):
                with self.assertRaisesRegex(ValueError, 'runner-provided Docker socket'):
                    artifacts.ordinary_docker_preflight()
            for failure in (subprocess.CalledProcessError(1, ['docker', 'info'], output='private'),
                            subprocess.TimeoutExpired(['docker', 'info'], 15, output='private')):
                with self.subTest(failure=type(failure).__name__), \
                     patch.object(artifacts.subprocess, 'check_output', side_effect=failure):
                    with self.assertRaisesRegex(ValueError, 'daemon info') as rejected:
                        artifacts.ordinary_docker_preflight()
                    self.assertNotIn('private', str(rejected.exception))

    def test_ordinary_image_cli_reports_the_checked_docker_capability(self):
        docker = {'driver': 'docker', 'server_api': '1.48'}
        output = io.StringIO()
        with patch.object(artifacts, 'ordinary_image_preflight', return_value={'same_directory': True}), \
             patch.object(artifacts, 'ordinary_docker_preflight', return_value=docker) as checked, \
             patch.object(sys, 'argv', ['ci-test-artifacts.py', 'ordinary-image-preflight']), \
             patch.multiple(artifacts, ROOT=artifacts.ROOT, CONFIG=artifacts.CONFIG,
                            FORMAL_AUTHORITY=artifacts.FORMAL_AUTHORITY), redirect_stdout(output):
            artifacts.main()
        checked.assert_called_once_with()
        self.assertEqual(json.loads(output.getvalue()), {'same_directory': True, 'docker': docker})

    def test_ordinary_image_cli_refuses_missing_canonical_docker(self):
        with patch.object(artifacts, 'ordinary_image_preflight', return_value={'same_directory': True}), \
             patch.object(artifacts.Path, 'is_socket', return_value=True), \
             patch.object(artifacts.subprocess, 'check_output', side_effect=FileNotFoundError('docker')), \
             patch.dict(os.environ, DOCKER_HOST='', DOCKER_CONTEXT='', BUILDX_BUILDER=''), \
             patch.object(sys, 'argv', ['ci-test-artifacts.py', 'ordinary-image-preflight']), \
             patch.multiple(artifacts, ROOT=artifacts.ROOT, CONFIG=artifacts.CONFIG,
                            FORMAL_AUTHORITY=artifacts.FORMAL_AUTHORITY), redirect_stdout(io.StringIO()):
            with self.assertRaisesRegex(ValueError, 'Docker CLI'):
                artifacts.main()

    def test_ordinary_image_preflight_preserves_canonical_real_directory(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            checkout, other = root / 'checkout', root / 'other'
            checkout.mkdir()
            other.mkdir()
            alias = root / 'symlink'
            alias.symlink_to(checkout, target_is_directory=True)
            original_cwd = Path.cwd()
            try:
                os.chdir(checkout)
                with patch.object(artifacts, 'ROOT', checkout), \
                     patch.object(artifacts, 'ORDINARY_HOST_CHECKOUT', checkout), \
                     patch.object(artifacts.platform, 'freedesktop_os_release',
                                  return_value={'ID': 'ubuntu', 'VERSION_ID': '24.04'}), \
                     patch.dict(os.environ, ImageOS='ubuntu24', ORDINARY_ARTIFACT_CHECKOUT=str(checkout),
                                GITHUB_WORKSPACE=str(checkout),
                                ORDINARY_BUILD_IMAGE='ghcr.io/nil-studio/oasis7/ci-rust-build@sha256:' + 'a' * 64):
                    report = artifacts.ordinary_image_preflight()
                    self.assertTrue(report['same_directory'])
                    self.assertEqual((report['device'], report['inode']),
                                     (checkout.stat().st_dev, checkout.stat().st_ino))
                    for changes in ({'GITHUB_WORKSPACE': str(other)},
                                    {'GITHUB_WORKSPACE': str(alias)}, {'GITHUB_WORKSPACE': ''},
                                    {'ORDINARY_ARTIFACT_CHECKOUT': str(other)},
                                    {'ORDINARY_BUILD_IMAGE': 'rust-build:latest'},
                                    {'ImageOS': ''}, {'ImageOS': 'linux'}):
                        with self.subTest(changes=changes), patch.dict(os.environ, changes), self.assertRaises(ValueError):
                            artifacts.ordinary_image_preflight()
                    self.assertEqual(report['runner_image'], 'ubuntu24')
                    for release in ({'ID': 'debian', 'VERSION_ID': '24.04'},
                                    {'ID': 'ubuntu', 'VERSION_ID': '22.04'}, {}):
                        with patch.object(artifacts.platform, 'freedesktop_os_release', return_value=release), self.assertRaises(ValueError):
                            artifacts.ordinary_image_preflight()
                    with patch.object(artifacts, 'ROOT', other), self.assertRaises(ValueError):
                        artifacts.ordinary_image_preflight()
                    os.chdir(other)
                    with self.assertRaises(ValueError):
                        artifacts.ordinary_image_preflight()
            finally:
                os.chdir(original_cwd)

    def test_ordinary_candidate_isolation_and_cli_forbid_docker_access(self):
        with tempfile.TemporaryDirectory(dir='/tmp') as temporary:
            sock_path = Path(temporary) / 'docker.sock'
            with patch.object(os, 'getuid', return_value=1001), \
                 patch.object(os, 'getgid', return_value=1001), \
                 patch.object(os, 'geteuid', return_value=1001), \
                 patch.object(os, 'getegid', return_value=1001), \
                 patch.object(os, 'getgroups', return_value=[1001]), \
                 patch.dict(os.environ, {'DOCKER_HOST': ''}, clear=False):
                self.assertEqual(
                    artifacts.ordinary_candidate_isolation_preflight(sock_path),
                    {'uid': 1001, 'gid': 1001, 'groups': [1001], 'docker_socket': 'absent'},
                )
                server = artifacts.socket.socket(artifacts.socket.AF_UNIX, artifacts.socket.SOCK_STREAM)
                try:
                    server.bind(str(sock_path))
                    server.listen(1)
                    sock_path.chmod(0o600)
                    with self.assertRaisesRegex(ValueError, 'must not access'):
                        artifacts.ordinary_candidate_isolation_preflight(sock_path)
                    with patch.object(os, 'access', return_value=False):
                        with self.assertRaisesRegex(ValueError, 'must not connect'):
                            artifacts.ordinary_candidate_isolation_preflight(sock_path)
                    if os.geteuid() != 0:
                        sock_path.chmod(0o000)
                        self.assertEqual(
                            artifacts.ordinary_candidate_isolation_preflight(sock_path),
                            {'uid': 1001, 'gid': 1001, 'groups': [1001], 'docker_socket': 'permission_denied'},
                        )
                finally:
                    try:
                        sock_path.chmod(0o600)
                    except OSError:
                        pass
                    server.close()
                for bad_patch in (
                    patch.object(os, 'getuid', return_value=0),
                    patch.object(os, 'getgid', return_value=0),
                    patch.object(os, 'geteuid', return_value=0),
                    patch.object(os, 'getgroups', return_value=[1001, 999]),
                    patch.dict(os.environ, {'DOCKER_HOST': 'unix:///var/run/docker.sock'}, clear=False),
                ):
                    with bad_patch, self.assertRaises(ValueError):
                        artifacts.ordinary_candidate_isolation_preflight(Path(temporary) / 'missing.sock')

            tested = 'a' * 40
            source = 'b' * 40
            base_plan = {
                'fast_plan_version': 1,
                'execution_profile': 'pr-fast',
                'scope': 'full',
                'groups': ['baseline', 'rust_baseline', 'oasis7_required'],
                'oasis7_required_packages': ['oasis7_runtime'],
                'test_policy_changed': False,
                'source_sha': source,
                'tested_sha': tested,
                'architecture': {'version': 1, 'base': 'c' * 40, 'source': source, 'tested': tested},
                'risk_test_inventory_version': 1,
                'ordinary_risk_targets': [{
                    'package': 'oasis7_runtime',
                    'target_kind': 'test',
                    'target_name': 'incremental_full_scales',
                    'default_features': True,
                    'features': ['test_tier_full'],
                    'cases': ['case_a'],
                }],
            }
            self.assertTrue(artifacts.ordinary_candidate_image_eligible(base_plan))
            for ineligible in (
                {**base_plan, 'test_policy_changed': True},
                {**base_plan, 'execution_profile': 'candidate-full'},
                {**base_plan, 'ordinary_full_fast_targets': []},
                {**base_plan, 'ordinary_risk_targets': [{**base_plan['ordinary_risk_targets'][0], 'target_kind': 'lib'}]},
                {**base_plan, 'ordinary_risk_targets': [{**base_plan['ordinary_risk_targets'][0], 'package': 'oasis7_chain_runtime'}]},
            ):
                self.assertFalse(artifacts.ordinary_candidate_image_eligible(ineligible))

            plan_file = Path(temporary) / 'selection.json'
            plan_file.write_text(json.dumps(base_plan))
            digest = hashlib.sha256(plan_file.read_bytes()).hexdigest()
            isolation = {'uid': 1001, 'gid': 1001, 'groups': [1001], 'docker_socket': 'permission_denied'}
            output = io.StringIO()
            with patch.object(artifacts, 'ordinary_image_preflight', return_value={'same_directory': True}), \
                 patch.object(artifacts, 'ordinary_candidate_isolation_preflight', return_value=isolation), \
                 patch.object(artifacts, 'ordinary_docker_preflight', side_effect=AssertionError('must not run docker preflight')), \
                 patch.object(artifacts, 'output', return_value=tested), \
                 patch.dict(os.environ, {'SOURCE_SHA': source}, clear=False), \
                 patch.object(sys, 'argv', [
                     'ci-test-artifacts.py', 'ordinary-image-preflight', '--forbid-docker',
                     '--plan', str(plan_file), '--expected-selection-sha256', digest,
                 ]), \
                 patch.multiple(artifacts, ROOT=artifacts.ROOT, CONFIG=artifacts.CONFIG,
                                FORMAL_AUTHORITY=artifacts.FORMAL_AUTHORITY), redirect_stdout(output):
                artifacts.main()
            self.assertEqual(json.loads(output.getvalue()),
                             {'same_directory': True, 'candidate_isolation': isolation})
            for bad_digest in ('0' * 64, 'not-a-sha256'):
                with patch.object(artifacts, 'ordinary_image_preflight', return_value={'same_directory': True}), \
                     patch.object(sys, 'argv', [
                         'ci-test-artifacts.py', 'ordinary-image-preflight', '--forbid-docker',
                         '--plan', str(plan_file), '--expected-selection-sha256', bad_digest,
                     ]), \
                     patch.multiple(artifacts, ROOT=artifacts.ROOT, CONFIG=artifacts.CONFIG,
                                    FORMAL_AUTHORITY=artifacts.FORMAL_AUTHORITY):
                    with self.assertRaisesRegex(ValueError, 'selection digest mismatch'):
                        artifacts.main()

            bundle_dir = Path(temporary) / 'forbid-bundle'
            with patch.object(artifacts, 'build_variants', return_value=[]), \
                 patch.object(artifacts, 'output', side_effect=lambda cmd: 'host: x86_64-unknown-linux-gnu' if cmd == ['rustc', '-vV'] else tested), \
                 patch.object(artifacts, 'digest', return_value='d' * 64), \
                 patch.object(artifacts, 'formal_configuration', return_value={'suites': {}, 'shell_tests': {}, 'environment_formal_batches': {}}), \
                 patch.object(artifacts.BuiltinTestArtifact, 'required', return_value=True), \
                 patch.object(artifacts.BuiltinTestArtifact, 'build', side_effect=AssertionError('must not invoke Docker WASM build')), \
                 patch.dict(os.environ, {'OASIS7_FORBID_ORDINARY_DOCKER_WASM': '1', 'SOURCE_SHA': tested}, clear=False):
                with self.assertRaisesRegex(ValueError, 'forbids canonical Docker WASM builds'):
                    artifacts.build(bundle_dir, 'ordinary')

    def test_build_timings_include_dependency_freshness_per_variant(self):
        with tempfile.TemporaryDirectory() as temp:
            base = Path(temp)
            root = base / 'checkout'
            root.mkdir()
            config = root / 'suites.json'
            config.write_text('{}')
            def artifact(local, fresh):
                return dict(reason='compiler-artifact', fresh=fresh,
                    manifest_path=str((root if local else base / 'dependency') / 'Cargo.toml'))
            messages = [
                [artifact(True, False), artifact(False, True), artifact(True, True),
                 artifact(False, False), artifact(False, False), dict(reason='build-script-executed')],
                [artifact(False, True), artifact(True, True), dict(reason='build-finished', success=True)],
            ]
            class CargoOutput:
                def __init__(self, entries):
                    self.stdout = iter(json.dumps(entry) + '\n' for entry in entries)
                def __enter__(self):
                    return self
                def __exit__(self, *args):
                    pass
                def wait(self):
                    return 0
            bundle = base / 'bundle'
            with patch.object(artifacts, 'ROOT', root), patch.object(artifacts, 'CONFIG', config), \
                 patch.object(artifacts, 'build_variants', return_value=[('first', ['build'], []), ('second', ['test'], [])]), \
                 patch.object(artifacts.subprocess, 'Popen', side_effect=[CargoOutput(entries) for entries in messages]), \
                 patch.object(artifacts, 'output', return_value='host: fixture'), patch('builtins.print') as printed:
                artifacts.build(bundle, 'ordinary')
            timings = json.loads((bundle / 'manifest.json').read_text())['build_timings']
            observed = [{key: value for key, value in timing.items() if key != 'elapsed_seconds'} for timing in timings]
            self.assertEqual(observed, [
                dict(variant='first', local_units=2, local_fresh=1, local_rebuilt=1,
                     dependency_units=3, dependency_fresh=1, dependency_rebuilt=2),
                dict(variant='second', local_units=1, local_fresh=1, local_rebuilt=0,
                     dependency_units=1, dependency_fresh=1, dependency_rebuilt=0),
            ])
            reported = [json.loads(call.args[0].removeprefix('native_build='))
                        for call in printed.call_args_list if call.args[0].startswith('native_build=')]
            self.assertEqual(reported, timings)

    def test_ordinary_cache_state_selects_libraries_and_other_scopes_keep_all(self):
        with tempfile.TemporaryDirectory() as temp:
            base = Path(temp)
            root = base / 'checkout'
            root.mkdir()
            manifest = root / 'Cargo.toml'
            manifest.write_text('[package]\nname="cache_local"\nversion="0.1.0"\n')
            source = root / 'lib.rs'
            source.write_text('pub fn value() {}\n')
            config = root / 'suites.json'
            config.write_text(json.dumps(dict(suites={}, legacy_ignored_filters={},
                                             environment_formal_batches={}, shell_tests={})))
            variants = ('viewer', 'required', 'snapshot-tests', 'viewer-snapshot-tests', 'required-full-fast-0')
            messages, fingerprints = [], []
            for index, variant in enumerate(variants):
                unit_hash = f'{index + 1:016x}'
                library = root / 'target/debug/deps' / ('libcache_local-' + unit_hash + '.rlib')
                library.parent.mkdir(parents=True, exist_ok=True)
                library.write_text(variant)
                fingerprint = 'debug/.fingerprint/cache_local-' + unit_hash
                fingerprints.append(fingerprint)
                directory = root / 'target' / fingerprint
                directory.mkdir(parents=True)
                (directory / 'lib-cache_local').write_text(variant)
                messages.append(dict(reason='compiler-artifact', fresh=index == 1,
                    manifest_path=str(manifest), package_id='cache_local', filenames=[str(library)],
                    target=dict(name='cache_local', kind=['lib'], src_path=str(source)), profile=dict(test=False)))

            class CargoOutput:
                def __init__(self, entries):
                    self.stdout = iter(json.dumps(entry) + '\n' for entry in entries)
                def __enter__(self):
                    return self
                def __exit__(self, *args):
                    pass
                def wait(self):
                    return 0

            for scope in ('ordinary', 'formal', 'storage', 'all'):
                with self.subTest(scope=scope), \
                     patch.object(artifacts, 'ROOT', root), patch.object(artifacts, 'CONFIG', config), \
                     patch.object(artifacts, 'build_variants', return_value=[(name, ['test'], []) for name in variants]), \
                     patch.object(artifacts.subprocess, 'Popen', side_effect=[CargoOutput([message])
                         for message in messages[:-1]] + [CargoOutput([dict(messages[0], fresh=True), messages[-1]])]), \
                     patch.object(artifacts, 'output', return_value='host: fixture'), \
                     patch.object(artifacts, 'cache_begin', wraps=artifacts.cache_begin) as begin, \
                     patch('builtins.print'):
                    state_path = base / (scope + '-state.json')
                    artifacts.build(base / scope, scope, state_path)
                    selection = 'libraries' if scope == 'ordinary' else 'all'
                    begin.assert_called_once_with(root, selection=selection)
                    state = json.loads(state_path.read_text())
                    self.assertEqual(state['selection'], selection)
                    self.assertTrue(state['complete'])
                    self.assertEqual(state['directories'], fingerprints)
                    if scope == 'ordinary':
                        self.assertEqual(state['ordinary_baseline_units'], fingerprints[:-1])
                    else:
                        self.assertNotIn('ordinary_baseline_units', state)

    def test_formal_builder_does_not_wait_for_ordinary_targets(self):
        formal = artifacts.build_variants('formal')
        ordinary = artifacts.build_variants('ordinary')
        self.assertEqual({v[0] for v in formal}, {'formal-required', 'runtime', 'endpoint', 'distfs-tests'})
        expected_ordinary = {'viewer', 'required', 'viewer-snapshot-tests'}
        if sys.platform == 'linux' and hasattr(artifacts, 'RUNTIME_WORKER_VARIANT'):
            expected_ordinary.add(artifacts.RUNTIME_WORKER_VARIANT)
        self.assertEqual({v[0] for v in ordinary}, expected_ordinary)
        args = next(args for name, args, _ in formal if name == 'formal-required')
        self.assertEqual([args[i + 1] for i, arg in enumerate(args) if arg == '-p'], ['oasis7_chain_runtime'])
        self.assertIn('--bins', args)
        self.assertIn('--lib', args)
        self.assertNotIn('oasis7_viewer_runtime', args)
        self.assertNotIn('oasis7_ops_tools', args)
        self.assertIn('ci_formal_required_features', args[args.index('--features') + 1].split(','))
        for variant, command, features in [*ordinary, *formal[1:], *artifacts.build_variants('all')]:
            self.assertNotIn('ci_formal_required_features', features, variant)
            self.assertFalse(any('ci_formal_required_features' in arg for arg in command), variant)
        for owner, package in [('viewer-snapshot-tests', 'oasis7_viewer_runtime')]:
            snapshot = next(args for name, args, _ in ordinary if name == owner)
            self.assertEqual(snapshot, ['test', '-p', package, '--lib', '--no-run'])
            self.assertEqual(next(features for name, _, features in ordinary if name == owner), [])
        required = next(args for name, args, _ in ordinary if name == 'required')
        required_packages = [required[index + 1] for index, value in enumerate(required) if value == '-p']
        scope_required_packages = json.loads((HERE / 'ci-required-scope.json').read_text(encoding='utf-8'))['oasis7_required_packages']
        self.assertEqual(required_packages, scope_required_packages)
        self.assertCountEqual(required_packages, [
            'oasis7_simulator', 'oasis7_runtime', 'oasis7_runtime_host', 'oasis7_world_state', 'oasis7_world_projection', 'oasis7_world_service', 'scenario_test_runner', 'oasis7_chain_runtime', 'oasis7_executable_image', 'oasis7_client_cli', 'oasis7_protocol_tools', 'oasis7_launcher_backend',
            'oasis7_newapi_bridge_service', 'oasis7_ops_tools', 'oasis7_provider_local_bridge',
            'oasis7_provider_tools', 'oasis7_viewer_runtime', 'oasis7_viewer_codec', 'oasis7_observability',
            'oasis7_hosted_access', 'oasis7_letai_credit_adapter',
            'oasis7_wasm_materializer', 'oasis7_game_client', 'oasis7_browser_game_client', 'oasis7_cas', 'oasis7_formal_protocol',
        ])
        self.assertIn('--tests', required)
        shell = (HERE / 'ci-tests.sh').read_text()
        self.assertIn('run_cargo test -p oasis7_viewer_runtime --lib snapshot_progress::', shell)
        self.assertIn('run_cargo test -p oasis7_runtime_host --lib snapshot_player_gameplay_execution_state_backfills_from_legacy_fields', shell)

    def test_formal_alias_requires_owning_package_and_exact_target_kind(self):
        classify = artifacts.artifact_logical_name
        for variant in ('required', 'formal-required'):
            with self.subTest(variant=variant):
                self.assertEqual(classify(variant, 'oasis7_chain_runtime',
                    dict(name='oasis7_chain_runtime', kind=['lib']), True), 'runtime-tests')
                self.assertEqual(classify(variant, 'oasis7',
                    dict(name='oasis7', kind=['lib']), True), 'oasis7-lib-tests')
                for package, kind, name in [('oasis7_chain_runtime', 'bin', 'oasis7_chain_runtime'),
                                            ('other_package', 'bin', 'oasis7_chain_runtime'),
                                            ('oasis7', 'bin', 'oasis7')]:
                    logical = classify(variant, package, dict(name=name, kind=[kind]), True)
                    self.assertNotIn(logical, ('runtime-tests', 'oasis7-lib-tests'))
                    self.assertIn(package + '-' + kind + '-', logical)
                self.assertTrue(classify(variant, 'oasis7_chain_runtime',
                    dict(name='oasis7_chain_runtime', kind=['bin']), False).startswith('required-executable-'))

    def test_chain_library_and_binary_harnesses_keep_distinct_suite_owners(self):
        # Cargo emits both executable harnesses with target.name == package name.
        # Exercise the producer JSON path without compiling a Cargo workspace.
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            package = root / 'crates/oasis7_chain_runtime'
            package.mkdir(parents=True)
            manifest_path = package / 'Cargo.toml'
            manifest_path.write_text('[package]\nname = "oasis7_chain_runtime"\n')
            messages = []
            for kind in ('lib', 'bin'):
                executable = root / ('chain-' + kind)
                executable.write_bytes(kind.encode())
                messages.append(dict(reason='compiler-artifact', manifest_path=str(manifest_path),
                    package_id='path+file://' + str(package) + '#0.1.0', fresh=False,
                    target=dict(name='oasis7_chain_runtime', kind=[kind]),
                    profile=dict(test=True, opt_level='0', debug_assertions=True, overflow_checks=True),
                    features=[], executable=str(executable)))
            class CargoOutput:
                def __enter__(self):
                    self.stdout = iter(json.dumps(message) + '\n' for message in messages)
                    return self
                def __exit__(self, *args):
                    pass
                def wait(self):
                    return 0
            config_path = root / 'suites.json'
            config = dict(suites={'runtime-protocol': [dict(binary='runtime-tests', name='formal_case', ignored=True)]},
                          legacy_ignored_filters={'runtime-tests': ['formal_case']},
                          environment_formal_batches={}, shell_tests={})
            config_path.write_text(json.dumps(config))
            def inventory(path, ignored=False, env=None):
                if path.name == 'runtime-tests':
                    return ['formal_case'] if ignored else ['config::contract']
                return []
            for variant, scope in [('required', 'ordinary'), ('formal-required', 'formal')]:
                with self.subTest(variant=variant):
                    bundle = root / variant
                    with patch.object(artifacts, 'ROOT', root), patch.object(artifacts, 'CONFIG', config_path), \
                         patch.object(artifacts, 'build_variants', return_value=[(variant, ['test'], [])]), \
                         patch.object(artifacts.subprocess, 'Popen', return_value=CargoOutput()), \
                         patch.object(artifacts, 'inventory', side_effect=inventory), \
                         patch.object(artifacts, 'output', return_value='host: fixture'), \
                         patch.object(artifacts.sys, 'platform', 'fixture'):
                        artifacts.build(bundle, scope)
                    built = json.loads((bundle / 'manifest.json').read_text())
                    binaries = built['binaries']
                    library = next(name for name, binary in binaries.items() if binary.get('target_kind') == ['lib'])
                    self.assertEqual(library, 'runtime-tests')
                    self.assertEqual(binaries['runtime-tests']['target_kind'], ['lib'])
                    self.assertEqual(binaries[library]['package'], 'oasis7_chain_runtime')
                    self.assertEqual(binaries[library]['ordinary'], scope == 'ordinary')
                    if scope == 'formal':
                        entries = artifacts.selected_entries(built, config, suite='runtime-protocol')
                        self.assertEqual([entry['binary'] for entry in entries], ['runtime-tests'])
                        continue
                    for owner, pattern in artifacts.DEFAULT_SNAPSHOT_REGRESSIONS:
                        binaries[owner] = dict(path=owner, cwd='.', test=True, ordinary=False,
                            tests=[pattern + ('one' if pattern.endswith('::') else '')], ignored_tests=[])
                    self.assertIn(library, artifacts.dependency_closure(built, config, suite='ordinary'))
                    with patch.object(artifacts, 'ROOT', root), patch.object(artifacts, 'load', return_value=(built, {})), \
                         patch('ci_process_runner.run', return_value=dict(passed=True, cleanup_result='clean')) as run_test:
                        artifacts.execute(bundle, 'ordinary', root / 'results.jsonl')
                    library_runs = [call for call in run_test.call_args_list
                                    if call.args[0][0] == str(bundle / binaries[library]['path'])]
                    self.assertEqual(len(library_runs), 1)
                    self.assertEqual(library_runs[0].kwargs['expected_count'], 1)
            messages.append(messages[0])
            with patch.object(artifacts, 'ROOT', root), \
                 patch.object(artifacts, 'build_variants', return_value=[('required', ['test'], [])]), \
                 patch.object(artifacts.subprocess, 'Popen', return_value=CargoOutput()), \
                 patch.object(artifacts.sys, 'platform', 'fixture'):
                with self.assertRaisesRegex(ValueError, 'duplicate executable: runtime-tests'):
                    artifacts.build(root / 'duplicate', 'ordinary')

    def test_closure_contains_child_executables_but_not_other_suites(self):
        config = {'binary_dependencies': {'runtime-tests': ['runtime', 'endpoint'],
                                           'distfs-tests': ['endpoint']}, 'script_dependencies': {}}
        binaries = {name: {'test': name.endswith('tests')} for name in
                    ('runtime-tests', 'runtime', 'endpoint', 'distfs-tests', 'viewer', 'snapshot-tests', 'viewer-snapshot-tests')}
        manifest = {'binaries': binaries, 'suites': {'storage-recovery': [
            {'binary': 'distfs-tests', 'name': 'storage', 'ignored': True}]}}
        self.assertEqual(artifacts.dependency_closure(manifest, config, suite='storage-recovery'),
                         {'distfs-tests', 'endpoint'})
        del binaries['endpoint']
        with self.assertRaises(ValueError):
            artifacts.dependency_closure(manifest, config, suite='storage-recovery')

    def test_low_compression_package_preserves_modes_and_omits_unused_binary(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            bundle = root / 'bundle'
            (bundle / 'bin').mkdir(parents=True)
            (bundle / 'lib').mkdir()
            binaries = {}
            for name in ('viewer', 'snapshot-tests', 'viewer-snapshot-tests', 'required-test-lib-example', 'endpoint'):
                path = bundle / 'bin' / name
                path.write_bytes(b'fixture executable\n' * 2000)
                path.chmod(0o755)
                binaries[name] = dict(path=f'bin/{name}', sha256=artifacts.digest(path),
                    bytes=path.stat().st_size, test=name.endswith('tests') or name.startswith('required-test'),
                    ordinary=name.startswith('required-test'), tests=['one'], ignored_tests=[], libraries=[])
            (bundle / 'manifest.json').write_text(json.dumps(dict(schema_version=2, scope='all',
                binaries=binaries, suites={}, libraries={})))
            config = root / 'suites.json'
            config.write_text(json.dumps({}))
            with patch.object(artifacts, 'CONFIG', config):
                report = artifacts.pack(bundle, root / 'ordinary.tar.gz', suite='ordinary')
            self.assertLess(report['compressed_bytes'], report['raw_bytes'])
            with tarfile.open(root / 'ordinary.tar.gz') as archive:
                names = archive.getnames()
                self.assertNotIn('bin/endpoint', names)
                self.assertIn('bin/viewer', names)
                self.assertEqual(archive.getmember('bin/viewer').mode & 0o777, 0o755)
                manifest = json.load(archive.extractfile('manifest.json'))
                self.assertEqual(manifest['selection'], {'suite': 'ordinary'})
                self.assertNotIn('endpoint', manifest['included_binaries'])
                self.assertTrue({'snapshot-tests', 'viewer-snapshot-tests'} <= set(manifest['included_binaries']))
            for owner in ('viewer-snapshot-tests',):
                incomplete = dict(binaries)
                del incomplete[owner]
                with self.assertRaisesRegex(ValueError, 'missing default-feature snapshot harness: ' + owner):
                    artifacts.dependency_closure(dict(binaries=incomplete), {}, suite='ordinary')

    def test_cache_budget_is_a_skip_decision_not_a_test_failure(self):
        with tempfile.TemporaryDirectory() as temp:
            target = Path(temp)
            (target / 'artifact').write_bytes(b'x' * 101)
            self.assertFalse(artifacts.cache_budget(target, 100)['save'])
            self.assertTrue(artifacts.cache_budget(target, 101)['save'])


if __name__ == '__main__':
    unittest.main()
