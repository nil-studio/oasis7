#!/usr/bin/env python3
"""Self-contained core workflow candidate full, browser/codegen/security, bindgen cache, and auxiliary workflow contract tests (part 9)."""
from pathlib import Path
import re
import subprocess
import unittest

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


class WorkflowCorePart9(unittest.TestCase):
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
        image = re.search(r'(?m)^          image=ghcr[.]io/nil-studio/oasis7/ci-rust-full-regression@sha256:([0-9a-f]{64})$', writer)
        if image:
            launcher = (ROOT / 'scripts/ci-trusted-full-regression.py').read_text()
            self.assertIn(image[1], launcher)
            self.assertIn("['./scripts/ci-tests.sh', 'full-extended']", launcher)
            self.assertIn("['npm', 'ci', '--prefix', 'crates/oasis7_viewer']", launcher)
            self.assertIn("['./scripts/ci-rust-governance-report.sh', '--out-dir', 'output/rust-governance']", launcher)
            self.assertIn('save-if: false', caches[0])
            self.assertIn('shared-key: ci-rust-full-regression-' + image[1] + '-native-v1', caches[0])
            self.assertNotIn('name: Install system deps', writer)
            self.assertIn('name: Install system deps', candidate)
            writer = writer.replace(caches[0], caches[0].replace(
                'ci-rust-full-regression-' + image[1] + '-native-v1', 'ci-shared-native-trusted-v4'), 1)
            caches[0] = caches[0].replace('ci-rust-full-regression-' + image[1] + '-native-v1', 'ci-shared-native-trusted-v4')
            writer = re.sub(r'      - name: Pull the fixed trusted full regression image\n.*?(?=      - name: Install viewer web dependencies in the fixed image)',
                            '', writer, flags=re.S)
            writer = writer.replace('Install viewer web dependencies in the fixed image', 'Install viewer web dependencies')
            writer = writer.replace('python3 scripts/ci-trusted-full-regression.py npm "$GITHUB_SHA"',
                                    'npm ci --prefix crates/oasis7_viewer')
            for label in ('Install system deps', 'Install product-document Markdown parser'):
                candidate = re.sub(r'      - name: ' + label + r'\n.*?(?=      - name:)', '', candidate, flags=re.S)
            writer = writer.replace('python3 scripts/ci-trusted-full-regression.py full "$SOURCE_SHA"',
                                    'CI_VERBOSE=1 ./scripts/ci-tests.sh full-extended')
            writer = writer.replace('python3 scripts/ci-trusted-full-regression.py governance "$GITHUB_SHA"',
                                    './scripts/ci-rust-governance-report.sh --out-dir output/rust-governance')
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
        for check in ('scripts/libp2p-security-source.test.py',
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
        if re.search(r'^FROM rust-viewer AS rust-release-web$', dockerfile, re.M):
            targets.append('rust-release-web')
            leaf = dockerfile.split('FROM rust-viewer AS rust-release-web\n', 1)[1].split('\nFROM ', 1)[0]
            self.assertIn('COPY --from=custody-node-tools /usr/local/bin/node /opt/oasis7-node22/bin/node', leaf)
            self.assertIn('netcat-openbsd xvfb xauth', leaf)
            smoke = (ROOT / 'docker/ci-base/build.sh').read_text().split('if [[ "$target" == rust-release-web ]]; then', 1)[1]
            self.assertIn('--network none --user 1001:1001', smoke)
            self.assertIn('export PATH=/opt/oasis7-node22/bin:$PATH', smoke)
            self.assertIn('PATH="/usr/local/bin:$PATH" python3 /opt/oasis7-viewer-browser/scripts/ci-browser-deps.py activate', smoke)
            self.assertIn('xvfb-run -a node', smoke)
            self.assertIn('headless: false', smoke)
            self.assertIn('--target wasm32-unknown-unknown --crate-type lib', smoke)
            self.assertNotIn('docker.sock', leaf)
        self.assertIn(f"target: [{', '.join(targets)}]\n", ci_base_jobs['build'])
        publish_job = ci_base_jobs['publish']
        publish_header = publish_job.split('    steps:', 1)[0]
        groups = 'base, specialized, app-wasm' if 'rust-viewer' in targets else 'base, specialized'
        if 'rust-release-web' in targets:
            groups += ', release-web'
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
            ('release-web', {'rust-release-web'}),
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
        self.assertIn('rustup target add wasm32-unknown-unknown', writer)
        if 'scripts/ci-trusted-full-regression.py full "$SOURCE_SHA"' in writer:
            launcher = (ROOT / 'scripts/ci-trusted-full-regression.py').read_text()
            dockerfile = (ROOT / 'docker/ci-base/Dockerfile').read_text()
            self.assertIn('FROM rust-custody-wasm AS rust-full-regression\n', dockerfile)
            self.assertIn('apt-get install -y --no-install-recommends clang llvm', dockerfile)
            for tool in ('clang --print-targets | grep -w wasm32', 'llvm-ar --version'):
                self.assertLess(launcher.index(tool), launcher.index("['./scripts/ci-tests.sh', 'full-extended']"))
        else:
            self.assertIn('            clang \\', writer)
            self.assertIn('            llvm \\', writer)
            for tool in ('clang --print-targets | grep -w wasm32', 'llvm-ar --version'):
                self.assertLess(writer.index(tool), writer.index('Run full test tier'))


if __name__ == '__main__':
    unittest.main()
