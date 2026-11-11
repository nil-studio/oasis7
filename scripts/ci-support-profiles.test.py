#!/usr/bin/env python3
"""Execute the real profile dispatcher with recording Cargo functions."""
import json
import os
import re
import shlex
import shutil
import signal
import subprocess
import sys
import tempfile
import time
import tomllib
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DRIVER = (ROOT / 'scripts/ci-tests.sh').read_text()
DRIVER_LIBRARY = '# ci-driver-library: functions-v1' in DRIVER.splitlines()


def support_driver_definitions():
    if DRIVER_LIBRARY:
        return 'source "$driver_dir/ci-tests.sh"\n'
    # Historical drivers have no source interface. Preserve their existing
    # fixture path; a missing dependency remains a failure, never a skipped test.
    worker = re.search(r'^run_worker_wasm_compile_profiles\(\) \{\n.*?^\}', DRIVER, re.M | re.S)
    protocol = DRIVER.split('run_signed_read_protocol_contract_tests() {', 1)[1].split('\n}', 1)[0]
    selected = DRIVER.split('run_selected_workspace_support() {', 1)[1].split(
        '\nrun_rustsec_advisory_check()', 1)[0]
    return ((worker.group() + '\n' if worker else '') + 'run_signed_read_protocol_contract_tests() {'
            + protocol + '\n}\nrun_selected_workspace_support() {' + selected + '\n')


def production_workspace_closure(package, *, wasm):
    """Inspect local normal dependencies, including optional production edges.

    Keep this conservative about features: an optional native client in a portable
    target is still a boundary violation. Dev/build dependencies are not linked
    into the application's production target.
    """
    target_matches = {
        'cfg(not(target_arch = "wasm32"))': not wasm,
        'cfg(target_arch = "wasm32")': wasm,
        'cfg(unix)': not wasm,
        'cfg(windows)': False,
    }
    workspace = tomllib.loads((ROOT / 'Cargo.toml').read_text())['workspace']
    pending = [ROOT / 'crates' / package / 'Cargo.toml']
    seen = set()
    packages = set()
    while pending:
        path = pending.pop().resolve()
        if path in seen:
            continue
        seen.add(path)
        manifest = tomllib.loads(path.read_text())
        packages.add(manifest['package']['name'])
        dependencies = list(manifest.get('dependencies', {}).items())
        for target, table in manifest.get('target', {}).items():
            if not table.get('dependencies'):
                continue
            # New target expressions need an explicit classification; never
            # silently skip an unfamiliar condition that could admit a client.
            if target not in target_matches:
                raise AssertionError(f'Unclassified production target: {path}: {target}')
            if target_matches[target]:
                dependencies.extend(table['dependencies'].items())
        for name, dependency in dependencies:
            dependency_root = path.parent
            if isinstance(dependency, dict) and dependency.get('workspace'):
                dependency = workspace['dependencies'][name]
                dependency_root = ROOT
            if isinstance(dependency, dict):
                packages.add(dependency.get('package', name))
                if 'path' in dependency:
                    pending.append(dependency_root / dependency['path'] / 'Cargo.toml')
            else:
                packages.add(name)
    return packages


