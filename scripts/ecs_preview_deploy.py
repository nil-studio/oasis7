"""Local artifact verification and the single-session Preview deployment client."""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re
import select
import shlex
import shutil
import stat
import struct
import subprocess
import sys
import tempfile
import time
from urllib.parse import urlsplit

from safe_git_archive import ArchiveLimits, ArchivePolicy, extract_archive

RESERVE = 528 * 1024**2
CHUNK = 1024**2
BINS = ("oasis7_chain_runtime", "oasis7_game_launcher", "oasis7_viewer_live", "oasis7_game_client")
LOCAL_FIELDS = {"ssh_host", "repository", "local_backup_root", "phase_timeout_seconds", "max_backup_bytes"}
REMOTE_FIELDS = {"release_root", "ops_root", "deploy_lock", "controller_lock", "chain_service", "player_service",
                 "unit_dir", "backup_paths", "public_origin", "launcher_http_port", "nginx_include", "probe"}


def digest(path):
    h = hashlib.sha256()
    with Path(path).open("rb") as stream:
        for block in iter(lambda: stream.read(CHUNK), b""):
            h.update(block)
    return h.hexdigest()


def gh_json(*args):
    env = dict(os.environ)
    env.pop("SSHPASS", None)
    result = subprocess.run(["gh", *args], check=True, stdout=subprocess.PIPE, env=env, text=True, timeout=120)
    return json.loads(result.stdout)


