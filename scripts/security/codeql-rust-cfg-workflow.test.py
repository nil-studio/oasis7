#!/usr/bin/env python3
"""Control regressions for the manual, non-security CFG diagnostic."""
import ast
import contextlib
import datetime
import email.utils
import io
import json
from pathlib import Path
import re
import os
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
TEXT = (ROOT / '.github/workflows/codeql-rust-cfg-diagnostic.yml').read_text().split('  evaluator_profile:',1)[0]


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
        self.assertIn('c65098923e8d9aad6813d49ac48cc2540627c96d', TEXT)
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

    def test_external_helpers_import_from_business_cwd(self):
        # Reproduce the real stdin-Python boundary: cwd has business source,
        # helpers live elsewhere, and PYTHONPATH supplies no accidental repair.
        for step in ('finalize', 'integrity'):
            block = TEXT.split('      - id: '+step+'\n', 1)[1].split('      - id:', 1)[0]
            raw = block.split("python3 - <<'PYCODE'\n", 1)[1].split('          PYCODE', 1)[0]
            code = '\n'.join(line[10:] for line in raw.splitlines())
            prefix = code.split('spec.loader.exec_module(m)', 1)[0]+'spec.loader.exec_module(m)'
            with tempfile.TemporaryDirectory() as directory:
                root = Path(directory).resolve()
                tools = root/'rust-capacity-helper';tools.mkdir()
                for file in (ROOT/'scripts/security').glob('*.py'):
                    shutil.copyfile(file, tools/file.name)
                source = root/'business';source.mkdir()
                # An input-tree module must not shadow the trusted snapshot.
                (source/'codeql_rust_diagnostic_runtime.py').write_text('raise RuntimeError("business module imported")')
                env = dict(os.environ, RUNNER_TEMP=str(root), PYTHONPATH='')
                code = prefix+'\nassert m.runtime.__file__ == '+repr(str(tools/'codeql_rust_diagnostic_runtime.py'))+'\nassert callable(m.after_identity)\n'
                result = subprocess.run([sys.executable, '-c', code], cwd=source, env=env, capture_output=True, text=True, timeout=20)
                self.assertEqual(result.returncode, 0, step+': '+result.stderr)

    def test_exclusion_verification_is_bound_to_new_source_everywhere(self):
        sha="c65098923e8d9aad6813d49ac48cc2540627c96d"
        self.assertIn(sha,TEXT)
        self.assertIn("ref: ${{ env.DIAGNOSTIC_SOURCE }}",TEXT)
        self.assertEqual(TEXT.count('--source-sha "$DIAGNOSTIC_SOURCE"'),3)
        self.assertEqual(TEXT.count("--verify-context-exclusion"),2)
        self.assertEqual(TEXT.count("--scope-identities"),2)
        self.assertNotIn("d2bf383d8c377b36de83aae4a06d9ec17612e205",TEXT)
        capacity=(ROOT/".github/workflows/codeql-rust-capacity.yml").read_text()
        self.assertIn("CAPACITY_SOURCE: ${{ inputs.mode == 'official-default-research' && 'c036927d33b917a036a731f071209e16f6deb033' || '7c2f99a0ff2fbc3a3523dee7a1cc06fa5bf187ef' }}",capacity)
        self.assertIn("ref: ${{ env.CAPACITY_SOURCE }}",capacity)
        self.assertEqual(capacity.count('--source-sha "$CAPACITY_SOURCE"'),4)

    def test_new_query_is_in_snapshot_and_ci(self):
        layout = (ROOT / 'scripts/security/codeql_rust_capacity_layout.py').read_text()
        self.assertIn("'codeql_rust_cfg_graph.py'", layout)
        ci = (ROOT / 'scripts/ci-tests.sh').read_text()
        self.assertIn('test_codeql_rust_cfg_graph.py', ci)
        self.assertIn('codeql-rust-cfg-workflow.test.py', ci)


class HostedClock(unittest.TestCase):
    """A hosted API timestamp must not be compared with a skewed runner clock."""

    def clocks(self):
        entries = []
        for file in ('codeql-rust-capacity.yml', 'codeql-rust-cfg-diagnostic.yml',
                     'codeql-rust-cargo-check.yml'):
            text = (ROOT / '.github/workflows' / file).read_text()
            for block in re.findall(r"python3 - <<'PY'\n(.*?)\n          PY", text, re.S):
                if "['started_at']" not in block:
                    continue
                code = '\n'.join(line[10:] for line in block.splitlines())
                name = re.search(r"\['name'\]=='([^']+)'", code).group(1)
                entries.append((file, name, code))
        self.assertEqual(len(entries), 4)
        return entries

    def run_clock(self, code, name, folder, *, offset=0, age=2, header=True):
        started = datetime.datetime(2026, 11, 1, 9, 45, 42, tzinfo=datetime.timezone.utc)
        server_now = started.timestamp() + age
        runner_now = server_now + offset
        response = io.StringIO(json.dumps({'jobs': [dict(
            id=123, name=name, status='in_progress', started_at=started.isoformat())]}))
        response.headers = {'Date': email.utils.format_datetime(
            datetime.datetime.fromtimestamp(server_now, datetime.timezone.utc), usegmt=True)} if header else {}
        output = Path(folder) / 'env'
        env = dict(GITHUB_REPOSITORY='example/repo', GITHUB_RUN_ID='1',
                   GITHUB_RUN_ATTEMPT='1', GH_TOKEN='test-token',
                   GITHUB_ENV=str(output), RUNNER_TEMP=folder, CAPACITY_MODE='capacity')
        with patch.dict(os.environ, env), patch('urllib.request.urlopen', return_value=response), \
                patch('time.time', return_value=runner_now), \
                patch('time.monotonic', side_effect=[10.0, 11.0]), contextlib.redirect_stdout(io.StringIO()):
            exec(compile(code, 'hosted-clock', 'exec'), {})
        values = dict(line.split('=', 1) for line in output.read_text().splitlines())
        return float(values.get('CAPACITY_DEADLINE', values.get('CHECK_DEADLINE')))-runner_now

    def test_runner_clock_skew_preserves_actual_job_budget(self):
        budgets = {'CodeQL Rust capacity': 5400, 'CodeQL Rust CFG diagnostic': 2700,
                   'CodeQL Rust evaluator profile': 3300, 'Cargo check diagnostic': 900}
        for file, name, code in self.clocks():
            for offset in (-3600, 0, 3600):
                with self.subTest(file=file, job=name, offset=offset), tempfile.TemporaryDirectory() as folder:
                    remaining = self.run_clock(code, name, folder, offset=offset)
                    # Two seconds of server-side age, one of request time and one
                    # for HTTP Date rounding have already consumed the budget.
                    self.assertEqual(remaining, budgets[name] - 4)

    def test_unknown_future_or_stale_server_clock_stops_before_setup(self):
        for file, name, code in self.clocks():
            for args in ({'header': False}, {'age': -10}, {'age': 121}):
                with self.subTest(file=file, job=name, args=args), tempfile.TemporaryDirectory() as folder:
                    with self.assertRaises((SystemExit, ValueError, TypeError)):
                        self.run_clock(code, name, folder, **args)
                    self.assertFalse((Path(folder) / 'env').exists())


if __name__ == '__main__':
    unittest.main()