class Profiles(unittest.TestCase):
    def test_custody_node_files_are_bounded_isolated_complete_and_propagate_failure(self):
        command = json.loads((ROOT / 'crates/oasis7_custody_worker/package.json').read_text())['scripts']['test']
        checker, runner = command.split(' && ')
        self.assertTrue(checker.startswith('tsc --noEmit --strict '))
        arguments = shlex.split(runner)
        self.assertEqual(arguments, ['node', '--test', '--test-concurrency=2', 'tests/*.test.mjs'])
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / 'tests').mkdir()
            source = '''import {test, after} from 'node:test';
import assert from 'node:assert/strict';
import {appendFileSync} from 'node:fs';
const record = kind => appendFileSync(process.env.EVENTS, JSON.stringify({kind,pid:process.pid})+'\\n');
record('start');
let active=0, completed=0;
for (const name of ['first', 'second']) test(name, async () => {
  assert.equal(active,0,'same-file cases must remain serial');
  active++;
  await new Promise(resolve=>setTimeout(resolve,60));
  active--; completed++;
  assert.notEqual(process.env.FAIL_FILE,import.meta.url.split('/').pop());
});
after(()=>{assert.equal(completed,2);record('finish');});
'''
            for index in range(3):
                (root / 'tests' / f'{index}.test.mjs').write_text(source)
            files = sorted(str(path.relative_to(root)) for path in (root / 'tests').glob('*.test.mjs'))
            for fail_file in ('', '1.test.mjs'):
                events = root / 'events.jsonl'
                events.unlink(missing_ok=True)
                result = subprocess.run([*arguments[:-1], '--test-reporter=tap', *files], cwd=root,
                    env=dict(os.environ, EVENTS=str(events), FAIL_FILE=fail_file), text=True, capture_output=True)
                self.assertEqual(result.returncode, 1 if fail_file else 0, result.stdout + result.stderr)
                self.assertIn('# tests 6', result.stdout)
                active, identities = set(), set()
                for line in events.read_text().splitlines():
                    event = json.loads(line)
                    if event['kind'] == 'start':
                        self.assertNotIn(event['pid'], identities)
                        identities.add(event['pid']); active.add(event['pid'])
                        self.assertLessEqual(len(active), 2)
                    else:
                        active.remove(event['pid'])
                self.assertEqual(len(identities), 3)
                self.assertEqual(active, set())

    def test_support_partitions_preserve_complete_profiles_and_reject_missing_owners(self):
        selected = dict(scope='targeted', groups=['workspace_support'],
                        workspace_support_profile_version=1,
                        workspace_support_profiles=['oasis7_custody_worker', 'oasis7_context'],
                        workspace_support_partition_version=1,
                        workspace_support_partitions=['custody', 'other'])
        output = []
        for partition in ('custody', 'other'):
            result = self.invoke('plan:' + json.dumps(selected), environment={'support_partition': partition})
            self.assertEqual(result.returncode, 0, result.stderr)
            profiles = [line for line in result.stdout.splitlines() if line.startswith('+ support profile: ')]
            self.assertEqual(len(profiles), 1)
            output.extend(profiles)
        self.assertCountEqual(output, ['+ support profile: oasis7_custody_worker', '+ support profile: oasis7_context'])
        for change in ({'workspace_support_partition_version': True}, {'workspace_support_partitions': ['custody']},
                       {'workspace_support_partitions': ['custody', 'custody']}, {'workspace_support_partitions': ['all']}):
            with self.subTest(change=change):
                result = self.invoke('plan:' + json.dumps(selected | change), environment={'support_partition': 'custody'})
                self.assertNotEqual(result.returncode, 0)
                self.assertNotIn('+ support profile: ', result.stdout)
        result = self.invoke('plan:' + json.dumps(selected), fail=True, environment={'support_partition': 'other'})
        self.assertEqual(result.returncode, 17)
        legacy = {key: value for key, value in selected.items() if not key.startswith('workspace_support_partition')}
        result = self.invoke('plan:' + json.dumps(legacy))
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout.count('+ support profile: '), 2)

    def test_unpartitioned_support_plan_runs_all_profiles_and_keeps_validation(self):
        for profiles, partitions in ((['oasis7_context'], ['other']),
                                     (['oasis7_custody_worker', 'oasis7_context'], ['custody', 'other'])):
            selected = dict(scope='targeted', groups=['workspace_support'],
                            workspace_support_profile_version=1, workspace_support_profiles=profiles,
                            workspace_support_partition_version=1, workspace_support_partitions=partitions)
            for environment in ({}, {'support_partition': 'all'}):
                with self.subTest(profiles=profiles, environment=environment):
                    result = self.invoke('plan:' + json.dumps(selected), environment=environment)
                    self.assertEqual(result.returncode, 0, result.stderr)
                    self.assertCountEqual([line for line in result.stdout.splitlines()
                                           if line.startswith('+ support profile: ')],
                                          ['+ support profile: ' + profile for profile in profiles])
                    result = self.invoke('plan:' + json.dumps(selected), fail=True, environment=environment)
                    self.assertEqual(result.returncode, 17, result.stderr)
            for partitions in ([], ['all'], ['custody', 'custody'], ['unknown']):
                result = self.invoke('plan:' + json.dumps(selected | {'workspace_support_partitions': partitions}))
                self.assertNotEqual(result.returncode, 0)
                self.assertNotIn('+ support profile: ', result.stdout)
            result = self.invoke('plan:' + json.dumps(selected), environment={'support_partition': 'unknown'})
            self.assertNotEqual(result.returncode, 0)
            self.assertNotIn('+ support profile: ', result.stdout)

    def test_driver_library_preserves_shell_context_without_running_cli(self):
        script = '''set +e
set +u
set +o pipefail
before_options=$(set +o)
before_directory=$PWD
before_arguments="$*"
source "$1"
[[ $(set +o) == "$before_options" ]] || exit 71
[[ $PWD == "$before_directory" ]] || exit 72
[[ $* == "$before_arguments" ]] || exit 73
[[ -z ${driver_dir+x} && -z ${repo_root+x} && -z ${ci_python+x} && -z ${tier+x} ]] || exit 74
declare -F run_selected_workspace_support >/dev/null || exit 75
declare -F run_worker_wasm_compile_profiles >/dev/null || exit 76
echo 'loaded definitions only'
'''
        with tempfile.TemporaryDirectory() as directory:
            # A definitions-only source must not even discover an interpreter,
            # run preflight, or invoke dirname/cd in the CLI initialization.
            result = subprocess.run([shutil.which('bash'), '-c', script, 'library-test',
                                     str(ROOT / 'scripts/ci-tests.sh'), 'untouched-argument'],
                                    cwd=directory, text=True, capture_output=True,
                                    env=dict(os.environ, PATH=str(Path(directory) / 'missing-bin')))
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout, 'loaded definitions only\n')

    def test_driver_cli_argument_errors_keep_their_exit_codes(self):
        cases = [((), 2), (('unknown-tier',), 1),
                 (('required', '--unknown-option'), 1),
                 (('required', '--group', 'unknown-group'), 2)]
        for arguments, expected in cases:
            with self.subTest(arguments=arguments):
                result = subprocess.run(['bash', str(ROOT / 'scripts/ci-tests.sh'), *arguments],
                                        text=True, capture_output=True)
                self.assertEqual(result.returncode, expected, result.stdout + result.stderr)

    def test_owner_setup_protocol_inputs_retain_actual_custody_world_coverage(self):
        config = json.loads((ROOT / 'scripts/ci-required-scope.json').read_text())
        paths = set(config['workspace_support_profile_paths']['oasis7_custody_worker'])
        required = [
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
        for path in required:
            with self.subTest(path=path):
                self.assertIn(path, paths)

    def test_participant_protocol_inputs_retain_actual_custody_world_coverage(self):
        config = json.loads((ROOT / 'scripts/ci-required-scope.json').read_text())
        paths = set(config['workspace_support_profile_paths']['oasis7_custody_worker'])
        required = [
            'crates/oasis7_launcher_backend/src/bin/oasis7_game_launcher/hosted_identity/participant_entry.rs',
            'crates/oasis7_launcher_backend/src/bin/oasis7_game_launcher/hosted_identity/participant_entry_tests.rs',
            'crates/oasis7_client_api/src/world_service.rs',
            'crates/oasis7_client_api/src/world_service/authority.rs',
            'crates/oasis7_client_api/src/world_service/private_goal.rs',
            'crates/oasis7_client_api/src/world_service/participant_discovery.rs',
            'crates/oasis7_client_api/src/world_service/participant_policy_original.rs',
            'crates/oasis7_client_api/src/world_service_correlation.rs',
            'crates/oasis7_chain_runtime/src/host/world_service_participant_api.rs',
            'crates/oasis7_chain_runtime/src/host/world_service_participant_owner_control.rs',
            'crates/oasis7_chain_runtime/src/host/execution_bridge/tests/participant_http.rs',
            'crates/oasis7_chain_runtime/src/host/execution_bridge/tests/participant_http_fixture.rs',
            'crates/oasis7_chain_runtime/src/host/execution_bridge/tests/participant_governance.rs',
            'crates/oasis7_chain_runtime/src/host/execution_bridge/tests/participant_transport.rs',
        ]
        for path in required:
            with self.subTest(path=path):
                self.assertIn(path, paths)

    def test_native_custody_protocol_inputs_retain_real_platform_coverage(self):
        config = json.loads((ROOT / 'scripts/ci-required-scope.json').read_text())
        paths = set(config['workspace_support_profile_paths']['oasis7_custody_worker'])
        prefix = 'crates/oasis7_launcher_backend/src/bin/oasis7_game_launcher/hosted_identity/'
        for name in ('custody_managed_binding.rs', 'custody_managed_binding_tests.rs',
                     'custody_owner_pin.rs', 'custody_owner_pin_tests.rs',
                     'native_managed_operation.rs', 'native_managed_operation_tests.rs',
                     'native_key.rs', 'native_key_complete.rs', 'native_key_tests.rs',
                     'machine_registration.rs', 'machine_registration_tests.rs'):
            with self.subTest(path=name):
                self.assertIn(prefix + name, paths)

    def invoke(self, profiles, fail=False, fail_protocol=False, fail_expanded=False, cwd=None,
               real_cargo=False, real_worker_compile=False, environment=None,
               fail_local_fixture=False):
        script = '''set -euo pipefail
ci_python="$1"; driver_dir="$2"; support_profiles="$3"
'''
        script += support_driver_definitions()
        script += '''
run() { echo "contract $*"; }
run_cargo() { echo "cargo $*"; }
run_cargo_clippy() { echo "clippy $*"; }
run_oasis7_workspace_support_crate_tests() { echo full-inventory; }
'''
        if fail_protocol:
            script += 'run() { echo "contract $*"; return 19; }\n'
        if fail_local_fixture:
            script += 'run() { echo "contract $*"; [[ "$1" != sudo ]] || return 41; }\n'
        if fail:
            script += 'run_cargo() { return 17; }\n'
        if fail_expanded:
            script += 'run_cargo() { echo "cargo $*"; [[ "$*" != *--features* ]] || return 23; }\n'
        if real_cargo:
            script += 'run_cargo() { echo "cargo $*"; env -u RUSTC_WRAPPER cargo "$@"; }\n'
            script += 'run_cargo_clippy() { env -u RUSTC_WRAPPER cargo clippy "$@" -- -D warnings; }\n'
        if real_worker_compile:
            script += 'run() { echo "contract $*"; if [[ "$1" == "$ci_python" && "${2:-}" == - ]]; then "$@"; fi; }\n'
        script += '\nrun_selected_workspace_support\n'
        return subprocess.run(['bash', '-c', script, 'test', sys.executable,
                               str(ROOT / 'scripts'), profiles],
                              text=True, capture_output=True, cwd=cwd, env=environment)

    def test_new_support_tier_executes_before_merge_and_propagates_failure(self):
        plan = dict(fast_plan_version=1, execution_profile='pr-fast', scope='targeted',
                    groups=['workspace_support'], workspace_support_profile_version=1,
                    workspace_support_profiles=['oasis7_agent_harness'], test_policy_changed=True)
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / 'Cargo.toml').write_text('[workspace]\nmembers=["harness"]\n')
            package = root / 'harness'
            package.mkdir()
            (package / 'Cargo.toml').write_text('[package]\nname="oasis7_agent_harness"\n'
                                              '[features]\ntest_tier_required=[]\n'
                                              'test_tier_full=["test_tier_required"]\n')
            for fail in (False, True):
                result = self.invoke('plan:' + json.dumps(plan), fail_expanded=fail, cwd=root)
                self.assertEqual(result.returncode, 23 if fail else 0, result.stderr)
                self.assertIn('cargo test -p oasis7_agent_harness --lib', result.stdout)
                self.assertIn('cargo test -p oasis7_agent_harness --all-targets --features oasis7_agent_harness/test_tier_required,oasis7_agent_harness/test_tier_full', result.stdout)
                self.assertEqual('support profile completed' in result.stdout, not fail)

    @unittest.skipUnless(shutil.which('cargo') and shutil.which('rustc'), 'requires a Rust toolchain')
    def test_changed_support_policy_runs_no_feature_integration_before_completion(self):
        plan = dict(fast_plan_version=1, execution_profile='pr-fast', scope='targeted',
                    groups=['workspace_support'], workspace_support_profile_version=1,
                    workspace_support_profiles=['oasis7_agent_harness'])
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / 'Cargo.toml').write_text('[workspace]\nresolver="2"\nmembers=["harness"]\n')
            member = root / 'harness'
            (member / 'src').mkdir(parents=True)
            (member / 'tests').mkdir()
            (member / 'Cargo.toml').write_text('[package]\nname="oasis7_agent_harness"\n'
                                               'version="0.1.0"\nedition="2021"\n')
            (member / 'src/lib.rs').write_text('#[test] fn original_library_case() {}\n')
            integration = member / 'tests/integration.rs'
            integration.write_text('#[test] fn no_feature_integration() { panic!("regression"); }\n')
            # An unchanged fast plan and an old trusted plan retain their
            # original --lib profile; the new policy flag is not self-implied.
            legacy = {key: value for key, value in plan.items() if key != 'fast_plan_version'}
            legacy['test_policy_changed'] = True
            for ordinary in (plan, legacy):
                result = self.invoke('plan:' + json.dumps(ordinary), cwd=root, real_cargo=True)
                self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                self.assertIn('original_library_case ... ok', result.stdout)
                self.assertNotIn('no_feature_integration ...', result.stdout)
            plan['test_policy_changed'] = True
            for fail in (True, False):
                integration.write_text('#[test] fn no_feature_integration() { '
                                       + ('panic!("regression");' if fail else '') + ' }\n')
                result = self.invoke('plan:' + json.dumps(plan), cwd=root, real_cargo=True)
                self.assertIn('original_library_case ... ok', result.stdout)
                self.assertNotIn('--features', result.stdout)
                if fail:
                    self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
                    self.assertIn('no_feature_integration ... FAILED', result.stdout)
                else:
                    self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                    self.assertIn('no_feature_integration ... ok', result.stdout)
                self.assertEqual('support profile completed' in result.stdout, not fail)

    def test_changed_policy_reuses_completed_all_targets_only_without_feature_expansion(self):
        plan = dict(fast_plan_version=1, execution_profile='pr-fast', scope='targeted',
                    groups=['workspace_support'], workspace_support_profile_version=1,
                    workspace_support_profiles=['oasis7_context'], test_policy_changed=True)
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / 'Cargo.toml').write_text('[workspace]\nmembers=["context"]\n')
            member = root / 'context'
            member.mkdir()
            for feature_section in ('', '[features]\ntest_tier_required=[]\ntest_tier_full=[]\n'):
                (member / 'Cargo.toml').write_text('[package]\nname="oasis7_context"\n' + feature_section)
                result = self.invoke('plan:' + json.dumps(plan), cwd=root)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertIn('cargo test -p oasis7_context --all-targets --locked', result.stdout)
                self.assertEqual(result.stdout.count('cargo test -p oasis7_context '),
                                 2 if feature_section else 1)

    @unittest.skipUnless(shutil.which('cargo') and shutil.which('rustc'), 'requires a Rust toolchain')
    def test_full_support_runs_featured_and_no_feature_integration_cases(self):
        import importlib.util
        spec = importlib.util.spec_from_file_location('full_support_scope', ROOT / 'scripts/plan-rust-required-scope.py')
        scope = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(scope)
        script = 'set -euo pipefail\nci_python="$1"; driver_dir="$2"\nexport OASIS7_WASM_BUILD_STD=1\n'
        if DRIVER_LIBRARY:
            script += 'source "$driver_dir/ci-tests.sh"\n'
        else:
            body = DRIVER.split('run_workspace_support_full_feature_tests() {', 1)[1].split('\n}', 1)[0]
            script += 'run_workspace_support_full_feature_tests() {' + body + '\n}\n'
        script += 'run_cargo() { echo "cargo $*"; env -u RUSTC_WRAPPER cargo "$@"; }\n'
        script += 'run_workspace_support_full_feature_tests\n'
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / 'Cargo.toml').write_text('[workspace]\nresolver="2"\nmembers=' +
                                            json.dumps(list(scope.SUPPORT_PROFILES)) + '\n')
            for package in scope.SUPPORT_PROFILES:
                member = root / package
                (member / 'src').mkdir(parents=True)
                manifest = '[package]\nname="' + package + '"\nversion="0.1.0"\nedition="2021"\n'
                if package == 'oasis7_agent_harness':
                    manifest += '[features]\ntest_tier_required=[]\ntest_tier_full=[]\nwasmtime=[]\nviewer_live_integration=[]\n'
                (member / 'Cargo.toml').write_text(manifest)
                (member / 'src/lib.rs').write_text('#[cfg(feature="test_tier_required")] #[test] fn required_case() {}\n'
                                                  if package == 'oasis7_agent_harness' else '')
                if package in ('wasm_build_suite', 'wasm_module_observe'):
                    (member / 'src/lib.rs').write_text('#[test] fn uses_installed_wasm_target() { '
                                                      'assert_eq!(std::env::var("OASIS7_WASM_BUILD_STD").as_deref(), Ok("0")); }\n')
            integration = root / 'oasis7_agent_harness/tests/integration.rs'
            integration.parent.mkdir()
            plain_integration = root / 'oasis7_client_storage/tests/integration.rs'
            plain_integration.parent.mkdir()
            for failure in ('no_features', None, 'features'):
                integration.write_text('#[cfg(feature="test_tier_full")] #[test] fn expanded_integration() { '
                                       'assert!(cfg!(feature="wasmtime") && cfg!(feature="viewer_live_integration")); '
                                       + ('panic!("expanded regression");' if failure == 'features' else '') + ' }\n')
                plain_integration.write_text('#[test] fn no_feature_integration() { '
                                             + ('panic!("plain regression");' if failure == 'no_features' else '') + ' }\n')
                result = subprocess.run(['bash', '-c', script, 'full-support', sys.executable, str(ROOT / 'scripts')],
                                        cwd=root, text=True, capture_output=True)
                if failure:
                    self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
                    case = 'no_feature_integration' if failure == 'no_features' else 'expanded_integration'
                    self.assertIn(case + ' ... FAILED', result.stdout)
                else:
                    self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                    self.assertIn('required_case ... ok', result.stdout)
                    self.assertIn('expanded_integration ... ok', result.stdout)
                    self.assertIn('no_feature_integration ... ok', result.stdout)
                    self.assertIn('cargo test -p oasis7_client_storage --all-targets\n', result.stdout)
                    self.assertEqual(result.stdout.count('uses_installed_wasm_target ... ok'), 2)
                    self.assertEqual(result.stdout.count('+ complete support features:'), len(scope.SUPPORT_PROFILES))
        full = DRIVER.split('run_full_extended_tier_tests() {', 1)[1].split('\n}', 1)[0]
        self.assertIn('run_workspace_support_full_feature_tests', full)

    def test_custody_worker_runs_platform_proof_in_isolation(self):
        result = self.invoke(json.dumps(['oasis7_custody_worker']))
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('contract bash crates/oasis7_custody_worker/scripts/verify.sh --platform', result.stdout)
        self.assertNotIn('cargo ', result.stdout)
        self.assertNotIn('full-inventory', result.stdout)

    def test_selected_worker_profile_runs_real_compile_helper_and_propagates_failure(self):
        profiles = ('identity', 'owner', 'executor', 'recovery-fence',
                    'owner,approval-harness', 'identity,native-runtime-harness')
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            manifest = root / 'crates/oasis7_custody_worker/Cargo.toml'
            manifest.parent.mkdir(parents=True)
            names = sorted({name for profile in profiles for name in profile.split(',')})
            # Closed command-capture fixture, not actual Worker compile evidence.
            manifest.write_text('[features]\ndefault = []\n'
                                + ''.join(name + ' = []\n' for name in names))
            binary = root / 'bin'
            binary.mkdir()
            capture = root / 'worker-compile-argv'
            cargo = binary / 'cargo'
            cargo.write_text('#!/bin/sh\nprintf \'%s\\n\' "$*" >> "$WORKER_COMPILE_CAPTURE"\nexit "$WORKER_COMPILE_RESULT"\n')
            cargo.chmod(0o755)
            environment = dict(os.environ, PATH=str(binary) + os.pathsep + os.environ['PATH'],
                               WORKER_COMPILE_CAPTURE=str(capture), WORKER_COMPILE_RESULT='0')
            result = self.invoke('["oasis7_custody_worker"]', real_worker_compile=True,
                                 cwd=root, environment=environment)
            self.assertEqual(result.returncode, 0, result.stderr)
            expected = ['check -p oasis7_custody_worker --target wasm32-unknown-unknown '
                        '--no-default-features --features ' + profile + ' --locked'
                        for profile in profiles]
            self.assertEqual(capture.read_text().splitlines(), expected)
            self.assertIn('support profile completed: oasis7_custody_worker', result.stdout)
            capture.unlink()
            environment['WORKER_COMPILE_RESULT'] = '42'
            failed = self.invoke('["oasis7_custody_worker"]', real_worker_compile=True,
                                 cwd=root, environment=environment)
            self.assertNotEqual(failed.returncode, 0)
            self.assertIn('exit status 42', failed.stderr)
            self.assertNotIn('support profile completed', failed.stdout)
            self.assertEqual(capture.read_text().splitlines(), expected[:1])

    def test_custody_core_retains_all_target_schema_tests(self):
        result = self.invoke(json.dumps(['oasis7_custody_core']))
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('cargo test -p oasis7_custody_core --all-targets --locked', result.stdout)
        self.assertNotIn('verify.sh', result.stdout)

    def test_custody_platform_failure_stops_profile_completion(self):
        result = self.invoke(json.dumps(['oasis7_custody_worker']), fail_protocol=True)
        self.assertEqual(result.returncode, 19, result.stderr)
        self.assertNotIn('support profile completed', result.stdout)

    def test_harness_isolated_and_strict(self):
        result = self.invoke(json.dumps(['oasis7_agent_harness']))
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('clippy -p oasis7_agent_harness --all-targets', result.stdout)
        self.assertIn('cargo test -p oasis7_agent_harness --lib', result.stdout)
        self.assertNotIn('oasis7_local_signer', result.stdout)
        self.assertNotIn('full-inventory', result.stdout)

    def test_application_keeps_native_sdk_out_of_wasm(self):
        manifest = tomllib.loads((ROOT / "crates/oasis7/Cargo.toml").read_text())
        native = manifest["target"]['cfg(not(target_arch = "wasm32"))']["dependencies"]
        # The SDK is owned by World Service; the core reaches it only through
        # native host services, rather than retaining an unused direct edge.
        self.assertNotIn("oasis7_game_client", manifest["dependencies"])
        self.assertNotIn("oasis7_game_client", native)
        self.assertIn("oasis7_world_service", native)
        world_service = tomllib.loads(
            (ROOT / "crates/oasis7_world_service/Cargo.toml").read_text())
        self.assertIn("oasis7_game_client", world_service["dependencies"])
        self.assertIn("oasis7_game_client",
                      production_workspace_closure('oasis7', wasm=False))
        for root in ('oasis7', 'oasis7_viewer_runtime'):
            with self.subTest(root=root):
                closure = production_workspace_closure(root, wasm=True)
                for package in ('oasis7_world_service', 'oasis7_game_client',
                                'oasis7_model_provider'):
                    self.assertNotIn(package, closure)

    def test_game_client_isolated_and_strict(self):
        result = self.invoke(json.dumps(['oasis7_game_client']))
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('clippy -p oasis7_game_client --all-targets', result.stdout)
        self.assertIn('cargo test -p oasis7_game_client --lib', result.stdout)
        self.assertIn('cargo test -p oasis7_game_client --test dev_test_service_cli '
                      '--test dev_test_local_transport --test dev_test_local_cli --locked', result.stdout)
        self.assertNotIn('cargo test -p oasis7 ', result.stdout)
        self.assertNotIn('full-inventory', result.stdout)

    def test_game_client_hosted_cross_uid_failure_prevents_completion(self):
        environment = dict(os.environ, GITHUB_ACTIONS='true', RUNNER_OS='Linux',
                           RUNNER_ENVIRONMENT='github-hosted')
        for fail in (False, True):
            with self.subTest(fail=fail):
                result = self.invoke('["oasis7_game_client"]', environment=environment,
                                     fail_local_fixture=fail)
                self.assertEqual(result.returncode, 41 if fail else 0, result.stderr)
                self.assertIn('cargo build -p oasis7_game_client --bin oasis7_game_client --locked',
                              result.stdout)
                self.assertIn('sudo -n timeout --signal=TERM --kill-after=5s 60s ' + sys.executable
                              + ' crates/oasis7_game_client/tests/fixtures/dev_test_local_linux.py '
                              'target/debug/oasis7_game_client', result.stdout)
                self.assertEqual('support profile completed' in result.stdout, not fail)
                self.assertNotIn('NOT EXECUTED', result.stdout)
        unrelated = self.invoke('["oasis7_agent_harness"]', environment=environment)
        self.assertEqual(unrelated.returncode, 0, unrelated.stderr)
        self.assertNotIn('sudo', unrelated.stdout)

    def test_game_client_ci_cannot_silently_skip_missing_isolation(self):
        for platform, runner in (('', 'github-hosted'), ('macOS', 'github-hosted'),
                                 ('Linux', ''), ('Linux', 'self-hosted')):
            with self.subTest(platform=platform, runner=runner):
                environment = dict(os.environ, GITHUB_ACTIONS='true', RUNNER_OS=platform,
                                   RUNNER_ENVIRONMENT=runner)
                result = self.invoke('["oasis7_game_client"]', environment=environment)
                self.assertEqual(result.returncode, 2, result.stderr)
                self.assertNotIn('support profile completed', result.stdout)
                self.assertNotIn('sudo', result.stdout)
                self.assertNotIn('NOT EXECUTED', result.stdout)
        local = self.invoke('["oasis7_game_client"]', environment=dict(os.environ, GITHUB_ACTIONS=''))
        self.assertEqual(local.returncode, 0, local.stderr)
        self.assertIn('cross-UID fixture NOT EXECUTED locally', local.stderr)
        self.assertNotIn('sudo', local.stdout)

    def test_model_provider_runs_integration_tests_and_strict_clippy(self):
        result = self.invoke(json.dumps(['oasis7_model_provider']))
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('clippy -p oasis7_model_provider --all-targets', result.stdout)
        self.assertIn('cargo test -p oasis7_model_provider --all-targets', result.stdout)
        self.assertNotIn('cargo test -p oasis7_model_provider --lib', result.stdout)
        self.assertNotIn('cargo test -p oasis7 ', result.stdout)
        self.assertNotIn('full-inventory', result.stdout)

    def test_main_portable_harness_keeps_native_clients_out_of_wasm(self):
        manifest = tomllib.loads((ROOT / 'crates/oasis7/Cargo.toml').read_text())
        native = manifest['target']['cfg(not(target_arch = "wasm32"))']['dependencies']
        self.assertIn('oasis7_agent_harness', manifest['dependencies'])
        self.assertNotIn('oasis7_agent_harness', native)
        self.assertNotIn('oasis7_model_provider', manifest['dependencies'])
        self.assertIn('oasis7_model_provider', native)
        harness = tomllib.loads((ROOT / 'crates/oasis7_agent_harness/Cargo.toml').read_text())
        native_harness = harness['target']['cfg(not(target_arch = "wasm32"))']['dependencies']
        for package in ('oasis7_game_client', 'oasis7_client_api'):
            self.assertNotIn(package, harness['dependencies'])
            self.assertIn(package, native_harness)
        for root in ('oasis7', 'oasis7_agent_harness'):
            with self.subTest(root=root):
                closure = production_workspace_closure(root, wasm=True)
                self.assertIn('oasis7_agent_harness', closure)
                for package in ('oasis7_game_client', 'oasis7_model_provider'):
                    self.assertNotIn(package, closure)

    def test_invalid_lists_fail_before_execution(self):
        for value in ['[]', '["unknown"]', '["oasis7_agent_api", "oasis7_agent_api"]',
                      'null', '{}', '[1]', 'invalid']:
            with self.subTest(value=value):
                result = self.invoke(value)
                self.assertNotEqual(result.returncode, 0)
                self.assertNotIn('cargo test', result.stdout)

    def test_agent_api_runs_protocol_contract_before_cargo(self):
        result = self.invoke('["oasis7_agent_api"]')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('protocol_codegen/generate.py check', result.stdout)
        self.assertIn('unittest discover -s crates/oasis7_agent_api/protocol_codegen', result.stdout)
        self.assertIn('npm ci --prefix crates/oasis7_agent_api/protocol_conformance --ignore-scripts', result.stdout)
        self.assertIn('npm test --prefix crates/oasis7_agent_api/protocol_conformance', result.stdout)
        self.assertLess(result.stdout.index('protocol_codegen/generate.py check'),
                        result.stdout.index('clippy -p oasis7_agent_api'))
        other = self.invoke('["oasis7_agent_harness"]')
        self.assertNotIn('contract ', other.stdout)

    def test_protocol_failure_stops_profile_before_cargo(self):
        result = self.invoke('["oasis7_agent_api"]', fail_protocol=True)
        self.assertEqual(result.returncode, 19, result.stderr)
        self.assertNotIn('clippy ', result.stdout)
        self.assertNotIn('cargo test', result.stdout)
        self.assertNotIn('support profile completed', result.stdout)

    def test_failure_is_not_completed(self):
        result = self.invoke('["oasis7_agent_api"]', fail=True)
        self.assertEqual(result.returncode, 17)
        self.assertNotIn('support profile completed', result.stdout)

    def test_legacy_full_default_preserved(self):
        result = self.invoke('')
        self.assertEqual(result.returncode, 0)
        self.assertIn('full-inventory', result.stdout)

    def test_plan_omissions_and_unknown_versions_rejected(self):
        for plan in [dict(groups=['workspace_support'], scope='targeted',
                          workspace_support_profile_version=1),
                     dict(groups=['workspace_support'], workspace_support_profile_version=2)]:
            result = self.invoke('plan:' + json.dumps(plan))
            self.assertNotEqual(result.returncode, 0, result.stdout)
            self.assertNotIn('cargo test', result.stdout)

    def test_trusted_base_full_plan_expands_new_candidate_profiles(self):
        plan = dict(groups=['workspace_support'], scope='full',
                    workspace_support_profile_version=1,
                    workspace_support_profiles=['oasis7_agent_api'])
        result = self.invoke('plan:' + json.dumps(plan))
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('support profile completed: oasis7_game_client', result.stdout)
        self.assertIn('support profile completed: oasis7_model_provider', result.stdout)
        self.assertIn('support profile completed: wasm_module_observe', result.stdout)

    def test_full_inventory_preserves_packages_and_special_commands(self):
        import importlib.util
        spec = importlib.util.spec_from_file_location('scope', ROOT / 'scripts/plan-rust-required-scope.py')
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        legacy = DRIVER.split('run_oasis7_workspace_support_crate_tests() {', 1)[1].split('\n}', 1)[0]
        self.assertIn('run_cargo test -p oasis7_model_provider --all-targets', legacy)
        self.assertIn('run_cargo_clippy -p oasis7_model_provider --all-targets', legacy)
        # Independent old full entry is retained: profiles must cover every package
        # in it, not merely agree with their own inventory constant.
        self.assertEqual(set(module.SUPPORT_PROFILES),
                         set(re.findall(r'-p\s+([a-zA-Z0-9_]+)', legacy)) | {'oasis7_custody_worker'})
        self.assertIn('run bash crates/oasis7_custody_worker/scripts/verify.sh --platform', legacy)
        plan = dict(groups=['workspace_support'], scope='full',
                    workspace_support_profile_version=1,
                    workspace_support_profiles=list(module.SUPPORT_PROFILES))
        result = self.invoke('plan:' + json.dumps(plan))
        self.assertEqual(result.returncode, 0, result.stderr)
        for profile in module.SUPPORT_PROFILES:
            self.assertIn('+ support profile completed: ' + profile, result.stdout)
        for command in ['oasis7_wasm_executor --features wasmtime --lib',
                        'oasis7_context --all-targets --locked',
                        'oasis7_secure_envelope --all-targets --locked',
                        'oasis7_local_signer --test context_cli --locked',
                        'oasis7_client_launcher --bin oasis7_client_launcher']:
            self.assertIn(command, result.stdout)

    def test_old_trusted_plan_cannot_narrow_support(self):
        result = self.invoke('plan:' + json.dumps(dict(groups=['workspace_support'], scope='targeted')))
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('support profile completed: wasm_module_observe', result.stdout)


