#!/usr/bin/env python3
"""Real small Node/process tests for the Action preload, not hosted CodeQL extraction."""
import hashlib
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import tempfile
import time
import unittest

import codeql_rust_action_deadline as m

HERE = Path(__file__).resolve().parent
MAIN = ('// Automatically generated from \'action-entry.js.tpl\' for \'src/analyze-action.ts\'.\n\n'
        '"use strict";\n\nconst import_entry_points = require("./entry-points");\n'
        'void (0, import_entry_points.runAnalyzeAction)();\n')
POST = MAIN.replace('src/analyze-action.ts', 'src/analyze-action-post.ts').replace('runAnalyzeAction', 'runAnalyzePostAction')
PAYLOAD = r'''
const fs = require('node:fs');
const cp = require('node:child_process');
function mark(s) {fs.appendFileSync(process.env.FIXTURE_MARKER, s + '\n');}
exports.runAnalyzeAction = function () {
  mark('main');
  console.log('::notice::fixture stdout'); console.error('fixture stderr');
  fs.appendFileSync(process.env.GITHUB_STATE, 'fixture=state-value\n');
  fs.appendFileSync(process.env.GITHUB_OUTPUT, 'fixture=output-value\n');
  fs.writeFileSync(process.env.FIXTURE_OBSERVED, JSON.stringify({node:process.execPath,
    exec_argv:process.execArgv, argv:process.argv.slice(1), cwd:process.cwd(),
    state:process.env.GITHUB_STATE, output:process.env.GITHUB_OUTPUT,
    token:process.env.INPUT_TOKEN, preload:process.env.NODE_OPTIONS ?? null,
    supervisor:process.env.OASIS7_CODEQL_ACTION_SUPERVISOR ?? null}));
  if (process.env.FIXTURE_BEHAVIOR === 'overflow') {
    const p = process.env.RUNNER_TEMP + '/rust-capacity-evidence/full';
    fs.closeSync(fs.openSync(p, 'w')); fs.truncateSync(p, 255 * 1024 * 1024 + 512 * 1024);
    setInterval(() => {}, 1000);
  }
  if (process.env.FIXTURE_BEHAVIOR === 'tree') {
    process.on('SIGTERM', () => mark('main-TERM'));
    const grandchild = cp.spawn(process.execPath, ['-e',
      "process.on('SIGTERM',()=>{});setInterval(()=>{},1000)"], {stdio:'inherit'});
    fs.writeFileSync(process.env.FIXTURE_PIDS, JSON.stringify([process.pid, grandchild.pid]));
    setInterval(() => {}, 1000);
  }
  if (process.env.FIXTURE_BEHAVIOR === 'nonzero') process.exitCode = 7;
};
exports.runAnalyzePostAction = function () {
  mark('post:' + (process.env.STATE_fixture || 'absent'));
  console.log('post stdout');
};
'''


