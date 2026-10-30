#!/usr/bin/env python3
"""Real-Git regression for the baseline workflow's tested documentation range."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

SCRIPTS = Path(__file__).resolve().parent
ROOT = SCRIPTS.parent
HELPER = SCRIPTS / 'resolve-product-doc-ci-range.py'


class RangeTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name) / 'repo'
        self.root.mkdir()
        self.git('init', '-q', '-b', 'main')
        self.git('config', 'user.name', 'Fixture')
        self.git('config', 'user.email', 'fixture@example.invalid')
        self.write('README.md', '# Fixture\n')
        self.base = self.commit('base')

    def git(self, *args):
        return subprocess.check_output(['git', '-C', str(self.root), *args], text=True, stderr=subprocess.PIPE).strip()

    def write(self, name, text):
        path = self.root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text, encoding='utf-8')

    def commit(self, message):
        self.git('add', '.')
        self.git('commit', '-qm', message)
        return self.git('rev-parse', 'HEAD')

    def history(self, pr_design=True, merge_design=True):
        self.git('checkout', '-qb', 'feature')
        self.write('source.txt', 'source change\n')
        if pr_design:
            self.write('doc/system/pr-change.design.md', '# Broken PR design\n')
        self.source = self.commit('PR change')
        self.git('checkout', '-q', 'main')
        self.write('doc/system/main-only.design.md', '# Broken main-only design\n')
        self.target = self.commit('main advanced after event base')
        self.git('merge', '--no-ff', '--no-commit', 'feature')
        if merge_design:
            self.write('doc/system/merge-only.design.md', '# Broken merge-result design\n')
        self.merge = self.commit('tested synthetic merge')
        self.event = {'pull_request': {'base': {'sha': self.base}, 'head': {'sha': self.source}}}

    def invoke(self, event=None, kind='pull_request', tested=None):
        event_path = Path(self.temp.name) / 'event.json'
        event_path.write_text(json.dumps(self.event if event is None else event))
        return subprocess.run([sys.executable, str(HELPER), '--repo-root', str(self.root),
                               '--event-name', kind, '--event-path', str(event_path),
                               '--tested-oid', tested or self.merge], text=True, capture_output=True)

    def assert_bad(self, result):
        self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
        self.assertEqual(result.stdout, '')
        self.assertIn('product-doc-ci-range: error:', result.stderr)

    def range(self):
        result = self.invoke()
        self.assertEqual(result.returncode, 0, result.stderr)
        values = dict(line.split('=', 1) for line in result.stdout.splitlines())
        self.assertEqual(values, {'OASIS7_PRODUCT_DOC_BASE': self.target,
                                  'OASIS7_PRODUCT_DOC_HEAD': self.merge})
        return values

    def check_designs(self):
        values = self.range()
        return subprocess.run([sys.executable, str(SCRIPTS / 'system-design-traceability-check.py'),
                               '--repo-root', str(self.root), '--base', values['OASIS7_PRODUCT_DOC_BASE'],
                               '--head', values['OASIS7_PRODUCT_DOC_HEAD'], '--worktree'], text=True, capture_output=True)

    def test_advanced_main_is_excluded_but_pr_and_merge_changes_stay_strict(self):
        self.history()
        before = (self.git('rev-parse', 'HEAD'), self.git('status', '--porcelain'))
        result = self.check_designs()
        self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn('doc/system/pr-change.design.md', result.stdout)
        self.assertIn('doc/system/merge-only.design.md', result.stdout)
        self.assertNotIn('doc/system/main-only.design.md', result.stdout)
        self.assertEqual(before, (self.git('rev-parse', 'HEAD'), self.git('status', '--porcelain')))

    def test_main_only_broken_design_does_not_fail_current_pr(self):
        self.history(pr_design=False, merge_design=False)
        result = self.check_designs()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertNotIn('main-only.design.md', result.stdout)

    def test_checkout_identity_mismatch_fails(self):
        self.history()
        self.assert_bad(self.invoke(tested=self.source))

    def test_wrong_event_pr_head_fails(self):
        self.history()
        self.event['pull_request']['head']['sha'] = self.target
        self.assert_bad(self.invoke())

    def test_one_parent_and_octopus_checkout_fail(self):
        self.history()
        tree = self.git('rev-parse', 'HEAD^{tree}')
        for parents in ((self.source,), (self.target, self.source, self.base)):
            with self.subTest(parents=parents):
                args = ['commit-tree', tree, '-m', 'invalid parent count']
                for parent in parents:
                    args.extend(['-p', parent])
                commit = self.git(*args)
                self.git('checkout', '-q', '--detach', commit)
                self.assert_bad(self.invoke(tested=commit))

    def test_malformed_or_incomplete_event_fails(self):
        self.history()
        for event in ([], {}, {'pull_request': None}, {'pull_request': {'head': {'sha': self.source}}},
                      {'pull_request': {'base': {'sha': self.base}, 'head': {'sha': 'HEAD'}}}):
            with self.subTest(event=event):
                self.assert_bad(self.invoke(event=event))

    def test_missing_or_invalid_json_fails(self):
        self.history()
        path = Path(self.temp.name) / 'invalid-event.json'
        for contents in (None, '{broken'):
            if contents is not None:
                path.write_text(contents)
            result = subprocess.run([sys.executable, str(HELPER), '--repo-root', str(self.root),
                                     '--event-name', 'pull_request', '--event-path', str(path),
                                     '--tested-oid', self.merge], text=True, capture_output=True)
            self.assert_bad(result)

    def test_push_retains_before_after_without_parent_reinterpretation(self):
        self.history()
        event = {'before': self.base, 'after': self.merge}
        result = self.invoke(event, kind='push')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout, f'OASIS7_PRODUCT_DOC_BASE={self.base}\nOASIS7_PRODUCT_DOC_HEAD={self.merge}\n')

    def test_push_zero_missing_or_wrong_after_fails(self):
        self.history()
        for event in ({'before': '0' * 40, 'after': self.merge}, {'before': self.base},
                      {'before': self.base, 'after': self.source},
                      {'before': 'f' * 40, 'after': self.merge}):
            with self.subTest(event=event):
                self.assert_bad(self.invoke(event, kind='push'))

    def test_unsupported_event_fails(self):
        self.history()
        self.assert_bad(self.invoke(kind='workflow_dispatch'))

    def test_explicit_cli_range_is_not_reinterpreted(self):
        self.history()
        driver = (SCRIPTS / 'ci-tests.sh').read_text()
        function = driver.split('product_doc_range() {', 1)[1].split('\nrun_product_doc_governance_check()', 1)[0]
        script = 'product_doc_range() {' + function + '\nproduct_doc_range\n'
        env = dict(os.environ, OASIS7_PRODUCT_DOC_BASE=self.base, OASIS7_PRODUCT_DOC_HEAD=self.source,
                   GITHUB_EVENT_NAME='pull_request', GITHUB_SHA=self.merge, GITHUB_EVENT_PATH='/missing-event')
        result = subprocess.run(['bash', '-c', script], env=env, capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout, f'{self.base}\n{self.source}\n')

    def test_workflow_wiring_is_baseline_only_and_cannot_override_exports(self):
        workflow = (ROOT / '.github/workflows/rust.yml').read_text()
        baseline = workflow.split('\n  baseline:', 1)[1].split('\n  rust-baseline:', 1)[0]
        self.assertEqual(workflow.count('python3 scripts/resolve-product-doc-ci-range.py'), 1)
        self.assertIn('--tested-oid "$GITHUB_SHA" >>"$GITHUB_ENV"', baseline)
        self.assertIn('--event-path "$GITHUB_EVENT_PATH"', baseline)
        self.assertIn('--event-name "$GITHUB_EVENT_NAME"', baseline)
        self.assertLess(baseline.index('Resolve tested product-document range'), baseline.index('Execute selected group'))
        self.assertNotIn('OASIS7_PRODUCT_DOC_BASE:', baseline)
        self.assertNotIn('OASIS7_PRODUCT_DOC_HEAD:', baseline)
        selector = workflow.split('\n  select:', 1)[1].split('\n  baseline:', 1)[0]
        self.assertIn('BASE_SHA: ${{ github.event.pull_request.base.sha || github.event.before }}', selector)
        self.assertEqual(workflow.count("base_oid=\"$(git rev-parse --verify 'HEAD^1')\""), 2)
        driver = (SCRIPTS / 'ci-tests.sh').read_text()
        group = driver.split('run_doc_checker_contract_tests() {', 1)[1].split('\n}', 1)[0]
        self.assertEqual(group.count('run python3 ./scripts/resolve-product-doc-ci-range.test.py'), 1)


if __name__ == '__main__':
    unittest.main()
