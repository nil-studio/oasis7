#!/usr/bin/env python3
"""Single-session, fail-closed remote engine for ECS Preview deployment.

The local controller embeds this source in one SSH ``python3 -c`` process.
The first stdin line is a bootstrap object containing ``probe_source``; later
JSON lines and bounded raw payloads implement the protocol in
``doc/scripts/ecs-preview-deployment.design.md``.
"""

from __future__ import annotations

import fcntl
import hashlib
import http.client
import json
import os
from pathlib import Path, PurePosixPath
import re
import shutil
import signal
import ssl
import stat
import subprocess
import sys
import tarfile
import time
import unicodedata
from typing import BinaryIO, Callable
from urllib.parse import urlsplit


MAX_BOOTSTRAP_BYTES = 1024 * 1024
MAX_FRAME_BYTES = 128 * 1024
MAX_ARCHIVE_BYTES = 2 * 1024**3
MAX_EXPANDED_BYTES = 8 * 1024**3
MAX_BACKUP_BYTES = 32 * 1024**3
MAX_TAR_MEMBERS = 100_000
MAX_CHUNK_BYTES = 1024 * 1024
MIN_FREE_BYTES = 528 * 1024**2
TRANSFER_TIMEOUT_SECONDS = 30 * 60
SERVICE_TIMEOUT_SECONDS = 90
SERVICE_START_TIMEOUT_SECONDS = 15
WORLD_READ_TIMEOUT_SECONDS = 15
SKILL_TIMEOUT_SECONDS = 60
WORLD_SAMPLE_COUNT = 4
WORLD_SAMPLE_INTERVAL_SECONDS = 20
SERVICE_BINS = {
    "chain": "oasis7_chain_runtime",
    "player": "oasis7_game_launcher",
}
PACKAGE_BINS = (
    "oasis7_chain_runtime",
    "oasis7_game_launcher",
    "oasis7_viewer_live",
    "oasis7_game_client",
)
CONTROLLED_PROCESS_NAMES = frozenset((*PACKAGE_BINS,))
SAFE_SERVICE_NAME = re.compile(r"^[A-Za-z0-9_.@:-]+\.service$")
HEX_256 = re.compile(r"^[0-9a-f]{64}$")
HEX_160 = re.compile(r"^[0-9a-f]{40}$")
RUN_ID = re.compile(r"^[1-9][0-9]{0,19}$")
SUM_LINE = re.compile(r"^([0-9a-f]{64})  (.+)$")


class RemoteError(Exception):
    """An error with a fixed protocol-safe category."""

    def __init__(self, code: str):
        super().__init__(code)
        self.code = code


class LimitedHashReader:
    """Expose exactly ``length`` input bytes while hashing the raw archive."""

    def __init__(self, source: BinaryIO, length: int):
        self.source = source
        self.remaining = length
        self.count = 0
        self.digest = hashlib.sha256()

    def read(self, size: int = -1) -> bytes:
        if self.remaining <= 0:
            return b""
        if size is None or size < 0:
            size = min(self.remaining, MAX_CHUNK_BYTES)
        data = self.source.read(min(size, self.remaining))
        if not data:
            raise RemoteError("payload_truncated")
        self.remaining -= len(data)
        self.count += len(data)
        self.digest.update(data)
        return data

    def drain(self) -> None:
        while self.remaining:
            self.read(min(self.remaining, MAX_CHUNK_BYTES))


def _absolute_path(value: object, *, must_exist: bool = True) -> Path:
    if not isinstance(value, str) or not value or "\x00" in value:
        raise RemoteError("config_invalid")
    path = Path(value)
    if not path.is_absolute() or ".." in path.parts:
        raise RemoteError("config_invalid")
    if must_exist:
        try:
            resolved = path.resolve(strict=True)
        except OSError:
            raise RemoteError("config_path_missing") from None
        if resolved != path:
            raise RemoteError("config_symlink")
    return path


def _ensure_no_symlink_components(path: Path, *, allow_missing_tail: bool = False) -> None:
    current = Path(path.anchor)
    parts = path.parts[1:]
    for index, part in enumerate(parts):
        current = current / part
        try:
            metadata = current.lstat()
        except FileNotFoundError:
            if allow_missing_tail:
                return
            raise RemoteError("config_path_missing") from None
        except OSError:
            raise RemoteError("config_path_invalid") from None
        if stat.S_ISLNK(metadata.st_mode):
            raise RemoteError("config_symlink")
        if index < len(parts) - 1 and not stat.S_ISDIR(metadata.st_mode):
            raise RemoteError("config_path_invalid")


def _validate_origin(value: object) -> str:
    if not isinstance(value, str) or len(value) > 512:
        raise RemoteError("config_invalid")
    parsed = urlsplit(value)
    if (parsed.scheme != "https" or not parsed.hostname or parsed.username is not None
            or parsed.password is not None or parsed.path not in ("", "/")
            or parsed.query or parsed.fragment):
        raise RemoteError("config_invalid")
    try:
        port = parsed.port
    except ValueError:
        raise RemoteError("config_invalid") from None
    host = parsed.hostname.encode("idna").decode("ascii").lower()
    if ":" in host and not host.startswith("["):
        host = f"[{host}]"
    return f"https://{host}" + (f":{port}" if port not in (None, 443) else "")


def _validate_config(raw: object) -> dict:
    if not isinstance(raw, dict):
        raise RemoteError("config_invalid")
    required = {
        "release_root", "ops_root", "deploy_lock", "chain_service",
        "player_service", "backup_paths", "public_origin", "launcher_http_port",
        "nginx_include", "probe",
    }
    optional = {"controller_lock", "unit_dir"}
    if not required.issubset(raw) or set(raw) - required - optional:
        raise RemoteError("config_invalid")
    config = dict(raw)
    config["release_root"] = _absolute_path(raw["release_root"])
    if not config["release_root"].is_dir():
        raise RemoteError("config_path_invalid")
    if config["release_root"].stat().st_mode & 0o022:
        raise RemoteError("config_path_permissions")
    config["ops_root"] = _absolute_path(raw["ops_root"], must_exist=False)
    _ensure_no_symlink_components(config["ops_root"], allow_missing_tail=True)
    config["deploy_lock"] = _absolute_path(raw["deploy_lock"], must_exist=False)
    _ensure_no_symlink_components(config["deploy_lock"], allow_missing_tail=True)
    controller_lock = raw.get("controller_lock")
    if controller_lock is None:
        controller_lock = str(config["deploy_lock"]) + ".controller"
    config["controller_lock"] = _absolute_path(controller_lock, must_exist=False)
    _ensure_no_symlink_components(config["controller_lock"], allow_missing_tail=True)
    if config["controller_lock"] == config["deploy_lock"]:
        raise RemoteError("config_invalid")
    config["unit_dir"] = _absolute_path(raw.get("unit_dir", "/etc/systemd/system"))
    if not config["unit_dir"].is_dir():
        raise RemoteError("config_path_invalid")
    for key in ("chain_service", "player_service"):
        name = raw[key]
        if not isinstance(name, str) or not SAFE_SERVICE_NAME.fullmatch(name):
            raise RemoteError("config_invalid")
    if raw["chain_service"] == raw["player_service"]:
        raise RemoteError("config_invalid")
    port = raw["launcher_http_port"]
    if type(port) is not int or not 1 <= port <= 65535:
        raise RemoteError("config_invalid")
    config["public_origin"] = _validate_origin(raw["public_origin"])
    config["nginx_include"] = _absolute_path(raw["nginx_include"])
    paths = raw["backup_paths"]
    if not isinstance(paths, list) or not paths or len(paths) > 64:
        raise RemoteError("config_invalid")
    normalized = []
    for value in paths:
        path = _absolute_path(value)
        if path == Path("/"):
            raise RemoteError("backup_path_invalid")
        if not (path.exists() or path.is_symlink()):
            raise RemoteError("backup_path_missing")
        normalized.append(path)
    if len(set(normalized)) != len(normalized):
        raise RemoteError("backup_paths_duplicate")
    ordered = sorted(normalized, key=lambda item: len(item.parts))
    for index, path in enumerate(ordered):
        if any(parent in path.parents for parent in ordered[:index]):
            raise RemoteError("backup_paths_overlap")
    units = [config["unit_dir"] / raw["chain_service"],
             config["unit_dir"] / raw["player_service"]]
    required_backup = [*units, config["nginx_include"]]
    probe = raw["probe"]
    if not isinstance(probe, dict):
        raise RemoteError("config_invalid")
    key_path = _absolute_path(probe.get("reader_key_file"))
    required_backup.append(key_path)
    if any(not any(backup == path or backup in path.parents for backup in normalized)
           for path in required_backup):
        raise RemoteError("backup_paths_incomplete")
    config["backup_paths"] = normalized
    return config