class ActionDeadline(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(); self.addCleanup(self.temp.cleanup)
        self.invocations = []
        self.root = Path(self.temp.name).resolve()
        self.runner, self.source = self.root / 'runner', self.root / 'source'
        self.source.mkdir(); self.runner.mkdir()
        self.out = self.runner / 'rust-capacity-evidence'; self.out.mkdir()
        self.tools = self.runner / 'rust-capacity-helper'; self.tools.mkdir()
        for name in ('codeql_rust_diagnostic_runtime.py', 'codeql_rust_action_deadline.py',
                     'codeql_rust_capacity_layout.py', 'codeql-rust-action-deadline.cjs'):
            shutil.copyfile(HERE / name, self.tools / name)
        self.lib = self.root / '_actions/github/codeql-action' / m.ACTION_SHA / 'lib'; self.lib.mkdir(parents=True)
        (self.lib / 'analyze-entry.js').write_text(MAIN)
        (self.lib / 'analyze-post-entry.js').write_text(POST)
        (self.lib / 'entry-points.js').write_text(PAYLOAD)
        self.node = shutil.which('node'); self.assertIsNotNone(self.node, 'the Action wrapper tests require Node')
        self.env = dict(os.environ, RUNNER_TEMP=str(self.runner), GITHUB_WORKSPACE=str(self.source),
                        CAPACITY_MODE=m.MODE, CAPACITY_SOURCE=m.SOURCE,
                        NODE_OPTIONS='--require=' + str(self.tools / 'codeql-rust-action-deadline.cjs'),
                        GITHUB_STATE=str(self.root / 'state'), GITHUB_OUTPUT=str(self.root / 'output'),
                        FIXTURE_MARKER=str(self.root / 'marker'), FIXTURE_OBSERVED=str(self.root / 'observed'),
                        FIXTURE_PIDS=str(self.root / 'pids'), INPUT_TOKEN='fixture-token-not-a-secret')
        self.env.update({'INPUT_SKIP-QUERIES': 'true', 'INPUT_UPLOAD': 'never', 'INPUT_UPLOAD-DATABASE': 'false'})
        for name in ('state', 'output'): (self.root / name).write_text('')
        self.budget(30)
        self.addCleanup(self.preserve_observations)

    def preserve_observations(self):
        destination = os.environ.get('OASIS7_CODEQL_DEADLINE_TEST_EVIDENCE')
        if not destination: return
        target = Path(destination) / self._testMethodName
        target.mkdir(parents=True, exist_ok=False)
        manifest = {}
        for path in sorted(self.out.iterdir()):
            if not path.is_file(): continue
            size = path.stat().st_size
            if size > 1024**2:
                manifest[path.name] = dict(bytes=size, retained=False, reason='sparse evidence-overflow fixture, not a retained log')
                continue
            shutil.copyfile(path, target / path.name)
            manifest[path.name] = dict(bytes=size, retained=True, sha256=hashlib.sha256(path.read_bytes()).hexdigest())
        for name in ('marker', 'observed', 'state', 'output', 'pids'):
            if (self.root / name).exists(): shutil.copyfile(self.root / name, target / ('fixture-' + name))
        (target / 'observations.json').write_text(json.dumps(dict(scope='real small Node fixture; not the real CodeQL payload or hosted runner',
            invocations=self.invocations, retained=manifest), indent=2) + '\n')

    def budget(self, remaining, elapsed=4000):
        now = time.time()
        value = dict(mode=m.MODE, job_budget_seconds=5400, fixed_job_deadline=now + 1200,
                     analysis_allocated_at=now - elapsed, analysis_allocated_minutes=75,
                     analysis_deadline=now + remaining, post_analysis_reserve_seconds=600,
                     helper_terminal_and_upload_reserve_seconds=120)
        (self.out / 'capacity-budget.json').write_text(json.dumps(value))
        self.env.update(CAPACITY_DEADLINE=str(value['fixed_job_deadline']),
                        CAPACITY_ANALYSIS_DEADLINE=str(value['analysis_deadline']))
        return value

    def start(self, *, post=False, options=()):
        return subprocess.Popen([self.node, *options, str(self.lib / ('analyze-post-entry.js' if post else 'analyze-entry.js'))],
                                cwd=self.source, env=self.env, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)

    def finish(self, proc, timeout=12):
        try: stdout, stderr = proc.communicate(timeout=timeout)
        except subprocess.TimeoutExpired:
            proc.kill(); proc.communicate(); self.fail('Action wrapper did not stop within bounded test time')
        self.invocations.append(dict(argv=proc.args, exit=proc.returncode, stdout=stdout, stderr=stderr))
        return proc.returncode, stdout, stderr

    def receipt(self): return json.loads((self.out / 'action-deadline-terminal.json').read_text())

    def test_normal_main_runs_once_and_preserves_runner_channels_inputs_and_node_flags(self):
        result, stdout, stderr = self.finish(self.start(options=['--no-warnings']))
        self.assertEqual(result, 0, stderr)
        self.assertEqual((self.root / 'marker').read_text(), 'main\n')
        self.assertIn('::notice::fixture stdout', stdout); self.assertIn('fixture stderr', stderr)
        self.assertEqual((self.root / 'state').read_text(), 'fixture=state-value\n')
        self.assertEqual((self.root / 'output').read_text(), 'fixture=output-value\n')
        observed = json.loads((self.root / 'observed').read_text())
        self.assertEqual(observed['exec_argv'], ['--no-warnings'])
        self.assertEqual(observed['node'], str(Path(self.node).resolve()))
        self.assertEqual(observed['argv'], [str(self.lib / 'analyze-entry.js')])
        self.assertEqual(observed['cwd'], str(self.source)); self.assertEqual(observed['token'], self.env['INPUT_TOKEN'])
        self.assertEqual(observed['state'], self.env['GITHUB_STATE']); self.assertEqual(observed['output'], self.env['GITHUB_OUTPUT'])
        self.assertIsNone(observed['preload']); self.assertIsNone(observed['supervisor'])
        terminal = self.receipt()
        self.assertEqual(terminal['status'], 'ACTION_MAIN_COMPLETED_WITHIN_DEADLINE')
        self.assertFalse((self.out / 'action-main.log').exists())
        self.assertFalse((self.out / 'action-main.log.stderr').exists())
        self.assertFalse(any('fixture-token-not-a-secret' in p.read_text() for p in self.out.iterdir() if p.is_file()))
        accepted = m.admit(self.out, self.source, float(self.env['CAPACITY_DEADLINE']))
        self.assertEqual(accepted['status'], 'PASS')
        process = json.loads((self.out / 'action-main.log.result.json').read_text())
        self.assertEqual(process['cleanup_errors'], []); self.assertIsNone(process['file_size_limit_bytes'])

    def test_already_expired_start_never_executes_main(self):
        self.budget(-1)
        result, _, _ = self.finish(self.start())
        self.assertNotEqual(result, 0); self.assertFalse((self.root / 'marker').exists())
        self.assertFalse((self.out / 'action-main.log.result.json').exists())
        self.assertIn('expired before Action main spawn', self.receipt()['primary_failure'])

    def test_delayed_real_node_startup_is_stopped_by_the_original_cutoff(self):
        budget = self.budget(4.5)
        delay = self.root / 'delay.cjs'
        delay.write_text('Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 5000);\n')
        result, _, _ = self.finish(self.start(options=['--require', str(delay)]))
        self.assertNotEqual(result, 0); self.assertFalse((self.root / 'marker').exists())
        self.assertEqual(self.receipt()['analysis_deadline'], budget['analysis_deadline'])
        self.assertLess(self.receipt()['finished_at'], budget['analysis_deadline'])
        process = json.loads((self.out / 'action-main.log.result.json').read_text())
        self.assertEqual(process['failure_kind'], 'timeout')

    def test_sixty_five_second_start_gap_cannot_extend_the_original_budget(self):
        value = dict(mode=m.MODE, job_budget_seconds=5400, fixed_job_deadline=6400,
                     analysis_allocated_at=1000, analysis_allocated_minutes=75, analysis_deadline=5500,
                     post_analysis_reserve_seconds=600, helper_terminal_and_upload_reserve_seconds=120)
        # Original review counterexample: 29 relative minutes after a 65s gap ends at 5505.
        relative_end = 3700 + 65 + ((5500 - 3700 - 60) // 60) * 60
        self.assertEqual(relative_end, 5505)
        absolute_end, _ = m.shared_budget(value, 6400, 5500, 3765, 100)
        self.assertEqual(absolute_end, 1835)
        self.assertEqual(absolute_end - 100, 5500 - 3765)

    def assert_tree_gone(self):
        for pid in json.loads((self.root / 'pids').read_text()):
            self.assertFalse((Path('/proc') / str(pid)).exists(), 'owned child was not reaped: ' + str(pid))

    def test_timeout_kills_and_reaps_owned_grandchildren_within_analysis_window(self):
        budget = self.budget(5)
        self.env['FIXTURE_BEHAVIOR'] = 'tree'
        result, _, _ = self.finish(self.start())
        self.assertNotEqual(result, 0); self.assert_tree_gone()
        terminal = self.receipt(); self.assertEqual(terminal['status'], 'ACTION_MAIN_INCOMPLETE')
        self.assertLess(terminal['finished_at'], budget['analysis_deadline'])
        process = json.loads((self.out / 'action-main.log.result.json').read_text())
        self.assertEqual(process['failure_kind'], 'timeout'); self.assertEqual(process['cleanup_errors'], [])

    def test_outer_action_cancellation_stops_its_real_owned_tree(self):
        self.env['FIXTURE_BEHAVIOR'] = 'tree'
        proc = self.start()
        until = time.monotonic() + 5
        while not (self.root / 'pids').exists() and proc.poll() is None and time.monotonic() < until:
            time.sleep(.01)
        self.assertTrue((self.root / 'pids').exists())
        proc.send_signal(signal.SIGTERM)
        result, _, _ = self.finish(proc)
        self.assertNotEqual(result, 0); self.assert_tree_gone()
        self.assertEqual(self.receipt()['status'], 'ACTION_MAIN_INCOMPLETE')
        with self.assertRaises(RuntimeError): m.admit(self.out, self.source, float(self.env['CAPACITY_DEADLINE']))

    def test_sampling_evidence_failure_never_passes_an_otherwise_running_action(self):
        self.env['FIXTURE_BEHAVIOR'] = 'overflow'
        result, _, _ = self.finish(self.start())
        self.assertNotEqual(result, 0)
        terminal = self.receipt(); self.assertEqual(terminal['status'], 'ACTION_MAIN_INCOMPLETE')
        self.assertIn('evidence budget', terminal['primary_failure'])
        self.assertLessEqual(m.runtime.output_bytes(self.out), m.OFFICIAL_CAP)

    def test_nonzero_action_exit_is_preserved_and_cannot_be_admitted(self):
        self.env['FIXTURE_BEHAVIOR'] = 'nonzero'
        result, _, _ = self.finish(self.start())
        self.assertNotEqual(result, 0)
        process = json.loads((self.out / 'action-main.log.result.json').read_text())
        self.assertEqual(process['exit'], 7); self.assertEqual(process['failure_kind'], 'child_exit')

    def test_expired_post_keeps_its_original_entry_and_saved_state(self):
        self.budget(-60); self.env['STATE_fixture'] = 'state-value'
        result, stdout, stderr = self.finish(self.start(post=True))
        self.assertEqual(result, 0, stderr); self.assertIn('post stdout', stdout)
        self.assertEqual((self.root / 'marker').read_text(), 'post:state-value\n')
        self.assertFalse((self.out / 'action-deadline-terminal.json').exists())

    def test_legacy_modes_do_not_use_the_new_supervisor(self):
        self.budget(-60); self.env['CAPACITY_MODE'] = 'capacity'
        result, _, stderr = self.finish(self.start())
        self.assertEqual(result, 0, stderr)
        self.assertEqual((self.root / 'marker').read_text(), 'main\n')
        self.assertFalse((self.out / 'action-deadline-terminal.json').exists())

    def test_changed_bootstrap_is_rejected_before_any_payload_runs(self):
        (self.lib / 'analyze-entry.js').write_text(MAIN + '\n// changed\n')
        result, _, stderr = self.finish(self.start())
        self.assertNotEqual(result, 0); self.assertIn('bootstrap identity', stderr)
        self.assertFalse((self.root / 'marker').exists())


if __name__ == '__main__': unittest.main()
