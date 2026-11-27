#!/usr/bin/env python3
"""Self-contained core workflow candidate-graph, gate, formal-diagnostics, and custody-bundle contract tests (part 14)."""
import itertools
import json
from pathlib import Path
import re
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


def workspace_support_image_matrix(job):
    pair = "matrix.execution == 'signer-context-image'" in job
    protocol = "matrix.execution == 'signer-protocol-image'" in job
    assert not protocol or pair
    container_job = job
    if pair:
        image_condition = "matrix.execution == 'signer-context-image'"
        if protocol:
            image_condition = "(" + image_condition + " || matrix.execution == 'signer-protocol-image')"
        pair_image = (
            image_condition + " && "
            "'ghcr.io/nil-studio/oasis7/ci-rust-build@sha256:"
            "f121df903f97527a7097134d39f25822c46a3775152768a9cbd591fe65ee987c' || "
        )
        assert pair_image in job
        container_job = job.replace(pair_image, '', 1)
    container = re.search(
        r"^    container:\n      image: \$\{\{ (?:matrix\.execution == 'signer-image' && "
        r"'ghcr\.io/nil-studio/oasis7/ci-rust-build@sha256:([0-9a-f]{64})' \|\| )?matrix\.execution == 'image' && "
        r"'ghcr\.io/nil-studio/oasis7/ci-rust-build@sha256:([0-9a-f]{64})' \|\| '' \}\}$",
        container_job, re.M)
    if not container:
        return None
    expected_matrix = (
        "        execution: ${{ fromJSON(needs.select.outputs.workspace_support_host != 'false' "
        "&& '[\"host\",\"image\"]' || '[\"image\"]') }}\n")
    if container[1]:
        expected_matrix = expected_matrix.replace(
            'fromJSON(', 'fromJSON(needs.select.outputs.workspace_support_signer_only == \'true\' '
            "&& '[\"signer-image\"]' || ")
    if pair:
        assert container[1]
        expected_matrix = expected_matrix.replace(
            'fromJSON(', "fromJSON(needs.select.outputs.workspace_support_signer_context == 'true' "
            "&& '[\"signer-context-image\"]' || ",
        )
    if protocol:
        expected_matrix = expected_matrix.replace(
            'fromJSON(', "fromJSON(needs.select.outputs.workspace_support_signer_protocol == 'true' "
            "&& '[\"signer-protocol-image\"]' || ",
        )
    assert expected_matrix in job and '      fail-fast: false\n' in job
    assert '--support-execution "$SUPPORT_EXECUTION"' in job
    assert '          SUPPORT_EXECUTION: ${{ matrix.execution }}\n' in job
    steps = re.findall(r'^      - .*?(?=^      - |\Z)', job, re.M | re.S)
    apt = [step for step in steps if 'scripts/ci-apt-install.py' in step]
    assert len(apt) == 1 and "        if: matrix.execution == 'host'\n" in apt[0]
    caches = [step for step in steps if 'Swatinem/rust-cache@' in step]
    keys = {'host': 'ci-shared-native-trusted-v4',
            'image': 'ci-rust-build-' + container[2]}
    if container[1]:
        keys['signer-image'] = 'ci-rust-build-' + container[1]
    assert len(caches) == len(keys)
    for mode, key in keys.items():
        matching = [step for step in caches if f"        if: matrix.execution == '{mode}'\n" in step]
        assert len(matching) == 1
        assert f'          shared-key: {key}\n' in matching[0]
        assert '          save-if: false\n' in matching[0]
        assert '          add-rust-environment-hash-key: true\n' in matching[0]
    return keys['image']


def rust_image_cache_namespace(name, job):
    if name not in ('node', 'consensus', 'distfs', 'rust-baseline', 'pixel-world-bridge', 'net', 'scenario-regression', 'launcher-web', 'custody-native-contracts', 'custody-native', 'custody-wasm-production', 'workspace-support-other', 'cargo-tooling-contracts') or '    container:' not in job:
        return None
    if name == 'workspace-support-other' and (matrix := workspace_support_image_matrix(job)):
        return matrix
    target = {'net': 'rust-wasm-build', 'custody-wasm-production': 'rust-custody-wasm'}.get(name, 'rust-build')
    image = re.search(r'^    container:\n      image: ghcr\.io/nil-studio/oasis7/ci-' + target
                      + r'@sha256:([0-9a-f]{64})$', job, re.M)
    assert image, f'Rust cache readers require the pinned {target} image'
    assert '          save-if: false\n' in job, 'Rust image consumers may only read cache'
    return f'ci-{target}-{image.group(1)}'


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


