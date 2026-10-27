#!/usr/bin/env python3
"""Package and publish the game Skill from the same source as an ECS release."""
import argparse
import contextlib
import hashlib
import html
import json
import os
from pathlib import Path
import re
import signal
import subprocess
import tempfile
import time
import urllib.error
import urllib.parse
import urllib.request

SKILL_ROOT = "skills/oasis7-play"
CANONICAL = "/skills/oasis7-play/"
RELOAD_VERIFY_SECONDS = 20


def sha256(data):
    return hashlib.sha256(data).hexdigest()


def source_files(repo, source):
    """Read committed objects, never a dirty checkout or untracked private files."""
    entries = subprocess.check_output(["git", "ls-tree", "-rz", source, "--", SKILL_ROOT], cwd=repo)
    files = {}
    for entry in entries.split(b"\0"):
        if not entry:
            continue
        info, raw_path = entry.split(b"\t", 1)
        mode, kind, oid = info.decode().split()
        name = Path(raw_path.decode()).relative_to(SKILL_ROOT).as_posix()
        if mode != "100644" or kind != "blob" or not re.fullmatch(r"[A-Za-z0-9_./-]+\.md", name):
            raise ValueError("Skill distribution accepts only committed regular Markdown files")
        files[name] = subprocess.check_output(["git", "cat-file", "blob", oid], cwd=repo)
    if not files.get("SKILL.md"):
        raise ValueError("missing committed game Skill")
    # Relative Markdown references must survive publication at the same paths.
    for name, data in files.items():
        for link in re.findall(r"\]\(([^)]+)\)", data.decode("utf-8")):
            parsed = urllib.parse.urlsplit(link)
            if parsed.scheme or parsed.netloc or not parsed.path or parsed.path.startswith("/"):
                continue
            target = os.path.normpath(str(Path(name).parent / urllib.parse.unquote(parsed.path)))
            if target not in files:
                raise ValueError(f"unpublished Skill reference: {name}: {link}")
    return files


