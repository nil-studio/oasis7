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


def portable_cache_container_layout(job, kind):
    """Validate the finite container ABI before reusing the original save guards."""
    image = {'browser': 'ci-runtime@sha256:3984caa42d82963f93af752bf79e3a07838f2c94279cfbb2deeb08f715cd62c9',
             'worker': 'ci-rust-build@sha256:0b13d0b5ba9051617eb6ab77a5af324f066af79bb2753d664b780535c2fe13a3'}[kind]
    published_worker = 'ci-rust-build@sha256:068f6982d684c23a39e4b52e56f35f307da388c5893c7837c10b5882ec7dd9fb'
    if kind == 'worker' and ('      image: ghcr.io/nil-studio/oasis7/' + published_worker + '\n') in job:
        image = published_worker
    container = ('    container:\n      image: ghcr.io/nil-studio/oasis7/' + image +
                 '\n      credentials:\n        username: ${{ github.actor }}\n        password: ${{ github.token }}\n'
                 '      options: --user 1001:1001 --init' +
                 (' --tmpfs /opt/oasis7-mold:uid=1001,gid=1001,mode=0755,exec' if kind == 'worker' else '') +
                 '\n      volumes:\n        - ${{ github.workspace }}:/home/runner/work/oasis7/oasis7\n'
                 '        - /home/runner/work/_temp:/home/runner/work/_temp\n' +
                 ('        - /home/runner/work/_temp:/home/runner/.cache/oasis7/worker-build\n' if kind == 'worker' else '') +
                 '    defaults:\n      run:\n        shell: bash\n        working-directory: /home/runner/work/oasis7/oasis7\n' +
                 ('    env:\n      HOME: /home/runner/work/_temp/portable-worker-home\n'
                  '      CARGO_HOME: /home/runner/work/_temp/portable-worker-home/.cargo\n' if kind == 'worker' else ''))
    assert job.count(container) == 1, 'portable cache requires exact finite mounts/image/UID/cwd'
    header = job.split('    steps:', 1)[0].replace(container, '')
    assert not re.search(r'^    (container|defaults|env):', header, re.M)
    assert all(value not in job for value in ('BASH_ENV', 'id: portable-cache-image', 'scripts/ci-portable-cache-container.py',
                                              'actions/setup-python@', 'rustup toolchain install', 'GITHUB_WORKSPACE='))
    guard = ('      - name: Verify canonical portable cache container\n'
             '        run: python3 scripts/ci-portable-cache-job.py ' + kind + '\n')
    assert job.count(guard) == 1
    checkouts = re.findall(r'^      - uses: actions/checkout@[^\n]+\n', job, re.M)
    assert len(checkouts) == 1 and job.index(checkouts[0]) < job.index(guard)
    first_tool = 'run: python3 scripts/ci-' + ('worker-build' if kind == 'worker' else 'browser-deps') + '.py '
    assert job.index(guard) < job.index(first_tool), 'portable preflight must precede tool I/O'
    if kind == 'worker':
        assert job.index(guard) < job.index('      - name: Configure original pinned mold without writing its auxiliary cache\n')
    job = job.replace(container, '').replace(guard, '')
    if kind == 'worker':
        mold = ('      - name: Configure original pinned mold without writing its auxiliary cache\n'
                '        run: python3 .github/actions/setup-linux-mold/setup.py --install-dir /opt/oasis7-mold/3.0.0-v1\n')
        assert job.count(mold) == 1 and 'uses: ./.github/actions/setup-linux-mold' not in job
        assert job.index(mold) < job.index(first_tool), 'pinned mold must precede Worker preparation'
        commands, flag, count = ('metadata', 'check', 'ensure', 'admit-save'), '--root /home/runner/.cache/oasis7/worker-build/0.7.5', 4
        components = [('worker-build', 'worker-tool-admission', '${{ steps.worker-meta.outputs.root }}',
                       'worker-build-cli-v1-${{ runner.os }}-${{ runner.arch }}-${{ env.RUST_TOOLCHAIN }}-0.7.5-${{ steps.worker-meta.outputs.definition_sha }}')]
    else:
        commands, flag, count = ('metadata', 'fetch', 'admit-save'), '--cache-base /home/runner/work/_temp/oasis7-browser-archives', 7
        components = [(component, 'browser-admission-' + slug, '${{ steps.browser-meta.outputs.' + prefix + '_root }}',
                       '${{ steps.browser-meta.outputs.' + prefix + '_key }}')
                      for component, prefix, slug in [('fonts-noto-cjk', 'fonts_noto_cjk', 'noto'),
                                                     ('fonts-freefont-ttf', 'fonts_freefont_ttf', 'freefont'), ('chrome', 'chrome', 'chrome')]]
    assert job.count(flag) == count
    helper = 'worker-build' if kind == 'worker' else 'browser-deps'
    command_pattern = r'scripts/ci-' + helper + r'\.py (?:' + '|'.join(commands) + r')[^\n]* ' + re.escape(flag) + r'$'
    assert len(re.findall(command_pattern, job, re.M)) == count
    for command in commands:
        if command != 'admit-save':
            pattern = r'(scripts/ci-' + helper + r'\.py ' + command + r'[^\n]*) ' + re.escape(flag) + r'$'
            job = re.sub(pattern, r'\1', job, flags=re.M)
    steps = re.findall(r'^      - .*?(?=^      - |\Z)', job, re.M | re.S)
    finite = [step for step in steps if 'uses: ./.github/actions/portable-cache-transport' in step]
    expected = [(operation, *component) for component in components for operation in ('restore', 'save')]
    assert len(finite) == len(expected), 'portable cache requires one exact restore/save pair per component'
    for step, (operation, component, admission, root, key) in zip(finite, expected):
        prefix, suffix = step.split('        uses:', 1)
        assert '        env:' not in prefix and '        run:' not in prefix
        original = (' ./.github/actions/portable-cache-transport\n        with:\n'
                    f'          operation: {operation}\n          contract: {"worker-build" if kind == "worker" else "browser-archive"}\n'
                    f'          component: {component}\n          key: {key}\n')
        if operation == 'save':
            original += ('          admitted: ${{ steps.' + admission + '.outputs.save }}\n'
                         '          upload-bound: ${{ steps.' + admission + '.outputs.upload_bound_bytes }}\n')
        assert suffix.rstrip('\n') == original.rstrip('\n'), 'portable cache finite input contract differs'
        replacement = (f' actions/cache/{operation}@55cc8345863c7cc4c66a329aec7e433d2d1c52a9 # v6.1.0\n'
                       f'        with:\n          path: {root}\n          key: {key}\n')
        job = job.replace(step, prefix + '        uses:' + replacement, 1)
    return job


