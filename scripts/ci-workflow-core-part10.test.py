#!/usr/bin/env python3
"""Self-contained core workflow workspace-support, custody approval, concurrency, fast cache writer, source/plan, token regression, and clippy order contract tests (part 10)."""
import hashlib
import itertools
import json
import os
from pathlib import Path
import re
import shlex
import subprocess
import sys
import tempfile
import textwrap
import unittest

ROOT = Path(__file__).resolve().parent.parent
RAW_WORKFLOW = (ROOT / '.github/workflows/rust.yml').read_text()
CONFIG = json.loads((ROOT / 'scripts/ci-required-scope.json').read_text())


def workspace_support_fixture_functions(driver):
    prefix = ('ci_python=' + shlex.quote(sys.executable) + '\n'
              + 'driver_dir=' + shlex.quote(str(ROOT / 'scripts')) + '\n')
    if '# ci-driver-library: functions-v1' in driver.splitlines():
        # Load the real transitive definitions before the fixture overrides its
        # recording boundaries, without maintaining a second dependency list.
        return prefix + 'source "$driver_dir/ci-tests.sh"\n'
    # Extract the real transitive shell dependency, rather than supplying a
    # no-op helper, for historical drivers without the source interface.
    names = ('run_dev_test_worker_checks',
             'run_oasis7_workspace_support_crate_tests')
    functions = [re.search(r'^' + name + r'\(\) \{\n.*?^\}',
                           driver, re.M | re.S).group() for name in names]
    return prefix + '\n'.join(functions) + '\n'


def workflow_jobs(workflow):
    jobs = dict(re.findall(r'^  ([a-z][a-z0-9_-]*):\n(.*?)(?=^  [a-z][a-z0-9_-]*:|\Z)',
                          workflow.split('\njobs:\n', 1)[1], re.M | re.S))
    anchors = {}
    for job in jobs.values():
        match = re.search(r'^    steps: &([a-z0-9-]+)\n(.*)', job, re.M | re.S)
        if match:
            assert match[1] not in anchors, 'duplicate step sequence anchor'
            anchors[match[1]] = match[2]
    jobs = {name: re.sub(r'^    steps: &[a-z0-9-]+\n', '    steps:\n',
                        re.sub(r'^    steps: \*([a-z0-9-]+)\n',
                               lambda match: '    steps:\n' + anchors[match[1]], job, flags=re.M), flags=re.M)
            for name, job in jobs.items()}
    # Resolve individual step aliases as well as complete step sequences. Keep
    # the old observation alias opaque: contracts inspect that step separately.
    definitions = re.findall(
        r'^      - &([a-z0-9-]+)\n(.*?)(?=^      - |^\S|^  [a-z]|\Z)',
        workflow, re.M | re.S)
    step_anchors = dict(definitions)
    assert len(step_anchors) == len(definitions), 'duplicate step anchor'
    assert not set(step_anchors).intersection(anchors), 'ambiguous step anchor'
    known_steps = set(step_anchors)
    step_anchors.pop('engineering-job-observation', None)
    for name, body in step_anchors.items():
        assert body.startswith('        '), f'invalid step anchor {name}'
        step_anchors[name] = '      - ' + body.removeprefix('        ')
    for name, job in jobs.items():
        for alias in re.findall(r'^      - \*([a-z0-9-]+)\n', job, re.M):
            assert alias in known_steps, f'undefined step alias {alias}'
        job = re.sub(r'^      - &([a-z0-9-]+)\n(.*?)(?=^      - |\Z)',
                     lambda match: step_anchors.get(match[1], match[0]), job, flags=re.M | re.S)
        jobs[name] = re.sub(r'^      - \*([a-z0-9-]+)\n',
                           lambda match: step_anchors.get(match[1], match[0]), job, flags=re.M)
    return jobs


def workflow_contract_view(workflow):
    """Use each job's actual steps for both positive contracts and mutations."""
    header = workflow.split('\njobs:\n', 1)[0]
    return header + '\njobs:\n' + ''.join(
        '  ' + name + ':\n' + body.rstrip('\n') + '\n\n'
        for name, body in workflow_jobs(workflow).items())


WORKFLOW = workflow_contract_view(RAW_WORKFLOW)
JOBS = workflow_jobs(WORKFLOW)

TRUSTED_SOURCE_WRITERS = (
    'oasis7-test-build-trusted', 'formal-debug-build-trusted',
    'formal-endpoint-build-trusted', 'full-regression',
)


def workflow_step_script(job, name):
    step = next(step for step in re.findall(r'^      - .*?(?=^      - |\Z)', job, re.M | re.S)
                if 'name: ' + name + '\n' in step)
    if '        run: |\n' in step:
        return textwrap.dedent(step.split('        run: |\n', 1)[1])
    return step.split('        run: ', 1)[1].splitlines()[0] + '\n'


