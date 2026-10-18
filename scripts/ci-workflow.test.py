#!/usr/bin/env python3
"""Regress the final DAG and execution boundaries, without Actions credentials."""
import json
import os
import subprocess
import sys
import tempfile
import textwrap
from pathlib import Path
import re
import unittest

ROOT = Path(__file__).resolve().parent.parent
WORKFLOW = (ROOT / '.github/workflows/rust.yml').read_text()
CONFIG = json.loads((ROOT / 'scripts/ci-required-scope.json').read_text())
def workflow_jobs(workflow):
    jobs = dict(re.findall(r'^  ([a-z][a-z0-9-]*):\n(.*?)(?=^  [a-z][a-z0-9-]*:|\Z)',
                          workflow.split('\njobs:\n', 1)[1], re.M | re.S))
    anchors = {}
    for job in jobs.values():
        match = re.search(r'^    steps: &([a-z0-9-]+)\n(.*)', job, re.M | re.S)
        if match:
            anchors[match[1]] = match[2]
    return {name: re.sub(r'^    steps: &[a-z0-9-]+\n', '    steps:\n',
                        re.sub(r'^    steps: \*([a-z0-9-]+)\n',
                               lambda match: '    steps:\n' + anchors[match[1]], job, flags=re.M), flags=re.M)
            for name, job in jobs.items()}


JOBS = workflow_jobs(WORKFLOW)


def validate_candidate_graph(workflow, config):
    jobs = dict(re.findall(r'^  ([a-z][a-z0-9-]*):\n(.*?)(?=^  [a-z][a-z0-9-]*:|\Z)',
                           workflow.split('\njobs:\n', 1)[1], re.M | re.S))
    gate = jobs['required-gate']
    dependencies = re.search(r'    needs: \[(.*)\]', gate).group(1).split(', ')
    expected = {'select', *(g.replace('_', '-') for g in config['groups'])}
    if set(dependencies) != expected or len(dependencies) != len(expected):
        raise ValueError('gate dependencies must exactly cover candidate groups')
    if 'always()' not in gate or 'continue-on-error:' in workflow:
        raise ValueError('failure propagation must remain intact')
    fleet = jobs['fleet-health']
    if ('os: [ubuntu-24.04, windows-2022, macos-15]' not in fleet
            or 'runs-on: ${{ matrix.os }}' not in fleet
            or 'fail-fast: false' not in fleet or 'include:' in fleet or 'exclude:' in fleet
            or "if: runner.os != 'Windows'" not in fleet or '--host-smoke' not in fleet):
        raise ValueError('fleet must run all three hosts and POSIX cleanup smoke')
    for group in config['groups']:
        job = jobs[group.replace('_', '-')]
        if group == 'formal_process':
            if 'always()' not in job or 'ci-formal-plan.py --selected' not in job:
                raise ValueError('formal aggregation must execute and validate results')
            continue
        if ("needs.select.result == 'success'" not in job
                or "fromJSON(needs.select.outputs.plan).scope == 'full'" not in job):
            raise ValueError('group requires successful select and full migration fallback')
        if group != 'fleet_health' and 'strategy:' in job:
            raise ValueError('ordinary groups must not have singleton matrices')
        if group == 'command_center' and 'uses: ./.github/workflows/command-center.yml' not in job:
            raise ValueError('Command Center checks must execute through the reusable workflow')
        if 'ci-authority' in job:
            raise ValueError('execution must use candidate checkout')
    return True