def package(repo, source, web):
    files = source_files(repo, source)
    root = web / "skills"
    root.mkdir(exist_ok=False)
    target = root / "oasis7-play"
    target.mkdir()
    for name, data in files.items():
        dest = target / name
        dest.parent.mkdir(parents=True, exist_ok=True)
        dest.write_bytes(data)
    manifest = {"schema": 1, "source": source, "canonical_path": CANONICAL + "SKILL.md",
                "files": {name: {"bytes": len(data), "sha256": sha256(data)} for name, data in sorted(files.items())}}
    (target / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    links = "".join(f'<li><a href="{CANONICAL}{html.escape(name)}">{html.escape(name)}</a></li>'
                    for name in sorted(files))
    (root / "index.html").write_text('<!doctype html><html lang="en"><meta charset="utf-8">'
        '<title>Oasis7 game Skill</title><h1>Official game Skill</h1>'
        f'<p>Source: {source}</p><ul>{links}<li><a href="{CANONICAL}manifest.json">Version and hashes</a></li></ul>'
        '<p>Publication grants no game authority. Follow the Skill and current service capabilities.</p></html>\n')


def release_bytes(path):
    if any(p.is_symlink() for p in (path, *path.parents)) or not path.is_file():
        raise ValueError("missing, symbolic or non-regular release payload")
    return path.read_bytes()


def checked_release(release, expected_source):
    if not re.fullmatch(r"[0-9a-f]{40}", expected_source):
        raise ValueError("expected source must be a full commit SHA")
    release = release.resolve(strict=True)
    manifest = json.loads(release_bytes(release / "manifest.json"))
    if any(manifest.get(k) != v for k, v in {"source": expected_source, "profile": "ecs", "platform": "linux-x64"}.items()):
        raise ValueError("ECS release identity mismatch")
    checksums = {}
    for line in release_bytes(release / "SHA256SUMS").decode().splitlines():
        digest, name = line.split("  ", 1)
        if name in checksums or not re.fullmatch(r"[0-9a-f]{64}", digest):
            raise ValueError("invalid bundle checksums")
        checksums[name] = digest
    root = release / "web-dist/skills"
    skill = json.loads(release_bytes(root / "oasis7-play/manifest.json"))
    if skill.get("schema") != 1 or skill.get("source") != expected_source or skill.get("canonical_path") != CANONICAL + "SKILL.md":
        raise ValueError("Skill source/schema mismatch")
    files = skill["files"]
    if not files.get("SKILL.md"):
        raise ValueError("Skill missing from manifest")
    public = {"/skills/index.html": root / "index.html", CANONICAL + "manifest.json": root / "oasis7-play/manifest.json"}
    for name, info in files.items():
        if not re.fullmatch(r"[A-Za-z0-9_./-]+\.md", name) or any(part in (".", "..") for part in name.split("/")) or name.startswith("/"):
            raise ValueError("unsafe Skill path")
        path = root / "oasis7-play" / name
        data = release_bytes(path)
        if info != {"sha256": sha256(data), "bytes": len(data)}:
            raise ValueError("Skill file mismatch: " + name)
        public[CANONICAL + name] = path
    for path in [*public.values(), release / "bin/oasis7_game_launcher"]:
        if checksums.get(path.relative_to(release).as_posix()) != sha256(release_bytes(path)):
            raise ValueError("release payload not bound to bundle checksums")
    return release, public


def nginx_config(public):
    blocks = ["# Generated by ecs-skill-release.py; exact public files only.\n"]
    aliases = dict(public)
    for url, path in public.items():
        if url.startswith(CANONICAL + "references/"):
            aliases["/skills/" + url[len(CANONICAL):]] = path
    for url, path in sorted(aliases.items()):
        if not re.fullmatch(r"/[A-Za-z0-9_./-]+", str(path)):
            raise ValueError("release path cannot be represented safely in nginx")
        mime = "application/json" if path.suffix == ".json" else "text/html" if path.suffix == ".html" else "text/markdown"
        blocks.append(f'location = {url} {{\n'
            '  if ($request_method !~ "^(GET|HEAD)$") { return 405; }\n'
            f'  alias {path};\n  types {{ }}\n  default_type {mime};\n'
            '  charset utf-8;\n  autoindex off;\n  expires -1;\n}\n')
    for url, target in {"/skills/": "/skills/index.html", "/skills/oasis7-play/": "/skills/index.html",
                        "/skills/oasis7.md": CANONICAL + "SKILL.md"}.items():
        blocks.append(f'location = {url} {{\n  if ($request_method !~ "^(GET|HEAD)$") {{ return 405; }}\n  return 302 {target};\n}}\n')
    return "".join(blocks).encode()


def public_origin(value):
    u = urllib.parse.urlsplit(value)
    if u.scheme != "https" or not u.hostname or u.username or u.password or u.path not in ("", "/") or u.query or u.fragment:
        raise ValueError("public origin must be an HTTPS origin without credentials or path")
    return value.rstrip("/")


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        return None


class PublicSkillMismatch(ValueError):
    pass


@contextlib.contextmanager
def reload_verification_deadline():
    # urllib's timeout covers socket operations, not a slowly streaming body.
    # ECS activation is POSIX/main-thread only; do not steal an existing timer.
    if not hasattr(signal, "setitimer") or signal.getitimer(signal.ITIMER_REAL) != (0.0, 0.0):
        raise ValueError("reload verification requires an unused POSIX wall-clock timer")
    previous = signal.getsignal(signal.SIGALRM)
    def expired(_signum, _frame):
        raise TimeoutError("public Skill reload verification deadline exceeded")
    signal.signal(signal.SIGALRM, expired)
    try:
        signal.setitimer(signal.ITIMER_REAL, RELOAD_VERIFY_SECONDS)
        yield
    finally:
        signal.setitimer(signal.ITIMER_REAL, 0)
        signal.signal(signal.SIGALRM, previous)


def verify(public, origin, *, after_reload=False):
    with reload_verification_deadline() if after_reload else contextlib.nullcontext():
        verify_public_set(public, origin, after_reload=after_reload)


def verify_public_set(public, origin, *, after_reload):
    origin = public_origin(origin)
    # A reload is asynchronous: old workers can finish requests after systemctl
    # returns. Require one entire matching set within a fixed convergence window.
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}), NoRedirect)
    expected = {url: release_bytes(path) for url, path in public.items()}
    deadline = time.monotonic() + RELOAD_VERIFY_SECONDS if after_reload else None
    while True:
        try:
            for url, wanted in expected.items():
                timeout = 15
                if deadline is not None:
                    remaining = deadline - time.monotonic()
                    if remaining <= 0:
                        raise TimeoutError("public Skill reload verification deadline exceeded")
                    timeout = min(3, remaining)
                with opener.open(origin + url, timeout=timeout) as response:
                    data = response.read(len(wanted) + 1)
                    if response.url != origin + url or response.status != 200:
                        raise ValueError("unexpected public Skill origin or status: " + url)
                    if data != wanted:
                        raise PublicSkillMismatch("public Skill differs from release: " + url)
            if deadline is not None and time.monotonic() > deadline:
                raise TimeoutError("public Skill reload verification deadline exceeded")
            break
        except (PublicSkillMismatch, urllib.error.HTTPError) as error:
            if deadline is None or (isinstance(error, urllib.error.HTTPError)
                                    and error.code not in (502, 503, 504)):
                raise
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                raise TimeoutError("public Skill reload verification deadline exceeded") from error
            time.sleep(min(0.5, remaining))
    print("Public Skill, references, discovery page and manifest match the release")


