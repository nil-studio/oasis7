#!/usr/bin/env python3
"""Self-contained core workflow operational, telemetry, toolchain, and wasm matrix contract tests (part 4)."""
import itertools
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parent.parent
RAW_WORKFLOW = (ROOT / '.github/workflows/rust.yml').read_text()
CONFIG = json.loads((ROOT / 'scripts/ci-required-scope.json').read_text())


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


def without_optional_dependency_cache_install(workflow, config=None):
    if config is None:
        config = CONFIG
    workflow = workflow_contract_view(workflow)
    # Only these exact optional installers and diagnostics uploads may fail.
    # Removing a writer condition, renaming the command or swallowing any build,
    # required artifact, merge or test failure is never exempt.
    for base, suffix in itertools.product(('formal-debug-build', 'formal-endpoint-build'), ('', '-trusted')):
        name = base + suffix
        job = re.search(r'^  ' + name + r':\n.*?(?=^  [a-z][a-z0-9-]*:|\Z)', workflow, re.M | re.S)
        if not job:
            continue
        condition = (
            '        if: >-\n'
            f"          github.job == '{base}-trusted' && github.ref == 'refs/heads/main' &&\n"
            "          github.workflow_sha == github.sha && needs.select.outputs.trusted_source == 'true' &&\n"
            "          (github.event_name == 'schedule' || (github.event_name == 'workflow_dispatch' && inputs.run_mode == 'full'))\n")
        installer = ('      - name: Install dependency cache admission\n'
                     '        id: dependency-cache-tools\n' + condition +
                     '        continue-on-error: true\n'
                     '        run: python3 scripts/ci-rust-cache-admission.py install\n')
        component = base.removesuffix('-build')
        diagnostics = (f'      - name: Upload {component} build diagnostics\n'
                       '        if: always()\n'
                       '        continue-on-error: true\n'
                       '        uses: actions/upload-artifact@cf430e030ddbb5b0abf93d22962f4752f3646cd9 # v7.0.2\n'
                       '        with:\n'
                       f'          name: {component}-build-diagnostics-${{{{ github.sha }}}}-${{{{ github.run_attempt }}}}\n'
                       f'          path: ${{{{ runner.temp }}}}/{component}-build-diagnostics\n'
                       '          if-no-files-found: warn\n'
                       '          retention-days: 2\n')
        workflow = (workflow[:job.start()] + job[0].replace(installer, '', 1).replace(diagnostics, '', 1)
                    + workflow[job.end():])
    if '      - &engineering-job-observation\n' in workflow:
        import engineering_telemetry as bindings
        import hashlib
        policy = config.get('engineering_job_observation', {})
        expected = dict(schema_version=1, workflow_path='.github/workflows/rust.yml',
                        step_name=bindings.STEP, step_anchor='engineering-job-observation',
                        timeout_minutes=1, failure_mode='nonblocking',
                        emitter_sha256=hashlib.sha256(bindings.EMITTER.encode()).hexdigest())
        step = re.search(r'^      - &engineering-job-observation\n.*?(?=^      - |^  [a-z][a-z0-9-]*:|\Z)',
                         workflow, re.M | re.S)
        if policy == expected and step and step[0].rstrip('\n') == bindings.workflow_step().rstrip('\n'):
            workflow = workflow[:step.start()] + workflow[step.end():]
    return workflow


def validate_custody_tools(node):
    if re.search(r'^    container:', node, re.M):
        if (not re.search(r'^    container:\n      image: ghcr\.io/nil-studio/oasis7/ci-build@sha256:[0-9a-f]{64}$',
                          node, re.M)
                or 'gdb --version && gcc --version && pkg-config --version' not in node
                or 'scripts/ci-apt-install.py' in node or 'apt-get ' in node):
            raise ValueError('custody image requires a pinned digest and preinstalled GDB tools')
    elif '-- gdb gcc pkg-config' not in node:
        raise ValueError('custody host runner requires GDB tools')


def validate_workerd_capture(node, summary):
    validate_custody_tools(node)
    run = next((line for line in node.splitlines() if 'ci-custody-platform.py run ' in line), '')
    validate = next((line for line in summary.splitlines() if 'ci-custody-platform.py validate ' in line), '')
    if '--capture-workerd-faults' not in run or '--capture-workerd-faults' not in validate:
        raise ValueError('client-abuse capture must be produced and consumed by the selected run')
    steps = re.findall(r'^      - .*?(?=^      - |\Z)', node, re.M | re.S)
    contracts = [step for step in steps if 'ci-custody-workerd-gdb-contracts.py' in step]
    if (len(contracts) != 1 or 'if: matrix.shard == 0' not in contracts[0]
            or "OASIS7_REQUIRE_GDB_TESTS: '1'" not in contracts[0]
            or '--failfast GdbContracts' not in contracts[0]
            or 'continue-on-error' in contracts[0]):
        raise ValueError('standard GDB process contracts must actually run on the hosted consumer')
    uploads = [step for step in steps if 'name: custody-workerd-fault-' in step]
    if (len(uploads) != 1 or 'if: always()' not in uploads[0]
            or not re.search(r'^        timeout-minutes: 5$', uploads[0], re.M)
            or 'continue-on-error' in uploads[0]
            or 'name: custody-workerd-fault-${{ github.run_id }}-${{ github.run_attempt }}-${{ matrix.shard }}' not in uploads[0]
            or 'overwrite:' in uploads[0]
            or '/workerd-fault/records.jsonl' not in uploads[0]
            or '/workerd-fault/report.json' not in uploads[0]
            or '/workerd-fault-attempt-1/records.jsonl' not in uploads[0]
            or '/workerd-fault-attempt-1/report.json' not in uploads[0]
            or '/client-abuse.integration.test.mjs.log' not in uploads[0]
            or '/client-abuse.integration.test.mjs.attempt-1.log' not in uploads[0]):
        raise ValueError('workerd failure evidence must survive later attempts')


