#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

python3 - <<'PY'
from __future__ import annotations

import os
import subprocess
import sys
from pathlib import PurePosixPath

INVALID_CHARS = set('<>:"\\|?*')
RESERVED_NAMES = {
    "CON",
    "PRN",
    "AUX",
    "NUL",
    *(f"COM{i}" for i in range(1, 10)),
    *(f"LPT{i}" for i in range(1, 10)),
}


def invalid_reason(path: str) -> str | None:
    for component in PurePosixPath(path).parts:
        if component in ("", "."):
            continue
        control_chars = sorted({ord(ch) for ch in component if ord(ch) < 32})
        if control_chars:
            formatted = ", ".join(f"0x{code:02x}" for code in control_chars)
            return f"contains Windows-invalid control characters [{formatted}] in path segment {component!r}"
        bad_chars = sorted({ch for ch in component if ch in INVALID_CHARS})
        if bad_chars:
            return f"contains Windows-invalid characters {''.join(bad_chars)!r} in path segment {component!r}"
        if component[-1] in {" ", "."}:
            return f"ends with trailing space/dot in path segment {component!r}"
        stem = component.split(".")[0].upper()
        if stem in RESERVED_NAMES:
            return f"uses Windows-reserved path segment {component!r}"
    return None


def run_git(*args: str) -> bytes:
    return subprocess.run(
        ["git", *args],
        check=True,
        stdout=subprocess.PIPE,
    ).stdout


def decode_paths(blob: bytes) -> list[str]:
    return [entry.decode("utf-8") for entry in blob.split(b"\0") if entry]


def apply_name_status(paths: set[str], blob: bytes) -> None:
    entries = [entry.decode("utf-8") for entry in blob.split(b"\0") if entry]
    idx = 0
    while idx < len(entries):
        status = entries[idx]
        idx += 1
        kind = status[0]
        if kind in {"R", "C"}:
            old_path = entries[idx]
            new_path = entries[idx + 1]
            idx += 2
            if kind == "R":
                paths.discard(old_path)
            paths.add(new_path)
            continue
        path = entries[idx]
        idx += 1
        if kind == "D":
            paths.discard(path)
        else:
            paths.add(path)


paths = set(decode_paths(run_git("ls-files", "-z")))
apply_name_status(paths, run_git("diff", "--cached", "--name-status", "-z"))
apply_name_status(paths, run_git("diff", "--name-status", "-z"))
local_check = os.environ.get("OASIS7_LOCAL_CHECK") == "1"
if local_check:
    paths.update(decode_paths(run_git("ls-files", "--others", "--exclude-standard", "-z")))
scope = "tracked and nonignored untracked" if local_check else "tracked"

violations = []
for path in sorted(paths):
    reason = invalid_reason(path)
    if reason is not None:
        violations.append((path, reason))

if violations:
    print(f"error: {scope} paths incompatible with Windows checkout detected:", file=sys.stderr)
    for path, reason in violations:
        print(f"  - {path}: {reason}", file=sys.stderr)
    sys.exit(1)

print(f"ok: checked {len(paths)} {scope} paths for Windows checkout compatibility")
PY
