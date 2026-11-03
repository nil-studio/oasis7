import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

ENTRY = Path(__file__).resolve().parent / 'ci-local-check-plan.py'
BASE_PLANNER = '''import json, sys\nprint(json.dumps({'groups': ['baseline'], 'policy_marker': 'trusted'}))\n'''


class CheckLocal(unittest.TestCase):
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
            (self.root / 'scripts' / name).write_text('{}')
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
                self.assertEqual(result.stdout, '')
                self.git('update-index', '--no-assume-unchanged', '--no-skip-worktree', 'file')
                self.git('reset', '--hard', '-q', 'HEAD')
                (self.root / 'new').unlink(missing_ok=True)

    def test_missing_base_and_unspecified_profile_are_blocked(self):
        result = subprocess.run([sys.executable, str(ENTRY), '--profile', 'affected', '--dry-run',
                                 '--base-ref', 'missing'], cwd=self.root, capture_output=True, text=True)
        self.assertEqual(result.returncode, 2)
        self.assertEqual(result.stdout, '')
        result = subprocess.run([sys.executable, str(ENTRY), '--dry-run'], cwd=self.root, capture_output=True)
        self.assertEqual(result.returncode, 2)

    def test_mode_change_is_blocked_even_when_git_config_ignores_it(self):
        self.git('config', 'core.filemode', 'false')
        (self.root / 'file').chmod(0o755)
        result = self.run_entry('--format', 'json')
        self.assertEqual(result.returncode, 2)
        self.assertEqual(result.stdout, '')

    def test_untrusted_candidate_policy_is_not_executed(self):
        (self.root / 'scripts/plan-rust-required-scope.py').write_text("from pathlib import Path\nPath('pwned').touch()\n")
        self.git('commit', '-qam', 'untrusted candidate')
        self.base = self.git('rev-parse', 'HEAD').strip()
        result = self.run_entry('--format', 'json')
        self.assertEqual(result.returncode, 2)
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
        self.assertEqual(result.stdout, '')
        self.assertIn('dirty', result.stderr)


if __name__ == '__main__':
    unittest.main()
