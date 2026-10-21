#!/usr/bin/env python3
"""Render or explicitly install the per-user, five-minute queue trigger.

Only PATH is copied into service definitions. gh and Codex retain their own
local authentication. STOP is shared with dispatcher/worker; service-manager
readback proves scheduling state, never task completion or worker termination.
"""

from __future__ import annotations

import argparse
from contextlib import contextmanager
from dataclasses import dataclass
import hashlib
import json
import os
from pathlib import Path
import plistlib
import re
import shlex
import shutil
import subprocess
import sys
import tempfile
from typing import Callable


LABEL = "tech.oasis7.codex-issue-queue"
INTERVAL_SECONDS = 300
RECORD_NAME = f".{LABEL}.json"


class ScheduleError(ValueError):
    """An unverified or conflicting installation must remain paused."""


def safe_text(value: str, name: str) -> str:
    if not isinstance(value, str) or not value or any(ord(c) < 32 or ord(c) == 127 for c in value):
        raise ScheduleError(f"{name} must be nonempty and contain no control characters")
    return value


def absolute(value: str | Path, name: str) -> Path:
    result = Path(safe_text(str(value), name)).expanduser()
    if not result.is_absolute():
        raise ScheduleError(f"{name} must be an absolute path")
    return result.resolve()


def systemd_word(value: str) -> str:
    """Quote one systemd word, with percent-specifier expansion disabled."""
    safe_text(value, "unit value")
    return '"' + value.replace("%", "%%").replace("\\", "\\\\").replace('"', '\\"') + '"'


def current_platform() -> str:
    if sys.platform == "darwin":
        return "macos"
    if sys.platform.startswith("linux"):
        return "linux"
    raise ScheduleError("Only macOS launchd and Linux systemd user services are supported")


