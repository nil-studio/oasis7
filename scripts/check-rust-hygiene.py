#!/usr/bin/env python3
"""Static Rust workspace hygiene checks for rust-baseline.

Checks:
1. Every `.rs` file under workspace crate `src/` trees must be reachable from
   some crate root/target via `mod`, `#[path = "..."]`, or `include!("...")`
   (including cross-crate `#[path]` mounts).
2. When `--base` and `--head` are supplied, newly added `#[allow(...dead_code...)]`
   attributes in non-test `src/` files must carry a reason comment on the same
   or immediately preceding line (e.g. `// dark: #<issue>`).
"""

from __future__ import annotations

import argparse
import re
import subprocess
import sys
from pathlib import Path

try:
    import tomllib
except ImportError:  # pragma: no cover
    tomllib = None  # type: ignore[assignment]


INCLUDE_RE = re.compile(r"""\binclude!\s*\(\s*"([^"]+)"\s*\)""")
ALLOW_DEAD_CODE_RE = re.compile(r"""#!?\[allow\([^)]*\bdead_code\b[^)]*\)\]""")
LINE_COMMENT_RE = re.compile(r"//")
BLOCK_COMMENT_RE = re.compile(r"/\*.*?\*/", re.DOTALL)
LINE_COMMENT_STRIP_RE = re.compile(r"//[^\n]*")


def strip_comments_and_raw_strings(source: str) -> str:
    """Fast linear comment stripping (no catastrophic backtracking)."""
    if "/*" in source:
        source = BLOCK_COMMENT_RE.sub("", source)
    if "//" in source:
        source = LINE_COMMENT_STRIP_RE.sub("", source)
    return source


def load_workspace_crates(repo_root: Path) -> list[Path]:
    cargo_toml = repo_root / "Cargo.toml"
    if not cargo_toml.is_file() or tomllib is None:
        return sorted(
            p.parent
            for p in list((repo_root / "crates").glob("*/Cargo.toml"))
            + list((repo_root / "tools").glob("*/Cargo.toml"))
        )
    data = tomllib.loads(cargo_toml.read_text(encoding="utf-8"))
    members = data.get("workspace", {}).get("members", [])
    excludes = set(data.get("workspace", {}).get("exclude", []))
    crates: list[Path] = []
    seen: set[Path] = set()
    for member in members:
        if any(ch in member for ch in "*?["):
            matched = sorted(repo_root.glob(member))
        else:
            matched = [repo_root / member]
        for path in matched:
            rel = path.relative_to(repo_root).as_posix()
            if rel in excludes or not (path / "Cargo.toml").is_file():
                continue
            resolved = path.resolve()
            if resolved not in seen:
                seen.add(resolved)
                crates.append(resolved)
    return crates


def crate_entry_files(crate_dir: Path) -> list[Path]:
    entries: list[Path] = []
    for rel in ("build.rs", "src/lib.rs", "src/main.rs"):
        candidate = crate_dir / rel
        if candidate.is_file():
            entries.append(candidate.resolve())
    for sub in ("src/bin", "tests", "examples", "benches"):
        sub_dir = crate_dir / sub
        if not sub_dir.is_dir():
            continue
        for rs in sorted(sub_dir.glob("*.rs")):
            entries.append(rs.resolve())
        for main_rs in sorted(sub_dir.glob("*/main.rs")):
            entries.append(main_rs.resolve())
    cargo_toml = crate_dir / "Cargo.toml"
    if cargo_toml.is_file() and tomllib is not None:
        data = tomllib.loads(cargo_toml.read_text(encoding="utf-8"))
        lib_tbl = data.get("lib")
        if isinstance(lib_tbl, dict) and isinstance(lib_tbl.get("path"), str):
            p = (crate_dir / lib_tbl["path"]).resolve()
            if p.is_file():
                entries.append(p)
        for section in ("bin", "test", "example", "bench"):
            for item in data.get(section, []) or []:
                if isinstance(item, dict) and isinstance(item.get("path"), str):
                    p = (crate_dir / item["path"]).resolve()
                    if p.is_file():
                        entries.append(p)
    return entries


