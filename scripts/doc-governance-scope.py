#!/usr/bin/env python3
"""Select documentation and reverse references without a tracked inventory."""

from __future__ import annotations

import argparse
import fnmatch
from functools import lru_cache
import os
from pathlib import Path
import posixpath
import re
import subprocess
import sys
from urllib.parse import unquote, urlsplit

from product_doc_markdown import markdown_anchor_ids, parse_markdown_links, parse_markdown_reference_text


CONTROL_PATHS = (
    "scripts/doc-*", "scripts/doc_*", "scripts/product-doc-*",
    "scripts/product_doc*", "scripts/system-design-traceability*",
    "scripts/resolve-product-doc-ci-range*", "scripts/find-python-with-module*",
    "doc/engineering/doc-governance/*", "doc/.governance/*",
)


def git(root: Path, *args: str) -> bytes:
    result = subprocess.run(["git", "-C", str(root), *args], capture_output=True)
    if result.returncode:
        raise ValueError(result.stderr.decode("utf-8", "replace").strip() or "Git input failed")
    return result.stdout


def revision(root: Path, value: str) -> str:
    if not re.fullmatch(r"[0-9a-fA-F]{40}", value):
        raise ValueError("base/head must be full 40-character commit OIDs")
    return git(root, "rev-parse", "--verify", value + "^{commit}").decode().strip()


def changes(root: Path, *args: str) -> list[tuple[str, str]]:
    tokens = git(root, "diff", "--name-status", "--no-renames", "-z", *args, "--").split(b"\0")
    if tokens[-1:] == [b""]:
        tokens.pop()
    if len(tokens) % 2:
        raise ValueError("malformed Git name/status input")
    return [(tokens[i].decode(), tokens[i + 1].decode()) for i in range(0, len(tokens), 2)]


def repository_link(source: str, target: str) -> str | None:
    parsed = urlsplit(target)
    if parsed.scheme or parsed.netloc:
        return None
    path = unquote(parsed.path)
    if not path:
        return source
    # Clickable Markdown paths follow the renderer's source-relative rules.
    # Bare doc/... references are checked separately as repository paths.
    return posixpath.normpath(posixpath.join(posixpath.dirname(source), path))


def documentation_files(root: Path) -> list[str]:
    result = []
    if (root / "doc").is_symlink() or not (root / "doc").is_dir():
        raise ValueError("doc must be a real documentation directory")
    def walk_error(error: OSError) -> None:
        raise error
    for directory, directories, files in os.walk(root / "doc", onerror=walk_error):
        parent = Path(directory)
        for name in directories + files:
            path = parent / name
            if path.is_symlink():
                raise ValueError(f"unsupported documentation symlink: {path.relative_to(root)}")
        directories[:] = [name for name in directories if name != "archive"
                           and (parent != root / "doc" or name != "devlog")]
        for name in files:
            path = parent / name
            if name.endswith(".md"):
                result.append(path.relative_to(root).as_posix())
    return sorted(result)


def normalized(text: str | None) -> str:
    text = re.sub(r"<!--[\s\S]*?-->", "", text or "")
    return "\n".join(line.rstrip() for line in text.splitlines() if line.strip())


def tree_files(root: Path, oid: str) -> dict[str, str]:
    result = {}
    for record in git(root, "ls-tree", "-r", "-z", oid).split(b"\0"):
        if record:
            metadata, path = record.split(b"\t", 1)
            result[path.decode()] = metadata.split()[0].decode()
    return result


def snapshot_reader(root: Path, head: str, worktree: bool, staged: dict[str, str],
                    unstaged: dict[str, str], head_files: dict[str, str]):
    @lru_cache(maxsize=None)
    def index_text(path: str) -> str:
        records = git(root, "ls-files", "--stage", "-z", "--", path).split(b"\0")
        entries = [record.split(b"\t", 1)[0].split() for record in records if record]
        if len(entries) != 1 or entries[0][2] != b"0":
            raise ValueError(f"unresolved index input: {path}")
        if entries[0][0] == b"120000":
            raise ValueError(f"unsupported documentation symlink: {path}")
        return git(root, "show", f":{path}").decode("utf-8")

    @lru_cache(maxsize=None)
    def committed(path: str) -> str | None:
        if path not in head_files:
            return None
        if head_files[path] == "120000":
            raise ValueError(f"unsupported documentation symlink: {path}")
        return git(root, "show", f"{head}:{path}").decode("utf-8")

    @lru_cache(maxsize=None)
    def read(path: str) -> str | None:
        if not worktree:
            return committed(path)
        target = root / path
        if not target.resolve().is_relative_to(root):
            raise ValueError(f"documentation path escapes repository: {path}")
        if target.is_symlink():
            raise ValueError(f"unsupported documentation symlink: {path}")
        current = target.read_text(encoding="utf-8") if target.is_file() else None
        # Match the existing product/design gate's staged-only semantics: an
        # edit staged and then restored on disk still checks the index candidate.
        if path in unstaged:
            if staged.get(path) not in (None, "D") and current == committed(path):
                return index_text(path)
            return current
        if path in staged:
            return None if staged[path] == "D" else index_text(path)
        return current
    return read