@dataclass(frozen=True)
class Schedule:
    platform: str
    config: Path
    state: Path
    repo: Path
    python: Path
    runtime_path: str
    unit_dir: Path
    uid: int
    executables: tuple[tuple[str, str], ...]

    @classmethod
    def load(cls, config: str, platform: str | None = None, *,
             python: str | None = None, runtime_path: str | None = None,
             home: Path | None = None, repo: Path | None = None,
             uid: int | None = None) -> "Schedule":
        config_path = absolute(config, "config")
        try:
            data = json.loads(config_path.read_text(encoding="utf-8"))
        except (OSError, ValueError) as exc:
            raise ScheduleError("Cannot read the private JSON config") from exc
        if not isinstance(data, dict) or not isinstance(data.get("state_dir"), str):
            raise ScheduleError("config.state_dir must be an absolute path")
        state = absolute(data["state_dir"], "config.state_dir")
        executables = []
        for name, default in (("gh_command", ["gh"]), ("codex_command", ["codex", "app-server"])):
            command = data.get(name, default)
            if (not isinstance(command, list) or not command
                    or any(not isinstance(word, str) or not word or "\x00" in word for word in command)):
                raise ScheduleError(f"config.{name} must be a nonempty argument array")
            executable = safe_text(command[0], f"config.{name} executable")
            if not Path(executable).is_absolute() and Path(executable).name != executable:
                raise ScheduleError(f"config.{name} executable must be absolute or a name resolved through PATH")
            executables.append((name, executable))
        platform = platform or current_platform()
        if platform not in {"macos", "linux"}:
            raise ScheduleError("platform must be macos or linux")
        runtime_path = safe_text(runtime_path or os.environ.get("PATH", ""), "PATH")
        if any(not entry or not Path(entry).is_absolute() for entry in runtime_path.split(os.pathsep)):
            raise ScheduleError("PATH must contain only nonempty absolute directories")
        home = absolute(home or Path.home(), "home")
        if platform == "macos":
            unit_dir = home / "Library" / "LaunchAgents"
        else:
            config_home = absolute(os.environ.get("XDG_CONFIG_HOME", str(home / ".config")), "XDG_CONFIG_HOME")
            unit_dir = config_home / "systemd" / "user"
        return cls(platform, config_path, state,
                   absolute(repo or Path(__file__).resolve().parents[2], "repo"),
                   absolute(python or sys.executable, "python"), runtime_path,
                   unit_dir, os.getuid() if uid is None else uid, tuple(executables))

    @property
    def command(self) -> list[str]:
        return [str(self.python), str(self.repo / "scripts/ci-codex-queue/dispatcher.py"),
                "--config", str(self.config), "tick"]

    @property
    def stop_file(self) -> Path:
        return self.state / "STOP"

    @property
    def record_file(self) -> Path:
        return self.unit_dir / RECORD_NAME

    def render(self) -> dict[str, bytes]:
        if self.platform == "macos":
            body = {
                "Label": LABEL,
                "ProgramArguments": self.command,
                "WorkingDirectory": str(self.repo),
                "EnvironmentVariables": {"PATH": self.runtime_path},
                "StartInterval": INTERVAL_SECONDS,
                "RunAtLoad": False,
                "AbandonProcessGroup": True,
                "ProcessType": "Background",
                "Umask": 0o077,
                "StandardOutPath": str(self.state / "scheduler.stdout.log"),
                "StandardErrorPath": str(self.state / "scheduler.stderr.log"),
            }
            return {f"{LABEL}.plist": plistlib.dumps(body, sort_keys=False)}
        # ':' disables systemd's $VAR expansion, and quoted tokens preserve
        # spaces. No shell is involved. Workers own their own lock and STOP
        # handling; the short tick ending must not kill its detached worker.
        service = (
            "[Unit]\nDescription=oasis7 Codex Issue queue tick\n\n"
            "[Service]\nType=oneshot\n"
            # All task inputs are absolute; use a stable neutral cwd. Unlike
            # ExecStart, WorkingDirectory does not accept quoted path words.
            "WorkingDirectory=/\n"
            f"Environment={systemd_word('PATH=' + self.runtime_path)}\n"
            "ExecStart=:" + " ".join(systemd_word(word) for word in self.command) + "\n"
            "KillMode=process\nUMask=0077\nTimeoutStartSec=240\n"
            "StandardOutput=journal\nStandardError=journal\n"
        )
        timer = (
            "[Unit]\nDescription=oasis7 Codex Issue queue every five minutes\n\n"
            "[Timer]\nOnActiveSec=300s\nOnUnitActiveSec=300s\n"
            f"Unit={LABEL}.service\nAccuracySec=1s\nRandomizedDelaySec=0\n\n"
            "[Install]\nWantedBy=timers.target\n"
        )
        return {f"{LABEL}.service": service.encode(), f"{LABEL}.timer": timer.encode()}


def private_write(path: Path, content: bytes) -> None:
    path.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
    if path.is_symlink() or (path.exists() and not path.is_file()):
        raise ScheduleError(f"Refusing to replace a non-regular file: {path}")
    fd, name = tempfile.mkstemp(prefix=f".{path.name}.", dir=path.parent)
    try:
        with os.fdopen(fd, "wb") as output:
            output.write(content)
            output.flush()
            os.fsync(output.fileno())
        os.replace(name, path)
    finally:
        if os.path.exists(name):
            os.unlink(name)


def pause(spec: Schedule) -> None:
    private_write(spec.stop_file, b"Scheduling stopped. Run schedule.py install to resume explicitly.\n")