def _safe_member_name(name: str) -> str:
    if (not isinstance(name, str) or not name or "\x00" in name or "\\" in name
            or any(unicodedata.category(char) == "Cc" or char in "\u2028\u2029" for char in name)):
        raise RemoteError("archive_member_path")
    while name.startswith("./"):
        name = name[2:]
    if name in ("", "."):
        return "."
    pure = PurePosixPath(name)
    if pure.is_absolute() or any(part in ("", ".", "..") for part in pure.parts):
        raise RemoteError("archive_member_path")
    canonical = pure.as_posix()
    if canonical != name or canonical.startswith("../") or canonical == "..":
        raise RemoteError("archive_member_path")
    return canonical


def _write_all(fd: int, data: bytes) -> None:
    offset = 0
    while offset < len(data):
        written = os.write(fd, data[offset:])
        if written <= 0:
            raise RemoteError("archive_write_failed")
        offset += written


def _extract_streaming_archive(
    source: BinaryIO,
    destination: Path,
    *,
    archive_bytes: int,
    expected_digest: str,
    expanded_bytes: int,
) -> dict[str, str]:
    if (type(archive_bytes) is not int or not 0 < archive_bytes <= MAX_ARCHIVE_BYTES
            or not HEX_256.fullmatch(expected_digest)
            or type(expanded_bytes) is not int or not 0 <= expanded_bytes <= MAX_EXPANDED_BYTES):
        raise RemoteError("archive_limits")
    reader = LimitedHashReader(source, archive_bytes)
    seen: dict[str, bool] = {}
    file_digests: dict[str, str] = {}
    total = 0
    members = 0
    try:
        with tarfile.open(fileobj=reader, mode="r|gz") as archive:
            for member in archive:
                members += 1
                if members > MAX_TAR_MEMBERS:
                    raise RemoteError("archive_member_limit")
                name = _safe_member_name(member.name)
                if name == ".":
                    if not member.isdir() or name in seen:
                        raise RemoteError("archive_member_type")
                    seen[name] = True
                    continue
                if member.isdir():
                    if member.size != 0:
                        raise RemoteError("archive_member_type")
                    is_dir = True
                elif member.isreg():
                    is_dir = False
                    if member.size < 0 or member.size > MAX_EXPANDED_BYTES - total:
                        raise RemoteError("archive_expanded_limit")
                    total += member.size
                else:
                    raise RemoteError("archive_member_type")
                if name in seen:
                    raise RemoteError("archive_duplicate_path")
                parts = name.split("/")
                for count in range(1, len(parts)):
                    parent = "/".join(parts[:count])
                    if parent in seen and not seen[parent]:
                        raise RemoteError("archive_path_collision")
                if not is_dir and any(existing.startswith(name + "/") for existing in seen):
                    raise RemoteError("archive_path_collision")
                seen[name] = is_dir
                directory_parts = parts if is_dir else parts[:-1]
                current = destination
                for part in directory_parts:
                    current = current / part
                    current.mkdir(exist_ok=True, mode=0o755)
                    if current.is_symlink() or not current.is_dir():
                        raise RemoteError("archive_path_collision")
                    os.chmod(current, 0o755)
                if is_dir:
                    continue
                target = current / parts[-1]
                extracted = archive.extractfile(member)
                if extracted is None:
                    raise RemoteError("archive_member_type")
                digest = hashlib.sha256()
                flags = os.O_WRONLY | os.O_CREAT | os.O_EXCL
                if hasattr(os, "O_NOFOLLOW"):
                    flags |= os.O_NOFOLLOW
                fd = os.open(target, flags, 0o600)
                try:
                    remaining = member.size
                    while remaining:
                        block = extracted.read(min(MAX_CHUNK_BYTES, remaining))
                        if not block:
                            raise RemoteError("archive_truncated_member")
                        _write_all(fd, block)
                        digest.update(block)
                        remaining -= len(block)
                    os.fsync(fd)
                finally:
                    os.close(fd)
                mode = 0o755 if name.startswith("bin/") else 0o644
                os.chmod(target, mode)
                file_digests[name] = digest.hexdigest()
        reader.drain()
    except RemoteError:
        raise
    except (tarfile.TarError, OSError, EOFError, ValueError):
        raise RemoteError("archive_invalid") from None
    if reader.count != archive_bytes or reader.digest.hexdigest() != expected_digest:
        raise RemoteError("archive_digest_mismatch")
    if total != expanded_bytes:
        raise RemoteError("archive_expanded_size_mismatch")
    return file_digests


def _sha256(path: Path) -> str:
    try:
        digest = hashlib.sha256()
        with path.open("rb") as stream:
            for block in iter(lambda: stream.read(MAX_CHUNK_BYTES), b""):
                digest.update(block)
        return digest.hexdigest()
    except OSError:
        raise RemoteError("file_unavailable") from None


def _validate_package(root: Path, file_digests: dict[str, str], source: str) -> None:
    try:
        build_lines = (root / "BUILDINFO").read_text(encoding="utf-8").splitlines()
        build = dict(line.split("=", 1) for line in build_lines if "=" in line)
        manifest = json.loads((root / "manifest.json").read_text(encoding="utf-8"))
        sums_text = (root / "SHA256SUMS").read_text(encoding="utf-8")
    except (OSError, UnicodeError, ValueError, json.JSONDecodeError):
        raise RemoteError("package_metadata_invalid") from None
    if build != {"source": source, "profile": "ecs", "platform": "linux-x64", "kind": "ecs-server"}:
        raise RemoteError("package_source_mismatch")
    if manifest != {
        "schema": 1,
        "source": source,
        "profile": "ecs",
        "platform": "linux-x64",
        "binaries": list(PACKAGE_BINS),
    }:
        raise RemoteError("package_manifest_mismatch")
    sums: dict[str, str] = {}
    for line in sums_text.splitlines():
        match = SUM_LINE.fullmatch(line)
        if not match:
            raise RemoteError("package_checksum_manifest_invalid")
        digest, name = match.groups()
        canonical = _safe_member_name(name)
        if canonical != name or name in sums or name == "SHA256SUMS":
            raise RemoteError("package_checksum_manifest_invalid")
        sums[name] = digest
    payload_files = set(file_digests) - {"SHA256SUMS"}
    if set(sums) != payload_files or any(file_digests[name] != digest for name, digest in sums.items()):
        raise RemoteError("package_checksum_mismatch")
    expected_bins = {"bin/" + name for name in PACKAGE_BINS}
    if {name for name in file_digests if name.startswith("bin/")} != expected_bins:
        raise RemoteError("package_binaries_invalid")
    for path in [root / "bin" / name for name in PACKAGE_BINS]:
        if not path.is_file() or not os.access(path, os.X_OK):
            raise RemoteError("package_binaries_invalid")
    for required in ("web-dist/index.html", "deployment/ecs-skill-release.py"):
        if required not in file_digests:
            raise RemoteError("package_payload_missing")


