#!/usr/bin/env python3
import contextlib
import importlib.util
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location('selector', HERE / 'plan-rust-required-scope.py')
selector = importlib.util.module_from_spec(spec)
spec.loader.exec_module(selector)


class SelectionPart5(unittest.TestCase):
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

    def test_additions_only_expand_and_groups_only(self):
        self.write('doc/readme.md', 'changed')
        head = self.commit()
        plan = selector.select(self.config, self.base, head, additions=['other'])
        self.assertEqual(set(plan['groups']), {'baseline', 'other'})
        self.assertNotIn('matrices', plan)
        with self.assertRaises(ValueError):
            selector.select(self.config, self.base, head, additions=['unknown'])

    def test_github_output_contains_plan_and_flags_without_matrix_outputs(self):
        self.write('doc/readme.md', 'changed')
        head = self.commit()
        Path('config.json').write_text(json.dumps(self.config))
        arguments = ['selector', '--base-ref', self.base, '--head-ref', head,
                     '--config', 'config.json', '--github-output', 'outputs.txt']
        with patch.object(sys, 'argv', arguments), contextlib.redirect_stdout(io.StringIO()):
            selector.main()
        outputs = dict(line.split('=', 1) for line in Path('outputs.txt').read_text().splitlines())
        plan = json.loads(outputs['plan'])
        self.assertEqual(set(plan), {'groups', 'resources', 'reasons', 'scope', 'architecture',
                                    'workspace_support_profiles', 'workspace_support_profile_version'})
        self.assertEqual(outputs['run_baseline'], 'true')
        self.assertEqual(outputs['run_other'], 'false')
        self.assertFalse(any(name.startswith('matrix') for name in outputs))

    def test_real_tool_rules_select_only_related_groups(self):
        config = json.loads((HERE / 'ci-required-scope.json').read_text())
        cases = {
            'resource-cleanup-executor.py': {'baseline', 'workflow_governance', 'fleet_health'},
            'new-task-worktree.sh': {'baseline', 'cargo_tooling_contracts', 'fleet_health'},
            'worktree-gc-report.sh': {'baseline', 'workflow_governance'},
            'pr-review-thread-closeout.py': {'baseline', 'workflow_governance'},
            'cargo-dev.sh': {'baseline', 'cargo_tooling_contracts'},
        }
        for filename, expected in cases.items():
            with self.subTest(filename=filename):
                self.write('scripts/' + filename, 'fixture')
                head = self.commit()
                plan = selector.select(config, self.base, head)
                self.assertEqual(set(plan['groups']), expected)
                self.assertEqual(plan['scope'], 'targeted')
                self.base = head


    def test_local_signer_precise_consumers(self):
        config = json.loads((HERE / 'ci-required-scope.json').read_text())
        common = {'baseline', 'packaging_contracts', 'macos_package_contract', 'fleet_health'}
        cases = {
            'scripts/local-signer/tests/test_future.py': common,
            'doc/p2p/blockchain/local-file-signing-backend.runbook.md': {'baseline', 'packaging_contracts', 'macos_package_contract'},
            **{'scripts/local-signer/' + name: common | {'workspace_support'}
               for name in ['installer.py', 'macos_host.py', 'install-release.py', 'package-release.py']},
        }
        for path, expected in cases.items():
            with self.subTest(path=path):
                self.write(path, 'fixture')
                head = self.commit()
                plan = selector.select(config, self.base, head)
                self.assertEqual(set(plan['groups']), expected)
                self.assertEqual(plan['scope'], 'targeted')
                self.base = head
        self.write('scripts/local-signer/new-production.py', 'fixture')
        with self.assertRaisesRegex(ValueError, 'scripts/local-signer/new-production.py'):
            selector.select(config, self.base, self.commit())

    def test_local_signer_deletion_and_rename(self):
        config = json.loads((HERE / 'ci-required-scope.json').read_text())
        self.write('scripts/local-signer/installer.py', 'unique production fixture')
        self.base = self.commit()
        Path('scripts/local-signer/tests').mkdir()
        self.git('mv', 'scripts/local-signer/installer.py', 'scripts/local-signer/tests/test_moved.py')
        head = self.commit()
        self.assertEqual(set(selector.select(config, self.base, head)['groups']),
                         {'baseline', 'packaging_contracts', 'macos_package_contract', 'fleet_health', 'workspace_support'})
        self.base = head
        Path('scripts/local-signer/tests/test_moved.py').unlink()
        self.assertEqual(set(selector.select(config, self.base, self.commit())['groups']),
                         {'baseline', 'packaging_contracts', 'macos_package_contract', 'fleet_health'})

    def test_local_signer_crate_keeps_reverse_consumers(self):
        config = json.loads((HERE / 'ci-required-scope.json').read_text())
        self.write('crates/oasis7_local_signer/Cargo.toml', '[package]\nname="oasis7_local_signer"\nversion="0.1.0"\n')
        self.write('crates/oasis7_local_signer/src/lib.rs', '')
        self.write('crates/consumer/Cargo.toml', '[package]\nname="consumer"\nversion="0.1.0"\n[dependencies]\noasis7_local_signer={path="../oasis7_local_signer"}\n')
        config['package_groups']['consumer'] = ['node']
        self.base = self.commit()
        self.write('crates/oasis7_local_signer/src/lib.rs', '// changed')
        plan = selector.select(config, self.base, self.commit())
        self.assertEqual(plan['scope'], 'targeted')
        self.assertTrue({'workspace_support', 'node', 'rust_baseline', 'packaging_contracts',
                         'macos_package_contract', 'fleet_health'}.issubset(plan['groups']))

    def test_local_signer_combination_and_security_overlay(self):
        config = json.loads((HERE / 'ci-required-scope.json').read_text())
        for path in ['scripts/local-signer/installer.py', 'scripts/local-signer/tests/test_fixture.py',
                     'doc/p2p/blockchain/local-file-signing-backend.runbook.md', 'scripts/security/fixture.py']:
            self.write(path, 'fixture')
        self.assertEqual(set(selector.select(config, self.base, self.commit())['groups']),
                         {'baseline', 'packaging_contracts', 'macos_package_contract', 'fleet_health',
                          'workspace_support', 'workflow_governance'})

    def test_custody_inputs_select_worker_platform_proof_and_core_consumers(self):
        config = json.loads((HERE / 'ci-required-scope.json').read_text())
        core = 'oasis7_custody_core'
        self.write(f'crates/{core}/Cargo.toml', f'[package]\nname="{core}"\nversion="0.1.0"\n')
        base = self.commit()
        graph = ({core: (f'crates/{core}', {})}, {core: set()}, {})
        cases = [(core, 'src/lib.rs'), (core, 'schema/custody.schema.json')]
        with patch.object(selector, 'cargo_graph', return_value=graph):
            for package, path in cases:
                with self.subTest(package=package, path=path):
                    self.write(f'crates/{package}/{path}', '// changed')
                    head = self.commit()
                    plan = selector.select(config, base, head)
                    self.assertEqual(plan['scope'], 'targeted')
                    self.assertEqual(set(plan['groups']),
                                     {'baseline', 'rust_baseline', 'workspace_support'})
                    self.assertEqual(set(plan['workspace_support_profiles']), {core})
                    base = head

    def test_native_custody_inputs_add_only_actual_worker_platform_profile(self):
        config = json.loads((HERE / 'ci-required-scope.json').read_text())
        backend, worker = 'oasis7_launcher_backend', 'oasis7_custody_worker'
        self.assertNotIn(worker, config['workspace_support_profile_paths'])
        for package in (backend, worker):
            self.write(f'crates/{package}/Cargo.toml',
                       f'[package]\nname="{package}"\nversion="0.1.0"\n')
        base = self.commit()
        graph = ({p: (f'crates/{p}', {}) for p in (backend, worker)},
                 {backend: set(), worker: set()}, {})
        paths = [
            'crates/oasis7_launcher_backend/src/bin/oasis7_game_launcher/hosted_identity/service.rs',
            'crates/oasis7_launcher_backend/src/bin/oasis7_game_launcher/hosted_identity/world.rs',
        ]
        with patch.object(selector, 'cargo_graph', return_value=graph):
            for path in paths:
                with self.subTest(path=path):
                    self.write(path, '// authority input only')
                    head = self.commit()
                    plan = selector.select(config, base, head)
                    self.assertEqual(plan['scope'], 'targeted')
                    self.assertNotIn(worker, plan['workspace_support_profiles'])
                    self.assertNotIn('workspace_support', plan['groups'])
                    self.assertIn(backend, plan['architecture']['packages'])
                    base = head

    @staticmethod
    def owner_setup_inputs():
        return [
            'crates/oasis7_agent_api/src/game/goal.rs',
            'crates/oasis7_agent_api/src/game/task.rs',
            'crates/oasis7_agent_api/src/game/goal_prepare.rs',
            'crates/oasis7_agent_api/src/game/goal_publish.rs',
            'crates/oasis7_agent_api/src/game/executor.rs',
            'crates/oasis7_client_api/src/world_service/entry_control.rs',
            'crates/oasis7_client_api/src/world_service/private_goal_read.rs',
            'crates/oasis7_client_api/src/world_service/owner_setup_observation.rs',
            'crates/oasis7_client_api/src/world_service_wire.rs',
            'crates/oasis7_launcher_backend/src/bin/oasis7_game_launcher/hosted_identity/entry_owner_control.rs',
            'crates/oasis7_chain_runtime/src/host/world_service_private_goal.rs',
        ]

    def test_owner_setup_inputs_select_worker_profile_with_real_package_ownership(self):
        config = json.loads((HERE / 'ci-required-scope.json').read_text())
        worker = 'oasis7_custody_worker'
        self.assertNotIn(worker, config['workspace_support_profile_paths'])
        paths = self.owner_setup_inputs()
        packages = {path.split('/')[1] for path in paths} | {worker}
        for package in packages:
            self.write(f'crates/{package}/Cargo.toml', f'[package]\nname="{package}"\nversion="0.1.0"\n')
        base = self.commit()
        graph = ({p: (f'crates/{p}', {}) for p in packages}, {p: set() for p in packages}, {})
        with patch.object(selector, 'cargo_graph', return_value=graph):
            for path in paths:
                with self.subTest(path=path):
                    self.write(path, '// fixed owner setup protocol input')
                    head = self.commit()
                    plan = selector.select(config, base, head)
                    self.assertEqual(plan['scope'], 'targeted')
                    self.assertNotIn(worker, plan['workspace_support_profiles'])
                    self.assertIn(path.split('/')[1], plan['architecture']['packages'])
                    base = head
            self.git('mv', paths[0], paths[0] + '.retired')
            head = self.commit()
            self.assertNotIn(worker, selector.select(config, base, head)['workspace_support_profiles'])
            base = head
            Path(paths[1]).unlink()
            self.assertNotIn(worker, selector.select(config, base, self.commit())['workspace_support_profiles'])

    def test_participant_sdk_and_world_inputs_add_actual_worker_platform_profile(self):
        config = json.loads((HERE / 'ci-required-scope.json').read_text())
        worker = 'oasis7_custody_worker'
        self.assertNotIn(worker, config['workspace_support_profile_paths'])
        packages = ('oasis7_client_api', 'oasis7_chain_runtime', worker)
        for package in packages:
            self.write(f'crates/{package}/Cargo.toml',
                       f'[package]\nname="{package}"\nversion="0.1.0"\n')
        base = self.commit()
        graph = ({p: (f'crates/{p}', {}) for p in packages},
                 {p: set() for p in packages}, {})
        paths = [
            'crates/oasis7_client_api/src/world_service/participant_entry.rs',
            'crates/oasis7_chain_runtime/src/host/world_service_session.rs',
        ]
        with patch.object(selector, 'cargo_graph', return_value=graph):
            for path in paths:
                with self.subTest(path=path):
                    self.write(path, '// participant canonical protocol input')
                    head = self.commit()
                    plan = selector.select(config, base, head)
                    self.assertEqual(plan['scope'], 'targeted')
                    self.assertNotIn(worker, plan['workspace_support_profiles'])
                    self.assertIn(path.split('/')[1], plan['architecture']['packages'])
                    base = head

    def test_domain_types_selects_its_own_support_profile(self):
        package = 'oasis7_domain_types'
        config = json.loads((HERE / 'ci-required-scope.json').read_text())
        manifest = f'[package]\nname="{package}"\nversion="0.1.0"\n'
        self.write(f'crates/{package}/Cargo.toml', manifest)
        self.write(f'crates/{package}/src/lib.rs', 'pub fn marker() {}\n')
        base = self.commit()
        self.write(f'crates/{package}/src/lib.rs', 'pub fn changed() {}\n')
        head = self.commit()

        plan = selector.select(config, base, head)
        self.assertEqual(plan['workspace_support_profiles'], [package])
        self.assertEqual(selector.support_partitions(plan['workspace_support_profiles']), ['other'])
        full = selector.full_plan(config, 'candidate-full', head)
        self.assertIn(package, full['workspace_support_profiles'])
        self.write('doc/readme.md', 'unrelated documentation\n')
        docs = selector.select(config, head, self.commit())
        self.assertNotIn(package, docs['workspace_support_profiles'])

    def test_profile_path_mapping_is_additive_and_does_not_bypass_full_or_ownership(self):
        config = json.loads(json.dumps(self.config))
        config['groups'].append('workspace_support')
        config['resources']['workspace_support'] = []
        profile = selector.SUPPORT_PROFILES[0]
        config['workspace_support_profile_paths'] = {profile: ['crates/core/src/lib.rs', 'unknown/input.rs']}
        self.write('crates/core/src/lib.rs', '// consumer input')
        head = self.commit()
        selected = selector.select(config, self.base, head)
        self.assertEqual(selected['workspace_support_profiles'], [profile])
        self.assertEqual(set(selector.select(config, self.base, head, additions=['workspace_support'])['workspace_support_profiles']), set(selector.SUPPORT_PROFILES))
        config['rules'].append({'match': ['crates/core/src/lib.rs'], 'groups': ['workspace_support'], 'reason': 'legacy group'})
        self.assertEqual(set(selector.select(config, self.base, head)['workspace_support_profiles']), set(selector.SUPPORT_PROFILES))
        self.write('Cargo.lock', '# shared control')
        self.assertEqual(set(selector.select(config, self.base, self.commit())['workspace_support_profiles']), set(selector.SUPPORT_PROFILES))
        self.write('unknown/input.rs', '// no owner')
        with self.assertRaises(selector.UnknownInputsError):
            selector.select(config, self.base, self.commit())

    def test_profile_path_mapping_rejects_unknown_profiles_and_malformed_paths(self):
        for mapping in [[], {'unknown-profile': ['crates/core/src/lib.rs']},
                        {selector.SUPPORT_PROFILES[0]: []},
                        *[{selector.SUPPORT_PROFILES[0]: [path]} for path in
                          ['', '/tmp/source.rs', '../source.rs', 'crates/../source.rs', 'crates\\source.rs', './source.rs', {}, None]],
                        {selector.SUPPORT_PROFILES[0]: ['source.rs', 'source.rs']}]:
            with self.subTest(mapping=mapping):
                config = json.loads(json.dumps(self.config))
                config['groups'].append('workspace_support')
                config['resources']['workspace_support'] = []
                config['workspace_support_profile_paths'] = mapping
                with self.assertRaises(ValueError):
                    selector.validate_config(config)

    def test_agent_api_and_harness_route_through_support_and_oasis7_consumers(self):
        config = json.loads((HERE / 'ci-required-scope.json').read_text())
        packages = ('oasis7_agent_api', 'oasis7_agent_harness', 'oasis7_world_service', 'oasis7_chain_runtime')
        for package in packages:
            self.write(f'crates/{package}/Cargo.toml',
                       f'[package]\nname="{package}"\nversion="0.1.0"\n')
            self.write(f'crates/{package}/src/lib.rs', '')
        base = self.commit()
        manifests = {name: (f'crates/{name}', {}) for name in packages}
        reverse = {
            'oasis7_agent_api': {'oasis7_agent_harness', 'oasis7_world_service'},
            'oasis7_agent_harness': {'oasis7_world_service'},
            'oasis7_world_service': {'oasis7_chain_runtime'}, 'oasis7_chain_runtime': set(),
        }
        graph = (manifests, reverse, {})
        expected = {'baseline', 'rust_baseline', 'workspace_support',
                    'oasis7_required', 'formal_process'}
        with patch.object(selector, 'cargo_graph', return_value=graph):
            for package in ('oasis7_agent_api', 'oasis7_agent_harness'):
                with self.subTest(package=package):
                    self.write(f'crates/{package}/src/lib.rs', '// changed')
                    plan = selector.select(config, base, self.commit())
                    self.assertEqual(plan['scope'], 'targeted')
                    expected_profiles = {'oasis7_agent_harness'}
                    if package == 'oasis7_agent_api':
                        expected_profiles.add('oasis7_agent_api')
                    self.assertEqual(set(plan['workspace_support_profiles']), expected_profiles)
                    self.assertEqual(set(plan['groups']), expected)
                    base = self.git('rev-parse', 'HEAD')

    def test_bootstrap_codegen_inputs_use_existing_cargo_owner(self):
        # These paths are already owned in the trusted baseline; no candidate
        # path rule or full-scope override is needed for first introduction.
        config = json.loads((HERE / 'ci-required-scope.json').read_text())
        self.write('crates/oasis7_agent_api/Cargo.toml',
                   '[package]\nname="oasis7_agent_api"\nversion="0.1.0"\n')
        base = self.commit()
        graph = ({'oasis7_agent_api': ('crates/oasis7_agent_api', {})},
                 {'oasis7_agent_api': set()}, {})
        inputs = (
            'src/game/read_bootstrap.openapi.json',
            'protocol_codegen/generate.py', 'protocol_codegen/profile.json', 'protocol_codegen/toolchain.lock.json',
            'protocol_codegen/test_generate.py',
            'protocol_codegen/generated/types.ts',
            'protocol_codegen/generated/schema.json',
            'protocol_codegen/generated/manifest.json',
            'protocol_conformance/package.json',
            'protocol_conformance/package-lock.json',
            'protocol_conformance/tsconfig.json',
            'protocol_conformance/consumer.ts', 'protocol_conformance/check-generated.mjs',
        )
        with patch.object(selector, 'cargo_graph', return_value=graph):
            for path in inputs:
                with self.subTest(path=path):
                    self.write('crates/oasis7_agent_api/' + path, 'fixture')
                    head = self.commit()
                    plan = selector.select(config, base, head)
                    self.assertEqual(plan['scope'], 'targeted')
                    self.assertIn('workspace_support', plan['groups'])
                    self.assertIn('oasis7_agent_api', plan['workspace_support_profiles'])
                    base = head

    def test_bootstrap_handler_and_fixtures_select_ts_conformance(self):
        config = json.loads((HERE / 'ci-required-scope.json').read_text())
        self.write('crates/oasis7_viewer_runtime/Cargo.toml',
                   '[package]\nname="oasis7_viewer_runtime"\nversion="0.1.0"\n')
        base = self.commit()
        graph = ({'oasis7_viewer_runtime': ('crates/oasis7_viewer_runtime', {})},
                 {'oasis7_viewer_runtime': set()}, {})
        with patch.object(selector, 'cargo_graph', return_value=graph):
            for path in ('src/game_read_api.rs', 'src/game_read_api_tests.rs',
                         'src/game_read_api_protocol_fixtures_tests.rs',
                         'tests/fixtures/signed_read_bootstrap.json'):
                self.write('crates/oasis7_viewer_runtime/' + path, 'fixture')
                head = self.commit()
                plan = selector.select(config, base, head)
                self.assertIn('workspace_support', plan['groups'])
                self.assertIn('oasis7_agent_api', plan['workspace_support_profiles'])
                base = head

    def test_game_client_routes_through_support_and_server_consumer(self):
        config = json.loads((HERE / 'ci-required-scope.json').read_text())
        packages = ('oasis7_client_api', 'oasis7_game_client', 'oasis7_world_service', 'oasis7_chain_runtime')
        for package in packages:
            self.write(f'crates/{package}/Cargo.toml',
                       f'[package]\nname="{package}"\nversion="0.1.0"\n')
            self.write(f'crates/{package}/src/lib.rs', '')
        base = self.commit()
        manifests = {name: (f'crates/{name}', {}) for name in packages}
        graph = (manifests, {
            'oasis7_client_api': {'oasis7_game_client', 'oasis7_world_service'},
            'oasis7_game_client': {'oasis7_world_service'}, 'oasis7_world_service': {'oasis7_chain_runtime'}, 'oasis7_chain_runtime': set(),
        }, {})
        with patch.object(selector, 'cargo_graph', return_value=graph):
            for package in ('oasis7_client_api', 'oasis7_game_client'):
                with self.subTest(package=package):
                    self.write(f'crates/{package}/src/lib.rs', '// changed')
                    plan = selector.select(config, base, self.commit())
                    self.assertEqual(plan['scope'], 'targeted')
                    expected_profiles = {'oasis7_game_client'}
                    if package == 'oasis7_client_api':
                        expected_profiles.add('oasis7_client_api')
                    self.assertEqual(set(plan['workspace_support_profiles']), expected_profiles)
                    self.assertEqual(set(plan['groups']), {'baseline', 'rust_baseline',
                                     'workspace_support', 'oasis7_required', 'formal_process'})
                    base = self.git('rev-parse', 'HEAD')

    def test_provider_tests_are_local_and_library_changes_reach_server_consumers(self):
        config = json.loads((HERE / 'ci-required-scope.json').read_text())
        packages = ('oasis7_agent_harness', 'oasis7_model_provider', 'oasis7_world_service', 'oasis7_chain_runtime')
        for package in packages:
            self.write(f'crates/{package}/Cargo.toml',
                       f'[package]\nname="{package}"\nversion="0.1.0"\n')
            self.write(f'crates/{package}/src/lib.rs', '')
            self.write(f'crates/{package}/tests/transport.rs', '')
        base = self.commit()
        manifests = {name: (f'crates/{name}', {}) for name in packages}
        graph = (manifests, {
            'oasis7_agent_harness': {'oasis7_model_provider', 'oasis7_world_service'},
            'oasis7_model_provider': {'oasis7_world_service'}, 'oasis7_world_service': {'oasis7_chain_runtime'}, 'oasis7_chain_runtime': set(),
        }, {})
        with patch.object(selector, 'cargo_graph', return_value=graph):
            for package, path in [('oasis7_model_provider', 'tests/transport.rs'),
                                  ('oasis7_agent_harness', 'src/lib.rs')]:
                with self.subTest(package=package):
                    self.write(f'crates/{package}/{path}', '// changed')
                    plan = selector.select(config, base, self.commit())
                    self.assertEqual(plan['scope'], 'targeted')
                    expected_profiles = {'oasis7_model_provider'}
                    if package == 'oasis7_agent_harness':
                        expected_profiles.add('oasis7_agent_harness')
                    self.assertEqual(set(plan['workspace_support_profiles']), expected_profiles)
                    expected = {'baseline', 'rust_baseline', 'workspace_support'}
                    if package == 'oasis7_agent_harness':
                        expected.update({'oasis7_required', 'formal_process'})
                    self.assertEqual(set(plan['groups']), expected)
                    base = self.git('rev-parse', 'HEAD')

    def test_deleted_agent_harness_source_uses_base_consumer_graph(self):
        config = json.loads((HERE / 'ci-required-scope.json').read_text())
        packages = ('oasis7_agent_api', 'oasis7_agent_harness', 'oasis7_world_service', 'oasis7_chain_runtime')
        for package in packages:
            self.write(f'crates/{package}/Cargo.toml',
                       f'[package]\nname="{package}"\nversion="0.1.0"\n')
            self.write(f'crates/{package}/src/lib.rs', '')
        base = self.commit()
        old_manifests = {name: (f'crates/{name}', {}) for name in packages}
        old_reverse = {
            'oasis7_agent_api': {'oasis7_agent_harness', 'oasis7_world_service'},
            'oasis7_agent_harness': {'oasis7_world_service'},
            'oasis7_world_service': {'oasis7_chain_runtime'}, 'oasis7_chain_runtime': set(),
        }
        new_manifests = {'oasis7_world_service': ('crates/oasis7_world_service', {}), 'oasis7_chain_runtime': ('crates/oasis7_chain_runtime', {})}
        new_reverse = {'oasis7_world_service': {'oasis7_chain_runtime'}, 'oasis7_chain_runtime': set()}
        Path('crates/oasis7_agent_harness/src/lib.rs').unlink()
        head = self.commit()

        def graph_for(ref, generated_inputs=(), *, source_cache=None):
            return ((old_manifests, old_reverse, {}) if ref == base
                    else (new_manifests, new_reverse, {}))

        with patch.object(selector, 'cargo_graph', side_effect=graph_for):
            plan = selector.select(config, base, head)
        self.assertEqual(plan['scope'], 'targeted')
        self.assertEqual(set(plan['groups']), {'baseline', 'rust_baseline', 'workspace_support',
                                               'oasis7_required', 'formal_process'})


if __name__ == '__main__':
    unittest.main()
