#!/usr/bin/env python3
"""Self-contained core workflow mold setup, checkout normalization, writer identity, and governance preview contract tests (part 8)."""
import itertools
import os
from pathlib import Path
import re
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


class WorkflowCorePart8(unittest.TestCase):
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


if __name__ == '__main__':
    unittest.main()