def select(root: Path, base: str, head: str, worktree: bool, full: bool) -> tuple[list[str], bool, list[str]]:
    base, head = revision(root, base), revision(root, head)
    merge_base = git(root, "merge-base", base, head).decode().strip()
    changed = changes(root, merge_base, head)
    staged, unstaged = {}, {}
    if worktree:
        staged = {path: status for status, path in changes(root, "--cached", head)}
        unstaged = {path: status for status, path in changes(root)}
        changed += [(status, path) for path, status in staged.items()]
        changed += [(status, path) for path, status in unstaged.items()]
        changed += [("A", path.decode()) for path in git(root, "ls-files", "--others", "--exclude-standard", "-z").split(b"\0") if path]
    paths = {path for _, path in changed}
    deleted = {path for status, path in changed if status == "D"}
    relevant = full or any(path.startswith("doc/") or path.endswith(".md") or any(fnmatch.fnmatchcase(path, pattern) for pattern in CONTROL_PATHS) for path in paths)
    targets = deleted | {path for path in paths if path.endswith(".md")}
    if not relevant and not targets:
        return [], False, []
    head_files, base_files = tree_files(root, head), tree_files(root, merge_base)
    current = snapshot_reader(root, head, worktree, staged, unstaged, head_files)
    previous = snapshot_reader(root, merge_base, False, {}, {}, base_files)
    candidates = set(documentation_files(root) if worktree else head_files)
    if worktree:
        candidates.update(staged)
    files = sorted(path for path in candidates if path.startswith("doc/")
                   and path.endswith(".md") and not path.startswith("doc/devlog/")
                   and "archive" not in Path(path).parts and current(path) is not None)
    changed_markdown = {path for path in paths if path.endswith(".md")
                        and normalized(current(path)) != normalized(previous(path))}
    targets = deleted | changed_markdown
    all_files = full or any(any(fnmatch.fnmatchcase(path, pattern) for pattern in CONTROL_PATHS) for path in paths)
    strict = changed_markdown.intersection(files)
    selected = set(files if all_files else strict)
    contents = {}
    for source in files:
        text = current(source)
        contents[source] = text
        links = {path for link in parse_markdown_links(text) if (path := repository_link(source, link.target)) is not None}
        links.update(re.findall(r"doc/[A-Za-z0-9_./-]+\.md", parse_markdown_reference_text(text)))
        if links.intersection(targets):
            selected.add(source)
    errors = []

    def exists(path: str) -> bool:
        target = root / path
        if path.endswith(".md"):
            return current(path) is not None
        if worktree:
            if path in staged and path not in unstaged:
                return staged[path] != "D"
            return target.exists()
        return path in head_files or any(name.startswith(path.rstrip("/") + "/") for name in head_files)

    def report(source: str, target: str, finding: str, *, historical_failure: bool = False) -> None:
        message = f"doc-governance-scope: {source}: {finding}"
        if source in strict or (target in targets and not historical_failure):
            errors.append(message)
        else:
            print("advisory: " + message, file=sys.stderr)

    historical = re.compile(
        r"(?:\b(?:eng-cc|nil-studio)/oasis7@(?P<repo>[0-9a-fA-F]{40}):"
        r"|https://github\.com/(?:eng-cc|nil-studio)/oasis7/blob/(?P<web>[0-9a-fA-F]{40})/)"
        r"(?P<path>doc/[A-Za-z0-9_./-]+\.md)(?=[#?)/\s`]|$)"
    )

    @lru_cache(maxsize=None)
    def historical_blob(oid: str, path: str) -> bool:
        if ".." in Path(path).parts:
            return False
        result = subprocess.run(["git", "-C", str(root), "cat-file", "-t", f"{oid}:{path}"],
                                capture_output=True)
        return result.returncode == 0 and result.stdout.strip() == b"blob"

    for source in sorted(selected):
        for link in parse_markdown_links(contents[source]):
            target = repository_link(source, link.target)
            if target is None:
                continue
            path = root / target
            resolved = path.resolve() if worktree else Path(os.path.abspath(path))
            if not resolved.is_relative_to(root):
                raise ValueError(f"{source}:{link.line}: link escapes repository: {link.target}")
            finding = None
            if not exists(target):
                finding = f"missing target: {link.target}"
            elif path.suffix == ".md":
                fragment = unquote(urlsplit(link.target).fragment).casefold()
                if fragment:
                    if target not in contents:
                        contents[target] = current(target)
                    if fragment not in markdown_anchor_ids(contents[target]):
                        finding = f"missing Markdown fragment: {link.target}"
            if finding:
                old_failure = False
                if source not in strict and target in targets:
                    if target.endswith(".md"):
                        old_text = previous(target)
                        fragment = unquote(urlsplit(link.target).fragment).casefold()
                        old_failure = old_text is None or bool(fragment and fragment not in markdown_anchor_ids(old_text))
                    else:
                        old_failure = target not in base_files
                report(source, target, f"line {link.line}: {finding}", historical_failure=old_failure)
        text = parse_markdown_reference_text(contents[source])
        pinned = list(historical.finditer(text))
        for match in re.finditer(r"doc/[A-Za-z0-9_./-]+\.md", text):
            target = match.group()
            if "YYYY-MM-DD" in target:
                continue
            if exists(target):
                continue
            if any(item.span("path") == match.span()
                   and historical_blob(item.group("repo") or item.group("web"), target)
                   for item in pinned):
                continue
            report(source, target, f"references missing markdown path: {target}",
                   historical_failure=target not in base_files)
    return sorted(selected), relevant or bool(selected), errors


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo-root", type=Path, default=Path("."))
    parser.add_argument("--base", required=True)
    parser.add_argument("--head", required=True)
    parser.add_argument("--worktree", action="store_true")
    parser.add_argument("--full-corpus", action="store_true")
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    try:
        files, active, errors = select(args.repo_root.resolve(), args.base, args.head, args.worktree, args.full_corpus)
        args.output.write_bytes(b"".join(path.encode() + b"\0" for path in files))
    except (OSError, UnicodeError, ValueError) as error:
        print(f"doc-governance-scope: error: {error}", file=sys.stderr)
        return 2
    print("run" if active else "skip")
    for error in errors:
        print(error, file=sys.stderr)
    return 1 if errors else 0


if __name__ == "__main__":
    raise SystemExit(main())