def validate_candidate_graph(workflow, config):
    jobs = workflow_jobs(workflow)
    gate = jobs['required-gate']
    dependencies = re.search(r'    needs: \[(.*)\]', gate).group(1).split(', ')
    expected = {'select', *(g.replace('_', '-') for g in config['groups'])}
    if config.get('workspace_support_direct_gate_version') in (1, 2):
        expected.add('workspace-support-other')
    if config.get('workspace_support_direct_gate_version') == 2:
        expected.update(('custody-native-contracts', 'custody-native', 'custody-wasm-production', 'custody-node'))
    if set(dependencies) != expected or len(dependencies) != len(expected):
        raise ValueError('gate dependencies must exactly cover candidate groups')
    if 'always()' not in gate or 'continue-on-error:' in without_optional_dependency_cache_install(workflow, config):
        raise ValueError('failure propagation must remain intact')
    fleet = jobs['fleet-health']
    subcontract_fleet_os = (
        'os: ${{ (fromJSON(needs.select.outputs.plan).fleet_health_subcontracts_version == 1 && '
        'fromJSON(needs.select.outputs.plan).scope != \'full\' && '
        'fromJSON(needs.select.outputs.plan).scope != \'explicit-full\' && '
        'join(fromJSON(needs.select.outputs.plan).fleet_health_oses, \',\') != \'\') && '
        'fromJSON(needs.select.outputs.plan).fleet_health_oses || '
        '((needs.select.outputs.run_fleet_health == \'true\' || '
        'fromJSON(needs.select.outputs.plan).scope == \'full\' || '
        'fromJSON(needs.select.outputs.plan).scope == \'explicit-full\' || '
        '(fromJSON(needs.select.outputs.plan).contract_job_consolidation_version != 3 && '
        'fromJSON(needs.select.outputs.plan).contract_job_consolidation_version != 4 && '
        'fromJSON(needs.select.outputs.plan).contract_job_consolidation_version != 5)) && '
        'fromJSON(\'["ubuntu-24.04","windows-2022","macos-15"]\') || fromJSON(\'["windows-2022"]\')) }}'
    )
    if (subcontract_fleet_os not in fleet
            or 'runs-on: ${{ matrix.os }}' not in fleet
            or 'fail-fast: false' not in fleet or 'include:' in fleet or 'exclude:' in fleet
            or not re.search(r"if:\s*(?:>-\s*)?runner\.os != 'Windows'", fleet)
            or '--host-smoke' not in fleet
            or 'p2p-public-testnet-package-rollout.test.sh' not in fleet):
        raise ValueError('fleet must run subcontract-sliced hosts when selected, Windows rollout when consolidated, and POSIX cleanup smoke')
    for group in config['groups']:
        job = jobs[group.replace('_', '-')]
        if group == 'workspace_support':
            summary = jobs['workspace-support']
            expected_support = ['select', 'workspace-support-legacy', 'workspace-support-other',
                                'custody-native-contracts', 'custody-native',
                                'custody-wasm-production', 'custody-node']
            dependencies = re.search(r'    needs: \[(.*)\]', summary).group(1).split(', ')
            if (dependencies != expected_support or 'always()' not in summary
                    or 'ci-custody-platform.py validate' not in summary
                    or 'pattern: custody-node-${{ github.run_id }}-*' not in summary):
                raise ValueError('support summary must validate every selected producer and complete Node inventory')
            legacy = jobs['workspace-support-legacy']
            header = legacy.split('    steps:', 1)[0]
            if ('fail-fast: false' not in header or 'max-parallel: 2' not in header
                    or 'partition: ${{ fromJSON(needs.select.outputs.workspace_support_partitions) }}' not in header
                    or 'include:' in header or 'exclude:' in header
                    or "workspace_support_partitions: ${{ steps.select.outputs.workspace_support_partitions || '[\"all\"]' }}" not in jobs['select']
                    or 'custody_execution_version != 1' not in header
                    or (config.get('workspace_support_direct_gate_version') in (1, 2)
                        and ('workspace_support_direct_gate_version != 1' not in header
                             or (config.get('workspace_support_direct_gate_version') == 2
                                 and 'workspace_support_direct_gate_version != 2' not in header)))
                    or '--support-partition "$SUPPORT_PARTITION"' not in legacy):
                raise ValueError('support legacy must retain every trusted partition and old BASE fallback')
            node = jobs['custody-node']
            validate_workerd_capture(node, summary)
            if ('fail-fast: false' not in node or 'max-parallel: 4' not in node
                    or 'custody_node_shards' not in node or 'exclude:' in node or 'include:' in node
                    or 'ci-custody-platform.py restore' not in node or 'ci-custody-platform.py run' not in node
                    or 'if: matrix.shard == 0' not in node or 'npm run test:types' not in node
                    or 'needs: [select, custody-native, custody-wasm-production]' not in node
                    or 'name: ${{ needs.custody-wasm-production.outputs.approval_artifact }}' not in node):
                raise ValueError('custody must run the complete trusted file shards and TypeScript consumer')
            proof = jobs['custody-native-contracts']
            if ('    needs: [select, fast-preflight]\n' not in proof or 'custody_execution_version == 1' not in proof
                    or 'verify.sh --stage native-contracts' not in proof
                    or 'RUSTUP_TOOLCHAIN: ${{ env.RUST_TOOLCHAIN }}' not in proof
                    or 'custody-native-contracts' in node):
                raise ValueError('native proof must remain required without blocking artifact consumers')
            if config.get('custody_policy_tests_version') == 1:
                policy_step = next((step for step in re.findall(r'^      - .*?(?=^      - |\Z)', proof, re.M | re.S)
                                    if 'name: Execute complete custody policy targets' in step), '')
                if ('if: fromJSON(needs.select.outputs.plan).custody_policy_tests_version == 1' not in policy_step
                        or 'ci-custody-platform.py policy-tests --plan "$RUNNER_TEMP/selection.json"' not in policy_step
                        or 'CUSTODY_SELECTION_SHA256: ${{ fromJSON(needs.select.outputs.plan).selection_sha256 }}' not in policy_step
                        or any('OASIS7_SUPPORT_' + key.upper() + ': ${{ needs.select.outputs.' + key + ' }}' not in policy_step
                               for key in ('source_sha', 'tested_sha', 'policy_sha'))
                        or 'name: ${{ needs.select.outputs.selection_artifact }}' not in proof
                        or 'path: ${{ runner.temp }}' not in proof):
                    raise ValueError('custody policy changes must execute the complete native target expansion')
            wasm_step = next((step for step in re.findall(r'^      - .*?(?=^      - |\Z)', proof, re.M | re.S)
                              if 'name: Execute complete custody WASM compile proofs' in step), '')
            if ('if: fromJSON(needs.select.outputs.plan).custody_compile_proof_version == 1' not in wasm_step
                    or 'CUSTODY_COMPONENT: wasm-contracts' not in wasm_step
                    or 'ci-custody-platform.py stage' not in wasm_step
                    or 'verify.sh --stage "$stage"' not in wasm_step
                    or 'run_worker_wasm_compile_profiles' not in wasm_step
                    or 'CUSTODY_SELECTION_SHA256: ${{ fromJSON(needs.select.outputs.plan).selection_sha256 }}' not in wasm_step
                    or 'actions/upload-artifact' in proof
                    or 'custody-wasm-contracts' in jobs
                    or 'custody-wasm-approval' in jobs):
                raise ValueError('WASM proof must preserve selected compile checks without blocking Node consumers')
            for component in ('native', 'wasm-production'):
                producer = jobs['custody-' + component]
                if ('    needs: [select, fast-preflight]' not in producer
                        or 'custody_execution_version == 1' not in producer
                        or 'CUSTODY_COMPONENT: ' + component not in producer
                        or 'verify.sh --stage "$stage"' not in producer
                        or 'ci-custody-platform.py stage' not in producer
                        or 'CUSTODY_SELECTION_SHA256: ${{ fromJSON(needs.select.outputs.plan).selection_sha256 }}' not in producer
                        or 'run_worker_wasm_compile_profiles' not in producer
                        or 'ci-custody-platform.py pack' not in producer):
                    raise ValueError('custody producer must preserve every verification stage')
                uploads = [step for step in re.findall(r'^      - .*?(?=^      - |\Z)', producer, re.M | re.S)
                           if 'uses: actions/upload-artifact@' in step]
                expected_bundles = ([('steps.bundle.outputs.artifact', 'custody-bundle')]
                                    if component == 'native'
                                    else [('steps.bundle.outputs.artifact', 'custody-bundle'),
                                          ('steps.approval-bundle.outputs.artifact', 'custody-approval-bundle')])
                if component == 'wasm-production' and (
                        producer.count('CUSTODY_COMPONENT: wasm-approval') != 2
                        or 'approval_artifact: ${{ steps.approval-bundle.outputs.artifact }}' not in producer):
                    raise ValueError('custody WASM producer must build and pack both production and approval components')
                if len(uploads) != len(expected_bundles):
                    raise ValueError('custody upload must preserve every manifest file from the isolated bundle')
                for upload, (artifact_expr, bundle_dir) in zip(uploads, expected_bundles):
                    if (f'\n          name: ${{{{ {artifact_expr} }}}}\n' not in upload
                            or f'\n          path: ${{{{ runner.temp }}}}/{bundle_dir}\n' not in upload
                            or '\n          include-hidden-files: true\n' not in upload
                            or '\n          compression-level: 0\n' not in upload
                            or '\n          if-no-files-found: error\n' not in upload
                            or '\n          retention-days: 7\n' not in upload):
                        raise ValueError('custody upload must preserve every manifest file from the isolated bundle')
            if ('    needs: [select, fast-preflight]' not in jobs['workspace-support-other']
                    or 'SUPPORT_PARTITION: other' not in jobs['workspace-support-other']
                    or (config.get('workspace_support_direct_gate_version') in (1, 2)
                        and ('workspace_support_direct_gate_version == 1' not in jobs['workspace-support-other']
                             or (config.get('workspace_support_direct_gate_version') == 2
                                 and 'workspace_support_direct_gate_version == 2' not in jobs['workspace-support-other'])))):
                raise ValueError('other support must remain independent of custody builds')
            if (config.get('workspace_support_direct_gate_version') == 2
                    and ('workspace_support_direct_gate_version != 2' not in summary
                         or 'ci-custody-platform.py" validate' not in jobs['required-gate'])):
                raise ValueError('direct gate v2 must bypass support summary and validate custody in required-gate')
            continue
        if group == 'formal_process':
            if ('always()' not in job or 'ci-formal-plan.py" --selected' not in job
                    or "needs.select.outputs.run_formal_process == 'true'" not in job):
                raise ValueError('formal aggregation must execute when selected or failed and validate results')
            continue
        if config.get('contract_job_consolidation_version') in (1, 2, 3, 4, 5) and group in config.get('group_jobs', {}):
            host_job = jobs[config['group_jobs'][group]]
            if group == 'command_center' and config.get('command_center_offline_version') == 1:
                version_guard = 'fromJSON(needs.select.outputs.plan).command_center_offline_version != 1'
            else:
                version_guard = f"fromJSON(needs.select.outputs.plan).contract_job_consolidation_version != {config['contract_job_consolidation_version']}"
            if (version_guard not in job
                    or f'needs.select.outputs.run_{group}' not in host_job):
                raise ValueError('consolidated contract group must gate legacy job and bind its select output on host job')
        if ("needs.select.result == 'success'" not in job
                or "fromJSON(needs.select.outputs.plan).scope == 'full'" not in job):
            raise ValueError('group requires successful select and full migration fallback')
        if group not in ('fleet_health', 'oasis7_required', 'workspace_support') and 'strategy:' in job:
            raise ValueError('ordinary groups must not have singleton matrices')
        if group == 'oasis7_required':
            strategy = ('    strategy:\n      fail-fast: false\n      matrix:\n'
                        '        partition: ${{ fromJSON(needs.select.outputs.ordinary_partitions) }}\n')
            header = job.split('    steps:', 1)[0]
            if (strategy not in header or 'include:' in header or 'exclude:' in header
                    or "ordinary_partitions: ${{ steps.select.outputs.ordinary_partitions || '[\"all\"]' }}" not in jobs['select']
                    or 'ORDINARY_PARTITION: ${{ matrix.partition }}' not in job
                    or '--ordinary-partition "$ORDINARY_PARTITION"' not in job):
                raise ValueError('ordinary matrix must preserve the complete trusted partition inventory and legacy all fallback')
        if group == 'command_center' and 'uses: ./.github/workflows/command-center.yml' not in job:
            raise ValueError('Command Center checks must execute through the reusable workflow')
        if 'ci-authority' in job and group != 'workflow_governance':
            raise ValueError('execution must use candidate checkout')
        if group == 'workflow_governance' and (
                'python3 "$RUNNER_TEMP/ci-authority/ci-python-control-contracts.py" run' not in job
                or 'bash ./scripts/ci-tests.sh required --group "workflow_governance"' not in job):
            raise ValueError('trusted Python assertions and candidate governance must both execute')
        if group in ('operational_contracts', 'packaging_contracts'):
            if f'needs.select.outputs.run_{group}' not in job:
                raise ValueError(f'{group} must bind its exact select output run_{group}')
            if f'bash ./scripts/ci-tests.sh required --group "{group}"' not in job:
                raise ValueError(f'{group} must execute its exact driver group {group}')
    return True


