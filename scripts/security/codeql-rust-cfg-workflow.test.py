#!/usr/bin/env python3
"""Control regressions for the manual, non-security CFG diagnostic."""
import ast
from pathlib import Path
import re
import unittest

ROOT = Path(__file__).resolve().parents[2]
TEXT = (ROOT / '.github/workflows/codeql-rust-cfg-diagnostic.yml').read_text()


class CfgWorkflow(unittest.TestCase):
    def test_explicit_manual_bounded_non_security_entry(self):
        trigger = TEXT.split('permissions:', 1)[0]
        self.assertIn('workflow_dispatch:', trigger)
        self.assertIn('default: false', trigger)
        for forbidden in ('pull_request:', 'push:', 'schedule:'):
            self.assertNotIn(forbidden, trigger)
        self.assertIn("if: github.event_name == 'workflow_dispatch' && inputs.execute == true", TEXT)
        self.assertIn('timeout-minutes: 45', TEXT)
        self.assertIn('budget=2700', TEXT)
        self.assertNotIn('21600', TEXT)
        self.assertNotIn('security-events:', TEXT)
        self.assertNotIn('codeql-action/analyze@', TEXT)
        self.assertNotIn('upload-sarif', TEXT)
        self.assertIn('oasis7-codeql-scan-slot-1', TEXT)
        self.assertIn('cancel-in-progress: false', TEXT)

    def test_fixed_source_official_bundle_and_external_tools(self):
        self.assertIn('d2bf383d8c377b36de83aae4a06d9ec17612e205', TEXT)
        self.assertIn('codeql-bundle-v2.27.3/codeql-bundle-rust-linux64.tar.zst', TEXT)
        self.assertIn('218a19c49a4e44dee236d2cc55e20cda03ac4f37ddc581b1b0e5fff2e7912237', TEXT)
        self.assertIn('persist-credentials: false', TEXT)
        self.assertIn('Freeze the helper closure outside the collection tree', TEXT)
        self.assertIn('--source "$GITHUB_WORKSPACE"', TEXT)
        self.assertIn('CODEQL_EXTRACTOR_RUST_OPTION_CARGO_ALL_TARGETS:', TEXT)
        self.assertIn('include-hidden-files: true', TEXT)
        self.assertIn('path: ${{ runner.temp }}/rust-capacity-evidence', TEXT)
        self.assertNotIn('path: ${{ runner.temp }}/rust-capacity-databases', TEXT)

    def test_embedded_python_parses_and_finalize_has_watchdog(self):
        blocks = re.findall(r"python3 - <<'(PY|PYCODE)'\n(.*?)\n          \1", TEXT, re.S)
        self.assertGreaterEqual(len(blocks), 3)
        for _, block in blocks:
            ast.parse('\n'.join(line[10:] for line in block.splitlines()))
        finalize = TEXT.split('      - id: finalize\n', 1)[1].split('      - id:', 1)[0]
        self.assertIn("'database','finalize'", finalize)
        self.assertIn("'database','trace-command','--use-build-mode'", finalize)
        self.assertIn('phase_end=min(end,time.monotonic()+1200)', finalize)
        self.assertIn('m.runtime.run(', finalize)
        self.assertIn('CAPACITY_DEADLINE', finalize)
        self.assertIn('-180', finalize)
        self.assertNotIn("'analyze'", finalize)
        self.assertIn("'--ram='+os.environ['CODEQL_RAM']", finalize)
        self.assertIn("'--threads='+os.environ['CODEQL_THREADS']", finalize)

    def test_new_query_is_in_snapshot_and_ci(self):
        layout = (ROOT / 'scripts/security/codeql_rust_capacity_layout.py').read_text()
        self.assertIn("'codeql_rust_cfg_graph.py'", layout)
        ci = (ROOT / 'scripts/ci-tests.sh').read_text()
        self.assertIn('test_codeql_rust_cfg_graph.py', ci)
        self.assertIn('codeql-rust-cfg-workflow.test.py', ci)


if __name__ == '__main__':
    unittest.main()