def atomic_write(path, data):
    with tempfile.NamedTemporaryFile(dir=path.parent, delete=False) as stream:
        temp = Path(stream.name)
        try:
            stream.write(data)
            stream.flush()
            os.fsync(stream.fileno())
            temp.chmod(0o644)
        except BaseException:
            temp.unlink(missing_ok=True)
            raise
    try:
        os.replace(temp, path)
    finally:
        temp.unlink(missing_ok=True)


def require_root_owned(paths):
    checked = set()
    for path in paths:
        for item in (path, *path.parents):
            if item in checked:
                continue
            checked.add(item)
            stat = item.lstat()
            if item.is_symlink() or stat.st_uid != 0 or stat.st_mode & 0o022:
                raise ValueError("publication path must be root-owned and not group/world writable: " + str(item))


@contextlib.contextmanager
def maintenance_lock(path, inherited_fd=None):
    """Reuse only a descriptor demonstrably holding this exact flock."""
    import fcntl
    import stat
    flags = os.O_RDWR | os.O_CREAT | getattr(os, "O_NOFOLLOW", 0)
    fd = os.open(path, flags, 0o600)
    try:
        identity = os.fstat(fd)
        if not stat.S_ISREG(identity.st_mode) or identity.st_nlink != 1:
            raise ValueError("maintenance lock must be a regular single-link file")
        if inherited_fd is None:
            fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
            yield fd
        else:
            inherited = os.fstat(inherited_fd)
            if (identity.st_dev, identity.st_ino) != (inherited.st_dev, inherited.st_ino):
                raise ValueError("inherited maintenance lock identity mismatch")
            # A separate open description must be blocked. Acquiring the passed
            # descriptor afterwards proves the conflicting lock belongs to it.
            try:
                fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
            except BlockingIOError:
                pass
            else:
                fcntl.flock(fd, fcntl.LOCK_UN)
                raise ValueError("inherited maintenance lock is not held")
            fcntl.flock(inherited_fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
            yield inherited_fd
    finally:
        os.close(fd)


def activate(args, public):
    import fcntl
    public_origin(args.origin)
    include = args.nginx_include.absolute()
    require_root_owned([*public.values(), args.release / "bin/oasis7_game_launcher", include])
    # Shared with the ECS operator; acquire before reading the previous config.
    with maintenance_lock(args.lock_file, getattr(args, "lock_fd", None)):
        pid = subprocess.check_output(["systemctl", "show", args.player_service, "-p", "MainPID", "--value"], text=True, timeout=3).strip()
        if not pid.isdigit() or int(pid) <= 0:
            raise ValueError("player service is not running")
        if sha256(Path(f"/proc/{pid}/exe").read_bytes()) != sha256((args.release / "bin/oasis7_game_launcher").read_bytes()):
            raise ValueError("running launcher does not match the selected release")
        if include.is_symlink() or not include.is_file():
            raise ValueError("bootstrap a regular nginx include before activation")
        config = subprocess.check_output(["nginx", "-T"], stderr=subprocess.STDOUT, text=True, timeout=6)
        if not re.search(r"^\s*include\s+" + re.escape(str(include)) + r"\s*;", config, re.M):
            raise ValueError("nginx does not include the managed Skill snippet")
        before = include.read_bytes()
        backup = include.with_name(include.name + ".previous")
        atomic_write(backup, before)
        try:
            atomic_write(include, nginx_config(public))
            subprocess.run(["nginx", "-t"], check=True, timeout=6)
            subprocess.run(["systemctl", "reload", "nginx"], check=True, timeout=6)
            verify(public, args.origin, after_reload=True)
        except BaseException:
            atomic_write(include, before)
            subprocess.run(["nginx", "-t"], check=True, timeout=6)
            subprocess.run(["systemctl", "reload", "nginx"], check=True, timeout=6)
            raise
        print("Skill publication activated; previous snippet: " + str(backup))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("verify", "activate"))
    parser.add_argument("--release", type=Path, required=True)
    parser.add_argument("--expected-source", required=True)
    parser.add_argument("--origin", required=True)
    parser.add_argument("--nginx-include", type=Path)
    parser.add_argument("--player-service")
    parser.add_argument("--lock-fd", type=int, help="inherited descriptor holding the exact maintenance lock")
    parser.add_argument("--lock-file", type=Path, default=Path("/var/lib/oasis7-preview-updater/deploy.lock"))
    args = parser.parse_args()
    args.release, public = checked_release(args.release, args.expected_source)
    if args.command == "verify":
        verify(public, args.origin)
    else:
        if not args.nginx_include or not args.player_service:
            parser.error("activate requires --nginx-include and --player-service")
        activate(args, public)


if __name__ == "__main__":
    main()