def portable_cache_host_layout(job, kind):
    """Check the explicit mixed tool lane; official host cache steps stay exact."""
    if '    container:' in job.split('    steps:', 1)[0]:
        return portable_cache_container_layout(job, kind)
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
            assert len(json_blocks) in (2, 3) and "inputs.run_mode == 'image-support' &&" in header
            support_records = json.loads(' '.join(json_blocks[0].split()))
            records = json.loads(' '.join(json_blocks[-1].split()))
            assert support_records == [row for row in records if row.get('profile') == 'support']
            if len(json_blocks) == 3:
                assert "inputs.run_mode == 'image-viewer' &&" in header
                viewer_records = json.loads(' '.join(json_blocks[1].split()))
                assert viewer_records == [row for row in records if row.get('profile') == 'viewer']
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
        'tooling': (('cargo-tooling-contracts',), 'rust-image-tooling'),
    }
    launcher_family = 'rust-image-launcher'
    if 'launcher' in rows or launcher_family in families:
        expected['launcher'] = (('launcher-web',), launcher_family)
    support_family = 'rust-image-support'
    if 'support' in rows or support_family in families:
        expected['support'] = (('workspace-support-other',), support_family)
    viewer_family = 'rust-image-viewer'
    if 'viewer' in rows or viewer_family in families:
        expected['viewer'] = (('viewer-js-required', 'viewer-performance-report'), viewer_family)
    assert len(records) == len(rows) == len(expected) and set(rows) == set(expected)
    for profile, (names, family) in expected.items():
        row = rows[profile]
        assert set(row) == {'family', 'image', 'namespace', 'options', 'mold-dir'}
        assert row['family'] == family
        assert families.get(family) == (row['namespace'], '.github/workflows/rust-cache.yml',
                                        'rust-image-component-cache')
        for name in names:
            reader = readers[name]
            expected_image_re = (
                r'ghcr.io/nil-studio/oasis7/ci-rust-viewer@sha256:[0-9a-f]{64}'
                if profile == 'viewer'
                else r'ghcr.io/nil-studio/oasis7/ci-rust-(?:wasm-)?build@sha256:[0-9a-f]{64}'
            )
            assert re.fullmatch(expected_image_re, row['image'])
            assert row['namespace'] == row['image'].rsplit('/', 1)[1].replace('@sha256:', '-')
            if name in ('viewer-js-required', 'viewer-performance-report'):
                assert ("needs.select.outputs.viewer_image_compatible == 'true' && '"
                        + row['image'] + "' || ''") in reader
                assert row['options'] == '--user 1001:1001 --init'
                assert '      options: --user 1001:1001 --init\n' in reader
                assert row['mold-dir'] == 'oasis7-viewer-mold'
                assert ("needs.select.outputs.viewer_image_compatible == 'true' && "
                        "format('{0}/" + row['mold-dir'] + "', runner.temp) || ''") in reader
                caches = rust_cache_steps(reader)
                assert len(caches) == 1
                assert ("env.VIEWER_IMAGE_COMPATIBLE == 'true' && '"
                        + row['namespace'] + "' || 'release-packages-web-wasm-trusted-v3'") in caches[0]
                assert 'save-if: false\n' in caches[0]
                assert 'add-rust-environment-hash-key: true' in caches[0]
                assert 'env-vars: CARGO CC CFLAGS CXX CMAKE RUST\n' in caches[0]
                assert not re.search(
                    r'^          (?:key|prefix-key|workspaces|cache-directories|cache-targets):',
                    caches[0],
                    re.M,
                )
                continue
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
    expected_env_vars = (
        "env-vars: ${{ matrix.profile == 'viewer' && 'CARGO CC CFLAGS CXX CMAKE RUST' || 'CARGO CC CFLAGS CXX CMAKE RUST OASIS7_WASM' }}"
        if 'viewer' in rows
        else 'env-vars: CARGO CC CFLAGS CXX CMAKE RUST OASIS7_WASM'
    )
    for required in ('Swatinem/rust-cache@d1527c8f67ef9af357290475d4af4b0fab8f12e0',
                     'OASIS7_DEPENDENCY_CACHE_FAMILY: ${{ matrix.family }}',
                     'OASIS7_DEPENDENCY_CACHE_BUDGET_BYTES: ${{ env.CI_CACHE_REPOSITORY_BUDGET_BYTES }}',
                     'shared-key: ${{ matrix.namespace }}', 'add-rust-environment-hash-key: true',
                     expected_env_vars,
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
                    'rust-image-tooling', 'rust-image-support', 'rust-image-viewer'}.intersection(families)
    assert set(cache_jobs) == (({'shared-native-cache', 'web-wasm-cache'} if migrated
                               else {'shared-native-cache'}) | tool_jobs
                               | ({'plan-image-component-cache'} if 'plan-image-component-cache' in cache_jobs else set())), 'incomplete cache producer layout'
    family = 'release-web'
    producer = cache_jobs.get('web-wasm-cache', '')
    if '    container:' in producer.split('    steps:', 1)[0]:
        image = 'ghcr.io/nil-studio/oasis7/ci-rust-release-web@sha256:f47cf4a0ce572de9b871475aed4e38c35fc7e6e128dda93a7d96e50480e03c59'
        assert re.findall(r'^      image: (.+)$', producer.split('    steps:', 1)[0], re.M) == [image], 'unknown Web image writer'
        header = producer.split('    steps:', 1)[0]
        assert '      options: --user 1001:1001 --init\n' in header
        assert '      RUST_RELEASE_WEB_IMAGE: ' + image + '\n' in header
        assert 'volumes:' not in header and '--privileged' not in header
        family = 'release-web-image'
        namespace = 'ci-rust-release-web-' + image.split('@sha256:', 1)[1]
        registered = (namespace, '.github/workflows/rust-cache.yml', 'web-wasm-cache')
        assert families.get(family) == registered, 'Web image registry must match the exact image namespace'
        assert [name for name, identity in families.items() if identity[0] == namespace] == [family]
    active = []
    for path, workflow, jobs in (('.github/workflows/rust-cache.yml', cache_workflow, cache_jobs),
                                 ('.github/workflows/release-packages.yml', release_workflow, release_jobs)):
        for name, job in jobs.items():
            for step in rust_cache_steps(job):
                keys = re.findall(r'^          shared-key: (.+)$', step, re.M)
                if not set(keys).intersection((namespace, 'release-packages-web-wasm-trusted-v3')):
                    continue
                assert len(keys) == 1, 'one Web namespace per cache step required'
                saves = re.findall(r'^          save-if: (.+)$', step, re.M)
                assert len(saves) == 1, 'one explicit Web save policy required'
                header = job.split('    steps:', 1)[0]
                if saves[0] != 'false':
                    active.append((keys[0], path, name))
                    assert 'cache-mode: read' not in header
                    assert ('cache-mode: write' in header or re.search(r'^cache-mode: write$',
                            workflow.split('\njobs:\n', 1)[0], re.M))
                    assert 'OASIS7_DEPENDENCY_CACHE_FAMILY: ' + family + '\n' in step
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


