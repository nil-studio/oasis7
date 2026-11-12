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

    def test_selected_unsupported_group_and_full_block_before_any_driver(self):
        self.install_policy(groups=('baseline', 'viewer_js_required'))
        for args in (('--profile', 'affected'), ('--profile', 'formal', '--tier', 'full')):
            report = self.assert_result(self.invoke(*args), 'blocked', 2)
            self.assertEqual(report['checks_executed'], 0)
            self.assertEqual(report['groups'][1]['status'], 'blocked')

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


if __name__ == '__main__':
    unittest.main()
