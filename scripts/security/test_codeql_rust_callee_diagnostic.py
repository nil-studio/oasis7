#!/usr/bin/env python3
"""Regression oracles for missing-stage visibility and manual control routing."""
from pathlib import Path
import json
import tempfile
import time
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


if __name__ == '__main__':
    unittest.main()