class Engine:
    """One target-side protocol session and its deployment state."""

    def __init__(
        self,
        stdin: BinaryIO | None = None,
        stdout: BinaryIO | None = None,
        stderr: BinaryIO | None = None,
        *,
        runner: Callable = subprocess.run,
        popen: Callable = subprocess.Popen,
        sleep: Callable[[float], None] = time.sleep,
        monotonic: Callable[[], float] = time.monotonic,
        disk_usage: Callable = shutil.disk_usage,
        proc_root: Path = Path("/proc"),
        cgroup_root: Path = Path("/sys/fs/cgroup"),
        uid: Callable[[], int] = os.geteuid,
        enforce_root: bool = True,
    ):
        self.stdin = stdin or sys.stdin.buffer
        self.stdout = stdout or sys.stdout.buffer
        self.stderr = stderr or sys.stderr.buffer
        self.runner = runner
        self.popen = popen
        self.sleep = sleep
        self.monotonic = monotonic
        self.disk_usage = disk_usage
        self.proc_root = proc_root
        self.cgroup_root = cgroup_root
        self.uid = uid
        self.enforce_root = enforce_root
        self.probe_ns: dict = {}
        self.config: dict | None = None
        self.source = ""
        self.run_id = ""
        self.archive_meta: dict = {}
        self.release: Path | None = None
        self.operation_dir: Path | None = None
        self.operation_dir_created = False
        self.locks: list[tuple[Path, int]] = []
        self.deploy_lock_held = False
        self.phase = "new"
        self.units: dict[str, Path] = {}
        self.before_units: dict[str, bytes] = {}
        self.new_units: dict[str, bytes] = {}
        self.unit_modes: dict[str, tuple[int, int, int]] = {}
        self.old_roots: dict[str, Path] = {}
        self.old_processes: dict[str, dict] = {}
        self.candidate_processes: dict[str, dict] = {}
        self.service_public_key = ""
        self.world_identity: dict | None = None
        self.backup_digest = ""
        self.backup_bytes = 0
        self.max_backup_bytes = 0
        self.backup_verified = False
        self.units_changed = False
        self.writer_boundary = False
        self.initialized = False
        self.accepted = False
        self.terminal = False
        self.stage = "bootstrap"

    def _stderr(self, message: str) -> None:
        try:
            self.stderr.write((message + "\n").encode("utf-8", "replace"))
            self.stderr.flush()
        except (OSError, BrokenPipeError):
            pass

    def _json_line(self, value: dict) -> None:
        try:
            self.stdout.write(json.dumps(value, separators=(",", ":"), ensure_ascii=True).encode("ascii") + b"\n")
            self.stdout.flush()
        except (OSError, BrokenPipeError):
            raise RemoteError("transport_lost") from None

    def _read_json_line(self, limit: int = MAX_FRAME_BYTES) -> dict | None:
        raw = self.stdin.readline(limit + 1)
        if raw == b"":
            return None
        if len(raw) > limit or not raw.endswith(b"\n"):
            raise RemoteError("frame_invalid")
        try:
            value = json.loads(raw.decode("utf-8"))
        except (UnicodeError, json.JSONDecodeError):
            raise RemoteError("frame_invalid") from None
        if not isinstance(value, dict):
            raise RemoteError("frame_invalid")
        return value

    def _run(self, argv: list[str], *, timeout: int = 60, input_data: bytes | None = None,
             accepted: tuple[int, ...] = (0,)) -> bytes:
        try:
            result = self.runner(argv, input=input_data, stdout=subprocess.PIPE,
                                 stderr=subprocess.DEVNULL, timeout=timeout, check=False)
        except (OSError, subprocess.TimeoutExpired):
            raise RemoteError("command_failed") from None
        if result.returncode not in accepted:
            raise RemoteError("command_failed")
        output = result.stdout or b""
        if isinstance(output, str):
            output = output.encode()
        return output

    def _systemctl(self, *args: str, timeout: int = SERVICE_TIMEOUT_SECONDS) -> bytes:
        return self._run(["systemctl", *args], timeout=timeout)

    def _space(self, path: Path) -> int:
        try:
            return self.disk_usage(path).free
        except OSError:
            raise RemoteError("disk_space_unknown") from None

    def _require_reserve(self, extra: int = 0) -> None:
        if extra < 0 or extra > MAX_EXPANDED_BYTES:
            raise RemoteError("disk_space_unknown")
        if self._space(self.config["release_root"]) < MIN_FREE_BYTES + extra:
            raise RemoteError("disk_reserve")

    def _acquire_lock(self, path: Path) -> None:
        flags = os.O_RDWR | os.O_CREAT
        if hasattr(os, "O_NOFOLLOW"):
            flags |= os.O_NOFOLLOW
        try:
            fd = os.open(path, flags, 0o600)
            metadata = os.fstat(fd)
            if (not stat.S_ISREG(metadata.st_mode) or metadata.st_nlink != 1
                    or (self.enforce_root and metadata.st_uid != 0)):
                os.close(fd)
                raise RemoteError("lock_invalid")
            if self.enforce_root:
                os.fchmod(fd, 0o600)
            fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            raise RemoteError("lock_busy") from None
        except RemoteError:
            raise
        except OSError:
            raise RemoteError("lock_invalid") from None
        self.locks.append((path, fd))
        if self.config is not None and path == self.config["deploy_lock"]:
            self.deploy_lock_held = True

    def _release_deploy_lock(self) -> None:
        if not self.locks:
            return
        deploy_path = self.config["deploy_lock"]
        for index in range(len(self.locks) - 1, -1, -1):
            path, fd = self.locks[index]
            if path == deploy_path:
                fcntl.flock(fd, fcntl.LOCK_UN)
                os.close(fd)
                self.locks.pop(index)
                self.deploy_lock_held = False
                return
        raise RemoteError("lock_state_invalid")

    def _reacquire_deploy_lock(self) -> None:
        self._acquire_lock(self.config["deploy_lock"])

    def _release_locks(self) -> None:
        for _path, fd in reversed(self.locks):
            try:
                fcntl.flock(fd, fcntl.LOCK_UN)
                os.close(fd)
            except OSError:
                pass
        self.locks.clear()
        self.deploy_lock_held = False

    def _prepare_paths(self) -> None:
        config = self.config
        release_root = config["release_root"]
        if self.enforce_root:
            for path in (release_root, config["unit_dir"]):
                if path.stat().st_uid != 0:
                    raise RemoteError("remote_path_owner")
        ops_root = config["ops_root"]
        ops_root.mkdir(parents=True, exist_ok=True, mode=0o700)
        _ensure_no_symlink_components(ops_root)
        if not ops_root.is_dir():
            raise RemoteError("remote_path_invalid")
        if self.enforce_root and ops_root.stat().st_uid != 0:
            raise RemoteError("remote_path_owner")
        os.chmod(ops_root, 0o700)
        leaf = f"deploy-{self.run_id}-{self.source[:12]}"
        self.operation_dir = ops_root / leaf
        try:
            operation_metadata = self.operation_dir.lstat()
        except FileNotFoundError:
            operation_metadata = None
        except OSError:
            raise RemoteError("remote_path_invalid") from None
        if operation_metadata is not None:
            raise RemoteError("operation_exists")
        self.release = release_root / f"ecs-{self.run_id}-{self.source[:12]}"
        try:
            release_metadata = self.release.lstat()
        except FileNotFoundError:
            release_metadata = None
        except OSError:
            raise RemoteError("release_invalid") from None
        if release_metadata is not None and not stat.S_ISDIR(release_metadata.st_mode):
            raise RemoteError("release_conflict")

    def _create_operation_dir(self) -> None:
        try:
            self.operation_dir.mkdir(mode=0o700)
        except FileExistsError:
            raise RemoteError("operation_exists") from None
        except OSError:
            raise RemoteError("remote_path_invalid") from None
        self.operation_dir_created = True
        if self.enforce_root and self.operation_dir.stat().st_uid != 0:
            raise RemoteError("remote_path_owner")
        os.chmod(self.operation_dir, 0o700)

    def _atomic_write(self, path: Path, content: bytes, mode: int = 0o600) -> None:
        temp = path.with_name(path.name + ".tmp-" + str(os.getpid()))
        flags = os.O_WRONLY | os.O_CREAT | os.O_EXCL
        if hasattr(os, "O_NOFOLLOW"):
            flags |= os.O_NOFOLLOW
        try:
            fd = os.open(temp, flags, 0o600)
            try:
                _write_all(fd, content)
                if self.enforce_root:
                    os.fchown(fd, 0, 0)
                os.fchmod(fd, mode)
                os.fsync(fd)
            finally:
                os.close(fd)
            os.replace(temp, path)
            self._fsync_dir(path.parent)
        except OSError:
            try:
                temp.unlink(missing_ok=True)
            except OSError:
                pass
            raise RemoteError("atomic_write_failed") from None

    @staticmethod
    def _fsync_dir(path: Path) -> None:
        try:
            fd = os.open(path, os.O_RDONLY | getattr(os, "O_DIRECTORY", 0))
            try:
                os.fsync(fd)
            finally:
                os.close(fd)
        except OSError:
            raise RemoteError("durability_failed") from None

    def _record(self, name: str, data: dict) -> None:
        if self.operation_dir is None:
            return
        allowed = {"started.json", "backup.json", "units-changing.json", "writer-boundary.json",
                   "prewriter-restored.json", "prewriter-restore-failed.json", "failed-fenced.json",
                   "fence-failed.json", "accepted.json", "abort.json"}
        if name not in allowed:
            raise RemoteError("record_invalid")
        raw = (json.dumps(data, sort_keys=True, separators=(",", ":")) + "\n").encode()
        path = self.operation_dir / name
        if path.exists():
            raise RemoteError("record_exists")
        self._atomic_write(path, raw)

    def _init(self, request: dict) -> dict:
        if self.phase != "new":
            raise RemoteError("protocol_state")
        if set(request) != {"op", "source", "run_id", "archive", "config"}:
            raise RemoteError("request_invalid")
        source = request.get("source")
        run_id = request.get("run_id")
        archive = request.get("archive")
        if not isinstance(source, str) or not HEX_160.fullmatch(source):
            raise RemoteError("source_invalid")
        if type(run_id) is int and run_id > 0:
            run_id = str(run_id)
        if not isinstance(run_id, str) or not RUN_ID.fullmatch(run_id):
            raise RemoteError("run_invalid")
        if not isinstance(archive, dict) or set(archive) != {"bytes", "sha256", "expanded_bytes"}:
            raise RemoteError("archive_limits")
        if (type(archive["bytes"]) is not int or not 0 < archive["bytes"] <= MAX_ARCHIVE_BYTES
                or type(archive["expanded_bytes"]) is not int
                or not 0 <= archive["expanded_bytes"] <= MAX_EXPANDED_BYTES
                or not isinstance(archive["sha256"], str) or not HEX_256.fullmatch(archive["sha256"])):
            raise RemoteError("archive_limits")
        self.config = _validate_config(request.get("config"))
        self.source, self.run_id, self.archive_meta = source, run_id, dict(archive)
        if self.enforce_root and self.uid() != 0:
            raise RemoteError("privilege_required")
        self._prepare_paths()
        try:
            locks = [self.config["deploy_lock"]]
            if self.config.get("controller_lock"):
                locks.append(self.config["controller_lock"])
            for path in locks:
                self._acquire_lock(path)
            self._require_reserve(self.archive_meta["expanded_bytes"])
            self._preflight_probe()
            self._create_operation_dir()
            self._snapshot_services()
            self._record("started.json", {
                "source": self.source,
                "run_id": self.run_id,
                "release": str(self.release),
                "archive_sha256": self.archive_meta["sha256"],
                "archive_bytes": self.archive_meta["bytes"],
                "started_unix": int(time.time()),
            })
            self.phase = "initialized"
            self.initialized = True
        except BaseException:
            if self.operation_dir is not None and self.operation_dir_created:
                shutil.rmtree(self.operation_dir, ignore_errors=True)
                self.operation_dir_created = False
            self._release_locks()
            raise
        return {"source": self.source, "release": str(self.release), "old_release_roots": {
            key: str(value) for key, value in self.old_roots.items()},
            "initial_world": self.world_identity}

    def _preflight_probe(self) -> None:
        try:
            probe = self.config["probe"]
            derived = self.probe_ns["preflight"](probe)
        except (KeyError, TypeError, ValueError, OSError):
            raise RemoteError("probe_preflight_failed") from None
        if not isinstance(derived, str) or derived.lower() != probe.get("reader_public_key", "").lower():
            raise RemoteError("reader_key_pin_mismatch")
        service_key = probe.get("service_public_key")
        if not isinstance(service_key, str) or not re.fullmatch(r"[0-9a-fA-F]{64}", service_key):
            raise RemoteError("service_key_pin_invalid")
        self.service_public_key = service_key.lower()

    def _parse_show(self, service: str) -> dict[str, str]:
        raw = self._systemctl("show", service, "-p", "MainPID", "-p", "ActiveState",
                              "-p", "ControlGroup").decode("utf-8", "strict")
        values = {}
        for line in raw.splitlines():
            if "=" in line:
                key, value = line.split("=", 1)
                values[key] = value
        if not {"MainPID", "ActiveState", "ControlGroup"}.issubset(values):
            raise RemoteError("service_state_invalid")
        return values

    def _cgroup_dir(self, service: str) -> Path:
        values = self._parse_show(service)
        group = values["ControlGroup"]
        if not group.startswith("/") or ".." in Path(group).parts:
            raise RemoteError("cgroup_invalid")
        root = (self.cgroup_root / group.lstrip("/")).resolve()
        try:
            root.relative_to(self.cgroup_root.resolve())
        except ValueError:
            raise RemoteError("cgroup_invalid") from None
        return root

    def _cgroup_pids(self, service: str) -> set[int]:
        root = self._cgroup_dir(service)
        pids: set[int] = set()
        if not root.exists():
            return pids
        dirs = [root]
        try:
            dirs.extend(path for path in root.rglob("*") if path.is_dir())
            for directory in dirs:
                proc_file = directory / "cgroup.procs"
                if proc_file.exists():
                    for value in proc_file.read_text().split():
                        if not value.isdecimal():
                            raise RemoteError("cgroup_invalid")
                        pids.add(int(value))
        except (OSError, ValueError):
            raise RemoteError("cgroup_unavailable") from None
        return pids

    def _start_ticks(self, pid: int) -> int:
        try:
            raw = (self.proc_root / str(pid) / "stat").read_text()
            closing = raw.rfind(")")
            fields = raw[closing + 2:].split()
            return int(fields[19])
        except (OSError, ValueError, IndexError):
            raise RemoteError("process_identity_unavailable") from None

    def _process_identity(self, pid: int) -> dict:
        exe_link = self.proc_root / str(pid) / "exe"
        try:
            executable = exe_link.resolve(strict=True)
            name = executable.name
            digest = _sha256(exe_link)
            start_ticks = self._start_ticks(pid)
        except (OSError, RuntimeError):
            raise RemoteError("process_identity_unavailable") from None
        return {"pid": pid, "start_ticks": start_ticks, "name": name,
                "executable": str(executable), "sha256": digest}

    def _service_expected(self, name: str) -> str:
        if name == self.config["chain_service"]:
            return SERVICE_BINS["chain"]
        if name == self.config["player_service"]:
            return SERVICE_BINS["player"]
        raise RemoteError("service_invalid")

    def _service_processes(self, service: str, expected_root: Path | None = None,
                           *, require_active: bool = True) -> tuple[dict, dict[str, dict]]:
        show = self._parse_show(service)
        state = show["ActiveState"]
        main_pid_text = show["MainPID"]
        if not main_pid_text.isdecimal():
            raise RemoteError("service_state_invalid")
        main_pid = int(main_pid_text)
        if require_active and (state != "active" or main_pid <= 1):
            raise RemoteError("service_inactive")
        if not require_active and (state not in ("inactive", "failed") or main_pid != 0):
            raise RemoteError("writers_still_active")
        if not require_active:
            return show, {}
        main = self._process_identity(main_pid)
        expected_name = self._service_expected(service)
        if main["name"] != expected_name:
            raise RemoteError("process_binary_mismatch")
        root = Path(main["executable"]).parent.parent
        try:
            root.relative_to(self.config["release_root"])
        except ValueError:
            raise RemoteError("release_path_mismatch") from None
        if expected_root is not None and root != expected_root:
            raise RemoteError("process_binary_mismatch")
        identities: dict[str, dict] = {str(main_pid): main}
        allowed_names = ({SERVICE_BINS["chain"]} if service == self.config["chain_service"]
                         else {SERVICE_BINS["player"], "oasis7_viewer_live"})
        pids = set(self._cgroup_pids(service))
        pids.add(main_pid)
        for pid in sorted(pids):
            identity = identities.get(str(pid)) or self._process_identity(pid)
            if identity["name"] not in CONTROLLED_PROCESS_NAMES:
                continue
            if identity["name"] not in allowed_names:
                raise RemoteError("process_binary_mismatch")
            if Path(identity["executable"]).parent.parent != root:
                raise RemoteError("process_release_mismatch")
            binary = root / "bin" / identity["name"]
            if not binary.is_file() or _sha256(binary) != identity["sha256"]:
                raise RemoteError("process_hash_mismatch")
            identities[str(pid)] = identity
        return show, identities

    def _snapshot_services(self) -> None:
        self.units = {
            self.config["chain_service"]: self.config["unit_dir"] / self.config["chain_service"],
            self.config["player_service"]: self.config["unit_dir"] / self.config["player_service"],
        }
        for service, path in self.units.items():
            try:
                metadata = path.lstat()
                if not stat.S_ISREG(metadata.st_mode) or (self.enforce_root and metadata.st_uid != 0):
                    raise RemoteError("unit_invalid")
                if metadata.st_mode & 0o022:
                    raise RemoteError("unit_permissions_invalid")
                content = path.read_bytes()
            except OSError:
                raise RemoteError("unit_unavailable") from None
            if not content or len(content) > 1024 * 1024:
                raise RemoteError("unit_invalid")
            self.before_units[service] = content
            self.unit_modes[service] = (metadata.st_uid, metadata.st_gid, stat.S_IMODE(metadata.st_mode))
        for service in (self.config["chain_service"], self.config["player_service"]):
            _, identities = self._service_processes(service)
            main = identities.get(str(int(self._parse_show(service)["MainPID"])))
            if not main:
                raise RemoteError("process_identity_unavailable")
            root = Path(main["executable"]).parent.parent
            if str(root).encode() not in self.before_units[service]:
                raise RemoteError("unit_release_mismatch")
            for identity in identities.values():
                if identity["name"] in {"oasis7_game_launcher", "oasis7_viewer_live", "oasis7_chain_runtime"}:
                    self.old_processes[f"{service}:{identity['pid']}"] = identity
            self.old_roots[service] = root
        try:
            baseline = self.probe_ns["read_world"](self.config["probe"])
        except (KeyError, TypeError, ValueError, OSError):
            raise RemoteError("world_preflight_failed") from None
        self._validate_world(baseline)
        self.world_identity = baseline
        for service, content in self.before_units.items():
            self._atomic_write(self.operation_dir / (service + ".before"), content, 0o600)

    def _validate_world(self, value: object) -> dict:
        if not isinstance(value, dict):
            raise RemoteError("world_response_invalid")
        expected = self.config["probe"]["expected_world"]
        service_key = value.get("service_public_key")
        if (value.get("world_id") != expected.get("world_id")
                or value.get("genesis_digest") != expected.get("genesis_digest")
                or not isinstance(service_key, str)
                or service_key.lower() != self.service_public_key
                or value.get("signature_verified") is not True
                or type(value.get("logical_tick")) is not int or value["logical_tick"] < 0):
            raise RemoteError("world_identity_mismatch")
        return {key: value[key] for key in
                ("world_id", "genesis_digest", "service_public_key", "logical_tick", "signature_verified")}

    def _install(self, request: dict) -> dict:
        if self.phase != "initialized":
            raise RemoteError("protocol_state")
        count = request.get("bytes")
        digest = request.get("sha256")
        if (type(count) is not int or not isinstance(digest, str)
                or count != self.archive_meta["bytes"] or digest != self.archive_meta["sha256"]):
            raise RemoteError("archive_metadata_mismatch")
        self._require_reserve(self.archive_meta["expanded_bytes"])
        staging = self.config["release_root"] / (".staging-" + self.source + "-" + str(os.getpid()))
        try:
            staging.mkdir(mode=0o700)
        except OSError:
            raise RemoteError("staging_unavailable") from None
        os.chmod(staging, 0o700)
        try:
            digests = _extract_streaming_archive(
                self.stdin, staging,
                archive_bytes=self.archive_meta["bytes"],
                expected_digest=self.archive_meta["sha256"],
                expanded_bytes=self.archive_meta["expanded_bytes"],
            )
            _validate_package(staging, digests, self.source)
            if self.enforce_root:
                for path in [staging, *staging.rglob("*")]:
                    if path.lstat().st_uid != 0:
                        raise RemoteError("package_owner_invalid")
            os.chmod(staging, 0o755)
            self._require_reserve()
            if self.release.exists():
                if not self.release.is_dir() or self.release.is_symlink():
                    raise RemoteError("release_conflict")
                existing = _tree_digests(self.release, enforce_root=self.enforce_root)
                if existing != digests:
                    raise RemoteError("release_conflict")
                shutil.rmtree(staging)
            else:
                _fsync_tree_directories(staging)
                os.replace(staging, self.release)
                self._fsync_dir(self.config["release_root"])
            self._require_reserve()
        except BaseException:
            if staging.exists():
                shutil.rmtree(staging, ignore_errors=True)
            raise
        self.phase = "installed"
        return {"release": str(self.release), "source": self.source, "sha256": digest}

    def _begin_backup(self, request: dict) -> dict:
        if self.phase != "installed":
            raise RemoteError("protocol_state")
        limit = request.get("max_bytes")
        if type(limit) is not int or not 1 <= limit <= MAX_BACKUP_BYTES:
            raise RemoteError("backup_limit_invalid")
        self.max_backup_bytes = limit
        self._require_reserve()
        self.stage = "stop_old_services"
        self._systemctl("stop", self.config["player_service"], timeout=SERVICE_TIMEOUT_SECONDS)
        self._systemctl("stop", self.config["chain_service"], timeout=SERVICE_TIMEOUT_SECONDS)
        self._assert_stopped()
        self.phase = "stopped"
        self._json_line({"ok": True, "stream": True, "format": "u32be-chunks-v1", "chunk_max": MAX_CHUNK_BYTES})
        self.stage = "stream_backup"
        relative = [path.relative_to("/").as_posix() for path in self.config["backup_paths"]]
        argv = ["tar", "--create", "--gzip", "--file=-", "--format=pax", "--acls",
                "--xattrs", "--numeric-owner", "--directory=/", "--", *relative]
        process = None
        digest = hashlib.sha256()
        count = 0
        stream_error: RemoteError | None = None
        try:
            process = self.popen(argv, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)
            if process.stdout is None:
                raise RemoteError("backup_stream_failed")
            while True:
                block = process.stdout.read(MAX_CHUNK_BYTES)
                if not block:
                    break
                if count + len(block) > limit:
                    process.terminate()
                    raise RemoteError("backup_limit_exceeded")
                digest.update(block)
                count += len(block)
                self.stdout.write(len(block).to_bytes(4, "big"))
                self.stdout.write(block)
                self.stdout.flush()
            code = process.wait(timeout=TRANSFER_TIMEOUT_SECONDS)
            if code != 0:
                raise RemoteError("backup_stream_failed")
            self._assert_stopped()
        except RemoteError as error:
            stream_error = error
        except (OSError, BrokenPipeError, subprocess.TimeoutExpired):
            stream_error = RemoteError("backup_stream_failed")
        finally:
            if process is not None and process.poll() is None:
                try:
                    process.terminate()
                    process.wait(timeout=10)
                except (OSError, subprocess.TimeoutExpired):
                    try:
                        process.kill()
                        process.wait(timeout=10)
                    except (OSError, subprocess.TimeoutExpired):
                        pass
            try:
                self.stdout.write((0).to_bytes(4, "big"))
                self.stdout.flush()
            except OSError:
                stream_error = RemoteError("transport_lost")
        if stream_error:
            raise stream_error
        self.backup_bytes = count
        self.backup_digest = digest.hexdigest()
        self.phase = "backup_streamed"
        self.stage = "backup_streamed"
        self._json_line({"ok": True, "bytes": count, "sha256": self.backup_digest})
        return {"bytes": count, "sha256": self.backup_digest}

    def _compare_backup(self, request: dict) -> dict:
        if self.phase != "backup_streamed":
            raise RemoteError("protocol_state")
        size, digest = request.get("bytes"), request.get("sha256")
        if (type(size) is not int or not isinstance(digest, str)
                or size != self.backup_bytes or digest != self.backup_digest):
            raise RemoteError("backup_metadata_mismatch")
        self._assert_stopped()
        argv = ["tar", "--compare", "--gzip", "--acls", "--xattrs", "--numeric-owner",
                "--file=-", "--directory=/"]
        try:
            process = self.popen(argv, stdin=subprocess.PIPE, stdout=subprocess.DEVNULL,
                                 stderr=subprocess.DEVNULL)
        except OSError:
            raise RemoteError("backup_compare_failed") from None
        if process.stdin is None:
            raise RemoteError("backup_compare_failed")
        digest_state = hashlib.sha256()
        remaining = size
        write_broken = False
        while remaining:
            block = self.stdin.read(min(MAX_CHUNK_BYTES, remaining))
            if not block:
                try:
                    process.terminate()
                    process.wait(timeout=5)
                except (OSError, subprocess.TimeoutExpired):
                    pass
                raise RemoteError("payload_truncated")
            remaining -= len(block)
            digest_state.update(block)
            if not write_broken:
                try:
                    process.stdin.write(block)
                except (OSError, BrokenPipeError):
                    write_broken = True
        try:
            process.stdin.close()
            code = process.wait(timeout=TRANSFER_TIMEOUT_SECONDS)
        except (OSError, subprocess.TimeoutExpired):
            try:
                process.kill()
                process.wait(timeout=5)
            except (OSError, subprocess.TimeoutExpired):
                pass
            raise RemoteError("backup_compare_failed") from None
        if digest_state.hexdigest() != digest:
            raise RemoteError("backup_digest_mismatch")
        if write_broken or code != 0:
            raise RemoteError("backup_compare_mismatch")
        self._assert_stopped()
        self.backup_verified = True
        self.phase = "backup_verified"
        self._record("backup.json", {"bytes": size, "sha256": digest, "tar_compare_verified": True})
        return {"bytes": size, "sha256": digest, "tar_compare_verified": True}

    def _assert_stopped(self) -> None:
        for service in (self.config["player_service"], self.config["chain_service"]):
            show = self._parse_show(service)
            if show["MainPID"] != "0" or show["ActiveState"] not in ("inactive", "failed"):
                raise RemoteError("writers_still_active")
            root = self._cgroup_dir(service)
            if root.exists():
                dirs = [root, *(path for path in root.rglob("*") if path.is_dir())]
                for directory in dirs:
                    for leaf in ("cgroup.procs", "cgroup.threads"):
                        item = directory / leaf
                        if item.exists() and item.read_text().strip():
                            raise RemoteError("writers_still_active")

    def _atomic_unit(self, service: str, content: bytes) -> None:
        path = self.units[service]
        uid, gid, mode = self.unit_modes[service]
        temp = path.with_name(path.name + ".ecs-preview-tmp-" + str(os.getpid()))
        flags = os.O_WRONLY | os.O_CREAT | os.O_EXCL
        if hasattr(os, "O_NOFOLLOW"):
            flags |= os.O_NOFOLLOW
        try:
            fd = os.open(temp, flags, 0o600)
            try:
                _write_all(fd, content)
                if self.enforce_root:
                    os.fchown(fd, uid, gid)
                os.fchmod(fd, mode)
                os.fsync(fd)
            finally:
                os.close(fd)
            os.replace(temp, path)
            self._fsync_dir(path.parent)
        except OSError:
            try:
                temp.unlink(missing_ok=True)
            except OSError:
                pass
            raise RemoteError("unit_replace_failed") from None

    def _prepare_new_units(self) -> None:
        for service, path in self.units.items():
            before = self.before_units[service]
            old_root = self.old_roots[service]
            prefix = str(old_root).encode() + b"/"
            if prefix not in before:
                raise RemoteError("unit_release_mismatch")
            target = before.replace(prefix, str(self.release).encode() + b"/")
            if target == before or prefix in target:
                raise RemoteError("unit_release_mismatch")
            self.new_units[service] = target

    def _switch(self) -> dict:
        if self.phase != "backup_verified" or not self.backup_verified:
            raise RemoteError("protocol_state")
        self._assert_stopped()
        self._require_reserve()
        self._prepare_new_units()
        for service, path in self.units.items():
            if path.read_bytes() != self.before_units[service]:
                raise RemoteError("unit_changed")
        self.units_changed = True
        self._record("units-changing.json", {"source": self.source, "release": str(self.release)})
        self.stage = "switch_units"
        for service, path in self.units.items():
            if path.read_bytes() != self.before_units[service]:
                raise RemoteError("unit_changed")
            self._atomic_unit(service, self.new_units[service])
        self._systemctl("daemon-reload")
        for service, path in self.units.items():
            if path.read_bytes() != self.new_units[service]:
                raise RemoteError("unit_replace_mismatch")
        self._record("writer-boundary.json", {
            "source": self.source,
            "release": str(self.release),
            "world_rollback": False,
        })
        self.writer_boundary = True
        self.phase = "writer_boundary"
        self.stage = "start_candidate_chain"
        self._systemctl("start", self.config["chain_service"], timeout=SERVICE_START_TIMEOUT_SECONDS)
        self.candidate_processes.update(self._verify_service(self.config["chain_service"], self.release))
        self.stage = "start_candidate_player"
        self._systemctl("start", self.config["player_service"], timeout=SERVICE_START_TIMEOUT_SECONDS)
        self.candidate_processes.update(self._verify_service(self.config["player_service"], self.release))
        self.phase = "switched"
        self.stage = "switched"
        return {"source": self.source, "release": str(self.release), "process_hashes_verified": True}

    def _verify_service(self, service: str, release: Path) -> dict[str, dict]:
        _, identities = self._service_processes(service, expected_root=release)
        prefix = service + ":"
        return {prefix + pid: identity for pid, identity in identities.items()}

    def _verify_all(self) -> dict:
        if self.phase != "switched":
            raise RemoteError("protocol_state")
        self.stage = "verify_processes"
        current = {}
        for service in (self.config["chain_service"], self.config["player_service"]):
            current.update(self._verify_service(service, self.release))
        if not self._same_processes(self.candidate_processes, current):
            raise RemoteError("service_identity_changed")
        self.stage = "verify_signed_world"
        samples = []
        start = self.monotonic()
        for index in range(WORLD_SAMPLE_COUNT):
            target = start + index * WORLD_SAMPLE_INTERVAL_SECONDS
            delay = target - self.monotonic()
            if delay > 0:
                self.sleep(delay)
            try:
                response = self.probe_ns["read_world"](self.config["probe"])
            except (KeyError, TypeError, ValueError, OSError):
                raise RemoteError("world_read_failed") from None
            checked = self._validate_world(response)
            if samples and checked["logical_tick"] <= samples[-1]["logical_tick"]:
                raise RemoteError("world_not_advancing")
            samples.append(checked)
            self._json_line({"event": "progress", "stage": f"signed_read_{index + 1}_of_4"})
        if self.monotonic() - start < 60:
            self.sleep(60 - (self.monotonic() - start))
        self.stage = "verify_public_api"
        self._verify_public_api()
        self.stage = "activate_skill"
        self._activate_skill()
        self.stage = "verify_processes_after_skill"
        after = {}
        for service in (self.config["chain_service"], self.config["player_service"]):
            after.update(self._verify_service(service, self.release))
        if not self._same_processes(current, after):
            raise RemoteError("service_identity_changed")
        self.candidate_processes = after
        self.phase = "verified"
        self.stage = "verified"
        return {
            "source": self.source,
            "release": str(self.release),
            "checks": {"process_hashes": True, "signed_reads": len(samples),
                       "tick_advances": len(samples) - 1, "public_api": True, "skill": True},
            "world": samples[-1],
        }

    @staticmethod
    def _same_processes(left: dict[str, dict], right: dict[str, dict]) -> bool:
        keys = ("pid", "start_ticks", "name", "executable", "sha256")
        return set(left) == set(right) and all(
            all(left[key].get(field) == right[key].get(field) for field in keys)
            for key in left
        )

    def _http(self, method: str, path: str, *, origin: str | None = None) -> tuple[int, bytes, dict]:
        parsed = urlsplit(self.config["public_origin"])
        port = parsed.port or 443
        try:
            connection = http.client.HTTPSConnection(
                parsed.hostname, port, timeout=15, context=ssl.create_default_context())
            headers = {"Host": parsed.netloc, "Connection": "close"}
            if origin is not None:
                headers["Origin"] = origin
            connection.request(method, path, headers=headers)
            response = connection.getresponse()
            body = response.read(65537)
            if len(body) > 65536:
                raise RemoteError("public_api_response_limit")
            result = (response.status, body, {key.lower(): value for key, value in response.getheaders()})
            connection.close()
            return result
        except RemoteError:
            raise
        except (OSError, http.client.HTTPException, ssl.SSLError):
            raise RemoteError("public_api_unavailable") from None

    def _verify_public_api(self) -> None:
        port = self.config["launcher_http_port"]
        path = "/api/public/player-session/admission"
        # The public response is required; also confirm the configured loopback listener.
        self._verify_admission("127.0.0.1", port, path, use_tls=False)
        self._verify_admission(urlsplit(self.config["public_origin"]).hostname,
                               urlsplit(self.config["public_origin"]).port or 443,
                               path, use_tls=True,
                               host_header=urlsplit(self.config["public_origin"]).netloc)
        status, body, _ = self._request_public("HEAD", path)
        if status != 200 or body:
            raise RemoteError("public_api_head_failed")
        status, _, _ = self._request_public("POST", path)
        if status != 405:
            raise RemoteError("public_api_post_failed")
        status, _, _ = self._request_public("GET", path + "/unexpected")
        if status != 404:
            raise RemoteError("public_api_unknown_failed")
        status, _, _ = self._request_public("GET", "/api/public/player-session/issue")
        if status != 404:
            raise RemoteError("public_api_issue_exposed")
        status, _, _ = self._request_public("GET", path, origin="https://invalid.example")
        if status != 403:
            raise RemoteError("public_api_origin_failed")

    def _verify_admission(self, host: str, port: int, path: str, *, use_tls: bool,
                          host_header: str | None = None) -> None:
        try:
            if use_tls:
                connection = http.client.HTTPSConnection(host, port, timeout=15,
                                                         context=ssl.create_default_context())
            else:
                connection = http.client.HTTPConnection(host, port, timeout=15)
            connection.request("GET", path, headers={"Host": host_header or host,
                                                     "Connection": "close"})
            response = connection.getresponse()
            body = response.read(65537)
            status = response.status
            connection.close()
            if status != 200 or len(body) > 65536:
                raise RemoteError("public_api_admission_failed")
            payload = json.loads(body.decode("utf-8"))
        except RemoteError:
            raise
        except (OSError, http.client.HTTPException, UnicodeError, json.JSONDecodeError):
            raise RemoteError("public_api_admission_failed") from None
        if (not isinstance(payload, dict) or payload.get("ok") is not True
                or payload.get("deployment_mode") != "hosted_public_join"
                or not isinstance(payload.get("admission"), dict)
                or payload["admission"].get("runtime_probe_status") != "ok"):
            raise RemoteError("public_api_admission_failed")

    def _request_public(self, method: str, path: str, *, origin: str | None = None) -> tuple[int, bytes, dict]:
        parsed = urlsplit(self.config["public_origin"])
        try:
            connection = http.client.HTTPSConnection(
                parsed.hostname, parsed.port or 443, timeout=15,
                context=ssl.create_default_context())
            headers = {"Host": parsed.netloc, "Connection": "close"}
            if origin:
                headers["Origin"] = origin
            connection.request(method, path, headers=headers)
            response = connection.getresponse()
            body = response.read(65537) if method != "HEAD" else response.read()
            if len(body) > 65536:
                raise RemoteError("public_api_response_limit")
            result = (response.status, body, {key.lower(): value for key, value in response.getheaders()})
            connection.close()
            return result
        except RemoteError:
            raise
        except (OSError, http.client.HTTPException, ssl.SSLError):
            raise RemoteError("public_api_unavailable") from None

    def _activate_skill(self) -> None:
        script = self.release / "deployment" / "ecs-skill-release.py"
        if not script.is_file() or script.is_symlink():
            raise RemoteError("skill_tool_missing")
        lock = self.config["deploy_lock"]
        self._release_deploy_lock()
        try:
            self._run([
                "python3", str(script), "activate", "--release", str(self.release),
                "--expected-source", self.source, "--origin", self.config["public_origin"],
                "--player-service", self.config["player_service"],
                "--nginx-include", str(self.config["nginx_include"]), "--lock-file", str(lock),
            ], timeout=SKILL_TIMEOUT_SECONDS)
            self._reacquire_deploy_lock()
            self._run([
                "python3", str(script), "verify", "--release", str(self.release),
                "--expected-source", self.source, "--origin", self.config["public_origin"],
            ], timeout=SKILL_TIMEOUT_SECONDS)
        except BaseException:
            if not self.deploy_lock_held:
                try:
                    self._reacquire_deploy_lock()
                except RemoteError:
                    pass
            raise

    def _fd_is_deploy_lock(self, fd: int) -> bool:
        return any(path == self.config["deploy_lock"] and held_fd == fd
                   for path, held_fd in self.locks)

    def _accept(self) -> dict:
        if self.phase != "verified" or not self.backup_verified:
            raise RemoteError("protocol_state")
        self._record("accepted.json", {
            "source": self.source,
            "run_id": self.run_id,
            "release": str(self.release),
            "backup_bytes": self.backup_bytes,
            "backup_sha256": self.backup_digest,
            "accepted_unix": int(time.time()),
            "old_release_roots": {key: str(value) for key, value in self.old_roots.items()},
            "world_identity": self.world_identity,
        })
        self.accepted = True
        self.phase = "accepted"
        self.terminal = True
        self._release_locks()
        return {"source": self.source, "release": str(self.release), "accepted": True}

    def _abort(self, reason: str) -> dict:
        if self.accepted:
            return {"accepted": True}
        if not self.initialized:
            self._release_locks()
            self.terminal = True
            return {"aborted": True, "restored": False}
        if self.writer_boundary:
            result = self._fence_candidate(reason)
        else:
            result = self._restore_old(reason)
        self.phase = "aborted"
        self.terminal = True
        self._release_locks()
        return result

    def _restore_old(self, reason: str) -> dict:
        try:
            if not self.deploy_lock_held:
                raise RemoteError("lock_unavailable")
            if self.units_changed:
                for service, path in self.units.items():
                    current = path.read_bytes()
                    if current not in (self.before_units[service], self.new_units.get(service)):
                        raise RemoteError("unit_identity_changed")
                for service, path in self.units.items():
                    if path.read_bytes() != self.before_units[service]:
                        self._atomic_unit(service, self.before_units[service])
                for service, path in self.units.items():
                    if path.read_bytes() != self.before_units[service]:
                        raise RemoteError("unit_restore_mismatch")
                self._systemctl("daemon-reload")
            elif self.units and any(path.read_bytes() != self.before_units[service]
                                    for service, path in self.units.items()):
                raise RemoteError("unit_identity_changed")
            for service in (self.config["chain_service"], self.config["player_service"]):
                show = self._parse_show(service)
                if show["MainPID"] == "0":
                    self._systemctl("start", service, timeout=SERVICE_START_TIMEOUT_SECONDS)
                self._verify_service(service, self.old_roots[service])
            try:
                world = self._validate_world(self.probe_ns["read_world"](self.config["probe"]))
            except BaseException:
                raise RemoteError("rollback_world_unverified") from None
            if not self._same_world_identity(self.world_identity, world):
                raise RemoteError("rollback_world_identity_changed")
            self._record("prewriter-restored.json", {
                "reason": reason, "old_processes_verified": True,
                "world_identity_verified": True, "source": self.source,
            })
            return {"aborted": True, "restored": True}
        except BaseException as error:
            self._safe_record("prewriter-restore-failed.json", {
                "reason": reason, "error": self._error_code(error),
                "writer_boundary": False,
            })
            raise RemoteError("prewriter_restore_failed") from None

    @staticmethod
    def _same_world_identity(left: dict | None, right: dict) -> bool:
        if not left:
            return False
        keys = ("world_id", "genesis_digest", "service_public_key")
        return all(left.get(key) == right.get(key) for key in keys)

    def _fence_candidate(self, reason: str) -> dict:
        try:
            if not self.deploy_lock_held:
                raise RemoteError("lock_unavailable")
            for service, path in self.units.items():
                if path.read_bytes() != self.new_units.get(service):
                    raise RemoteError("service_identity_changed")
            current_main: dict[str, dict] = {}
            for service in (self.config["chain_service"], self.config["player_service"]):
                show = self._parse_show(service)
                if show["MainPID"] == "0":
                    continue
                expected_name = self._service_expected(service)
                main = self._process_identity(int(show["MainPID"]))
                if main["name"] != expected_name or main["executable"] != str(self.release / "bin" / expected_name):
                    raise RemoteError("service_identity_changed")
                old = self.old_processes.get(f"{service}:{main['pid']}")
                if old and old["start_ticks"] == main["start_ticks"]:
                    raise RemoteError("service_identity_changed")
                saved_main = next((identity for key, identity in self.candidate_processes.items()
                                   if key.startswith(service + ":") and identity["name"] == expected_name), None)
                if saved_main and (saved_main["pid"] != main["pid"]
                                   or saved_main["start_ticks"] != main["start_ticks"]):
                    raise RemoteError("service_identity_changed")
                for pid in self._cgroup_pids(service):
                    identity = self._process_identity(pid)
                    if identity["name"] in CONTROLLED_PROCESS_NAMES:
                        if (identity["name"] not in {expected_name, "oasis7_viewer_live"}
                                or Path(identity["executable"]).parent.parent != self.release):
                            raise RemoteError("service_identity_changed")
                current_main[service] = main
            errors = []
            for service in (self.config["player_service"], self.config["chain_service"]):
                try:
                    show = self._parse_show(service)
                    if show["MainPID"] != "0":
                        self._systemctl("stop", service, timeout=SERVICE_TIMEOUT_SECONDS)
                except BaseException as error:
                    errors.append(self._error_code(error))
            self._assert_stopped()
            self._safe_record("failed-fenced.json", {
                "reason": reason, "source": self.source, "writers_stopped_verified": True,
                "old_binary_rollback": False, "stop_errors": errors,
            })
            return {"aborted": True, "fenced": True, "old_binary_rollback": False}
        except BaseException as error:
            self._safe_record("fence-failed.json", {
                "reason": reason, "source": self.source,
                "error": self._error_code(error), "writers_stopped_verified": False,
                "identity_changed": self._error_code(error) == "service_identity_changed",
            })
            raise RemoteError("fence_failed") from None

    def _safe_record(self, name: str, data: dict) -> None:
        try:
            if self.operation_dir is not None and not (self.operation_dir / name).exists():
                self._record(name, data)
        except BaseException:
            pass

    @staticmethod
    def _error_code(error: BaseException) -> str:
        return error.code if isinstance(error, RemoteError) else "internal_error"

    def _dispatch(self, request: dict) -> tuple[dict, bool]:
        op = request.get("op")
        if op == "init":
            return self._init(request), False
        if op == "install":
            if set(request) != {"op", "bytes", "sha256"}:
                raise RemoteError("request_invalid")
            return self._install(request), False
        if op == "backup":
            if set(request) != {"op", "max_bytes"}:
                raise RemoteError("request_invalid")
            return self._begin_backup(request), False
        if op == "compare":
            if set(request) != {"op", "bytes", "sha256"}:
                raise RemoteError("request_invalid")
            return self._compare_backup(request), False
        if op == "switch":
            if set(request) != {"op"}:
                raise RemoteError("request_invalid")
            return self._switch(), False
        if op == "verify":
            if set(request) != {"op"}:
                raise RemoteError("request_invalid")
            return self._verify_all(), False
        if op == "accept":
            if set(request) != {"op"}:
                raise RemoteError("request_invalid")
            return self._accept(), True
        if op == "abort":
            if set(request) - {"op", "reason"}:
                raise RemoteError("request_invalid")
            reason = request.get("reason", "controller_abort")
            if not isinstance(reason, str) or not re.fullmatch(r"[a-z0-9_]{1,64}", reason):
                reason = "controller_abort"
            return self._abort(reason), True
        raise RemoteError("operation_invalid")

    def _bootstrap_probe(self) -> None:
        request = self._read_json_line(MAX_BOOTSTRAP_BYTES)
        if request is None or set(request) != {"probe_source"}:
            raise RemoteError("bootstrap_invalid")
        source = request["probe_source"]
        if not isinstance(source, str) or not source or len(source.encode("utf-8")) > MAX_BOOTSTRAP_BYTES:
            raise RemoteError("bootstrap_invalid")
        namespace = {"__name__": "ecs_preview_acceptance_probe"}
        try:
            exec(compile(source, "<ecs-preview-probe>", "exec"), namespace, namespace)
        except BaseException:
            raise RemoteError("bootstrap_invalid") from None
        if not callable(namespace.get("preflight")) or not callable(namespace.get("read_world")):
            raise RemoteError("bootstrap_invalid")
        self.probe_ns = namespace

    def serve(self) -> int:
        os.umask(0o077)
        try:
            self._bootstrap_probe()
        except BaseException as error:
            code = self._error_code(error)
            self._stderr("remote_deploy_error=" + code)
            try:
                self._json_line({"ok": False, "error": code})
            except RemoteError:
                pass
            return 1

        def interrupt(_signum, _frame):
            raise RemoteError("transport_lost")

        old_handlers = {}
        for sig in (signal.SIGHUP, signal.SIGTERM):
            try:
                old_handlers[sig] = signal.signal(sig, interrupt)
            except (ValueError, OSError):
                pass
        try:
            while not self.terminal:
                request = self._read_json_line()
                if request is None:
                    if self.initialized and not self.accepted:
                        try:
                            self._abort("transport_eof")
                        except RemoteError:
                            pass
                    break
                try:
                    result, terminal = self._dispatch(request)
                    if request.get("op") not in ("backup",):
                        self._json_line({"ok": True, **result})
                    if terminal:
                        self.terminal = True
                except BaseException as error:
                    code = self._error_code(error)
                    self._stderr("remote_deploy_stage=" + self.stage + " error=" + code)
                    if self.initialized and not self.accepted:
                        try:
                            self._abort(code)
                        except RemoteError as abort_error:
                            code = abort_error.code
                    try:
                        self._json_line({"ok": False, "error": code})
                    except RemoteError:
                        pass
                    self.terminal = True
                    break
        except RemoteError as error:
            self._stderr("remote_deploy_error=" + error.code)
            if self.initialized and not self.accepted:
                try:
                    self._abort(error.code)
                except RemoteError:
                    pass
            try:
                self._json_line({"ok": False, "error": error.code})
            except RemoteError:
                pass
            self.terminal = True
        finally:
            self._release_locks()
            for sig, handler in old_handlers.items():
                try:
                    signal.signal(sig, handler)
                except (ValueError, OSError):
                    pass
        return 0 if self.accepted else 1