def module_search_dirs(file_path: Path, is_crate_root: bool, inline_stack: list[str]) -> list[Path]:
    stem = file_path.stem
    parent = file_path.parent
    if is_crate_root or stem == "mod":
        base = parent
    else:
        base = parent / stem
    if inline_stack:
        nested = base.joinpath(*inline_stack)
        return [nested, base, parent]
    return [base, parent]


def resolve_mod_candidates(
    file_path: Path,
    is_crate_root: bool,
    inline_stack: list[str],
    mod_name: str,
    explicit_path: str | None,
) -> list[Path]:
    dirs = module_search_dirs(file_path, is_crate_root, inline_stack)
    found: list[Path] = []
    if explicit_path:
        for base in dirs:
            candidate = (base / explicit_path).resolve()
            if candidate.is_file():
                found.append(candidate)
                break
        if not found:
            candidate = (file_path.parent / explicit_path).resolve()
            if candidate.is_file():
                found.append(candidate)
        return found

    for base in dirs:
        for rel in (f"{mod_name}.rs", f"{mod_name}/mod.rs"):
            candidate = (base / rel).resolve()
            if candidate.is_file():
                found.append(candidate)
                return found
    return found


MOD_OR_BRACE_RE = re.compile(
    r"""(?P<lbrace>\{)|(?P<rbrace>\})|"""
    r"""(?P<path_attr>\#\[path\s*=\s*"([^"]+)"\])|"""
    r"""(?P<mod_decl>\bmod\s+([A-Za-z_][A-Za-z0-9_]*)\s*(;|{))"""
)


def scan_file_references(file_path: Path, is_crate_root: bool) -> list[tuple[Path, bool]]:
    try:
        raw = file_path.read_text(encoding="utf-8")
    except OSError:
        return []
    if "mod " not in raw and "include!" not in raw:
        return []
    cleaned = strip_comments_and_raw_strings(raw)
    refs: list[tuple[Path, bool]] = []

    for m in INCLUDE_RE.finditer(cleaned):
        inc_path = m.group(1)
        if inc_path:
            candidate = (file_path.parent / inc_path).resolve()
            if candidate.is_file():
                refs.append((candidate, is_crate_root))

    inline_stack: list[tuple[str, int]] = []
    brace_depth = 0
    pending_path: str | None = None
    pending_path_depth = -1

    for m in MOD_OR_BRACE_RE.finditer(cleaned):
        if m.group("lbrace"):
            brace_depth += 1
            pending_path = None
        elif m.group("rbrace"):
            brace_depth = max(0, brace_depth - 1)
            pending_path = None
            while inline_stack and brace_depth < inline_stack[-1][1]:
                inline_stack.pop()
        elif m.group("path_attr"):
            pending_path = m.group(4)
            pending_path_depth = brace_depth
        elif m.group("mod_decl"):
            mod_name = m.group(6)
            delim = m.group(7)
            explicit_path = pending_path if pending_path_depth == brace_depth else None
            pending_path = None
            stack_names = [name for name, _ in inline_stack]
            if delim == ";":
                for target in resolve_mod_candidates(
                    file_path, is_crate_root, stack_names, mod_name, explicit_path
                ):
                    refs.append((target, False))
            else:
                brace_depth += 1
                inline_stack.append((explicit_path or mod_name, brace_depth))
    return refs


