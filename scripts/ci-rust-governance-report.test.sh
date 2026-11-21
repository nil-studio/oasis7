#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)

exec python3 -I - "$repo_root/scripts/ci-rust-governance-report.sh" <<'PY'
import json
import os
from pathlib import Path
import shlex
import shutil
import subprocess
import sys
import tempfile
import unittest

REPORT = Path(sys.argv.pop())
BASH = shutil.which("bash")
HITS = (
    "./crates/oasis7/examples/archive_memory.rs:21:unsafe impl Example {}\n"
    "crates/oasis7/src/main.rs:10:unsafe fn run() {}\n"
    "./vendor-foo/src/lib.rs:5:unsafe { example(); }\n"
    "warning: cannot access fixture\n"
)


class GovernanceReport(unittest.TestCase):
    def report(self, *, scanner=None, scan_rc=0, scan_output="", deny_rc=0,
               deny_output="", deny_available=True):
        with tempfile.TemporaryDirectory(prefix="o7-governance-report-") as work:
            root = Path(work)
            tools = root / "bin"
            scripts = root / "scripts"
            tools.mkdir()
            scripts.mkdir()
            report = scripts / REPORT.name
            shutil.copyfile(REPORT, report)

            # No inherited PATH or CI/tool configuration: only fixture tools
            # and the core commands used by the production entry are available.
            for name in ("bash", "cat", "dirname", "mkdir"):
                executable = shutil.which(name)
                self.assertIsNotNone(executable, name)
                (tools / name).symlink_to(executable)
            (tools / "python3").symlink_to(sys.executable)

            def tool(path, body):
                path.write_text("#!" + BASH + "\nset -eu\n" + body + "\n", encoding="utf-8")
                path.chmod(0o755)

            def emit(output, rc):
                return f"printf '%s' {shlex.quote(output)}\nexit {rc}"

            tool(tools / "cargo", 'case "$*" in\n'
                 f'  "deny check")\n{emit(deny_output, deny_rc)}\n;;\n'
                 '  "tree -d") printf "fixture dependency\\n" ;;\n'
                 '  *) exit 99 ;;\nesac')
            tool(scripts / "ensure-cargo-deny.sh", f"exit {0 if deny_available else 127}")
            for name in ("check-rustsec-ignore-baseline.sh",
                         "check-standalone-tool-lockfiles.sh",
                         "check-launcher-p2p-dependency-surface.sh"):
                tool(scripts / name, "exit 0")
            # Prove the entry hands its first JSON summary to the baseline
            # checker and appends that command's report-only result.
            tool(scripts / "check-duplicate-dependency-baseline.sh",
                 '[[ "$#" == 1 && -s "$1" && "$1" == */summary.json ]] || exit 98\nexit 5')
            if scanner == "rg":
                tool(tools / "rg", '[[ "$1" == -n && "$2" == --glob ]] || exit 98\n'
                     + emit(scan_output, scan_rc))
            elif scanner == "git":
                tool(tools / "git", 'case "$1" in\n'
                     '  rev-parse) [[ "$*" == "rev-parse --is-inside-work-tree" ]] || exit 98; printf "true\\n" ;;\n'
                     f'  grep)\n{emit(scan_output, scan_rc)}\n;;\n'
                     '  *) exit 99 ;;\nesac')

            out = root / "reports"
            result = subprocess.run(
                [BASH, str(report), "--out-dir", str(out)], cwd=root,
                env={"PATH": str(tools), "LC_ALL": "C"}, capture_output=True,
                text=True, encoding="utf-8", timeout=15,
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            summary = json.loads((out / "summary.json").read_text(encoding="utf-8"))
            markdown = (out / "summary.md").read_text(encoding="utf-8")
            self.assertEqual(summary["duplicate_dependency_baseline_rc"], 5)
            self.assertIn("| duplicate dependency baseline | 5 |", markdown)
            self.assertEqual(result.stdout, markdown)
            return summary, markdown

    def test_missing_scanner_is_unknown_not_a_hit(self):
        summary, markdown = self.report()
        self.assertEqual(summary["unsafe_usage_rc"], 127)
        self.assertIsNone(summary["unsafe_usage_total"])
        self.assertEqual(summary["unsafe_usage_top_buckets"], [])
        self.assertIn("- Unsafe usage matches: `unknown`", markdown)
        self.assertNotIn("Top Unsafe Usage Buckets", markdown)
        self.assertNotIn(REPORT.name, markdown)

    def test_missing_ripgrep_uses_git_instead(self):
        summary, markdown = self.report(scanner="git", scan_output=HITS)
        self.assertEqual(summary["unsafe_usage_rc"], 0)
        self.assertEqual(summary["unsafe_usage_total"], 3)
        self.assertIn("- Unsafe usage matches: `3`", markdown)

    def test_successful_scan_without_matches_is_zero(self):
        summary, markdown = self.report(scanner="rg", scan_rc=1)
        self.assertEqual(summary["unsafe_usage_rc"], 1)
        self.assertEqual(summary["unsafe_usage_total"], 0)
        self.assertEqual(summary["unsafe_usage_top_buckets"], [])
        self.assertIn("- Unsafe usage matches: `0`", markdown)

    def test_successful_scan_buckets_only_rust_hits(self):
        summary, markdown = self.report(scanner="rg", scan_output=HITS)
        self.assertEqual(summary["unsafe_usage_rc"], 0)
        self.assertEqual(summary["unsafe_usage_total"], 3)
        self.assertEqual(dict(summary["unsafe_usage_top_buckets"]), {
            "crates/oasis7/examples": 1, "crates/oasis7/src": 1, "vendor-foo/src/lib.rs": 1,
        })
        self.assertIn("- Unsafe usage matches: `3`", markdown)

    def test_ansi_and_plain_duplicate_reports_match(self):
        plain = "".join(
            f"warning[duplicate]: found {3 if i < 8 else 2} duplicate entries for crate 'fixture-{i:02d}'\n"
            for i in range(79)
        )
        # The fixture preserves the incident's counts (79/79/166), not raw
        # dependency names or hosted logs.
        colored = plain.replace("warning[duplicate]", "\x1b[33mwarning[duplicate]\x1b[0m\x1b[1m")
        actual, markdown = self.report(deny_output=colored)
        control, plain_markdown = self.report(deny_output=plain)
        self.assertEqual(actual, control)
        self.assertEqual(markdown, plain_markdown)
        self.assertEqual(actual["duplicate_dependency_cluster_count"], 79)
        self.assertEqual(actual["duplicate_dependency_unique_crates"], 79)
        self.assertEqual(actual["duplicate_dependency_entry_total"], 166)
        self.assertIn("- Duplicate dependency entries: `166`", markdown)

    def test_license_failure_preserves_exit_code_and_duplicate_data(self):
        summary, markdown = self.report(
            deny_rc=4, deny_output="warning[duplicate]\x1b[0m: found 2 duplicate entries for crate 'foo'\n"
            "error[licenses]: failed license check\n",
        )
        self.assertEqual(summary["cargo_deny_rc"], 4)
        self.assertEqual(summary["duplicate_dependency_cluster_count"], 1)
        self.assertEqual(summary["duplicate_dependency_unique_crates"], 1)
        self.assertEqual(summary["duplicate_dependency_entry_total"], 2)
        self.assertIn("| cargo deny check | 4 |", markdown)
        self.assertIn("- Duplicate dependency entries: `2`", markdown)

    def test_unavailable_cargo_deny_displays_unknown(self):
        summary, markdown = self.report(deny_available=False)
        self.assertEqual(summary["cargo_deny_rc"], 127)
        self.assertIn("- Duplicate dependency clusters: `unknown`", markdown)
        self.assertIn("- Duplicate dependency unique crates: `unknown`", markdown)
        self.assertIn("- Duplicate dependency entries: `unknown`", markdown)


unittest.main(argv=[sys.argv[0]])
PY
