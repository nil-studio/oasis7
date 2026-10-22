#!/usr/bin/env python3
"""Exercise ECS compilation selection and real assembly without compiling Rust."""
import importlib.util
import argparse
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
from unittest.mock import Mock, patch

ROOT = Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location("ecs", ROOT / "scripts/package-ecs.py")
ecs = importlib.util.module_from_spec(spec)
spec.loader.exec_module(ecs)


class PackageTests(unittest.TestCase):
    def test_smoke_failure_receipt_projects_only_fixed_safe_values(self):
        import time
        module_spec = importlib.util.spec_from_file_location(
            "ecs_cold_entry_smoke_diagnostics_test", ROOT / "scripts/package-ecs-cold-entry-smoke.py")
        smoke_module = importlib.util.module_from_spec(module_spec)
        module_spec.loader.exec_module(smoke_module)
        with tempfile.TemporaryDirectory() as directory:
            smoke = smoke_module.Smoke({}, Path(directory), time.monotonic() + 10)
            smoke.stage = "cold_entry_response_loss_recovery"
            smoke.faults = [{"hit": True}, {"hit": False}]
            secret = "/private/secret-account-operation-body"
            smoke.begin_cold_entry_attempt()
            smoke.record_cold_entry_result(0, json.dumps({
                "stage": "executor_bind", "state": "unknown",
                "next_action": "query_original_owner_operation",
                "operation_id": secret, "error": secret}).encode())
            receipt = smoke.write_failure_receipt()
            self.assertEqual(receipt["last_result"], {
                "rc": 0, "stage": "executor_bind", "state": "unknown",
                "next_action": "query_original_owner_operation"})
            self.assertEqual(receipt["attempts"], 1)
            self.assertEqual(receipt["response_loss"], {"registration": True, "executor_bind": False})
            self.assertEqual(set(receipt), {"state", "failure_stage", "private_details_omitted",
                                          "attempts", "last_result", "response_loss"})
            self.assertNotIn(secret, json.dumps(receipt))
            self.assertEqual(json.loads((Path(directory) / "receipt.json").read_text()), receipt)
            for code, output in [(0, json.dumps({"stage": secret, "state": secret,
                                                 "next_action": secret}).encode()),
                                 (1, json.dumps({"stage": "complete", "state": "confirmed",
                                                 "next_action": secret}).encode()),
                                 (0, b'private diagnostic\n{"stage":"complete"}'),
                                 (0, b'[]'), (0, b'{"stage":[]}')]:
                smoke.record_cold_entry_result(code, output)
                result = smoke.write_failure_receipt()
                self.assertEqual(result["last_result"], {
                    "rc": code, "stage": "unavailable", "state": "unavailable",
                    "next_action": "unavailable"})
                self.assertNotIn(secret, json.dumps(result))

    def test_smoke_failure_receipt_resets_stale_result_and_counts_attempts(self):
        import time
        module_spec = importlib.util.spec_from_file_location(
            "ecs_cold_entry_smoke_attempts_test", ROOT / "scripts/package-ecs-cold-entry-smoke.py")
        smoke_module = importlib.util.module_from_spec(module_spec)
        module_spec.loader.exec_module(smoke_module)
        with tempfile.TemporaryDirectory() as directory:
            smoke = smoke_module.Smoke({}, Path(directory), time.monotonic() + 10)
            for _ in range(30):
                smoke.begin_cold_entry_attempt()
                smoke.record_cold_entry_result(0, b'{"stage":"login","state":"action_required",'
                                                   b'"next_action":"explicit_fresh_login"}')
            receipt = smoke.write_failure_receipt()
            self.assertEqual(receipt["attempts"], 30)
            self.assertEqual(receipt["last_result"]["state"], "action_required")
            self.assertEqual(receipt["response_loss"], {"registration": False, "executor_bind": False})
            # A command that times out before returning cannot inherit prior progress.
            smoke.begin_cold_entry_attempt()
            smoke.stage = "/private/arbitrary-stage"
            receipt = smoke.write_failure_receipt()
            self.assertEqual(receipt["failure_stage"], "unavailable")
            self.assertEqual(receipt["last_result"], {
                "rc": None, "stage": "unavailable", "state": "unavailable", "next_action": "unavailable"})

    def test_smoke_main_writes_sanitized_failure_receipt_before_cleanup(self):
        module_spec = importlib.util.spec_from_file_location(
            "ecs_cold_entry_smoke_finally_test", ROOT / "scripts/package-ecs-cold-entry-smoke.py")
        smoke_module = importlib.util.module_from_spec(module_spec)
        module_spec.loader.exec_module(smoke_module)
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            fixture = root / "fixture.json"
            fixture.write_text(json.dumps({
                "client_binary": "unused", "setup_commands": [], "server_commands": [["unused"]],
                "plan_template": {}, "identity_upstream": ["127.0.0.1", 12345],
                "initial_probe": [], "canonical_probe": [],
                "binding_response_path": "/api/public/hosted-account/v2/entry-owner/control"}))
            fixture.chmod(0o600)
            evidence = root / "evidence"
            output = io.StringIO()
            with patch.object(sys, "argv", ["smoke", "--fixture", str(fixture),
                                            "--evidence-directory", str(evidence)]), \
                    patch.object(sys, "platform", "linux"), \
                    patch.object(smoke_module.os, "umask"), \
                    patch.object(smoke_module, "generate_tls", side_effect=RuntimeError("private secret")), \
                    patch.object(sys, "stderr", output):
                with self.assertRaises(RuntimeError):
                    smoke_module.main()
            receipt = json.loads((evidence / "receipt.json").read_text())
            self.assertEqual(json.loads(output.getvalue()), receipt)
            self.assertEqual(receipt["failure_stage"], "temporary_tls")
            self.assertNotIn("private secret", json.dumps(receipt))

    def test_smoke_cleanup_survives_receipt_print_and_server_errors(self):
        module_spec = importlib.util.spec_from_file_location(
            "ecs_cold_entry_smoke_cleanup_errors_test", ROOT / "scripts/package-ecs-cold-entry-smoke.py")
        smoke_module = importlib.util.module_from_spec(module_spec)
        module_spec.loader.exec_module(smoke_module)
        for failure in ("receipt", "print", "shutdown", "server_close"):
            with self.subTest(failure=failure), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                fixture = root / "fixture.json"
                fixture.write_text(json.dumps({
                    "client_binary": "unused", "setup_commands": [], "server_commands": [["unused"]],
                    "plan_template": {}, "identity_upstream": ["127.0.0.1", 12345],
                    "initial_probe": [], "canonical_probe": [],
                    "binding_response_path": "/api/public/hosted-account/v2/entry-owner/control"}))
                fixture.chmod(0o600)
                server = Mock(server_port=12345)
                if failure in ("shutdown", "server_close"):
                    getattr(server, failure).side_effect = OSError("injected cleanup I/O failure")
                with patch.object(sys, "argv", ["smoke", "--fixture", str(fixture),
                                                "--evidence-directory", str(root / "evidence")]), \
                        patch.object(sys, "platform", "linux"), patch.object(smoke_module.os, "umask"), \
                        patch.object(smoke_module, "generate_tls"), \
                        patch.object(smoke_module, "forwarder", return_value=server), \
                        patch.object(smoke_module.Smoke, "command", side_effect=RuntimeError("setup failed")), \
                        patch.object(smoke_module.Smoke, "close") as close, \
                        patch.object(smoke_module.Smoke, "write_failure_receipt", **(
                            {"side_effect": OSError("receipt I/O failed")} if failure == "receipt"
                            else {"return_value": {"state": "failed"}})), \
                        patch("builtins.print", **(
                            {"side_effect": OSError("print I/O failed")} if failure == "print" else {})):
                    with self.assertRaises(OSError):
                        smoke_module.main()
                    server.shutdown.assert_called_once_with()
                    server.server_close.assert_called_once_with()
                    close.assert_called_once_with()

    def test_smoke_recovery_obeys_server_backoff_and_deadline(self):
        module_spec = importlib.util.spec_from_file_location(
            "ecs_cold_entry_smoke_backoff_test", ROOT / "scripts/package-ecs-cold-entry-smoke.py")
        module = importlib.util.module_from_spec(module_spec)
        module_spec.loader.exec_module(module)
        with tempfile.TemporaryDirectory() as directory:
            now = [100.0]
            with patch.object(module.time, "monotonic", side_effect=lambda: now[0]), \
                    patch.object(module.time, "sleep", side_effect=lambda seconds: now.__setitem__(0, now[0] + seconds)) as sleep:
                smoke = module.Smoke({}, Path(directory), 300)
                smoke.observe_retry_after(200, "120")
                smoke.wait_recovery(1)
                self.assertEqual(now[0], 101)
                smoke.observe_retry_after(429, "60")
                smoke.observe_retry_after(429, "1")
                smoke.wait_recovery(1)
                self.assertEqual(now[0], 161)
                smoke.observe_retry_after(429, "invalid/private-header")
                smoke.deadline = 200
                sleep.reset_mock()
                with self.assertRaises(RuntimeError):
                    smoke.wait_recovery(1)
                sleep.assert_not_called()
                smoke.deadline = 400
                smoke.wait_recovery(1)
                self.assertEqual(now[0], 221)
                smoke.observe_retry_after(429, None)
                smoke.wait_recovery(4)
                self.assertEqual(now[0], 281)

    def test_smoke_recovery_reuses_original_command_and_stops_owner_actions(self):
        module_spec = importlib.util.spec_from_file_location(
            "ecs_cold_entry_smoke_recovery_test", ROOT / "scripts/package-ecs-cold-entry-smoke.py")
        module = importlib.util.module_from_spec(module_spec)
        module_spec.loader.exec_module(module)
        with tempfile.TemporaryDirectory() as directory:
            now = [0.0]
            argv = ["client", "cold-entry", "run", "--directory", "original-vault"]
            with patch.object(module.time, "monotonic", side_effect=lambda: now[0]), \
                    patch.object(module.time, "sleep", side_effect=lambda seconds: now.__setitem__(0, now[0] + seconds)) as sleep:
                smoke = module.Smoke({}, Path(directory), 200)
                responses = [(0, b'{"stage":"task","state":"unknown"}')] * 31
                responses.append((0, b'{"stage":"complete","state":"confirmed"}'))
                with patch.object(smoke, "command", side_effect=responses) as command:
                    smoke.recover_cold_entry(argv)
                self.assertEqual(smoke.attempts, 32)
                self.assertTrue(all(call.args == (argv,) and call.kwargs == {"private": True}
                                    for call in command.call_args_list))
                self.assertEqual([call.args[0] for call in sleep.call_args_list[:4]], [1, 2, 4, 4])
                for state in ("blocked", "action_required"):
                    smoke = module.Smoke({}, Path(directory), 300)
                    sleep.reset_mock()
                    with patch.object(smoke, "command", return_value=(0, json.dumps({
                            "stage": "login", "state": state}).encode())) as command:
                        with self.assertRaises(RuntimeError):
                            smoke.recover_cold_entry(argv)
                    command.assert_called_once_with(argv, private=True)
                    sleep.assert_not_called()

    def test_completed_restart_requires_confirmed_and_preserves_retry_after(self):
        spec = importlib.util.spec_from_file_location(
            "ecs_cold_entry_restart_test", ROOT / "scripts/package-ecs-cold-entry-smoke.py")
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        with tempfile.TemporaryDirectory() as directory:
            for deadline in (50, 200):
                now = [0.0]
                argv = ["client", "cold-entry", "run", "--directory", "original-vault"]
                with patch.object(module.time, "monotonic", side_effect=lambda: now[0]), \
                        patch.object(module.time, "sleep", side_effect=lambda seconds: now.__setitem__(0, now[0] + seconds)):
                    smoke = module.Smoke({}, Path(directory), deadline)
                    smoke.stage = "completed_workflow_restart"
                    responses = iter([
                        (1, b"private CLI error"),
                        (0, b'{"stage":"complete","state":"unknown"}'),
                        (0, b'{"stage":"complete","state":"confirmed"}'),
                    ])

                    def reply(*args, **kwargs):
                        if smoke.attempts == 1:
                            smoke.observe_retry_after(429, "60")
                        return next(responses)

                    with patch.object(smoke, "command", side_effect=reply) as command:
                        if deadline == 50:
                            with self.assertRaises(RuntimeError):
                                smoke.recover_cold_entry(argv)
                            self.assertEqual(command.call_count, 1)
                        else:
                            smoke.recover_cold_entry(argv)
                            self.assertEqual(command.call_count, 3)
                            self.assertGreaterEqual(now[0], 62)
                            self.assertEqual(smoke.last_result["state"], "confirmed")
                        self.assertTrue(all(call.args == (argv,) and call.kwargs == {"private": True}
                                            for call in command.call_args_list))

    def test_smoke_cleanup_does_not_signal_reaped_process_group_numbers(self):
        import time
        module_spec = importlib.util.spec_from_file_location(
            "ecs_cold_entry_smoke_cleanup_test", ROOT / "scripts/package-ecs-cold-entry-smoke.py")
        smoke_module = importlib.util.module_from_spec(module_spec)
        module_spec.loader.exec_module(smoke_module)
        with tempfile.TemporaryDirectory() as directory:
            smoke = smoke_module.Smoke({}, Path(directory), time.monotonic() + 10)
            result, _ = smoke.command([sys.executable, "-c", "pass"])
            self.assertEqual(result, 0)
            self.assertTrue(all(child.returncode is not None for child in smoke.children))
            with patch.object(smoke_module.os, "killpg") as signal_group:
                smoke.close()
                signal_group.assert_not_called()

    @unittest.skipUnless(shutil.which("openssl"), "actual TLS fixture requires OpenSSL")
    def test_fixture_uses_signed_server_leaf_and_independent_ca(self):
        import time
        module_spec = importlib.util.spec_from_file_location(
            "ecs_cold_entry_smoke_tls_test", ROOT / "scripts/package-ecs-cold-entry-smoke.py")
        smoke_module = importlib.util.module_from_spec(module_spec)
        module_spec.loader.exec_module(smoke_module)
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            smoke = smoke_module.Smoke({}, root, time.monotonic() + 30)
            try:
                smoke_module.generate_tls(smoke)
                self.assertEqual(smoke.env["SSL_CERT_FILE"], str(root / "root.crt"))
                leaf = subprocess.check_output(["openssl", "x509", "-in", str(root / "server.crt"), "-noout", "-text"], text=True)
                self.assertIn("CA:FALSE", leaf)
                self.assertIn("TLS Web Server Authentication", leaf)
                self.assertIn("DNS:localhost", leaf)
                subprocess.run(["openssl", "verify", "-purpose", "sslserver", "-CAfile", str(root / "root.crt"), str(root / "server.crt")], check=True, capture_output=True)
                self.assertNotEqual((root / "root.crt").read_bytes(), (root / "server.crt").read_bytes())
                for name in ("root.key", "server.key"):
                    self.assertEqual((root / name).stat().st_mode & 0o777, 0o600)
            finally:
                smoke.close()

    def test_fixture_tls_proxy_supplies_one_actual_identity_source_ip(self):
        module_spec = importlib.util.spec_from_file_location(
            "ecs_cold_entry_smoke_headers_test", ROOT / "scripts/package-ecs-cold-entry-smoke.py")
        smoke = importlib.util.module_from_spec(module_spec)
        module_spec.loader.exec_module(smoke)
        from email.message import Message
        incoming = Message()
        for key, value in (("X-Real-IP", "203.0.113.9"), ("x-real-ip", "198.51.100.9"),
                           ("x-forwarded-proto", "http"), ("Host", "localhost:12345"),
                           ("Origin", "https://localhost:12345"), ("Cookie", "fixture-cookie")):
            incoming[key] = value
        headers = smoke.trusted_forward_headers(incoming, "127.0.0.1")
        self.assertEqual([(key, value) for key, value in headers.items() if key.lower() == "x-real-ip"],
                         [("X-Real-IP", "127.0.0.1")])
        self.assertEqual([(key, value) for key, value in headers.items() if key.lower() == "x-forwarded-proto"],
                         [("X-Forwarded-Proto", "https")])
        self.assertEqual(headers["Origin"], "https://localhost:12345")
        self.assertEqual(headers["Host"], "localhost:12345")
        self.assertEqual(headers["Cookie"], "fixture-cookie")
        with self.assertRaises(ValueError):
            smoke.trusted_forward_headers(incoming, "invalid-ip")

    def test_fixture_private_goal_storage_is_private_independent_and_exclusive(self):
        module_spec = importlib.util.spec_from_file_location(
            "ecs_cold_entry_fixture_goal_storage_test", ROOT / "scripts/package-ecs-cold-entry-fixture.py")
        fixture = importlib.util.module_from_spec(module_spec)
        module_spec.loader.exec_module(fixture)
        with tempfile.TemporaryDirectory() as directory:
            material = []
            for name in ("fixture-a", "fixture-b"):
                root = Path(directory) / name
                root.mkdir(mode=0o700)
                public_root = root / "preview"
                public_root.mkdir(mode=0o700)
                environment = fixture.private_goal_storage_environment(root)
                self.assertEqual(set(environment), {"OASIS7_PRIVATE_GOAL_DIR", "OASIS7_PRIVATE_GOAL_KEY_FILE"})
                store = Path(environment["OASIS7_PRIVATE_GOAL_DIR"])
                key = Path(environment["OASIS7_PRIVATE_GOAL_KEY_FILE"])
                self.assertTrue(store.is_dir())
                self.assertTrue(key.is_file())
                self.assertFalse(store.is_symlink())
                self.assertFalse(key.is_symlink())
                self.assertEqual(store.stat().st_mode & 0o777, 0o700)
                self.assertEqual(key.stat().st_mode & 0o777, 0o600)
                self.assertEqual(len(key.read_bytes()), 32)
                for path in (store, key):
                    self.assertEqual(path.parent.resolve(), root.resolve())
                    self.assertNotIn(public_root.resolve(), path.resolve().parents)
                original = key.read_bytes()
                with self.assertRaises(FileExistsError):
                    fixture.private_goal_storage_environment(root)
                self.assertTrue(key.read_bytes() == original)
                material.append(original)
            self.assertTrue(material[0] != material[1])

    def test_fixture_identity_http_does_not_target_runtime_websocket(self):
        module_spec = importlib.util.spec_from_file_location(
            "ecs_cold_entry_fixture_ports_test", ROOT / "scripts/package-ecs-cold-entry-fixture.py")
        fixture = importlib.util.module_from_spec(module_spec)
        module_spec.loader.exec_module(fixture)
        args = fixture.launcher_transport_args(4173, 5023, 5011)
        options = dict(zip(args[::2], args[1::2]))
        self.assertEqual(options["--viewer-port"], "4173")
        self.assertEqual(options["--viewer-host"], "127.0.0.1")
        self.assertEqual(options["--web-bind"], "127.0.0.1:5011")
        self.assertEqual(options["--live-bind"], "127.0.0.1:5023")
        for ports in ((4173, 5023, 4173), (0, 5023, 5011), (4173, 5023, 65536)):
            with self.assertRaises(ValueError):
                fixture.launcher_transport_args(*ports)

    def test_actual_fixture_isolates_required_hosted_issuer_and_ledgers(self):
        module_spec = importlib.util.spec_from_file_location(
            "ecs_cold_entry_fixture_test", ROOT / "scripts/package-ecs-cold-entry-fixture.py")
        fixture = importlib.util.module_from_spec(module_spec)
        module_spec.loader.exec_module(fixture)
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            environment = fixture.hosted_startup_isolation(root)
            key = root / "hosted-registration-issuer.key"
            self.assertEqual(len(key.read_bytes()), 32)
            self.assertEqual(key.stat().st_mode & 0o777, 0o600)
            self.assertEqual(environment["OASIS7_HOSTED_REGISTRATION_ISSUER_PRIVATE_KEY"], key.read_bytes().hex())
            paths = [Path(environment[name]) for name in (
                "OASIS7_HOSTED_REGISTRATION_REPLAY_LEDGER_PATH", "OASIS7_HOSTED_PLAYER_SESSION_LEDGER_PATH")]
            self.assertEqual(len(set(paths)), 2)
            self.assertTrue(all(path.parent == root and not path.exists() for path in paths))
            self.assertNotIn("OASIS7_HOSTED_ACCOUNT_STORE_PATH", environment)
            with self.assertRaises(FileExistsError):
                fixture.hosted_startup_isolation(root)
            self.assertEqual(environment["OASIS7_HOSTED_REGISTRATION_ISSUER_PRIVATE_KEY"], key.read_bytes().hex())

    def test_cold_entry_packaging_tools_parse_and_expose_bounded_help(self):
        for name in ("package-ecs-cold-entry-smoke.py", "package-ecs-cold-entry-fixture.py"):
            path = ROOT / "scripts" / name
            compile(path.read_bytes(), str(path), "exec")
        result = subprocess.run(
            [sys.executable, str(ROOT / "scripts/package-ecs-cold-entry-smoke.py"), "--help"],
            text=True, capture_output=True, timeout=10,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        for option in ("--bundle", "--probe-binary", "--evidence-directory", "--timeout-seconds"):
            self.assertIn(option, result.stdout)

    def test_actual_cold_entry_gates_bundle_and_probe_stays_out_of_delivery(self):
        workflow = (ROOT / ".github/workflows/ecs-packages.yml").read_text()
        native = workflow.split("  native:\n", 1)[1].split("  native-trusted:\n", 1)[0]
        probe = native.split("      - name: Compile actual cold entry integration probe\n", 1)[1].split("      - name:", 1)[0]
        self.assertIn("cargo build --locked --profile ecs -p oasis7_game_client --example cold_entry_probe", probe)
        self.assertNotIn("continue-on-error", probe)
        self.assertIn("ref: ${{ github.sha }}", native)
        self.assertIn("name: ecs-cold-entry-probe-stage", native)
        self.assertNotIn("cold_entry_probe", ecs.BINS)
        package = workflow.split("  package:\n", 1)[1]
        assembled = package.index("python3 scripts/package-ecs.py assemble")
        downloaded = package.index("name: ecs-cold-entry-probe-stage")
        verified = package.index("name: Verify assembled bundle through actual cold entry services")
        published = package.index("name: oasis7-ecs-linux-x64")
        self.assertLess(assembled, downloaded)
        self.assertLess(downloaded, verified)
        self.assertLess(verified, published)
        smoke = package[verified:].split("      - name: Upload public cold entry verification receipt", 1)[0]
        self.assertIn('scripts/package-ecs-cold-entry-smoke.py', smoke)
        self.assertIn('--bundle "$GITHUB_WORKSPACE/output/ecs"', smoke)
        self.assertIn('--probe-binary "$GITHUB_WORKSPACE/output/cold-entry-probe/cold_entry_probe"', smoke)
        self.assertNotIn("continue-on-error", smoke)
        self.assertNotIn("if:", smoke)
        self.assertNotIn("|| true", smoke)
        self.assertIn("ref: ${{ github.sha }}", package)

    def test_bundle_delivers_cold_entry_client_with_the_server_revision(self):
        self.assertIn("oasis7_game_client", ecs.BINS)

    def test_cli_helper_imports_do_not_add_cache_source_inputs(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            scripts = root / "scripts"
            scripts.mkdir()
            for name in ("package-ecs.py", "package-ecs-skills.py"):
                shutil.copyfile(ROOT / "scripts" / name, scripts / name)
            env = dict(os.environ)
            env.pop("PYTHONDONTWRITEBYTECODE", None)
            result = subprocess.run([sys.executable, str(scripts / "package-ecs.py"), "--help"],
                                    cwd=root, env=env, text=True, capture_output=True)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(set(path.name for path in scripts.iterdir()),
                             {"package-ecs.py", "package-ecs-skills.py"})

    @unittest.skipUnless(sys.platform == "linux" and Path("/usr/bin/time").is_file(), "Linux ECS resource collection")
    def test_compile_step_preserves_build_status_and_stops_monitor(self):
        workflow = (ROOT / ".github/workflows/ecs-packages.yml").read_text()
        step = workflow.split("      - name: Compile ECS service and game client binaries\n", 1)[1].split("      - name:", 1)[0]
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
                self.assertEqual([command[i + 1] for i, arg in enumerate(command) if arg == "-p"], ["oasis7_chain_runtime", "oasis7_launcher_backend", "oasis7_viewer_runtime", "oasis7_game_client"])
                self.assertNotIn("RUSTC_WRAPPER", cargo.call_args.kwargs["env"])
                self.assertEqual(command[command.index("--profile") + 1], "ecs")
                self.assertIn("--timings", command)
            original = (stage / "native.json").read_text()
            self.assertEqual(json.loads(original)["profile"], "ecs")

            def assemble(output):
                with patch.object(ecs.skills, "source_files", return_value={name: (ROOT / "skills/oasis7-play" / name).read_bytes() for name in ("SKILL.md", "references/identity-entry.md")}), patch.object(subprocess, "check_output", return_value="a" * 40), patch.object(sys, "argv", ["ecs", "assemble", "--native", str(stage), "--web", str(web), "--out", str(output)]), patch.object(subprocess, "run", side_effect=AssertionError("assembly must not compile")):
                    ecs.main()

            out = root / "bundle"
            assemble(out)
            self.assertEqual(set(path.name for path in (out / "bin").iterdir()), set(ecs.BINS))
            self.assertIn("profile=ecs\n", (out / "BUILDINFO").read_text())
            self.assertTrue((out / "web-dist/viewer-optional-payload/payload.wasm").is_file())
            skill = out / "web-dist/skills/oasis7-play"
            self.assertEqual((skill / "SKILL.md").read_bytes(), (ROOT / "skills/oasis7-play/SKILL.md").read_bytes())
            self.assertEqual(json.loads((skill / "manifest.json").read_text())["source"], "a" * 40)
            self.assertTrue((skill / "references/identity-entry.md").is_file())
            self.assertTrue((out / "deployment/ecs-skill-release.py").is_file())
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

    def test_workflow_parallel_join_and_trust(self):
        workflow = ROOT / ".github/workflows/ecs-packages.yml"
        # Use the same YAML parser as the repository workflow contracts.
        text = workflow.read_text()
        self.assertNotIn("ref_or_sha", text)
        self.assertNotIn("secrets.", text)
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
            self.assertNotIn("needs:", section)
        self.assertEqual(text.count("ref: ${{ github.sha }}"), 4)



class SkillReleaseTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name).resolve()
        self.source = "a" * 40
        self.files = {"SKILL.md": b"# Game Skill\n[Identity](references/identity-entry.md)\n",
                      "references/identity-entry.md": b"# Identity\n"}
        self.release = self.root / "release"
        self.web = self.release / "web-dist"
        self.web.mkdir(parents=True)
        with patch.object(ecs.skills, "source_files", return_value=self.files):
            ecs.skills.package(ROOT, self.source, self.web)
        (self.release / "manifest.json").write_text(json.dumps({"source": self.source, "profile": "ecs", "platform": "linux-x64"}))
        (self.release / "bin").mkdir()
        (self.release / "bin/oasis7_game_launcher").write_bytes(b"launcher")
        self.seal()

    def seal(self):
        (self.release / "SHA256SUMS").write_text("".join(
            f"{ecs.digest(path)}  {path.relative_to(self.release).as_posix()}\n"
            for path in sorted(self.release.rglob("*")) if path.is_file() and path.name != "SHA256SUMS"))

    def checked(self):
        return ecs.skills.checked_release(self.release, self.source)[1]

    def test_committed_source_excludes_dirty_and_untracked_files(self):
        repo = self.root / "repo"
        repo.mkdir()
        subprocess.run(["git", "init", "-q", str(repo)], check=True)
        for name, data in self.files.items():
            path = repo / "skills/oasis7-play" / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(data)
        subprocess.run(["git", "add", "."], cwd=repo, check=True)
        subprocess.run(["git", "-c", "user.name=Test", "-c", "user.email=test@example.invalid", "-c", "commit.gpgsign=false", "commit", "-qm", "Fixture"], cwd=repo, check=True)
        source = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=repo, text=True).strip()
        (repo / "skills/oasis7-play/SKILL.md").write_bytes(b"dirty")
        (repo / "skills/oasis7-play/private.md").write_bytes(b"private fixture")
        self.assertEqual(ecs.skills.source_files(repo, source), self.files)
        (repo / "skills/oasis7-play/SKILL.md").write_bytes(self.files["SKILL.md"])
        (repo / "skills/oasis7-play/references/identity-entry.md").unlink()
        subprocess.run(["git", "add", "-u"], cwd=repo, check=True)
        subprocess.run(["git", "-c", "user.name=Test", "-c", "user.email=test@example.invalid", "-c", "commit.gpgsign=false", "commit", "-qm", "Missing reference"], cwd=repo, check=True)
        with self.assertRaisesRegex(ValueError, "unpublished Skill reference"):
            ecs.skills.source_files(repo, "HEAD")

    def test_version_tamper_symlink_and_unsealed_index_reject(self):
        self.checked()
        with self.assertRaisesRegex(ValueError, "identity mismatch"):
            ecs.skills.checked_release(self.release, "b" * 40)
        path = self.web / "skills/oasis7-play/references/identity-entry.md"
        path.write_bytes(b"tampered")
        with self.assertRaisesRegex(ValueError, "file mismatch"):
            self.checked()
        path.write_bytes(self.files["references/identity-entry.md"])
        path.unlink()
        source = self.root / "ref.md"
        source.write_bytes(self.files["references/identity-entry.md"])
        path.symlink_to(source)
        with self.assertRaisesRegex(ValueError, "symbolic"):
            self.checked()
        path.unlink()
        path.write_bytes(source.read_bytes())
        (self.web / "skills/index.html").write_bytes(b"wrong version index")
        with self.assertRaisesRegex(ValueError, "bundle checksums"):
            self.checked()

    @unittest.skipUnless(hasattr(os, "mkfifo"), "POSIX FIFO rejection")
    def test_non_regular_payload_is_rejected_before_reading(self):
        path = self.web / "skills/oasis7-play/references/identity-entry.md"
        path.unlink()
        os.mkfifo(path)
        with self.assertRaisesRegex(ValueError, "non-regular"):
            self.checked()

    def test_mutable_publication_files_and_ancestors_are_rejected(self):
        protected = Path("/opt/preview/releases/v1/web-dist/skills/SKILL.md")
        def stat_for(bad_path=None, owner=0, mode=0o100644):
            def stat(path):
                return argparse.Namespace(st_uid=owner if path == bad_path else 0,
                                          st_mode=mode if path == bad_path else 0o100755)
            return stat
        for target in (protected, protected.parent, Path("/opt")):
            for owner, mode in ((1000, 0o100644), (0, 0o100664), (0, 0o100646)):
                with self.subTest(target=target, owner=owner, mode=mode), patch.object(Path, "lstat", stat_for(target, owner, mode)), patch.object(Path, "is_symlink", return_value=False):
                    with self.assertRaisesRegex(ValueError, "root-owned"):
                        ecs.skills.require_root_owned([protected])
        with patch.object(Path, "lstat", stat_for()), patch.object(Path, "is_symlink", return_value=False):
            ecs.skills.require_root_owned([protected])

    def test_activation_rejects_mutable_paths_before_any_service_call(self):
        include = self.root / "skills.conf"
        include.write_bytes(b"old")
        args = argparse.Namespace(release=self.release, origin="https://game.invalid", player_service="fixture",
                                  nginx_include=include, lock_file=self.root / "deploy.lock")
        with patch.object(ecs.skills, "require_root_owned", side_effect=ValueError("root-owned path required")), patch.object(subprocess, "check_output") as command:
            with self.assertRaisesRegex(ValueError, "root-owned"):
                ecs.skills.activate(args, self.checked())
            command.assert_not_called()
        self.assertEqual(include.read_bytes(), b"old")

    def test_tampered_launcher_cannot_bind_another_runtime_version(self):
        (self.release / "bin/oasis7_game_launcher").write_bytes(b"old running launcher")
        with self.assertRaisesRegex(ValueError, "bundle checksums"):
            self.checked()

    def test_exact_nginx_routes_preserve_api_boundary(self):
        config = ecs.skills.nginx_config(self.checked()).decode()
        self.assertIn("location = /skills/oasis7-play/SKILL.md", config)
        self.assertIn("location = /skills/references/identity-entry.md", config)
        self.assertIn("return 302 /skills/index.html", config)
        self.assertNotIn("location ~", config)
        self.assertNotIn("proxy_pass", config)
        self.assertNotIn("/api/", config)
        self.assertIn("expires -1", config)
        self.assertNotIn("add_header", config)  # preserve server security headers

    def test_public_bytes_must_match_and_redirects_are_rejected(self):
        public = self.checked()
        class Response(io.BytesIO):
            status = 200
        def fetch(url, timeout):
            response = Response(public[url.removeprefix("https://game.invalid")].read_bytes())
            response.url = url
            return response
        opener = unittest.mock.Mock()
        opener.open.side_effect = fetch
        with patch("urllib.request.build_opener", return_value=opener):
            ecs.skills.verify(public, "https://game.invalid")
            def bad_fetch(url, timeout):
                response = Response(b"wrong")
                response.url = url
                return response
            opener.open.side_effect = bad_fetch
            with self.assertRaises(ValueError):
                ecs.skills.verify(public, "https://game.invalid")
        self.assertIsNone(ecs.skills.NoRedirect().redirect_request(None, None, 302, None, None, "http://other.invalid"))
        for origin in ("http://game.invalid", "https://user:secret@game.invalid", "https://game.invalid/path"):
            with self.assertRaises(ValueError):
                ecs.skills.verify(public, origin)

    @unittest.skipUnless(os.name == "posix", "ECS deployment requires flock and /proc")
    def test_failed_publication_restores_previous_nginx_include(self):
        include = self.root / "skills.conf"
        include.write_bytes(b"# previous version\n")
        args = argparse.Namespace(release=self.release, origin="https://game.invalid", player_service="fixture",
                                  nginx_include=include, lock_file=self.root / "deploy.lock")
        public = self.checked()
        real_read = Path.read_bytes
        def read(path):
            return b"launcher" if str(path) == "/proc/123/exe" else real_read(path)
        def output(command, **kwargs):
            return "123\n" if command[0] == "systemctl" else f"include {include};\n"
        with patch.object(ecs.skills, "require_root_owned"), patch.object(Path, "read_bytes", read), patch.object(subprocess, "check_output", side_effect=output), patch.object(subprocess, "run") as run, patch.object(ecs.skills, "verify", side_effect=ValueError("public mismatch")):
            with self.assertRaisesRegex(ValueError, "public mismatch"):
                ecs.skills.activate(args, public)
        self.assertEqual(include.read_bytes(), b"# previous version\n")
        self.assertEqual(include.with_name("skills.conf.previous").read_bytes(), include.read_bytes())
        self.assertEqual([call.args[0] for call in run.call_args_list],
                         [["nginx", "-t"], ["systemctl", "reload", "nginx"], ["nginx", "-t"], ["systemctl", "reload", "nginx"]])

    @unittest.skipUnless(os.name == "posix", "ECS deployment requires flock and /proc")
    def test_wrong_running_version_does_not_touch_nginx(self):
        include = self.root / "skills.conf"
        include.write_bytes(b"old")
        args = argparse.Namespace(release=self.release, origin="https://game.invalid", player_service="fixture",
                                  nginx_include=include, lock_file=self.root / "deploy.lock")
        public = self.checked()
        real_read = Path.read_bytes
        def read(path):
            return b"different launcher" if str(path) == "/proc/123/exe" else real_read(path)
        with patch.object(ecs.skills, "require_root_owned"), patch.object(Path, "read_bytes", read), patch.object(subprocess, "check_output", return_value="123\n"), patch.object(subprocess, "run") as run:
            with self.assertRaisesRegex(ValueError, "running launcher"):
                ecs.skills.activate(args, public)
            run.assert_not_called()
        self.assertEqual(include.read_bytes(), b"old")


if __name__ == "__main__":
    unittest.main()
