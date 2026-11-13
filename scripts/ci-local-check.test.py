#!/usr/bin/env python3
"""Local entry orchestration faults and immutable Git-input regressions.

Small fixture checkers below exercise orchestration, not product acceptance.
Real repository baseline consumers are additionally run through the public CLI.
"""
import contextlib
import io
import importlib.util
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import sys
import tempfile
import time
import unittest
from unittest import mock
from types import SimpleNamespace

HERE = Path(__file__).resolve().parent


def module(name):
    spec = importlib.util.spec_from_file_location(name.replace('-', '_'), HERE / (name + '.py'))
    value = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(value)
    return value


entry = module('check-local')
inputs = module('ci-local-input')
command = module('ci-local-check-command')


class GitFixture(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name) / 'repo'
        self.root.mkdir()
        self.reports = Path(self.temp.name) / 'reports'
        self.git('init', '-q')
        self.git('config', 'user.name', 'Local Check Test')
        self.git('config', 'user.email', 'local@example.invalid')
        self.write('.gitignore', 'target/\noutput/\nignored.txt\ncrates/sample/src/hidden.rs\n')
        self.write('README.md', '# fixture\n')
        self.write('Cargo.toml', '[workspace]\nmembers=["crates/sample"]\nresolver="2"\n')
        self.write('crates/sample/Cargo.toml', '[package]\nname="oasis7_consensus"\nversion="0.1.0"\nedition="2021"\n')
        self.write('crates/sample/src/lib.rs', 'pub fn value() -> usize { 1 }\n')
        self.base = self.commit()
        self.git('update-ref', 'refs/remotes/origin/main', self.base)

    def git(self, *args, env=None):
        return subprocess.check_output(['git', '-C', str(self.root), *args],
                                       env=env or inputs.environment(), stderr=subprocess.PIPE).decode().strip()

    def write(self, path, content, executable=False):
        target = self.root / path
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(content)
        if executable:
            target.chmod(0o755)
        return target

    def commit(self):
        self.git('add', '.')
        self.git('commit', '-qm', 'fixture')
        return self.git('rev-parse', 'HEAD')

    def state(self):
        return (self.git('rev-parse', 'HEAD'), (self.root / '.git/index').read_bytes(),
                self.git('show-ref'), {path.relative_to(self.root).as_posix(): path.read_bytes()
                                      for path in self.root.rglob('*') if path.is_file()
                                      and '.git' not in path.relative_to(self.root).parts})

    def install_policy(self, real=False, groups=('baseline',)):
        for name in ('ci-required-scope.json', 'ci-formal-process-suites.json'):
            self.write('scripts/' + name, (HERE / name).read_text() if real else json.dumps({'groups': list(groups)}))
        if real:
            self.write('scripts/plan-rust-required-scope.py', (HERE / 'plan-rust-required-scope.py').read_text())
        else:
            self.write('scripts/plan-rust-required-scope.py',
                       'import json, pathlib, sys\n'
                       f'plan = {dict(groups=list(groups), reasons=["fixture policy"], scope="targeted")!r}\n'
                       'pathlib.Path(sys.argv[sys.argv.index("--output") + 1]).write_text(json.dumps(plan))\n'
                       'print(json.dumps(plan))\n')
        self.base = self.commit()
        self.git('update-ref', 'refs/remotes/origin/main', self.base)


class InputCapture(GitFixture):
    def test_inactive_missing_fixture_is_versioned_as_absent_until_it_appears(self):
        self.write('crates/sample/src/lib.rs',
                   '#[cfg(feature="optional")] const DATA: &str = include_str!("../../../ignored.txt");\n')
        snapshot = inputs.capture(self.root, self.base)
        extra = inputs.referenced_inputs(self.root, snapshot, HERE / 'plan-rust-required-scope.py')
        self.assertIn('ignored.txt', extra)
        before = inputs.capture(self.root, self.base, extra)
        self.assertIn('ignored.txt', before['summary']['absent_referenced_paths'])
        self.assertNotIn('ignored.txt', before['files'])
        self.write('ignored.txt', 'generated during run\n')
        after = inputs.capture(self.root, self.base, extra)
        self.assertNotEqual(before['summary'], after['summary'])
        self.assertNotIn('ignored.txt', after['summary']['absent_referenced_paths'])

    def test_custom_ignored_cargo_target_build_and_modules_are_in_read_set(self):
        with (self.root / '.gitignore').open('a') as stream:
            stream.write('crates/sample/private/\ncrates/sample/builders/\n')
        self.write('crates/sample/Cargo.toml', '[package]\nname="oasis7_consensus"\nversion="0.1.0"\n'
                   'build="builders/custom.rs"\n[lib]\npath="private/lib.rs"\n')
        expected = {'crates/sample/private/lib.rs', 'crates/sample/private/child.rs',
                    'crates/sample/builders/custom.rs'}
        for path in expected:
            self.write(path, 'fn main() {}\n')
        snapshot = inputs.capture(self.root, self.base)
        self.assertEqual(inputs.referenced_inputs(self.root, snapshot, HERE / 'plan-rust-required-scope.py'), expected)

    def test_ignored_directory_symlink_cannot_hide_baseline_input(self):
        outside = Path(self.temp.name) / 'foreign'
        outside.mkdir()
        (outside / 'SKILL.md').write_text('external input')
        (self.root / '.agents/skills').mkdir(parents=True)
        with (self.root / '.gitignore').open('a') as stream:
            stream.write('.agents/skills/foreign\n')
        os.symlink(outside, self.root / '.agents/skills/foreign')
        with self.assertRaisesRegex(inputs.Blocked, 'symlink directory'):
            inputs.capture(self.root, self.base)

    def test_filters_are_blocked_before_execution_and_eol_bytes_stay_dirty(self):
        marker = Path(self.temp.name) / 'filter-executed'
        self.git('config', 'filter.unsafe.clean', f'touch {marker}; cat')
        self.write('.gitattributes', '*.rs filter=unsafe\n')
        with self.assertRaisesRegex(inputs.Blocked, 'content transformation'):
            inputs.capture(self.root, self.base)
        self.assertFalse(marker.exists())
        self.write('.gitattributes', '*.rs text eol=lf\n')
        self.base = self.commit()
        source = self.root / 'crates/sample/src/lib.rs'
        source.write_bytes(b'pub fn value() -> usize { 1 }\r\n')
        self.assertEqual(self.git('diff', '--name-only'), '')
        snapshot = inputs.capture(self.root, self.base)
        self.assertTrue(snapshot['summary']['dirty'])
        self.assertIn(dict(origin='unstaged', status='M', path='crates/sample/src/lib.rs'),
                      snapshot['summary']['changes'])

    def test_private_trees_preserve_every_origin_and_do_not_write_user_git(self):
        self.write('README.md', '# committed\n')
        self.commit()
        self.write('README.md', '# staged\n')
        self.git('add', 'README.md')
        self.write('README.md', '# committed\n')
        self.write('doc/new name\n雪.md', '# new\n')
        self.git('mv', 'crates/sample/src/lib.rs', 'crates/sample/src/renamed.rs')
        original = self.state()
        objects = sorted(path.relative_to(self.root / '.git/objects').as_posix()
                         for path in (self.root / '.git/objects').rglob('*'))
        captured = inputs.capture(self.root, self.base)
        with tempfile.TemporaryDirectory() as directory:
            index, tree, env = inputs.private_trees(self.root, captured, Path(directory))
            self.assertEqual(self.git('show', index + ':README.md', env=env), '# staged')
            self.assertEqual(self.git('show', tree + ':README.md', env=env), '# committed')
            self.assertEqual(self.git('show', tree + ':doc/new name\n雪.md', env=env), '# new')
        origins = {item['origin'] for item in captured['summary']['changes']}
        self.assertEqual(origins, {'committed', 'staged', 'unstaged', 'untracked'})
        renamed = [item for item in captured['summary']['changes'] if item['status'].startswith('R')]
        self.assertEqual(renamed[0]['old_path'], 'crates/sample/src/lib.rs')
        self.assertEqual(self.state(), original)
        self.assertEqual(objects, sorted(path.relative_to(self.root / '.git/objects').as_posix()
                                        for path in (self.root / '.git/objects').rglob('*')))

    def test_ignored_rust_modules_and_literal_inputs_are_captured(self):
        self.write('crates/sample/src/lib.rs', 'mod hidden;\n#[path="other.rs"] mod other;\n'
                   'const DATA: &str = include_str!("../../../ignored.txt");\n')
        self.write('crates/sample/src/hidden.rs', 'pub fn hidden() {}\n')
        self.write('crates/sample/src/other.rs', 'pub fn other() {}\n')
        self.write('ignored.txt', 'input\n')
        snapshot = inputs.capture(self.root, self.base)
        extra = inputs.referenced_inputs(self.root, snapshot, HERE / 'plan-rust-required-scope.py')
        self.assertEqual(extra, {'ignored.txt', 'crates/sample/src/hidden.rs'})
        expanded = inputs.capture(self.root, self.base, extra)
        self.assertIn('crates/sample/src/hidden.rs', expanded['files'])
        self.assertIn('ignored.txt', expanded['files'])
        self.write('ignored.txt', 'changed\n')
        self.assertNotEqual(inputs.capture(self.root, self.base, extra)['summary'], expanded['summary'])

    def test_symlink_unreadable_and_external_dependency_block(self):
        if os.name == 'posix':
            os.symlink('ignored.txt', self.root / 'link')
            with self.assertRaisesRegex(inputs.Blocked, 'symlink'):
                inputs.capture(self.root, self.base)
            (self.root / 'link').unlink()
            path = self.write('unreadable', 'input')
            path.chmod(0)
            with self.assertRaisesRegex(inputs.Blocked, 'unreadable'):
                inputs.capture(self.root, self.base)
            path.chmod(0o600)
            path.unlink()
        self.write('crates/sample/Cargo.toml', '[package]\nname="oasis7_consensus"\nversion="0.1.0"\n'
                   '[dependencies]\nexternal={path="../../../../external"}\n')
        with self.assertRaisesRegex(inputs.Blocked, 'external referenced'):
            inputs.referenced_inputs(self.root, inputs.capture(self.root, self.base), HERE / 'plan-rust-required-scope.py')

    def test_index_change_during_capture_is_rejected(self):
        original = inputs.file_bytes
        edited = False
        def edit_index(root, path):
            nonlocal edited
            if not edited:
                edited = True
                self.write('README.md', 'new staged\n')
                self.git('add', 'README.md')
            return original(root, path)
        with mock.patch.object(inputs, 'file_bytes', edit_index):
            with self.assertRaisesRegex(inputs.Blocked, 'changed while capturing'):
                inputs.capture(self.root, self.base)

    def test_real_trusted_planner_reads_dirty_tree_and_old_policy(self):
        self.install_policy(real=True)
        self.write('README.md', '# staged\n')
        self.git('add', 'README.md')
        self.write('README.md', '# fixture\n')
        self.write('doc/new.md', '# new\n')
        result = subprocess.run([sys.executable, str(HERE / 'check-local.py'), '--profile', 'affected',
                                 '--dry-run', '--format', 'json', '--output-dir', str(self.reports)],
                                cwd=self.root, capture_output=True, text=True, timeout=30)
        self.assertEqual(result.returncode, 0, result.stderr)
        report = json.loads(result.stdout)
        self.assertEqual(report['status'], 'planned')
        self.assertEqual(report['plan']['groups'], ['baseline', 'site_quality'])
        self.assertEqual(report['policy_sha'], self.base)
        self.assertEqual(report['plan']['source_sha'], self.base)
        local = report['plan']['local_inputs']
        self.assertNotEqual(local['index_tree'], local['worktree_tree'])
        self.assertEqual({item['origin'] for item in local['changes']}, {'staged', 'unstaged', 'untracked'})
        self.assertFalse(report['validated'])


