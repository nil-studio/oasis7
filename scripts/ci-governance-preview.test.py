#!/usr/bin/env python3
"""Exercise diagnostic completeness, immutable plans, and legacy applicability."""
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parent.parent
SPEC = importlib.util.spec_from_file_location('governance_preview', ROOT / 'scripts/ci-governance-preview.py')
PREVIEW = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(PREVIEW)


class GovernancePreview(unittest.TestCase):
    def test_registered_scenario_keeps_reverse_contracts_and_one_identity(self):
        sha = 'a' * 40
        plan = PREVIEW.benchmark_plan(ROOT, sha)
        self.assertEqual({plan[field] for field in ('source_sha', 'tested_sha', 'policy_sha')}, {sha})
        self.assertEqual(plan['python_control_paths'], ['scripts/ci-workflow.test.py'])
        selected = plan['python_control_tests']['workflow_governance']
        for path in ('scripts/ci-workflow.test.py', 'scripts/ci-formal-process.test.py',
                     'scripts/ci-full-result.test.py', 'scripts/ci-rust-cache-build.test.py',
                     'scripts/ci-governance-preview.test.py'):
            self.assertIn(path, selected)
        self.assertEqual(selected, sorted(set(selected)))

    def test_complete_command_sequence_keeps_original_extra_contracts(self):
        root, output, sha = Path('/candidate'), Path('/diagnostic'), 'a' * 40
        commands = PREVIEW.comparison_commands(root, output, sha)
        self.assertEqual([name for name, _ in commands],
                         ['legacy-controls', 'legacy-complete-driver',
                          'legacy-extra-contracts', 'candidate-complete-owner'])
        self.assertEqual(commands[0][1][1], '/diagnostic/ci-python-control-contracts.py')
        self.assertEqual(commands[3][1][1], '/candidate/scripts/ci-python-control-contracts.py')
        self.assertEqual(commands[0][1][2:], commands[3][1][2:-1])
        self.assertEqual(commands[3][1][-1], '--governance')
        shell = commands[1][1][2]
        self.assertTrue(shell.startswith('set -euo pipefail\n'))
        self.assertTrue(shell.endswith('\nrun_workflow_governance_contract_tests\n'))
        self.assertNotIn('if run_workflow', shell)
        self.assertNotIn('run_workflow_governance_contract_tests ||', shell)
        self.assertEqual(commands[2][1][-1].splitlines(),
                         ['python3 scripts/package-source-plan.test.py',
                          'python3 scripts/safe-git-archive.test.py',
                          'node scripts/cache-permission-probe.test.cjs'])

    def test_failure_stops_later_stages_and_never_produces_a_speedup(self):
        names = ['legacy-controls', 'legacy-complete-driver',
                 'legacy-extra-contracts', 'candidate-complete-owner']
        for failure in (None, 1, 2, 3):
            with self.subTest(failure=failure), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                output = root / 'output'
                output.mkdir()
                marker = root / 'started'
                commands = [(name, [sys.executable, '-c',
                    'import pathlib,sys; p=pathlib.Path(sys.argv[1]); '
                    'p.write_text((p.read_text() if p.exists() else "")+sys.argv[2]+"\\n"); '
                    'print(sys.argv[2]); sys.exit(int(sys.argv[3]))',
                    str(marker), name, '23' if index == failure else '0'])
                    for index, name in enumerate(names)]
                report = {'authority': 'diagnostic-only'}
                if failure is None:
                    PREVIEW.run_comparison(commands, root, output, dict(os.environ), report)
                else:
                    with self.assertRaises(RuntimeError):
                        PREVIEW.run_comparison(commands, root, output, dict(os.environ), report)
                result = json.loads((output / 'result.json').read_text())
                expected = names if failure is None else names[:failure + 1]
                self.assertEqual(marker.read_text().splitlines(), expected)
                self.assertEqual([stage['name'] for stage in result['stages']], expected)
                self.assertEqual(result['complete'], failure is None)
                self.assertEqual('saved_seconds' in result, failure is None)

    def test_a_baseline_with_the_new_owner_is_not_a_legacy_comparison(self):
        with tempfile.TemporaryDirectory() as directory:
            root, output = Path(directory) / 'repo', Path(directory) / 'output'
            (root / 'scripts').mkdir(parents=True)
            (root / 'scripts/ci-python-control-contracts.py').write_text('print("--governance")\n')
            (root / 'scripts/ci-tests.sh').write_text('# historical source interface\n')
            for args in (['init', '-q'], ['add', '.'],
                         ['-c', 'user.name=fixture', '-c', 'user.email=fixture@example.invalid',
                          'commit', '-qm', 'fixture']):
                subprocess.run(['git', '-C', str(root), *args], check=True, capture_output=True)
            sha = PREVIEW.git(root, 'rev-parse', 'HEAD')
            report = PREVIEW.compare(root, sha, sha, sha, output)
            self.assertEqual(report['not_applicable'], 'base_already_single_owner')
            self.assertFalse(report['complete'])
            self.assertNotIn('saved_seconds', report)
            self.assertFalse((output / 'plan.json').exists())


if __name__ == '__main__':
    unittest.main()