def validate_mold_checkout_reuse(tools, setup, source):
    def inputs(step):
        fields = re.findall(r'^          ([a-z][a-z-]*): (.*)$', step, re.M)
        assert len(fields) == len(dict(fields)), 'duplicate checkout input'
        return dict(fields)

    def guards(step):
        return re.findall(r'^        if: (.+)$', step, re.M)

    first, selected = inputs(tools), inputs(source)
    assert set(selected) <= {'ref', 'fetch-depth', 'persist-credentials'}
    assert selected.get('persist-credentials') == 'false'
    ref = selected['ref'].removeprefix('${{').removesuffix('}}').strip()
    assert ref in ('needs.select.outputs.tested_sha', 'github.sha')
    depth = selected.get('fetch-depth', '1')
    assert depth in ('0', '1')
    different = f"{ref} == '' || github.workflow_sha != {ref}"
    sparse = "${{ (" + different + ") && '.github/actions/setup-linux-mold' || '' }}"
    first_depth = (depth if depth == '1' else "${{ " +
                   f"{ref} != '' && github.workflow_sha == {ref} && '0' || '1'" + " }}")
    assert first == {'ref': '${{ github.workflow_sha }}', 'persist-credentials': 'false',
                     'fetch-depth': first_depth, 'sparse-checkout': sparse}
    platform = guards(tools)
    assert platform == guards(setup) and platform in ([], ["runner.os == 'Linux'"])
    fallback = ("runner.os != 'Linux' || " if platform else '') + different
    assert guards(source) == ["${{ " + fallback + " }}"], 'only the identical nonempty source may skip checkout'


def selected_source_steps(name):
    """Normalize only the equivalent first checkout of admitted main writers."""
    steps = JOBS[name].split('    steps:\n', 1)[1]
    checkouts = [step for step in re.findall(r'^      - .*?(?=^      - |\Z)', steps, re.M | re.S)
                 if re.search(r'^\s+(?:- )?uses: actions/checkout@', step, re.M)]
    if 'uses: ./.github/actions/setup-linux-mold' in steps:
        assert len(checkouts) == 2, 'workflow tools and one selected checkout are required'
        assert '          ref: ${{ github.workflow_sha }}\n' in checkouts[0], 'linker tools must use the workflow revision'
        if '          sparse-checkout: .github/actions/setup-linux-mold\n' not in checkouts[0]:
            setup = next(step for step in re.findall(r'^      - .*?(?=^      - |\Z)', steps, re.M | re.S)
                         if 'uses: ./.github/actions/setup-linux-mold' in step)
            validate_mold_checkout_reuse(checkouts[0], setup, checkouts[-1])
        else:
            assert not re.findall(r'^        if: .*workflow_sha', checkouts[-1], re.M), (
                'sparse mold checkout cannot replace the selected source')
    else:
        assert len(checkouts) == 1, 'one selected checkout is required'
    if name in TRUSTED_SOURCE_WRITERS:
        assert steps.startswith(checkouts[0]), 'writer checkout must be first'
    checkout = checkouts[-1]
    refs = re.findall(r'^          ref: (.*)$', checkout, re.M)
    tested = '${{ needs.select.outputs.tested_sha }}'
    allowed = (tested, '${{ github.sha }}') if name in TRUSTED_SOURCE_WRITERS else (tested,)
    assert len(refs) == 1 and refs[0] in allowed, 'checkout must use the selected or proven identical source'
    # Do not normalize any other checkout setting, later step, or candidate ref.
    if refs[0] == '${{ github.sha }}':
        for step in checkouts:
            steps = steps.replace(step, step.replace('github.sha', 'needs.select.outputs.tested_sha'), 1)
    return steps


def effective_job_condition(condition, values, ancestor_results, *, cancelled=False):
    """Include Actions' implicit success() instead of evaluating only written YAML."""
    expression = condition.strip().removeprefix('>-').strip()
    if not re.search(r'\b(?:always|success|failure|cancelled)\s*\(', expression):
        expression = 'success() && (' + expression + ')'
    values = {
        'needs.fast-preflight.result': 'success',
        **values,
        'always()': True,
        'success()': not cancelled and all(result == 'success' for result in ancestor_results),
        'failure()': 'failure' in ancestor_results,
        'cancelled()': cancelled,
    }
    for token, value in sorted(values.items(), key=lambda item: -len(item[0])):
        expression = expression.replace(token, repr(value))
    expression = re.sub(r'!(?!=)', 'not ', expression.replace('&&', ' and ').replace('||', ' or '))
    return bool(eval(' '.join(expression.split()), {'__builtins__': {}},
                     {'contains': lambda collection, item: item in collection}))


