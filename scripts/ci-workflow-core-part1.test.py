#!/usr/bin/env python3
"""Self-contained core workflow runner, viewer bridge, and production guard contract tests (part 1)."""
import json
import os
from pathlib import Path
import re
import shlex
import shutil
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parent.parent


class WorkflowCorePart1(unittest.TestCase):
    def test_group_commands_execute_once_and_propagate_failure(self):
        driver = (ROOT / 'scripts/ci-tests.sh').read_text()
        network = re.search(r'^run_oasis7_net_libp2p_tests\(\) \{\n.*?^\}', driver, re.M | re.S).group()
        script = "set -euo pipefail\nrun() { printf '%s\\n' \"$*\"; }\nrun_cargo() { run cargo \"$@\"; }\n" + network + "\nrun_oasis7_net_libp2p_tests\n"
        result = subprocess.run(['bash', '-c', script], text=True, capture_output=True, env=dict(os.environ, GITHUB_ACTIONS='true', RUNNER_OS='Linux'))
        self.assertEqual(result.returncode, 0, result.stderr)
        for command in ('libp2p-security-source.test.py',
                        'check -p oasis7_net', 'check -p oasis7_node'):
            self.assertEqual(result.stdout.count(command), 1)
        failure = subprocess.run(['bash', '-c', script.replace("printf '%s\\n' \"$*\"", 'return 42')], text=True, capture_output=True, env=dict(os.environ, GITHUB_ACTIONS='true', RUNNER_OS='Linux'))
        self.assertEqual(failure.returncode, 42)
        for group, command in [('viewer_js_required', 'viewer-auth-browser-security-smoke.mjs'), ('viewer_performance_report', 'build:viewer:visual-test')]:
            case = re.search(r'^    ' + group + r'\) (.*?) ;;$', driver, re.M).group(1)
            definitions = "set -euo pipefail\nrun() { printf '%s\\n' \"$*\"; }\n"
            names = re.findall(r'run_oasis7_[a-z_]+', case)
            for name in names:
                if name == 'run_oasis7_viewer_visual_test_build':
                    definitions += name + "() { run npm run build:viewer:visual-test; }\n"
                else:
                    definitions += name + "() { run " + name + "; }\n"
            result = subprocess.run(['bash', '-c', definitions + case], text=True, capture_output=True)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(result.stdout.count(command), 1)
            failed = subprocess.run(['bash', '-c', definitions.replace("printf '%s\\n' \"$*\"", 'return 42') + case], text=True, capture_output=True)
            self.assertEqual(failed.returncode, 42)

    def test_full_viewer_agency_bridge_uses_existing_rust_run_and_propagates_failure(self):
        driver = (ROOT / 'scripts/ci-tests.sh').read_text()
        opt_in = re.search(r'^agency_snapshot_bridge=0\ncase "\$tier:\$group" in\n.*?^esac$', driver, re.M | re.S).group()
        full = re.search(r'^run_oasis7_full_tier_tests\(\) \{\n.*?^\}', driver, re.M | re.S).group()
        script = """set -euo pipefail
run_oasis7_test_tier_with_viewer() {
  [[ "$1" == test_tier_full,wasmtime,viewer_live_integration ]]
  printf 'rust\\n'
  if [[ "${OASIS7_AGENCY_TEST_SNAPSHOT_OUT:-}" != "" ]]; then
    printf '{}' > "$OASIS7_AGENCY_TEST_SNAPSHOT_OUT"
    printf '{}' > "$OASIS7_AGENCY_STALE_SNAPSHOT_OUT"
  fi
  return "$rust_status"
}
run() {
  [[ "$1" == node && "$2" == */verify-agency-snapshot-bridge.mjs ]]
  [[ -s "$3/restored.json" && -s "$3/stale.json" ]]
  printf 'viewer\\n'
  return "$viewer_status"
}
""" + full + '\n' + opt_in + '\nrun_oasis7_full_tier_tests\n'
        with tempfile.TemporaryDirectory() as root:
            target = Path(root) / 'target'
            target.mkdir()
            for tier, group, rust_status, viewer_status, code, expected in (
                ('full', '', 0, 0, 0, ['rust', 'viewer']),
                ('full-standard', '', 0, 0, 0, ['rust', 'viewer']),
                ('full-extended', '', 0, 0, 0, ['rust', 'viewer']),
                ('full-extended', '', 71, 0, 71, ['rust']),
                ('full-extended', '', 0, 72, 72, ['rust', 'viewer']),
                ('full-support', '', 0, 0, 0, ['rust']),
                ('required', 'viewer_js_required', 0, 0, 0, ['rust']),
            ):
                with self.subTest(tier=tier, group=group, code=code):
                    env = dict(os.environ, tier=tier, group=group, repo_root=root,
                               rust_status=str(rust_status), viewer_status=str(viewer_status))
                    result = subprocess.run(['bash', '-c', script], text=True,
                                            capture_output=True, env=env)
                    self.assertEqual(result.returncode, code, result.stderr)
                    self.assertEqual(result.stdout.splitlines(), expected)
                    self.assertEqual(list(target.iterdir()), [])

    def test_viewer_agency_bridge_receipt_rejects_absent_skipped_or_wrong_tests(self):
        script = """import assert from 'node:assert/strict';
import { pathToFileURL } from 'node:url';
const { verifyAgencySnapshotReceipt } = await import(pathToFileURL(process.env.BRIDGE_VERIFIER));
const name = '/viewer/software_safe_src/control_proof_panel.test.jsx';
const report = {
  success: true, numFailedTests: 0, numPassedTests: 2,
  testResults: [{ name, assertionResults: [
    { title: 'preserves durable correction and canonical receipt identity through feedback and the rendered panel',
      ancestorTitles: ['actual restored Rust owner snapshot bridge'], status: 'passed' },
    { title: 'keeps the earliest prepared identity separate from the committed replacement and its receipt',
      ancestorTitles: ['actual stale-replacement owner snapshot bridge'], status: 'passed' },
  ] }],
};
verifyAgencySnapshotReceipt(report, name);
for (const mutate of [
  r => { r.numPassedTests = 0; },
  r => { r.testResults[0].assertionResults[1].status = 'skipped'; },
  r => { r.testResults[0].assertionResults[1].title = 'unrelated passed test'; },
  r => { r.testResults[0].name = '/other/control_proof_panel.test.jsx'; },
  r => { r.testResults[0].assertionResults.push(r.testResults[0].assertionResults[0]); },
]) {
  const changed = structuredClone(report);
  mutate(changed);
  assert.throws(() => verifyAgencySnapshotReceipt(changed, name));
}
"""
        env = dict(os.environ, BRIDGE_VERIFIER=str(
            ROOT / 'crates/oasis7_viewer/scripts/verify-agency-snapshot-bridge.mjs'))
        result = subprocess.run(['node', '--input-type=module', '-e', script],
                                env=env, text=True, capture_output=True)
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_required_and_full_tier_runners_share_feature_matched_viewer_artifact(self):
        driver = (ROOT / 'scripts/ci-tests.sh').read_text()
        names = ('run_oasis7_package_tests', 'build_oasis7_viewer_binary', 'run_oasis7_test_tier_with_viewer',
                 'run_oasis7_required_tier_tests', 'run_oasis7_full_tier_tests')
        functions = [re.search(r'^' + name + r'\(\) \{\n.*?^\}', driver, re.M | re.S).group()
                     for name in names]
        with tempfile.TemporaryDirectory() as temporary:
            viewer = Path(temporary) / 'oasis7_viewer_live'
            viewer.write_text('test viewer executable')
            viewer.chmod(0o755)
            log = Path(temporary) / 'commands.log'
            script = """set -euo pipefail
ci_python=python3
run_cargo() {
  if [[ "$1" == build ]]; then
    printf 'BUILD|%s\\n' "$*" >> "$CI_LOG"
    [[ "${FAIL_PHASE:-}" != build ]] || return 47
    printf '{"reason":"compiler-artifact","target":{"name":"oasis7_viewer_live"},"executable":"%s"}\\n' "$VIEWER_BIN"
  elif [[ "$1" == check ]]; then
    printf 'CHECK|%s\\n' "$*" >> "$CI_LOG"
    [[ "${FAIL_PHASE:-}" != check ]] || return 49
  else
    printf 'TEST|%s|VIEWER=%s\\n' "$*" "${PRE2_VIEWER_BINARY:-}" >> "$CI_LOG"
    [[ "${FAIL_PHASE:-}" != test ]] || return 48
  fi
}
""" + "\n".join(functions) + "\nrun_oasis7_required_tier_tests\nrun_oasis7_full_tier_tests\n"
            environment = dict(os.environ, CI_LOG=str(log), VIEWER_BIN=str(viewer))
            environment.pop('PRE2_VIEWER_BINARY', None)
            # Exercise the Cargo fallback even when the outer CI uses a bundle.
            environment.pop('OASIS7_CI_TEST_BUNDLE', None)
            result = subprocess.run(['bash', '-c', script], text=True, capture_output=True, env=environment)
            self.assertEqual(result.returncode, 0, result.stderr)
            calls = log.read_text().splitlines()
            self.assertEqual(sum(call.startswith('BUILD|') for call in calls), 2)
            self.assertEqual(sum(call.startswith('TEST|') for call in calls), 2)
            self.assertIn('CHECK|check -p oasis7_agent_api -p oasis7_simulator -p oasis7_viewer_codec -p oasis7_viewer_runtime --lib --target wasm32-unknown-unknown --locked', calls)
            self.assertTrue(all('-p oasis7_simulator' in call for call in calls if call.startswith('TEST|')))
            for feature in ('test_tier_required', 'test_tier_full,wasmtime,viewer_live_integration'):
                build_index = next((index for index, call in enumerate(calls)
                                    if call.startswith('BUILD|') and f'--features {feature} ' in call), None)
                test_index = next((index for index, call in enumerate(calls)
                                   if call.startswith('TEST|') and f'--features {feature}|' in call
                                   and f'VIEWER={viewer}' in call), None)
                self.assertIsNotNone(build_index, feature)
                self.assertIsNotNone(test_index, feature)
                self.assertLess(build_index, test_index, feature)

            full_only_script = script.replace('run_oasis7_required_tier_tests\nrun_oasis7_full_tier_tests\n',
                                              'run_oasis7_full_tier_tests\n')
            self.assertNotEqual(full_only_script, script)
            for phase, expected in (('build', 47), ('test', 48)):
                failure_log = Path(temporary) / f'{phase}.log'
                failure_environment = dict(environment, CI_LOG=str(failure_log), FAIL_PHASE=phase)
                failed = subprocess.run(['bash', '-c', full_only_script], text=True, capture_output=True,
                                        env=failure_environment)
                self.assertEqual(failed.returncode, expected, failed.stderr)
                failure_calls = failure_log.read_text().splitlines()
                if phase == 'build':
                    self.assertTrue(any(call.startswith('BUILD|') and '--features test_tier_full,wasmtime,viewer_live_integration' in call
                                        for call in failure_calls))
                    self.assertFalse(any(call.startswith('TEST|') for call in failure_calls))
                else:
                    self.assertTrue(any(call.startswith('TEST|') and '--features test_tier_full,wasmtime,viewer_live_integration' in call
                                        for call in failure_calls))

    def test_full_support_supplies_feature_matched_viewer_and_stops_on_build_failure(self):
        driver = (ROOT / 'scripts/ci-tests.sh').read_text()
        names = ('run_full_support_tier_tests', 'run_oasis7_package_tests',
                 'build_oasis7_viewer_binary', 'run_oasis7_test_tier_with_viewer')
        functions = [re.search(r'^' + name + r'\(\) \{\n.*?^\}', driver, re.M | re.S).group()
                     for name in names]
        support = functions[0]
        dependencies = set(re.findall(r'^  (run_[a-z0-9_]+)', support, re.M)) - set(names)
        with tempfile.TemporaryDirectory() as temporary:
            viewer = Path(temporary) / 'oasis7_viewer_live'
            viewer.write_text('controlled artifact for runner wiring only')
            viewer.chmod(0o755)
            script = "set -euo pipefail\nci_python=python3\n"
            script += "\n".join(name + '() { :; }' for name in dependencies) + "\n"
            script += r"""
run_cargo() {
  if [[ "$1" == build ]]; then
    [[ "${FAIL_BUILD:-}" != 1 ]] || return 47
    printf '{"reason":"compiler-artifact","target":{"name":"oasis7_viewer_live"},"executable":"%s"}\n' "$VIEWER_BIN"
  else
    [[ -n "${PRE2_VIEWER_BINARY:-}" && "$PRE2_VIEWER_BINARY" == "$VIEWER_BIN" ]] || return 48
    [[ "$*" == *"--lib --bins --features wasmtime" ]] || return 49
  fi
}
"""
            script += "\n".join(functions) + "\nrun_full_support_tier_tests\n"
            for fail_build, expected in (('0', 0), ('1', 47)):
                result = subprocess.run(['bash', '-c', script], text=True, capture_output=True,
                    env=dict(os.environ, PRE2_VIEWER_BINARY='', VIEWER_BIN=str(viewer), FAIL_BUILD=fail_build))
                self.assertEqual(result.returncode, expected, result.stderr)

    def test_primary_rejects_non_loopback_test_login_listener(self):
        with tempfile.TemporaryDirectory() as directory:
            # Verify argument handling without agent-browser or its npx fallback.
            fixture = Path(directory)
            (fixture / 'dirname').symlink_to(shutil.which('dirname'))
            output = fixture / 'output'
            command = [shutil.which('bash'), str(ROOT / 'scripts/viewer-primary-web-entry-regression.sh'),
                       '--out-dir', str(output)]
            for options, status, diagnostic in (
                    (['--viewer-host', '0.0.0.0'], 2, 'requires a loopback viewer host'),
                    (['--web-bind', '127.0.0.1'], 2, '--web-bind must be in <host:port> format'),
                    (['--web-bind', '127.0.0.1:no-port'], 2, 'invalid --web-bind'),
                    ([], 1, 'missing required command: agent-browser')):
                with self.subTest(options=options):
                    result = subprocess.run(command + options, capture_output=True, text=True,
                                            env=dict(os.environ, PATH=directory))
                    self.assertEqual(result.returncode, status, result.stderr)
                    self.assertIn(diagnostic, result.stderr)
                    self.assertFalse(output.exists())

    def test_primary_login_requires_guest_gate_and_visible_backend_issued_session(self):
        source = (ROOT / 'scripts/viewer-primary-web-entry-regression.sh').read_text()
        function = 'enter_primary_player_surface() {' + source.split(
            'enter_primary_player_surface() {', 1)[1].split('\n}', 1)[0] + '\n}'
        script = r"""set -euo pipefail
out_dir=/unused
assert_eval_true() {
  if [[ "$2" == *'authSource'* ]]; then
    echo issued >> "$TRACE_LOG"
    [[ "$FAIL_PHASE" != issued ]]
  else
    echo guest >> "$TRACE_LOG"
    [[ "$FAIL_PHASE" != guest ]]
  fi
}
capture_screenshot_best_effort() { :; }
ab_cmd() {
  [[ "$2" == click && "$3" == '[data-auth-action="test-login"]' ]] || return 42
  echo click >> "$TRACE_LOG"
  [[ "$FAIL_PHASE" != click ]]
}
""" + function.replace('>>"$out_dir/test-login.log" 2>&1', '') + '\nenter_primary_player_surface owned default'
        with tempfile.TemporaryDirectory() as directory:
            for phase, calls in (('', ['guest', 'click', 'issued']), ('guest', ['guest']),
                                 ('click', ['guest', 'click']), ('issued', ['guest', 'click', 'issued'])):
                trace = Path(directory) / (phase + 'trace')
                result = subprocess.run(['bash', '-c', script], capture_output=True, text=True,
                    env=dict(os.environ, TRACE_LOG=str(trace), FAIL_PHASE=phase))
                self.assertEqual(result.returncode == 0, phase == '', result.stderr)
                self.assertEqual(trace.read_text().splitlines(), calls)

    def test_primary_flow_requires_visible_unique_action_context_and_receipt(self):
        source = (ROOT / 'scripts/viewer-primary-web-entry-regression.sh').read_text()
        self.assertTrue('primary_entry_surface_predicate()' in source, 'primary flow predicate missing')
        function = source.split('primary_entry_surface_predicate() {', 1)[1]
        expression = function.split("<<'JS'\n", 1)[1].split('\nJS', 1)[0]
        valid = dict(root=True, actions=1, context=True, receipt=True,
                     visible=True, href='#command', text='Inspect current blocker')
        variants = [(valid, True)]
        for field, value in (('root', False), ('actions', 0), ('actions', 2),
                             ('context', False), ('receipt', False), ('visible', False),
                             ('href', ''), ('text', '')):
            variants.append((dict(valid, **{field: value}), False))
        for candidate, expected in variants:
            script = 'const state=' + json.dumps(candidate) + ';\n' + r"""
const node = () => ({textContent:state.text, getClientRects:()=>state.visible?[{}]:[],
  getAttribute:()=>state.href});
const root = node();
root.querySelectorAll = () => Array.from({length:state.actions}, node);
root.querySelector = selector => selector.includes('supporting-context')
  ? (state.context ? node() : null) : (state.receipt ? node() : null);
global.document = {querySelector:()=>state.root ? root : null};
global.getComputedStyle = () => ({visibility:'visible', display:'block'});
"""
            script += 'if ((' + expression + ') !== ' + str(expected).lower() + ') process.exit(1);'
            result = subprocess.run(['node', '-e', script], capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, str(candidate) + result.stderr)

    def test_release_web_fixture_starts_only_after_complete_stack_build(self):
        source = (ROOT / 'scripts/viewer-software-safe-step-regression.sh').read_text()
        match = re.search(r'^prepare_release_web_stack\(\) \{\n.*?^\}', source, re.M | re.S)
        self.assertIsNotNone(match, 'release Web startup must finish builds before starting fixture')
        self.assertIn('    prepare_release_web_stack\n', source)
        script = r"""set -euo pipefail
out_dir=/unused
ab_log=/dev/null
BOOTSTRAP_USES_BUNDLE="${CASE_BUNDLE:-0}"
unset OASIS7_RUN_LAUNCHER_STACK_SKIP_SOURCE_BUILD
log_note() { :; }
record() { echo "$1" >> "$TRACE_LOG"; [[ "$FAIL_PHASE" != "$1" ]]; }
release_web_fixture_build_viewer() { record viewer; }
release_web_fixture_build() { record fixture; }
oasis7_cargo_dev() {
  for binary in oasis7_llm_provider_probe oasis7_game_launcher oasis7_viewer_live; do
    [[ "$*" == *"--bin $binary"* ]] || return 49
  done
  record stack
}
release_web_fixture_start() { record start; }
""" + match.group() + '\nprepare_release_web_stack\nif [[ "$BOOTSTRAP_USES_BUNDLE" == 1 ]]; then [[ -z "${OASIS7_RUN_LAUNCHER_STACK_SKIP_SOURCE_BUILD+x}" ]]; else [[ "$OASIS7_RUN_LAUNCHER_STACK_SKIP_SOURCE_BUILD" == 1 ]]; fi'
        with tempfile.TemporaryDirectory() as directory:
            for phase, calls in (('', ['viewer', 'fixture', 'stack', 'start']),
                                 ('viewer', ['viewer']), ('fixture', ['viewer', 'fixture']),
                                 ('stack', ['viewer', 'fixture', 'stack'])):
                trace = Path(directory) / (phase + 'trace')
                result = subprocess.run(['bash', '-c', script], capture_output=True, text=True,
                    env=dict(os.environ, TRACE_LOG=str(trace), FAIL_PHASE=phase))
                self.assertEqual(result.returncode == 0, phase == '', result.stderr)
                self.assertEqual(trace.read_text().splitlines(), calls)
            trace = Path(directory) / 'bundle-trace'
            result = subprocess.run(['bash', '-c', script], capture_output=True, text=True,
                env=dict(os.environ, TRACE_LOG=str(trace), FAIL_PHASE='', CASE_BUNDLE='1'))
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(trace.read_text().splitlines(), ['viewer', 'fixture', 'start'])

    def test_full_provider_artifact_is_prepared_once_before_each_full_dispatch(self):
        driver = (ROOT / 'scripts/ci-tests.sh').read_text()
        prelude = re.search(r'^if \[\[ -z "\$group" \]\]; then\n.*?^fi$', driver, re.M | re.S).group()
        extended = re.search(r'^run_full_extended_tier_tests\(\) \{\n.*?^\}', driver, re.M | re.S).group()
        self.assertNotIn('prepare-provider', extended)
        self.assertNotIn('prepare_full_provider_artifact', extended)
        with tempfile.TemporaryDirectory() as temporary:
            for tier in ('full', 'full-standard', 'full-extended', 'full-core'):
                case = r'full\|full-standard' if tier in ('full', 'full-standard') else tier
                dispatch = re.search(r'^    ' + case + r'\) (.*?) ;;$', driver, re.M).group(1)
                script = """set -euo pipefail
group=
prepared=0
prepare_full_provider_artifact() { ((prepared+=1)); printf 'PREPARE\\n' >> "$CI_LOG"; }
require_prepared() { [[ "$prepared" == 1 ]]; printf 'CONSUME\\n' >> "$CI_LOG"; }
run_full_required_superset() { require_prepared; }
run_rust_baseline() { require_prepared; }
run_oasis7_full_tier_tests() { require_prepared; }
run_workspace_support_full_feature_tests() { require_prepared; }
run_oasis7_llm_baseline_fixture_smoke() { require_prepared; }
run_oasis7_production_mock_guard() { require_prepared; }
run_oasis7_test_tier_with_viewer() { require_prepared; }
run_full_core_tier_tests() { require_prepared; }
run_group() { require_prepared; }
""" + 'tier=' + shlex.quote(tier) + '\n' + extended + '\n' + prelude + '\n' + dispatch
                log = Path(temporary) / (tier + '.log')
                result = subprocess.run(['bash', '-c', script], text=True, capture_output=True,
                                        env=dict(os.environ, CI_LOG=str(log)))
                self.assertEqual(result.returncode, 0, result.stderr)
                calls = log.read_text().splitlines()
                self.assertEqual(calls[0], 'PREPARE')
                self.assertEqual(calls.count('PREPARE'), 1)
                self.assertGreater(calls.count('CONSUME'), 0)

    def test_full_tail_keeps_strict_wasmtime_profile_and_fixture_viewer(self):
        driver = (ROOT / 'scripts/ci-tests.sh').read_text()
        functions = [re.search(r'^' + name + r'\(\) \{\n.*?^\}', driver, re.M | re.S).group()
                     for name in ('run_oasis7_package_tests', 'build_oasis7_viewer_binary', 'run_oasis7_test_tier_with_viewer', 'run_full_extended_tier_tests')]
        full = re.search(r'^    full\|full-standard\) (.*?) ;;$', driver, re.M).group(1)
        with tempfile.TemporaryDirectory() as temporary:
            viewer = Path(temporary) / 'oasis7_viewer_live'
            viewer.write_text('test viewer executable')
            viewer.chmod(0o755)
            script = """set -euo pipefail
tier=full-standard
ci_python=python3
run_full_required_superset() { :; }
run_rust_baseline() { :; }
run_oasis7_full_tier_tests() { :; }
run_workspace_support_full_feature_tests() { :; }
run_oasis7_llm_baseline_fixture_smoke() { :; }
run_group() { [[ "$1" == formal_process ]] || return 50; printf 'FORMAL\\n' >> "$CI_LOG"; }
run_oasis7_production_mock_guard() { printf 'GUARD\\n' >> "$CI_LOG"; [[ "${FAIL_PHASE:-}" != guard ]] || return 49; }
run_cargo() {
  if [[ "$1" == build ]]; then
    printf 'BUILD|%s\\n' "$*" >> "$CI_LOG"
    [[ "${FAIL_PHASE:-}" != build ]] || return 47
    printf '{"reason":"compiler-artifact","target":{"name":"oasis7_viewer_live"},"executable":"%s"}\\n' "$VIEWER_BIN"
  else
    printf 'TEST|%s|VIEWER=%s\\n' "$*" "${PRE2_VIEWER_BINARY:-}" >> "$CI_LOG"
    [[ "${FAIL_PHASE:-}" != test ]] || return 48
  fi
}
""" + "\n".join(functions) + "\n" + full + "\n"
            environment = dict(os.environ, VIEWER_BIN=str(viewer))
            environment.pop('PRE2_VIEWER_BINARY', None)
            for phase, expected in (('', 0), ('guard', 49), ('build', 47), ('test', 48)):
                log = Path(temporary) / (phase + 'calls.log')
                result = subprocess.run(['bash', '-c', script], text=True, capture_output=True,
                                        env=dict(environment, CI_LOG=str(log), FAIL_PHASE=phase))
                self.assertEqual(result.returncode, expected, result.stderr)
                calls = log.read_text().splitlines()
                self.assertEqual(calls.pop(0), 'GUARD')
                if phase == 'guard':
                    self.assertEqual(calls, [])
                    continue
                self.assertTrue(calls[0].startswith('BUILD|'), calls)
                self.assertIn('--features test_tier_required,wasmtime ', calls[0])
                self.assertNotIn('test_tier_full', '\n'.join(calls))
                self.assertNotIn('viewer_live_integration', '\n'.join(calls))
                tests = [call for call in calls if call.startswith('TEST|')]
                self.assertEqual(len(tests), 0 if phase == 'build' else 1)
                if tests:
                    self.assertIn('--lib --bins', tests[0])
                    self.assertIn('--features test_tier_required,wasmtime|', tests[0])
                    self.assertIn(f'VIEWER={viewer}', tests[0])

            formal_log = Path(temporary) / 'formal-calls.log'
            formal_script = script.replace('tier=full-standard', 'tier=full')
            result = subprocess.run(['bash', '-c', formal_script], text=True, capture_output=True,
                                    env=dict(environment, CI_LOG=str(formal_log)))
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(formal_log.read_text().splitlines()[-1], 'FORMAL')
            failed_log = Path(temporary) / 'formal-failed-calls.log'
            failed = subprocess.run(['bash', '-c', formal_script], text=True, capture_output=True,
                                    env=dict(environment, CI_LOG=str(failed_log), FAIL_PHASE='test'))
            self.assertEqual(failed.returncode, 48, failed.stderr)
            self.assertNotIn('FORMAL', failed_log.read_text())

    def test_production_mock_guard_checks_actual_exit_and_rejection(self):
        import runpy
        guard = runpy.run_path(str(ROOT / 'scripts/hosted-local-mock-production-guard.py'))
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            manifest = directory / 'artifacts.jsonl'
            calls = directory / 'calls.log'
            executables = {}
            artifacts = {}
            packages = {
                'oasis7_chain_runtime': ('oasis7_chain_runtime', ['default', 'node-libp2p', 'wasmtime']),
                'oasis7_game_launcher': ('oasis7_launcher_backend', ['default']),
            }

            def write_executable(name, output, code):
                executables[name].write_text(
                    f'#!{sys.executable}\nfrom pathlib import Path\n'
                    f'with Path({str(calls)!r}).open("a") as log: log.write({name!r} + "\\n")\n'
                    f'print({output!r})\nraise SystemExit({code})\n'
                )
                executables[name].chmod(0o755)

            for name, (package, features) in packages.items():
                executable = directory / name
                executables[name] = executable
                write_executable(name, guard['REJECTION'], 1)
                artifacts[name] = {
                    'reason': 'compiler-artifact', 'target': {'name': name, 'kind': ['bin']},
                    'manifest_path': str(ROOT / 'crates' / package / 'Cargo.toml'),
                    'profile': {'test': False},
                    'executable': str(executable), 'features': features,
                }

            def write_manifest(records):
                # Cargo's JSON stream can also contain driver status lines.
                manifest.write_text('+ cargo build\n' + '\n'.join(json.dumps(record) for record in records))
                calls.unlink(missing_ok=True)

            # Match the split packages: only Chain declares wasmtime.
            write_manifest(artifacts.values())
            guard['verify'](manifest)
            self.assertEqual(calls.read_text().splitlines(), list(packages))

            for name, artifact in artifacts.items():
                invalid = [
                    {'features': [*artifact['features'], feature]}
                    for feature in ('test_tier_required', 'test_tier_full')
                ]
                invalid.extend([
                    {'target': {'name': name, 'kind': ['test']}},
                    {'profile': {'test': True}},
                    {'manifest_path': str(ROOT / 'crates/oasis7/Cargo.toml')},
                    {'manifest_path': None},
                ])
                if name == 'oasis7_chain_runtime':
                    invalid.append({'features': ['default', 'node-libp2p']})
                for changes in invalid:
                    with self.subTest(target=name, changes=changes):
                        write_manifest(dict(record, **changes) if target == name else record
                                       for target, record in artifacts.items())
                        with self.assertRaises(AssertionError):
                            guard['verify'](manifest)
                        self.assertFalse(calls.exists(), 'invalid artifacts must fail before execution')
                for records in ([record for target, record in artifacts.items() if target != name],
                                [*artifacts.values(), artifact]):
                    with self.subTest(target=name, count=len(records)):
                        write_manifest(records)
                        with self.assertRaises(AssertionError):
                            guard['verify'](manifest)
                        self.assertFalse(calls.exists())

            for name in executables:
                for output, code in ((guard['REJECTION'], 0), ('unrelated failure', 1)):
                    with self.subTest(target=name, output=output, code=code):
                        write_manifest(artifacts.values())
                        write_executable(name, output, code)
                        with self.assertRaises(AssertionError):
                            guard['verify'](manifest)
                write_executable(name, guard['REJECTION'], 1)


if __name__ == '__main__':
    unittest.main()
