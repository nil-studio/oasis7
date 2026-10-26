"""Single-host Preview update coordinator. No world initialization or data writes."""
import argparse
import contextlib
import fcntl
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import platform
import re
import shutil
import secrets
import subprocess
import sys
import tarfile
import tempfile
import time


def module(name):
    spec = importlib.util.spec_from_file_location(name.replace("-", "_"), Path(__file__).with_name(name + ".py"))
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


def digest(path):
    path = Path(path)
    if path.is_file():
        h = hashlib.sha256()
        with path.open("rb") as stream:
            for block in iter(lambda: stream.read(1024 * 1024), b""):
                h.update(block)
        return h.hexdigest()
    h = hashlib.sha256()
    for item in sorted(path.rglob("*")):
        if item.is_symlink():
            raise ValueError("symbolic component payload")
        if item.is_file():
            h.update((item.relative_to(path).as_posix() + "\0" + digest(item) + "\n").encode())
    return h.hexdigest()


def trusted_path(path):
    path = Path(path)
    for item in (path, *path.parents):
        info = item.lstat()
        if item.is_symlink() or info.st_uid != 0 or info.st_mode & 0o022:
            raise ValueError("configuration/control paths must be root-owned and not writable by others: " + str(item))


def load_config(path, trusted=True):
    if trusted:
        trusted_path(path)
    config = json.loads(Path(path).read_text())
    required = {"schema_version", "endpoint", "bucket", "prefix", "channel_key", "releases_dir", "state_dir", "lock_path", "compatibility_file", "verifier_command", "world_identity", "components", "skill"}
    if set(config) != required or config["schema_version"] != 1:
        raise ValueError("invalid updater configuration schema")
    if set(config["components"]) != {"chain", "player", "web", "skill"}:
        raise ValueError("four independent component bindings required")
    paths = []
    for name, info in config["components"].items():
        if not {"path", "relative_path"} <= set(info) or set(info) - {"path", "relative_path", "service"}:
            raise ValueError("invalid component configuration")
        rel = Path(info["relative_path"])
        if rel.is_absolute() or ".." in rel.parts:
            raise ValueError("unsafe component relative path")
        path = Path(info["path"])
        if not path.is_absolute():
            raise ValueError("component paths must be absolute")
        if info.get("service") is not None and not re.fullmatch(r"[A-Za-z0-9_.@-]+\.service", info["service"]):
            raise ValueError("invalid systemd service name")
        paths.append(path)
    if len(set(paths)) != len(paths) or any(a in b.parents for a in paths for b in paths if a != b):
        raise ValueError("component bindings must be independent")
    for key in ("releases_dir", "state_dir", "lock_path", "compatibility_file"):
        if not Path(config[key]).is_absolute():
            raise ValueError("absolute control paths required")
    command = config["verifier_command"]
    if not isinstance(command, list) or not command or not Path(command[0]).is_absolute():
        raise ValueError("fixed absolute verifier command required")
    if trusted:
        trusted_path(Path(command[0]).parent)
        trusted_path(Path(command[0]).resolve(strict=True))
        trusted_path(config["compatibility_file"])
        for key in ("state_dir", "releases_dir", "lock_path"):
            control = Path(config[key])
            while not control.exists():
                control = control.parent
            trusted_path(control)
        for info in config["components"].values():
            trusted_path(Path(info["path"]).parent)
    return config


def preflight(config, runner=subprocess.run):
    for info in config["components"].values():
        path = Path(info["path"])
        if not path.is_symlink() or not path.exists():
            raise ValueError("bootstrap existing independent component symlinks before installation")
    if platform.system() != "Linux" or platform.machine() not in ("x86_64", "AMD64"):
        raise ValueError("unsupported host platform")
    for name in ("chain", "player"):
        info = config["components"][name]
        if not info.get("service"):
            raise ValueError("chain/player service required")
        shown = runner(["systemctl", "show", info["service"], "-p", "ExecStart", "-p", "MainPID"], check=True, capture_output=True, text=True, timeout=10).stdout
        if not re.search(r"(?:^|[ {;])path=" + re.escape(info["path"]) + r"(?:[ ;}])", shown):
            raise ValueError("service ExecStart does not bind independent component path")
        pid = next((line.split("=", 1)[1] for line in shown.splitlines() if line.startswith("MainPID=")), "")
        if not pid.isdigit() or int(pid) <= 0 or digest(Path("/proc") / pid / "exe") != digest(Path(info["path"])):
            raise ValueError("running service bytes do not match component binding")
    # No service, world, or identity mutations occur here.
    return True


