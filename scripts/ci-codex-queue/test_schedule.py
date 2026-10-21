#!/usr/bin/env python3
"""Scheduler lifecycle tests; never install a real user service."""

import contextlib
import importlib.util
import io
import json
import os
from pathlib import Path
import plistlib
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest import mock


MODULE_SPEC = importlib.util.spec_from_file_location("queue_schedule", Path(__file__).with_name("schedule.py"))
schedule = importlib.util.module_from_spec(MODULE_SPEC)
sys.modules[MODULE_SPEC.name] = schedule
MODULE_SPEC.loader.exec_module(schedule)


class FakeManager:
    def __init__(self, spec):
        self.spec = spec
        self.calls = []
        self.loaded = False
        self.enabled = False
        self.active = False
        self.fail_action = None
        self.bad_readback = False
        self.stop_observations = []

    def which(self, name, *, path):
        return f"/fake/bin/{name}"

    def __call__(self, command, **kwargs):
        self.calls.append(command)
        self.stop_observations.append(self.spec.stop_file.exists())
        if kwargs != {"check": False, "capture_output": True, "text": True, "timeout": 30}:
            raise AssertionError("service manager must use bounded argument-vector execution")
        args = command[1:]
        if args[0] == "--user":
            args = args[1:]
        action = args[0]
        if action == self.fail_action:
            return subprocess.CompletedProcess(command, 1, "", "injected failure")
        out = ""
        if self.spec.platform == "macos":
            if action == "print":
                if not self.loaded:
                    return subprocess.CompletedProcess(command, 113, "", "Could not find service")
                out = "state = not running\n"
            elif action == "print-disabled":
                disabled = "false" if self.enabled else "true"
                if self.bad_readback:
                    disabled = "true"
                out = f'{{\n "{schedule.LABEL}" => {disabled}\n}}\n'
            elif action == "bootstrap":
                self.loaded = True
            elif action == "bootout":
                self.loaded = False
            elif action == "enable":
                self.enabled = True
            elif action == "disable":
                self.enabled = False
        else:
            if action == "show":
                suffix = args[1].rsplit(".", 1)[-1]
                exists = (self.spec.unit_dir / args[1]).exists()
                load = "loaded" if exists or self.loaded else "not-found"
                active = "active" if suffix == "timer" and self.active else "inactive"
                if self.bad_readback:
                    active = "inactive" if self.active else "active"
                enabled = "enabled" if self.enabled else "disabled"
                fragment = str(self.spec.unit_dir / args[1]) if load == "loaded" else ""
                out = f"LoadState={load}\nActiveState={active}\nUnitFileState={enabled}\nFragmentPath={fragment}\nDropInPaths=\n"
            elif action == "enable":
                self.active = self.enabled = self.loaded = True
            elif action == "disable":
                self.active = self.enabled = False
        return subprocess.CompletedProcess(command, 0, out, "")


class ScheduleTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='oasis7 schedule $ % " ')
        self.root = Path(self.temp.name)
        self.config = self.root / "private config.json"
        self.state = self.root / "private state"
        self.repo = self.root / "source checkout"
        self.dispatcher = self.repo / "scripts/ci-codex-queue/dispatcher.py"
        self.dispatcher.parent.mkdir(parents=True)
        self.dispatcher.write_text("raise RuntimeError('must never run during these tests')\n")
        self.config.write_text(json.dumps({"state_dir": str(self.state), "repository": "nil-studio/oasis7",
                                          "checkout": str(self.repo), "trusted_users": ["maintainer"]}))
        self.config.chmod(0o600)
        self.env = mock.patch.dict(os.environ, {"XDG_CONFIG_HOME": str(self.root / "xdg config"),
                                               "GH_TOKEN": "must-not-be-copied",
                                               "OPENAI_API_KEY": "must-not-be-copied"})
        self.env.start()

    def tearDown(self):
        self.env.stop()
        self.temp.cleanup()

    def spec(self, platform="linux"):
        return schedule.Schedule.load(str(self.config), platform, repo=self.repo, home=self.root,
                                      uid=1000, runtime_path=str(self.root / "tools $ % & bin") + ":/usr/bin:/bin")

    def install(self, spec, fake):
        with mock.patch.object(schedule, "current_platform", return_value=spec.platform):
            return schedule.install(spec, run=fake, which=fake.which)

    def stop(self, spec, fake, uninstall=False):
        with mock.patch.object(schedule, "current_platform", return_value=spec.platform):
            return schedule.stop(spec, run=fake, which=fake.which, uninstall=uninstall)

    def test_launchd_exact_argv_special_paths_and_detached_worker(self):
        spec = self.spec("macos")
        content = spec.render()[f"{schedule.LABEL}.plist"]
        plist = plistlib.loads(content)
        self.assertEqual(plist["ProgramArguments"], [str(spec.python), str(self.dispatcher),
                                                   "--config", str(self.config), "tick"])
        self.assertEqual(plist["StartInterval"], 300)
        self.assertFalse(plist["RunAtLoad"])
        self.assertTrue(plist["AbandonProcessGroup"])
        self.assertEqual(plist["EnvironmentVariables"], {"PATH": spec.runtime_path})
        self.assertNotIn(b"must-not-be-copied", content)

    def test_systemd_command_disables_expansion_and_preserves_worker(self):
        files = self.spec().render()
        service = files[f"{schedule.LABEL}.service"].decode()
        timer = files[f"{schedule.LABEL}.timer"].decode()
        self.assertIn("ExecStart=:", service)
        self.assertIn('"--config"', service)
        self.assertIn('"tick"', service)
        self.assertIn("%%", service)
        self.assertIn('\\"', service)
        self.assertIn("KillMode=process", service)
        self.assertIn("OnActiveSec=300s", timer)
        self.assertIn("OnUnitActiveSec=300s", timer)
        self.assertNotIn("Persistent=true", timer)
        self.assertNotIn("must-not-be-copied", service)

    @unittest.skipUnless(shutil.which("systemd-analyze"), "optional actual systemd unit parser unavailable")
    def test_rendered_linux_units_pass_actual_systemd_parser_without_activation(self):
        paths = []
        for name, content in self.spec().render().items():
            path = self.root / name
            path.write_bytes(content)
            paths.append(str(path))
        result = subprocess.run([shutil.which("systemd-analyze"), "verify", *paths],
                                capture_output=True, text=True, timeout=15)
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_render_does_not_activate_create_state_or_clear_stop(self):
        spec = self.spec("macos")
        output = self.root / "rendered"
        with mock.patch.object(schedule.Schedule, "load", return_value=spec), \
                mock.patch.object(schedule, "install", side_effect=AssertionError("render installed")), \
                contextlib.redirect_stdout(io.StringIO()) as stdout:
            result = schedule.main(["--config", str(self.config), "render", "--output-dir", str(output)])
        self.assertEqual(result, 0)
        self.assertFalse(self.state.exists())
        self.assertFalse(json.loads(stdout.getvalue())["activated"])
        schedule.pause(spec)
        with mock.patch.object(schedule.Schedule, "load", return_value=spec), contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(schedule.main(["--config", str(self.config), "render", "--output-dir", str(output)]), 0)
        self.assertTrue(spec.stop_file.exists())

    def test_linux_install_reads_back_before_clearing_stop(self):
        spec = self.spec()
        fake = FakeManager(spec)
        result = self.install(spec, fake)
        self.assertEqual(result["state"], "scheduled")
        self.assertTrue(all(fake.stop_observations))
        self.assertFalse(spec.stop_file.exists())
        self.assertTrue(any("enable" in call and "--now" in call for call in fake.calls))
        self.assertTrue(all(call[1] == "--user" for call in fake.calls))
        self.assertFalse(any("codex" in Path(call[0]).name for call in fake.calls))
        self.assertEqual(spec.record_file.stat().st_mode & 0o777, 0o600)

    def test_launchd_install_verifies_enabled_registration_without_kickstart(self):
        spec = self.spec("macos")
        fake = FakeManager(spec)
        result = self.install(spec, fake)
        self.assertEqual(result["state"], "scheduled")
        self.assertTrue(any("bootstrap" in call for call in fake.calls))
        self.assertTrue(any("print-disabled" in call for call in fake.calls))
        self.assertFalse(any("kickstart" in call for call in fake.calls))
        self.assertTrue(all(fake.stop_observations))
        self.assertFalse(spec.stop_file.exists())

    def test_failed_install_does_not_silently_resume(self):
        for platform in ("linux", "macos"):
            with self.subTest(platform=platform):
                spec = self.spec(platform)
                fake = FakeManager(spec)
                fake.fail_action = "enable"
                with self.assertRaises(schedule.ScheduleError):
                    self.install(spec, fake)
                self.assertTrue(spec.stop_file.exists())

    def test_successful_manager_command_without_verified_state_is_failure(self):
        for platform in ("linux", "macos"):
            with self.subTest(platform=platform):
                spec = self.spec(platform)
                fake = FakeManager(spec)
                fake.bad_readback = True
                with self.assertRaises(schedule.ScheduleError):
                    self.install(spec, fake)
                self.assertTrue(spec.stop_file.exists())

    def test_stop_and_uninstall_preserve_config_database_and_worker_request(self):
        for platform in ("linux", "macos"):
            with self.subTest(platform=platform):
                spec = self.spec(platform)
                fake = FakeManager(spec)
                self.install(spec, fake)
                database = self.state / "queue.sqlite3"
                database.write_bytes(b"durable task ownership")
                result = self.stop(spec, fake)
                self.assertEqual(result["state"], "paused")
                self.assertTrue(spec.stop_file.exists())
                self.assertTrue(spec.record_file.exists())
                result = self.stop(spec, fake, uninstall=True)
                self.assertEqual(result["state"], "uninstalled")
                self.assertEqual(database.read_bytes(), b"durable task ownership")
                self.assertTrue(self.config.exists())
                self.assertTrue(spec.stop_file.exists())
                self.assertFalse(spec.record_file.exists())

    def test_second_config_cannot_replace_the_single_scheduler(self):
        spec = self.spec()
        fake = FakeManager(spec)
        self.install(spec, fake)
        other = self.root / "other config.json"
        other.write_text(json.dumps({"state_dir": str(self.root / "other state")}))
        other_spec = schedule.Schedule.load(str(other), "linux", repo=self.repo, home=self.root,
                                            uid=1000, runtime_path=spec.runtime_path)
        count = len(fake.calls)
        with self.assertRaisesRegex(schedule.ScheduleError, "different configuration"):
            self.install(other_spec, fake)
        self.assertEqual(len(fake.calls), count)
        self.assertFalse(spec.stop_file.exists())

    def test_concurrent_install_or_stop_cannot_race_activation(self):
        spec = self.spec()
        fake = FakeManager(spec)
        with schedule.operation_lock(spec):
            with self.assertRaisesRegex(schedule.ScheduleError, "in progress"):
                self.install(spec, fake)
            with self.assertRaisesRegex(schedule.ScheduleError, "in progress"):
                self.stop(spec, fake)
        self.assertFalse(fake.calls)
        self.assertFalse(spec.stop_file.exists())

    def test_uninstall_preserves_modified_files(self):
        spec = self.spec()
        fake = FakeManager(spec)
        self.install(spec, fake)
        service = spec.unit_dir / f"{schedule.LABEL}.service"
        service.write_text("user-edited service\n")
        with self.assertRaisesRegex(schedule.ScheduleError, "modified"):
            self.stop(spec, fake, uninstall=True)
        self.assertEqual(service.read_text(), "user-edited service\n")
        self.assertTrue(spec.stop_file.exists())

    def test_reinstall_preserves_modified_files_and_existing_manager_state(self):
        spec = self.spec()
        fake = FakeManager(spec)
        self.install(spec, fake)
        service = spec.unit_dir / f"{schedule.LABEL}.service"
        service.write_text("user-edited service\n")
        original_record = spec.record_file.read_bytes()
        count = len(fake.calls)
        with self.assertRaisesRegex(schedule.ScheduleError, "modified"):
            self.install(spec, fake)
        self.assertEqual(service.read_text(), "user-edited service\n")
        self.assertEqual(spec.record_file.read_bytes(), original_record)
        self.assertEqual(len(fake.calls), count)

    def test_unowned_units_are_not_overwritten(self):
        spec = self.spec()
        spec.unit_dir.mkdir(parents=True)
        path = spec.unit_dir / f"{schedule.LABEL}.timer"
        path.write_text("unowned\n")
        fake = FakeManager(spec)
        with self.assertRaisesRegex(schedule.ScheduleError, "no ownership record"):
            self.install(spec, fake)
        self.assertEqual(path.read_text(), "unowned\n")
        self.assertFalse(fake.calls)

    def test_invalid_path_and_environment_inputs_are_rejected(self):
        for path in ("relative", "/bin::/usr/bin", "/bin\nExecStart=/evil"):
            with self.subTest(path=path), self.assertRaises(schedule.ScheduleError):
                schedule.Schedule.load(str(self.config), "linux", runtime_path=path)
        with self.assertRaises(schedule.ScheduleError):
            schedule.Schedule.load("relative.json", "linux")

    def test_missing_tool_fails_before_activating(self):
        spec = self.spec()
        fake = FakeManager(spec)
        def missing_codex(name, *, path):
            return None if name == "codex" else fake.which(name, path=path)
        with mock.patch.object(schedule, "current_platform", return_value="linux"), \
                self.assertRaisesRegex(schedule.ScheduleError, "codex"):
            schedule.install(spec, run=fake, which=missing_codex)
        self.assertFalse(fake.calls)
        self.assertFalse(spec.record_file.exists())

    def test_install_checks_configured_executables_without_invoking_them(self):
        codex = self.root / "absolute custom codex"
        codex.write_text("must never execute\n")
        codex.chmod(0o700)
        data = json.loads(self.config.read_text())
        data.update(codex_command=[str(codex), "app-server"], gh_command=["custom-gh"])
        self.config.write_text(json.dumps(data))
        spec = self.spec()
        fake = FakeManager(spec)
        lookups = []
        def configured_only(name, *, path):
            lookups.append(name)
            return fake.which(name, path=path) if name in ("custom-gh", "systemctl") else None
        with mock.patch.object(schedule, "current_platform", return_value="linux"):
            self.assertEqual(schedule.install(spec, run=fake, which=configured_only)["state"], "scheduled")
        self.assertEqual(lookups, ["custom-gh", "systemctl"])
        self.assertTrue(all("systemctl" in call[0] for call in fake.calls))

    def test_missing_absolute_configured_executable_fails_before_activation(self):
        data = json.loads(self.config.read_text())
        data["codex_command"] = [str(self.root / "missing codex"), "app-server"]
        self.config.write_text(json.dumps(data))
        spec = self.spec()
        fake = FakeManager(spec)
        with self.assertRaisesRegex(schedule.ScheduleError, "codex_command"):
            self.install(spec, fake)
        self.assertFalse(fake.calls)
        self.assertFalse(spec.record_file.exists())


if __name__ == "__main__":
    unittest.main()
