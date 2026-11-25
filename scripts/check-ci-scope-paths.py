#!/usr/bin/env python3
"""Verify that exact repository paths declared in scripts/ci-required-scope.json exist."""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

REPO_TOP_DIRS = frozenset(
    {"crates", "tools", "scripts", "doc", "site", "skills", "docker", ".github", ".agents"}
)
SKIP_OBJECT_KEYS = frozenset({"reserved_empty_dir_prefixes"})


def is_exact_repo_path(value: str) -> bool:
    if "/" not in value:
        return False
    if any(ch in value for ch in "*?[]"):
        return False
    if value.startswith(("http://", "https://", "/")):
        return False
    top = value.split("/", 1)[0]
    return top in REPO_TOP_DIRS


def find_missing_scope_paths(repo_root: Path, scope_file: Path | None = None) -> list[tuple[str, str]]:
    scope_path = scope_file or (repo_root / "scripts/ci-required-scope.json")
    data = json.loads(scope_path.read_text(encoding="utf-8"))
    missing: list[tuple[str, str]] = []

    def walk(obj: object, trail: str) -> None:
        if isinstance(obj, dict):
            for key, val in obj.items():
                if key in SKIP_OBJECT_KEYS:
                    continue
                if isinstance(key, str) and is_exact_repo_path(key):
                    if not (repo_root / key).exists():
                        missing.append((f"{trail}.{key} (key)", key))
                walk(val, f"{trail}.{key}" if trail else str(key))
        elif isinstance(obj, list):
            for idx, item in enumerate(obj):
                walk(item, f"{trail}[{idx}]")
        elif isinstance(obj, str):
            if is_exact_repo_path(obj) and not (repo_root / obj).exists():
                missing.append((trail, obj))

    walk(data, "")
    return missing


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Check exact paths in scripts/ci-required-scope.json.")
    parser.add_argument("--repo-root", default=".", help="Repository root directory")
    parser.add_argument("--scope-file", default=None, help="Optional override path to ci-required-scope.json")
    args = parser.parse_args(argv)

    repo_root = Path(args.repo_root).resolve()
    scope_file = Path(args.scope_file).resolve() if args.scope_file else None
    missing = find_missing_scope_paths(repo_root, scope_file)
    if missing:
        print(
            "check-ci-scope-paths: FAIL: exact path(s) declared in scripts/ci-required-scope.json do not exist:",
            file=sys.stderr,
        )
        for loc, rel_path in missing:
            print(f"  - {rel_path} (at {loc})", file=sys.stderr)
        return 1

    print("check-ci-scope-paths: OK (0 missing exact paths)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