def profile(path):
    path = Path(path)
    info = path.lstat()
    if not stat.S_ISREG(info.st_mode) or info.st_mode & 0o022 or info.st_uid != os.getuid():
        raise ValueError("profile must be an owned, non-writable regular file")
    cfg = json.loads(path.read_text())
    if not isinstance(cfg, dict) or set(cfg) - LOCAL_FIELDS - REMOTE_FIELDS:
        raise ValueError("unknown deployment profile fields")
    required = LOCAL_FIELDS - {"repository", "phase_timeout_seconds", "max_backup_bytes"}
    required |= REMOTE_FIELDS - {"controller_lock", "unit_dir"}
    if required - set(cfg):
        raise ValueError("deployment profile fields missing")
    host = cfg["ssh_host"]
    if not isinstance(host, str) or host.startswith("-") or not re.fullmatch(r"[A-Za-z0-9_.@:\[\]-]+", host):
        raise ValueError("invalid SSH target")
    repo = cfg.get("repository", "nil-studio/oasis7")
    if not re.fullmatch(r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+", repo):
        raise ValueError("invalid repository")
    cfg["repository"] = repo
    for key in ("release_root", "ops_root", "deploy_lock", "nginx_include", "unit_dir", "controller_lock"):
        if key in cfg and (not isinstance(cfg[key], str) or not cfg[key].startswith("/") or "\x00" in cfg[key]):
            raise ValueError("invalid absolute deployment path")
    for key in ("chain_service", "player_service"):
        if not isinstance(cfg[key], str) or not re.fullmatch(r"[A-Za-z0-9_.@:-]+\.service", cfg[key]):
            raise ValueError("invalid service name")
    if (not isinstance(cfg["backup_paths"], list) or not cfg["backup_paths"]
            or any(not isinstance(path, str) or not path.startswith("/") or "\x00" in path for path in cfg["backup_paths"])):
        raise ValueError("invalid backup paths")
    if type(cfg["launcher_http_port"]) is not int or not 1 <= cfg["launcher_http_port"] <= 65535:
        raise ValueError("invalid launcher HTTP port")
    if not isinstance(cfg["public_origin"], str) or not isinstance(cfg["local_backup_root"], str):
        raise ValueError("invalid origin or local backup path")
    probe_keys = {"expected_world", "service_public_key", "reader_public_key", "reader_key_file", "world_service_url", "python_path", "reader_uid"}
    if not isinstance(cfg["probe"], dict) or set(cfg["probe"]) - probe_keys:
        raise ValueError("invalid probe configuration; credentials are file references only")
    origin = urlsplit(cfg["public_origin"])
    if origin.scheme != "https" or not origin.hostname or origin.username or origin.password or origin.path not in ("", "/") or origin.query or origin.fragment:
        raise ValueError("public_origin must be a credential-free HTTPS origin")
    cfg["public_origin"] = cfg["public_origin"].rstrip("/")
    cfg.setdefault("phase_timeout_seconds", 1800)
    cfg.setdefault("max_backup_bytes", 32 * 1024**3)
    if type(cfg["phase_timeout_seconds"]) is not int or not 120 <= cfg["phase_timeout_seconds"] <= 7200:
        raise ValueError("invalid phase timeout")
    if type(cfg["max_backup_bytes"]) is not int or not RESERVE <= cfg["max_backup_bytes"] <= 128 * 1024**3:
        raise ValueError("invalid backup bound")
    cfg["local_backup_root"] = str(Path(cfg["local_backup_root"]).expanduser().absolute())
    return cfg


def selected_run(repo, run_id):
    run = gh_json("api", f"repos/{repo}/actions/runs/{run_id}")
    source = run.get("head_sha", "")
    if (run.get("status") != "completed" or run.get("conclusion") != "success"
            or run.get("head_branch") != "main" or run.get("path") != ".github/workflows/ecs-packages.yml"
            or not re.fullmatch(r"[0-9a-f]{40}", source)):
        raise ValueError("select a successful main ECS Server Package run")
    pages = gh_json("api", f"repos/{repo}/actions/workflows/rust.yml/runs?branch=main&head_sha={source}&event=push&per_page=100", "--paginate", "--slurp")
    runs = [item for page in pages for item in page["workflow_runs"]
            if item.get("head_sha") == source and item.get("head_branch") == "main"
            and item.get("path") == ".github/workflows/rust.yml" and item.get("event") == "push"]
    if not runs:
        raise ValueError("same-source main Rust workflow is missing")
    current = max(runs, key=lambda item: item["id"])
    if current.get("status") != "completed" or current.get("conclusion") != "success":
        raise ValueError("same-source main Rust workflow has not passed")
    jobs = gh_json("api", f"repos/{repo}/actions/runs/{current['id']}/jobs?filter=latest&per_page=100", "--paginate", "--slurp")
    gates = [job for page in jobs for job in page["jobs"] if job.get("name") == "required-gate"]
    if len(gates) != 1 or gates[0].get("status") != "completed" or gates[0].get("conclusion") != "success":
        raise ValueError("same-source Rust required-gate has not passed")
    return source


def package_info(directory, source):
    archives = list(Path(directory).glob("*.tar.gz"))
    if len(archives) != 1:
        raise ValueError("expected one final ECS archive")
    archive = archives[0]
    checksum = archive.with_name(archive.name + ".sha256").read_text().strip().split()
    actual = digest(archive)
    if len(checksum) != 2 or checksum[0] != actual or checksum[1].lstrip("*") != archive.name:
        raise ValueError("ECS archive checksum mismatch")
    with tempfile.TemporaryDirectory(prefix="oasis7-ecs-verify-") as temp:
        root = Path(temp) / "package"
        extract_archive(archive, root, policy=ArchivePolicy.AUTHORITY,
                        limits=ArchiveLimits(archive_bytes=2 * 1024**3, member_bytes=512 * 1024**2,
                                             expanded_bytes=4 * 1024**3, seconds=300))
        build = dict(line.split("=", 1) for line in (root / "BUILDINFO").read_text().splitlines() if "=" in line)
        expected = {"source": source, "platform": "linux-x64", "profile": "ecs", "kind": "ecs-server"}
        if any(build.get(key) != value for key, value in expected.items()):
            raise ValueError("ECS BUILDINFO identity mismatch")
        manifest = json.loads((root / "manifest.json").read_text())
        if any(manifest.get(key) != value for key, value in expected.items() if key != "kind") or manifest.get("binaries") != list(BINS):
            raise ValueError("ECS manifest identity mismatch")
        entries = {}
        for line in (root / "SHA256SUMS").read_text().splitlines():
            match = re.fullmatch(r"([0-9a-f]{64})  (.+)", line)
            if not match:
                raise ValueError("invalid package checksum record")
            name = match[2]
            path = PurePosixPath(name)
            if path.is_absolute() or ".." in path.parts or name in entries or name != path.as_posix():
                raise ValueError("unsafe or duplicate checksum path")
            entries[name] = match[1]
        files = {path.relative_to(root).as_posix(): path for path in root.rglob("*") if path.is_file() and path != root / "SHA256SUMS"}
        if set(entries) != set(files) or any(digest(path) != entries[name] for name, path in files.items()):
            raise ValueError("package checksum coverage mismatch")
        for binary in BINS:
            if not os.access(root / "bin" / binary, os.X_OK):
                raise ValueError("non-executable ECS binary")
        if not (root / "deployment/ecs-skill-release.py").is_file() or not (root / "web-dist/index.html").is_file():
            raise ValueError("incomplete ECS deployment package")
        expanded = sum(path.stat().st_size for path in root.rglob("*") if path.is_file())
    return archive, {"bytes": archive.stat().st_size, "sha256": actual, "expanded_bytes": expanded}


class Session:
    def __init__(self, cfg):
        engine = Path(__file__).with_name("ecs_preview_remote.py").read_text()
        probe = Path(__file__).with_name("ecs_preview_probe.py").read_text()
        command = ["ssh", "-o", "StrictHostKeyChecking=yes", "-o", "ConnectTimeout=15", "-o", "NumberOfPasswordPrompts=1",
                   "-o", "ServerAliveInterval=15", "-o", "ServerAliveCountMax=3", cfg["ssh_host"], shlex.join(["python3", "-u", "-c", engine])]
        if os.environ.get("SSHPASS"):
            if not shutil.which("sshpass"):
                raise ValueError("SSHPASS requires the installed sshpass utility")
            command = ["sshpass", "-e", *command]
        self.process = subprocess.Popen(command, stdin=subprocess.PIPE, stdout=subprocess.PIPE, bufsize=0)
        self.buffer = bytearray()
        self.timeout = cfg["phase_timeout_seconds"]
        self.deadline = time.monotonic() + self.timeout
        for stream in (self.process.stdin, self.process.stdout):
            os.set_blocking(stream.fileno(), False)
        self.send({"probe_source": probe})

    def write(self, data):
        deadline = self.deadline
        while data:
            if not select.select([], [self.process.stdin], [], max(0, deadline - time.monotonic()))[1]:
                raise TimeoutError("SSH write deadline")
            try:
                size = os.write(self.process.stdin.fileno(), data[:65536])
            except BlockingIOError:
                continue
            if size <= 0:
                raise EOFError("SSH input closed")
            data = data[size:]

    def send(self, frame):
        if "op" in frame:
            self.deadline = time.monotonic() + self.timeout
        self.write(json.dumps(frame, separators=(",", ":")).encode() + b"\n")

    def read(self, count):
        deadline = self.deadline
        while len(self.buffer) < count:
            if not select.select([self.process.stdout], [], [], max(0, deadline - time.monotonic()))[0]:
                raise TimeoutError("SSH read deadline")
            block = os.read(self.process.stdout.fileno(), min(CHUNK, max(count - len(self.buffer), 4096)))
            if not block:
                raise EOFError("SSH ended before deployment acknowledgement")
            self.buffer.extend(block)
        result = bytes(self.buffer[:count])
        del self.buffer[:count]
        return result

    def response(self):
        deadline = self.deadline
        while True:
            line = bytearray()
            while not line.endswith(b"\n"):
                if len(line) >= 65536 or time.monotonic() > deadline:
                    raise ValueError("invalid SSH protocol frame")
                line.extend(self.read(1))
            result = json.loads(line)
            if result.get("event") == "progress":
                print("deploy: " + str(result.get("stage", "working")), file=sys.stderr, flush=True)
                continue
            if result.get("ok") is not True:
                raise RuntimeError("remote deployment failed: " + str(result.get("error", "unknown")))
            return result

    def call(self, op, **fields):
        self.send({"op": op, **fields})
        return self.response()

    def upload(self, path):
        with Path(path).open("rb") as stream:
            for block in iter(lambda: stream.read(CHUNK), b""):
                self.write(block)

    def close(self, accepted=False):
        # EOF is a remote abort/fence, never an implicit successful deployment.
        self.process.stdin.close()
        try:
            try:
                code = self.process.wait(timeout=240)
            except subprocess.TimeoutExpired:
                self.process.terminate()
                try:
                    self.process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    self.process.kill()
                    self.process.wait(timeout=5)
                raise RuntimeError("remote cleanup not acknowledged; inspect target before retrying")
            if accepted and code:
                raise RuntimeError("remote completion exit was unsuccessful")
        finally:
            self.process.stdout.close()



def execute(cfg, run_id, source, archive, info, *, session_factory=Session):
    backup_root = Path(cfg["local_backup_root"])
    backup_root.mkdir(mode=0o700, parents=True, exist_ok=True)
    if backup_root.is_symlink() or backup_root.stat().st_mode & 0o077 or backup_root.stat().st_uid != os.getuid():
        raise ValueError("local backup root must be owned and mode 0700")
    attempt = backup_root / f"ecs-{run_id}-{source[:12]}-{time.time_ns()}"
    attempt.mkdir(mode=0o700)
    remote = {key: value for key, value in cfg.items() if key in REMOTE_FIELDS}
    session = session_factory(cfg)
    accepted = False
    try:
        session.call("init", config=remote, source=source, run_id=run_id, archive=info)
        session.send({"op": "install", "bytes": info["bytes"], "sha256": info["sha256"]})
        session.upload(archive)
        session.response()
        bound = min(cfg["max_backup_bytes"], shutil.disk_usage(backup_root).free - RESERVE)
        if bound < RESERVE:
            raise ValueError("insufficient local backup reserve")
        header = session.call("backup", max_bytes=bound)
        if header.get("stream") is not True:
            raise ValueError("missing backup stream")
        backup = attempt / "pre-upgrade.tar.gz"
        h = hashlib.sha256()
        size = 0
        with backup.open("xb") as output:
            os.chmod(backup, 0o600)
            while True:
                count = struct.unpack(">I", session.read(4))[0]
                if not count:
                    break
                if count > CHUNK or size + count > bound:
                    raise ValueError("backup stream exceeds bound")
                block = session.read(count)
                output.write(block)
                h.update(block)
                size += count
            output.flush()
            os.fsync(output.fileno())
        receipt = session.response()
        sha = h.hexdigest()
        if receipt.get("bytes") != size or receipt.get("sha256") != sha or size == 0:
            raise ValueError("backup transfer mismatch")
        session.send({"op": "compare", "bytes": size, "sha256": sha})
        session.upload(backup)
        session.response()
        session.call("switch")
        checks = session.call("verify")
        result = {"source": source, "run_id": run_id, "archive_sha256": info["sha256"],
                  "backup": {"path": str(backup), "bytes": size, "sha256": sha}, "verification": checks}
        # Keep the actual backup identity durable before asking the target to commit.
        receipt_path = attempt / "deployment.json"
        receipt_path.write_text(json.dumps(result, indent=2) + "\n")
        receipt_path.chmod(0o600)
        with receipt_path.open("rb") as saved:
            os.fsync(saved.fileno())
        result["acceptance"] = session.call("accept")
        accepted = True
        try:
            receipt_path.write_text(json.dumps(result, indent=2) + "\n")
        except OSError:
            print("deployment accepted; local final receipt update failed", file=sys.stderr)
        return result
    finally:
        session.close(accepted=accepted)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--config", type=Path, required=True, help="one-time local deployment profile; no password or seed")
    parser.add_argument("--run", type=int, required=True, help="successful main ECS Server Package run ID")
    args = parser.parse_args(argv)
    os.umask(0o077)
    try:
        if args.run <= 0:
            raise ValueError("invalid run ID")
        cfg = profile(args.config)
        source = selected_run(cfg["repository"], args.run)
        with tempfile.TemporaryDirectory(prefix="oasis7-ecs-artifact-") as temp:
            env = dict(os.environ)
            env.pop("SSHPASS", None)
            subprocess.run(["gh", "run", "download", str(args.run), "--repo", cfg["repository"],
                            "--name", "oasis7-ecs-linux-x64", "--dir", temp], check=True, env=env, timeout=600)
            archive, info = package_info(temp, source)
            print(f"deploy: verified source {source}; opening one SSH session", file=sys.stderr, flush=True)
            result = execute(cfg, args.run, source, archive, info)
        print(json.dumps(result, indent=2))
    except (ValueError, RuntimeError, OSError, subprocess.SubprocessError, EOFError) as error:
        # Do not dump profile, subprocess argv, environment, or credentials.
        print(f"deployment incomplete: {type(error).__name__}: {error if isinstance(error, (ValueError, RuntimeError, EOFError)) else 'inspect the last completed stage'}", file=sys.stderr)
        raise SystemExit(1)
