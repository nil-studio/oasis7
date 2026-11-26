#!/usr/bin/env python3
"""Regress the final DAG and execution boundaries, without Actions credentials."""
import ast
import hashlib
import json
import itertools
import os
import subprocess
import sys
import tempfile
import textwrap
from pathlib import Path
import re
import shlex
import shutil
import unittest
from unittest.mock import patch

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
    names = ('run_worker_wasm_compile_profiles',
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

def workflow_step_script(job, name):
    step = next(step for step in re.findall(r'^      - .*?(?=^      - |\Z)', job, re.M | re.S)
                if 'name: ' + name + '\n' in step)
    if '        run: |\n' in step:
        return textwrap.dedent(step.split('        run: |\n', 1)[1])
    return step.split('        run: ', 1)[1].splitlines()[0] + '\n'


def project_selection(directory, plan=None):
    root = Path(directory)
    selection = root / 'selection.json'
    if plan is not None:
        selection.write_text(json.dumps(plan, ensure_ascii=False, indent=2) + '\n', encoding='utf-8')
    output = root / 'projection-output'
    output.unlink(missing_ok=True)
    result = subprocess.run(['bash', '-euo', 'pipefail', '-c',
        workflow_step_script(JOBS['select'], 'Publish workflow scheduling projection')],
        env=dict(os.environ, RUNNER_TEMP=str(root), GITHUB_OUTPUT=str(output)), text=True, capture_output=True)
    if result.returncode:
        raise AssertionError(result.stdout + result.stderr)
    return json.loads(dict(line.split('=', 1) for line in output.read_text().splitlines())['plan'])


def large_transport_plan():
    return dict(scope='targeted', execution_profile='pr-fast', fast_plan_version=1,
        fast_execution_version=1, ordinary_execution='artifact', ordinary_artifact_delivery_version=1,
        ordinary_artifact_partition_version=1, oasis7_required_packages=['oasis7_runtime'],
        custody_execution_version=1, custody_node_shards=[0, 1, 2, 3], python_control_contract_version=1,
        groups=['baseline', 'workspace_support'], source_sha='a' * 40, tested_sha='b' * 40,
        policy_sha='c' * 40, check_inventory_version=1, fast_plan_exceptions=['preserve every exception'],
        architecture=dict(version=1, mode='full', packages=[], base='c' * 40, source='a' * 40, tested='b' * 40),
        reasons=["quoted ' \" \\\n$(touch transport-must-not-execute)\nPY\n" * 4000])


TRUSTED_SOURCE_WRITERS = (
    'oasis7-test-build-trusted', 'formal-debug-build-trusted',
    'formal-endpoint-build-trusted', 'full-regression',
)


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


def comparable_source_steps(name):
    steps = selected_source_steps(name)
    if (name not in ('oasis7-test-build-candidate', 'oasis7-test-build-trusted')
            or '    container:' not in JOBS['oasis7-test-build-trusted']):
        return steps
    # The image changes only environment/cache preparation. Preserve an exact
    # comparison of every workspace/artifact/delivery step after that boundary.
    boundary = '      - name: Identify ordinary workspace cache'
    assert steps.count(boundary) == 1, 'ordinary workspace boundary must be unique'
    prefix, steps = steps.split(boundary, 1)
    expected = [
        '- name: Check out workflow linker setup',
        '- name: Configure mold for Linux host linking',
        '- uses: actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1 # v7.0.1',
        '- uses: actions/download-artifact@9000827ccba6bdab643e8b6fd33ac0654aef8333 # v8.0.2',
        '- name: Report ordinary workspace cache capacity',
        '- name: Reclaim unused candidate SDK capacity',
        '- name: Stabilize Rust cache toolchains',
        '- uses: Swatinem/rust-cache@d1527c8f67ef9af357290475d4af4b0fab8f12e0 # v2',
    ]
    if name == 'oasis7-test-build-trusted':
        expected.insert(1, '- name: Verify trusted ordinary image and source')
        expected.insert(-1, '- name: Install ordinary dependency cache admission')
    else:
        expected.insert(4, '- uses: actions/setup-python@6d4fd22b20465ccc0f7354476588e8df7c9ca0e0 # v7')
        if '    container:' in JOBS['oasis7-test-build-candidate']:
            expected.insert(5, '- name: Verify candidate ordinary image and source')
        expected.insert(-2, '- name: Install pinned Rust toolchains')
    actual = [step.splitlines()[0].strip()
              for step in re.findall(r'^      - .*?(?=^      - |\Z)', prefix, re.M | re.S)]
    assert actual == expected, 'ordinary environment prefix cannot hide other business steps'
    steps = boundary + steps
    assert steps.count('      - name: Install native libraries\n') == (name == 'oasis7-test-build-candidate')
    assert steps.count('      - name: Wait for candidate SDK reclamation\n') == 1

    return ''.join(step for step in re.findall(r'^      - .*?(?=^      - |\Z)', steps, re.M | re.S)
                   if not step.startswith(('      - name: Install native libraries\n',
                                           '      - name: Wait for candidate SDK reclamation\n')))


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


def rust_cache_steps(job):
    return [step for step in re.findall(r'^      - .*?(?=^      - |\Z)', job, re.M | re.S)
            if 'uses: Swatinem/rust-cache@' in step]


def portable_cache_host_layout(job, kind):
    """Check the explicit mixed tool lane; official host cache steps stay exact."""
    if 'id: portable-cache-image' not in job:
        return job
    images = {
        'browser': 'ghcr.io/nil-studio/oasis7/ci-runtime@sha256:3984caa42d82963f93af752bf79e3a07838f2c94279cfbb2deeb08f715cd62c9',
        'worker': 'ghcr.io/nil-studio/oasis7/ci-rust-build@sha256:0b13d0b5ba9051617eb6ab77a5af324f066af79bb2753d664b780535c2fe13a3',
    }
    assert f'          image={images[kind]}\n' in job
    assert 'packages: read' in job and '    container:' not in job
    assert 'docker --config "$config" pull "$image"' in job
    assert 'test "$(id -u):$(id -g)" = 1001:1001' in job
    assert job.index('test "$(git rev-parse HEAD)"') < job.index('docker --config')
    assert 'actions/setup-python@' not in job and 'rustup toolchain install' not in job
    assert '--privileged' not in job and 'docker.sock' not in job
    expected = ('command python3 "$GITHUB_WORKSPACE/scripts/ci-portable-cache-container.py" '
                f'--kind {kind} -- "$@"')
    assert job.count(expected) == 1
    assert 'chmod 600 "$bootstrap"' in job
    assert 'BASH_ENV=%s' in job and '"$bootstrap" >> "$GITHUB_ENV"' in job
    return job


def worker_tool_cache_writer(job):
    """The optional tool seed is an exact, admitted main-only writer."""
    job = portable_cache_host_layout(job, 'worker')
    header = job.split('    steps:', 1)[0]
    for required in ('cache-mode: write', 'contents: read', 'actions: read',
                     "github.ref == 'refs/heads/main'", 'github.workflow_sha == github.sha',
                     "github.event_name == 'schedule'", "github.event_name == 'workflow_dispatch'",
                     "inputs.run_mode == 'full'"):
        assert required in header, 'Worker tool writer must remain a bounded trusted main job'
    timeout_match = re.search(r'^    timeout-minutes:\s*(\d+)\s*$', header, re.M)
    assert timeout_match and 0 < int(timeout_match.group(1)) <= 12, 'Worker tool writer must declare bounded timeout-minutes <= 12'
    assert 'actions: write' not in job and 'secrets.' not in job
    assert 'test "$(git rev-parse HEAD)" = "$GITHUB_SHA"' in job
    assert 'persist-credentials: false' in job
    steps = re.findall(r'^      - .*?(?=^      - |\Z)', job, re.M | re.S)
    restores = [s for s in steps if 'uses: actions/cache/restore@' in s]
    saves = [s for s in steps if 'uses: actions/cache/save@' in s]
    admissions = [s for s in steps if 'ci-worker-build.py admit-save' in s]
    assert len(restores) == len(saves) == len(admissions) == 1
    key = 'key: worker-build-cli-v1-${{ runner.os }}-${{ runner.arch }}-${{ env.RUST_TOOLCHAIN }}-0.7.5-${{ steps.worker-meta.outputs.definition_sha }}'
    for step in (*restores, *saves):
        assert key in step and 'path: ${{ steps.worker-meta.outputs.root }}' in step
        assert 'restore-keys:' not in step
    for required in ("success()", "steps.prepare-worker.outcome == 'success'",
                     "steps.worker-tool-cache.outputs.cache-hit != 'true'",
                     "steps.worker-tool-admission.outcome == 'success'",
                     "steps.worker-tool-admission.outputs.save == 'true'"):
        assert required in saves[0], 'Worker tool save requires successful preparation and capacity admission'
    assert 'id: worker-tool-admission' in admissions[0]
    assert 'GH_TOKEN: ${{ github.token }}' in admissions[0]
    assert '"$TOOL_CACHE_KEY"' in admissions[0] and '--github-output "$GITHUB_OUTPUT"' in admissions[0]
    assert job.count('GH_TOKEN:') == 1
    assert 'continue-on-error' not in job and 'ci-worker-build.py ensure' in job and 'ci-worker-build.py check' in job
    return admissions[0]


def browser_archive_cache_writer(job):
    """Browser downloads have one bounded, verified main writer."""
    job = portable_cache_host_layout(job, 'browser')
    header = job.split('    steps:', 1)[0]
    for required in ('cache-mode: write', 'contents: read', 'actions: read',
                     "github.ref == 'refs/heads/main'", 'github.workflow_sha == github.sha',
                     "github.event_name == 'schedule'", "github.event_name == 'workflow_dispatch'",
                     "inputs.run_mode == 'full'", 'runs-on: ubuntu-24.04'):
        assert required in header, 'browser cache requires the bounded trusted main writer'
    assert 'strategy:' not in header and 'matrix:' not in header
    timeout_match = re.search(r'^    timeout-minutes:\s*(\d+)\s*$', header, re.M)
    assert timeout_match and 0 < int(timeout_match.group(1)) <= 6, 'browser cache requires bounded timeout-minutes <= 6'
    assert 'actions: write' not in job and 'secrets.' not in job and 'continue-on-error' not in job
    assert 'test "$(git rev-parse HEAD)" = "$GITHUB_SHA"' in job
    assert 'persist-credentials: false' in job
    assert 'python3 scripts/ci-browser-deps.py metadata --browser agent --github-output "$GITHUB_OUTPUT"' in job
    steps = re.findall(r'^      - .*?(?=^      - |\Z)', job, re.M | re.S)
    restores = [step for step in steps if 'uses: actions/cache/restore@' in step]
    saves = [step for step in steps if 'uses: actions/cache/save@' in step]
    admissions = [step for step in steps if 'ci-browser-deps.py admit-save' in step]
    expected_components = (
        ('fonts-noto-cjk', 'fonts_noto_cjk', 'noto'),
        ('fonts-freefont-ttf', 'fonts_freefont_ttf', 'freefont'),
        ('chrome', 'chrome', 'chrome'),
    )
    assert len(restores) == len(saves) == len(admissions) == len(expected_components)
    for idx, (component, prefix, slug) in enumerate(expected_components):
        restore, save, admission = restores[idx], saves[idx], admissions[idx]
        for step in (restore, save):
            assert f'key: ${{{{ steps.browser-meta.outputs.{prefix}_key }}}}' in step
            assert f'path: ${{{{ steps.browser-meta.outputs.{prefix}_root }}}}' in step
            assert 'restore-keys:' not in step
        assert f'id: browser-archive-{slug}\n' in restore
        assert f'python3 scripts/ci-browser-deps.py fetch --component {component}\n' in job
        for required in ('success()', f"steps.prepare-browser-{slug}.outcome == 'success'",
                         f"steps.browser-archive-{slug}.outputs.cache-hit != 'true'",
                         f"steps.browser-admission-{slug}.outcome == 'success'",
                         f"steps.browser-admission-{slug}.outputs.save == 'true'"):
            assert required in save, 'browser cache requires verified preparation and capacity admission'
        assert f'id: browser-admission-{slug}\n' in admission
        assert f"if: steps.browser-archive-{slug}.outputs.cache-hit != 'true'\n" in admission
        assert f'BROWSER_CACHE_KEY: ${{{{ steps.browser-meta.outputs.{prefix}_key }}}}\n' in admission
        assert f'ci-browser-deps.py admit-save --component {component} --key "$BROWSER_CACHE_KEY" --github-output "$GITHUB_OUTPUT"' in admission
        assert 'GH_TOKEN: ${{ github.token }}' in admission and admission.count('GH_TOKEN:') == 1
        assert job.index(restore) < job.index(f'id: prepare-browser-{slug}\n') < job.index(admission) < job.index(save)
    assert job.count('GH_TOKEN:') == len(expected_components)
    return admissions


def image_cache_admission_layout(job, cache, save_condition):
    """Recognize the complete before/after layouts during container rollout."""
    steps = re.findall(r'^      - .*?(?=^      - |\Z)', job, re.M | re.S)
    installers = [step for step in steps if 'id: dependency-cache-tools\n' in step]
    assert len(installers) == 1
    installer = installers[0]
    assert job.index(installer) < job.index(cache)
    assert not re.search(r'^        (?:if|continue-on-error):', installer, re.M)
    path_adapter = '          PATH: ${{ steps.dependency-cache-tools.outputs.path }}\n'
    preload = '          NODE_OPTIONS: ${{ steps.dependency-cache-tools.outputs.node_options }}\n'
    interpreter = '          OASIS7_DEPENDENCY_CACHE_PYTHON: ${{ steps.dependency-cache-tools.outputs.python }}\n'
    guard = "steps.dependency-cache-tools.outputs.guard_ready == 'true'"
    if 'run: python3 scripts/ci-rust-cache-admission.py install-release\n' in installer:
        assert preload in cache and interpreter in cache and path_adapter not in cache
        assert '          cache-provider: github\n' in cache
        save_condition = save_condition.replace("job.status == 'success'", "job.status == 'success' && " + guard, 1)
    else:
        assert 'run: python3 scripts/ci-rust-cache-admission.py install\n' in installer
        assert path_adapter in cache and preload not in cache and interpreter not in cache
    assert '          save-if: ${{ ' + save_condition + ' }}\n' in cache


def component_image_cache_writer(job, families, readers=JOBS, planner=None):
    """Bind each complete writer row to the actual reader cache environment."""
    header, steps = job.split('    steps:', 1)
    for required in ('cache-mode: write', 'contents: read', 'actions: read', 'packages: read',
                     "github.ref == 'refs/heads/main' && github.workflow_sha == github.sha",
                     "github.event_name == 'schedule'", "github.event_name == 'workflow_dispatch'",
                     "inputs.run_mode == 'full'", "inputs.run_mode == 'image-native'",
                     'fail-fast: false', 'max-parallel: 1', 'runs-on: ubuntu-24.04',
                     'image: ${{ matrix.image }}', 'options: ${{ matrix.options }}'):
        assert required in header, 'component image writer needs its complete trusted main layout'
    timeout_match = re.search(r'^    timeout-minutes:\s*(\d+)\s*$', header, re.M)
    assert timeout_match and 0 < int(timeout_match.group(1)) <= 35, 'component image writer needs bounded timeout-minutes <= 35'
    if planner is None:
        json_blocks = re.findall(r"fromJSON\('(\[.*?\])'\)", header, re.DOTALL)
        if json_blocks:
            assert len(json_blocks) == 2 and "inputs.run_mode == 'image-support' &&" in header
            support_records = json.loads(' '.join(json_blocks[0].split()))
            records = json.loads(' '.join(json_blocks[1].split()))
            assert support_records == [row for row in records if row.get('profile') == 'support']
            rows = {row['profile']: {key: value for key, value in row.items() if key != 'profile'}
                    for row in records}
        else:
            records = re.findall(r'^          - profile: ([a-z]+)\n((?:^            .+\n)+)', header, re.M)
            rows = {profile: dict((key, value.strip("'")) for key, value in re.findall(
                r'^            ([a-z-]+): (.*)$', body, re.M)) for profile, body in records}
    else:
        assert 'needs: plan-image-component-cache' in header
        assert 'include: ${{ fromJSON(needs.plan-image-component-cache.outputs.rows) }}' in header
        assert 'cache-mode: none' in planner and 'permissions: {}' in planner
        records = json.loads(planner.split("<<'JSON'\n", 1)[1].split('          JSON\n', 1)[0])
        rows = {row['profile']: {key: value for key, value in row.items() if key != 'profile'}
                for row in records}
    expected = {
        'native': (('consensus', 'distfs'), 'rust-image-components'),
        'node': (('node',), 'rust-image-components'),
        'net': (('net',), 'rust-image-net'),
        'launcher': (('launcher-web',), 'rust-image-launcher'),
        'tooling': (('cargo-tooling-contracts',), 'rust-image-tooling'),
    }
    support_family = 'rust-image-support'
    if 'support' in rows or support_family in families:
        expected['support'] = (('workspace-support-other',), support_family)
    assert len(records) == len(rows) == len(expected) and set(rows) == set(expected)
    for profile, (names, family) in expected.items():
        row = rows[profile]
        assert set(row) == {'family', 'image', 'namespace', 'options', 'mold-dir'}
        assert row['family'] == family
        assert families.get(family) == (row['namespace'], '.github/workflows/rust-cache.yml',
                                        'rust-image-component-cache')
        for name in names:
            reader = readers[name]
            assert re.fullmatch(r'ghcr.io/nil-studio/oasis7/ci-rust-(?:wasm-)?build@sha256:[0-9a-f]{64}', row['image'])
            assert row['namespace'] == row['image'].rsplit('/', 1)[1].replace('@sha256:', '-')
            if name == 'workspace-support-other':
                assert workspace_support_image_matrix(reader) == row['namespace']
                assert row['options'] == row['mold-dir'] == ''
                if "matrix.execution == 'signer-image'" in reader:
                    node = rows['node']
                    assert "matrix.execution == 'signer-image' && '" + node['image'] + "'" in reader
                    options_prefix = ''
                    mold_prefix = ''
                    if "matrix.execution == 'signer-context-image'" in reader:
                        options_prefix = "matrix.execution == 'signer-context-image' && '" + node['options'] + "' || "
                        mold_prefix = ("matrix.execution == 'signer-context-image' && "
                                       "format('{0}/oasis7-signer-context-mold', runner.temp) || ")
                    if "matrix.execution == 'signer-protocol-image'" in reader:
                        options_prefix = ("(matrix.execution == 'signer-context-image' || "
                                          "matrix.execution == 'signer-protocol-image') && '" + node['options'] + "' || ")
                        mold_prefix = ("matrix.execution == 'signer-protocol-image' && "
                                       "format('{0}/oasis7-signer-protocol-mold', runner.temp) || " + mold_prefix)
                    assert ("      options: ${{ " + options_prefix + "matrix.execution == 'signer-image' && '" +
                            node['options'] + "' || '' }}\n") in reader
                    assert ("          install-dir: ${{ " + mold_prefix +
                            "matrix.execution == 'signer-image' && format('{0}/" + node['mold-dir'] +
                            "', runner.temp) || '' }}\n") in reader
                    signer = [step for step in rust_cache_steps(reader)
                              if "        if: matrix.execution == 'signer-image'\n" in step]
                    assert len(signer) == 1 and 'shared-key: ' + node['namespace'] + '\n' in signer[0]
                    assert 'env-vars: CARGO CC CFLAGS CXX CMAKE RUST OASIS7_WASM' in signer[0]
                    assert not re.search(r'^          (?:key|prefix-key|workspaces|cache-directories|cache-targets):', signer[0], re.M)
                else:
                    assert not re.search(r'^      options:', reader, re.M)
                    assert 'uses: ./.github/actions/setup-linux-mold\n      - ' in reader
                continue
            assert 'image: ' + row['image'] + '\n' in reader
            caches = rust_cache_steps(reader)
            assert len(caches) == 1 and 'shared-key: ' + row['namespace'] + '\n' in caches[0]
            assert 'save-if: false\n' in caches[0]
            assert 'add-rust-environment-hash-key: true' in caches[0]
            assert 'env-vars: CARGO CC CFLAGS CXX CMAKE RUST OASIS7_WASM' in caches[0]
            assert not re.search(r'^          (?:key|prefix-key|workspaces|cache-directories|cache-targets):', caches[0], re.M)
            options = re.search(r'^      options: (.+)$', reader, re.M)
            assert row['options'] == (options[1] if options else '')
            mold = re.search(r'^          install-dir: \$\{\{ runner.temp \}\}/([^\n]+)$', reader, re.M)
            assert row['mold-dir'] == (mold[1] if mold else '')
    assert "install-dir: ${{ matrix.mold-dir != '' && format('{0}/{1}', runner.temp, matrix.mold-dir) || '' }}" in steps
    for required in ('ref: ${{ github.sha }}', 'persist-credentials: false',
                     'test "$GITHUB_REF" = refs/heads/main', 'test "$GITHUB_WORKFLOW_SHA" = "$GITHUB_SHA"',
                     'test "$(git rev-parse HEAD)" = "$GITHUB_SHA"',
                     'python3 scripts/ci-rust-cache-toolchains.py "$RUST_TOOLCHAIN" "$OASIS7_WASM_TOOLCHAIN"',
                     "if: steps.image-cache.outputs.cache-hit != 'true'",
                     'IMAGE_CACHE_PROFILE: ${{ matrix.profile }}',
                     'python3 scripts/ci-image-cache.py --profile "$IMAGE_CACHE_PROFILE"'):
        assert required in steps
    assert 'continue-on-error' not in job and 'actions: write' not in job and 'secrets.' not in job
    caches = rust_cache_steps(job)
    assert len(caches) == 1
    cache = caches[0]
    for required in ('Swatinem/rust-cache@d1527c8f67ef9af357290475d4af4b0fab8f12e0',
                     'OASIS7_DEPENDENCY_CACHE_FAMILY: ${{ matrix.family }}',
                     'OASIS7_DEPENDENCY_CACHE_BUDGET_BYTES: ${{ env.CI_CACHE_REPOSITORY_BUDGET_BYTES }}',
                     'shared-key: ${{ matrix.namespace }}', 'add-rust-environment-hash-key: true',
                     'env-vars: CARGO CC CFLAGS CXX CMAKE RUST OASIS7_WASM',
                     'cache-on-failure: false'):
        assert required in cache
    assert not re.search(r'^          (?:key|prefix-key|workspaces|cache-directories|cache-targets|restore-keys):', cache, re.M)
    image_cache_admission_layout(job, cache,
        "job.status == 'success' && github.ref == 'refs/heads/main' && github.workflow_sha == github.sha")
    assert steps.index('Verify current main cache source') < steps.index('ci-rust-cache-admission.py install')
    assert steps.index('setup-linux-mold') < steps.index('Stabilize Rust cache toolchains') < steps.index(cache)
    assert steps.index(cache) < steps.index('Warm the exact component dependency contexts')
    return rows


def web_cache_layout(cache_workflow, release_workflow, admission_source):
    """Accept only the complete old or new sole-writer wiring during migration."""
    assignments = [node for node in ast.parse(admission_source).body
                   if isinstance(node, ast.Assign)
                   and any(isinstance(target, ast.Name) and target.id == 'FAMILIES'
                           for target in node.targets)]
    assert len(assignments) == 1, 'one literal admission family registry required'
    families = ast.literal_eval(assignments[0].value)
    namespace = 'release-packages-web-wasm-trusted-v3'
    old = (namespace, '.github/workflows/release-packages.yml', 'build-web-dist')
    new = (namespace, '.github/workflows/rust-cache.yml', 'web-wasm-cache')
    registered = families.get('release-web')
    assert registered in (old, new), 'unknown Web writer identity'
    assert [name for name, identity in families.items() if identity[0] == namespace] == ['release-web']
    migrated = registered == new
    cache_jobs, release_jobs = workflow_jobs(cache_workflow), workflow_jobs(release_workflow)
    tool_jobs = {'worker-build-cache'} if 'worker-build-cache' in cache_jobs else set()
    if tool_jobs:
        worker_tool_cache_writer(cache_jobs['worker-build-cache'])
    if 'browser-archive-cache' in cache_jobs:
        browser_archive_cache_writer(cache_jobs['browser-archive-cache'])
        tool_jobs.add('browser-archive-cache')
    image_writer = 'rust-image-native-cache'
    if image_writer in cache_jobs:
        job = cache_jobs[image_writer]
        digest = re.search(r'^      image: ghcr\.io/nil-studio/oasis7/ci-rust-build@sha256:([0-9a-f]{64})$', job, re.M)
        assert digest, 'image cache writer requires the published Rust digest'
        image_namespace = 'ci-rust-build-' + digest[1]
        assert families.get('rust-image-native') == (image_namespace, '.github/workflows/rust-cache.yml', image_writer)
        assert 'shared-key: ' + image_namespace + '\n' in job
        assert "github.ref == 'refs/heads/main' && github.workflow_sha == github.sha" in job
        assert "github.event_name == 'workflow_dispatch' && inputs.run_mode == 'image-native'" in job
        assert 'OASIS7_DEPENDENCY_CACHE_FAMILY: rust-image-native\n' in job
        caches = rust_cache_steps(job)
        assert len(caches) == 1
        image_cache_admission_layout(job, caches[0], "job.status == 'success' && github.ref == 'refs/heads/main' && "
                                     "github.workflow_sha == github.sha && inputs.run_mode == 'image-native'")
        tool_jobs.add(image_writer)
    else:
        assert 'rust-image-native' not in families
    component_writer = 'rust-image-component-cache'
    if component_writer in cache_jobs:
        component_image_cache_writer(cache_jobs[component_writer], families,
                                     planner=cache_jobs.get('plan-image-component-cache'))
        tool_jobs.add(component_writer)
    else:
        assert not {'rust-image-components', 'rust-image-net', 'rust-image-launcher',
                    'rust-image-tooling', 'rust-image-support'}.intersection(families)
    assert set(cache_jobs) == (({'shared-native-cache', 'web-wasm-cache'} if migrated
                               else {'shared-native-cache'}) | tool_jobs
                               | ({'plan-image-component-cache'} if 'plan-image-component-cache' in cache_jobs else set())), 'incomplete cache producer layout'
    active = []
    for path, workflow, jobs in (('.github/workflows/rust-cache.yml', cache_workflow, cache_jobs),
                                 ('.github/workflows/release-packages.yml', release_workflow, release_jobs)):
        for name, job in jobs.items():
            for step in rust_cache_steps(job):
                if 'shared-key: ' + namespace + '\n' not in step:
                    continue
                saves = re.findall(r'^          save-if: (.+)$', step, re.M)
                assert len(saves) == 1, 'one explicit Web save policy required'
                header = job.split('    steps:', 1)[0]
                if saves[0] != 'false':
                    active.append((namespace, path, name))
                    assert 'cache-mode: read' not in header
                    assert ('cache-mode: write' in header or re.search(r'^cache-mode: write$',
                            workflow.split('\njobs:\n', 1)[0], re.M))
                    assert 'OASIS7_DEPENDENCY_CACHE_FAMILY: release-web\n' in step
                else:
                    assert 'cache-mode: write' not in header
    assert active == [registered], 'Web cache must have exactly the registered sole writer'
    if migrated:
        reader = release_jobs['build-web-dist']
        assert '    cache-mode: read\n' in reader.split('    steps:', 1)[0]
        assert len(rust_cache_steps(reader)) == 1
        assert 'save-if: false\n' in rust_cache_steps(reader)[0]
        assert 'dependency-cache-tools' not in reader and 'OASIS7_DEPENDENCY_CACHE_' not in reader
    return migrated


def viewer_image_digest(job):
    header = job.split('    steps:', 1)[0]
    if '    container:' not in header:
        return None
    match = re.search(
        r"(?m)^      image: \$\{\{ needs\.select\.outputs\.viewer_image_compatible == 'true' && "
        r"'ghcr\.io/nil-studio/oasis7/ci-rust-viewer@sha256:([0-9a-f]{64})' \|\| '' \}\}$", header)
    assert match, 'Viewer requires the pinned image with a host fallback'
    assert '      options: --user 1001:1001 --init\n' in header
    assert header.count('    env:\n') == 1
    assert ('    env:\n      VIEWER_IMAGE_COMPATIBLE: ${{ needs.select.outputs.viewer_image_compatible }}\n' in header)
    return match[1]


def viewer_cache_kind(job):
    """The two Viewer lanes may move as a complete known cache configuration."""
    steps = rust_cache_steps(job)
    assert len(steps) == 1
    cache = steps[0]
    assert 'uses: Swatinem/rust-cache@d1527c8f67ef9af357290475d4af4b0fab8f12e0 # v2\n' in cache
    entries = re.findall(r'^          ([\w-]+): (.+)$', cache, re.M)
    inputs = dict(entries)
    assert len(inputs) == len(entries), 'duplicate cache input'
    common = {'save-if': 'false', 'add-rust-environment-hash-key': 'true', 'cache-on-failure': 'false'}
    native = common | {'shared-key': 'ci-shared-native-trusted-v4',
                       'env-vars': 'CARGO CC CFLAGS CXX CMAKE RUST OASIS7_WASM'}
    web = common | {'shared-key': 'release-packages-web-wasm-trusted-v3',
                    'env-vars': 'CARGO CC CFLAGS CXX CMAKE RUST'}
    if digest := viewer_image_digest(job):
        expected_key = ("${{ env.VIEWER_IMAGE_COMPATIBLE == 'true' && 'ci-rust-viewer-" + digest
                        + "' || 'release-packages-web-wasm-trusted-v3' }}")
        assert inputs.get('shared-key') == expected_key, 'Viewer image and host cache identities must stay isolated'
        inputs['shared-key'] = web['shared-key']
    assert inputs in (native, web), 'unknown or incomplete Viewer cache configuration'
    assert 'cache-mode: write' not in job
    assert ('    env:' not in job.split('    steps:', 1)[0] or digest) and '        env:' not in cache
    assert 'GH_TOKEN:' not in cache and 'OASIS7_DEPENDENCY_CACHE_' not in cache
    prepare = next(step for step in re.findall(r'^      - .*?(?=^      - |\Z)', job, re.M | re.S)
                   if 'python3 scripts/ci-rust-cache-toolchains.py' in step)
    expected = '"$RUST_TOOLCHAIN"' + (' "$OASIS7_WASM_TOOLCHAIN"' if inputs == native else '')
    assert re.search(r'^        run: python3 scripts/ci-rust-cache-toolchains.py ' +
                     re.escape(expected) + r'$', prepare, re.M)
    assert 'continue-on-error:' not in prepare
    prepare_if = re.search(r'^        if: .*?(?=^        [a-z-]+:|\Z)', prepare, re.M | re.S)
    cache_if = re.search(r'^        if: .*?(?=^        [a-z-]+:|\Z)', cache, re.M | re.S)
    if digest:
        host_only = "env.VIEWER_IMAGE_COMPATIBLE != 'true'"
        if cache_if:
            original = cache_if.group(0).removeprefix('        if: >-\n').strip()
            expected = "        if: >-\n          " + host_only + " &&\n          (" + original + ")"
        else:
            expected = '        if: ' + host_only
        assert prepare_if and prepare_if.group(0).strip() == expected.strip()
    else:
        assert (prepare_if.group(0) if prepare_if else None) == (cache_if.group(0) if cache_if else None)
    assert job.index('rustup default') < job.index(prepare) < job.index(cache)
    before_cache = job[:job.index(cache)]
    assert 'rustup target add wasm32-unknown-unknown' in before_cache
    if inputs == web:
        assert 'OASIS7_WASM' not in before_cache and 'nightly' not in before_cache
        assert re.findall(r'^          rustup toolchain install (\S+)', before_cache, re.M) == ['"$RUST_TOOLCHAIN"']
    else:
        assert 'rustup toolchain install "$OASIS7_WASM_TOOLCHAIN"' in before_cache
    return 'native' if inputs == native else 'web'


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


def validate_mold_identity_order(steps, *, trusted_image=False):
    action = 'uses: ./.github/actions/setup-linux-mold'
    assert steps.count(action) == 1
    assert steps.index('uses: actions/checkout@') < steps.index(action)
    preflight = '          python3 scripts/ci-test-artifacts.py ordinary-image-preflight\n'
    exempt = None
    if trusted_image:
        assert steps.count(preflight) == 1
        exempt = steps.index(preflight) + preflight.index('ci-test-artifacts.py')
    for boundary in re.finditer(
            r'Swatinem/rust-cache@|actions/cache(?:/restore)?@|'
            r'ci-workspace-cache\.py|ci-ecs-cache\.py|ci-test-artifacts\.py', steps):
        # This exact command checks cwd/source/OS, without deriving a cache or
        # artifact identity. Every actual identity operation still follows mold.
        if boundary.start() != exempt:
            assert steps.index(action) < boundary.start()


class Workflow(unittest.TestCase):
    def test_linux_mold_precedes_cache_identity_in_builders_warmers_and_formal_merger(self):
        # Missing one producer/merger or a warmer silently changes the cache or
        # Formal component identity even if the ordinary build itself succeeds.
        required = {
            'rust.yml': (
                'pr-governance-preview', 'packaging-contracts',
                'oasis7-test-build-candidate', 'oasis7-test-build-trusted',
                'formal-debug-build', 'formal-debug-build-trusted',
                'formal-endpoint-build', 'formal-endpoint-build-trusted',
                'formal-test-build', 'oasis7-required', 'viewer-js-required',
                'viewer-performance-report', 'launcher-web',
                'custody-wasm-production', 'doc-checker-contracts',
            ),
            'rust-cache.yml': ('worker-build-cache', 'shared-native-cache', 'web-wasm-cache',
                               'rust-image-native-cache', 'rust-image-component-cache'),
            'ecs-packages.yml': ('web', 'native', 'native-trusted', 'build-metrics'),
            'native-cache-diagnostic.yml': ('diagnose',),
            'storage-acceptance.yml': ('build',),
        }
        action = 'uses: ./.github/actions/setup-linux-mold'
        for filename, names in required.items():
            jobs = workflow_jobs((ROOT / '.github/workflows' / filename).read_text())
            for name in names:
                with self.subTest(workflow=filename, job=name):
                    steps = jobs[name].split('    steps:', 1)[1]
                    validate_mold_identity_order(steps, trusted_image=(
                        filename == 'rust.yml' and name == 'oasis7-test-build-trusted'
                        and '    container:' in jobs[name]))

    def test_mold_identity_order_only_exempts_exact_trusted_image_preflight(self):
        steps = ('uses: actions/checkout@pin\n'
                 '          python3 scripts/ci-test-artifacts.py ordinary-image-preflight\n'
                 'uses: ./.github/actions/setup-linux-mold\n'
                 'python3 scripts/ci-workspace-cache.py identify\n'
                 'python3 scripts/ci-test-artifacts.py build\n')
        validate_mold_identity_order(steps, trusted_image=True)
        for broken, trusted in (
                (steps, False),
                (steps.replace('ordinary-image-preflight', 'build'), True),
                (steps.replace('python3 scripts/ci-workspace-cache.py identify\n', '')
                 .replace('uses: ./.github/actions/setup-linux-mold\n',
                          'python3 scripts/ci-workspace-cache.py identify\n'
                          'uses: ./.github/actions/setup-linux-mold\n'), True),
                (steps.replace('uses: ./.github/actions/setup-linux-mold\n', '')
                 + 'uses: ./.github/actions/setup-linux-mold\n', True)):
            with self.subTest(trusted=trusted, steps=broken), self.assertRaises(AssertionError):
                validate_mold_identity_order(broken, trusted_image=trusted)

    def test_mold_checkout_reuse_rejects_skipping_a_different_source(self):
        ref = 'needs.select.outputs.tested_sha'
        tools = ("          ref: ${{ github.workflow_sha }}\n"
                 "          persist-credentials: false\n"
                 "          fetch-depth: 1\n"
                 "          sparse-checkout: ${{ (" + ref + " == '' || github.workflow_sha != " + ref
                 + ") && '.github/actions/setup-linux-mold' || '' }}\n")
        source = ("        if: ${{ " + ref + " == '' || github.workflow_sha != " + ref + " }}\n"
                  "          ref: ${{ " + ref + " }}\n"
                  "          persist-credentials: false\n")
        validate_mold_checkout_reuse(tools, '', source)
        with self.assertRaises(AssertionError):
            validate_mold_checkout_reuse(tools, '', source.replace('workflow_sha !=', 'workflow_sha =='))

    def test_historical_source_builds_load_mold_from_the_workflow_revision(self):
        # A historical product checkout may predate the local action entirely.
        for filename in ('rust.yml', 'mainnet-packages.yml', 'release-packages.yml', 'testnet-packages.yml'):
            jobs = workflow_jobs((ROOT / '.github/workflows' / filename).read_text())
            for name, job in jobs.items():
                if 'uses: ./.github/actions/setup-linux-mold' not in job:
                    continue
                if filename == 'rust.yml' and name == 'newapi-bridge-package':
                    continue  # This job always builds the executing github.sha.
                with self.subTest(workflow=filename, job=name):
                    steps = re.findall(r'^      - .*?(?=^      - |\Z)', job, re.M | re.S)
                    setup = next(i for i, step in enumerate(steps)
                                 if 'uses: ./.github/actions/setup-linux-mold' in step)
                    tools_index = setup - 1
                    if (filename == 'rust.yml' and name == 'oasis7-test-build-trusted'
                            and '    container:' in job):
                        self.assertIn('name: Verify trusted ordinary image and source', steps[tools_index])
                        self.assertIn('test "$(git rev-parse HEAD)" = "$GITHUB_SHA"', steps[tools_index])
                        tools_index -= 1
                    tools = steps[tools_index]
                    self.assertIn('uses: actions/checkout@', tools)
                    self.assertIn('ref: ${{ github.workflow_sha }}', tools)
                    source_index = setup + 1
                    if (filename == 'rust.yml'
                            and 'sparse-checkout: .github/actions/setup-linux-mold\n' not in tools):
                        validate_mold_checkout_reuse(tools, steps[setup], steps[source_index])
                    else:
                        self.assertIn('sparse-checkout: .github/actions/setup-linux-mold\n', tools)
                    self.assertNotIn('sparse-checkout-cone-mode: false', tools)
                    if (filename == 'mainnet-packages.yml' and name == 'build-web-dist'
                            and '    container:\n' in job):
                        self.assertIn('persist-credentials: false', tools)
                        self.assertEqual([line for line in steps[source_index].splitlines()
                                          if line.strip() and not line.lstrip().startswith("#")], [
                            "      - name: Stage the executing workflow's pinned Trunk installer",
                            '        env:',
                            '          WORKFLOW_TOOLS_SHA: ${{ github.workflow_sha }}',
                            '        run: |',
                            '          git config --global --add safe.directory "$GITHUB_WORKSPACE"',
                            '          test "$(git rev-parse HEAD)" = "$WORKFLOW_TOOLS_SHA"',
                            '          installer="$RUNNER_TEMP/oasis7-mainnet-install-trunk.sh"',
                            '          git show HEAD:scripts/install-ci-trunk.sh > "$installer"',
                            '          test -s "$installer"',
                            '          chmod 700 "$installer"',
                        ])
                        source_index += 1
                        self.assertIn('ref: ${{ needs.plan.outputs.checkoutSha }}', steps[source_index])
                        self.assertIn('persist-credentials: false', steps[source_index])
                    self.assertIn('uses: actions/checkout@', steps[source_index])
                    self.assertNotIn('sparse-checkout:', steps[source_index])
                    if 'runs-on: ${{ matrix.' in job:
                        self.assertIn("if: runner.os == 'Linux'", steps[setup - 1])
                        self.assertIn("if: runner.os == 'Linux'", steps[setup])

    def test_real_git_restores_business_files_after_sparse_linker_tools(self):
        tools = re.search(r'^      - name: Check out workflow linker setup\n.*?(?=^      - |\Z)',
                          JOBS['pr-governance-preview'], re.M | re.S).group()
        pattern = re.search(r'^          sparse-checkout: (.+)$', tools, re.M)[1]
        # The historical-source branch still checks out only the action directory.
        if pattern.startswith('${{'):
            self.assertIn("&& '.github/actions/setup-linux-mold' || ''", pattern)
            pattern = '.github/actions/setup-linux-mold'
        cone = 'sparse-checkout-cone-mode: false' not in tools
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / 'source'
            source.mkdir()

            def git(cwd, *args):
                return subprocess.run(['git', *args], cwd=cwd, text=True,
                                      capture_output=True, check=True).stdout.strip()

            git(source, 'init', '-q', '--initial-branch=main')
            git(source, 'config', 'user.name', 'CI fixture')
            git(source, 'config', 'user.email', 'fixture@example.invalid')
            git(source, 'config', 'commit.gpgSign', 'false')
            (source / 'scripts').mkdir()
            (source / 'scripts/ci-tests.sh').write_text('echo historical-source\n')
            (source / 'Cargo.toml').write_text('[workspace]\n')
            git(source, 'add', '.')
            git(source, 'commit', '-qm', 'Business source before linker action')
            historical = git(source, 'rev-parse', 'HEAD')
            action = source / '.github/actions/setup-linux-mold'
            action.mkdir(parents=True)
            (action / 'action.yml').write_text('name: fixture\n')
            (source / 'scripts/ci-tests.sh').write_text('echo current-source\n')
            git(source, 'add', '.')
            git(source, 'commit', '-qm', 'Add workflow linker tools')
            current = git(source, 'rev-parse', 'HEAD')
            for label, revision in (('current', current), ('historical', historical)):
                with self.subTest(source=label):
                    checkout = root / label
                    git(root, 'clone', '-q', '--no-checkout', str(source), str(checkout))
                    if cone:
                        git(checkout, 'sparse-checkout', 'set', pattern)
                    else:
                        git(checkout, 'config', 'core.sparseCheckout', 'true')
                        (checkout / '.git/info/sparse-checkout').write_text(pattern + '\n')
                    git(checkout, 'checkout', '--force', current)
                    self.assertTrue((checkout / '.github/actions/setup-linux-mold/action.yml').is_file())
                    self.assertFalse((checkout / 'scripts/ci-tests.sh').exists())
                    # Exact sparse-disable sequence in the pinned checkout action:
                    # non-cone common config can become active again after unset.
                    git(checkout, 'sparse-checkout', 'disable')
                    git(checkout, 'config', '--local', '--unset-all', 'extensions.worktreeConfig')
                    git(checkout, 'checkout', '--force', revision)
                    self.assertEqual((checkout / 'scripts/ci-tests.sh').read_text(), f'echo {label}-source\n')
                    self.assertTrue((checkout / 'Cargo.toml').is_file())
                    self.assertEqual(git(checkout, 'rev-parse', 'HEAD'), revision)
                    self.assertEqual(git(checkout, 'status', '--porcelain'), '')
                    self.assertEqual((checkout / '.github/actions/setup-linux-mold/action.yml').exists(),
                                     label == 'current')

    def test_step_aliases_preserve_order_and_reject_unknown_or_duplicate_definitions(self):
        workflow = '''jobs:
  reader:
    steps:
      - &prepare
        name: Prepare
        run: prepare
      - &validate
        run: validate
  writer:
    steps:
      - *prepare
      - *validate
'''
        jobs = workflow_jobs('\n' + workflow)
        self.assertEqual(jobs['reader'], jobs['writer'])
        self.assertIn('      - name: Prepare\n        run: prepare\n      - run: validate\n', jobs['writer'])
        reordered = workflow.replace('      - *prepare\n      - *validate', '      - *validate\n      - *prepare')
        self.assertNotEqual(workflow_jobs('\n' + reordered)['writer'], jobs['reader'])
        for broken in (workflow.replace('*prepare', '*missing'),
                       workflow.replace('&validate', '&prepare'),
                       workflow.replace('        run: validate\n', '')):
            with self.subTest(workflow=broken), self.assertRaises(AssertionError):
                workflow_jobs('\n' + broken)

    def test_selected_checkout_normalizes_only_the_trusted_writer_ref(self):
        reader, writer = 'oasis7-test-build-candidate', 'oasis7-test-build-trusted'
        original = JOBS[writer]
        direct = original.replace('ref: ${{ needs.select.outputs.tested_sha }}', 'ref: ${{ github.sha }}', 1)
        with patch.dict(JOBS, {writer: direct}):
            self.assertEqual(comparable_source_steps(writer).strip(), comparable_source_steps(reader).strip())
        for ref in ('${{ inputs.ref }}', '${{ needs.other.outputs.tested_sha }}',
                    '${{ github.sha || inputs.ref }}'):
            with patch.dict(JOBS, {writer: direct.replace('${{ github.sha }}', ref, 1)}):
                with self.subTest(ref=ref), self.assertRaises(AssertionError):
                    selected_source_steps(writer)
        with patch.dict(JOBS, {reader: JOBS[reader].replace('ref: ${{ needs.select.outputs.tested_sha }}',
                                                        'ref: ${{ github.sha }}', 1)}):
            with self.assertRaises(AssertionError):
                selected_source_steps(reader)
        extra_checkout = ('      - name: Another checkout\n'
                          '        uses: actions/checkout@v4\n'
                          '        with:\n          ref: ${{ inputs.ref }}\n')
        with patch.dict(JOBS, {writer: direct + extra_checkout}):
            with self.assertRaises(AssertionError):
                selected_source_steps(writer)
        for changed in (direct.replace('persist-credentials: false', 'persist-credentials: true', 1),
                        direct.replace('          fetch-depth: 0', '          path: another', 1),
                        direct.replace('          fetch-depth: 0', '          repository: another/repo', 1),
                        direct.replace('run: python3 scripts/ci-workspace-cache.py identity', 'run: false #', 1)):
            with patch.dict(JOBS, {writer: changed}):
                if 'sparse-checkout: ${{' in direct and 'run: false #' not in changed:
                    with self.assertRaises(AssertionError):
                        selected_source_steps(writer)
                else:
                    self.assertNotEqual(comparable_source_steps(writer).strip(), comparable_source_steps(reader).strip())

    def test_writer_identity_is_bound_by_the_real_selector_and_job_conditions(self):
        selection = re.search(r'^          trusted_source=false\n.*?^          fi\n',
                              JOBS['select'], re.M | re.S).group()
        script = textwrap.dedent(selection) + 'printf "%s" "$trusted_source"\n'
        for ref, workflow_sha, source_sha in itertools.product(
                ('refs/heads/main', 'refs/heads/candidate', 'refs/pull/1/merge'), ('a', 'b'), ('a', 'b')):
            result = subprocess.run(['bash', '-euo', 'pipefail', '-c', script],
                                    env=dict(os.environ, GITHUB_REF=ref, WORKFLOW_SHA=workflow_sha,
                                             GITHUB_SHA='a', source_sha=source_sha),
                                    text=True, capture_output=True, check=True)
            trusted = ref == 'refs/heads/main' and workflow_sha == source_sha == 'a'
            self.assertEqual(result.stdout, str(trusted).lower())
            for event, mode in (('push', ''), ('schedule', ''), ('workflow_dispatch', 'full'),
                                ('workflow_dispatch', 'required'), ('pull_request', '')):
                values = {
                    'needs.select.result': 'success',
                    'needs.select.outputs.run_oasis7_required': 'true',
                    'needs.select.outputs.run_formal_process': 'true',
                    'needs.select.outputs.trusted_source': result.stdout,
                    'fromJSON(needs.select.outputs.plan).scope': 'full',
                    'fromJSON(needs.select.outputs.plan).execution_profile': 'periodic-full',
                    'fromJSON(needs.select.outputs.plan).fast_plan_version': 0,
                    'fromJSON(needs.select.outputs.plan).ordinary_main_fast_cache_writer_version': 0,
                    'fromJSON(needs.select.outputs.plan).fast_execution_version': 0,
                    'fromJSON(needs.select.outputs.plan).ordinary_execution': '',
                    'github.event_name': event, 'github.ref': ref, 'inputs.run_mode': mode,
                    'github.workflow_sha': workflow_sha, 'github.sha': 'a',
                }
                for name in TRUSTED_SOURCE_WRITERS:
                    condition = JOBS[name].split('    if:', 1)[1].split('    runs-on:', 1)[0]
                    allowed_event = (event == 'schedule' or (event == 'workflow_dispatch' and mode == 'full')
                                     or (event == 'push' and name != 'full-regression'))
                    with self.subTest(job=name, ref=ref, event=event, workflow=workflow_sha, source=source_sha):
                        self.assertEqual(effective_job_condition(condition, values, ['success']),
                                         trusted and allowed_event)

    def test_governance_preview_is_complete_read_only_and_has_no_gate_authority(self):
        job = JOBS['pr-governance-preview']
        for required in (
                'needs: [select, pr-fast-preview-plan]',
                "github.event_name == 'pull_request'",
                "needs.pr-fast-preview-plan.outputs.enabled == 'true'",
                'contents: read', 'persist-credentials: false', 'fetch-depth: 0',
                'ref: ${{ needs.select.outputs.tested_sha }}',
                'python3 scripts/ci-governance-preview.py',
                '--base-sha "$BASE_SHA"', '--source-sha "$ACTUAL_SOURCE_SHA"',
                '--tested-sha "$TESTED_SHA"',
                'BASE_SHA: ${{ github.event.pull_request.base.sha }}',
                'ACTUAL_SOURCE_SHA: ${{ needs.select.outputs.source_sha }}',
                'TESTED_SHA: ${{ needs.select.outputs.tested_sha }}',
                'name: governance-preview-${{ github.run_id }}-${{ github.run_attempt }}',
                'if: always()'):
            self.assertIn(required, job)
        condition = re.search(r'^    if: >-\n(.*?)^    runs-on:', job, re.M | re.S)[1]
        values = {'github.event_name': 'pull_request', 'needs.select.result': 'success',
                  'needs.pr-fast-preview-plan.result': 'success',
                  'needs.pr-fast-preview-plan.outputs.enabled': 'true'}
        # select admits PRs after its periodic-only ancestor is skipped.
        ancestors = ['skipped', 'success', 'success']
        self.assertTrue(effective_job_condition(condition, values, ancestors))
        self.assertFalse(effective_job_condition(condition, values, ancestors, cancelled=True))
        for prerequisite in ('needs.select.result', 'needs.pr-fast-preview-plan.result'):
            for outcome in ('failure', 'cancelled', 'skipped'):
                changed = dict(values, **{prerequisite: outcome})
                self.assertFalse(effective_job_condition(condition, changed, ancestors))
        self.assertNotIn('continue-on-error', job)
        self.assertNotRegex(job, r'(?m)^\s+[a-z-]+:\s+write\s*$')
        self.assertNotIn('uses: actions/cache', job)
        self.assertNotIn('uses: Swatinem/rust-cache', job)
        self.assertNotIn('full-result', job)
        for gate in ('required-gate', 'explicit-full-gate'):
            self.assertNotIn('pr-governance-preview', JOBS[gate])
            self.assertNotIn('governance-preview-', JOBS[gate])

    def shared_cache_workflow(self):
        path = ROOT / '.github/workflows/rust-cache.yml'
        self.assertTrue(path.is_file(), 'dependency warming must have an independent workflow')
        workflow = path.read_text()
        jobs = workflow_jobs(workflow)
        web_cache_layout(workflow, (ROOT / '.github/workflows/release-packages.yml').read_text(),
                         (ROOT / 'scripts/ci-rust-cache-admission.py').read_text())
        return workflow, jobs['shared-native-cache']

    def test_component_image_writer_matches_reader_contexts_and_rejects_partial_wiring(self):
        workflow, _ = self.shared_cache_workflow()
        jobs = workflow_jobs(workflow)
        job, planner = jobs['rust-image-component-cache'], jobs.get('plan-image-component-cache')
        source = ast.parse((ROOT / 'scripts/ci-rust-cache-admission.py').read_text())
        families = ast.literal_eval(next(node.value for node in source.body
            if isinstance(node, ast.Assign) and any(isinstance(target, ast.Name)
                and target.id == 'FAMILIES' for target in node.targets)))
        expected_profiles = {'native', 'node', 'net', 'launcher', 'tooling'}
        image_families = {'rust-image-components', 'rust-image-net', 'rust-image-launcher', 'rust-image-tooling'}
        if 'rust-image-support' in families:
            expected_profiles.add('support')
            image_families.add('rust-image-support')
        self.assertEqual(set(component_image_cache_writer(job, families, planner=planner)), expected_profiles)
        # These global values enter the upstream Rust environment hash. The
        # matrix must not create a different identity before any Cargo runs.
        def cache_environment(text):
            header = text.split('\njobs:\n', 1)[0]
            return dict(re.findall(r'^  ((?:CARGO|RUST|OASIS7_WASM)[A-Z0-9_]*): (.+)$', header, re.M))
        self.assertEqual(cache_environment(workflow), cache_environment(RAW_WORKFLOW))
        mutations = (
            job.replace('max-parallel: 1', 'max-parallel: 5', 1),
            job.replace('add-rust-environment-hash-key: true', 'add-rust-environment-hash-key: false', 1),
            job.replace('cache-on-failure: false', 'cache-on-failure: true', 1),
            job.replace('shared-key: ${{ matrix.namespace }}', 'shared-key: ci-shared-native-trusted-v4', 1),
            job.replace('env-vars: CARGO CC CFLAGS CXX CMAKE RUST OASIS7_WASM', 'env-vars: CARGO', 1),
            job.replace('cache-on-failure: false', 'cache-on-failure: false\n          workspaces: . -> other-target', 1),
            job.replace('OASIS7_DEPENDENCY_CACHE_BUDGET_BYTES: ${{ env.CI_CACHE_REPOSITORY_BUDGET_BYTES }}',
                        'OASIS7_DEPENDENCY_CACHE_BUDGET_BYTES: 999999999999', 1),
            job.replace('python3 scripts/ci-rust-cache-admission.py install', 'true', 1),
            job.replace('ci-image-cache.py --profile "$IMAGE_CACHE_PROFILE"', 'ci-image-cache.py --profile native', 1),
            job.replace("job.status == 'success'", "job.status != 'cancelled'", 1),
            job.replace('persist-credentials: false', 'persist-credentials: true', 1),
        )
        for changed in mutations:
            with self.subTest(change=changed), self.assertRaises(AssertionError):
                component_image_cache_writer(changed, families, planner=planner)
        row_mutations = (
            ('profile: node', 'profile: native', '"profile":"node"', '"profile":"native"',
             '"profile": "node"', '"profile": "native"'),
            ('mold-dir: oasis7-node-mold', 'mold-dir: oasis7-other-mold',
             '"mold-dir":"oasis7-node-mold"', '"mold-dir":"oasis7-other-mold"',
             '"mold-dir": "oasis7-node-mold"', '"mold-dir": "oasis7-other-mold"'),
            ('options: --user 1001:1001', "options: ''",
             '"options":"--user 1001:1001"', '"options":""',
             '"options": "--user 1001:1001"', '"options": ""'),
            ('family: rust-image-net', 'family: rust-image-components',
             '"family":"rust-image-net"', '"family":"rust-image-components"',
             '"family": "rust-image-net"', '"family": "rust-image-components"'),
        )
        for old_static, new_static, old_inline, new_inline, old_planned, new_planned in row_mutations:
            with self.subTest(original=old_static), self.assertRaises(AssertionError):
                if planner is None:
                    mutated = (job.replace(old_inline, new_inline, 1)
                               if 'fromJSON(' in job else job.replace(old_static, new_static, 1))
                    component_image_cache_writer(mutated, families)
                else:
                    component_image_cache_writer(job, families,
                                                 planner=planner.replace(old_planned, new_planned, 1))
        for family in sorted(image_families):
            with self.subTest(missing=family), self.assertRaises(AssertionError):
                component_image_cache_writer(job, {key: value for key, value in families.items() if key != family}, planner=planner)
        changed_reader = dict(JOBS, node=JOBS['node'].replace('oasis7-node-mold', 'different-node-mold'))
        with self.assertRaises(AssertionError):
            component_image_cache_writer(job, families, changed_reader, planner=planner)

    def test_container_image_writer_post_guard_requires_the_complete_transition(self):
        workflow, _ = self.shared_cache_workflow()
        jobs = workflow_jobs(workflow)
        for name in ('rust-image-native-cache', 'rust-image-component-cache'):
            job = jobs[name]
            save = re.search(r'^          save-if: (.+)$', job, re.M)[1]
            save = save.removeprefix('${{ ').removesuffix(' }}')
            guard = "steps.dependency-cache-tools.outputs.guard_ready == 'true'"
            save = save.replace(" && " + guard, "")
            guarded = job.replace('ci-rust-cache-admission.py install\n', 'ci-rust-cache-admission.py install-release\n')
            guarded = guarded.replace('          PATH: ${{ steps.dependency-cache-tools.outputs.path }}\n',
                '          NODE_OPTIONS: ${{ steps.dependency-cache-tools.outputs.node_options }}\n'
                '          OASIS7_DEPENDENCY_CACHE_PYTHON: ${{ steps.dependency-cache-tools.outputs.python }}\n')
            guarded = guarded.replace('          save-if: ${{ ' + save + ' }}\n',
                '          cache-provider: github\n'
                '          save-if: ${{ ' + save.replace("job.status == 'success'",
                                                       "job.status == 'success' && " + guard, 1) + ' }}\n')
            image_cache_admission_layout(guarded, rust_cache_steps(guarded)[0], save)
            for absent in ('NODE_OPTIONS', 'OASIS7_DEPENDENCY_CACHE_PYTHON', 'cache-provider'):
                changed = re.sub(r'^          ' + absent + r': .+\n', '', guarded, flags=re.M)
                self.assertNotEqual(changed, guarded)
                with self.subTest(name=name, absent=absent), self.assertRaises(AssertionError):
                    image_cache_admission_layout(changed, rust_cache_steps(changed)[0], save)
            changed = guarded.replace(' && ' + guard, '')
            with self.subTest(name=name, absent='guard_ready'), self.assertRaises(AssertionError):
                image_cache_admission_layout(changed, rust_cache_steps(changed)[0], save)

    def test_component_image_writer_runs_only_for_current_main_cache_events(self):
        workflow, _ = self.shared_cache_workflow()
        job = workflow_jobs(workflow)['rust-image-component-cache']
        supports_image_support = ('plan-image-component-cache' in workflow_jobs(workflow)
                                  or 'image-support' in workflow.split('jobs:', 1)[0])
        expression = ' '.join(job.split('    if:', 1)[1].split('    strategy:', 1)[0]
                              .strip().removeprefix('>-').split())
        for event, ref, matching, mode, repository in itertools.product(
                ('schedule', 'workflow_dispatch', 'pull_request', 'push', 'workflow_run'),
                ('refs/heads/main', 'refs/heads/topic', 'refs/pull/1/merge', 'refs/tags/v1'),
                (True, False), ('full', 'image-native', 'image-support', 'pr-fast', ''), ('nil-studio/oasis7', 'fork/oasis7')):
            values = {'github.event_name': event, 'github.ref': ref, 'github.workflow_sha': 'a',
                      'github.sha': 'a' if matching else 'b', 'inputs.run_mode': mode, 'github.repository': repository}
            predicate = expression
            for token, value in values.items():
                predicate = predicate.replace(token, repr(value))
            with self.subTest(event=event, ref=ref, matching=matching, mode=mode):
                self.assertEqual(eval(predicate.replace('&&', ' and ').replace('||', ' or '), {'__builtins__': {}}, {}),
                    ref == 'refs/heads/main' and matching and (event == 'schedule'
                    or event == 'workflow_dispatch' and (mode in ('full', 'image-native')
                        or mode == 'image-support' and repository == 'nil-studio/oasis7' and supports_image_support)))

    def test_component_cache_planner_runs_the_complete_or_support_only_matrix(self):
        workflow, _ = self.shared_cache_workflow()
        jobs = workflow_jobs(workflow)
        planner = jobs.get('plan-image-component-cache')
        writer = jobs['rust-image-component-cache']
        if planner is None:
            if 'fromJSON(' in writer:
                self.assertIn('image-support', workflow.split('jobs:', 1)[0])
                include_expr = writer.split('        include:', 1)[1].split('\n    runs-on:', 1)[0]
                include_expr = ' '.join(include_expr.strip().removeprefix('>-').split())
                self.assertTrue(include_expr.startswith('${{') and include_expr.endswith('}}'))
                inner = include_expr[3:-2].strip()
                full_rows = None
                for mode in ('', 'full', 'image-native', 'image-support'):
                    expr = inner.replace('inputs.run_mode', repr(mode)).replace('&&', ' and ').replace('||', ' or ')
                    rows = eval(expr, {'__builtins__': {}}, {'fromJSON': json.loads})
                    profiles = [row['profile'] for row in rows]
                    if mode == 'image-support':
                        self.assertEqual(profiles, ['support'])
                        self.assertEqual(rows, [row for row in full_rows if row['profile'] == 'support'])
                    else:
                        self.assertEqual(profiles, ['native', 'node', 'net', 'launcher', 'tooling', 'support'])
                        full_rows = rows
                return
            self.assertNotIn('image-support', workflow.split('jobs:', 1)[0])
            self.assertIn('matrix:\n        include:\n', writer)
            self.assertEqual(re.findall(r'^          - profile: ([a-z]+)$', writer, re.M),
                             ['native', 'node', 'net', 'launcher', 'tooling', 'support'])
            return
        predicate = lambda job: ' '.join(job.split('    if: >-\n', 1)[1].split('\n    ', 1)[0].split())
        self.assertEqual(predicate(planner), predicate(writer))
        script = textwrap.dedent(planner.split('        run: |\n', 1)[1]).strip()
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / 'output'
            for mode in ('', 'full', 'image-native', 'image-support'):
                output.write_text('')
                subprocess.run(['bash', '-e', '-c', script], check=True,
                               env=os.environ | {'MODE': mode, 'GITHUB_OUTPUT': str(output)})
                rows = json.loads(output.read_text().removeprefix('rows='))
                profiles = [row['profile'] for row in rows]
                if mode == 'image-support':
                    self.assertEqual(profiles, ['support'])
                    self.assertEqual(rows, [row for row in full_rows if row['profile'] == 'support'])
                else:
                    self.assertEqual(profiles, ['native', 'node', 'net', 'launcher', 'tooling', 'support'])
                    full_rows = rows

    def test_web_cache_migration_rejects_partial_or_multiple_writers(self):
        namespace = 'release-packages-web-wasm-trusted-v3'
        old_identity = (namespace, '.github/workflows/release-packages.yml', 'build-web-dist')
        new_identity = (namespace, '.github/workflows/rust-cache.yml', 'web-wasm-cache')
        # Small wiring fixtures exercise the layout decision; the actual
        # producer's trust, environment and complete build are checked below.
        native = 'name: fixture\ncache-mode: read\njobs:\n  shared-native-cache:\n    steps: []\n'
        web_step = ('      - uses: Swatinem/rust-cache@pinned\n'
                    '        env:\n          OASIS7_DEPENDENCY_CACHE_FAMILY: release-web\n'
                    '        with:\n          shared-key: ' + namespace + '\n'
                    "          save-if: ${{ job.status == 'success' }}\n")
        old_release = ('name: fixture\ncache-mode: write\njobs:\n  build-web-dist:\n'
                       '    steps:\n' + web_step)
        new_cache = native + '  web-wasm-cache:\n    cache-mode: write\n    steps:\n' + web_step
        new_release = ('name: fixture\ncache-mode: write\njobs:\n  build-web-dist:\n'
                       '    cache-mode: read\n    steps:\n' + web_step.replace(
                           '        env:\n          OASIS7_DEPENDENCY_CACHE_FAMILY: release-web\n', '').replace(
                           "save-if: ${{ job.status == 'success' }}", 'save-if: false'))
        def source(identity):
            return 'FAMILIES = ' + repr({'release-web': identity})
        self.assertFalse(web_cache_layout(native, old_release, source(old_identity)))
        self.assertTrue(web_cache_layout(new_cache, new_release, source(new_identity)))
        for cache, release, registry in (
            (new_cache, old_release, source(new_identity)),
            (native, new_release, source(old_identity)),
            (new_cache, new_release, source(old_identity)),
            (native, old_release, source(new_identity)),
            (new_cache, new_release.replace('    cache-mode: read\n', ''), source(new_identity)),
            (new_cache.replace('    cache-mode: write\n', '    cache-mode: read\n'), new_release, source(new_identity)),
            (new_cache, new_release, source((namespace, '.github/workflows/unknown.yml', 'web-wasm-cache'))),
            (new_cache, new_release, 'FAMILIES = ' + repr({'release-web': new_identity, 'extra': new_identity})),
        ):
            with self.subTest(cache=cache, release=release, registry=registry):
                with self.assertRaises(AssertionError):
                    web_cache_layout(cache, release, registry)

    def test_viewer_cache_transition_requires_the_entire_known_configuration(self):
        for name in ('viewer-js-required', 'viewer-performance-report'):
            actual = JOBS[name]
            kind = viewer_cache_kind(actual)
            if viewer_image_digest(actual):
                self.assertEqual(kind, 'web')
                for before, after in (
                    ('ci-rust-viewer-', 'ci-rust-build-'),
                    ("env.VIEWER_IMAGE_COMPATIBLE == 'true' && 'ci-rust-viewer-",
                     "env.VIEWER_IMAGE_COMPATIBLE != 'true' && 'ci-rust-viewer-"),
                    ('release-packages-web-wasm-trusted-v3', 'ci-shared-native-trusted-v4'),
                    ('save-if: false', 'save-if: true'),
                    ('add-rust-environment-hash-key: true', 'add-rust-environment-hash-key: false'),
                    ('env-vars: CARGO CC CFLAGS CXX CMAKE RUST\n',
                     'env-vars: CARGO CC CFLAGS CXX CMAKE RUST OASIS7_WASM\n'),
                    ('cache-on-failure: false', 'cache-on-failure: false\n          workspaces: . -> other-target'),
                ):
                    self.assertIn(before, actual)
                    with self.subTest(job=name, mutation=before), self.assertRaises(AssertionError):
                        viewer_cache_kind(actual.replace(before, after))
                preparation = re.search(r'(?ms)^      - name: Stabilize Rust cache toolchains\n.*?(?=^      - )', actual)[0]
                false_guard = preparation.replace("env.VIEWER_IMAGE_COMPATIBLE != 'true'",
                                                  "env.VIEWER_IMAGE_COMPATIBLE == 'true'")
                self.assertNotEqual(preparation, false_guard)
                with self.subTest(job=name, mutation='mutable image toolchain'), self.assertRaises(AssertionError):
                    viewer_cache_kind(actual.replace(preparation, false_guard))
                continue
            native = actual if kind == 'native' else actual.replace(
                'shared-key: release-packages-web-wasm-trusted-v3', 'shared-key: ci-shared-native-trusted-v4').replace(
                'env-vars: CARGO CC CFLAGS CXX CMAKE RUST\n', 'env-vars: CARGO CC CFLAGS CXX CMAKE RUST OASIS7_WASM\n').replace(
                'python3 scripts/ci-rust-cache-toolchains.py "$RUST_TOOLCHAIN"\n',
                'python3 scripts/ci-rust-cache-toolchains.py "$RUST_TOOLCHAIN" "$OASIS7_WASM_TOOLCHAIN"\n').replace(
                '          rustup default "$RUST_TOOLCHAIN"\n',
                '          rustup default "$RUST_TOOLCHAIN"\n'
                '          rustup toolchain install "$OASIS7_WASM_TOOLCHAIN" --profile minimal --component rust-src\n'
                '          rustup target add "$OASIS7_WASM_TARGET" --toolchain "$OASIS7_WASM_TOOLCHAIN"\n')
            web = native.replace('shared-key: ci-shared-native-trusted-v4',
                                 'shared-key: release-packages-web-wasm-trusted-v3').replace(
                'env-vars: CARGO CC CFLAGS CXX CMAKE RUST OASIS7_WASM',
                'env-vars: CARGO CC CFLAGS CXX CMAKE RUST').replace(
                ' "$OASIS7_WASM_TOOLCHAIN"\n', '\n')
            web = re.sub(r'^          rustup (?:toolchain install|target add) [^\n]*OASIS7_WASM[^\n]*\n', '', web, flags=re.M)
            self.assertEqual(viewer_cache_kind(native), 'native', name)
            self.assertEqual(viewer_cache_kind(web), 'web', name)
            for mutation in (
                native.replace('shared-key: ci-shared-native-trusted-v4', 'shared-key: release-packages-web-wasm-trusted-v3'),
                web.replace('save-if: false', 'save-if: true'),
                web.replace('add-rust-environment-hash-key: true', 'add-rust-environment-hash-key: false'),
                web.replace('env-vars: CARGO CC CFLAGS CXX CMAKE RUST\n',
                            'env-vars: CARGO CC CFLAGS CXX CMAKE RUST OASIS7_WASM\n'),
                web.replace('python3 scripts/ci-rust-cache-toolchains.py "$RUST_TOOLCHAIN"',
                            'python3 scripts/ci-rust-cache-toolchains.py "$RUST_TOOLCHAIN" "$OASIS7_WASM_TOOLCHAIN"'),
                web.replace('          rustup default', '          rustup toolchain install nightly\n          rustup default'),
                web.replace('          cache-on-failure: false', '          cache-on-failure: false\n          workspaces: . -> other-target'),
            ):
                with self.subTest(job=name, mutation=mutation):
                    with self.assertRaises(AssertionError):
                        viewer_cache_kind(mutation)

    def test_viewer_image_eligibility_uses_tested_git_inputs_and_preserves_host_fallback(self):
        selection = JOBS['select']
        if '      viewer_image_compatible:' not in selection:
            return
        step = workflow_step_script(selection, 'Select the compatible Viewer image environment')
        inputs = ('rust-toolchain.toml', 'scripts/ci-browser-deps.py', 'scripts/ci_process_runner.py',
                  'scripts/ci-browser-tools/package.json', 'scripts/ci-browser-tools/package-lock.json',
                  'scripts/ensure-wasm-bindgen-cli.sh', 'scripts/wasm-bindgen-cli-common.sh')
        with tempfile.TemporaryDirectory() as temporary:
            fixture = Path(temporary).resolve()

            def git(*args):
                return subprocess.check_output(['git', '-C', str(fixture), '-c', 'user.email=ci@example.invalid',
                    '-c', 'user.name=CI Fixture', '-c', 'commit.gpgsign=false', *args], text=True).strip()

            git('init', '-q')
            for name in inputs:
                path = fixture / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(name)
            git('add', '.')
            git('commit', '-qm', 'Frozen Viewer image inputs')
            source = git('rev-parse', 'HEAD')
            script, count = re.subn(r'(?m)^image_source=[0-9a-f]{40}$', 'image_source=' + source, step)
            self.assertEqual(count, 1)

            def check(tested_sha, compatible):
                output = fixture / 'github-output'
                output.unlink(missing_ok=True)
                result = subprocess.run(['bash', '-euo', 'pipefail', '-c', script], cwd=fixture,
                    env=dict(os.environ, TESTED_SHA=tested_sha, GITHUB_OUTPUT=str(output)), text=True, capture_output=True)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(output.read_text(), 'compatible=' + str(compatible).lower() + '\n')

            (fixture / 'unrelated').write_text('new payload')
            git('add', 'unrelated')
            git('commit', '-qm', 'Unrelated new source')
            check(git('rev-parse', 'HEAD'), True)
            for name in inputs:
                with self.subTest(changed=name):
                    path = fixture / name
                    path.unlink()
                    git('add', name)
                    git('commit', '-qm', 'Incompatible historical input')
                    check(git('rev-parse', 'HEAD'), False)
                    git('reset', '--hard', '-q', source)
            check('f' * 40, False)
            check('not-a-commit', False)

        for name, browser in (('viewer-js-required', 'security smoke'),
                              ('viewer-performance-report', 'performance')):
            job = JOBS[name]
            self.assertIsNotNone(viewer_image_digest(job))
            self.assertIn("Install pinned " + browser + " browser\n        if: env.VIEWER_IMAGE_COMPATIBLE != 'true'", job)
            self.assertIn("if: env.VIEWER_IMAGE_COMPATIBLE == 'true'", job)
            self.assertIn('activate --prebuilt --browser agent', job)
            self.assertLess(job.index('activate --prebuilt --browser agent'), job.index('Execute selected group'))
            self.assertIn('export XDG_CACHE_HOME=/home/runner/.cache', job)
            self.assertIn('export XDG_CACHE_HOME=/.cache', job)

    def test_web_cache_writer_preserves_trust_and_complete_build(self):
        workflow, native = self.shared_cache_workflow()
        jobs = workflow_jobs(workflow)
        release_workflow = (ROOT / '.github/workflows/release-packages.yml').read_text()
        if 'web-wasm-cache' not in jobs:
            release = workflow_jobs(release_workflow)['build-web-dist']
            self.assertIn('./scripts/build-viewer-software-safe.sh --optional-payload-dir', release)
            self.assertIn('env -u NO_COLOR trunk build --release --dist', release)
            return
        self.assert_web_cache_writer(workflow, release_workflow)

    def assert_web_cache_writer(self, workflow, release_workflow):
        jobs = workflow_jobs(workflow)
        native = jobs['shared-native-cache']
        writer = jobs['web-wasm-cache']
        header = writer.split('    steps:', 1)[0]
        for required in ('cache-mode: write', 'runs-on: ubuntu-24.04',
                         'permissions:\n      contents: read\n      actions: read'):
            self.assertIn(required, header)
        self.assertNotIn('needs:', header)
        self.assertNotIn('env:', header)
        self.assertNotIn('web-wasm-cache', WORKFLOW)
        self.assertEqual(re.search(r'    if: >-\n(.*?)    runs-on:', writer, re.S)[1],
                         re.search(r'    if: >-\n(.*?)    runs-on:', native, re.S)[1])
        steps = re.findall(r'^      - .*?(?=^      - |\Z)', writer, re.M | re.S)
        source = next(step for step in steps if 'name: Verify current main cache source\n' in step)
        native_source = next(step for step in re.findall(r'^      - .*?(?=^      - |\Z)', native, re.M | re.S)
                             if 'name: Verify current main cache source\n' in step)
        self.assertEqual(source, native_source)
        self.assertEqual(writer.count('uses: actions/checkout@'), 1)
        self.assertIn('ref: ${{ github.sha }}\n          persist-credentials: false', writer)
        self.assertIn('npm ci --prefix crates/oasis7_viewer', writer)
        self.assertNotIn('OASIS7_WASM', writer)
        self.assertNotIn('nightly', writer)
        self.assertIn('run: python3 scripts/ci-rust-cache-toolchains.py "$RUST_TOOLCHAIN"\n', writer)
        self.assertEqual(len(rust_cache_steps(writer)), 1)
        cache = rust_cache_steps(writer)[0]
        self.assertNotRegex(cache, re.compile(r'^        if:', re.M))
        self.assertIn('uses: Swatinem/rust-cache@d1527c8f67ef9af357290475d4af4b0fab8f12e0 # v2\n', cache)
        before_cache = writer[:writer.index(cache)]
        self.assertEqual(re.findall(r'^          rustup toolchain install (\S+)', before_cache, re.M),
                         ['"${RUST_TOOLCHAIN}"'])
        self.assertIn('rustup default "${RUST_TOOLCHAIN}"', before_cache)
        self.assertIn('rustup target add wasm32-unknown-unknown --toolchain "${RUST_TOOLCHAIN}"', before_cache)
        install = next(step for step in steps if 'id: dependency-cache-tools\n' in step)
        self.assertIn('run: python3 scripts/ci-rust-cache-admission.py install-release\n', install)
        self.assertIn('continue-on-error: true', install)
        for required in ('continue-on-error: true', 'cache-provider: github', 'cache-on-failure: false',
                         'NODE_OPTIONS: ${{ steps.dependency-cache-tools.outputs.node_options }}',
                         'OASIS7_DEPENDENCY_CACHE_PYTHON: ${{ steps.dependency-cache-tools.outputs.python }}',
                         'OASIS7_DEPENDENCY_CACHE_SOURCE_SHA: ${{ github.sha }}',
                         'OASIS7_DEPENDENCY_CACHE_BUDGET_BYTES: ${{ env.CI_CACHE_REPOSITORY_BUDGET_BYTES }}'):
            self.assertIn(required, cache)
        self.assertNotIn('env-vars:', cache)
        save = re.search(r'^          save-if: \$\{\{ (.*) \}\}$', cache, re.M)[1]
        with_body = cache.split('        with:\n', 1)[1]
        inputs = re.findall(r'^          ([\w-]+): (.+)$', with_body, re.M)
        self.assertEqual(len(inputs), len(with_body.strip().splitlines()))
        expected_inputs = {'shared-key': 'release-packages-web-wasm-trusted-v3',
                           'cache-provider': 'github', 'cache-on-failure': 'false',
                           'save-if': '${{ ' + save + ' }}'}
        if ('add-rust-environment-hash-key', 'true') in inputs:
            expected_inputs['add-rust-environment-hash-key'] = 'true'
        self.assertEqual(len(inputs), len(dict(inputs)))
        self.assertEqual(dict(inputs), expected_inputs)
        environment = re.search(r'^        env:\n(.*?)(?=^        \S|\Z)', cache, re.M | re.S)[1]
        entries = re.findall(r'^          ([\w-]+): (.+)$', environment, re.M)
        self.assertEqual(len(entries), len(environment.strip().splitlines()))
        self.assertEqual(len(entries), len(dict(entries)))
        self.assertEqual(dict(entries), {
            'NODE_OPTIONS': '${{ steps.dependency-cache-tools.outputs.node_options }}',
            'OASIS7_DEPENDENCY_CACHE_PYTHON': '${{ steps.dependency-cache-tools.outputs.python }}',
            'OASIS7_DEPENDENCY_CACHE_FAMILY': 'release-web',
            'OASIS7_DEPENDENCY_CACHE_SOURCE_SHA': '${{ github.sha }}',
            'OASIS7_DEPENDENCY_CACHE_BUDGET_BYTES': '${{ env.CI_CACHE_REPOSITORY_BUDGET_BYTES }}',
            'GH_TOKEN': '${{ github.token }}',
        })
        self.assertNotRegex(save, r'(?i)\b(?:always|cancelled|failure|success)\s*\(')
        for status, guard, event, mode, ref, matching in itertools.product(
                ('success', 'failure', 'cancelled'), ('true', 'false', ''),
                ('schedule', 'workflow_dispatch', 'push', 'pull_request'), ('full', '', 'pr-fast'),
                ('refs/heads/main', 'refs/heads/topic'), (False, True)):
            values = {'job.status': status, 'steps.dependency-cache-tools.outputs.guard_ready': guard,
                      'github.event_name': event, 'inputs.run_mode': mode, 'github.ref': ref,
                      'github.workflow_sha': 'a' if matching else 'b', 'github.sha': 'a'}
            predicate = save
            for key in sorted(values, key=len, reverse=True):
                predicate = predicate.replace(key, repr(values[key]))
            expected = (status == 'success' and guard == 'true' and ref == 'refs/heads/main'
                        and matching and (event == 'schedule' or event == 'workflow_dispatch' and mode == 'full'))
            self.assertEqual(eval(predicate.replace('&&', ' and ').replace('||', ' or '),
                                  {'__builtins__': {}}, {}), expected, values)
        self.assertNotIn('NODE_OPTIONS:', workflow.split('\njobs:\n', 1)[0])
        self.assertLess(writer.index(source), writer.index(cache))
        normalizer = next(step for step in steps if 'python3 scripts/ci-rust-cache-toolchains.py' in step)
        self.assertNotRegex(normalizer, re.compile(r'^        if:', re.M))
        self.assertLess(writer.index('rustup default'), writer.index('python3 scripts/ci-rust-cache-toolchains.py'))
        self.assertLess(writer.index('python3 scripts/ci-rust-cache-toolchains.py'), writer.index(cache))
        self.assertLess(steps.index(install), steps.index(cache))
        completion = next(step for step in steps if 'name: Complete Web WASM cache\n' in step)
        self.assertIn('id: web-cache\n', cache)
        self.assertIn('set -euo pipefail', completion)
        self.assertIn('./scripts/build-viewer-software-safe.sh --optional-payload-dir output/release/viewer-optional-payload', completion)
        self.assertIn('cd crates/oasis7_client_launcher', completion)
        self.assertIn('env -u NO_COLOR trunk build --release --dist ../../output/release/web-launcher-dist', completion)
        self.assertLess(steps.index(cache), steps.index(completion))
        release_steps = re.findall(r'^      - .*?(?=^      - |\Z)',
                                   workflow_jobs(release_workflow)['build-web-dist'], re.M | re.S)
        self.assertEqual(re.search(r'^  TRUNK_VERSION: (.+)$', workflow, re.M)[1],
                         re.search(r'^  TRUNK_VERSION: (.+)$', release_workflow, re.M)[1])
        for name in ('Restore pinned Trunk release archive', 'Install verified pinned Trunk', 'Verify pinned trunk'):
            step = next(step for step in steps if 'name: ' + name + '\n' in step)
            self.assertEqual(step, next(step for step in release_steps if 'name: ' + name + '\n' in step))
            self.assertLess(steps.index(step), steps.index(completion))
        self.assertNotIn('uses: actions/cache@', writer)
        self.assertNotIn('uses: actions/cache/save@', writer)
        condition = re.search(r'^        if: (.*)$', completion, re.M)[1]
        for hit in ('true', 'false', '', 'unknown'):
            self.assertEqual(eval(condition.replace('steps.web-cache.outputs.cache-hit', repr(hit)),
                                  {'__builtins__': {}}, {}), hit != 'true')
        for step in steps:
            if step != cache:
                self.assertNotIn('NODE_OPTIONS:', step)
                self.assertNotIn('OASIS7_DEPENDENCY_CACHE_', step)
                self.assertNotIn('        env:', step)
            if step not in (install, cache):
                self.assertNotIn('continue-on-error:', step)
        self.assertNotRegex(writer, r'\b(?:CARGO|RUST|CC|CFLAGS|CXX|CMAKE)[A-Z0-9_]*=')

    def test_web_writer_complete_fixture_rejects_cache_context_drift(self):
        workflow, native = self.shared_cache_workflow()
        release = (ROOT / '.github/workflows/release-packages.yml').read_text()
        release_web = workflow_jobs(release)['build-web-dist']
        def step(job, marker):
            return next(part for part in re.findall(r'^      - .*?(?=^      - |\Z)', job, re.M | re.S)
                        if marker in part)
        # The host writer owns toolchain installation after the Release reader
        # moves to a preinstalled compiler. Keep the older layout as a fixture.
        host_web = workflow_jobs(workflow).get('web-wasm-cache', release_web)
        header = native.split('    steps:', 1)[0]
        preparation = ''.join((step(native, 'uses: actions/checkout@'),
            step(native, 'name: Verify current main cache source\n'),
            step(release_web, 'uses: actions/setup-node@'),
            step(release_web, 'name: Install viewer web dependencies\n'),
            step(host_web, 'name: Install pinned Rust toolchain\n')))
        cache = '''      - name: Normalize Rust cache toolchains
        shell: bash
        run: python3 scripts/ci-rust-cache-toolchains.py "$RUST_TOOLCHAIN"
      - name: Install Web cache admission
        id: dependency-cache-tools
        continue-on-error: true
        run: python3 scripts/ci-rust-cache-admission.py install-release
      - uses: Swatinem/rust-cache@d1527c8f67ef9af357290475d4af4b0fab8f12e0 # v2
        id: web-cache
        continue-on-error: true
        env:
          NODE_OPTIONS: ${{ steps.dependency-cache-tools.outputs.node_options }}
          OASIS7_DEPENDENCY_CACHE_PYTHON: ${{ steps.dependency-cache-tools.outputs.python }}
          OASIS7_DEPENDENCY_CACHE_FAMILY: release-web
          OASIS7_DEPENDENCY_CACHE_SOURCE_SHA: ${{ github.sha }}
          OASIS7_DEPENDENCY_CACHE_BUDGET_BYTES: ${{ env.CI_CACHE_REPOSITORY_BUDGET_BYTES }}
          GH_TOKEN: ${{ github.token }}
        with:
          shared-key: release-packages-web-wasm-trusted-v3
          save-if: ${{ job.status == 'success' && steps.dependency-cache-tools.outputs.guard_ready == 'true' && github.ref == 'refs/heads/main' && github.workflow_sha == github.sha && (github.event_name == 'schedule' || (github.event_name == 'workflow_dispatch' && inputs.run_mode == 'full')) }}
          cache-provider: github
          cache-on-failure: false
'''
        trunk = ''.join(step(release_web, 'name: ' + name + '\n') for name in (
            'Restore pinned Trunk release archive', 'Install verified pinned Trunk', 'Verify pinned trunk'))
        completion = '''      - name: Complete Web WASM cache
        if: steps.web-cache.outputs.cache-hit != 'true'
        shell: bash
        run: |
          set -euo pipefail
          mkdir -p output/release/viewer-optional-payload output/release/web-launcher-dist
          OASIS7_FORCE_VIEWER_SOFTWARE_SAFE_BUILD=1 ./scripts/build-viewer-software-safe.sh --optional-payload-dir output/release/viewer-optional-payload
          (
            cd crates/oasis7_client_launcher
            env -u NO_COLOR trunk build --release --dist ../../output/release/web-launcher-dist
          )
'''
        writer = header + '    steps:\n' + preparation + cache + trunk + completion
        prefix = workflow.split('\njobs:\n', 1)[0]
        if '  TRUNK_VERSION:' not in prefix:
            prefix += '\n  TRUNK_VERSION: ' + re.search(r'^  TRUNK_VERSION: (.+)$', release, re.M)[1] + '\n'
        def assembled(candidate):
            return prefix + '\njobs:\n  shared-native-cache:\n' + native + '\n  web-wasm-cache:\n' + candidate
        self.assert_web_cache_writer(assembled(writer), release)
        for mutation in (
            writer.replace('          cache-provider: github', '          cache-provider: github\n          workspaces: . -> different-target'),
            writer.replace('          cache-provider: github', '          cache-provider: github\n          add-rust-environment-hash-key: false'),
            writer.replace('          cache-provider: github', '          cache-provider: github\n          cache-targets: false'),
            writer.replace('    steps:\n', '    env:\n      CARGO_PROFILE_RELEASE_OPT_LEVEL: "0"\n    steps:\n', 1),
            writer.replace('          GH_TOKEN:', '          CARGO_TARGET_DIR: other-target\n          GH_TOKEN:'),
            writer.replace('          rustup default', '          rustup toolchain install 1.95.0\n          rustup default'),
        ):
            with self.subTest(mutation=mutation):
                with self.assertRaises(AssertionError):
                    self.assert_web_cache_writer(assembled(mutation), release)

    def test_web_cache_build_runs_both_contexts_and_propagates_each_failure(self):
        workflow, _ = self.shared_cache_workflow()
        writer = workflow_jobs(workflow)['web-wasm-cache']
        completion = next(step for step in re.findall(r'^      - .*?(?=^      - |\Z)', writer, re.M | re.S)
                          if 'name: Complete Web WASM cache\n' in step)
        script = textwrap.dedent(completion.split('        run: |\n', 1)[1])
        self.assertNotIn('continue-on-error:', completion)
        with tempfile.TemporaryDirectory(prefix='web-cache-build-') as directory:
            root = Path(directory)
            (root / 'scripts').mkdir()
            (root / 'crates/oasis7_client_launcher').mkdir(parents=True)
            (root / 'bin').mkdir()
            viewer = root / 'scripts/build-viewer-software-safe.sh'
            viewer.write_text('''#!/usr/bin/env bash
set -euo pipefail
test "$OASIS7_FORCE_VIEWER_SOFTWARE_SAFE_BUILD" = 1
test "$*" = '--optional-payload-dir output/release/viewer-optional-payload'
printf 'viewer\\n' >> "$WEB_CACHE_FIXTURE_TRACE"
exit "$WEB_CACHE_FIXTURE_VIEWER_EXIT"
''')
            trunk = root / 'bin/trunk'
            trunk.write_text('''#!/usr/bin/env bash
set -euo pipefail
test -z "${NO_COLOR+x}"
test "$PWD" = "$WEB_CACHE_FIXTURE_ROOT/crates/oasis7_client_launcher"
test "$*" = 'build --release --dist ../../output/release/web-launcher-dist'
printf 'trunk\\n' >> "$WEB_CACHE_FIXTURE_TRACE"
exit "$WEB_CACHE_FIXTURE_TRUNK_EXIT"
''')
            for path in (viewer, trunk):
                path.chmod(0o755)
            trace = root / 'calls'
            for viewer_exit, trunk_exit, expected_calls in (
                    (0, 0, ['viewer', 'trunk']), (17, 0, ['viewer']), (0, 23, ['viewer', 'trunk'])):
                trace.write_text('')
                environment = dict(os.environ, PATH=str(root / 'bin') + os.pathsep + os.environ['PATH'],
                    NO_COLOR='fixture-value', WEB_CACHE_FIXTURE_ROOT=str(root),
                    WEB_CACHE_FIXTURE_TRACE=str(trace), WEB_CACHE_FIXTURE_VIEWER_EXIT=str(viewer_exit),
                    WEB_CACHE_FIXTURE_TRUNK_EXIT=str(trunk_exit))
                result = subprocess.run(['bash', '--noprofile', '--norc', '-e', '-o', 'pipefail', '-c', script],
                                        cwd=root, env=environment, capture_output=True, text=True, timeout=10)
                with self.subTest(viewer=viewer_exit, trunk=trunk_exit):
                    self.assertEqual(result.returncode, viewer_exit or trunk_exit, result.stdout + result.stderr)
                    self.assertEqual(trace.read_text().splitlines(), expected_calls)

    def run_selection(self, directory, *, event='schedule', legacy=False, candidate=False, source_ref='',
                      real_receipt=False, ref=None, policy_config=None, candidate_config=None):
        """Execute the real workflow shell against immutable local Git fixtures."""
        repo = Path(directory) / 'repo'
        repo.mkdir()
        subprocess.run(['git', 'init', '-q', '-b', 'main', str(repo)], check=True)
        def git(*args):
            return subprocess.check_output(['git', '-C', str(repo), *args], text=True).strip()
        git('config', 'user.name', 'CI fixture')
        git('config', 'user.email', 'fixture@example.invalid')
        (repo / 'scripts').mkdir()
        files = ('plan-rust-required-scope.py', 'ci-required-scope.json', 'ci-required-result.py',
                 'ci-formal-plan.py', 'ci-formal-process-suites.json')
        for name in files:
            (repo / 'scripts' / name).write_bytes((ROOT / 'scripts' / name).read_bytes())
        if policy_config is not None:
            (repo / 'scripts/ci-required-scope.json').write_text(json.dumps(policy_config))
        if not legacy:
            (repo / 'scripts/ci-python-control-contracts.py').write_bytes(
                (ROOT / 'scripts/ci-python-control-contracts.py').read_bytes())
        if real_receipt:
            import runpy
            policy_files = set(runpy.run_path(str(ROOT / 'scripts/ci-full-result.py'))['POLICY_FILES'])
            if CONFIG.get('custody_execution_version') == 1:
                policy_files.add('scripts/ci-custody-platform.py')
            if CONFIG.get('custody_compile_proof_version') == 1:
                policy_files.add('crates/oasis7_custody_worker/scripts/verify.sh')
            if 'scripts/ci-worker-build.py' in CONFIG.get('python_control_contracts', {}):
                policy_files.add('scripts/ci-worker-build.py')
            if 'scripts/ci-custody-workerd-capture.py' in CONFIG.get('python_control_contracts', {}):
                policy_files.update(('scripts/ci-custody-workerd-capture.py', 'scripts/ci-custody-workerd-gdb.py',
                                     'scripts/ci-custody-workerd-gdb-contracts.py'))
            policy_files.update(test for entry in CONFIG.get('python_control_contracts', {}).values()
                                for test in entry['tests'])
            policy_files.update(CONFIG.get('workflow_governance_python_tests', []))
            for path in policy_files:
                destination = repo / path
                destination.parent.mkdir(parents=True, exist_ok=True)
                destination.write_bytes((ROOT / path).read_bytes())
        else:
            (repo / 'scripts/ci-full-result.py').write_text('# producer is not executed by selection\n')
        if legacy:
            (repo / 'scripts/plan-rust-required-scope.py').write_text('''import argparse,json,pathlib
p=argparse.ArgumentParser()
for key in ('base-ref','head-ref','test-ref','github-output','output'): p.add_argument('--'+key)
a=p.parse_args()
plan={'scope':'full','groups':json.loads(pathlib.Path(__file__).with_name('ci-required-scope.json').read_text())['groups'],
      'architecture':{'version':1,'mode':'full','packages':[],
                      'base':a.base_ref,'source':a.head_ref,'tested':a.test_ref}}
pathlib.Path(a.output).write_text(json.dumps(plan))
with pathlib.Path(a.github_output).open('a') as f: f.write('plan='+json.dumps(plan)+'\\nfuture_contract=preserved\\n')
''')
        git('add', '.')
        git('commit', '-qm', 'trusted policy')
        policy = git('rev-parse', 'HEAD')
        git('remote', 'add', 'origin', str(repo))
        git('checkout', '-qb', 'candidate')
        (repo / 'README.md').write_text('candidate behavior\n')
        # If the shell accidentally runs candidate policy instead of BASE, fail.
        (repo / 'scripts/plan-rust-required-scope.py').write_text("raise SystemExit('untrusted candidate planner executed')\n")
        (repo / 'scripts/ci-python-control-contracts.py').write_text("raise SystemExit('untrusted candidate control authority executed')\n")
        if candidate_config is not None:
            (repo / 'scripts/ci-required-scope.json').write_text(json.dumps(candidate_config))
        git('add', '.')
        git('commit', '-qm', 'candidate')
        source = git('rev-parse', 'HEAD')
        if not candidate and not source_ref:
            git('checkout', '-q', 'main')
            source = policy
        requested = source if source_ref == 'candidate' else source_ref
        script = workflow_step_script(JOBS['select'], 'Select using trusted baseline Git data')
        output = Path(directory) / 'github-output'
        (Path(directory) / 'selection-outputs.txt').write_text('stale_output=must-not-survive\n')
        environment = dict(os.environ, EVENT_NAME=event, SOURCE_REF=requested,
            SOURCE_SHA=source if candidate else policy, BASE_SHA=policy, WORKFLOW_SHA=source if candidate else policy,
            GITHUB_SHA=source if candidate else policy,
            GITHUB_REF=ref or ('refs/heads/candidate' if candidate and event != 'pull_request' else
                              'refs/pull/1/merge' if event == 'pull_request' else 'refs/heads/main'),
            RUNNER_TEMP=str(directory), GITHUB_OUTPUT=str(output),
            GITHUB_STEP_SUMMARY=str(Path(directory) / 'summary'))
        result = subprocess.run(['bash', '-c', script], cwd=repo, env=environment,
                                text=True, capture_output=True)
        values = dict(line.split('=', 1) for line in output.read_text().splitlines()) if output.exists() else {}
        self.assertNotIn('plan', values, 'the trusted planner must not expose the complete plan as an Actions output')
        self.assertNotIn('stale_output', values)
        plan = None
        if result.returncode == 0:
            plan = json.loads((Path(directory) / 'selection.json').read_text())
            values['plan'] = json.dumps(project_selection(directory), separators=(',', ':'))
        return result, values, plan, policy, source

    def test_large_workflow_projection_preserves_fields_and_binds_complete_file_and_identity(self):
        fields = {'scope', 'execution_profile', 'fast_plan_version', 'fast_execution_version',
                  'ordinary_execution', 'ordinary_artifact_delivery_version', 'ordinary_artifact_partition_version',
                  'oasis7_required_packages', 'custody_execution_version', 'custody_node_shards',
                  'python_control_contract_version', 'groups', 'source_sha', 'tested_sha', 'policy_sha',
                  'check_inventory_version', 'fast_plan_exceptions'}
        consumers = set(re.findall(r'fromJSON\(needs\.select\.outputs\.plan\)\.([A-Za-z0-9_]+)', RAW_WORKFLOW))
        fleet_subcontract_fields = {'fleet_health_subcontracts_version', 'fleet_health_oses', 'fleet_health_checks'}
        self.assertLessEqual(consumers, fields | fleet_subcontract_fields | {
            'ordinary_dynamic_partition_version', 'ordinary_controls_in_rust_baseline_version',
            'ordinary_main_fast_cache_writer_version',
            'contract_job_consolidation_version', 'command_center_offline_version',
            'workspace_support_direct_gate_version',
            'custody_compile_proof_version', 'custody_policy_tests_version', 'selection_sha256'})
        self.assertIn('plan: ${{ steps.workflow-plan.outputs.plan }}', JOBS['select'])
        guard = workflow_step_script(JOBS['required-gate'], 'Validate complete selection handoff')
        self.assertEqual(guard, workflow_step_script(JOBS['explicit-full-gate'], 'Validate complete selection handoff'))
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            plan = large_transport_plan()
            projected = project_selection(root, plan)
            raw = (root / 'selection.json').read_bytes()
            self.assertGreater(len(raw), 128 * 1024)
            self.assertLess(len(json.dumps(projected).encode()), 16 * 1024)
            self.assertEqual(set(projected), fields | {'workflow_projection_version', 'selection_sha256'})
            self.assertEqual({key: projected[key] for key in fields}, {key: plan[key] for key in fields})
            self.assertEqual(project_selection(root), projected)
            self.assertEqual((root / 'selection.json').read_bytes(), raw)
            environment = dict(os.environ, RUNNER_TEMP=directory, SOURCE_SHA=plan['source_sha'],
                               TESTED_SHA=plan['tested_sha'], POLICY_SHA=plan['policy_sha'])
            def invoke(value):
                return subprocess.run(['bash', '-euo', 'pipefail', '-c', guard], cwd=root,
                    env=dict(environment, WORKFLOW_PLAN=json.dumps(value)), text=True, capture_output=True)
            result = invoke(projected)
            self.assertEqual(result.returncode, 0, result.stderr)
            if fleet_subcontract_fields.intersection(consumers):
                fleet_extra = {
                    'fleet_health_subcontracts_version': 1,
                    'fleet_health_checks': ['macos_signer'],
                    'fleet_health_oses': ['macos-15'],
                }
                fleet_projected = project_selection(root, plan | fleet_extra)
                for key, expected_val in fleet_extra.items():
                    self.assertEqual(fleet_projected.get(key), expected_val)
                fleet_result = invoke(fleet_projected)
                self.assertEqual(fleet_result.returncode, 0, fleet_result.stderr)
                self.assertNotEqual(invoke(fleet_projected | {'fleet_health_oses': ['windows-2022']}).returncode, 0)
                project_selection(root, plan)
            for key in fields:
                for broken in ({k: v for k, v in projected.items() if k != key},
                               projected | {key: {'changed': True}}):
                    with self.subTest(field=key, missing=key not in broken):
                        self.assertNotEqual(invoke(broken).returncode, 0)
            for change in ({'workflow_projection_version': True}, {'selection_sha256': '0' * 64},
                           {'unknown_projection_field': 1}, {'fast_plan_version': True}):
                self.assertNotEqual(invoke(projected | change).returncode, 0)
            (root / 'selection.json').write_bytes(raw + b' ')
            self.assertNotEqual(invoke(projected).returncode, 0)
            for key in ('source_sha', 'tested_sha', 'policy_sha'):
                with self.subTest(identity=key):
                    changed = project_selection(root, plan | {key: '0' * 40})
                    self.assertNotEqual(invoke(changed).returncode, 0)
            legacy = {key: plan[key] for key in ('scope', 'groups', 'architecture', 'reasons')}
            old = project_selection(root, legacy)
            result = invoke(old)
            self.assertEqual(result.returncode, 0, result.stderr)
            for change in ({'base': '0' * 40}, {'source': None}, {'tested': '0' * 40}):
                changed = project_selection(root, legacy | {'architecture': legacy['architecture'] | change})
                self.assertNotEqual(invoke(changed).returncode, 0)
            changed = project_selection(root, {key: value for key, value in legacy.items() if key != 'architecture'})
            self.assertNotEqual(invoke(changed).returncode, 0)
            (root / 'selection.json').unlink()
            self.assertNotEqual(invoke(projected).returncode, 0)
            self.assertFalse((root / 'transport-must-not-execute').exists())

    def test_required_gate_runs_real_json_cli_with_large_file_and_preserves_failure(self):
        gate = JOBS['required-gate']
        script = (workflow_step_script(gate, 'Validate complete selection handoff') + '\n'
                  + workflow_step_script(gate, 'Validate all expected results'))
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            authority = root / 'ci-authority'
            authority.mkdir()
            validator = authority / 'ci-required-result.py'
            source = (ROOT / 'scripts/ci-required-result.py').read_text()
            (authority / 'ci-required-scope.json').write_text(json.dumps(CONFIG))
            (authority / 'adjacent_fixture.py').write_text('VALUE = "authority sibling"\n')
            validator.write_text('from adjacent_fixture import VALUE\nassert VALUE == "authority sibling"\n' + source)
            plan = large_transport_plan()
            projected = project_selection(root, plan)
            needs = {group.replace('_', '-'): {'result': 'success' if group in plan['groups'] else 'skipped'}
                     for group in CONFIG['groups']}
            needs['select'] = {'result': 'success', 'outputs': {'plan': json.dumps(projected)}}
            environment = dict(os.environ, RUNNER_TEMP=directory, SOURCE_SHA=plan['source_sha'],
                TESTED_SHA=plan['tested_sha'], POLICY_SHA=plan['policy_sha'], WORKFLOW_PLAN=json.dumps(projected),
                NEEDS=json.dumps(needs))
            def invoke(**changes):
                return subprocess.run(['bash', '-euo', 'pipefail', '-c', script], cwd=root,
                    env=environment | changes, text=True, capture_output=True)
            result = invoke()
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertIn('All selected CI groups succeeded.', result.stdout)
            self.assertLess(len(environment['NEEDS'].encode()), 16 * 1024)
            for result_name in ('failure', 'skipped', 'cancelled'):
                with self.subTest(result=result_name):
                    broken = needs | {'workspace-support': {'result': result_name}}
                    self.assertNotEqual(invoke(NEEDS=json.dumps(broken)).returncode, 0)
            self.assertNotEqual(invoke(NEEDS=json.dumps(needs | {'extra-job': {'result': 'failure'}})).returncode, 0)
            self.assertNotEqual(invoke(NEEDS='[]').returncode, 0)
            missing = project_selection(root, plan | {'groups': ['workspace_support']})
            self.assertNotEqual(invoke(WORKFLOW_PLAN=json.dumps(missing)).returncode, 0)
            project_selection(root, plan)
            for prefix, status in (('raise SystemExit(19)\n', 19), ('raise RuntimeError("authority failed")\n', 1)):
                validator.write_text(prefix + source)
                self.assertEqual(invoke().returncode, status)
            self.assertFalse((root / 'transport-must-not-execute').exists())

    def test_trusted_selection_projects_custody_proof_only_for_the_capable_baseline(self):
        job_condition = re.search(r'^    if: (.*?)(?=^    [a-z-]+:|\Z)',
                                  JOBS['custody-native-contracts'], re.M | re.S)[1]
        proof_step = next(step for step in re.findall(r'^      - .*?(?=^      - |\Z)',
                                                      JOBS['custody-native-contracts'], re.M | re.S)
                          if 'name: Execute complete custody WASM compile proofs\n' in step)
        step_condition = re.search(r'^        if: (.+)$', proof_step, re.M)[1]
        for version in ('new', 'old-custody', 'legacy'):
            policy = dict(CONFIG)
            if version != 'new':
                del policy['custody_compile_proof_version']
                del policy['custody_policy_tests_version']
            if version == 'legacy':
                del policy['custody_execution_version']
            with self.subTest(version=version), tempfile.TemporaryDirectory() as directory:
                result, outputs, plan, _, _ = self.run_selection(directory, policy_config=policy)
                self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                projected = json.loads(outputs['plan'])
                self.assertEqual('custody_compile_proof_version' in plan, version == 'new')
                self.assertEqual('custody_compile_proof_version' in projected, version == 'new')
                values = {'needs.select.result': 'success',
                          'fromJSON(needs.select.outputs.plan).custody_execution_version': projected.get('custody_execution_version', 0),
                          'fromJSON(needs.select.outputs.plan).custody_compile_proof_version': projected.get('custody_compile_proof_version', 0)}
                job_runs = effective_job_condition(job_condition, values, ['skipped', 'success'])
                step_runs = job_runs and effective_job_condition(step_condition, values, [])
                self.assertEqual(job_runs, version in ('new', 'old-custody'))
                self.assertEqual(step_runs, version == 'new')

    def test_current_workflow_preserves_the_fixed_historical_custody_source_protocol(self):
        old = {key: value for key, value in CONFIG.items()
               if key not in ('custody_compile_proof_version', 'custody_policy_tests_version')}
        for event in ('workflow_dispatch', 'pull_request', 'push'):
            with self.subTest(event=event), tempfile.TemporaryDirectory() as directory:
                result, outputs, plan, policy, source = self.run_selection(
                    directory, event=event, source_ref='candidate' if event == 'workflow_dispatch' else '',
                    candidate=event != 'workflow_dispatch', ref='refs/heads/main' if event == 'push' else None,
                    candidate_config=old)
                self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                self.assertNotEqual(source, policy)
                self.assertEqual(plan['custody_execution_version'], 1)
                self.assertNotIn('custody_compile_proof_version', plan)
                projected = json.loads(outputs['plan'])
                self.assertNotIn('custody_compile_proof_version', projected)
                self.assertEqual(plan['tested_sha'], source)
                self.assertIn('oasis7_custody_worker', plan['workspace_support_profiles'])

    def test_manual_and_scheduled_selection_expands_complete_fixed_inventory(self):
        for event in ('workflow_dispatch', 'schedule'):
            with self.subTest(event=event), tempfile.TemporaryDirectory() as directory:
                result, values, plan, policy, source = self.run_selection(directory, event=event)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(set(plan['groups']), set(CONFIG['groups']))
                self.assertEqual(plan['scope'], 'explicit-full')
                self.assertEqual(plan['source_sha'], source)
                self.assertEqual(plan['tested_sha'], source)
                self.assertEqual(plan['policy_sha'], policy)
                self.assertEqual(values['trusted_source'], 'true')
                for group in CONFIG['groups']:
                    self.assertEqual(values['run_' + group], 'true')
        with tempfile.TemporaryDirectory() as directory:
            result, values, plan, policy, source = self.run_selection(directory, event='workflow_dispatch', source_ref='candidate')
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertNotEqual(source, policy)
            self.assertEqual(plan['source_sha'], source)
            self.assertEqual(plan['policy_sha'], policy)
            self.assertEqual(values['trusted_source'], 'false')

    def test_old_policy_cannot_be_replaced_by_candidate_fast_policy(self):
        with tempfile.TemporaryDirectory() as directory:
            result, values, plan, policy, source = self.run_selection(directory, event='pull_request', legacy=True, candidate=True)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertNotIn('fast_plan_version', plan)
            self.assertEqual(set(plan['groups']), set(CONFIG['groups']))
            self.assertEqual(values['policy_sha'], policy)
            self.assertEqual(values['future_contract'], 'preserved')
        with tempfile.TemporaryDirectory() as directory:
            result, values, plan, policy, source = self.run_selection(directory, legacy=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertIsNone(plan)
        with tempfile.TemporaryDirectory() as directory:
            result, values, plan, policy, source = self.run_selection(directory, event='workflow_dispatch', legacy=True, candidate=True)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertTrue(plan['bootstrap_full'])
            self.assertNotIn('fast_plan_version', plan)
            self.assertEqual(set(plan['groups']), set(CONFIG['groups']))
            self.assertEqual(values['trusted_source'], 'false')

    def test_candidate_source_input_requires_trusted_main_dispatch(self):
        for event, candidate in (('schedule', False), ('workflow_dispatch', True)):
            with self.subTest(event=event), tempfile.TemporaryDirectory() as directory:
                result, values, plan, policy, source = self.run_selection(
                    directory, event=event, candidate=candidate, source_ref='candidate')
                self.assertNotEqual(result.returncode, 0)
                self.assertIsNone(plan)

    def test_selection_artifacts_preserve_the_select_attempt_on_failed_job_reruns(self):
        job = JOBS['select']
        step = job.split('      - id: selection-artifacts\n', 1)[1].split('      - uses:', 1)[0]
        script = textwrap.dedent(step.split('        run: |\n', 1)[1])
        attempts = []
        with tempfile.TemporaryDirectory() as directory:
            for attempt in (1, 2):
                output = Path(directory) / str(attempt)
                result = subprocess.run(['bash', '-euo', 'pipefail', '-c', script],
                    env=dict(os.environ, GITHUB_RUN_ID='123', GITHUB_RUN_ATTEMPT=str(attempt),
                             GITHUB_OUTPUT=str(output)), text=True, capture_output=True)
                self.assertEqual(result.returncode, 0, result.stderr)
                names = dict(line.split('=', 1) for line in output.read_text().splitlines())
                self.assertEqual(names, {'selection_artifact': f'ci-selection-123-{attempt}',
                                         'authority_artifact': f'ci-authority-123-{attempt}'})
                attempts.append(names)
        self.assertNotEqual(attempts[0], attempts[1])
        for kind in ('selection', 'authority'):
            output = kind + '_artifact'
            self.assertIn(f'{output}: ${{{{ steps.selection-artifacts.outputs.{output} }}}}', job)
            self.assertIn(f'name: ${{{{ steps.selection-artifacts.outputs.{output} }}}}', job)
            consumers = [body for name, body in JOBS.items() if name != 'select'
                         and f'needs.select.outputs.{output}' in body]
            self.assertTrue(consumers)
            for body in consumers:
                self.assertIn(f'name: ${{{{ needs.select.outputs.{output} }}}}', body)
                self.assertNotIn(f'name: ci-{kind}-${{{{ github.run_id }}}}', body)

    def test_explicit_full_gate_covers_every_group_and_records_only_success(self):
        gate = JOBS['explicit-full-gate']
        dependencies = re.search(r'    needs: \[(.*)\]', gate).group(1).split(', ')
        self.assertEqual(set(dependencies), {
            'select', 'oasis7-test-build-candidate', 'oasis7-test-build-trusted',
            'full-regression', 'candidate-full-regression',
            *(group.replace('_', '-') for group in CONFIG['groups'])})
        self.assertEqual(len(dependencies), len(set(dependencies)))
        self.assertIn('always()', gate)
        self.assertIn("trusted_main('ci-required-result.py', ['--full'", gate)
        self.assertIn("trusted_main('ci-full-result.py', ['produce'", gate)
        self.assertIn('name: ci-full-result-${{ github.run_id }}-${{ github.run_attempt }}', gate)
        self.assertNotIn('if: always()', gate.split("trusted_main('ci-full-result.py'", 1)[1])
        self.assertIn('if-no-files-found: error', gate)

    def test_full_gate_produces_verifiable_policy_bound_receipt_from_real_authority(self):
        import runpy
        helper = runpy.run_path(str(ROOT / 'scripts/ci-full-result.py'))
        gate = JOBS['explicit-full-gate']
        script = (workflow_step_script(gate, 'Validate complete selection handoff') + '\n'
                  + workflow_step_script(gate, 'Require complete full inventory and exact source'))
        for event, requested in (('schedule', ''), ('workflow_dispatch', 'candidate')):
            with self.subTest(event=event), tempfile.TemporaryDirectory() as directory:
                result, outputs, plan, policy, source = self.run_selection(
                    directory, event=event, source_ref=requested, real_receipt=True)
                self.assertEqual(result.returncode, 0, result.stderr)
                plan['reasons'] = ["quoted ' \" \\\n$(touch transport-must-not-execute)\nPY\n" * 4000]
                projected = project_selection(directory, plan)
                self.assertGreater((Path(directory) / 'selection.json').stat().st_size, 128 * 1024)
                repo = Path(directory) / 'repo'
                subprocess.run(['git', '-C', str(repo), 'checkout', '-q', policy], check=True)
                retired = ({group.replace('_', '-') for group in CONFIG.get('group_jobs', {})}
                           if plan.get('contract_job_consolidation_version') in (1, 2, 3, 4, 5) else set())
                needs = {group.replace('_', '-'): {'result': 'skipped' if group.replace('_', '-') in retired else 'success'}
                         for group in CONFIG['groups']}
                trusted = source == policy
                needs.update(select={'result': 'success', 'outputs': {'plan': json.dumps(projected)}}, **{
                    'oasis7-test-build-candidate': {'result': 'skipped' if trusted else 'success'},
                    'oasis7-test-build-trusted': {'result': 'success' if trusted else 'skipped'},
                })
                needs['full-regression'] = {'result': 'success' if trusted else 'skipped'}
                needs['candidate-full-regression'] = {'result': 'skipped' if trusted else 'success'}
                environment = dict(os.environ, RUNNER_TEMP=directory, GITHUB_WORKSPACE=str(repo),
                    GITHUB_REPOSITORY='fixture/oasis7', GITHUB_EVENT_NAME=event,
                    GITHUB_REF='refs/heads/main', GITHUB_SHA=policy, GITHUB_WORKFLOW_SHA=policy,
                    GITHUB_WORKFLOW_REF='fixture/oasis7/.github/workflows/rust.yml@refs/heads/main',
                    GITHUB_RUN_ID='123', GITHUB_RUN_ATTEMPT='2',
                    EVENT_NAME=event, REF='refs/heads/main', WORKFLOW_SHA=policy, SOURCE_SHA=source,
                    RUN_MODE='full' if event == 'workflow_dispatch' else '',
                    TESTED_SHA=source, POLICY_SHA=policy, WORKFLOW_PLAN=json.dumps(projected),
                    NEEDS=json.dumps(needs))
                result = subprocess.run(['bash', '-euo', 'pipefail', '-c', script],
                    cwd=repo, env=environment, text=True, capture_output=True)
                self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                receipt_path = Path(directory) / 'full-result.json'
                receipt = json.loads(receipt_path.read_text())
                self.assertEqual(receipt['context']['run_attempt'], 2)
                self.assertEqual(receipt['plan']['source_sha'], source)
                self.assertEqual(receipt['plan']['policy_sha'], policy)
                self.assertEqual(receipt['plan'], plan)
                self.assertEqual(receipt['needs'], needs)
                self.assertFalse((repo / 'transport-must-not-execute').exists())
                digest = helper['definition_digest'](repo, policy)
                self.assertEqual(receipt['definition_sha256'], digest)
                run = dict(id=123, run_attempt=2, workflow_id=10, path='.github/workflows/rust.yml',
                           head_branch='main', head_sha=policy, event=event, status='completed', conclusion='success')
                jobs = [dict(id=1, name='explicit-full-gate', run_attempt=2, status='completed', conclusion='success')]
                verified = helper['verify_receipt'](receipt, run, jobs, source=source,
                    repository='fixture/oasis7', workflow_id=10, config=CONFIG, digest=digest)
                self.assertEqual(verified, receipt)
                # The same exact gate shell may never publish success after a
                # selected native platform group fails, skips or is cancelled.
                for result_name in ('failure', 'skipped', 'cancelled'):
                    receipt_path.unlink(missing_ok=True)
                    failed = dict(needs, **{'fleet-health': {'result': result_name}})
                    result = subprocess.run(['bash', '-euo', 'pipefail', '-c', script],
                        cwd=repo, env=dict(environment, NEEDS=json.dumps(failed)), text=True, capture_output=True)
                    self.assertNotEqual(result.returncode, 0)
                    self.assertFalse(receipt_path.exists())
                for change in ({'groups': plan['groups'][1:]}, {'execution_profile': 'pr-fast'},
                               {'source_sha': '0' * 40}):
                    with self.subTest(invalid_plan=change):
                        changed_projection = project_selection(directory, plan | change)
                        result = subprocess.run(['bash', '-euo', 'pipefail', '-c', script],
                            cwd=repo, env=dict(environment, WORKFLOW_PLAN=json.dumps(changed_projection)),
                            text=True, capture_output=True)
                        self.assertNotEqual(result.returncode, 0)
                        self.assertFalse(receipt_path.exists())

    def test_command_center_is_reusable_and_required(self):
        workflow = (ROOT / '.github/workflows/command-center.yml').read_text()
        self.assertIn('  workflow_call:', workflow)
        self.assertIn('\ncache-mode: read\n', workflow)
        self.assertIn('ref: ${{ inputs.source_sha }}', workflow)
        self.assertIn('persist-credentials: false', workflow)
        self.assertNotIn('  pull_request:', workflow)
        self.assertNotIn('  push:', workflow)
        self.assertIn('uses: ./.github/workflows/command-center.yml', JOBS['command-center'])
        self.assertIn('fromJSON(needs.select.outputs.plan).command_center_offline_version != 1', JOBS['command-center'])
        self.assertIn('run_command_center: ${{ steps.select.outputs.run_command_center }}', JOBS['select'])
        self.assertEqual(CONFIG.get('command_center_offline_version'), 1)
        self.assertEqual(CONFIG.get('group_jobs', {}).get('command_center'), 'baseline')
        for command in ('cargo test --locked', '--example export_types', 'npm test',
                        'npm run build', 'cargo clippy --locked', 'desktop:build -- --debug'):
            self.assertIn(command, workflow)
        self.assertNotIn('continue-on-error:', workflow)
        for package in ('oasis7_signer_protocol', 'oasis7_context', 'oasis7_local_signer', 'oasis7_secure_envelope'):
            self.assertNotIn('command_center', CONFIG['package_groups'][package])

    def test_corpus_data_uses_baseline_and_implementation_uses_platforms(self):
        for path in (
            '.github/workflows/document-corpus-platform.yml',
            'doc/.governance/document-corpus-inventory.json',
            'doc/.governance/document-corpus',
            'doc/testing/evidence/inventory.json',
            'scripts/document_corpus.py',
            'scripts/document-corpus-inventory.py',
            'scripts/document-corpus-inventory-check.py',
            'scripts/doc-evidence-inventory-check.py',
        ):
            self.assertFalse((ROOT / path).exists(), path)
        driver = (ROOT / 'scripts/ci-tests.sh').read_text()
        self.assertIn('baseline) run_required_gate_checks', driver)
        baseline = driver.split('run_required_gate_checks() {', 1)[1].split('\n}', 1)[0]
        self.assertIn('run_product_doc_governance_check', baseline)
        governance = (ROOT / 'scripts/doc-governance-check.sh').read_text()
        self.assertNotIn('scripts/document-corpus-inventory-check.py', governance)

    def test_baseline_document_range_binds_ordinary_event_and_fixed_full_source(self):
        baseline = JOBS['baseline']
        self.assertIn('ref: ${{ needs.select.outputs.tested_sha }}', baseline)
        self.assertIn('TESTED_SHA: ${{ needs.select.outputs.tested_sha }}', baseline)
        self.assertIn('CI_PROFILE: ${{ fromJSON(needs.select.outputs.plan).execution_profile }}', baseline)
        self.assertIn('OASIS7_CI_PROFILE: ${{ fromJSON(needs.select.outputs.plan).execution_profile }}', baseline)
        step = baseline.split('      - name: Resolve tested product-document range\n', 1)[1]
        step = step.split('      - name: Execute selected group\n', 1)[0]
        script = textwrap.dedent(step.split('        run: |\n', 1)[1])
        self.assertLess(script.index('test "$TESTED_SHA" = "$GITHUB_SHA"'),
                        script.index('python3 scripts/resolve-product-doc-ci-range.py'))
        self.assertIn('--tested-oid "$GITHUB_SHA" >>"$GITHUB_ENV"', script)
        self.assertNotRegex(script, r'\bGITHUB_SHA\s*=')
        with tempfile.TemporaryDirectory() as directory:
            repo = Path(directory) / 'repo'
            repo.mkdir()
            def git(*args):
                return subprocess.check_output(['git', '-C', str(repo), *args],
                                               text=True, stderr=subprocess.PIPE).strip()
            def commit(message):
                git('add', '.')
                git('commit', '-qm', message)
                return git('rev-parse', 'HEAD')
            git('init', '-q', '-b', 'main')
            git('config', 'user.name', 'Range fixture')
            git('config', 'user.email', 'fixture@example.invalid')
            (repo / 'scripts').mkdir()
            helper = 'scripts/resolve-product-doc-ci-range.py'
            (repo / helper).write_bytes((ROOT / helper).read_bytes())
            (repo / 'README.md').write_text('# Base\n')
            base = commit('event base')
            git('checkout', '-qb', 'feature')
            (repo / 'candidate.md').write_text('# Candidate\n')
            source = commit('candidate')
            git('checkout', '-q', 'main')
            (repo / 'main-only.md').write_text('# Main advanced\n')
            target = commit('main advanced after event base')
            git('merge', '--no-ff', '--no-commit', 'feature')
            (repo / 'merge-only.md').write_text('# Tested merge\n')
            merged = commit('synthetic merge')
            pr = {'pull_request': {'base': {'sha': base}, 'head': {'sha': source}}}
            event_path = Path(directory) / 'event.json'
            exports = Path(directory) / 'github-env'
            cases = (
                ('pr-fast', 'pull_request', pr, merged, merged, target),
                ('', 'pull_request', pr, merged, merged, target),
                ('main-fast', 'push', {'before': base, 'after': merged}, merged, merged, base),
                ('periodic-full', 'schedule', {}, merged, target, target),
                ('candidate-full', 'workflow_dispatch', {}, source, merged, base),
            )
            for profile, event_name, event, tested, event_sha, expected_base in cases:
                with self.subTest(profile=profile, event_name=event_name):
                    git('checkout', '-q', '--detach', tested)
                    event_path.write_text(json.dumps(event))
                    exports.write_text('')
                    environment = dict(os.environ, CI_PROFILE=profile, TESTED_SHA=tested,
                                       GITHUB_SHA=event_sha, GITHUB_EVENT_NAME=event_name,
                                       GITHUB_EVENT_PATH=str(event_path), GITHUB_ENV=str(exports),
                                       GITHUB_WORKSPACE=str(repo))
                    result = subprocess.run(['bash', '--noprofile', '--norc', '-eo', 'pipefail', '-c', script],
                                            cwd=repo, env=environment, text=True, capture_output=True)
                    self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                    self.assertEqual(exports.read_text(),
                                     f'OASIS7_PRODUCT_DOC_BASE={expected_base}\nOASIS7_PRODUCT_DOC_HEAD={tested}\n')
                    self.assertEqual(git('rev-parse', 'HEAD'), tested)
                    if event_name in ('pull_request', 'push'):
                        # The old helper alone would accept this event/checkout.
                        # A conflicting selected SHA must fail before any export.
                        exports.write_text('')
                        environment['TESTED_SHA'] = source
                        result = subprocess.run(['bash', '--noprofile', '--norc', '-eo', 'pipefail', '-c', script],
                                                cwd=repo, env=environment, text=True, capture_output=True)
                        self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
                        self.assertEqual(exports.read_text(), '')

    def test_formal_components_build_in_parallel_before_the_stable_mergers(self):
        producers = ('formal-debug-build', 'formal-debug-build-trusted',
                     'formal-endpoint-build', 'formal-endpoint-build-trusted')
        for name in producers:
            with self.subTest(producer=name):
                self.assertIn(name, JOBS)
                job = JOBS[name]
                dependencies = re.search(r'    needs: (.*)', job).group(1)
                expected = '[select, cache-bootstrap-maintenance]' if name.endswith('-trusted') else 'select'
                self.assertEqual(dependencies, expected)
                self.assertIn('ref: ${{ needs.select.outputs.tested_sha }}', selected_source_steps(name))
                self.assertIn('name: ${{ needs.select.outputs.selection_artifact }}', job)
                self.assertIn('name: ${{ needs.select.outputs.authority_artifact }}', job)
                component = 'formal-debug' if 'debug' in name else 'formal-endpoint'
                self.assertIn(f'build --scope {component}', job)
                self.assertIn('--plan "$RUNNER_TEMP/selection.json"', job)
                self.assertIn('--formal-config "$RUNNER_TEMP/ci-authority/ci-formal-process-suites.json"', job)
                self.assertNotIn('merge-formal', job)
                self.assertNotIn('--split-formal', job)
        for name in ('formal-test-build', 'formal-test-build-trusted'):
            job = JOBS[name]
            dependencies = re.search(r'    needs: \[(.*)\]', job).group(1).split(', ')
            self.assertEqual(set(dependencies), {'select', *producers})
            self.assertEqual(len(dependencies), len(set(dependencies)))
            self.assertIn('cache-mode: read', job)
            self.assertNotIn('actions/cache/', job)
            self.assertNotIn('Swatinem/rust-cache', job)
            self.assertNotIn('ci-workspace-cache.py', job)
            self.assertNotIn('ci-test-artifacts.py build ', job)
            self.assertIn('ci-test-artifacts.py merge-formal ', job)
            self.assertIn('--split-formal', job)

    def test_formal_mergers_reject_every_incomplete_producer_combination(self):
        import runpy
        aggregate = runpy.run_path(str(ROOT / 'scripts/ci-formal-plan.py'))['aggregate']
        producers = ('formal-debug-build', 'formal-debug-build-trusted',
                     'formal-endpoint-build', 'formal-endpoint-build-trusted')
        # These states include absent/unknown API results, not just ordinary
        # failure. An unselected alternative is the only permissible skip.
        states = ('success', 'skipped', 'failure', 'cancelled', '', 'unknown')
        for trusted, cancelled in itertools.product((False, True), repeat=2):
            values = {
                'always()': True, 'cancelled()': cancelled,
                'needs.select.result': 'success',
                'needs.select.outputs.run_formal_process': 'true',
                'needs.select.outputs.trusted_source': str(trusted).lower(),
                'fromJSON(needs.select.outputs.plan).execution_profile': 'periodic-full' if trusted else 'pr-fast',
                'fromJSON(needs.select.outputs.plan).fast_plan_version': 1,
                'fromJSON(needs.select.outputs.plan).ordinary_main_fast_cache_writer_version': 0,
                'github.event_name': 'schedule' if trusted else 'pull_request',
                'github.ref': 'refs/heads/main' if trusted else 'refs/pull/1/merge',
                'github.workflow_sha': 'a', 'github.sha': 'a', 'inputs.run_mode': '',
            }
            expected_success = ('skipped', 'success', 'skipped', 'success') if trusted else (
                'success', 'skipped', 'success', 'skipped')
            for results in itertools.product(states, repeat=4):
                values.update({f'needs.{name}.result': result for name, result in zip(producers, results)})
                merger_results = {}
                for name in ('formal-test-build', 'formal-test-build-trusted'):
                    condition = JOBS[name].split('    if:', 1)[1].split('    runs-on:', 1)[0].strip().removeprefix('>-')
                    for token, value in sorted(values.items(), key=lambda item: -len(item[0])):
                        condition = condition.replace(token, repr(value))
                    condition = re.sub(r'!(?!=)', 'not ', condition.replace('&&', ' and ').replace('||', ' or '))
                    actual = bool(eval(' '.join(condition.split()), {'__builtins__': {}}, {}))
                    expected = not cancelled and results == expected_success and name.endswith('-trusted') == trusted
                    with self.subTest(job=name, trusted=trusted, cancelled=cancelled, producers=results):
                        self.assertEqual(actual, expected)
                    merger_results[name] = {'result': 'success' if actual else 'skipped'}
                needs = dict(merger_results, select={'result': 'success'},
                             **{'formal-process-tests': {'result': 'success'}})
                if not cancelled and results == expected_success:
                    self.assertEqual(aggregate(True, needs), 'passed')
                else:
                    with self.assertRaises(ValueError):
                        aggregate(True, needs)

    def test_formal_component_artifacts_preserve_successful_prior_attempt_outputs(self):
        for component in ('debug', 'endpoint'):
            name = f'formal-{component}-build'
            self.assertIn(name, JOBS)
            job = JOBS[name]
            self.assertIn('artifact: ${{ steps.component-artifact.outputs.artifact }}', job)
            step = job.split('      - id: component-artifact\n', 1)[1].split('      - ', 1)[0]
            script = textwrap.dedent(step.split('        run: |\n', 1)[1])
            with tempfile.TemporaryDirectory() as directory:
                outputs = []
                for attempt in (1, 2):
                    output = Path(directory) / str(attempt)
                    result = subprocess.run(['bash', '-euo', 'pipefail', '-c', script],
                        env=dict(os.environ, GITHUB_RUN_ID='123', GITHUB_RUN_ATTEMPT=str(attempt),
                                 GITHUB_OUTPUT=str(output)), text=True, capture_output=True)
                    self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                    values = dict(line.split('=', 1) for line in output.read_text().splitlines())
                    self.assertEqual(values, {'artifact': f'formal-{component}-123-{attempt}'})
                    outputs.append(values['artifact'])
                self.assertNotEqual(*outputs)
            self.assertIn('name: ${{ steps.component-artifact.outputs.artifact }}', job)
            for merger in ('formal-test-build', 'formal-test-build-trusted'):
                body = JOBS[merger]
                self.assertIn(f'needs.formal-{component}-build.outputs.artifact', body)
                self.assertIn(f'needs.formal-{component}-build-trusted.outputs.artifact', body)
                self.assertNotIn(f'name: formal-{component}-${{{{ github.run_id }}}}', body)

    def test_formal_merge_stops_before_pack_when_component_validation_fails(self):
        job = JOBS['formal-test-build']
        self.assertIn('      - name: Merge and pack formal process dependencies\n', job)
        step = job.split('      - name: Merge and pack formal process dependencies\n', 1)[1].split('      - ', 1)[0]
        script = textwrap.dedent(step.split('        run: |\n', 1)[1])
        stub = '''python3() {
          printf '%s\\n' "$*" >> "$CALLS";
          if [[ "$2" == merge-formal ]]; then return "$MERGE_EXIT"; fi
          return 0;
        }
        '''
        with tempfile.TemporaryDirectory() as directory:
            calls = Path(directory) / 'calls'
            for code in (0, 17):
                calls.unlink(missing_ok=True)
                result = subprocess.run(['bash', '-euo', 'pipefail', '-c', stub + script],
                    env=dict(os.environ, RUNNER_TEMP=directory, SOURCE_SHA='a' * 40,
                             CALLS=str(calls), MERGE_EXIT=str(code)), text=True, capture_output=True)
                self.assertEqual(result.returncode, code, result.stdout + result.stderr)
                commands = calls.read_text().splitlines()
                self.assertEqual(len(commands), 1 if code else 2)
                self.assertIn(' merge-formal ', commands[0])
                self.assertIn('--debug-bundle ', commands[0])
                self.assertIn('--endpoint-bundle ', commands[0])
                for command in commands:
                    self.assertIn('--plan ', command)
                    self.assertIn('--formal-config ', command)
                if not code:
                    self.assertIn(' pack ', commands[1])
                    self.assertIn('--split-formal', commands[1])

    def test_formal_component_transport_preserves_modes_and_stops_after_build_failure(self):
        import tarfile
        with tempfile.TemporaryDirectory(prefix='oasis7-formal-workflow-') as directory:
            root = Path(directory)
            (root / 'scripts').mkdir()
            # Substitute only the producer CLI. The production Bash, tar and
            # unpack commands run unchanged against real executable fixtures.
            (root / 'scripts/ci-test-artifacts.py').write_text(textwrap.dedent('''
                import json, os, pathlib, sys
                arguments = sys.argv[1:]
                pathlib.Path(os.environ['CALLS']).write_text(json.dumps(arguments))
                if int(os.environ['BUILD_EXIT']):
                    raise SystemExit(int(os.environ['BUILD_EXIT']))
                bundle = pathlib.Path(arguments[arguments.index('--bundle') + 1])
                (bundle / 'bin').mkdir(parents=True)
                (bundle / 'lib').mkdir()
                (bundle / 'manifest.json').write_text('{}')
                executable = bundle / 'bin/fixture'
                executable.write_text('#!/bin/sh\\nexit 0\\n')
                executable.chmod(0o751)
                (bundle / 'lib/fixture.so').write_bytes(b'fixture-library')
            '''))
            calls = root / 'calls'
            for component in ('debug', 'endpoint'):
                job = JOBS[f'formal-{component}-build']
                step = job.split(f'      - name: Build and archive formal-{component} component\n', 1)[1].split('      - ', 1)[0]
                script = textwrap.dedent(step.split('        run: |\n', 1)[1])
                archive = root / f'formal-{component}-tests.tar.gz'
                for code in (23, 0):
                    result = subprocess.run(['bash', '-euo', 'pipefail', '-c', script], cwd=root,
                        env=dict(os.environ, RUNNER_TEMP=directory, SOURCE_SHA='a' * 40,
                                 CALLS=str(calls), BUILD_EXIT=str(code)), text=True, capture_output=True)
                    self.assertEqual(result.returncode, code, result.stdout + result.stderr)
                    arguments = json.loads(calls.read_text())
                    self.assertEqual(arguments[:3], ['build', '--scope', f'formal-{component}'])
                    for option, relative in (('--plan', 'selection.json'),
                            ('--formal-config', 'ci-authority/ci-formal-process-suites.json'),
                            ('--cache-state', f'formal-{component}-cache-state.json')):
                        self.assertEqual(arguments[arguments.index(option) + 1], str(root / relative))
                    self.assertEqual(archive.exists(), code == 0)
                with tarfile.open(archive) as payload:
                    self.assertEqual(payload.getmember('./bin/fixture').mode, 0o751)
                    self.assertEqual(payload.extractfile('./lib/fixture.so').read(), b'fixture-library')
                destination = root / 'formal-components' / component
                destination.mkdir(parents=True)
                archive.rename(destination / archive.name)
                (root / f'formal-{component}-tests').rename(root / f'original-{component}')
            step = JOBS['formal-test-build'].split('      - name: Unpack Formal components preserving executable modes\n', 1)[1].split('      - ', 1)[0]
            script = textwrap.dedent(step.split('        run: |\n', 1)[1])
            result = subprocess.run(['bash', '-euo', 'pipefail', '-c', script], cwd=root,
                env=dict(os.environ, RUNNER_TEMP=directory), text=True, capture_output=True)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            for component in ('debug', 'endpoint'):
                executable = root / f'formal-{component}-tests/bin/fixture'
                self.assertEqual(executable.stat().st_mode & 0o777, 0o751)
                self.assertEqual(subprocess.run([str(executable)]).returncode, 0)
            # A missing selected component must stop the actual unpack shell.
            (root / 'formal-components/endpoint/formal-endpoint-tests.tar.gz').unlink()
            result = subprocess.run(['bash', '-euo', 'pipefail', '-c', script + '\nprintf unreachable'],
                cwd=root, env=dict(os.environ, RUNNER_TEMP=directory), text=True, capture_output=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertNotIn('unreachable', result.stdout)

    def test_formal_merge_artifact_names_reject_missing_cross_run_and_cross_role_inputs(self):
        job = JOBS['formal-test-build']
        step = job.split('      - name: Require exact successful Formal component artifacts\n', 1)[1].split('      - ', 1)[0]
        script = textwrap.dedent(step.split('        run: |\n', 1)[1])
        cases = [('formal-debug-123-1', 'formal-endpoint-123-2', True),
                 ('formal-debug-123-2', 'formal-endpoint-123-1', True),
                 ('formal-debug-123-2', 'formal-endpoint-123-2', True),
                 ('', 'formal-endpoint-123-2', False),
                 ('formal-debug-123-2', '', False),
                 ('formal-debug-124-2', 'formal-endpoint-123-2', False),
                 ('formal-debug-123-2', 'formal-endpoint-124-2', False),
                 ('formal-endpoint-123-2', 'formal-debug-123-2', False),
                 ('formal-debug-123-0', 'formal-endpoint-123-2', False),
                 ('formal-debug-123-2', 'formal-endpoint-123-unknown', False)]
        for debug, endpoint, expected in cases:
            result = subprocess.run(['bash', '-euo', 'pipefail', '-c', script], text=True, capture_output=True,
                env=dict(os.environ, GITHUB_RUN_ID='123', GITHUB_RUN_ATTEMPT='2',
                         DEBUG_ARTIFACT=debug, ENDPOINT_ARTIFACT=endpoint))
            with self.subTest(debug=debug, endpoint=endpoint):
                self.assertEqual(result.returncode == 0, expected, result.stdout + result.stderr)

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

    def test_formal_dependency_caches_match_the_real_component_profile_environment(self):
        import runpy
        build_environment = runpy.run_path(str(ROOT / 'scripts/ci-test-artifacts.py'))['build_environment']
        expected = {f'CARGO_PROFILE_{profile}_{field}': value for profile in ('DEV', 'TEST')
                    for field, value in (('OPT_LEVEL', '0'), ('DEBUG_ASSERTIONS', 'true'),
                                         ('OVERFLOW_CHECKS', 'true'))}
        for component in ('debug', 'endpoint'):
            bodies = []
            for suffix in ('', '-trusted'):
                name = f'formal-{component}-build{suffix}'
                job = JOBS[name]
                environment = re.search(r'^    env:\n((?:      .*\n)+)', job, re.M)
                self.assertIsNotNone(environment, name)
                values = {key: json.loads(value) for key, value in re.findall(
                    r'^      ([A-Z_]+): (.*)$', environment[1], re.M)}
                self.assertEqual(values, expected)
                actual = build_environment(f'formal-{component}')
                self.assertEqual({key: actual[key] for key in expected}, values)
                caches = [step for step in re.findall(r'^      - .*?(?=^      - |\Z)', job, re.M | re.S)
                          if 'uses: Swatinem/rust-cache@' in step]
                self.assertEqual(len(caches), 2, name)
                for cache in caches:
                    self.assertIn(f'shared-key: ci-formal-{component}-deps-trusted-v1', cache)
                    self.assertIn('add-rust-environment-hash-key: true', cache)
                    self.assertIn('env-vars: CARGO CC CFLAGS CXX CMAKE RUST OASIS7_WASM OASIS7_FORMAL_BUILD_INPUTS', cache)
                    self.assertIn("OASIS7_FORMAL_BUILD_INPUTS: ${{ hashFiles('scripts/ci-formal-process-suites.json') }}", cache)
                    self.assertNotIn('scripts/ci-test-artifacts.py', cache)
                    self.assertIn('cache-on-failure: false', cache)
                self.assertNotIn('OASIS7_FORMAL_BUILD_INPUTS', job.split('    steps:', 1)[0])
                self.assertNotIn("hashFiles('.github/workflows/rust.yml')", job)
                self.assertIn('save-if: false', caches[0])
                self.assertNotIn('GH_TOKEN:', caches[0])
                self.assertNotIn('OASIS7_DEPENDENCY_CACHE_FAMILY:', caches[0])
                self.assertIn('PATH: ${{ steps.dependency-cache-tools.outputs.path }}', caches[1])
                self.assertIn(f'OASIS7_DEPENDENCY_CACHE_FAMILY: formal-{component}', caches[1])
                self.assertIn('GH_TOKEN: ${{ github.token }}', caches[1])
                self.assertIn('OASIS7_DEPENDENCY_CACHE_BUDGET_BYTES: ${{ env.CI_CACHE_REPOSITORY_BUDGET_BYTES }}', caches[1])
                self.assertNotRegex(caches[1].split('          save-if:', 1)[1],
                                    r'(?i)\b(?:always|cancelled|failure|success)\s*\(')
                for step in re.findall(r'^      - .*?(?=^      - |\Z)', job, re.M | re.S):
                    if step != caches[1]:
                        self.assertNotIn('steps.dependency-cache-tools.outputs.path', step)
                        self.assertNotIn('OASIS7_DEPENDENCY_CACHE_', step)
                bodies.append(caches)
            self.assertEqual(bodies[0], bodies[1])

    def test_formal_dependency_upload_only_admits_successful_trusted_full_components(self):
        def evaluate(condition, values):
            expression = condition.strip().removeprefix('>-').strip()
            if expression.startswith('${{'):
                expression = expression[3:-2].strip()
            for token, value in sorted(values.items(), key=lambda item: -len(item[0])):
                expression = expression.replace(token, repr(value))
            expression = re.sub(r'!(?!=)', 'not ', expression.replace('&&', ' and ').replace('||', ' or '))
            return bool(eval(' '.join(expression.split()), {'__builtins__': {}}, {}))

        for component in ('debug', 'endpoint'):
            for suffix in ('', '-trusted'):
                name = f'formal-{component}-build{suffix}'
                job = JOBS[name]
                header = job.split('    if:', 1)[1].split('    runs-on:', 1)[0]
                caches = [step for step in re.findall(r'^      - .*?(?=^      - |\Z)', job, re.M | re.S)
                          if 'uses: Swatinem/rust-cache@' in step]
                self.assertEqual(len(caches), 2, name)
                reader = caches[0].split('        if:', 1)[1].split('        env:', 1)[0]
                writer = caches[1].split('        if:', 1)[1].split('        env:', 1)[0]
                save = caches[1].split('          save-if:', 1)[1].split('          cache-on-failure:', 1)[0]
                for event, ref, source, matching, profile, mode in (
                        ('pull_request', 'refs/pull/1/merge', False, True, 'pr-fast', ''),
                        ('push', 'refs/heads/main', True, True, 'main-fast', ''),
                        ('push', 'refs/heads/main', True, True, '', ''),
                        ('schedule', 'refs/heads/main', True, True, 'periodic-full', ''),
                        ('workflow_dispatch', 'refs/heads/main', True, True, 'candidate-full', 'full'),
                        ('workflow_dispatch', 'refs/heads/main', False, True, 'candidate-full', 'full'),
                        ('workflow_dispatch', 'refs/heads/topic', True, True, 'candidate-full', 'full'),
                        ('workflow_dispatch', 'refs/heads/main', True, False, 'candidate-full', 'full'),
                        ('workflow_dispatch', 'refs/heads/main', True, True, 'candidate-full', 'newapi_bridge_package')):
                    for installation, result in itertools.product(('success', 'failure', 'skipped'),
                                                                  ('success', 'failure', 'cancelled')):
                        values = {'always()': True, 'cancelled()': False, 'needs.select.result': 'success',
                            'needs.select.outputs.run_formal_process': 'true',
                            'needs.select.outputs.trusted_source': str(source).lower(),
                            'fromJSON(needs.select.outputs.plan).execution_profile': profile,
                            'fromJSON(needs.select.outputs.plan).fast_plan_version': 1 if profile else 0,
                            'fromJSON(needs.select.outputs.plan).ordinary_main_fast_cache_writer_version': 0,
                            'github.job': name, 'github.event_name': event, 'github.ref': ref,
                            'github.workflow_sha': 'a' if matching else 'b', 'github.sha': 'a',
                            'inputs.run_mode': mode, 'steps.dependency-cache-tools.outcome': installation,
                            'job.status': result}
                        active = evaluate(header, values)
                        reads, writes = evaluate(reader, values), evaluate(writer, values)
                        eligible = (suffix == '-trusted' and ref == 'refs/heads/main' and matching and source
                                    and (event == 'schedule' or event == 'workflow_dispatch' and mode == 'full'))
                        with self.subTest(job=name, event=event, ref=ref, source=source, matching=matching,
                                          installation=installation, result=result, profile=profile):
                            self.assertFalse(reads and writes)
                            self.assertEqual(reads, not eligible)
                            self.assertEqual(writes, eligible and installation == 'success')
                            self.assertEqual(active and writes and evaluate(save, values),
                                             active and eligible and installation == 'success' and result == 'success')

    def test_formal_suites_share_build_and_propagate_all_failures(self):
        matrix = JOBS['formal-process-tests']
        self.assertIn('needs: [select, formal-test-build, formal-test-build-trusted]', matrix)
        self.assertNotIn('needs: oasis7-required', matrix)
        self.assertIn('fail-fast: false', matrix)
        self.assertIn('max-parallel: 5', matrix)
        self.assertIn('shard: ${{ fromJSON(needs.select.outputs.formal_shards) }}', matrix)
        self.assertIn('actions/download-artifact@', matrix)
        self.assertNotIn('cargo build', matrix)
        self.assertNotIn('cargo test', matrix)
        self.assertNotIn('rust-cache@', matrix)
        self.assertIn('--results output/formal-process/results.jsonl', matrix)
        self.assertIn('needs: [select, formal-test-build, formal-test-build-trusted, formal-process-tests]', JOBS['formal-process'])
        for name in ['full-regression', 'candidate-full-regression']:
            self.assertIn('./scripts/ci-tests.sh full-extended', JOBS[name])
            self.assertIn('needs: select', JOBS[name])
            self.assertNotIn('oasis7-test-build', JOBS[name])
        self.assertNotIn('oasis7-test-build', JOBS)
        self.assertIn('compression-level: 0', JOBS['oasis7-test-build-candidate'])
        self.assertIn('overwrite: true', JOBS['oasis7-test-build-candidate'])
        self.assertIn('--scope ordinary', JOBS['oasis7-test-build-candidate'])
        self.assertNotIn('run_formal_process', JOBS['oasis7-test-build-candidate'])
        self.assertNotIn('run_formal_process', JOBS['oasis7-test-build-trusted'])
        self.assertIn('merge-formal --debug-bundle', JOBS['formal-test-build'])
        self.assertIn('--split-formal', JOBS['formal-test-build'])
        self.assertIn('formal-tests-${{ matrix.shard }}-${{ github.sha }}', matrix)
        self.assertIn('--shard "$SHARD"', matrix)
        driver = (ROOT / 'scripts/ci-tests.sh').read_text()
        required_body = driver.split('run_oasis7_required_tier_tests() {', 1)[1].split('run_scenario_regression_tests()', 1)[0]
        self.assertNotIn('formal-live-local-tests.sh', required_body)
        self.assertIn('if [[ "$tier" == full ]]; then run_group formal_process; fi', driver)

    def test_workspace_caches_have_exclusive_trust_and_shared_admission(self):
        for scope, reader_name, writer_name in (
                ('ordinary', 'oasis7-test-build-candidate', 'oasis7-test-build-trusted'),
                ('formal-debug', 'formal-debug-build', 'formal-debug-build-trusted'),
                ('formal-endpoint', 'formal-endpoint-build', 'formal-endpoint-build-trusted')):
            reader, writer = JOBS[reader_name], JOBS[writer_name]
            self.assertIn('cache-mode: read', reader)
            self.assertNotIn('cache-mode: write', reader)
            self.assertIn('cache-mode: write', writer)
            for text in ('refs/heads/main', "github.workflow_sha == github.sha", "github.event_name == 'push'",
                         "github.event_name == 'schedule'", "github.event_name == 'workflow_dispatch'",
                         "inputs.run_mode == 'full'"):
                self.assertIn(text, writer.split('    steps:', 1)[0])
                self.assertIn(text, reader.split('    steps:', 1)[0])
            self.assertIn('!(', reader.split('    steps:', 1)[0])
            self.assertEqual(comparable_source_steps(reader_name).strip(), comparable_source_steps(writer_name).strip())
            for job in (reader, writer):
                self.assertIn('actions: read', job)
                self.assertNotIn('actions: write', job)
                for mode in ('identity', 'preflight', 'measure', 'restore', 'stage', 'observe-save'):
                    self.assertIn(f'ci-workspace-cache.py {mode} --scope {scope}', job)
                self.assertIn('actions/cache/restore@55cc8345863c7cc4c66a329aec7e433d2d1c52a9', job)
                self.assertIn('actions/cache/save@55cc8345863c7cc4c66a329aec7e433d2d1c52a9', job)
                self.assertIn(f"steps.{scope}-cache-stage.outputs.save == 'true'", job)
                self.assertIn(f'path: ${{{{ runner.temp }}}}/{scope}-workspace-cache/workspace-units.tar.zst', job)
                self.assertIn(f'--cache-state "$RUNNER_TEMP/{scope}-cache-state.json"', job)
                self.assertLess(job.index('Swatinem/rust-cache@'), job.index(f'ci-workspace-cache.py restore --scope {scope}'))
                staging = job.split(f'- name: Stage bounded {scope} workspace units', 1)[1].split('- name:', 1)[0]
                self.assertIn(f"github.job == '{writer_name}'", staging)
                self.assertIn(f"steps.{scope}-workspace-cache.outputs.cache-hit != 'true'", staging)
                observation = job.split(f'- name: Confirm {scope} workspace cache', 1)[1].split('- name:', 1)[0]
                self.assertIn(f"steps.{scope}-cache-save.outcome == 'success'", observation)
                self.assertIn(f"steps.{scope}-workspace-cache.outputs.cache-hit == 'true'", observation)
                self.assertIn('CACHE_INVENTORY_TOKEN: ${{ github.token }}', observation)
            self.assertIn(f'cache_key: ${{{{ steps.{scope}-cache-identity.outputs.key }}}}', writer)
            self.assertIn(f'cache_confirmed: ${{{{ steps.{scope}-cache-observe.outputs.saved_confirmed }}}}', writer)
        matrix = JOBS['formal-process-tests']
        self.assertIn('always()', matrix)
        for name in ('formal-test-build', 'formal-test-build-trusted'):
            self.assertIn(f"needs.{name}.result == 'success'", matrix)
            self.assertIn(f"needs.{name}.result == 'skipped'", matrix)

    def test_formal_consumers_stop_when_cancelled_and_require_one_successful_builder(self):
        header = JOBS['formal-process-tests'].split('    timeout-minutes:', 1)[0]
        condition = header.split('    if:', 1)[1].strip().removeprefix('>-').strip()
        # One producer is deliberately skipped; retain the explicit status check.
        self.assertIn('always()', condition)
        states = ('success', 'skipped', 'failure', 'cancelled', '', 'unknown')
        for candidate in states:
            for trusted in states:
                for select in states:
                    for selected in (False, True):
                        for cancelled in (False, True):
                            values = {
                                'always()': True, 'cancelled()': cancelled,
                                'needs.select.result': select,
                                'needs.select.outputs.run_formal_process': str(selected).lower(),
                                'needs.formal-test-build.result': candidate,
                                'needs.formal-test-build-trusted.result': trusted,
                            }
                            expression = condition
                            for token, value in values.items():
                                expression = expression.replace(token, repr(value))
                            expression = re.sub(r'!(?!=)', 'not ', expression.replace('&&', ' and ').replace('||', ' or '))
                            expected = (not cancelled and select == 'success' and selected
                                        and (candidate, trusted) in (('success', 'skipped'), ('skipped', 'success')))
                            with self.subTest(candidate=candidate, trusted=trusted, select=select,
                                              selected=selected, cancelled=cancelled):
                                actual = eval(' '.join(expression.split()), {'__builtins__': {}}, {})
                                self.assertEqual(actual, expected)

    def test_ordinary_build_aggregation_rejects_every_incomplete_lane_combination(self):
        consumer = JOBS['oasis7-required']
        self.assertTrue(
            'needs: [select, oasis7-test-build-candidate, oasis7-test-build-trusted]' in consumer
            or 'needs: [select, fast-preflight, oasis7-test-build-candidate, oasis7-test-build-trusted]' in consumer
        )
        condition = consumer.split('    runs-on:', 1)[0].split('    if:', 1)[1].strip().removeprefix('>-').strip()
        self.assertIn('always() && !cancelled()', condition)
        self.assertIn("needs.select.result == 'success'", condition)
        states = ('success', 'skipped', 'failure', 'cancelled', '', 'unknown')
        for candidate in states:
            for trusted in states:
                with self.subTest(candidate=candidate, trusted=trusted):
                    values = {
                        'always()': True, 'cancelled()': False,
                        'needs.select.result': 'success',
                        'needs.oasis7-test-build-candidate.result': candidate,
                        'needs.oasis7-test-build-trusted.result': trusted,
                        'needs.select.outputs.run_oasis7_required': 'true',
                        'fromJSON(needs.select.outputs.plan).scope': 'targeted',
                        'fromJSON(needs.select.outputs.plan).fast_execution_version': 1,
                        'fromJSON(needs.select.outputs.plan).ordinary_execution': 'artifact',
                    }
                    actual = effective_job_condition(condition, values, ['success', candidate, trusted])
                    expected = (candidate, trusted) in (('success', 'skipped'), ('skipped', 'success'))
                    self.assertEqual(actual, expected)

    def test_consumers_require_their_actual_dependency_and_handle_cancellation(self):
        for name in ('oasis7-required', 'full-regression', 'candidate-full-regression'):
            header = JOBS[name].split('    runs-on:', 1)[0]
            condition = header.split('    if:', 1)[1].strip()
            condition = condition.removeprefix('>-').strip()
            self.assertIn('always() && !cancelled()', condition)
            self.assertIn("needs.select.result == 'success'", condition)
            if name == 'oasis7-required':
                self.assertIn("needs.oasis7-test-build-candidate.result == 'success'", condition)
                self.assertIn("needs.oasis7-test-build-trusted.result == 'success'", condition)
            for result in ('success', 'failure', 'skipped', 'cancelled', '', 'unknown'):
                for cancelled in (False, True):
                    for event, ref, mode, scope, expected in (
                            ('pull_request', 'refs/pull/1/merge', '', 'full', 'oasis7-required'),
                            ('push', 'refs/heads/main', '', 'full', 'oasis7-required'),
                            ('workflow_dispatch', 'refs/heads/main', 'full', 'explicit-full', 'full-regression'),
                            ('schedule', 'refs/heads/main', '', 'explicit-full', 'full-regression'),
                            ('workflow_dispatch', 'refs/heads/candidate', 'full', 'explicit-full', 'candidate-full-regression'),
                            ('workflow_dispatch', 'refs/heads/main', 'newapi_bridge_package', 'explicit-full', None)):
                        values = {
                            'always()': True, 'cancelled()': cancelled,
                            'needs.fast-preflight.result': 'success',
                            'needs.oasis7-test-build-candidate.result': result,
                            'needs.oasis7-test-build-trusted.result': 'skipped',
                            'needs.select.result': 'success' if name == 'oasis7-required' else result,
                            'needs.select.outputs.run_oasis7_required': 'false',
                            'fromJSON(needs.select.outputs.plan).scope': scope,
                            'needs.select.outputs.trusted_source': str(ref == 'refs/heads/main').lower(),
                            'fromJSON(needs.select.outputs.plan).execution_profile': '',
                            'fromJSON(needs.select.outputs.plan).fast_plan_version': 0,
                            'fromJSON(needs.select.outputs.plan).ordinary_main_fast_cache_writer_version': 0,
                            'fromJSON(needs.select.outputs.plan).fast_execution_version': 0,
                            'fromJSON(needs.select.outputs.plan).ordinary_execution': '',
                            'github.event_name': event, 'github.ref': ref, 'inputs.run_mode': mode,
                        }
                        expression = condition
                        for token, value in values.items():
                            expression = expression.replace(token, repr(value))
                        expression = re.sub(r'!(?!=)', 'not ', expression.replace('&&', ' and ').replace('||', ' or '))
                        expression = ' '.join(expression.split())
                        with self.subTest(job=name, result=result, cancelled=cancelled, event=event, ref=ref, mode=mode):
                            # Evaluate only this checked-in boolean condition with fixture literals.
                            actual = eval(expression, {'__builtins__': {}}, {})
                            self.assertEqual(actual, result == 'success' and not cancelled and name == expected)

    def test_inline_execution_skips_only_compatible_producers_and_rejects_bad_dependency_states(self):
        for version, mode in ((0, ''), (2, 'inline'), (1, 'artifact'), (1, 'inline')):
            inline = version == 1 and mode == 'inline'
            for candidate, trusted in (('skipped', 'skipped'), ('success', 'skipped'),
                                       ('skipped', 'success'), ('success', 'success'),
                                       ('failure', 'skipped'), ('cancelled', 'skipped')):
                for cancelled in (False, True):
                    values = {
                        'always()': True, 'cancelled()': cancelled, 'needs.select.result': 'success',
                        'needs.fast-preflight.result': 'success',
                        'needs.oasis7-test-build-candidate.result': candidate,
                        'needs.oasis7-test-build-trusted.result': trusted,
                        'needs.select.outputs.run_oasis7_required': 'true',
                        'needs.select.outputs.trusted_source': 'false',
                        'fromJSON(needs.select.outputs.plan).scope': 'targeted',
                        'fromJSON(needs.select.outputs.plan).execution_profile': 'pr-fast',
                        'fromJSON(needs.select.outputs.plan).fast_plan_version': 1,
                        'fromJSON(needs.select.outputs.plan).ordinary_main_fast_cache_writer_version': 0,
                        'fromJSON(needs.select.outputs.plan).fast_execution_version': version,
                        'fromJSON(needs.select.outputs.plan).ordinary_execution': mode,
                        'github.event_name': 'pull_request', 'github.ref': 'refs/pull/1/merge',
                        'github.workflow_sha': 'a', 'github.sha': 'b', 'inputs.run_mode': '',
                    }
                    for name in ('oasis7-test-build-candidate', 'oasis7-test-build-trusted', 'oasis7-required'):
                        condition = JOBS[name].split('    if:', 1)[1].split('    runs-on:', 1)[0].strip().removeprefix('>-')
                        for token, value in values.items():
                            condition = condition.replace(token, repr(value))
                        condition = re.sub(r'!(?!=)', 'not ', condition.replace('&&', ' and ').replace('||', ' or '))
                        actual = eval(' '.join(condition.split()), {'__builtins__': {}}, {})
                        if name == 'oasis7-required':
                            valid_producers = ((candidate, trusted) == ('skipped', 'skipped') if inline
                                               else (candidate, trusted) in (('success', 'skipped'), ('skipped', 'success')))
                            expected = not cancelled and valid_producers
                        elif name == 'oasis7-test-build-trusted':
                            expected = False
                        else:
                            expected = not inline and not cancelled
                        with self.subTest(job=name, version=version, mode=mode,
                                          candidate=candidate, trusted=trusted, cancelled=cancelled):
                            self.assertEqual(actual, expected)

    def test_preview_is_cold_read_only_and_independent_of_required_authority(self):
        planner, preview = JOBS['pr-fast-preview-plan'], JOBS['pr-fast-preview']
        self.assertIn(
            "if: github.event_name == 'pull_request' && github.event.action != 'labeled' && "
            "!github.event.pull_request.draft && "
            "contains(github.event.pull_request.labels.*.name, 'ci-fast-preview')",
            planner,
        )
        self.assertNotIn('needs: select', planner)
        self.assertIn('ci-fast-preview.py plan-matrix --base-ref "$BASE_SHA" --head-ref "$TESTED_SHA"', planner)
        self.assertIn('SOURCE_SHA: ${{ github.event.pull_request.head.sha }}', planner)
        self.assertIn('--source-ref "$SOURCE_SHA"', planner)
        self.assertIn('artifact: pr-fast-preview-plan-${{ github.run_id }}-${{ github.run_attempt }}', planner)
        self.assertIn('name: ${{ needs.pr-fast-preview-plan.outputs.artifact }}', preview)
        self.assertIn("needs.pr-fast-preview-plan.result == 'success'", preview)
        self.assertIn("needs.pr-fast-preview-plan.outputs.enabled == 'true'", preview)
        self.assertIn("github.event.action != 'opened'", preview)
        self.assertIn("contains(github.event.pull_request.labels.*.name, 'ci-fast-preview')", preview)
        label_workflow = (ROOT / '.github/workflows/pr-fast-preview.yml').read_text()
        label_jobs = workflow_jobs(label_workflow)
        self.assertIn('types: [labeled]', label_workflow)
        self.assertIn(
            "github.event_name == 'pull_request' && !github.event.pull_request.draft && "
            "github.event.label.name == 'ci-fast-preview'",
            label_jobs['pr-fast-preview-plan'],
        )
        self.assertIn("needs.pr-fast-preview-plan.result == 'success' && needs.pr-fast-preview-plan.outputs.enabled == 'true'",
                      label_jobs['pr-fast-preview'])
        for name, job in label_jobs.items():
            with self.subTest(label_workflow_job=name):
                self.assertIn('contents: read', job)
                self.assertIn('persist-credentials: false', job)
                for forbidden in ('contents: write', 'actions: write', 'cache-mode: write', 'continue-on-error:',
                                  'rust-cache@', 'actions/cache/', 'ci-full-result.py', 'ci-required-result.py'):
                    self.assertNotIn(forbidden, job)
        self.assertIn('matrix: ${{ steps.preview.outputs.matrix }}', planner)
        self.assertIn('matrix: ${{ fromJSON(needs.pr-fast-preview-plan.outputs.matrix) }}', preview)
        self.assertIn('PREVIEW_SAMPLE: ${{ matrix.sample }}', preview)
        self.assertIn('PREVIEW_PARTITION: ${{ matrix.partition }}', preview)
        self.assertEqual(len(re.findall(r'^    name:', preview, re.M)), 1)
        self.assertIn('name: pr-fast-preview (${{ matrix.sample }}, ${{ matrix.group }}, ${{ matrix.partition }})', preview)
        self.assertIn('--group "$PREVIEW_GROUP" --partition "$PREVIEW_PARTITION"', preview)
        self.assertIn('OASIS7_PRODUCT_DOC_BASE: ${{ matrix.synthetic_base_sha }}', preview)
        self.assertIn('name: pr-fast-preview-${{ matrix.sample }}-${{ matrix.group }}-${{ matrix.partition }}-', preview)
        self.assertIn("'partition': os.environ['PREVIEW_PARTITION']", preview)
        self.assertIn('fail-fast: false', preview)
        self.assertIn('ref: ${{ needs.pr-fast-preview-plan.outputs.tested_sha }}', preview)
        self.assertIn('ci-fast-preview.py restore --sample "$PREVIEW_SAMPLE"', preview)
        self.assertIn('--metadata "$RUNNER_TEMP/previews/$PREVIEW_SAMPLE/selection.json.metadata.json"', preview)
        self.assertIn('--plan "$RUNNER_TEMP/previews/$PREVIEW_SAMPLE/selection.json"', preview)
        labeled_preview = label_jobs['pr-fast-preview']
        self.assertEqual(preview.split('    runs-on:', 1)[1].strip(),
                         labeled_preview.split('    runs-on:', 1)[1].strip())
        compiler_name = ('Verify the preinstalled WASM C compiler'
                         if '    container:' in preview else 'Install WASM C compiler')
        compiler = next(step for step in re.findall(r'^      - .*?(?=^      - |\Z)', preview, re.M | re.S)
                        if f'name: {compiler_name}' in step)
        self.assertIn("if: matrix.group == 'net'", compiler)
        for command in ('clang --print-targets | grep -w wasm32', 'llvm-ar --version'):
            self.assertIn(command, compiler)
            self.assertIn(command, JOBS['net'])
        if '    container:' in preview:
            for text in ("matrix.group == 'baseline' && 'ghcr.io/nil-studio/oasis7/ci-runtime@sha256:3984caa42d82963f93af752bf79e3a07838f2c94279cfbb2deeb08f715cd62c9'",
                         "matrix.group == 'net' && 'ghcr.io/nil-studio/oasis7/ci-rust-wasm-build@sha256:57c5784d5c038fbd43037bdf2b6c1bdaed3a5e4433354b21a5eee39797da3aa6'",
                         "|| 'ghcr.io/nil-studio/oasis7/ci-rust-build@sha256:f121df903f97527a7097134d39f25822c46a3775152768a9cbd591fe65ee987c'",
                         'cache-mode: none', 'packages: read', '--user 1001:1001 --init',
                         "if: matrix.group == 'rust_baseline'", 'tool: cargo-deny@0.19.9',
                         "'includes_container_initialization': False", "python3 -m venv"):
                self.assertIn(text, preview)
            for install in ('ci-apt-install.py', 'rustup toolchain install', 'actions/setup-python@'):
                self.assertNotIn(install, preview)
        else:
            self.assertIn('--hosted-dns-recovery -- clang llvm', compiler)
        self.assertIn('scripts/ci-tests.sh required --group "$PREVIEW_GROUP" "${plan_args[@]}"', preview)
        self.assertIn('--requirement scripts/doc-governance-requirements.txt', preview)
        self.assertIn('OASIS7_CI_PROFILE: pr-fast', preview)
        self.assertIn('set -euo pipefail', preview)
        self.assertIn('diagnostic_only', preview)
        for name, job in (('plan', planner), ('preview', preview)):
            with self.subTest(job=name):
                self.assertIn('contents: read', job)
                self.assertIn('persist-credentials: false', job)
                for forbidden in ('contents: write', 'actions: write', 'cache-mode: write', 'continue-on-error:',
                                  'rust-cache@', 'actions/cache/', 'ci-full-result.py', 'ci-required-result.py'):
                    self.assertNotIn(forbidden, job)
        for gate in ('required-gate', 'explicit-full-gate'):
            self.assertNotIn('pr-fast-preview', JOBS[gate])
        baseline = JOBS['rust-baseline']
        self.assertIn('name: ${{ needs.select.outputs.selection_artifact }}', baseline)
        self.assertIn('--plan "$RUNNER_TEMP/selection.json"', baseline)
        self.assertIn('SOURCE_SHA: ${{ needs.select.outputs.source_sha }}', baseline)

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
        for name in ('cache-bootstrap-maintenance', 'cache-maintenance'):
            with self.subTest(job=name):
                job = JOBS[name]
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
                                    ('workflow_dispatch', 'refs/heads/main', 'newapi_bridge_package', True, scope == 'ecs'),
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
                                ('workflow_dispatch', 'refs/heads/main', 'newapi_bridge_package', True, False),
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

    def test_manual_candidate_full_is_read_only_and_not_scheduled(self):
        candidate = JOBS['candidate-full-regression']
        self.assertIn("needs.select.outputs.trusted_source != 'true' && github.event_name == 'workflow_dispatch' && inputs.run_mode == 'full'", candidate)
        self.assertIn('cache-mode: read', candidate)
        self.assertNotIn('cache-mode: write', candidate)
        self.assertNotIn('trusted cache sentinel', candidate)
        self.assertNotIn('actions/cache/save@', candidate)
        self.assertIn('save-if: false', candidate)
        self.assertIn('actions/cache/restore@', candidate)
        self.assertIn('git rev-parse --verify', candidate)
        self.assertIn('SOURCE_SHA', candidate)
        self.assertIn('GITHUB_STEP_SUMMARY', candidate)
        self.assertIn('CI_VERBOSE=1 ./scripts/ci-tests.sh full', candidate)

    def test_candidate_full_preserves_full_setup_and_execution(self):
        # Both lanes run the same full validation. Only the trusted main lane
        # additionally saves its existing tool caches and sentinel.
        writer = selected_source_steps('full-regression')
        candidate = selected_source_steps('candidate-full-regression')
        for job in (writer, candidate):
            self.assertNotIn('cache-native', job)
            self.assertNotIn('dependency-cache-tools', job)
        caches = []
        for job in (writer, candidate):
            matches = [step for step in re.findall(r'^      - .*?(?=^      - |\Z)', job, re.M | re.S)
                       if 'uses: Swatinem/rust-cache@' in step]
            self.assertEqual(len(matches), 1)
            caches.append(matches[0])
        # Action input order is immaterial; compare every setting before
        # comparing the remaining compiler setup, tests and diagnostics.
        self.assertEqual(sorted(caches[0].splitlines()), sorted(caches[1].splitlines()))
        writer = writer.replace(caches[0], '', 1)
        candidate = candidate.replace(caches[1], '', 1)
        candidate = re.sub(r'      - name: Record exact candidate identity\n.*?(?=      - name: Resolve)', '', candidate, flags=re.S)
        writer = re.sub(r'      - name: Prepare trusted cache sentinel\n.*?(?=      - (?:name: Run full|uses: actions/download-artifact))', '', writer, flags=re.S)
        writer = re.sub(r'      - name: Save prepared wasm-bindgen tool cache\n.*?(?=      - name:)', '', writer, flags=re.S)
        candidate = candidate.replace('actions/cache/restore@55cc8345863c7cc4c66a329aec7e433d2d1c52a9 # v6.1.0',
                                      'actions/cache@55cc8345863c7cc4c66a329aec7e433d2d1c52a9 # v6.1.0')
        self.assertEqual(candidate.strip(), writer.strip())

    def test_pinned_trunk_helper_failure_and_success_paths(self):
        # The protected baseline already executes this suite during migration.
        subprocess.run(['bash',str(ROOT/'scripts/install-ci-trunk.test.sh')],cwd=ROOT,check=True)

    def test_viewer_security_smoke_uses_installed_browser_after_formal_build(self):
        job = JOBS['viewer-js-required']
        self.assertIn('uses: ./.github/actions/setup-ci-browser', job)
        self.assertIn('browser: agent', job)
        self.assertIn("node-version: '24'", job)
        self.assertNotIn('cache-mode: write', job)
        group = re.search(r'^    viewer_js_required\) (.*?) ;;$',
                          (ROOT / 'scripts/ci-tests.sh').read_text(), re.M).group(1)
        self.assertLess(group.index('run_oasis7_viewer_software_safe_build'),
                        group.index('viewer-auth-browser-security-smoke.mjs'))

    def test_signed_read_codegen_runs_in_support_and_full(self):
        job = JOBS['workspace-support-legacy']
        self.assertIn('actions/setup-node@949feb2413d6458794dcd2491c4babbbce0c15c1', job)
        self.assertIn("node-version: '22'", job)
        driver = (ROOT / 'scripts/ci-tests.sh').read_text()
        function = driver.split('run_signed_read_protocol_contract_tests() {', 1)[1].split('\n}', 1)[0]
        self.assertIn('protocol_codegen/generate.py check', function)
        self.assertNotIn('generate.py write', function)
        self.assertIn('--ignore-scripts', function)
        self.assertIn('npm test --prefix crates/oasis7_agent_api/protocol_conformance', function)
        self.assertIn('PYTHON="$ci_python"', function)
        support = driver.split('run_oasis7_workspace_support_crate_tests() {', 1)[1].split('\n}', 1)[0]
        self.assertIn('run_signed_read_protocol_contract_tests', support)
        self.assertIn('oasis7_agent_api) run_signed_read_protocol_contract_tests;', driver)
        for full in ('run_full_support_tier_tests', 'run_full_required_superset'):
            self.assertIn('run_oasis7_workspace_support_crate_tests',
                          driver.split(full + '() {', 1)[1].split('\n}', 1)[0])

    def test_additive_security_steps_preserve_candidate_groups(self):
        for name in ('net', 'viewer-js-required', 'viewer-performance-report', 'workflow-governance'):
            self.assertEqual(JOBS[name].count('name: Execute selected group'), 1)
            self.assertIn(f'bash ./scripts/ci-tests.sh required --group "{name.replace(chr(45), chr(95))}"', JOBS[name])
        performance = JOBS['viewer-performance-report']
        self.assertIn('uses: ./.github/actions/setup-ci-browser', performance)
        self.assertIn('browser: agent', performance)
        self.assertIn("node-version: '24'", performance)
        self.assertLess(performance.index('Install pinned performance browser'), performance.index('Execute selected group'))
        driver = (ROOT / 'scripts/ci-tests.sh').read_text()
        for job, forbidden in [('net', 'Verify pinned network source'),
                               ('viewer-js-required', 'Verify browser authentication security'),
                               ('viewer-performance-report', 'Build performance test artifact')]:
            self.assertNotIn(forbidden, JOBS[job])
        net = JOBS['net']
        compiler_step = ('Verify the preinstalled WASM C compiler' if rust_image_cache_namespace('net', net)
                         else 'Install WASM C compiler')
        self.assertLess(net.index(compiler_step), net.index('Execute selected group'))
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
            if 'rustup toolchain install' in job and name != 'newapi-bridge-package':
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

    def test_pr_auxiliary_workflows_scope_triggers_and_skip_drafts(self):
        ci_base = (ROOT / '.github/workflows/ci-base-images.yml').read_text()
        ci_base_pr = ci_base.split('  pull_request:\n', 1)[1].split('  workflow_dispatch:\n', 1)[0]
        self.assertNotIn('.github/workflows/rust.yml', ci_base_pr)
        self.assertIn(
            "if: (github.event_name == 'pull_request' && !github.event.pull_request.draft) || github.ref == 'refs/heads/main'",
            ci_base,
        )
        ci_base_jobs = workflow_jobs(ci_base)
        self.assertEqual(set(ci_base_jobs), {'build', 'publish'})
        targets = ['runtime', 'build', 'browser', 'rust-build', 'rust-wasm-build']
        dockerfile = (ROOT / 'docker/ci-base/Dockerfile').read_text()
        if re.search(r'^FROM build AS codeql-rust$', dockerfile, re.M):
            targets.append('codeql-rust')
        if re.search(r'^FROM runtime AS api-runtime$', dockerfile, re.M):
            targets.append('api-runtime')
        if re.search(r'^FROM rust-build AS rust-ordinary-builder$', dockerfile, re.M):
            targets.append('rust-ordinary-builder')
            self.assertRegex(dockerfile, r'(?m)^FROM docker:[0-9]+[.][0-9]+[.][0-9]+-cli@sha256:[0-9a-f]{64} AS docker-cli-tools$')
        if re.search(r'^FROM rust-build AS rust-custody-wasm$', dockerfile, re.M):
            targets.append('rust-custody-wasm')
            self.assertIn('ARG CUSTODY_NODE_BASE=node:22-bookworm-slim', dockerfile)
            self.assertIn('install -d -m 0700 -o 1001 -g 1001 /.cache/oasis7', dockerfile)
        if re.search(r'^FROM rust-build AS rust-viewer$', dockerfile, re.M):
            targets.append('rust-viewer')
            self.assertIn('activate --prebuilt --browser agent', (ROOT / 'docker/ci-base/build.sh').read_text())
            self.assertIn('COPY --from=viewer-browser . /opt/oasis7-viewer-browser/', dockerfile)
        if re.search(r'^FROM rust-custody-wasm AS rust-full-regression$', dockerfile, re.M):
            targets.append('rust-full-regression')
            full_leaf = dockerfile.split('FROM rust-custody-wasm AS rust-full-regression\n', 1)[1].split('\nFROM ', 1)[0]
            self.assertIn('apt-get install -y --no-install-recommends clang llvm', full_leaf)
            self.assertIn('COPY --from=rust-ordinary-builder /usr/local/bin/docker /usr/local/bin/docker', full_leaf)
            self.assertNotIn('dockerd', full_leaf)
        self.assertIn(f"target: [{', '.join(targets)}]\n", ci_base_jobs['build'])
        publish_job = ci_base_jobs['publish']
        publish_header = publish_job.split('    steps:', 1)[0]
        groups = 'base, specialized, app-wasm' if 'rust-viewer' in targets else 'base, specialized'
        self.assertIn(f'    strategy:\n      matrix:\n        group: [{groups}]\n', publish_header)
        self.assertIn('timeout-minutes: 10\n', publish_header)
        self.assertIn("if: github.event_name != 'pull_request' && github.ref == 'refs/heads/main'\n", publish_header)
        self.assertIn('needs: build\n', publish_header)
        self.assertIn('packages: write\n', publish_header)
        self.assertIn('rm -f "ci-${target}.tar"\n', publish_job)
        self.assertIn('docker rmi "$ref" "oasis7-ci-${target}:candidate"\n', publish_job)
        self.assertNotIn('if [[ "$target" == "browser" ]]', publish_job)
        expected_groups = (
            ('base', {'runtime', 'build', 'browser', 'rust-build', 'rust-wasm-build'}),
            ('specialized', {'codeql-rust', 'api-runtime', 'rust-ordinary-builder', 'rust-full-regression'}),
            ('app-wasm' if 'rust-viewer' in targets else 'specialized', {'rust-custody-wasm', 'rust-viewer'}),
        )
        by_target = {target: group for group, members in expected_groups for target in members}
        routes = [(by_target[target], target) for target in targets]
        downloads = re.findall(
            r"^      - uses: actions/download-artifact@9000827ccba6bdab643e8b6fd33ac0654aef8333\n"
            r"        if: matrix.group == '([a-z-]+)'\n        with:\n          name: ci-image-([a-z-]+)$",
            publish_job, re.M,
        )
        pushes = re.findall(
            r"^      - name: Publish verified [^\n]+\n        if: matrix.group == '([a-z-]+)'\n"
            r'        run: bash "\$RUNNER_TEMP/publish-ci-image.sh" ([a-z-]+)$',
            publish_job, re.M,
        )
        self.assertEqual(downloads, routes)
        self.assertEqual(pushes, routes)
        self.assertEqual(re.findall(r'^          name: ci-image-([a-z-]+)$', publish_job, re.M), targets)
        self.assertEqual(publish_job.count('run: bash "$RUNNER_TEMP/publish-ci-image.sh"'), len(targets))
        self.assertIn('      - name: Log out of GHCR\n        if: always()\n        run: docker logout ghcr.io\n', publish_job)

        wasm_wf = (ROOT / '.github/workflows/wasm-determinism-gate.yml').read_text()
        wasm_pr = wasm_wf.split('  pull_request:\n', 1)[1].split('  workflow_dispatch:\n', 1)[0]
        wf_paths = re.findall(r'^      - (.+)$', wasm_pr, re.M)
        self.assertTrue(wf_paths)
        self.assertIn(
            "if: github.event_name != 'pull_request' || !github.event.pull_request.draft",
            wasm_wf,
        )
        planner = (ROOT / 'scripts/plan-wasm-determinism-scope.sh').read_text()
        classify_body = planner.split('classify_changed_path() {', 1)[1].split('\n}', 1)[0]
        # Extract every pattern branch from classify_changed_path() and ensure the workflow paths cover it.
        raw_patterns = re.findall(r'^\s+([^\s#][^\n]*?)(?:\|\\|\))', classify_body, re.M)
        planner_patterns = []
        for line in raw_patterns:
            for item in line.split('|'):
                item = item.strip()
                if item and item != 'case "$path" in':
                    planner_patterns.append(item)
        self.assertTrue(planner_patterns)
        for pattern in planner_patterns:
            # Strip trailing /* or /** to compare directory prefixes cleanly against workflow globs.
            normalized = pattern
            if normalized in (
                'crates/oasis7_wasm_build',
                'crates/oasis7_wasm_build/*',
                'crates/oasis7_wasm_build/**/*',
            ):
                normalized = 'crates/oasis7_wasm_build/**'
            elif normalized in (
                'crates/oasis7_wasm_sdk',
                'crates/oasis7_wasm_sdk/*',
                'crates/oasis7_wasm_sdk/**/*',
            ):
                normalized = 'crates/oasis7_wasm_sdk/**'
            elif normalized in (
                'crates/oasis7_wasm_abi',
                'crates/oasis7_wasm_abi/*',
                'crates/oasis7_wasm_abi/**/*',
            ):
                normalized = 'crates/oasis7_wasm_abi/**'
            elif normalized == 'docker/wasm-builder/*':
                normalized = 'docker/wasm-builder/**'
            elif normalized in ('tools/wasm_build_suite/*', 'tools/wasm_build_suite/**/*'):
                normalized = 'tools/wasm_build_suite/**'
            self.assertIn(normalized, wf_paths, f'wasm-determinism-gate.yml paths missing {pattern}')

    def test_network_wasm_tools_precede_group_execution(self):
        net = JOBS['net']
        compiler_step = ('Verify the preinstalled WASM C compiler' if rust_image_cache_namespace('net', net)
                         else 'Install WASM C compiler')
        self.assertLess(net.index(compiler_step), net.index('Execute selected group'))
        self.assertLess(net.index('llvm-ar --version'), net.index('Execute selected group'))
        writer = JOBS['full-regression']
        self.assertIn('            clang \\', writer)
        self.assertIn('            llvm \\', writer)
        self.assertIn('rustup target add wasm32-unknown-unknown', writer)
        for tool in ('clang --print-targets | grep -w wasm32', 'llvm-ar --version'):
            self.assertLess(writer.index(tool), writer.index('Run full test tier'))

    def test_trusted_writer_only_runs_on_protected_main(self):
        workflow, writer = self.shared_cache_workflow()
        on = re.search(r'^on:\n(.*?)(?=^[^\s#]|\Z)', workflow, re.M | re.S)[1]
        self.assertEqual(set(re.findall(r'^  ([a-z_]+):', on, re.M)), {'schedule', 'workflow_dispatch'})
        image_writer = workflow_jobs(workflow).get('rust-image-native-cache')
        modes = ['full']
        if image_writer:
            modes.append('image-native')
        if ('plan-image-component-cache' in workflow_jobs(workflow)
                or 'image-support' in workflow_jobs(workflow).get('rust-image-component-cache', '')):
            modes.append('image-support')
        self.assertIn(f'options: [{", ".join(modes)}]', on)
        self.assertNotIn('source_ref', on)
        self.assertNotIn('workflow_run', on)
        self.assertIn('cache-mode: write', writer)
        self.assertIn('test "$GITHUB_REF" = refs/heads/main', writer)
        self.assertNotIn('test "$ACTIONS_CACHE_MODE"', writer)
        header = writer.split('    runs-on:', 1)[0]
        expression = ' '.join(header.split('    if:', 1)[1].strip().removeprefix('>-').split())
        for event in ('schedule', 'workflow_dispatch', 'push', 'pull_request', 'workflow_run'):
            for ref in ('refs/heads/main', 'refs/heads/topic', 'refs/pull/1/merge', 'refs/tags/v1'):
                for matching in (False, True):
                    for mode in ('full', 'image-native', '', 'newapi_bridge_package'):
                        values = {'github.event_name': event, 'github.ref': ref,
                                  'github.workflow_sha': 'a' if matching else 'b',
                                  'github.sha': 'a', 'inputs.run_mode': mode}
                        predicate = expression
                        for token, value in values.items():
                            predicate = predicate.replace(token, repr(value))
                        predicate = predicate.replace('&&', ' and ').replace('||', ' or ')
                        with self.subTest(event=event, ref=ref, matching=matching, mode=mode):
                            self.assertEqual(eval(predicate, {'__builtins__': {}}, {}),
                                ref == 'refs/heads/main' and matching
                                and (event == 'schedule' or event == 'workflow_dispatch' and mode == 'full'))

    def test_shared_cache_workflow_is_independent_of_full_validation(self):
        workflow, writer = self.shared_cache_workflow()
        self.assertNotIn('shared-native-cache', JOBS)
        self.assertNotIn('needs:', writer.split('    steps:', 1)[0])
        self.assertIn('group: ci-shared-native-cache-main', workflow)
        self.assertIn('cancel-in-progress: false', workflow)
        self.assertNotIn('ci-shared-native-cache-main', WORKFLOW)
        writer_timeout = re.search(r'^    timeout-minutes:\s*(\d+)\s*$', writer.split('    steps:', 1)[0], re.M)
        self.assertIsNotNone(writer_timeout)
        self.assertGreater(int(writer_timeout.group(1)), 0)
        self.assertLessEqual(int(writer_timeout.group(1)), 150)
        self.assertIn('permissions:\n      contents: read\n      actions: read', writer)
        self.assertNotIn('actions: write', workflow)
        for forbidden in ('ci-full-result.py', 'ci-required-result.py',
                          'actions/upload-artifact@', 'actions/download-artifact@',
                          'full-extended', 'OASIS7_CI_TEST_BUNDLE'):
            self.assertNotIn(forbidden, workflow)
        for name in ('required-gate', 'explicit-full-gate'):
            self.assertNotIn('shared-native-cache', JOBS[name])
            self.assertNotIn('rust-cache.yml', JOBS[name])
        for name in ('full-regression', 'candidate-full-regression'):
            job = JOBS[name]
            self.assertIn('timeout-minutes: 210', job)
            self.assertIn('./scripts/ci-tests.sh full-extended', job)
            self.assertNotIn('cache-native', job)
            self.assertNotIn('dependency-cache-tools', job)
            cache = next(step for step in re.findall(r'^      - .*?(?=^      - |\Z)', job, re.M | re.S)
                         if 'uses: Swatinem/rust-cache@' in step)
            self.assertIn('save-if: false', cache)
            self.assertNotIn('GH_TOKEN:', cache)
            full = next(step for step in re.findall(r'^      - .*?(?=^      - |\Z)', job, re.M | re.S)
                        if './scripts/ci-tests.sh full-extended' in step)
            self.assertNotIn('if:', full)
            self.assertNotIn('continue-on-error:', full)

    def test_shared_cache_checkout_verifies_actual_current_main_source(self):
        _, writer = self.shared_cache_workflow()
        self.assertIn('ref: ${{ github.sha }}', writer)
        self.assertEqual(len(re.findall(r'uses: actions/checkout@[0-9a-f]{40}\b', writer)), 1)
        self.assertIn('persist-credentials: false', writer)
        step = next(step for step in re.findall(r'^      - .*?(?=^      - |\Z)', writer, re.M | re.S)
                    if 'name: Verify current main cache source\n' in step)
        self.assertNotIn('if:', step)
        self.assertNotIn('continue-on-error:', step)
        self.assertLess(writer.index(step), writer.index('uses: Swatinem/rust-cache@'))
        script = textwrap.dedent(step.split('        run: |\n', 1)[1])
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            subprocess.run(['git', 'init', '-q', '-b', 'main', str(root)], check=True)
            subprocess.run(['git', '-C', str(root), '-c', 'user.name=CI fixture',
                            '-c', 'user.email=ci-fixture@example.invalid', 'commit', '-q',
                            '--allow-empty', '-m', 'current cache source'], check=True)
            sha = subprocess.check_output(['git', '-C', str(root), 'rev-parse', 'HEAD'], text=True).strip()
            for ref in ('refs/heads/main', 'refs/heads/topic', 'refs/pull/1/merge'):
                for event_matches in (False, True):
                    for workflow_matches in (False, True):
                        environment = dict(os.environ, GITHUB_REF=ref,
                            GITHUB_SHA=sha if event_matches else 'b' * 40,
                            GITHUB_WORKFLOW_SHA=sha if workflow_matches else 'b' * 40)
                        result = subprocess.run(['bash', '-c', script], cwd=root,
                                                env=environment, capture_output=True, text=True)
                        with self.subTest(ref=ref, event_matches=event_matches, workflow_matches=workflow_matches):
                            self.assertEqual(result.returncode == 0,
                                ref == 'refs/heads/main' and event_matches and workflow_matches,
                                result.stdout + result.stderr)

    def test_readers_share_compatible_trusted_writer_identity(self):
        cache_workflow, writer = self.shared_cache_workflow()
        self.assertIn('cache-mode: write', writer)
        self.assertIn('shared-key: ci-shared-native-trusted-v4', writer)
        self.assertIn('env-vars: CARGO CC CFLAGS CXX CMAKE RUST OASIS7_WASM', writer)
        environments = []
        for text in (WORKFLOW, cache_workflow):
            block = re.search(r'^env:\n(.*?)(?=^[^\s#]|\Z)', text, re.M | re.S)[1]
            environments.append(dict(re.findall(
                r'^  ((?:CARGO|RUST|OASIS7_WASM)[A-Z0-9_]*): (.+)$', block, re.M)))
        self.assertEqual(environments[0], environments[1],
                         'shared dependency contexts require the same pinned compiler and Cargo/WASM environment')
        for name, job in JOBS.items():
            if 'save-if: false' in job and 'Swatinem/rust-cache@' in job:
                if name in ('viewer-js-required', 'viewer-performance-report'):
                    viewer_cache_kind(job)
                    continue
                if name == 'oasis7-test-build-candidate' and '    container:' in job:
                    trusted_image = re.search(
                        r'^      image: ghcr\.io/nil-studio/oasis7/ci-rust-ordinary-builder@sha256:([0-9a-f]{64})$',
                        JOBS['oasis7-test-build-trusted'], re.M,
                    )
                    self.assertIsNotNone(trusted_image)
                    self.assertIn(
                        f"shared-key: ${{{{ job.container.id && 'ci-ordinary-image-{trusted_image[1]}' || 'ci-shared-native-trusted-v4' }}}}\n",
                        job, name,
                    )
                    self.assertIn('env-vars: CARGO CC CFLAGS CXX CMAKE RUST OASIS7_WASM', job, name)
                    continue
                component = re.fullmatch(r'formal-(debug|endpoint)-build(?:-trusted)?', name)
                namespace = f'ci-formal-{component[1]}-deps-trusted-v1' if component else 'ci-shared-native-trusted-v4'
                self.assertIn('shared-key: ' + (rust_image_cache_namespace(name, job) or namespace) + '\n', job, name)
                self.assertIn('env-vars: CARGO CC CFLAGS CXX CMAKE RUST OASIS7_WASM', job, name)
        self.assertNotIn('ordinary-required-v2', WORKFLOW)

    def test_dependency_admission_is_local_to_the_two_cache_actions(self):
        admission = (ROOT / 'scripts/ci-rust-cache-admission.py').read_text()
        revision = re.search(r"^ACTION_REVISION = '([0-9a-f]{40})'$", admission, re.M)
        self.assertIsNotNone(revision)
        # Tar-before-reserve behavior was verified on this exact upstream
        # package. A pin upgrade must review/update the guard and this contract.
        self.assertEqual(revision[1], 'd1527c8f67ef9af357290475d4af4b0fab8f12e0')
        ecs_text = (ROOT / '.github/workflows/ecs-packages.yml').read_text()
        ecs_jobs = workflow_jobs(ecs_text)
        cache_workflow, shared_writer = self.shared_cache_workflow()
        lanes = ((shared_writer, 'shared-native', 'CI_CACHE_REPOSITORY_BUDGET_BYTES'),
                 (ecs_jobs['native'], 'ecs', 'ECS_CACHE_REPOSITORY_BUDGET_BYTES'),
                 (ecs_jobs['native-trusted'], 'ecs', 'ECS_CACHE_REPOSITORY_BUDGET_BYTES'))
        for job, family, budget in lanes:
            with self.subTest(family=family, trusted='cache-mode: write' in job):
                steps = re.findall(r'^      - .*?(?=^      - |\Z)', job, re.M | re.S)
                installs = [step for step in steps if 'id: dependency-cache-tools\n' in step]
                self.assertEqual(len(installs), 1)
                install = installs[0]
                self.assertIn('continue-on-error: true', install)
                self.assertIn('run: python3 scripts/ci-rust-cache-admission.py install', install)
                self.assertNotIn('GITHUB_PATH', install)
                cache = next(step for step in steps if 'uses: Swatinem/rust-cache@' in step)
                self.assertIn('uses: Swatinem/rust-cache@' + revision[1] + ' # v2', cache)
                self.assertLess(steps.index(install), steps.index(cache))
                self.assertIn("steps.dependency-cache-tools.outcome == 'success'", cache)
                self.assertIn('PATH: ${{ steps.dependency-cache-tools.outputs.path }}', cache)
                self.assertIn('OASIS7_DEPENDENCY_CACHE_FAMILY: ' + family, cache)
                self.assertIn('OASIS7_DEPENDENCY_CACHE_BUDGET_BYTES: ${{ env.' + budget + ' }}', cache)
                self.assertIn('GH_TOKEN: ${{ github.token }}', cache)
                self.assertIn('cache-on-failure: false', cache)
                self.assertNotIn('lookup-only:', cache)
                condition = re.search(r'^        if: (.*)$', cache, re.M).group(1)
                for outcome in ('success', 'failure', 'skipped', 'cancelled'):
                    for has_toolchains in (False, True):
                        predicate = condition.replace('steps.dependency-cache-tools.outcome', repr(outcome))
                        predicate = predicate.replace("hashFiles('scripts/ci-rust-cache-toolchains.py')",
                                                      repr('hash' if has_toolchains else ''))
                        allowed = eval(predicate.replace('&&', ' and '), {'__builtins__': {}}, {})
                        self.assertEqual(allowed, outcome == 'success' and (family != 'ecs' or has_toolchains))
                for step in steps:
                    if step != cache:
                        self.assertNotIn('steps.dependency-cache-tools.outputs.path', step)
                        self.assertNotIn('OASIS7_DEPENDENCY_CACHE_', step)
                self.assertNotIn('steps.dependency-cache-tools.outputs.path', job.split('    steps:', 1)[0])
        self.assertIn('permissions:\n      contents: read\n      actions: read', shared_writer)
        # Native uses one optional installer. The migrated Web lane also has
        # its optional Node installer and restore/save action, checked below.
        migrated = 'web-wasm-cache' in workflow_jobs(cache_workflow)
        self.assertEqual(cache_workflow.count('continue-on-error:'), 3 if migrated else 1)
        self.assertNotIn('dependency-cache-tools', JOBS['full-regression'])
        self.assertNotIn('dependency-cache-tools', JOBS['candidate-full-regression'])
        self.assertNotIn('dependency-cache-tools', ecs_jobs['web'])
        governance = (ROOT / 'scripts/ci-tests.sh').read_text().split(
            'run_workflow_governance_baseline_contract_tests() {', 1)[1].split('\n}', 1)[0]
        self.assertEqual(CONFIG['workflow_governance_python_tests'].count('scripts/ci-rust-cache-admission.test.py'), 1)

    def test_dependency_save_inputs_reject_step_only_status_functions(self):
        # GitHub permits status functions in step `if`, not action inputs.
        # The `job` context is available in `with`, so save-if uses job.status.
        ecs_jobs = workflow_jobs((ROOT / '.github/workflows/ecs-packages.yml').read_text())
        _, shared_writer = self.shared_cache_workflow()
        for name, job in (('shared-native-cache', shared_writer),
                          ('native', ecs_jobs['native']), ('native-trusted', ecs_jobs['native-trusted'])):
            cache = next(step for step in re.findall(r'^      - .*?(?=^      - |\Z)', job, re.M | re.S)
                         if 'uses: Swatinem/rust-cache@' in step)
            save = re.search(r'          save-if: \$\{\{ (.*?) \}\}', cache)
            self.assertIsNotNone(save, name)
            with self.subTest(job=name):
                self.assertNotRegex(save.group(1), r'(?i)\b(?:always|cancelled|failure|success)\s*\(')

    def test_dependency_save_requires_success_and_current_trusted_source(self):
        ecs_jobs = workflow_jobs((ROOT / '.github/workflows/ecs-packages.yml').read_text())
        _, shared_writer = self.shared_cache_workflow()
        for name, job in (('shared-native-cache', shared_writer),
                          ('native', ecs_jobs['native']), ('native-trusted', ecs_jobs['native-trusted'])):
            cache = next(step for step in re.findall(r'^      - .*?(?=^      - |\Z)', job, re.M | re.S)
                         if 'uses: Swatinem/rust-cache@' in step)
            save = re.search(r'          save-if: \$\{\{ (.*?) \}\}', cache)
            self.assertIsNotNone(save, name)
            expression = save.group(1)
            for status in ('success', 'failure', 'cancelled'):
                for ref in ('refs/heads/main', 'refs/heads/topic', 'refs/pull/1/merge'):
                    for matching_source in (False, True):
                        values = {'job.status': status, 'github.ref': ref, 'github.job': name,
                                  'github.workflow_sha': 'a' if matching_source else 'b', 'github.sha': 'a'}
                        predicate = expression
                        for token, value in values.items():
                            predicate = predicate.replace(token, repr(value))
                        predicate = predicate.replace('&&', ' and ').replace('||', ' or ')
                        with self.subTest(job=name, status=status, ref=ref, matching=matching_source):
                            actual = eval(predicate, {'__builtins__': {}}, {})
                            self.assertEqual(actual, status == 'success' and ref == 'refs/heads/main'
                                             and matching_source and name != 'native')

    def test_shared_cache_is_populated_completely_before_successful_save(self):
        _, writer = self.shared_cache_workflow()
        steps = re.findall(r'^      - .*?(?=^      - |\Z)', writer, re.M | re.S)
        cache = next(step for step in steps if 'uses: Swatinem/rust-cache@' in step)
        self.assertIn('id: native-cache', cache)
        self.assertIn('cache-on-failure: false', cache)
        self.assertNotIn('save-if: false', cache)
        completion = next(step for step in steps if 'name: Complete shared native cache\n' in step)
        self.assertIn('./scripts/ci-tests.sh cache-native', completion)
        self.assertNotIn('continue-on-error', completion)
        self.assertLess(steps.index(cache), steps.index(completion))
        condition = re.search(r'^        if: (.*)$', completion, re.M)[1]
        for hit in ('true', 'false', '', 'unknown'):
            with self.subTest(cache_hit=hit):
                expression = condition.replace('steps.native-cache.outputs.cache-hit', repr(hit))
                self.assertEqual(eval(expression, {'__builtins__': {}}, {}), hit != 'true')
        self.assertNotIn('full-extended', writer)
        for name, job in JOBS.items():
            if 'shared-key: ci-shared-native-trusted-v4' in job:
                self.assertIn('save-if: false', job, name)
                # Full validation still writes tool caches; the ordinary producer
                # writes its workspace snapshot, not shared dependencies.
                if name not in ('full-regression', 'oasis7-test-build-trusted'):
                    self.assertNotIn('cache-mode: write', job, name)

    def test_shared_cache_normalization_runs_after_install_before_restore(self):
        _, shared_writer = self.shared_cache_workflow()
        for name, job in (*JOBS.items(), ('shared-native-cache', shared_writer)):
            if re.search(r'shared-key: (?:ci-shared-native-trusted-v4|ci-formal-(?:debug|endpoint)-deps-trusted-v1)\n', job):
                prepare = job.index('python3 scripts/ci-rust-cache-toolchains.py')
                self.assertIn('"$RUST_TOOLCHAIN" "$OASIS7_WASM_TOOLCHAIN"', job, name)
                self.assertLess(job.index('rustup default'), prepare, name)
                self.assertLess(prepare, job.index('uses: Swatinem/rust-cache@'), name)

    def test_download_caches_follow_actual_worksets(self):
        node_jobs={name for name,job in JOBS.items() if 'uses: actions/setup-node@' in job}
        expected = {'viewer-js-required','viewer-performance-report','site-quality','workspace-support-legacy',
                    'workspace-support-other','custody-native','custody-node','full-regression','candidate-full-regression'}
        custody_wasm = JOBS['custody-wasm-production']
        if rust_image_cache_namespace('custody-wasm-production', custody_wasm):
            self.assertIn('export PATH=/opt/oasis7-node22/bin:$PATH', custody_wasm)
            self.assertIn('process.version.startsWith("v22.")', custody_wasm)
            self.assertIn('npm --version', custody_wasm)
        else:
            expected.add('custody-wasm-production')
        self.assertEqual(node_jobs, expected)
        self.assertNotIn('oasis7_viewer', JOBS['launcher-web'])
        self.assertIn("node-version: '24'", JOBS['site-quality'])
        for name in node_jobs - {'site-quality','workspace-support-legacy','workspace-support-other','custody-native','custody-wasm-production','custody-node'}:
            self.assertIn('cache: npm',JOBS[name])
            self.assertIn('cache-dependency-path: crates/oasis7_viewer/package-lock.json',JOBS[name])
            self.assertIn('npm ci --prefix crates/oasis7_viewer',JOBS[name])
        for name,job in JOBS.items():
            if 'Swatinem/rust-cache@' in job and name in {g.replace('_', '-') for g in CONFIG['groups']}:
                if name in ('viewer-js-required', 'viewer-performance-report'):
                    viewer_cache_kind(job)
                else:
                    namespace = rust_image_cache_namespace(name, job) or 'ci-shared-native-trusted-v4'
                    self.assertIn('shared-key: ' + namespace + '\n', job)
                    self.assertIn('env-vars: CARGO CC CFLAGS CXX CMAKE RUST OASIS7_WASM',job)
                self.assertIn('add-rust-environment-hash-key: true',job)
                preparation = ('name: Verify the preinstalled Rust toolchains'
                               if rust_image_cache_namespace(name, job) else 'rustup default')
                self.assertLess(job.index(preparation), job.index('uses: Swatinem/rust-cache@d1527c8f67ef9af357290475d4af4b0fab8f12e0 # v2'))
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
        for name in ('oasis7-test-build-candidate', 'oasis7-test-build-trusted',
                     'formal-test-build', 'formal-test-build-trusted', 'formal-process-tests', 'command-center'):
            self.assertIn("needs.select.result == 'success'", JOBS[name])

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

    def test_platform_product_jobs_still_gate(self):
        self.assertIn('runs-on: windows-2022', JOBS['windows-rollout'])
        self.assertIn('p2p-public-testnet-package-rollout.test.sh', JOBS['windows-rollout'])
        self.assertIn('testnet-packages-macos-arm64-contract.test.sh', JOBS['baseline'])
        self.assertIn('testnet-packages-macos-arm64-contract.test.sh', JOBS['macos-package-contract'])
        self.assertIn('p2p-public-testnet-fleet-health.test.py', JOBS['fleet-health'])
        self.assertIn('p2p-public-testnet-package-rollout.test.sh', JOBS['fleet-health'])
        self.assertIn('fromJSON(\'["ubuntu-24.04","windows-2022","macos-15"]\')', JOBS['fleet-health'])
        self.assertIn('--host-smoke', JOBS['fleet-health'])
        self.assertEqual(WORKFLOW.count('testnet-packages-macos-arm64-contract.test.sh'), 2)
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
        self.assertIn('policy_sha="$BASE_SHA"', select)
        self.assertIn('git show "${policy_sha}:scripts/${file}"', select)
        self.assertIn('--test-ref "$tested_sha"', select)
        self.assertIn('for file in plan-rust-required-scope.py ci-required-scope.json ci-required-result.py ci-formal-plan.py ci-formal-process-suites.json;', select)
        self.assertIn('bootstrap_full=true', select)
        self.assertIn('&& "$GITHUB_REF" != refs/heads/main', select)
        authority_consumers = {name for name, job in JOBS.items()
                               if 'name: ${{ needs.select.outputs.authority_artifact }}' in job}
        self.assertEqual(authority_consumers, {'formal-debug-build', 'formal-debug-build-trusted',
            'formal-endpoint-build', 'formal-endpoint-build-trusted', 'formal-test-build',
            'formal-test-build-trusted', 'formal-process-tests', 'formal-process',
            'workflow-governance', 'required-gate', 'explicit-full-gate'})
        for name, job in JOBS.items():
            if name != 'pr-fast-preview':
                self.assertNotIn('matrix.group', job, name)
        self.assertEqual(len(re.findall(r'uses: actions/checkout@[0-9a-f]{40}\b', WORKFLOW)), WORKFLOW.count('persist-credentials: false'))
        self.assertNotIn('secrets.', WORKFLOW)
        without_admitted_uploads = WORKFLOW
        for family, suffix in itertools.product(('formal-debug', 'formal-endpoint'), ('', '-trusted')):
            name = family + '-build' + suffix
            dependency_cache = next(step for step in re.findall(r'^      - .*?(?=^      - |\Z)',
                                                                 JOBS[name], re.M | re.S)
                                    if 'uses: Swatinem/rust-cache@' in step
                                    and 'OASIS7_DEPENDENCY_CACHE_FAMILY: ' + family in step)
            self.assertEqual(dependency_cache.count('GH_TOKEN: ${{ github.token }}'), 1)
            without_admitted_uploads = without_admitted_uploads.replace(dependency_cache, '', 1)
        if '    container:' in JOBS['oasis7-test-build-trusted']:
            ordinary_cache = rust_cache_steps(JOBS['oasis7-test-build-trusted'])[0]
            self.assertIn('OASIS7_DEPENDENCY_CACHE_FAMILY: ordinary-image', ordinary_cache)
            self.assertEqual(ordinary_cache.count('GH_TOKEN: ${{ github.token }}'), 1)
            without_admitted_uploads = without_admitted_uploads.replace(ordinary_cache, '', 1)
        admission = JOBS['periodic-admission']
        self.assertIn("if: github.event_name == 'schedule'", admission)
        self.assertIn('actions: read', admission)
        self.assertNotIn('write', admission)
        self.assertIn('ref: ${{ github.workflow_sha }}', admission)
        self.assertEqual(admission.count('GH_TOKEN: ${{ github.token }}'), 1)
        without_admitted_uploads = without_admitted_uploads.replace(admission, '', 1)
        self.assertNotIn('GH_TOKEN:', without_admitted_uploads)
        cache_workflow, _ = self.shared_cache_workflow()
        without_admitted_uploads = cache_workflow
        for name, writer in workflow_jobs(cache_workflow).items():
            if name == 'plan-image-component-cache':
                self.assertIn('    cache-mode: none\n', writer)
                self.assertIn('    permissions: {}\n', writer)
                self.assertNotIn('uses:', writer)
                self.assertNotIn('github.token', writer)
                continue
            if name == 'browser-archive-cache':
                for admission in browser_archive_cache_writer(writer):
                    without_admitted_uploads = without_admitted_uploads.replace(admission, '', 1)
                continue
            if name == 'worker-build-cache':
                admission = worker_tool_cache_writer(writer)
                without_admitted_uploads = without_admitted_uploads.replace(admission, '', 1)
                continue
            dependency_caches = rust_cache_steps(writer)
            self.assertEqual(len(dependency_caches), 1, name)
            dependency_cache = dependency_caches[0]
            family = {'shared-native-cache': 'shared-native',
                      'rust-image-native-cache': 'rust-image-native',
                      'rust-image-component-cache': '${{ matrix.family }}',
                      'web-wasm-cache': 'release-web'}[name]
            self.assertIn('OASIS7_DEPENDENCY_CACHE_FAMILY: ' + family + '\n', dependency_cache)
            self.assertEqual(dependency_cache.count('GH_TOKEN: ${{ github.token }}'), 1)
            without_admitted_uploads = without_admitted_uploads.replace(dependency_cache, '', 1)
        self.assertNotIn('GH_TOKEN:', without_admitted_uploads)
        self.assertNotIn('secrets.', cache_workflow)
        self.assertNotIn('scripts/pm/', WORKFLOW)
        self.assertNotIn('task_uid', WORKFLOW)
        self.assertIn('permissions:\n  contents: read', WORKFLOW)

    def test_python_control_assertions_use_fixed_baseline_and_pinned_offline_tools(self):
        job = JOBS['workflow-governance']
        trusted = 'python3 "$RUNNER_TEMP/ci-authority/ci-python-control-contracts.py" run'
        self.assertIn(trusted, job)
        self.assertIn('--policy-sha "$POLICY_SHA"', job)
        self.assertIn('--plan "$RUNNER_TEMP/ci-control-selection/selection.json"', job)
        self.assertIn('python_control_contract_version == 1', job)
        if '    container:' in job:
            self.assertRegex(job, r'(?m)^    container:\n'
                                  r'      image: ghcr\.io/nil-studio/oasis7/ci-rust-build@sha256:[0-9a-f]{64}$')
            self.assertRegex(job, r'(?m)^      options: --user 1001:1001(?: --init)?$')
            permissions = re.search(r'(?m)^    permissions:\n((?:      [^\n]+\n)+)', job)
            self.assertIsNotNone(permissions)
            self.assertCountEqual(permissions[1].splitlines(), [
                '      contents: read', '      actions: read', '      packages: read',
            ])
            self.assertNotRegex(job, r'(?m)^\s+[a-z-]+: write$')
            verify = job.split('      - name: Verify preinstalled governance tools\n', 1)
            self.assertEqual(len(verify), 2)
            self.assertLess(job.index('name: Verify preinstalled governance tools'), job.index(trusted))
            preparation = verify[1].split('\n      - ', 1)[0]
            for command in (
                    'cargo +"$RUST_TOOLCHAIN" --version',
                    'test "$(rustc --version)" = "$(rustc +"$RUST_TOOLCHAIN" --version)"',
                    'cargo +"$RUST_TOOLCHAIN" fmt --version',
                    'cargo +"$RUST_TOOLCHAIN" clippy --version',
                    'command -v zstd', 'command -v zstdmt'):
                self.assertIn('          ' + command + '\n', preparation + '\n')
            self.assertNotRegex(job, r'rustup (?:toolchain install|default)\b')
        else:
            self.assertIn('rustup toolchain install "$RUST_TOOLCHAIN" --profile minimal', job)
            self.assertIn('rustup default "$RUST_TOOLCHAIN"', job)
            self.assertIn('command -v zstdmt', job)
        self.assertIn('pip install --disable-pip-version-check --requirement scripts/doc-governance-requirements.txt', job)
        self.assertLess(job.index(trusted), job.index('bash ./scripts/ci-tests.sh required'))
        self.assertNotIn('cargo test -p', job)
        self.assertNotIn('Swatinem/rust-cache', job)
        driver = (ROOT / 'scripts/ci-tests.sh').read_text()
        self.assertIn('scripts/ci-python-control-contracts.test.py', CONFIG['workflow_governance_python_tests'])
        self.assertIn('ci-python-control-contracts.py local-governance', driver)
        for legacy in (False, True):
            with self.subTest(legacy=legacy), tempfile.TemporaryDirectory() as directory:
                result, _, plan, _, _ = self.run_selection(
                    directory, event='pull_request', legacy=legacy, candidate=True)
                self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                runner = Path(directory) / 'ci-authority/ci-python-control-contracts.py'
                if legacy:
                    self.assertFalse(runner.exists())
                    self.assertEqual(plan['scope'], 'full')
                    self.assertNotIn('python_control_contract_version', plan)
                else:
                    self.assertEqual(runner.read_bytes(), (ROOT / 'scripts/ci-python-control-contracts.py').read_bytes())

    def test_governance_mode_is_fixed_before_candidate_code_can_write_outputs(self):
        job = JOBS['workflow-governance']
        steps = re.findall(r'^      - .*?(?=^      - |\Z)', job, re.M | re.S)
        mode = next(step for step in steps if 'id: control-mode\n' in step)
        execute = next(step for step in steps if 'name: Execute trusted baseline Python' in step)
        fallback = next(step for step in steps if 'name: Execute selected group\n' in step)
        self.assertLess(job.index(mode), job.index('name: Install document contract parser'))
        self.assertIn('python3 -I "$RUNNER_TEMP/ci-authority/ci-python-control-contracts.py" --help', mode)
        self.assertIn('COMPLETE_GOVERNANCE: ${{ steps.control-mode.outputs.complete_governance }}', execute)
        self.assertNotIn('GITHUB_OUTPUT', execute)
        self.assertIn("if: steps.control-mode.outputs.complete_governance != 'true'", fallback)

        def script(step):
            body = step.split('        run: ', 1)[1]
            return textwrap.dedent(body[2:] if body.startswith('|\n') else body)

        for capable, fails in ((False, False), (True, False), (True, True)):
            with self.subTest(capable=capable, fails=fails), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                authority = root / 'temp/ci-authority'
                authority.mkdir(parents=True)
                (root / 'scripts').mkdir()
                (root / 'scripts/ci-tests.sh').write_text('echo fallback-completed\n')
                # The candidate advertises the new mode even when BASE cannot.
                (root / 'scripts/ci-python-control-contracts.py').write_text('print("--governance")\n')
                (authority / 'ci-python-control-contracts.py').write_text(
                    'import os,sys\nfrom pathlib import Path\n'
                    'if "--help" in sys.argv:\n'
                    f'    print({"--governance" if capable else "legacy runner"!r})\n'
                    '    raise SystemExit(0)\n'
                    'print("complete" if "--governance" in sys.argv else "legacy")\n'
                    'Path(os.environ["GITHUB_OUTPUT"]).write_text("complete_governance=true\\n")\n'
                    f'raise SystemExit({42 if fails else 0})\n')
                mode_output = root / 'mode-output'
                environment = dict(os.environ, RUNNER_TEMP=str(root / 'temp'),
                                   GITHUB_WORKSPACE=str(root), GITHUB_OUTPUT=str(mode_output),
                                   POLICY_SHA='a' * 40)
                probe = subprocess.run(['bash', '-e', '-u', '-o', 'pipefail', '-c', script(mode)],
                                       env=environment, cwd=root, text=True, capture_output=True)
                self.assertEqual(probe.returncode, 0, probe.stderr)
                fixed_mode = mode_output.read_text().strip().split('=', 1)[1]
                self.assertEqual(fixed_mode, str(capable).lower())
                execution_output = root / 'execution-output'
                environment.update(GITHUB_OUTPUT=str(execution_output), COMPLETE_GOVERNANCE=fixed_mode)
                result = subprocess.run(['bash', '-e', '-u', '-o', 'pipefail', '-c', script(execute)],
                                        env=environment, cwd=root, text=True, capture_output=True)
                self.assertEqual(result.returncode, 42 if fails else 0, result.stderr)
                self.assertEqual(result.stdout.strip(), 'complete' if capable else 'legacy')
                self.assertEqual(execution_output.read_text().strip(), 'complete_governance=true')
                # Actions snapshots the earlier step's outputs and applies its
                # default success() condition; later output writes cannot replace it.
                if result.returncode == 0 and fixed_mode != 'true':
                    tail = subprocess.run(['bash', '-e', '-c', script(fallback)],
                                          env=environment, cwd=root, text=True, capture_output=True)
                    self.assertEqual(tail.returncode, 0, tail.stderr)
                    self.assertEqual(tail.stdout.strip(), 'fallback-completed')
                self.assertEqual(mode_output.read_text().strip(), 'complete_governance=' + fixed_mode)

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
        functions = workspace_support_fixture_functions(driver)
        script = "set -euo pipefail\n" + functions + """run_signed_read_protocol_contract_tests() { :; }
run_cargo() { :; }
run_cargo_clippy() { :; }
run() { if [[ "$1" == "$ci_python" && "${2:-}" == - ]]; then "$@"; fi; }
""" + "\nrun_oasis7_workspace_support_crate_tests\n"
        features = ('identity', 'owner', 'executor', 'recovery-fence',
                    'owner,approval-harness', 'identity,native-runtime-harness')
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            manifest = root / 'crates/oasis7_custody_worker/Cargo.toml'
            manifest.parent.mkdir(parents=True)
            # A closed manifest fixture keeps this shell-consumer proof separate
            # from the conditional future-profile contracts in the Python suite.
            names = sorted({name for profile in features for name in profile.split(',')})
            manifest.write_text('[features]\ndefault = []\n'
                                + ''.join(name + ' = []\n' for name in names))
            binary = root / 'bin'
            binary.mkdir()
            capture = root / 'cargo-command-capture'
            cargo = binary / 'cargo'
            # This executable captures argv only; no real Cargo or compiler runs.
            cargo.write_text('#!/bin/sh\nprintf \'%s\\n\' "$*" >> "$WORKER_COMPILE_CAPTURE"\nexit "$WORKER_COMPILE_RESULT"\n')
            cargo.chmod(0o755)
            environment = dict(os.environ, PATH=str(binary) + os.pathsep + os.environ['PATH'],
                               WORKER_COMPILE_CAPTURE=str(capture), WORKER_COMPILE_RESULT='0')
            result = subprocess.run(['bash', '-c', script], cwd=root, env=environment,
                                    text=True, capture_output=True)
            self.assertEqual(result.returncode, 0, result.stderr)
            expected = ['check -p oasis7_custody_worker --target wasm32-unknown-unknown '
                        '--no-default-features --features ' + feature + ' --locked'
                        for feature in features]
            self.assertEqual(capture.read_text().splitlines(), expected)
            capture.unlink()
            environment['WORKER_COMPILE_RESULT'] = '42'
            failed = subprocess.run(['bash', '-c', script], cwd=root, env=environment,
                                    text=True, capture_output=True)
            self.assertNotEqual(failed.returncode, 0)
            self.assertIn('exit status 42', failed.stderr)
            self.assertEqual(capture.read_text().splitlines(), expected[:1])

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

    def test_invalid_selected_group_cannot_silently_skip_command(self):
        driver = (ROOT / 'scripts/ci-tests.sh').read_text()
        self.assertIn('Unknown CI group:', driver)
        self.assertNotIn('disabled_by_scope_planner', driver)
        self.assertNotIn('required_gate_execution_contract', driver)
        self.assertNotIn('scripts/pm/', driver)

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

    def test_periodic_observation_gates_full_without_blocking_other_events(self):
        select = JOBS['select']
        self.assertIn('needs: [periodic-admission, main-fast-admission]', select)
        condition = select.split('    if: >-\n', 1)[1].split('    runs-on:', 1)[0]
        for event, ref, draft, mode, admission, full, mf_adm, mf_run, expected in (
                ('pull_request', 'refs/pull/1/merge', False, '', 'skipped', '', 'skipped', '', True),
                ('pull_request', 'refs/pull/1/merge', True, '', 'skipped', '', 'skipped', '', False),
                ('push', 'refs/heads/main', False, '', 'skipped', '', 'success', 'true', True),
                ('push', 'refs/heads/main', False, '', 'skipped', '', 'success', 'false', False),
                ('push', 'refs/heads/main', False, '', 'skipped', '', 'failure', 'true', False),
                ('push', 'refs/heads/other', False, '', 'skipped', '', 'skipped', '', True),
                ('workflow_dispatch', 'refs/heads/main', False, 'full', 'skipped', '', 'skipped', '', True),
                ('workflow_dispatch', 'refs/heads/main', False, 'newapi_bridge_package', 'skipped', '', 'skipped', '', False),
                ('schedule', 'refs/heads/main', False, '', 'success', 'true', 'skipped', '', True),
                ('schedule', 'refs/heads/main', False, '', 'success', 'false', 'skipped', '', False),
                ('schedule', 'refs/heads/main', False, '', 'failure', 'true', 'skipped', '', False),
                ('schedule', 'refs/heads/main', False, '', 'cancelled', 'true', 'skipped', '', False)):
            expr = condition
            for token, value in {'always()': True, 'cancelled()': False,
                    'github.event_name': event, 'github.ref': ref,
                    'github.event.pull_request.draft': draft,
                    'github.event.action': 'opened', 'github.event.label.name': '',
                    'github.event.pull_request.head.repo.full_name': 'nil-studio/oasis7',
                    'github.repository': 'nil-studio/oasis7',
                    'inputs.run_mode': mode,
                    'needs.periodic-admission.result': admission,
                    'needs.periodic-admission.outputs.run_full': full,
                    'needs.main-fast-admission.result': mf_adm,
                    'needs.main-fast-admission.outputs.run_main_fast': mf_run}.items():
                expr = expr.replace(token, repr(value))
            expr = re.sub(r'!(?!=)', 'not ', expr.replace('&&', ' and ').replace('||', ' or '))
            with self.subTest(event=event, ref=ref, draft=draft, admission=admission, run_full=full, mf_adm=mf_adm, mf_run=mf_run):
                self.assertEqual(eval(' '.join(expr.split()), {'__builtins__': {}}, {}), expected)

    def test_selected_jobs_survive_skipped_admission_and_keep_failure_boundaries(self):
        names = (
            'command-center', 'baseline', 'rust-baseline', 'oasis7-test-build-candidate',
            'consensus', 'distfs', 'node', 'net', 'viewer-js-required',
            'viewer-performance-report', 'pixel-world-bridge', 'launcher-web',
            'workspace-support-legacy', 'workspace-support-other', 'custody-native', 'custody-native-contracts',
            'custody-wasm-production', 'custody-node',
            'scenario-regression', 'operational-contracts', 'packaging-contracts',
            'workflow-governance', 'codex-agent-config-validation', 'compile-metrics',
            'site-quality', 'doc-checker-contracts', 'cargo-tooling-contracts',
            'windows-rollout', 'macos-package-contract', 'fleet-health',
        )
        self.assertEqual(len(names), 30)
        producers = ('custody-native', 'custody-wasm-production')
        split_jobs = {*producers, 'custody-native-contracts', 'custody-node', 'workspace-support-other'}
        conditions = {name: re.search(r'^    if: (.*?)(?=^    [a-z-]+:|\Z)',
                                      JOBS[name], re.M | re.S)[1] for name in names}

        def values_for(name, event, selected=True, select='success', scope='targeted', results=None,
                       consolidation=0, direct_support=0, offline=0):
            values = {
                'needs.select.result': select,
                'needs.select.outputs.trusted_source': 'true' if event == 'push' else 'false',
                'fromJSON(needs.select.outputs.workspace_support_partitions)': ['custody', 'other'] if selected else [],
                'fromJSON(needs.select.outputs.plan).scope': scope,
                'fromJSON(needs.select.outputs.plan).fast_plan_version': 1,
                'fromJSON(needs.select.outputs.plan).ordinary_main_fast_cache_writer_version': 0,
                'fromJSON(needs.select.outputs.plan).contract_job_consolidation_version': consolidation,
                'fromJSON(needs.select.outputs.plan).command_center_offline_version': offline,
                'fromJSON(needs.select.outputs.plan).workspace_support_direct_gate_version': direct_support,
                'fromJSON(needs.select.outputs.plan).fast_execution_version': 1,
                'fromJSON(needs.select.outputs.plan).ordinary_execution': 'artifact',
                'fromJSON(needs.select.outputs.plan).custody_execution_version': 1 if selected and name in split_jobs and not direct_support else 0,
                'fromJSON(needs.select.outputs.plan).custody_compile_proof_version': 1 if selected and not direct_support else 0,
                'fromJSON(needs.select.outputs.plan).execution_profile': {
                    'pull_request': 'pr-fast', 'push': 'main-fast',
                    'workflow_dispatch': 'candidate-full',
                }[event],
                'github.event_name': event,
                'github.ref': 'refs/pull/1/merge' if event == 'pull_request' else 'refs/heads/main',
                'github.workflow_sha': 'workflow', 'github.sha': 'workflow',
                'inputs.run_mode': 'full' if event == 'workflow_dispatch' else '',
            }
            values.update({token: str(selected).lower() for token in
                           re.findall(r'needs\.select\.outputs\.run_[a-z0-9_]+', conditions[name])})
            values.update({f'needs.{producer}.result': (results or {}).get(producer, 'success')
                           for producer in producers})
            return values

        # These events deliberately skip the schedule-only admission ancestor.
        # The effective default success() is false even after select succeeds.
        for event in ('pull_request', 'push', 'workflow_dispatch'):
            for name, condition in conditions.items():
                with self.subTest(job=name, event=event, mutation='remove status prefix'):
                    values = values_for(name, event)
                    self.assertTrue(effective_job_condition(condition, values, ['skipped', 'success']))
                    original_guard = re.sub(r'always\(\)\s*&&\s*!cancelled\(\)\s*&&\s*',
                                            '', condition, count=1)
                    self.assertNotEqual(original_guard, condition)
                    self.assertFalse(effective_job_condition(original_guard, values, ['skipped', 'success']))
                    self.assertTrue(effective_job_condition(original_guard, values, ['success', 'success']))
                for select, selected, cancelled in itertools.product(
                        ('success', 'failure', 'cancelled', 'skipped'), (False, True), (False, True)):
                    with self.subTest(job=name, event=event, select=select, selected=selected, cancelled=cancelled):
                        values = values_for(name, event, selected, select)
                        self.assertEqual(effective_job_condition(condition, values, ['skipped', select],
                                                                 cancelled=cancelled),
                                         select == 'success' and selected and not cancelled)
                if name not in split_jobs:
                    # Old trusted planners retain the complete full-scope fallback.
                    with self.subTest(job=name, event=event, scope='full'):
                        values = values_for(name, event, selected=False, scope='full')
                        self.assertTrue(effective_job_condition(condition, values, ['skipped', 'success']))
            for ver in (1, 2, 3):
                for retired in ('codex-agent-config-validation', 'compile-metrics', 'macos-package-contract'):
                    for scope in ('targeted', 'full', 'explicit-full'):
                        values = values_for(retired, event, selected=True, scope=scope, consolidation=ver)
                        self.assertFalse(effective_job_condition(conditions[retired], values, ['skipped', 'success']))
            for scope in ('targeted', 'full', 'explicit-full'):
                values = values_for('command-center', event, selected=True, scope=scope, consolidation=5, offline=1)
                self.assertFalse(effective_job_condition(conditions['command-center'], values, ['skipped', 'success']))
            absorbed_center = values_for('baseline', event, selected=False, consolidation=5, offline=1)
            absorbed_center['needs.select.outputs.run_command_center'] = 'true'
            self.assertTrue(effective_job_condition(conditions['baseline'], absorbed_center, ['skipped', 'success']))
            for retired in ('consensus', 'pixel-world-bridge', 'viewer-performance-report'):
                self.assertTrue(effective_job_condition(
                    conditions[retired], values_for(retired, event, selected=True, consolidation=1), ['skipped', 'success']))
                for ver in (2, 3):
                    for scope in ('targeted', 'full', 'explicit-full'):
                        values = values_for(retired, event, selected=True, scope=scope, consolidation=ver)
                        self.assertFalse(effective_job_condition(conditions[retired], values, ['skipped', 'success']))
            for retired in ('packaging-contracts', 'cargo-tooling-contracts', 'windows-rollout'):
                for ver in (1, 2):
                    self.assertTrue(effective_job_condition(
                        conditions[retired], values_for(retired, event, selected=True, consolidation=ver), ['skipped', 'success']))
                for scope in ('targeted', 'full', 'explicit-full'):
                    values = values_for(retired, event, selected=True, scope=scope, consolidation=3)
                    self.assertFalse(effective_job_condition(conditions[retired], values, ['skipped', 'success']))
            # Consolidated owners trigger even if only the absorbed group was selected.
            for host, flag, ver in (
                ('baseline', 'needs.select.outputs.run_macos_package_contract', 1),
                ('baseline', 'needs.select.outputs.run_macos_package_contract', 2),
                ('baseline', 'needs.select.outputs.run_macos_package_contract', 3),
                ('doc-checker-contracts', 'needs.select.outputs.run_compile_metrics', 1),
                ('doc-checker-contracts', 'needs.select.outputs.run_compile_metrics', 2),
                ('doc-checker-contracts', 'needs.select.outputs.run_compile_metrics', 3),
                ('node', 'needs.select.outputs.run_consensus', 2),
                ('node', 'needs.select.outputs.run_consensus', 3),
                ('scenario-regression', 'needs.select.outputs.run_pixel_world_bridge', 2),
                ('scenario-regression', 'needs.select.outputs.run_pixel_world_bridge', 3),
                ('viewer-js-required', 'needs.select.outputs.run_viewer_performance_report', 2),
                ('viewer-js-required', 'needs.select.outputs.run_viewer_performance_report', 3),
                ('doc-checker-contracts', 'needs.select.outputs.run_packaging_contracts', 3),
                ('scenario-regression', 'needs.select.outputs.run_cargo_tooling_contracts', 3),
                ('fleet-health', 'needs.select.outputs.run_windows_rollout', 3),
            ):
                absorbed = values_for(host, event, selected=False, consolidation=ver)
                absorbed[flag] = 'true'
                self.assertTrue(effective_job_condition(conditions[host], absorbed, ['skipped', 'success']))
            # Direct workspace-support gate skips legacy and join jobs while running workspace-support-other.
            direct_legacy = values_for('workspace-support-legacy', event, selected=True, direct_support=1)
            self.assertFalse(effective_job_condition(conditions['workspace-support-legacy'], direct_legacy, ['skipped', 'success']))
            direct_other = values_for('workspace-support-other', event, selected=True, direct_support=1)
            direct_other['fromJSON(needs.select.outputs.workspace_support_partitions)'] = ['other']
            self.assertTrue(effective_job_condition(conditions['workspace-support-other'], direct_other, ['skipped', 'success']))
            summary_if = re.search(r'^    if: (.*?)(?=^    [a-z-]+:|\Z)', JOBS['workspace-support'], re.M | re.S)[1]
            direct_summary = values_for('workspace-support-legacy', event, selected=True, direct_support=1)
            self.assertFalse(effective_job_condition(summary_if, direct_summary, ['skipped', 'success']))
            failed_select_summary = values_for('workspace-support-legacy', event, selected=False, select='failure', direct_support=1)
            self.assertTrue(effective_job_condition(summary_if, failed_select_summary, ['skipped', 'failure']))

        # An explicit status function also bypasses implicit producer success.
        # Every producer must therefore be checked by the actual Node job guard.
        states = ('success', 'failure', 'cancelled', 'skipped')
        for results in itertools.product(states, repeat=len(producers)):
            values = values_for('custody-node', 'pull_request', results=dict(zip(producers, results)))
            with self.subTest(producers=results):
                self.assertEqual(effective_job_condition(conditions['custody-node'], values,
                                                         ['skipped', 'success', *results]),
                                 all(result == 'success' for result in results))

    def test_periodic_skips_only_no_full_and_keeps_select_failure_visible(self):
        for name in ('formal-process', 'explicit-full-gate'):
            job = JOBS[name]
            self.assertIn('always()', job)
            self.assertIn("needs.select.result != 'skipped'", job)
            self.assertNotIn("needs.select.result == 'success'", job.split('    runs-on:', 1)[0])
        formal_cond = re.search(r'^    if: (.*?)(?=^    [a-z-]+:|\Z)', JOBS['formal-process'], re.M | re.S)[1]
        for select, run_formal, expected in (
            ('success', 'false', False),
            ('success', '', False),
            ('success', 'true', True),
            ('failure', '', True),
            ('cancelled', '', True),
            ('skipped', 'true', False),
        ):
            actual = effective_job_condition(formal_cond, {
                'needs.select.result': select,
                'needs.select.outputs.run_formal_process': run_formal,
                'github.event_name': 'pull_request',
                'inputs.run_mode': '',
            }, [select])
            self.assertEqual(actual, expected, (select, run_formal))
        gate = JOBS['explicit-full-gate']
        self.assertNotIn('periodic-admission', re.search(r'    needs: \[(.*)\]', gate).group(1))
        admission = JOBS['periodic-admission']
        self.assertIn('timeout-minutes: 3', admission)
        self.assertIn('group: rust-periodic-admission-main', admission)
        self.assertIn('cancel-in-progress: false', admission)
        self.assertIn('--github-output "$GITHUB_OUTPUT"', admission)
        self.assertIn('name: ci-periodic-admission-${{ github.run_id }}-${{ github.run_attempt }}', admission)
        self.assertIn('if-no-files-found: error', admission)
        self.assertNotIn('continue-on-error', admission)
        self.assertIn("needs.periodic-admission.result == 'success'", JOBS['select'])

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
        summary = JOBS['workspace-support']
        for before, after in [
                ('custody-node]', ']'),
                ('custody-native-contracts, ', ''),
                ('ci-custody-platform.py validate', 'true'),
                ('pattern: custody-node-${{ github.run_id }}-*', 'pattern: stale-*')]:
            with self.subTest(before=before), self.assertRaises(ValueError):
                validate_candidate_graph(WORKFLOW.replace(summary, summary.replace(before, after)), CONFIG)
        for before, after in [('fail-fast: false', 'fail-fast: true'),
                              ('custody_node_shards', 'unknown_shards'),
                              ('ci-custody-platform.py restore', 'true'),
                              ('npm run test:types', 'true')]:
            with self.subTest(before=before), self.assertRaises(ValueError):
                validate_candidate_graph(WORKFLOW.replace(node, node.replace(before, after)), CONFIG)
        native = JOBS['custody-native-contracts']
        native_needs = ('    needs: [select, fast-preflight]\n'
                        if '    needs: [select, fast-preflight]\n' in native else '    needs: select\n')
        for before, after in [('if: fromJSON(needs.select.outputs.plan).custody_compile_proof_version == 1', 'if: fromJSON(needs.select.outputs.plan).custody_compile_proof_version == 2'),
                              ('ci-custody-platform.py stage', 'true'),
                              ('verify.sh --stage "$stage"', 'true'),
                              ('run_worker_wasm_compile_profiles', 'true'),
                              ('    needs: [select, fast-preflight]\n', '    needs: custody-wasm-production\n')]:
            with self.subTest(before=before), self.assertRaises(ValueError):
                validate_candidate_graph(WORKFLOW.replace(native, native.replace(before, after)), CONFIG)
        for before, after in [('if: fromJSON(needs.select.outputs.plan).custody_policy_tests_version == 1', 'if: fromJSON(needs.select.outputs.plan).custody_policy_tests_version == 2'),
                              ('ci-custody-platform.py policy-tests', 'true'),
                              ('name: ${{ needs.select.outputs.selection_artifact }}', 'name: unrelated'),
                              ('OASIS7_SUPPORT_TESTED_SHA:', 'OMITTED_TESTED_SHA:')]:
            with self.subTest(before=before), self.assertRaisesRegex(ValueError, 'policy'):
                validate_candidate_graph(WORKFLOW.replace(native, native.replace(before, after)), CONFIG)

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
                script = 'set -euo pipefail\n' + scripts_by_component[environment['CUSTODY_COMPONENT']]
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
                selected = [*features, *(['executor,execution-signer-harness', 'identity,execution-signer-harness',
                                         'owner,execution-signer-harness', 'recovery-fence,execution-signer-harness'] if signer else [])]
                expected = ['check -p oasis7_custody_worker --target wasm32-unknown-unknown '
                            '--no-default-features --features ' + feature + ' --locked' for feature in selected]
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
                        self.assertEqual(actual, [] if stage == 'wasm-approval-artifacts' else expected)
                        failed = invoke({'VERIFY_RESULT': '37'})
                        self.assertEqual(failed.returncode, 37)
                        self.assertFalse(capture.exists())
                        if stage != 'wasm-approval-artifacts':
                            failed = invoke({'WORKER_COMPILE_RESULT': '42'})
                            self.assertNotEqual(failed.returncode, 0)
                            self.assertIn('exit status 42', failed.stderr)
                            self.assertEqual(capture.read_text().splitlines(), expected[:1])
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

    def test_partial_fast_builds_and_main_dispatched_candidates_never_write_cache(self):
        cases = (
            ('main-fast', 1, 'push', 'refs/heads/main', True, False),
            ('pr-fast', 1, 'pull_request', 'refs/pull/1/merge', False, False),
            ('periodic-full', 1, 'schedule', 'refs/heads/main', True, True),
            ('candidate-full', 1, 'workflow_dispatch', 'refs/heads/main', True, True),
            ('candidate-full', 1, 'workflow_dispatch', 'refs/heads/main', False, False),
            ('candidate-full', 1, 'workflow_dispatch', 'refs/heads/candidate', False, False),
            ('', 0, 'push', 'refs/heads/main', True, True),
        )
        for profile, version, event, ref, trusted_source, writer_expected in cases:
            values = {
                'always()': True, 'cancelled()': False, 'needs.select.result': 'success',
                'needs.fast-preflight.result': 'skipped',
                'needs.select.outputs.run_oasis7_required': 'true',
                'needs.select.outputs.run_formal_process': 'true',
                'needs.select.outputs.trusted_source': str(trusted_source).lower(),
                'fromJSON(needs.select.outputs.plan).scope': 'full',
                'fromJSON(needs.select.outputs.plan).execution_profile': profile,
                'fromJSON(needs.select.outputs.plan).fast_plan_version': version,
                'fromJSON(needs.select.outputs.plan).ordinary_main_fast_cache_writer_version': 0,
                'fromJSON(needs.select.outputs.plan).fast_execution_version': 0,
                'fromJSON(needs.select.outputs.plan).ordinary_execution': '',
                'github.event_name': event, 'github.ref': ref, 'inputs.run_mode': 'full',
                'github.workflow_sha': 'a', 'github.sha': 'a',
            }
            for reader, writer in (('oasis7-test-build-candidate', 'oasis7-test-build-trusted'),
                                   ('formal-debug-build', 'formal-debug-build-trusted'),
                                   ('formal-endpoint-build', 'formal-endpoint-build-trusted')):
                for name, expected in ((reader, not writer_expected), (writer, writer_expected)):
                    condition = JOBS[name].split('    if:', 1)[1].split('    runs-on:', 1)[0].strip().removeprefix('>-')
                    for token, value in values.items():
                        condition = condition.replace(token, repr(value))
                    condition = re.sub(r'!(?!=)', 'not ', condition.replace('&&', ' and ').replace('||', ' or '))
                    with self.subTest(job=name, profile=profile, trusted=trusted_source):
                        self.assertEqual(eval(' '.join(condition.split()), {'__builtins__': {}}, {}), expected)

    def test_main_push_cache_capability_flows_from_trusted_selection_to_the_dag(self):
        field = 'ordinary_main_fast_cache_writer_version'
        jobs = ('oasis7-test-build-candidate', 'oasis7-test-build-trusted',
                'cache-bootstrap-maintenance', 'cache-maintenance')
        conditions = {name: re.search(r'^    if: (.*?)(?=^    [a-z-]+:|\Z)',
                                      JOBS[name], re.M | re.S)[1] for name in jobs}
        for enabled in (False, True):
            config = {key: value for key, value in CONFIG.items() if key != field}
            if enabled:
                config[field] = 1
            with self.subTest(enabled=enabled), tempfile.TemporaryDirectory() as directory:
                result, outputs, plan, policy, source = self.run_selection(
                    directory, event='push', candidate=True, ref='refs/heads/main', policy_config=config)
                self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                self.assertEqual((plan['scope'], plan['execution_profile']), ('full', 'main-fast'))
                self.assertEqual((plan['policy_sha'], plan['source_sha'], plan['tested_sha']),
                                 (policy, source, source))
                self.assertNotEqual(policy, source)
                self.assertEqual(plan.get(field), 1 if enabled else None)
                self.assertEqual(outputs['trusted_source'], 'true')
                projected = json.loads(outputs['plan'])
                self.assertEqual(projected.get(field), plan.get(field))
                values = {'needs.select.result': 'success', 'github.event_name': 'push',
                          'github.ref': 'refs/heads/main', 'github.sha': source,
                          'github.workflow_sha': source, 'inputs.run_mode': '',
                          'needs.oasis7-test-build-trusted.result': 'success' if enabled else 'skipped',
                          'needs.formal-debug-build-trusted.result': 'skipped',
                          'needs.formal-endpoint-build-trusted.result': 'skipped'}
                values.update({f'needs.select.outputs.{key}': value for key, value in outputs.items()})
                for name in re.findall(r'fromJSON\(needs\.select\.outputs\.plan\)\.([A-Za-z0-9_]+)',
                                       '\n'.join(conditions.values())):
                    values[f'fromJSON(needs.select.outputs.plan).{name}'] = projected.get(name)
                for name, condition in conditions.items():
                    self.assertEqual(effective_job_condition(condition, values, ['skipped', 'success']),
                                     not enabled if name == 'oasis7-test-build-candidate' else enabled, name)
                for maintenance in ('success', 'failure', 'skipped'):
                    self.assertEqual(effective_job_condition(conditions['oasis7-test-build-trusted'], values,
                                                            ['success', maintenance]), enabled)

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
            self.assertIn('./scripts/ci-tests.sh full-extended', JOBS[name])
            self.assertNotIn('full-extended --plan', JOBS[name])
            self.assertNotIn('OASIS7_CI_TEST_BUNDLE', JOBS[name])
            self.assertNotIn('name: native-tests-', JOBS[name])
        center = (ROOT / '.github/workflows/command-center.yml').read_text()
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
        for job in ('full-regression', 'candidate-full-regression'):
            self.assertIn('./scripts/ci-tests.sh full-extended', JOBS[job])
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


class OrdinaryDeliveryWorkflow(unittest.TestCase):
    def steps(self, name):
        return re.findall(r'^      - .*?(?=^      - |\Z)', JOBS[name], re.M | re.S)

    def condition(self, step):
        return re.search(r'^        if: (.*?)(?=^        \w|\Z)', step, re.M | re.S)[1]

    def test_native_consumers_download_exactly_their_closure_and_controls_download_none(self):
        steps = [step for step in self.steps('oasis7-required')
                 if 'name: native-tests-' in step or 'name: Unpack ' in step]
        self.assertEqual(len(steps), 5)
        for delivery, inline, owner in itertools.product((None, 1, 2), (False, True),
                ('controls', 'runtime', 'component-libs', 'integrations-0', 'integrations-1', 'all', 'native-0', 'native-1', 'native-2', 'native-3')):
            if delivery is not None and (inline or owner == 'all'
                    or delivery == 1 and owner.startswith('native-')
                    or delivery == 2 and owner not in ('controls', 'native-0', 'native-1', 'native-2', 'native-3')):
                continue  # Independently rejected by the plan consumer.
            values = {'fromJSON(needs.select.outputs.plan).ordinary_artifact_delivery_version': delivery,
                      'fromJSON(needs.select.outputs.plan).fast_execution_version': 1 if inline else None,
                      'fromJSON(needs.select.outputs.plan).ordinary_execution': 'inline' if inline else None,
                      'matrix.partition': owner}
            active = [step for step in steps if effective_job_condition(self.condition(step), values, [])]
            with self.subTest(delivery=delivery, inline=inline, owner=owner):
                self.assertEqual(len(active), 0 if inline or delivery is not None and owner == 'controls'
                                 else 3 if delivery is not None else 2)
                if delivery is not None and owner != 'controls':
                    self.assertIn('name: native-tests-common-${{ github.sha }}', ''.join(active))
                    self.assertIn('name: native-tests-${{ matrix.partition }}-${{ github.sha }}', ''.join(active))
                    self.assertIn('ci-test-artifacts.py unpack-ordinary --plan "$RUNNER_TEMP/selection.json"', ''.join(active))
                for step in steps:
                    self.assertFalse(effective_job_condition(self.condition(step), values, ['failure']))

    def test_producer_keeps_one_build_fails_closed_and_publishes_all_five_packages(self):
        steps = self.steps('oasis7-test-build-candidate')
        build = next(step for step in steps if 'name: Build ordinary test artifacts' in step)
        script = textwrap.dedent(build.split('        run: |\n', 1)[1])
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            stub = ('python3() { printf "%s\\n" "$*" >> "$CALLS"; '
                    'if [[ "$2" == "$FAIL_ACTION" ]]; then return 17; fi; };\n')
            for version, failed in itertools.product(('0', '1', '2'), ('', 'build', 'pack')):
                calls = root / 'calls'
                calls.unlink(missing_ok=True)
                result = subprocess.run(['bash', '-c', stub + script], text=True, capture_output=True,
                    env=dict(os.environ, RUNNER_TEMP=str(root), ORDINARY_DELIVERY_VERSION=version,
                             CALLS=str(calls), FAIL_ACTION=failed))
                with self.subTest(version=version, failed=failed):
                    self.assertEqual(result.returncode, 17 if failed else 0, result.stderr)
                    lines = calls.read_text().splitlines()
                    self.assertEqual(len(lines), 1 if failed == 'build' else 2)
                    self.assertIn('build --scope ordinary --plan', lines[0])
                    if len(lines) == 2:
                        self.assertEqual('--split-ordinary' in lines[1], version in ('1', '2'))
        uploads = [step for step in steps if 'uses: actions/upload-artifact@' in step and 'name: native-tests-' in step]
        self.assertEqual(len(uploads), 6)
        for version, compact, baseline_controls in (
                (None, False, None), (1, False, None), (1, False, 1),
                (2, False, None), (2, False, 1), (2, True, None), (2, True, 1)):
            native_owners = (
                [] if version is None
                else ['runtime', 'component-libs', 'integrations-0', 'integrations-1'] if version == 1
                else ['native-0', 'native-1'] if compact
                else ['native-0', 'native-1', 'native-2', 'native-3']
            )
            owners = (['all'] if version is None
                      else native_owners if baseline_controls == 1
                      else ['controls', *native_owners])
            partition_values = {
                'fromJSON(needs.select.outputs.plan).ordinary_artifact_delivery_version': version,
                'fromJSON(needs.select.outputs.plan).ordinary_controls_in_rust_baseline_version': baseline_controls,
                'fromJSON(needs.select.outputs.ordinary_partitions)[0]': owners[0] if len(owners) > 0 else None,
                'fromJSON(needs.select.outputs.ordinary_partitions)[1]': owners[1] if len(owners) > 1 else None,
                'fromJSON(needs.select.outputs.ordinary_partitions)[2]': owners[2] if len(owners) > 2 else None,
                'fromJSON(needs.select.outputs.ordinary_partitions)[3]': owners[3] if len(owners) > 3 else None,
                'fromJSON(needs.select.outputs.ordinary_partitions)[4]': owners[4] if len(owners) > 4 else None,
            }
            selected = [step for step in uploads if effective_job_condition(self.condition(step), partition_values, [])]
            self.assertEqual(len(selected), 1 if version is None else 1 + len(native_owners))
            for step in selected:
                for required in ('compression-level: 0', 'overwrite: true', 'if-no-files-found: error'):
                    self.assertIn(required, step)
            if version is not None:
                resolved = '\n'.join(selected)
                for index, owner in enumerate(native_owners):
                    expression = (
                        '${{ fromJSON(needs.select.outputs.plan).ordinary_controls_in_rust_baseline_version == 1 && '
                        f'fromJSON(needs.select.outputs.ordinary_partitions)[{index}] || '
                        f'fromJSON(needs.select.outputs.ordinary_partitions)[{index + 1}] }}}}'
                    )
                    self.assertEqual(resolved.count(expression), 2)
                    resolved = resolved.replace(expression, owner)
                self.assertEqual(set(re.findall(r'/ordinary-archives/(.*)\.tar\.gz', resolved)),
                                 {'common', *native_owners})
                self.assertEqual(set(re.findall(r'name: native-tests-(.*)-\$\{\{ github.sha \}\}', resolved)),
                                 {'common', *native_owners})

    def test_retired_ordinary_delivery_preview_is_absent(self):
        self.assertNotIn('pr-ordinary-delivery-preview', JOBS)
        self.assertNotIn('pr-ordinary-delivery-preview', WORKFLOW)


class BrowserDownloadCache(unittest.TestCase):
    def test_readers_preserve_exact_caches_locked_tools_and_real_consumers(self):
        action = (ROOT / '.github/actions/setup-ci-browser/action.yml').read_text()
        steps = re.findall(r'^    - .*?(?=^    - |\Z)', action, re.M | re.S)
        restores = [step for step in steps if 'uses: actions/cache/restore@' in step]
        self.assertEqual(len(restores), 3)
        for component in ('fonts_noto_cjk', 'fonts_freefont_ttf', 'chrome'):
            restore = next(step for step in restores if f'steps.downloads.outputs.{component}_root' in step)
            self.assertIn(f'key: ${{{{ steps.downloads.outputs.{component}_key }}}}', restore)
            self.assertNotIn('restore-keys:', restore)
            if component != 'fonts_freefont_ttf':
                self.assertIn("if: inputs.browser == 'agent'", restore)
        self.assertNotIn('actions/cache/save@', action)
        self.assertNotIn('GH_TOKEN', action)
        for job in ('viewer-js-required', 'viewer-performance-report', 'site-quality'):
            self.assertIn('uses: ./.github/actions/setup-ci-browser', JOBS[job])
            self.assertIn("node-version: '24'", JOBS[job])
            self.assertNotIn('cache-mode: write', JOBS[job])
        lock = json.loads((ROOT / 'scripts/ci-browser-tools/package-lock.json').read_text())
        for name, version in (('agent-browser', '0.43.0'), ('@agent-browser/cli-linux-x64-gnu', '0.43.0'),
                              ('playwright', '1.62.1'), ('playwright-core', '1.62.1')):
            entry = lock['packages']['node_modules/' + name]
            self.assertEqual(entry['version'], version)
            self.assertTrue(entry['integrity'].startswith('sha512-'))
        helper = (ROOT / 'scripts/ci-browser-deps.py').read_text()
        self.assertIn("'--ignore-scripts'", helper)
        self.assertIn('AGENT_BROWSER_EXECUTABLE_PATH', helper)
        self.assertIn('PLAYWRIGHT_BROWSERS_PATH', helper)

    def test_archive_writer_rejects_foreign_identity_unbounded_restore_and_unadmitted_save(self):
        writer = portable_cache_host_layout(workflow_jobs((ROOT / '.github/workflows/rust-cache.yml').read_text())['browser-archive-cache'], 'browser')
        browser_archive_cache_writer(writer)
        mutations = [
            ("github.ref == 'refs/heads/main'", 'true'),
            ('github.workflow_sha == github.sha', 'true'),
            ("inputs.run_mode == 'full'", 'true'),
            ('ci-browser-deps.py admit-save', 'ci-browser-deps.py metadata'),
        ]
        for prefix, slug in (('fonts_noto_cjk', 'noto'), ('fonts_freefont_ttf', 'freefont'), ('chrome', 'chrome')):
            mutations.extend([
                (f'key: ${{{{ steps.browser-meta.outputs.{prefix}_key }}}}', 'key: unbound'),
                (f'path: ${{{{ steps.browser-meta.outputs.{prefix}_root }}}}', 'path: ${{ github.workspace }}'),
                (f"steps.prepare-browser-{slug}.outcome == 'success'", 'true'),
                (f"steps.browser-admission-{slug}.outputs.save == 'true'", 'true'),
            ])
        for before, after in mutations:
            with self.subTest(before=before), self.assertRaises(AssertionError):
                browser_archive_cache_writer(writer.replace(before, after, 1))
        with self.assertRaises(AssertionError):
            browser_archive_cache_writer(writer.replace('          key:', '          restore-keys: ci-browser-\n          key:', 1))

    def test_shared_inputs_keep_governance_and_all_three_browser_consumers(self):
        for path in ('scripts/ci-browser-deps.py', '.github/actions/setup-ci-browser/action.yml'):
            self.assertEqual(set(CONFIG['scoped_control_paths'][path]),
                             {'workflow_governance', 'viewer_js_required', 'viewer_performance_report', 'site_quality'})
        for path, test in (('scripts/ci-browser-deps.py', 'scripts/ci-browser-deps.test.py'),
                           ('scripts/ci-tool-cache.py', 'scripts/ci-tool-cache.test.py')):
            self.assertIn(test, CONFIG['python_control_contracts'][path]['tests'])
            self.assertIn(test, CONFIG['workflow_governance_python_tests'])
        self.assertIn('scripts/ci-worker-build.test.py',
                      CONFIG['python_control_contracts']['scripts/ci-tool-cache.py']['tests'])


class WorkerToolCache(unittest.TestCase):
    def test_legacy_reader_covers_actual_worker_selection_and_keeps_other_partitions_cold(self):
        job = JOBS['workspace-support-legacy']
        steps = re.findall(r'^      - .*?(?=^      - |\Z)', job, re.M | re.S)
        tool_steps = [step for step in steps if 'ci-worker-build.py' in step or 'id: worker-tool-cache' in step]
        self.assertEqual(len(tool_steps), 3)
        identify, restore, prepare = tool_steps
        condition = re.search(r'^        if: (.*?)(?=^        [a-z-]+:|\Z)', identify, re.M | re.S)[1]
        for partition in ('all', 'custody', 'other'):
            values = {'matrix.partition': partition}
            with self.subTest(partition=partition):
                self.assertEqual(effective_job_condition(condition, values, ['success']), partition != 'other')
                self.assertFalse(effective_job_condition(condition, values, ['failure']))
        writer = portable_cache_host_layout(workflow_jobs((ROOT / '.github/workflows/rust-cache.yml').read_text())['worker-build-cache'], 'worker')
        expected_key = re.search(r'^          key: (worker-build-cli-v1-.*)$', writer, re.M)[1]
        self.assertIn('key: ' + expected_key, restore)
        self.assertIn('path: ${{ steps.worker-meta.outputs.root }}', restore)
        self.assertIn('uses: actions/cache/restore@55cc8345863c7cc4c66a329aec7e433d2d1c52a9', restore)
        self.assertNotIn('restore-keys:', restore)
        self.assertNotIn('actions/cache/save@', job)
        self.assertNotIn('GH_TOKEN:', job)
        for step in tool_steps:
            self.assertNotIn('continue-on-error', step)
        for step in (restore, prepare):
            condition = re.search(r'^        if: (.*)$', step, re.M)[1]
            for root in ('', '/fixture/worker-cache'):
                values = {'steps.worker-meta.outputs.root': root}
                self.assertEqual(effective_job_condition(condition, values, ['success']), bool(root))
                self.assertFalse(effective_job_condition(condition, values, ['failure']))
        self.assertLess(job.index('Install pinned Rust toolchains'), job.index('Identify pinned Worker build tool'))
        self.assertLess(job.index('Prepare verified Worker build tool'), job.index('Execute selected group'))
        self.assertIn('--support-plan-file "$RUNNER_TEMP/selection.json" --support-partition "$SUPPORT_PARTITION"', job)

    def test_legacy_metadata_preserves_old_source_and_rejects_existing_broken_helpers(self):
        script = workflow_step_script(JOBS['workspace-support-legacy'], 'Identify pinned Worker build tool')
        for kind, expected in (('missing', 0), ('regular', 0), ('failure', 17),
                               ('empty', 1), ('directory', 1), ('symlink', 1), ('dangling', 1)):
            with self.subTest(kind=kind), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                helper = root / 'scripts/ci-worker-build.py'
                helper.parent.mkdir()
                output = root / 'metadata-output'
                if kind in ('regular', 'failure'):
                    helper.write_text('import os, pathlib, sys\n'
                        'assert sys.argv[1:] == ["metadata", "--github-output", os.environ["GITHUB_OUTPUT"]]\n'
                        + ('sys.exit(17)\n' if kind == 'failure' else
                           'pathlib.Path(os.environ["GITHUB_OUTPUT"]).write_text("root=/fixture/worker-cache\\n")\n'))
                elif kind == 'empty':
                    helper.touch()
                elif kind == 'directory':
                    helper.mkdir()
                elif kind in ('symlink', 'dangling'):
                    target = root / 'other.py'
                    if kind == 'symlink':
                        target.write_text('raise SystemExit("must not execute")\n')
                    helper.symlink_to(target)
                # Read the full downloaded plan, not the compact DAG projection:
                # full and pre-profile BASE plans expand the driver inventory.
                for scope, version, profiles, selected in (
                        ('targeted', 1, ['oasis7_custody_worker'], True),
                        ('targeted', 1, ['oasis7_context'], False),
                        ('targeted', None, [], True),
                        ('full', 1, ['oasis7_context'], True),
                        ('full', None, [], True)):
                    plan = dict(scope=scope, workspace_support_profiles=profiles)
                    if version is not None:
                        plan['workspace_support_profile_version'] = version
                    (root / 'selection.json').write_text(json.dumps(plan))
                    output.unlink(missing_ok=True)
                    result = subprocess.run(['bash', '-euo', 'pipefail', '-c', script], cwd=root,
                        env=dict(os.environ, GITHUB_OUTPUT=str(output), RUNNER_TEMP=str(root)),
                        text=True, capture_output=True)
                    with self.subTest(scope=scope, version=version, profiles=profiles):
                        self.assertEqual(result.returncode, expected if selected else 0,
                                         result.stdout + result.stderr)
                        self.assertEqual(output.exists(), kind == 'regular' and selected)

    def test_signer_image_requires_an_exact_current_targeted_plan(self):
        job = JOBS['workspace-support-other']
        if "matrix.execution == 'signer-image'" not in job:
            # The original two-lane layout remains covered by the image matrix helper.
            self.assertEqual(len(re.findall(r"shared-key: ci-rust-build-", job)), 1)
            return
        self.assertIsNotNone(workspace_support_image_matrix(job))
        self.assertIn("matrix.execution == 'signer-image' && '--user 1001:1001'", job)
        self.assertIn("format('{0}/oasis7-node-mold', runner.temp)", job)
        self.assertIn('SUPPORT_EXECUTION=host', job)
        self.assertIn('test "$(id -u):$(id -g)" = 1001:1001', job)
        script = workflow_step_script(JOBS['select'], 'Publish workflow scheduling projection')
        modern = dict(
            scope='targeted', groups=['workspace_support'], execution_profile='pr-fast', fast_plan_version=1,
            workspace_support_direct_gate_version=1, workspace_support_partition_version=1,
            workspace_support_partitions=['other'], workspace_support_profile_version=1,
            workspace_support_profiles=['oasis7_local_signer'], test_policy_changed=False,
        )
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            output = root / 'output'
            for overrides, expected in (
                ({}, True),
                ({'scope': 'full'}, False),
                ({'groups': []}, False),
                ({'workspace_support_profiles': ['oasis7_game_client', 'oasis7_local_signer']}, False),
                ({'workspace_support_profiles': ['oasis7_local_signer', 'oasis7_context']}, False),
                ({'workspace_support_profile_version': True}, False),
                ({'test_policy_changed': True}, False),
                ({'workspace_support_profile_version': None}, False),
            ):
                plan = {key: value for key, value in (modern | overrides).items() if value is not None}
                (root / 'selection.json').write_text(json.dumps(plan))
                output.unlink(missing_ok=True)
                subprocess.run(['bash', '-euo', 'pipefail', '-c', script],
                               env=dict(os.environ, RUNNER_TEMP=tmp, GITHUB_OUTPUT=str(output)), check=True)
                self.assertIn('workspace_support_signer_only=' + str(expected).lower(), output.read_text())

    def test_signer_context_projection_and_artifact_require_the_same_exact_pair(self):
        job = JOBS['workspace-support-other']
        if 'workspace_support_signer_context' not in job:
            return
        project = workflow_step_script(JOBS['select'], 'Publish workflow scheduling projection')
        verify = workflow_step_script(job, 'Verify the non-root signer execution plan')
        pair = ['oasis7_context', 'oasis7_local_signer']
        plan = dict(
            scope='targeted', groups=['workspace_support'], execution_profile='pr-fast',
            fast_plan_version=1, workspace_support_direct_gate_version=2,
            workspace_support_partition_version=1, workspace_support_partitions=['other'],
            workspace_support_profile_version=1, workspace_support_profiles=pair,
            test_policy_changed=False,
        )
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            output = root / 'output'
            artifact = root / 'selection.json'
            fixture_id = root / 'id'
            fixture_id.write_text('#!/bin/sh\nprintf "1001\\n"\n')
            fixture_id.chmod(0o755)
            protocol_pair = ['oasis7_local_signer', 'oasis7_signer_protocol']
            protocol_triple = ['oasis7_context', *protocol_pair]
            has_protocol_lane = 'workspace_support_signer_protocol' in job
            projection_cases = [
                ({}, True, False),
                ({'scope': 'explicit-full'}, False, False),
                ({'workspace_support_direct_gate_version': None}, False, False),
                ({'test_policy_changed': True}, False, False),
                ({'workspace_support_profiles': [*pair, 'oasis7_dev_test_worker']}, False, False),
                ({'workspace_support_profiles': [*pair, 'oasis7_game_client']}, False, False),
            ]
            if has_protocol_lane:
                projection_cases.extend([
                    ({'workspace_support_profiles': protocol_pair}, False, True),
                    ({'workspace_support_profiles': protocol_triple}, False, True),
                    ({'workspace_support_profiles': ['oasis7_game_client', *protocol_pair]}, False, False),
                    ({'workspace_support_profiles': protocol_triple, 'test_policy_changed': True}, False, False),
                ])
            for overrides, selected, protocol_selected in projection_cases:
                with self.subTest(overrides=overrides):
                    candidate = {key: value for key, value in (plan | overrides).items() if value is not None}
                    artifact.write_text(json.dumps(candidate))
                    output.unlink(missing_ok=True)
                    subprocess.run(['bash', '-euo', 'pipefail', '-c', project],
                                   env=dict(os.environ, RUNNER_TEMP=tmp, GITHUB_OUTPUT=str(output)), check=True)
                    self.assertIn('workspace_support_signer_context=' + str(selected).lower(), output.read_text())
                    self.assertIn('workspace_support_signer_only=false', output.read_text())
                    if has_protocol_lane:
                        self.assertIn('workspace_support_signer_protocol=' + str(protocol_selected).lower(),
                                      output.read_text())
            artifact_cases = [
                (pair, 'true', False, 0), (pair, 'false', False, 1),
                (pair, 'true', True, 1), (pair, '', False, 1),
                (['oasis7_local_signer'], 'false', False, 0),
                (['oasis7_local_signer'], 'true', False, 1),
                ([*pair, 'oasis7_dev_test_worker'], 'true', False, 1),
            ]
            if has_protocol_lane:
                artifact_cases.extend([
                    (protocol_pair, 'protocol', False, 0), (protocol_triple, 'protocol', False, 0),
                    (protocol_triple, 'protocol', True, 1), (protocol_pair, 'true', False, 1),
                    (['oasis7_game_client', *protocol_pair], 'protocol', False, 1),
                ])
            for profiles, pair_flag, corrupt, expected in artifact_cases:
                with self.subTest(profiles=profiles, pair_flag=pair_flag, corrupt=corrupt):
                    artifact.write_text(json.dumps(dict(plan, workspace_support_profiles=profiles)))
                    digest = hashlib.sha256(artifact.read_bytes()).hexdigest()
                    if 'SIGNER_EXECUTION:' in job:
                        lane = {'false': 'signer-image', 'true': 'signer-context-image',
                                'protocol': 'signer-protocol-image', '': ''}[pair_flag]
                        selector = {'SIGNER_EXECUTION': lane}
                    else:
                        selector = {'SIGNER_WITH_CONTEXT': pair_flag}
                    result = subprocess.run(
                        ['bash', '-euo', 'pipefail', '-c', verify],
                        env=dict(os.environ, RUNNER_TEMP=tmp, PATH=f"{tmp}:{os.environ['PATH']}",
                                 SELECTION_SHA256='0' * 64 if corrupt else digest,
                                 **selector),
                        text=True, capture_output=True,
                    )
                    self.assertEqual(result.returncode, expected, result.stderr)

    def test_support_group_driver_preserves_original_filter_for_each_lane(self):
        job = JOBS['workspace-support-other']
        script = workflow_step_script(job, 'Execute selected group')
        lanes = [('host', 'host'), ('image', 'image')]
        if "matrix.execution == 'signer-image'" in job:
            lanes.append(('signer-image', 'host'))
        if 'workspace_support_signer_context' in job:
            lanes.append(('signer-context-image', 'all'))
        if 'workspace_support_signer_protocol' in job:
            lanes.append(('signer-protocol-image', 'all'))
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / 'scripts').mkdir()
            (root / 'scripts/ci-tests.sh').write_text(
                "support_capacity_snapshot() { :; }\n"
                "if [[ $# -gt 0 ]]; then printf '%s\\n' \"$@\"; exit \"${FIXTURE_DRIVER_EXIT:-0}\"; fi\n")
            for lane, expected in lanes:
                with self.subTest(lane=lane):
                    result = subprocess.run(
                        ['bash', '-euo', 'pipefail', '-c', script], cwd=root,
                        env=dict(os.environ, RUNNER_TEMP=tmp, GITHUB_WORKSPACE=tmp,
                                 SUPPORT_PARTITION='other', SUPPORT_EXECUTION=lane),
                        text=True, capture_output=True,
                    )
                    self.assertEqual(result.returncode, 0, result.stderr)
                    self.assertEqual(result.stdout.splitlines(), [
                        'required', '--group', 'workspace_support', '--repo-root', tmp,
                        '--support-plan-file', str(root / 'selection.json'),
                        '--support-partition', 'other', '--support-execution', expected,
                    ])
            if 'workspace_support_signer_context' in job:
                failed = subprocess.run(
                    ['bash', '-euo', 'pipefail', '-c', script], cwd=root,
                    env=dict(os.environ, RUNNER_TEMP=tmp, GITHUB_WORKSPACE=tmp,
                             SUPPORT_PARTITION='other', SUPPORT_EXECUTION='signer-context-image',
                             FIXTURE_DRIVER_EXIT='37'),
                    text=True, capture_output=True,
                )
                self.assertEqual(failed.returncode, 37, failed.stderr)

    def test_image_support_exact_readonly_worker_tool_reader(self):
        job = JOBS['workspace-support-other']
        steps = re.findall(r'^      - .*?(?=^      - |\Z)', job, re.M | re.S)
        selected = [step for step in steps if ('Identify pinned image Worker build tool' in step
                    or 'id: worker-image-cache' in step
                    or 'Prepare verified image Worker build tool' in step)]
        self.assertEqual(len(selected), 3)
        identity, restore, verify = selected
        self.assertIn("if: matrix.execution == 'image'", identity)
        for text in ('selection.json', 'workspace_support_profile_version',
                     'oasis7_dev_test_worker', 'helper.is_symlink()',
                     'os.execv(sys.executable'):
            self.assertIn(text, identity)
        writer = portable_cache_host_layout(workflow_jobs((ROOT / '.github/workflows/rust-cache.yml').read_text())['worker-build-cache'], 'worker')
        worker_tool_cache_writer(writer)
        key = re.search(r'^          key: (worker-build-cli-v1-.*)$', writer, re.M)[1]
        self.assertIn(key.replace('steps.worker-meta.outputs', 'steps.worker-image-meta.outputs'), restore)
        self.assertIn('steps.worker-image-meta.outputs.root', restore)
        self.assertNotIn('restore-keys:', restore)
        self.assertNotIn('actions/cache/save@', job)
        self.assertNotIn('GH_TOKEN:', job)
        for step in selected:
            self.assertNotIn('continue-on-error:', step)
        for step in (restore, verify):
            expr = re.search(r'^        if: (.*)$', step, re.M)[1]
            for mode in ('host', 'image'):
                for root in ('', '/fixture/tool'):
                    values = {'matrix.execution': mode, 'steps.worker-image-meta.outputs.root': root}
                    self.assertEqual(effective_job_condition(expr, values, ['success']),
                                     mode == 'image' and bool(root))
                    self.assertFalse(effective_job_condition(expr, values, ['failure']))
        self.assertIn('python3 scripts/ci-worker-build.py check', verify)
        self.assertIn('python3 scripts/ci-worker-build.py ensure', verify)
        self.assertIn('check --root "$WORKER_TOOL_ROOT"', verify)
        self.assertIn('ensure --root "$WORKER_TOOL_ROOT"', verify)
        self.assertIn('WORKER_TOOL_ROOT: ${{ steps.worker-image-meta.outputs.restored_root }}', verify)
        self.assertIn('timeout-minutes: 11', verify)
        execute = workflow_step_script(job, 'Execute selected group')
        self.assertIn('export PATH="$WORKER_TOOL_ROOT/bin:$PATH"', execute)
        self.assertLess(job.index('Prepare verified image Worker build tool'),
                        job.index('Execute selected group'))

    def test_image_worker_bindgen_cache_matches_writer_and_passes_verified_path(self):
        job = JOBS['workspace-support-other']
        writer = JOBS['full-regression']
        key = re.search(r'^          key: (wasm-bindgen-cli-v1-.*)$', writer, re.M)[1]
        self.assertIn(key.replace('steps.bindgen-meta.outputs', 'steps.worker-bindgen-meta.outputs'), job)
        self.assertIn("if: matrix.execution == 'image' && steps.worker-image-meta.outputs.root != ''", job)
        self.assertIn('path: ${{ steps.worker-bindgen-meta.outputs.root }}', job)
        self.assertIn('WORKER_WASM_BINDGEN_BIN: ${{ steps.worker-bindgen.outputs.bin }}', job)
        execute = workflow_step_script(job, 'Execute selected group')
        self.assertIn('export WASM_BINDGEN_BIN="$WORKER_WASM_BINDGEN_BIN"', execute)
        self.assertNotIn('actions/cache/save@', job)
        prepare = workflow_step_script(job, 'Prepare verified image Worker wasm-bindgen')
        with tempfile.TemporaryDirectory() as tmp:
            cache = Path(tmp)
            env = dict(os.environ, XDG_CACHE_HOME=tmp, GITHUB_OUTPUT=str(cache / 'output'),
                       BINDGEN_CACHE_HIT='true', WASM_BINDGEN_BIN='')
            metadata = subprocess.check_output(
                ['bash', '-c', 'source scripts/wasm-bindgen-cli-common.sh; '
                 'printf "%s\\n%s\\n" "$WASM_BINDGEN_CACHE_ROOT" "$WASM_BINDGEN_LOCK_VERSION"'],
                cwd=ROOT, env=env, text=True,
            ).splitlines()
            cli_root, version = metadata
            bindgen = Path(cli_root) / 'bin' / 'wasm-bindgen'
            bindgen.parent.mkdir(parents=True)
            bindgen.write_text(f'#!/bin/bash\necho "wasm-bindgen {version}"\n')
            bindgen.chmod(0o755)
            result = subprocess.run(['bash', '-euo', 'pipefail', '-c', prepare], cwd=ROOT, env=env,
                                    text=True, capture_output=True)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual((cache / 'output').read_text(), f'bin={bindgen}\n')
            bindgen.write_text('#!/bin/bash\necho "wasm-bindgen wrong-version"\n')
            result = subprocess.run(['bash', '-euo', 'pipefail', '-c', prepare], cwd=ROOT, env=env,
                                    text=True, capture_output=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertEqual((cache / 'output').read_text(), f'bin={bindgen}\n')

    @unittest.skipUnless(sys.platform == "linux", "Linux cache paths reject macOS /home symlink")
    def test_image_worker_cache_identity_matches_real_host_metadata(self):
        helper = ROOT / 'scripts/ci-worker-build.py'
        environment = dict(os.environ)
        environment.pop('XDG_CACHE_HOME', None)
        host = subprocess.run([sys.executable, str(helper), 'metadata'],
            env=dict(environment, HOME='/home/runner'), text=True,
            capture_output=True, check=True)
        script = workflow_step_script(JOBS['workspace-support-other'],
                                      'Identify pinned image Worker build tool')
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            (root / 'selection.json').write_text(json.dumps(dict(
                scope='targeted', workspace_support_profile_version=1,
                workspace_support_profiles=['oasis7_dev_test_worker'])))
            output = root / 'output'
            result = subprocess.run(['bash', '-euo', 'pipefail', '-c', script],
                cwd=ROOT, env=dict(environment, HOME='/github/home',
                    XDG_CACHE_HOME='/github/home/.cache',
                    RUNNER_TEMP=str(root), GITHUB_WORKSPACE='/__w/oasis7/oasis7', GITHUB_OUTPUT=str(output)),
                text=True, capture_output=True)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            image_metadata = dict(line.split('=', 1)
                                  for line in output.read_text().splitlines())
            host_metadata = json.loads(host.stdout)
            for field in ('root', 'bin', 'version', 'definition_sha'):
                self.assertEqual(image_metadata[field], host_metadata[field])

    def test_worker_archive_relative_names_restore_at_the_consumed_root(self):
        # Same layout and ../../../ member names as the host writer; extraction
        # uses the container checkout. A matching cache key does not remap tar.
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            host = root / 'host/home/runner'
            writer = host / 'work/oasis7/oasis7'
            source = host / '.cache/oasis7/worker-build/0.7.5'
            writer.mkdir(parents=True)
            (source / 'bin').mkdir(parents=True)
            (source / 'bin/worker-build').write_text('archive payload')
            container = root / 'container'
            reader = container / '__w/oasis7/oasis7'
            reader.mkdir(parents=True)
            archive = root / 'cache.tar'
            member = os.path.relpath(source, writer)
            self.assertEqual(member, '../../../.cache/oasis7/worker-build/0.7.5')
            subprocess.run(['tar', '-cPf', str(archive), '-C', str(writer), member], check=True)
            subprocess.run(['tar', '-xPf', str(archive), '-C', str(reader)], check=True)
            actual = container / '.cache/oasis7/worker-build/0.7.5'
            self.assertEqual((actual / 'bin/worker-build').read_text(), 'archive payload')
            self.assertFalse((container / 'home/runner/.cache/oasis7/worker-build/0.7.5').exists())
            job = JOBS['workspace-support-other']
            self.assertEqual(job.count('WORKER_TOOL_ROOT: ${{ steps.worker-image-meta.outputs.restored_root }}'), 2)
            script = workflow_step_script(job, 'Identify pinned image Worker build tool')
            self.assertIn('os.path.normpath(os.path.join(', script)
            self.assertIn("writer_workspace = '/home/runner/work/oasis7/oasis7'", script)
            self.assertIn("os.environ['GITHUB_WORKSPACE'] != '/__w/oasis7/oasis7'", script)
            (root / 'scripts').mkdir()
            (root / 'scripts/ci-worker-build.py').write_text(
                'import os\nwith open(os.environ["GITHUB_OUTPUT"], "a") as output:\n'
                '    output.write("root=/home/runner/.cache/oasis7/worker-build/0.7.5\\n")\n')
            (root / 'selection.json').write_text('{"scope":"full"}')
            output = root / 'metadata'
            environment = dict(os.environ, RUNNER_TEMP=str(root),
                               GITHUB_WORKSPACE='/__w/oasis7/oasis7', GITHUB_OUTPUT=str(output))
            result = subprocess.run(['bash', '-euo', 'pipefail', '-c', script],
                                    cwd=root, env=environment, text=True, capture_output=True)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            metadata = dict(line.split('=', 1) for line in output.read_text().splitlines())
            self.assertEqual(metadata['root'], '/home/runner/.cache/oasis7/worker-build/0.7.5')
            self.assertEqual(container / metadata['restored_root'].lstrip('/'), actual)
            output.unlink()
            result = subprocess.run(['bash', '-euo', 'pipefail', '-c', script],
                cwd=root, env=environment | {'GITHUB_WORKSPACE': '/unexpected/layout'},
                text=True, capture_output=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertFalse(output.exists())

    def test_image_support_worker_metadata_cold_and_selected(self):
        script = workflow_step_script(JOBS['workspace-support-other'],
                                      'Identify pinned image Worker build tool')
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            helper = root / 'scripts/ci-worker-build.py'
            helper.parent.mkdir()
            output = root / 'output'
            helper.write_text('import os, pathlib, sys\n'
                'assert sys.argv[1:] == ["metadata", "--root", "/home/runner/.cache/oasis7/worker-build/0.7.5", "--github-output", os.environ["GITHUB_OUTPUT"]]\n'
                'pathlib.Path(os.environ["GITHUB_OUTPUT"]).write_text("root=/fixture/tool\\n")\n')
            for profile, selected in (('oasis7_dev_test_worker', True),
                                      ('oasis7_context', False)):
                (root / 'selection.json').write_text(json.dumps(dict(
                    scope='targeted', workspace_support_profile_version=1,
                    workspace_support_profiles=[profile])))
                output.unlink(missing_ok=True)
                result = subprocess.run(['bash', '-euo', 'pipefail', '-c', script],
                    cwd=root, env=dict(os.environ, GITHUB_OUTPUT=str(output),
                                       RUNNER_TEMP=str(root), GITHUB_WORKSPACE='/__w/oasis7/oasis7'), capture_output=True, text=True)
                self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                self.assertEqual(output.exists(), selected)
            helper.unlink()
            helper.symlink_to(root / 'missing')
            (root / 'selection.json').write_text(json.dumps(dict(
                scope='full', workspace_support_profile_version=1,
                workspace_support_profiles=['oasis7_context'])))
            result = subprocess.run(['bash', '-euo', 'pipefail', '-c', script],
                cwd=root, env=dict(os.environ, GITHUB_OUTPUT=str(output),
                                   RUNNER_TEMP=str(root), GITHUB_WORKSPACE='/__w/oasis7/oasis7'), capture_output=True, text=True)
            self.assertNotEqual(result.returncode, 0)

    def test_exact_readers_and_sole_trusted_writer(self):
        cache_jobs = workflow_jobs((ROOT / '.github/workflows/rust-cache.yml').read_text())
        writer = cache_jobs['worker-build-cache']
        worker_tool_cache_writer(writer)
        expected_key = re.search(r'^          key: (worker-build-cli-v1-.*)$', writer, re.M)[1]
        for name in ('custody-native', 'custody-wasm-production'):
            job = JOBS[name]
            steps = re.findall(r'^      - .*?(?=^      - |\Z)', job, re.M | re.S)
            tool_steps = [step for step in steps if 'ci-worker-build.py' in step or 'id: worker-tool-cache' in step]
            self.assertEqual(len(tool_steps), 3)
            for step in tool_steps:
                self.assertIn("if: env.CUSTODY_COMPONENT != 'native'", step)
                self.assertNotIn('continue-on-error', step)
            restore = next(step for step in tool_steps if 'id: worker-tool-cache' in step)
            self.assertIn('key: ' + expected_key, restore)
            self.assertNotIn('restore-keys:', restore)
            self.assertNotIn('actions/cache/save@', job)
            self.assertNotIn('GH_TOKEN:', job)
        wasm = JOBS['custody-wasm-production']
        if rust_image_cache_namespace('custody-wasm-production', wasm):
            full = JOBS['full-regression']
            bindgen_key = re.search(r'^          key: (wasm-bindgen-cli-v1-.*)$', full, re.M)[1]
            self.assertIn(bindgen_key.replace('steps.bindgen-meta.outputs', 'steps.custody-bindgen-meta.outputs'), wasm)
            self.assertEqual(wasm.count('WASM_BINDGEN_BIN: ${{ steps.custody-bindgen.outputs.bin }}'), 2)
            self.assertIn('XDG_CACHE_HOME: /.cache', wasm)
            self.assertIn('check --root "$restored"', wasm)
            self.assertIn('check --root "$runtime_root"', wasm)
            self.assertIn('test ! -e "$runtime_root"\n            test ! -L "$runtime_root"', wasm)
        self.assertEqual(CONFIG['python_control_contracts']['scripts/ci-worker-build.py']['group'], 'workflow_governance')
        self.assertIn('scripts/ci-worker-build.test.py', CONFIG['python_control_contracts']['scripts/ci-worker-build.py']['tests'])
        self.assertIn('scripts/ci-worker-build.test.py', CONFIG['workflow_governance_python_tests'])

    def test_custody_wasm_worker_rehomes_only_verified_cache_into_empty_runtime(self):
        job = JOBS['custody-wasm-production']
        if '    container:' not in job:
            return
        step = workflow_step_script(job, 'Prepare verified Worker build tool')
        step = step.replace('restored=/.cache/oasis7/worker-build/0.7.5', 'restored="$FIXTURE_ARCHIVE_ROOT"')
        for version, occupied, should_move in (('0.7.5', False, True),
                                                ('wrong-version', False, False),
                                                ('0.7.5', True, False)):
            with self.subTest(version=version, occupied=occupied):
                with tempfile.TemporaryDirectory() as temporary:
                    root = Path(temporary).resolve()
                    archive = root / 'archived-worker'
                    cache = root / 'runtime-cache'
                    runtime = cache / 'oasis7/worker-build/0.7.5'
                    binary = archive / 'bin/worker-build'
                    binary.parent.mkdir(parents=True)
                    binary.write_text(f'#!/bin/bash\nprintf "%s\\n" "{version}"\n')
                    binary.chmod(0o755)
                    if occupied:
                        runtime.mkdir(parents=True)
                        (runtime / 'sentinel').write_text('untouched')
                    result = subprocess.run(['bash', '-euo', 'pipefail', '-c', step], cwd=ROOT,
                        env=dict(os.environ, TOOL_CACHE_HIT='true', XDG_CACHE_HOME=str(cache),
                                 FIXTURE_ARCHIVE_ROOT=str(archive)), text=True, capture_output=True)
                    self.assertEqual(result.returncode == 0, should_move, result.stderr)
                    self.assertEqual((runtime / 'bin/worker-build').is_file(), should_move)
                    self.assertEqual(archive.is_dir(), not should_move)
                    if occupied:
                        self.assertEqual((runtime / 'sentinel').read_text(), 'untouched')

    def test_writer_rejects_foreign_identity_unbounded_keys_and_unadmitted_save(self):
        writer = portable_cache_host_layout(workflow_jobs((ROOT / '.github/workflows/rust-cache.yml').read_text())['worker-build-cache'], 'worker')
        for before, after in [
                ("github.ref == 'refs/heads/main'", 'true'),
                ('github.workflow_sha == github.sha', 'true'),
                ("inputs.run_mode == 'full'", 'true'),
                ('worker-build-cli-v1-', 'unbound-tool-'),
                ('          path: ${{ steps.worker-meta.outputs.root }}', '          path: ${{ github.workspace }}'),
                ("steps.worker-tool-admission.outputs.save == 'true'", 'true'),
                ("steps.prepare-worker.outcome == 'success'", 'true'),
                ('ci-worker-build.py admit-save', 'ci-worker-build.py metadata')]:
            with self.subTest(before=before), self.assertRaises(AssertionError):
                worker_tool_cache_writer(writer.replace(before, after))
        with self.assertRaises(AssertionError):
            worker_tool_cache_writer(writer.replace('          key: worker-build-cli-v1-',
                '          restore-keys: worker-build-cli-\n          key: worker-build-cli-v1-', 1))

    def test_prepare_hit_miss_and_real_failure_propagation(self):
        writer = portable_cache_host_layout(workflow_jobs((ROOT / '.github/workflows/rust-cache.yml').read_text())['worker-build-cache'], 'worker')
        for name, job in [('writer', writer), ('legacy', JOBS['workspace-support-legacy']),
                          ('production', JOBS['custody-wasm-production'])]:
            step = next(step for step in re.findall(r'^      - .*?(?=^      - |\Z)', job, re.M | re.S)
                        if 'name: Prepare verified Worker build tool' in step)
            self.assertIn('timeout-minutes: 11', step)
            script = textwrap.dedent(step.split('        run: |\n', 1)[1])
            for hit, command in [('true', 'check'), ('false', 'ensure'), ('', 'ensure')]:
                for code in (0, 19):
                    with self.subTest(job=name, hit=hit, code=code), tempfile.TemporaryDirectory() as temporary:
                        root = Path(temporary).resolve()
                        archive = root / 'archive'
                        archive.mkdir()
                        isolated = script.replace('restored=/.cache/oasis7/worker-build/0.7.5',
                                                  'restored="$FIXTURE_ARCHIVE_ROOT"')
                        stub = """python3() {
                          if [[ "$1" == scripts/ci-worker-build.py && "$2" != metadata ]]; then
                            printf 'called:%s\\n' "$*"
                            return "$CODE"
                          fi
                          command python3 "$@"
                        };
"""
                        result = subprocess.run(['bash', '-euo', 'pipefail', '-c', stub + isolated + '\necho reached-build'],
                            env=dict(os.environ, TOOL_CACHE_HIT=hit, CODE=str(code), XDG_CACHE_HOME=str(root / 'cache'),
                                     FIXTURE_ARCHIVE_ROOT=str(archive)), text=True, capture_output=True)
                        self.assertEqual(result.returncode, code, result.stderr)
                        self.assertIn('called:scripts/ci-worker-build.py ' + command, result.stdout)
                        self.assertEqual('reached-build' in result.stdout, code == 0)


class RustGovernanceReportContract(unittest.TestCase):
    def test_governance_report_uses_real_entry_point(self):
        result = subprocess.run(
            ['bash', str(ROOT / 'scripts/ci-rust-governance-report.test.sh')],
            cwd=ROOT, capture_output=True, text=True, timeout=30,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_ci_tests_warning_zero_contract(self):
        result = subprocess.run(
            ['bash', str(ROOT / 'scripts/ci-tests-warning-zero-contract.test.sh')],
            cwd=ROOT, capture_output=True, text=True, timeout=30,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)


class HeavyPrJobTimeoutContractTests(unittest.TestCase):
    def test_heavy_pr_jobs_declare_bounded_fallback_timeout_minutes(self):
        expected_limits = {
            '.github/workflows/rust.yml': {
                'oasis7-test-build-candidate': 40,
                'oasis7-test-build-trusted': 45,
                'formal-debug-build': 35,
                'formal-debug-build-trusted': 35,
                'formal-endpoint-build': 35,
                'formal-endpoint-build-trusted': 35,
                'formal-test-build': 20,
                'formal-test-build-trusted': 20,
                'formal-process-tests': 45,
                'workspace-support-legacy': 60,
                'workspace-support-other': 35,
                'operational-contracts': 12,
            },
            '.github/workflows/wasm-determinism-gate.yml': {
                'collect-wasm-summaries': 22,
            },
        }
        for rel_path, limits in expected_limits.items():
            jobs = workflow_jobs((ROOT / rel_path).read_text())
            for job_name, max_minutes in limits.items():
                with self.subTest(workflow=rel_path, job=job_name):
                    self.assertIn(job_name, jobs)
                    match = re.search(r'^    timeout-minutes:\s*(\d+)\s*$', jobs[job_name], re.M)
                    self.assertIsNotNone(match, f'{rel_path}:{job_name} must declare explicit timeout-minutes')
                    actual = int(match.group(1))
                    self.assertGreater(actual, 0)
                    self.assertLessEqual(actual, max_minutes)

        required_job = workflow_jobs((ROOT / '.github/workflows/rust.yml').read_text())['oasis7-required']
        expr_match = re.search(r'^    timeout-minutes:\s*(\$\{\{.+?\}\})\s*$', required_job, re.M)
        self.assertIsNotNone(expr_match, '.github/workflows/rust.yml:oasis7-required must declare partition-bounded timeout-minutes')
        condition, heavy_raw, fast_raw = re.fullmatch(
            r'\$\{\{\s*(.+?)\s*&&\s*(\d+)\s*\|\|\s*(\d+)\s*\}\}',
            expr_match.group(1),
        ).groups()
        heavy_minutes, fast_minutes = int(heavy_raw), int(fast_raw)
        self.assertGreater(fast_minutes, 0)
        self.assertLessEqual(fast_minutes, 25)
        self.assertGreater(heavy_minutes, fast_minutes)
        self.assertLessEqual(heavy_minutes, 50)
        heavy_list = re.search(r"fromJSON\('([^']+)'\)", condition)[1]
        for execution, owner in itertools.product(
            (None, 'inline'),
            ('controls', 'all', 'runtime', 'component-libs', 'integrations-0', 'integrations-1',
             'native-0', 'native-1', 'native-2', 'native-3'),
        ):
            values = {
                'fromJSON(needs.select.outputs.plan).ordinary_execution': execution,
                f"fromJSON('{heavy_list}')": json.loads(heavy_list),
                'matrix.partition': owner,
            }
            expect_heavy = execution == 'inline' or owner in ('all', 'component-libs', 'runtime')
            with self.subTest(execution=execution, owner=owner):
                self.assertEqual(effective_job_condition(condition, values, []), expect_heavy)


class PriorityRunnerRoutingContractTests(unittest.TestCase):
    def test_ci_priority_label_routes_same_repo_pr_required_jobs_only(self):
        expected_expr = (
            "runs-on: ${{ github.event_name == 'pull_request' && "
            "github.event.pull_request.head.repo.full_name == github.repository && "
            "contains(github.event.pull_request.labels.*.name, 'ci-priority') && "
            "'oasis7-priority-linux' || 'ubuntu-24.04' }}"
        )
        priority_jobs = {
            'select', 'baseline', 'rust-baseline',
            'oasis7-test-build-candidate',
            'formal-debug-build', 'formal-endpoint-build', 'formal-test-build',
            'oasis7-required', 'formal-process-tests', 'formal-process',
            'consensus', 'distfs', 'node', 'net',
            'viewer-js-required', 'viewer-performance-report', 'pixel-world-bridge',
            'launcher-web', 'workspace-support-other',
            'custody-native-contracts', 'custody-native',
            'custody-wasm-production',
            'custody-node', 'workspace-support',
            'scenario-regression',
            'workflow-governance', 'codex-agent-config-validation',
            'compile-metrics', 'site-quality', 'doc-checker-contracts',
            'cargo-tooling-contracts', 'macos-package-contract', 'required-gate',
        }
        excluded_jobs = {
            'operational-contracts', 'packaging-contracts',
            'pr-fast-preview-plan', 'pr-fast-preview',
            'pr-governance-preview',
            'periodic-admission', 'cache-bootstrap-maintenance', 'cache-maintenance',
            'oasis7-test-build-trusted', 'formal-debug-build-trusted',
            'formal-endpoint-build-trusted', 'formal-test-build-trusted',
            'full-regression', 'candidate-full-regression', 'explicit-full-gate',
            'workspace-support-legacy',
        }
        for name in priority_jobs:
            with self.subTest(priority_job=name):
                self.assertIn(expected_expr, JOBS[name])
        for name in excluded_jobs:
            with self.subTest(excluded_job=name):
                self.assertNotIn('oasis7-priority-linux', JOBS[name])

    def test_draft_pr_fail_closed_gate_and_main_fast_cancelled_push_range_recovery(self):
        pr_header = RAW_WORKFLOW.split('  pull_request:\n', 1)[1].split('  schedule:\n', 1)[0]
        self.assertIn('types: [opened, synchronize, reopened, ready_for_review, labeled]', pr_header)
        concurrency = RAW_WORKFLOW.split('concurrency:\n', 1)[1].split('permissions:\n', 1)[0]
        self.assertIn(
            "(github.event_name == 'pull_request' && github.event.action == 'labeled' && "
            "(github.event.label.name != 'ci-priority' || "
            "github.event.pull_request.head.repo.full_name != github.repository || "
            "github.event.pull_request.draft)) && format('pr-label-noop-{0}', github.run_id)",
            concurrency,
        )
        self.assertEqual(CONFIG.get('main_fast_range_recovery_version'), 1)
        self.assertEqual(CONFIG.get('main_fast_coalesce_concurrency_version'), 1)
        self.assertEqual(CONFIG.get('draft_pr_heavy_ci_skip_version'), 1)
        self.assertEqual(CONFIG.get('auxiliary_workflow_consolidation_version'), 1)
        self.assertEqual(CONFIG.get('workspace_member_cargo_manifest_scope_version'), 2)
        self.assertEqual(CONFIG.get('ordinary_controls_in_rust_baseline_version'), 1)
        self.assertEqual(CONFIG.get('contract_job_consolidation_version'), 5)
        self.assertEqual(CONFIG.get('workspace_support_direct_gate_version'), 2)
        self.assertEqual(CONFIG.get('ordinary_scope_config_slice_version'), 2)
        self.assertEqual(CONFIG.get('scoped_production_cfg_version'), 1)

        gate = JOBS['required-gate']
        self.assertIn(
            "if: >-\n"
            "      always() && ((github.event_name == 'push' &&\n"
            "        (github.ref != 'refs/heads/main' || needs.select.result != 'skipped')) ||\n"
            "      (github.event_name == 'pull_request' &&\n"
            "       (github.event.action != 'labeled' ||\n"
            "        (github.event.label.name == 'ci-priority' &&\n"
            "         github.event.pull_request.head.repo.full_name == github.repository &&\n"
            "         !github.event.pull_request.draft))))",
            gate,
        )
        self.assertIn(
            "(github.event.action != 'labeled' ||\n"
            "         (github.event.label.name == 'ci-priority' &&\n"
            "          github.event.pull_request.head.repo.full_name == github.repository))",
            JOBS['select'],
        )
        self.assertIn(
            "IS_DRAFT_PR: ${{ github.event_name == 'pull_request' && github.event.pull_request.draft && 'true' || 'false' }}",
            gate,
        )
        gate_step = workflow_step_script(gate, 'Require successful scope planning')
        for select_result in ('skipped', 'success', 'failure'):
            draft_res = subprocess.run(
                ['bash', '-euo', 'pipefail', '-c', gate_step],
                env=dict(os.environ, IS_DRAFT_PR='true', SELECT_RESULT=select_result),
                text=True, capture_output=True,
            )
            self.assertNotEqual(draft_res.returncode, 0)
            self.assertIn('Draft PR skipped main CI jobs', draft_res.stdout + draft_res.stderr)
        ready_ok = subprocess.run(
            ['bash', '-euo', 'pipefail', '-c', gate_step],
            env=dict(os.environ, IS_DRAFT_PR='false', SELECT_RESULT='success'),
            text=True, capture_output=True,
        )
        self.assertEqual(ready_ok.returncode, 0, ready_ok.stdout + ready_ok.stderr)

        select_job = JOBS['select']
        self.assertIn('permissions:\n      contents: read\n      actions: read', select_job)
        self.assertIn(
            "MAIN_FAST_BASE_TOKEN: ${{ github.event_name == 'push' && github.ref == 'refs/heads/main' && github.token || '' }}",
            select_job,
        )
        with tempfile.TemporaryDirectory() as directory:
            repo = Path(directory) / 'repo'
            repo.mkdir()
            def git(*args):
                return subprocess.check_output(['git', '-C', str(repo), *args], text=True).strip()
            git('init', '-q', '-b', 'main')
            git('config', 'user.name', 'Fixture')
            git('config', 'user.email', 'fixture@example.invalid')
            (repo / 'scripts').mkdir()
            for name in ('ci-required-scope.json', 'ci-required-result.py', 'ci-formal-plan.py',
                         'ci-formal-process-suites.json'):
                (repo / 'scripts' / name).write_bytes((ROOT / 'scripts' / name).read_bytes())
            (repo / 'scripts/plan-rust-required-scope.py').write_text('''import argparse, json, pathlib, subprocess
p = argparse.ArgumentParser()
for key in ('profile', 'base-ref', 'head-ref', 'test-ref', 'policy-ref', 'formal-config', 'github-output', 'output'):
    p.add_argument('--' + key)
a = p.parse_args()
diff_files = subprocess.check_output(['git', 'diff', '--name-only', a.base_ref, a.head_ref], text=True).splitlines()
groups = ['baseline', 'oasis7_required'] if 'rust_touch.txt' in diff_files else ['baseline']
plan = {'scope': 'targeted', 'execution_profile': a.profile, 'fast_plan_version': 1,
        'check_inventory_version': 1, 'groups': groups,
        'source_sha': a.head_ref, 'tested_sha': a.test_ref, 'policy_sha': a.policy_ref}
pathlib.Path(a.output).write_text(json.dumps(plan))
with pathlib.Path(a.github_output).open('a') as f:
    for g in groups:
        f.write(f'run_{g}=true\\n')
''')
            (repo / 'README.md').write_text('m0\n')
            git('add', '.')
            git('commit', '-qm', 'm0')
            m0 = git('rev-parse', 'HEAD')
            # m1 changes rust_touch.txt AND installs a deterministic resolve-main-fast-base stub in trusted policy.
            (repo / 'rust_touch.txt').write_text('changed in cancelled m1\n')
            (repo / 'scripts/ci-full-result.py').write_text(f'''import argparse, os, sys
p = argparse.ArgumentParser()
sub = p.add_subparsers(dest='command', required=True)
r = sub.add_parser('resolve-main-fast-base')
for k in ('repo-root', 'before', 'head'):
    r.add_argument('--' + k, required=True)
a = p.parse_args()
assert os.environ.get('MAIN_FAST_BASE_TOKEN') == 'push-read-token'
print({m0!r})
''')
            git('add', '.')
            git('commit', '-qm', 'm1')
            m1 = git('rev-parse', 'HEAD')
            # m2 only touches README.md; without range recovery m1..m2 would miss rust_touch.txt!
            (repo / 'README.md').write_text('m2 doc-only\n')
            git('add', '.')
            git('commit', '-qm', 'm2')
            m2 = git('rev-parse', 'HEAD')

            script = workflow_step_script(select_job, 'Select using trusted baseline Git data')
            out_file = Path(directory) / 'github-output'
            env = dict(
                os.environ, EVENT_NAME='push', SOURCE_REF='', SOURCE_SHA=m2, BASE_SHA=m1,
                WORKFLOW_SHA=m2, GITHUB_SHA=m2, GITHUB_REF='refs/heads/main',
                GITHUB_WORKSPACE=str(repo), GITHUB_REPOSITORY='nil-studio/oasis7',
                MAIN_FAST_BASE_TOKEN='push-read-token',
                RUNNER_TEMP=str(directory), GITHUB_OUTPUT=str(out_file),
                GITHUB_STEP_SUMMARY=str(Path(directory) / 'summary'),
            )
            res = subprocess.run(['bash', '-c', script], cwd=repo, env=env, text=True, capture_output=True)
            self.assertEqual(res.returncode, 0, res.stdout + res.stderr)
            outputs = dict(line.split('=', 1) for line in out_file.read_text().splitlines())
            self.assertEqual(outputs['policy_sha'], m1)
            self.assertEqual(outputs['range_base'], m0)
            self.assertEqual(outputs['run_oasis7_required'], 'true')
            plan = json.loads((Path(directory) / 'selection.json').read_text())
            self.assertEqual((plan['policy_sha'], plan['source_sha'], plan['tested_sha']), (m1, m2, m2))
            self.assertIn('oasis7_required', plan['groups'])

    def test_noop_label_runs_never_publish_skipped_required_gate_or_cancel_preview(self):
        gate = JOBS['required-gate']
        name_match = re.search(r'^    name:\s*(.+?)\s*$', gate, re.M)
        self.assertIsNotNone(name_match, 'required-gate must declare an explicit name')
        raw_name = name_match.group(1)
        # GitHub treats a skipped required check as passing; when if: skips
        # required-gate on unrelated labels (e.g. merge:managed, bug), neither
        # the unexpanded expression nor its evaluated result may equal 'required-gate'.
        self.assertNotEqual(raw_name, 'required-gate')
        self.assertRegex(raw_name, r'^\$\{\{.+\}\}$')
        self.assertLessEqual(len(raw_name), 255)

        if_match = re.search(r'^    if:\s*(.*?)(?=^    [a-z-]+:|\Z)', gate, re.M | re.S)
        self.assertIsNotNone(if_match)
        if_expr = if_match.group(1).strip().removeprefix('>-').strip()
        name_expr = raw_name.removeprefix('${{').removesuffix('}}').strip()

        def eval_gh_expr(expr, *, event_name, action='', label='', same_repo=True, draft=False, ref='refs/heads/main', select_result='success'):
            py_expr = ' '.join(expr.split())
            replacements = {
                'always()': 'True',
                'github.event_name': repr(event_name),
                'github.ref': repr(ref),
                'needs.select.result': repr(select_result),
                'github.event.action': repr(action),
                'github.event.label.name': repr(label),
                'github.event.pull_request.head.repo.full_name': repr('nil-studio/oasis7' if same_repo else 'fork/oasis7'),
                'github.repository': repr('nil-studio/oasis7'),
                'github.event.pull_request.draft': repr(draft),
            }
            for token in sorted(replacements, key=len, reverse=True):
                py_expr = py_expr.replace(token, replacements[token])
            py_expr = re.sub(r'!(?!=)', 'not ', py_expr.replace('&&', ' and ').replace('||', ' or '))
            return eval(py_expr, {'__builtins__': {}}, {})

        cases = (
            # Executed gate runs must publish exact CheckRun name 'required-gate'.
            ('push', '', '', True, False, True, 'required-gate'),
            ('pull_request', 'opened', '', True, False, True, 'required-gate'),
            ('pull_request', 'synchronize', '', False, False, True, 'required-gate'),
            ('pull_request', 'reopened', '', True, False, True, 'required-gate'),
            ('pull_request', 'ready_for_review', '', True, False, True, 'required-gate'),
            ('pull_request', 'opened', '', True, True, True, 'required-gate'),
            ('pull_request', 'labeled', 'ci-priority', True, False, True, 'required-gate'),
            # Skipped no-op label runs must never publish 'required-gate'.
            ('pull_request', 'labeled', 'merge:managed', True, False, False, 'required-gate-label-noop'),
            ('pull_request', 'labeled', 'bug', True, False, False, 'required-gate-label-noop'),
            ('pull_request', 'labeled', 'ci-fast-preview', True, False, False, 'required-gate-label-noop'),
            ('pull_request', 'labeled', 'ci-priority', False, False, False, 'required-gate-label-noop'),
            ('pull_request', 'labeled', 'ci-priority', True, True, False, 'required-gate-label-noop'),
        )
        for event_name, action, label, same_repo, draft, expected_run, expected_name in cases:
            with self.subTest(event=event_name, action=action, label=label, same_repo=same_repo, draft=draft):
                actual_run = bool(eval_gh_expr(
                    if_expr, event_name=event_name, action=action,
                    label=label, same_repo=same_repo, draft=draft,
                ))
                actual_name = eval_gh_expr(
                    name_expr, event_name=event_name, action=action,
                    label=label, same_repo=same_repo, draft=draft,
                )
                self.assertEqual(actual_run, expected_run)
                self.assertEqual(actual_name, expected_name)
                if not actual_run:
                    self.assertNotEqual(actual_name, 'required-gate')
                    self.assertNotEqual(raw_name, 'required-gate')
        # Cooldown-deferred main push skips select and must also skip required-gate.
        self.assertFalse(bool(eval_gh_expr(
            if_expr, event_name='push', ref='refs/heads/main', select_result='skipped',
        )))

        preview_workflow = (ROOT / '.github/workflows/pr-fast-preview.yml').read_text()
        preview_concurrency = preview_workflow.split('concurrency:\n', 1)[1].split('permissions:\n', 1)[0]
        group_match = re.search(r'^  group:\s*(.+?)\s*$', preview_concurrency, re.M)
        self.assertIsNotNone(group_match)
        group_suffix_expr = group_match.group(1).removeprefix('${{ github.workflow }}-${{').removesuffix('}}').strip()
        for label, draft, expected_group in (
            ('ci-fast-preview', False, 'pr-6630'),
            ('merge:managed', False, 'pr-label-noop-999'),
            ('bug', False, 'pr-label-noop-999'),
            ('ci-fast-preview', True, 'pr-label-noop-999'),
        ):
            py_group = group_suffix_expr
            for token, val in {
                'github.event.label.name': repr(label),
                'github.event.pull_request.draft': repr(draft),
                "format('pr-{0}', github.event.pull_request.number)": repr('pr-6630'),
                "format('pr-label-noop-{0}', github.run_id)": repr('pr-label-noop-999'),
            }.items():
                py_group = py_group.replace(token, val)
            py_group = re.sub(r'!(?!=)', 'not ', py_group.replace('&&', ' and ').replace('||', ' or '))
            with self.subTest(preview_label=label, draft=draft):
                self.assertEqual(eval(py_group, {'__builtins__': {}}, {}), expected_group)

    def test_fast_preflight_and_main_fast_admission_contracts(self):
        admission = JOBS['main-fast-admission']
        self.assertIn("if: github.event_name == 'push' && github.ref == 'refs/heads/main'", admission)
        self.assertIn('fetch-depth: 0', admission)
        self.assertIn('scripts/ci-full-result.py admit-main-fast --head "$GITHUB_SHA"', admission)
        self.assertIn('ci-main-fast-deferred-${{ github.run_id }}-${{ github.run_attempt }}', admission)
        self.assertIn('ci-main-fast-admission-${{ github.run_id }}-${{ github.run_attempt }}', admission)

        preflight = JOBS['fast-preflight']
        self.assertIn('needs: select', preflight)
        self.assertIn("github.event_name == 'pull_request'", preflight)
        self.assertIn('cargo +"$RUST_TOOLCHAIN" fmt --all -- --check', preflight)
        self.assertNotIn('fast-preflight', JOBS['required-gate'].split('    steps:', 1)[0])
        self.assertEqual(
            re.search(r'^\s+image:\s*(\S+)\s*$', preflight, re.M).group(1),
            re.search(r'^\s+image:\s*(\S+)\s*$', JOBS['rust-baseline'], re.M).group(1),
            'fast-preflight and rust-baseline must share the same container image when skipping duplicate rustfmt',
        )

        for heavy in ('rust-baseline', 'oasis7-test-build-candidate', 'workspace-support-other',
                      'custody-native-contracts', 'custody-native', 'custody-wasm-production',
                      'distfs', 'node', 'net', 'launcher-web', 'scenario-regression'):
            job = JOBS[heavy]
            self.assertIn('needs: [select, fast-preflight]', job)
            self.assertIn("(needs.fast-preflight.result == 'success' || needs.fast-preflight.result == 'skipped')", job)
            cond = re.search(r'^    if: (.*?)(?=^    [a-z-]+:|\Z)', job, re.M | re.S).group(1)
            base_vals = {
                'needs.select.result': 'success',
                'needs.select.outputs.run_rust_baseline': 'true',
                'needs.select.outputs.run_oasis7_required': 'true',
                'needs.select.outputs.run_distfs': 'true',
                'needs.select.outputs.run_node': 'true',
                'needs.select.outputs.run_consensus': 'false',
                'needs.select.outputs.run_net': 'true',
                'needs.select.outputs.run_launcher_web': 'true',
                'needs.select.outputs.run_scenario_regression': 'true',
                'needs.select.outputs.run_pixel_world_bridge': 'false',
                'needs.select.outputs.run_cargo_tooling_contracts': 'false',
                'needs.select.outputs.trusted_source': 'false',
                'needs.select.outputs.workspace_support_partitions': ['other'],
                'fromJSON(needs.select.outputs.workspace_support_partitions)': ['other'],
                'fromJSON(needs.select.outputs.plan).scope': 'targeted',
                'fromJSON(needs.select.outputs.plan).execution_profile': 'pr-fast',
                'fromJSON(needs.select.outputs.plan).fast_plan_version': 1,
                'fromJSON(needs.select.outputs.plan).fast_execution_version': 0,
                'fromJSON(needs.select.outputs.plan).ordinary_execution': '',
                'fromJSON(needs.select.outputs.plan).ordinary_main_fast_cache_writer_version': 0,
                'fromJSON(needs.select.outputs.plan).custody_execution_version': 1,
                'fromJSON(needs.select.outputs.plan).workspace_support_direct_gate_version': 2,
                'fromJSON(needs.select.outputs.plan).contract_job_consolidation_version': 5,
                'github.event_name': 'pull_request',
                'github.ref': 'refs/pull/1/merge',
                'github.workflow_sha': 'a',
                'github.sha': 'b',
                'inputs.run_mode': '',
            }
            self.assertFalse(effective_job_condition(
                cond, dict(base_vals, **{'needs.fast-preflight.result': 'failure'}), ('success', 'failure'),
            ), f'{heavy} must skip when fast-preflight fails')
            self.assertTrue(effective_job_condition(
                cond, dict(base_vals, **{'needs.fast-preflight.result': 'success'}), ('success', 'success'),
            ), f'{heavy} must run when fast-preflight succeeds')
            self.assertTrue(effective_job_condition(
                cond, dict(base_vals, **{'needs.fast-preflight.result': 'skipped'}), ('success', 'skipped'),
            ), f'{heavy} must run when fast-preflight is skipped')

        for host_job, overrides in (
            ('node', {'needs.select.outputs.run_node': 'false', 'needs.select.outputs.run_consensus': 'true'}),
            ('scenario-regression', {
                'needs.select.outputs.run_scenario_regression': 'false',
                'needs.select.outputs.run_pixel_world_bridge': 'true',
            }),
            ('scenario-regression', {
                'needs.select.outputs.run_scenario_regression': 'false',
                'needs.select.outputs.run_cargo_tooling_contracts': 'true',
            }),
        ):
            cond = re.search(r'^    if: (.*?)(?=^    [a-z-]+:|\Z)', JOBS[host_job], re.M | re.S).group(1)
            vals = dict(base_vals, **overrides)
            self.assertFalse(effective_job_condition(
                cond, dict(vals, **{'needs.fast-preflight.result': 'failure'}), ('success', 'failure'),
            ), f'{host_job} consolidated trigger must skip when fast-preflight fails')
            self.assertTrue(effective_job_condition(
                cond, dict(vals, **{'needs.fast-preflight.result': 'success'}), ('success', 'success'),
            ), f'{host_job} consolidated trigger must run when fast-preflight succeeds')
            self.assertTrue(effective_job_condition(
                cond, dict(vals, **{'needs.fast-preflight.result': 'skipped'}), ('success', 'skipped'),
            ), f'{host_job} consolidated trigger must run when fast-preflight is skipped')

        import importlib.util
        spec = importlib.util.spec_from_file_location('ci_required_result', ROOT / 'scripts/ci-required-result.py')
        required_result = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(required_result)
        plan = {'scope': 'targeted', 'contract_job_consolidation_version': 5, 'groups': ['baseline']}
        group_jobs = required_result.configured_group_jobs(CONFIG, plan)
        retired_jobs = required_result.retired_group_jobs(CONFIG, plan)
        for selected_group in ('rust_baseline', 'distfs', 'node', 'consensus', 'net',
                               'launcher_web', 'scenario_regression', 'pixel_world_bridge',
                               'cargo_tooling_contracts'):
            plan_with_group = dict(plan, groups=['baseline', selected_group])
            needs = {'select': {'result': 'success'}}
            for grp, job_name in group_jobs.items():
                needs[job_name] = {'result': 'skipped'}
            needs['baseline'] = {'result': 'success'}
            for rj in retired_jobs:
                needs[rj] = {'result': 'skipped'}
            with self.subTest(skipped_group=selected_group), self.assertRaisesRegex(
                ValueError, rf'{selected_group}: unexpected result \'skipped\'',
            ):
                required_result.validate(plan_with_group, needs, group_jobs, retired_jobs=retired_jobs)

        inline_job = JOBS['oasis7-required']
        self.assertIn('needs: [select, fast-preflight, oasis7-test-build-candidate, oasis7-test-build-trusted]', inline_job)
        inline_cond = re.search(r'^    if: (.*?)(?=^    [a-z-]+:|\Z)', inline_job, re.M | re.S).group(1)
        inline_vals = {
            'needs.select.result': 'success',
            'needs.select.outputs.run_oasis7_required': 'true',
            'needs.oasis7-test-build-candidate.result': 'skipped',
            'needs.oasis7-test-build-trusted.result': 'skipped',
            'fromJSON(needs.select.outputs.plan).scope': 'targeted',
            'fromJSON(needs.select.outputs.plan).fast_execution_version': 1,
            'fromJSON(needs.select.outputs.plan).ordinary_execution': 'inline',
        }
        self.assertFalse(effective_job_condition(
            inline_cond, dict(inline_vals, **{'needs.fast-preflight.result': 'failure'}),
            ('success', 'failure', 'skipped', 'skipped'),
        ), 'inline oasis7-required must skip when fast-preflight fails')
        self.assertTrue(effective_job_condition(
            inline_cond, dict(inline_vals, **{'needs.fast-preflight.result': 'success'}),
            ('success', 'success', 'skipped', 'skipped'),
        ), 'inline oasis7-required must run when fast-preflight succeeds')
        self.assertTrue(effective_job_condition(
            inline_cond, dict(inline_vals, **{'needs.fast-preflight.result': 'skipped'}),
            ('success', 'skipped', 'skipped', 'skipped'),
        ), 'inline oasis7-required must run when fast-preflight is skipped')

        skip_fmt_match = re.search(
            r'OASIS7_SKIP_RUSTFMT:\s*\$\{\{\s*(.+?)\s*\}\}',
            JOBS['rust-baseline'],
        )
        self.assertIsNotNone(skip_fmt_match, 'rust-baseline must wire OASIS7_SKIP_RUSTFMT')
        skip_expr_raw = skip_fmt_match.group(1)
        for event_name, preflight_res, scope, expected_flag in (
            ('pull_request', 'success', 'targeted', '1'),
            ('pull_request', 'skipped', 'targeted', '0'),
            ('pull_request', 'success', 'full', '0'),
            ('push', 'skipped', 'targeted', '0'),
            ('schedule', 'skipped', 'explicit-full', '0'),
            ('workflow_dispatch', 'skipped', 'explicit-full', '0'),
        ):
            py_expr = skip_expr_raw
            for token, val in {
                'github.event_name': repr(event_name),
                'needs.fast-preflight.result': repr(preflight_res),
                'fromJSON(needs.select.outputs.plan).scope': repr(scope),
            }.items():
                py_expr = py_expr.replace(token, val)
            py_expr = py_expr.replace('&&', ' and ').replace('||', ' or ').replace('!=', ' != ').replace('==', ' == ')
            with self.subTest(event_name=event_name, preflight=preflight_res, scope=scope):
                self.assertEqual(eval(py_expr, {'__builtins__': {}}, {}), expected_flag)

        driver = (ROOT / 'scripts/ci-tests.sh').read_text()
        body = dict(re.findall(r'^(\w+)\(\) \{\n(.*?)^\}', driver, re.M | re.S))['run_rust_baseline']
        script = (
            'set -euo pipefail\ntier=required; required_plan=""\n'
            'run() { printf "run:%s\\n" "$*"; }\n'
            'run_cargo() { printf "cargo:%s\\n" "$*"; }\n'
            'run_rustsec_advisory_check() { echo advisories; }\n'
            'run_oasis7_required_clippy_for_group() { echo clippy; }\n'
            'run_newapi_bridge_service_accounting_tests() { echo newapi; }\n'
            'run_standalone_tool_lockfiles_checks() { echo lockfiles; }\n'
            'run_rust_baseline() {\n' + body + '}\nrun_rust_baseline\n'
        )
        for env_override, expect_fmt in (
            ({}, True),
            ({'OASIS7_SKIP_RUSTFMT': '0'}, True),
            ({'OASIS7_SKIP_RUSTFMT': '1'}, False),
        ):
            env = dict(os.environ)
            env.pop('OASIS7_SKIP_RUSTFMT', None)
            env.update(env_override)
            res = subprocess.run(['bash', '-c', script], text=True, capture_output=True, env=env, check=True)
            lines = res.stdout.splitlines()
            self.assertEqual(
                'run:env -u RUSTC_WRAPPER cargo fmt --all -- --check' in lines,
                expect_fmt,
                (env_override, lines),
            )
            self.assertIn('cargo:fetch --locked', lines)
            self.assertIn('advisories', lines)
            self.assertIn('clippy', lines)


class AllWorkflowJobsTimeoutGuardTests(unittest.TestCase):
    def test_every_executable_workflow_job_declares_timeout_minutes(self):
        workflow_dir = ROOT / '.github/workflows'
        workflow_paths = sorted(
            path for path in workflow_dir.iterdir()
            if path.is_file() and path.suffix in ('.yml', '.yaml')
        )
        self.assertGreater(len(workflow_paths), 0)
        checked_jobs = 0
        for path in workflow_paths:
            rel_path = path.relative_to(ROOT).as_posix()
            jobs = workflow_jobs(path.read_text())
            self.assertGreater(len(jobs), 0, f'{rel_path} must declare at least one job')
            for job_name, body in jobs.items():
                if re.search(r'^    uses:\s*\S+', body, re.M):
                    continue
                checked_jobs += 1
                with self.subTest(workflow=rel_path, job=job_name):
                    match = re.search(r'^    timeout-minutes:\s*(.+?)\s*$', body, re.M)
                    self.assertIsNotNone(
                        match,
                        f'{rel_path}:{job_name} must declare explicit timeout-minutes',
                    )
                    value = match.group(1)
                    if value.isdigit():
                        minutes = int(value)
                        self.assertGreater(minutes, 0)
                        self.assertLessEqual(minutes, 360)
                    else:
                        self.assertRegex(value, r'^\$\{\{.+\}\}$')
        self.assertGreater(checked_jobs, 0)

    def test_consolidation_v4_runs_operational_contracts_in_workflow_governance_without_governance_overhead(self):
        gov_job = JOBS['workflow-governance']
        op_job = JOBS['operational-contracts']
        gov_cond = re.search(r'^    if: (.*?)(?=^    [a-z-]+:|\Z)', gov_job, re.M | re.S)[1]
        op_cond = re.search(r'^    if: (.*?)(?=^    [a-z-]+:|\Z)', op_job, re.M | re.S)[1]
        steps = re.findall(r'^      - .*?(?=^      - |\Z)', gov_job, re.M | re.S)
        op_exec = next(s for s in steps if 'name: Execute consolidated operational contracts group\n' in s)
        op_tools = next(s for s in steps if 'name: Verify preinstalled operational contract tools\n' in s)
        parser_step = next(s for s in steps if 'name: Install document contract parser\n' in s)
        authority_step = next(s for s in steps if 'name: Download trusted control authority\n' in s)

        def step_cond(step):
            return re.search(r'^        if: (.*?)(?=^        \w|\Z)', step, re.M | re.S)[1]

        for version, run_gov, run_op, scope in (
            (3, 'false', 'true', 'targeted'),
            (4, 'false', 'true', 'targeted'),
            (4, 'true', 'false', 'targeted'),
            (4, 'true', 'true', 'targeted'),
            (4, 'false', 'false', 'full'),
        ):
            vals = {
                'needs.select.result': 'success',
                'needs.select.outputs.run_workflow_governance': run_gov,
                'needs.select.outputs.run_operational_contracts': run_op,
                'fromJSON(needs.select.outputs.plan).contract_job_consolidation_version': version,
                'fromJSON(needs.select.outputs.plan).scope': scope,
                'fromJSON(needs.select.outputs.plan).python_control_contract_version': 1,
                'steps.control-mode.outputs.complete_governance': 'true',
            }
            with self.subTest(version=version, run_gov=run_gov, run_op=run_op, scope=scope):
                expect_legacy_op = version != 4 and (run_op == 'true' or scope == 'full')
                expect_gov_job = (run_gov == 'true' or scope == 'full'
                                  or (version == 4 and run_op == 'true'))
                self.assertEqual(effective_job_condition(op_cond, vals, ['success']), expect_legacy_op)
                self.assertEqual(effective_job_condition(gov_cond, vals, ['success']), expect_gov_job)
                if expect_gov_job:
                    expect_gov_steps = run_gov == 'true' or scope in ('full', 'explicit-full') or version != 4
                    expect_op_steps = version == 4 and (run_op == 'true' or scope in ('full', 'explicit-full'))
                    self.assertEqual(effective_job_condition(step_cond(parser_step), vals, []), expect_gov_steps)
                    self.assertEqual(effective_job_condition(step_cond(authority_step), vals, []), expect_gov_steps)
                    self.assertEqual(effective_job_condition(step_cond(op_tools), vals, []), expect_op_steps)
                    self.assertEqual(effective_job_condition(step_cond(op_exec), vals, []), expect_op_steps)


class WorkflowStepTimeoutBoundTests(unittest.TestCase):
    def test_numeric_step_timeouts_never_exceed_enclosing_numeric_job_timeout(self):
        workflow_dir = ROOT / '.github/workflows'
        workflow_paths = sorted(
            path for path in workflow_dir.iterdir()
            if path.is_file() and path.suffix in ('.yml', '.yaml')
        )
        checked_steps = 0
        for path in workflow_paths:
            rel_path = path.relative_to(ROOT).as_posix()
            jobs = workflow_jobs(path.read_text())
            for job_name, body in jobs.items():
                if re.search(r'^    uses:\s*\S+', body, re.M):
                    continue
                job_match = re.search(r'^    timeout-minutes:\s*(\d+)\s*$', body, re.M)
                if not job_match:
                    continue
                job_minutes = int(job_match.group(1))
                for step_match in re.finditer(r'^        timeout-minutes:\s*(\d+)\s*$', body, re.M):
                    checked_steps += 1
                    step_minutes = int(step_match.group(1))
                    with self.subTest(workflow=rel_path, job=job_name, step_minutes=step_minutes):
                        self.assertGreater(step_minutes, 0)
                        self.assertLess(
                            step_minutes,
                            job_minutes,
                            f'{rel_path}:{job_name} step timeout ({step_minutes}m) must stay strictly below job timeout ({job_minutes}m)',
                        )
        self.assertGreater(checked_steps, 0)


class WorkflowEnvUsageTests(unittest.TestCase):
    IMPLICIT_ENV_PREFIXES = ('CARGO_', 'RUST_', 'NODE_', 'NPM_', 'PIP_', 'PYTHON')
    IMPLICIT_ENV_EXACT = frozenset({'RUSTFLAGS', 'RUSTDOCFLAGS', 'CI', 'FORCE_COLOR', 'NO_COLOR'})

    def test_top_level_custom_env_keys_have_consumers(self):
        workflow_dir = ROOT / '.github/workflows'
        workflow_paths = sorted(
            path for path in workflow_dir.iterdir()
            if path.is_file() and path.suffix in ('.yml', '.yaml')
        )
        consumer_corpus_parts = []
        for scan_root in (ROOT / 'scripts', ROOT / '.github/actions', ROOT / 'crates', ROOT / 'tools'):
            if not scan_root.is_dir():
                continue
            for file_path in scan_root.rglob('*'):
                if not file_path.is_file():
                    continue
                if file_path.suffix not in ('.py', '.sh', '.rs', '.js', '.mjs', '.cjs', '.yml', '.yaml'):
                    continue
                if '.test.' in file_path.name:
                    continue
                try:
                    consumer_corpus_parts.append(file_path.read_text(encoding='utf-8', errors='ignore'))
                except OSError:
                    continue
        consumer_corpus = '\n'.join(consumer_corpus_parts)

        unused = []
        for path in workflow_paths:
            rel_path = path.relative_to(ROOT).as_posix()
            text = path.read_text(encoding='utf-8')
            in_env = False
            env_keys = []
            for lineno, line in enumerate(text.splitlines(), 1):
                if re.match(r'^env:\s*$', line):
                    in_env = True
                    continue
                if in_env:
                    if line and not line.startswith((' ', '\t', '#')):
                        in_env = False
                        continue
                    match = re.match(r'^  ([A-Z0-9_]+)\s*:', line)
                    if match:
                        env_keys.append((lineno, match.group(1)))
            for lineno, key in env_keys:
                if key in self.IMPLICIT_ENV_EXACT or key.startswith(self.IMPLICIT_ENV_PREFIXES):
                    continue
                if len(re.findall(rf'\b{re.escape(key)}\b', text)) > 1:
                    continue
                if re.search(rf'\b{re.escape(key)}\b', consumer_corpus):
                    continue
                unused.append(f'{rel_path}:{lineno}: {key}')
        self.assertEqual(
            unused,
            [],
            'Top-level custom workflow env vars with zero consumers: ' + ', '.join(unused),
        )


if __name__ == '__main__':
    unittest.main()