def check_orphan_rust_files(repo_root: Path) -> list[str]:
    crates = load_workspace_crates(repo_root)
    queue: list[tuple[Path, bool]] = []
    for crate_dir in crates:
        for entry in crate_entry_files(crate_dir):
            queue.append((entry, True))

    visited: set[tuple[Path, bool]] = set()
    visited_paths: set[Path] = set()
    while queue:
        current, is_root = queue.pop()
        if (current, is_root) in visited:
            continue
        visited.add((current, is_root))
        visited_paths.add(current)
        for nxt, nxt_is_root in scan_file_references(current, is_root):
            if (nxt, nxt_is_root) not in visited:
                queue.append((nxt, nxt_is_root))

    orphans: list[str] = []
    for crate_dir in crates:
        src_dir = crate_dir / "src"
        if not src_dir.is_dir():
            continue
        for rs_file in sorted(src_dir.rglob("*.rs")):
            resolved = rs_file.resolve()
            if resolved not in visited_paths:
                orphans.append(resolved.relative_to(repo_root.resolve()).as_posix())
    return sorted(orphans)


def is_test_source_path(rel_path: str) -> bool:
    parts = Path(rel_path).parts
    name = Path(rel_path).name
    if "tests" in parts or "benches" in parts or "examples" in parts:
        return True
    if name.endswith(("_tests.rs", "_test.rs")) or name.startswith("tests_"):
        return True
    return False


def check_allow_dead_code_comments(repo_root: Path, base: str, head: str) -> list[str]:
    cmd = [
        "git",
        "-C",
        str(repo_root),
        "diff",
        "--unified=1",
        "--no-color",
        base,
        head,
        "--",
        ":(glob)crates/*/src/**/*.rs",
        ":(glob)tools/*/src/**/*.rs",
    ]
    result = subprocess.run(cmd, capture_output=True, text=True, check=False)
    if result.returncode != 0:
        return [f"git diff failed ({result.returncode}): {result.stderr.strip()}"]

    violations: list[str] = []
    current_file: str | None = None
    prev_line_text = ""
    current_line_no = 0
    hunk_re = re.compile(r"^@@ -\d+(?:,\d+)? \+(\d+)(?:,\d+)? @@")

    for raw_line in result.stdout.splitlines():
        if raw_line.startswith("+++ b/"):
            current_file = raw_line[6:]
            prev_line_text = ""
            continue
        if raw_line.startswith("@@"):
            m = hunk_re.match(raw_line)
            current_line_no = int(m.group(1)) if m else 0
            prev_line_text = ""
            continue
        if current_file is None or is_test_source_path(current_file):
            continue
        if raw_line.startswith("+") and not raw_line.startswith("+++"):
            text = raw_line[1:]
            if ALLOW_DEAD_CODE_RE.search(text):
                if not LINE_COMMENT_RE.search(text) and not LINE_COMMENT_RE.search(prev_line_text):
                    violations.append(
                        f"{current_file}:{current_line_no}: newly added allow(dead_code) "
                        "requires an explanatory comment on the same or preceding line "
                        "(e.g. `// dark: #<issue>`)"
                    )
            prev_line_text = text
            current_line_no += 1
        elif raw_line.startswith(" "):
            prev_line_text = raw_line[1:]
            current_line_no += 1
    return violations


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Check Rust workspace hygiene.")
    parser.add_argument("--repo-root", default=".", help="Repository root directory")
    parser.add_argument("--base", default=None, help="Optional git base ref for diff checks")
    parser.add_argument("--head", default=None, help="Optional git head ref for diff checks")
    args = parser.parse_args(argv)

    repo_root = Path(args.repo_root).resolve()
    orphans = check_orphan_rust_files(repo_root)
    violations: list[str] = []
    if args.base and args.head:
        violations = check_allow_dead_code_comments(repo_root, args.base, args.head)

    if orphans:
        print("check-rust-hygiene: FAIL: orphaned Rust source file(s) not reachable from any crate target:", file=sys.stderr)
        for path in orphans:
            print(f"  - {path}", file=sys.stderr)
    if violations:
        print("check-rust-hygiene: FAIL: uncommented allow(dead_code) addition(s):", file=sys.stderr)
        for item in violations:
            print(f"  - {item}", file=sys.stderr)
    if orphans or violations:
        return 1

    print("check-rust-hygiene: OK (0 orphaned src/*.rs files)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
