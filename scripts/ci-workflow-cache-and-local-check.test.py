#!/usr/bin/env python3
"""Self-contained portable cache, local check driver, and workerd fault capture workflow contract tests."""
import json
import os
from pathlib import Path
import re
import shlex
import shutil
import subprocess
import sys
import tempfile
import textwrap
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


class WorkerdFaultCapture(unittest.TestCase):
    def test_prebuilt_custody_tools_require_pinning_and_preflight(self):
        node = (f"    container:\n      image: ghcr.io/nil-studio/oasis7/ci-build@sha256:{'a' * 64}\n"
                "      - run: gdb --version && gcc --version && pkg-config --version\n")
        validate_custody_tools(node)
        for bad in (node.replace('@sha256:', ':'), node.replace('gdb --version', 'true'),
                    node + '      - run: sudo apt-get update\n'):
            with self.assertRaises(ValueError):
                validate_custody_tools(bad)

    def test_evidence_upload_is_bounded_without_masking_failure(self):
        node, summary = JOBS['custody-node'], JOBS['workspace-support']
        marker = '      - name: Preserve workerd evidence for this attempt\n'
        self.assertEqual(node.count(marker), 1)
        before, upload = node.split(marker)
        validate_workerd_capture(node, summary)
        for replacement in ('', '        timeout-minutes: 0\n',
                            '        timeout-minutes: 60\n',
                            '        timeout-minutes: 5\n        continue-on-error: true\n'):
            with self.subTest(replacement=replacement), self.assertRaises(ValueError):
                validate_workerd_capture(before + marker + upload.replace(
                    '        timeout-minutes: 5\n', replacement, 1), summary)

    def test_real_capture_and_attempt_scoped_artifacts_are_required(self):
        validate_workerd_capture(JOBS['custody-node'], JOBS['workspace-support'])
        for path in ('scripts/ci-custody-workerd-capture.py', 'scripts/ci-custody-workerd-gdb.py',
                     'scripts/ci-custody-workerd-gdb-contracts.py',
                     'scripts/ci-custody-workerd-capture.test.py'):
            self.assertIn('scripts/ci-custody-workerd-capture.test.py',
                          CONFIG['python_control_contracts'][path]['tests'])

    def test_missing_capture_contracts_or_overwritten_failure_evidence_is_rejected(self):
        node, summary = JOBS['custody-node'], JOBS['workspace-support']
        for before, after in (
                (' --capture-workerd-faults', ''),
                ("OASIS7_REQUIRE_GDB_TESTS: '1'", "OASIS7_REQUIRE_GDB_TESTS: '0'"),
                ('--failfast GdbContracts', 'Receipts'),
                (('gdb --version && gcc --version && pkg-config --version'
                  if re.search(r'^    container:', node, re.M) else '-- gdb gcc pkg-config'), '-- pkg-config'),
                ('custody-workerd-fault-${{ github.run_id }}-${{ github.run_attempt }}-',
                 'custody-workerd-fault-${{ github.run_id }}-'),
                ('name: custody-workerd-fault-', 'overwrite: true\n          name: custody-workerd-fault-'),
                ('/workerd-fault-attempt-1/records.jsonl', '/workerd-fault-dropped/records.jsonl'),
                ('/client-abuse.integration.test.mjs.attempt-1.log', '/client-abuse.dropped.log')):
            with self.subTest(before=before), self.assertRaises(ValueError):
                validate_workerd_capture(node.replace(before, after), summary)
        with self.assertRaises(ValueError):
            validate_workerd_capture(node, summary.replace(' --capture-workerd-faults', ''))


