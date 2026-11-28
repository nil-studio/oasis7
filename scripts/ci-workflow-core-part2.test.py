#!/usr/bin/env python3
"""Self-contained core workflow ordinary image cache and projection contract tests (part 2)."""
import hashlib
import json
import os
from pathlib import Path
import re
import shlex
import subprocess
import tempfile
import textwrap
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parent.parent
RAW_WORKFLOW = (ROOT / '.github/workflows/rust.yml').read_text()


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


def rust_cache_steps(job):
    return [step for step in re.findall(r'^      - .*?(?=^      - |\Z)', job, re.M | re.S)
            if 'uses: Swatinem/rust-cache@' in step]


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


def ordinary_image_cache_admission(job):
    cache = rust_cache_steps(job)
    assert len(cache) == 1, 'ordinary image must use one admitted dependency cache'
    cache = cache[0]
    for field in (
        '          OASIS7_DEPENDENCY_CACHE_FAMILY: ordinary-image\n',
        '          OASIS7_DEPENDENCY_CACHE_BUDGET_BYTES: ${{ env.CI_CACHE_REPOSITORY_BUDGET_BYTES }}\n',
        '          GH_TOKEN: ${{ github.token }}\n',
        '          cache-on-failure: false\n',
    ):
        assert cache.count(field) == 1, 'ordinary image cache needs its exact bounded writer environment'
    save = "job.status == 'success' && github.ref == 'refs/heads/main' && github.workflow_sha == github.sha"
    # Reuse the complete installer/preload/interpreter/provider/save transition;
    # only the ordinary producer's independent step ID differs.
    image_cache_admission_layout(job.replace('ordinary-dependency-cache-tools', 'dependency-cache-tools'),
                                 cache.replace('ordinary-dependency-cache-tools', 'dependency-cache-tools'), save)
    assert 'run: python3 scripts/ci-rust-cache-admission.py install-release\n' in job
    preflight = workflow_step_script(job, 'Verify trusted ordinary image and source')
    for guard in ('test "$GITHUB_REF" = refs/heads/main',
                  'test "$GITHUB_WORKFLOW_SHA" = "$GITHUB_SHA"',
                  'test "$(git rev-parse HEAD)" = "$GITHUB_SHA"'):
        assert guard in preflight, 'ordinary cache source must match the actual trusted main checkout'