class WorkflowCorePart14(unittest.TestCase):
    def test_formal_build_diagnostics_are_optional_without_hiding_required_failures(self):
        for component, suffix in itertools.product(('debug', 'endpoint'), ('', '-trusted')):
            job = JOBS[f'formal-{component}-build{suffix}']
            build = job.split(f'      - name: Build and archive formal-{component} component\n', 1)[1].split('      - ', 1)[0]
            self.assertIn(f'--build-diagnostics "$RUNNER_TEMP/formal-{component}-build-diagnostics"', build)
            self.assertNotIn('continue-on-error:', build)
            self.assertNotIn('|| true', build)
            upload = job.split(f'      - name: Upload formal-{component} build diagnostics\n', 1)[1].split('      - ', 1)[0]
            self.assertIn('        if: always()\n', upload)
            self.assertIn('        continue-on-error: true\n', upload)
            self.assertIn(f'          path: ${{{{ runner.temp }}}}/formal-{component}-build-diagnostics\n', upload)
            self.assertIn(f'          name: formal-{component}-build-diagnostics-${{{{ github.sha }}}}-${{{{ github.run_attempt }}}}\n', upload)
            self.assertIn('          if-no-files-found: warn\n', upload)
        self.assertTrue(validate_candidate_graph(WORKFLOW, CONFIG))
        mutations = (
            WORKFLOW.replace('      - name: Build and archive formal-debug component\n',
                             '      - name: Build and archive formal-debug component\n        continue-on-error: true\n', 1),
            WORKFLOW.replace('      - name: Merge and pack formal process dependencies\n',
                             '      - name: Merge and pack formal process dependencies\n        continue-on-error: true\n', 1),
            WORKFLOW.replace('      - name: Upload formal-debug build diagnostics\n        if: always()\n',
                             '      - name: Upload formal-debug build diagnostics\n        if: success()\n', 1),
            WORKFLOW.replace('          path: ${{ runner.temp }}/formal-debug-build-diagnostics\n',
                             '          path: ${{ runner.temp }}/formal-debug-tests.tar.gz\n', 1),
        )
        for workflow in mutations:
            self.assertNotEqual(workflow, WORKFLOW, 'failure-propagation mutation did not change the workflow')
            with self.assertRaisesRegex(ValueError, 'failure propagation'):
                validate_candidate_graph(workflow, CONFIG)

    def test_gate_is_last_and_all_groups_are_dependencies(self):
        gate = JOBS['required-gate']
        dependencies = re.search(r'    needs: \[(.*)\]', gate).group(1).split(', ')
        expected_gate = {'select', *(g.replace('_', '-') for g in CONFIG['groups'])}
        if CONFIG.get('workspace_support_direct_gate_version') in (1, 2):
            expected_gate.add('workspace-support-other')
        if CONFIG.get('workspace_support_direct_gate_version') == 2:
            expected_gate.update(('custody-native-contracts', 'custody-native', 'custody-wasm-production', 'custody-node'))
        self.assertEqual(set(dependencies), expected_gate)
        self.assertIn('always()', gate)
        self.assertIn('ci-required-result.py', gate)
        for group in CONFIG['groups']:
            job = JOBS[group.replace('_', '-')]
            if group == 'formal_process':
                self.assertIn('always()', job)
                self.assertIn('needs: [select, formal-test-build, formal-test-build-trusted, formal-process-tests]', job)
                continue
            if group == 'workspace_support':
                self.assertTrue(validate_candidate_graph(WORKFLOW, CONFIG))
                continue
            preflight_groups = {
                'rust_baseline', 'node', 'distfs', 'net', 'launcher_web', 'scenario_regression',
            }
            allowed = (
                ('needs: [select, oasis7-test-build-candidate, oasis7-test-build-trusted]',
                 'needs: [select, fast-preflight, oasis7-test-build-candidate, oasis7-test-build-trusted]')
                if group == 'oasis7_required'
                else (('needs: [select, fast-preflight]',)
                      if group in preflight_groups else ('needs: select',))
            )
            self.assertTrue(any(dep in job for dep in allowed), (group, allowed))
            self.assertIn(f"needs.select.outputs.run_{group} == 'true'", job)
            self.assertIn("needs.select.result == 'success'", job)
            self.assertIn("fromJSON(needs.select.outputs.plan).scope == 'full'", job)
            if group not in ('fleet_health', 'oasis7_required', 'workspace_support'):
                self.assertNotIn('strategy:', job)
            if group != 'workflow_governance':
                self.assertNotIn('ci-authority', job)
        self.assertNotIn('needs: required-gate', WORKFLOW)
        self.assertNotIn('continue-on-error:', without_optional_dependency_cache_install(WORKFLOW))
        self.assertNotIn('paths-ignore:', WORKFLOW)

    def test_candidate_graph_rejects_missing_dependencies_platforms_and_failure_swallowing(self):
        self.assertTrue(validate_candidate_graph(WORKFLOW, CONFIG))
        mutations = [WORKFLOW.replace(', fleet-health]', ']'),
                     WORKFLOW.replace('["ubuntu-24.04","windows-2022","macos-15"]', '["ubuntu-24.04","windows-2022"]'),
                     WORKFLOW.replace('    runs-on: ${{ matrix.os }}', '    runs-on: windows-2022'),
                     WORKFLOW.replace('      fail-fast: false', '      continue-on-error: true'),
                     WORKFLOW.replace('scripts/ci-rust-cache-admission.py install', 'scripts/ci-tests.sh full'),
                     WORKFLOW.replace("          github.job == 'formal-debug-build-trusted' && github.ref",
                                      "          github.job != 'formal-debug-build-trusted' && github.ref", 1),
                     WORKFLOW.replace('      - name: Run full test tier\n',
                                      '      - name: Run full test tier\n        continue-on-error: true\n', 1),
                     WORKFLOW.replace(' --host-smoke', ''),
                     WORKFLOW.replace('    if: always()', '    if: success()'),
                     WORKFLOW.replace("fromJSON(needs.select.outputs.plan).scope == 'full'", 'false', 1),
                     WORKFLOW.replace('needs.select.outputs.run_operational_contracts', 'needs.select.outputs.run_other_contracts', 1),
                     WORKFLOW.replace('needs.select.outputs.run_packaging_contracts', 'needs.select.outputs.run_other_contracts'),
                     WORKFLOW.replace('required --group "operational_contracts"', 'required --group "other_contracts"', 1),
                     WORKFLOW.replace('required --group "packaging_contracts"', 'required --group "other_contracts"')]
        for workflow in mutations:
            self.assertNotEqual(workflow, WORKFLOW, 'mutation must exercise the current workflow')
            with self.subTest(workflow=workflow[-100:]), self.assertRaises(ValueError):
                validate_candidate_graph(workflow, CONFIG)
        new_config = dict(CONFIG, groups=[*CONFIG['groups'], 'new_group'])
        with self.assertRaises(ValueError):
            validate_candidate_graph(WORKFLOW, new_config)

    def test_custody_build_once_and_complete_consumer_graph_rejects_omissions(self):
        self.assertTrue(validate_candidate_graph(WORKFLOW, CONFIG))
        node = JOBS['custody-node']
        self.assertNotIn('rustup', node)
        self.assertNotIn('Swatinem/rust-cache', node)
        self.assertNotIn('cargo test', node)
        native = JOBS['custody-native']
        if '    container:' in native:
            self.assertIn('options: --user 1001:1001 --init', native)
            self.assertIn('node-version: \'22\'', native)
            self.assertIn('strip --version', native)
            self.assertIn('install-dir: ${{ runner.temp }}/oasis7-custody-contracts-mold', native)
            self.assertIn('save-if: false', native)
            self.assertNotIn('ci-apt-install.py', native)
            self.assertNotIn('rustup toolchain install', native)
            self.assertIn(rust_image_cache_namespace('custody-native-contracts', JOBS['custody-native-contracts']),
                          native)
        for producer_name, artifact_output in (
            ('custody-native', 'artifact'),
            ('custody-wasm-production', 'artifact'),
            ('custody-wasm-production', 'approval_artifact'),
        ):
            self.assertIn(f'name: ${{{{ needs.{producer_name}.outputs.{artifact_output} }}}}', node)
            self.assertIn(f'needs.{producer_name}.outputs.attempt', node)
        for producer_name, expected_paths in (
            ('custody-native', ['${{ runner.temp }}/custody-bundle']),
            ('custody-wasm-production', ['${{ runner.temp }}/custody-bundle', '${{ runner.temp }}/custody-approval-bundle']),
        ):
            uploads = [step for step in re.findall(r'^      - .*?(?=^      - |\Z)',
                       JOBS[producer_name], re.M | re.S)
                       if 'uses: actions/upload-artifact@' in step]
            self.assertEqual(len(uploads), len(expected_paths))
            for upload, expected_path in zip(uploads, expected_paths):
                self.assertIn('path: ' + expected_path, upload)
                self.assertIn('include-hidden-files: true', upload)
                self.assertIn('compression-level: 0', upload)
                self.assertIn('if-no-files-found: error', upload)
                self.assertIn('retention-days: 7', upload)
        self.assertIn('overwrite: true', node)
        self.assertIn('if: always()', node)
        legacy_config = dict(CONFIG, custody_execution_version=1, custody_compile_proof_version=1,
                             custody_policy_tests_version=1, custody_node_shards_version=2)
        summary = JOBS['workspace-support']
        for before, after in [
                ('custody-node]', ']'),
                ('custody-native-contracts, ', ''),
                ('ci-custody-platform.py validate', 'true'),
                ('pattern: custody-node-${{ github.run_id }}-*', 'pattern: stale-*')]:
            with self.subTest(before=before), self.assertRaises(ValueError):
                validate_candidate_graph(WORKFLOW.replace(summary, summary.replace(before, after)), legacy_config)
        for before, after in [('fail-fast: false', 'fail-fast: true'),
                              ('custody_node_shards', 'unknown_shards'),
                              ('ci-custody-platform.py restore', 'true'),
                              ('npm run test:types', 'true')]:
            with self.subTest(before=before), self.assertRaises(ValueError):
                validate_candidate_graph(WORKFLOW.replace(node, node.replace(before, after)), legacy_config)
        native = JOBS['custody-native-contracts']
        native_needs = ('    needs: [select, fast-preflight]\n'
                        if '    needs: [select, fast-preflight]\n' in native else '    needs: select\n')
        for before, after in [('if: fromJSON(needs.select.outputs.plan).custody_compile_proof_version == 1', 'if: fromJSON(needs.select.outputs.plan).custody_compile_proof_version == 2'),
                              ('ci-custody-platform.py stage', 'true'),
                              ('verify.sh --stage "$stage"', 'true'),
                              ('run_worker_wasm_compile_profiles', 'true'),
                              ('    needs: [select, fast-preflight]\n', '    needs: custody-wasm-production\n')]:
            with self.subTest(before=before), self.assertRaises(ValueError):
                validate_candidate_graph(WORKFLOW.replace(native, native.replace(before, after)), legacy_config)
        for before, after in [('if: fromJSON(needs.select.outputs.plan).custody_policy_tests_version == 1', 'if: fromJSON(needs.select.outputs.plan).custody_policy_tests_version == 2'),
                              ('ci-custody-platform.py policy-tests', 'true'),
                              ('name: ${{ needs.select.outputs.selection_artifact }}', 'name: unrelated'),
                              ('OASIS7_SUPPORT_TESTED_SHA:', 'OMITTED_TESTED_SHA:')]:
            with self.subTest(before=before), self.assertRaisesRegex(ValueError, 'policy'):
                validate_candidate_graph(WORKFLOW.replace(native, native.replace(before, after)), legacy_config)

    def test_custody_bundle_upload_rejects_hidden_file_filtering_or_checkout_upload(self):
        self.assertTrue(validate_candidate_graph(WORKFLOW, CONFIG))
        anchor = '    steps: &custody-build-steps\n'
        original = RAW_WORKFLOW.split(anchor, 1)[1].split('\n  custody-wasm-production:', 1)[0]
        for before, after in [
                ('          include-hidden-files: true\n', ''),
                ('          include-hidden-files: true\n', '          include-hidden-files: false\n'),
                ('          path: ${{ runner.temp }}/custody-bundle\n', '          path: ${{ github.workspace }}\n'),
                ('          if-no-files-found: error\n', '          if-no-files-found: warn\n'),
                ('          compression-level: 0\n', ''),
                ('          compression-level: 0\n', '          compression-level: 6\n'),
                ('          retention-days: 7\n', ''),
                ('          retention-days: 7\n', '          retention-days: 70\n'),
                ('          name: ${{ steps.bundle.outputs.artifact }}\n', '          name: stale-custody\n')]:
            with self.subTest(after=after), self.assertRaisesRegex(ValueError, 'custody upload'):
                self.assertIn(before, original)
                candidate = RAW_WORKFLOW.replace(original, original.replace(before, after, 1), 1)
                validate_candidate_graph(candidate, CONFIG)
        wasm_original = RAW_WORKFLOW.split('\n  custody-wasm-production:\n', 1)[1].split('\n  custody-node:', 1)[0]
        for before, after in [
                ('          path: ${{ runner.temp }}/custody-approval-bundle\n', '          path: ${{ runner.temp }}/custody-bundle\n'),
                ('          name: ${{ steps.approval-bundle.outputs.artifact }}\n', '          name: ${{ steps.bundle.outputs.artifact }}\n')]:
            with self.subTest(wasm_after=after), self.assertRaisesRegex(ValueError, 'custody upload'):
                self.assertIn(before, wasm_original)
                candidate = RAW_WORKFLOW.replace(wasm_original, wasm_original.replace(before, after, 1), 1)
                validate_candidate_graph(candidate, CONFIG)
        upload = next(step for step in re.findall(r'^      - .*?(?=^      - |\Z)', original, re.M | re.S)
                      if 'uses: actions/upload-artifact@' in step)
        for replacement in ('', upload + upload.replace('steps.bundle.outputs.artifact', 'steps.other.outputs.artifact')):
            with self.subTest(replacement=replacement), self.assertRaisesRegex(ValueError, 'custody upload'):
                candidate = RAW_WORKFLOW.replace(original, original.replace(upload, replacement, 1), 1)
                validate_candidate_graph(candidate, CONFIG)


if __name__ == '__main__':
    unittest.main()
