#!/usr/bin/env python3
"""Check producer boundaries and legacy coverage without compiling fixture code."""

from collections import Counter
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest


MOCK_COMMAND = r'''#!/usr/bin/env python3
import json
import os
from pathlib import Path
import sys

command = Path(sys.argv[0]).name
arguments = sys.argv[1:]
if command == "mock-command":
    command, *arguments = arguments
with open(os.environ["OASIS7_STAGE_MOCK_LOG"], "a") as output:
    output.write(json.dumps([command, *arguments]) + "\n")
if command == os.environ.get("OASIS7_STAGE_MOCK_FAIL") and os.environ.get(
    "OASIS7_STAGE_MOCK_FAIL_MATCH", ""
) in " ".join(arguments):
    sys.exit(17)
if command == "worker-build":
    print(os.environ.get("OASIS7_STAGE_MOCK_WORKER_VERSION", "0.7.5"))
elif command == "cargo" and arguments[0] == "check" and "oasis7_custody_worker" in arguments:
    features = arguments[arguments.index("--features") + 1].split(",") if "--features" in arguments else []
    roles = set(features) & {"identity", "owner", "executor", "recovery-fence"}
    error = None
    if len(roles) != 1:
        error = "requires exactly one role feature"
    elif "approval-harness" in features and "owner" not in roles:
        error = "approval-harness requires owner"
    elif "native-runtime-harness" in features and "identity" not in roles:
        error = "native-runtime-harness requires identity"
    if error:
        if os.environ.get("OASIS7_STAGE_MOCK_ACCEPT_INVALID") == "1":
            sys.exit(0)
        print(error, file=sys.stderr)
        sys.exit(1)
elif command == "build-world-fixture.sh":
    artifacts = Path(os.environ["OASIS7_STAGE_MOCK_ROOT"]) / "crates/oasis7_custody_worker/artifacts"
    artifacts.mkdir(exist_ok=True)
    executable = artifacts / "world-fixture"
    executable.symlink_to(os.environ["OASIS7_STAGE_MOCK_COMMAND"])
    (artifacts / "world-test-binary.txt").write_text(str(executable) + "\n")
elif command == "world-fixture":
    case = "tests::custody_happy_submit_barrier_preserves_unknown_and_settles_exact_originals"
    if arguments == ["--list"]:
        print(case + ": test")
        if os.environ.get("OASIS7_STAGE_MOCK_DUPLICATE_WORLD") == "1":
            print("other::" + case + ": test")
    elif arguments == ["--exact", case, "--nocapture"]:
        passed = "0" if os.environ.get("OASIS7_STAGE_MOCK_EMPTY_WORLD") == "1" else "1"
        print(f"test result: ok. {passed} passed; 0 failed; 0 ignored; 0 measured; 0 filtered out")
    else:
        sys.exit(19)
'''


class VerifyStagesTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="custody-verify-stages-")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.scripts = self.root / "crates/oasis7_custody_worker/scripts"
        self.scripts.mkdir(parents=True)
        self.verify = self.scripts / "verify.sh"
        shutil.copyfile(Path(__file__).with_name("verify.sh"), self.verify)
        self.commands = self.root / "bin"
        self.commands.mkdir()
        self.mock = self.commands / "mock-command"
        self.mock.write_text(MOCK_COMMAND)
        self.mock.chmod(0o755)
        for command in ("cargo", "worker-build", "npm", "npx", "node"):
            (self.commands / command).symlink_to(self.mock)
        for script in (
            "build-role.sh",
            "build-approval-harness.sh",
            "build-native-runtime-harness.sh",
            "build-native-fixture.sh",
            "build-world-fixture.sh",
            "export-context-body-fixture.sh",
        ):
            (self.scripts / script).write_text(
                '#!/usr/bin/env bash\nset -euo pipefail\n'
                f'exec "$OASIS7_STAGE_MOCK_COMMAND" "{script}" "$@"\n'
            )
        self.log = self.root / "commands.jsonl"
        self.environment = {
            **os.environ,
            "PATH": str(self.commands) + os.pathsep + os.environ["PATH"],
            "OASIS7_SUPPORT_CAPACITY": "0",
            "OASIS7_STAGE_MOCK_COMMAND": str(self.mock),
            "OASIS7_STAGE_MOCK_LOG": str(self.log),
            "OASIS7_STAGE_MOCK_ROOT": str(self.root),
        }

    def run_verify(self, *arguments, extra_environment=None):
        self.log.write_text("")
        # A fresh runner never inherits a previous producer's output.
        shutil.rmtree(self.scripts.parent / "artifacts", ignore_errors=True)
        result = subprocess.run(
            ["bash", str(self.verify), *arguments],
            cwd=self.root,
            env={**self.environment, **(extra_environment or {})},
            capture_output=True,
            text=True,
            check=False,
        )
        commands = [json.loads(line) for line in self.log.read_text().splitlines()]
        return result, commands

    def assert_success(self, result):
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_unknown_or_incomplete_stage_fails_before_any_command(self):
        for arguments in (
            ("--stage",),
            ("--stage", "unknown"),
            ("--stage", ""),
            ("--stage", "native", "--platform"),
            ("--platform", "extra"),
            ("unknown",),
        ):
            with self.subTest(arguments=arguments):
                result, commands = self.run_verify(*arguments)
                self.assertEqual(result.returncode, 2)
                self.assertEqual(commands, [])
                self.assertIn("usage: verify.sh", result.stderr)

    def test_default_preserves_native_tests_and_complete_role_matrix(self):
        result, commands = self.run_verify()
        self.assert_success(result)
        self.assertEqual(commands[0], [
            "cargo", "test", "-p", "oasis7_custody_core", "-p",
            "oasis7_custody_worker", "--locked",
        ])
        checks = [command for command in commands if command[:2] == ["cargo", "check"]]
        self.assertEqual(len(checks), 16)
        self.assertTrue(all("--locked" in command for command in commands))
        worker_checks = [command for command in checks if "oasis7_custody_worker" in command]
        actual_features = {
            command[command.index("--features") + 1] if "--features" in command else ""
            for command in worker_checks
        }
        self.assertEqual(actual_features, {
            "", "identity", "owner", "executor", "recovery-fence",
            "identity,owner", "identity,recovery-fence", "owner,recovery-fence",
            "executor,identity", "executor,owner", "executor,recovery-fence",
            "identity,approval-harness", "owner,native-runtime-harness",
            "executor,native-runtime-harness", "recovery-fence,native-runtime-harness",
        })
        self.assertTrue(all(command[0] == "cargo" for command in commands))

    def test_invalid_role_success_is_rejected(self):
        result, commands = self.run_verify(
            "--stage", "wasm-production",
            extra_environment={"OASIS7_STAGE_MOCK_ACCEPT_INVALID": "1"},
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("invalid role-less Worker unexpectedly built", result.stderr)
        self.assertFalse(any(command[0] == "build-role.sh" for command in commands))

    def test_three_producers_preserve_full_platform_proof_inventory(self):
        result, full = self.run_verify("--platform")
        self.assert_success(result)
        self.assertEqual(full[-1], ["npm", "test", "--prefix", "crates/oasis7_custody_worker"])
        self.assertEqual([command for command in full if command[0] == "build-role.sh"], [
            ["build-role.sh", role] for role in ("identity", "owner", "executor", "recovery-fence")
        ])
        self.assertEqual(len([command for command in full if command[0] == "npx"]), 4)
        self.assertIn([
            "cargo", "test", "-p", "oasis7_launcher_backend", "--bin",
            "oasis7_game_launcher", "hosted_identity::custody::tests", "--locked",
        ], full)
        self.assertIn(["node", "crates/oasis7_custody_worker/scripts/contract.mjs", "check"], full)
        stages = []
        for stage in ("native", "wasm-production", "wasm-approval"):
            result, commands = self.run_verify("--stage", stage)
            self.assert_success(result)
            stages.extend(commands)
            self.assertFalse(any(command[:2] == ["npm", "test"] for command in commands))
        # Both isolated WASM checkouts install their own tools; only consumers run
        # npm test. Every actual proof/build command remains present exactly once.
        def proof_inventory(commands):
            return Counter(tuple(command) for command in commands if command[0] not in {"npm", "worker-build"})
        self.assertEqual(proof_inventory(stages), proof_inventory(full))

    def test_native_stage_does_not_build_or_install_wasm(self):
        result, commands = self.run_verify("--stage", "native")
        self.assert_success(result)
        self.assertTrue(all(command[0] not in {"worker-build", "npm", "npx", "build-role.sh"} for command in commands))
        self.assertFalse(any("wasm32-unknown-unknown" in command for command in commands))
        self.assertEqual(commands[-1], ["export-context-body-fixture.sh"])

    def test_wasm_stages_own_disjoint_build_outputs(self):
        result, production = self.run_verify("--stage", "wasm-production")
        self.assert_success(result)
        result, approval = self.run_verify("--stage", "wasm-approval")
        self.assert_success(result)
        production_builds = [command[0] for command in production if command[0].startswith("build-")]
        self.assertEqual(production_builds, ["build-role.sh"] * 4 + ["build-native-runtime-harness.sh"])
        approval_builds = [command[0] for command in approval if command[0].startswith("build-")]
        self.assertEqual(approval_builds, ["build-approval-harness.sh"])
        self.assertFalse(any(command[0] == "cargo" for command in approval))

    def test_mismatched_worker_version_installs_pinned_tool(self):
        result, commands = self.run_verify(
            "--stage", "wasm-approval",
            extra_environment={"OASIS7_STAGE_MOCK_WORKER_VERSION": "0.7.4"},
        )
        self.assert_success(result)
        self.assertIn(["cargo", "install", "worker-build", "--version", "0.7.5", "--locked"], commands)

    def test_producer_and_commit_barrier_failures_propagate(self):
        for stage, command, match in (
            ("native", "cargo", "test -p oasis7_custody_core"),
            ("native", "world-fixture", "--exact"),
            ("native", "node", "contract.mjs check"),
            ("wasm-production", "build-role.sh", "owner"),
            ("wasm-approval", "build-approval-harness.sh", ""),
        ):
            with self.subTest(stage=stage, command=command):
                result, commands = self.run_verify("--stage", stage, extra_environment={
                    "OASIS7_STAGE_MOCK_FAIL": command,
                    "OASIS7_STAGE_MOCK_FAIL_MATCH": match,
                })
                self.assertEqual(result.returncode, 17, result.stdout + result.stderr)
                self.assertEqual(commands[-1][0], command)

    def test_commit_barrier_requires_exactly_one_executed_regression(self):
        for environment in ("OASIS7_STAGE_MOCK_EMPTY_WORLD", "OASIS7_STAGE_MOCK_DUPLICATE_WORLD"):
            with self.subTest(environment=environment):
                result, commands = self.run_verify(
                    "--stage", "native", extra_environment={environment: "1"},
                )
                self.assertNotEqual(result.returncode, 0)
                self.assertFalse(any(command[0] == "node" for command in commands))


if __name__ == "__main__":
    unittest.main()