class WorkflowCorePart10(unittest.TestCase):
    def test_runner_discovers_supported_python_when_default_is_unsupported(self):
        with tempfile.TemporaryDirectory() as temporary:
            for name in ('python', 'python3'):
                path = Path(temporary) / name
                path.write_text('#!/usr/bin/env bash\nexit 91\n')
                path.chmod(0o755)
            environment = dict(os.environ, PATH=temporary + os.pathsep + os.environ['PATH'])
            result = subprocess.run(['bash', str(ROOT / 'scripts/ci-tests.sh'), 'required',
                                     '--group', 'codex_agent_config_validation'],
                                    cwd=ROOT, env=environment, text=True, capture_output=True)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertIn('validated agent configurations:', result.stdout)

    def test_native_tool_suite_does_not_inherit_nightly_build_std(self):
        driver = (ROOT / 'scripts/ci-tests.sh').read_text()
        function = workspace_support_fixture_functions(driver)
        script = "set -euo pipefail\n" + function + """run_signed_read_protocol_contract_tests() { :; }
run() { :; }
run_cargo() { printf '%s|%s\\n' "$OASIS7_WASM_BUILD_STD" "$*"; }
run_cargo_clippy() { printf '%s|clippy %s\\n' "$OASIS7_WASM_BUILD_STD" "$*"; }
""" + "\nrun_oasis7_workspace_support_crate_tests\nprintf 'after|%s\\n' \"$OASIS7_WASM_BUILD_STD\"\n"
        result = subprocess.run(['bash', '-c', script], text=True, capture_output=True,
                                env=dict(os.environ, OASIS7_WASM_BUILD_STD='1'))
        self.assertEqual(result.returncode, 0, result.stderr)
        calls = result.stdout.splitlines()
        self.assertEqual(calls[-2], '0|test -p wasm_build_suite -p wasm_module_observe')
        self.assertTrue(all(call.startswith('1|') for call in calls[:-2]))
        self.assertEqual(calls[-1], 'after|1')

    def test_agent_contract_and_harness_checks_are_in_workspace_support(self):
        driver = (ROOT / 'scripts/ci-tests.sh').read_text()
        function = workspace_support_fixture_functions(driver)
        script = "set -euo pipefail\n" + function + """run_signed_read_protocol_contract_tests() { :; }
run() { :; }
run_cargo() { printf 'test|%s\\n' "$*"; }
run_cargo_clippy() { printf 'clippy|%s\\n' "$*"; }
""" + "\nrun_oasis7_workspace_support_crate_tests\n"
        result = subprocess.run(['bash', '-c', script], text=True, capture_output=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        commands = result.stdout.splitlines()
        self.assertIn('clippy|-p oasis7_agent_api --all-targets', commands)
        self.assertIn('clippy|-p oasis7_agent_harness --all-targets', commands)
        self.assertIn('test|test -p oasis7_agent_api', commands)
        self.assertIn('test|test -p oasis7_agent_harness --lib', commands)
        self.assertLess(commands.index('clippy|-p oasis7_agent_harness --all-targets'),
                        commands.index('test|test -p oasis7_agent_harness --lib'))

    def test_private_context_all_targets_and_caller_are_executed(self):
        driver = (ROOT / 'scripts/ci-tests.sh').read_text()
        function = workspace_support_fixture_functions(driver)
        script = "set -euo pipefail\n" + function + "run_signed_read_protocol_contract_tests() { :; }\nrun() { :; }\nrun_cargo() { printf '%s\\n' \"$*\"; }\nrun_cargo_clippy() { printf 'clippy %s\\n' \"$*\"; }\n\nrun_oasis7_workspace_support_crate_tests\n"
        result = subprocess.run(['bash', '-c', script], text=True, capture_output=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        calls = [line.split() for line in result.stdout.splitlines()]
        self.assertIn(['clippy', '-p', 'oasis7_agent_api', '--all-targets'], calls)
        self.assertIn(['clippy', '-p', 'oasis7_agent_harness', '--all-targets'], calls)
        for package in ('oasis7_context', 'oasis7_secure_envelope'):
            selected = [call for call in calls if package in call]
            self.assertTrue(selected, package)
            self.assertTrue(any('--all-targets' in call and '--locked' in call for call in selected))
            self.assertEqual(CONFIG['package_groups'][package], ['workspace_support'])
        self.assertTrue(any('oasis7_local_signer' in call and '--test' in call and 'context_cli' in call for call in calls))
        for path in ('crates/oasis7_context/**', 'crates/oasis7_secure_envelope/**'):
            selected_groups = {group for rule in CONFIG['rules'] if path in rule['match'] for group in rule['groups']}
            self.assertIn('fleet_health', selected_groups)

    def test_workspace_support_runs_real_worker_compile_helper_and_propagates_failure(self):
        driver = (ROOT / 'scripts/ci-tests.sh').read_text()
        self.assertIn('run_worker_wasm_compile_profiles()', driver)
        functions = workspace_support_fixture_functions(driver)
        script = "set -euo pipefail\n" + functions + """
declare -F run_worker_wasm_compile_profiles >/dev/null
run_signed_read_protocol_contract_tests() { :; }
run_game_client_cli_contracts() { :; }
run_cargo() { printf 'cargo %s\\n' "$*" >> "$SUPPORT_CAPTURE"; }
run_cargo_clippy() { printf 'clippy %s\\n' "$*" >> "$SUPPORT_CAPTURE"; }
run() { printf 'run %s\\n' "$*" >> "$SUPPORT_CAPTURE"; if [[ "$*" == *"verify.sh"* ]]; then return "${VERIFY_RESULT:-0}"; fi; }
worker-build() { echo "0.7.5"; }
""" + "\nrun_oasis7_workspace_support_crate_tests\n"
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            capture = root / 'support-capture'
            environment = dict(os.environ, SUPPORT_CAPTURE=str(capture), VERIFY_RESULT='0')
            result = subprocess.run(['bash', '-c', script], cwd=root, env=environment,
                                    text=True, capture_output=True)
            self.assertEqual(result.returncode, 0, result.stderr)
            lines = capture.read_text().splitlines()
            self.assertIn('run bash crates/oasis7_dev_test_worker/scripts/verify.sh', lines)
            self.assertIn('cargo test -p oasis7_custody_core --all-targets --locked', lines)
            self.assertFalse(any('oasis7_custody_worker' in line for line in lines))
            capture.unlink()
            environment['VERIFY_RESULT'] = '42'
            failed = subprocess.run(['bash', '-c', script], cwd=root, env=environment,
                                    text=True, capture_output=True)
            self.assertEqual(failed.returncode, 42)
            self.assertEqual(capture.read_text().splitlines(),
                             ['run bash crates/oasis7_dev_test_worker/scripts/verify.sh'])

    def test_private_context_macos_uses_fixed_toolchain_and_declared_resource(self):
        fleet = JOBS['fleet-health']
        self.assertIn('rust_toolchain', CONFIG['resources']['fleet_health'])
        prepare = fleet.index('name: Prepare native filesystem contract Rust toolchain')
        verify = fleet.index('name: Verify private context filesystem and caller on macOS')
        self.assertLess(prepare, verify)
        hosted = fleet.index('name: Verify hosted access cross-process ownership on native host')
        self.assertLess(prepare, hosted)
        self.assertLess(hosted, verify)
        preparation = fleet[prepare:hosted]
        prep_cond = re.search(r'^        if: (.*?)(?=^        [a-z-]+:|\Z)', preparation, re.M | re.S).group(1)
        for runner_os in ('Linux', 'Windows', 'macOS'):
            self.assertTrue(effective_job_condition(prep_cond, {
                'runner.os': runner_os,
                'needs.select.outputs.run_fleet_health': 'true',
                'fromJSON(needs.select.outputs.plan).scope': 'targeted',
                'fromJSON(needs.select.outputs.plan).contract_job_consolidation_version': 5,
                'fromJSON(needs.select.outputs.plan).fleet_health_subcontracts_version': 1,
                'fromJSON(needs.select.outputs.plan).fleet_health_checks': ['hosted_access'],
            }, ('success',)), f'hosted_access must prepare Rust toolchain on {runner_os}')
        self.assertIn("needs.select.outputs.run_fleet_health == 'true'", preparation)
        hosted_validation = fleet[hosted:verify]
        self.assertNotIn('runner.os', hosted_validation)
        self.assertIn("needs.select.outputs.run_fleet_health == 'true'", hosted_validation)
        self.assertIn('cargo test --locked -p oasis7_hosted_access', hosted_validation)
        self.assertIn('fleet_health', CONFIG['package_groups']['oasis7_hosted_access'])
        self.assertIn('rustup toolchain install "$RUST_TOOLCHAIN"', preparation)
        self.assertIn('rustup default "$RUST_TOOLCHAIN"', preparation)
        self.assertTrue((ROOT / 'rust-toolchain.toml').is_file())
        validation = fleet[verify:fleet.index('name: Ensure Linux process inspection dependency')]
        self.assertRegex(validation, r"if:\s*(?:>-\s*)?runner\.os == 'macOS'")
        self.assertIn('cargo test --locked -p oasis7_context -p oasis7_secure_envelope --all-targets', validation)
        self.assertIn('cargo test --locked -p oasis7_local_signer --test context_cli', validation)

    def test_custody_approval_step_initializes_real_driver_and_propagates_compile_failure(self):
        scripts_by_component = {
            'wasm-approval': workflow_step_script(JOBS['custody-wasm-production'], 'Build and verify the complete custody approval WASM component'),
            'wasm-contracts': workflow_step_script(JOBS['custody-native-contracts'], 'Execute complete custody WASM compile proofs'),
        }
        self.assertEqual(*(script.split('bash crates/', 1)[1].rstrip() for script in scripts_by_component.values()))
        features = ('identity', 'owner', 'executor', 'recovery-fence',
                    'owner,approval-harness', 'identity,native-runtime-harness')
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            scripts = root / 'scripts'
            scripts.mkdir()
            for name in ('ci-tests.sh', 'find-python-with-module.sh', 'plan-rust-required-scope.py',
                         'ci-custody-platform.py', 'ci-custody-workerd-capture.py', 'ci_process_runner.py'):
                (scripts / name).write_bytes((ROOT / 'scripts' / name).read_bytes())
            if (ROOT / 'scripts/run-ci-authority-python.py').is_file():
                (scripts / 'run-ci-authority-python.py').write_bytes(
                    (ROOT / 'scripts/run-ci-authority-python.py').read_bytes())
            if (ROOT / 'scripts/ci_engine').is_dir():
                for ef in sorted((ROOT / 'scripts/ci_engine').rglob('*.py')):
                    dst = root / ef.relative_to(ROOT)
                    dst.parent.mkdir(parents=True, exist_ok=True)
                    dst.write_bytes(ef.read_bytes())
            worker = root / 'crates/oasis7_custody_worker'
            (worker / 'scripts').mkdir(parents=True)
            names = sorted({name for profile in features for name in profile.split(',')})
            (worker / 'Cargo.toml').write_text('[features]\ndefault = []\n'
                + ''.join(name + ' = []\n' for name in names))
            (worker / 'tests').mkdir()
            (worker / 'tests/fixture.test.mjs').write_text('// complete inventory fixture\n')
            (root / 'rust-toolchain.toml').write_text('[toolchain]\nchannel="1.96.0"\n')
            (worker / 'scripts/verify.sh').write_text(
                '#!/bin/bash\nprintf "%s\\n" "$*" >> "$VERIFY_CAPTURE"\n'
                '[[ "$*" == "--stage $EXPECTED_STAGE" ]] || exit 2\n'
                'exit "${VERIFY_RESULT:-0}"\n')
            binary = root / 'bin'
            binary.mkdir()
            capture = root / 'commands'
            cargo = binary / 'cargo'
            cargo.write_text('#!/bin/sh\nprintf \'%s\\n\' "$*" >> "$WORKER_COMPILE_CAPTURE"\n'
                '[ "$WORKER_COMPILE_RESULT" = 0 ] || exit "$WORKER_COMPILE_RESULT"\n'
                'case "$*" in *"--features executor,execution-signer-harness "*) ;;\n'
                '*execution-signer-harness*) echo "execution-signer-harness requires executor and is not a deployment profile"; exit 1;; esac\n')
            cargo.chmod(0o755)
            def git(*args):
                return subprocess.check_output(['git', '-C', str(root), *args], text=True).strip()
            git('init', '-q')
            git('add', '.')
            git('-c', 'user.name=CI fixture', '-c', 'user.email=fixture@example.invalid', 'commit', '-qm', 'stage fixture')
            verify_capture = root / 'verify-commands'
            plan = dict(source_sha='a' * 40, tested_sha=git('rev-parse', 'HEAD'), policy_sha='b' * 40,
                groups=['workspace_support'], custody_execution_version=1, custody_node_shards=[0, 1, 2, 3],
                workspace_support_profile_version=1, workspace_support_profiles=['oasis7_custody_worker'],
                workspace_support_partition_version=1, workspace_support_partitions=['custody'], test_policy_changed=False)
            selection = root / 'selection.json'
            environment = dict(os.environ, PATH=str(binary) + os.pathsep + os.environ['PATH'],
                               GITHUB_WORKSPACE=str(root), RUNNER_TEMP=str(root), GITHUB_RUN_ID='71', GITHUB_RUN_ATTEMPT='1',
                               VERIFY_CAPTURE=str(verify_capture),
                               **{'OASIS7_SUPPORT_' + key.upper(): plan[key] for key in ('source_sha', 'tested_sha', 'policy_sha')},
                               WORKER_COMPILE_CAPTURE=str(capture), WORKER_COMPILE_RESULT='0')
            current_helper = (scripts / 'ci-custody-platform.py').read_text()
            def invoke(extra=None):
                capture.unlink(missing_ok=True)
                verify_capture.unlink(missing_ok=True)
                script = (
                    'set -euo pipefail\n'
                    'run_worker_wasm_compile_profiles() { printf "compile-profiles\\n" >> "$WORKER_COMPILE_CAPTURE"; return "${WORKER_COMPILE_RESULT:-0}"; }\n'
                    + scripts_by_component[environment['CUSTODY_COMPONENT']]
                )
                return subprocess.run(['bash', '-c', script], cwd=root, env=environment | (extra or {}),
                                      text=True, capture_output=True)
            for signer in (False, True):
                if signer:
                    with (worker / 'Cargo.toml').open('a') as manifest:
                        manifest.write('execution-signer-harness = []\n')
                    git('add', 'crates/oasis7_custody_worker/Cargo.toml')
                    git('-c', 'user.name=CI fixture', '-c', 'user.email=fixture@example.invalid', 'commit', '-qm', 'signer profile')
                    plan['tested_sha'] = git('rev-parse', 'HEAD')
                    environment['OASIS7_SUPPORT_TESTED_SHA'] = plan['tested_sha']
                for proof, component, stage in [(False, 'wasm-approval', 'wasm-approval'),
                                                (True, 'wasm-approval', 'wasm-approval-artifacts'),
                                                (True, 'wasm-contracts', 'wasm-contracts')]:
                    with self.subTest(signer=signer, proof=proof, component=component):
                        current = plan | ({'custody_compile_proof_version': 1} if proof else {})
                        selection.write_text(json.dumps(current))
                        environment.update(CUSTODY_COMPONENT=component, EXPECTED_STAGE=stage,
                                           CUSTODY_COMPILE_PROOF_VERSION='1' if proof else '',
                                           CUSTODY_SELECTION_SHA256=hashlib.sha256(selection.read_bytes()).hexdigest())
                        # The old source's helper has no stage CLI. Its complete
                        # approval path must not try that optional new interface.
                        (scripts / 'ci-custody-platform.py').write_text(current_helper if proof else
                            "raise SystemExit('legacy helper does not support stage')\n")
                        result = invoke()
                        self.assertEqual(result.returncode, 0, result.stderr)
                        self.assertEqual(verify_capture.read_text().splitlines(), ['--stage ' + stage])
                        actual = capture.read_text().splitlines() if capture.exists() else []
                        self.assertEqual(actual, [] if stage == 'wasm-approval-artifacts' else ['compile-profiles'])
                        failed = invoke({'VERIFY_RESULT': '37'})
                        self.assertEqual(failed.returncode, 37)
                        self.assertFalse(capture.exists())
                        if stage != 'wasm-approval-artifacts':
                            failed = invoke({'WORKER_COMPILE_RESULT': '42'})
                            self.assertEqual(failed.returncode, 42)
                            self.assertEqual(capture.read_text().splitlines(), ['compile-profiles'])
                        invalid_contexts = ({'CUSTODY_SELECTION_SHA256': '0' * 64},
                                            {'OASIS7_SUPPORT_POLICY_SHA': 'c' * 40}) if proof else ()
                        for invalid in invalid_contexts:
                            failed = invoke(invalid)
                            self.assertNotEqual(failed.returncode, 0)
                            self.assertFalse(verify_capture.exists())
                            self.assertFalse(capture.exists())

    def test_concurrency_is_per_pr_and_per_non_pr_run(self):
        self.assertIn("format('pr-{0}', github.event.pull_request.number)", WORKFLOW)
        self.assertIn("format('push-{0}', github.ref)", WORKFLOW)
        self.assertIn("format('run-{0}', github.run_id)", WORKFLOW)
        concurrency = WORKFLOW.split('concurrency:', 1)[1].split('permissions:', 1)[0]
        push_group = "github.event_name == 'push' && format('push-{0}', github.ref)"
        self.assertIn(push_group, concurrency)
        self.assertIn("cancel-in-progress: ${{ github.event_name == 'pull_request' }}", concurrency)
        self.assertNotIn("github.event_name == 'push'", concurrency.split('cancel-in-progress:', 1)[1])
        self.assertIn("format('periodic-observe-{0}', github.run_id)", concurrency)
        self.assertIn('group: rust-periodic-admission-main', JOBS['periodic-admission'])
        self.assertIn('cancel-in-progress: false', JOBS['periodic-admission'])
        self.assertIn("format('candidate-full-{0}-{1}'", concurrency)
        self.assertIn('cron: "17,37,57 * * * *"', WORKFLOW)
        self.assertIn('github.workflow', concurrency)

    def test_main_fast_cache_writer_preserves_identity_inline_and_recovery_boundaries(self):
        prefix = 'fromJSON(needs.select.outputs.plan).'
        ordinary = ('oasis7-test-build-candidate', 'oasis7-test-build-trusted')
        formal = ('formal-debug-build-trusted', 'formal-endpoint-build-trusted')
        names = (*ordinary, *formal, 'cache-bootstrap-maintenance', 'cache-maintenance')
        conditions = {name: re.search(r'^    if: (.*?)(?=^    [a-z-]+:|\Z)',
                                      JOBS[name], re.M | re.S)[1] for name in names}
        defaults = {
            'needs.select.result': 'success', 'needs.select.outputs.run_oasis7_required': 'true',
            'needs.select.outputs.run_formal_process': 'true', 'needs.select.outputs.trusted_source': 'true',
            prefix + 'scope': 'full', prefix + 'execution_profile': 'main-fast',
            prefix + 'fast_plan_version': 1, prefix + 'ordinary_main_fast_cache_writer_version': 1,
            prefix + 'fast_execution_version': 0, prefix + 'ordinary_execution': '',
            'github.event_name': 'push', 'github.ref': 'refs/heads/main',
            'github.workflow_sha': 'a', 'github.sha': 'a', 'inputs.run_mode': 'full',
        }
        cases = (
            ({}, True),
            ({prefix + 'ordinary_main_fast_cache_writer_version': 0}, False),
            ({prefix + 'ordinary_main_fast_cache_writer_version': 2}, False),
            ({prefix + 'scope': 'targeted'}, False),
            ({prefix + 'execution_profile': 'pr-fast'}, False),
            ({'github.event_name': 'pull_request', 'github.ref': 'refs/pull/1/merge'}, False),
            ({'github.event_name': 'schedule'}, False),
            ({'github.event_name': 'workflow_dispatch'}, False),
            ({'github.ref': 'refs/heads/candidate'}, False),
            ({'github.workflow_sha': 'b'}, False),
            ({'needs.select.outputs.trusted_source': 'false'}, False),
            ({prefix + 'fast_execution_version': 1, prefix + 'ordinary_execution': 'inline'}, False),
        )
        for changes, admitted in cases:
            for select, cancelled in itertools.product(('success', 'failure', 'skipped', 'cancelled'), (False, True)):
                values = defaults | changes | {'needs.select.result': select}
                inline = values[prefix + 'fast_execution_version'] == 1 and values[prefix + 'ordinary_execution'] == 'inline'
                active = select == 'success' and not cancelled
                writer = active and admitted
                with self.subTest(changes=changes, select=select, cancelled=cancelled):
                    for name, expected in ((ordinary[0], active and not inline and not admitted),
                                           (ordinary[1], writer), ('cache-bootstrap-maintenance', writer),
                                           *[(name, False) for name in formal]):
                        self.assertEqual(effective_job_condition(conditions[name], values,
                                                                ['skipped', select], cancelled=cancelled), expected, name)
                    # In a fast profile, a Formal result must never admit
                    # recovery in place of the selected ordinary writer.
                    for ordinary_result, formal_result in itertools.product(
                            ('success', 'failure', 'cancelled', 'skipped', '', 'unknown'), ('success', 'skipped')):
                        completion = values | {
                            'needs.oasis7-test-build-trusted.result': ordinary_result if active else 'skipped',
                            'needs.formal-debug-build-trusted.result': formal_result,
                            'needs.formal-endpoint-build-trusted.result': formal_result,
                        }
                        self.assertEqual(effective_job_condition(conditions['cache-maintenance'], completion,
                            ['skipped', ordinary_result, formal_result], cancelled=cancelled),
                            writer and ordinary_result == 'success')

    def test_selected_jobs_consume_fixed_source_and_same_plan(self):
        for name in ('baseline', 'oasis7-test-build-candidate', 'formal-test-build', 'formal-test-build-trusted',
                     'formal-debug-build', 'formal-debug-build-trusted',
                     'formal-endpoint-build', 'formal-endpoint-build-trusted', 'oasis7-required',
                     'formal-process-tests', 'workspace-support', 'full-regression', 'candidate-full-regression',
                     'windows-rollout', 'fleet-health'):
            if name == 'windows-rollout' and name not in JOBS:
                continue
            with self.subTest(job=name):
                self.assertIn('ref: ${{ needs.select.outputs.tested_sha }}', selected_source_steps(name))
        for name in ('oasis7-test-build-candidate', 'formal-test-build', 'formal-test-build-trusted',
                     'formal-debug-build', 'formal-debug-build-trusted',
                     'formal-endpoint-build', 'formal-endpoint-build-trusted', 'oasis7-required', 'formal-process-tests'):
            with self.subTest(plan_consumer=name):
                self.assertIn('name: ${{ needs.select.outputs.selection_artifact }}', JOBS[name])
                self.assertIn('--plan "$RUNNER_TEMP/selection.json"', JOBS[name])
        for name in ('formal-test-build', 'formal-test-build-trusted',
                     'formal-debug-build', 'formal-debug-build-trusted',
                     'formal-endpoint-build', 'formal-endpoint-build-trusted', 'formal-process-tests'):
            with self.subTest(formal_policy_consumer=name):
                self.assertIn('name: ${{ needs.select.outputs.authority_artifact }}', JOBS[name])
                commands = [line for line in JOBS[name].splitlines()
                            if re.search(r'ci-test-artifacts.py (?:build|pack|run|merge-formal) ', line)]
                self.assertTrue(commands)
                for command in commands:
                    self.assertIn('--formal-config "$RUNNER_TEMP/ci-authority/ci-formal-process-suites.json"', command)
        for name in ('full-regression', 'candidate-full-regression'):
            if name == 'full-regression' and 'scripts/ci-trusted-full-regression.py full "$SOURCE_SHA"' in JOBS[name]:
                self.assertIn("['./scripts/ci-tests.sh', 'full-extended']",
                              (ROOT / 'scripts/ci-trusted-full-regression.py').read_text())
            else:
                self.assertIn('./scripts/ci-tests.sh full-extended', JOBS[name])
            self.assertNotIn('full-extended --plan', JOBS[name])
            self.assertNotIn('OASIS7_CI_TEST_BUNDLE', JOBS[name])
            self.assertNotIn('name: native-tests-', JOBS[name])
        center_path = ROOT / '.github/workflows/command-center.yml'
        if center_path.is_file():
            center = center_path.read_text()
            self.assertIn('ref: ${{ inputs.source_sha }}', center)
            self.assertIn('package_app: ${{ needs.select.outputs.command_center_package', JOBS['command-center'])
            self.assertIn('- name: Package development App\n        if: inputs.package_app', center)
            host = center.split('      - name: Check desktop host\n', 1)[1].split('      - name:', 1)[0]
            self.assertNotIn('if:', host)
            self.assertIn('--workspace --all-targets', host)
            self.assertIn('cancel-in-progress: false', center)

    def test_token_regression_is_witnessed_in_governance_and_full_only(self):
        entry = 'scripts/main-token-regression.sh'
        driver = (ROOT / 'scripts/ci-tests.sh').read_text()
        dispatch = re.search(r'^case "\$tier:\$group" in\n.*?^esac$', driver, re.M | re.S)
        self.assertIsNotNone(dispatch)
        self.assertEqual(driver.count('run bash "$repo_root/' + entry + '" full'), 1)
        self.assertLess(dispatch.end(), driver.index('if [[ -n "$group" ]]; then\n  [[ "$tier"'))
        self.assertIn('./scripts/ci-tests.sh full-extended', JOBS['candidate-full-regression'])
        if 'scripts/ci-trusted-full-regression.py full "$SOURCE_SHA"' in JOBS['full-regression']:
            self.assertIn("['./scripts/ci-tests.sh', 'full-extended']",
                          (ROOT / 'scripts/ci-trusted-full-regression.py').read_text())
        else:
            self.assertIn('./scripts/ci-tests.sh full-extended', JOBS['full-regression'])
        mapping = next(rule for rule in CONFIG['rules'] if entry in rule['match'])
        self.assertIn('workflow_governance', mapping['groups'])
        self.assertIn('scripts/ci-local-check.test.py', CONFIG['workflow_governance_python_tests'])
        for tier, group, status, selected in (
                ('full', '', 0, True), ('full-standard', '', 0, True), ('full-extended', '', 0, True),
                ('full-extended', '', 7, True), ('required', '', 7, False),
                ('required', 'workflow_governance', 7, False),
                ('full-core', '', 7, False), ('full-support', '', 7, False),
                ('full-extended', 'workflow_governance', 7, False)):
            with self.subTest(tier=tier, group=group, status=status), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                fixture = root / entry
                fixture.parent.mkdir()
                fixture.write_text('printf "token:%s\\n" "$*"\n'
                                   'exit "$TOKEN_FIXTURE_STATUS"\n')
                command = ('repo_root=$1\ntier=$2\ngroup=$3\nrun() { "$@"; }\n'
                           + dispatch.group() + "\necho 'remaining full checks'\n")
                observed = subprocess.run(
                    ['bash', '-euo', 'pipefail', '-c', command, 'full-fixture', str(root), tier, group],
                    cwd=ROOT, env=dict(os.environ, TOKEN_FIXTURE_STATUS=str(status)),
                    capture_output=True, text=True, timeout=15)
                expected = status if selected else 0
                self.assertEqual(observed.returncode, expected, observed.stdout + observed.stderr)
                self.assertEqual(observed.stdout.splitlines().count('token:full'), int(selected))
                self.assertEqual('remaining full checks' in observed.stdout, expected == 0)

    def test_clippy_precedes_heavy_group_tests(self):
        driver = (ROOT / 'scripts/ci-tests.sh').read_text()
        ordinary = driver.split('    oasis7_required)\n', 1)[1].split('    formal_process)', 1)[0]
        self.assertLess(ordinary.index('run_oasis7_required_clippy_for_group oasis7_required'),
                        ordinary.index('run_oasis7_required_tier_tests'))
        consumer = ordinary.split('        *)', 1)[0]
        self.assertIn('--ordinary-partition "$ordinary_partition"', consumer)
        self.assertNotIn('run_cargo', consumer)
        self.assertIn('Complete ordinary partition requires the shared artifact', consumer)
        baseline = driver.split('run_rust_baseline() {', 1)[1].split('\n}', 1)[0]
        self.assertIn('run_oasis7_required_clippy_for_group rust_baseline', baseline)
        for group, prefix in [('consensus', 'consensus'), ('distfs', 'distfs'),
                              ('node', 'node'), ('net', 'net')]:
            case = re.search(r'^    ' + group + r'\).*?;;', driver, re.M).group()
            self.assertLess(case.index('run_oasis7_' + prefix + '_clippy'),
                            case.index('run_oasis7_' + prefix + '_tests'))


if __name__ == '__main__':
    unittest.main()
