#!/usr/bin/env python3
"""Self-contained core workflow browser download cache, worker tool cache, consolidation v4, step timeout bound, and top-level workflow env usage contract tests (part 12)."""
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
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


def workflow_step_script(job, name):
    step = next(step for step in re.findall(r'^      - .*?(?=^      - |\Z)', job, re.M | re.S)
                if 'name: ' + name + '\n' in step)
    if '        run: |\n' in step:
        return textwrap.dedent(step.split('        run: |\n', 1)[1])
    return step.split('        run: ', 1)[1].splitlines()[0] + '\n'


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


class WorkflowCorePart12(unittest.TestCase):
    IMPLICIT_ENV_PREFIXES = ('CARGO_', 'RUST_', 'NODE_', 'NPM_', 'PIP_', 'PYTHON')
    IMPLICIT_ENV_EXACT = frozenset({'RUSTFLAGS', 'RUSTDOCFLAGS', 'CI', 'FORCE_COLOR', 'NO_COLOR'})

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
            if job not in JOBS:
                continue
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
