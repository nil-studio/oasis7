#!/usr/bin/env python3
"""Self-contained core workflow preview, distfs, matrix, and recovery contract tests (part 3)."""
import hashlib
import itertools
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
import textwrap
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


class WorkflowCorePart3(unittest.TestCase):
    def test_preview_invocation_passes_plan_only_to_its_actual_consumers(self):
        step = next(step for step in re.findall(r'^      - .*?(?=^      - |\Z)',
                                                JOBS['pr-fast-preview'], re.M | re.S)
                    if 'name: Execute actual preview group' in step)
        script = textwrap.dedent(step.split('        run: |\n', 1)[1])
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / 'scripts').mkdir()
            (root / 'scripts/ci-tests.sh').write_text(
                '#!/usr/bin/env bash\nprintf "%s\\0" "$@" > "$ARGV_LOG"\n'
                + 'cat "$OASIS7_CI_ARCHITECTURE_PLAN_FILE" > "$PLAN_LOG"\nexit "$DRIVER_EXIT"\n')
            for sample, groups in (
                ('runtime-world', ('baseline', 'rust_baseline', 'oasis7_required', 'consensus', 'distfs', 'net', 'node')),
                ('game-client-private-unix', ('baseline', 'rust_baseline', 'oasis7_required')),
                ('world-state-unit-tests', ('baseline', 'rust_baseline', 'oasis7_required')),
            ):
                selection = root / 'previews' / sample / 'selection.json'
                selection.parent.mkdir(parents=True)
                data = {'sample': sample, 'reasons': ["quoted ' \" \\\n$(touch transport-must-not-execute)\nPY\n" * 4000]}
                selection.write_text(json.dumps(data))
                self.assertGreater(selection.stat().st_size, 128 * 1024)
                for group in groups:
                    partitions = ('oasis7_runtime', 'oasis7_world_state') if (
                        sample == 'runtime-world' and group == 'oasis7_required') else ('all',)
                    for partition in partitions:
                        for code in (0, 19):
                            result = subprocess.run(['bash', '-c', script], cwd=root, text=True, capture_output=True,
                                env=dict(os.environ, PREVIEW_GROUP=group, PREVIEW_SAMPLE=sample,
                                         PREVIEW_PARTITION=partition, RUNNER_TEMP=str(root),
                                         GITHUB_WORKSPACE=str(root), ARGV_LOG=str(root / 'args'),
                                         PLAN_LOG=str(root / 'plan-log'), DRIVER_EXIT=str(code)))
                            args = (root / 'args').read_text().strip('\0').split('\0')
                            expected = ['required', '--group', group]
                            if group in ('rust_baseline', 'oasis7_required', 'distfs'):
                                expected.extend(['--plan', str(selection)])
                            if group == 'oasis7_required':
                                expected.extend(['--ordinary-partition', partition])
                            expected.extend(['--repo-root', str(root)])
                            with self.subTest(sample=sample, group=group, partition=partition, driver_exit=code):
                                self.assertEqual(args, expected)
                                self.assertEqual(json.loads((root / 'plan-log').read_text()), data)
                                self.assertEqual(result.returncode, code, result.stdout + result.stderr)
                                self.assertFalse((root / 'transport-must-not-execute').exists())

    def test_distfs_workflow_binds_new_handoffs_and_preserves_old_source_ref_cli(self):
        job = JOBS['distfs']
        self.assertIn('ref: ${{ needs.select.outputs.tested_sha }}', job)
        self.assertIn('name: ${{ needs.select.outputs.selection_artifact }}', job)
        self.assertIn('SOURCE_SHA: ${{ needs.select.outputs.source_sha }}', job)
        step = next(step for step in re.findall(r'^      - .*?(?=^      - |\Z)', job, re.M | re.S)
                    if 'name: Execute selected group' in step)
        script = textwrap.dedent(step.split('        run: |\n', 1)[1])
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / 'scripts').mkdir()
            (root / 'scripts/ci-tests.sh').write_text(
                '#!/usr/bin/env bash\nprintf "%s\\0" "$@" > "$ARGV_LOG"\n'
                'for arg in "$@"; do if [[ "$OLD_DRIVER" == 1 && "$arg" == --plan ]]; then exit 2; fi; done\n'
                'exit "$DRIVER_EXIT"\n')
            plans = [{}, {'execution_profile': 'candidate-full'},
                     {'distfs_clippy_contract_version': 1, 'distfs_clippy_group': 'rust_baseline'},
                     {'distfs_clippy_contract_version': 2}, {'distfs_clippy_group': 'other'}]
            for plan in plans:
                handoff = bool({'distfs_clippy_contract_version', 'distfs_clippy_group'}.intersection(plan))
                (root / 'selection.json').write_text(json.dumps(plan))
                for code in (0, 19):
                    result = subprocess.run(['bash', '-c', script], cwd=root, text=True, capture_output=True,
                        env=dict(os.environ, RUNNER_TEMP=str(root), GITHUB_WORKSPACE=str(root),
                                 ARGV_LOG=str(root / 'args'), OLD_DRIVER='0' if handoff else '1', DRIVER_EXIT=str(code)))
                    args = (root / 'args').read_text().strip('\0').split('\0')
                    expected = ['required', '--group', 'distfs']
                    if handoff:
                        expected += ['--plan', str(root / 'selection.json')]
                    expected += ['--repo-root', str(root)]
                    with self.subTest(plan=plan, code=code):
                        self.assertEqual(args, expected)
                        self.assertEqual(result.returncode, code, result.stdout + result.stderr)
            for malformed in ('[', '[]'):
                (root / 'selection.json').write_text(malformed)
                (root / 'args').unlink(missing_ok=True)
                result = subprocess.run(['bash', '-c', script], cwd=root, text=True, capture_output=True,
                    env=dict(os.environ, RUNNER_TEMP=str(root), GITHUB_WORKSPACE=str(root),
                             ARGV_LOG=str(root / 'args'), OLD_DRIVER='0', DRIVER_EXIT='0'))
                self.assertNotEqual(result.returncode, 0)
                self.assertFalse((root / 'args').exists())

    def test_distfs_workflow_transports_exact_inventory_without_extending_old_cli(self):
        step = next(step for step in re.findall(r'^      - .*?(?=^      - |\Z)', JOBS['distfs'], re.M | re.S)
                    if 'name: Execute selected group' in step)
        script = textwrap.dedent(step.split('        run: |\n', 1)[1])
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / 'scripts').mkdir()
            (root / 'scripts/ci-tests.sh').write_text(
                '#!/usr/bin/env bash\nprintf "%s\\0" "$@" > "$ARGV_LOG"\n'
                'cat "$OASIS7_CI_DISTFS_PLAN_FILE" > "$PLAN_LOG"\nexit 19\n')
            selection = dict(fast_plan_version=1, scope='targeted', execution_profile='pr-fast',
                             fast_test_inventory_version=1, ordinary_fast_targets=[dict(
                                 package='oasis7_runtime', target_kind='lib', target_name='oasis7_runtime',
                                 default_features=False, features=['test_tier_required'], cases=['retained'])])
            payload = json.dumps(selection, indent=2) + '\n'
            (root / 'selection.json').write_text(payload)
            result = subprocess.run(['bash', '-c', script], cwd=root, text=True, capture_output=True,
                env=dict(os.environ, RUNNER_TEMP=str(root), GITHUB_WORKSPACE=str(root),
                         ARGV_LOG=str(root / 'args'), PLAN_LOG=str(root / 'observed-plan')))
            self.assertEqual(result.returncode, 19, result.stdout + result.stderr)
            self.assertEqual((root / 'observed-plan').read_text(), payload)
            self.assertEqual((root / 'args').read_text().strip('\0').split('\0'),
                             ['required', '--group', 'distfs', '--repo-root', str(root)])

    def test_ordinary_matrix_keeps_the_trusted_complete_partition_list_and_failure_boundary(self):
        job = JOBS['oasis7-required']
        self.assertTrue(validate_candidate_graph(WORKFLOW, CONFIG))
        self.assertTrue(
            'needs: [select, oasis7-test-build-candidate, oasis7-test-build-trusted]' in job
            or 'needs: [select, fast-preflight, oasis7-test-build-candidate, oasis7-test-build-trusted]' in job
        )
        self.assertIn('oasis7-required', JOBS['required-gate'])
        for damaged in (
            job.replace('fail-fast: false', 'fail-fast: true'),
            job.replace('${{ fromJSON(needs.select.outputs.ordinary_partitions) }}', '[oasis7_runtime]'),
            job.replace('      matrix:\n', '      matrix:\n        exclude: [{partition: oasis7_world_state}]\n'),
            job.replace('--ordinary-partition "$ORDINARY_PARTITION"', ''),
        ):
            with self.subTest(damaged=damaged[:120]), self.assertRaises(ValueError):
                validate_candidate_graph(WORKFLOW.replace(job, damaged), CONFIG)
        fallback = "ordinary_partitions: ${{ steps.select.outputs.ordinary_partitions || '[\"all\"]' }}"
        with self.assertRaises(ValueError):
            validate_candidate_graph(WORKFLOW.replace(fallback, "ordinary_partitions: '[\"oasis7_runtime\"]'"), CONFIG)

    def test_support_matrix_retains_all_selected_profiles_and_failure_boundary(self):
        job = JOBS['workspace-support-legacy']
        self.assertTrue(validate_candidate_graph(WORKFLOW, CONFIG))
        for damaged in (
            job.replace('fail-fast: false', 'fail-fast: true'),
            job.replace('${{ fromJSON(needs.select.outputs.workspace_support_partitions) }}', '[other]'),
            job.replace('      matrix:\n', '      matrix:\n        exclude: [{partition: custody}]\n'),
            job.replace('--support-partition "$SUPPORT_PARTITION"', ''),
        ):
            with self.subTest(damaged=damaged[:120]), self.assertRaises(ValueError):
                validate_candidate_graph(WORKFLOW.replace(job, damaged), CONFIG)

    def test_cache_maintenance_jobs_use_readonly_published_runtime_environment(self):
        release = workflow_jobs((ROOT / '.github/workflows/release-packages.yml').read_text())
        ecs = workflow_jobs((ROOT / '.github/workflows/ecs-packages.yml').read_text())
        maintenance_jobs = (
            ('rust/bootstrap', JOBS['cache-bootstrap-maintenance']),
            ('rust/maintenance', JOBS['cache-maintenance']),
            ('release/bootstrap', release['cache-bootstrap-maintenance']),
            ('release/maintenance', release['cache-maintenance']),
            ('ecs/maintenance', ecs['cache-maintenance']),
        )
        for name, job in maintenance_jobs:
            with self.subTest(job=name):
                self.assertIn('image: ghcr.io/nil-studio/oasis7/ci-runtime@sha256:3984caa42d82963f93af752bf79e3a07838f2c94279cfbb2deeb08f715cd62c9', job)
                self.assertIn('options: --user 1001:1001 --init', job)
                self.assertIn('packages: read', job)
                self.assertIn('sys.version_info[:2] == (3, 12)', job)
                self.assertIn('timeout-minutes: 5', job)
                self.assertIn('cache-mode: none', job)
                self.assertIn('actions: write', job)
                self.assertIn('group: ci-cache-maintenance-main', job)
                self.assertIn('cancel-in-progress: false', job)
                self.assertNotIn('privileged', job)
                self.assertNotIn('volumes:', job)
                self.assertNotIn('apt-get', job)
                self.assertLess(job.index('persist-credentials: false'), job.index('Verify the preinstalled'))
                self.assertLess(job.index('Verify the preinstalled'), job.index('CACHE_MAINTENANCE_TOKEN:'))

    def test_workspace_maintenance_recovers_without_a_new_confirmed_save(self):
        job = JOBS.get('cache-maintenance', '')
        self.assertIn('needs: [select, oasis7-test-build-trusted, formal-debug-build-trusted, formal-endpoint-build-trusted]', job)
        self.assertNotIn('full-regression', job)
        self.assertNotIn('formal-test-build-trusted', job)
        self.assertIn('always() && !cancelled()', job.split('    steps:', 1)[0])
        for producer in ('oasis7-test-build-trusted', 'formal-debug-build-trusted', 'formal-endpoint-build-trusted'):
            self.assertIn(f"needs.{producer}.result == 'success'", job)
            self.assertNotIn(f"needs.{producer}.outputs.cache_confirmed", job.split('    steps:', 1)[0])
        for condition in ("github.ref == 'refs/heads/main'", "github.workflow_sha == github.sha",
                          "github.event_name == 'push'", "github.event_name == 'schedule'", "inputs.run_mode == 'full'"):
            self.assertIn(condition, job.split('    steps:', 1)[0])
        self.assertIn('cache-mode: none', job)
        self.assertIn('actions: write', job)
        self.assertIn('cancel-in-progress: false', job)
        self.assertIn('group: ci-cache-maintenance-main', job)
        self.assertIn('ref: ${{ needs.select.outputs.tested_sha }}', job)
        self.assertIn('CACHE_MAINTENANCE_TOKEN: ${{ github.token }}', job)
        self.assertIn('ci-cache-maintenance.py --recover --apply', job)
        self.assertNotIn('Swatinem/rust-cache', job)
        self.assertNotIn('actions/cache/', job)
        self.assertNotIn('cache-maintenance', JOBS['required-gate'])
        self.assertNotIn('cache-maintenance', JOBS['explicit-full-gate'])
        for name, body in JOBS.items():
            if name not in ('cache-maintenance', 'cache-bootstrap-maintenance'):
                self.assertNotIn('actions: write', body, name)
        # Execute the exact selection shell, substituting only the API boundary.
        script = textwrap.dedent(job.split('        run: |\n', 1)[1])
        stub = 'python3() { printf "<%s>\\n" "$@"; };\n'
        for ordinary, debug, endpoint in itertools.product((False, True), repeat=3):
            values = dict(ORDINARY_RESULT='success' if ordinary else 'failure',
                          FORMAL_DEBUG_RESULT='success', FORMAL_ENDPOINT_RESULT='success',
                          ORDINARY_CONFIRMED='true', FORMAL_DEBUG_CONFIRMED='true' if debug else 'false',
                          FORMAL_ENDPOINT_CONFIRMED='true' if endpoint else 'false',
                          ORDINARY_KEY='ordinary-key', FORMAL_DEBUG_KEY='formal-debug-key',
                          FORMAL_ENDPOINT_KEY='formal-endpoint-key')
            result = subprocess.run(['bash', '-c', stub + script], text=True, capture_output=True,
                                    env=dict(os.environ, **values))
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual('<ordinary-key>' in result.stdout, ordinary)
            self.assertEqual('<formal-debug-key>' in result.stdout, debug)
            self.assertEqual('<formal-endpoint-key>' in result.stdout, endpoint)
            self.assertIn('<--recover>', result.stdout)
            self.assertIn('<--apply>', result.stdout)

    def test_recovery_selection_preserves_success_cancellation_and_trust_boundaries(self):
        ecs = workflow_jobs((ROOT / '.github/workflows/ecs-packages.yml').read_text())
        for scope, job in (('rust', JOBS['cache-maintenance']), ('ecs', ecs['cache-maintenance'])):
            header = job.split('    steps:', 1)[0]
            condition = header.split('    if:', 1)[1].split('    cache-mode:', 1)[0].strip().removeprefix('>-')
            for ordinary in ('success', 'failure', 'cancelled', 'skipped'):
                for formal in ('success', 'failure', 'cancelled', 'skipped'):
                    for confirmed in ('true', 'false', ''):
                        for cancelled in (False, True):
                            for event, ref, mode, matching_workflow, trusted in (
                                    ('push', 'refs/heads/main', '', True, scope == 'rust'),
                                    ('schedule', 'refs/heads/main', '', True, scope == 'rust'),
                                    ('workflow_dispatch', 'refs/heads/main', 'full', True, True),
                                    ('workflow_dispatch', 'refs/heads/main', 'other', True, scope == 'ecs'),
                                    ('pull_request', 'refs/pull/1/merge', '', True, False),
                                    ('push', 'refs/heads/topic', '', True, False),
                                    ('workflow_dispatch', 'refs/heads/main', 'full', False, False)):
                                values = {
                                    'always()': True, 'cancelled()': cancelled,
                                    'needs.oasis7-test-build-trusted.result': ordinary,
                                    'needs.formal-debug-build-trusted.result': formal,
                                    'needs.formal-endpoint-build-trusted.result': formal,
                                    'needs.native-trusted.result': ordinary,
                                    'needs.oasis7-test-build-trusted.outputs.cache_confirmed': confirmed,
                                    'needs.formal-debug-build-trusted.outputs.cache_confirmed': confirmed,
                                    'needs.formal-endpoint-build-trusted.outputs.cache_confirmed': confirmed,
                                    'needs.native-trusted.outputs.cache_confirmed': confirmed,
                                    'needs.select.outputs.trusted_source': str(ref == 'refs/heads/main').lower(),
                                    'fromJSON(needs.select.outputs.plan).execution_profile': '',
                                    'fromJSON(needs.select.outputs.plan).fast_plan_version': 0,
                                    'fromJSON(needs.select.outputs.plan).ordinary_main_fast_cache_writer_version': 0,
                                    'fromJSON(needs.select.outputs.plan).fast_execution_version': 0,
                                    'fromJSON(needs.select.outputs.plan).ordinary_execution': '',
                                    'github.event_name': event, 'github.ref': ref, 'inputs.run_mode': mode,
                                    'github.workflow_sha': 'a' if matching_workflow else 'b', 'github.sha': 'a',
                                }
                                expression = condition
                                for token, value in values.items():
                                    expression = expression.replace(token, repr(value))
                                expression = re.sub(r'!(?!=)', 'not ', expression.replace('&&', ' and ').replace('||', ' or '))
                                success = ordinary == 'success' or (scope == 'rust' and formal == 'success')
                                with self.subTest(scope=scope, ordinary=ordinary, formal=formal,
                                                  confirmed=confirmed, cancelled=cancelled, event=event, ref=ref):
                                    actual = eval(' '.join(expression.split()), {'__builtins__': {}}, {})
                                    self.assertEqual(actual, success and trusted and not cancelled)

    def test_formal_component_cache_recovery_does_not_wait_for_merge_or_other_success(self):
        job = JOBS['cache-maintenance']
        condition = job.split('    if:', 1)[1].split('    cache-mode:', 1)[0].strip().removeprefix('>-')
        states = ('success', 'failure', 'cancelled', 'skipped', '', 'unknown')
        for debug, endpoint, cancelled in itertools.product(states, states, (False, True)):
            values = {
                'always()': True, 'cancelled()': cancelled,
                'needs.oasis7-test-build-trusted.result': 'failure',
                'needs.formal-debug-build-trusted.result': debug,
                'needs.formal-endpoint-build-trusted.result': endpoint,
                'needs.select.outputs.trusted_source': 'true',
                'fromJSON(needs.select.outputs.plan).execution_profile': 'periodic-full',
                'fromJSON(needs.select.outputs.plan).fast_plan_version': 1,
                'fromJSON(needs.select.outputs.plan).ordinary_main_fast_cache_writer_version': 0,
                'github.event_name': 'schedule', 'github.ref': 'refs/heads/main',
                'inputs.run_mode': '', 'github.workflow_sha': 'a', 'github.sha': 'a',
            }
            expression = condition
            for token, value in sorted(values.items(), key=lambda item: -len(item[0])):
                expression = expression.replace(token, repr(value))
            expression = re.sub(r'!(?!=)', 'not ', expression.replace('&&', ' and ').replace('||', ' or '))
            with self.subTest(debug=debug, endpoint=endpoint, cancelled=cancelled):
                self.assertEqual(bool(eval(' '.join(expression.split()), {'__builtins__': {}}, {})),
                                 not cancelled and 'success' in (debug, endpoint))

    def test_bootstrap_is_trusted_and_cannot_block_or_expand_build_selection(self):
        bootstrap = JOBS.get('cache-bootstrap-maintenance', '')
        self.assertIn('needs: select', bootstrap)
        self.assertIn('cache-mode: none', bootstrap)
        self.assertIn('actions: write', bootstrap)
        self.assertIn('group: ci-cache-maintenance-main', bootstrap)
        self.assertIn('cancel-in-progress: false', bootstrap)
        self.assertIn('ref: ${{ needs.select.outputs.tested_sha }}', bootstrap)
        self.assertIn('persist-credentials: false', bootstrap)
        self.assertIn('ci-cache-maintenance.py --recover --apply', bootstrap)
        for forbidden in ('--current-key', 'actions/cache/', 'Swatinem/rust-cache', 'cargo '):
            self.assertNotIn(forbidden, bootstrap)
        for name in ('oasis7-test-build-candidate', 'formal-debug-build', 'formal-endpoint-build',
                     'formal-test-build', 'formal-test-build-trusted', 'required-gate', 'explicit-full-gate'):
            self.assertNotIn('cache-bootstrap-maintenance', JOBS[name])
        for name in ('oasis7-test-build-trusted', 'formal-debug-build-trusted', 'formal-endpoint-build-trusted'):
            self.assertIn('needs: [select, cache-bootstrap-maintenance]', JOBS[name])
            self.assertIn('always() && !cancelled()', JOBS[name])
        for select in ('success', 'failure', 'skipped'):
            for cancelled in (False, True):
                for maintenance_result in ('success', 'failure', 'skipped'):
                    for ordinary, formal, scope in ((True, False, 'affected'), (False, True, 'affected'),
                                                   (False, False, 'affected'), (False, False, 'explicit-full'),
                                                   (False, False, 'full')):
                        for event, ref, mode, matching_workflow, trusted in (
                                ('push', 'refs/heads/main', '', True, True),
                                ('schedule', 'refs/heads/main', '', True, True),
                                ('workflow_dispatch', 'refs/heads/main', 'full', True, True),
                                ('workflow_dispatch', 'refs/heads/main', 'other', True, False),
                                ('pull_request', 'refs/pull/1/merge', '', True, False),
                                ('push', 'refs/heads/topic', '', True, False),
                                ('workflow_dispatch', 'refs/heads/main', 'full', False, False)):
                            expected_ordinary = ordinary or scope in ('full', 'explicit-full')
                            values = {
                                'always()': True, 'cancelled()': cancelled,
                                'needs.select.result': select,
                                'needs.cache-bootstrap-maintenance.result': maintenance_result,
                                'needs.select.outputs.run_oasis7_required': str(ordinary).lower(),
                                'needs.select.outputs.run_formal_process': str(formal).lower(),
                                'fromJSON(needs.select.outputs.plan).scope': scope,
                            'needs.select.outputs.trusted_source': str(ref == 'refs/heads/main').lower(),
                            'fromJSON(needs.select.outputs.plan).execution_profile': '',
                            'fromJSON(needs.select.outputs.plan).fast_plan_version': 0,
                            'fromJSON(needs.select.outputs.plan).ordinary_main_fast_cache_writer_version': 0,
                            'fromJSON(needs.select.outputs.plan).fast_execution_version': 0,
                            'fromJSON(needs.select.outputs.plan).ordinary_execution': '',
                                'github.event_name': event, 'github.ref': ref, 'inputs.run_mode': mode,
                                'github.workflow_sha': 'a' if matching_workflow else 'b', 'github.sha': 'a',
                            }
                            for name, selected in (('cache-bootstrap-maintenance', expected_ordinary or formal),
                                                   ('oasis7-test-build-trusted', expected_ordinary),
                                                   ('formal-debug-build-trusted', formal),
                                                   ('formal-endpoint-build-trusted', formal)):
                                condition = JOBS[name].split('    if:', 1)[1].split('    runs-on:', 1)[0]
                                condition = condition.split('\n    cache-mode:', 1)[0].strip().removeprefix('>-')
                                for token, value in values.items():
                                    condition = condition.replace(token, repr(value))
                                condition = re.sub(r'!(?!=)', 'not ', condition.replace('&&', ' and ').replace('||', ' or '))
                                with self.subTest(job=name, select=select, cancelled=cancelled,
                                                  maintenance=maintenance_result, event=event, scope=scope):
                                    actual = eval(' '.join(condition.split()), {'__builtins__': {}}, {})
                                    self.assertEqual(actual, select == 'success' and not cancelled and trusted and selected)

    def test_ecs_maintenance_recovers_after_native_success_without_a_new_save(self):
        ecs = workflow_jobs((ROOT / '.github/workflows/ecs-packages.yml').read_text())
        writer, job = ecs['native-trusted'], ecs['cache-maintenance']
        self.assertIn('needs: native-trusted', job)
        self.assertIn("needs.native-trusted.result == 'success'", job)
        self.assertIn('always() && !cancelled()', job.split('    steps:', 1)[0])
        self.assertNotIn('needs.native-trusted.outputs.cache_confirmed', job.split('    steps:', 1)[0])
        self.assertIn('group: ci-cache-maintenance-main', job)
        self.assertIn('cache-mode: none', job)
        self.assertIn('actions: write', job)
        self.assertIn('ci-cache-maintenance.py --recover --apply "${args[@]}"', job)
        self.assertIn('CI_CACHE_REPOSITORY_BUDGET_BYTES: ${{ env.ECS_CACHE_REPOSITORY_BUDGET_BYTES }}', job)
        self.assertIn("steps.ecs-workspace-cache.outputs.cache-hit == 'true'", writer)
        self.assertIn('cache_confirmed: ${{ steps.ecs-cache-observe.outputs.saved_confirmed }}', writer)
        self.assertNotIn('cache-maintenance', ecs['package'].split('    steps:', 1)[0])
        for name, body in ecs.items():
            if name != 'cache-maintenance':
                self.assertNotIn('actions: write', body, name)
        script = textwrap.dedent(job.split('        run: |\n', 1)[1])
        stub = 'python3() { printf "<%s>\\n" "$@"; };\n'
        for result in ('success', 'failure'):
            for confirmed in ('true', 'false', ''):
                output = subprocess.run(['bash', '-c', stub + script], text=True, capture_output=True,
                                        env=dict(os.environ, NATIVE_RESULT=result,
                                                 CURRENT_CACHE_CONFIRMED=confirmed, CURRENT_CACHE_KEY='ecs-key'))
                self.assertEqual(output.returncode, 0, output.stderr)
                self.assertIn('<--recover>', output.stdout)
                self.assertIn('<--apply>', output.stdout)
                self.assertEqual('<ecs-key>' in output.stdout, result == 'success' and confirmed == 'true')


if __name__ == '__main__':
    unittest.main()
