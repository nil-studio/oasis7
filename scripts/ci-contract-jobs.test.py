#!/usr/bin/env python3
"""Comprehensive test suite for CI contract jobs generator and consistency validator."""

import io
import json
import os
from pathlib import Path
import stat
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "scripts"))
import importlib
contract_jobs = importlib.import_module("ci-contract-jobs")


class TestContractJobs(unittest.TestCase):
    def setUp(self):
        self.temp_dir = tempfile.TemporaryDirectory()
        self.repo_root = Path(self.temp_dir.name)
        self.workflows_dir = self.repo_root / ".github" / "workflows"
        self.workflows_dir.mkdir(parents=True)
        self.scripts_dir = self.repo_root / "scripts"
        self.scripts_dir.mkdir(parents=True)

        self.scope_file = self.scripts_dir / "ci-required-scope.json"
        self.scope_file.write_text(
            json.dumps({"groups": list(contract_jobs.MANAGED_GROUPS)}),
            encoding="utf-8",
        )

        self.workflow_file = self.workflows_dir / "rust.yml"
        header = "name: Rust\non:\n  pull_request:\njobs:\n  select:\n    runs-on: ubuntu-24.04\n    steps: []\n\n"
        managed = contract_jobs.render_managed_block()
        footer = "\n\n  workflow-governance:\n    runs-on: ubuntu-24.04\n    steps: []\n"
        self.workflow_file.write_text(header + managed + footer, encoding="utf-8")

    def tearDown(self):
        self.temp_dir.cleanup()

    def test_deterministic_generation(self):
        first = contract_jobs.render_managed_block()
        second = contract_jobs.render_managed_block()
        self.assertEqual(first, second)
        self.assertEqual(first.encode("utf-8"), second.encode("utf-8"))
        self.assertNotIn("\r\n", first)
        self.assertTrue(first.startswith(contract_jobs.BEGIN_MARKER))
        self.assertTrue(first.endswith(contract_jobs.END_MARKER))
        self.assertIn("operational-contracts:", first)
        self.assertIn("packaging-contracts:", first)

    def test_check_mode_success_when_synchronized(self):
        with patch("sys.stdout", new_callable=io.StringIO):
            with self.assertRaises(SystemExit) as cm:
                contract_jobs.main(["--check", "--repo-root", str(self.repo_root)])
            self.assertEqual(cm.exception.code, 0)

    def test_check_mode_detects_drift_without_modifying_file(self):
        content = self.workflow_file.read_text(encoding="utf-8")
        tampered = content.replace("python-version: '3.12'", "python-version: '3.11'")
        self.workflow_file.write_text(tampered, encoding="utf-8")

        stdout_buf = io.StringIO()
        with patch("sys.stdout", stdout_buf):
            with self.assertRaises(SystemExit) as cm:
                contract_jobs.main(["--check", "--repo-root", str(self.repo_root)])
            self.assertEqual(cm.exception.code, 1)

        diff_output = stdout_buf.getvalue()
        self.assertIn("Drift detected in managed contract jobs in .github/workflows/rust.yml.", diff_output)
        self.assertIn("python3 scripts/ci-contract-jobs.py --write", diff_output)
        self.assertIn("-          python-version: '3.11'", diff_output)
        self.assertIn("+          python-version: '3.12'", diff_output)

        # Ensure file was not modified by --check
        self.assertEqual(self.workflow_file.read_text(encoding="utf-8"), tampered)

    def test_write_mode_repairs_drift_and_preserves_surrounding_bytes(self):
        original = self.workflow_file.read_text(encoding="utf-8")
        tampered = original.replace("CI_VERBOSE: \"1\"", "CI_VERBOSE: \"0\"")
        self.workflow_file.write_text(tampered, encoding="utf-8")

        with patch("sys.stdout", new_callable=io.StringIO):
            with self.assertRaises(SystemExit) as cm:
                contract_jobs.main(["--write", "--repo-root", str(self.repo_root)])
            self.assertEqual(cm.exception.code, 0)

        restored = self.workflow_file.read_text(encoding="utf-8")
        self.assertEqual(restored, original)

        # Subsequent check succeeds
        with patch("sys.stdout", new_callable=io.StringIO):
            with self.assertRaises(SystemExit) as cm:
                contract_jobs.main(["--check", "--repo-root", str(self.repo_root)])
            self.assertEqual(cm.exception.code, 0)

        # Subsequent write reports already up to date and exits 0
        with patch("sys.stdout", new_callable=io.StringIO):
            with self.assertRaises(SystemExit) as cm:
                contract_jobs.main(["--write", "--repo-root", str(self.repo_root)])
            self.assertEqual(cm.exception.code, 0)

    def test_preserves_file_mode(self):
        self.workflow_file.chmod(0o644)
        original_mode = stat.S_IMODE(self.workflow_file.stat().st_mode)

        content = self.workflow_file.read_text(encoding="utf-8")
        self.workflow_file.write_text(content.replace("ubuntu-24.04", "ubuntu-22.04"), encoding="utf-8")

        with patch("sys.stdout", new_callable=io.StringIO):
            with self.assertRaises(SystemExit) as cm:
                contract_jobs.main(["--write", "--repo-root", str(self.repo_root)])
            self.assertEqual(cm.exception.code, 0)

        new_mode = stat.S_IMODE(self.workflow_file.stat().st_mode)
        self.assertEqual(original_mode, new_mode)

    def test_rejects_missing_begin_marker(self):
        content = self.workflow_file.read_text(encoding="utf-8")
        corrupted = content.replace(contract_jobs.BEGIN_MARKER, "# missing")
        self.workflow_file.write_text(corrupted, encoding="utf-8")

        for flag in ("--check", "--write"):
            with self.subTest(flag=flag):
                with patch("sys.stderr", new_callable=io.StringIO):
                    with self.assertRaises(SystemExit) as cm:
                        contract_jobs.main([flag, "--repo-root", str(self.repo_root)])
                    self.assertEqual(cm.exception.code, 2)
                self.assertEqual(self.workflow_file.read_text(encoding="utf-8"), corrupted)

    def test_rejects_missing_end_marker(self):
        content = self.workflow_file.read_text(encoding="utf-8")
        corrupted = content.replace(contract_jobs.END_MARKER, "# missing")
        self.workflow_file.write_text(corrupted, encoding="utf-8")

        for flag in ("--check", "--write"):
            with self.subTest(flag=flag):
                with patch("sys.stderr", new_callable=io.StringIO):
                    with self.assertRaises(SystemExit) as cm:
                        contract_jobs.main([flag, "--repo-root", str(self.repo_root)])
                    self.assertEqual(cm.exception.code, 2)
                self.assertEqual(self.workflow_file.read_text(encoding="utf-8"), corrupted)

    def test_rejects_duplicate_markers(self):
        content = self.workflow_file.read_text(encoding="utf-8")
        corrupted = contract_jobs.BEGIN_MARKER + "\n" + content
        self.workflow_file.write_text(corrupted, encoding="utf-8")

        with patch("sys.stderr", new_callable=io.StringIO):
            with self.assertRaises(SystemExit) as cm:
                contract_jobs.main(["--check", "--repo-root", str(self.repo_root)])
            self.assertEqual(cm.exception.code, 2)

    def test_rejects_reversed_markers(self):
        content = self.workflow_file.read_text(encoding="utf-8")
        corrupted = content.replace(contract_jobs.BEGIN_MARKER, "PLACEHOLDER")
        corrupted = corrupted.replace(contract_jobs.END_MARKER, contract_jobs.BEGIN_MARKER)
        corrupted = corrupted.replace("PLACEHOLDER", contract_jobs.END_MARKER)
        self.workflow_file.write_text(corrupted, encoding="utf-8")

        with patch("sys.stderr", new_callable=io.StringIO):
            with self.assertRaises(SystemExit) as cm:
                contract_jobs.main(["--check", "--repo-root", str(self.repo_root)])
            self.assertEqual(cm.exception.code, 2)

    def test_rejects_markers_outside_jobs(self):
        content = self.workflow_file.read_text(encoding="utf-8")
        managed = contract_jobs.render_managed_block()
        content_without_managed = content.replace(managed, "")
        corrupted = managed + "\n" + content_without_managed
        self.workflow_file.write_text(corrupted, encoding="utf-8")

        with patch("sys.stderr", new_callable=io.StringIO):
            with self.assertRaises(SystemExit) as cm:
                contract_jobs.main(["--check", "--repo-root", str(self.repo_root)])
            self.assertEqual(cm.exception.code, 2)

    def test_rejects_duplicate_jobs_outside_managed_block(self):
        content = self.workflow_file.read_text(encoding="utf-8")
        duplicated = content + "\n  operational-contracts:\n    runs-on: ubuntu-24.04\n"
        self.workflow_file.write_text(duplicated, encoding="utf-8")

        with patch("sys.stderr", new_callable=io.StringIO):
            with self.assertRaises(SystemExit) as cm:
                contract_jobs.main(["--check", "--repo-root", str(self.repo_root)])
            self.assertEqual(cm.exception.code, 2)

    def test_rejects_unknown_or_missing_group_in_scope_config(self):
        self.scope_file.write_text(json.dumps({"groups": ["other_group"]}), encoding="utf-8")

        with patch("sys.stderr", new_callable=io.StringIO):
            with self.assertRaises(SystemExit) as cm:
                contract_jobs.main(["--check", "--repo-root", str(self.repo_root)])
            self.assertEqual(cm.exception.code, 2)

    def test_rejects_symlink_workflow(self):
        real_file = self.repo_root / "real_workflow.yml"
        real_file.write_text(self.workflow_file.read_text(encoding="utf-8"), encoding="utf-8")
        self.workflow_file.unlink()
        self.workflow_file.symlink_to(real_file)

        with patch("sys.stderr", new_callable=io.StringIO):
            with self.assertRaises(SystemExit) as cm:
                contract_jobs.main(["--check", "--repo-root", str(self.repo_root)])
            self.assertEqual(cm.exception.code, 2)

    def test_rejects_symlink_parent_directory(self):
        real_workflows = self.repo_root / "real_workflows"
        real_workflows.mkdir()
        (real_workflows / "rust.yml").write_text(self.workflow_file.read_text(encoding="utf-8"), encoding="utf-8")

        self.workflow_file.unlink()
        self.workflows_dir.rmdir()
        self.workflows_dir.symlink_to(real_workflows)

        with patch("sys.stderr", new_callable=io.StringIO):
            with self.assertRaises(SystemExit) as cm:
                contract_jobs.main(["--check", "--repo-root", str(self.repo_root)])
            self.assertEqual(cm.exception.code, 2)

    def test_concurrent_modification_detected_on_write(self):
        content = self.workflow_file.read_text(encoding="utf-8")
        tampered = content.replace("ubuntu-24.04", "ubuntu-22.04")
        self.workflow_file.write_text(tampered, encoding="utf-8")

        original_read_bytes = Path.read_bytes
        call_count = [0]

        wf_file = self.workflow_file
        def simulated_read_bytes(*args, **kwargs):
            call_count[0] += 1
            if call_count[0] == 2:  # Re-reading in write_workflow
                return b"CONCURRENT_MODIFICATION"
            return original_read_bytes(wf_file)

        with patch("sys.stderr", new_callable=io.StringIO):
            with patch.object(Path, "read_bytes", side_effect=simulated_read_bytes):
                with self.assertRaises(SystemExit) as cm:
                    contract_jobs.main(["--write", "--repo-root", str(self.repo_root)])
                self.assertEqual(cm.exception.code, 2)

        # Verify no temp files left behind
        temp_files = list(self.workflows_dir.glob(".rust.yml.tmp-*"))
        self.assertEqual(temp_files, [])

    def test_cli_flags_mutual_exclusion(self):
        with patch("sys.stderr", new_callable=io.StringIO):
            with self.assertRaises(SystemExit) as cm:
                contract_jobs.main([])
            self.assertEqual(cm.exception.code, 2)

        with patch("sys.stderr", new_callable=io.StringIO):
            with self.assertRaises(SystemExit) as cm:
                contract_jobs.main(["--check", "--write"])
            self.assertEqual(cm.exception.code, 2)

    def test_semantic_equivalence_of_managed_jobs(self):
        block = contract_jobs.render_managed_block()

        # Job IDs and sequence
        op_idx = block.find("  operational-contracts:\n")
        pkg_idx = block.find("  packaging-contracts:\n")
        self.assertNotEqual(op_idx, -1)
        self.assertNotEqual(pkg_idx, -1)
        self.assertLess(op_idx, pkg_idx)

        # Both depend on select
        self.assertEqual(block.count("    needs: select"), 2)

        # Runs on ubuntu-24.04
        self.assertEqual(block.count("    runs-on: ubuntu-24.04"), 2)

        # Exact conditional expressions
        self.assertIn("needs.select.outputs.run_operational_contracts == 'true'", block)
        self.assertIn("needs.select.outputs.run_packaging_contracts == 'true'", block)
        self.assertEqual(block.count("fromJSON(needs.select.outputs.plan).scope == 'full'"), 2)
        self.assertEqual(block.count("always() && !cancelled() && needs.select.result == 'success'"), 2)

        # Driver invocations
        self.assertIn('bash ./scripts/ci-tests.sh required --group "operational_contracts" --repo-root "$GITHUB_WORKSPACE"', block)
        self.assertIn('bash ./scripts/ci-tests.sh required --group "packaging_contracts" --repo-root "$GITHUB_WORKSPACE"', block)

        # mold in packaging-contracts but not operational-contracts
        op_section = block[op_idx:pkg_idx]
        pkg_section = block[pkg_idx:]
        self.assertNotIn("setup-linux-mold", op_section)
        self.assertIn("options: --user 1001:1001", op_section)
        self.assertIn('test "$(id -u)" -ne 0', op_section)
        self.assertIn('mktemp -d "$GITHUB_WORKSPACE/.operational-contracts.XXXXXX"', op_section)
        self.assertIn("dpkg-deb --version", op_section)
        self.assertNotIn("actions/setup-python", op_section)
        self.assertIn("setup-linux-mold", pkg_section)
        self.assertIn("*linux-mold-tools-checkout", pkg_section)

    def test_live_repository_passes_check(self):
        with patch("sys.stdout", new_callable=io.StringIO):
            with self.assertRaises(SystemExit) as cm:
                contract_jobs.main(["--check", "--repo-root", str(ROOT)])
            self.assertEqual(cm.exception.code, 0)


if __name__ == "__main__":
    unittest.main()
