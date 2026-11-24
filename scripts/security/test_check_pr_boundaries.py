#!/usr/bin/env python3
"""Contract and scenario regressions for scripts/check-pr-boundaries.sh and pr-boundary-gate.yml."""
from __future__ import annotations

import os
from pathlib import Path
import re
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[2]
SCRIPT = ROOT / "scripts/check-pr-boundaries.sh"
WORKFLOW = ROOT / ".github/workflows/pr-boundary-gate.yml"


class PrBoundaryGateTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temp_dir = tempfile.TemporaryDirectory()
        self.repo = Path(self.temp_dir.name) / "repo"
        self.repo.mkdir()
        self.git("init", "-b", "main")
        self.git("config", "user.name", "Test User")
        self.git("config", "user.email", "test@example.com")
        self.write("README.md", "# Oasis7 Fixture\n")
        self.write("crates/oasis7_proto/src/lib.rs", "pub fn proto_v1() {}\n")
        self.write("crates/oasis7_simulator/src/lib.rs", "pub fn sim_v1() {}\n")
        self.write("crates/oasis7_world_state/src/lib.rs", "pub fn state_v1() {}\n")
        self.write("crates/oasis7_viewer/src/lib.rs", "pub fn viewer_v1() {}\n")
        self.write("crates/oasis7_runtime/src/lib.rs", "pub fn runtime_v1() {}\n")
        self.write(
            "fixtures/staged_template.rs",
            "\n".join(f"pub fn staged_{i}() {{}}" for i in range(180)) + "\n",
        )
        self.git("add", ".")
        self.git("commit", "-m", "initial main")
        self.git("update-ref", "refs/remotes/origin/main", "HEAD")

    def tearDown(self) -> None:
        self.temp_dir.cleanup()

    def git(self, *args: str) -> str:
        proc = subprocess.run(
            ["git", *args],
            cwd=self.repo,
            check=True,
            text=True,
            capture_output=True,
        )
        return proc.stdout.strip()

    def write(self, rel_path: str, content: str) -> None:
        target = self.repo / rel_path
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(content, encoding="utf-8")

    def run_gate(
        self,
        *cli_args: str,
        env_overrides: dict[str, str] | None = None,
    ) -> subprocess.CompletedProcess[str]:
        env = dict(os.environ)
        for key in (
            "GITHUB_ACTIONS",
            "BASE_REF",
            "HEAD_REF",
            "PR_TITLE",
            "PR_BODY",
            "PR_LABELS",
            "PR_AUTHOR",
            "MAX_CRATES",
            "MAX_RING_SPAN",
            "WARN_BEHIND_COMMITS",
            "MAX_BEHIND_COMMITS",
            "MAX_LOC_DARK",
            "MAX_LOC_ADAPTER",
            "MAX_LOC_CUTOVER",
            "MAX_LOC_BUGFIX",
            "MAX_LOC_CHORE",
        ):
            env.pop(key, None)
        if env_overrides:
            env.update(env_overrides)
        return subprocess.run(
            ["bash", str(SCRIPT), *cli_args],
            cwd=self.repo,
            env=env,
            text=True,
            capture_output=True,
        )

    def test_1_zero_diff_contract_draft_pr_short_circuits_cleanly(self) -> None:
        self.git("checkout", "-b", "task/empty-contract")
        self.git("commit", "--allow-empty", "-m", "chore: lock scope contract")
        body = (
            "## Boundary Metadata (Machine-Checked)\n"
            "- Slice-Type: Dark <!-- Dark -> Adapter -> Cutover -->\n"
            "- Allowed-Crates: oasis7_simulator, oasis7_world_state <!-- L1 -> L2 -->\n"
            "- Active-Path-Impact: <!-- filled once code lands -->\n"
        )
        res = self.run_gate(
            env_overrides={
                "GITHUB_ACTIONS": "true",
                "PR_TITLE": "[Slice: Dark] feat(sim): prepare dark state model",
                "PR_BODY": body,
            }
        )
        self.assertEqual(res.returncode, 0, res.stdout + res.stderr)
        self.assertIn("Zero-diff Contract Draft PR verified", res.stdout)
        self.assertIn("Slice: Dark", res.stdout)

        # Missing Allowed-Crates fails even on zero-diff Draft
        missing_whitelist = self.run_gate(
            env_overrides={
                "GITHUB_ACTIONS": "true",
                "PR_TITLE": "[Slice: Dark] feat(sim): missing whitelist",
                "PR_BODY": "- Slice-Type: Dark\n- Allowed-Crates: <!-- L1 -> L2 -->\n",
            }
        )
        self.assertNotEqual(missing_whitelist.returncode, 0)
        self.assertIn("Missing 'Allowed-Crates:' scope contract", missing_whitelist.stderr)

        # Invalid upfront ring jump (L0 + L3) in Allowed-Crates fails even on zero-diff Draft
        bad_contract = self.run_gate(
            env_overrides={
                "GITHUB_ACTIONS": "true",
                "PR_TITLE": "[Slice: Dark] feat: invalid contract rings",
                "PR_BODY": "- Slice-Type: Dark\n- Allowed-Crates: oasis7_proto, oasis7_viewer\n",
            }
        )
        self.assertNotEqual(bad_contract.returncode, 0)
        self.assertIn("Declared Allowed-Crates spans L0 -> L3", bad_contract.stderr)

    def test_2_valid_whitelisted_adjacent_ring_changes_pass_and_ignore_locks(self) -> None:
        self.git("checkout", "-b", "task/valid-dark-slice")
        self.write(
            "crates/oasis7_simulator/src/new_dark.rs",
            "\n".join(f"pub const C_{i}: usize = {i};" for i in range(40)) + "\n",
        )
        self.write(
            "crates/oasis7_world_state/src/new_state.rs",
            "\n".join(f"pub const S_{i}: usize = {i};" for i in range(30)) + "\n",
        )
        # Intra-codebase pure file rename within allowed crate should add 0 churn
        self.git("mv", "crates/oasis7_simulator/src/lib.rs", "crates/oasis7_simulator/src/lib_moved.rs")
        # Lockfiles and fixtures should be excluded from effective line churn
        self.write(
            "Cargo.lock",
            "\n".join(f"# lock line {i}" for i in range(1000)) + "\n",
        )
        self.write(
            "fixtures/sample.json",
            "\n".join(f'{{"k": {i}}}' for i in range(900)) + "\n",
        )
        self.git("add", ".")
        self.git("commit", "-m", "feat(sim): add unwired dark slice")

        body = (
            "## Boundary Metadata (Machine-Checked)\n"
            "- **Slice-Type:** `Dark`\n"
            "- **Allowed-Crates:** `oasis7_simulator, oasis7_world_state`\n"
            "- **Active-Path-Impact:** None (Unwired / Additive Only)\n"
        )
        res = self.run_gate(env_overrides={"GITHUB_ACTIONS": "true", "PR_BODY": body})
        self.assertEqual(res.returncode, 0, res.stdout + res.stderr)
        self.assertIn("Effective Churn : 70 lines", res.stdout)
        self.assertIn("oasis7_simulator(L1)", res.stdout)
        self.assertIn("oasis7_world_state(L2)", res.stdout)
        self.assertIn("[PASS] PR boundaries and scope-lock contract within safe limits.", res.stdout)

    def test_3_out_of_whitelist_crate_rename_and_non_ascii_paths_trigger_scope_drift(self) -> None:
        self.git("checkout", "-b", "task/scope-drift")
        self.write("crates/oasis7_simulator/src/lib.rs", "pub fn sim_v2() {}\n")
        self.write("crates/oasis7_world_state/src/lib.rs", "pub fn state_v2() {}\n")
        self.git("add", ".")
        self.git("commit", "-m", "feat: touch unapproved crate")

        res = self.run_gate(
            "--slice",
            "Dark",
            "--allowed-crates",
            "oasis7_simulator",
        )
        self.assertNotEqual(res.returncode, 0)
        self.assertIn("Scope Drift Detected! Touched crate 'oasis7_world_state'", res.stderr)
        self.assertIn("[CIRCUIT BREAKER TRIGGERED]", res.stderr)

        # Allowed-Crates: none rejects any crate modification
        none_res = self.run_gate("--slice", "Chore", "--allowed-crates", "none")
        self.assertNotEqual(none_res.returncode, 0)
        self.assertIn("Scope Drift Detected!", none_res.stderr)

        # Cross-directory git mv into an unapproved crate must still detect Scope Drift
        self.git("checkout", "main")
        self.git("checkout", "-b", "task/rename-drift")
        (self.repo / "crates/oasis7_world_state/moved").mkdir(parents=True, exist_ok=True)
        self.git(
            "mv",
            "crates/oasis7_simulator/src/lib.rs",
            "crates/oasis7_world_state/moved/from_sim.rs",
        )
        self.git("commit", "-m", "refactor: move file into unapproved crate")
        mv_res = self.run_gate(
            "--slice",
            "Dark",
            "--allowed-crates",
            "oasis7_simulator",
        )
        self.assertNotEqual(mv_res.returncode, 0)
        self.assertIn("Scope Drift Detected! Touched crate 'oasis7_world_state'", mv_res.stderr)

        # Non-ASCII path under crates/* (where default core.quotePath=true C-quotes output)
        # must still enforce both [Slice: Docs] ban and Allowed-Crates Scope Drift!
        self.git("checkout", "main")
        self.git("checkout", "-b", "task/non-ascii-crate-path")
        self.write("crates/oasis7_viewer/assets/图标 资源.svg", "<svg/>\n")
        self.git("add", ".")
        self.git("commit", "-m", "feat: add non-ascii viewer asset")

        docs_utf8 = self.run_gate("--slice", "Docs", "--allowed-crates", "none")
        self.assertNotEqual(docs_utf8.returncode, 0)
        self.assertIn("[Slice: Docs] cannot modify Rust/Viewer crate files", docs_utf8.stderr)

        drift_utf8 = self.run_gate(
            "--slice",
            "Dark",
            "--allowed-crates",
            "oasis7_world_service",
        )
        self.assertNotEqual(drift_utf8.returncode, 0)
        self.assertIn("Scope Drift Detected! Touched crate 'oasis7_viewer'", drift_utf8.stderr)

    def test_4_non_adjacent_rings_and_cutover_loc_cap_and_fixture_rename_bypass_blocked(self) -> None:
        # Case 4a: L1 (oasis7_runtime) + L3 (oasis7_viewer) -> ring span 2 > 1
        self.git("checkout", "-b", "task/ring-jump")
        self.write("crates/oasis7_runtime/src/lib.rs", "pub fn runtime_v2() {}\n")
        self.write("crates/oasis7_viewer/src/lib.rs", "pub fn viewer_v2() {}\n")
        self.git("add", ".")
        self.git("commit", "-m", "feat: cross L1 and L3")

        ring_res = self.run_gate(
            "--slice",
            "Adapter",
            "--allowed-crates",
            "oasis7_runtime,oasis7_viewer",
        )
        self.assertNotEqual(ring_res.returncode, 0)
        self.assertIn(
            "Declared Allowed-Crates spans L1 -> L3",
            ring_res.stderr,
        )
        # Also verify actual touched-crate ring span check when unconstrained locally
        ring_local = self.run_gate("--slice", "Adapter")
        self.assertNotEqual(ring_local.returncode, 0)
        self.assertIn("Architecture ring jump too wide: L1 -> L3 (span 2 > 1)", ring_local.stderr)

        # Case 4b: Cutover > 150 lines rejected
        self.git("checkout", "main")
        self.git("checkout", "-b", "task/oversized-cutover")
        self.write(
            "crates/oasis7_runtime/src/cutover.rs",
            "\n".join(f"pub fn wire_{i}() {{}}" for i in range(155)) + "\n",
        )
        self.git("add", ".")
        self.git("commit", "-m", "feat: oversized cutover")

        cutover_res = self.run_gate(
            "--slice",
            "Cutover",
            "--allowed-crates",
            "oasis7_runtime",
        )
        self.assertNotEqual(cutover_res.returncode, 0)
        self.assertIn("exceeds 'Cutover' cap (150)", cutover_res.stderr)

        # Case 4c: Moving 180-line file out of excluded fixtures/* into crates/* must count full 180 lines
        self.git("checkout", "main")
        self.git("checkout", "-b", "task/fixture-to-crate-move")
        self.git("mv", "fixtures/staged_template.rs", "crates/oasis7_runtime/src/from_fixture.rs")
        self.git("commit", "-m", "feat: move staged fixture into active crate")
        fixture_mv_res = self.run_gate(
            "--slice",
            "Cutover",
            "--allowed-crates",
            "oasis7_runtime",
        )
        self.assertNotEqual(fixture_mv_res.returncode, 0)
        self.assertIn("Effective Churn : 180 lines", fixture_mv_res.stdout)
        self.assertIn("exceeds 'Cutover' cap (150)", fixture_mv_res.stderr)

        # Case 4d: > 3 crates rejected even within adjacent rings
        self.git("checkout", "main")
        self.git("checkout", "-b", "task/too-many-crates")
        for crate in ("oasis7_proto", "oasis7_node", "oasis7_net", "oasis7_consensus"):
            self.write(f"crates/{crate}/src/extra.rs", "pub const X: u8 = 1;\n")
        self.git("add", ".")
        self.git("commit", "-m", "feat: touch 4 crates")
        crates_res = self.run_gate("--slice", "Dark")
        self.assertNotEqual(crates_res.returncode, 0)
        self.assertIn("Touched 4 workspace crates (limit: 3)", crates_res.stderr)

    def test_5_override_label_and_dependabot_exemptions_and_staleness_breaker(self) -> None:
        self.git("checkout", "-b", "task/override-exempt")
        self.write("crates/oasis7_proto/src/lib.rs", "pub fn proto_v2() {}\n")
        self.write("crates/oasis7_viewer/src/lib.rs", "pub fn viewer_v2() {}\n")
        self.git("add", ".")
        self.git("commit", "-m", "chore: mechanical cross-ring rename")

        # Without override -> fails ring jump (L0 -> L3)
        blocked = self.run_gate("--slice", "Chore")
        self.assertNotEqual(blocked.returncode, 0)

        # With pr-boundary-override label -> passes with maintainer override warning
        overridden = self.run_gate(
            env_overrides={
                "GITHUB_ACTIONS": "true",
                "PR_LABELS": "type:code,pr-boundary-override,ci-priority",
            }
        )
        self.assertEqual(overridden.returncode, 0, overridden.stdout + overridden.stderr)
        self.assertIn("pr-boundary-override", overridden.stdout)

        # dependabot[bot] author -> passes automatically
        bot_res = self.run_gate(
            env_overrides={
                "GITHUB_ACTIONS": "true",
                "PR_AUTHOR": "dependabot[bot]",
            }
        )
        self.assertEqual(bot_res.returncode, 0, bot_res.stdout + bot_res.stderr)
        self.assertIn("dependabot[bot]", bot_res.stdout)

        # Staleness circuit-breaker when behind main > MAX_BEHIND_COMMITS
        self.git("checkout", "main")
        for idx in range(5):
            self.write("README.md", f"# Advance {idx}\n")
            self.git("add", "README.md")
            self.git("commit", "-m", f"main advance {idx}")
        self.git("update-ref", "refs/remotes/origin/main", "HEAD")
        self.git("checkout", "task/override-exempt")
        stale = self.run_gate(
            "--slice",
            "Dark",
            "--allowed-crates",
            "oasis7_proto",
            env_overrides={"MAX_BEHIND_COMMITS": "3"},
        )
        self.assertNotEqual(stale.returncode, 0)
        self.assertIn("commits behind origin/main (limit: 3)", stale.stderr)

    def test_workflow_contract_pins_actions_and_avoids_script_injection(self) -> None:
        self.assertTrue(SCRIPT.is_file() and os.access(SCRIPT, os.X_OK))
        wf_text = WORKFLOW.read_text(encoding="utf-8")
        self.assertIn("\npermissions:\n", wf_text)
        self.assertIn("persist-credentials: false", wf_text)
        for action in re.findall(r"uses:\s*([^\s]+)", wf_text):
            if not action.startswith("./"):
                self.assertRegex(action, r"^[\w/-]+@[0-9a-f]{40}$")
        # Ensure untrusted PR fields are passed only via env:, never interpolated in run:
        for run_block in re.findall(r"run:\s*\|\n((?:          .*\n)+)", wf_text):
            self.assertNotIn("github.event.pull_request", run_block)


if __name__ == "__main__":
    unittest.main()