class ViewerChecks(unittest.TestCase):
    """Run the real Viewer dispatcher with two independent executable fixtures."""

    def viewer_fixture(self, directory, *, failing='', leak=False, timeout=False,
                       cancel=False, source_failure=False):
        root = Path(directory)
        (root / 'bin').mkdir()
        (root / 'scripts').mkdir()
        (root / 'dist').mkdir()
        (root / 'dist/current').write_text('stale')
        executable = f'''#!{sys.executable}
import json, os, pathlib, subprocess, sys, time
root = pathlib.Path(os.environ['VIEWER_FIXTURE_ROOT'])
role = pathlib.Path(sys.argv[0]).name
role = {{'npm': 'ui', 'build-viewer-software-safe.sh': 'build', 'node': 'smoke'}}[role]
if role == 'smoke' and len(sys.argv) > 1 and sys.argv[1].endswith('browser-functional-regression.mjs'):
    role = 'browser'
with (root / 'events.jsonl').open('a') as stream:
    stream.write(json.dumps([role, sys.argv[1:]]) + '\\n')
if role in ('smoke', 'browser'):
    expected = ('crates/oasis7_viewer/scripts/viewer-auth-browser-security-smoke.mjs'
                if role == 'smoke' else 'crates/oasis7_viewer/scripts/browser-functional-regression.mjs')
    assert sys.argv[1:] == [expected]
    assert (root / 'ui.complete').is_file() and (root / 'build.complete').is_file()
    assert (root / 'dist/current').read_text() == 'current'
    raise SystemExit(0)
assert sys.argv[1:] == (['--prefix', 'crates/oasis7_viewer', 'run', 'test:ui'] if role == 'ui' else [])
(root / (role + '.started')).write_text('started')
other = 'build' if role == 'ui' else 'ui'
deadline = time.monotonic() + 3
while not (root / (other + '.started')).is_file():
    if time.monotonic() >= deadline:
        raise SystemExit('parallel peer did not start')
    time.sleep(.01)
time.sleep(float(os.environ.get('VIEWER_HOLD_SECONDS', '0')))
if role == os.environ.get('VIEWER_FAIL_ROLE'):
    raise SystemExit(17 if role == 'ui' else 19)
if role == 'ui' and os.environ.get('VIEWER_LEAK') == '1':
    # An intentionally detached, short-lived child must still be observed as
    # a leak after both direct commands exit zero. It cannot survive the fixture.
    subprocess.Popen([sys.executable, '-c', 'import time; time.sleep(.4)'],
                     start_new_session=True)
if role == 'build':
    (root / 'dist/current').write_text('current')
(root / (role + '.complete')).write_text('complete')
'''
        for path in (root / 'bin/npm', root / 'bin/node',
                     root / 'scripts/build-viewer-software-safe.sh'):
            path.write_text(executable)
            path.chmod(0o755)
        environment = dict(os.environ, VIEWER_FIXTURE_ROOT=str(root), VIEWER_FAIL_ROLE=failing,
                           VIEWER_LEAK='1' if leak else '0',
                           VIEWER_HOLD_SECONDS='.8' if timeout or cancel else '0',
                           VIEWER_SOURCE_FAIL='1' if source_failure else '0',
                           PATH=str(root / 'bin') + os.pathsep + os.environ['PATH'])
        interpreter = root / 'bin/fixture-python'
        interpreter.write_text(f'''#!{sys.executable}
import os, pathlib, sys
pathlib.Path(os.environ['VIEWER_FIXTURE_ROOT'], 'supervisor.pid').write_text(str(os.getpid()))
sys.argv = sys.argv[1:]
if {timeout!r}:
    sys.path.insert(0, {str(ROOT / 'scripts')!r})
    import ci_process_runner
    actual_run = ci_process_runner.run
    def short_deadline(*args, **kwargs):
        assert kwargs['timeout_seconds'] > 0
        kwargs['timeout_seconds'] = .1
        return actual_run(*args, **kwargs)
    ci_process_runner.run = short_deadline
exec(compile(sys.stdin.read(), '<viewer-driver>', 'exec'), {{'__name__': '__main__'}})
''')
        interpreter.chmod(0o755)
        script = '''set -euo pipefail
source "$1/ci-tests.sh"
driver_dir="$1"; ci_python="$2"; repo_root="$3"
viewer_dependency_preflight() { :; }
run() {
  case "$1" in
    ./scripts/viewer-dependency-preflight.test.sh)
      [[ "$VIEWER_SOURCE_FAIL" != 1 ]] || return 23 ;;
    npm)
      if [[ "$*" == *test:ui ]]; then "$@"; fi ;;
    ./scripts/build-viewer-software-safe.sh) "$@" ;;
    node)
      if [[ "$*" == *viewer-auth-browser-security-smoke.mjs || "$*" == *browser-functional-regression.mjs ]]; then "$@"; fi ;;
    "$ci_python"|python3)
      if [[ "${2:-}" == - ]]; then "$@"; fi ;;
  esac
}
run_group viewer_js_required
'''
        arguments = ['bash', '-c', script, 'viewer-checks', str(ROOT / 'scripts'),
                     str(interpreter), str(root)]
        if cancel:
            process = subprocess.Popen(arguments, cwd=root, env=environment, text=True,
                                       stdout=subprocess.PIPE, stderr=subprocess.PIPE)
            try:
                deadline = time.monotonic() + 5
                while not all((root / marker).is_file() for marker in
                              ('supervisor.pid', 'ui.started', 'build.started')):
                    if process.poll() is not None or time.monotonic() >= deadline:
                        self.fail('Viewer parallel supervisor did not start both branches')
                    time.sleep(.01)
                os.kill(int((root / 'supervisor.pid').read_text()), signal.SIGTERM)
                stdout, stderr = process.communicate(timeout=10)
                result = subprocess.CompletedProcess(arguments, process.returncode, stdout, stderr)
            finally:
                if process.poll() is None:
                    process.kill()
                    process.communicate(timeout=10)
        else:
            result = subprocess.run(arguments, cwd=root, env=environment,
                                    text=True, capture_output=True, timeout=15)
        event_path = root / 'events.jsonl'
        events = [json.loads(line) for line in event_path.read_text().splitlines()] if event_path.exists() else []
        return result, events

    def test_viewer_ui_and_build_overlap_before_security_smoke(self):
        with tempfile.TemporaryDirectory(prefix='oasis7-viewer-parallel-') as directory:
            result, events = self.viewer_fixture(directory)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertCountEqual([role for role, _ in events], ['ui', 'build', 'browser', 'smoke'])
        self.assertEqual(events[-1][0], 'smoke')

    def test_viewer_ui_failure_cannot_be_overwritten_by_successful_build(self):
        with tempfile.TemporaryDirectory(prefix='oasis7-viewer-parallel-') as directory:
            result, events = self.viewer_fixture(directory, failing='ui')
        self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertNotIn('smoke', [role for role, _ in events])

    def test_viewer_failed_build_does_not_smoke_stale_dist(self):
        with tempfile.TemporaryDirectory(prefix='oasis7-viewer-parallel-') as directory:
            result, events = self.viewer_fixture(directory, failing='build')
            self.assertEqual((Path(directory) / 'dist/current').read_text(), 'stale')
        self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertNotIn('smoke', [role for role, _ in events])

    def test_viewer_contract_failure_stops_before_parallel_children(self):
        with tempfile.TemporaryDirectory(prefix='oasis7-viewer-parallel-') as directory:
            result, events = self.viewer_fixture(directory, source_failure=True)
        self.assertEqual(result.returncode, 23, result.stdout + result.stderr)
        self.assertEqual(events, [])

    def test_viewer_timeout_cannot_reach_security_smoke(self):
        with tempfile.TemporaryDirectory(prefix='oasis7-viewer-parallel-') as directory:
            result, events = self.viewer_fixture(directory, timeout=True)
        self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn('"timeout": true', result.stdout)
        self.assertNotIn('smoke', [role for role, _ in events])

    def test_viewer_cancellation_cannot_reach_security_smoke(self):
        with tempfile.TemporaryDirectory(prefix='oasis7-viewer-parallel-') as directory:
            result, events = self.viewer_fixture(directory, cancel=True)
        self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn('"cancelled": true', result.stdout)
        self.assertNotIn('smoke', [role for role, _ in events])

    def test_viewer_zero_exit_with_detached_child_cannot_reach_security_smoke(self):
        with tempfile.TemporaryDirectory(prefix='oasis7-viewer-parallel-') as directory:
            result, events = self.viewer_fixture(directory, leak=True)
        self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn('"leaked_processes": true', result.stdout)
        self.assertNotIn('smoke', [role for role, _ in events])