class Workflow(unittest.TestCase):
    def test_manual_and_scheduled_selection_stays_explicit_full(self):
        self.assertIn("if: github.event_name != 'workflow_dispatch' || inputs.run_mode == 'full'",
                      JOBS['select'])
        select_step = JOBS['select'].split('      - id: select\n', 1)[1].split(
            '      - id: formal-select\n', 1)[0]
        select_script = textwrap.dedent(select_step.split('        run: |\n', 1)[1])
        formal_step = JOBS['select'].split('      - id: formal-select\n', 1)[1].split(
            '      - uses:', 1)[0]
        formal_script = textwrap.dedent(formal_step.split('        run: |\n', 1)[1])
        for event in ('workflow_dispatch', 'schedule'):
            with self.subTest(event=event), tempfile.TemporaryDirectory() as directory:
                output = Path(directory) / 'github-output'
                result = subprocess.run(
                    ['bash', '-c', select_script],
                    cwd=ROOT,
                    env=dict(os.environ, EVENT_NAME=event, GITHUB_OUTPUT=str(output)),
                    text=True,
                    capture_output=True,
                )
                self.assertEqual(result.returncode, 0, result.stderr)
                plan_line = next(line for line in output.read_text().splitlines()
                                 if line.startswith('plan='))
                plan = json.loads(plan_line.removeprefix('plan='))
                self.assertEqual(plan, {'scope': 'explicit-full', 'groups': []})
                output.write_text('')
                formal_result = subprocess.run(
                    ['bash', '-c', formal_script],
                    cwd=ROOT,
                    env=dict(os.environ, EVENT_NAME=event, GITHUB_OUTPUT=str(output)),
                    text=True,
                    capture_output=True,
                )
                self.assertEqual(formal_result.returncode, 0, formal_result.stderr)
                self.assertIn('run_formal_process=true', output.read_text().splitlines())

        # Explicit-full builds the shared bundle without activating the
        # baseline job's PR/push-only range resolver.
        self.assertIn("fromJSON(needs.select.outputs.plan).scope == 'explicit-full'",
                      JOBS['oasis7-test-build'])
        self.assertNotIn("fromJSON(needs.select.outputs.plan).scope == 'explicit-full'",
                         JOBS['baseline'])
        self.assertIn("fromJSON(needs.select.outputs.plan).scope == 'full'", JOBS['baseline'])

    def test_explicit_full_gate_requires_event_specific_completion(self):
        gate = JOBS['explicit-full-gate']
        condition = gate.split('    runs-on:', 1)[0]
        self.assertIn('always()', condition)
        self.assertIn("github.event_name == 'schedule'", condition)
        self.assertIn("github.event_name == 'workflow_dispatch' && inputs.run_mode == 'full'", condition)
        self.assertNotIn("inputs.run_mode == 'newapi_bridge_package'", condition)
        self.assertIn('needs: [select, oasis7-test-build, formal-process, full-regression, candidate-full-regression]', gate)
        script = textwrap.dedent(gate.split('        run: |\n', 1)[1])

        def run_gate(event, ref, run_mode, overrides=None):
            values = {
                'EVENT_NAME': event,
                'REF': ref,
                'RUN_MODE': run_mode,
                'PLAN': '{"scope":"explicit-full","groups":[]}',
                'SELECT_RESULT': 'success',
                'TEST_BUILD_RESULT': 'success',
                'FORMAL_RESULT': 'success',
                'FULL_RESULT': 'skipped',
                'CANDIDATE_RESULT': 'skipped',
            }
            values.update(overrides or {})
            return subprocess.run(['bash', '-c', script], text=True, capture_output=True,
                                  env=dict(os.environ, **values))

        valid_runs = (
            ('schedule', 'refs/heads/main', '', {'FULL_RESULT': 'success'}),
            ('workflow_dispatch', 'refs/heads/main', 'full', {'FULL_RESULT': 'success'}),
            ('workflow_dispatch', 'refs/heads/feature', 'full', {'CANDIDATE_RESULT': 'success'}),
        )
        for event, ref, run_mode, overrides in valid_runs:
            with self.subTest(event=event, ref=ref):
                result = run_gate(event, ref, run_mode, overrides)
                self.assertEqual(result.returncode, 0, result.stderr)

        for variable in ('SELECT_RESULT', 'TEST_BUILD_RESULT', 'FORMAL_RESULT', 'FULL_RESULT'):
            for state in ('', 'skipped', 'failure', 'cancelled'):
                with self.subTest(variable=variable, state=state):
                    result = run_gate('schedule', 'refs/heads/main', '',
                                      {'FULL_RESULT': 'success', variable: state})
                    self.assertNotEqual(result.returncode, 0)

        for state in ('', 'success', 'failure', 'cancelled'):
            with self.subTest(unexpected_candidate_state=state):
                result = run_gate('schedule', 'refs/heads/main', '',
                                  {'FULL_RESULT': 'success', 'CANDIDATE_RESULT': state})
                self.assertNotEqual(result.returncode, 0)
        for state in ('', 'skipped', 'failure', 'cancelled'):
            with self.subTest(missing_or_failed_candidate_state=state):
                result = run_gate('workflow_dispatch', 'refs/heads/feature', 'full',
                                  {'CANDIDATE_RESULT': state})
                self.assertNotEqual(result.returncode, 0)
        for state in ('', 'success', 'failure', 'cancelled'):
            with self.subTest(unexpected_main_state=state):
                result = run_gate('workflow_dispatch', 'refs/heads/feature', 'full',
                                  {'CANDIDATE_RESULT': 'success', 'FULL_RESULT': state})
                self.assertNotEqual(result.returncode, 0)

        nonfull = run_gate('workflow_dispatch', 'refs/heads/main', 'newapi_bridge_package',
                           {'FULL_RESULT': 'success'})
        self.assertNotEqual(nonfull.returncode, 0)

    def test_command_center_is_reusable_and_required(self):
        workflow = (ROOT / '.github/workflows/command-center.yml').read_text()
        self.assertIn('  workflow_call:', workflow)
        self.assertNotIn('  pull_request:', workflow)
        self.assertNotIn('  push:', workflow)
        self.assertIn('uses: ./.github/workflows/command-center.yml', JOBS['command-center'])
        self.assertIn('run_command_center: ${{ steps.select.outputs.run_command_center }}', JOBS['select'])
        for command in ('cargo test --locked', '--example export_types', 'npm test',
                        'npm run build', 'cargo clippy --locked', 'desktop:build -- --debug'):
            self.assertIn(command, workflow)
        self.assertNotIn('continue-on-error:', workflow)
        for package in ('oasis7_context', 'oasis7_local_signer', 'oasis7_secure_envelope'):
            self.assertIn('command_center', CONFIG['package_groups'][package])

    def test_corpus_data_uses_baseline_and_implementation_uses_platforms(self):
        workflow = (ROOT / '.github/workflows/document-corpus-platform.yml').read_text()
        for path in ('doc/.governance/document-corpus-inventory.json',
                     'doc/.governance/document-corpus/**', 'doc/testing/evidence/inventory.json'):
            self.assertNotIn('      - ' + path + '\n', workflow)
        for path in ('scripts/document_corpus.py', 'scripts/document_evidence_policy.py',
                     'scripts/document-corpus-inventory*', 'scripts/doc-evidence-inventory-check*',
                     'scripts/fixtures/document-corpus-v3/**', '.gitattributes', '.gitignore'):
            self.assertEqual(workflow.count('      - ' + path + '\n'), 2)
        for os in ('ubuntu-24.04', 'macos-latest', 'windows-latest'):
            self.assertIn('- ' + os, workflow)
        driver = (ROOT / 'scripts/ci-tests.sh').read_text()
        self.assertIn('baseline) run_required_gate_checks', driver)
        baseline = driver.split('run_required_gate_checks() {', 1)[1].split('\n}', 1)[0]
        self.assertIn('run_product_doc_governance_check', baseline)
        governance = (ROOT / 'scripts/doc-governance-check.sh').read_text()
        self.assertIn('scripts/document-corpus-inventory-check.py', governance)
        checker = (ROOT / 'scripts/document-corpus-inventory-check.py').read_text()
        self.assertIn('dc.validate_evidence(view, model)', checker)
        self.assertIn('evidence_policy.validate_evidence_policy(view, model, baseline)', checker)

    def test_formal_suites_share_build_and_propagate_all_failures(self):
        matrix = JOBS['formal-process-tests']
        self.assertIn('needs: [select, formal-test-build, formal-test-build-trusted]', matrix)
        self.assertNotIn('needs: oasis7-required', matrix)
        self.assertIn('fail-fast: false', matrix)
        self.assertIn('max-parallel: 5', matrix)
        self.assertIn('shard: [runtime-protocol, world-history, healthy-set, storage-recovery, lifecycle-cli]', matrix)
        self.assertIn('actions/download-artifact@', matrix)
        self.assertNotIn('cargo build', matrix)
        self.assertNotIn('cargo test', matrix)
        self.assertNotIn('rust-cache@', matrix)
        self.assertIn('--results output/formal-process/results.jsonl', matrix)
        self.assertIn('needs: [select, formal-test-build, formal-test-build-trusted, formal-process-tests]', JOBS['formal-process'])
        for name in ['full-regression', 'candidate-full-regression']:
            self.assertIn('./scripts/ci-tests.sh full-standard', JOBS[name])
            self.assertIn('needs: oasis7-test-build', JOBS[name])
        self.assertIn('compression-level: 0', JOBS['oasis7-test-build'])
        self.assertIn('overwrite: true', JOBS['oasis7-test-build'])
        self.assertIn('--scope ordinary', JOBS['oasis7-test-build'])
        self.assertNotIn('run_formal_process', JOBS['oasis7-test-build'])
        self.assertIn('--scope formal', JOBS['formal-test-build'])
        self.assertIn('--split-formal', JOBS['formal-test-build'])
        self.assertIn('formal-tests-${{ matrix.shard }}-${{ github.sha }}', matrix)
        self.assertIn('--shard "$SHARD"', matrix)
        driver = (ROOT / 'scripts/ci-tests.sh').read_text()
        required_body = driver.split('run_oasis7_required_tier_tests() {', 1)[1].split('run_scenario_regression_tests()', 1)[0]
        self.assertNotIn('formal-live-local-tests.sh', required_body)
        self.assertIn('if [[ "$tier" == full ]]; then run_group formal_process; fi', driver)

    def test_formal_workspace_cache_has_exclusive_trust_and_a_size_budget(self):
        reader, writer = JOBS['formal-test-build'], JOBS['formal-test-build-trusted']
        self.assertIn('cache-mode: read', reader)
        self.assertNotIn('cache-mode: write', reader)
        self.assertIn('cache-mode: write', writer)
        for text in ('refs/heads/main', "github.event_name == 'push'", "github.event_name == 'schedule'",
                     "github.event_name == 'workflow_dispatch'", "inputs.run_mode == 'full'"):
            self.assertIn(text, writer.split('    steps:', 1)[0])
            self.assertIn(text, reader.split('    steps:', 1)[0])
        self.assertIn('!(', reader.split('    steps:', 1)[0])
        self.assertEqual(reader.split('    steps:', 1)[1].strip(), writer.split('    steps:', 1)[1].strip())
        for job in (reader, writer):
            self.assertIn('actions/cache/restore@55cc8345863c7cc4c66a329aec7e433d2d1c52a9', job)
            self.assertIn('actions/cache/save@55cc8345863c7cc4c66a329aec7e433d2d1c52a9', job)
            self.assertIn('cache-budget --max-bytes 8589934592', job)
            self.assertIn("steps.cache-budget.outputs.save == 'true'", job)
            self.assertIn('formal-workspace-v1-ubuntu24-', job)
            for value in ('runner.arch', 'RUST_TOOLCHAIN', 'OASIS7_WASM_TOOLCHAIN', 'Cargo.lock',
                          '**/Cargo.toml', '.cargo/config.toml', '.github/workflows/rust.yml', 'github.sha'):
                self.assertIn(value, job)
        matrix = JOBS['formal-process-tests']
        self.assertIn('always()', matrix)
        for name in ('formal-test-build', 'formal-test-build-trusted'):
            self.assertIn(f"needs.{name}.result == 'success'", matrix)
            self.assertIn(f"needs.{name}.result == 'skipped'", matrix)

    def test_manual_candidate_full_is_read_only_and_not_scheduled(self):
        candidate = JOBS['candidate-full-regression']
        self.assertIn("if: github.ref != 'refs/heads/main' && github.event_name == 'workflow_dispatch' && inputs.run_mode == 'full'", candidate)
        self.assertIn('cache-mode: read', candidate)
        self.assertNotIn('cache-mode: write', candidate)
        self.assertNotIn('trusted cache sentinel', candidate)
        self.assertNotIn('actions/cache/save@', candidate)
        self.assertIn('save-if: false', candidate)
        self.assertIn('actions/cache/restore@', candidate)
        self.assertIn('git rev-parse --verify', candidate)
        self.assertIn('GITHUB_SHA', candidate)
        self.assertIn('GITHUB_STEP_SUMMARY', candidate)
        self.assertIn('CI_VERBOSE=1 ./scripts/ci-tests.sh full', candidate)

    def test_candidate_full_preserves_full_setup_and_execution(self):
        # Compare the complete execution body, allowing only identity reporting
        # and the cache trust boundary to differ from the protected main writer.
        writer = JOBS['full-regression'].split('    steps:\n', 1)[1]
        candidate = JOBS['candidate-full-regression'].split('    steps:\n', 1)[1]
        candidate = re.sub(r'      - name: Record exact candidate identity\n.*?(?=      - name: Resolve)', '', candidate, flags=re.S)
        writer = re.sub(r'      - name: Prepare trusted cache sentinel\n.*?(?=      - (?:name: Run full|uses: actions/download-artifact))', '', writer, flags=re.S)
        writer = re.sub(r'      - name: Save prepared wasm-bindgen tool cache\n.*?(?=      - name:)', '', writer, flags=re.S)
        candidate = candidate.replace('          save-if: false\n', '')
        candidate = candidate.replace('actions/cache/restore@55cc8345863c7cc4c66a329aec7e433d2d1c52a9 # v6.1.0',
                                      'actions/cache@55cc8345863c7cc4c66a329aec7e433d2d1c52a9 # v6.1.0')
        self.assertEqual(candidate.strip(), writer.strip())

    def test_pinned_trunk_helper_failure_and_success_paths(self):
        # The protected baseline already executes this suite during migration.
        subprocess.run(['bash',str(ROOT/'scripts/install-ci-trunk.test.sh')],cwd=ROOT,check=True)

    def test_viewer_security_smoke_uses_installed_browser_after_formal_build(self):
        job = JOBS['viewer-js-required']
        self.assertIn('agent-browser@0.37.1', job)
        self.assertIn('--prefix "$RUNNER_TEMP/viewer-security-browser"', job)
        self.assertIn('"$browser_bin" install --with-deps', job)
        self.assertIn("printf 'AGENT_BROWSER_BIN=%s\\n'", job)
        self.assertNotIn('cache-mode: write', job)
        group = re.search(r'^    viewer_js_required\) (.*?) ;;$',
                          (ROOT / 'scripts/ci-tests.sh').read_text(), re.M).group(1)
        self.assertLess(group.index('run_oasis7_viewer_software_safe_build'),
                        group.index('viewer-auth-browser-security-smoke.mjs'))

    def test_additive_security_steps_preserve_candidate_groups(self):
        for name in ('net', 'viewer-js-required', 'viewer-performance-report', 'workflow-governance'):
            self.assertEqual(JOBS[name].count('name: Execute selected group'), 1)
            self.assertIn(f'bash ./scripts/ci-tests.sh required --group "{name.replace(chr(45), chr(95))}"', JOBS[name])
        performance = JOBS['viewer-performance-report']
        self.assertIn('agent-browser@0.37.1', performance)
        self.assertIn('--prefix "$RUNNER_TEMP/viewer-performance-browser"', performance)
        self.assertIn('"$browser_bin" install --with-deps', performance)
        self.assertIn("printf 'AGENT_BROWSER_BIN=%s\\n'", performance)
        self.assertLess(performance.index('Install pinned performance browser'), performance.index('Execute selected group'))
        driver = (ROOT / 'scripts/ci-tests.sh').read_text()
        for job, forbidden in [('net', 'Verify pinned network source'),
                               ('viewer-js-required', 'Verify browser authentication security'),
                               ('viewer-performance-report', 'Build performance test artifact')]:
            self.assertNotIn(forbidden, JOBS[job])
        net = JOBS['net']
        self.assertLess(net.index('Install WASM C compiler'), net.index('Execute selected group'))
        for check in ('scripts/libp2p-security-source.test.py', 'scripts/libp2p-compat.test.py',
                      'CC_wasm32_unknown_unknown=clang', 'AR_wasm32_unknown_unknown=llvm-ar',
                      'cargo check -p oasis7_net --no-default-features --target wasm32-unknown-unknown --locked',
                      'cargo check -p oasis7_node --features libp2p --target wasm32-unknown-unknown --locked'):
            self.assertIn(check, driver)
        governance = JOBS['workflow-governance']
        self.assertGreater(governance.index('Verify new source archive'), governance.index('Execute selected group'))
        for test in ('package-source-plan.test.py', 'safe-git-archive.test.py', 'cache-permission-probe.test.cjs'):
            self.assertIn(test, governance)
        self.assertNotIn('run: python3 scripts/ci-workflow.test.py', governance) # already in candidate dispatcher

    def test_bindgen_cache_has_exact_identity_and_single_trusted_writer(self):
        identities = []
        for name in ('viewer-js-required', 'viewer-performance-report', 'full-regression'):
            job = JOBS[name]
            self.assertIn('source scripts/wasm-bindgen-cli-common.sh', job)
            self.assertIn('uses: actions/cache/restore@caa296126883cff596d87d8935842f9db880ef25', job)
            self.assertIn('--ensure-cache --print-bin', job)
            self.assertIn('test -x "$bindgen_bin"', job)
            self.assertIn('test "$("$bindgen_bin" --version)" = "wasm-bindgen $WASM_BINDGEN_LOCK_VERSION"', job)
            self.assertLess(job.index('Swatinem/rust-cache@'), job.index('id: bindgen-meta'))
            prepare = job.split('id: prepare-bindgen', 1)[1].split('      - ', 1)[0]
            self.assertIn('set -euo pipefail', prepare)
            hot, cold = prepare.split('          else\n', 1)
            self.assertNotIn('--ensure-cache', hot)
            self.assertIn('--ensure-cache', cold)
            self.assertIn('WASM_BINDGEN_BIN=%s', prepare)
            identity = re.search(r'          key: (wasm-bindgen-cli-v1-.*)', job).group(1)
            identities.append(identity)
            self.assertIn("hashFiles('scripts/ensure-wasm-bindgen-cli.sh', 'scripts/wasm-bindgen-cli-common.sh')", identity)
            self.assertIn('path: ${{ steps.bindgen-meta.outputs.root }}', job)
            if name != 'full-regression':
                self.assertNotIn('actions/cache/save@', job)
            else:
                save = job.split('name: Save prepared wasm-bindgen tool cache', 1)[1].split('      - ', 1)[0]
                for condition in ('success()', "github.ref == 'refs/heads/main'", "steps.bindgen-cache.outputs.cache-hit != 'true'", "steps.prepare-bindgen.outcome == 'success'"):
                    self.assertIn(condition, save)
                self.assertIn('key: ' + identity, save)
                self.assertLess(job.index('Save prepared wasm-bindgen'), job.index('Run full test tier'))
        self.assertEqual(len(set(identities)), 1)
        self.assertNotIn('bindgen-meta', JOBS['launcher-web'])

    def test_local_signer_and_new_contracts_are_connected_once(self):
        fleet = JOBS['fleet-health']
        self.assertIn("if: runner.os == 'macOS'", fleet)
        self.assertIn('TMPDIR: /private/tmp', fleet)
        self.assertIn('python3 scripts/ci-local-signer-tests.py --require-darwin', fleet)
        driver = (ROOT / 'scripts/ci-tests.sh').read_text()
        packaging = driver.split('run_packaging_contract_tests() {', 1)[1].split('\n}', 1)[0]
        self.assertEqual(packaging.count('run python3 ./scripts/ci-local-signer-tests.py'), 1)
        governance = driver.split('run_workflow_governance_baseline_contract_tests() {', 1)[1].split('\n}', 1)[0]
        for test in ('plan-wasm-determinism-scope.test.sh', 'ensure-wasm-bindgen-cli.test.sh', 'ci-local-signer-tests.test.py'):
            self.assertEqual(governance.count(test), 1)

    def test_stable_install_is_minimal_with_required_components(self):
        self.assertNotIn('--profile default', WORKFLOW)
        for name, job in JOBS.items():
            if 'rustup toolchain install' in job and name != 'newapi-bridge-package':
                self.assertRegex(job, r'rustup toolchain install .*RUST_TOOLCHAIN.* --profile minimal --component rustfmt --component clippy')

    def test_wasm_dynamic_matrix_and_job_failure_boundaries(self):
        workflow = (ROOT / '.github/workflows/wasm-determinism-gate.yml').read_text()
        jobs = dict(re.findall(r'^  ([a-z][a-z0-9-]*):\n(.*?)(?=^  [a-z][a-z0-9-]*:|\Z)', workflow.split('\njobs:\n', 1)[1], re.M | re.S))
        expression = '${{ fromJSON(needs.plan-wasm-determinism-scope.outputs.selected_module_sets_json) }}'
        self.assertIn('selected_module_sets_json: ${{ steps.scope.outputs.selected_module_sets_json }}', jobs['plan-wasm-determinism-scope'])
        for name in ('collect-wasm-summaries', 'verify-wasm-determinism'):
            job = jobs[name]
            self.assertIn('module_set: ' + expression, job)
            guard = job.split('    if: >-', 1)[1].split('    strategy:', 1)[0]
            self.assertIn("needs.plan-wasm-determinism-scope.result == 'success'", guard)
            self.assertIn("outputs.scope == 'partial'", guard)
            self.assertIn("outputs.scope == 'all'", guard)
            self.assertIn('fail-fast: false', job)
            self.assertNotIn('Scope note', job)
            self.assertNotIn('outputs.run_m', job)
            self.assertNotIn('        if:', job)
            self.assertIn('if-no-files-found: error', job)
        self.assertIn("needs.collect-wasm-summaries.result == 'success'", jobs['verify-wasm-determinism'])
        self.assertNotIn('continue-on-error:', workflow)
        for filename in ('wasm-determinism-gate.yml', 'document-corpus-platform.yml'):
            workflow = (ROOT / '.github/workflows' / filename).read_text()
            self.assertIn("group: ${{ github.workflow }}-${{ github.event_name == 'pull_request' && format('pr-{0}', github.event.pull_request.number) || format('run-{0}', github.run_id) }}", workflow)
            self.assertIn("cancel-in-progress: ${{ github.event_name == 'pull_request' }}", workflow)

    def test_group_commands_execute_once_and_propagate_failure(self):
        driver = (ROOT / 'scripts/ci-tests.sh').read_text()
        network = re.search(r'^run_oasis7_net_libp2p_tests\(\) \{\n.*?^\}', driver, re.M | re.S).group()
        script = "set -euo pipefail\nrun() { printf '%s\\n' \"$*\"; }\nrun_cargo() { run cargo \"$@\"; }\n" + network + "\nrun_oasis7_net_libp2p_tests\n"
        result = subprocess.run(['bash', '-c', script], text=True, capture_output=True, env=dict(os.environ, GITHUB_ACTIONS='true', RUNNER_OS='Linux'))
        self.assertEqual(result.returncode, 0, result.stderr)
        for command in ('libp2p-security-source.test.py', 'libp2p-compat.test.py', 'check -p oasis7_net', 'check -p oasis7_node'):
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

    def test_required_and_full_tier_runners_share_feature_matched_viewer_artifact(self):
        driver = (ROOT / 'scripts/ci-tests.sh').read_text()
        names = ('build_oasis7_viewer_binary', 'run_oasis7_test_tier_with_viewer',
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
            self.assertIn('CHECK|check -p oasis7_agent_api -p oasis7_simulator --target wasm32-unknown-unknown --locked', calls)
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

    def test_full_tail_keeps_strict_wasmtime_profile_and_fixture_viewer(self):
        driver = (ROOT / 'scripts/ci-tests.sh').read_text()
        functions = [re.search(r'^' + name + r'\(\) \{\n.*?^\}', driver, re.M | re.S).group()
                     for name in ('build_oasis7_viewer_binary', 'run_oasis7_test_tier_with_viewer')]
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
            executables = {}
            for name in guard['TARGETS']:
                executable = directory / name
                executable.write_text(f'#!{sys.executable}\nprint({guard["REJECTION"]!r})\nraise SystemExit(1)\n')
                executable.chmod(0o755)
                executables[name] = executable
            def write_manifest(features):
                manifest.write_text('\n'.join(json.dumps({
                    'reason': 'compiler-artifact', 'target': {'name': name},
                    'executable': str(executable), 'features': features,
                }) for name, executable in executables.items()))
            write_manifest(['wasmtime'])
            guard['verify'](manifest)
            write_manifest(['wasmtime', 'test_tier_required'])
            with self.assertRaises(AssertionError):
                guard['verify'](manifest)
            write_manifest(['wasmtime'])
            executable = executables['oasis7_chain_runtime']
            for output, code in ((guard['REJECTION'], 0), ('unrelated failure', 1)):
                executable.write_text(f'#!{sys.executable}\nprint({output!r})\nraise SystemExit({code})\n')
                with self.assertRaises(AssertionError):
                    guard['verify'](manifest)

    def test_network_wasm_tools_precede_group_execution(self):
        net = JOBS['net']
        self.assertLess(net.index('Install WASM C compiler'), net.index('Execute selected group'))
        self.assertLess(net.index('llvm-ar --version'), net.index('Execute selected group'))
        writer = JOBS['full-regression']
        self.assertIn('            clang \\', writer)
        self.assertIn('            llvm \\', writer)
        self.assertIn('rustup target add wasm32-unknown-unknown', writer)
        for tool in ('clang --print-targets | grep -w wasm32', 'llvm-ar --version'):
            self.assertLess(writer.index(tool), writer.index('Run full test tier'))

    def test_trusted_writer_only_runs_on_protected_main(self):
        writer = JOBS['full-regression']
        self.assertIn("if: github.ref == 'refs/heads/main' && (github.event_name == 'schedule' || (github.event_name == 'workflow_dispatch' && inputs.run_mode == 'full'))", writer)
        self.assertIn('cache-mode: write', writer)
        self.assertIn('test "$GITHUB_REF" = refs/heads/main', writer)
        self.assertNotIn('test "$ACTIONS_CACHE_MODE"', writer)

    def test_readers_share_compatible_trusted_writer_identity(self):
        writer = JOBS['full-regression']
        self.assertIn('cache-mode: write', writer)
        self.assertIn('shared-key: ci-full-regression-trusted-v2', writer)
        self.assertIn('env-vars: CARGO CC CFLAGS CXX CMAKE RUST OASIS7_WASM', writer)
        for name, job in JOBS.items():
            if 'save-if: false' in job and 'Swatinem/rust-cache@' in job:
                self.assertIn('shared-key: ci-full-regression-trusted-v2', job, name)
                self.assertIn('env-vars: CARGO CC CFLAGS CXX CMAKE RUST OASIS7_WASM', job, name)
        self.assertNotIn('ordinary-required-v2', WORKFLOW)

    def test_download_caches_follow_actual_worksets(self):
        node_jobs={name for name,job in JOBS.items() if 'uses: actions/setup-node@249970729cb0ef3589644e2896645e5dc5ba9c38' in job}
        self.assertEqual(node_jobs,{'viewer-js-required','viewer-performance-report','launcher-web','full-regression','candidate-full-regression'})
        for name in node_jobs:
            self.assertIn('cache: npm',JOBS[name])
            self.assertIn('cache-dependency-path: crates/oasis7_viewer/package-lock.json',JOBS[name])
            self.assertIn('npm ci --prefix crates/oasis7_viewer',JOBS[name])
        for name,job in JOBS.items():
            if 'Swatinem/rust-cache@' in job and name in {g.replace('_', '-') for g in CONFIG['groups']}:
                self.assertIn('shared-key: ci-full-regression-trusted-v2',job)
                self.assertIn('add-rust-environment-hash-key: true',job)
                self.assertIn('env-vars: CARGO CC CFLAGS CXX CMAKE RUST OASIS7_WASM',job)
                self.assertLess(job.index('rustup default'),job.index('uses: Swatinem/rust-cache@6323deb102c322ba6fcbdcafc7e3dddab59af2b6 # v2'))
        for name in ('launcher-web','full-regression'):
            job=JOBS[name]
            self.assertIn('uses: actions/cache@55cc8345863c7cc4c66a329aec7e433d2d1c52a9',job)
            self.assertIn("hashFiles('scripts/install-ci-trunk.sh')",job)
            self.assertNotIn('cargo install trunk',job)
            self.assertNotIn('restore-keys:',job)
            self.assertLess(job.index('Cache pinned Trunk release archive'),job.index('bash scripts/install-ci-trunk.sh'))
            invocation='Execute selected group' if name=='launcher-web' else 'Run full test tier'
            self.assertLess(job.index('bash scripts/install-ci-trunk.sh'),job.index(invocation))

    def test_failed_scope_planning_reports_gate_failure_before_artifact_download(self):
        gate = JOBS['required-gate']
        self.assertIn('always()', gate)
        self.assertLess(gate.index('Require successful scope planning'),
                        gate.index('actions/download-artifact@'))
        script = gate.split('        run: |\n', 1)[1].split('      - uses:', 1)[0]
        for state in ('failure', 'cancelled', 'skipped', '', 'success'):
            with self.subTest(state=state):
                result = subprocess.run(['bash', '-c', script], text=True, capture_output=True,
                                        env=dict(os.environ, SELECT_RESULT=state))
                self.assertEqual(result.returncode, 0 if state == 'success' else 1)
                if state != 'success':
                    self.assertIn('CI scope planning did not succeed', result.stdout)
        for name in ('oasis7-test-build', 'formal-test-build', 'formal-test-build-trusted', 'formal-process-tests', 'command-center'):
            self.assertIn("needs.select.result == 'success'", JOBS[name])

    def test_gate_is_last_and_all_groups_are_dependencies(self):
        gate = JOBS['required-gate']
        dependencies = re.search(r'    needs: \[(.*)\]', gate).group(1).split(', ')
        self.assertEqual(set(dependencies), {'select', *(g.replace('_', '-') for g in CONFIG['groups'])})
        self.assertIn('always()', gate)
        self.assertIn('ci-required-result.py', gate)
        for group in CONFIG['groups']:
            job = JOBS[group.replace('_', '-')]
            if group == 'formal_process':
                self.assertIn('always()', job)
                self.assertIn('needs: [select, formal-test-build, formal-test-build-trusted, formal-process-tests]', job)
                continue
            dependency = 'needs: [select, oasis7-test-build]' if group == 'oasis7_required' else 'needs: select'
            self.assertIn(dependency, job)
            self.assertIn(f"needs.select.outputs.run_{group} == 'true'", job)
            self.assertIn("needs.select.result == 'success'", job)
            self.assertIn("fromJSON(needs.select.outputs.plan).scope == 'full'", job)
            if group != 'fleet_health':
                self.assertNotIn('strategy:', job)
            self.assertNotIn('ci-authority', job)
        self.assertNotIn('needs: required-gate', WORKFLOW)
        self.assertNotIn('continue-on-error:', WORKFLOW)
        self.assertNotIn('paths-ignore:', WORKFLOW)

    def test_platform_product_jobs_still_gate(self):
        self.assertIn('runs-on: windows-2022', JOBS['windows-rollout'])
        self.assertIn('p2p-public-testnet-package-rollout.test.sh', JOBS['windows-rollout'])
        self.assertIn('testnet-packages-macos-arm64-contract.test.sh', JOBS['macos-package-contract'])
        self.assertIn('p2p-public-testnet-fleet-health.test.py', JOBS['fleet-health'])
        self.assertIn('os: [ubuntu-24.04, windows-2022, macos-15]', JOBS['fleet-health'])
        self.assertIn('--host-smoke', JOBS['fleet-health'])
        self.assertEqual(WORKFLOW.count('run: bash ./scripts/testnet-packages-macos-arm64-contract.test.sh'), 1)
        self.assertIn('./scripts/ci-tests.sh full', JOBS['full-regression'])
        self.assertIn('package-newapi-bridge-service.sh', JOBS['newapi-bridge-package'])
        report = JOBS['viewer-performance-report']
        self.assertIn('rustup default', report)
        self.assertIn('rustup target add wasm32-unknown-unknown', report)
        self.assertIn('path: output/ci/viewer-performance/**', report)
        driver = (ROOT / 'scripts/ci-tests.sh').read_text()
        self.assertIn('viewer-performance-report-only-contract.test.sh', driver)
        full = driver.split('run_full_required_superset() {', 1)[1].split('\n}', 1)[0]
        self.assertIn('testnet-packages-macos-arm64-contract.test.sh', full)

    def test_trusted_closure_and_safe_checkout(self):
        select = JOBS['select']
        self.assertIn('git show "${BASE_SHA}:scripts/${file}"', select)
        self.assertIn('--test-ref "$tested_sha"', select)
        self.assertIn('for file in plan-rust-required-scope.py ci-required-scope.json ci-required-result.py;', select)
        self.assertNotIn('cp scripts/', select)
        self.assertEqual(WORKFLOW.count('name: ci-authority'), 2)
        self.assertNotIn('matrix.group', WORKFLOW)
        self.assertEqual(len(re.findall(r'uses: actions/checkout@[0-9a-f]{40}\b', WORKFLOW)), WORKFLOW.count('persist-credentials: false'))
        self.assertNotIn('secrets.', WORKFLOW)
        self.assertNotIn('GH_TOKEN:', WORKFLOW)
        self.assertNotIn('scripts/pm/', WORKFLOW)
        self.assertNotIn('task_uid', WORKFLOW)
        self.assertIn('permissions:\n  contents: read', WORKFLOW)

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
        function = re.search(r'^run_oasis7_workspace_support_crate_tests\(\) \{\n.*?^\}', driver, re.M | re.S).group()
        script = """set -euo pipefail
run_cargo() { printf '%s|%s\\n' "$OASIS7_WASM_BUILD_STD" "$*"; }
run_cargo_clippy() { printf '%s|clippy %s\\n' "$OASIS7_WASM_BUILD_STD" "$*"; }
""" + function + "\nrun_oasis7_workspace_support_crate_tests\nprintf 'after|%s\\n' \"$OASIS7_WASM_BUILD_STD\"\n"
        result = subprocess.run(['bash', '-c', script], text=True, capture_output=True,
                                env=dict(os.environ, OASIS7_WASM_BUILD_STD='1'))
        self.assertEqual(result.returncode, 0, result.stderr)
        calls = result.stdout.splitlines()
        self.assertEqual(calls[-2], '0|test -p wasm_build_suite -p wasm_module_observe')
        self.assertTrue(all(call.startswith('1|') for call in calls[:-2]))
        self.assertEqual(calls[-1], 'after|1')

    def test_agent_contract_and_harness_checks_are_in_workspace_support(self):
        driver = (ROOT / 'scripts/ci-tests.sh').read_text()
        function = re.search(r'^run_oasis7_workspace_support_crate_tests\(\) \{\n.*?^\}', driver, re.M | re.S).group()
        script = """set -euo pipefail
run_cargo() { printf 'test|%s\\n' "$*"; }
run_cargo_clippy() { printf 'clippy|%s\\n' "$*"; }
""" + function + "\nrun_oasis7_workspace_support_crate_tests\n"
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
        function = re.search(r'^run_oasis7_workspace_support_crate_tests\(\) \{\n.*?^\}', driver, re.M | re.S).group()
        script = "set -euo pipefail\nrun_cargo() { printf '%s\\n' \"$*\"; }\nrun_cargo_clippy() { printf 'clippy %s\\n' \"$*\"; }\n" + function + "\nrun_oasis7_workspace_support_crate_tests\n"
        result = subprocess.run(['bash', '-c', script], text=True, capture_output=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        calls = [line.split() for line in result.stdout.splitlines()]
        self.assertIn(['clippy', '-p', 'oasis7_agent_api', '--all-targets'], calls)
        self.assertIn(['clippy', '-p', 'oasis7_agent_harness', '--all-targets'], calls)
        for package in ('oasis7_context', 'oasis7_secure_envelope'):
            selected = [call for call in calls if package in call]
            self.assertTrue(selected, package)
            self.assertTrue(any('--all-targets' in call and '--locked' in call for call in selected))
            self.assertEqual(CONFIG['package_groups'][package], ['workspace_support', 'command_center'])
        self.assertTrue(any('oasis7_local_signer' in call and '--test' in call and 'context_cli' in call for call in calls))
        for path in ('crates/oasis7_context/**', 'crates/oasis7_secure_envelope/**'):
            selected_groups = {group for rule in CONFIG['rules'] if path in rule['match'] for group in rule['groups']}
            self.assertIn('fleet_health', selected_groups)

    def test_private_context_macos_uses_fixed_toolchain_and_declared_resource(self):
        fleet = JOBS['fleet-health']
        self.assertIn('rust_toolchain', CONFIG['resources']['fleet_health'])
        prepare = fleet.index('name: Prepare context Rust toolchain on macOS')
        verify = fleet.index('name: Verify private context filesystem and caller on macOS')
        self.assertLess(prepare, verify)
        preparation = fleet[prepare:verify]
        self.assertIn("if: runner.os == 'macOS'", preparation)
        self.assertIn('rustup toolchain install "$RUST_TOOLCHAIN"', preparation)
        self.assertIn('rustup default "$RUST_TOOLCHAIN"', preparation)
        self.assertTrue((ROOT / 'rust-toolchain.toml').is_file())
        validation = fleet[verify:fleet.index('name: Ensure Linux process inspection dependency')]
        self.assertIn("if: runner.os == 'macOS'", validation)
        self.assertIn('cargo test --locked -p oasis7_context -p oasis7_secure_envelope --all-targets', validation)
        self.assertIn('cargo test --locked -p oasis7_local_signer --test context_cli', validation)

    def test_invalid_selected_group_cannot_silently_skip_command(self):
        driver = (ROOT / 'scripts/ci-tests.sh').read_text()
        self.assertIn('Unknown CI group:', driver)
        self.assertNotIn('disabled_by_scope_planner', driver)
        self.assertNotIn('required_gate_execution_contract', driver)
        self.assertNotIn('scripts/pm/', driver)

    def test_candidate_graph_rejects_missing_dependencies_platforms_and_failure_swallowing(self):
        self.assertTrue(validate_candidate_graph(WORKFLOW, CONFIG))
        mutations = [WORKFLOW.replace(', fleet-health]', ']'),
                     WORKFLOW.replace('windows-2022, macos-15]', 'windows-2022]'),
                     WORKFLOW.replace('    runs-on: ${{ matrix.os }}', '    runs-on: windows-2022'),
                     WORKFLOW.replace('      fail-fast: false', '      continue-on-error: true'),
                     WORKFLOW.replace(' --host-smoke', ''),
                     WORKFLOW.replace('    if: always()', '    if: success()'),
                     WORKFLOW.replace("fromJSON(needs.select.outputs.plan).scope == 'full'", 'false', 1)]
        for workflow in mutations:
            with self.subTest(workflow=workflow[-100:]), self.assertRaises(ValueError):
                validate_candidate_graph(workflow, CONFIG)
        new_config = dict(CONFIG, groups=[*CONFIG['groups'], 'new_group'])
        with self.assertRaises(ValueError):
            validate_candidate_graph(WORKFLOW, new_config)

    def test_candidate_driver_executes_atomic_rename_and_new_failure(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            scripts = root / 'scripts'
            scripts.mkdir()
            for filename in ('ci-tests.sh', 'find-python-with-module.sh', 'viewer-dependency-preflight.sh'):
                (scripts / filename).write_bytes((ROOT / 'scripts' / filename).read_bytes())
            driver = scripts / 'ci-tests.sh'
            driver.write_text(driver.read_text().replace('validate-codex-agent-config.py', 'renamed-validator.py'))
            command = ['bash', str(driver), 'required', '--group', 'codex_agent_config_validation', '--repo-root', str(root)]
            entry = scripts / 'renamed-validator.py'
            entry.write_text('print("candidate entry executed")\n')
            passed = subprocess.run(command, text=True, capture_output=True)
            self.assertEqual(passed.returncode, 0, passed.stderr)
            self.assertIn('candidate entry executed', passed.stdout)
            entry.write_text('raise SystemExit(42)\n')
            failed = subprocess.run(command, text=True, capture_output=True)
            self.assertEqual(failed.returncode, 42, failed.stderr)
            self.assertFalse((scripts / 'validate-codex-agent-config.py').exists())

    def test_concurrency_is_per_pr_and_per_non_pr_run(self):
        self.assertIn("format('pr-{0}', github.event.pull_request.number)", WORKFLOW)
        self.assertIn("format('run-{0}', github.run_id)", WORKFLOW)
        self.assertIn("cancel-in-progress: ${{ github.event_name == 'pull_request' }}", WORKFLOW)
        concurrency = WORKFLOW.split('concurrency:', 1)[1].split('permissions:', 1)[0]
        self.assertNotIn('github.sha', concurrency)
        self.assertIn('github.workflow', concurrency)

    def test_clippy_precedes_heavy_group_tests(self):
        driver = (ROOT / 'scripts/ci-tests.sh').read_text()
        for group, prefix in [('oasis7_required', 'required_tier'), ('consensus', 'consensus'),
                              ('distfs', 'distfs'), ('node', 'node'), ('net', 'net')]:
            case = re.search(r'^    ' + group + r'\).*?;;', driver, re.M).group()
            self.assertLess(case.index('run_oasis7_' + prefix + '_clippy'),
                            case.index('run_oasis7_' + prefix + '_tests'))


if __name__ == '__main__':
    unittest.main()