class PortableCacheContainers(unittest.TestCase):
    def authority(self):
        import importlib.util
        spec = importlib.util.spec_from_file_location('portable_workflow_authority', ROOT / 'scripts/ci-workflow.test.py')
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        return module

    def test_both_jobs_use_finite_containers_and_remove_the_mixed_launcher(self):
        jobs = workflow_jobs((ROOT / '.github/workflows/rust-cache.yml').read_text())
        authority = self.authority()
        for kind, name in (('browser', 'browser-archive-cache'), ('worker', 'worker-build-cache')):
            job = jobs[name]
            self.assertIn('    container:', job)
            authority.portable_cache_container_layout(job, kind)
            self.assertNotIn('BASH_ENV', job)
            self.assertNotIn('portable-cache-image', job)
            self.assertNotIn('actions/cache/save@', job)
            self.assertNotIn('actions/cache/restore@', job)
            self.assertNotIn('env:\n          GITHUB_WORKSPACE:', job)
        self.assertFalse((ROOT / 'scripts/ci-portable-cache-container.py').exists())

    def test_original_admission_output_and_save_guards_are_the_only_writers(self):
        jobs = workflow_jobs((ROOT / '.github/workflows/rust-cache.yml').read_text())
        authority = self.authority()
        authority.browser_archive_cache_writer(jobs['browser-archive-cache'])
        authority.worker_tool_cache_writer(jobs['worker-build-cache'])
        for kind, name in (('browser', 'browser-archive-cache'), ('worker', 'worker-build-cache')):
            job = jobs[name]
            for before, after in [('--user 1001:1001', '--user 0:0'),
                                  ('/home/runner/work/_temp:/home/runner/work/_temp', '/:/home/runner/work/_temp'),
                                  ('operation: restore', 'operation: lookup'),
                                  ('outputs.upload_bound_bytes', 'outputs.unbounded')]:
                self.assertIn(before, job)
                with self.subTest(kind=kind, before=before), self.assertRaises(AssertionError):
                    authority.portable_cache_container_layout(job.replace(before, after), kind)

    def test_direct_tool_commands_preserve_real_failure_without_startup_interception(self):
        jobs = workflow_jobs((ROOT / '.github/workflows/rust-cache.yml').read_text())
        worker = ('if [[ "$TOOL_CACHE_HIT" == true ]]; then\n'
                  '  python3 scripts/ci-worker-build.py check --root /home/runner/.cache/oasis7/worker-build/0.7.5\n'
                  'else\n  python3 scripts/ci-worker-build.py ensure --root /home/runner/.cache/oasis7/worker-build/0.7.5\nfi\n')
        step = next(step for step in re.findall(r'^      - .*?(?=^      - |\Z)', jobs['worker-build-cache'], re.M | re.S)
                    if 'id: prepare-worker' in step)
        self.assertEqual(textwrap.dedent(step.split('        run: |\n', 1)[1]).strip(), worker.strip())
        browser = ('python3 scripts/ci-browser-deps.py fetch --component fonts-noto-cjk '
                   '--cache-base /home/runner/work/_temp/oasis7-browser-archives\n')
        self.assertIn('        run: ' + browser, jobs['browser-archive-cache'])
        with tempfile.TemporaryDirectory() as name:
            root = Path(name).resolve()
            stub = root / 'python3'
            stub.write_text('#!/bin/sh\nprintf "<%s>\\n" "$@"\nexit "${FIXTURE_EXIT:-0}"\n')
            stub.chmod(0o755)
            for template in (worker, browser):
                for hit in ('true', 'false'):
                    for code in (0, 19):
                        env = dict(PATH=str(root) + ':' + os.environ['PATH'], HOME=str(root),
                                   TOOL_CACHE_HIT=hit, FIXTURE_EXIT=str(code))
                        result = subprocess.run(['bash', '-e', '-c', template + 'echo reached\n'],
                                                env=env, capture_output=True, text=True)
                        self.assertEqual(result.returncode, code, result.stderr)
                        self.assertEqual('reached' in result.stdout, code == 0)



