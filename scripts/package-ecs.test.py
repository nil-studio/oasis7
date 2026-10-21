#!/usr/bin/env python3
"""Exercise ECS compilation selection and real assembly without compiling Rust."""
import importlib.util
import io
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import textwrap
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location("ecs", ROOT / "scripts/package-ecs.py")
ecs = importlib.util.module_from_spec(spec)
spec.loader.exec_module(ecs)


class PackageTests(unittest.TestCase):
    @unittest.skipUnless(sys.platform == "linux" and Path("/usr/bin/time").is_file(), "Linux ECS resource collection")
    def test_compile_step_preserves_build_status_and_stops_monitor(self):
        workflow = (ROOT / ".github/workflows/ecs-packages.yml").read_text()
        step = workflow.split("      - name: Compile three ECS binaries\n", 1)[1].split("      - name:", 1)[0]
        script = textwrap.dedent(step.split("        run: |\n", 1)[1])
        # Execute the actual workflow shell with a short build fixture. A failed
        # observer must neither fail a good build nor hide a failed build.
        for build_status in (0, 7):
            for monitor_fails in (False, True):
                with self.subTest(build_status=build_status, monitor_fails=monitor_fails), tempfile.TemporaryDirectory() as directory:
                    root = Path(directory)
                    (root / "scripts").mkdir()
                    (root / "scripts/package-ecs.py").write_text(
                        f"import time; time.sleep(0.3); raise SystemExit({build_status})\n"
                    )
                    monitor = root / "scripts/package-ecs-resource-monitor.py"
                    if monitor_fails:
                        monitor.write_text("raise SystemExit(9)\n")
                    else:
                        shutil.copyfile(ROOT / "scripts/package-ecs-resource-monitor.py", monitor)
                    result = subprocess.run(
                        ["bash", "--noprofile", "--norc", "-eo", "pipefail", "-c", script],
                        cwd=root, env=dict(os.environ, CARGO_BUILD_JOBS="2", RUNNER_TEMP=str(root / "runner-temp")),
                        text=True, capture_output=True, timeout=10,
                    )
                    self.assertEqual(result.returncode, build_status, result.stdout + result.stderr)
                    self.assertTrue((root / "runner-temp/ecs-reports/ecs-time.txt").is_file())
                    if monitor_fails:
                        self.assertIn("::warning::ECS resource monitor", result.stdout)
                    else:
                        report = json.loads((root / "runner-temp/ecs-reports/ecs-resources.json").read_text())
                        self.assertTrue(report["complete"])

    def test_build_assembly_and_rejection(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            target = root / "target/ecs"
            target.mkdir(parents=True)
            for name in ecs.BINS:
                (target / name).write_bytes(b"ELF fixture")
                (target / name).chmod(0o755)
            stage, web = root / "native", root / "web"
            web.mkdir()
            (web / "index.html").write_text("<html>Viewer</html>")
            (web / "viewer-optional-payload").mkdir()
            (web / "viewer-optional-payload/payload.wasm").write_bytes(b"wasm")
            with patch.object(subprocess, "check_output", return_value="a" * 40), patch.dict(os.environ, {"CARGO_TARGET_DIR": str(target.parent), "RUSTC_WRAPPER": "wrong"}), patch.object(subprocess, "run") as cargo:
                with patch.object(sys, "argv", ["ecs", "native", "--out", str(stage)]):
                    ecs.main()
                command = cargo.call_args.args[0]
                self.assertEqual([command[i + 1] for i, arg in enumerate(command) if arg == "--bin"], list(ecs.BINS))
                self.assertEqual([command[i + 1] for i, arg in enumerate(command) if arg == "-p"], ["oasis7_chain_runtime", "oasis7_launcher_backend", "oasis7_viewer_runtime"])
                self.assertNotIn("RUSTC_WRAPPER", cargo.call_args.kwargs["env"])
                self.assertEqual(command[command.index("--profile") + 1], "ecs")
                self.assertIn("--timings", command)
            original = (stage / "native.json").read_text()
            self.assertEqual(json.loads(original)["profile"], "ecs")

            def assemble(output):
                with patch.object(subprocess, "check_output", return_value="a" * 40), patch.object(sys, "argv", ["ecs", "assemble", "--native", str(stage), "--web", str(web), "--out", str(output)]), patch.object(subprocess, "run", side_effect=AssertionError("assembly must not compile")):
                    ecs.main()

            out = root / "bundle"
            assemble(out)
            self.assertEqual(set(path.name for path in (out / "bin").iterdir()), set(ecs.BINS))
            self.assertIn("profile=ecs\n", (out / "BUILDINFO").read_text())
            self.assertTrue((out / "web-dist/viewer-optional-payload/payload.wasm").is_file())
            for line in (out / "SHA256SUMS").read_text().splitlines():
                sha, path = line.split("  ", 1)
                self.assertEqual(sha, ecs.digest(out / path))
            with self.assertRaises(FileExistsError):
                assemble(out)
            for key in ("source", "profile", "platform", "binaries"):
                bad = json.loads(original)
                bad[key] = "wrong"
                (stage / "native.json").write_text(json.dumps(bad))
                with self.assertRaises(ValueError):
                    assemble(root / ("bad-" + key))
            (stage / "native.json").write_text(original)
            binary = stage / ecs.BINS[0]
            binary.write_bytes(b"tampered")
            with self.assertRaises(ValueError):
                assemble(root / "tampered")
            binary.write_bytes(b"ELF fixture")
            binary.chmod(0o644)
            with self.assertRaises(ValueError):
                assemble(root / "bad-mode")
            binary.chmod(0o755)
            (web / "link").symlink_to(web / "index.html")
            with self.assertRaises(ValueError):
                assemble(root / "symlink")

    def test_native_staging_reset_removes_previous_job_outputs(self):
        text = (ROOT / ".github/workflows/ecs-packages.yml").read_text()
        section = text.split("      - name: Reset native staging directories\n", 1)[1].split("      - uses:", 1)[0]
        script = textwrap.dedent(section.split("        run: |\n", 1)[1])
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for name in ("ecs-native", "ecs-workspace-cache", "ecs-reports"):
                (root / name).mkdir()
                (root / name / "stale").write_text("old source")
            for name in ("ecs-cache-state.json", "ecs-cache-restore.json", "ecs-cache-stage.json", "ecs-units.json", "unrelated"):
                (root / name).write_text("preserve unrelated state")
            for _ in range(2):
                subprocess.run(["bash", "-euo", "pipefail", "-c", script],
                    env=dict(os.environ, RUNNER_TEMP=str(root)), check=True)
                self.assertEqual([path.name for path in root.iterdir()], ["unrelated"])

    def test_runner_selection_and_fallback(self):
        text = (ROOT / ".github/workflows/ecs-packages.yml").read_text()
        section = text.split("  validate:\n", 1)[1].split("  web:\n", 1)[0]
        script = textwrap.dedent(section.split("        run: |\n", 1)[1])
        online = dict(name="scc-ubuntu", status="online", busy=False,
                      labels=[dict(name=name) for name in ("self-hosted", "Linux", "X64")])
        cases = [
            ("auto", "auto", True, "token", online, "self-hosted", "8", 1),
            ("auto", "16", True, "token", online, "self-hosted", "16", 1),
            ("auto", "auto", True, "token", dict(online, status="offline"), "github-hosted", "2", 1),
            ("auto", "16", True, "token", dict(online, busy=True), "github-hosted", "2", 1),
            ("auto", "1", True, "token", dict(online, labels=[]), "github-hosted", "1", 1),
            ("auto", "auto", True, "token", dict(online, name="other"), "github-hosted", "2", 1),
            ("auto", "auto", True, "token", OSError("unavailable"), "github-hosted", "2", 1),
            ("auto", "auto", True, "token", [], "github-hosted", "2", 1),
            ("auto", "auto", True, "token", b"not JSON", "github-hosted", "2", 1),
            ("auto", "auto", True, "", online, "github-hosted", "2", 0),
            ("auto", "auto", False, "token", online, "github-hosted", "2", 0),
            ("github-hosted", "8", True, "token", online, "github-hosted", "2", 0),
        ]
        for mode, jobs, trusted, token, payload, selected, expected_jobs, calls in cases:
            with self.subTest(mode=mode, jobs=jobs, trusted=trusted, payload=payload), tempfile.TemporaryDirectory() as directory:
                output = Path(directory) / "outputs"
                env = dict(NATIVE_RUNNER=mode, NATIVE_JOBS=jobs, SOURCE_REF="refs/heads/main",
                           SOURCE_SHA="a" * 40, WORKFLOW_SHA=("a" if trusted else "b") * 40,
                           RUNNER_STATUS_TOKEN=token, GITHUB_REPOSITORY="nil-studio/oasis7",
                           GITHUB_OUTPUT=str(output))
                with patch.dict(os.environ, env), patch("urllib.request.urlopen") as request:
                    if isinstance(payload, Exception):
                        request.side_effect = payload
                    else:
                        data = payload if isinstance(payload, bytes) else json.dumps(payload).encode()
                        request.return_value = io.BytesIO(data)
                    exec(compile(script, "runner-selection", "exec"), {})
                    self.assertEqual(request.call_count, calls)
                    if calls:
                        self.assertEqual(request.call_args.kwargs["timeout"], 10)
                self.assertEqual(output.read_text(), f"native_runner={selected}\nnative_jobs={expected_jobs}\n")
        # A branch cannot select self-hosted even when source/workflow SHAs match.
        with tempfile.TemporaryDirectory() as directory, patch.dict(os.environ, dict(
                NATIVE_RUNNER="auto", NATIVE_JOBS="auto", SOURCE_REF="refs/heads/feature",
                SOURCE_SHA="same", WORKFLOW_SHA="same", RUNNER_STATUS_TOKEN="token",
                GITHUB_OUTPUT=str(Path(directory) / "outputs"))), patch("urllib.request.urlopen") as request:
            exec(compile(script, "runner-selection", "exec"), {})
            request.assert_not_called()
            self.assertIn("native_runner=github-hosted", (Path(directory) / "outputs").read_text())
        trusted = text.split("  native-trusted:\n", 1)[1].split("  package:\n", 1)[0]
        self.assertIn("needs: validate", trusted)
        self.assertIn("github.ref == 'refs/heads/main' && github.workflow_sha == github.sha", trusted)
        self.assertIn("needs.validate.outputs.native_runner == 'self-hosted'", trusted)
        self.assertIn('fromJSON(\'["self-hosted","Linux","X64"]\')', trusted)
        self.assertIn("steps: *native-steps", trusted)
        self.assertIn("group: ecs-workspace-cache-main-writer", trusted)
        self.assertNotIn("secrets.", text.split("  web:\n", 1)[1])
        self.assertIn("github.ref == 'refs/heads/main' && github.workflow_sha == github.sha && secrets.ECS_RUNNER_STATUS_TOKEN", section)

    def test_workflow_parallel_join_and_trust(self):
        workflow = ROOT / ".github/workflows/ecs-packages.yml"
        # Use the same YAML parser as the repository workflow contracts.
        text = workflow.read_text()
        self.assertNotIn("ref_or_sha", text)
        self.assertNotIn("contents: write", text)
        self.assertIn("needs: [web, native, native-trusted]", text)
        self.assertIn("--web output/web", text)
        self.assertNotIn("build-game-launcher-bundle", text)
        self.assertNotIn("crates/oasis7_client_launcher", text)
        self.assertIn("save-if: ${{ github.job == 'native-trusted' && github.ref == 'refs/heads/main' && github.workflow_sha == github.sha }}", text)
        self.assertIn("!cancelled() && needs.web.result == 'success'", text)
        self.assertIn("needs.native.result == 'success' && needs.native-trusted.result == 'skipped'", text)
        self.assertIn("needs.native.result == 'skipped' && needs.native-trusted.result == 'success'", text)
        for section in (text.split("  web:\n", 1)[1].split("  native:", 1)[0], text.split("  native:\n", 1)[1].split("  native-trusted:", 1)[0]):
            self.assertIn("needs: validate", section)
        self.assertEqual(text.count("ref: ${{ github.sha }}"), 3)


if __name__ == "__main__":
    unittest.main()