CUSTODY_BUILD_CASES = [
    ('build-role.sh', 'identity', 'identity'),
    ('build-role.sh', 'owner', 'owner'),
    ('build-role.sh', 'executor', 'executor'),
    ('build-role.sh', 'recovery-fence', 'recovery-fence'),
    ('build-approval-harness.sh', None, 'owner,approval-harness'),
    ('build-native-runtime-harness.sh', None, 'identity,native-runtime-harness'),
]


class CustodyBuildToolContracts(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.worker = self.root / 'crates/oasis7_custody_worker'
        (self.worker / 'scripts').mkdir(parents=True)
        (self.worker / 'platform').mkdir()
        for name, _, _ in CUSTODY_BUILD_CASES:
            shutil.copyfile(ROOT / 'crates/oasis7_custody_worker/scripts' / name, self.worker / 'scripts' / name)
        for name in ('identity', 'owner', 'executor', 'fence'):
            (self.worker / 'platform' / f'{name}-entry.mjs').write_text('// fixture entry\n')
        (self.root / 'scripts').mkdir()
        self.spies = self.root / 'spies'
        self.spies.mkdir()
        self.write_executable(self.root / 'scripts/ensure-wasm-bindgen-cli.sh', '''#!/bin/bash
set -eu
[[ "$*" == --print-bin ]]
printf 'resolve\n' >> "$CALLS"
[[ "${RESOLVE_FAIL:-0}" == 0 ]] || exit 67
printf '%s\n' "$EXPECTED_BINDGEN"
''')
        self.write_executable(self.spies / 'worker-build', '''#!/bin/bash
set -eu
if [[ "$*" == --version ]]; then echo "${WORKER_VERSION:-0.7.5}"; exit 0; fi
[[ "$WASM_BINDGEN_BIN" == "$EXPECTED_BINDGEN" ]]
[[ -z "${RUSTC_WRAPPER+x}" ]]
printf 'build %s\n' "$*" >> "$CALLS"
mkdir -p build
printf '// generated fixture\n' > build/index.js
''')
        self.write_executable(self.spies / 'cargo', '''#!/bin/bash
set -eu
[[ -z "${RUSTC_WRAPPER+x}" ]]
printf 'cargo %s\n' "$*" >> "$CALLS"
printf '{}\n' > "$OASIS7_CUSTODY_ADMISSION_FIXTURE_OUTPUT"
''')
        self.calls = self.root / 'calls'
        self.environment = dict(os.environ, PATH=str(self.spies) + os.pathsep + os.environ['PATH'],
                                EXPECTED_BINDGEN=str(self.root / 'tools with spaces/wasm-bindgen'),
                                WASM_BINDGEN_BIN='/wrong/old-cli', RUSTC_WRAPPER='must-not-run', CALLS=str(self.calls))

    def write_executable(self, path, source):
        path.write_text(source)
        path.chmod(0o755)

    def execute(self, script, role):
        command = ['bash', str(self.worker / 'scripts' / script)]
        if role is not None:
            command.append(role)
        return subprocess.run(command, env=self.environment, text=True, capture_output=True)

    def test_every_producer_uses_resolved_cli_and_keeps_original_build_flags(self):
        for script, role, features in CUSTODY_BUILD_CASES:
            with self.subTest(script=script, role=role):
                self.calls.write_text('')
                result = self.execute(script, role)
                self.assertEqual(result.returncode, 0, result.stderr)
                calls = self.calls.read_text().splitlines()
                self.assertEqual(calls[0], 'resolve')
                self.assertEqual(sum(line.startswith('resolve') for line in calls), 1)
                self.assertEqual([line for line in calls if line.startswith('build ')],
                                 [f'build --release --no-opt -- --locked --no-default-features --features {features}'])
                self.assertFalse((self.worker / '.role-build-lock').exists())

    def test_failed_resolution_never_builds_and_releases_lock(self):
        self.environment['RESOLVE_FAIL'] = '1'
        for script, role, _ in CUSTODY_BUILD_CASES:
            with self.subTest(script=script, role=role):
                self.calls.write_text('')
                result = self.execute(script, role)
                self.assertEqual(result.returncode, 67, result.stderr)
                self.assertEqual(self.calls.read_text().splitlines(), ['resolve'])
                self.assertFalse((self.worker / '.role-build-lock').exists())

    def test_wrong_worker_version_fails_before_dependency_resolution(self):
        self.environment['WORKER_VERSION'] = '0.0.0'
        for script, role, _ in CUSTODY_BUILD_CASES:
            with self.subTest(script=script, role=role):
                result = self.execute(script, role)
                self.assertEqual(result.returncode, 2, result.stderr)
                self.assertFalse(self.calls.exists())
                self.assertFalse((self.worker / '.role-build-lock').exists())



if __name__ == '__main__':
    unittest.main()