def atomic_json(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.NamedTemporaryFile(dir=path.parent, mode="w", delete=False) as stream:
        tmp = Path(stream.name)
        json.dump(value, stream, sort_keys=True, indent=2)
        stream.flush()
        os.fsync(stream.fileno())
    os.replace(tmp, path)
    fd = os.open(path.parent, os.O_RDONLY)
    try:
        os.fsync(fd)
    finally:
        os.close(fd)


def binding(path, source=None):
    target = Path(path).resolve(strict=True)
    root = target
    while not (root / "manifest.json").is_file():
        if root == root.parent:
            raise ValueError("component does not belong to a verified ECS release")
        root = root.parent
    identity = json.loads((root / "manifest.json").read_text())
    return {"path": str(target), "release": str(root), "source_sha": source or identity["source"], "sha256": digest(target)}


class Updater:
    def __init__(self, config, store=None, runner=None):
        self.c = config
        self.path = Path(config["state_dir"]) / "state.json"
        self.state = self.read_state()
        self.store = store
        self.runner = runner or subprocess.run
        self.fd = None

    def read_state(self):
        try:
            if self.path.exists():
                state = json.loads(self.path.read_text())
                if not isinstance(state, dict) or state.get("schema_version") != 1 or state.get("phase") not in ("idle", "downloading", "staged", "switching", "verifying", "rolling_back", "blocked") or type(state.get("paused")) is not bool:
                    raise ValueError("invalid persistent state schema")
                return state
            return {"schema_version": 1, "phase": "idle", "paused": False, "components": self.actual()}
        except (ValueError, OSError, KeyError) as error:
            return {"schema_version": 1, "phase": "blocked", "paused": False, "result": "state_error", "error": str(error), "components": self.diagnostic_components()}

    def diagnostic_components(self):
        result = {}
        for name, info in self.c["components"].items():
            try:
                result[name] = binding(info["path"])
            except (ValueError, OSError, KeyError) as error:
                result[name] = {"error": str(error), "binding": info["path"]}
        return result

    def save(self, **changes):
        self.state.update(changes)
        atomic_json(self.path, self.state)

    def run(self, args, **kwargs):
        return self.runner(args, check=True, timeout=180, **kwargs)

    def service(self, action, name):
        unit = self.c["components"][name].get("service")
        if unit:
            self.run(["systemctl", action, unit])

    def actual(self):
        return {k: binding(v["path"]) for k, v in self.c["components"].items()}

    def verify(self, components):
        nonce = secrets.token_hex(32)
        request = {"nonce": nonce, "identity": self.c["world_identity"], "components": components}
        started = time.monotonic()
        result = self.run(self.c["verifier_command"], input=json.dumps(request), text=True, capture_output=True)
        evidence = json.loads(result.stdout)
        if evidence.get("nonce") != nonce or time.monotonic() - started < 60:
            raise ValueError("verifier requires fresh nonce and actual 60-second observation")
        module("ecs_preview_health").validate_evidence(evidence, self.c["world_identity"], components)
        return evidence

    def stage(self, manifest):
        release = Path(self.c["releases_dir"]) / manifest["release_id"]
        def check(root):
            sums = {}
            for line in (root / "SHA256SUMS").read_text().splitlines():
                sha, name = line.split("  ", 1)
                rel = Path(name)
                if rel.is_absolute() or ".." in rel.parts or name in sums or not re.fullmatch(r"[0-9a-f]{64}", sha):
                    raise ValueError("invalid checksum path")
                sums[name] = sha
            actual = {p.relative_to(root).as_posix() for p in root.rglob("*") if p.is_file() and p.relative_to(root).as_posix() != "SHA256SUMS"}
            if set(sums) != actual or any(digest(root / name) != sha for name, sha in sums.items()):
                raise ValueError("bundle checksums do not cover exact payload")
            buildinfo = (root / "BUILDINFO").read_text().splitlines()
            if "source=" + manifest["source_sha"] not in buildinfo or "platform=linux-x64" not in buildinfo or "profile=ecs" not in buildinfo:
                raise ValueError("BUILDINFO identity mismatch")
            identity = json.loads((root / "manifest.json").read_text())
            if identity.get("source") != manifest["source_sha"] or identity.get("platform") != manifest["platform"] or identity.get("profile") != "ecs":
                raise ValueError("bundle source/platform mismatch")
            for name in manifest["components"]:
                if not (root / self.c["components"][name]["relative_path"]).exists():
                    raise ValueError("missing component payload")
            for name in manifest["components"]:
                binary = root / self.c["components"][name]["relative_path"]
                result = self.run(["ldd", str(binary)], capture_output=True, text=True)
                if "not found" in result.stdout:
                    raise ValueError("missing runtime dependency")
            local = Path(__file__).with_name("package-ecs-skills.py")
            if digest(root / "deployment/ecs-skill-release.py") != digest(local):
                raise ValueError("install matching trusted Skill coordinator before this release")
            module("package-ecs-skills").checked_release(root, manifest["source_sha"])
        if release.exists():
            check(release)
            return release
        release.parent.mkdir(parents=True, exist_ok=True)
        if shutil.disk_usage(release.parent).free < 528 * 1024**2:
            raise ValueError("insufficient download disk reserve")
        with tempfile.TemporaryDirectory(dir=release.parent, prefix=".stage-") as tmp:
            tmp = Path(tmp)
            archive = tmp / "bundle.tar.gz"
            limit = min(20 * 1024**3, shutil.disk_usage(release.parent).free - 528 * 1024**2)
            self.store.download_to(manifest["archive_key"], archive, limit)
            if digest(archive) != manifest["archive_sha256"]:
                raise ValueError("archive SHA256 mismatch")
            root = tmp / "payload"
            root.mkdir()
            with tarfile.open(archive) as tar:
                members = tar.getmembers()
                total = sum(m.size for m in members)
                if total > 20 * 1024**3 or shutil.disk_usage(root).free < total + 528 * 1024**2:
                    raise ValueError("insufficient staging disk space")
                if len({str(Path(m.name)) for m in members}) != len(members):
                    raise ValueError("duplicate archive member")
                for member in members:
                    rel = Path(member.name)
                    if rel.is_absolute() or ".." in rel.parts or not (member.isfile() or member.isdir()) or member.mode & 0o7000:
                        raise ValueError("unsafe archive member")
                if len(members) > 100000:
                    raise ValueError("archive contains too many members")
                for member in members:
                    target = root / member.name
                    if member.isdir():
                        target.mkdir(parents=True, exist_ok=True)
                    else:
                        target.parent.mkdir(parents=True, exist_ok=True)
                        with tar.extractfile(member) as source, target.open("xb") as output:
                            shutil.copyfileobj(source, output, length=1024 * 1024)
                        target.chmod(member.mode & 0o755)
            # package workflow archives one top-level directory.
            if not (root / "manifest.json").exists():
                children = list(root.iterdir())
                if len(children) != 1 or not children[0].is_dir():
                    raise ValueError("invalid archive root")
                root = children[0]
            check(root)
            os.rename(root, release)
        return release

    def permit(self, before, after, manifest):
        policy = json.loads(Path(self.c["compatibility_file"]).read_text())
        if not isinstance(policy, dict) or not {"schema_version", "permissions"} <= set(policy) or set(policy) - {"schema_version", "permissions", "viewer_policy"} or policy["schema_version"] != 1 or not isinstance(policy["permissions"], list):
            raise ValueError("invalid compatibility permission schema")
        wanted = {"from": {k: v["source_sha"] for k,v in before.items()}, "to": {k: v["source_sha"] for k,v in after.items()}, "contracts": manifest["compatibility"]}
        for item in policy["permissions"]:
            if not isinstance(item, dict) or set(item) != {"from", "to", "contracts", "evidence", "rollback_allowed"}:
                raise ValueError("invalid compatibility permission entry")
            if all(item.get(k) == v for k,v in wanted.items()) and isinstance(item.get("evidence"), str) and item["evidence"].strip() and isinstance(item.get("rollback_allowed"), bool):
                return item
        viewer = policy.get("viewer_policy")
        if viewer is not None:
            # Only the player bundle's read-only viewer is eligible. Chain
            # identity, bytes, release and actual binding must remain identical.
            if set(manifest["components"]) != {"player"} or before["chain"] != after["chain"]:
                raise ValueError("viewer policy cannot authorize a chain update")
            if module("ecs_preview_compatibility").evaluate_viewer(Path(before["player"]["release"]), Path(after["player"]["release"]), viewer):
                return {**wanted, "rollback_allowed": True, "evidence": "local read-only viewer input policy: " + viewer["evidence"]}
            raise ValueError("viewer input compatibility proof was not accepted")
        raise ValueError("no independent exact from/to compatibility permission")

    def point(self, name, target):
        path = Path(self.c["components"][name]["path"])
        temp = path.with_name(path.name + ".updating")
        temp.unlink(missing_ok=True)
        temp.symlink_to(target)
        os.replace(temp, path)

    def skill(self, components):
        info = components["player"]
        skill = self.c["skill"]
        self.run([sys.executable, str(Path(__file__).with_name("package-ecs-skills.py")), "activate", "--release", info["release"], "--expected-source", info["source_sha"], "--origin", skill["origin"], "--nginx-include", skill["nginx_include"], "--player-service", skill["player_service"], "--lock-file", self.c["lock_path"], "--lock-fd", str(self.fd)], pass_fds=(self.fd,))

    def switch(self, target, changed):
        self.service("stop", "player")
        if "chain" in changed:
            self.service("stop", "chain")
        for name in changed:
            self.save(phase="switching", switching_component=name)
            self.point(name, target[name]["path"])
            self.save(components=self.actual())
        if "chain" in changed:
            self.service("start", "chain")
        self.service("start", "player")
        self.skill(target)

    def recover(self):
        # Interrupted switches are ambiguous: never resume destructive steps or
        # roll old chain code onto storage whose writes were not observed.
        self.save(phase="blocked", result="interrupted", error="interrupted deployment requires operator inspection", components=self.actual(), failed_intent=self.state.get("target", {}).get("intent"))

    def tick(self):
        if self.state["paused"] or self.state["phase"] == "blocked":
            return
        if self.state["phase"] in ("switching", "verifying", "rolling_back"):
            self.recover()
            return
        release_api = module("ecs_preview_release")
        if self.store is None:
            self.store = release_api.OssStore(self.c["endpoint"], self.c["bucket"], self.c["prefix"])
        channel = json.loads(self.store.get(self.c["channel_key"]))
        release_api.validate_channel(channel, self.c["prefix"])
        intent = channel["intent"]
        if self.state.get("attempt_intent") == intent and time.time() < self.state.get("retry_after", 0):
            return
        if intent in (self.state.get("successful_intent"), self.state.get("failed_intent")):
            return
        if channel["sequence"] < self.state.get("observed_sequence", -1):
            raise ValueError("channel sequence regressed")
        self.save(target=channel, observed_sequence=channel["sequence"], phase="downloading")
        switched = False
        try:
            manifest = json.loads(self.store.get(channel["manifest_key"]))
            release_api.validate_manifest(manifest, self.c["prefix"])
            if any(manifest[k] != channel[k] for k in ("release_id", "source_sha")) or manifest["platform"] != "linux-x64":
                raise ValueError("channel/release/platform mismatch")
            changed = list(manifest["components"])
            if "player" in changed:
                changed.extend(["web", "skill"])
            release = self.stage(manifest)
            before = self.actual()
            after = dict(before)
            for name in changed:
                after[name] = binding(release / self.c["components"][name]["relative_path"], manifest["source_sha"])
            permission = self.permit(before, after, manifest)
            self.save(previous=before, expected=after, phase="staged", rollback_allowed=permission["rollback_allowed"])
            self.verify(before)
            switched = True
            self.save(phase="switching")
            self.switch(after, changed)
            self.save(phase="verifying")
            evidence = self.verify(after)
            self.save(phase="idle", result="success", successful_intent=intent, components=self.actual(), evidence=evidence, error=None)
        except Exception as error:
            transient = not switched and (isinstance(error, (OSError, TimeoutError)) or (type(error).__module__.startswith("oss2") and any(cls.__name__ in ("RequestError", "ServerError", "ServiceUnavailable", "RequestTimeout") for cls in type(error).__mro__)))
            attempts = self.state.get("attempts", 0) + 1 if self.state.get("attempt_intent") == intent else 1
            self.save(error=str(error), failed_intent=None if transient and attempts < 3 else intent, result="failed", components=self.actual(), attempt_intent=intent, attempts=attempts, retry_after=time.time() + min(300, 30 * 2 ** attempts))
            if switched and self.state.get("rollback_allowed"):
                try:
                    # A trusted verifier must explicitly attest the running
                    # chain storage remains compatible before any old code runs.
                    if "chain" in changed:
                        nonce = secrets.token_hex(32)
                        result = self.run(self.c["verifier_command"], input=json.dumps({"mode": "rollback-safety", "nonce": nonce, "identity": self.c["world_identity"], "components": self.actual()}), text=True, capture_output=True)
                        evidence = json.loads(result.stdout)
                        if evidence.get("nonce") != nonce or evidence.get("identity") != self.c["world_identity"] or evidence.get("chain_write_status") not in ("unchanged", "compatible"):
                            raise ValueError("chain writes unknown/incompatible; automatic rollback prohibited")
                    self.save(phase="rolling_back")
                    self.switch(before, changed)
                    proof = self.verify(before)
                    self.save(phase="idle", rollback_result="success", rollback_evidence=proof, components=self.actual())
                except Exception as rollback_error:
                    self.save(phase="blocked", rollback_result="failed", rollback_error=str(rollback_error), components=self.actual())
            else:
                self.save(phase="blocked" if switched else "idle")
            raise

    @contextlib.contextmanager
    def locked(self):
        path = Path(self.c["lock_path"])
        path.parent.mkdir(parents=True, exist_ok=True)
        fd = os.open(path, os.O_CREAT | os.O_RDWR | getattr(os, "O_NOFOLLOW", 0), 0o600)
        try:
            fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
            self.fd = fd
            if self.path.exists():
                self.state = self.read_state()
            yield
        finally:
            self.fd = None
            os.close(fd)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--config", required=True, type=Path)
    parser.add_argument("command", choices=("tick", "status", "pause", "resume", "retry"))
    args = parser.parse_args()
    config = load_config(args.config)
    updater = Updater(config)
    with updater.locked():
        if args.command == "status":
            print(json.dumps(updater.state, indent=2))
        elif args.command == "pause":
            updater.save(paused=True)
        elif args.command == "resume":
            updater.save(paused=False)
        elif args.command == "retry":
            if updater.state["phase"] == "blocked":
                if updater.state.get("result") == "state_error":
                    raise ValueError("repair persistent state before retry; status records diagnostic error")
                actual = updater.actual()
                if actual not in (updater.state.get("previous"), updater.state.get("expected")):
                    raise ValueError("partial component switch requires operator reconciliation before retry")
                proof = updater.verify(actual)
                if actual == updater.state.get("expected"):
                    updater.save(phase="idle", result="success", successful_intent=updater.state["target"]["intent"], evidence=proof, components=actual)
                    return
                updater.save(phase="idle", components=actual, rollback_result="operator_reconciled", rollback_evidence=proof)
            updater.save(failed_intent=None, retry_after=0, attempts=0)
            updater.tick()
        else:
            if not updater.state["paused"] and updater.state["phase"] not in ("blocked", "switching", "verifying", "rolling_back"):
                preflight(config)
            updater.tick()
