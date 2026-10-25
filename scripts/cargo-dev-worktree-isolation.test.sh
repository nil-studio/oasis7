#!/usr/bin/env bash
# Real Cargo proof: worktrees own their target and explicit overrides stay native.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
python3 - "$ROOT" <<'PYTHON'
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

root = Path(sys.argv[1])
with tempfile.TemporaryDirectory(prefix="oasis7-local-target-") as directory:
    parent = Path(directory)
    repo = parent / "repo"
    (repo / "src").mkdir(parents=True)
    (repo / "scripts").mkdir()
    shutil.copy2(root / "scripts/cargo-dev.sh", repo / "scripts/cargo-dev.sh")
    (repo / "Cargo.toml").write_text('[package]\nname="target_proof"\nversion="0.1.0"\nedition="2021"\n')
    source = 'pub fn value() -> u8 { VALUE }\n#[test] fn verify() { assert_eq!(value(), VALUE); }\n'
    (repo / "src/lib.rs").write_text(source.replace("VALUE", "1"))
    def git(*args):
        subprocess.run(["git", "-C", str(repo), *args], check=True, capture_output=True)
    git("init", "-qb", "main")
    git("-c", "user.name=Fixture", "-c", "user.email=fixture@example.test", "add", ".")
    git("-c", "user.name=Fixture", "-c", "user.email=fixture@example.test", "commit", "-qm", "initial")
    worktree = parent / "worktree with spaces"
    git("worktree", "add", "-qb", "topic", str(worktree))
    (worktree / "src/lib.rs").write_text(source.replace("VALUE", "2"))
    env = {k: v for k, v in os.environ.items() if k not in ("CARGO_TARGET_DIR", "RUSTC_WRAPPER")}
    env.update(CI="", OASIS7_CARGO_SCCACHE="0")
    def cargo(path, *args, target_env=None):
        build_env = dict(env)
        if target_env is not None:
            build_env["CARGO_TARGET_DIR"] = target_env
        return subprocess.run([str(path / "scripts/cargo-dev.sh"), *args], cwd=path,
                              env=build_env, check=True, capture_output=True, text=True).stdout
    for path in (repo, worktree):
        cargo(path, "test", "--offline")
        assert (path / "target/debug").is_dir()
        assert not (path / "target").is_symlink()
        cargo(path, "test", "--offline")
    # A configured directory must remain authoritative, including spaces.
    (repo / ".cargo").mkdir()
    (repo / ".cargo/config.toml").write_text('[build]\ntarget-dir="configured target"\n')
    cargo(repo, "build", "--offline")
    assert (repo / "configured target/debug").is_dir()
    cargo(repo, "build", "--offline", target_env="environment target")
    assert (repo / "environment target/debug").is_dir()
    cargo(repo, "build", "--offline", "--target-dir", "argument target", target_env="environment target")
    assert (repo / "argument target/debug").is_dir()
    # Failed Cargo calls keep their exit status; no external registry or links exist.
    assert not (parent / ".oasis7-cache").exists()
    for path in (repo, worktree):
        assert not list(path.glob("target.*"))
    print("cargo-dev-worktree-isolation.test: OK (real Cargo)")
PYTHON