class Evidence(unittest.TestCase):
    def test_structured_completion_rejects_boolean_counts_and_truthy_flags(self):
        argv = ['python3', 'scripts/ci-python-control-contracts.py', 'local-contract']
        valid = dict(expected_count=2, passed=True, count_ok=True)
        for field, value in [('expected_count', True), ('expected_count', 0),
                             ('passed', 1), ('passed', 'true'), ('count_ok', 1), ('count_ok', 'yes')]:
            with self.subTest(field=field, value=value):
                report = command.evidence(argv, json.dumps(dict(valid, **{field: value})), 0,
                                          'doc_checker_contracts')
                self.assertEqual(report['status'], 'incomplete')
        self.assertEqual(command.evidence(argv, json.dumps(valid), 7, 'doc_checker_contracts')['status'], 'failed')

    def test_engineering_cases_are_tool_evidence_and_missing_summary_is_incomplete(self):
        for group in entry.ENGINEERING_CHECKS:
            report = command.evidence(['python3', 'scripts/checker.test.py'], 'Ran 2 tests in 0.1s\nOK\n', 0, group)
            self.assertEqual((report['tests_executed'], report['business_tests_executed']), (2, 0))
            self.assertEqual(command.evidence(['python3', 'scripts/checker.test.py'], 'OK', 0, group)['status'],
                             'incomplete')


    def test_viewer_tap_flat_requires_the_complete_unique_plan_before_other_summaries(self):
        argv = ['npm', '--prefix', 'crates/oasis7_viewer', 'run', 'test:ui', '--', '--reporter=tap-flat']
        valid = 'TAP version 13\n1..2\nok 1 first\nok 2 second\n'
        self.assertEqual(command.evidence(argv, valid, 0, 'viewer_js_required')['tests_executed'], 2)
        invalid = ['', 'TAP version 13\n1..0\n', 'TAP version 13\n1..2\nok 1 first\n',
                   valid.replace('ok 2', 'ok 1'), valid.replace('ok 2', 'ok 3'),
                   valid + '1..2\n', valid + 'Bail out! broken runner\n',
                   valid.replace('1..2', '1..2 # SKIP whole suite'),
                   valid.replace('1..2', '1..2 # TODO pending'),
                   valid.replace('second', 'second # SKIP unavailable'),
                   valid.replace('second', 'second # TODO pending'), valid.replace('ok 2', 'not ok 2')]
        for output in invalid:
            with self.subTest(output=output):
                for decoy in ['', '{"expected_count":2,"count_ok":true,"passed":true}\n# tests 2\n# pass 2\n']:
                    self.assertEqual(command.evidence(argv, output + decoy, 0, 'viewer_js_required')['status'], 'incomplete')
        self.assertEqual(command.evidence(argv, valid, 17, 'viewer_js_required')['status'], 'failed')
        self.assertEqual(command.evidence(argv[:-2], '', 0, 'viewer_js_required')['status'], 'incomplete')

    def test_viewer_frontend_and_browser_require_their_actual_execution_evidence(self):
        frontend = ['npm', '--prefix', 'crates/oasis7_viewer', 'run', 'test:frontend-structure']
        browser = ['node', 'crates/oasis7_viewer/scripts/browser-functional-regression.mjs', '--skip-build']
        structured = '{"expected_count":26,"count_ok":true,"passed":true}\n'
        node = '# tests 2\n# pass 2\n'
        self.assertEqual(command.evidence(frontend, node, 0, 'viewer_js_required')['status'], 'passed')
        self.assertEqual(command.evidence(frontend, structured, 0, 'viewer_js_required')['status'], 'incomplete')
        self.assertEqual(command.evidence(frontend, '# tests 0\n# pass 0\n', 0, 'viewer_js_required')['status'], 'incomplete')
        self.assertEqual(command.evidence(browser, node, 0, 'viewer_js_required')['status'], 'incomplete')
        self.assertEqual(command.evidence(browser, structured, 0, 'viewer_js_required')['tests_executed'], 26)
        for invalid in [structured.replace('26', 'true'), structured.replace('true', '"true"')]:
            self.assertEqual(command.evidence(browser, invalid, 0, 'viewer_js_required')['status'], 'incomplete')

    def test_command_summary_preserves_flat_tap_and_rejects_fixture_environment_leaks(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            log = root / 'output.log'
            log.write_text('unrelated output\nTAP version 13\n1..2\nok 1 yes\nnot ok 2 no # SKIP\nBail out!\n')
            self.assertEqual(command.summaries(log), log.read_text().replace('unrelated output\n', ''))
            fixture = root / 'environment.test.py'
            fixture.write_text('import os,unittest\nclass Fixture(unittest.TestCase):\n'
                               ' def test_environment(self):\n'
                               '  self.assertFalse(any(k.startswith(("OASIS7_LOCAL_CHECK","AGENT_BROWSER_")) for k in os.environ))\n'
                               '  os.environ["OASIS7_LOCAL_CHECK"]="1"\n'
                               '  self.assertEqual(os.environ["OASIS7_LOCAL_CHECK"],"1")\n'
                               'unittest.main()\n')
            env = dict(os.environ, OASIS7_LOCAL_CHECK='1', OASIS7_LOCAL_CHECK_GROUP='viewer_js_required',
                       OASIS7_LOCAL_CHECK_RUN_DIR=str(root), AGENT_BROWSER_SESSION='foreign-session')
            completed = subprocess.run([sys.executable, str(HERE / 'ci-local-check-command.py'), '--',
                                        sys.executable, str(fixture)], env=env, capture_output=True, text=True)
            self.assertEqual(completed.returncode, 0, completed.stdout + completed.stderr)
            record, = [json.loads(path.read_text()) for path in (root / 'commands').glob('*.json')]
            self.assertEqual((record['tests_executed'], record['business_tests_executed']), (1, 0))

    def test_counts_require_real_completion_and_reject_zero_skip_unknown(self):
        scenarios = [
            ('running 2 tests\ntest result: ok. 2 passed; 0 failed; 0 ignored;', 'passed', 2),
            ('running 2 tests\ntest result: ok. 1 passed; 0 failed; 1 ignored;', 'incomplete', 1),
            ('running 0 tests\ntest result: ok. 0 passed; 0 failed; 0 ignored;', 'incomplete', 0),
            ('running 2 tests\ntest result: ok. 1 passed; 0 failed; 0 ignored;', 'incomplete', 1),
            ('success without a summary', 'incomplete', None),
        ]
        for output, status, count in scenarios:
            with self.subTest(output=output):
                report = command.evidence(['cargo', 'test'], output, 0, 'consensus')
                self.assertEqual((report['status'], report['tests_executed']), (status, count))
        for output, status in [('Ran 2 tests in 0.001s\nOK\n', 'passed'),
                               ('Ran 2 tests in 0.001s\nOK (skipped=1)\n', 'incomplete'),
                               ('Ran 2 tests in 0.001s\n', 'incomplete')]:
            self.assertEqual(command.evidence(['python3', 'script.test.py'], output, 0, 'net')['status'], status)

    def test_baseline_and_shell_validators_never_invent_business_counts(self):
        report = command.evidence(['bash', 'script.test.sh'], 'validator OK', 0, 'rust_baseline')
        self.assertEqual(report['tests_executed'], 0)
        self.assertEqual(report['status'], 'passed')
        report = command.evidence(['bash', './scripts/lint-skills.sh'], 'Ran 13 tests in 0.3s\nOK\n', 0, 'baseline')
        self.assertEqual((report['tests_executed'], report['business_tests_executed']), (13, 0))


class Entry(GitFixture):
    def setUp(self):
        super().setUp()
        # Real driver + real subprocess/receipt supervision, small fake
        # checkers for deterministic orchestration and failure injection.
        self.write('scripts/ci-tests.sh', (HERE / 'ci-tests.sh').read_text(), True)
        self.write('scripts/viewer-dependency-preflight.sh', (HERE / 'viewer-dependency-preflight.sh').read_text(), True)
        self.write('scripts/find-python-with-module.sh', f'#!/bin/sh\nprintf "%s\\n" "{sys.executable}"\n', True)
        for name in ('unified-world-code-terminology-scan.test.sh', 'doc-governance-check.sh', 'lint-skills.sh',
                     'check-windows-paths.sh', 'check-script-executable-bits.sh'):
            self.write('scripts/' + name, '#!/bin/sh\necho fixture-check-completed\n', True)
        self.write('scripts/validate-codex-agent-config.py', 'assert 2 + 2 == 4\nprint("fixture checked")\n')
        self.install_policy()

    def invoke(self, *args, env=None, timeout=30):
        return subprocess.run([sys.executable, str(HERE / 'check-local.py'), '--format', 'json',
                               '--output-dir', str(self.reports), *args], cwd=self.root,
                              env=env, capture_output=True, text=True, timeout=timeout)

    def assert_result(self, process, status, code):
        self.assertEqual(process.returncode, code, process.stderr)
        report = json.loads(process.stdout)
        self.assertEqual(report['status'], status, report.get('reason'))
        self.assertEqual(report['validated'], status == 'passed')
        if report['logs'].get('report'):
            self.assertEqual(json.loads(Path(report['logs']['report']).read_bytes()), report)
        return report

    def test_quick_affected_and_formal_required_execute_same_driver(self):
        before = self.state()
        for args in (('--profile', 'quick'), ('--profile', 'affected'),
                     ('--profile', 'formal', '--tier', 'required')):
            with self.subTest(args=args):
                report = self.assert_result(self.invoke(*args), 'passed', 0)
                self.assertEqual(report['checks_expected'], 10)
                self.assertEqual(report['checks_executed'], 10)
                self.assertEqual(report['business_tests_executed'], 0)
                self.assertEqual(report['groups'][0]['name'], 'baseline')
                self.assertEqual(self.state(), before)

    def test_plan_reports_public_evidence_without_environment_values(self):
        sentinel = 'local-check-environment-value-must-stay-out-of-reports'
        env = dict(os.environ, LOCAL_ENTRY_FIXTURE_API_KEY=sentinel)
        for output_format in ('json', 'text'):
            with self.subTest(output_format=output_format):
                process = subprocess.run(
                    [sys.executable, str(HERE / 'check-local.py'), '--profile', 'affected',
                     '--dry-run', '--format', output_format, '--output-dir', str(self.reports)],
                    cwd=self.root, env=env, capture_output=True, text=True, timeout=30)
                self.assertEqual(process.returncode, 0, process.stderr)
                payload = (process.stdout if output_format == 'json'
                           else process.stdout.split('Full result:\n', 1)[1])
                report = json.loads(payload)
                self.assertEqual(report['status'], 'planned')
                self.assertFalse(report['validated'])
                self.assertEqual([group['name'] for group in report['groups']], ['baseline'])
                self.assertEqual(report['plan']['groups'], ['baseline'])
                self.assertEqual(report['selection_reasons'], ['fixture policy'])
                self.assertEqual(report['policy_sha'], self.base)
                self.assertEqual(json.loads(Path(report['logs']['report']).read_text()), report)
                self.assertNotIn(sentinel, process.stdout + process.stderr)
                for path in Path(report['logs']['directory']).rglob('*'):
                    if path.is_file():
                        self.assertNotIn(sentinel, path.read_text(), str(path))

    def test_arguments_and_formal_dirty_are_blocked(self):
        for args in ((), ('--profile', 'formal'), ('--profile', 'quick', '--tier', 'required'),
                     ('--profile', 'quick', '--timeout-seconds', 'nan'),
                     ('--profile', 'quick', '--base-ref', 'missing')):
            self.assert_result(self.invoke(*args), 'blocked', 2)
        self.write('README.md', '# changed\n')
        self.assert_result(self.invoke('--profile', 'formal', '--tier', 'required'), 'blocked', 2)

    def test_dirty_quick_and_dirty_unsupported_policy_are_distinct(self):
        self.write('README.md', '# changed\n')
        self.assert_result(self.invoke('--profile', 'quick'), 'passed', 0)
        report = self.assert_result(self.invoke('--profile', 'affected', '--dry-run'), 'blocked', 2)
        self.assertIn('snapshot capability', report['reason'])

    def test_old_policy_cannot_be_replaced_by_candidate_and_unknown_stays_blocked(self):
        self.write('scripts/plan-rust-required-scope.py', 'raise RuntimeError("candidate executed")\n')
        self.commit()
        report = self.assert_result(self.invoke('--profile', 'affected', '--dry-run'), 'planned', 0)
        self.assertEqual(report['plan']['reasons'], ['fixture policy'])
        self.assertEqual(report['policy_sha'], self.base)

    def test_incomplete_viewer_checkout_blocks_before_rust_input_discovery(self):
        # This minimal trusted planner can select groups but cannot discover
        # Rust references. Missing Viewer sources must block before that stage.
        for group in ('viewer_js_required', 'viewer_performance_report'):
            self.install_policy(groups=('baseline', group))
            before = self.state()
            report = self.assert_result(self.invoke('--profile', 'affected'), 'blocked', 2)
            self.assertIn('missing Viewer prerequisite source', report['reason'])
            self.assertEqual(report['checks_executed'], 0)
            self.assertEqual(report['groups'][1]['status'], 'blocked')
            self.assertEqual(self.state(), before)

    def test_selected_unsupported_group_and_full_block_before_any_driver(self):
        self.install_policy(groups=('baseline', 'command_center'))
        for args in (('--profile', 'affected'), ('--profile', 'formal', '--tier', 'full')):
            report = self.assert_result(self.invoke(*args), 'blocked', 2)
            self.assertEqual(report['checks_executed'], 0)
            self.assertEqual(report['groups'][1]['status'], 'blocked')

    def test_engineering_rust_prerequisites_block_before_running_any_group(self):
        available = shutil.which
        for group in ('cargo_tooling_contracts', 'compile_metrics'):
            result = dict(groups=[dict(name='baseline'), dict(name=group)], configuration={})
            with mock.patch.object(entry.shutil, 'which', side_effect=lambda name, **kw:
                                   None if name in ('cargo', 'rustc', 'rustup', 'zstdmt') else available(name, **kw)):
                with self.assertRaisesRegex(entry.Blocked, 'missing tool: cargo'):
                    entry.preflight(self.root, SimpleNamespace(tier='required', offline=True), result,
                                    entry.execution_environment())
            if group == 'compile_metrics':
                self.assertIn('missing tool: zstdmt', result['preconditions'])
        self.assertTrue(entry.RUST_GROUPS.isdisjoint(entry.ENGINEERING_CHECKS))

    def test_document_interpreter_is_checked_before_any_contract_command(self):
        python = self.write('not-ready-python', '#!/bin/sh\nexit 17\n', True)
        self.write('scripts/find-python-with-module.sh',
                   '#!/bin/sh\nprintf "%s\\n" "' + str(python) + '"\n', True)
        self.install_policy(groups=('doc_checker_contracts',))
        report = self.assert_result(self.invoke('--profile', 'affected'), 'blocked', 2)
        self.assertEqual(report['checks_executed'], 0)
        self.assertIn('one Python with tomllib and markdown_it', report['reason'])

    def test_compile_metrics_fixture_worktrees_never_register_in_the_callers_git(self):
        source = (HERE / 'ci-compile-metrics-contract.test.sh').read_text()
        setup = source[source.index('fixture_git='):source.index('mutation_fake_bin=')]
        add = next(line for line in source.splitlines() if 'worktree add --detach "$mutation_worktree"' in line)
        remove = next(line for line in source.splitlines() if 'worktree remove --force "$mutation_worktree"' in line)
        before = self.git('worktree', 'list', '--porcelain')
        for interrupted in (False, True):
            output = Path(self.temp.name) / ('interrupted' if interrupted else 'normal')
            output.mkdir()
            shell = ('set -euo pipefail\nrepo_root="$1"; tmp_dir="$2"; expected_commit_oid="$3"\n'
                     'mutation_worktree="$tmp_dir/worktree"\n' + setup + '\n' + add + '\n'
                     + ('exit 19\n' if interrupted else remove + '\n'))
            process = subprocess.run(['bash', '-c', shell, 'metrics-fixture', str(self.root), str(output), self.base],
                                     cwd=self.root, text=True, capture_output=True)
            self.assertEqual(process.returncode, 19 if interrupted else 0, process.stderr)
            self.assertEqual(self.git('worktree', 'list', '--porcelain'), before)
            if interrupted:
                self.assertTrue((output / 'worktree/.git').is_file())
                self.assertIn(str(output / 'worktree-fixture.git'), (output / 'worktree/.git').read_text())

    def test_document_contract_group_requires_every_command_and_real_counts(self):
        document_python = subprocess.check_output(['bash', str(HERE / 'find-python-with-module.sh'),
                                                   'markdown_it'], text=True).strip()
        self.write('scripts/find-python-with-module.sh',
                   '#!/bin/sh\nprintf "%s\\n" "' + document_python + '"\n', True)
        self.write('scripts/ci-python-control-contracts.py', (HERE / 'ci-python-control-contracts.py').read_text())
        for name in entry.ENGINEERING_CHECKS['doc_checker_contracts']:
            if name in ('product-doc-governance-check.test.py', 'product-doc-content-check.test.py',
                        'system-design-traceability-check.test.py'):
                source = 'def main():\n    assert 2 + 2 == 4\n\nif __name__ == "__main__":\n    main()\n'
            elif name.endswith('.py'):
                source = ('import unittest\nclass Test(unittest.TestCase):\n'
                          '    def test_contract(self):\n        self.assertEqual(2 + 2, 4)\n'
                          'if __name__ == "__main__":\n    unittest.main()\n')
            else:
                source = ('#!/bin/sh\nset -eu\n'
                          'test -z "${OASIS7_LOCAL_CHECK:-}"\n'
                          'test -z "${OASIS7_LOCAL_CHECK_RUN_DIR:-}"\n'
                          'test -z "${OASIS7_PRODUCT_DOC_BASE:-}"\n'
                          'echo fixture contract passed\n')
            self.write('scripts/' + name, source, executable=True)
        self.install_policy(groups=('doc_checker_contracts',))
        before = self.state()
        report = self.assert_result(self.invoke('--profile', 'affected'), 'passed', 0)
        self.assertEqual((report['checks_expected'], report['checks_executed'], report['tests_executed'],
                          report['business_tests_executed']), (11, 11, 8, 0))
        self.assertEqual(report['configuration']['document_python'], str(Path(document_python).resolve()))
        self.assertEqual(self.state(), before)
        # One missing stage cannot turn a partial driver into a successful group.
        driver = self.root / 'scripts/ci-tests.sh'
        driver.write_text(driver.read_text().replace('  run python3 ./scripts/doc-core-content-check.test.py\n', ''))
        self.commit()
        report = self.assert_result(self.invoke('--profile', 'affected'), 'incomplete', 1)
        self.assertIn('inventory', report['reason'])

    def test_failure_stops_later_commands(self):
        self.write('scripts/doc-governance-check.sh', '#!/bin/sh\nexit 19\n', True)
        report = self.assert_result(self.invoke('--profile', 'quick'), 'failed', 1)
        commands = report['groups'][0]['commands']
        self.assertEqual(len(commands), 2)
        self.assertEqual(commands[-1]['exit_code'], 19)

    def test_source_change_during_check_is_stale(self):
        self.write('scripts/doc-governance-check.sh', '#!/bin/sh\necho changed >> README.md\n', True)
        report = self.assert_result(self.invoke('--profile', 'quick'), 'stale_input', 2)
        self.assertIn('changed during', report['reason'])

    def test_timeout_and_cancel_kill_owned_descendants_only(self):
        marker = Path(self.temp.name) / 'child.pid'
        self.write('scripts/doc-governance-check.sh',
                   '#!/bin/sh\nsleep 60 &\necho "$!" > "$LOCAL_ENTRY_FIXTURE_MARKER"\nwait\n', True)
        env = dict(os.environ, LOCAL_ENTRY_FIXTURE_MARKER=str(marker))
        report = self.assert_result(self.invoke('--profile', 'quick', '--timeout-seconds', '1', env=env), 'failed', 1)
        self.assertTrue(report['groups'][0]['timeout'])
        self.assertEqual(report['groups'][0]['cleanup_result'], 'clean')
        marker.unlink(missing_ok=True)
        process = subprocess.Popen([sys.executable, str(HERE / 'check-local.py'), '--profile', 'quick',
                                    '--format', 'json', '--output-dir', str(self.reports)],
                                   cwd=self.root, env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        try:
            deadline = time.monotonic() + 10
            while not marker.exists() and time.monotonic() < deadline:
                time.sleep(.02)
            self.assertTrue(marker.exists())
            process.send_signal(signal.SIGINT)
            stdout, stderr = process.communicate(timeout=15)
        finally:
            if process.poll() is None:
                process.kill()
                process.communicate()
        report = self.assert_result(subprocess.CompletedProcess(process.args, process.returncode, stdout, stderr), 'cancelled', 130)
        self.assertEqual(report['groups'][0]['cleanup_result'], 'clean')
        pid = int(marker.read_text())
        state = Path(f'/proc/{pid}/stat')
        self.assertTrue(not state.exists() or state.read_text().rsplit(')', 1)[1].split()[0] == 'Z')

    def test_report_parent_cannot_be_source_or_checker_read_tree(self):
        for path in (self.root, self.root / 'scripts/ignored-output'):
            with self.assertRaises(inputs.Blocked):
                # Entry's helper uses its own module instance of Blocked.
                try:
                    entry.report_directory(self.root, str(path))
                except entry.Blocked as error:
                    raise inputs.Blocked(str(error)) from error

    def test_report_write_failure_never_leaves_success_summary(self):
        real_write = Path.write_text
        def write(path, data, *args, **kwargs):
            if path.name == 'result.tmp':
                raise OSError('injected full report volume')
            return real_write(path, data, *args, **kwargs)
        previous = Path.cwd()
        output = io.StringIO()
        try:
            os.chdir(self.root)
            with mock.patch.object(Path, 'write_text', write), contextlib.redirect_stdout(output):
                code = entry.main(['--profile', 'quick', '--dry-run', '--format', 'json',
                                   '--output-dir', str(self.reports)])
        finally:
            os.chdir(previous)
        report = json.loads(output.getvalue())
        self.assertEqual((code, report['status'], report['validated']), (1, 'failed', False))
        self.assertIsNone(report['logs']['report'])
        self.assertFalse(list(self.reports.rglob('result.json')))

    def test_actual_cargo_target_is_resolved_before_any_build(self):
        bins = Path(self.temp.name) / 'bin'
        bins.mkdir()
        target = str(Path(self.temp.name) / 'external-target')
        for name in ('cargo', 'rustc', 'rustup'):
            path = bins / name
            path.write_text('#!/bin/sh\nprintf \'%s\\n\' \'' + json.dumps({'target_directory': target}) + '\'\n')
            path.chmod(0o755)
        env = dict(os.environ, PATH=str(bins) + os.pathsep + os.environ['PATH'],
                   CARGO_HOME=str(Path(self.temp.name) / 'cargo-home'))
        result = dict(groups=[dict(name='consensus', status='not_run')], configuration={})
        with self.assertRaisesRegex(entry.Blocked, 'external/nonstandard target'):
            entry.preflight(self.root, SimpleNamespace(tier='required', offline=False), result, env)
        self.assertFalse(Path(target).exists())
        self.assertIn('--locked', result['cargo_preflight']['argv'])
        self.assertIn('--offline', result['cargo_preflight']['argv'])

    @unittest.skipUnless(Path('/proc').is_dir(), 'Linux ownership enumeration')
    def test_nested_supervisor_timeout_reaps_term_ignoring_leaf(self):
        marker = Path(self.temp.name) / 'nested-leaf.pid'
        leaf = Path(self.temp.name) / 'leaf.py'
        leaf.write_text('import os, pathlib, signal, time\n'
                        'signal.signal(signal.SIGTERM, signal.SIG_IGN)\n'
                        f'pathlib.Path({str(marker)!r}).write_text(str(os.getpid()))\n'
                        'time.sleep(60)\n')
        supervisor = Path(self.temp.name) / 'supervisor.py'
        supervisor.write_text('import os, pathlib, sys\n'
                              f'sys.path.insert(0, {str(HERE)!r})\n'
                              'from ci_process_runner import run\n'
                              f'run([sys.executable, {str(leaf)!r}], pathlib.Path.cwd(), os.environ.copy(), '
                              f'"inner", "leaf", pathlib.Path({str(Path(self.temp.name) / "inner.jsonl")!r}), '
                              'timeout_seconds=30)\n')
        self.write('scripts/doc-governance-check.sh',
                   f'#!/bin/sh\nexec "{sys.executable}" "{supervisor}"\n', True)
        unrelated = subprocess.Popen([sys.executable, '-c', 'import time; time.sleep(60)'])
        try:
            report = self.assert_result(self.invoke('--profile', 'quick', '--timeout-seconds', '1.5'), 'failed', 1)
            self.assertTrue(marker.exists(), report['reason'])
            self.assertIsNone(unrelated.poll())
            state = Path(f'/proc/{int(marker.read_text())}/stat')
            self.assertTrue(not state.exists() or state.read_text().rsplit(')', 1)[1].split()[0] == 'Z')
            self.assertEqual(report['groups'][0]['cleanup_result'], 'clean')
        finally:
            unrelated.terminate()
            unrelated.wait(timeout=5)

    def test_empty_plan_is_not_success_and_unique_reports_preserve_previous(self):
        self.install_policy(groups=())
        report = self.assert_result(self.invoke('--profile', 'affected', '--dry-run'), 'no_checks', 3)
        path = Path(report['logs']['report'])
        before = path.read_bytes()
        again = self.assert_result(self.invoke('--profile', 'affected', '--dry-run'), 'no_checks', 3)
        self.assertNotEqual(path, Path(again['logs']['report']))
        self.assertEqual(path.read_bytes(), before)


class SupportContracts(GitFixture):
    """The real support dispatcher/receipt runner, with an explicit Cargo double.

    These cases prove local wiring and failure semantics, not Rust execution.
    """

    def setUp(self):
        super().setUp()
        for name in ('ci-tests.sh', 'viewer-dependency-preflight.sh',
                     'plan-rust-required-scope.py'):
            self.write('scripts/' + name, (HERE / name).read_text(), True)
        self.write('scripts/find-python-with-module.sh',
                   f'#!/bin/sh\nprintf "%s\\n" "{sys.executable}"\n', True)
        self.calls = Path(self.temp.name) / 'cargo-calls.jsonl'
        binaries = Path(self.temp.name) / 'bin'
        binaries.mkdir()
        cargo = binaries / 'cargo'
        cargo.write_text(
            f'#!{sys.executable}\n'
            'import json, os, pathlib, sys\n'
            'args = sys.argv[1:]\n'
            'with open(os.environ["LOCAL_SUPPORT_CALLS"], "a") as stream:\n'
            '    stream.write(json.dumps(args) + "\\n")\n'
            'if args[:1] == ["--verbose"]: args = args[1:]\n'
            'if args[0] == "metadata":\n'
            '    print(json.dumps({"target_directory": str(pathlib.Path.cwd() / "target")}))\n'
            'elif args[0] == "test":\n'
            '    mode = os.environ.get("LOCAL_SUPPORT_MODE", "passed")\n'
            '    if mode == "failed": sys.exit(19)\n'
            '    if mode == "unknown": print("finished without case evidence"); sys.exit(0)\n'
            '    count = 0 if mode == "zero" else 1\n'
            '    ignored = 1 if mode == "skipped" else 0\n'
            '    print(f"running {count} tests")\n'
            '    print(f"test result: ok. {count - ignored} passed; 0 failed; {ignored} ignored;")\n'
            'elif args[0] != "clippy":\n'
            '    raise SystemExit("unexpected Cargo operation: " + repr(args))\n')
        cargo.chmod(0o755)
        for name in ('rustc', 'rustup'):
            binary = binaries / name
            binary.write_text('#!/bin/sh\nexit 0\n')
            binary.chmod(0o755)
        self.env = dict(entry.execution_environment(), PATH=str(binaries) + os.pathsep + os.environ['PATH'],
                        CARGO_HOME=str(Path(self.temp.name) / 'cargo-home'),
                        LOCAL_SUPPORT_CALLS=str(self.calls))
        self.base = self.commit()

    def result(self, profiles=('oasis7_signer_protocol',), **changes):
        plan = dict(scope='targeted', groups=['workspace_support'], test_policy_changed=False,
                    workspace_support_profile_version=1, workspace_support_profiles=list(profiles))
        plan.update(changes)
        return dict(groups=[dict(name='workspace_support', status='not_run')], plan=plan,
                    configuration={}, checks_executed=0, base_sha=self.base, source_sha=self.base)

    def execute(self, result, **environment):
        directory = Path(tempfile.mkdtemp(prefix='support-', dir=self.temp.name))
        (directory / 'plan.json').write_text(json.dumps(result['plan']))
        entry.execute(self.root, SimpleNamespace(tier='required', offline=False, timeout_seconds=15),
                      result, directory, dict(self.env, **environment))
        return directory

    def recorded_calls(self):
        calls = [json.loads(line) for line in self.calls.read_text().splitlines()] if self.calls.exists() else []
        # CI adds Cargo's global verbosity flag before the subcommand. Keep
        # raw argv in the fixture journal; ignore only this prefix for identity.
        return [args[1:] if args[:1] == ['--verbose'] else args for args in calls]

    def test_verbose_cargo_keeps_real_argv_and_complete_support_receipts(self):
        result = self.result(('oasis7_proto', 'oasis7_signer_protocol'))
        self.execute(result, CI_VERBOSE='1')
        raw = [json.loads(line) for line in self.calls.read_text().splitlines()]
        self.assertEqual(sum(args[:2] == ['--verbose', 'test'] for args in raw), 2)
        self.assertIn(['clippy', '--verbose', '--locked', '-p', 'oasis7_signer_protocol',
                       '--all-targets', '--', '-D', 'warnings', '-D', 'clippy::correctness',
                       '-D', 'clippy::suspicious'], raw)
        self.assertEqual([args[args.index('-p') + 1] for args in self.recorded_calls()
                          if args[0] == 'test'], ['oasis7_proto', 'oasis7_signer_protocol'])
        self.assertEqual(result['status'], 'passed')
        self.assertEqual(result['groups'][0]['tests_executed'], 2)
        self.assertEqual(len(result['groups'][0]['commands']), 3)

    def test_native_profiles_keep_the_complete_canonical_command_chain(self):
        profiles = ('oasis7_agent_harness', 'oasis7_browser_game_client', 'oasis7_client_api',
                    'oasis7_model_provider', 'oasis7_client_storage', 'oasis7_signer_protocol',
                    'oasis7_custody_core', 'oasis7_launcher_ui', 'oasis7_proto', 'oasis7_wasm_build',
                    'oasis7_wasm_sdk', 'oasis7_wasm_store', 'pixel_world_bridge',
                    'oasis7_secure_envelope', 'oasis7_context', 'oasis7_client_launcher')
        before = self.state()
        result = self.result(profiles)
        self.execute(result)
        group = result['groups'][0]
        self.assertEqual(result['status'], 'passed')
        self.assertEqual(group['profiles'], list(profiles))
        self.assertEqual(group['tests_executed'], 16)
        self.assertEqual(len(group['commands']), 21)
        self.assertIn('--support-plan-file', group['argv'])
        self.assertNotIn('--plan', group['argv'])
        calls = self.recorded_calls()
        tests = [args for args in calls if args[0] == 'test']
        self.assertEqual([args[args.index('-p') + 1] for args in tests], sorted(profiles))
        clippy = [args for args in calls if args[0] == 'clippy']
        self.assertEqual([args[args.index('-p') + 1] for args in clippy],
                         ['oasis7_agent_harness', 'oasis7_browser_game_client',
                          'oasis7_client_storage', 'oasis7_model_provider', 'oasis7_signer_protocol'])
        for args in tests + clippy:
            self.assertIn('--locked', args)
        self.assertIn(['test', '--locked', '-p', 'oasis7_client_launcher',
                       '--bin', 'oasis7_client_launcher'], tests)
        for profile in ('oasis7_custody_core', 'oasis7_model_provider',
                        'oasis7_secure_envelope', 'oasis7_context'):
            selected = next(args for args in tests if args[args.index('-p') + 1] == profile)
            self.assertIn('--all-targets', selected)
        self.assertEqual(self.state(), before)

    def test_unsupported_profile_blocks_the_whole_plan_before_any_test(self):
        cases = ('oasis7_agent_api', 'oasis7_dev_test_worker', 'oasis7_custody_worker',
                 'oasis7_game_client', 'wasm_build_suite', 'wasm_module_observe',
                 'oasis7_local_signer', 'oasis7_wasm_abi', 'oasis7_wasm_router',
                 'oasis7_wasm_executor', 'future_support_profile')
        for profile in cases:
            with self.subTest(profile=profile):
                result = self.result(('oasis7_signer_protocol', profile))
                with self.assertRaisesRegex(entry.Blocked, profile):
                    self.execute(result)
                self.assertEqual(result['checks_executed'], 0)
                self.assertEqual(result['groups'][0]['status'], 'blocked')
                self.assertEqual(result['groups'][0]['profiles'], ['oasis7_signer_protocol', profile])
        self.assertTrue(all(args[0] == 'metadata' for args in self.recorded_calls()))

    def test_partial_success_cannot_replace_the_selected_support_inventory(self):
        actual_invoke = entry.invoke
        for mode in ('missing_test', 'missing_clippy', 'duplicate_test', 'unselected_test'):
            with self.subTest(mode=mode):
                result = self.result(('oasis7_proto', 'oasis7_signer_protocol'))

                def tamper(argv, root, env, directory, name, timeout):
                    outcome = actual_invoke(argv, root, env, directory, name, timeout)
                    self.assertTrue(outcome['passed'])
                    records = [(path, json.loads(path.read_bytes()))
                               for path in (directory / 'commands').glob('*.json')]
                    self.assertEqual(len(records), 3)
                    proto = next((path, record) for path, record in records
                                 if 'oasis7_proto' in record['argv'])
                    clippy = next((path, record) for path, record in records
                                  if 'clippy' in record['argv'])
                    if mode == 'missing_test':
                        proto[0].unlink()
                    elif mode == 'missing_clippy':
                        clippy[0].unlink()
                    elif mode == 'duplicate_test':
                        signer = next(record for _, record in records
                                      if 'test' in record['argv'] and 'oasis7_signer_protocol' in record['argv'])
                        proto[0].write_text(json.dumps(signer))
                    else:
                        command = proto[1]['argv']
                        command[command.index('-p') + 1] = 'oasis7_client_api'
                        proto[0].write_text(json.dumps(proto[1]))
                    return outcome

                with mock.patch.object(entry, 'invoke', side_effect=tamper):
                    with self.assertRaisesRegex(entry.Failed, 'support command inventory'):
                        self.execute(result, CI_VERBOSE='1')
                group = result['groups'][0]
                self.assertEqual(group['status'], 'incomplete')
                self.assertGreater(group['tests_executed'], 0)
                self.assertTrue(all(command['status'] == 'passed' for command in group['commands']))
                self.assertFalse(result.get('validated', False))

    def test_invalid_and_expanded_support_plans_never_start_the_driver(self):
        cases = (
            {'workspace_support_profiles': []},
            {'workspace_support_profiles': ['oasis7_signer_protocol'] * 2},
            {'workspace_support_profiles': [{'profile': 'oasis7_signer_protocol'}]},
            {'workspace_support_profiles': 'oasis7_signer_protocol'},
            {'workspace_support_profile_version': None},
            {'workspace_support_profile_version': True},
            {'workspace_support_profile_version': 2},
            {'scope': 'full'}, {'scope': 'explicit-full'}, {'scope': None},
            {'test_policy_changed': True}, {'test_policy_changed': None},
        )
        for fields in cases:
            with self.subTest(fields=fields):
                result = self.result(**fields)
                with self.assertRaisesRegex(entry.Blocked, 'workspace_support:'):
                    self.execute(result)
                self.assertEqual(result['checks_executed'], 0)
                self.assertEqual(result['driver']['status'], 'not_started')
        self.assertTrue(all(args[0] == 'metadata' for args in self.recorded_calls()))

    def test_support_failure_unknown_zero_and_skipped_cases_stop_later_profiles(self):
        for mode in ('failed', 'unknown', 'zero', 'skipped'):
            with self.subTest(mode=mode):
                self.calls.unlink(missing_ok=True)
                result = self.result(('oasis7_proto', 'oasis7_signer_protocol'))
                with self.assertRaises(entry.Failed):
                    self.execute(result, LOCAL_SUPPORT_MODE=mode)
                self.assertEqual(result['groups'][0]['status'], 'failed' if mode == 'failed' else 'incomplete')
                tests = [args for args in self.recorded_calls() if args[0] == 'test']
                self.assertEqual([args[args.index('-p') + 1] for args in tests], ['oasis7_proto'])

    def test_missing_rust_prerequisites_block_before_support_execution(self):
        result = self.result()
        actual_which = entry.shutil.which
        def which(name, *args, **kwargs):
            return None if name in ('cargo', 'rustc', 'rustup') else actual_which(name, *args, **kwargs)
        with mock.patch.object(entry.shutil, 'which', side_effect=which):
            with self.assertRaisesRegex(entry.Blocked, 'missing tool: cargo'):
                self.execute(result)
        self.assertEqual(result['checks_executed'], 0)
        self.assertEqual(self.recorded_calls(), [])

    def test_existing_environment_sanitizer_disables_native_fixture_exports(self):
        names = ('OASIS7_CUSTODY_SCHEMA_OUTPUT', 'OASIS7_CUSTODY_ADMISSION_FIXTURE_OUTPUT',
                 'OASIS7_CONTEXT_BODY_FIXTURE_OUTPUT', 'PIXEL_WORLD_BEVY_RENDER_PROBE_OUT_DIR',
                 'PIXEL_WORLD_BEVY_PIXEL_PROBE_OUT_DIR')
        with mock.patch.dict(os.environ, {name: '/unowned/output' for name in names}):
            sanitized = entry.execution_environment()
        self.assertFalse(set(names) & sanitized.keys())
        self.assertEqual(sanitized['RUSTUP_AUTO_INSTALL'], '0')


class ViewerEntry(GitFixture):
    """Real entry/driver/receipts; build and browser executables are test doubles."""

    def setUp(self):
        super().setUp()
        self.bins = Path(self.temp.name) / 'bin'
        self.bins.mkdir()
        self.tool_log = Path(self.temp.name) / 'tools.jsonl'
        self.write('scripts/ci-tests.sh', (HERE / 'ci-tests.sh').read_text(), True)
        for name in ('viewer-dependency-preflight.sh', 'ensure-wasm-bindgen-cli.sh', 'wasm-bindgen-cli-common.sh',
                     'ci_process_runner.py'):
            self.write('scripts/' + name, (HERE / name).read_text(), True)
        self.write('scripts/find-python-with-module.sh', f'#!/bin/sh\nprintf "%s\\n" "{sys.executable}"\n', True)
        self.write('Cargo.lock', '[[package]]\nname="wasm-bindgen"\nversion="0.2.999"\n'.replace('=', ' = '))
        self.write('crates/oasis7_viewer/package.json', '{}\n')
        self.write('crates/oasis7_viewer/package-lock.json', '{}\n')
        self.write('crates/oasis7_viewer/node_modules/.package-lock.json', '{}\n')
        for name in ('vite', 'vitest'):
            self.write('crates/oasis7_viewer/node_modules/.bin/' + name, '#!/bin/sh\nexit 0\n', True)
        fixture_tool = f'''#!{sys.executable}
import json,os,pathlib,sys
root=pathlib.Path(os.environ['LOCAL_VIEWER_FIXTURE_ROOT']); role=pathlib.Path(sys.argv[0]).name; args=sys.argv[1:]
with open(os.environ['LOCAL_VIEWER_FIXTURE_LOG'],'a') as f:f.write(json.dumps([role,args])+"\\n")
if args==['--version']:
 print({{'agent-browser':'agent-browser 0.37.1','wasm-bindgen':'wasm-bindgen 0.2.999'}}.get(role,role+' fixture 1'));sys.exit(0)
if role=='rustup' and args==['target','list','--installed']:
 print('' if os.environ.get('LOCAL_VIEWER_FIXTURE_NO_WASM') else 'wasm32-unknown-unknown');sys.exit(0)
if role=='cargo' and args==['metadata','--no-deps','--format-version','1','--locked','--offline']:
 print(json.dumps({{'target_directory':str(root/'target')}}));sys.exit(0)
def tap(n):print('TAP version 13\\n1..'+str(n)+'\\n'+'\\n'.join('ok '+str(i)+' fixture assertion' for i in range(1,n+1))+'\\n# tests '+str(n)+'\\n# pass '+str(n))
if role=='npm' and 'test:frontend-structure' in args:
 assert 'OASIS7_LOCAL_CHECK' not in os.environ;tap(2);sys.exit(0)
if role=='npm' and 'test:ui' in args:
 assert args[-2:]==['--','--reporter=tap-flat'];assert os.environ['OASIS7_LOCAL_CHECK']=='1'
 (root/'target/ui.complete').write_text('complete');tap(2);sys.exit(0)
if role=='npm' and 'build:viewer:bundle' in args:
 assert (root/'target/ui.complete').exists() and (root/'target/build.complete').exists()
 (root/'target/visual.complete').write_text('complete');sys.exit(0)
if role=='npm' and 'build:viewer:visual-test' in args:
 assert os.environ['WASM_BINDGEN_BIN'];sys.exit(0)
if role=='node' and '--test' in args:
 assert '--test-reporter=tap' in args;assert 'OASIS7_LOCAL_CHECK' not in os.environ;tap(1);sys.exit(0)
if role=='node' and args and args[0].endswith('browser-functional-regression.mjs'):
 assert (root/'target/visual.complete').exists();assert args[-1]=='--skip-build'
 print(json.dumps({{'expected_count':26,'count_ok':True,'passed':True}}));sys.exit(0)
if role=='node' and args and args[0].endswith('viewer-auth-browser-security-smoke.mjs'):
 assert (root/'target/visual.complete').exists()
 print(json.dumps({{'expected_count':1,'count_ok':True,'passed':True}}));sys.exit(0)
raise SystemExit('unexpected build/install/command '+repr((role,args)))
'''
        for name in ('node', 'npm', 'cargo', 'rustc', 'rustup', 'agent-browser', 'chromium', 'wasm-bindgen'):
            path = self.bins / name
            path.write_text(fixture_tool)
            path.chmod(0o755)
        for name in ('viewer-dependency-preflight.test.sh', 'ensure-wasm-bindgen-cli.test.sh',
                     'copy-viewer-web-dist.test.sh', 'agent-browser-viewer-dist-freshness-test.sh',
                     'agent-browser-lifecycle-contract.test.sh', 'viewer-prompt-control-regression.test.sh',
                     'viewer-prompt-control-race.test.sh', 'viewer-prompt-control-race-identity.test.sh',
                     'bundle-freshness-lib.test.sh', 'viewer-performance-report-only-contract.test.sh'):
            self.write('scripts/' + name, '#!/bin/sh\n[ -z "${OASIS7_LOCAL_CHECK:-}" ] || exit 71\necho fixture-contract-completed\n',
                       os.access(HERE / name, os.X_OK))
        self.write('scripts/tests/test_industrial_starter_evidence.py',
                   'import unittest\nclass Fixture(unittest.TestCase):\n def test_assertion(self): self.assertEqual(2+2,4)\nunittest.main()\n')
        self.write('scripts/build-viewer-software-safe.sh', '#!/bin/sh\n'
                   '[ "${OASIS7_LOCAL_CHECK:-}" = 1 ] || exit 72\n'
                   '[ -d "$OASIS7_VIEWER_LOCAL_OUTPUT_ROOT" ] || exit 73\n'
                   '[ -d "$AGENT_BROWSER_SOCKET_DIR" ] || exit 74\n'
                   'printf "%s\\n" "$AGENT_BROWSER_SOCKET_DIR" > target/socket-path\n'
                   'test "${LOCAL_VIEWER_FIXTURE_BUILD_FAIL:-0}" = 0 || exit 19\n'
                   'touch target/build.complete\n', True)
        self.write('scripts/viewer-performance-report-only.sh', '#!/bin/sh\n'
                   '[ "${OASIS7_LOCAL_CHECK:-}" = 1 ] || exit 75\necho fixture-collection-completed\n', True)
        (self.root / 'target').mkdir()
        self.env = dict(entry.execution_environment(), PATH=str(self.bins) + os.pathsep + os.environ['PATH'],
                        CARGO_HOME=str(Path(self.temp.name) / 'cargo'), XDG_CACHE_HOME=str(Path(self.temp.name) / 'cache'),
                        LOCAL_VIEWER_FIXTURE_ROOT=str(self.root), LOCAL_VIEWER_FIXTURE_LOG=str(self.tool_log))
        self.args = SimpleNamespace(tier='required', offline=True, timeout_seconds=10)

    def result(self, groups=('viewer_js_required',)):
        return dict(groups=[dict(name=name, status='not_run') for name in groups], configuration={},
                    checks_executed=0, base_sha=self.base, source_sha=self.base)

    def test_preflight_checks_prepared_dependencies_without_building_or_installing(self):
        result = self.result()
        entry.preflight(self.root, self.args, result, self.env)
        self.assertEqual(result['viewer_tools']['agent-browser'], str(self.bins / 'agent-browser'))
        self.assertEqual(result['viewer_tools']['wasm_bindgen'], str(self.bins / 'wasm-bindgen'))
        self.assertIn('--locked', result['cargo_preflight']['argv'])
        self.assertIn('--offline', result['cargo_preflight']['argv'])
        calls = [json.loads(line) for line in self.tool_log.read_text().splitlines()]
        self.assertFalse(any('install' in args or 'build' in args for _, args in calls))

    def test_missing_viewer_tools_target_or_npm_preparation_blocks_the_whole_plan(self):
        original = entry.shutil.which
        for missing in ('node', 'npm', 'cargo', 'rustc', 'rustup', 'agent-browser', 'chromium'):
            def which(name, *args, **kwargs):
                if name == missing or (missing == 'chromium' and name in ('chromium-browser', 'google-chrome', 'google-chrome-stable')):
                    return None
                return original(name, *args, **kwargs)
            with self.subTest(missing=missing), mock.patch.object(entry.shutil, 'which', side_effect=which), mock.patch.object(entry, 'invoke') as invoke:
                with self.assertRaises(entry.Blocked):
                    entry.execute(self.root, self.args, self.result(('baseline', 'viewer_js_required')),
                                  Path(self.temp.name), self.env)
                invoke.assert_not_called()
        for variation in ('target', 'npm', 'bindgen'):
            env = dict(self.env)
            if variation == 'target': env['LOCAL_VIEWER_FIXTURE_NO_WASM'] = '1'
            path = (self.root / 'crates/oasis7_viewer/node_modules/.bin/vitest' if variation == 'npm'
                    else self.bins / 'wasm-bindgen')
            if variation != 'target': path.rename(path.with_suffix('.saved'))
            try:
                with self.subTest(variation=variation), mock.patch.object(entry, 'invoke') as invoke:
                    with self.assertRaises(entry.Blocked):
                        entry.execute(self.root, self.args, self.result(), Path(self.temp.name), env)
                    invoke.assert_not_called()
            finally:
                if variation != 'target': path.with_suffix('.saved').rename(path)
        self.assertFalse((self.root / 'target/build.complete').exists())

    def test_real_driver_executes_complete_viewer_groups_and_isolates_owned_outputs(self):
        result = self.result(('viewer_js_required', 'viewer_performance_report'))
        directory = Path(self.temp.name) / 'run'
        directory.mkdir()
        with (directory / 'fixture-output.log').open('w') as log, contextlib.redirect_stderr(log):
            entry.execute(self.root, self.args, result, directory, self.env)
        self.assertEqual(result['status'], 'passed')
        self.assertEqual(result['checks_executed'], 23)
        self.assertEqual([len(group['commands']) for group in result['groups']], [19, 4])
        self.assertEqual(result['groups'][0]['business_tests_executed'], 29)
        self.assertEqual(result['groups'][1]['business_tests_executed'], 0)
        self.assertFalse(Path((self.root / 'target/socket-path').read_text().strip()).exists())
        for group in result['groups']:
            self.assertTrue(Path(group['viewer_output']).is_dir())
        for group in result['groups']:
            for index in range(len(group['commands'])):
                with self.subTest(group=group['name'], missing=index):
                    self.assertFalse(entry.viewer_complete(group['commands'][:index] + group['commands'][index+1:],
                                                           group['name'], result['viewer_tools']['wasm_bindgen']))

    def test_build_failure_does_not_start_browser_checks_and_cleans_socket_directory(self):
        result = self.result()
        directory = Path(self.temp.name) / 'run'
        directory.mkdir()
        with (directory / 'fixture-output.log').open('w') as log, contextlib.redirect_stderr(log):
            with self.assertRaises(entry.Failed):
                entry.execute(self.root, self.args, result, directory,
                              dict(self.env, LOCAL_VIEWER_FIXTURE_BUILD_FAIL='1'))
        calls = [json.loads(line) for line in self.tool_log.read_text().splitlines()]
        self.assertFalse(any('browser-functional-regression.mjs' in ' '.join(args) for _, args in calls))
        self.assertFalse(Path((self.root / 'target/socket-path').read_text().strip()).exists())

    def test_inherited_browser_sessions_and_output_overrides_are_removed(self):
        inherited = dict(AGENT_BROWSER_SESSION='foreign', AGENT_BROWSER_CDP='9222',
                         AGENT_BROWSER_PROVIDER='remote', AGENT_BROWSER_EXECUTABLE_PATH='/foreign/chrome',
                         AGENT_BROWSER_SOCKET_DIR='/foreign/sockets', AGENT_VISUAL_ARTIFACT_ROOT='/foreign/dist',
                         WASM_BINDGEN_BIN='/foreign/bindgen', OASIS7_VIEWER_LOCAL_OUTPUT_ROOT='/foreign/output')
        with mock.patch.dict(os.environ, inherited):
            env = entry.execution_environment()
        self.assertFalse(set(inherited) & set(env))


if __name__ == '__main__':
    unittest.main()