def _fsync_tree_directories(root: Path) -> None:
    directories = [path for path in root.rglob("*") if path.is_dir()]
    directories.sort(key=lambda path: len(path.parts), reverse=True)
    for path in [*directories, root]:
        Engine._fsync_dir(path)


def _tree_digests(root: Path, *, enforce_root: bool = False) -> dict[str, str]:
    result = {}
    try:
        root_info = root.lstat()
    except OSError:
        raise RemoteError("release_invalid") from None
    if not stat.S_ISDIR(root_info.st_mode) or stat.S_IMODE(root_info.st_mode) != 0o755:
        raise RemoteError("release_permissions_invalid")
    paths = sorted(root.rglob("*"), key=lambda path: path.relative_to(root).as_posix())
    for path in paths:
        try:
            info = path.lstat()
        except OSError:
            raise RemoteError("release_invalid") from None
        if enforce_root and info.st_uid != 0:
            raise RemoteError("release_owner_invalid")
        if stat.S_ISLNK(info.st_mode):
            raise RemoteError("release_symlink")
        relative = path.relative_to(root).as_posix()
        if stat.S_ISDIR(info.st_mode):
            if stat.S_IMODE(info.st_mode) != 0o755:
                raise RemoteError("release_permissions_invalid")
        elif stat.S_ISREG(info.st_mode):
            expected_mode = 0o755 if relative.startswith("bin/") else 0o644
            if stat.S_IMODE(info.st_mode) != expected_mode:
                raise RemoteError("release_permissions_invalid")
            result[relative] = _sha256(path)
        else:
            raise RemoteError("release_member_type")
    if enforce_root and root_info.st_uid != 0:
        raise RemoteError("release_owner_invalid")
    return result


def serve() -> int:
    return Engine().serve()


if __name__ == "__main__":
    raise SystemExit(serve())