class Workflow(unittest.TestCase):
    def test_trusted_regression_image_refuses_candidate_sources_and_foreign_docker(self):
        import runpy
        Launcher = runpy.run_path(str(ROOT / 'scripts/ci-trusted-full-regression.py'))
        source = 'a' * 40
        trusted = {
            'GITHUB_REPOSITORY': 'nil-studio/oasis7', 'GITHUB_REF': 'refs/heads/main',
            'GITHUB_WORKFLOW_REF': 'nil-studio/oasis7/.github/workflows/rust.yml@refs/heads/main',
            'GITHUB_SHA': source, 'GITHUB_WORKFLOW_SHA': source, 'GITHUB_JOB': 'full-regression',
            'GITHUB_ACTIONS': 'true', 'RUNNER_ENVIRONMENT': 'github-hosted',
            'RUNNER_OS': 'Linux', 'RUNNER_ARCH': 'X64', 'GITHUB_EVENT_NAME': 'schedule',
            'GITHUB_WORKSPACE': '/home/runner/work/oasis7/oasis7', 'RUNNER_TEMP': '/home/runner/work/_temp',
        }
        with patch('os.getuid', return_value=1001), patch('os.getgid', return_value=1001):
            Launcher['trusted_main'](trusted, source)
            for key, value in (
                    ('GITHUB_REF', 'refs/heads/candidate'), ('GITHUB_SHA', 'b' * 40),
                    ('GITHUB_WORKFLOW_SHA', 'b' * 40), ('GITHUB_JOB', 'candidate-full-regression'),
                    ('GITHUB_REPOSITORY', 'foreign/oasis7'), ('GITHUB_EVENT_NAME', 'pull_request'),
                    ('DOCKER_HOST', 'tcp://remote.example:2375')):
                with self.subTest(key=key), self.assertRaisesRegex(ValueError, 'exact hosted main|unexpected trusted full trigger'):
                    Launcher['trusted_main'](dict(trusted, **{key: value}), source)

    def test_trusted_regression_image_prepares_owned_cargo_bind_points(self):
        import runpy
        launcher = runpy.run_path(str(ROOT / 'scripts/ci-trusted-full-regression.py'))
        command = launcher['image_command']
        revision = 'a' * 40
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            paths = {name: root / name for name in ('scratch', 'workspace', 'mold', 'cargo')}
            for path in paths.values():
                path.mkdir()
            with patch.dict(command.__globals__, {'ROOT': paths['workspace'], 'MOLD': paths['mold'],
                                                   'HOST_CARGO': paths['cargo'], 'head': lambda _: revision}), \
                    patch('shutil.which', return_value=None):
                argv = command('governance', revision, paths['scratch'], 'owned-full-regression',
                               {'OASIS7_PRODUCT_DOC_HEAD': revision, 'OASIS7_PRODUCT_DOC_BASE': 'b' * 40})
            for relative in ('cargo', 'cargo/registry', 'cargo/git'):
                target = paths['scratch'] / relative
                self.assertEqual(target.stat().st_uid, os.getuid())
                self.assertEqual(target.stat().st_mode & 0o777, 0o700)
            for name in ('registry', 'git'):
                self.assertIn(f'type=bind,source={paths["cargo"] / name},target={paths["scratch"] / "cargo" / name}', argv)
            self.assertNotIn('--group-add', argv)
            self.assertNotIn('/var/run/docker.sock', ' '.join(argv))

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
        expected_profiles = {'native', 'node', 'net', 'tooling'}
        image_families = {'rust-image-components', 'rust-image-net', 'rust-image-tooling'}
        if 'rust-image-launcher' in families and 'launcher-web' in JOBS:
            expected_profiles.add('launcher')
            image_families.add('rust-image-launcher')
        if 'rust-image-support' in families:
            expected_profiles.add('support')
            image_families.add('rust-image-support')
        if 'rust-image-viewer' in families:
            expected_profiles.add('viewer')
            image_families.add('rust-image-viewer')
        self.assertEqual(set(component_image_cache_writer(job, families, planner=planner)), expected_profiles)
        # These global values enter the upstream Rust environment hash. The
        # matrix must not create a different identity before any Cargo runs.
        def cache_environment(text):
            header = text.split('\njobs:\n', 1)[0]
            return dict(re.findall(r'^  ((?:CARGO|RUST|OASIS7_WASM)[A-Z0-9_]*): (.+)$', header, re.M))
        self.assertEqual(cache_environment(workflow), cache_environment(RAW_WORKFLOW))
        current_env_vars_line = (
            "env-vars: ${{ matrix.profile == 'viewer' && 'CARGO CC CFLAGS CXX CMAKE RUST' || 'CARGO CC CFLAGS CXX CMAKE RUST OASIS7_WASM' }}"
            if 'rust-image-viewer' in families
            else 'env-vars: CARGO CC CFLAGS CXX CMAKE RUST OASIS7_WASM'
        )
        mutations = (
            job.replace('max-parallel: 1', 'max-parallel: 5', 1),
            job.replace('add-rust-environment-hash-key: true', 'add-rust-environment-hash-key: false', 1),
            job.replace('cache-on-failure: false', 'cache-on-failure: true', 1),
            job.replace('shared-key: ${{ matrix.namespace }}', 'shared-key: ci-shared-native-trusted-v4', 1),
            job.replace(current_env_vars_line, 'env-vars: CARGO', 1),
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
        image = 'ghcr.io/nil-studio/oasis7/ci-rust-release-web@sha256:f47cf4a0ce572de9b871475aed4e38c35fc7e6e128dda93a7d96e50480e03c59'
        image_namespace = 'ci-rust-release-web-' + image.split('@sha256:', 1)[1]
        image_identity = (image_namespace, new_identity[1], new_identity[2])
        image_registry = 'FAMILIES = ' + repr({'release-web': new_identity, 'release-web-image': image_identity})
        image_cache = new_cache.replace('    cache-mode: write\n', '    cache-mode: write\n    container:\n      image: ' + image + '\n      options: --user 1001:1001 --init\n    env:\n      RUST_RELEASE_WEB_IMAGE: ' + image + '\n').replace(
            'OASIS7_DEPENDENCY_CACHE_FAMILY: release-web\n', 'OASIS7_DEPENDENCY_CACHE_FAMILY: release-web-image\n').replace(
            'shared-key: ' + namespace, 'shared-key: ' + image_namespace)
        self.assertTrue(web_cache_layout(image_cache, new_release, image_registry))
        for bad_cache, bad_registry in (
                (image_cache.replace(image, image.replace('ci-rust-release-web@', 'ci-rust-viewer@')), image_registry),
                (image_cache.replace(image_namespace, namespace), image_registry),
                (image_cache, source(new_identity)),
                (image_cache + '  extra-writer:\n    steps:\n' + web_step, image_registry)):
            with self.assertRaises(AssertionError):
                web_cache_layout(bad_cache, new_release, bad_registry)
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
        if (ROOT / 'scripts/run-ci-authority-python.py').is_file():
            (repo / 'scripts/run-ci-authority-python.py').write_bytes(
                (ROOT / 'scripts/run-ci-authority-python.py').read_bytes())
        if (ROOT / 'scripts/ci_engine').is_dir():
            for engine_file in sorted((ROOT / 'scripts/ci_engine').rglob('*.py')):
                dst = repo / engine_file.relative_to(ROOT)
                dst.parent.mkdir(parents=True, exist_ok=True)
                dst.write_bytes(engine_file.read_bytes())
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
                    for mode in ('full', 'image-native', ''):
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
                if name == 'full-regression' and 'scripts/ci-trusted-full-regression.py full "$SOURCE_SHA"' in job:
                    image = re.search(r'(?m)^          image=ghcr[.]io/nil-studio/oasis7/ci-rust-full-regression@sha256:([0-9a-f]{64})$', job)
                    self.assertIsNotNone(image)
                    keys = re.findall(r'(?m)^          shared-key: ([^\n]+)$', job)
                    self.assertEqual(keys, [f'ci-rust-full-regression-{image[1]}-native-v1'])
                    self.assertIn('save-if: false\n', job)
                    self.assertIn('env-vars: CARGO CC CFLAGS CXX CMAKE RUST OASIS7_WASM', job)
                    continue
                component = re.fullmatch(r'formal-(debug|endpoint)-build(?:-trusted)?', name)
                namespace = f'ci-formal-{component[1]}-deps-trusted-v1' if component else 'ci-shared-native-trusted-v4'
                self.assertIn('shared-key: ' + (rust_image_cache_namespace(name, job) or namespace) + '\n', job, name)
                self.assertIn('env-vars: CARGO CC CFLAGS CXX CMAKE RUST OASIS7_WASM', job, name)
        self.assertNotIn('ordinary-required-v2', WORKFLOW)

    def test_dependency_save_requires_success_and_current_trusted_source(self):
        ecs_jobs = workflow_jobs((ROOT / '.github/workflows/ecs-packages.yml').read_text())
        _, shared_writer = self.shared_cache_workflow()
        for name, job in (('shared-native-cache', shared_writer),
                          ('native', ecs_jobs['native']), ('native-trusted', ecs_jobs['native-trusted'])):
            caches = [step for step in re.findall(r'^      - .*?(?=^      - |\Z)', job, re.M | re.S)
                      if 'uses: Swatinem/rust-cache@' in step]
            image = name in ('native', 'native-trusted') and len(caches) == 2
            if image:
                host, cache = caches
                self.assertIn("if: github.job == 'native' && hashFiles('scripts/ci-rust-cache-toolchains.py') != '' && steps.dependency-cache-tools.outcome == 'success'\n", host)
                self.assertIn('save-if: false\n', host)
                self.assertIn("if: github.job == 'native-trusted' && hashFiles('scripts/ci-rust-cache-toolchains.py') != '' && steps.image-dependency-cache-tools.outputs.guard_ready == 'true'\n", cache)
                self.assertIn('NODE_OPTIONS: ${{ steps.image-dependency-cache-tools.outputs.node_options }}', cache)
                self.assertIn('OASIS7_DEPENDENCY_CACHE_PYTHON: ${{ steps.image-dependency-cache-tools.outputs.python }}', cache)
                self.assertIn('cache-provider: github\n', cache)
            else:
                self.assertEqual(len(caches), 1, name)
                cache, = caches
            save = re.search(r'          save-if: \$\{\{ (.*?) \}\}', cache)
            self.assertIsNotNone(save, name)
            expression = save.group(1)
            for status in ('success', 'failure', 'cancelled'):
                for ref in ('refs/heads/main', 'refs/heads/topic', 'refs/pull/1/merge'):
                    for matching_source in (False, True):
                        for guard in ('true', 'false', '') if image else ('true',):
                            values = {'job.status': status, 'github.ref': ref, 'github.job': name,
                                      'github.workflow_sha': 'a' if matching_source else 'b', 'github.sha': 'a',
                                      'steps.image-dependency-cache-tools.outputs.guard_ready': guard}
                            predicate = expression
                            for token, value in values.items():
                                predicate = predicate.replace(token, repr(value))
                            predicate = predicate.replace('&&', ' and ').replace('||', ' or ')
                            with self.subTest(job=name, status=status, ref=ref, matching=matching_source, guard=guard):
                                actual = eval(predicate, {'__builtins__': {}}, {})
                                self.assertEqual(actual, status == 'success' and ref == 'refs/heads/main'
                                                 and matching_source and name != 'native' and guard == 'true')

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
        if 'launcher-web' in JOBS:
            self.assertNotIn('oasis7_viewer', JOBS['launcher-web'])
        self.assertIn("node-version: '24'", JOBS['site-quality'])
        for name in node_jobs - {'site-quality','workspace-support-legacy','workspace-support-other','custody-native','custody-wasm-production','custody-node'}:
            self.assertIn('cache: npm',JOBS[name])
            self.assertIn('cache-dependency-path: crates/oasis7_viewer/package-lock.json',JOBS[name])
            if name == 'full-regression' and 'scripts/ci-trusted-full-regression.py npm' in JOBS[name]:
                self.assertIn("['npm', 'ci', '--prefix', 'crates/oasis7_viewer']",
                              (ROOT / 'scripts/ci-trusted-full-regression.py').read_text())
            else:
                self.assertIn('npm ci --prefix crates/oasis7_viewer', JOBS[name])
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
        for name in (n for n in ('launcher-web', 'full-regression') if n in JOBS):
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
            if name == 'web-wasm-cache' and '    container:' in writer.split('    steps:', 1)[0]:
                family = 'release-web-image'
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

    def test_invalid_selected_group_cannot_silently_skip_command(self):
        driver = (ROOT / 'scripts/ci-tests.sh').read_text()
        self.assertIn('Unknown CI group:', driver)
        self.assertNotIn('disabled_by_scope_planner', driver)
        self.assertNotIn('required_gate_execution_contract', driver)
        self.assertNotIn('scripts/pm/', driver)

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


class BrowserDownloadCache(unittest.TestCase):
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
            'workspace-support-other',
            'custody-native-contracts', 'custody-native',
            'custody-wasm-production',
            'custody-node', 'workspace-support',
            'scenario-regression',
            'workflow-governance', 'codex-agent-config-validation',
            'compile-metrics', 'site-quality', 'doc-checker-contracts',
            'cargo-tooling-contracts', 'macos-package-contract', 'required-gate',
        }
        if 'launcher-web' in JOBS:
            priority_jobs.add('launcher-web')
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


