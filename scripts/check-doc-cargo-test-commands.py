#!/usr/bin/env python3
"""Advisory static check for `cargo test -p <crate> <filter>` commands in active docs.

Scans changed (or specified) active Markdown files for explicit `cargo test -p <crate> ... <filter>`
commands and verifies that the target crate's source/tests contain at least one matching test/symbol.
Always exits 0 (advisory) unless `--strict` is explicitly passed.
"""

from __future__ import annotations

import argparse
import re
import subprocess
import sys
from pathlib import Path

CARGO_TEST_CMD_RE = re.compile(
    r"cargo\s+test\s+[^\n`]*?-p\s+([A-Za-z0-9_-]+)([^\n`]*)"
)
SKIP_PATH_FRAGMENTS = ("/devlog/", "/evidence/", "/archive/", "doc/devlog/")
FLAGS_WITH_VALUE = frozenset(
    {
        "-p",
        "--package",
        "--features",
        "--test",
        "--bin",
        "--example",
        "--bench",
        "--target",
        "--profile",
        "--manifest-path",
        "--target-dir",
        "-j",
        "--jobs",
    }
)


def is_active_doc(rel_path: str) -> bool:
    if not rel_path.endswith(".md"):
        return False
    norm = rel_path.replace("\\", "/")
    return not any(frag in norm for frag in SKIP_PATH_FRAGMENTS)


def extract_test_filter(rest: str) -> str | None:
    before_args = rest.split("--", 1)[0]
    tokens = before_args.split()
    i = 0
    positional: list[str] = []
    while i < len(tokens):
        tok = tokens[i]
        if tok in FLAGS_WITH_VALUE:
            i += 2
            continue
        if tok.startswith("-"):
            i += 1
            continue
        positional.append(tok)
        i += 1
    if not positional:
        return None
    candidate = positional[0].strip("'\"")
    # Skip placeholder filters like <test_name> or very short generic words
    if not candidate or any(ch in candidate for ch in "<>$*{}[]()|;"):
        return None
    if not re.fullmatch(r"[A-Za-z0-9_:]+", candidate):
        return None
    return candidate


def crate_dir_map(repo_root: Path) -> dict[str, Path]:
    mapping: dict[str, Path] = {}
    for parent in ("crates", "tools"):
        base = repo_root / parent
        if not base.is_dir():
            continue
        for cargo_toml in base.glob("*/Cargo.toml"):
            text = cargo_toml.read_text(encoding="utf-8", errors="ignore")
            m = re.search(r'(?m)^name\s*=\s*"([^"]+)"', text)
            if m:
                mapping[m.group(1)] = cargo_toml.parent
            mapping.setdefault(cargo_toml.parent.name, cargo_toml.parent)
    return mapping


def crate_contains_filter(crate_dir: Path, test_filter: str) -> bool:
    leaf = test_filter.split("::")[-1]
    for sub in ("src", "tests", "examples", "benches"):
        sub_dir = crate_dir / sub
        if not sub_dir.is_dir():
            continue
        for rs in sub_dir.rglob("*.rs"):
            try:
                content = rs.read_text(encoding="utf-8", errors="ignore")
            except OSError:
                continue
            if leaf in content:
                return True
    return False


def collect_changed_docs(repo_root: Path, base: str, head: str) -> list[Path]:
    result = subprocess.run(
        ["git", "-C", str(repo_root), "diff", "--name-only", "--diff-filter=AM", base, head],
        capture_output=True,
        text=True,
        check=False,
    )
    if result.returncode != 0:
        return []
    docs: list[Path] = []
    for line in result.stdout.splitlines():
        rel = line.strip()
        if rel and is_active_doc(rel):
            p = repo_root / rel
            if p.is_file():
                docs.append(p)
    return docs


def check_docs(repo_root: Path, doc_paths: list[Path]) -> list[str]:
    crates = crate_dir_map(repo_root)
    cache: dict[tuple[str, str], bool] = {}
    warnings: list[str] = []

    for doc_path in doc_paths:
        rel = doc_path.relative_to(repo_root).as_posix()
        try:
            lines = doc_path.read_text(encoding="utf-8", errors="ignore").splitlines()
        except OSError:
            continue
        for lineno, line in enumerate(lines, 1):
            for m in CARGO_TEST_CMD_RE.finditer(line):
                crate_name = m.group(1)
                test_filter = extract_test_filter(m.group(2))
                if not test_filter or crate_name not in crates:
                    continue
                key = (crate_name, test_filter)
                if key not in cache:
                    cache[key] = crate_contains_filter(crates[crate_name], test_filter)
                if not cache[key]:
                    warnings.append(
                        f"{rel}:{lineno}: `cargo test -p {crate_name} ... {test_filter}` "
                        f"did not match any symbol in {crates[crate_name].relative_to(repo_root).as_posix()}"
                    )
    return warnings


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Advisory static check for doc cargo test commands.")
    parser.add_argument("--repo-root", default=".", help="Repository root directory")
    parser.add_argument("--base", default=None, help="Git base ref")
    parser.add_argument("--head", default=None, help="Git head ref")
    parser.add_argument("--strict", action="store_true", help="Exit non-zero on warnings")
    parser.add_argument("files", nargs="*", help="Explicit markdown files to check")
    args = parser.parse_args(argv)

    repo_root = Path(args.repo_root).resolve()
    if args.files:
        doc_paths = [
            (repo_root / f).resolve()
            for f in args.files
            if is_active_doc(f) and (repo_root / f).is_file()
        ]
    elif args.base and args.head:
        doc_paths = collect_changed_docs(repo_root, args.base, args.head)
    else:
        return 0

    warnings = check_docs(repo_root, doc_paths)
    if warnings:
        for item in warnings:
            print(f"doc-cargo-test-check: advisory: {item}", file=sys.stderr)
        return 1 if args.strict else 0
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
