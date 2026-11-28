#!/usr/bin/env python3
"""Trusted authority Python launcher: validates non-symlink tree and isolates sys.path."""

from __future__ import annotations
import os
from pathlib import Path
import runpy
import sys
import sysconfig


def _assert_no_symlinks_in_tree(root: Path) -> None:
    if root.is_symlink() or not root.is_dir():
        raise SystemExit(f"trusted authority directory invalid or symlinked: {root}")
    for dirpath, dirnames, filenames in os.walk(root, followlinks=False):
        cur = Path(dirpath)
        if cur.is_symlink():
            raise SystemExit(f"symlinked directory in trusted authority: {cur}")
        for name in (*dirnames, *filenames):
            if (cur / name).is_symlink():
                raise SystemExit(f"symlinked entry in trusted authority: {cur / name}")


def _is_trusted_runtime_path(entry: str, roots: tuple[str, ...]) -> bool:
    if not entry:
        return False
    try:
        res = str(Path(entry).resolve())
    except OSError:
        return False
    return any(res == r or res.startswith(r + os.sep) or entry.endswith(".zip") for r in roots)


def main(argv: list[str] | None = None) -> None:
    args = list(sys.argv[1:] if argv is None else argv)
    if not args:
        raise SystemExit("usage: run-ci-authority-python.py <authority-entry.py> [args...]")
    launcher = Path(__file__)
    if launcher.is_symlink():
        raise SystemExit(f"trusted launcher must not be a symlink: {launcher}")
    authority = launcher.parent
    _assert_no_symlinks_in_tree(authority)
    target = Path(args[0])
    if target.is_symlink() or not target.is_file():
        raise SystemExit(f"trusted authority target invalid or symlinked: {target}")
    authority_resolved, target_resolved = authority.resolve(), target.resolve()
    try:
        target_resolved.relative_to(authority_resolved)
    except ValueError as exc:
        raise SystemExit(f"target escapes trusted authority root: {target}") from exc
    paths = sysconfig.get_paths()
    roots = tuple({str(Path(p).resolve()) for k in ("stdlib", "platstdlib", "purelib", "platlib") if (p := paths.get(k))})
    sys.path[:] = [str(authority_resolved), *[p for p in sys.path if _is_trusted_runtime_path(p, roots)]]
    sys.argv[:] = [str(target_resolved), *args[1:]]
    runpy.run_path(str(target_resolved), run_name="__main__")


if __name__ == "__main__":
    main()
