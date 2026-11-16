import contextlib
import hashlib
import io
import json
import os
from pathlib import Path
import re
import runpy
import signal
import time
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

ENTRY = Path(__file__).resolve().parent / 'ci-local-check-plan.py'
BASE_PLANNER = '''import json, sys\nprint(json.dumps({'groups': ['baseline'], 'reasons': ['fixture'], 'policy_marker': 'trusted'}))\n'''


class CheckLocal(unittest.TestCase):
    def test_governance_driver_runs_adapter_tests_and_propagates_failure(self):
        config = json.loads((ENTRY.parent / 'ci-required-scope.json').read_text())
        path = 'scripts/ci-local-check-plan.test.py'
        self.assertEqual(config['workflow_governance_python_tests'].count(path), 1)
        (self.root / 'scripts/ci-required-scope.json').write_text(json.dumps(
            dict(workflow_governance_python_tests=[path])))
        (self.root / 'scripts/ci-tests.sh').write_text(
            'run_workflow_governance_remaining_contract_tests() { echo completed; }\n')
        for status in (0, 37):
            with self.subTest(status=status):
                (self.root / path).write_text(f'print("adapter")\nraise SystemExit({status})\n')
                result = subprocess.run([sys.executable, str(ENTRY.parent / 'ci-python-control-contracts.py'),
                                         'local-governance', '--repo-root', str(self.root)],
                                        text=True, capture_output=True)
                self.assertEqual(result.returncode, status, result.stderr)
                self.assertEqual(result.stdout.splitlines().count('adapter'), 1)
                self.assertEqual('completed' in result.stdout.splitlines(), status == 0)

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.git('init', '-q')
        self.git('config', 'user.name', 'Test')
        self.git('config', 'user.email', 'test@example.invalid')
        (self.root / 'scripts').mkdir()
        (self.root / 'scripts/plan-rust-required-scope.py').write_text(BASE_PLANNER)
        for name in ('ci-required-scope.json', 'ci-formal-process-suites.json'):
            (self.root / 'scripts' / name).write_text(json.dumps({'groups': ['baseline', 'rust_baseline']}))
        (self.root / 'file').write_text('base')
        self.git('add', '.')
        self.git('commit', '-qm', 'base')
        self.base = self.git('rev-parse', 'HEAD').strip()
        self.git('update-ref', 'refs/remotes/origin/main', self.base)

    def git(self, *args):
        return subprocess.check_output(['git', '-C', str(self.root), *args], text=True, stderr=subprocess.PIPE)

    def run_entry(self, *args):
        return subprocess.run([sys.executable, str(ENTRY), '--profile', 'affected', '--dry-run',
                               '--base-ref', self.base, *args], cwd=self.root, text=True,
                              stdout=subprocess.PIPE, stderr=subprocess.PIPE)

    def assert_report(self, result, status, code):
        self.assertEqual(result.returncode, code, result.stderr)
        output = result.stdout.decode() if isinstance(result.stdout, bytes) else result.stdout
        if 'Full result:' in output:
            output = output.split('Full result:', 1)[1]
        data = json.loads(output)
        self.assertIsInstance(data, dict)
        self.assertEqual(data['status'], status)
        self.assertEqual(data['exit_code'], code)
        self.assertIs(data['validated'], False)
        self.assertEqual(data['checks_executed'], 0)
        self.assertEqual(data['tests_executed'], 0)
        return data

    def install_planner(self, source):
        (self.root / 'scripts/plan-rust-required-scope.py').write_text(source)
        self.git('add', 'scripts/plan-rust-required-scope.py')
        self.git('commit', '-qm', 'trusted fixture')
        self.base = self.git('rev-parse', 'HEAD').strip()
        self.git('update-ref', 'refs/remotes/origin/main', self.base)

    def invoke(self, *args):
        return subprocess.run([sys.executable, str(ENTRY), *args], cwd=self.root,
                              capture_output=True, text=True, timeout=20)

    def test_uses_trusted_base_not_candidate_and_reports_no_execution(self):
        (self.root / 'scripts/plan-rust-required-scope.py').write_text("raise RuntimeError('candidate must not run')\n")
        (self.root / 'scripts/ci-required-scope.json').write_text('{"candidate": true}')
        self.git('commit', '-qam', 'candidate')
        before = self.git('status', '--porcelain=v1')
        index = (self.root / '.git/index').read_bytes()
        result = self.run_entry('--format', 'json')
        self.assertEqual(result.returncode, 0, result.stderr)
        data = json.loads(result.stdout)
        self.assertEqual(data['plan']['policy_marker'], 'trusted')
        self.assertEqual(data['policy_sha'], self.base)
        self.assertEqual(data['source_sha'], self.git('rev-parse', 'HEAD').strip())
        self.assertFalse(data['validated'])
        self.assertEqual(data['checks_executed'], 0)
        self.assertEqual(data['status'], 'planned')
        self.assertEqual(index, (self.root / '.git/index').read_bytes())
        self.assertEqual(before, self.git('status', '--porcelain=v1'))
        text = self.run_entry('--format', 'text')
        self.assertEqual(text.returncode, 0)
        self.assertIn('not validated', text.stdout)
        self.assertIn(self.base, text.stdout)
        self.assertIn('baseline', text.stdout)

    def test_dirty_staged_untracked_hidden_flags_are_blocked(self):
        for kind in ('unstaged', 'staged', 'untracked', 'assume', 'skip'):
            with self.subTest(kind=kind):
                if kind == 'untracked':
                    (self.root / 'new').write_text('new')
                else:
                    if kind in ('assume', 'skip'):
                        self.git('update-index', '--assume-unchanged' if kind == 'assume' else '--skip-worktree', 'file')
                    (self.root / 'file').write_text('changed')
                    if kind == 'staged':
                        self.git('add', 'file')
                result = self.run_entry('--format', 'json')
                self.assertEqual(result.returncode, 2, result.stderr)
                self.assert_report(result, 'blocked', 2)
                self.git('update-index', '--no-assume-unchanged', '--no-skip-worktree', 'file')
                self.git('reset', '--hard', '-q', 'HEAD')
                (self.root / 'new').unlink(missing_ok=True)

    def test_missing_base_and_unspecified_profile_are_blocked(self):
        result = subprocess.run([sys.executable, str(ENTRY), '--profile', 'affected', '--dry-run',
                                 '--base-ref', 'missing'], cwd=self.root, capture_output=True, text=True)
        self.assertEqual(result.returncode, 2)
        self.assert_report(result, 'blocked', 2)
        result = subprocess.run([sys.executable, str(ENTRY), '--dry-run'], cwd=self.root, capture_output=True)
        self.assert_report(result, 'blocked', 2)

    def test_mode_change_is_blocked_even_when_git_config_ignores_it(self):
        self.git('config', 'core.filemode', 'false')
        (self.root / 'file').chmod(0o755)
        result = self.run_entry('--format', 'json')
        self.assertEqual(result.returncode, 2)
        self.assert_report(result, 'blocked', 2)

    def test_untrusted_candidate_policy_is_not_executed(self):
        (self.root / 'scripts/plan-rust-required-scope.py').write_text("from pathlib import Path\nPath('pwned').touch()\n")
        self.git('commit', '-qam', 'untrusted candidate')
        self.base = self.git('rev-parse', 'HEAD').strip()
        result = self.run_entry('--format', 'json')
        self.assertEqual(result.returncode, 2)
        self.assert_report(result, 'blocked', 2)
        self.assertIn('untrusted policy', result.stderr)
        self.assertFalse((self.root / 'pwned').exists())

    def test_mutation_during_planning_blocks(self):
        (self.root / 'scripts/plan-rust-required-scope.py').write_text(
            "from pathlib import Path\nPath('file').write_text('changed during plan')\n" + BASE_PLANNER)
        self.git('commit', '-qam', 'inject mutation')
        self.base = self.git('rev-parse', 'HEAD').strip()
        self.git('update-ref', 'refs/remotes/origin/main', self.base)
        result = self.run_entry('--format', 'json')
        self.assertEqual(result.returncode, 2, result.stderr)
        self.assert_report(result, 'stale_input', 2)
        self.assertEqual(json.loads(result.stdout)['plan']['groups'], ['baseline'])

    def test_evidence_and_logs_match_the_actual_trusted_planner(self):
        with tempfile.TemporaryDirectory() as output:
            result = self.run_entry('--format', 'json', '--output-dir', output)
            data = self.assert_report(result, 'planned', 0)
            for key in ('source_sha', 'base_sha', 'policy_sha', 'trust_anchor_sha'):
                self.assertEqual(data[key], self.base)
            self.assertTrue(data['started_at'])
            self.assertTrue(data['finished_at'])
            self.assertGreaterEqual(data['duration_seconds'], 0)
            planner = data['planner']
            self.assertEqual(planner['cwd'], str(self.root))
            self.assertEqual(planner['timeout_seconds'], 120)
            self.assertEqual(planner['returncode'], 0)
            self.assertEqual(planner['sha256'], hashlib.sha256(BASE_PLANNER.encode()).hexdigest())
            config = (self.root / 'scripts/ci-required-scope.json').read_bytes()
            self.assertEqual(planner['config_sha256']['ci-required-scope.json'], hashlib.sha256(config).hexdigest())
            self.assertIn('--policy-ref', planner['argv'])
            self.assertEqual(planner['argv'][planner['argv'].index('--policy-ref') + 1], self.base)
            self.assertEqual(data['groups'], [{'name': 'baseline', 'status': 'not_run', 'reason': 'dry_run'}])
            logs = data['logs']
            directory = Path(logs['directory'])
            self.assertEqual(directory.parent, Path(output))
            for key in ('stdout', 'stderr', 'report'):
                self.assertTrue(Path(logs[key]).is_file(), logs)
            self.assertEqual(json.loads(Path(logs['stdout']).read_text())['groups'], ['baseline'])
            self.assertEqual(json.loads(Path(logs['report']).read_text()), data)
            second = self.assert_report(self.run_entry('--format', 'json', '--output-dir', output), 'planned', 0)
            self.assertNotEqual(second['logs']['directory'], logs['directory'])
            self.assertEqual(json.loads(Path(logs['report']).read_text()), data)

    def test_text_contains_the_complete_report(self):
        result = self.run_entry('--format', 'text')
        self.assertIn('Full result:', result.stdout)
        data = self.assert_report(result, 'planned', 0)
        self.assertEqual(data['plan']['groups'], ['baseline'])
        self.assertIn('not validated', result.stdout)

    def test_empty_groups_are_no_checks_not_validation(self):
        self.install_planner("import json\nprint(json.dumps({'groups': [], 'reasons': []}))\n")
        data = self.assert_report(self.run_entry('--format', 'json'), 'no_checks', 3)
        self.assertEqual(data['plan']['groups'], [])
        self.assertEqual(data['groups'], [])

    def test_invalid_planner_payloads_fail_closed(self):
        payloads = ('not json', '[]', '{}', '{"groups": ["baseline"]}',
                    '{"groups": "baseline", "reasons": []}',
                    '{"groups": ["baseline", "baseline"], "reasons": []}',
                    '{"groups": ["unregistered"], "reasons": []}',
                    '{"groups": [1], "reasons": []}',
                    '{"groups": ["baseline"], "reasons": "why"}',
                    '{"groups": ["baseline"], "reasons": [1]}')
        for payload in payloads:
            with self.subTest(payload=payload):
                self.install_planner('print(' + repr(payload) + ')\n')
                self.assert_report(self.run_entry('--format', 'json'), 'failed', 1)

    def test_invalid_trusted_configuration_is_structured_blocked(self):
        configs = ('not json', '[]', 'null', '{}', '{"groups": null}',
                   '{"groups": "baseline"}', '{"groups": [1]}',
                   '{"groups": ["baseline", null]}')
        for config in configs:
            with self.subTest(config=config):
                (self.root / 'scripts/ci-required-scope.json').write_text(config)
                self.git('add', 'scripts/ci-required-scope.json')
                self.git('commit', '-qm', 'invalid trusted configuration')
                self.base = self.git('rev-parse', 'HEAD').strip()
                self.git('update-ref', 'refs/remotes/origin/main', self.base)
                data = self.assert_report(self.run_entry('--format', 'json'), 'blocked', 2)
                self.assertEqual(data['planner']['status'], 'not_started')

    def test_planner_nonzero_exit_is_failed_and_captures_stderr(self):
        self.install_planner("import sys\nprint('planner diagnostic', file=sys.stderr)\nsys.exit(17)\n")
        data = self.assert_report(self.run_entry('--format', 'json'), 'failed', 1)
        self.assertEqual(data['planner']['returncode'], 17)
        self.assertIn('planner diagnostic', Path(data['logs']['stderr']).read_text())

    def test_timeout_is_failed_and_bounded(self):
        self.install_planner('import time\ntime.sleep(30)\n' + BASE_PLANNER)
        start = time.monotonic()
        data = self.assert_report(self.run_entry('--format', 'json', '--timeout-seconds', '0.1'), 'failed', 1)
        self.assertLess(time.monotonic() - start, 10)
        self.assertEqual(data['planner']['timeout_seconds'], 0.1)

    def test_invalid_arguments_are_structured_blocked_reports(self):
        cases = [[], ['--dry-run'], ['--profile', 'affected'],
                 ['--profile', 'other', '--dry-run'],
                 ['--profile', 'affected', '--dry-run', '--unknown-option']]
        cases.extend(['--profile', 'affected', '--dry-run', '--timeout-seconds', value]
                     for value in ('0', '-1', 'nan', 'inf', 'invalid'))
        for args in cases:
            with self.subTest(args=args):
                self.assert_report(self.invoke(*args, '--format', 'json'), 'blocked', 2)

    def test_default_base_is_unique_merge_base(self):
        (self.root / 'file').write_text('candidate')
        self.git('commit', '-qam', 'candidate')
        data = self.assert_report(self.invoke('--profile', 'affected', '--dry-run', '--format', 'json'), 'planned', 0)
        self.assertEqual(data['base_sha'], self.base)
        self.assertEqual(data['source_sha'], self.git('rev-parse', 'HEAD').strip())

    def test_missing_trust_anchor_is_blocked(self):
        self.git('update-ref', '-d', 'refs/remotes/origin/main')
        self.assert_report(self.run_entry('--format', 'json'), 'blocked', 2)

    def test_ambiguous_merge_base_is_blocked(self):
        tree = self.git('rev-parse', 'HEAD^{tree}').strip()
        def make_commit(message, *parents):
            args = ['commit-tree', tree, '-m', message]
            for parent in parents:
                args.extend(['-p', parent])
            return self.git(*args).strip()
        left = make_commit('left', self.base)
        right = make_commit('right', self.base)
        first = make_commit('first merge', left, right)
        second = make_commit('second merge', right, left)
        self.git('reset', '--hard', '-q', first)
        self.git('update-ref', 'refs/remotes/origin/main', second)
        self.assert_report(self.invoke('--profile', 'affected', '--dry-run', '--format', 'json'), 'blocked', 2)

    def test_unmerged_index_is_blocked(self):
        self.git('checkout', '-qb', 'other')
        (self.root / 'file').write_text('other')
        self.git('commit', '-qam', 'other')
        other = self.git('rev-parse', 'HEAD').strip()
        self.git('checkout', '-q', '--detach', self.base)
        (self.root / 'file').write_text('ours')
        self.git('commit', '-qam', 'ours')
        merge = subprocess.run(['git', '-C', str(self.root), 'merge', '--no-edit', other], capture_output=True)
        self.assertNotEqual(merge.returncode, 0)
        self.assert_report(self.run_entry('--format', 'json'), 'blocked', 2)

    def test_output_file_is_report_io_failure(self):
        with tempfile.TemporaryDirectory() as output:
            target = Path(output) / 'file'
            target.write_text('preserve me')
            self.assert_report(self.run_entry('--format', 'json', '--output-dir', str(target)), 'failed', 1)
            self.assertEqual(target.read_text(), 'preserve me')

    def test_worktree_output_must_be_ignored(self):
        self.assert_report(self.run_entry('--format', 'json', '--output-dir', str(self.root / 'not-ignored')), 'blocked', 2)
        self.assertFalse((self.root / 'not-ignored').exists())
        (self.root / '.gitignore').write_text('local-output/\n')
        self.git('add', '.gitignore')
        self.git('commit', '-qm', 'ignore local outputs')
        data = self.assert_report(self.run_entry('--format', 'json', '--output-dir', str(self.root / 'local-output')), 'planned', 0)
        self.assertTrue(Path(data['logs']['report']).is_file())
        self.assertEqual(self.git('status', '--porcelain=v1'), '')

    def test_anchor_mutation_is_stale_with_plan_preserved(self):
        self.install_planner("import subprocess\nsubprocess.run(['git', 'update-ref', '-d', 'refs/remotes/origin/main'], check=True)\n" + BASE_PLANNER)
        data = self.assert_report(self.run_entry('--format', 'json'), 'stale_input', 2)
        self.assertEqual(data['plan']['groups'], ['baseline'])

    def module_result(self, namespace):
        stdout, stderr = io.StringIO(), io.StringIO()
        with mock.patch.object(Path, 'cwd', return_value=self.root), contextlib.redirect_stdout(stdout), contextlib.redirect_stderr(stderr):
            code = namespace['main'](['--profile', 'affected', '--dry-run', '--base-ref', self.base, '--format', 'json'])
        return subprocess.CompletedProcess([], code, stdout.getvalue(), stderr.getvalue())

    def test_git_tool_failure_returns_structured_blocked_report(self):
        namespace = runpy.run_path(str(ENTRY), run_name='adapter_test')
        with mock.patch.object(subprocess, 'check_output', side_effect=FileNotFoundError('git unavailable')):
            data = self.assert_report(self.module_result(namespace), 'blocked', 2)
        self.assertIn('Git command failed', data['reason'])

    def test_unreadable_index_returns_structured_blocked_report(self):
        namespace = runpy.run_path(str(ENTRY), run_name='adapter_test')
        read_bytes = Path.read_bytes
        def unreadable(path):
            if path == self.root / '.git/index':
                raise PermissionError('index unreadable')
            return read_bytes(path)
        with mock.patch.object(Path, 'read_bytes', unreadable):
            data = self.assert_report(self.module_result(namespace), 'blocked', 2)
        self.assertIn('cannot read index', data['reason'])

    def test_report_write_failure_returns_failed_without_claiming_a_report(self):
        namespace = runpy.run_path(str(ENTRY), run_name='adapter_test')
        write_text = Path.write_text
        def unwritable(path, *args, **kwargs):
            if path.name == 'result.json.tmp':
                raise PermissionError('report write denied')
            return write_text(path, *args, **kwargs)
        with mock.patch.object(Path, 'write_text', unwritable):
            data = self.assert_report(self.module_result(namespace), 'failed', 1)
        self.assertIsNone(data['logs']['report'])
        self.assertEqual(data['plan']['groups'], ['baseline'])

    def test_conflicting_git_environment_is_not_used(self):
        # Caller-provided alternate worktree/index paths cannot redefine the inspected input.
        with tempfile.TemporaryDirectory() as other:
            alternate_index = Path(other) / 'index'
            alternate_index.write_text('must stay unchanged')
            env = os.environ.copy()
            env.update(GIT_DIR=other, GIT_WORK_TREE=other, GIT_INDEX_FILE=str(alternate_index),
                       GIT_EXTERNAL_DIFF='invalid-external-diff')
            result = subprocess.run([sys.executable, str(ENTRY), '--profile', 'affected', '--dry-run',
                                     '--base-ref', self.base, '--format', 'json'], cwd=self.root,
                                    env=env, capture_output=True, text=True, timeout=20)
            data = self.assert_report(result, 'planned', 0)
            self.assertEqual(data['source_sha'], self.base)
            self.assertEqual(alternate_index.read_text(), 'must stay unchanged')

    @unittest.skipUnless(os.name == 'posix', 'process group cancellation requires POSIX')
    def test_sigint_cancels_own_children_and_preserves_unrelated_files(self):
        with tempfile.TemporaryDirectory() as external:
            external = Path(external)
            ready = external / 'ready'
            child_marker = external / 'child-survived'
            unrelated = external / 'unrelated'
            unrelated.write_text('keep')
            child = "import time; from pathlib import Path; time.sleep(2); Path(" + repr(str(child_marker)) + ").touch(); time.sleep(30)"
            planner = ("import subprocess, sys, time\nfrom pathlib import Path\n"
                       "subprocess.Popen([sys.executable, '-c', " + repr(child) + "])\n"
                       "Path(" + repr(str(ready)) + ").touch()\ntime.sleep(30)\n")
            self.install_planner(planner)
            proc = subprocess.Popen([sys.executable, str(ENTRY), '--profile', 'affected', '--dry-run',
                                     '--format', 'json', '--base-ref', self.base], cwd=self.root,
                                    text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
            try:
                deadline = time.monotonic() + 10
                while not ready.exists() and proc.poll() is None and time.monotonic() < deadline:
                    time.sleep(0.02)
                self.assertTrue(ready.exists(), 'trusted planner did not start')
                proc.send_signal(signal.SIGINT)
                stdout, stderr = proc.communicate(timeout=10)
                self.assert_report(subprocess.CompletedProcess(proc.args, proc.returncode, stdout, stderr), 'cancelled', 130)
                time.sleep(2.2)
                self.assertFalse(child_marker.exists(), 'planner descendant survived cancellation')
                self.assertEqual(unrelated.read_text(), 'keep')
            finally:
                if proc.poll() is None:
                    proc.kill()
                    proc.communicate()


if __name__ == '__main__':
    unittest.main()
