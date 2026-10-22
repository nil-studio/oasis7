#!/usr/bin/env python3
"""Build a Linux ECS server bundle, or assemble already compiled binaries."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys

# The native cache scans all source inputs. Helpers must not create producer-only
# __pycache__ files before cache_begin that a clean consumer cannot restore.
sys.dont_write_bytecode = True

BINS = ("oasis7_chain_runtime", "oasis7_game_launcher", "oasis7_viewer_live")
ROOT = Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location("ecs_skill_release", ROOT / "scripts/package-ecs-skills.py")
skills = importlib.util.module_from_spec(spec)
spec.loader.exec_module(skills)


def digest(path):
    h = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            h.update(block)
    return h.hexdigest()


def regular(path):
    if path.is_symlink() or not path.is_file() or not path.stat().st_size:
        raise ValueError(f"missing, empty or symbolic file: {path.name}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=("native", "assemble"))
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--cache-state", type=Path, help="optional complete workspace Cargo unit state outside checkout")
    parser.add_argument("--native", type=Path)
    parser.add_argument("--web", type=Path)
    args = parser.parse_args()
    sha = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
    identity = {"schema": 1, "source": sha, "profile": "ecs", "platform": "linux-x64", "binaries": list(BINS)}
    # Never merge a new package with stale output from another source.
    args.out.mkdir(parents=True, exist_ok=False)
    if args.mode == "native":
        env = dict(os.environ)
        env.pop("RUSTC_WRAPPER", None)
        command = ["cargo", "build", "--locked", "--timings", "--profile", "ecs",
                        "-p", "oasis7_chain_runtime", "-p", "oasis7_launcher_backend",
                        "-p", "oasis7_viewer_runtime", *[arg for name in BINS for arg in ("--bin", name)]]
        if args.cache_state:
            spec = importlib.util.spec_from_file_location("ecs_cache", ROOT / "scripts/ci-ecs-cache.py")
            cache = importlib.util.module_from_spec(spec)
            spec.loader.exec_module(cache)
            cache.build(command, env, args.cache_state)
        else:
            subprocess.run(command, cwd=ROOT, env=env, check=True)
        target = Path(env.get("CARGO_TARGET_DIR", ROOT / "target")) / "ecs"
        files = {}
        for name in BINS:
            source = target / name
            regular(source)
            shutil.copy2(source, args.out / name)
            files[name] = {"sha256": digest(source), "mode": source.stat().st_mode & 0o777}
        (args.out / "native.json").write_text(json.dumps({**identity, "files": files}, indent=2) + "\n")
    else:
        if not args.native or not args.web:
            parser.error("assemble requires --native and --web")
        regular(args.native / "native.json")
        stage = json.loads((args.native / "native.json").read_text())
        if any(stage.get(key) != value for key, value in identity.items()) or set(stage.get("files", {})) != set(BINS):
            raise ValueError("native stage source or build identity mismatch")
        (args.out / "bin").mkdir()
        for name in BINS:
            source = args.native / name
            regular(source)
            if stage["files"][name] != {"sha256": digest(source), "mode": source.stat().st_mode & 0o777} or not os.access(source, os.X_OK):
                raise ValueError("native stage binary mismatch")
            shutil.copy2(source, args.out / "bin" / name)
        regular(args.web / "index.html")
        for path in args.web.rglob("*"):
            if path.is_symlink() or not (path.is_file() or path.is_dir()):
                raise ValueError("unsupported Web payload file")
        shutil.copytree(args.web, args.out / "web-dist")
        skills.package(ROOT, sha, args.out / "web-dist")
        (args.out / "deployment").mkdir()
        shutil.copy2(ROOT / "scripts/package-ecs-skills.py", args.out / "deployment/ecs-skill-release.py")
        (args.out / "BUILDINFO").write_text(f"source={sha}\nprofile=ecs\nplatform=linux-x64\nkind=ecs-server\n")
        (args.out / "manifest.json").write_text(json.dumps(identity, indent=2) + "\n")
        files = sorted(path for path in args.out.rglob("*") if path.is_file())
        (args.out / "SHA256SUMS").write_text("".join(f"{digest(path)}  {path.relative_to(args.out).as_posix()}\n" for path in files))


if __name__ == "__main__":
    main()
