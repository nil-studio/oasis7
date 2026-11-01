#!/usr/bin/env python3
"""Regression oracles for missing-stage visibility and manual control routing."""
from pathlib import Path
import json
import tempfile
import time
import subprocess
import sys
from types import SimpleNamespace
from unittest.mock import patch
import unittest
import codeql_rust_callee_diagnostic as m

ROOT = Path(__file__).resolve().parents[2]


class CalleeDiagnostic(unittest.TestCase):
    def test_each_first_missing_stage_is_visible(self):
        cases = [
            ([0, 0, 0, 0, 0, 0], 'CALL_AST_NOT_FOUND_AT_LINE'),
            ([2, 0, 0, 0, 0, 0], 'SYNTACTIC_NAME_NOT_MATCHED'),
            ([2, 1, 0, 0, 0, 1], 'STATIC_TARGET_NOT_RESOLVED'),
            ([2, 1, 1, 0, 0, 1], 'STATIC_TARGET_NAME_DIFFERS'),
            ([2, 1, 1, 1, 0, 1], 'TARGET_LOCATION_MISSING'),
            ([2, 1, 1, 1, 1, 1], 'RESOLUTION_PRESENT_CHECK_ORIGINAL_ROWS'),
        ]
        stages = ['call-at-line', 'syntactic-name', 'resolved-any', 'resolved-name',
                  'target-file', 'function-declaration']
        for values, expected in cases:
            result = m.classify(list(zip(stages, values)))
            self.assertEqual(result['first_missing_boundary'], expected)
            self.assertEqual(result['root_cause'], 'UNKNOWN')
            self.assertEqual(result['full_rust'], 'HOLD')

    def test_missing_duplicate_and_bad_count_rejected(self):
        rows = [[s, 0] for s in sorted(m.STAGES)]
        for invalid in (rows[:-1], rows + [rows[0]], [[s, True] for s in m.STAGES],
                        [[s, -1] for s in m.STAGES]):
            with self.assertRaises(RuntimeError):
                m.classify(invalid)

    def test_original_oracle_is_preserved_and_syntax_is_independent(self):
        source = Path('/business')
        texts = m.queries(source)
        original = m.load('test_capacity', 'codeql-rust-capacity.py').semantic_queries(source)['CallsCandidate']
        self.assertEqual(texts['OriginalCalls'], original)
        self.assertNotIn('getStaticTarget', texts['Syntax'])
        self.assertNotIn('getTargetName', texts['Syntax'])
        self.assertIn('count(Call c, Function f', texts['ResolutionCounts'])
        self.assertIn('NO_FILE', texts['TargetFunctions'])
        self.assertIn('NO_LOCATION', texts['TargetFunctions'])

    def test_opt_in_routing_source_and_profile_mutual_exclusion(self):
        text = (ROOT / '.github/workflows/codeql-rust-cfg-diagnostic.yml').read_text()
        cfg, profile = text.split('  evaluator_profile:', 1)
        self.assertIn('      callee:', cfg)
        self.assertIn('callee and profile are mutually exclusive', cfg)
        self.assertIn("inputs.callee == true && '" + m.SOURCE + "'", cfg)
        self.assertEqual(cfg.count('CALLEE_DIAGNOSTIC: ${{ inputs.callee }}'), 3)
        self.assertIn('inputs.callee != true', profile)
        self.assertIn('--source-sha "$DIAGNOSTIC_SOURCE"', cfg)
        self.assertIn('timeout-minutes: 45', cfg)
        self.assertNotIn('codeql-action/analyze@', cfg)
        self.assertIn("'codeql_rust_callee_diagnostic.py'", (ROOT / 'scripts/security/codeql_rust_capacity_layout.py').read_text())
        self.assertIn('test_codeql_rust_callee_diagnostic.py', (ROOT / 'scripts/ci-tests.sh').read_text())

    def test_bad_source_admission_preserves_failure_without_running_codeql(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            args = SimpleNamespace(output=root/'out', work=root/'work', source=root,
                                   codeql=root/'codeql', deadline=time.time()+600,
                                   threads=4, ram=4096, check_only=False)
            capacity = SimpleNamespace(evidence=SimpleNamespace(
                capture_identity=lambda *a, **kw: {'status': 'FAIL'}))
            with patch.object(m, 'load', return_value=capacity), patch.object(m.runtime, 'run') as run:
                self.assertEqual(m.run_diagnostic(args), 1)
                run.assert_not_called()
            terminal = json.loads((root/'out/callee-terminal.json').read_text())
            self.assertEqual(terminal['status'], 'INCOMPLETE')
            self.assertIn('frozen source identity failed', terminal['error'])
            self.assertEqual(terminal['capacity'], 'UNKNOWN')

    def exercise_two_phases(self, change_source=False):
        # Real Git and real exclusive runtime receipts; only CodeQL is a stand-in.
        # This crosses the same check-only -> evaluation boundary as the workflow.
        real_capacity = m.load('two_phase_capacity', 'codeql-rust-capacity.py')
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            source = root/'source'; source.mkdir()
            def git(*args):
                return subprocess.check_output(['git', *args], cwd=source, text=True).strip()
            git('init', '--quiet')
            git('config', 'user.name', 'Callee phase regression')
            git('config', 'user.email', 'fixture@example.invalid')
            tracked = source/'fixture.rs'; tracked.write_text('fn main() {}\n')
            git('add', 'fixture.rs'); git('commit', '--quiet', '-m', 'Freeze fixture')
            sha = git('rev-parse', 'HEAD')
            cli = root/'fake-codeql'
            cli.write_text('#!' + sys.executable + '\n' + r'''
import json,sys
from pathlib import Path
a=sys.argv[1:]
with (Path(__file__).parent/'invocations.jsonl').open('a') as f:f.write(json.dumps(a)+'\n')
def option(name):return next(x.split('=',1)[1] for x in a if x.startswith(name+'='))
rows = {
 'Syntax': [[42,42,'libp2p::SwarmBuilder::with_existing_identity']],
 'ResolutionCounts': [['call-at-line',1],['syntactic-name',1],['resolved-any',0],
                      ['resolved-name',0],['target-file',0],['function-declaration',1]],
 'TargetFunctions': [['with_existing_identity','NO_FILE','NO_LOCATION']],
 'OriginalCalls': []}
if a[:2]==['resolve','library-path']:print('{}')
elif a[:2]==['query','compile']:pass
elif a[:2]==['query','run']:Path(option('--output')).write_text('private fake BQRS')
elif a[:2]==['bqrs','info']:
 print(json.dumps({'result-sets':[{'name':'#select','rows':len(rows[Path(a[2]).stem])}]}))
elif a[:2]==['bqrs','decode']:
 Path(option('--output')).write_text(json.dumps({'#select':{'tuples':rows[Path(a[2]).stem]}}))
else:raise SystemExit('unexpected fake CLI command')
''')
            cli.chmod(0o755)
            model = SimpleNamespace(bundle_closure=lambda *a: {'rust': {'root': str(root)}},
                                    verify_library_path=lambda *a: None)
            capacity = SimpleNamespace(evidence=real_capacity.evidence, bundle=model,
                                       semantic_queries=real_capacity.semantic_queries)
            args = SimpleNamespace(output=root/'out', work=root/'work', source=source,
                                   database=root/'database', codeql=cli,
                                   deadline=time.time()+600, threads=4, ram=4096, check_only=True)
            with patch.object(m, 'load', return_value=capacity), patch.object(m, 'SOURCE', sha):
                self.assertEqual(m.run_diagnostic(args), 0)
                first = {p.name: p.read_bytes() for p in args.output.iterdir() if p.is_file()}
                invocations = (root/'invocations.jsonl').read_bytes()
                if change_source:
                    tracked.write_text('fn changed() {}\n')
                args.check_only = False
                second_exit = m.run_diagnostic(args)
                terminal = json.loads((args.output/'callee-terminal.json').read_text())
                if change_source:
                    self.assertEqual(second_exit, 1)
                    self.assertIn('frozen source identity failed', terminal['error'])
                    self.assertTrue(all(v == 'NOT_REACHED' for v in terminal['stages'].values()))
                    self.assertEqual((root/'invocations.jsonl').read_bytes(), invocations)
                    identity = json.loads((args.output/'callee-evaluate-identity.json').read_text())
                    self.assertEqual(identity['identity_gate'], 'PASS')
                    self.assertEqual(identity['clean_gate'], 'FAIL')
                else:
                    self.assertEqual(second_exit, 0, terminal)
                    self.assertEqual(terminal['status'], 'DIAGNOSTIC_COMPLETED_NOT_ACCEPTANCE')
                    self.assertEqual(terminal['original_result_rows'], 0)
                    self.assertEqual(terminal['observation']['first_missing_boundary'], 'STATIC_TARGET_NOT_RESOLVED')
                    self.assertTrue(all(v == 'PASS' for v in terminal['stages'].values()))
                    self.assertEqual(terminal['capacity'], 'UNKNOWN')
                for name, content in first.items():
                    self.assertEqual((args.output/name).read_bytes(), content, 'prior check evidence changed: '+name)

    def test_check_then_evaluate_same_directory_preserves_check_receipts(self):
        self.exercise_two_phases()

    def test_real_source_change_between_phases_still_blocks_queries(self):
        self.exercise_two_phases(change_source=True)


if __name__ == '__main__':
    unittest.main()