class LocalCheckDriverContract(unittest.TestCase):
    """Exercise the real driver functions and Git inputs without a Rust build."""

    def shell(self, command, *, cwd=ROOT, **environment):
        script = ('set -euo pipefail\nci_python=' + shlex.quote(sys.executable)
                  + '\nsource ' + shlex.quote(str(ROOT / 'scripts/ci-tests.sh'))
                  + '\n' + command)
        return subprocess.run(['bash', '-c', script], cwd=cwd, text=True,
                              capture_output=True, env=dict(os.environ,
                              **({'OASIS7_LOCAL_CHECK': '0', 'GIT_OPTIONAL_LOCKS': '0'} | environment)))

    def runner(self, directory):
        directory = Path(directory)
        runner = directory / 'command runner.py'
        runner.write_text(
            'import json, os, pathlib, subprocess, sys\n'
            'assert sys.argv[1] == "--"\n'
            'path = pathlib.Path(os.environ["OASIS7_LOCAL_CHECK_RUN_DIR"]) / "command.json"\n'
            'path.write_text(json.dumps(sys.argv[2:]))\n'
            'raise SystemExit(subprocess.run(sys.argv[2:]).returncode)\n')
        return dict(OASIS7_LOCAL_CHECK='1', OASIS7_LOCAL_CHECK_COMMAND_RUNNER=str(runner),
                    OASIS7_LOCAL_CHECK_RUN_DIR=str(directory))

    def test_command_runner_preserves_python_resolution_arguments_and_failures(self):
        with tempfile.TemporaryDirectory() as directory:
            environment = self.runner(directory)
            argument = 'space $() "quote"\nnext line'
            program = 'import json,sys; print(json.dumps(sys.argv[1:])); sys.exit(int(sys.argv[1]))'
            for code in (0, 37):
                with self.subTest(code=code):
                    command = 'run python3 -c ' + shlex.quote(program)
                    command += ' ' + str(code) + ' ' + shlex.quote(argument) + '\necho reached-next-command\n'
                    result = self.shell(command, **environment)
                    self.assertEqual(result.returncode, code, result.stderr)
                    recorded = json.loads((Path(directory) / 'command.json').read_text())
                    self.assertEqual(recorded, [sys.executable, '-c', program, str(code), argument])
                    self.assertIn(json.dumps([str(code), argument]), result.stdout)
                    self.assertEqual('reached-next-command' in result.stdout, code == 0)

    def test_normal_driver_does_not_use_local_runner(self):
        result = self.shell('run python3 -c "print(12345)"\n',
                            OASIS7_LOCAL_CHECK_COMMAND_RUNNER='/missing/runner.py')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('12345', result.stdout)

    def test_local_runner_requires_existing_absolute_paths(self):
        with tempfile.TemporaryDirectory() as directory:
            environment = self.runner(directory)
            for invalid in ({'OASIS7_LOCAL_CHECK_COMMAND_RUNNER': 'relative.py'},
                            {'OASIS7_LOCAL_CHECK_COMMAND_RUNNER': '/missing/runner.py'},
                            {'OASIS7_LOCAL_CHECK_RUN_DIR': 'relative'},
                            {'OASIS7_LOCAL_CHECK_RUN_DIR': '/missing/reports'}):
                with self.subTest(invalid=invalid):
                    result = self.shell('run python3 -c "print(12345)"\n', **(environment | invalid))
                    self.assertEqual(result.returncode, 2, result.stderr)
                    self.assertFalse((Path(directory) / 'command.json').exists())

    def test_local_cargo_locks_stay_before_test_and_clippy_arguments(self):
        recorder = 'run() { "$ci_python" -c \'import json,sys; print(json.dumps(sys.argv[1:]))\' "$@"; }\n'
        commands = ('run_cargo test -p fixture -- --ignored\n'
                    'run_cargo test --locked -p fixture -- --locked\n'
                    'run_cargo_clippy -p fixture\n'
                    'run_cargo fmt --all -- --check\n')
        for mode in ('0', '1'):
            for verbose in ('0', '1'):
                with self.subTest(mode=mode, verbose=verbose):
                    result = self.shell(recorder + commands, OASIS7_LOCAL_CHECK=mode, CI_VERBOSE=verbose)
                    self.assertEqual(result.returncode, 0, result.stderr)
                    rows = [json.loads(line) for line in result.stdout.splitlines()]
                    cargo_prefix = ['env', '-u', 'RUSTC_WRAPPER', 'cargo']
                    verbose_args = ['--verbose'] if verbose == '1' else []
                    lock = ['--locked'] if mode == '1' else []
                    self.assertEqual(rows[0], cargo_prefix + verbose_args +
                                     ['test', *lock, '-p', 'fixture', '--', '--ignored'])
                    self.assertEqual(rows[1], cargo_prefix + verbose_args +
                                     ['test', '--locked', '-p', 'fixture', '--', '--locked'])
                    self.assertEqual(rows[2], cargo_prefix + ['clippy', *verbose_args, *lock,
                                     '-p', 'fixture', '--', '-D', 'warnings', '-D',
                                     'clippy::correctness', '-D', 'clippy::suspicious'])
                    self.assertEqual(rows[3], cargo_prefix + verbose_args + ['fmt', '--all', '--', '--check'])

    def test_local_cargo_deny_requires_the_installed_exact_version_without_installing(self):
        cases = [('1', None), ('1', '0.0.0'), ('1', '0.19.9'), ('0', None), ('0', '0.0.0')]
        for mode, version in cases:
            with self.subTest(mode=mode, version=version), tempfile.TemporaryDirectory() as directory:
                directory = Path(directory)
                scripts, binaries, report = (directory / name for name in ('scripts', 'bin', 'report'))
                for path in (scripts, binaries, report):
                    path.mkdir()
                for name in ('bash', 'env', 'awk', 'python3'):
                    (binaries / name).symlink_to(shutil.which(name))
                helper = scripts / 'ensure-cargo-deny.sh'
                helper.write_bytes((ROOT / 'scripts/ensure-cargo-deny.sh').read_bytes())
                helper.chmod(0o755)
                # Only unrelated validators are fixture no-ops. The real driver,
                # command receipt runner and unmodified provisioning helper run.
                for name in ('check-rustsec-ignore-baseline.test.sh', 'check-rustsec-ignore-baseline.sh'):
                    path = scripts / name
                    path.write_text('#!/bin/sh\nexit 0\n')
                    path.chmod(0o755)
                cargo = binaries / 'cargo'
                cargo.write_text('#!/bin/sh\nprintf "%s\\n" "$*" >> "$CARGO_CALLS"\n'
                                 'if [ "$1" = install ]; then printf attempted > "$INSTALL_LOG"; fi\n')
                cargo.chmod(0o755)
                if version is not None:
                    deny = binaries / 'cargo-deny'
                    deny.write_text('#!/bin/sh\nprintf "cargo-deny ' + version + '\\n"\n')
                    deny.chmod(0o755)
                result = self.shell('run_rustsec_advisory_check\nprintf "after-ci=%s\\n" "$CI"\n',
                    cwd=directory, PATH=str(binaries), CI='false', CARGO_DENY_VERSION='0.0.0',
                    OASIS7_LOCAL_CHECK=mode, OASIS7_LOCAL_CHECK_GROUP='rust_baseline',
                    OASIS7_LOCAL_CHECK_COMMAND_RUNNER=str(ROOT / 'scripts/ci-local-check-command.py'),
                    OASIS7_LOCAL_CHECK_RUN_DIR=str(report), INSTALL_LOG=str(directory / 'installed'),
                    CARGO_CALLS=str(directory / 'cargo-calls'))
                passed = mode == '0' or version == '0.19.9'
                self.assertEqual(result.returncode, 0 if passed else 1, result.stdout + result.stderr)
                self.assertEqual((directory / 'installed').exists(), mode == '0' and version is None)
                if mode == '1':
                    receipts = [json.loads(path.read_text()) for path in (report / 'commands').glob('*.json')]
                    ensures = [row for row in receipts if any(arg.endswith('/ensure-cargo-deny.sh') for arg in row['argv'])]
                    self.assertEqual(len(ensures), 1)
                    self.assertEqual(ensures[0]['argv'], ['env', 'CI=true', 'CARGO_DENY_VERSION=0.19.9',
                                                        './scripts/ensure-cargo-deny.sh'])
                    self.assertEqual(ensures[0]['exit_code'], 0 if passed else 127)
                    self.assertEqual(ensures[0]['status'], 'passed' if passed else 'failed')
                    if not passed:
                        self.assertIn('cargo-deny 0.19.9 is required in CI', result.stdout)
                        self.assertFalse((directory / 'cargo-calls').exists())
                else:
                    self.assertFalse((report / 'commands').exists())
                    if version is None:
                        self.assertIn('install cargo-deny --version 0.0.0 --locked',
                                      (directory / 'cargo-calls').read_text())
                if passed:
                    self.assertIn('after-ci=false', result.stdout)

    def test_local_standalone_lockfile_inspection_locks_root_metadata_too(self):
        with tempfile.TemporaryDirectory() as directory:
            directory = Path(directory)
            repo = self.repository(directory)
            tool = repo / 'tools/demo'
            tool.mkdir(parents=True)
            (tool / 'Cargo.toml').write_text('[package]\nname="demo"\nversion="0.1.0"\n')
            (tool / 'Cargo.lock').write_text('version = 4\n')
            self.git(repo, 'add', 'tools')
            binaries = directory / 'bin'
            binaries.mkdir()
            cargo = binaries / 'cargo'
            cargo.write_text('#!' + sys.executable + '\nimport json,os,sys\n'
                'with open(os.environ["CALLS"], "a") as log: log.write(json.dumps(sys.argv[1:])+"\\n")\n'
                'if sys.argv[1] == "locate-project": print(os.environ["TOOL_MANIFEST"])\n'
                'else: print(json.dumps({"packages":[], "workspace_members":[]}))\n')
            cargo.chmod(0o755)
            calls = directory / 'calls'
            before = self.identity(repo)
            for mode in ('0', '1'):
                calls.unlink(missing_ok=True)
                result = subprocess.run(['bash', str(ROOT / 'scripts/check-standalone-tool-lockfiles.sh')],
                    text=True, capture_output=True, env=dict(os.environ, PATH=str(binaries) + os.pathsep + os.environ['PATH'],
                    OASIS7_LOCAL_CHECK=mode, OASIS7_STANDALONE_TOOL_REPO_ROOT=str(repo),
                    OASIS7_CI_RUN_RUST_BASELINE='true', GIT_OPTIONAL_LOCKS='0', CALLS=str(calls),
                    TOOL_MANIFEST=str(tool / 'Cargo.toml')))
                self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                metadata = [args for args in map(json.loads, calls.read_text().splitlines()) if args[0] == 'metadata']
                self.assertEqual(len(metadata), 2)
                self.assertEqual('--locked' in metadata[0], mode == '1')
                self.assertIn('--locked', metadata[1])
                self.assertEqual(self.identity(repo), before)

    def repository(self, directory):
        repo = Path(directory) / 'repo'
        repo.mkdir()
        self.git(repo, 'init', '-q')
        self.git(repo, 'config', 'user.name', 'Local checks fixture')
        self.git(repo, 'config', 'user.email', 'fixture@example.invalid')
        (repo / 'tracked.txt').write_text('clean\n')
        (repo / '.gitignore').write_text('ignored/\n')
        self.git(repo, 'add', '.')
        self.git(repo, 'commit', '-qm', 'Base fixture')
        return repo

    @staticmethod
    def git(repo, *args):
        return subprocess.check_output(['git', '-C', str(repo), *args], stderr=subprocess.PIPE,
                                       env=dict(os.environ, GIT_OPTIONAL_LOCKS='0')).decode().strip()

    def identity(self, repo):
        return (self.git(repo, 'rev-parse', 'HEAD'), (repo / '.git/index').read_bytes(),
                {str(path.relative_to(repo)): path.read_bytes() for path in repo.rglob('*')
                 if path.is_file() and '.git' not in path.relative_to(repo).parts})

    def test_local_baseline_checks_each_whitespace_origin_without_mutation(self):
        # Isolate unrelated baseline checks; execute the actual Git/Python checks.
        script = '''run() {
  if [[ "$1" == git || ( "$1" == "$ci_python" && "${2:-}" == - ) ]]; then "$@"; fi
}
run_product_doc_governance_check() { :; }
run_required_gate_checks
'''
        for case in ('committed', 'staged-cancelled', 'unstaged', 'untracked',
                     'untracked-clean', 'untracked-binary', 'ignored'):
            with self.subTest(case=case), tempfile.TemporaryDirectory() as directory:
                repo = self.repository(directory)
                base = self.git(repo, 'rev-parse', 'HEAD')
                if case in ('committed', 'staged-cancelled', 'unstaged'):
                    (repo / 'tracked.txt').write_text('bad trailing whitespace  \n')
                    if case != 'unstaged':
                        self.git(repo, 'add', 'tracked.txt')
                    if case == 'committed':
                        self.git(repo, 'commit', '-qm', 'Changed fixture')
                    elif case == 'staged-cancelled':
                        (repo / 'tracked.txt').write_text('clean\n')
                else:
                    path = repo / ('ignored/file.txt' if case == 'ignored' else 'new file\nname.txt')
                    path.parent.mkdir(exist_ok=True)
                    path.write_bytes(b'clean\n' if case == 'untracked-clean' else
                                     b'\0binary  \n' if case == 'untracked-binary' else b'bad  \n')
                before = self.identity(repo)
                result = self.shell(script, cwd=repo, OASIS7_LOCAL_CHECK='1',
                                    OASIS7_PRODUCT_DOC_BASE=base,
                                    OASIS7_PRODUCT_DOC_HEAD=self.git(repo, 'rev-parse', 'HEAD'))
                passed = case in ('untracked-clean', 'untracked-binary', 'ignored')
                self.assertEqual(result.returncode == 0, passed, result.stdout + result.stderr)
                self.assertEqual(self.identity(repo), before)
                if not passed:
                    self.assertIn('trailing whitespace', result.stdout)

    def test_windows_local_paths_include_untracked_and_preserve_nul_boundaries(self):
        for name, valid in [('new:invalid.txt', False), ('line\nbreak.txt', False),
                            ('tab\tname.txt', False), ('new file 中文.txt', True),
                            ('ignored/NUL.txt', True)]:
            with self.subTest(name=name), tempfile.TemporaryDirectory() as directory:
                repo = self.repository(directory)
                (repo / 'scripts').mkdir()
                checker = repo / 'scripts/check-windows-paths.sh'
                checker.write_bytes((ROOT / 'scripts/check-windows-paths.sh').read_bytes())
                path = repo / name
                path.parent.mkdir(exist_ok=True)
                path.write_text('input\n')
                before = self.identity(repo)
                for mode, expected in [('0', True), ('1', valid)]:
                    result = subprocess.run(['bash', str(checker)], cwd=repo, text=True,
                        capture_output=True, env=dict(os.environ, OASIS7_LOCAL_CHECK=mode,
                                                       GIT_OPTIONAL_LOCKS='0'))
                    self.assertEqual(result.returncode == 0, expected, result.stdout + result.stderr)
                self.assertEqual(self.identity(repo), before)


if __name__ == '__main__':
    unittest.main()