@contextmanager
def operation_lock(spec: Schedule):
    """Serialize install/stop for the fixed per-user label, across configs."""
    import fcntl
    spec.unit_dir.mkdir(parents=True, exist_ok=True, mode=0o700)
    lock_path = spec.unit_dir / f".{LABEL}.lock"
    if lock_path.is_symlink():
        raise ScheduleError("Scheduler operation lock must not be a symlink")
    fd = os.open(lock_path, os.O_CREAT | os.O_RDWR | os.O_NOFOLLOW, 0o600)
    with os.fdopen(fd, "a+") as handle:
        try:
            fcntl.flock(handle.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError as exc:
            raise ScheduleError("Another scheduler install/stop is in progress; no activation was changed") from exc
        yield
    # Do not unlink this inode: contenders must always lock the same file.


def read_record(spec: Schedule) -> dict | None:
    if spec.record_file.is_symlink():
        raise ScheduleError("Scheduler ownership record must not be a symlink")
    if not spec.record_file.exists():
        if any((spec.unit_dir / name).exists() or (spec.unit_dir / name).is_symlink()
               for name in spec.render()):
            raise ScheduleError("Existing scheduler files have no ownership record; refusing to overwrite them")
        return None
    try:
        record = json.loads(spec.record_file.read_text(encoding="utf-8"))
    except (OSError, ValueError) as exc:
        raise ScheduleError("Cannot verify scheduler ownership record") from exc
    if (not isinstance(record, dict) or record.get("schema") != 1
            or record.get("config") != str(spec.config) or record.get("state_dir") != str(spec.state)
            or record.get("platform") != spec.platform):
        raise ScheduleError("A different configuration already owns this user's scheduler; stop/uninstall it first")
    return record


def verify_owned_files(spec: Schedule, record: dict) -> None:
    files = record.get("files")
    if not isinstance(files, dict) or set(files) != set(spec.render()):
        raise ScheduleError("Cannot verify filenames in scheduler ownership record")
    for name, expected_hash in files.items():
        if not isinstance(expected_hash, str) or not re.fullmatch(r"[0-9a-f]{64}", expected_hash):
            raise ScheduleError("Cannot verify hashes in scheduler ownership record")
        path = spec.unit_dir / name
        if path.is_symlink() or (path.exists() and (
                not path.is_file() or hashlib.sha256(path.read_bytes()).hexdigest() != expected_hash)):
            raise ScheduleError("A scheduler file was modified; preserving that file")


class Manager:
    def __init__(self, spec: Schedule, *, run: Callable = subprocess.run,
                 which: Callable = shutil.which):
        self.spec = spec
        self.run = run
        self.which = which
        binary = "launchctl" if spec.platform == "macos" else "systemctl"
        self.binary = which(binary, path=spec.runtime_path)
        if not self.binary:
            raise ScheduleError(f"{binary} is not available in the selected PATH")

    def call(self, *args: str, check: bool = True) -> subprocess.CompletedProcess:
        command = [self.binary, *args]
        if self.spec.platform == "linux":
            command.insert(1, "--user")
        try:
            result = self.run(command, check=False, capture_output=True, text=True, timeout=30)
        except (OSError, subprocess.TimeoutExpired) as exc:
            raise ScheduleError("Service manager did not complete; scheduling state is unverified") from exc
        if check and result.returncode:
            raise ScheduleError(f"Service manager failed ({result.returncode}): {shlex.join(command)}")
        return result

    @property
    def target(self) -> str:
        return f"gui/{self.spec.uid}/{LABEL}"

    def launch_loaded(self) -> bool:
        result = self.call("print", self.target, check=False)
        if result.returncode == 0:
            return True
        if re.search(r"could not find service|service not found|no such process",
                     result.stdout + result.stderr, re.IGNORECASE):
            return False
        raise ScheduleError("Cannot determine whether the launchd job is loaded")

    def systemd_state(self, suffix: str) -> dict[str, str]:
        result = self.call("show", f"{LABEL}.{suffix}",
                           "--property=LoadState,ActiveState,UnitFileState,FragmentPath,DropInPaths", check=False)
        fields = dict(line.split("=", 1) for line in result.stdout.splitlines() if "=" in line)
        if not fields.get("LoadState") or (result.returncode and fields.get("LoadState") != "not-found"):
            raise ScheduleError("Cannot verify systemd user service state")
        return fields

    def verify_active(self) -> None:
        if self.spec.platform == "macos":
            if not self.launch_loaded():
                raise ScheduleError("launchd did not load the scheduler")
            disabled = self.call("print-disabled", f"gui/{self.spec.uid}").stdout
            if not re.search(r'"' + re.escape(LABEL) + r'"\s*=>\s*false', disabled):
                raise ScheduleError("launchd enabled state is unverified")
        else:
            for suffix in ("timer", "service"):
                observed = self.systemd_state(suffix)
                if (observed.get("LoadState") != "loaded" or observed.get("DropInPaths")
                        or observed.get("FragmentPath") != str(self.spec.unit_dir / f"{LABEL}.{suffix}")):
                    raise ScheduleError("systemd loaded an unexpected unit or extra overrides")
                if suffix == "timer" and (observed.get("ActiveState") != "active"
                                           or observed.get("UnitFileState") != "enabled"):
                    raise ScheduleError("systemd timer is not both active and enabled")

    def deactivate(self, known: bool) -> None:
        if self.spec.platform == "macos":
            if not known and self.launch_loaded():
                raise ScheduleError("An unowned launchd job uses the scheduler label")
            self.call("disable", self.target)
            if self.launch_loaded():
                self.call("bootout", self.target)
            if self.launch_loaded():
                raise ScheduleError("launchd job remains loaded")
            disabled = self.call("print-disabled", f"gui/{self.spec.uid}").stdout
            if not re.search(r'"' + re.escape(LABEL) + r'"\s*=>\s*true', disabled):
                raise ScheduleError("launchd disabled state is unverified")
        else:
            if not known:
                for suffix in ("timer", "service"):
                    if self.systemd_state(suffix).get("LoadState") != "not-found":
                        raise ScheduleError("An unowned systemd unit uses the scheduler name")
                return
            self.call("disable", "--now", f"{LABEL}.timer")
            self.call("stop", f"{LABEL}.service")
            observed = self.systemd_state("timer")
            if (observed.get("ActiveState") not in {"inactive", "failed"}
                    or observed.get("UnitFileState") not in {"disabled", "masked", "not-found", ""}):
                raise ScheduleError("systemd timer did not become inactive and disabled")
            if self.systemd_state("service").get("ActiveState") not in {"inactive", "failed"}:
                raise ScheduleError("systemd tick service has not stopped")


def install(spec: Schedule, *, run: Callable = subprocess.run,
            which: Callable = shutil.which) -> dict:
    if spec.platform != current_platform() or spec.uid == 0:
        raise ScheduleError("Install using the actual non-root login user on the selected platform")
    with operation_lock(spec):
        return _install_locked(spec, run=run, which=which)


def _install_locked(spec: Schedule, *, run: Callable, which: Callable) -> dict:
    record = read_record(spec)
    if record:
        verify_owned_files(spec, record)
    if not spec.python.is_file() or not os.access(spec.python, os.X_OK):
        raise ScheduleError("The absolute Python executable is unavailable")
    if not Path(spec.command[1]).is_file():
        raise ScheduleError("The checkout's dispatcher.py is unavailable")
    for name, executable in spec.executables:
        path = Path(executable)
        available = (path.is_file() and os.access(path, os.X_OK)) if path.is_absolute() else which(
            executable, path=spec.runtime_path)
        if not available:
            raise ScheduleError(f"{name} executable is unavailable; configure its absolute path or the selected PATH")
    manager = Manager(spec, run=run, which=which)
    # Keep a failed or partially activated install paused. Never restore work
    # simply because unit files have been generated.
    pause(spec)
    manager.deactivate(record is not None)
    files = spec.render()
    for name, content in files.items():
        private_write(spec.unit_dir / name, content)
    metadata = {"schema": 1, "config": str(spec.config), "state_dir": str(spec.state),
                "platform": spec.platform,
                "files": {name: hashlib.sha256(content).hexdigest() for name, content in files.items()}}
    private_write(spec.record_file, (json.dumps(metadata, indent=2) + "\n").encode())
    if spec.platform == "macos":
        for name in ("scheduler.stdout.log", "scheduler.stderr.log"):
            log = spec.state / name
            if not log.exists():
                private_write(log, b"")
        manager.call("enable", manager.target)
        manager.call("bootstrap", f"gui/{spec.uid}", str(spec.unit_dir / f"{LABEL}.plist"))
    else:
        manager.call("daemon-reload")
        manager.call("enable", "--now", f"{LABEL}.timer")
    manager.verify_active()
    spec.stop_file.unlink(missing_ok=True)
    return {"state": "scheduled", "interval_seconds": INTERVAL_SECONDS,
            "first_tick": "after 300 seconds; no Codex task was started by install",
            "config": str(spec.config), "state_dir": str(spec.state)}


def stop(spec: Schedule, *, uninstall: bool = False,
         run: Callable = subprocess.run, which: Callable = shutil.which) -> dict:
    if spec.platform != current_platform():
        raise ScheduleError("Stop on the platform where the scheduler was installed")
    with operation_lock(spec):
        return _stop_locked(spec, uninstall=uninstall, run=run, which=which)


def _stop_locked(spec: Schedule, *, uninstall: bool, run: Callable, which: Callable) -> dict:
    pause(spec)
    record = read_record(spec)
    manager = Manager(spec, run=run, which=which)
    manager.deactivate(record is not None)
    if uninstall and record:
        verify_owned_files(spec, record)
        for name in record.get("files", {}):
            (spec.unit_dir / name).unlink(missing_ok=True)
        spec.record_file.unlink()
        if spec.platform == "linux":
            manager.call("daemon-reload")
    return {"state": "uninstalled" if uninstall else "paused", "stop_file": str(spec.stop_file),
            "worker": "stop requested; an active worker exits after observing STOP",
            "persistent_state": "preserved"}


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--config", required=True, help="absolute path to the private JSON config")
    parser.add_argument("--platform", choices=("macos", "linux"), help="cross-platform rendering only")
    parser.add_argument("--python", help="absolute Python executable; defaults to this interpreter")
    parser.add_argument("--runtime-path", help="absolute PATH entries for local gh, codex and service manager")
    actions = parser.add_subparsers(dest="action", required=True)
    render = actions.add_parser("render", help="write definitions only; never activate or clear STOP")
    render.add_argument("--output-dir", required=True)
    actions.add_parser("install", help="install and verify the user scheduler, then explicitly resume")
    actions.add_parser("stop", help="write STOP and disable/unload the scheduler; preserve state")
    actions.add_parser("uninstall", help="stop and remove owned scheduler files; preserve state")
    args = parser.parse_args(argv)
    try:
        spec = Schedule.load(args.config, args.platform, python=args.python, runtime_path=args.runtime_path)
        if args.action == "render":
            output_dir = absolute(args.output_dir, "output-dir")
            paths = []
            for name, content in spec.render().items():
                path = output_dir / name
                private_write(path, content)
                paths.append(str(path))
            command = [str(spec.python), str(Path(__file__).resolve()), "--config", str(spec.config),
                       "--runtime-path", spec.runtime_path, "install"]
            result = {"state": "rendered_only", "files": paths, "activated": False,
                      "next_step": shlex.join(command)}
        elif args.action == "install":
            result = install(spec)
        else:
            result = stop(spec, uninstall=args.action == "uninstall")
    except (ScheduleError, OSError) as exc:
        print(json.dumps({"state": "error", "message": str(exc)}, ensure_ascii=False), file=sys.stderr)
        return 1
    print(json.dumps(result, ensure_ascii=False, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