class ViewerImageCacheWriterGuardTests(unittest.TestCase):
    def test_component_image_cache_writer_accepts_optional_viewer_profile_and_rejects_mismatch(self):
        cache_jobs = workflow_jobs((ROOT / '.github/workflows/rust-cache.yml').read_text())
        admission_source = (ROOT / 'scripts/ci-rust-cache-admission.py').read_text()
        assignments = [
            node for node in ast.parse(admission_source).body
            if isinstance(node, ast.Assign)
            and any(isinstance(t, ast.Name) and t.id == 'FAMILIES' for t in node.targets)
        ]
        families = dict(ast.literal_eval(assignments[0].value))
        base_job = cache_jobs['rust-image-component-cache']
        rows = component_image_cache_writer(base_job, families)
        self.assertNotIn('viewer', rows)

        viewer_match = re.search(
            r"(ghcr\.io/nil-studio/oasis7/ci-rust-viewer@sha256:[0-9a-f]{64})",
            JOBS['viewer-js-required'],
        )
        self.assertIsNotNone(viewer_match)
        viewer_image = viewer_match.group(1)
        viewer_ns = viewer_image.rsplit('/', 1)[1].replace('@sha256:', '-')
        viewer_row_json = json.dumps({
            'profile': 'viewer',
            'family': 'rust-image-viewer',
            'image': viewer_image,
            'namespace': viewer_ns,
            'options': '--user 1001:1001 --init',
            'mold-dir': 'oasis7-viewer-mold',
        }, separators=(',', ':'))
        # Replace the final fromJSON block (all profiles), not the first (image-support).
        blocks = re.findall(r"fromJSON\('(\[.*?\])'\)", base_job, re.DOTALL)
        migrated_all = blocks[-1].rstrip()[:-1].rstrip() + ',\n              ' + viewer_row_json + '\n            ]'
        migrated_job = base_job.replace(blocks[-1], migrated_all, 1).replace(
            'env-vars: CARGO CC CFLAGS CXX CMAKE RUST OASIS7_WASM',
            "env-vars: ${{ matrix.profile == 'viewer' && 'CARGO CC CFLAGS CXX CMAKE RUST' || 'CARGO CC CFLAGS CXX CMAKE RUST OASIS7_WASM' }}",
        )
        migrated_families = dict(
            families,
            **{'rust-image-viewer': (viewer_ns, '.github/workflows/rust-cache.yml', 'rust-image-component-cache')},
        )
        migrated_rows = component_image_cache_writer(migrated_job, migrated_families)
        self.assertIn('viewer', migrated_rows)

        # Mismatched options or dropping $OASIS7_WASM_TOOLCHAIN stabilization must fail closed.
        for bad_job in (
            migrated_job.replace('--user 1001:1001 --init', ''),
            migrated_job.replace('"$RUST_TOOLCHAIN" "$OASIS7_WASM_TOOLCHAIN"', '"$RUST_TOOLCHAIN"'),
        ):
            with self.assertRaises(AssertionError):
                component_image_cache_writer(bad_job, migrated_families)


if __name__ == '__main__':
    unittest.main()