class WorkflowCorePart4(unittest.TestCase):
    def test_local_cargo_contract_regressions_execute_and_propagate_failures(self):
        driver = (ROOT / 'scripts/ci-tests.sh').read_text()
        names = ('run_packaging_artifact_contract_tests', 'run_packaging_contract_tests',
                 'run_operational_identity_contract_tests', 'run_operational_node_contract_tests',
                 'run_operational_contract_tests')
        functions = '\n'.join(re.search(r'^' + name + r'\(\) \{\n.*?^\}', driver, re.M | re.S).group()
                              for name in names)
        setup = 'set -euo pipefail\nrun_provider_remote_https_smoke() { :; }\n'
        setup += 'run() { echo "$*"; if [[ -n "$FAIL_TEST" && "$*" == *"$FAIL_TEST"* ]]; then return 37; fi; }\n'
        calls = '\nrun_operational_contract_tests\nrun_packaging_contract_tests\necho contracts-complete\n'
        tests = ('local-letai-provider-bridge.test.sh', 'build-wasm-module.test.sh')
        for failing in ('', *tests):
            result = subprocess.run(['bash', '-c', setup + functions + calls], text=True,
                                    capture_output=True, env=dict(os.environ, FAIL_TEST=failing))
            if failing:
                self.assertEqual(result.returncode, 37, result.stdout + result.stderr)
                self.assertNotIn('contracts-complete', result.stdout)
            else:
                self.assertEqual(result.returncode, 0, result.stderr)
                for test in tests:
                    self.assertEqual(result.stdout.count('./scripts/' + test), 1, result.stdout)

    def test_operational_contracts_parallel_pool_and_adapter_shards(self):
        driver = (ROOT / 'scripts/ci-tests.sh').read_text()
        names = ('run_operational_identity_contract_tests', 'run_operational_node_contract_tests',
                 'run_operational_contract_tests')
        functions = '\n'.join(re.search(r'^' + name + r'\(\) \{\n.*?^\}', driver, re.M | re.S).group()
                              for name in names)
        with tempfile.TemporaryDirectory() as temporary:
            active_dir = Path(temporary) / 'active'
            active_dir.mkdir()
            max_seen = Path(temporary) / 'max_seen'
            setup = (
                'set -euo pipefail\n'
                'run_provider_remote_https_smoke() { run ./scripts/smoke-a.test.sh; }\n'
                f'run() {{\n'
                f'  local marker="{active_dir}/$$.$BASHPID"\n'
                f'  : >"$marker"\n'
                f'  local count\n'
                f'  count=$(ls -1 "{active_dir}" | wc -l)\n'
                f'  printf "%s\\n" "$count" >>"{max_seen}"\n'
                f'  sleep 0.05\n'
                f'  rm -f "$marker"\n'
                f'  echo "OUT:$*"\n'
                f'  echo "ERR:$*" >&2\n'
                f'  if [[ -n "${{FAIL_TEST:-}}" && "$*" == *"$FAIL_TEST"* ]]; then return 41; fi\n'
                f'}}\n'
            )
            ok = subprocess.run(
                ['bash', '-c', setup + functions + '\nrun_operational_contract_tests\n'],
                text=True, capture_output=True,
                env=dict(os.environ, OASIS7_OPERATIONAL_CONTRACT_JOBS='4', FAIL_TEST=''),
            )
            self.assertEqual(ok.returncode, 0, ok.stderr)
            peaks = [int(line.strip()) for line in max_seen.read_text().splitlines() if line.strip()]
            self.assertGreaterEqual(max(peaks), 2)
            adapter_subsuites = sorted(
                (ROOT / 'scripts').glob('p2p-public-testnet-full-network-clean-room-adapter*.test.py')
            )
            self.assertGreaterEqual(len(adapter_subsuites), 2)
            for subsuite in adapter_subsuites:
                self.assertIn(
                    f'OUT:python3 ./scripts/{subsuite.name}',
                    ok.stdout,
                )
            self.assertNotIn('--shard', ok.stdout)
            self.assertIn('OUT:./scripts/smoke-a.test.sh', ok.stdout)
            self.assertIn('ERR:./scripts/smoke-a.test.sh', ok.stderr)

            failed = subprocess.run(
                ['bash', '-c', setup + functions + '\nrun_operational_contract_tests\necho never\n'],
                text=True, capture_output=True,
                env=dict(os.environ, OASIS7_OPERATIONAL_CONTRACT_JOBS='4',
                         FAIL_TEST='local-letai-provider-bridge.test.sh'),
            )
            self.assertEqual(failed.returncode, 41, failed.stdout + failed.stderr)
            self.assertNotIn('never', failed.stdout)

            max_seen.write_text('')
            serial = subprocess.run(
                ['bash', '-c', setup + functions + '\nrun_operational_contract_tests\n'],
                text=True, capture_output=True,
                env={k: v for k, v in dict(os.environ, OASIS7_LOCAL_CHECK='1', FAIL_TEST='').items()
                     if k != 'OASIS7_OPERATIONAL_CONTRACT_JOBS'},
            )
            self.assertEqual(serial.returncode, 0, serial.stderr)
            serial_peaks = [int(line.strip()) for line in max_seen.read_text().splitlines() if line.strip()]
            self.assertEqual(max(serial_peaks), 1)

        import importlib.util

        def _flatten(suite):
            cases = []
            for item in suite:
                if isinstance(item, unittest.TestSuite):
                    cases.extend(_flatten(item))
                else:
                    cases.append(item)
            return cases

        discovered_ids: list[str] = []
        subsuite_loader = unittest.TestLoader()
        for idx, subsuite in enumerate(adapter_subsuites):
            mod_name = f'clean_room_adapter_subsuite_mod_{idx}'
            spec = importlib.util.spec_from_file_location(mod_name, subsuite)
            mod = importlib.util.module_from_spec(spec)
            sys.modules[mod_name] = mod
            try:
                spec.loader.exec_module(mod)
                loaded = subsuite_loader.loadTestsFromModule(mod)
                cases = _flatten(loaded)
            finally:
                sys.modules.pop(mod_name, None)
            self.assertTrue(cases, f'no tests discovered in {subsuite.name}')
            self.assertFalse(
                any(case.__class__.__name__ == '_FailedTest' for case in cases),
                f'failed test discovery in {subsuite.name}',
            )
            for case in cases:
                discovered_ids.append(case.id().removeprefix(f'{mod_name}.'))
        self.assertEqual(len(discovered_ids), len(set(discovered_ids)), 'duplicate test ids across adapter subsuites')

    def test_doc_checker_and_compile_metrics_contracts_use_parallel_pool(self):
        driver = (ROOT / 'scripts/ci-tests.sh').read_text()
        names = (
            'run_parallel_contract_commands',
            'run_doc_checker_contract_tests',
            'run_procedural_doc_contract',
            'run_system_design_traceability_tests',
            'run_compile_metrics_contract_tests',
        )
        functions = '\n'.join(re.search(r'^' + name + r'\(\) \{\n.*?^\}', driver, re.M | re.S).group()
                              for name in names)
        with tempfile.TemporaryDirectory() as temporary:
            active_dir = Path(temporary) / 'active'
            active_dir.mkdir()
            max_seen = Path(temporary) / 'max_seen'
            setup = (
                'set -euo pipefail\n'
                'repo_root="."\n'
                f'run() {{\n'
                f'  local marker="{active_dir}/$$.$BASHPID"\n'
                f'  : >"$marker"\n'
                f'  local count\n'
                f'  count=$(ls -1 "{active_dir}" | wc -l)\n'
                f'  printf "%s\\n" "$count" >>"{max_seen}"\n'
                f'  sleep 0.05\n'
                f'  rm -f "$marker"\n'
                f'  echo "OUT:$*"\n'
                f'  echo "ERR:$*" >&2\n'
                f'  if [[ -n "${{FAIL_TEST:-}}" && "$*" == *"$FAIL_TEST"* ]]; then return 41; fi\n'
                f'}}\n'
            )
            ok_doc = subprocess.run(
                ['bash', '-c', setup + functions + '\nrun_doc_checker_contract_tests\n'],
                text=True, capture_output=True,
                env=dict(os.environ, OASIS7_DOC_CHECKER_CONTRACT_JOBS='4', OASIS7_LOCAL_CHECK='0', FAIL_TEST=''),
            )
            self.assertEqual(ok_doc.returncode, 0, ok_doc.stderr)
            doc_peaks = [int(line.strip()) for line in max_seen.read_text().splitlines() if line.strip()]
            self.assertGreaterEqual(max(doc_peaks), 2)
            for script in (
                'doc-governance-check.test.sh',
                'product-doc-governance-check.test.py',
                'product-doc-content-check.test.py',
                'system-design-traceability-check.test.py',
                'resolve-product-doc-ci-range.test.py',
            ):
                self.assertIn(script, ok_doc.stdout)
                self.assertIn(script, ok_doc.stderr)

            failed_doc = subprocess.run(
                ['bash', '-c', setup + functions + '\nrun_doc_checker_contract_tests\necho never\n'],
                text=True, capture_output=True,
                env=dict(os.environ, OASIS7_DOC_CHECKER_CONTRACT_JOBS='4', OASIS7_LOCAL_CHECK='0',
                         FAIL_TEST='doc-core-content-check.test.py'),
            )
            self.assertEqual(failed_doc.returncode, 41, failed_doc.stdout + failed_doc.stderr)
            self.assertNotIn('never', failed_doc.stdout)

            max_seen.write_text('')
            ok_metrics = subprocess.run(
                ['bash', '-c', setup + functions + '\nrun_compile_metrics_contract_tests\n'],
                text=True, capture_output=True,
                env=dict(os.environ, OASIS7_COMPILE_METRICS_CONTRACT_JOBS='3', OASIS7_LOCAL_CHECK='0', FAIL_TEST=''),
            )
            self.assertEqual(ok_metrics.returncode, 0, ok_metrics.stderr)
            metrics_peaks = [int(line.strip()) for line in max_seen.read_text().splitlines() if line.strip()]
            self.assertGreaterEqual(max(metrics_peaks), 2)
            for script in ('ci-compile-metrics-contract.test.sh', 'ci-ecs-build-metrics.test.py', 'ci-ecs-cache.test.py'):
                self.assertIn(script, ok_metrics.stdout)

            max_seen.write_text('')
            serial = subprocess.run(
                ['bash', '-c', setup + functions + '\nrun_doc_checker_contract_tests\nrun_compile_metrics_contract_tests\n'],
                text=True, capture_output=True,
                env={k: v for k, v in dict(os.environ, OASIS7_LOCAL_CHECK='1', FAIL_TEST='').items()
                     if k not in ('OASIS7_DOC_CHECKER_CONTRACT_JOBS', 'OASIS7_COMPILE_METRICS_CONTRACT_JOBS')},
            )
            self.assertEqual(serial.returncode, 0, serial.stderr)
            serial_peaks = [int(line.strip()) for line in max_seen.read_text().splitlines() if line.strip()]
            self.assertEqual(max(serial_peaks), 1)
            self.assertIn('ci-python-control-contracts.py local-contract', serial.stdout)

    def test_local_public_testnet_build_uses_selected_repo_in_dev_and_ci(self):
        with tempfile.TemporaryDirectory() as directory:
            temporary = Path(directory)
            repo = (temporary / 'build repo').resolve()
            (repo / 'scripts').mkdir(parents=True)
            bin_dir = temporary / 'build-bin'
            bin_dir.mkdir()
            for path, mode in ((repo / 'scripts/cargo-dev.sh', 'wrapper'), (bin_dir / 'cargo', 'raw')):
                path.write_text('#!/usr/bin/env bash\n{ printf "' + mode + '\\n"; printf "%s\\n" "$PWD" "$@"; } > "$BUILD_TRACE"\nexit "${BUILD_EXIT:-0}"\n')
                path.chmod(0o755)
            source = (ROOT / 'scripts/run-local-public-testnet-letai-test-environment.sh').read_text()
            function = re.search(r'^build_bins\(\) \{\n.*?^\}', source, re.M | re.S).group()
            script = 'set -euo pipefail\nROOT_DIR="$1"; source "$2"; BUILD=1; log() { :; }\n' + function + '\nbuild_bins\n'
            for ci, raw in (('', '0'), ('1', '0'), ('true', '0'), ('', '1')):
                trace = temporary / ('build-trace-' + (ci or 'dev') + raw)
                env = dict(os.environ, CI=ci, OASIS7_FORCE_RAW_CARGO=raw,
                           OASIS7_CARGO_DEV_REPO_ROOT=str(temporary / 'unrelated-repo'), BUILD_TRACE=str(trace),
                           PATH=str(bin_dir) + os.pathsep + os.environ['PATH'])
                for code in (0, 37):
                    result = subprocess.run(['bash', '-c', script, '_', str(repo), str(ROOT / 'scripts/cargo-dev-lib.sh')],
                                            cwd=temporary, env=dict(env, BUILD_EXIT=str(code)), capture_output=True, text=True)
                    assert result.returncode == code, result.stderr
                    records = trace.read_text().splitlines()
                    assert records[:2] == [('raw' if ci or raw == '1' else 'wrapper'), str(repo)], records
                    assert 'oasis7_viewer_live' in records, records

    def test_engineering_observation_policy_is_exact_and_never_exempts_required_steps(self):
        import engineering_telemetry as bindings
        from unittest.mock import patch
        self.assertNotIn('continue-on-error:', without_optional_dependency_cache_install(WORKFLOW))
        changed = dict(CONFIG)
        changed['engineering_job_observation'] = dict(CONFIG['engineering_job_observation'], emitter_sha256='0' * 64)
        with patch.dict(globals(), CONFIG=changed):
            self.assertIn('continue-on-error:', without_optional_dependency_cache_install(WORKFLOW))
        absent = {key: value for key, value in CONFIG.items() if key != 'engineering_job_observation'}
        for policy in (absent, changed):
            with self.assertRaises(ValueError):
                validate_candidate_graph(WORKFLOW, policy)
        for mutation in (bindings.workflow_step().replace('timeout-minutes: 1', 'timeout-minutes: 10'),
                         bindings.workflow_step().replace('PY_OBSERVATION\n', 'PY_OBSERVATION\n          false\n')):
            self.assertIn('continue-on-error:', without_optional_dependency_cache_install(WORKFLOW.replace(bindings.workflow_step(), mutation)))
        required = WORKFLOW.replace('      - name: Validate all expected results\n',
                                    '      - name: Validate all expected results\n        continue-on-error: true\n')
        with self.assertRaises(ValueError):validate_candidate_graph(required,CONFIG)

    def test_engineering_telemetry_entry_requires_complete_nonempty_suites(self):
        driver = (ROOT / 'scripts/ci-tests.sh').read_text()
        function = re.search(r'^run_engineering_telemetry_contract_tests\(\) \{\n.*?^\}', driver, re.M | re.S)
        self.assertIsNotNone(function)
        governance = re.search(r'^run_workflow_governance_remaining_contract_tests\(\) \{\n.*?^\}', driver, re.M | re.S)
        self.assertIsNotNone(governance)
        self.assertEqual(governance.group().count('  run_engineering_telemetry_contract_tests'), 1)
        paths = (
            'scripts/engineering_telemetry.py',
            'scripts/engineering_telemetry_adapters.py',
            'scripts/tests/fixtures/engineering-telemetry/input.json',
            'scripts/tests/test_engineering_telemetry.py',
            'scripts/tests/test_engineering_telemetry_adapters.py',
        )
        cases = [('absent', None), ('complete', None), ('all-skipped', None),
                 ('some-skipped', None)]
        cases += [(kind, path) for kind in ('missing', 'directory', 'symlink') for path in paths]
        cases += [(kind, path) for kind in ('empty', 'failing', 'skipped') for path in paths[-2:]]
        for kind, changed in cases:
            with self.subTest(kind=kind, path=changed), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                (root / 'scripts').mkdir()
                for archive in ('package-source-plan.test.py', 'safe-git-archive.test.py'):
                    (root / 'scripts' / archive).write_text('pass\n')
                if kind != 'absent':
                    for path in paths:
                        target = root / path
                        target.parent.mkdir(parents=True, exist_ok=True)
                        if path == changed and kind == 'missing':
                            continue
                        if path == changed and kind == 'directory':
                            target.mkdir()
                            continue
                        if path == changed and kind == 'symlink':
                            target.symlink_to(root / 'missing-target')
                            continue
                        content = '# synthetic telemetry input\n'
                        if path in paths[-2:] and not (path == changed and kind == 'empty'):
                            content = ("import unittest\nfrom pathlib import Path\n"
                                       "class Contract(unittest.TestCase):\n"
                                       "    def test_runs(self):\n"
                                       "        Path(%r).write_text('once')\n"
                                       "        self.assertTrue(%r)\n" %
                                       (target.name + '.ran', not (path == changed and kind == 'failing')))
                            if kind == 'all-skipped' or (kind == 'skipped' and path == changed):
                                content = content.replace('    def test_runs', '    @unittest.skip("fixture only")\n    def test_runs')
                            if kind == 'some-skipped':
                                content += ('    @unittest.skip("platform unavailable")\n'
                                            '    def test_platform_specific(self): pass\n')
                        target.write_text(content)
                shell = ('set -euo pipefail\nrun() { "$@"; }\n'
                         'run_workflow_governance_baseline_contract_tests() { echo governance-baseline; }\n'
                         'run_workflow_governance_operational_contract_tests() { echo governance-operational; }\n'
                         + function.group() + '\n' + governance.group()
                         + '\nrun_workflow_governance_remaining_contract_tests\n')
                result = subprocess.run(['bash', '-c', shell], cwd=root, capture_output=True, text=True)
                if kind in ('absent', 'complete', 'some-skipped'):
                    self.assertEqual(result.returncode, 0, result.stderr)
                    self.assertEqual(result.stdout.splitlines()[-2:],
                                     ['governance-baseline', 'governance-operational'])
                else:
                    self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
                    self.assertNotIn('governance-baseline', result.stdout)
                    self.assertNotIn('governance-operational', result.stdout)
                if kind == 'absent':
                    self.assertIn('not introduced', result.stdout)
                if kind in ('complete', 'failing'):
                    for path in paths[-2:]:
                        self.assertTrue((root / (Path(path).name + '.ran')).is_file(), result.stderr)
                    self.assertIn('Ran 2 tests', result.stderr)
                if kind == 'empty':
                    self.assertIn('no tests in', result.stderr)
                if kind == 'some-skipped':
                    self.assertIn('skipped=2', result.stderr)
                if kind in ('skipped', 'all-skipped'):
                    for path in ((changed,) if changed else paths[-2:]):
                        self.assertIn('no passing tests in ' + Path(path).name, result.stderr)

    def test_local_signer_and_new_contracts_are_connected_once(self):
        fleet = JOBS['fleet-health']
        self.assertRegex(fleet, r"if:\s*(?:>-\s*)?runner\.os == 'macOS'")
        self.assertIn('TMPDIR: /private/tmp', fleet)
        self.assertIn('python3 scripts/ci-local-signer-tests.py --require-darwin', fleet)
        self.assertLess(fleet.index('rustup default "$RUST_TOOLCHAIN"'),
                        fleet.index('python3 scripts/ci-local-signer-tests.py --require-darwin'))
        driver = (ROOT / 'scripts/ci-tests.sh').read_text()
        packaging = driver.split('run_packaging_contract_tests() {', 1)[1].split('\n}', 1)[0]
        self.assertEqual(packaging.count('run python3 ./scripts/ci-local-signer-tests.py'), 1)
        governance = driver.split('run_workflow_governance_baseline_contract_tests() {', 1)[1].split('\n}', 1)[0]
        for test in ('plan-wasm-determinism-scope.test.sh', 'ensure-wasm-bindgen-cli.test.sh'):
            self.assertEqual(governance.count(test), 1)
        self.assertEqual(CONFIG['workflow_governance_python_tests'].count('scripts/ci-local-signer-tests.test.py'), 1)

    def test_stable_install_is_minimal_with_required_components(self):
        self.assertNotIn('--profile default', WORKFLOW)
        for name, job in JOBS.items():
            if 'rustup toolchain install' in job:
                if name in ('formal-test-build', 'formal-test-build-trusted'):
                    # Merge needs the real compiler identity but cannot run
                    # build/lint commands. Existing jobs retain their checks.
                    self.assertIn('rustup toolchain install "$RUST_TOOLCHAIN" --profile minimal\n', job)
                    self.assertIn('rustup default "$RUST_TOOLCHAIN"', job)
                    self.assertIn('ci-rust-cache-toolchains.py "$RUST_TOOLCHAIN"\n', job)
                    self.assertIn('ci-test-artifacts.py merge-formal ', job)
                    self.assertNotRegex(job, r'cargo (?:build|test|check|clippy|fmt)\b')
                    self.assertNotIn('ci-test-artifacts.py build ', job)
                else:
                    self.assertRegex(job, r'rustup toolchain install .*RUST_TOOLCHAIN.* --profile minimal --component rustfmt --component clippy')

    def test_wasm_dynamic_matrix_and_job_failure_boundaries(self):
        workflow = (ROOT / '.github/workflows/wasm-determinism-gate.yml').read_text()
        jobs = dict(re.findall(r'^  ([a-z][a-z0-9-]*):\n(.*?)(?=^  [a-z][a-z0-9-]*:|\Z)', workflow.split('\njobs:\n', 1)[1], re.M | re.S))
        expression = '${{ fromJSON(needs.plan-wasm-determinism-scope.outputs.selected_module_sets_json) }}'
        self.assertIn('selected_module_sets_json: ${{ steps.scope.outputs.selected_module_sets_json }}', jobs['plan-wasm-determinism-scope'])
        self.assertNotIn('verify-wasm-determinism', jobs)
        job = jobs['collect-wasm-summaries']
        self.assertIn('    name: verify-wasm-determinism (${{ matrix.module_set }})\n', job)
        self.assertIn('module_set: ' + expression, job)
        guard = job.split('    if: >-', 1)[1].split('    strategy:', 1)[0]
        self.assertIn("needs.plan-wasm-determinism-scope.result == 'success'", guard)
        self.assertIn("outputs.scope == 'partial'", guard)
        self.assertIn("outputs.scope == 'all'", guard)
        self.assertIn('fail-fast: false', job)
        self.assertNotIn('Scope note', job)
        self.assertNotIn('outputs.run_m', job)
        for required_step in (
            'Upload ${{ matrix.module_set }} summary artifact',
            'Stage ${{ matrix.module_set }} summaries for evidence verify',
            'Verify ${{ matrix.module_set }} multi-runner summaries and write evidence report',
            'Upload ${{ matrix.module_set }} release evidence report',
        ):
            self.assertIn('      - name: ' + required_step + '\n', job)
        execution = re.sub(r'^      - name: Preserve failed candidate build evidence\n.*?(?=^      - |\Z)',
                           '', job, flags=re.M | re.S)
        self.assertNotIn('        if:', execution)
        self.assertEqual(job.count('if-no-files-found: error'), 2)
        evidence = re.search(r'^      - name: Preserve failed candidate build evidence\n.*?(?=^      - |\Z)',
                             job, re.M | re.S).group()
        self.assertIn('        if: failure()\n', evidence)
        self.assertIn('uses: actions/upload-artifact@', evidence)
        self.assertIn('include-hidden-files: true', evidence)
        self.assertIn('if-no-files-found: warn', evidence)
        self.assertNotIn('        run:', evidence)
        self.assertEqual(re.findall(r'^            (.+)$', evidence, re.M), [
            '.tmp/builtin-wasm-sync-modules/*.wasm',
            '.tmp/builtin-wasm-sync-modules/*.metadata.json',
            '.tmp/builtin-wasm-sync-modules/*.build-receipt.json',
        ])
        self.assertNotIn('continue-on-error:', workflow)
        for filename in ('wasm-determinism-gate.yml', 'ci-base-images.yml'):
            workflow = (ROOT / '.github/workflows' / filename).read_text()
            self.assertNotRegex(workflow, r'(?m)^  push:')
            self.assertIn("group: ${{ github.workflow }}-${{ github.event_name == 'pull_request' && format('pr-{0}', github.event.pull_request.number) || format('run-{0}', github.run_id) }}", workflow)
            self.assertIn("cancel-in-progress: ${{ github.event_name == 'pull_request' }}", workflow)
            self.assertIn('types: [opened, synchronize, reopened, ready_for_review]', workflow)


if __name__ == '__main__':
    unittest.main()
