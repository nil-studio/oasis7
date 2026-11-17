#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"
python_bin=$(./scripts/find-python-with-module.sh tomllib)

"$python_bin" - "${OASIS7_RUSTSEC_DENY_TOML:-deny.toml}" <<'PYTHON'
from pathlib import Path
import sys
import subprocess
import tomllib

try:
    config = tomllib.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
    advisories = config.get("advisories", {})
    bans = config.get("bans", {})
    if not isinstance(advisories, dict) or not isinstance(bans, dict):
        raise ValueError("advisories and bans must be tables")
    ignored = advisories.get("ignore")
    # cargo-deny remains the advisory evaluator. This guard only constrains the
    # approved config, never filters findings or grants path-specific exemptions.
    if not isinstance(ignored, list):
        raise ValueError("advisories.ignore must be empty or the single approved maintenance exception")
    if ignored:
        if (len(ignored) != 1 or not isinstance(ignored[0], dict)
                or set(ignored[0]) != {"id", "reason"}
                or ignored[0]["id"] != "RUSTSEC-2026-0543"
                or not isinstance(ignored[0]["reason"], str)
                or not ignored[0]["reason"].strip()):
            raise ValueError("advisories.ignore must be empty or the single approved maintenance exception")
    denied = bans.get("deny")
    if not isinstance(denied, list):
        raise ValueError("bans.deny must be an array")
    for package in ("serde_cbor", "paste", "ttf-parser"):
        # A version or wrappers condition would permit part of the retired graph.
        if {"name": package} not in denied:
            raise ValueError(f"bans.deny must unconditionally prohibit {package}")
    # cargo-deny covers the root graph; standalone fixtures/modules also retain
    # package identities in their own lockfiles, independent of dependency aliases.
    paths = subprocess.check_output([
        "git", "ls-files", "-z", "--cached", "--others", "--exclude-standard", "--",
        "Cargo.lock", "**/Cargo.lock",
    ]).decode().split("\0")
    retired = {"serde_cbor", "paste", "ttf-parser"}
    for name in sorted(set(paths) - {""}):
        path = Path(name)
        if not path.exists():
            continue  # A tracked lockfile can be deleted by the current patch.
        lock = tomllib.loads(path.read_text(encoding="utf-8"))
        present = retired.intersection(package["name"] for package in lock.get("package", []))
        if present:
            raise ValueError(f"{path}: retired package(s) still locked: {', '.join(sorted(present))}")
except (OSError, ValueError, TypeError, KeyError, subprocess.CalledProcessError) as error:
    print(f"error: {error}", file=sys.stderr)
    raise SystemExit(1)

print("ok: RustSec exceptions limited to the approved chrono maintenance ID; retired packages are banned and absent from all repository lockfiles")
PYTHON