class WorkflowCorePart2(unittest.TestCase):
    def test_trusted_ordinary_image_pairs_bounded_cache_with_original_artifacts(self):
        job = JOBS['oasis7-test-build-trusted']
        if '    container:' not in job:
            self.assertNotIn('ORDINARY_BUILD_IMAGE:', job)
            self.assertIn('rustup toolchain install', job)
            self.assertIn('scripts/ci-apt-install.py', job)
            self.assertIn('shared-key: ci-shared-native-trusted-v4', job)
            self.assertIn('save-if: false', rust_cache_steps(job)[0])
            return
        allowed_images = (
            'ghcr.io/nil-studio/oasis7/ci-rust-build@sha256:f121df903f97527a7097134d39f25822c46a3775152768a9cbd591fe65ee987c',
            'ghcr.io/nil-studio/oasis7/ci-rust-ordinary-builder@sha256:714b3f428097659c56639b37083269f04a67f1dcbde56cdcef501a7b18d3c7a8',
        )
        image = re.search(r'^      image: (\S+)$', job, re.M)[1]
        self.assertIn(image, allowed_images)
        digest = image.rsplit(':', 1)[1]
        self.assertIn('container:\n      image: ' + image, job)
        self.assertIn('ORDINARY_BUILD_IMAGE: ' + image, job)
        self.assertIn('      ImageOS: ubuntu24\n', job)
        checkout = '/home/runner/work/oasis7/oasis7'
        self.assertIn('ORDINARY_ARTIFACT_CHECKOUT: ' + checkout, job)
        self.assertIn('      volumes:\n        - ' + checkout + ':' + checkout + '\n', job)
        self.assertIn('          cd -- "$ORDINARY_ARTIFACT_CHECKOUT"\n', job)
        self.assertIn('python3 scripts/ci-test-artifacts.py ordinary-image-preflight\n', job)
        self.assertIn("printf 'BASH_ENV=%s\\n' \"$RUNNER_TEMP/ordinary-image-cwd.bash\" >> \"$GITHUB_ENV\"", job)
        startup = re.search(r"<<'ORDINARY_CWD'\n(.*?)^          ORDINARY_CWD$", job, re.M | re.S)
        self.assertIsNotNone(startup)
        startup = textwrap.dedent(startup[1])
        expected_startup = ('export ORDINARY_ARTIFACT_CHECKOUT=' + checkout
                            + '\ncd -- "$ORDINARY_ARTIFACT_CHECKOUT" || exit 1\n')
        self.assertEqual(startup, expected_startup)
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            canonical = root / 'canonical'
            canonical.mkdir()
            translated = root / 'runner-mapped'
            translated.mkdir()
            script = root / 'startup.bash'
            script.write_text(expected_startup.replace(checkout, shlex.quote(str(canonical))))
            result = subprocess.run(['bash', '--noprofile', '--norc', '-e', '-c', 'pwd -P'],
                                    cwd=translated, env=dict(os.environ, BASH_ENV=str(script),
                                                            ORDINARY_ARTIFACT_CHECKOUT=str(translated)),
                                    text=True, capture_output=True)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(Path(result.stdout.strip()), canonical.resolve())
            canonical.rmdir()
            self.assertNotEqual(subprocess.run(['bash', '--noprofile', '--norc', '-e', '-c', 'exit 0'],
                cwd=translated, env=dict(os.environ, BASH_ENV=str(script),
                                         ORDINARY_ARTIFACT_CHECKOUT=str(translated)), capture_output=True).returncode, 0)


        self.assertIn('timeout-minutes: 45', job)
        self.assertIn('packages: read', job)
        self.assertNotIn('apt-install.py', job)
        self.assertNotIn('rustup toolchain install', job)
        self.assertIn('$(git rev-parse HEAD)', job)
        self.assertIn('build --scope ordinary --plan', job)
        caches = rust_cache_steps(job)
        self.assertEqual(len(caches), 1)
        cache = caches[0]
        self.assertIn('shared-key: ci-ordinary-image-' + digest, cache)
        self.assertIn('add-rust-environment-hash-key: true', cache)
        self.assertIn('env-vars: CARGO CC CFLAGS CXX CMAKE RUST OASIS7_WASM', cache)
        self.assertIn('cache-provider: github', cache)
        self.assertIn('OASIS7_DEPENDENCY_CACHE_FAMILY: ordinary-image', cache)
        ordinary_image_cache_admission(job)
        save = re.search(r'^          save-if: .+$', cache, re.M).group()
        mutations = (
            job.replace(save, '          save-if: true', 1),
            job.replace('python3 scripts/ci-rust-cache-admission.py install-release', 'echo guard_ready', 1),
            job.replace('          NODE_OPTIONS: ${{ steps.ordinary-dependency-cache-tools.outputs.node_options }}\n', '', 1),
            job.replace('          OASIS7_DEPENDENCY_CACHE_PYTHON: ${{ steps.ordinary-dependency-cache-tools.outputs.python }}\n', '', 1),
            job.replace('OASIS7_DEPENDENCY_CACHE_BUDGET_BYTES: ${{ env.CI_CACHE_REPOSITORY_BUDGET_BYTES }}',
                        'OASIS7_DEPENDENCY_CACHE_BUDGET_BYTES: 999999999999', 1),
            job.replace('GH_TOKEN: ${{ github.token }}', 'GH_TOKEN: unrelated', 1),
            job.replace('cache-provider: github', 'cache-provider: other', 1),
            job.replace('cache-on-failure: false', 'cache-on-failure: true', 1),
            job.replace('test "$(git rev-parse HEAD)" = "$GITHUB_SHA"', ': # no actual source check', 1),
            job.replace(" && github.workflow_sha == github.sha }}", " }}", 1),
        )
        for changed in mutations:
            # A disconnected marker must never substitute for the real post hook.
            changed = changed.replace('OASIS7_DEPENDENCY_CACHE_FAMILY: ordinary-image',
                                      'guard_ready: ignored\n          OASIS7_DEPENDENCY_CACHE_FAMILY: ordinary-image', 1)
            with self.subTest(cache_admission=changed), self.assertRaises(AssertionError):
                ordinary_image_cache_admission(changed)
        candidate = JOBS['oasis7-test-build-candidate']
        self.assertIn('cache-mode: read', candidate)
        self.assertNotIn('cache-mode: write', candidate)
        self.assertIn('packages: read', candidate)
        self.assertNotIn('GH_TOKEN:', candidate)
        self.assertNotIn('ordinary-dependency-cache-tools', candidate)
        self.assertIn(
            "    container:\n"
            "      image: >-\n"
            "        ${{ needs.select.outputs.ordinary_candidate_image == 'true' &&\n"
            f"            '{image}' || '' }}}}\n"
            "      options: --user 1001:1001 --init --tmpfs /opt/oasis7-mold:uid=1001,gid=1001,mode=0755,exec\n"
            f"      volumes:\n        - {checkout}:{checkout}\n",
            candidate,
        )
        candidate_cache = rust_cache_steps(candidate)[0]
        self.assertIn(
            f"shared-key: ${{{{ job.container.id && 'ci-ordinary-image-{digest}' || 'ci-shared-native-trusted-v4' }}}}\n",
            candidate_cache,
        )
        self.assertIn('save-if: false', candidate_cache)
        self.assertIn('cache-provider: github', candidate_cache)
        candidate_preflight = workflow_step_script(candidate, 'Verify candidate ordinary image and source')
        for guard in (
            'test "$(id -u):$(id -g)" = 1001:1001',
            '[[ "$(< /proc/1/comm)" =~ ^(docker-init|tini)$ ]]',
            'test "$(git rev-parse HEAD)" = "$EXPECTED_TESTED_SHA"',
            'python3 scripts/ci-test-artifacts.py ordinary-image-preflight --forbid-docker',
            '--plan "$RUNNER_TEMP/selection.json" --expected-selection-sha256 "$SELECTION_SHA256"',
            'test "$CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER" = /opt/oasis7-mold/3.0.0-v1/bin/oasis7-mold-gcc',
            'OASIS7_FORBID_ORDINARY_DOCKER_WASM=1',
        ):
            self.assertIn(guard, candidate_preflight)
        for fallback_marker in (
            'uses: actions/setup-python@',
            'name: Install pinned Rust toolchains',
            'name: Install native libraries',
        ):
            fallback_step = next(
                step for step in re.findall(r'^      - .*?(?=^      - |\Z)', candidate, re.M | re.S)
                if fallback_marker in step
            )
            self.assertIn('if: ${{ !job.container.id }}', fallback_step)
        for sdk_marker in ('name: Reclaim unused candidate SDK capacity', 'name: Wait for candidate SDK reclamation'):
            sdk_step = next(
                step for step in re.findall(r'^      - .*?(?=^      - |\Z)', candidate, re.M | re.S)
                if sdk_marker in step
            )
            self.assertIn("if: github.job == 'oasis7-test-build-candidate' && !job.container.id", sdk_step)
        self.assertIn(
            'ordinary_candidate_image: ${{ steps.workflow-plan.outputs.ordinary_candidate_image }}',
            JOBS['select'],
        )
        projection_script = workflow_step_script(JOBS['select'], 'Publish workflow scheduling projection')
        with tempfile.TemporaryDirectory() as proj_tmp:
            proj_root = Path(proj_tmp)
            proj_out = proj_root / 'github_output'
            base_plan = {
                'scope': 'targeted',
                'groups': ['baseline', 'rust_baseline', 'oasis7_required'],
                'execution_profile': 'pr-fast',
                'fast_plan_version': 1,
                'test_policy_changed': False,
            }
            for overrides, expected_eligible in (
                ({}, True),
                ({'scope': 'full', 'ordinary_risk_targets': [
                    {'package': 'oasis7_runtime', 'target_kind': 'test'}]}, True),
                ({'execution_profile': 'main-fast'}, True),
                ({'test_policy_changed': True}, False),
                ({'execution_profile': 'candidate-full'}, False),
                ({'ordinary_full_fast_targets': []}, False),
                ({'ordinary_risk_targets': [{'package': 'oasis7_runtime', 'target_kind': 'lib'}]}, False),
                ({'scope': 'full', 'ordinary_risk_targets': [
                    {'package': 'oasis7_chain_runtime', 'target_kind': 'test'}]}, False),
            ):
                (proj_root / 'selection.json').write_text(json.dumps(base_plan | overrides))
                proj_out.unlink(missing_ok=True)
                subprocess.run(
                    ['bash', '--noprofile', '--norc', '-euo', 'pipefail', '-c', projection_script],
                    env=dict(os.environ, RUNNER_TEMP=proj_tmp, GITHUB_OUTPUT=str(proj_out)),
                    check=True,
                )
                self.assertIn(
                    f'ordinary_candidate_image={str(expected_eligible).lower()}\n',
                    proj_out.read_text(),
                )
        boundary = '      - name: Identify ordinary workspace cache'
        for name in ('oasis7-test-build-candidate', 'oasis7-test-build-trusted'):
            original = JOBS[name]
            added = '      - name: Unrelated artifact business step\n        run: echo unexpected\n'
            business = next(step for step in re.findall(r'^      - .*?(?=^      - |\Z)', original, re.M | re.S)
                            if step.startswith('      - name: Build ordinary test artifacts\n'))
            for changed in (original.replace(boundary, added + boundary, 1),
                            original.replace(business, '', 1).replace(boundary, business + boundary, 1),
                            original.replace(boundary, boundary + '\n' + boundary, 1)):
                with self.subTest(name=name), patch.dict(JOBS, {name: changed}), self.assertRaises(AssertionError):
                    comparable_source_steps(name)
        preflight = workflow_step_script(job, 'Verify trusted ordinary image and source')
        for guard in ('test "$GITHUB_REF" = refs/heads/main',
                      'test "$GITHUB_WORKFLOW_SHA" = "$GITHUB_SHA"',
                      'test "$(git rev-parse HEAD)" = "$GITHUB_SHA"'):
            self.assertIn(guard, preflight)

    def test_optional_ordinary_cache_capability_is_bound_to_the_complete_plan(self):
        field = 'ordinary_main_fast_cache_writer_version'
        guard = workflow_step_script(JOBS['required-gate'], 'Validate complete selection handoff')
        self.assertEqual(guard, workflow_step_script(JOBS['explicit-full-gate'], 'Validate complete selection handoff'))
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            plan = large_transport_plan()
            environment = dict(os.environ, RUNNER_TEMP=directory, SOURCE_SHA=plan['source_sha'],
                               TESTED_SHA=plan['tested_sha'], POLICY_SHA=plan['policy_sha'])

            def invoke(value):
                return subprocess.run(['bash', '-euo', 'pipefail', '-c', guard], cwd=root,
                    env=dict(environment, WORKFLOW_PLAN=json.dumps(value)), text=True, capture_output=True)

            legacy = project_selection(root, plan)
            self.assertNotIn(field, legacy)
            self.assertEqual(invoke(legacy).returncode, 0)
            self.assertNotEqual(invoke(legacy | {field: 1}).returncode, 0)
            current = project_selection(root, plan | {field: 1})
            self.assertEqual(current[field], 1)
            self.assertEqual(invoke(current).returncode, 0)
            self.assertNotEqual(invoke({key: value for key, value in current.items() if key != field}).returncode, 0)
            for changed in (0, 2, True, '1', None):
                with self.subTest(value=changed):
                    self.assertNotEqual(invoke(current | {field: changed}).returncode, 0)
            (root / 'selection.json').write_text(json.dumps(plan | {field: 2}))
            self.assertNotEqual(invoke(current).returncode, 0)
            for invalid in (0, 2, True, False, 1.0, '1', None):
                with self.subTest(invalid_plan_capability=invalid):
                    with self.assertRaises(AssertionError):
                        project_selection(root, plan | {field: invalid})
                    raw = (root / 'selection.json').read_bytes()
                    # Matching file and projection must still reject values
                    # that Actions would coerce to the supported integer.
                    forged = current | {field: invalid, 'selection_sha256': hashlib.sha256(raw).hexdigest()}
                    self.assertNotEqual(invoke(forged).returncode, 0)

    def test_projection_keeps_all_needs_aggregates_below_process_argument_limits(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            authority = root / 'ci-authority'
            authority.mkdir()
            (authority / 'ci-formal-plan.py').write_bytes((ROOT / 'scripts/ci-formal-plan.py').read_bytes())
            plan = large_transport_plan()
            plan['tested_sha'] = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip()
            del plan['custody_execution_version']
            projected = project_selection(root, plan)
            selection = {'result': 'success', 'outputs': {'plan': json.dumps(projected)}}
            needs = dict(select=selection, **{name: {'result': 'skipped'} for name in (
                'formal-test-build', 'formal-test-build-trusted', 'formal-process-tests')})
            script = workflow_step_script(JOBS['formal-process'], 'Validate all formal process results')
            environment = dict(os.environ, RUNNER_TEMP=directory, SELECTED='false', NEEDS=json.dumps(needs))
            result = subprocess.run(['bash', '-euc', script], env=environment, text=True, capture_output=True)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertIn('not_selected', result.stdout)
            needs['formal-process-tests']['result'] = 'failure'
            result = subprocess.run(['bash', '-euc', script], env=environment | {'NEEDS': json.dumps(needs)},
                                    text=True, capture_output=True)
            self.assertNotEqual(result.returncode, 0)
            observation = workflow_step_script(JOBS['select'], 'Record engineering job observation')
            result = subprocess.run(['bash', '-euc', observation], cwd=root,
                env=dict(environment, TELEMETRY_NEEDS=json.dumps({'select': selection}), TELEMETRY_MATRIX='{}',
                         GITHUB_WORKSPACE=directory, GITHUB_JOB='formal-process', GITHUB_REPOSITORY='fixture/oasis7',
                         GITHUB_RUN_ID='123', GITHUB_RUN_ATTEMPT='1'), text=True, capture_output=True)
            self.assertEqual(result.returncode, 0, result.stderr)
            row = json.loads(result.stdout.strip().split('=', 1)[1])
            self.assertEqual(row['needs'], {'select': 'success'})

    def test_custody_proof_projection_rejects_forged_or_invalid_complete_capabilities(self):
        field = 'custody_compile_proof_version'
        guard = workflow_step_script(JOBS['required-gate'], 'Validate complete selection handoff')
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            plan = large_transport_plan() | {'workspace_support_partition_version': 1, 'test_policy_changed': False}
            environment = dict(os.environ, RUNNER_TEMP=directory, SOURCE_SHA=plan['source_sha'],
                               TESTED_SHA=plan['tested_sha'], POLICY_SHA=plan['policy_sha'])
            def invoke(projected):
                return subprocess.run(['bash', '-euo', 'pipefail', '-c', guard], cwd=root,
                    env=environment | {'WORKFLOW_PLAN': json.dumps(projected)}, text=True, capture_output=True)
            legacy = project_selection(root, plan)
            self.assertNotIn(field, legacy)
            self.assertEqual(invoke(legacy).returncode, 0)
            self.assertNotEqual(invoke(legacy | {field: 1}).returncode, 0)
            current = project_selection(root, plan | {field: 1})
            self.assertEqual(current[field], 1)
            self.assertEqual(invoke(current).returncode, 0)
            self.assertNotEqual(invoke({key: value for key, value in current.items() if key != field}).returncode, 0)
            for key in (field, 'custody_execution_version', 'workspace_support_partition_version'):
                for value in (None, False, True, 0, 2, 1.0, '1', 'missing'):
                    changed = plan | {field: 1, key: value}
                    if value == 'missing':
                        if key == field:
                            continue
                        del changed[key]
                    with self.subTest(key=key, value=value):
                        with self.assertRaises(AssertionError):
                            project_selection(root, changed)
                        raw = (root / 'selection.json').read_bytes()
                        forged = current | {'selection_sha256': hashlib.sha256(raw).hexdigest()}
                        if key in current:
                            if value == 'missing':
                                del forged[key]
                            else:
                                forged[key] = value
                        self.assertNotEqual(invoke(forged).returncode, 0)
            for policy in (None, True, 0, 'false'):
                with self.subTest(policy=policy), self.assertRaises(AssertionError):
                    project_selection(root, plan | {field: 1, 'test_policy_changed': policy})

    def test_custody_policy_projection_preserves_and_requires_the_complete_capability(self):
        field = 'custody_policy_tests_version'
        guard = workflow_step_script(JOBS['required-gate'], 'Validate complete selection handoff')
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            plan = large_transport_plan() | {field: 1, 'custody_compile_proof_version': 1,
                'workspace_support_partition_version': 1, 'test_policy_changed': True}
            environment = dict(os.environ, RUNNER_TEMP=directory, SOURCE_SHA=plan['source_sha'],
                               TESTED_SHA=plan['tested_sha'], POLICY_SHA=plan['policy_sha'])
            def invoke(projected):
                return subprocess.run(['bash', '-euo', 'pipefail', '-c', guard], cwd=root,
                    env=environment | {'WORKFLOW_PLAN': json.dumps(projected)}, text=True, capture_output=True)
            current = project_selection(root, plan)
            self.assertEqual(current[field], 1)
            self.assertEqual(invoke(current).returncode, 0)
            self.assertNotEqual(invoke({key: value for key, value in current.items() if key != field}).returncode, 0)
            for key in (field, 'custody_compile_proof_version', 'custody_execution_version',
                        'workspace_support_partition_version', 'fast_plan_version'):
                for value in (None, False, True, 0, 2, 1.0, '1', 'missing'):
                    changed = plan | {key: value}
                    if value == 'missing':
                        del changed[key]
                    with self.subTest(key=key, value=value), self.assertRaises(AssertionError):
                        project_selection(root, changed)
            for key, value in [('test_policy_changed', False), ('test_policy_changed', 1),
                               ('execution_profile', 'candidate-full')]:
                with self.subTest(key=key, value=value), self.assertRaises(AssertionError):
                    project_selection(root, plan | {key: value})
            old = {key: value for key, value in plan.items() if key != field} | {'test_policy_changed': False}
            projected = project_selection(root, old)
            self.assertNotEqual(invoke(projected | {field: 1}).returncode, 0)


if __name__ == '__main__':
    unittest.main()
